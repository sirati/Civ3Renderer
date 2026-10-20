//! Combat-owned FLC clocks only. Simulation and host/audio clocks are untouched.
use std::cell::Cell;
use std::sync::atomic::{AtomicU8, Ordering};
use inproc::Ptr32;
use crate::{Build, Civ3, layout as L};

const TICK_SLOT: u32 = 0x665448;
const TICK: u32 = 0x402F70;
const TICK_PROLOGUE: &[u8] = &[0x56, 0x8B, 0xF1, 0x8B, 0x46, 0x24];
const FIGHT: u32 = 0x4A5430;
const FIGHT_PROLOGUE: &[u8] = &[0x6A, 0xFF, 0x68, 0x7B, 0x95, 0x65, 0x00];
const UNIT_ANIMATION: u32 = 0x27C;
const ANIMATION_UNIT: u32 = 0x134;
static QUARTERS: AtomicU8 = AtomicU8::new(4);

inproc::fastcall_hook! {
    /// FLC vtable +4: uint32 thiscall FLC_Animation::update(float seconds).
    /// `seconds` carries the original float stack bits, never an integer clock.
    pub mod tick(animation, edx; seconds) -> u32;
}
inproc::fastcall_hook! {
    /// C3X's Fighter_fight ABI, preserving its redirected implementation.
    pub mod fight(fighter, edx; attacker, direction, defender) -> u32;
}

// Only these native combat callers may shorten Animator's real-time pump.
// Every call still pumps the original Animator; neither QPC nor audio is scaled.
macro_rules! combat_wait {
    ($name:ident, $site:expr, $bytes:expr, $duration:expr) => {
        inproc::fastcall_hook! { pub mod $name(animator, edx; milliseconds, process_input) -> (); }
        impl $name::Installed {
            fn install_combat() -> Result<Self, inproc::hook::HookError> {
                $name::install_call_site($site, &$bytes, |call| {
                    let scope = COMBAT.get();
                    let delay = scaled_wait(call.milliseconds, $duration, QUARTERS.load(Ordering::Relaxed),
                        scope.active && scope.count > 0 && call.animator == 0xA26878);
                    call.original_with(call.animator, delay, call.process_input)
                })
            }
        }
    };
}
combat_wait!(army_wait100, 0x4A5A73, [0xE8, 0x58, 0xA6, 0x04, 0x00], 100);
combat_wait!(army_wait200, 0x4A5B33, [0xE8, 0x98, 0xA5, 0x04, 0x00], 200);
combat_wait!(round_wait_a, 0x4A5C2A, [0xE8, 0xA1, 0xA4, 0x04, 0x00], 650);
combat_wait!(round_wait_b, 0x4A677F, [0xE8, 0x4C, 0x99, 0x04, 0x00], 650);
combat_wait!(round_wait_c, 0x4A7244, [0xE8, 0x87, 0x8E, 0x04, 0x00], 650);
combat_wait!(start_wait, 0x4A8398, [0xE8, 0x33, 0x7D, 0x04, 0x00], 650);

pub struct Installed {
    _tick: tick::Installed,
    _fight: fight::Installed,
    _army100: army_wait100::Installed,
    _army200: army_wait200::Installed,
    _round_a: round_wait_a::Installed,
    _round_b: round_wait_b::Installed,
    _round_c: round_wait_c::Installed,
    _start: start_wait::Installed,
}

fn scaled_wait(milliseconds: u32, expected: u32, quarters: u8, armed: bool) -> u32 {
    if !armed || quarters == 4 || milliseconds != expected { return milliseconds; }
    // Round upward to keep a nonzero pump budget, with at most 1 ms error.
    (milliseconds * 4).div_ceil(u32::from(quarters.clamp(4, 16))).max(1)
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
struct Owner { unit: u32, id: i32 }
#[derive(Copy, Clone, Default)]
struct Scope { active: bool, game: Option<Civ3>, owners: [Owner; 8], count: u8 }
thread_local! { static COMBAT: Cell<Scope> = Cell::new(Scope::default()); }

/// Game-thread guard. Nested combat restores the exact enclosing ownership.
pub struct CombatScope { previous: Scope, _thread: std::marker::PhantomData<std::rc::Rc<()>> }
impl Drop for CombatScope { fn drop(&mut self) { COMBAT.set(self.previous); } }
pub fn combat_scope() -> CombatScope {
    CombatScope { previous: COMBAT.replace(Scope { active: true, ..Default::default() }), _thread: Default::default() }
}

/// Set only from committed preferences. Four quarter steps are normal time.
pub fn set_speed_quarters(speed: u8) { QUARTERS.store(speed.clamp(4, 16), Ordering::Relaxed); }

fn add(scope: &mut Scope, owner: Owner) {
    if owner.unit == 0 || scope.owners[..scope.count as usize].contains(&owner) { return; }
    if let Some(slot) = scope.owners.get_mut(scope.count as usize) { *slot = owner; scope.count += 1; }
}
fn owner(game: Civ3, unit: u32) -> Option<crate::DrawnUnit> {
    if unit == 0 { return None; }
    let record = game.drawn_unit_at(unit).ok()?;
    (record.id >= 0 && game.unit_ptr(record.id).ok()?? == unit).then_some(record)
}
fn register(game: Civ3, scope: &mut Scope, unit: u32) {
    let Some(unit) = owner(game, unit) else { return };
    add(scope, Owner { unit: unit.native_address, id: unit.id });

}

/// Called by the visibility hook only after the camera wait has completed.
/// Accepted native combat pairs, including a resolved defender, own the clock.
pub fn visible_combat_ready(game: Civ3, attacker: u32, defender: u32) {
    COMBAT.with(|cell| {
        let mut scope = cell.get();
        if !scope.active { return; }
        scope.game = Some(game);
        register(game, &mut scope, attacker); register(game, &mut scope, defender);
        cell.set(scope);
    });
}

fn owns(scope: Scope, animation: u32, unit: u32, id: i32) -> bool {
    scope.active && scope.owners[..scope.count as usize].iter().any(|owner|
        owner.unit == unit && owner.id == id && owner.unit.checked_add(UNIT_ANIMATION) == Some(animation))
}

fn displayed_member(parent: (i32, i32, i32), member: (i32, i32, i32)) -> bool {
    let (parent_id, parent_civ, displayed_id) = parent;
    let (member_id, member_civ, container) = member;
    displayed_id >= 0 && displayed_id == member_id && member_civ == parent_civ && container == parent_id
}

fn scaled_seconds(bits: u32, quarters: u8, owned: bool, animation: i32) -> u32 {
    if !owned || quarters == 4 || !matches!(animation, 3..=6 | 9) { return bits; }
    let seconds = f32::from_bits(bits);
    if !seconds.is_finite() || seconds <= 0.0 { return bits; }
    let scaled = seconds * (f32::from(quarters.clamp(4, 16)) / 4.0);
    if scaled.is_finite() { scaled.to_bits() } else { bits }
}
fn seconds(animation: u32, bits: u32) -> u32 {
    let speed = QUARTERS.load(Ordering::Relaxed);
    if speed == 4 { return bits; }
    let scope = COMBAT.get();
    if !scope.active || scope.count == 0 { return bits; }
    let p = Ptr32::<u8>::new(animation);
    let kind = p.offset::<i32>(0x24).and_then(|p| p.read()).unwrap_or(-1);
    if !matches!(kind, 3..=6 | 9) { return bits; }
    let Some(unit) = p.offset::<u32>(ANIMATION_UNIT).and_then(|p| p.read()).ok() else { return bits };
    if unit.checked_add(UNIT_ANIMATION) != Some(animation) { return bits; }
    let Some(game) = scope.game else { return bits };
    let Some(candidate) = owner(game, unit) else { return bits };
    let valid = owns(scope, animation, unit, candidate.id) || scope.owners[..scope.count as usize].iter().any(|primary| {
        let Some(parent) = owner(game, primary.unit) else { return false };
        if parent.id != primary.id || !displayed_member(
            (parent.id, parent.civ, parent.army_member), (candidate.id, candidate.civ, candidate.container)) { return false; }
        let bic = Ptr32::<u8>::new(game.addrs.bic_data);
        let count = bic.offset::<i32>(L::BIC_UNITTYPECOUNT).and_then(|p| p.read()).unwrap_or(0);
        if parent.unit_type < 0 || parent.unit_type >= count { return false; }
        bic.offset::<Ptr32<u8>>(L::BIC_UNITTYPES).and_then(|p| p.read())
            .and_then(|p| p.offset::<u32>(parent.unit_type as u32 * L::UNITTYPE_SIZE + L::UNITTYPE_UNITABILITIES))
            .and_then(|p| p.read()).is_ok_and(|abilities| abilities & (1 << 18) != 0)
    });
    if !valid { return bits; }
    scaled_seconds(bits, speed, true, kind)
}

/// Install as a transaction. Known C3X entry jumps remain in the call chain.
pub fn install(game: Civ3) -> Result<Installed, inproc::hook::HookError> {
    use inproc::hook::HookError;
    if game.build != Build::PcGames { return Err(HookError::Unsupported); }
    let target = Ptr32::<u32>::new(TICK_SLOT).read()?;
    if target != TICK { return Err(HookError::Unsupported); }
    let bytes = Ptr32::<u8>::new(TICK).read_array(TICK_PROLOGUE.len())?;
    if bytes != TICK_PROLOGUE && bytes.first() != Some(&0xE9) { return Err(HookError::Unsupported); }
    let tick = tick::install_slot(Ptr32::<u32>::new(TICK_SLOT), |call| {
        call.original_with(call.animation, seconds(call.animation, call.seconds))
    })?;
    let fight = fight::install_at_start(FIGHT, FIGHT_PROLOGUE, |call| {
        let _combat = combat_scope();
        let _camera = crate::camera_arrival::combat_scope();
        call.original()
    })?;
    Ok(Installed {
        _tick: tick, _fight: fight,
        _army100: army_wait100::Installed::install_combat()?,
        _army200: army_wait200::Installed::install_combat()?,
        _round_a: round_wait_a::Installed::install_combat()?,
        _round_b: round_wait_b::Installed::install_combat()?,
        _round_c: round_wait_c::Installed::install_combat()?,
        _start: start_wait::Installed::install_combat()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_owned_combat_poses_change_elapsed_and_one_x_is_bit_exact() {
        for kind in 0..=18 { for owned in [false, true] { for q in 4..=16 {
            let elapsed = 0.0125f32;
            let want = if owned && matches!(kind, 3..=6 | 9) { elapsed * (f32::from(q) / 4.0) } else { elapsed };
            assert_eq!(scaled_seconds(elapsed.to_bits(), q, owned, kind), want.to_bits(), "kind={kind} owner={owned} speed={q}/4");
        }}}
        for bits in [0, (-0.0f32).to_bits(), f32::NAN.to_bits(), f32::INFINITY.to_bits(), (-1.0f32).to_bits(), f32::MAX.to_bits()] {
            assert_eq!(scaled_seconds(bits, 4, true, 3), bits);
            if !f32::from_bits(bits).is_finite() || f32::from_bits(bits) <= 0.0 || bits == f32::MAX.to_bits() { assert_eq!(scaled_seconds(bits, 16, true, 3), bits); }
        }
    }
    #[test]
    fn preview_unrelated_and_recycled_units_never_own_the_visual_clock() {
        let mut scope = Scope { active: true, ..Default::default() };
        let owner = Owner { unit: 0x1000, id: 19 };
        assert!(!owns(scope, 0x127C, 0x1000, 19), "camera has not armed an owner");
        add(&mut scope, owner);
        assert!(owns(scope, 0x127C, 0x1000, 19));
        assert!(!owns(scope, 0x2000, 0x1000, 19), "preview copy is not the unit's FLC");
        assert!(!owns(scope, 0x127C, 0x1000, 20), "recycled native object identity");
        assert!(!owns(scope, 0x327C, 0x3000, 19), "unrelated animation during combat");
        scope.active = false; assert!(!owns(scope, 0x127C, 0x1000, 19));
    }

    #[test]
    fn nested_fights_restore_ownership_and_unarmed_scopes_do_not_advance() {
        let _outer = combat_scope();
        assert_eq!(seconds(100, 0.1f32.to_bits()), 0.1f32.to_bits());
        COMBAT.with(|s| { let mut scope = s.get(); add(&mut scope, Owner { unit: 10, id: 1 }); s.set(scope); });
        { let _inner = combat_scope(); assert_eq!(COMBAT.get().count, 0); }
        assert_eq!(COMBAT.get().owners[0], Owner { unit: 10, id: 1 });
    }
    #[test]
    fn army_member_rotation_rechecks_the_current_member_and_container() {
        assert!(displayed_member((10, 2, 19), (19, 2, 10)));
        assert!(!displayed_member((10, 2, 20), (19, 2, 10)), "former member after rotation");
        assert!(!displayed_member((10, 2, 19), (19, 3, 10)), "different civilization");
        assert!(!displayed_member((10, 2, 19), (19, 2, 11)), "different army container");
        assert!(!displayed_member((10, 2, -1), (-1, 2, 10)), "no displayed member");
    }
    #[test]
    fn combat_pump_and_flc_budget_scale_together_without_touching_other_waits() {
        for q in 4..=16 { for delay in [100, 200, 650] {
            let shortened = scaled_wait(delay, delay, q, true);
            let host_seconds = shortened as f32 / 1000.0;
            let visual_seconds = f32::from_bits(scaled_seconds(host_seconds.to_bits(), q, true, 3));
            assert!((visual_seconds - delay as f32 / 1000.0).abs() <= 0.0041);
            assert_eq!(scaled_wait(delay, delay, q, false), delay, "unarmed camera or noncombat scope");
            assert_eq!(scaled_wait(delay + 1, delay, q, true), delay + 1, "unrecognized request");
            assert_eq!(scaled_wait(delay, delay, 4, true), delay, "1x is exact");
            if q > 4 { assert!(shortened < delay); }
        }}
        assert_eq!(scaled_wait(650, 650, 16, true), 163);
        assert_eq!(scaled_wait(100, 100, 16, true), 25);
        assert_eq!(scaled_wait(200, 200, 16, true), 50);
    }
    #[test]
    fn visual_duration_scales_without_retiming_noncombat_ticks() {
        let frames = 24; let frame_time = 0.05f32; let host_step = 0.001f32;
        for q in [4, 5, 8, 12, 16] {
            let mut elapsed = 0.0; let mut steps = 0;
            while elapsed + 0.000001 < frames as f32 * frame_time { elapsed += f32::from_bits(scaled_seconds(host_step.to_bits(), q, true, 3)); steps += 1; }
            let expected = frames as f32 * frame_time / (f32::from(q) / 4.0);
            assert!((steps as f32 * host_step - expected).abs() < 0.002, "q={q} steps={steps}");
            assert_eq!(scaled_seconds(host_step.to_bits(), q, true, 2), host_step.to_bits());
        }
    }
}
