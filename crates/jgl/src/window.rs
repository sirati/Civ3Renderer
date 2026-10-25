//! The window group (`docs/jgl-replacement.md` §6, group 7): jgl's game
//! window, its window procedure and its message pumps, as values.
//!
//! What jgl does [V, jgl.dll disassembled, RVAs relative to its base]:
//!
//! - graphics +0x04 `create_window(controller, title, owner, icon)`
//!   (0x3B400) registers the class `"JackalClass"` (style 0x2B, procedure
//!   0x3C730, the icon `icon` of the exe or `IDI_APPLICATION`, no cursor,
//!   the black brush) and makes a hidden popup window of the screen's size
//!   (`WS_EX_APPWINDOW`, `WS_POPUP | WS_CLIPSIBLINGS | WS_CLIPCHILDREN`).
//!   The controller (the exe's 0xCACD30) is what the window procedure
//!   tells; the owner (the back buffer, 0xCACFF0) is told the screen's
//!   mode (its method 0 with width, height, 8, 1).
//! - The window procedure turns messages into calls of the controller
//!   ([`Controller`]): [`route`].
//! - The pumps +0x120..+0x150 peek the thread's queue (any window) with a
//!   filter each ([`Pump`]), translate and dispatch what they take; +0x124
//!   waits for a message when +0x128 found none; +0x12C takes the oldest of
//!   three peeks ([`oldest`]); +0x148 and +0x14C throw input away.
//!
//! Nothing here calls Windows: `jgl-ffi` does what the values say.

/// The controller's methods the window procedure calls (the exe's
/// controller class, vtable 0x67138C), by vtable offset, with the
/// arguments it passes.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Controller {
    /// +0x00 `WM_SIZE`: (width, height).
    Size(i32, i32),
    /// +0x04 `WM_PAINT`: the rectangle to paint (the paint's `rcPaint`).
    Paint,
    /// +0x08 `WM_NCHITTEST`: (x, y); its answer is the message's.
    HitTest(i32, i32),
    /// +0x0C a key: (virtual key, down).
    Key(u32, bool),
    /// +0x10 `WM_CHAR`: the character.
    Char(u32),
    /// +0x14 `WM_MOUSEMOVE`: (x, y).
    Move(i32, i32),
    /// +0x18 `WM_MOUSEWHEEL`: (delta, x, y).
    Wheel(i32, i32, i32),
    /// +0x1C left button down: (double click, x, y).
    LeftDown(bool, i32, i32),
    /// +0x20 left button up: (x, y).
    LeftUp(i32, i32),
    /// +0x24 right button down: (double click, x, y).
    RightDown(bool, i32, i32),
    /// +0x28 right button up: (x, y).
    RightUp(i32, i32),
    /// +0x2C `WM_ACTIVATE`: (state, minimized), `LOWORD`/`HIWORD` of wParam.
    Activate(u32, u32),
    /// +0x30 `WM_USER + 1`, the game's timers: the timer (wParam).
    Timer(u32),
    /// +0x34 message 0x402, taken by pump +0x128 (never dispatched).
    Message402,
}

impl Controller {
    /// The method's offset in the controller's vtable.
    pub fn slot(self) -> u16 {
        match self {
            Controller::Size(..) => 0x00,
            Controller::Paint => 0x04,
            Controller::HitTest(..) => 0x08,
            Controller::Key(..) => 0x0C,
            Controller::Char(..) => 0x10,
            Controller::Move(..) => 0x14,
            Controller::Wheel(..) => 0x18,
            Controller::LeftDown(..) => 0x1C,
            Controller::LeftUp(..) => 0x20,
            Controller::RightDown(..) => 0x24,
            Controller::RightUp(..) => 0x28,
            Controller::Activate(..) => 0x2C,
            Controller::Timer(..) => 0x30,
            Controller::Message402 => 0x34,
        }
    }

    /// The method's arguments as the stack gets them (`Paint`'s rectangle
    /// pointer is the caller's: none here).
    pub fn args(self) -> Vec<u32> {
        let b = |v: bool| v as u32;
        let i = |v: i32| v as u32;
        match self {
            Controller::Size(w, h) | Controller::HitTest(w, h) | Controller::Move(w, h) | Controller::LeftUp(w, h) | Controller::RightUp(w, h) => vec![i(w), i(h)],
            Controller::Paint | Controller::Message402 => vec![],
            Controller::Key(vk, down) => vec![vk, b(down)],
            Controller::Char(c) | Controller::Timer(c) => vec![c],
            Controller::Wheel(d, x, y) => vec![i(d), i(x), i(y)],
            Controller::LeftDown(dbl, x, y) | Controller::RightDown(dbl, x, y) => vec![b(dbl), i(x), i(y)],
            Controller::Activate(s, m) => vec![s, m],
        }
    }
}

/// What the window procedure does with a message ([`route`]).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// `DefWindowProcA`'s answer.
    Default,
    /// This answer, nothing done.
    Answer(isize),
    /// Tell the controller, answer 0.
    Tell(Controller),
    /// Tell the controller, answer what it returns (`WM_NCHITTEST`).
    Ask(Controller),
    /// Tell the controller, then answer `DefWindowProcA`'s (the wheel, the
    /// timers).
    TellThenDefault(Controller),
    /// A key ([`Controller::Key`]); then, for a key going down, the
    /// characters it queued: dropped for the number pad's keys
    /// (0x60..=0x6F), else dispatched now through graphics +0x138. Answer 0.
    Key { vk: u32, down: bool },
    /// `WM_PAINT`: `BeginPaint`; if it gave a DC, tell the controller the
    /// paint's rectangle; `EndPaint`. Answer 0.
    Paint,
    /// `WM_ACTIVATE`: invalidate the whole window; realize the palette if
    /// [`Route::Activate::realize`]; tell the controller; `DefWindowProcA`;
    /// answer 0.
    Activate { realize: bool, tell: Controller },
    /// Realize jgl's palette in the window's DC, answer this.
    Realize(isize),
    /// `WM_COPYDATA`: jgl's callback (graphics +0x154) if one was set,
    /// with (wParam, lParam), answer its value; else 0.
    CopyData,
}

/// `SC_SCREENSAVE`, `SC_MONITORPOWER`: jgl keeps the screen on.
const SCREEN_SAVER: [usize; 2] = [0xF140, 0xF170];

fn lo(v: usize) -> u32 {
    (v & 0xFFFF) as u32
}

fn hi(v: usize) -> u32 {
    ((v >> 16) & 0xFFFF) as u32
}

/// The window procedure's choice for message `msg` (0x3C730): the
/// window, wParam and lParam as it gets them.
pub fn route(window: usize, msg: u32, wparam: usize, lparam: isize) -> Route {
    let l = lparam as usize;
    let (x, y) = (lo(l) as u16 as i16 as i32, hi(l) as u16 as i16 as i32);
    match msg {
        0x05 => Route::Tell(Controller::Size(x, y)),
        0x06 => Route::Activate { realize: lo(wparam) != 0 && hi(wparam) == 0, tell: Controller::Activate(lo(wparam), hi(wparam)) },
        0x0F => Route::Paint,
        0x4A => Route::CopyData,
        0x84 => Route::Ask(Controller::HitTest(x, y)),
        0x100 | 0x104 => Route::Key { vk: wparam as u32, down: true },
        0x101 | 0x105 => Route::Key { vk: wparam as u32, down: false },
        0x102 => Route::Tell(Controller::Char(wparam as u32)),
        0x112 if SCREEN_SAVER.contains(&wparam) => Route::Answer(0),
        0x200 => Route::Tell(Controller::Move(x, y)),
        0x201 | 0x203 => Route::Tell(Controller::LeftDown(msg == 0x203, x, y)),
        0x202 => Route::Tell(Controller::LeftUp(x, y)),
        0x204 | 0x206 => Route::Tell(Controller::RightDown(msg == 0x206, x, y)),
        0x205 => Route::Tell(Controller::RightUp(x, y)),
        0x20A => Route::TellThenDefault(Controller::Wheel(hi(wparam) as u16 as i16 as i32, x, y)),
        // PBT_APMQUERYSUSPEND: BROADCAST_QUERY_DENY.
        0x218 if wparam == 0 => Route::Answer(0x424D_5144),
        0x218 if SCREEN_SAVER.contains(&wparam) => Route::Answer(0),
        0x30F => Route::Realize(1),
        0x311 if wparam != window => Route::Realize(0),
        0x311 => Route::Answer(0),
        0x401 => Route::TellThenDefault(Controller::Timer(wparam as u32)),
        _ => Route::Default,
    }
}

/// Whether a key going down drops the characters it queued (the number
/// pad's keys) rather than dispatching them.
pub fn drops_chars(vk: u32) -> bool {
    (0x60..=0x6F).contains(&vk)
}

/// The message jgl's pump +0x128 hands to the controller (+0x34) instead
/// of dispatching it.
pub const MESSAGE_402: u32 = 0x402;

/// `PeekMessageA`'s `PM_REMOVE`.
pub const PM_REMOVE: u32 = 1;

/// A message filter of the pumps: messages `first..=last`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    pub first: u32,
    pub last: u32,
}

/// The pumps: graphics +0x120..+0x150 (jgl's 0x3D430..0x3D8A0).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Pump {
    /// Take one message of `filter` (all: 0..=0), translate and dispatch
    /// it; 1 if there was one, else 0. +0x120 (`WM_USER + 1`), +0x130
    /// (paints), +0x134 (keys), +0x138 (characters), +0x13C (mouse),
    /// +0x140 (moves).
    One(Filter),
    /// +0x124: +0x128 through the vtable; if it took nothing,
    /// `WaitMessage` (its answer), else 1.
    Wait,
    /// +0x128: take any message; 0x402 goes to the controller (+0x34),
    /// the rest is translated and dispatched; 1 if there was one.
    Any,
    /// +0x12C: the oldest of the first non-input messages, timers and
    /// others ([`OLDEST_FILTERS`], [`oldest`]), translated and dispatched.
    Oldest,
    /// +0x144: one mouse message; a button going down takes its button
    /// going up with it if that is queued too. 1 if there was one.
    Click,
    /// +0x148, +0x14C: every queued message of `filter` thrown away; 0.
    Discard(Filter),
    /// +0x150: every queued `WM_USER + 1`, translated and dispatched; 0.
    Drain(Filter),
}

const fn f(first: u32, last: u32) -> Filter {
    Filter { first, last }
}

/// Mouse messages (`WM_MOUSEMOVE`..`WM_MOUSEWHEEL`).
pub const MOUSE: Filter = f(0x200, 0x20A);
/// Keyboard messages (`WM_KEYDOWN`..0x108).
pub const KEYS: Filter = f(0x100, 0x108);

/// +0x12C's three peeks, in order: all but input, timers and the rest.
pub const OLDEST_FILTERS: [Filter; 3] = [f(0x000, 0x0FF), f(0x109, 0x1FF), f(0x20B, 0xFFFF)];

impl Pump {
    /// The pump at graphics vtable offset `slot`.
    pub fn at(slot: u16) -> Option<Pump> {
        Some(match slot {
            0x120 => Pump::One(f(0x401, 0x401)),
            0x124 => Pump::Wait,
            0x128 => Pump::Any,
            0x12C => Pump::Oldest,
            0x130 => Pump::One(f(0x0F, 0x0F)),
            0x134 => Pump::One(KEYS),
            0x138 => Pump::One(f(0x102, 0x102)),
            0x13C => Pump::One(MOUSE),
            0x140 => Pump::One(f(0x200, 0x200)),
            0x144 => Pump::Click,
            0x148 => Pump::Discard(KEYS),
            0x14C => Pump::Discard(MOUSE),
            0x150 => Pump::Drain(f(0x401, 0x401)),
            _ => return None,
        })
    }
}

/// +0x12C's choice among its peeks (each `true` if that peek found a
/// message): jgl makes them in order and stops at the first that finds
/// one (a short-circuit `||`), then takes the smallest message time of the
/// three buffers, which it filled with 0xFFFFFFFF first. Only the peek that
/// found one wrote its buffer, so it is the one taken (a message stamped
/// 0xFFFFFFFF, after 49.7 days of uptime, would tie and jgl would take
/// buffer 0, whatever it holds: not modelled).
pub fn oldest(found: [bool; 3]) -> Option<usize> {
    found.iter().position(|&f| f)
}

/// The button-up message that goes with a button-down in pump +0x144.
pub fn button_up(down: u32) -> Option<u32> {
    match down {
        0x201 => Some(0x202),
        0x204 => Some(0x205),
        _ => None,
    }
}

/// `"JackalClass"`, jgl's window class.
pub const CLASS: &[u8] = b"JackalClass\0";
/// The class style: `CS_VREDRAW | CS_HREDRAW | CS_DBLCLKS | CS_OWNDC`.
pub const CLASS_STYLE: u32 = 0x2B;
/// The window's styles: `WS_EX_APPWINDOW`; `WS_POPUP | WS_CLIPSIBLINGS |
/// WS_CLIPCHILDREN` (not visible).
pub const EX_STYLE: u32 = 0x0004_0000;
pub const STYLE: u32 = 0x8600_0000;
/// `ShowWindow` commands of graphics +0x10..+0x1C.
pub const SHOW: [(u16, i32); 4] = [(0x10, 1), (0x14, 0), (0x18, 6), (0x1C, 3)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_and_keys_go_to_the_controller() {
        let at = |x: i32, y: i32| ((y as u16 as isize) << 16) | x as u16 as isize;
        assert_eq!(route(1, 0x200, 0, at(10, -2)), Route::Tell(Controller::Move(10, -2)));
        assert_eq!(route(1, 0x203, 0, at(3, 4)), Route::Tell(Controller::LeftDown(true, 3, 4)));
        assert_eq!(route(1, 0x204, 0, at(3, 4)), Route::Tell(Controller::RightDown(false, 3, 4)));
        assert_eq!(route(1, 0x20A, (-120i16 as u16 as usize) << 16, at(5, 6)), Route::TellThenDefault(Controller::Wheel(-120, 5, 6)));
        assert_eq!(route(1, 0x104, 0x12, 0), Route::Key { vk: 0x12, down: true });
        assert_eq!(route(1, 0x105, 0x12, 0), Route::Key { vk: 0x12, down: false });
        assert_eq!(route(1, 0x84, 0, at(7, 8)), Route::Ask(Controller::HitTest(7, 8)));
        assert_eq!(route(1, 0x401, 0xABC, 0), Route::TellThenDefault(Controller::Timer(0xABC)));
        assert_eq!(route(1, 0x207, 0, 0), Route::Default, "the middle button is not jgl's");
        assert_eq!(route(1, 0x10, 0, 0), Route::Default, "WM_CLOSE");
    }

    #[test]
    fn the_screen_stays_on_and_the_palette_follows() {
        assert_eq!(route(1, 0x112, 0xF140, 0), Route::Answer(0));
        assert_eq!(route(1, 0x112, 0xF141, 0), Route::Default, "exact compare, no 0xFFF0 mask");
        assert_eq!(route(1, 0x218, 0, 0), Route::Answer(0x424D_5144));
        assert_eq!(route(1, 0x218, 7, 0), Route::Default);
        assert_eq!(route(1, 0x30F, 0, 0), Route::Realize(1));
        assert_eq!(route(1, 0x311, 1, 0), Route::Answer(0));
        assert_eq!(route(1, 0x311, 2, 0), Route::Realize(0));
        assert_eq!(route(1, 0x06, 0x0001, 0), Route::Activate { realize: true, tell: Controller::Activate(1, 0) });
        assert_eq!(route(1, 0x06, 0x1_0001, 0), Route::Activate { realize: false, tell: Controller::Activate(1, 1) });
    }

    #[test]
    fn controller_slots_and_arguments() {
        assert_eq!(Controller::Timer(5).slot(), 0x30);
        assert_eq!(Controller::Message402.slot(), 0x34);
        assert_eq!(Controller::LeftDown(true, 1, -1).args(), vec![1, 1, u32::MAX]);
        assert_eq!(Controller::Key(0x41, false).args(), vec![0x41, 0]);
    }

    #[test]
    fn oldest_takes_the_first_peek_that_found_one() {
        assert_eq!(oldest([false; 3]), None);
        assert_eq!(oldest([true, false, false]), Some(0));
        assert_eq!(oldest([false, true, false]), Some(1));
        assert_eq!(oldest([false, false, true]), Some(2));
        // A later peek is never made once one found a message.
        assert_eq!(oldest([true, true, true]), Some(0));
    }

    #[test]
    fn pumps_by_slot() {
        assert_eq!(Pump::at(0x128), Some(Pump::Any));
        assert_eq!(Pump::at(0x148), Some(Pump::Discard(KEYS)));
        assert_eq!(Pump::at(0x150), Some(Pump::Drain(Filter { first: 0x401, last: 0x401 })));
        assert_eq!(Pump::at(0x154), None);
        assert_eq!(button_up(0x201), Some(0x202));
        assert!(drops_chars(0x60) && drops_chars(0x6F) && !drops_chars(0x70));
    }
}
