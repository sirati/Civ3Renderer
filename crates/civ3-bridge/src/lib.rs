//! The one Civilization III specific crate: runs inside the game process.
// No unsafe code here; the only exception is the exported DLL entry symbol
// that `inproc::entry!` generates (an unsafe *attribute*, `no_mangle`).
#![deny(unsafe_code)]

pub mod assets;
pub mod rules;
pub mod scene_map;
pub mod world;
#[cfg(windows)]
pub mod live;
#[cfg(windows)]
pub mod trace;

#[cfg(windows)]
use civ3_sys::Civ3;

inproc::entry!(init);

#[cfg(windows)]
fn init() {
    let _ = inproc::log_to_file(std::path::Path::new("civ3-bridge.log"), log::LevelFilter::Debug);
    log::info!("civ3-bridge loaded");
    let game = match Civ3::attach() {
        Ok(g) => g,
        Err(e) => return log::error!("not attaching: {e}"),
    };
    log::info!("detected build {:?}", game.build);
    if std::env::var("CIV3R_GAME_MAP").is_ok_and(|v| v == "off") {
        log::info!("the game's map rendering is off");
        trace::GAME_MAP_OFF.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    match trace::install(game) {
        // Hooks stay installed for the life of the process.
        Ok(tracer) => std::mem::forget(tracer),
        Err(e) => log::error!("installing tracer: {e}"),
    }
    if std::env::var_os("CIV3R_VIEWER").is_some() {
        if let Err(e) = live::start(game) {
            log::error!("starting the renderer: {e}");
        }
    }
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let r = std::sync::atomic::Ordering::Relaxed;
        log::debug!("calls: draw_tiles {} sprite_draw_on_map {}", trace::DRAW_TILES_CALLS.load(r), trace::SPRITE_CALLS.load(r));
    }
}
