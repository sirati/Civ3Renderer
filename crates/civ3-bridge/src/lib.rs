//! The one Civilization III specific crate: runs inside the game process.
// No unsafe code here; the only exception is the exported DLL entry symbol
// that `inproc::entry!` generates (an unsafe *attribute*, `no_mangle`).
#![deny(unsafe_code)]

pub mod art;
pub mod art_schedule;
pub mod assets;
pub mod city_hud;
pub mod city_labels;
pub mod city_site;
pub mod cursor;
pub mod district_art;
pub mod districts;
pub mod edge;
pub mod effects;
pub mod game_render;
pub mod game_size;
pub mod interface_parts;
pub mod label_check;
pub mod looks;
pub mod minimap;
pub mod modal;
pub mod preload;
pub mod prefs;
pub mod preview;
pub mod revealed;
pub mod replay;
pub mod replay_thread;
pub mod rings;
pub mod rules;
pub mod targeting;
pub mod scene_map;
pub mod text;
pub mod ui;
pub mod ui_screen;
#[cfg(windows)]
pub mod ui_trace;
pub mod units;
pub mod world;
#[cfg(windows)]
pub mod game_window;
#[cfg(windows)]
pub mod live;
pub mod presenter;
#[cfg(windows)]
pub mod trace;

#[cfg(windows)]
use civ3_sys::Civ3;

inproc::entry!(init, early = early);

/// In `DllMain`, before the game's own code runs (`game_window`).
#[cfg(windows)]
fn early() {
    game_window::early();
}

#[cfg(windows)]
fn init() {
    let _ = inproc::log_to_file(std::path::Path::new("civ3-bridge.log"), log::LevelFilter::Debug);
    // (The wall clock, so that the log's times since loading can be
    // matched with outside measurements.)
    let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    log::info!("civ3-bridge loaded (unix time {:.3})", unix.as_secs_f64());
    for note in game_window::take_notes() {
        log::info!("{note}");
    }
    let game = match Civ3::attach() {
        Ok(g) => g,
        Err(e) => return log::error!("not attaching: {e}"),
    };
    log::info!("detected build {:?}", game.build);
    // `CIV3R_GAME_RENDER=off` (the default with the native viewer, its
    // window not shown): the game skips its pixel work, the map's
    // included (`game_render`).
    let var = |name: &str| std::env::var(name).ok();
    let game_draws = game_render::game_draws(var("CIV3R_GAME_RENDER").as_deref(), var("CIV3R_VIEWER").as_deref(), var("CIV3R_GAME_WINDOW").as_deref());
    ui_trace::set_game_draws(game_draws);
    if std::env::var("CIV3R_GAME_MAP").is_ok_and(|v| v == "off") || !game_draws {
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
            // Without it, the game's own window is the only one.
            if game_window::hidden() {
                inproc::hidden::set_hiding(false);
                for w in inproc::window::Window::all_of_this_process() {
                    w.show();
                }
                log::warn!("the game's window is shown: there is no viewer");
            }
        }
    }
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let r = std::sync::atomic::Ordering::Relaxed;
        log::debug!("calls: draw_tiles {} sprite_draw_on_map {}", trace::DRAW_TILES_CALLS.load(r), trace::SPRITE_CALLS.load(r));
    }
}
