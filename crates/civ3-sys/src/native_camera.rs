//! The renderer owns map cameras. Normally the engine retains a small parked
//! map range for bookkeeping. Explicit original-renderer comparison keeps its
//! full viewport centered on renderer A. Native focus requests still cross to A.
//! UI and City_Form canvases keep their original dimensions and camera rules.

use std::{cell::{Cell, RefCell}, sync::{LazyLock, OnceLock, atomic::{AtomicBool, Ordering}}};

use inproc::Ptr32;
use crate::{Civ3, Civ3Error, layout as L};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Focus { pub serial: u64, pub center: (i32, i32) }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Arrival { pub serial: u64, pub bounds: [i32; 4] }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct MapKey { tiles: u32, width: i32, height: i32, seed: u32, player: i32 }

#[derive(Default)]
struct Lifecycle { installed: bool, gone: bool }

impl Lifecycle {
    fn active(&self, endpoint: bool) -> bool { self.installed && endpoint && !self.gone }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ComparisonSync {
    center: (i32, i32),
    rect: [i32; 4],
    zoomed_out: bool,
}

impl ComparisonSync {
    fn camera(self) -> (i32, i32) {
        let n = if self.zoomed_out { 2 } else { 1 };
        ((self.center.0 / n).saturating_sub(self.rect[2].saturating_sub(self.rect[0]) / 2),
         (self.center.1 / n).saturating_sub(self.rect[3].saturating_sub(self.rect[1]) / 2))
    }
}

#[derive(Default)]
struct State {
    lifecycle: Lifecycle,
    restore: Option<(i32, i32)>,
    map: Option<MapKey>,
    focus: Focus,
    arrival: Option<Arrival>,
    /// Full-size pixels, retained when the game's Z mode changes.
    initialized: bool,
    city_seen: bool,
    pending_tile: Option<(i32, i32)>,
    zoomed_out: Option<bool>,
    comparison: bool,
    comparison_target: Option<(MapKey, (i32, i32))>,
    comparison_synced: Option<ComparisonSync>,
}

impl State {
    fn comparison(&mut self, enabled: bool) {
        if self.comparison != enabled {
            self.comparison = enabled;
            self.comparison_synced = None;
        }
    }
    fn renderer_view(&mut self, key: MapKey, main: Option<(i32, i32, i32, i32)>, zoom: bool) {
        self.map(key);
        self.comparison_target = main.filter(|r| r.2 > 0 && r.3 > 0).map(|(x, y, w, h)| {
            (key, full_size((x.saturating_add(w / 2), y.saturating_add(h / 2)), zoom))
        });
    }
    fn comparison_plan(&self, key: MapKey, rect: [i32; 4], zoomed_out: bool) -> Option<ComparisonSync> {
        if !self.comparison || !self.initialized || self.map != Some(key)
            || rect[2] <= rect[0] || rect[3] <= rect[1] { return None; }
        let center = self.comparison_target.filter(|t| t.0 == key).map_or(self.focus.center, |t| t.1);
        let plan = ComparisonSync { center, rect, zoomed_out };
        (self.comparison_synced != Some(plan)).then_some(plan)
    }
    fn request(&mut self, center: (i32, i32)) {
        self.focus.serial = self.focus.serial.wrapping_add(1).max(1);
        self.focus.center = center;
        self.arrival = None;
        self.initialized = true;
        log::debug!("native camera focus {} at full-size world pixel {:?}", self.focus.serial, self.focus.center);
    }
    fn request_tile(&mut self, key: MapKey, tile: (i32, i32)) -> bool {
        // These are native unit/animation coordinates, not renderer copies
        // beyond a wrapping seam. Removed units can use (-1,-1), which would
        // otherwise become a false focus at world pixel (0,0).
        if !(0..key.width).contains(&tile.0) || !(0..key.height).contains(&tile.1) {
            log::debug!("native camera rejected off-map AI tile {:?} for map {}x{}", tile, key.width, key.height);
            return false;
        }
        self.map(key);
        self.request(tile_center(tile));
        true
    }
    /// Observe an actual native setter call, after bring-tile's policy checks.
    /// Returns whether the physical map can be parked for this call.
    fn setter(&mut self, proposal: (i32, i32), rect: [i32; 4], zoom: bool, city: bool) -> bool {
        self.comparison_synced = None;
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
        !self.comparison
    }
    fn map(&mut self, key: MapKey) {
        if self.map != Some(key) {
            self.map = Some(key);
            self.initialized = false;
            self.arrival = None;
            self.city_seen = false;
            self.pending_tile = None;
            self.zoomed_out = None;
            self.comparison_target = None;
            self.comparison_synced = None;
        }
    }
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static COMPARISON: AtomicBool = AtomicBool::new(false);
static RESTORE_PENDING: AtomicBool = AtomicBool::new(false);
static RESTORE_FN: OnceLock<restore_camera::Function> = OnceLock::new();
/// `Lifecycle` as the other threads set it (the state's copy follows).
static INSTALLED: AtomicBool = AtomicBool::new(false);
static GONE: AtomicBool = AtomicBool::new(false);

/// What other threads ask of the camera state. It lives on the game's
/// thread (all native calls and hooks are there) and applies these at its
/// next use; nobody waits for it.
enum Command {
    Comparison(bool),
    RendererView(MapKey, Option<(i32, i32, i32, i32)>, bool),
    Gone { game: Civ3, center: Option<(i32, i32)>, map: Option<MapKey> },
}
static COMMANDS: LazyLock<handoff::Mailbox<Command>> = LazyLock::new(handoff::Mailbox::new);

/// What the publisher reads of the state (`focus`), as of its last change.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct Published { map: Option<MapKey>, initialized: bool, focus: Focus, arrival: Option<Arrival> }
static PUBLISHED: handoff::Latest<Published> = handoff::Latest::new();

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    static LAST_PUBLISHED: Cell<Option<Published>> = const { Cell::new(None) };
}

fn apply(state: &mut State, command: Command) {
    match command {
        Command::Comparison(enabled) => state.comparison(enabled),
        Command::RendererView(key, main, zoom) => if state.comparison { state.renderer_view(key, main, zoom) },
        Command::Gone { game, center, map } => {
            state.lifecycle.gone = true;
            if state.initialized && state.map == map && state.restore.is_none() {
                state.restore = Some(center.map(|p| canonical(game, p)).unwrap_or(state.focus.center));
                RESTORE_PENDING.store(true, Ordering::Release);
            }
        }
    }
}

/// The game thread's state, with what other threads asked applied first
/// and any change the publisher reads published after. Never call native
/// code inside `f`: it may come back here through a hook.
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(|state| {
        state.lifecycle.installed = INSTALLED.load(Ordering::Acquire);
        COMMANDS.drain().for_each(|c| apply(state, c));
        let result = f(state);
        let now = Published { map: state.map, initialized: state.initialized, focus: state.focus, arrival: state.arrival };
        if LAST_PUBLISHED.replace(Some(now)) != Some(now) {
            PUBLISHED.publish(now);
        }
        result
    })
}

thread_local! {
    /// A synchronization call recomputes original map bounds without creating
    /// a semantic camera event that would feed back into renderer A.
    static COMPARISON_SYNCING: Cell<bool> = const { Cell::new(false) };
}

struct ComparisonGuard;
impl ComparisonGuard {
    fn enter() -> Option<Self> {
        COMPARISON_SYNCING.with(|active| if active.replace(true) { None } else { Some(Self) })
    }
}
impl Drop for ComparisonGuard {
    fn drop(&mut self) { COMPARISON_SYNCING.with(|active| active.set(false)); }
}

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
/// On the game's thread.
pub fn set_available(available: bool) {
    let (active, comparison) = with_state(|state| (state.lifecycle.active(available) && !GONE.load(Ordering::SeqCst), state.comparison));
    let changed = ENABLED.swap(active, Ordering::SeqCst) != active;
    // `viewer_gone` on another thread sets GONE before clearing ENABLED: if
    // it ran meanwhile, its clear may have come before this swap.
    if active && GONE.load(Ordering::SeqCst) {
        ENABLED.store(false, Ordering::SeqCst);
    crate::camera_arrival::viewer_gone();
        return;
    }
    if changed && active {
        if comparison {
            log::info!("native camera active: original comparison keeps full viewport and follows renderer A");
        } else {
            log::info!("native camera active: parked (0,0), map tile range -1..5; semantic focus drives renderer A");
        }
    }
}

/// Original drawing is permitted only by the bridge's explicit comparison
/// policy. This changes no process memory and is safe on the publisher thread.
pub fn set_comparison(enabled: bool) {
    COMMANDS.send(Command::Comparison(enabled));
    COMPARISON.store(enabled, Ordering::Release);
}

/// Queue renderer A's world rectangle, in the current native zoom's pixels.
/// Only game-thread safe points apply it to the original camera. Keeping its
/// native viewport size permits direct screenshots around the same center.
pub fn renderer_view(game: Civ3, main: Option<(i32, i32, i32, i32)>) {
    if !COMPARISON.load(Ordering::Acquire) { return; }
    let Some(key) = map_key(game) else { return };
    let zoom = game.zoomed_out().unwrap_or(false);
    COMMANDS.send(Command::RendererView(key, main, zoom));
}

fn synchronize_comparison(game: Civ3) {
    if !COMPARISON.load(Ordering::Acquire) || !active() || city_active() { return; }
    let Some(_guard) = ComparisonGuard::enter() else { return };
    let Some(function) = RESTORE_FN.get() else { return };
    let Some(key) = map_key(game) else { return };
    let Ok(rect) = Ptr32::<[i32; 4]>::new(0x9C_B3FC).read() else { return };
    let zoom = game.zoomed_out().unwrap_or(false);
    let plan = with_state(|state| state.comparison_plan(key, rect, zoom));
    let Some(plan) = plan else { return };
    let (x, y) = plan.camera();
    function.call(game.addrs.main_screen_form, x as u32, y as u32, 0, 1);
    with_state(|state| {
        if state.map == Some(key) && state.comparison {
            state.comparison_synced = Some(plan);
        }
    });
    if let Ok(view) = game.view() {
        log::info!("native camera comparison: renderer A center {:?}, full native viewport {:?}, actual native camera {:?}, tile range x={:?} y={:?}, zoomed_out={zoom}",
            (plan.center.0 / if zoom { 2 } else { 1 }, plan.center.1 / if zoom { 2 } else { 1 }), rect,
            (view.camera_x, view.camera_y), view.tile_x, view.tile_y);
    }
}

/// Called before the native unit-list build, also after a City_Form closes.
/// This path never invents a semantic focus request from parked coordinates.
pub fn maintain(game: Civ3) {
    restore_if_requested(game);
    if !ENABLED.load(Ordering::Acquire) { return; }
    let Some(key) = map_key(game) else { return };
    // Whether comparison is on, once maintained (None: nothing more to do).
    let comparison = with_state(|state| {
        state.map(key);
        if city_active() { state.city_seen = true; return None; }
        if !state.initialized {
            let Ok(view) = game.view() else { return None };
            // Capture the original camera before the first physical park.
            let viewport = Ptr32::<i32>::new(0x9C_B3FC).read_array(4).ok();
            let center = viewport.filter(|r| r[2] > r[0] && r[3] > r[1])
                .map(|r| (view.camera_x.saturating_add((r[2] - r[0]) / 2), view.camera_y.saturating_add((r[3] - r[1]) / 2)))
                .unwrap_or((view.camera_x, view.camera_y));
            state.request(canonical(game, center));
            state.zoomed_out = game.zoomed_out().ok();
        }
        state.city_seen = false;
        if !state.comparison { state.comparison_synced = None; }
        Some(state.comparison)
    });
    match comparison {
        Some(true) => synchronize_comparison(game),
        Some(false) => if let Err(e) = park(game) { log::warn!("parked native camera: {e}"); },
        None => {}
    }
}

/// Read-only publisher snapshot. The native camera may be in City_Form while
/// this remains the last semantic primary-map request.
pub fn focus(game: Civ3) -> Option<Focus> {
    if !ENABLED.load(Ordering::Acquire) { return None; }
    let key = map_key(game)?;
    let state = PUBLISHED.get()?;
    if state.map != Some(key) || !state.initialized { return None; }
    let n = if game.zoomed_out().ok()? { 2 } else { 1 };
    Some(Focus { serial: state.focus.serial, center: (state.focus.center.0 / n, state.focus.center.1 / n) })
}

/// Read from the same published native snapshot as its focus serial.
pub fn arrival(game: Civ3) -> Option<Arrival> {
    if !active() { return None; }
    let key = map_key(game)?;
    let state = PUBLISHED.get()?;
    if state.map != Some(key) || !state.initialized { return None; }
    let n = if game.zoomed_out().ok()? { 2 } else { 1 };
    let mut request = state.arrival?;
    request.bounds = [request.bounds[0].div_euclid(n), request.bounds[1].div_euclid(n),
        request.bounds[2].div_euclid(n) + i32::from(request.bounds[2].rem_euclid(n) != 0),
        request.bounds[3].div_euclid(n) + i32::from(request.bounds[3].rem_euclid(n) != 0)];
    Some(request)
}

fn combat_bounds(center: (i32, i32), other: Option<(i32, i32)>, period: (i32, i32), wrap: (bool, bool)) -> [i32; 4] {
    let mut bounds = [center.0 - 64, center.1 - 32, center.0 + 64, center.1 + 32];
    if let Some(p) = other {
        let near = |v: i32, at: i32, period: i32, wrap: bool| if wrap && period > 0 { at + (v - at + period / 2).rem_euclid(period) - period / 2 } else { v };
        let p = (near(p.0, center.0, period.0, wrap.0), near(p.1, center.1, period.1, wrap.1));
        bounds = [bounds[0].min(p.0 - 64), bounds[1].min(p.1 - 32), bounds[2].max(p.0 + 64), bounds[3].max(p.1 + 32)];
    }
    bounds
}

/// Both endpoints have already passed native visibility/detection policy.
/// The barrier is armed before with_state publishes its immutable request.
pub fn request_combat(game: Civ3, first: (i32, i32), other: Option<(i32, i32)>) -> Option<u64> {
    if !active() || city_active() { return None; }
    let key = map_key(game)?;
    let map = game.map().ok()?;
    with_state(|state| {
        if !state.request_tile(key, first) { return None; }
        let center = state.focus.center;
        let other = other.filter(|t| (0..key.width).contains(&t.0) && (0..key.height).contains(&t.1)).map(tile_center);
        let bounds = combat_bounds(center, other, (key.width * 64, key.height * 32), (map.wrap_x, map.wrap_y));
        state.focus.center = (bounds[0] + (bounds[2] - bounds[0]) / 2, bounds[1] + (bounds[3] - bounds[1]) / 2);
        let serial = state.focus.serial;
        state.arrival = Some(Arrival { serial, bounds });
        log::debug!("native combat camera {serial}: full-size bounds {bounds:?}");
        crate::camera_arrival::arm(serial);
        Some(serial)
    })
}

pub fn finish_combat(serial: u64) {
    with_state(|state| {
        if state.arrival.is_some_and(|a| a.serial == serial) { state.arrival = None; }
    });
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
pub fn request_tile(game: Civ3, tile: (i32, i32)) -> bool {
    if !ENABLED.load(Ordering::Acquire) || city_active() { return false; }
    let Some(key) = map_key(game) else { return false };
    with_state(|state| state.request_tile(key, tile))
}

/// The bring-tile function may reject a request before reaching its setter.
/// Record the semantic tile only if the original actually invokes the setter.
pub struct BringTileGuard { previous: Option<(i32, i32)> }

pub fn begin_bring_tile(screen: u32, tile: (i32, i32)) -> BringTileGuard {
    with_state(|state| {
        let previous = state.pending_tile;
        if ENABLED.load(Ordering::Acquire) && screen == crate::Build::PcGames.addresses().main_screen_form {
            state.pending_tile = Some(tile);
        }
        BringTileGuard { previous }
    })
}

impl Drop for BringTileGuard {
    fn drop(&mut self) {
        let previous = self.previous;
        with_state(|state| state.pending_tile = previous);
    }
}

/// Permanent viewer loss. This function only queues restoration; all native
/// calls and writes remain on the game's thread.
pub fn viewer_gone(game: Civ3, renderer_center: Option<(i32, i32)>) {
    let current_map = map_key(game);
    if GONE.swap(true, Ordering::SeqCst) { return; }
    ENABLED.store(false, Ordering::SeqCst);
    // The game's thread decides on restoration at its next safe point.
    COMMANDS.send(Command::Gone { game, center: renderer_center, map: current_map });
}

/// WM_NULL and the native animation update provide game-thread safe points.
/// City_Form keeps its own camera until it closes, then this restores the full
/// native map and its normal tile bounds at the last renderer A location.
pub fn restore_if_requested(game: Civ3) {
    synchronize_comparison(game);
    // (Applies a viewer loss queued since.)
    with_state(|_| ());
    if !RESTORE_PENDING.load(Ordering::Acquire) || city_active() { return; }
    let center = with_state(|state| {
        let same_map = state.map == map_key(game);
        let center = state.restore.take().filter(|_| same_map);
        RESTORE_PENDING.store(false, Ordering::Release);
        center
    });
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
        if call.screen != game.addrs.main_screen_form || !ENABLED.load(Ordering::Acquire)
            || COMPARISON_SYNCING.with(Cell::get) {
            return call.original();
        }
        let Some(key) = map_key(game) else { return call.original() };
        let city = city_active();
        let zoom = game.zoomed_out().unwrap_or(false);
        let comparison = with_state(|state| {
            // Preserve pending bring requests across the initial map identity.
            let pending = state.pending_tile;
            state.map(key);
            state.pending_tile = pending;
            let rect = Ptr32::<[i32; 4]>::new(0x9C_B3FC).read().unwrap_or([0, 0, 1024, 768]);
            state.setter((call.x as i32, call.y as i32), rect, zoom, city);
            state.comparison
        });
        if city || comparison { return call.original(); }
        // Preserve the setter's invalidation and bookkeeping, then constrain
        // the native map range without changing any UI/client dimensions.
        call.original_with(call.screen, 0, 0, call.param_3, call.always_update);
        if let Err(e) = park(game) { log::warn!("parked native camera setter: {e}"); }
    })?;
    let ai = crate::ai_camera_focus::install(game)?;
    std::mem::forget((hook, ai));
    let _ = RESTORE_FN.set(restore);
    INSTALLED.store(true, Ordering::Release);
    log::info!("native camera hooks ready; parking waits for the renderer world-input endpoint");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_comparison_keeps_native_bounds_and_centers_full_view_on_renderer_a() {
        let key = MapKey { tiles: 100, width: 100, height: 60, seed: 1, player: 1 };
        let mut s = State::default();
        s.map(key);
        s.request((3200, 1600));
        assert!(s.setter((100, 200), [0, 0, 1280, 800], false, false));
        s.comparison(true);
        assert!(!s.setter((100, 200), [0, 0, 1280, 800], false, false), "explicit comparison must never park the original setter");
        s.renderer_view(key, Some((2800, 1200, 800, 600)), false);
        let focus = s.focus;
        let sync = s.comparison_plan(key, [0, 0, 1280, 800], false).unwrap();
        assert_eq!(sync.center, (3200, 1500));
        assert_eq!(sync.rect, [0, 0, 1280, 800]);
        assert_eq!(sync.camera(), (2560, 1100));
        s.comparison_synced = Some(sync);
        assert_eq!(s.comparison_plan(key, sync.rect, false), None);
        assert_eq!(s.focus, focus, "renderer synchronization must not publish a semantic refocus");
        assert!(s.comparison_plan(key, [0, 0, 1024, 768], false).is_some());
        s.comparison(false);
        assert!(s.setter((0, 0), [0, 0, 1280, 800], false, false));
        assert_eq!(s.comparison_plan(key, sync.rect, false), None);
    }
    #[test]
    fn comparison_tracks_zoom_and_renderer_updates_without_feeding_back_focus() {
        let key = MapKey { tiles: 100, width: 100, height: 60, seed: 1, player: 1 };
        let mut s = State::default();
        s.map(key); s.request((3200, 1600)); s.comparison(true);
        s.renderer_view(key, Some((1200, 600, 800, 400)), true);
        let sync = s.comparison_plan(key, [0, 0, 1024, 768], true).unwrap();
        assert_eq!(sync.center, (3200, 1600));
        assert_eq!(sync.camera(), (1088, 416));
        s.comparison_synced = Some(sync);
        assert_eq!(s.comparison_plan(key, sync.rect, false).unwrap().camera(), (2688, 1216));
        s.renderer_view(key, Some((2000, 1000, 800, 400)), true);
        assert_eq!(s.comparison_plan(key, sync.rect, true).unwrap().center, (4800, 2400));
        assert_eq!(s.focus.center, (3200, 1600));
    }
    #[test]
    fn comparison_discards_stale_world_targets_and_rejects_empty_native_viewports() {
        let key = MapKey { tiles: 100, width: 100, height: 60, seed: 1, player: 1 };
        let mut s = State::default();
        s.map(key); s.request((3200, 1600)); s.comparison(true);
        s.renderer_view(key, Some((6000, 3000, 800, 600)), false);
        let next = MapKey { tiles: 200, ..key };
        s.map(next);
        assert_eq!(s.comparison_target, None);
        assert_eq!(s.comparison_plan(next, [0, 0, 1024, 768], false), None);
        s.request((5000, 2500));
        assert_eq!(s.comparison_plan(next, [0, 0, 1024, 768], false).unwrap().center, (5000, 2500));
        assert_eq!(s.comparison_plan(key, [0, 0, 1024, 768], false), None);
        for rect in [[0, 0, 0, 768], [0, 768, 1024, 768], [0, 0, -1, 768]] {
            assert_eq!(s.comparison_plan(next, rect, false), None);
        }
        s.renderer_view(next, Some((1, 2, -3, 4)), false);
        assert_eq!(s.comparison_target, None);
        assert_eq!(ComparisonSync { center: (i32::MIN, i32::MAX), rect: [0, 0, 1024, 768], zoomed_out: false }.camera(), (i32::MIN, i32::MAX - 384));
    }
    #[test]
    fn city_comparison_defers_sync_and_native_requests_invalidate_the_cached_camera() {
        let key = MapKey { tiles: 100, width: 100, height: 60, seed: 1, player: 1 };
        let mut s = State::default();
        s.map(key); s.request((3200, 1600)); s.comparison(true);
        s.comparison_synced = s.comparison_plan(key, [0, 0, 1024, 768], false);
        let focus = s.focus;
        assert!(!s.setter((9000, 6000), [0, 0, 320, 240], false, true));
        assert_eq!(s.comparison_synced, None);
        assert_eq!(s.focus, focus);
        assert!(!s.setter((0, 0), [0, 0, 1024, 768], false, false));
        assert_eq!(s.focus, focus);
        assert!(s.comparison_plan(key, [0, 0, 1024, 768], false).is_some());
    }
    #[test]
    fn native_comparison_guard_blocks_reentry_and_always_releases() {
        assert!(!COMPARISON_SYNCING.with(Cell::get));
        {
            let _guard = ComparisonGuard::enter().unwrap();
            assert!(COMPARISON_SYNCING.with(Cell::get));
            assert!(ComparisonGuard::enter().is_none());
            assert!(COMPARISON_SYNCING.with(Cell::get));
        }
        assert!(!COMPARISON_SYNCING.with(Cell::get));
    }
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
    fn combat_bounds_include_both_complete_tiles_and_wrap_without_hidden_endpoints() {
        assert_eq!(combat_bounds((640, 320), None, (8000, 4000), (false, false)), [576, 288, 704, 352]);
        assert_eq!(combat_bounds((640, 320), Some((704, 352)), (8000, 4000), (false, false)), [576, 288, 768, 384]);
        assert_eq!(combat_bounds((64, 320), Some((7936, 320)), (8000, 4000), (true, false)), [-128, 288, 128, 352]);
        let mut state = State::default(); state.request((640, 320));
        state.arrival = Some(Arrival { serial: state.focus.serial, bounds: [576, 288, 704, 352] });
        state.map(MapKey { tiles: 200, width: 100, height: 60, seed: 1, player: 1 });
        assert!(state.arrival.is_none(), "another map cannot inherit a pending combat region");
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
    fn off_map_ai_tiles_do_not_change_focus_or_wrap_a_removed_unit_sentinel() {
        let key = MapKey { tiles: 100, width: 256, height: 256, seed: 1, player: 1 };
        let mut s = State::default();
        assert!(s.request_tile(key, (156, 156)));
        let original = s.focus;
        for tile in [(-1,-1), (-1,1), (1,-1), (256,0), (0,256), (i32::MIN,i32::MAX)] {
            assert!(!s.request_tile(key, tile));
            assert_eq!(s.focus, original);
            assert!(s.initialized);
        }
    }
    #[test]
    fn ai_focus_keeps_real_origin_and_last_valid_native_tile() {
        let key = MapKey { tiles: 100, width: 256, height: 256, seed: 1, player: 1 };
        let mut s = State::default();
        assert!(s.request_tile(key, (0,0)));
        assert_eq!(s.focus.center, (64,32));
        let first = s.focus;
        assert!(s.request_tile(key, (255,255)));
        assert_eq!(s.focus.center, (16384,8192));
        assert!(s.focus.serial > first.serial);
    }
    #[test]
    fn invalid_ai_tile_does_not_initialize_or_replace_the_current_map() {
        let key = MapKey { tiles: 100, width: 256, height: 256, seed: 1, player: 1 };
        let mut s = State::default();
        assert!(!s.request_tile(key, (-1,-1)));
        assert!(!s.initialized);
        assert_eq!(s.map, None);
        assert!(s.request_tile(key, (0,0)));
        let original = s.focus;
        assert!(!s.request_tile(MapKey { tiles: 200, ..key }, (-1,-1)));
        assert_eq!(s.map, Some(key));
        assert_eq!(s.focus, original);
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
