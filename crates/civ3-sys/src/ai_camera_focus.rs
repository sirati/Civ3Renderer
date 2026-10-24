//! Semantic camera-focus events for AI movement and combat.
//!
//! The native animator only visits the engine's tiny parked camera range.  AI
//! moves and combat animations still go through the game thread even when the
//! event tile is outside that range, so publish the event before the native
//! animator's camera-bounded fast paths.  Visibility and the game's move
//! preference bits remain gates: this module never changes fog or detection.

use inproc::Ptr32;

use crate::{Build, Civ3, Civ3Error, layout as L};

/// Native preference bits from Civ3Conquests.h.
pub const P_SHOW_FRIEND_MOVES: u32 = 0x0080_0000;
pub const P_SHOW_ENEMY_MOVES: u32 = 0x0100_0000;
/// Bombard effects use this preference. Ordinary combat focus follows the
/// accepted native visibility gate independently of the movement preferences.
pub const P_ANIMATE_BATTLES: u32 = 0x0000_0400;

pub const UNIT_MOVE_SITE: u32 = 0x5B_CF30;
pub const UNIT_MOVE_PROLOGUE: &[u8] = &[0x83, 0xEC, 0x08, 0x53, 0x55];
pub const UNIT_BOMBARD_SITE: u32 = 0x5C_1120;
pub const UNIT_BOMBARD_PROLOGUE: &[u8] = &[0x53, 0x8B, 0x5C, 0x24, 0x08, 0x56, 0x57, 0x8B, 0x7C, 0x24, 0x14, 0x8B, 0xF1];
pub const COMBAT_VISIBILITY_SITE: u32 = 0x4A_1D60;
pub const COMBAT_VISIBILITY_PROLOGUE: &[u8] = &[0x51, 0x8A, 0x44, 0x24, 0x10, 0x53, 0x55];

inproc::fastcall_hook! {
    /// `void Unit::move(this, int edx, int tile_x, int tile_y)`.
    pub mod unit_move(unit, edx; tile_x, tile_y) -> ();
}

inproc::fastcall_hook! {
    /// `void Unit::bombard_tile(this, int edx, int x, int y)`.
    pub mod unit_bombard(unit, edx; tile_x, tile_y) -> ();
}

inproc::fastcall_hook! {
    /// `bool Fighter::check_combat_anim_visibility(this, int edx,
    /// Unit *attacker, Unit *defender, bool param_3)`.
    pub mod combat_visibility(fighter, edx; attacker, defender, param_3) -> u32;
}

inproc::thiscall_fn! {
    /// Leader::is_enemy_unit classifies hidden nationality through civ zero.
    /// This is the exact classifier used at Fighter 0x4A1DED.
    pub fn enemy_unit(leader; unit) -> u32;
}

inproc::thiscall_fn! {
    /// Game::get_preferences(unit_class), including native air-unit policy.
    pub fn game_preferences(game; unit_class) -> u32;
}

const ENEMY_UNIT_SITE: u32 = 0x55_8F20;
const ENEMY_UNIT_PROLOGUE: &[u8] = &[0x53, 0x8B, 0x5C, 0x24, 0x08, 0x56];
const GAME_PREFERENCES_SITE: u32 = 0x53_9350;
const GAME_PREFERENCES_PROLOGUE: &[u8] = &[0x56, 0x8B, 0x71, 0x20, 0xE8, 0x67, 0x22, 0xF4, 0xFF];

#[derive(Copy, Clone)]
struct Policy {
    visible: crate::unit_views::visible_to::Function,
    tile: crate::unit_views::tile_visible::Function,
    enemy: enemy_unit::Function,
    preferences: game_preferences::Function,
}

impl Policy {
    fn preferences(self, game: Civ3, unit: u32) -> Option<u32> {
        let kind = Ptr32::<i32>::new(unit.checked_add(L::UNIT_BODY + L::UNIT_BODY_UNITTYPEID)?).read().ok()?;
        let count = Ptr32::<i32>::new(game.addrs.bic_data + L::BIC_UNITTYPECOUNT).read().ok()?;
        if kind < 0 || kind >= count { return None; }
        let types = Ptr32::<Ptr32<u8>>::new(game.addrs.bic_data + L::BIC_UNITTYPES).read().ok()?;
        let class = types.offset::<i32>(kind as u32 * L::UNITTYPE_SIZE + L::UNITTYPE_UNIT_CLASS).ok()?.read().ok()?;
        Some(self.preferences.call(game.addrs.game, class as u32))
    }
    fn tile_visible(self, game: Civ3, unit: u32, player: i32, tile: (i32, i32)) -> bool {
        if !(0..32).contains(&player) || unit == 0 { return false; }
        // Match native animation visibility, including debug and diplomatic
        // shared vision. A camera/range test never participates here.
        game.debug_mode_bits().unwrap_or(0) & 8 != 0
            || self.tile.call(unit, player as u32, tile.0 as u32, tile.1 as u32) & 0xff != 0
    }
    fn enemy(self, game: Civ3, unit: u32, player: i32) -> bool {
        self.enemy.call(game.addrs.leaders + player as u32 * L::LEADER_SIZE, unit) & 0xff != 0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sites {
    pub unit_move: (u32, &'static [u8]),
    pub unit_bombard: (u32, &'static [u8]),
    pub combat_visibility: (u32, &'static [u8]),
}

pub fn sites(build: Build) -> Option<Sites> {
    (build == Build::PcGames).then_some(Sites {
        unit_move: (UNIT_MOVE_SITE, UNIT_MOVE_PROLOGUE),
        unit_bombard: (UNIT_BOMBARD_SITE, UNIT_BOMBARD_PROLOGUE),
        combat_visibility: (COMBAT_VISIBILITY_SITE, COMBAT_VISIBILITY_PROLOGUE),
    })
}

fn wrap(v: i32, n: i32, enabled: bool) -> Option<i32> {
    if n <= 0 { return None; }
    if enabled { Some(v.rem_euclid(n)) } else { Some(v).filter(|&v| (0..n).contains(&v)) }
}

/// The game's current tile visibility words.  This mirrors the native
/// `Leader_is_tile_visible` rule and deliberately excludes exploration-only
/// (`Fog_Of_War`) data.
pub fn tile_visible_to_player(game: Civ3, tile: (i32, i32), player: i32) -> Result<bool, Civ3Error> {
    if !(0..32).contains(&player) { return Ok(false); }
    let map = game.map()?;
    let x = wrap(tile.0, map.width, map.wrap_x).ok_or(Civ3Error::NoGame)?;
    let y = wrap(tile.1, map.height, map.wrap_y).ok_or(Civ3Error::NoGame)?;
    // Civ3 stores only real (even-parity) tiles in the packed Tile array.
    if ((x + y) & 1) != 0 { return Ok(false); }
    let index = ((y as i64 * i64::from(map.width) + i64::from(x)) / 2) as usize;
    let tiles: Ptr32<Ptr32<u8>> = Ptr32::new(game.addrs.bic_data + L::BIC_MAP + L::MAP_TILES).read()?;
    let tile = tiles.index(index as u32)?.read()?;
    if tile.is_null() { return Ok(false); }
    if game.debug_mode_bits()? & 8 != 0 { return Ok(true); }
    let base = tile.raw().checked_add(L::TILE_BODY).ok_or(Civ3Error::NoGame)?;
    let bits = Ptr32::<u32>::new(base + L::TILE_BODY_FOWSTATUS).read()?
        | Ptr32::<u32>::new(base + L::TILE_BODY_V3).read()?
        | Ptr32::<u32>::new(base + L::TILE_BODY_VISIBILITY).read()?
        | Ptr32::<u32>::new(base + L::TILE_BODY_FIELD_D0_VISIBILITY).read()?;
    Ok(bits & (1u32 << player) != 0)
}

fn player_civ(game: Civ3) -> Result<i32, Civ3Error> {
    Ok(Ptr32::<i32>::new(game.addrs.main_screen_form + L::MAIN_SCREEN_FORM_PLAYER_CIVID).read()?)
}

fn unit_owner(unit: u32) -> Option<i32> {
    let body = unit.checked_add(L::UNIT_BODY).and_then(|p| p.checked_add(L::UNIT_BODY_CIVID))?;
    Ptr32::<i32>::new(body).read().ok().filter(|owner| (0..32).contains(owner))
}

fn unit_tile(unit: u32) -> Option<(i32, i32)> {
    let body = unit.checked_add(L::UNIT_BODY)?;
    Some((Ptr32::<i32>::new(body + L::UNIT_BODY_X).read().ok()?, Ptr32::<i32>::new(body + L::UNIT_BODY_Y).read().ok()?))
}

fn unit_detected(game: Civ3, visible: crate::unit_views::visible_to::Function, unit: u32, player: i32) -> bool {
    if unit == 0 || !(0..32).contains(&player) { return false; }
    game.debug_mode_bits().unwrap_or(0) & 8 != 0 || visible.call(unit, player as u32, 1) & 0xff != 0
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FocusEndpoint { Target, Source }

/// Native movement preference and visibility policy, independent of process
/// memory. `enemy` is the native Leader.At_War classification.
pub fn choose_move_focus(owner: i32, player: i32, prefs: u32, enemy: bool,
    target_visible: bool, target_detected: bool, source_visible: bool, source_detected: bool) -> Option<FocusEndpoint> {
    if owner == player || !(0..32).contains(&owner) || !(0..32).contains(&player) { return None; }
    let required = if enemy { P_SHOW_ENEMY_MOVES } else { P_SHOW_FRIEND_MOVES };
    if prefs & required == 0 { return None; }
    if target_visible && target_detected { Some(FocusEndpoint::Target) }
    else if source_visible && source_detected { Some(FocusEndpoint::Source) }
    else { None }
}

pub fn should_focus_combat_endpoint(tile_visible: bool, detected: bool) -> bool {
    tile_visible && detected
}

fn visible_endpoint(game: Civ3, policy: Policy, unit: u32, player: i32) -> Option<(i32, i32)> {
    let tile = unit_tile(unit)?;
    should_focus_combat_endpoint(policy.tile_visible(game, unit, player, tile), unit_detected(game, policy.visible, unit, player)).then_some(tile)
}

fn focus_move_unit(game: Civ3, policy: Policy, unit: u32, target: (i32, i32)) {
    let Ok(player) = player_civ(game) else { return };
    let Some(owner) = unit_owner(unit) else { return };
    if owner == player || !(0..32).contains(&player) { return; }
    let Some(prefs) = policy.preferences(game, unit) else { return };
    let enemy = policy.enemy(game, unit, player);
    let required = if enemy { P_SHOW_ENEMY_MOVES } else { P_SHOW_FRIEND_MOVES };
    if prefs & required == 0 { return; }
    let detected = unit_detected(game, policy.visible, unit, player);
    if !detected { return; }
    let target_visible = policy.tile_visible(game, unit, player, target);
    let source = unit_tile(unit);
    let source_visible = !target_visible && source.is_some_and(|source| policy.tile_visible(game, unit, player, source));
    match choose_move_focus(owner, player, prefs, enemy, target_visible, detected, source_visible, detected) {
        Some(FocusEndpoint::Target) => { crate::native_camera::request_tile(game, target); },
        Some(FocusEndpoint::Source) => if let Some(source) = source { crate::native_camera::request_tile(game, source); },
        None => (),
    }
}

fn focus_bombard(game: Civ3, policy: Policy, unit: u32, target: (i32, i32)) {
    let Ok(player) = player_civ(game) else { return };
    let Some(prefs) = policy.preferences(game, unit) else { return };
    // A visible hit is a combat event even when movement-following is off.
    // Focusing this already visible target does not expose a hidden shooter.
    if prefs & P_ANIMATE_BATTLES != 0 && tile_visible_to_player(game, target, player).ok() == Some(true) {
        let source = visible_endpoint(game, policy, unit, player);
        if let Some(serial) = crate::native_camera::request_combat(game, target, source) {
            let arrived = crate::camera_arrival::wait(serial);
            crate::native_camera::finish_combat(serial);
            log::debug!("bombard camera {serial}: {}", if arrived { "accepted camera frame with complete area" } else { "viewer unavailable, simulation continues" });
        }
        if source.is_some() { crate::combat_speed::visible_combat_ready(game, unit, 0); }
    } else if let Some(source) = unit_tile(unit) {
        // Shared vision grants the unit's position, not an unseen bombard
        // destination. With hit focus disabled, follow only the shooter.
        focus_move_unit(game, policy, unit, source);
    }
}

/// Install all semantic hooks as one transaction. Callers must `forget` the
/// returned handles together; a partial install is not safe because movement,
/// bombard and combat visibility must share the same visibility gates.
/// Called on the game's thread after each `Unit::move` while the native
/// camera is parked, with the unit (set once: a walk's end,
/// `civ3_bridge::unit_slides`).
pub static AFTER_MOVE: std::sync::OnceLock<Box<dyn Fn(u32) + Send + Sync>> = std::sync::OnceLock::new();

pub fn install(game: Civ3) -> Result<(unit_move::Installed, unit_bombard::Installed, combat_visibility::Installed), inproc::hook::HookError> {
    let Some(s) = sites(game.build) else { return Err(inproc::hook::HookError::Unsupported) };
    let visible = crate::unit_views::visible_to::Function::at(0x5B_B360, &[0x83, 0xEC, 0x14, 0x53, 0x8B, 0x5C, 0x24, 0x1C])?;
    let policy = Policy {
        visible,
        tile: crate::unit_views::tile_visible::Function::at(0x4F_0DE0, &[0x51, 0x53, 0x8B, 0xD9, 0x56])?,
        enemy: enemy_unit::Function::at(ENEMY_UNIT_SITE, ENEMY_UNIT_PROLOGUE)?,
        preferences: game_preferences::Function::at(GAME_PREFERENCES_SITE, GAME_PREFERENCES_PROLOGUE)?,
    };
    let movement_policy = policy;
    let movement = unit_move::install_at_start(s.unit_move.0, s.unit_move.1, move |call| {
        let _camera = crate::camera_arrival::combat_scope();
        let _speed = crate::combat_speed::combat_scope();
        if !crate::native_camera::active() { return call.original(); }
        let target = (call.tile_x as i32, call.tile_y as i32);
        let no_op = unit_tile(call.unit) == Some(target);
        if !no_op { focus_move_unit(game, movement_policy, call.unit, target); }
        call.original();
        if let Some(after) = AFTER_MOVE.get() { after(call.unit); }
    })?;
    let bombard_policy = policy;
    let bombard = match unit_bombard::install_at_start(s.unit_bombard.0, s.unit_bombard.1, move |call| {
        let _camera = crate::camera_arrival::combat_scope();
        let _speed = crate::combat_speed::combat_scope();
        if crate::native_camera::active() { focus_bombard(game, bombard_policy, call.unit, (call.tile_x as i32, call.tile_y as i32)); }
        call.original();
    }) {
        Ok(h) => h,
        Err(e) => { drop(movement); return Err(e); }
    };
    let combat_policy = policy;
    let combat = match combat_visibility::install_at_start(s.combat_visibility.0, s.combat_visibility.1, move |call| {
        let result = call.original();
        if (result & 0xff) == 0 { return result; }
        if crate::native_camera::active() && crate::camera_arrival::first_visible_event(call.attacker, call.defender) {
            if let Ok(player) = player_civ(game) {
                // Keep native animation legality, detection and tile visibility.
                // Include both legal endpoints so a fight straddling an edge
                // cannot begin with one of its units still clipped.
                let defender = visible_endpoint(game, combat_policy, call.defender, player);
                let attacker = visible_endpoint(game, combat_policy, call.attacker, player);
                if let Some(first) = defender.or(attacker) {
                    let other = defender.and(attacker);
                    if let Some(serial) = crate::native_camera::request_combat(game, first, other) {
                        let arrived = crate::camera_arrival::wait(serial);
                        crate::native_camera::finish_combat(serial);
                        log::debug!("combat camera {serial}: {}", if arrived { "accepted camera frame with complete area" } else { "viewer unavailable, simulation continues" });
                    }
                }
            }
        }
        crate::combat_speed::visible_combat_ready(game, call.attacker, call.defender);
        result
    }) {
        Ok(h) => h,
        Err(e) => { drop(movement); drop(bombard); return Err(e); }
    };
    Ok((movement, bombard, combat))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn friend_and_enemy_move_preferences_are_classified_separately() {
        assert_eq!(choose_move_focus(2, 1, P_SHOW_FRIEND_MOVES, false, true, true, false, false), Some(FocusEndpoint::Target));
        assert_eq!(choose_move_focus(2, 1, P_SHOW_FRIEND_MOVES, true, true, true, true, true), None);
        assert_eq!(choose_move_focus(2, 1, P_SHOW_ENEMY_MOVES, true, true, true, false, false), Some(FocusEndpoint::Target));
    }
    #[test]
    fn movement_never_focuses_fogged_target_and_falls_back_to_visible_source() {
        assert_eq!(choose_move_focus(2, 1, P_SHOW_ENEMY_MOVES, true, false, true, true, true), Some(FocusEndpoint::Source));
        assert_eq!(choose_move_focus(2, 1, P_SHOW_ENEMY_MOVES, true, false, false, false, true), None);
        assert_eq!(choose_move_focus(2, 1, P_SHOW_ENEMY_MOVES, true, true, false, true, false), None);
    }
    #[test]
    fn own_unit_does_not_become_an_ai_focus_event() {
        assert_eq!(choose_move_focus(1, 1, P_SHOW_ENEMY_MOVES | P_SHOW_FRIEND_MOVES, true, true, true, true, true), None);
    }
    #[test]
    fn combat_focus_does_not_depend_on_move_preferences() {
        assert!(should_focus_combat_endpoint(true, true));
        assert!(!should_focus_combat_endpoint(false, true));
        assert!(!should_focus_combat_endpoint(true, false));
    }
}
