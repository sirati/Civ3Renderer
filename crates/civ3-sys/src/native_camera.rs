//! The renderer owns map cameras. The engine retains a small parked map
//! viewport for native bookkeeping, while explicit camera requests cross to A.
//! UI and City_Form canvases keep their original dimensions and camera rules.

use std::sync::{Mutex, OnceLock, atomic::{AtomicBool, Ordering}};

use inproc::Ptr32;
use crate::{Civ3, Civ3Error, layout as L};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Focus { pub serial: u64, pub center: (i32, i32) }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct MapKey { tiles: u32, width: i32, height: i32, seed: u32, player: i32 }

#[derive(Default)]
struct Lifecycle { installed: bool, gone: bool }

impl Lifecycle {
    fn active(&self, endpoint: bool) -> bool { self.installed && endpoint && !self.gone }
}

#[derive(Default)]
struct State {
    lifecycle: Lifecycle,
    restore: Option<(i32, i32)>,
    map: Option<MapKey>,
    focus: Focus,
    /// Full-size pixels, retained when the game's Z mode changes.
    initialized: bool,
    city_seen: bool,
    pending_tile: Option<(i32, i32)>,
    zoomed_out: Option<bool>,
}

impl State {
    fn request(&mut self, center: (i32, i32)) {
        self.focus.serial = self.focus.serial.wrapping_add(1).max(1);
        self.focus.center = center;
        self.initialized = true;
        log::debug!("native camera focus {} at full-size world pixel {:?}", self.focus.serial, self.focus.center);
    }
    /// Observe an actual native setter call, after bring-tile's policy checks.
    /// Returns whether the physical map can be parked for this call.
    fn setter(&mut self, proposal: (i32, i32), rect: [i32; 4], zoom: bool, city: bool) -> bool {
        if city { self.city_seen = true; return false; }
        let restored_city = std::mem::take(&mut self.city_seen);
        let changed_zoom = self.zoomed_out.is_some_and(|old| old != zoom);
        if !restored_city && !changed_zoom {
            if let Some((x, y)) = self.pending_tile {
                self.request(tile_center((x, y)));
            } else {
                let center = (proposal.0.saturating_add((rect[2] - rect[0]) / 2), proposal.1.saturating_add((rect[3] - rect[1]) / 2));
                self.request(full_size(center, zoom));
            }
        }
        self.zoomed_out = Some(zoom);
        true
    }
    fn map(&mut self, key: MapKey) {
        if self.map != Some(key) {
            self.map = Some(key);
            self.initialized = false;
            self.city_seen = false;
            self.pending_tile = None;
            self.zoomed_out = None;
        }
    }
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static RESTORE_PENDING: AtomicBool = AtomicBool::new(false);
static RESTORE_FN: OnceLock<restore_camera::Function> = OnceLock::new();
static STATE: Mutex<State> = Mutex::new(State {
    lifecycle: Lifecycle { installed: false, gone: false }, restore: None,
    map: None, focus: Focus { serial: 0, center: (0, 0) },
    initialized: false, city_seen: false, pending_tile: None, zoomed_out: None,
});

/// Whether the checked hooks, renderer and native input endpoint are active.
pub fn active() -> bool { ENABLED.load(Ordering::Acquire) }

/// Fixed native map range. The exclusive visibility checks see coordinates
/// 0..4 on each axis. No screen, window, panel or canvas is resized.
pub const PARKED_TILE_RANGE: (i32, i32) = (-1, 5);

fn city_active() -> bool {
    // The native setter itself tests City_Form+0x19CD before clamping.
    Ptr32::<u8>::new(0x74_238D).read().unwrap_or(0) != 0
        || Ptr32::<u32>::new(crate::world_input::FOCUS_CITY_SLOT).read().unwrap_or(0) != 0
}

fn map_key(game: Civ3) -> Option<MapKey> {
    let map = game.map().ok()?;
    let view = game.view().ok()?;
    Some(MapKey { tiles: map.tiles_address(), width: map.width, height: map.height, seed: map.seed, player: view.player_civ })
}

fn canonical(game: Civ3, center: (i32, i32)) -> (i32, i32) {
    full_size(center, game.zoomed_out().unwrap_or(false))
}

fn tile_center((mut x, y): (i32, i32)) -> (i32, i32) {
    // bring_tile_into_view adjusts X to the real staggered tile parity.
    if (x ^ y) & 1 != 0 { x = x.saturating_add(if x & 1 == 0 { 1 } else { -1 }); }
    (x.saturating_mul(64).saturating_add(64), y.saturating_mul(32).saturating_add(32))
}

fn full_size(center: (i32, i32), zoomed_out: bool) -> (i32, i32) {
    let n = if zoomed_out { 2 } else { 1 };
    (center.0.saturating_mul(n), center.1.saturating_mul(n))
}

/// Physical parked-camera writes belong on the game thread. The map range is
/// deliberately independent of the native client rectangle used by UI/input.
fn park(game: Civ3) -> Result<(), Civ3Error> {
    let p = Ptr32::<u8>::new(game.addrs.main_screen_form);
    for (offset, value) in [
        (L::MAIN_SCREEN_FORM_CAMERA_X, 0), (L::MAIN_SCREEN_FORM_CAMERA_Y, 0),
        (L::MAIN_SCREEN_FORM_TILEX_MIN, PARKED_TILE_RANGE.0),
        (L::MAIN_SCREEN_FORM_TILEX_MAX, PARKED_TILE_RANGE.1),
        (L::MAIN_SCREEN_FORM_TILEY_MIN, PARKED_TILE_RANGE.0),
        (L::MAIN_SCREEN_FORM_TILEY_MAX, PARKED_TILE_RANGE.1),
    ] { p.offset::<i32>(offset)?.write(&value)?; }
    Ok(())
}

/// Runtime readiness of the renderer's native world-input endpoint. Installation
/// alone is insufficient because the game's HWND may not exist at that time.
pub fn set_available(available: bool) {
    let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let active = state.lifecycle.active(available);
    if ENABLED.swap(active, Ordering::AcqRel) != active && active {
        log::info!("native camera active: parked (0,0), map tile range -1..5; semantic focus drives renderer A");
    }
}

/// Called before the native unit-list build, also after a City_Form closes.
/// This path never invents a semantic focus request from parked coordinates.
pub fn maintain(game: Civ3) {
    restore_if_requested(game);
    if !ENABLED.load(Ordering::Acquire) { return; }
    let Some(key) = map_key(game) else { return };
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.map(key);
    if city_active() { state.city_seen = true; return; }
    if !state.initialized {
        let Ok(view) = game.view() else { return };
        // Capture the original camera before the first physical park.
        let viewport = Ptr32::<i32>::new(0x9C_B3FC).read_array(4).ok();
        let center = viewport.filter(|r| r[2] > r[0] && r[3] > r[1])
            .map(|r| (view.camera_x.saturating_add((r[2] - r[0]) / 2), view.camera_y.saturating_add((r[3] - r[1]) / 2)))
            .unwrap_or((view.camera_x, view.camera_y));
        state.request(canonical(game, center));
        state.zoomed_out = game.zoomed_out().ok();
    }
    state.city_seen = false;
    if let Err(e) = park(game) { log::warn!("parked native camera: {e}"); }
}

/// Read-only publisher snapshot. The native camera may be in City_Form while
/// this remains the last semantic primary-map request.
pub fn focus(game: Civ3) -> Option<Focus> {
    if !ENABLED.load(Ordering::Acquire) { return None; }
    let key = map_key(game)?;
    let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if state.map != Some(key) || !state.initialized { return None; }
    let n = if game.zoomed_out().ok()? { 2 } else { 1 };
    Some(Focus { serial: state.focus.serial, center: (state.focus.center.0 / n, state.focus.center.1 / n) })
}

/// Native map-wheel displacement for one notch. Speed is the actual map
/// form's 0..2 setting used by PCGames 0x4DE360, not a viewer constant.
pub fn map_wheel_step(game: Civ3) -> Result<(i32, i32), Civ3Error> {
    let speed = Ptr32::<i32>::new(game.addrs.main_screen_form + 0x4DB8).read()?;
    Ok(wheel_step(speed, game.zoomed_out()?))
}

fn wheel_step(speed: i32, zoomed_out: bool) -> (i32, i32) {
    let speed = speed.clamp(0, 2);
    let divisor = (1 << (2 - speed)) * if zoomed_out { 2 } else { 1 };
    (256 / divisor, 128 / divisor)
}

/// Observe a native tile focus after its caller has applied native visibility
/// and show-moves policies. Selection-only renderer clicks suppress this.
pub fn request_tile(game: Civ3, tile: (i32, i32)) {
    if !ENABLED.load(Ordering::Acquire) || city_active() { return; }
    let Some(key) = map_key(game) else { return };
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.map(key);
    state.request(tile_center(tile));
}

/// The bring-tile function may reject a request before reaching its setter.
/// Record the semantic tile only if the original actually invokes the setter.
pub struct BringTileGuard { previous: Option<(i32, i32)> }

pub fn begin_bring_tile(screen: u32, tile: (i32, i32)) -> BringTileGuard {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let previous = state.pending_tile;
    if ENABLED.load(Ordering::Acquire) && screen == crate::Build::PcGames.addresses().main_screen_form {
        state.pending_tile = Some(tile);
    }
    BringTileGuard { previous }
}

impl Drop for BringTileGuard {
    fn drop(&mut self) {
        STATE.lock().unwrap_or_else(|e| e.into_inner()).pending_tile = self.previous;
    }
}

/// Permanent viewer loss. This function only queues restoration; all native
/// calls and writes remain on the game's thread.
pub fn viewer_gone(game: Civ3, renderer_center: Option<(i32, i32)>) {
    let current_map = map_key(game);
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if state.lifecycle.gone { return; }
    state.lifecycle.gone = true;
    ENABLED.store(false, Ordering::Release);
    if state.initialized && state.map == current_map && state.restore.is_none() {
        state.restore = Some(renderer_center.map(|p| canonical(game, p)).unwrap_or(state.focus.center));
        RESTORE_PENDING.store(true, Ordering::Release);
    }
}

/// WM_NULL and the native animation update provide game-thread safe points.
/// City_Form keeps its own camera until it closes, then this restores the full
/// native map and its normal tile bounds at the last renderer A location.
pub fn restore_if_requested(game: Civ3) {
    if !RESTORE_PENDING.load(Ordering::Acquire) || city_active() { return; }
    let center = {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        let same_map = state.map == map_key(game);
        let center = state.restore.take().filter(|_| same_map);
        RESTORE_PENDING.store(false, Ordering::Release);
        center
    };
    let Some(center) = center else { return };
    let Some(function) = RESTORE_FN.get() else { return };
    let n = if game.zoomed_out().unwrap_or(false) { 2 } else { 1 };
    let r = Ptr32::<[i32; 4]>::new(0x9C_B3FC).read().unwrap_or([0, 0, 1024, 768]);
    let x = center.0 / n - (r[2] - r[0]) / 2;
    let y = center.1 / n - (r[3] - r[1]) / 2;
    function.call(game.addrs.main_screen_form, x as u32, y as u32, 0, 1);
    log::info!("native camera restored after viewer loss at world pixel {:?}", (center.0 / n, center.1 / n));
}

inproc::thiscall_fn! {
    /// The checked setter entry; when parking is disabled its hook forwards
    /// directly to the native original and recomputes full viewport bounds.
    pub fn restore_camera(screen; x, y, param_3, always_update) -> ();
}

inproc::fastcall_hook! {
    /// PCGames Main_Screen_Form camera setter. Four callee-popped arguments.
    pub mod set_camera(screen, edx; x, y, param_3, always_update) -> ();
}

pub const SET_CAMERA_SITE: u32 = 0x4D_F7C0;
pub const SET_CAMERA_PROLOGUE: &[u8] = &[0x83, 0xEC, 0x10, 0x53, 0x55];

/// Install only after independent world input and animation collection work.
/// A failed hook leaves ENABLED false and the original game camera intact.
pub fn install(game: Civ3) -> Result<(), inproc::hook::HookError> {
    if game.build != crate::Build::PcGames { return Err(inproc::hook::HookError::Unsupported); }
    let restore = restore_camera::Function::at(SET_CAMERA_SITE, SET_CAMERA_PROLOGUE)?;
    let hook = set_camera::install_inline(SET_CAMERA_SITE, SET_CAMERA_PROLOGUE, move |call| {
        if call.screen != game.addrs.main_screen_form || !ENABLED.load(Ordering::Acquire) {
            return call.original();
        }
        let Some(key) = map_key(game) else { return call.original() };
        let city = city_active();
        let zoom = game.zoomed_out().unwrap_or(false);
        {
            let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
            // Preserve pending bring requests across the initial map identity.
            let pending = state.pending_tile;
            state.map(key);
            state.pending_tile = pending;
            let rect = Ptr32::<[i32; 4]>::new(0x9C_B3FC).read().unwrap_or([0, 0, 1024, 768]);
            state.setter((call.x as i32, call.y as i32), rect, zoom, city);
        }
        if city { return call.original(); }
        // Preserve the setter's invalidation and bookkeeping, then constrain
        // the native map range without changing any UI/client dimensions.
        call.original_with(call.screen, 0, 0, call.param_3, call.always_update);
        if let Err(e) = park(game) { log::warn!("parked native camera setter: {e}"); }
    })?;
    let ai = crate::ai_camera_focus::install(game)?;
    std::mem::forget((hook, ai));
    let _ = RESTORE_FN.set(restore);
    STATE.lock().unwrap_or_else(|e| e.into_inner()).lifecycle.installed = true;
    log::info!("native camera hooks ready; parking waits for the renderer world-input endpoint");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_lifecycle_waits_for_endpoint_and_never_restarts_after_viewer_loss() {
        let mut lifecycle = Lifecycle::default();
        assert!(!lifecycle.active(true), "endpoint alone cannot activate missing hooks");
        lifecycle.installed = true;
        assert!(!lifecycle.active(false), "hook installation does not prove input readiness");
        assert!(lifecycle.active(true));
        lifecycle.gone = true;
        assert!(!lifecycle.active(true), "an attached HWND must not reactivate a lost viewer");
        assert!(!lifecycle.active(false));
        lifecycle.installed = true;
        assert!(!lifecycle.active(true));
    }
    #[test]
    fn repeated_native_requests_are_events_even_at_the_same_tile() {
        let mut s = State::default();
        s.request((3200, 1600)); let first = s.focus;
        s.request(first.center);
        assert_eq!(s.focus.center, first.center);
        assert_ne!(s.focus.serial, first.serial);
    }
    #[test]
    fn world_replacement_keeps_serial_monotonic_and_resets_initial_placement() {
        let mut s = State::default();
        s.map(MapKey { tiles: 100, width: 100, height: 60, seed: 1, player: 1 }); s.request((3200, 1600));
        let old = s.focus.serial;
        s.map(MapKey { tiles: 200, width: 100, height: 60, seed: 1, player: 1 });
        assert!(!s.initialized);
        s.request((6400, 3200)); assert!(s.focus.serial > old);
    }
    #[test]
    fn zoom_rebuild_never_replaces_semantic_focus_with_parked_camera() {
        let mut s = State::default();
        assert!(s.setter((2400, 1200), [0, 0, 1024, 768], false, false));
        let original = s.focus;
        assert!(s.setter((0, 0), [0, 0, 1024, 768], true, false));
        assert_eq!(s.focus, original);
        assert!(s.setter((0, 0), [0, 0, 1024, 768], false, false));
        assert_eq!(s.focus, original);
    }
    #[test]
    fn city_camera_and_its_restore_do_not_move_renderer_world_views() {
        let mut s = State::default();
        s.setter((2400, 1200), [0, 0, 1024, 768], false, false);
        let original = s.focus;
        assert!(!s.setter((9000, 6000), [0, 0, 320, 240], false, true));
        assert!(s.setter((0, 0), [0, 0, 1024, 768], false, false));
        assert_eq!(s.focus, original);
        s.pending_tile = Some((50, 30));
        s.setter((2000, 1000), [0, 0, 1024, 768], false, false);
        assert_eq!(s.focus.center, (3264, 992));
        assert_ne!(s.focus.serial, original.serial);
    }
    #[test]
    fn direct_corner_requests_are_not_mistaken_for_parked_bookkeeping() {
        let mut s = State::default();
        s.setter((2400, 1200), [0, 0, 1024, 768], false, false);
        let old = s.focus;
        s.setter((0, 0), [0, 0, 1024, 768], false, false);
        assert_eq!(s.focus.center, (512, 384));
        assert_ne!(s.focus.serial, old.serial);
        let first_corner = s.focus;
        s.setter((0, 0), [0, 0, 1024, 768], false, false);
        assert_eq!(s.focus.center, first_corner.center);
        assert_ne!(s.focus.serial, first_corner.serial);
        s.pending_tile = Some((0, 0));
        s.setter((0, 0), [0, 0, 1024, 768], false, false);
        assert_eq!(s.focus.center, (64, 32));
    }
    #[test]
    fn tile_focus_matches_native_staggered_parity_adjustment() {
        assert_eq!(tile_center((0, 0)), (64, 32));
        assert_eq!(tile_center((1, 1)), (128, 64));
        assert_eq!(tile_center((0, 1)), tile_center((1, 1)));
        assert_eq!(tile_center((1, 0)), tile_center((0, 0)));
    }
    #[test]
    fn wheel_step_matches_native_speed_and_game_zoom() {
        for (speed, normal) in [(-1, (64,32)), (0, (64,32)), (1, (128,64)), (2, (256,128)), (3, (256,128))] {
            assert_eq!(wheel_step(speed, false), normal);
            assert_eq!(wheel_step(speed, true), (normal.0 / 2, normal.1 / 2));
        }
    }
    #[test]
    fn focus_uses_canonical_pixels_through_native_zoom() {
        let mut s = State::default();
        s.setter((1200, 600), [0, 0, 1024, 768], true, false);
        assert_eq!(s.focus.center, (3424, 1968));
        assert_eq!(full_size((1712, 984), true), s.focus.center);
    }
}
