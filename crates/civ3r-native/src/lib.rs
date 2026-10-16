//! The native Linux side of the renderer inside a Wine program: the unixlib
//! of the builtin DLL `civ3r_native.dll`. The game-side driver (a 32-bit
//! Windows DLL in the same process) sends render assets and scenes and
//! collects input through `__wine_unix_call`; this side runs the tile
//! viewer natively (Wayland, Vulkan) on a thread of its own.
//!
//! Calls (argument structs are plain data, see [`Assets`], [`Publish`],
//! [`TakeInput`]):
//! 0. assets: compile a render description; starts the viewer on first use
//! 1. publish: show a scene (`scene::wire`)
//! 2. input: take the viewer's queued input (`scene::wire`)

use wine_unixlib::Buffer32;

/// Call 0. `data`: `scene::wire::encode_assets`; `flags` bit 0: fullscreen.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Assets {
    pub data: Buffer32,
    pub flags: u32,
    pub _pad: u32,
}

/// Call 1. `scene`: `scene::wire::encode_scene`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Publish {
    pub scene: Buffer32,
}

/// Call 2. Writes `scene::wire::encode_input` into `out`; `len` is the
/// size written (or needed, with `STATUS_BUFFER_TOO_SMALL`).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TakeInput {
    pub out: Buffer32,
    pub len: u32,
    pub _pad: u32,
}

/// Call numbers.
pub const CALL_ASSETS: u32 = 0;
pub const CALL_PUBLISH: u32 = 1;
pub const CALL_INPUT: u32 = 2;

#[cfg(target_os = "linux")]
mod unix_side {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use civ3_assets::GameFiles;
    use civ3_assets::loader::Civ3Loader;
    use scene::Link;
    use wine_unixlib::{STATUS_BUFFER_TOO_SMALL, STATUS_INVALID_PARAMETER, STATUS_SUCCESS, STATUS_UNSUCCESSFUL, Status};

    use super::*;

    struct Viewer {
        link: Link,
        assets: tile_viewer::Assets,
        /// Input taken but not yet delivered (the caller's buffer was too
        /// small).
        pending: Vec<scene::Input>,
    }

    static VIEWER: Mutex<Option<Viewer>> = Mutex::new(None);

    fn log_init() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).try_init();
        });
    }

    /// A Windows path of the Wine prefix as a Unix path (`C:\x` →
    /// `$WINEPREFIX/dosdevices/c:/x`).
    fn unix_path(windows: &str) -> PathBuf {
        let prefix = std::env::var_os("WINEPREFIX")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".wine")))
            .unwrap_or_default();
        let w = windows.replace('\\', "/");
        match w.split_once(':') {
            Some((drive, rest)) if drive.len() == 1 => {
                let p = prefix.join("dosdevices").join(format!("{}:", drive.to_ascii_lowercase())).join(rest.trim_start_matches('/'));
                std::fs::canonicalize(&p).unwrap_or(p)
            }
            _ => PathBuf::from(w),
        }
    }

    fn compile(bytes: &[u8]) -> Result<render_desc::Compiled, String> {
        let a = scene::wire::decode_assets(bytes).map_err(|e| e.to_string())?;
        let files = GameFiles::with_roots(a.roots.iter().map(|r| unix_path(r)).collect());
        let desc = render_desc::Description::parse(&a.description).map_err(|e| e.to_string())?;
        let desc = if a.scale_down > 1 { desc.scaled_down(a.scale_down) } else { desc };
        // Same page size as the driver, so both sides agree on nothing but
        // names and values (the atlas itself never crosses).
        desc.compile(&Civ3Loader::new(files), 2048).map_err(|e| e.to_string())
    }

    pub fn assets(args: &mut Assets) -> Status {
        log_init();
        let Ok(bytes) = wine_unixlib::unix::read(args.data) else { return STATUS_INVALID_PARAMETER };
        let compiled = match compile(&bytes) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                log::error!("civ3r-native: assets: {e}");
                return STATUS_UNSUCCESSFUL;
            }
        };
        let mut viewer = VIEWER.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = viewer.as_ref() {
            v.assets.replace(compiled);
            return STATUS_SUCCESS;
        }
        let link = Link::new();
        let assets = tile_viewer::Assets::new(compiled);
        let config = tile_viewer::ViewerConfig {
            title: "Civ III (native renderer)".into(),
            size: (1024, 768),
            fullscreen: args.flags & 1 != 0,
            overhang: (0, 128, 0, 128),
            ..Default::default()
        };
        let (l, a) = (link.clone(), assets.clone());
        let started = wine_unixlib::unix::spawn_native("civ3r-native-viewer", move || {
            if let Err(e) = tile_viewer::run(config, a, l.clone()) {
                log::error!("civ3r-native: viewer: {e}");
            }
            l.send_input(scene::Input::Quit);
        });
        if let Err(e) = started {
            log::error!("civ3r-native: starting the viewer: {e}");
            return STATUS_UNSUCCESSFUL;
        }
        log::info!("civ3r-native: viewer started");
        *viewer = Some(Viewer { link, assets, pending: Vec::new() });
        STATUS_SUCCESS
    }

    pub fn publish(args: &mut Publish) -> Status {
        let Ok(bytes) = wine_unixlib::unix::read(args.scene) else { return STATUS_INVALID_PARAMETER };
        let scene = match scene::wire::decode_scene(&bytes) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("civ3r-native: scene: {e}");
                return STATUS_INVALID_PARAMETER;
            }
        };
        match VIEWER.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            Some(v) => {
                v.link.publish(scene);
                STATUS_SUCCESS
            }
            None => STATUS_UNSUCCESSFUL,
        }
    }

    pub fn input(args: &mut TakeInput) -> Status {
        let mut viewer = VIEWER.lock().unwrap_or_else(|e| e.into_inner());
        let Some(v) = viewer.as_mut() else {
            args.len = 0;
            return STATUS_SUCCESS;
        };
        v.pending.extend(v.link.take_input());
        let bytes = scene::wire::encode_input(&v.pending);
        args.len = bytes.len() as u32;
        if bytes.len() > args.out.len as usize {
            return STATUS_BUFFER_TOO_SMALL;
        }
        if wine_unixlib::unix::write(args.out, &bytes).is_err() {
            return STATUS_INVALID_PARAMETER;
        }
        v.pending.clear();
        STATUS_SUCCESS
    }
}

#[cfg(target_os = "linux")]
use unix_side::{assets, input, publish};

#[cfg(target_os = "linux")]
wine_unixlib::unix_call_table![assets, publish, input];
