//! The mouse wheel where the game never took it (docs §16.11, "Wheel").
//!
//! The game gives a wheel message to the form under the pointer only
//! (0x62C6D0: hit test 0x609030, then that form's `vt+0x64(delta, x, y)`
//! in its own pixels), never to its parents. `Base_Form`'s slot is
//! `ret 0xC` (0x411680), and so is every control's that does not override
//! it: buttons, scroll bars, check boxes, text boxes, the Civilopedia, the
//! Foreign advisor (F4), the Hall of Fame, the load/save file dialog,
//! diplomacy, popups. A list box's own (0x618E40) acts only while its form
//! has a vertical bar (Status1 & 8). The screens that override the slot
//! scroll their own lists by region: the city screen (0x425840), F1
//! Domestic (0x5228D0), F3 Military (0x575F10), F5 Cultural (0x4FBD10),
//! F7 Wonders (0x5D1490), F8 Histograph (0x543330), the Trade advisor
//! (0x5AFA50), Espionage (0x52B2D0), the multiplayer lobby (0x482B10) and
//! game setup screens (0x4932A0, 0x4994A0), and the map (0x4DE460).
//!
//! [`route`] finds, for a wheel the game would drop, the scrolling the
//! pointer is over, and how the game itself would scroll it a step:
//! - a vertical `Scroll_Bar` (vtable 0x6660A8) under the pointer, a form's
//!   own bar (`Base_Form`+0x26C, made with Status1 & 8 at 0x60426C; a
//!   `TextBox`'s at +0x574), or a free bar of a screen beside the pointer:
//!   its arrow buttons' command (0x60DEB0: 1 up, 2 down), as a click on
//!   them;
//! - a parent screen that takes the wheel itself (the pointer on one of its
//!   buttons or pictures): its own handler, at the pointer;
//! - diplomacy's trade trees (`DiploForm`, vtable 0x66C0D8), whose arrows
//!   are painted regions (+0x13E0, ids 3/4 the right tree's, 5/6 the
//!   left's): a click on the arrow (vt+0x74, 0x5076A0).
//!
//! Nothing over the map or its interface panel: the walk up the parents
//! stops at the main screen form, which keeps its own wheel.

use super::{Form, Mem, Sites, list_parts, typed_draw, visible, CHILDREN, CHILD_COUNT, MAIN_GUI_IN_SCREEN, MAX_CHILDREN, MAX_DEPTH, PARENT, WHEEL};

/// `Base_Form`'s wheel slot (vt+0x64): `ret 0xC`.
pub const NO_WHEEL: u32 = 0x41_1680;
pub const WHEEL_SLOT: u32 = 0x64;
/// A form's click (vt+0x74, `(x, y)` in its own pixels).
pub const CLICK_SLOT: u32 = 0x74;
/// `Scroll_Bar`: its vtable, range and position.
pub const SCROLL_BAR: u32 = crate::sliders::VTABLE;
const BAR_MIN: u32 = 0x580;
const BAR_MAX: u32 = 0x584;
const BAR_VALUE: u32 = crate::sliders::VALUE;
/// A form's own vertical bar (made by 0x603D10 with Status1 & 8).
const OWN_BAR: u32 = 0x26C;
/// `TextBox` (0x6222C0) and its bar.
const TEXT_BOX: u32 = 0x66_7188;
const TEXT_BOX_BAR: u32 = 0x574;
/// `DiploForm`: its painted regions (`entries` +0x50 of 32 bytes, a RECT at
/// +4 and the id at +0x18; count +0x58; found by 0x60F170), the ids of the
/// trade tables' arrows (their up, down; ours), the x the click handler
/// leaves to its talk lines (0x5076E0: 0x130..=0x2D8), the trade trees'
/// rows (0x518D4F; `controls::screens`), shown in mode 2 on page 1.
pub const DIPLOMACY: u32 = super::screens::DIPLOMACY;
const DIPLO_REGIONS: u32 = 0x13E0;
const DIPLO_MODE: u32 = 0xED8;
const DIPLO_PAGE: u32 = 0xEEC;
/// Right tree (scroll +0xEC4), left tree (+0xEC8).
const DIPLO_RIGHT_ARROWS: (u32, u32) = (3, 4);
const DIPLO_LEFT_ARROWS: (u32, u32) = (5, 6);
const DIPLO_TALK: (i32, i32) = (0x130, 0x2D8);
const DIPLO_TREES: (i32, i32) = (0xE4, 0x266);

/// One arrow step a notch: the game's own lists move one entry a notch.
pub const STEPS_PER_NOTCH: u32 = 1;
/// At most this many notches act from one message.
const MAX_NOTCHES: i32 = 32;

/// How a form takes the wheel: the inventory of the game's scrolling.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Scrolling {
    /// Its own wheel handler acts (a list box with a bar, the screens
    /// above, the map).
    Own,
    /// A scroll bar: vertical (taller than wide, as 0x60CDE0 lays it out),
    /// and whether it has anywhere to go.
    Bar { vertical: bool, scrollable: bool },
    /// Diplomacy's painted arrows.
    PaintedArrows,
    /// It drops the wheel; `bars` vertical bars that can scroll are its
    /// children (its own bar among them, if `own_bar`).
    Drops { bars: usize, own_bar: bool },
}

/// What a wheel message at a point does.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// The game's own (what it does with it, or nothing).
    Native,
    /// `form`'s own wheel handler, with the message's delta, at (x, y) in
    /// its pixels.
    Forward { form: u32, x: i32, y: i32 },
    /// Scroll bar `bar`'s arrow command (1 up, 2 down), `steps` times.
    Arrow { bar: u32, command: u32, steps: u32 },
    /// A click on `form` at (x, y) in its pixels, `times` times.
    Click { form: u32, x: i32, y: i32, times: u32 },
}

fn slot(mem: &dyn Mem, form: &Form) -> u32 {
    mem.u32(form.vtable + WHEEL_SLOT).unwrap_or(NO_WHEEL)
}

/// Whether the game's wheel message to `form` does anything.
fn takes_wheel(mem: &dyn Mem, form: &Form) -> bool {
    if slot(mem, form) == NO_WHEEL {
        return false;
    }
    if list_parts(mem, form).is_some() {
        return form.status1 & WHEEL != 0;
    }
    // A typed control other than a list box: its slot returns (0x619AF0).
    typed_draw(mem, form).is_none()
}

/// A scroll bar's shape and range: (vertical, scrollable).
fn bar_shape(mem: &dyn Mem, bar: &Form) -> Option<(bool, bool)> {
    if bar.vtable != SCROLL_BAR {
        return None;
    }
    let (min, max, value) = (mem.i32(bar.addr + BAR_MIN)?, mem.i32(bar.addr + BAR_MAX)?, mem.i32(bar.addr + BAR_VALUE)?);
    Some((bar.size.0 <= bar.size.1, max > min && (min..=max).contains(&value)))
}

fn children(mem: &dyn Mem, form: u32) -> Vec<u32> {
    let n = mem.u32(form + CHILD_COUNT).unwrap_or(0).min(MAX_CHILDREN);
    let array = mem.u32(form + CHILDREN).unwrap_or(0);
    if array == 0 {
        return vec![];
    }
    (0..n).filter_map(|i| mem.u32(array + 4 * i)).filter(|&c| c != 0 && c != form).collect()
}

/// The shown vertical bars among `form`'s children that can scroll.
fn vertical_bars(mem: &dyn Mem, form: u32) -> Vec<Form> {
    children(mem, form)
        .into_iter()
        .filter(|&c| visible(mem, c))
        .filter_map(|c| Form::read(mem, c))
        .filter(|b| !super::empty(b.shown) && bar_shape(mem, b) == Some((true, true)))
        .collect()
}

/// A form's own bar: `Base_Form`+0x26C, or a `TextBox`'s.
fn own_bar(mem: &dyn Mem, form: &Form) -> Option<u32> {
    let at = if form.vtable == TEXT_BOX { TEXT_BOX_BAR } else { OWN_BAR };
    let bar = if form.vtable == TEXT_BOX { form.addr + at } else { mem.u32(form.addr + at)? };
    (bar != 0).then_some(bar)
}

/// What `form` is to the wheel.
pub fn classify(mem: &dyn Mem, addr: u32) -> Option<Scrolling> {
    let form = Form::read(mem, addr)?;
    if let Some((vertical, scrollable)) = bar_shape(mem, &form) {
        return Some(Scrolling::Bar { vertical, scrollable });
    }
    if takes_wheel(mem, &form) {
        return Some(Scrolling::Own);
    }
    if form.vtable == DIPLOMACY {
        return Some(Scrolling::PaintedArrows);
    }
    let bars = vertical_bars(mem, addr);
    let own = own_bar(mem, &form);
    Some(Scrolling::Drops { bars: bars.len(), own_bar: own.is_some_and(|o| bars.iter().any(|b| b.addr == o)) })
}

/// The bar of `form` that scrolls what is at screen point `p`: its own bar
/// anywhere on it; else a free bar whose rows the point is on, the
/// nearest at or right of it, the point not beyond another bar to its
/// left.
fn bar_at(mem: &dyn Mem, form: &Form, p: (i32, i32)) -> Option<u32> {
    let bars = vertical_bars(mem, form.addr);
    if let Some(own) = own_bar(mem, form).filter(|o| bars.iter().any(|b| b.addr == *o)) {
        return super::contains(form.shown, p).then_some(own);
    }
    let rows = |b: &Form| p.1 >= b.shown[1] && p.1 < b.shown[1] + b.shown[3];
    let right = |b: &Form| b.shown[0] + b.shown[2];
    let bar = bars.iter().filter(|b| rows(b) && p.0 < right(b)).min_by_key(|b| right(b))?;
    // Not past another bar of the same rows (that one's content is there).
    let left = bars.iter().filter(|b| b.addr != bar.addr && rows(b) && right(b) <= bar.shown[0]).map(right).max().unwrap_or(form.shown[0]);
    (p.0 >= left && super::contains(form.shown, p)).then_some(bar.addr)
}

/// Diplomacy: a click on the up or down arrow of the trade table under
/// `p` (in the form's pixels), if the table has those arrows.
fn diplomacy(mem: &dyn Mem, form: &Form, p: (i32, i32), up: bool) -> Option<(i32, i32)> {
    if mem.i32(form.addr + DIPLO_MODE)? != 2 || mem.i32(form.addr + DIPLO_PAGE)? != 1 {
        return None;
    }
    let (x, y) = (p.0 - form.origin.0, p.1 - form.origin.1);
    let (up_id, down_id) = if x < DIPLO_TALK.0 {
        DIPLO_LEFT_ARROWS
    } else if x > DIPLO_TALK.1 {
        DIPLO_RIGHT_ARROWS
    } else {
        return None;
    };
    let regions = form.addr + DIPLO_REGIONS;
    let entries = mem.u32(regions + 0x50)?;
    let count = mem.i32(regions + 0x58)?.clamp(0, 256) as u32;
    let rect = |id: u32| {
        (0..count).map(|i| entries + 32 * i).find(|&e| mem.u32(e + 0x18) == Some(id)).and_then(|e| super::rect_at(mem, e + 4))
    };
    let (Some(top), Some(bottom)) = (rect(up_id), rect(down_id)) else { return None };
    // The tree's rows (its arrows among them).
    if y < DIPLO_TREES.0 || y > DIPLO_TREES.1.max(top[3]).max(bottom[3]) || [top, bottom].iter().any(|r| r[2] <= r[0] || r[3] <= r[1]) {
        return None;
    }
    let [l, t, r, b] = if up { top } else { bottom };
    Some(((l + r) / 2, (t + b) / 2))
}

/// What a wheel message of `delta` (120 a notch, positive away from the
/// user: up) at screen point `p` does, `hit` the form the game's hit test
/// gives it to.
pub fn route(mem: &dyn Mem, sites: &Sites, hit: u32, p: (i32, i32), delta: i32) -> Action {
    let notches = (delta / 120).clamp(-MAX_NOTCHES, MAX_NOTCHES);
    let notches = if notches == 0 { delta.signum() } else { notches };
    if notches == 0 || hit == 0 {
        return Action::Native;
    }
    let up = notches > 0;
    let main_screen = sites.main_gui.wrapping_sub(MAIN_GUI_IN_SCREEN);
    let mut at = hit;
    for _ in 0..=MAX_DEPTH {
        let Some(form) = Form::read(mem, at) else { return Action::Native };
        if let Some((vertical, scrollable)) = bar_shape(mem, &form) {
            if !vertical || !scrollable {
                return Action::Native;
            }
            return Action::Arrow { bar: at, command: if up { 1 } else { 2 }, steps: notches.unsigned_abs() * STEPS_PER_NOTCH };
        }
        if takes_wheel(mem, &form) {
            // (The hit form's own handler gets the message anyway; the map
            // keeps its wheel to itself.)
            if at == hit || at == main_screen {
                return Action::Native;
            }
            return Action::Forward { form: at, x: p.0 - form.origin.0, y: p.1 - form.origin.1 };
        }
        if at == main_screen {
            return Action::Native;
        }
        if form.vtable == DIPLOMACY {
            return match diplomacy(mem, &form, p, up) {
                Some((x, y)) => Action::Click { form: at, x, y, times: notches.unsigned_abs() * STEPS_PER_NOTCH },
                None => Action::Native,
            };
        }
        if let Some(bar) = bar_at(mem, &form, p) {
            return Action::Arrow { bar, command: if up { 1 } else { 2 }, steps: notches.unsigned_abs() * STEPS_PER_NOTCH };
        }
        match mem.u32(at + PARENT) {
            Some(parent) if parent != 0 && parent != at => at = parent,
            _ => return Action::Native,
        }
    }
    Action::Native
}

/// The game's hit test and a wheel step done the way the game does them.
/// Call only on the game's window thread, from its message handling.
pub mod live {
    use super::{Action, BAR_MAX, BAR_MIN, BAR_VALUE, CLICK_SLOT, SCROLL_BAR, WHEEL_SLOT};
    use crate::controls::Mem;

    const HIT_TEST: u32 = 0x60_9030;
    /// push ebx; push ebp; push esi; mov esi,[esp+0x10]
    const HIT_TEST_PROLOGUE: &[u8] = &[0x53, 0x55, 0x56, 0x8B, 0x74, 0x24, 0x10];
    const ARROW: u32 = 0x60_DEB0;
    const ARROW_PROLOGUE: &[u8] = &[0x56, 0x8B, 0xF1, 0x8B, 0x86, 0x30, 0x01, 0x00, 0x00];

    inproc::cdecl_fn! {
        /// `Base_Form *hit_test(int *x, int *y)` (0x609030): screen
        /// pixels in, the form's own out (docs §16.11).
        fn hit_test(x, y) -> u32;
    }
    inproc::thiscall_fn! {
        /// `Scroll_Bar`'s arrow notification (0x60DEB0).
        fn arrow(bar; command) -> ();
    }
    inproc::thiscall_fn! {
        /// A form's wheel slot (vt+0x64).
        fn wheel(form; delta, x, y) -> ();
    }
    inproc::thiscall_fn! {
        /// A form's click slot (vt+0x74).
        fn click(form; x, y) -> ();
    }

    pub fn validate() -> Result<(), inproc::hook::HookError> {
        hit_test::Function::at(HIT_TEST, HIT_TEST_PROLOGUE)?;
        arrow::Function::at(ARROW, ARROW_PROLOGUE)?;
        Ok(())
    }

    /// The form the game would give a mouse message at client (x, y).
    pub fn hit(x: i32, y: i32) -> Option<u32> {
        let f = hit_test::Function::at(HIT_TEST, HIT_TEST_PROLOGUE).ok()?;
        let (mut x, mut y) = (x, y);
        let form = f.call(std::ptr::addr_of_mut!(x) as usize as u32, std::ptr::addr_of_mut!(y) as usize as u32);
        (form != 0).then_some(form)
    }

    /// The method in `form`'s vtable `slot` (what the game would call
    /// there), and its first bytes (readable code).
    fn method(form: u32, slot: u32) -> Option<(u32, Vec<u8>)> {
        let mem = crate::controls::Live;
        let target = mem.u32(mem.u32(form)? + slot)?;
        Some((target, mem.read_bytes(target, 5)?))
    }

    /// Does `action` (not `Native`) for a wheel message of `delta`.
    /// Re-reads the controls it acts on first.
    pub fn perform(action: Action, delta: i32) -> bool {
        let mem = crate::controls::Live;
        match action {
            Action::Native => false,
            Action::Arrow { bar, command, steps } => {
                let Ok(f) = arrow::Function::at(ARROW, ARROW_PROLOGUE) else { return false };
                for _ in 0..steps {
                    // (At its end the bar would only notify and redraw.)
                    let (Some(SCROLL_BAR), Some(min), Some(max), Some(value)) = (mem.u32(bar), mem.i32(bar + BAR_MIN), mem.i32(bar + BAR_MAX), mem.i32(bar + BAR_VALUE)) else { break };
                    if (command == 1 && value <= min) || (command == 2 && value >= max) {
                        break;
                    }
                    f.call(bar, command);
                }
                true
            }
            Action::Forward { form, x, y } => {
                let Some((target, start)) = method(form, WHEEL_SLOT) else { return false };
                let Ok(f) = wheel::Function::at(target, &start) else { return false };
                f.call(form, delta as u32, x as u32, y as u32);
                true
            }
            Action::Click { form, x, y, times } => {
                let Some((target, start)) = method(form, CLICK_SLOT) else { return false };
                let Ok(f) = click::Function::at(target, &start) else { return false };
                for _ in 0..times {
                    f.call(form, x as u32, y as u32);
                }
                true
            }
        }
    }
}

#[cfg(test)]
mod tests;
