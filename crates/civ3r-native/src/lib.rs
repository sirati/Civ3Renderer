//! The native Linux side of the renderer inside a Wine program: the unixlib
//! of the builtin DLL `civ3r_native.dll`. The game-side driver (a 32-bit
//! Windows DLL in the same process) sends render assets and scenes and
//! collects input through `__wine_unix_call`; this side runs the tile
//! viewer natively (Wayland, Vulkan) on a thread of its own.
//!
//! Calls (argument structs are plain data, see [`Assets`], [`Publish`],
//! [`TakeInput`]):
//! 0. assets: compile a render description; starts the viewer on first use
//! 1. publish: a batch of scene updates (`scene::wire`)
//! 2. input: take the viewer's queued input (`scene::wire`)
//! 3. log: take this side's log lines (`log_lines`), for the driver's log

use wine_unixlib::Buffer32;

pub mod log_lines;

/// Call 0. `data`: `scene::wire::encode_assets`; `flags` bit 0: fullscreen.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Assets {
    pub data: Buffer32,
    pub flags: u32,
    pub _pad: u32,
}

/// Call 1. `scene`: a batch of updates, `scene::wire::encode_updates`.
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

/// Call 3. Writes `log_lines::encode` of the lines logged since the last
/// call into `out`, like [`TakeInput`].
pub type TakeLog = TakeInput;

/// Call numbers.
pub const CALL_ASSETS: u32 = 0;
pub const CALL_PUBLISH: u32 = 1;
pub const CALL_INPUT: u32 = 2;
pub const CALL_LOG: u32 = 3;

#[cfg(target_os = "linux")]
mod unix_side {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use civ3_assets::GameFiles;
    use civ3_assets::loader::Civ3Loader;
    use scene::Driver;
    use wine_unixlib::{STATUS_BUFFER_TOO_SMALL, STATUS_DEVICE_BUSY, STATUS_INVALID_PARAMETER, STATUS_SUCCESS, STATUS_UNSUCCESSFUL, Status};

    use super::*;

    struct Viewer {
        driver: Driver,
        assets: tile_viewer::Assets,
        /// Input taken but not yet delivered (the caller's buffer was too
        /// small).
        pending: Vec<scene::Input>,
    }

    static VIEWER: Mutex<Option<Viewer>> = Mutex::new(None);

    /// Lines for the driver's log (`CALL_LOG`); at most this many wait.
    static FOR_DRIVER: Mutex<crate::log_lines::LogQueue> = Mutex::new(crate::log_lines::LogQueue::new(1000));

    /// Logs to standard error (`RUST_LOG`, default `info`) and, from `info`
    /// up, for the driver's log.
    struct Logger(env_logger::Logger);

    impl log::Log for Logger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            self.0.enabled(metadata) || metadata.level() <= log::Level::Info
        }

        fn log(&self, record: &log::Record) {
            if self.0.matches(record) {
                self.0.log(record);
            }
            if record.level() <= log::Level::Info {
                let line = crate::log_lines::LogLine { level: record.level(), target: record.target().to_owned(), message: record.args().to_string() };
                FOR_DRIVER.lock().unwrap_or_else(|e| e.into_inner()).push(line);
            }
        }

        fn flush(&self) {
            self.0.flush();
        }
    }

    fn log_init() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let inner = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).build();
            let max = inner.filter().max(log::LevelFilter::Info);
            if log::set_boxed_logger(Box::new(Logger(inner))).is_ok() {
                log::set_max_level(max);
            }
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

    /// The compiled description and its generation.
    fn compile(bytes: &[u8]) -> Result<(render_desc::Compiled, u64), String> {
        let a = scene::wire::decode_assets(bytes).map_err(|e| e.to_string())?;
        let files = GameFiles::with_roots(a.roots.iter().map(|r| unix_path(r)).collect());
        let desc = render_desc::Description::parse(&a.description).map_err(|e| e.to_string())?;
        let desc = if a.scale_down > 1 { desc.scaled_down(a.scale_down) } else { desc };
        // Same page size as the driver, so both sides agree on nothing but
        // names and values (the atlas itself never crosses). The sheets of
        // the last compile are kept: art grows a little at a time, and only
        // the new sheets are loaded.
        let mut cache = SHEETS.lock().unwrap_or_else(|e| e.into_inner());
        let started = std::time::Instant::now();
        let compiled = desc.compile_cached(&Civ3Loader::new(files), 2048, &mut cache).map_err(|e| e.to_string())?;
        log::info!("civ3r-native: art {}: {} sprites compiled in {} ms", a.generation, compiled.sprite_names.len(), started.elapsed().as_millis());
        Ok((compiled, a.generation))
    }

    /// The sheets of the last compile (see `render_desc::SheetCache`).
    static SHEETS: Mutex<render_desc::SheetCache> = Mutex::new(render_desc::SheetCache::new());

    /// Assets waiting for the art thread (only the latest counts), and
    /// whether it runs.
    static NEXT: (Mutex<Option<Vec<u8>>>, std::sync::Condvar) = (Mutex::new(None), std::sync::Condvar::new());

    /// Compiles new assets off the caller's thread (the game's publisher
    /// goes on meanwhile; the viewer draws the last scene of its old art
    /// until the new art is there and a scene refers to it).
    fn compile_later(bytes: Vec<u8>) -> Result<(), std::io::Error> {
        static STARTED: std::sync::Once = std::sync::Once::new();
        let mut result = Ok(());
        STARTED.call_once(|| {
            result = wine_unixlib::unix::spawn_native("civ3r-native-art", || loop {
                let bytes = {
                    let mut next = NEXT.0.lock().unwrap_or_else(|e| e.into_inner());
                    loop {
                        if let Some(b) = next.take() {
                            break b;
                        }
                        next = NEXT.1.wait(next).unwrap_or_else(|e| e.into_inner());
                    }
                };
                match compile(&bytes) {
                    Ok((c, generation)) => {
                        if let Some(v) = VIEWER.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                            v.assets.set(Arc::new(c), generation);
                        }
                    }
                    Err(e) => log::error!("civ3r-native: assets: {e}"),
                }
            })
            .map(drop);
        });
        result?;
        *NEXT.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(bytes);
        NEXT.1.notify_one();
        Ok(())
    }

    pub fn assets(args: &mut Assets) -> Status {
        log_init();
        let Ok(bytes) = wine_unixlib::unix::read(args.data) else { return STATUS_INVALID_PARAMETER };
        // A running viewer gets new art when it is compiled.
        if VIEWER.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
            return match compile_later(bytes) {
                Ok(()) => STATUS_SUCCESS,
                Err(e) => {
                    log::error!("civ3r-native: starting the art thread: {e}");
                    STATUS_UNSUCCESSFUL
                }
            };
        }
        // The first, before the viewer starts.
        let (compiled, generation) = match compile(&bytes) {
            Ok((c, g)) => (Arc::new(c), g),
            Err(e) => {
                log::error!("civ3r-native: assets: {e}");
                return STATUS_UNSUCCESSFUL;
            }
        };
        let mut viewer = VIEWER.lock().unwrap_or_else(|e| e.into_inner());
        let (driver, display) = scene::connect();
        let quit = display.input_sender();
        let assets = tile_viewer::Assets::new(compiled.clone());
        assets.set(compiled, generation);
        let config = tile_viewer::ViewerConfig {
            title: "Civ III (native renderer)".into(),
            size: (1024, 768),
            fullscreen: args.flags & 1 != 0,
            overhang: (0, 128, 0, 128),
            ..Default::default()
        };
        let a = assets.clone();
        let started = wine_unixlib::unix::spawn_native("civ3r-native-viewer", move || {
            if let Err(e) = tile_viewer::run(config, a, display) {
                log::error!("civ3r-native: viewer: {e}");
            }
            let _ = quit.send(scene::Input::Quit);
        });
        if let Err(e) = started {
            log::error!("civ3r-native: starting the viewer: {e}");
            return STATUS_UNSUCCESSFUL;
        }
        log::info!("civ3r-native: viewer started");
        *viewer = Some(Viewer { driver, assets, pending: Vec::new() });
        STATUS_SUCCESS
    }

    /// `STATUS_DEVICE_BUSY` while the viewer has not taken the last batch:
    /// the bridge folds this one into its next.
    pub fn publish(args: &mut Publish) -> Status {
        if VIEWER.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|v| v.driver.busy()) {
            return STATUS_DEVICE_BUSY;
        }
        let Ok(bytes) = wine_unixlib::unix::read(args.scene) else { return STATUS_INVALID_PARAMETER };
        let updates = match scene::wire::decode_updates(&bytes) {
            Ok(u) => u,
            Err(e) => {
                log::warn!("civ3r-native: updates: {e}");
                return STATUS_INVALID_PARAMETER;
            }
        };
        match VIEWER.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            Some(v) => {
                v.driver.send(updates);
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
        v.pending.extend(v.driver.take_input());
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

    /// Lines kept for the driver's log until it has room for them all.
    static LOG_PENDING: Mutex<Vec<crate::log_lines::LogLine>> = Mutex::new(Vec::new());

    pub fn take_log(args: &mut TakeLog) -> Status {
        let mut pending = LOG_PENDING.lock().unwrap_or_else(|e| e.into_inner());
        pending.extend(FOR_DRIVER.lock().unwrap_or_else(|e| e.into_inner()).take());
        let bytes = crate::log_lines::encode(&pending);
        args.len = bytes.len() as u32;
        if bytes.len() > args.out.len as usize {
            return STATUS_BUFFER_TOO_SMALL;
        }
        if wine_unixlib::unix::write(args.out, &bytes).is_err() {
            return STATUS_INVALID_PARAMETER;
        }
        pending.clear();
        STATUS_SUCCESS
    }
}

#[cfg(target_os = "linux")]
use unix_side::{assets, input, publish, take_log};

#[cfg(target_os = "linux")]
wine_unixlib::unix_call_table![assets, publish, input, take_log];
