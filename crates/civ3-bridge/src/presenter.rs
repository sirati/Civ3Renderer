//! Where the scenes go: a viewer on a thread of this (Windows) process, or
//! — inside Wine — a native Linux viewer in the same process, reached
//! through the unixlib of `civ3_native.dll` (`civ3r-native`). Either way
//! the renderer owns its copy of the world and gets only what changed.

use std::sync::Arc;

use render_desc::Compiled;
use scene::{Driver, Input, Scene, Update};

pub struct Presenter {
    sink: Sink,
    /// The scene as the renderer has it (what was sent so far).
    sent: Scene,
    /// The generation of the assets scenes refer to (`Scene::assets`), and
    /// of the latest prepared (see [`Presenter::prepare_assets`]).
    generation: u64,
    prepared: u64,
    /// The pixels last sent per run-time sprite (unchanged ones are not
    /// sent again: the renderer keeps them, also across new art).
    sent_patches: std::collections::HashMap<String, Vec<u8>>,
    /// Index pages to ask a renderer compiling its own art to make room
    /// for, with the next art sent (see [`Presenter::reserve`]).
    reserve: u32,
    /// What the last [`Presenter::publish`] took (for profiling).
    pub stats: PublishStats,
}

/// The cost of one [`Presenter::publish`].
#[derive(Copy, Clone, Debug, Default)]
pub struct PublishStats {
    /// Finding what changed.
    pub diff: std::time::Duration,
    /// Encoding and handing it over.
    pub send: std::time::Duration,
    /// Keeping the scene as sent.
    pub keep: std::time::Duration,
    pub updates: usize,
    /// Cell changes (values, phases, offsets) among them.
    pub cells: usize,
    /// Encoded size (the native viewer's wire), else 0.
    pub bytes: usize,
}

enum Sink {
    InProcess { assets: tile_viewer::Assets, driver: Driver },
    Native(Native),
}

impl Presenter {
    /// A viewer window on a thread of this process.
    pub fn in_process(config: tile_viewer::ViewerConfig, compiled: Arc<Compiled>) -> std::io::Result<Presenter> {
        let assets = tile_viewer::Assets::new(compiled);
        let (driver, display) = scene::connect();
        let a = assets.clone();
        // The game's own windows stay as DPI aware as the game is (made
        // aware, a DPI-unaware window reports its client area in the
        // screen's pixels, and the game scales its mouse by it): only the
        // viewer's thread has the screen's pixels.
        let config = tile_viewer::ViewerConfig { process_dpi_aware: false, ..config };
        std::thread::Builder::new().name("civ3r-viewer".into()).spawn(move || {
            #[cfg(windows)]
            if !inproc::dpi::thread_per_monitor_aware() {
                log::warn!("viewer: its thread could not be made DPI aware (its window in logical pixels)");
            }
            if let Err(e) = tile_viewer::run(config, a, display) {
                log::error!("viewer: {e}");
            }
        })?;
        Ok(Presenter { sink: Sink::InProcess { assets, driver }, sent: Scene::default(), generation: 1, prepared: 1, sent_patches: Default::default(), reserve: 0, stats: PublishStats::default() })
    }

    /// The native Linux viewer (only in a Wine process with
    /// `civ3r_native.dll`).
    pub fn native(native: Native) -> Presenter {
        Presenter { sink: Sink::Native(native), sent: Scene::default(), generation: 0, prepared: 0, sent_patches: Default::default(), reserve: 0, stats: PublishStats::default() }
    }

    /// New render assets: `compiled` for this side, `wire` for a renderer
    /// that compiles its own copy.
    pub fn set_assets(&mut self, compiled: &Arc<Compiled>, wire: &scene::wire::Assets) {
        let generation = self.prepare_assets(wire);
        self.adopt_assets(compiled, generation);
    }

    /// Announces new render assets, before this side has compiled them: a
    /// renderer compiling its own copy (the native viewer) starts now, so
    /// that both compile at once. Scenes still refer to the assets before
    /// until [`Presenter::adopt_assets`]. Returns their generation.
    pub fn prepare_assets(&mut self, wire: &scene::wire::Assets) -> u64 {
        self.prepared = self.prepared.max(self.generation) + 1;
        if let Sink::Native(n) = &mut self.sink {
            n.set_assets(&scene::wire::Assets { generation: self.prepared, reserve: std::mem::take(&mut self.reserve), ..wire.clone() });
        }
        self.prepared
    }

    /// Scenes from now on refer to the assets of `generation` (prepared,
    /// `compiled` on this side); the renderer draws them with these assets
    /// only. An in-process renderer gets them (pixels and all) now.
    pub fn adopt_assets(&mut self, compiled: &Arc<Compiled>, generation: u64) {
        self.generation = generation;
        if let Sink::InProcess { assets, .. } = &mut self.sink {
            assets.set(compiled.clone(), generation);
        }
    }

    /// Scenes from now on refer to growth `generation` (prepared) of the
    /// assets; an in-process renderer gets `growth` (its pixels) now, a
    /// native one compiled it itself (see [`Presenter::art_status`]).
    pub fn adopt_growth(&mut self, growth: &Arc<render_desc::Growth>, generation: u64) {
        self.generation = generation;
        if let Sink::InProcess { assets, .. } = &mut self.sink {
            assets.grow(growth.clone(), generation);
        }
    }

    /// Asks the renderer to make room in its atlas for this many colour and
    /// index pages (art expected soon): one given the art now, one
    /// compiling its own with the next art sent (index pages only).
    pub fn reserve(&mut self, pages: u32, index_pages: u32) {
        match &self.sink {
            Sink::InProcess { assets, .. } => assets.reserve(pages, index_pages),
            Sink::Native(_) => self.reserve = index_pages,
        }
    }

    /// The latest art generation a renderer that compiles its own art has
    /// (scenes referring to it are drawn at once); `None` for one given
    /// the art.
    pub fn art_status(&mut self) -> Option<u64> {
        match &mut self.sink {
            Sink::InProcess { .. } => None,
            Sink::Native(n) => Some(n.status()),
        }
    }

    /// Whether every art request sent so far has reached the drawable GPU
    /// atlas. Background preload starts only then, bounding its backlog to
    /// one chunk even when compilation is much faster than uploading.
    pub fn preload_ready(&mut self) -> bool {
        let uploaded = match &mut self.sink {
            Sink::InProcess { assets, .. } => assets.uploaded_generation(),
            Sink::Native(n) => n.uploaded_generation(),
        };
        uploaded >= self.prepared.max(self.generation)
    }

    /// Makes the renderer's scene `scene`, sending only what changed.
    /// While the renderer has not taken the last batch, nothing is sent: what
    /// changed goes with a later one (a slow renderer does not queue up
    /// batches). Whether the renderer has everything (else publish again).
    pub fn publish(&mut self, mut scene: Scene) -> bool {
        self.stats = PublishStats::default();
        if let Sink::InProcess { driver, .. } = &self.sink && driver.busy() {
            return false;
        }
        let t = std::time::Instant::now();
        scene.assets = self.generation;
        scene.patches.retain(|p| self.sent_patches.get(&p.sprite) != Some(&p.rgba));
        let updates = scene.updates_since(&self.sent);
        self.stats.diff = t.elapsed();
        if updates.is_empty() {
            return true;
        }
        self.stats.updates = updates.len();
        self.stats.cells = updates
            .iter()
            .map(|u| match u {
                Update::Cells(c) => c.len(),
                Update::Phases(c) => c.len(),
                Update::Offsets(c) => c.len(),
                _ => 0,
            })
            .sum();
        let t = std::time::Instant::now();
        let taken = match &mut self.sink {
            Sink::InProcess { driver, .. } => driver.send(updates),
            Sink::Native(n) => {
                let (taken, bytes) = n.publish(&updates);
                self.stats.bytes = bytes;
                taken
            }
        };
        self.stats.send = t.elapsed();
        let t = std::time::Instant::now();
        if taken {
            for p in scene.patches.drain(..) {
                self.sent_patches.insert(p.sprite, p.rgba);
            }
            self.sent = scene;
        }
        self.stats.keep = t.elapsed();
        taken
    }

    pub fn take_input(&mut self) -> Vec<Input> {
        match &mut self.sink {
            Sink::InProcess { driver, .. } => driver.take_input(),
            Sink::Native(n) => n.take_input(),
        }
    }

    /// Writes what the renderer logged into this log (the native viewer's
    /// lines from `info` up; an in-process viewer logs here itself).
    pub fn forward_log(&mut self) {
        if let Sink::Native(n) = &mut self.sink {
            for line in n.take_log() {
                log::log!(target: "civ3r_native", line.level, "{}: {}", line.target, line.message);
            }
        }
    }

    /// Which renderer this is, for the log.
    pub fn describe(&self) -> &'static str {
        match self.sink {
            Sink::InProcess { .. } => "in-process: the viewer is a window of the game's own (32-bit Windows) process",
            Sink::Native(_) => "native: the viewer is native Linux code in the game's process (civ3r_native.dll's unixlib, Wine's new WoW64)",
        }
    }
}

/// The native viewer (only in a Wine process with `civ3r_native.dll`).
pub struct Native {
    #[cfg(all(windows, target_arch = "x86"))]
    lib: wine_unixlib::pe::UnixLib,
    #[cfg(all(windows, target_arch = "x86"))]
    upload_status: Option<bool>,
    fullscreen: bool,
    input: Vec<u8>,
    log: Vec<u8>,
}

impl Native {
    #[cfg(all(windows, target_arch = "x86"))]
    pub fn load(fullscreen: bool) -> Result<Native, String> {
        let lib = wine_unixlib::pe::UnixLib::load("civ3r_native.dll").map_err(|s| format!("civ3r_native.dll: status {s:#x}"))?;
        Ok(Native { lib, upload_status: None, fullscreen, input: vec![0; 64 * 1024], log: vec![0; 16 * 1024] })
    }

    #[cfg(not(all(windows, target_arch = "x86")))]
    pub fn load(_fullscreen: bool) -> Result<Native, String> {
        Err("the native viewer needs a 32-bit Wine process".into())
    }

    /// The largest texture side of the native viewer's GPU (`None` if it
    /// cannot tell, e.g. an older `civ3r_native.so`).
    pub fn max_texture(&self) -> Option<u32> {
        #[cfg(all(windows, target_arch = "x86"))]
        {
            let mut args = civ3r_native::Limits::default();
            match self.lib.call(civ3r_native::CALL_LIMITS, &mut args) {
                0 if args.max_texture > 0 => Some(args.max_texture),
                status => {
                    log::warn!("native viewer: limits: status {status:#x}");
                    None
                }
            }
        }
        #[cfg(not(all(windows, target_arch = "x86")))]
        None
    }

    fn set_assets(&mut self, wire: &scene::wire::Assets) {
        #[cfg(all(windows, target_arch = "x86"))]
        {
            let bytes = scene::wire::encode_assets(wire);
            let mut args = civ3r_native::Assets { data: wine_unixlib::pe::buffer(&bytes), flags: self.fullscreen as u32, _pad: 0 };
            let status = self.lib.call(civ3r_native::CALL_ASSETS, &mut args);
            if status != 0 {
                log::error!("native viewer: assets: status {status:#x}");
            }
        }
        #[cfg(not(all(windows, target_arch = "x86")))]
        let _ = (wire, self.fullscreen);
    }

    /// The latest art generation it has compiled (see `civ3r_native::ArtStatus`).
    fn status(&mut self) -> u64 {
        #[cfg(all(windows, target_arch = "x86"))]
        {
            let mut args = civ3r_native::ArtStatus::default();
            if self.lib.call(civ3r_native::CALL_STATUS, &mut args) != 0 {
                return 0;
            }
            args.generation_lo as u64 | (args.generation_hi as u64) << 32
        }
        #[cfg(not(all(windows, target_arch = "x86")))]
        0
    }

    /// The latest art committed to the drawable atlas. An older native
    /// library without this call safely disables optional preloading.
    fn uploaded_generation(&mut self) -> u64 {
        #[cfg(all(windows, target_arch = "x86"))]
        {
            let supported = *self.upload_status.get_or_insert_with(|| {
                let mut limits = civ3r_native::Limits::default();
                self.lib.call(civ3r_native::CALL_LIMITS, &mut limits) == 0 && limits._pad & civ3r_native::LIMIT_UPLOADED != 0
            });
            if !supported { return 0; }
            let mut args = civ3r_native::ArtStatus::default();
            if self.lib.call(civ3r_native::CALL_UPLOADED, &mut args) != 0 {
                return 0;
            }
            args.generation_lo as u64 | (args.generation_hi as u64) << 32
        }
        #[cfg(not(all(windows, target_arch = "x86")))]
        0
    }

    /// Whether the native viewer took `updates` (not when busy), and their
    /// size on the wire.
    fn publish(&mut self, updates: &[Update]) -> (bool, usize) {
        #[cfg(all(windows, target_arch = "x86"))]
        {
            let bytes = scene::wire::encode_updates(updates);
            let mut args = civ3r_native::Publish { scene: wine_unixlib::pe::buffer(&bytes) };
            let status = self.lib.call(civ3r_native::CALL_PUBLISH, &mut args);
            if status != 0 && status != wine_unixlib::STATUS_DEVICE_BUSY {
                log::debug!("native viewer: publish: status {status:#x}");
            }
            (status == 0, bytes.len())
        }
        #[cfg(not(all(windows, target_arch = "x86")))]
        {
            let _ = updates;
            (false, 0)
        }
    }

    fn take_log(&mut self) -> Vec<civ3r_native::log_lines::LogLine> {
        #[cfg(all(windows, target_arch = "x86"))]
        {
            loop {
                let mut args = civ3r_native::TakeLog { out: wine_unixlib::pe::buffer_mut(&mut self.log), len: 0, _pad: 0 };
                match self.lib.call(civ3r_native::CALL_LOG, &mut args) {
                    0 => return civ3r_native::log_lines::decode(&self.log[..args.len as usize]).unwrap_or_default(),
                    wine_unixlib::STATUS_BUFFER_TOO_SMALL => self.log.resize(args.len as usize, 0),
                    _ => return Vec::new(),
                }
            }
        }
        #[cfg(not(all(windows, target_arch = "x86")))]
        {
            let _ = &self.log;
            Vec::new()
        }
    }

    fn take_input(&mut self) -> Vec<Input> {
        #[cfg(all(windows, target_arch = "x86"))]
        {
            loop {
                let mut args = civ3r_native::TakeInput { out: wine_unixlib::pe::buffer_mut(&mut self.input), len: 0, _pad: 0 };
                match self.lib.call(civ3r_native::CALL_INPUT, &mut args) {
                    0 => return scene::wire::decode_input(&self.input[..args.len as usize]).unwrap_or_default(),
                    wine_unixlib::STATUS_BUFFER_TOO_SMALL => self.input.resize(args.len as usize, 0),
                    _ => return Vec::new(),
                }
            }
        }
        #[cfg(not(all(windows, target_arch = "x86")))]
        {
            let _ = &self.input;
            Vec::new()
        }
    }
}
