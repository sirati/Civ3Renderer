//! PCGames/v1.22 native Scroll_Bar metadata and change notifications.
//!
//! The constructor at 0x60CAE0 writes vtable 0x6660A8. Range/value
//! offsets are read by 0x60D270 and drawing at 0x60DFC0. The latter's
//! knob-centre travel is reproduced here, including arrow and border gaps.
//! Calls in this module must run on the game's window thread.

use crate::controls::Mem;
use inproc::Ptr32;

pub const VTABLE: u32 = 0x66_60A8;
pub const VALUE: u32 = 0x58C;
const NOTIFY: u32 = 0x60_DEB0;
const NOTIFY_PROLOGUE: &[u8] = &[0x56, 0x8B, 0xF1, 0x8B, 0x86, 0x30, 0x01, 0x00, 0x00];
pub const SCALE: i32 = 1_000_000;

/// Identity fields that remain fixed while the same control is alive.
/// Canvas and parent identity prevent a queued preview reaching a reused
/// form address after a menu is replaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub address: u32,
    parent: u32,
    parent_vtable: u32,
    canvas: u32,
    callback_id: u32,
    minimum: i32,
    maximum: i32,
    reversed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slider {
    pub identity: Identity,
    /// The exact integer stored by the game. Do not round-trip it through
    /// the wire's normalized value when rolling an edit back.
    pub exact: i32,
    /// Knob-centre travel in the form's local pixels, x/y/width/height.
    pub track: [i32; 4],
}

impl Slider {
    pub fn read(mem: &dyn Mem, address: u32) -> Option<Self> {
        if mem.u32(address)? != VTABLE { return None; }
        let minimum = mem.i32(address + 0x580)?;
        let maximum = mem.i32(address + 0x584)?;
        let exact = mem.i32(address + VALUE)?;
        if maximum <= minimum || !(minimum..=maximum).contains(&exact) { return None; }
        let parent = mem.u32(address + 0x130)?;
        if parent == 0 { return None; }
        let parent_vtable = mem.u32(parent)?;
        let canvas = mem.u32(address + 0x278)?;
        if canvas == 0 { return None; }
        let w = mem.i32(address + 0x1B4)?.checked_sub(mem.i32(address + 0x1AC)?)?;
        let h = mem.i32(address + 0x1B8)?.checked_sub(mem.i32(address + 0x1B0)?)?;
        if !(1..=16384).contains(&w) || !(1..=16384).contains(&h) { return None; }
        let horizontal = w > h;
        let first = if horizontal { 0x5AC } else { 0x5B0 };
        let knob = mem.i32(address + first + 8)?.checked_sub(mem.i32(address + first)?)?;
        let length = if horizontal { w } else { h };
        if !(1..=length).contains(&knob) { return None; }
        let bordered = i32::from(mem.i32(address + 0x57C)? != -1);
        let arrows = mem.u32(address + 0x574)? & 2 == 0;
        let inset = if arrows { knob + 1 } else { 0 };
        let travel = length - 1 - knob * if arrows { 3 } else { 1 } - 2 * bordered;
        if travel <= 0 { return None; }
        let start = inset + bordered + knob / 2;
        let cross = if horizontal { h / 2 } else { w / 2 };
        let track = if horizontal { [start, cross, travel + 1, 1] } else { [cross, start, 1, travel + 1] };
        Some(Self {
            identity: Identity { address, parent, parent_vtable, canvas, callback_id: mem.u32(address + 0x578)?, minimum, maximum, reversed: mem.u32(address + 0x588)? != 0 },
            exact,
            track,
        })
    }

    /// Position increases left-to-right or top-to-bottom. Native reversal
    /// changes the parent notification, not the drawn knob position.
    pub fn normalized(self) -> i32 {
        let min = i64::from(self.identity.minimum);
        let span = i64::from(self.identity.maximum) - min;
        (((i64::from(self.exact) - min) * i64::from(SCALE) + span / 2) / span) as i32
    }

    pub fn exact_at(self, normalized: i32) -> i32 {
        let min = i64::from(self.identity.minimum);
        let span = i64::from(self.identity.maximum) - min;
        (min + (span * i64::from(normalized.clamp(0, SCALE)) + i64::from(SCALE) / 2) / i64::from(SCALE)) as i32
    }

    /// The exact value `steps` of the game's own arrow-button step (one
    /// stored unit; 0x60DEB0 commands 1 and 2) away, clamped to its range.
    pub fn stepped(self, steps: i32) -> i32 {
        self.exact.saturating_add(steps).clamp(self.identity.minimum, self.identity.maximum)
    }

    pub fn same_control(self, other: Self) -> bool { self.identity == other.identity }
}

inproc::thiscall_fn! {
    /// Scroll_Bar's arrow-button notification. Command zero skips both
    /// +/-1 branches, then performs the native parent m57, optional C
    /// callback, redraw and invalidation. It holds no mouse button.
    fn notify(slider; command) -> ();
}

pub fn validate() -> Result<(), inproc::hook::HookError> {
    notify::Function::at(NOTIFY, NOTIFY_PROLOGUE).map(|_| ())
}

/// Apply an exact native position and notify its owner through the same
/// path as an ordinary arrow click. Re-read identity on the game thread.
pub fn set_exact(expected: Slider, exact: i32) -> bool {
    let Some(current) = Slider::read(&crate::controls::Live, expected.identity.address) else { return false };
    if !current.same_control(expected) || !(current.identity.minimum..=current.identity.maximum).contains(&exact) { return false; }
    if current.exact == exact { return true; }
    let Ok(notify) = notify::Function::at(NOTIFY, NOTIFY_PROLOGUE) else { return false };
    if Ptr32::<i32>::new(current.identity.address + VALUE).write(&exact).is_err() { return false; }
    notify.call(current.identity.address, 0);
    true
}
