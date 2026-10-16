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
    /// The generation of the latest assets (`Scene::assets`).
    generation: u64,
    /// The pixels last sent per run-time sprite (unchanged ones are not
    /// sent again: the renderer keeps them, also across new art).
    sent_patches: std::collections::HashMap<String, Vec<u8>>,
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
        std::thread::Builder::new().name("civ3r-viewer".into()).spawn(move || {
            if let Err(e) = tile_viewer::run(config, a, display) {
                log::error!("viewer: {e}");
            }
        })?;
        Ok(Presenter { sink: Sink::InProcess { assets, driver }, sent: Scene::default(), generation: 1, sent_patches: Default::default() })
    }

    /// The native Linux viewer (only in a Wine process with
    /// `civ3r_native.dll`).
    pub fn native(native: Native) -> Presenter {
        Presenter { sink: Sink::Native(native), sent: Scene::default(), generation: 0, sent_patches: Default::default() }
    }

    /// New render assets: `compiled` for this side, `wire` for a renderer
    /// that compiles its own copy.
    pub fn set_assets(&mut self, compiled: &Arc<Compiled>, wire: &scene::wire::Assets) {
        // Scenes from now on refer to this generation; the renderer draws
        // them with these assets only.
        self.generation += 1;
        match &mut self.sink {
            Sink::InProcess { assets, .. } => assets.set(compiled.clone(), self.generation),
            Sink::Native(n) => n.set_assets(&scene::wire::Assets { generation: self.generation, ..wire.clone() }),
        }
    }

    /// Makes the renderer's scene `scene`, sending only what changed.
    /// While the renderer has not taken the last batch, nothing is sent: what
    /// changed goes with a later one (a slow renderer does not queue up
    /// batches). Whether the renderer has everything (else publish again).
    pub fn publish(&mut self, mut scene: Scene) -> bool {
        if let Sink::InProcess { driver, .. } = &self.sink && driver.busy() {
            return false;
        }
        scene.assets = self.generation;
        scene.patches.retain(|p| self.sent_patches.get(&p.sprite) != Some(&p.rgba));
        let updates = scene.updates_since(&self.sent);
        if updates.is_empty() {
            return true;
        }
        let taken = match &mut self.sink {
            Sink::InProcess { driver, .. } => driver.send(updates),
            Sink::Native(n) => n.publish(&updates),
        };
        if taken {
            for p in scene.patches.drain(..) {
                self.sent_patches.insert(p.sprite, p.rgba);
            }
            self.sent = scene;
        }
        taken
    }

    pub fn take_input(&mut self) -> Vec<Input> {
        match &mut self.sink {
            Sink::InProcess { driver, .. } => driver.take_input(),
            Sink::Native(n) => n.take_input(),
        }
    }
}

/// The native viewer (only in a Wine process with `civ3r_native.dll`).
pub struct Native {
    #[cfg(all(windows, target_arch = "x86"))]
    lib: wine_unixlib::pe::UnixLib,
    fullscreen: bool,
    input: Vec<u8>,
}

impl Native {
    #[cfg(all(windows, target_arch = "x86"))]
    pub fn load(fullscreen: bool) -> Result<Native, String> {
        let lib = wine_unixlib::pe::UnixLib::load("civ3r_native.dll").map_err(|s| format!("civ3r_native.dll: status {s:#x}"))?;
        Ok(Native { lib, fullscreen, input: vec![0; 64 * 1024] })
    }

    #[cfg(not(all(windows, target_arch = "x86")))]
    pub fn load(_fullscreen: bool) -> Result<Native, String> {
        Err("the native viewer needs a 32-bit Wine process".into())
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

    /// Whether the native viewer took `updates` (not when busy).
    fn publish(&mut self, updates: &[Update]) -> bool {
        #[cfg(all(windows, target_arch = "x86"))]
        {
            let bytes = scene::wire::encode_updates(updates);
            let mut args = civ3r_native::Publish { scene: wine_unixlib::pe::buffer(&bytes) };
            let status = self.lib.call(civ3r_native::CALL_PUBLISH, &mut args);
            if status != 0 && status != wine_unixlib::STATUS_DEVICE_BUSY {
                log::debug!("native viewer: publish: status {status:#x}");
            }
            status == 0
        }
        #[cfg(not(all(windows, target_arch = "x86")))]
        {
            let _ = updates;
            false
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
