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
//! 4. status: the latest art generation the viewer has been given
//! 5. limits: the renderer's limits ([`Limits`]), e.g. for the driver's
//!    compile of the same description

use wine_unixlib::Buffer32;

pub mod log_lines;

#[cfg(target_os = "linux")]
mod art_queue;

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

/// Call 4. `generation`: the latest art generation compiled and given to
/// the viewer (whole or grown, `scene::wire::Assets::generation`), or that
/// failed to compile: scenes referring to it are drawn without waiting.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ArtStatus {
    pub generation_lo: u32,
    pub generation_hi: u32,
}

/// Call 5. Fills in the limits of the GPU the viewer draws with.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Limits {
    /// The largest texture side: the atlas pages' limit
    /// (`render_desc::page_size`).
    pub max_texture: u32,
    pub _pad: u32,
}

/// Call numbers.
pub const CALL_ASSETS: u32 = 0;
pub const CALL_PUBLISH: u32 = 1;
pub const CALL_INPUT: u32 = 2;
pub const CALL_LOG: u32 = 3;
pub const CALL_STATUS: u32 = 4;
pub const CALL_LIMITS: u32 = 5;

#[cfg(target_os = "linux")]
mod unix_side {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use civ3_assets::GameFiles;
    use civ3_assets::loader::Civ3Loader;
    use scene::Driver;
    use wine_unixlib::{STATUS_BUFFER_TOO_SMALL, STATUS_DEVICE_BUSY, STATUS_INVALID_PARAMETER, STATUS_SUCCESS, STATUS_UNSUCCESSFUL, Status};

    use super::art_queue::ArtQueue;
    use super::*;

    struct Viewer {
        driver: Driver,
        assets: tile_viewer::Assets,
        /// Input taken but not yet delivered (the caller's buffer was too
        /// small).
        pending: Vec<scene::Input>,
    }

    static VIEWER: Mutex<Option<Viewer>> = Mutex::new(None);

    /// The publish calls' profile, if kept (made at the first call).
    static PROFILE: Mutex<Option<Option<tile_viewer::profile::Profile>>> = Mutex::new(None);

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

    /// The art compiled so far, to be grown (`render_desc::Growing`): its
    /// tables (the pixels go to the viewer) and how it grows.
    static ART: Mutex<Option<(render_desc::Compiled, render_desc::Growing)>> = Mutex::new(None);

    /// The art of `a` (a whole description) compiled, kept to be grown.
    fn compile(a: &scene::wire::Assets, stop: &dyn Fn() -> bool) -> Result<(render_desc::Compiled, render_desc::Growing), String> {
        let files = GameFiles::with_roots(a.roots.iter().map(|r| unix_path(r)).collect());
        let desc = render_desc::Description::parse(&a.description).map_err(|e| e.to_string())?;
        // Pages for this GPU, the same the driver's compile of the tables
        // checks against (`CALL_LIMITS`), as large as the description's
        // largest sprite (or what it may grow by) needs: both sides agree on
        // nothing but names and values (the atlas itself never crosses).
        let max_page = max_page();
        let started = std::time::Instant::now();
        let (compiled, growing) = desc.compile_growing(&render_desc::StopWhen::new(Civ3Loader::new(files), stop), max_page, true).map_err(|e| e.to_string())?;
        if compiled.atlas.page_size > render_desc::DEFAULT_PAGE {
            log::info!("civ3r-native: art {}: atlas pages of {1}×{1} for a sprite of {2}", a.generation, compiled.atlas.page_size, desc.largest_sprite());
        }
        let (rgba, index) = compiled.atlas.page_bytes();
        log::info!(
            "civ3r-native: art {}: {} sprites compiled in {} ms; atlas {} colour pages ({} MiB), {} index pages ({} MiB)",
            a.generation,
            compiled.sprite_names.len(),
            started.elapsed().as_millis(),
            compiled.atlas.pages.len(),
            rgba >> 20,
            compiled.atlas.index_pages.len(),
            index >> 20
        );
        Ok((compiled, growing))
    }

    /// The growth of the art by `a` (an addition), applied to the tables
    /// kept.
    fn grow(a: &scene::wire::Assets, stop: &dyn Fn() -> bool) -> Result<render_desc::Growth, String> {
        let files = GameFiles::with_roots(a.roots.iter().map(|r| unix_path(r)).collect());
        let mut art = ART.lock().unwrap_or_else(|e| e.into_inner());
        let Some((compiled, growing)) = art.as_mut() else { return Err("no art to grow".into()) };
        let started = std::time::Instant::now();
        let growth = growing.grow(compiled, &a.description, &render_desc::StopWhen::new(Civ3Loader::new(files), stop)).map_err(|e| e.to_string())?;
        compiled.apply(&growth);
        let (rgba, index) = growing.pages();
        log::info!(
            "civ3r-native: art {}: grown by {} sprites ({} KiB of pixels) in {} ms; atlas {rgba} colour pages, {index} index pages",
            a.generation,
            growth.atlas.sprites.len(),
            growth.atlas.bytes() >> 10,
            started.elapsed().as_millis(),
        );
        Ok(growth)
    }

    /// The largest atlas page the viewer's GPU takes (probed once, on a
    /// native thread: GPU drivers start threads of their own, which must not
    /// inherit a Wine thread's signal handling).
    fn max_page() -> u32 {
        static MAX: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        *MAX.get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            let probed = wine_unixlib::unix::spawn_native("civ3r-native-limits", move || {
                let _ = tx.send(tile_render::gpu::max_texture_dimension());
            });
            let max = probed.ok().and_then(|_| rx.recv().ok()).flatten();
            max.unwrap_or_else(|| {
                log::warn!("civ3r-native: the GPU's texture limit is unknown, atlas pages of at most {}", render_desc::ANY_DEVICE_PAGE);
                render_desc::ANY_DEVICE_PAGE
            })
        })
    }

    pub fn limits(args: &mut Limits) -> Status {
        log_init();
        *args = Limits { max_texture: max_page(), _pad: 0 };
        STATUS_SUCCESS
    }

    /// Art waiting for the art thread, in order (a whole description makes
    /// what waits before it obsolete).
    static NEXT: ArtQueue = ArtQueue::new();

    /// Compiles new art off the caller's thread (the game's publisher goes
    /// on meanwhile; the viewer draws the last scene of its old art until a
    /// new whole description is there and a scene refers to it, growths
    /// at once).
    fn compile_later(a: scene::wire::Assets) -> Result<(), std::io::Error> {
        static STARTED: std::sync::Once = std::sync::Once::new();
        let mut result = Ok(());
        STARTED.call_once(|| {
            result = wine_unixlib::unix::spawn_native("civ3r-native-art", || loop {
                let a = NEXT.pop();
                let stop = || NEXT.superseded(a.generation);
                let viewer = || VIEWER.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|v| v.assets.clone());
                let started = std::time::Instant::now();
                let result = if a.append {
                    grow(&a, &stop).map(|g| {
                        NEXT.with_current(a.generation, || {
                            if let Some(assets) = viewer() {
                                assets.grow_with_reserve(Arc::new(g), a.generation, 0, a.reserve);
                            }
                        })
                    })
                } else {
                    compile(&a, &stop).map(|(c, growing)| {
                        let tables = c.without_pixels();
                        let mut retired = None;
                        let accepted = NEXT.with_current(a.generation, || {
                            retired = ART.lock().unwrap_or_else(|e| e.into_inner()).replace((tables, growing));
                            if let Some(assets) = viewer() {
                                assets.set_with_reserve(Arc::new(c), a.generation, 0, a.reserve);
                            }
                        });
                        // Dropping the previous compile's shelves and shared
                        // pixels must not hold up the next art request.
                        drop(retired);
                        accepted
                    })
                };
                match result {
                    Ok(true) => {}
                    Ok(false) => log::info!("civ3r-native: art {} superseded while finishing after {} ms", a.generation, started.elapsed().as_millis()),
                    Err(e) if stop() => log::info!("civ3r-native: art {} superseded after {} ms: {e}", a.generation, started.elapsed().as_millis()),
                    Err(e) => log::error!("civ3r-native: art {}: {e}", a.generation),
                }
                // (Done with it, compiled or not: the bridge waits for no more.)
                PROCESSED.fetch_max(a.generation, std::sync::atomic::Ordering::Release);
            })
            .map(drop);
        });
        result?;
        NEXT.push(a);
        Ok(())
    }

    /// The latest art generation the art thread is done with (see `status`).
    static PROCESSED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    pub fn status(args: &mut ArtStatus) -> Status {
        let given = VIEWER.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map_or(0, |v| v.assets.latest());
        let generation = given.max(PROCESSED.load(std::sync::atomic::Ordering::Acquire));
        (args.generation_lo, args.generation_hi) = (generation as u32, (generation >> 32) as u32);
        STATUS_SUCCESS
    }

    pub fn assets(args: &mut Assets) -> Status {
        log_init();
        let Ok(bytes) = wine_unixlib::unix::read(args.data) else { return STATUS_INVALID_PARAMETER };
        let a = match scene::wire::decode_assets(&bytes) {
            Ok(a) => a,
            Err(e) => {
                log::error!("civ3r-native: assets: {e}");
                return STATUS_INVALID_PARAMETER;
            }
        };
        // A running viewer gets new art when it is compiled.
        if VIEWER.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
            return match compile_later(a) {
                Ok(()) => STATUS_SUCCESS,
                Err(e) => {
                    log::error!("civ3r-native: starting the art thread: {e}");
                    STATUS_UNSUCCESSFUL
                }
            };
        }
        // The first, before the viewer starts.
        if a.append {
            log::error!("civ3r-native: art {}: an addition before any art", a.generation);
            return STATUS_INVALID_PARAMETER;
        }
        let generation = a.generation;
        let compiled = match compile(&a, &|| false) {
            Ok((c, growing)) => {
                *ART.lock().unwrap_or_else(|e| e.into_inner()) = Some((c.without_pixels(), growing));
                Arc::new(c)
            }
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
            // (As `civ3_bridge::scene_map::OVERHANG`: tall art above and
            // below, units' frames and an army's banner to the sides.)
            overhang: (128, 128, 128, 128),
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
        let started = std::time::Instant::now();
        let Ok(bytes) = wine_unixlib::unix::read(args.scene) else { return STATUS_INVALID_PARAMETER };
        let read = started.elapsed();
        let decoding = std::time::Instant::now();
        let decoded = scene::wire::decode_updates(&bytes);
        // Where the time goes (`TILE_VIEWER_PROFILE=1`).
        let mut profile = PROFILE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(p) = profile.get_or_insert_with(|| tile_viewer::profile::env_on("TILE_VIEWER_PROFILE").then(|| tile_viewer::profile::Profile::new("native publish", std::time::Duration::from_secs(5)))) {
            p.round_start();
            p.add("read", read);
            p.add("decode", decoding.elapsed());
            p.count("bytes", bytes.len());
            p.round_end();
            if let Some(report) = p.report() {
                log::info!("{report}");
            }
        }
        drop(profile);
        let updates = match decoded {
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
use unix_side::{assets, input, limits, publish, status, take_log};

#[cfg(target_os = "linux")]
wine_unixlib::unix_call_table![assets, publish, input, take_log, status, limits];
