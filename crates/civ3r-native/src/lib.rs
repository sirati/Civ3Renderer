//! The native Linux side of the renderer inside a Wine program: the unixlib
//! of the builtin DLL `civ3r_native.dll`. The game-side driver (a 32-bit
//! Windows DLL in the same process) sends render assets and scenes and
//! collects input through `__wine_unix_call`; this side runs the tile
//! viewer natively: on Linux (Wayland, Vulkan) on a thread of its own, on
//! macOS (AppKit, Metal) on the process's main thread, in the
//! `NSApplication` Wine's Mac driver runs there (the game's threads are
//! Wine's, never the main one). Everything else is the same on both.
//!
//! Calls (argument structs are plain data, see [`Assets`], [`Publish`],
//! [`TakeInput`]):
//! 0. assets: compile a render description; starts the viewer on first use
//! 1. publish: a batch of scene updates (`scene::wire`)
//! 2. input: take the viewer's queued input (`scene::wire`), waiting for
//!    some if asked to ([`LIMIT_INPUT_WAIT`])
//! 3. log: take this side's log lines (`log_lines`), for the driver's log
//! 4. status: the latest art generation the viewer has been given
//! 5. limits: the renderer's limits ([`Limits`]), e.g. for the driver's
//!    compile of the same description
//! 6. uploaded: the latest generation committed to the drawable GPU atlas

use wine_unixlib::Buffer32;

pub mod log_lines;

#[cfg(any(target_os = "linux", target_os = "macos"))]
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
    /// Input: with none queued, how many milliseconds to wait for some
    /// (at most a second; [`LIMIT_INPUT_WAIT`]). 0 for the log.
    pub _pad: u32,
}

/// Call 3. Writes `log_lines::encode` of the lines logged since the last
/// call into `out`, like [`TakeInput`].
pub type TakeLog = TakeInput;

/// Call 4. `generation`: the latest art generation compiled and given to
/// the viewer (whole or grown, `scene::wire::Assets::generation`), or that
/// failed to compile. This is CPU completion only; [`CALL_UPLOADED`]
/// separately reports generations committed to the drawable GPU atlas.
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
    /// Optional protocol capabilities. Older libraries leave this zero.
    pub _pad: u32,
}

/// Call numbers.
pub const CALL_ASSETS: u32 = 0;
pub const CALL_PUBLISH: u32 = 1;
pub const CALL_INPUT: u32 = 2;
pub const CALL_LOG: u32 = 3;
pub const CALL_STATUS: u32 = 4;
pub const CALL_LIMITS: u32 = 5;
/// Call 6. Returns [`ArtStatus`] for drawable GPU art, rather than CPU
/// compilation. Optional preloading waits for this acknowledgment.
pub const CALL_UPLOADED: u32 = 6;
/// [`Limits::_pad`] advertises that [`CALL_UPLOADED`] exists. Check this
/// before dispatching the extra call against an older native library.
pub const LIMIT_UPLOADED: u32 = 1;
/// [`Limits::_pad`] advertises that [`CALL_INPUT`] waits for input as long
/// as [`TakeInput::_pad`] asks (older libraries return at once).
pub const LIMIT_INPUT_WAIT: u32 = 2;


#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix_side {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, LazyLock, OnceLock};

    use civ3_assets::GameFiles;
    use civ3r_sync::{Latest, Mailbox};
    use civ3_assets::loader::Civ3Loader;
    use scene::Driver;
    use wine_unixlib::{STATUS_BUFFER_TOO_SMALL, STATUS_DEVICE_BUSY, STATUS_INVALID_PARAMETER, STATUS_SUCCESS, STATUS_UNSUCCESSFUL, Status};

    use super::art_queue::ArtQueue;
    use super::*;

    struct Viewer {
        driver: Driver,
        assets: tile_viewer::Assets,
    }

    /// Set once the viewer runs. Everything in it is shared without locks
    /// (the driver's channels and counters, the art's hand-off).
    static VIEWER: OnceLock<Viewer> = OnceLock::new();

    /// Input taken but not yet delivered (the caller's buffer was too
    /// small); each call takes it whole (see [`take_all`]).
    static INPUT_PENDING: Latest<Vec<scene::Input>> = Latest::new();

    thread_local! {
        /// The publish calls' profile, if kept (made at the first call).
        /// lock-free: per calling thread, which is the game's publisher.
        static PROFILE: std::cell::RefCell<Option<Option<tile_viewer::profile::Profile>>> = const { std::cell::RefCell::new(None) };
    }

    /// Lines for the driver's log (`CALL_LOG`), sent from any thread and
    /// folded by `take_log` (`log_lines::LogQueue`, at most `LOG_CAP`
    /// lines). Beyond `LOG_RAW_CAP` unread lines the oldest are dropped
    /// and counted in `LOG_DROPPED`.
    static FOR_DRIVER: LazyLock<Mailbox<crate::log_lines::LogLine>> = LazyLock::new(Mailbox::new);
    static LOG_DROPPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    const LOG_CAP: usize = 1000;
    const LOG_RAW_CAP: usize = 10 * LOG_CAP;

    /// Everything in `slot`, which is then empty: a concurrent caller
    /// finds nothing there rather than waiting.
    fn take_all<T: Clone>(slot: &Latest<Vec<T>>) -> Vec<T> {
        slot.take().map(Arc::unwrap_or_clone).unwrap_or_default()
    }

    /// Puts what `take_all` gave back, before anything put back meanwhile.
    fn put_back<T: Clone>(slot: &Latest<Vec<T>>, mine: Vec<T>) {
        if !mine.is_empty() {
            slot.update(|now| Some(mine.iter().chain(now.into_iter().flatten()).cloned().collect()));
        }
    }

    #[cfg(test)]
    #[test]
    fn undelivered_items_are_put_back_in_order_and_never_lost() {
        let slot = Latest::new();
        put_back(&slot, vec![1, 2]);
        let mine = take_all(&slot);
        // Another caller took and returned newer items meanwhile.
        assert!(take_all(&slot).is_empty());
        put_back(&slot, vec![3]);
        put_back(&slot, mine);
        assert_eq!(take_all(&slot), [1, 2, 3]);
        assert!(take_all(&slot).is_empty());
    }

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
                while FOR_DRIVER.len() >= LOG_RAW_CAP && FOR_DRIVER.try_recv().is_some() {
                    LOG_DROPPED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                FOR_DRIVER.send(line);
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
    /// tables (the pixels go to the viewer) and how it grows. Owned by the
    /// art thread; the first, compiled before it starts, is handed over here.
    type Art = (render_desc::Compiled, render_desc::Growing);
    static FIRST_ART: LazyLock<Mailbox<Art>> = LazyLock::new(Mailbox::new);

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
    fn grow(a: &scene::wire::Assets, stop: &dyn Fn() -> bool, art: &mut Option<Art>) -> Result<render_desc::Growth, String> {
        let files = GameFiles::with_roots(a.roots.iter().map(|r| unix_path(r)).collect());
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
        *args = Limits { max_texture: max_page(), _pad: LIMIT_UPLOADED | LIMIT_INPUT_WAIT };
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
            result = wine_unixlib::unix::spawn_native("civ3r-native-art", || { let mut art: Option<Art> = None; loop {
                let a = NEXT.pop();
                // Sent before the viewer was set, so before any job.
                for first in FIRST_ART.drain() {
                    art = Some(first);
                }
                let stop = || NEXT.superseded(a.generation);
                let viewer = || VIEWER.get().map(|v| v.assets.clone());
                let started = std::time::Instant::now();
                let result = if a.append {
                    grow(&a, &stop, &mut art).map(|g| {
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
                            retired = art.replace((tables, growing));
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
            }})
            .map(drop);
        });
        result?;
        NEXT.push(a);
        Ok(())
    }

    /// The latest art generation the art thread is done with (see `status`).
    static PROCESSED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    pub fn status(args: &mut ArtStatus) -> Status {
        let given = VIEWER.get().map_or(0, |v| v.assets.latest());
        let generation = given.max(PROCESSED.load(std::sync::atomic::Ordering::Acquire));
        (args.generation_lo, args.generation_hi) = (generation as u32, (generation >> 32) as u32);
        STATUS_SUCCESS
    }

    pub fn uploaded(args: &mut ArtStatus) -> Status {
        let generation = VIEWER.get().map_or(0, |v| v.assets.uploaded_generation());
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
        if VIEWER.get().is_some() {
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
                FIRST_ART.send((c.without_pixels(), growing));
                Arc::new(c)
            }
            Err(e) => {
                log::error!("civ3r-native: assets: {e}");
                return STATUS_UNSUCCESSFUL;
            }
        };
        let (driver, display) = scene::connect();
        let quit = display.input_sender();
        let assets = tile_viewer::Assets::new(compiled.clone());
        assets.set(compiled, generation);
        let config = tile_viewer::ViewerConfig {
            title: "Civ III (native renderer)".into(),
            native_thread_init: Some(wine_unixlib::unix::block_native_signals),
            size: (1024, 768),
            fullscreen: args.flags & 1 != 0,
            // (As `civ3_bridge::scene_map::OVERHANG`: tall art above and
            // below, units' frames and an army's banner to the sides.)
            overhang: (128, 128, 128, 128),
            ..Default::default()
        };
        let a = assets.clone();
        let started = start_viewer(config, a, display, QuitWhenStopped(quit));
        if let Err(e) = started {
            log::error!("civ3r-native: starting the viewer: {e}");
            return STATUS_UNSUCCESSFUL;
        }
        log::info!("civ3r-native: viewer started");
        if VIEWER.set(Viewer { driver, assets }).is_err() {
            // (Only with concurrent first calls; the first viewer stays.)
            log::warn!("civ3r-native: a second viewer was started");
        }
        STATUS_SUCCESS
    }

    /// Tells the driver the viewer is gone when dropped, however it ends.
    struct QuitWhenStopped(scene::InputSender);
    impl Drop for QuitWhenStopped {
        fn drop(&mut self) {
            let _ = self.0.send(scene::Input::Quit);
        }
    }

    /// The viewer on a native thread of its own (Wayland and Vulkan take
    /// any thread).
    #[cfg(target_os = "linux")]
    fn start_viewer(config: tile_viewer::ViewerConfig, assets: tile_viewer::Assets, display: scene::Display, quit: QuitWhenStopped) -> std::io::Result<()> {
        wine_unixlib::unix::spawn_native("civ3r-native-viewer", move || {
            let _quit = quit;
            if let Err(e) = tile_viewer::run(config, assets, display) {
                log::error!("civ3r-native: viewer: {e}");
            }
        })
        .map(drop)
    }

    /// The viewer in Wine's `NSApplication` on the main thread (AppKit's
    /// windows live there only); its frames and events run there, between
    /// the Mac driver's. Wine starts its application when the game makes
    /// its first window, normally long before the first art.
    #[cfg(target_os = "macos")]
    fn start_viewer(config: tile_viewer::ViewerConfig, assets: tile_viewer::Assets, display: scene::Display, quit: QuitWhenStopped) -> std::io::Result<()> {
        let config = tile_viewer::ViewerConfig { application_icon: application_icon(), ..config };
        tile_viewer::start_in_host_application(config, assets, display, std::time::Duration::from_secs(120), Box::new(move |end| {
            let _quit = quit;
            match end {
                Ok(()) => log::info!("civ3r-native: viewer closed"),
                Err(e) => log::error!("civ3r-native: viewer: {e}"),
            }
        }));
        Ok(())
    }

    /// The original game's icon for the Dock (the game's own window, which
    /// would give Wine's application its icon, stays hidden).
    #[cfg(target_os = "macos")]
    fn application_icon() -> Option<application_icon::ApplicationIcon> {
        application_icon::ApplicationIcon::from_png(include_bytes!("../../civ3r-viewer/assets/civ3-conquests-icon.png"))
            .map_err(|e| log::warn!("civ3r-native: application icon: {e}"))
            .ok()
    }

    /// `STATUS_DEVICE_BUSY` while the viewer has not taken the last batch:
    /// the bridge folds this one into its next.
    pub fn publish(args: &mut Publish) -> Status {
        if VIEWER.get().is_some_and(|v| v.driver.busy()) {
            return STATUS_DEVICE_BUSY;
        }
        let started = std::time::Instant::now();
        let Ok(bytes) = wine_unixlib::unix::read(args.scene) else { return STATUS_INVALID_PARAMETER };
        let read = started.elapsed();
        let decoding = std::time::Instant::now();
        let decoded = scene::wire::decode_updates(&bytes);
        // Where the time goes (`TILE_VIEWER_PROFILE=1`).
        PROFILE.with_borrow_mut(|profile| {
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
        });
        let updates = match decoded {
            Ok(u) => u,
            Err(e) => {
                log::warn!("civ3r-native: updates: {e}");
                return STATUS_INVALID_PARAMETER;
            }
        };
        match VIEWER.get() {
            Some(v) => {
                v.driver.send(updates);
                STATUS_SUCCESS
            }
            None => STATUS_UNSUCCESSFUL,
        }
    }

    pub fn input(args: &mut TakeInput) -> Status {
        let Some(v) = VIEWER.get() else {
            args.len = 0;
            return STATUS_SUCCESS;
        };
        let mut pending = take_all(&INPUT_PENDING);
        pending.extend(v.driver.take_input());
        if pending.is_empty() && args._pad > 0 {
            pending = v.driver.wait_input(std::time::Duration::from_millis(args._pad.min(1000).into()));
        }
        let bytes = scene::wire::encode_input(&pending);
        args.len = bytes.len() as u32;
        if bytes.len() > args.out.len as usize {
            put_back(&INPUT_PENDING, pending);
            return STATUS_BUFFER_TOO_SMALL;
        }
        if wine_unixlib::unix::write(args.out, &bytes).is_err() {
            put_back(&INPUT_PENDING, pending);
            return STATUS_INVALID_PARAMETER;
        }
        STATUS_SUCCESS
    }

    /// Lines kept for the driver's log until it has room for them all.
    static LOG_PENDING: Latest<Vec<crate::log_lines::LogLine>> = Latest::new();

    /// What the loggers sent since the last call, folded as `LogQueue`
    /// folds them, with a note of the lines dropped unread.
    fn new_log_lines() -> Vec<crate::log_lines::LogLine> {
        let mut queue = crate::log_lines::LogQueue::new(LOG_CAP);
        let dropped = LOG_DROPPED.swap(0, std::sync::atomic::Ordering::Relaxed);
        if dropped > 0 {
            queue.push(crate::log_lines::LogLine { level: log::Level::Warn, target: "civ3r_native".into(), message: format!("({dropped} log lines dropped)") });
        }
        for line in FOR_DRIVER.drain() {
            queue.push(line);
        }
        queue.take()
    }

    pub fn take_log(args: &mut TakeLog) -> Status {
        let mut pending = take_all(&LOG_PENDING);
        pending.extend(new_log_lines());
        let bytes = crate::log_lines::encode(&pending);
        args.len = bytes.len() as u32;
        if bytes.len() > args.out.len as usize {
            put_back(&LOG_PENDING, pending);
            return STATUS_BUFFER_TOO_SMALL;
        }
        if wine_unixlib::unix::write(args.out, &bytes).is_err() {
            put_back(&LOG_PENDING, pending);
            return STATUS_INVALID_PARAMETER;
        }
        STATUS_SUCCESS
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
use unix_side::{assets, input, limits, publish, status, take_log, uploaded};

#[cfg(any(target_os = "linux", target_os = "macos"))]
wine_unixlib::unix_call_table![assets, publish, input, take_log, status, limits, uploaded];
