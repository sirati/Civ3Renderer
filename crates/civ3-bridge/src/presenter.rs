//! Where the scenes go: a viewer window on a thread of this (Windows)
//! process, or — inside Wine — a native Linux viewer in the same process,
//! reached through the unixlib of `civ3_native.dll` (`civ3r-native`).

use std::sync::Arc;

use render_desc::Compiled;
use scene::{Input, Link, Scene};

pub enum Presenter {
    InProcess { assets: tile_viewer::Assets, link: Link },
    Native(Native),
}

impl Presenter {
    /// A viewer window on a thread of this process.
    pub fn in_process(config: tile_viewer::ViewerConfig, compiled: Arc<Compiled>) -> std::io::Result<Presenter> {
        let assets = tile_viewer::Assets::new(compiled);
        let link = Link::new();
        let (a, l) = (assets.clone(), link.clone());
        std::thread::Builder::new().name("civ3r-viewer".into()).spawn(move || {
            if let Err(e) = tile_viewer::run(config, a, l) {
                log::error!("viewer: {e}");
            }
        })?;
        Ok(Presenter::InProcess { assets, link })
    }

    /// New render assets: `compiled` for this side, `wire` for a renderer
    /// that compiles its own copy.
    pub fn set_assets(&mut self, compiled: &Arc<Compiled>, wire: &scene::wire::Assets) {
        match self {
            Presenter::InProcess { assets, .. } => assets.replace(compiled.clone()),
            Presenter::Native(n) => n.set_assets(wire),
        }
    }

    pub fn publish(&mut self, scene: Scene) {
        match self {
            Presenter::InProcess { link, .. } => link.publish(scene),
            Presenter::Native(n) => n.publish(&scene),
        }
    }

    pub fn take_input(&mut self) -> Vec<Input> {
        match self {
            Presenter::InProcess { link, .. } => link.take_input(),
            Presenter::Native(n) => n.take_input(),
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

    fn publish(&mut self, scene: &Scene) {
        #[cfg(all(windows, target_arch = "x86"))]
        {
            let bytes = scene::wire::encode_scene(scene);
            let mut args = civ3r_native::Publish { scene: wine_unixlib::pe::buffer(&bytes) };
            let status = self.lib.call(civ3r_native::CALL_PUBLISH, &mut args);
            if status != 0 {
                log::debug!("native viewer: publish: status {status:#x}");
            }
        }
        #[cfg(not(all(windows, target_arch = "x86")))]
        let _ = scene;
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
