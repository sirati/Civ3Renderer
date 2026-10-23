//! Where scenes go: a viewer in this Windows process, the native Linux
//! viewer reached through `civ3r_native.dll`, or an authenticated native
//! viewer in a separate process. Each keeps its world and receives changes.

use std::sync::Arc;

use render_desc::Compiled;
use scene::{Driver, Input, Scene, Update};

/// The publisher's bell: rung when there is something new to show (the
/// game presented a changed frame, input arrived, a preview stage is
/// ready); the publisher waits for it between its rounds, at most its poll
/// interval (for what nobody rings for, e.g. units moving on the map).
pub static WAKE: civ3r_sync::Wake = civ3r_sync::Wake::new();

pub struct Presenter {
    sink: Sink,
    /// The scene as the renderer has it (what was sent so far).
    sent: Scene,
    /// The generation of the assets scenes refer to (`Scene::assets`), and
    /// of the latest prepared (see [`Presenter::prepare_assets`]).
    generation: u64,
    prepared: u64,
    /// Last generation actually announced to a remote renderer.
    announced: u64,
    /// Which sprite table every prepared generation belongs to: scenes
    /// only ever name a generation of the table this side compiled
    /// ([`Tables`]).
    tables: Tables,
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
    External(viewer_transport::Client),
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
        let quit = display.input_sender();
        std::thread::Builder::new().name("civ3r-viewer".into()).spawn(move || {
            struct QuitWhenStopped(scene::InputSender);
            impl Drop for QuitWhenStopped { fn drop(&mut self) { let _ = self.0.send(Input::Quit); } }
            let _quit = QuitWhenStopped(quit);
            #[cfg(windows)]
            if !inproc::dpi::thread_per_monitor_aware() {
                log::warn!("viewer: its thread could not be made DPI aware (its window in logical pixels)");
            }
            if let Err(e) = tile_viewer::run(config, a, display) {
                log::error!("viewer: {e}");
            }
            // The drop guard also covers initialization errors and unwinding
            // before the viewer's normal close handler exists.
        })?;
        Ok(Presenter { sink: Sink::InProcess { assets, driver }, sent: Scene::default(), generation: 1, prepared: 1, announced: 1, tables: Tables::adopted(1), sent_patches: Default::default(), reserve: 0, stats: PublishStats::default() })
    }

    /// The native Linux viewer (only in a Wine process with
    /// `civ3r_native.dll`).
    pub fn native(native: Native) -> Presenter {
        Self::remote(Sink::Native(native))
    }

    /// Connects to the native viewer started by the launcher. Numeric
    /// loopback addresses keep the transport on this machine; the
    /// per-launch token authenticates the session before any scenes or art.
    /// Connecting also waits for the viewer's GPU limits.
    pub fn external(fullscreen: bool) -> std::io::Result<Presenter> {
        let address = std::env::var("CIV3R_VIEWER_ADDR")
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "CIV3R_VIEWER_ADDR is required for the external viewer"))?;
        let address = external_address(&address)?;
        let token = std::env::var("CIV3R_VIEWER_TOKEN")
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "CIV3R_VIEWER_TOKEN is required for the external viewer"))?;
        if token.is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "CIV3R_VIEWER_TOKEN must not be empty"));
        }
        let client = viewer_transport::Client::connect(address, &token, fullscreen)?;
        client.wake_on_input(&WAKE);
        Ok(Self::remote(Sink::External(client)))
    }

    fn remote(sink: Sink) -> Presenter {
        Presenter { sink, sent: Scene::default(), generation: 0, prepared: 0, announced: 0, tables: Tables::default(), sent_patches: Default::default(), reserve: 0, stats: PublishStats::default() }
    }

    /// The remote GPU's texture limit, before compiling matching atlas
    /// tables. The in-process viewer probes its own GPU separately.
    pub fn max_texture(&self) -> Option<u32> {
        match &self.sink {
            Sink::InProcess { .. } => None,
            Sink::Native(n) => n.max_texture(),
            Sink::External(client) => client.max_texture(),
        }
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
        // (An addition to a whole description given up is not sent: the
        // renderer would only grow art no scene will name.)
        if self.tables.prepare(self.prepared, wire.append) && !matches!(self.sink, Sink::InProcess { .. }) {
            self.announced = self.prepared;
            let wire = scene::wire::Assets { generation: self.prepared, reserve: std::mem::take(&mut self.reserve), ..wire.clone() };
            match &mut self.sink {
                Sink::Native(n) => n.set_assets(&wire),
                Sink::External(client) => {
                    if let Err(error) = client.set_assets(&wire) {
                        log::error!("external viewer: assets: {error}");
                    }
                }
                Sink::InProcess { .. } => unreachable!(),
            }
        }
        self.prepared
    }

    /// Scenes from now on refer to the assets of `generation` (prepared,
    /// `compiled` on this side); the renderer draws them with these assets
    /// only. An in-process renderer gets them (pixels and all) now.
    /// Refused (false, nothing changes) for a generation that is not of
    /// the renderer's latest sprite table ([`Tables::adoptable`]).
    pub fn adopt_assets(&mut self, compiled: &Arc<Compiled>, generation: u64) -> bool {
        if let Err(why) = self.tables.adopt(generation) {
            log::info!("art: generation {generation} not adopted: {why}");
            return false;
        }
        self.generation = generation;
        if let Sink::InProcess { assets, .. } = &mut self.sink {
            assets.set(compiled.clone(), generation);
        }
        true
    }

    /// The whole description of `generation` was given up (its art
    /// folders are no longer shown): neither it nor an addition to it is
    /// ever named by a scene, nor is such an addition sent.
    pub fn abandon_assets(&mut self, generation: u64) {
        self.tables.abandon(generation);
    }

    /// Whether a prepared result belongs to a table replaced or abandoned.
    pub fn assets_superseded(&self, generation: u64) -> bool {
        self.tables.superseded(generation)
    }

    /// Scenes from now on refer to growth `generation` (prepared) of the
    /// assets; an in-process renderer gets `growth` (its pixels) now, a
    /// native one compiled it itself (see [`Presenter::art_status`]).
    /// Refused (false, nothing changes; this side must not apply it
    /// either) for a growth of a sprite table scenes do not name now.
    pub fn adopt_growth(&mut self, growth: &Arc<render_desc::Growth>, generation: u64) -> bool {
        if let Err(why) = self.tables.adopt(generation) {
            log::info!("art: growth {generation} not adopted: {why}");
            return false;
        }
        self.generation = generation;
        if let Sink::InProcess { assets, .. } = &mut self.sink {
            assets.grow(growth.clone(), generation);
        }
        true
    }

    /// Asks the renderer to make room in its atlas for this many colour and
    /// index pages (art expected soon): one given the art now, one
    /// compiling its own with the next art sent (index pages only).
    pub fn reserve(&mut self, pages: u32, index_pages: u32) {
        match &self.sink {
            Sink::InProcess { assets, .. } => assets.reserve(pages, index_pages),
            Sink::Native(_) | Sink::External(_) => self.reserve = index_pages,
        }
    }

    /// The latest art generation a renderer that compiles its own art has
    /// (scenes referring to it are drawn at once); `None` for one given
    /// the art.
    pub fn art_status(&mut self) -> Option<u64> {
        match &mut self.sink {
            Sink::InProcess { .. } => None,
            Sink::Native(n) => Some(n.status()),
            Sink::External(client) => Some(client.status()),
        }
    }

    /// Whether the atlas used by current scenes is uploaded. Optional preview
    /// replacement must not wait for an abandoned, newer announcement.
    pub fn current_assets_ready(&mut self) -> bool {
        let uploaded = match &mut self.sink {
            Sink::InProcess { assets, .. } => assets.uploaded_generation(),
            Sink::Native(n) => n.uploaded_generation(),
            Sink::External(client) => client.uploaded_generation(),
        };
        uploaded >= self.generation
    }

    /// Whether every art request sent so far has reached the drawable GPU
    /// atlas. Background preload starts only then, bounding its backlog to
    /// one chunk even when compilation is much faster than uploading.
    pub fn preload_ready(&mut self) -> bool {
        let uploaded = match &mut self.sink {
            Sink::InProcess { assets, .. } => assets.uploaded_generation(),
            Sink::Native(n) => n.uploaded_generation(),
            Sink::External(client) => client.uploaded_generation(),
        };
        // Suppressed additions to abandoned tables never reach the renderer.
        // An in-process renderer receives art only when it is adopted.
        uploaded >= self.announced.max(self.generation)
    }

    /// Makes the renderer's scene `scene`, sending only what changed.
    /// While the renderer has not taken the last batch, nothing is sent: what
    /// changed goes with a later one (a slow renderer does not queue up
    /// batches). Whether the renderer has everything (else publish again).
    pub fn publish(&mut self, mut scene: Scene) -> bool {
        self.stats = PublishStats::default();
        let busy = match &self.sink {
            Sink::InProcess { driver, .. } => driver.busy(),
            Sink::External(client) => client.busy(),
            Sink::Native(_) => false,
        };
        if busy { return false; }
        let t = std::time::Instant::now();
        scene.assets = self.generation;
        scene.patches.retain(|p| self.sent_patches.get(&p.sprite) != Some(&p.rgba));
        let mut updates = scene.updates_since(&self.sent);
        self.stats.diff = t.elapsed();
        if updates.is_empty() {
            return true;
        }
        // How much of each slide has passed as this batch goes out: the
        // renderer's clock for it starts there (`crate::unit_slides`).
        let now = std::time::Instant::now();
        for u in &mut updates {
            if let Update::Motions(motions) = u {
                for m in motions { m.elapsed_ms = crate::unit_slides::elapsed_ms(m.serial, now); }
            }
        }
        // (Every message says whether the game answers, `send_host`.)
        updates.push(Update::Host(crate::heartbeat::status()));
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
            Sink::External(client) => {
                let (taken, bytes) = client.publish(&updates);
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

    /// Sends the host's status (`crate::heartbeat`) on its own, whatever
    /// changed: the viewer takes no status for a while as the publisher or
    /// the connection gone silent. Not part of the scene sent.
    pub fn send_host(&mut self) {
        let updates = vec![Update::Host(crate::heartbeat::status())];
        match &mut self.sink {
            Sink::InProcess { driver, .. } => {
                driver.send(updates);
            }
            Sink::Native(n) => {
                n.publish(&updates);
            }
            Sink::External(client) => {
                client.publish(&updates);
            }
        }
    }

    pub fn take_input(&mut self) -> Vec<Input> {
        match &mut self.sink {
            Sink::InProcess { driver, .. } => driver.take_input(),
            Sink::Native(n) => n.take_input(),
            Sink::External(client) => client.take_input(),
        }
    }

    /// Writes what the renderer logged into this log (the native viewer's
    /// lines from `info` up; an in-process viewer logs here itself).
    pub fn forward_log(&mut self) {
        match &mut self.sink {
            Sink::Native(n) => {
                for line in n.take_log() {
                    log::log!(target: "civ3r_native", line.level, "{}: {}", line.target, line.message);
                }
            }
            Sink::External(client) => {
                for line in client.take_log() {
                    log::info!(target: "civ3r_external", "{line}");
                }
            }
            Sink::InProcess { .. } => {}
        }
    }

    /// Which renderer this is, for the log.
    pub fn describe(&self) -> &'static str {
        match self.sink {
            Sink::InProcess { .. } => "in-process: the viewer is a window of the game's own (32-bit Windows) process",
            Sink::Native(_) => "native: the viewer is native Linux code in the game's process (civ3r_native.dll's unixlib, Wine's new WoW64)",
            Sink::External(_) => "external: the viewer is a native process connected over an authenticated loopback socket",
        }
    }
}

/// The sprite tables (whole descriptions) behind the art generations, so
/// that a scene never names a generation whose sprite ids mean other art
/// in the renderer than on this side. Each prepared generation is of one
/// whole description (its own, or the one an addition grows). A renderer
/// compiling its own copy grows the latest whole it was sent and drops
/// what it had for an older one, whether this side finished that whole or
/// gave it up: so only generations of the latest whole are adopted, a
/// growth only while its whole is the adopted one, and nothing of a whole
/// given up ([`Tables::abandon`]).
#[derive(Debug, Default)]
pub struct Tables {
    /// Prepared and not adopted yet: generation, its whole description.
    of: std::collections::BTreeMap<u64, u64>,
    /// The latest whole prepared, and whether it was given up.
    latest: u64,
    abandoned: bool,
    /// The whole description scenes name now.
    adopted: u64,
}

impl Tables {
    /// Whole description `generation` in use from the start.
    pub fn adopted(generation: u64) -> Tables {
        Tables { latest: generation, adopted: generation, ..Tables::default() }
    }

    /// Records prepared `generation` (an addition: to the latest whole);
    /// false for an addition to a whole given up (not to be sent).
    pub fn prepare(&mut self, generation: u64, append: bool) -> bool {
        if !append {
            self.latest = generation;
            self.abandoned = false;
        }
        self.of.insert(generation, self.latest);
        !(append && self.abandoned)
    }

    /// Whole description `generation` given up.
    pub fn abandon(&mut self, generation: u64) {
        if generation == self.latest {
            self.abandoned = true;
        }
        self.of.retain(|_, whole| *whole != generation);
    }

    /// Whether a result can be discarded without waiting for its upload.
    pub fn superseded(&self, generation: u64) -> bool {
        self.abandoned || self.of.get(&generation).is_none_or(|whole| *whole != self.latest)
    }

    /// Whether scenes may name `generation` from now on.
    pub fn adoptable(&self, generation: u64) -> Result<(), &'static str> {
        let Some(&whole) = self.of.get(&generation) else { return Err("not prepared, or of a whole description given up") };
        if whole != self.latest {
            return Err("of a sprite table superseded by a newer whole description");
        }
        if self.abandoned {
            return Err("of a whole description given up");
        }
        if whole != generation {
            if whole != self.adopted {
                return Err("grows a sprite table that is not the adopted one");
            }
            if self.of.range(..generation).any(|(_, prior)| *prior == whole) {
                return Err("an earlier growth of this sprite table is not adopted yet");
            }
        }
        Ok(())
    }

    /// Adopts `generation` if [`Tables::adoptable`].
    pub fn adopt(&mut self, generation: u64) -> Result<(), &'static str> {
        self.adoptable(generation)?;
        self.adopted = self.of[&generation];
        self.of.retain(|g, _| *g > generation);
        Ok(())
    }
}

fn external_address(value: &str) -> std::io::Result<std::net::SocketAddr> {
    let address: std::net::SocketAddr = value.parse()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "CIV3R_VIEWER_ADDR must be a numeric loopback socket address"))?;
    if !address.ip().is_loopback() || address.port() == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "CIV3R_VIEWER_ADDR must use a loopback address and a nonzero port"));
    }
    Ok(address)
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
    #[cfg(all(windows, target_arch = "x86"))]
    quit_reported: bool,
    /// Input taken by a thread of its own (`civ3r-input`) as it arrives,
    /// which rings [`WAKE`]: the publisher goes on at once. `None` until the
    /// first take, and with an older library that cannot wait for input.
    #[cfg(all(windows, target_arch = "x86"))]
    arriving: Option<Option<Arc<civ3r_sync::Mailbox<Input>>>>,
}

/// How long one wait of the input thread for input lasts (it waits again).
#[cfg(all(windows, target_arch = "x86"))]
const INPUT_WAIT_MS: u32 = 200;

impl Native {
    #[cfg(all(windows, target_arch = "x86"))]
    pub fn load(fullscreen: bool) -> Result<Native, String> {
        let lib = wine_unixlib::pe::UnixLib::load("civ3r_native.dll").map_err(|s| format!("civ3r_native.dll: status {s:#x}"))?;
        Ok(Native { lib, upload_status: None, fullscreen, input: vec![0; 64 * 1024], log: vec![0; 16 * 1024], quit_reported: false, arriving: None })
    }

    /// The input thread's mailbox, started at the first call if the library
    /// waits for input.
    #[cfg(all(windows, target_arch = "x86"))]
    fn arriving(&mut self) -> Option<Arc<civ3r_sync::Mailbox<Input>>> {
        let lib = self.lib;
        self.arriving.get_or_insert_with(|| {
            let mut limits = civ3r_native::Limits::default();
            if lib.call(civ3r_native::CALL_LIMITS, &mut limits) != 0 || limits._pad & civ3r_native::LIMIT_INPUT_WAIT == 0 {
                log::info!("native viewer: input is polled (the library cannot wait for it)");
                return None;
            }
            let mailbox = Arc::new(civ3r_sync::Mailbox::new());
            let to = mailbox.clone();
            let spawned = std::thread::Builder::new().name("civ3r-input".into()).spawn(move || {
                let mut buffer = vec![0u8; 64 * 1024];
                let quit = |to: &civ3r_sync::Mailbox<Input>| {
                    to.send(Input::Quit);
                    WAKE.ring();
                };
                loop {
                    let mut args = civ3r_native::TakeInput { out: wine_unixlib::pe::buffer_mut(&mut buffer), len: 0, _pad: INPUT_WAIT_MS };
                    match lib.call(civ3r_native::CALL_INPUT, &mut args) {
                        0 => match scene::wire::decode_input(&buffer[..args.len as usize]) {
                            Ok(input) if input.is_empty() => {}
                            Ok(input) => {
                                input.into_iter().for_each(|i| to.send(i));
                                WAKE.ring();
                            }
                            Err(error) => {
                                log::error!("native viewer input: {error}");
                                return quit(&to);
                            }
                        },
                        wine_unixlib::STATUS_BUFFER_TOO_SMALL => buffer.resize(args.len as usize, 0),
                        status => {
                            log::error!("native viewer input: status {status:#x}");
                            return quit(&to);
                        }
                    }
                }
            });
            match spawned {
                Ok(_) => Some(mailbox),
                Err(e) => {
                    log::warn!("native viewer: no input thread ({e}): input is polled");
                    None
                }
            }
        }).clone()
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
            if self.quit_reported { return Vec::new(); }
            if let Some(arriving) = self.arriving() {
                return arriving.drain().collect();
            }
            loop {
                let mut args = civ3r_native::TakeInput { out: wine_unixlib::pe::buffer_mut(&mut self.input), len: 0, _pad: 0 };
                match self.lib.call(civ3r_native::CALL_INPUT, &mut args) {
                    0 => match scene::wire::decode_input(&self.input[..args.len as usize]) {
                        Ok(input) => return input,
                        Err(error) => {
                            log::error!("native viewer input: {error}");
                            self.quit_reported = true;
                            return vec![Input::Quit];
                        }
                    },
                    wine_unixlib::STATUS_BUFFER_TOO_SMALL => self.input.resize(args.len as usize, 0),
                    status => {
                        log::error!("native viewer input: status {status:#x}");
                        self.quit_reported = true;
                        return vec![Input::Quit];
                    },
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

#[cfg(test)]
mod tests {
    use super::{external_address, Presenter, Sink};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    use scene::{Input, Scene};
    use viewer_transport::{Client, Message, accept, read_message, write_message};

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn input_marker(presenter: &mut Presenter, width: u32) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let inputs = presenter.take_input();
            if inputs.iter().any(|i| matches!(i, Input::Resized { width: w, .. } if *w == width)) { return; }
            assert!(!inputs.iter().any(|i| matches!(i, Input::Quit)), "viewer disconnected before marker");
            assert!(Instant::now() < deadline, "viewer marker timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn preview_replacement_waits_only_for_adopted_assets_not_abandoned_uploads() {
        let (driver, _display) = scene::connect();
        let compiled = crate::assets::compile("grid = { layout = 'orthogonal', step = [1, 1] }", civ3_assets::GameFiles::with_roots(Vec::new())).unwrap();
        let assets = tile_viewer::Assets::new(std::sync::Arc::new(compiled));
        let mut presenter = Presenter::remote(Sink::InProcess { driver, assets });
        assert!(presenter.current_assets_ready());
        // Mirror a remote-only optional whole announcement that was canceled
        // before an upload. The active title/menu atlas remains generation0.
        presenter.announced = 9;
        presenter.tables.prepare(9, false);
        presenter.abandon_assets(9);
        assert!(!presenter.preload_ready(), "background preload still honors global backpressure");
        assert!(presenter.current_assets_ready(), "latest preview replacement must recover without waiting for canceled optional art");
    }

    #[test]
    fn external_viewer_accepts_only_numeric_loopback_sessions() {
        for address in ["127.0.0.1:41877", "[::1]:41877"] {
            assert!(external_address(address).is_ok(), "{address}");
        }
        for address in ["localhost:41877", "192.168.0.1:41877", "[::]:41877", "0.0.0.0:41877", "127.0.0.1:0", "127.0.0.1", ""] {
            assert_eq!(external_address(address).unwrap_err().kind(), std::io::ErrorKind::InvalidInput, "{address}");
        }
    }

    #[test]
    fn external_art_readiness_scene_backpressure_and_disconnect() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (step, advance) = mpsc::channel();
        let (observed, received) = mpsc::channel();
        let host = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let (mut stream, fullscreen) = accept(stream, TOKEN, 8192).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            stream.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
            assert!(fullscreen);
            let Message::Assets(bytes) = read_message(&mut stream).unwrap() else { panic!("expected assets") };
            let assets = scene::wire::decode_assets(&bytes).unwrap();
            assert_eq!(assets.generation, 1);
            assert_eq!(assets.reserve, 7);
            assert!(!assets.append);
            observed.send(()).unwrap();
            advance.recv_timeout(Duration::from_secs(5)).unwrap();
            write_message(&mut stream, &Message::Status { processed: 1, uploaded: 0 }).unwrap();
            write_message(&mut stream, &Message::Input(scene::wire::encode_input(&[Input::Resized { width: 1, height: 1 }]))).unwrap();
            let Message::Scene { serial, bytes } = read_message(&mut stream).unwrap() else { panic!("expected scene") };
            let mut scene = Scene::default();
            for update in scene::wire::decode_updates(&bytes).unwrap() { scene.apply(update); }
            assert_eq!(scene.assets, 1);
            assert_eq!(scene.tick, 17);
            observed.send(()).unwrap();
            advance.recv_timeout(Duration::from_secs(5)).unwrap();
            write_message(&mut stream, &Message::SceneTaken(serial)).unwrap();
            write_message(&mut stream, &Message::Status { processed: 1, uploaded: 1 }).unwrap();
            write_message(&mut stream, &Message::Input(scene::wire::encode_input(&[Input::Resized { width: 2, height: 1 }]))).unwrap();
            advance.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let client = Client::connect(address, TOKEN, true).unwrap();
        let mut presenter = Presenter::remote(Sink::External(client));
        assert_eq!(presenter.max_texture(), Some(8192));
        presenter.reserve(19, 7);
        let generation = presenter.prepare_assets(&scene::wire::Assets {
            description: "[grid]".into(), roots: vec!["C:\\Game".into()], generation: 0, append: false, reserve: 0,
        });
        assert_eq!(generation, 1);
        presenter.generation = generation;
        received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(presenter.art_status(), Some(0));
        assert!(!presenter.preload_ready());
        step.send(()).unwrap();
        input_marker(&mut presenter, 1);
        assert_eq!(presenter.art_status(), Some(1));
        assert!(!presenter.preload_ready(), "CPU art completion must not admit preload");
        assert!(presenter.publish(Scene { tick: 17, ..Default::default() }));
        received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!presenter.publish(Scene { tick: 18, ..Default::default() }));
        assert_eq!(presenter.sent.tick, 17, "a busy renderer must not advance the sent scene");
        step.send(()).unwrap();
        input_marker(&mut presenter, 2);
        assert!(presenter.preload_ready());
        presenter.abandon_assets(generation);
        let suppressed = presenter.prepare_assets(&scene::wire::Assets {
            description: "unused".into(), roots: Vec::new(), generation: 0, append: true, reserve: 0,
        });
        assert!(suppressed > generation);
        assert!(presenter.preload_ready(), "an unsent abandoned addition cannot wait for an upload");
        assert!(matches!(&presenter.sink, Sink::External(client) if !client.busy()));
        step.send(()).unwrap();
        host.join().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if presenter.take_input().iter().any(|i| matches!(i, Input::Quit)) { break; }
            assert!(Instant::now() < deadline, "a lost host must restore the game window through Quit");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(presenter.take_input().is_empty(), "disconnect must emit Quit once");
        assert!(!presenter.publish(Scene { tick: 18, ..Default::default() }));
    }
}

#[cfg(test)]
mod table_tests {
    use super::Tables;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// The crash of 2026-10-19: the whole description for a scenario's art
    /// (generation 2) was given up on this side after it was sent; the
    /// renderer built it anyway and grew it with the next addition (3).
    /// Adopting 3 had scenes name the scenario's table with the base
    /// table's sprite ids: garbage text, no backgrounds.
    #[test]
    fn nothing_of_a_whole_description_given_up_is_adopted_or_sent() {
        let mut t = Tables::adopted(1);
        assert!(t.prepare(2, false));
        assert!(t.prepare(3, true), "sent before the stop is known");
        t.abandon(2);
        assert!(!t.prepare(4, true), "an addition to a whole given up is not sent");
        for g in [2, 3, 4] {
            assert!(t.superseded(g), "generation {g} must not await an upload");
            assert!(t.adopt(g).is_err(), "generation {g}");
        }
        // The next whole description, and what grows it, are adopted.
        assert!(t.prepare(5, false));
        assert!(t.prepare(6, true));
        assert!(!t.superseded(5) && !t.superseded(6));
        assert_eq!(t.adopt(5), Ok(()));
        assert_eq!(t.adopt(6), Ok(()));
    }

    /// A renderer drops what it has for a whole description once a newer
    /// one arrives: neither the older whole nor its growths are adopted
    /// after that, even if they finish first on this side.
    #[test]
    fn a_newer_whole_description_supersedes_the_older_and_its_growths() {
        let mut t = Tables::adopted(1);
        assert!(t.prepare(2, true));
        assert!(t.prepare(3, false));
        assert!(t.adopt(2).is_err(), "a growth of table 1 after whole 3 was sent");
        assert!(t.prepare(4, false));
        assert!(t.adopt(3).is_err(), "whole 3 replaced by whole 4");
        assert!(t.prepare(5, true));
        assert!(t.adopt(5).is_err(), "grows whole 4, not adopted yet");
        assert_eq!(t.adopt(4), Ok(()));
        assert_eq!(t.adopt(5), Ok(()));
        // Growths in order of the adopted whole.
        assert!(t.prepare(6, true));
        assert!(t.prepare(7, true));
        assert!(t.adopt(7).is_err(), "a no-op completion must not skip an earlier growth awaiting upload");
        assert_eq!(t.adopt(6), Ok(()));
        assert_eq!(t.adopt(7), Ok(()));
        assert!(t.adopt(7).is_err(), "adopted once");
    }

    /// The art thread and the tables together, as the publisher drives
    /// them: a whole description for art folders no longer shown is
    /// stopped (the cancelled rebuild); the addition after it builds
    /// nothing and is refused, so the table this side has stays the one
    /// scenes name.
    #[test]
    fn a_cancelled_rebuild_leaves_scenes_on_the_table_they_had() {
        use crate::art::{ArtJob, ArtStop, ArtThread, Built, Work};
        const BASE: &str = "grid = { layout = \"orthogonal\", step = [8, 8] }\n\
            [images.a]\nsource = \"solid:1,2,3,255:8x8\"\n[sheets.a]\nimage = \"a\"\ncell = [8, 8]\ngroup = \"A\"\n\
            [[layers]]\nname = \"l\"\nsets = [\"A/*\", \"B@*\"]\n";
        let job = |work, generation| ArtJob {
            name: "test".into(),
            files: civ3_assets::GameFiles::with_roots(Vec::new()),
            work,
            pixels: false,
            max_page: render_desc::ANY_DEVICE_PAGE,
            generation,
            token: None,
            reason: "test".into(),
            wanted: Instant::now(),
            stop: None,
        };
        let mut tables = Tables::default();
        let mut art = ArtThread::start(None).unwrap();
        tables.prepare(1, false);
        art.send(job(Work::Whole(BASE.into()), 1));
        let changed = Arc::new(std::sync::atomic::AtomicU64::new(1));
        tables.prepare(2, false);
        art.send(ArtJob { stop: Some(ArtStop { started: 0, now: changed.clone() }), ..job(Work::Whole(BASE.into()), 2) });
        tables.prepare(3, true);
        art.send(job(Work::Addition("[images.b]\nsource = \"solid:9,9,9,255:4x4\"\n[sheets.b]\nimage = \"b\"\ncell = [4, 4]\ngroup = \"B@x\"\n".into()), 3));
        let mut adopted = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut done = 0;
        while done < 3 {
            assert!(Instant::now() < deadline);
            let Some(d) = art.try_done() else {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            };
            done += 1;
            match d.built {
                Ok(Built::Whole(_)) | Ok(Built::Growth(_)) => {
                    if tables.adopt(d.generation).is_ok() {
                        adopted.push(d.generation);
                    }
                }
                Err(e) => {
                    assert!(e.contains(crate::assets::STOPPED), "{e}");
                    tables.abandon(d.generation);
                }
            }
        }
        // Whole 1 finished first, while whole 2 was already sent: the
        // renderer replaced 1 by 2, which this side then gave up.
        assert!(adopted.is_empty(), "{adopted:?}");
        assert!(!tables.prepare(4, true), "nothing more grows the table given up");
    }
}
