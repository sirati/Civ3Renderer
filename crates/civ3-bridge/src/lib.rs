//! The one Civilization III specific crate: runs inside the game process.
// No unsafe code here; the only exception is the exported DLL entry symbol
// that `inproc::entry!` generates (an unsafe *attribute*, `no_mangle`).
#![deny(unsafe_code)]

pub mod trace;

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
    match trace::install(game) {
        // Hooks stay installed for the life of the process.
        Ok(tracer) => std::mem::forget(tracer),
        Err(e) => log::error!("installing tracer: {e}"),
    }
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let r = std::sync::atomic::Ordering::Relaxed;
        log::debug!("calls: draw_tiles {} sprite_draw_on_map {}", trace::DRAW_TILES_CALLS.load(r), trace::SPRITE_CALLS.load(r));
    }
}
