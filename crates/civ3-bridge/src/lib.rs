//! The one Civilization III specific crate: runs inside the game process.
// No unsafe code here; the only exception is the exported DLL entry symbol
// that `inproc::entry!` generates (an unsafe *attribute*, `no_mangle`).
#![deny(unsafe_code)]

/// The longest a thread of ours sleeps waiting for work before it looks
/// again. Under Wine/FEX on macOS a parked thread's wake-up was seen lost
/// (a worker then waited forever with jobs queued); a bounded wait turns
/// such a loss into this much delay.
pub const BOUNDED_WAIT: std::time::Duration = std::time::Duration::from_millis(20);

/// Takes the next item of a tokio channel, waiting in bounded sleeps (see
/// [`BOUNDED_WAIT`]) instead of a parked receive; `None` once closed.
pub fn recv_bounded<T>(rx: &mut tokio::sync::mpsc::UnboundedReceiver<T>, poll: std::time::Duration) -> Option<T> {
    use tokio::sync::mpsc::error::TryRecvError;
    loop {
        match rx.try_recv() {
            Ok(item) => return Some(item),
            Err(TryRecvError::Disconnected) => return None,
            Err(TryRecvError::Empty) => std::thread::sleep(poll),
        }
    }
}

pub mod anim_steps;
pub mod build_queue;
pub mod art;
pub mod art_schedule;
pub mod assets;
pub mod city_hud;
pub mod city_labels;
pub mod city_screen;
pub mod city_site;
pub mod cursor;
pub mod district_art;
pub mod districts;
pub mod async_popups;
pub mod detached;
pub mod edge;
pub mod first_run;
pub mod effects;
pub mod game_render;
pub mod game_size;
pub mod heartbeat;
#[cfg(windows)]
mod jgl_hooks;
pub mod interface_parts;
pub mod label_check;
pub mod lit_orders;
#[cfg(test)]
mod leader_head_tests;
pub mod looks;
pub mod minimap;
#[cfg(any(windows, test))]
pub mod native_sliders;
pub mod native_camera;
pub mod modal;
pub mod pass_through;
pub mod preload;
pub mod prefs;
pub mod preview;
pub mod revealed;
pub mod replay;
pub mod replay_thread;
pub mod rings;
pub mod rules;
pub mod runtime;
pub mod targeting;
pub mod setup_controls;
pub mod stale_input;
pub mod scene_map;
pub mod text;
pub mod ui;
mod ui_background;
#[cfg(any(windows, test))]
mod ui_capture;
#[cfg(any(windows, test))]
mod ui_opacity;
pub mod ui_screen;
pub mod loading_cursor;
pub mod ui_transaction;
#[cfg(windows)]
pub mod ui_wheel;
#[cfg(windows)]
pub mod ui_trace;
pub mod units;
pub mod unit_notices;
pub mod unit_slides;
#[cfg(any(windows, test))]
pub mod unit_views;
pub mod world;
pub mod world_items;
#[cfg(any(windows, test))]
pub mod world_input;
#[cfg(windows)]
pub mod game_window;
#[cfg(windows)]
pub mod live;
pub mod presenter;
#[cfg(windows)]
pub mod trace;

#[cfg(windows)]
use civ3_sys::Civ3;

/// Our threads allocate from a heap of their own, never contending with
/// the game for its process heap (`inproc::alloc`).
#[cfg(windows)]
#[global_allocator]
static ALLOCATOR: inproc::alloc::PrivateHeap = inproc::alloc::PrivateHeap::new();

inproc::entry!(init, early = early);

/// In `DllMain`, before the game's own code runs (`game_window`).
#[cfg(windows)]
fn early() {
    if !pass_through::active_here() { return; }
    civ3_sys::audio_init::early();
    // Set the explicit map-only override before any load callback can
    // activate full capture and drawing suppression.
    ui_trace::configure_map_policy();
    game_window::early();
}

#[cfg(windows)]
fn init() {
    if !pass_through::active_here() {
        // (Only the note: no logger, hooks or threads.)
        let _ = std::fs::write("civ3-bridge.log", "civ3-bridge: pass-through (CIV3R_VIEWER unset): forwarding to ifc23_orig.dll only\n");
        return;
    }
    let _ = inproc::log_to_file(std::path::Path::new("civ3-bridge.log"), log::LevelFilter::Debug);
    // Render errors, every type once per run, from every side (`render_errors`).
    if let Err(e) = render_errors::log_to_file(std::path::Path::new(render_errors::FILE_NAME)) {
        log::warn!("no render error log ({}): {e}", render_errors::FILE_NAME);
    }
    log::info!("native audio failed-init cleanup: {:?}", civ3_sys::audio_init::status());
    // Hangs (a stuck allocator guard, a publisher that stopped) are noted
    // even when the log's writer is stuck too.
    static WATCHED: [(&str, &std::sync::atomic::AtomicU64); 1] = [("the publisher", &live::PUBLISHER_ROUNDS)];
    if let Err(e) = inproc::alloc::watchdog(std::path::Path::new("civ3-bridge-watchdog.log"), &WATCHED) {
        log::warn!("no hang watchdog: {e}");
    }
    // (The wall clock, so that the log's times since loading can be
    // matched with outside measurements.)
    let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    log::info!("civ3-bridge loaded (unix time {:.3})", unix.as_secs_f64());
    for note in game_window::take_notes() {
        log::info!("{note}");
    }
    let game = match Civ3::attach() {
        Ok(g) => g,
        Err(e) => {
            if std::env::var_os("CIV3R_VIEWER").is_some() {
                ui_trace::replacement_failed(&format!("not attaching renderer: {e}"));
            }
            return log::error!("not attaching: {e}");
        },
    };
    log::info!("detected build {:?}", game.build);
    match trace::install(game) {
        // Hooks stay installed for the life of the process.
        Ok(tracer) => std::mem::forget(tracer),
        Err(e) => {
            if std::env::var_os("CIV3R_VIEWER").is_some() && !ui_trace::requested_game_draws() {
                ui_trace::replacement_failed(&format!("installing required map tracer: {e}"));
            }
            log::error!("installing tracer: {e}");
        },
    }
    match unit_notices::install(game) {
        Ok(Some(hook)) => std::mem::forget(hook),
        Ok(None) => {},
        Err(e) => log::warn!("unit notice capture: {e}"),
    }
    if std::env::var_os("CIV3R_VIEWER").is_some() {
        if let Err(e) = live::start(game) {
            ui_trace::replacement_failed(&format!("starting the renderer: {e}"));
            native_camera::viewer_gone(game, None);
            ui_trace::restore_original_drawing();
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
