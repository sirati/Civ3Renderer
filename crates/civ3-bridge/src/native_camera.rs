//! Bridge for semantic primary-map focus. The engine camera is bookkeeping,
//! never the source of renderer A/B placement once this module is active.

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
