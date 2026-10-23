//! What the screens after a game click without button forms: rectangles
//! their own click code tests (docs §16.11). As in [`super::screens`], the
//! geometry comes from the forms' fields and the handlers' constants.
//!
//! Summary replay (vtable 0x66E838), click 0x58CEF0: five playback
//! buttons painted into the screen, inclusive RECTs set when it opens
//! (0x58C4F0): first +0x51B0, back +0x5160, stop +0x5170, play +0x5180,
//! last +0x5190. (Its +0x51A0 is the map's rectangle, which takes no
//! click.) Its close button, event list, check box and combo box are forms.
//!
//! Historical ranking (vtable 0x66B700), press 0x58A4E0: its close button,
//! the sprite at 0xB3F36C drawn at (0x3B8, 0x2CE), as large as that sprite
//! (0xB3F384, 0xB3F388), half-open.
//!
//! Victory screen (vtable 0x66B888), press 0x5CF3D0: a 42×42 close button
//! (`PtInRect`) at (0x3CC, 0xF) from its board's corner, the board
//! (+0x6D8 × +0x6DC) in the middle of the game's screen.
//!
//! Hall of fame (vtable 0x66D1F0), click 0x53FE40, all inclusive RECTs:
//! each civilization's name (+0x3904 + 16·i, as many as the races less the
//! barbarians, [0x9C3D74] − 1, set by 0x53F988) toggles it, the five
//! column titles (+0x3B04 + 16·i) sort, "All Leaders On" (+0x3B54) toggles
//! them all. (A sixth rectangle, +0x3B74 at the title's right, closes it
//! with nothing drawn there; the close button is a form.)
//!
//! Demographics (F11, 0x66BF10) and the histograph (F8, 0x66D380) click
//! nothing of their own (0x540020 is `ret 8`): their buttons are forms.

use super::advisors::{CELL_BUTTON, CELL_CIV, CELL_HEADER, cell_id};
use super::{Control, ControlKind, Form, Mem, empty, rect_at};

pub const REPLAY: u32 = 0x66_E838;
pub const RANKING: u32 = 0x66_B700;
pub const VICTORY: u32 = 0x66_B888;
pub const HALL_OF_FAME: u32 = 0x66_D1F0;

/// The replay's playback buttons, left to right.
pub const REPLAY_BUTTONS: [u32; 5] = [0x51B0, 0x5160, 0x5170, 0x5180, 0x5190];
const RANKING_SPRITE_SIZE: u32 = 0xB3_F384;
const RANKING_BUTTON: (i32, i32) = (0x3B8, 0x2CE);
const VICTORY_BOARD: u32 = 0x6D8;
const VICTORY_BUTTON: (i32, i32, i32) = (0x3CC, 0xF, 0x2A);
const SCREEN_W: u32 = 0x9C_72F8;
const SCREEN_H: u32 = 0x9C_72FC;
const RACES: u32 = 0x9C_3D74;
const HOF_CIVS: u32 = 0x3904;
const HOF_COLUMNS: u32 = 0x3B04;
const HOF_ALL: u32 = 0x3B54;

/// An inclusive RECT as x, y, w, h (none if inverted).
fn inclusive(mem: &dyn Mem, at: u32) -> Option<[i32; 4]> {
    let [l, t, r, b] = rect_at(mem, at)?;
    (r >= l && b >= t).then(|| [l, t, r - l + 1, b - t + 1])
}

/// The screen's own click targets: (cell kind, index, rectangle in its
/// pixels).
pub fn targets(mem: &dyn Mem, form: u32) -> Vec<(u32, u32, [i32; 4])> {
    let mut out = Vec::new();
    match mem.u32(form) {
        Some(REPLAY) => {
            for (i, &at) in REPLAY_BUTTONS.iter().enumerate() {
                out.extend(inclusive(mem, form + at).map(|r| (CELL_BUTTON, i as u32, r)));
            }
        }
        Some(RANKING) => {
            if let (Some(w), Some(h)) = (mem.i32(RANKING_SPRITE_SIZE), mem.i32(RANKING_SPRITE_SIZE + 4)) {
                out.push((CELL_BUTTON, 0, [RANKING_BUTTON.0, RANKING_BUTTON.1, w, h]));
            }
        }
        Some(VICTORY) => {
            let size = |screen: u32, board: u32| Some((mem.i32(screen)? - mem.i32(form + board)?) / 2);
            if let (Some(x), Some(y)) = (size(SCREEN_W, VICTORY_BOARD), size(SCREEN_H, VICTORY_BOARD + 4)) {
                let (dx, dy, side) = VICTORY_BUTTON;
                out.push((CELL_BUTTON, 0, [x + dx, y + dy, side, side]));
            }
        }
        Some(HALL_OF_FAME) => {
            let civs = mem.i32(RACES).map_or(0, |n| (n - 1).clamp(0, 32)) as u32;
            for i in 0..civs {
                out.extend(inclusive(mem, form + HOF_CIVS + 16 * i).map(|r| (CELL_CIV, i, r)));
            }
            for i in 0..5 {
                out.extend(inclusive(mem, form + HOF_COLUMNS + 16 * i).map(|r| (CELL_HEADER, i, r)));
            }
            out.extend(inclusive(mem, form + HOF_ALL).map(|r| (CELL_BUTTON, 0, r)));
        }
        _ => {}
    }
    out
}

pub(super) fn controls(mem: &dyn Mem, form: &Form, out: &mut Vec<Control>) {
    if !matches!(form.vtable, REPLAY | RANKING | VICTORY | HALL_OF_FAME) {
        return;
    }
    for (cell, index, r) in targets(mem, form.addr) {
        let rect = form.on_screen(r);
        if !empty(rect) {
            out.push(Control { rect, kind: ControlKind::Button, id: cell_id(form.addr, cell, index) });
        }
    }
}

#[cfg(test)]
#[path = "end_screens/tests.rs"]
mod tests;
