//! Bridge for semantic primary-map focus. Normally the engine camera is
//! parked bookkeeping. Explicit original-renderer comparison keeps its full
//! viewport centered on renderer A, without driving A from physical camera data.

#[cfg(windows)]
fn comparison() -> bool {
    static REQUESTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *REQUESTED.get_or_init(crate::ui_trace::requested_game_draws)
}

#[cfg(windows)]
pub fn maintain(game: civ3_sys::Civ3) {
    civ3_sys::native_camera::set_available(crate::world_input::endpoint_ready());
    civ3_sys::native_camera::maintain(game);
}

#[cfg(windows)]
pub fn focus(game: civ3_sys::Civ3) -> Option<scene::CameraFocus> {
    civ3_sys::native_camera::focus(game).map(|f| scene::CameraFocus { serial: f.serial, center: f.center })
}

#[cfg(windows)]
pub fn install(game: civ3_sys::Civ3) -> Result<(), String> {
    civ3_sys::native_camera::set_comparison(comparison());
    civ3_sys::native_camera::install(game).map_err(|e| e.to_string())
}

#[cfg(windows)]
pub fn viewer_gone(game: civ3_sys::Civ3, main: Option<(i32, i32, i32, i32)>) {
    let center = main.map(|(x, y, w, h)| (x.saturating_add(w / 2), y.saturating_add(h / 2)));
    civ3_sys::native_camera::viewer_gone(game, center);
}

#[cfg(windows)]
pub fn restore_if_requested(game: civ3_sys::Civ3) {
    civ3_sys::native_camera::restore_if_requested(game);
}

#[cfg(windows)]
pub fn renderer_view(game: civ3_sys::Civ3, main: Option<(i32, i32, i32, i32)>) {
    if comparison() { civ3_sys::native_camera::renderer_view(game, main); }
}
