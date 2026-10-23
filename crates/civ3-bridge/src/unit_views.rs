//! Units for independent map views. The game's animator only collects
//! units inside its own camera. Extend its existing animation tick to
//! the complete visible map on the game thread, using the same native
//! unit selector and visibility rules. Keep snapshots and argument arrays
//! separate from the viewport-limited native draw lists.

use std::collections::HashSet;

use civ3_sys::{DrawnUnit, ViewState};

/// Occupied, unloaded tiles once each. The original viewport is irrelevant
/// here: B can show any part of the world known to the viewing player.
fn occupied_tiles(units: &[(i32, i32, i32)], size: (i32, i32)) -> Vec<(i32, i32)> {
    let mut seen = HashSet::new();
    units.iter().filter_map(|&(x, y, container)| {
        (container == -1 && (0..size.0).contains(&x) && (0..size.1).contains(&y) && (x ^ y) & 1 == 0 && seen.insert((x, y)))
            .then_some((x, y))
    }).collect()
}

/// Keep the engine's choices on tiles it already collected. For other
/// tiles use the same selector, including the defender under a moving or
/// always-on-top unit. `select` also checks the engine's visibility gates.
fn additional_units(
    main: &[DrawnUnit],
    tiles: &[(i32, i32)],
    mut select: impl FnMut((i32, i32), Option<i32>) -> Option<DrawnUnit>,
) -> Vec<DrawnUnit> {
    let covered: HashSet<_> = main.iter().map(|u| (u.x, u.y)).collect();
    let mut out = Vec::new();
    for &tile in tiles {
        if covered.contains(&tile) { continue; }
        let Some(u) = select(tile, None).filter(|u| (u.x, u.y) == tile && u.container == -1) else { continue };
        if u.moving || u.always_on_top {
            if let Some(under) = select(tile, Some(u.id)).filter(|under| under.id != u.id && (under.x, under.y) == tile && under.container == -1) {
                out.push(under);
            }
        }
        out.push(u);
    }
    out
}

/// Merge fresh main-view animations with the complete map selection. A
/// tile's current engine draw list wins over a previous capture, so a
/// changed defender never draws on top of its replacement. Keep the two
/// units of a moving stack in order and all always-on-top units last.
fn complete_units(main: Vec<DrawnUnit>, other: Vec<DrawnUnit>, view: &ViewState, size: (i32, i32), wrap: (bool, bool)) -> Vec<DrawnUnit> {
    let covered: HashSet<_> = main.iter().map(|u| (u.x, u.y)).collect();
    let mut seen = HashSet::new();
    let mut out: Vec<_> = main.into_iter().chain(other.into_iter().filter(|u| !covered.contains(&(u.x, u.y))))
        .filter(|u| seen.insert(u.id)).collect();
    let axis = |v: i32, n: i32, from: i32, wraps: bool| if wraps && n > 0 { from + (v - from).rem_euclid(n) } else { v };
    out.sort_by_key(|u| if u.always_on_top {
        (1, 0, 0)
    } else {
        (0, axis(u.y, size.1, view.tile_y.0, wrap.1), -axis(u.x, size.0, view.tile_x.0, wrap.0))
    });
    out
}

#[cfg(windows)]
pub use live::{copy_captured_movement, drawn_units, install};

/// Identity must match before a calculated stat crosses from the game
/// thread's capture into a fresh memory-only publisher record. Addresses
/// are compared as opaque values; they are never dereferenced here.
fn copy_movement(current: &mut [DrawnUnit], captured: &[DrawnUnit]) {
    let identity = |u: &DrawnUnit| (u.native_address, u.id, u.unit_type, u.civ, u.army_member, u.army_size);
    let values: std::collections::HashMap<_, _> = captured.iter().filter(|u| u.native_address != 0)
        .filter_map(|u| u.max_move_points.map(|points| (identity(u), points))).collect();
    for unit in current {
        unit.max_move_points = unit.max_move_points.or_else(|| values.get(&identity(unit)).copied());
    }
}

/// Called only from the game's post-animation capture. Keeping the call
/// behind the nonzero identity gate also handles an object disappearing.
#[cfg(any(windows, test))]
fn sample_movement(unit: &mut DrawnUnit, read: impl FnOnce(u32) -> i32) {
    if unit.native_address != 0 { unit.max_move_points = Some(read(unit.native_address)); }
}

/// The same display gates are checked again for cached ids on each native
/// tick. An id surviving in the cache must not keep a now-fogged, loaded,
/// dead or undetected unit animating in an independent view.
#[cfg(any(windows, test))]
fn unit_may_animate(unit: &DrawnUnit, suppressed: bool, bypass_fog: bool, debug: bool, detected: bool, tile_visible: bool) -> bool {
    unit.container == -1 && unit.visible && !suppressed
        && (debug || detected) && (bypass_fog || unit.always_on_top || tile_visible)
}

/// The selected stack and pending animation stacks must reflect a new
/// command immediately even while the full-map defender cache is fresh.
#[cfg(any(windows, test))]
fn refresh_active_units(
    mut units: Vec<DrawnUnit>,
    active: &[(i32, i32)],
    select: impl FnMut((i32, i32), Option<i32>) -> Option<DrawnUnit>,
) -> Vec<DrawnUnit> {
    let active_set: HashSet<_> = active.iter().copied().collect();
    units.retain(|u| !active_set.contains(&(u.x, u.y)));
    units.extend(additional_units(&[], active, select));
    units
}

/// Retiring our membership must preserve a unit the original list now
/// owns. Otherwise a unit entering the native rectangle would have its
/// freshly set native membership cleared by our previous snapshot.
#[cfg(any(windows, test))]
fn retired_members(previous: &[i32], desired: &[i32], native: &HashSet<i32>) -> Vec<i32> {
    let wanted: HashSet<_> = desired.iter().copied().collect();
    previous.iter().copied().filter(|id| !wanted.contains(id) && !native.contains(id)).collect()
}

/// The native tick may contain the same unit through several display
/// paths. A single update must advance each object only once, including
/// extras already present in the native effect vector.
#[cfg(any(windows, test))]
fn animation_objects(native: &[u32], extra: impl IntoIterator<Item = u32>) -> Vec<u32> {
    let mut seen = HashSet::new();
    native.iter().copied().chain(extra).filter(|&p| p != 0 && seen.insert(p)).collect()
}

/// A failed post-update snapshot must not escape to the hook's generic
/// panic fallback, which would run the native animation tick twice.
#[cfg(any(windows, test))]
fn advance_and_capture(seconds: u32, advance: impl FnOnce(u32), capture: impl FnOnce()) {
    advance(seconds);
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(capture)).is_err() {
        log::error!("world animation snapshot panicked after the native tick");
    }
}

#[cfg(windows)]
mod live {
    use std::cell::RefCell;
    use std::time::{Duration, Instant};

    use inproc::Ptr32;
    use civ3_sys::{Civ3, Civ3Error, MapInfo, layout as L};
    use civ3_sys::modal::find_visible_unit;
    use civ3_sys::targeting::max_move_points;
    use civ3_sys::unit_views::{self as sys, advance_animations, build_unit_list, tile_visible, visible_to};

    use super::*;

    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    struct MapKey { tiles: u32, width: i32, height: i32, seed: u32, player: i32 }

    impl MapKey {
        fn of(map: &MapInfo, player: i32) -> Self {
            Self { tiles: map.tiles_address(), width: map.width, height: map.height, seed: map.seed, player }
        }
    }

    struct Capture { key: MapKey, units: Vec<DrawnUnit>, main_count: usize }
    /// Published by the game's thread after each tick, read by the publisher.
    static CAPTURE: civ3r_sync::Latest<Capture> = civ3r_sync::Latest::new();
    // Cache ids only. Raw pointers can become invalid when a unit dies,
    // loads into a transport, or a game is replaced between two ticks.
    struct Selection { key: MapKey, at: Instant, ids: Vec<i32> }
    thread_local! {
        // Both only on the game's thread (its animation hooks).
        static SELECTION: RefCell<Option<Selection>> = const { RefCell::new(None) };
        static MEMBERSHIP: RefCell<Option<Membership>> = const { RefCell::new(None) };
    }
    struct Membership { key: MapKey, ids: Vec<i32> }
    const PERIOD: Duration = Duration::from_millis(33);

    struct Selector {
        game: Civ3,
        find: find_visible_unit::Function,
        visible: visible_to::Function,
        tile: tile_visible::Function,
        max_moves: max_move_points::Function,
    }

    impl Selector {
        fn allowed(&self, ptr: u32, unit: &DrawnUnit, view: &ViewState, map: &MapInfo, debug: bool) -> bool {
            let tile = (unit.x, unit.y);
            if unit.container != -1 || !unit.visible || !(0..map.width).contains(&tile.0)
                || !(0..map.height).contains(&tile.1) || (tile.0 ^ tile.1) & 1 != 0 { return false; }
            let Ok((suppressed, bypass_fog)) = sys::display_flags(ptr) else { return false; };
            let detected = debug || self.visible.call(ptr, view.player_civ as u32, 1) & 0xFF != 0;
            let seen = bypass_fog || unit.always_on_top
                || self.tile.call(ptr, view.player_civ as u32, tile.0 as u32, tile.1 as u32) & 0xFF != 0;
            unit_may_animate(unit, suppressed, bypass_fog, debug, detected, seen)
        }

        fn selected(&self, map: &MapInfo, view: &ViewState, debug: bool) -> Result<Vec<i32>, Civ3Error> {
            let key = MapKey::of(map, view.player_civ);
            let now = Instant::now();
            let cached = SELECTION.with_borrow(|state| state.as_ref().filter(|s| s.key == key && now.saturating_duration_since(s.at) < PERIOD).map(|s| s.ids.clone()));
            if let Some(ids) = cached {
                return Ok(ids);
            }
            let occupied = occupied_tiles(&self.game.unit_tiles()?, (map.width, map.height));
            let units = additional_units(&[], &occupied, |(x, y), excluded| {
                let excluded = excluded.and_then(|id| self.game.unit_ptr(id).ok().flatten()).unwrap_or(0);
                let ptr = self.find.call(self.game.addrs.main_screen_form, x as u32, y as u32, excluded);
                if ptr == 0 { return None; }
                let unit = self.game.drawn_unit_at(ptr).ok()?;
                self.allowed(ptr, &unit, view, map, debug).then_some(unit)
            });
            let ids: Vec<_> = units.into_iter().map(|u| u.id).collect();
            SELECTION.set(Some(Selection { key, at: now, ids: ids.clone() }));
            Ok(ids)
        }

        fn records(&self, map: &MapInfo, view: &ViewState, debug: bool) -> Result<Vec<DrawnUnit>, Civ3Error> {
            let mut units: Vec<_> = self.selected(map, view, debug)?.into_iter().filter_map(|id| {
                let ptr = self.game.unit_ptr(id).ok().flatten()?;
                let unit = self.game.drawn_unit_at(ptr).ok()?;
                self.allowed(ptr, &unit, view, map, debug).then_some(unit)
            }).collect();
            let mut active = sys::pending_unit_tiles(self.game)?;
            if let Some(id) = self.game.current_unit()? && let Some(unit) = self.game.unit(id)? {
                active.push((unit.x, unit.y));
            }
            active.retain(|&(x, y)| (0..map.width).contains(&x) && (0..map.height).contains(&y) && (x ^ y) & 1 == 0);
            active.sort_unstable();
            active.dedup();
            if !active.is_empty() {
                units = refresh_active_units(units, &active, |(x, y), excluded| {
                    let excluded = excluded.and_then(|id| self.game.unit_ptr(id).ok().flatten()).unwrap_or(0);
                    let ptr = self.find.call(self.game.addrs.main_screen_form, x as u32, y as u32, excluded);
                    if ptr == 0 { return None; }
                    let unit = self.game.drawn_unit_at(ptr).ok()?;
                    self.allowed(ptr, &unit, view, map, debug).then_some(unit)
                });
            }
            Ok(units)
        }

        fn clear_membership(&self) {
            let previous = MEMBERSHIP.take();
            let current = self.game.map().ok().zip(self.game.view().ok());
            if let Some(previous) = previous && let Some((map, view)) = current
                && previous.key == MapKey::of(&map, view.player_civ)
            {
                for id in previous.ids {
                    if let Ok(Some(ptr)) = self.game.unit_ptr(id) {
                        let _ = sys::set_animation_member(ptr, false, true);
                    }
                }
            }
        }

        fn sync_membership(&self, key: MapKey, units: &[DrawnUnit], native: &HashSet<i32>) {
            let desired: Vec<_> = units.iter().map(|u| u.id).filter(|id| !native.contains(id)).collect();
            let previous = MEMBERSHIP.take();
            if let Some(previous) = previous.filter(|p| p.key == key) {
                for id in retired_members(&previous.ids, &desired, native) {
                    if let Ok(Some(ptr)) = self.game.unit_ptr(id) {
                        let _ = sys::set_animation_member(ptr, false, true);
                    }
                }
            }
            let mut ids = Vec::new();
            for id in desired {
                if let Ok(Some(ptr)) = self.game.unit_ptr(id) && sys::set_animation_member(ptr, true, false).is_ok() {
                    ids.push(id);
                }
            }
            MEMBERSHIP.set(Some(Membership { key, ids }));
        }

        fn after_build(&self) -> Result<(), Civ3Error> {
            let map = self.game.map()?;
            let view = self.game.view()?;
            let debug = self.game.debug_mode_bits()? & 8 != 0;
            let main = self.game.drawn_units()?;
            let native = main.iter().map(|u| u.id).collect();
            let extra = self.records(&map, &view, debug)?;
            let units = complete_units(main, extra, &view, (map.width, map.height), (map.wrap_x, map.wrap_y));
            self.sync_membership(MapKey::of(&map, view.player_civ), &units, &native);
            Ok(())
        }

        fn prepare(&self, call: &advance_animations::Call) -> Result<Prepared, Civ3Error> {
            let game = self.game;
            let map = game.map()?;
            let view = game.view()?;
            let debug = game.debug_mode_bits()? & 8 != 0;
            // These arrays exist only for this native invocation. No list
            // capacity or pointer in Animator is changed.
            if call.count > 0x10000 { return Err(Civ3Error::NoGame); }
            let native = Ptr32::<u32>::new(call.units).read_array(call.count as usize)?;
            let main: Vec<_> = native.iter().filter_map(|&ptr| game.drawn_unit_at(ptr).ok()).collect();
            let main_count = main.len();
            let native_ids = main.iter().map(|u| u.id).collect();
            let extra = self.records(&map, &view, debug)?;
            let units = complete_units(main, extra, &view, (map.width, map.height), (map.wrap_x, map.wrap_y));
            let ids = units.iter().map(|u| u.id).collect();
            let unit_ptrs = animation_objects(&native, units.iter().filter_map(|u| game.unit_ptr(u.id).ok().flatten()));

            let mut effect_header: [u32; 4] = Ptr32::new(call.effects).read()?;
            let (begin, end) = (effect_header[1], effect_header[2]);
            if end < begin || (end - begin) % 4 != 0 || (end - begin) / 4 > 0x10000 {
                return Err(Civ3Error::NoGame);
            }
            let native_effects = if begin == end { Vec::new() } else { Ptr32::<u32>::new(begin).read_array(((end - begin) / 4) as usize)? };
            let viewer = sys::effect_viewer(game, view.player_civ)?;
            let extra_effects = sys::effect_animations(game)?.into_iter().filter(|e| {
                let (x, y) = e.tile;
                (0..map.width).contains(&x) && (0..map.height).contains(&y) && (x ^ y) & 1 == 0
                    && game.tile_record(x, y).is_ok_and(|tile| sys::effect_tile_visible(&tile, viewer))
            }).map(|e| e.animation);
            let effect_ptrs = animation_objects(&native_effects, extra_effects);
            effect_header[1] = effect_ptrs.as_ptr() as usize as u32;
            effect_header[2] = effect_header[1] + effect_ptrs.len() as u32 * 4;
            effect_header[3] = effect_header[2];
            self.sync_membership(MapKey::of(&map, view.player_civ), &units, &native_ids);
            Ok(Prepared { key: MapKey::of(&map, view.player_civ), ids, unit_ptrs, effect_ptrs, effect_header, main_count })
        }

        fn capture(&self, prepared: &Prepared) {
            // Read *after* the original elapsed-time update, so the
            // publisher receives current animation frames, not a frozen
            // record copied by the viewport's earlier list builder.
            let current = self.game.map().ok().zip(self.game.view().ok());
            let Some((map, view)) = current.filter(|(map, view)| MapKey::of(map, view.player_civ) == prepared.key) else {
                CAPTURE.clear();
                SELECTION.set(None);
                return;
            };
            let debug = self.game.debug_mode_bits().unwrap_or(0) & 8 != 0;
            let units = prepared.ids.iter().filter_map(|&id| {
                let ptr = self.game.unit_ptr(id).ok().flatten()?;
                let mut unit = self.game.drawn_unit_at(ptr).ok()?;
                if !self.allowed(ptr, &unit, &view, &map, debug) { return None; }
                // This getter reads native movement rules and army members.
                // C3X's entry redirection is retained by the checked handle.
                // It runs after the original tick, on the game thread only.
                sample_movement(&mut unit, |address| self.max_moves.call(address) as i32);
                Some(unit)
            }).collect();
            let units = complete_units(Vec::new(), units, &view, (map.width, map.height), (map.wrap_x, map.wrap_y));
            let capture = Capture { key: prepared.key, units, main_count: prepared.main_count };
            let previous = CAPTURE.load();
            if previous.as_deref().map(|p| (p.key, p.main_count, p.units.len())) != Some((capture.key, capture.main_count, capture.units.len())) {
                log::debug!("world units: {} in the game camera, {} visible across the map; {} effect animations", capture.main_count, capture.units.len(), prepared.effect_ptrs.len());
            }
            CAPTURE.publish(capture);
        }
    }

    struct Prepared {
        key: MapKey,
        ids: Vec<i32>,
        unit_ptrs: Vec<u32>,
        // Keeps the backing allocation alive through the original call.
        effect_ptrs: Vec<u32>,
        effect_header: [u32; 4],
        main_count: usize,
    }

    /// Extend the game's one animation tick to the complete visible map.
    /// The original function alone advances time, the cursor and every
    /// effect. Its float argument is passed through bit-for-bit. This is
    /// deliberately neither another timer nor a second call per frame.
    pub fn install(game: Civ3) -> Result<(), String> {
        let sites = sys::sites(game.build).ok_or_else(|| format!("no world-unit sites for {:?}", game.build))?;
        let find_site = civ3_sys::modal::function_sites(game.build).ok_or("no unit selector")?.find_visible_unit;
        let e = |what: &str, error: inproc::hook::HookError| format!("{what}: {error}");
        let moves_site = civ3_sys::targeting::sites(game.build).ok_or("no unit movement getter")?.max_move_points;
        let selector = std::sync::Arc::new(Selector {
            game,
            max_moves: max_move_points::Function::at(moves_site.0, moves_site.1).map_err(|err| e("unit max movement", err))?,
            find: find_visible_unit::Function::at(find_site.0, find_site.1).map_err(|err| e("find_visible_unit", err))?,
            visible: visible_to::Function::at(sites.visible_to.0, sites.visible_to.1).map_err(|err| e("unit visibility", err))?,
            tile: tile_visible::Function::at(sites.tile_visible.0, sites.tile_visible.1).map_err(|err| e("tile visibility", err))?,
        });
        let build_selector = std::sync::Arc::clone(&selector);
        let animator = game.addrs.main_screen_form + L::MAIN_SCREEN_FORM_ANIMATOR;
        let advance = advance_animations::install_at_start(sites.advance_animations.0, sites.advance_animations.1, move |call| {
            // Other callers can animate UI previews. They retain their own
            // arrays and timing; only the main map's invocation expands.
            if call.units != animator + L::ANIMATOR_UNITS || call.effects != animator + 0x18DC {
                return call.original();
            }
            note_frame(animator);
            match selector.prepare(call) {
                Ok(prepared) => {
                    advance_and_capture(call.seconds, |seconds| {
                        call.original_with(call.images, seconds, prepared.unit_ptrs.as_ptr() as usize as u32,
                            prepared.unit_ptrs.len() as u32, prepared.effect_header.as_ptr() as usize as u32);
                    }, || selector.capture(&prepared));
                }
                Err(error) => {
                    log::debug!("world animation selection: {error}");
                    CAPTURE.clear();
                    SELECTION.set(None);
                    call.original();
                }
            }
        }).map_err(|err| e("world animation tick", err))?;
        let build = build_unit_list::install_at_start(sites.build_unit_list.0, sites.build_unit_list.1, move |call| {
            crate::native_camera::maintain(game);
            build_selector.clear_membership();
            call.original();
            // Membership, like frame capture, must not panic into the
            // generic hook fallback after the original already ran.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if let Err(error) = build_selector.after_build() { log::debug!("world animation membership: {error}"); }
            })).is_err() {
                log::error!("world animation membership snapshot panicked after list build");
            }
        }).map_err(|err| e("animator unit list", err))?;
        // Keep either both hooks or neither, so camera parking can depend
        // on this installation completing successfully.
        std::mem::forget(advance);
        std::mem::forget(build);
        log::info!("world units: independent views select and animate visible units and effects beyond the game's camera");
        Ok(())
    }

    /// One frame of the animator's (`crate::anim_steps`): a unit moves
    /// while one in its queue of one-shot animations (+0x100C, count at
    /// +0x180C) has its moving byte (+0x5D, set by the step loop 0x5BAB70).
    fn note_frame(animator: u32) {
        thread_local! {
            static STEPS: RefCell<crate::anim_steps::Steps> = RefCell::new(Default::default());
            static EPOCH: Instant = Instant::now();
        }
        let now = EPOCH.with(|e| e.elapsed().as_nanos() as u64);
        let count = Ptr32::<i32>::new(animator + 0x180C).read().unwrap_or(0).clamp(0, 16) as usize;
        let moving = Ptr32::<u32>::new(animator + 0x100C).read_array(count).unwrap_or_default().into_iter()
            .any(|unit| unit != 0 && Ptr32::<u8>::new(unit + 0x5D).read().is_ok_and(|b| b != 0));
        let at = crate::anim_steps::Sample { now, moving, cpu: inproc::process::thread_cpu_ns(), accesses: inproc::mem::thread_accesses() };
        if let Some(line) = STEPS.with_borrow_mut(|s| s.frame(at, inproc::hook_time::snapshot)) {
            log::info!("{line}");
        }
    }

    /// Enrich a trace's native-camera records from copied game-thread stats.
    /// This API performs memory reads and comparisons, never native calls.
    pub fn copy_captured_movement(game: &Civ3, units: &mut [DrawnUnit]) -> Result<(), Civ3Error> {
        let map = game.map()?;
        let view = game.view()?;
        if let Some(capture) = CAPTURE.load().filter(|c| c.key == MapKey::of(&map, view.player_civ)) {
            copy_movement(units, &capture.units);
        }
        Ok(())
    }

    /// Called by the publisher. Only copied snapshots cross threads;
    /// selectors, visibility checks and native animation stay on the game
    /// thread. Fresh native entries replace stale choices on their tile.
    pub fn drawn_units(game: &Civ3) -> Result<Vec<DrawnUnit>, Civ3Error> {
        let mut main = game.drawn_units()?;
        let map = game.map()?;
        let view = game.view()?;
        let other = CAPTURE.load()
            .filter(|c| c.key == MapKey::of(&map, view.player_civ)).map(|c| c.units.clone()).unwrap_or_default();
        copy_movement(&mut main, &other);
        Ok(complete_units(main, other, &view, (map.width, map.height), (map.wrap_x, map.wrap_y)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(id: i32, x: i32, y: i32) -> DrawnUnit {
        DrawnUnit { native_address: 0x1000 + id as u32 * 0x100, max_move_points: None, id, x, y, civ: 1, unit_type: 0, leader_kind: 0, experience: 0, damage: 0, moves: 0, container: -1, state: 0, always_on_top: false,
            pixel: (64*x+64, 32*y+32), direction: 3, animation: 1, frame: 0, visible: true, army_member: -1, army_size: 0, moving: false }
    }
    fn view() -> ViewState {
        ViewState { camera_x: 0, camera_y: 0, mouse_x: 0, mouse_y: 0, tile_x: (0, 10), tile_y: (0, 10), current_unit: 0, player_civ: 1 }
    }
    fn ids(units: &[DrawnUnit]) -> Vec<i32> { units.iter().map(|u| u.id).collect() }

    #[test]
    fn calculated_moves_require_the_same_native_object_and_army_contents() {
        let captured = DrawnUnit { max_move_points: Some(17), ..unit(2, 80, 40) };
        let fresh = DrawnUnit { frame: 9, x: 82, moves: 3, ..unit(2, 80, 40) };
        let mut current = [fresh];
        copy_movement(&mut current, &[captured]);
        assert_eq!(current[0].max_move_points, Some(17));
        for replaced in [
            DrawnUnit { native_address: captured.native_address + 0x100, ..fresh },
            DrawnUnit { id: captured.id + 1, ..fresh },
            DrawnUnit { unit_type: captured.unit_type + 1, ..fresh },
            DrawnUnit { civ: captured.civ + 1, ..fresh },
            DrawnUnit { army_size: captured.army_size + 1, ..fresh },
            DrawnUnit { army_member: 999, ..fresh },
            DrawnUnit { native_address: 0, ..fresh },
        ] {
            let mut current = [replaced];
            copy_movement(&mut current, &[captured]);
            assert_eq!(current[0].max_move_points, None, "a reused id/address or changed unit cannot inherit old movement: {replaced:?}");
        }
    }

    #[test]
    fn movement_getter_runs_only_in_the_post_animation_capture() {
        use std::cell::RefCell;
        let events = RefCell::new(Vec::new());
        let mut captured = unit(2, 80, 40);
        assert_eq!(captured.max_move_points, None);
        advance_and_capture(0.125f32.to_bits(), |_| events.borrow_mut().push("native animation"), || {
            sample_movement(&mut captured, |address| {
                assert_eq!(address, 0x1200);
                events.borrow_mut().push("native movement getter");
                0 // Immobile is a valid result, not missing metadata.
            });
            events.borrow_mut().push("publish copied stats");
        });
        assert_eq!(*events.borrow(), ["native animation", "native movement getter", "publish copied stats"]);
        assert_eq!(captured.max_move_points, Some(0));
        let mut publisher = [unit(2, 80, 40)];
        copy_movement(&mut publisher, &[captured]);
        assert_eq!(publisher[0].max_move_points, Some(0));
        assert_eq!(events.borrow().len(), 3, "publisher copies never invoke a game function");
        let mut gone = DrawnUnit { native_address: 0, ..unit(3, 90, 40) };
        sample_movement(&mut gone, |_| panic!("a missing object must never be called"));
        assert_eq!(gone.max_move_points, None);
    }

    #[test]
    fn a_new_command_refreshes_only_its_stack_before_the_selector_cache_expires() {
        let old_defender = unit(1, 80, 40);
        let unrelated = unit(2, 60, 20);
        let selected = unit(3, 80, 40);
        let cached = vec![old_defender, unrelated];
        let active = [(80, 40)];
        let updated = refresh_active_units(cached.clone(), &active, |_, _| Some(selected));
        assert_eq!(ids(&updated), vec![2, 3], "new selection replaces the former defender without another full-map scan");
        let moving = DrawnUnit { moving: true, ..selected };
        let updated = refresh_active_units(cached.clone(), &active, |_, excluded| {
            Some(if excluded == Some(3) { old_defender } else { moving })
        });
        assert_eq!(ids(&updated), vec![2, 1, 3], "pending movement keeps the underlying defender");
        let hidden = refresh_active_units(cached, &active, |_, _| None);
        assert_eq!(ids(&hidden), vec![2], "an active stack becoming invisible does not retain its cached defender");
    }

    #[test]
    fn disappearing_animation_members_are_cleared_without_clearing_native_members() {
        let native = HashSet::from([2, 4]);
        assert_eq!(retired_members(&[1, 2, 3], &[1, 5], &native), vec![3]);
        assert_eq!(retired_members(&[1, 2, 3], &[], &native), vec![1, 3]);
        assert!(retired_members(&[2, 4], &[], &native).is_empty(), "moving into the native rectangle keeps its membership");
    }

    #[test]
    fn cached_unit_ids_cannot_advance_after_their_visibility_is_lost() {
        let visible = unit(1, 80, 40);
        assert!(unit_may_animate(&visible, false, false, false, true, true));
        assert!(!unit_may_animate(&visible, false, false, false, true, false), "new fog applies before the next cached selector refresh");
        assert!(!unit_may_animate(&visible, false, false, false, false, true), "invisible enemy without a detector");
        assert!(!unit_may_animate(&visible, true, true, true, true, true), "engine suppression still applies in debug");
        assert!(!unit_may_animate(&DrawnUnit { container: 7, ..visible }, false, false, false, true, true), "cargo does not acquire its own tick");
        assert!(!unit_may_animate(&DrawnUnit { visible: false, ..visible }, false, true, true, true, true), "finished death animation stays hidden");
        assert!(unit_may_animate(&visible, false, true, false, true, false), "engine fog bypass during a boundary-crossing animation");
        assert!(unit_may_animate(&DrawnUnit { always_on_top: true, ..visible }, false, false, false, true, false));
        assert!(!unit_may_animate(&DrawnUnit { always_on_top: true, ..visible }, false, false, false, false, false), "always-on-top does not reveal an undetected enemy");
        assert!(unit_may_animate(&visible, false, false, true, false, true), "debug native tile gate supplies current visibility");
    }

    #[test]
    fn native_and_independent_views_advance_each_unit_and_effect_once() {
        // Native stationary units, a unit shared by A/B, and the native
        // always-on-top combat unit may overlap the independent selection.
        let native = [0x1000, 0x2000, 0x4000, 0x2000];
        let extra = [0x2000, 0x3000, 0x4000, 0x5000];
        assert_eq!(animation_objects(&native, extra), vec![0x1000, 0x2000, 0x4000, 0x3000, 0x5000]);
        // The original vector includes global aircraft/SAM extras. The
        // city and tile additions must not duplicate them or each other.
        let native_effects = [0x8000, 0xA000, 0xB000];
        assert_eq!(animation_objects(&native_effects, [0x8000, 0x9000, 0xA000, 0x9000]),
            vec![0x8000, 0xA000, 0xB000, 0x9000]);
        assert!(animation_objects(&[], [0]).is_empty());
    }

    #[test]
    fn one_native_elapsed_time_update_precedes_the_current_frame_snapshot() {
        use std::cell::{Cell, RefCell};
        let calls = Cell::new(0);
        let frame = Cell::new(0);
        let snapshots = RefCell::new(Vec::new());
        // Includes fractional seconds at differing frame rates and a
        // long tick, whose native >0.5-second fallback must remain native.
        // No conversion to milliseconds or independent clock is allowed.
        let elapsed = [0.0f32, 1.0/240.0, 1.0/60.0, 0.0012345678, 0.6];
        let received = RefCell::new(Vec::new());
        for seconds in elapsed {
            advance_and_capture(seconds.to_bits(), |word| {
                calls.set(calls.get() + 1);
                received.borrow_mut().push(word);
                frame.set(frame.get() + 1);
            }, || snapshots.borrow_mut().push(frame.get()));
        }
        assert_eq!(calls.get(), elapsed.len());
        assert_eq!(*received.borrow(), elapsed.map(f32::to_bits));
        assert_eq!(*snapshots.borrow(), vec![1, 2, 3, 4, 5]);
        // A snapshot failure must not invoke the outer foreign-hook
        // fallback, whose default behavior calls the original again.
        advance_and_capture(0.125f32.to_bits(), |_| calls.set(calls.get() + 1), || panic!("bad snapshot"));
        assert_eq!(calls.get(), elapsed.len() + 1);
    }

    #[test]
    fn a_unit_only_in_b_is_collected_without_moving_a() {
        let main = vec![unit(1, 4, 4)];
        let b = DrawnUnit { max_move_points: Some(12), ..unit(2, 80, 40) };
        let tiles = occupied_tiles(&[(4,4,-1), (80,40,-1), (80,40,-1), (90,40,2), (-1,3,-1), (81,40,-1)], (100,60));
        assert_eq!(tiles, vec![(4,4), (80,40)]);
        let mut requested = Vec::new();
        let extra = additional_units(&main, &tiles, |tile, excluded| { requested.push((tile, excluded)); Some(b) });
        assert_eq!(requested, vec![((80,40),None)]);
        let a = view();
        let full = complete_units(main, extra, &a, (100,60), (false,false));
        assert_eq!(ids(&full), vec![1,2]);
        assert!(full.iter().any(|u| u.pixel == b.pixel && u.max_move_points == Some(12)));
        assert_eq!(a, view(), "collecting B never changes A's camera");
    }

    #[test]
    fn off_camera_fog_and_invisible_units_are_not_added() {
        let tiles = vec![(40,20), (60,20), (80,20)];
        // The engine's selector/visibility gates reject the fogged tile
        // and an undetected invisible unit, even with a valid unit there.
        let extra = additional_units(&[], &tiles, |tile, _| (tile == (60,20)).then(|| unit(2,60,20)));
        assert_eq!(ids(&extra), vec![2]);
    }

    #[test]
    fn a_moving_unit_in_b_keeps_its_underlying_defender_and_stack_order() {
        let moving = DrawnUnit { moving: true, frame: 7, ..unit(2,80,40) };
        let under = unit(3,80,40);
        let extra = additional_units(&[], &[(80,40)], |_, excluded| Some(if excluded == Some(2) { under } else { moving }));
        assert_eq!(ids(&extra), vec![3,2]);
        let full = complete_units(vec![unit(1,4,4)], extra, &view(), (100,60), (false,false));
        assert_eq!(ids(&full), vec![1,3,2]);
        assert_eq!(full.last().unwrap().frame,7);
    }

    #[test]
    fn current_a_defender_replaces_stale_selection_and_top_units_stay_last() {
        let current = DrawnUnit { frame: 9, ..unit(4,80,40) };
        let top = DrawnUnit { always_on_top: true, ..unit(5,2,2) };
        let full = complete_units(vec![current,top], vec![unit(2,80,40),unit(3,80,40),unit(6,40,20)], &view(), (100,60), (false,false));
        assert_eq!(ids(&full),vec![6,4,5]);
        assert_eq!(full[1].frame,9);
    }

    #[test]
    fn engine_overrides_and_loaded_under_units_do_not_duplicate_sprites() {
        let top = DrawnUnit { always_on_top: true, ..unit(2,80,40) };
        assert_eq!(ids(&additional_units(&[], &[(80,40)], |_,_| Some(top))),vec![2]);
        let loaded = DrawnUnit { container: 2, ..unit(3,80,40) };
        assert_eq!(ids(&additional_units(&[], &[(80,40)], |_,excluded| Some(if excluded.is_some(){loaded}else{top}))),vec![2]);
        assert!(additional_units(&[], &[(20,20)], |_,_| Some(top)).is_empty(),"a C3X display override from another tile is not repeated across the map");
    }

    #[test]
    fn wrapped_rows_and_columns_match_the_map_layer_draw_order() {
        let mut a=view(); a.tile_x.0=80; a.tile_y.0=40;
        let full=complete_units(vec![unit(1,90,40)],vec![unit(2,2,40),unit(3,90,2),unit(4,4,2)],&a,(100,60),(true,true));
        assert_eq!(ids(&full),vec![2,1,4,3]);
    }
}
