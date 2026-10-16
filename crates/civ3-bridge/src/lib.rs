//! The one Civilization III specific crate: runs inside the game process.
// No unsafe code here; the only exception is the exported DLL entry symbol
// that `inproc::entry!` generates (an unsafe *attribute*, `no_mangle`).
#![deny(unsafe_code)]

use std::time::Duration;

use civ3_sys::Civ3;

inproc::entry!(init);

fn init() {
    let _ = inproc::log_to_file(std::path::Path::new("civ3-bridge.log"), log::LevelFilter::Debug);
    log::info!("civ3-bridge loaded");
    let game = match Civ3::attach() {
        Ok(g) => g,
        Err(e) => return log::error!("not attaching: {e}"),
    };
    log::info!("detected build {:?}", game.build);
    let mut last = None;
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let state = game.view().map(|v| (v, game.map().ok()));
        if state.as_ref().ok() != last.as_ref() {
            match &state {
                Ok((view, map)) => log::info!("view {view:?} map {map:?}"),
                Err(e) => log::warn!("reading view: {e}"),
            }
            if let Ok((_, Some(map))) = &state {
                match game.tiles(map) {
                    Ok(tiles) => {
                        log::info!("{} tiles; first {:?}", tiles.len(), tiles.first());
                    }
                    Err(e) => log::warn!("reading tiles: {e}"),
                }
            }
            last = state.ok();
        }
    }
}
