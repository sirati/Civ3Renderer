//! The multiplayer screens' controls the screens paint and test in their
//! own click code (docs §16.11), as in [`super::screens`]: the geometry
//! comes from the forms' fields and the handlers' constants, each control
//! listed when its click does something.
//!
//! Multiplayer setup (vtable 0x667540), `On_Left_Click` 0x48BE80, every
//! RECT inclusive in the form's pixels, set by 0x485DC0 (0x486737 for
//! Ready):
//! - Rename (+0x6202C) while the local player is not ready and the setup
//!   is no scenario's with fixed players (+0xCCC) (0x48E730).
//! - Ready, the local player's box (+0x6201C), unless the setup's state
//!   [0x990350] is 3 or 4.
//! - A row's player number button (+0x6207C + 16·row, left of its name;
//!   0x48C246, 0x48D04B): it steps the player number that slot plays
//!   (0x910234 + 0x63C·row), which picks the player of a saved game or of
//!   a scenario with fixed players (in one, only those that can be played,
//!   the civilization box following it). Listed only where the setup draws
//!   them (0x486A20: the player data [0x9C719C] read for the rows): a
//!   loaded game ([0x9C3D6C] > 0 and [0x9C3D34] > −1) or such a scenario;
//!   in a new standard game none is drawn. The local player's own row
//!   while not ready, and the host's (below) every human slot's (kind
//!   0x910238 + 0x63C·row 2 or 3) while the host is not ready. (The host's
//!   click also acts in the setup's state 3 or 4 with none drawn, reading
//!   that player data: in a new game it is NULL and the click crashes the
//!   game, 0x48D182. Nothing drawn there is a control.)
//! - The host's only (session role [0x9AFD34] 2 or 4, 0x49A050): the
//!   game's name (+0x6206C, a prompt), Launch (+0x6203C) while no
//!   countdown runs (+0x6210D), and, while the host is not ready, the game
//!   rules (0x48E4E0): 18 rows in two columns of 9 (x 382 or 672, 253
//!   wide), five shown from the scroll (+0x73F40), row `r` at y 464 + 23r,
//!   21 high; rule `i` takes clicks if its flag [0x667318 + 4i] is not
//!   0x20000, the rules are not locked ([0x91D054]), and its flag is in
//!   0x1001F with [0x9C4094] set, in 0x6FFE0 with [0x9C4090] set, or
//!   [0x990354] is 3.
//!
//! The local player's slot (1..8): the one whose id (0x90FC04 + 0x63C·(k −
//! 1)) is the local player's ([0x9AFD1C]) in a network game ([0x74AF3C]),
//! else the current player ([0x9FD47C]); its ready flag is the byte at
//! 0x90FBEC + 0x63C·slot.
//!
//! Multiplayer lobby (`MP_Manager_Form`, vtable 0x667000), `On_Left_Click`
//! 0x481640: its `Rects[14]` (+0xDA8 + 16·i, inclusive), each with an
//! action of its own: the column of buttons 0..5 (not 4, which no click
//! tests), the six column titles 6..11 (the sort), 12 and the check box 13
//! (set by 0x47E520 and 0x47F5A0). The game list: x 0xEC..0x3C2 and y
//! 0x84..0x232 (open), rows of 24 from y 0x84, row `r` the entry after
//! `r` + the scroll (+0x1417F8) named ones (+0x96E88, 300 bytes each,
//! 0x800 of them, the walk stopping at the last; named: not the string
//! at 0x73D9F8), if it is named.

use super::advisors::{CELL_BUTTON, CELL_HEADER, cell_id};
use super::{Control, ControlKind, Form, Mem, empty, rect_at};

pub const SETUP: u32 = 0x66_7540;
pub const LOBBY: u32 = 0x66_7000;

/// A game rule's row on the setup screen.
pub const CELL_RULE: u32 = 16;
/// A game's entry in the lobby's list.
pub const CELL_GAME: u32 = 17;
/// A setup row's player number button.
pub const CELL_PLAYER: u32 = 18;

const NETWORK_GAME: u32 = 0x74_AF3C;
const CURRENT_PLAYER: u32 = 0x9F_D47C;
const LOCAL_ID: u32 = 0x9A_FD1C;
const SESSION_ROLE: u32 = 0x9A_FD34;
const SLOT_IDS: u32 = 0x90_FC04;
const SLOT_READY: u32 = 0x90_FBEC;
const SLOT_SIZE: u32 = 0x63C;
/// Row `k`'s (slot `k` + 1's) kind (+0x63C·k): 2 and 3 human.
const SLOT_KINDS: u32 = 0x91_0238;
const SLOTS: u32 = 8;
const SETUP_STATE: u32 = 0x99_0350;
/// A loaded game: [0x9C3D6C] > 0 and [0x9C3D34] > −1.
const LOADED_A: u32 = 0x9C_3D6C;
const LOADED_B: u32 = 0x9C_3D34;

/// A scenario's set-up with fixed players.
const S_FIXED_PLAYERS: u32 = 0xCCC;
const S_COUNTDOWN: u32 = 0x6_210D;
const S_READY: u32 = 0x6_201C;
const S_RENAME: u32 = 0x6_202C;
const S_LAUNCH: u32 = 0x6_203C;
const S_NAME: u32 = 0x6_206C;
const S_PLAYER_BUTTONS: u32 = 0x6_207C;
const S_RULES_SCROLL: u32 = 0x7_3F40;

const RULE_FLAGS: u32 = 0x66_7318;
const RULES_A: u32 = 0x9C_4094;
const RULES_B: u32 = 0x9C_4090;
const RULES_STATE: u32 = 0x99_0354;
const RULES_LOCKED: u32 = 0x91_D054;
const RULES: u32 = 18;
const RULES_PER_COLUMN: u32 = 9;
const RULES_SHOWN: i32 = 5;
const RULE_LEFT: [i32; 2] = [382, 672];
const RULE_TOP: i32 = 464;
const RULE_PITCH: i32 = 23;
const RULE_SIZE: (i32, i32) = (253, 21);

const L_RECTS: u32 = 0xDA8;
/// The lobby's rectangles its click tests, and as what.
const L_CELLS: [(u32, u32, u32); 13] = [
    (0, CELL_BUTTON, 0), (1, CELL_BUTTON, 1), (2, CELL_BUTTON, 2), (3, CELL_BUTTON, 3), (5, CELL_BUTTON, 5),
    (6, CELL_HEADER, 0), (7, CELL_HEADER, 1), (8, CELL_HEADER, 2), (9, CELL_HEADER, 3), (10, CELL_HEADER, 4), (11, CELL_HEADER, 5),
    (12, CELL_BUTTON, 12), (13, CELL_BUTTON, 13),
];
const L_LIST_X: (i32, i32) = (0xEC, 0x3C2);
const L_LIST_Y: (i32, i32) = (0x84, 0x232);
const L_ROW: i32 = 24;
const L_SCROLL: u32 = 0x14_17F8;
const L_GAMES: u32 = 0x9_6E88;
const L_GAME_SIZE: u32 = 300;
/// The last game entry (the walk stops there).
const L_GAMES_LAST: u32 = 0x7FF;
const EMPTY_NAME: u32 = 0x73_D9F8;
/// The longest name compared (the game's are shorter).
const NAME_MAX: u32 = 64;

fn inclusive(mem: &dyn Mem, at: u32) -> Option<[i32; 4]> {
    let [l, t, r, b] = rect_at(mem, at)?;
    (r >= l && b >= t).then(|| [l, t, r - l + 1, b - t + 1])
}

/// The local player's slot on the setup screen (0: none found).
fn local_slot(mem: &dyn Mem) -> Option<u32> {
    if mem.u8(NETWORK_GAME)? == 0 {
        return mem.u32(CURRENT_PLAYER);
    }
    let id = mem.u32(LOCAL_ID)?;
    Some((1..=SLOTS).find(|k| mem.u32(SLOT_IDS + SLOT_SIZE * (k - 1)) == Some(id)).unwrap_or(0))
}

/// The setup screen's own controls: (cell kind, index, rect in its pixels).
pub fn setup(mem: &dyn Mem, form: u32) -> Option<Vec<(u32, u32, [i32; 4])>> {
    if mem.u32(form)? != SETUP {
        return None;
    }
    let slot = local_slot(mem)?;
    let ready = mem.u8(SLOT_READY.checked_add(SLOT_SIZE.checked_mul(slot)?)?)? != 0;
    let fixed_players = mem.u8(form + S_FIXED_PLAYERS)? != 0;
    let loaded = mem.i32(LOADED_A)? > 0 && mem.i32(LOADED_B)? > -1;
    let state = mem.i32(SETUP_STATE)?;
    let mut out = Vec::new();
    if !ready && !fixed_players {
        out.extend(inclusive(mem, form + S_RENAME).map(|r| (CELL_BUTTON, 1, r)));
    }
    if !matches!(state, 3 | 4) {
        out.extend(inclusive(mem, form + S_READY).map(|r| (CELL_BUTTON, 0, r)));
    }
    // The rows whose player number button takes clicks.
    let mut players: Vec<u32> = Vec::new();
    if !ready && (loaded || fixed_players) && (1..=SLOTS).contains(&slot) {
        players.push(slot - 1);
    }
    let host = matches!(mem.i32(SESSION_ROLE)?, 2 | 4);
    if host {
        out.extend(inclusive(mem, form + S_NAME).map(|r| (CELL_BUTTON, 3, r)));
        if mem.u8(form + S_COUNTDOWN)? == 0 {
            out.extend(inclusive(mem, form + S_LAUNCH).map(|r| (CELL_BUTTON, 2, r)));
        }
        if !ready {
            out.extend(rules(mem, form).unwrap_or_default());
            if loaded || fixed_players {
                players.extend((0..SLOTS).filter(|&k| matches!(mem.u32(SLOT_KINDS + SLOT_SIZE * k), Some(2 | 3))));
            }
        }
    }
    players.sort_unstable();
    players.dedup();
    for k in players {
        out.extend(inclusive(mem, form + S_PLAYER_BUTTONS + 16 * k).map(|r| (CELL_PLAYER, k, r)));
    }
    Some(out)
}

/// The game rules' rows that take clicks (0x48E4E0), as [`setup`]'s.
fn rules(mem: &dyn Mem, form: u32) -> Option<Vec<(u32, u32, [i32; 4])>> {
    let scroll = mem.i32(form + S_RULES_SCROLL)?;
    let (a, b, state) = (mem.u32(RULES_A)? != 0, mem.u32(RULES_B)? != 0, mem.i32(RULES_STATE)?);
    if mem.u8(RULES_LOCKED)? != 0 {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for i in 0..RULES {
        let flag = mem.u32(RULE_FLAGS + 4 * i)?;
        let open = (a && flag & 0x1_001F != 0) || (b && flag & 0x6_FFE0 != 0) || state == 3;
        let row = (i % RULES_PER_COLUMN) as i32 - scroll;
        if open && flag != 0x2_0000 && (0..RULES_SHOWN).contains(&row) {
            let left = RULE_LEFT[(i / RULES_PER_COLUMN) as usize];
            out.push((CELL_RULE, i, [left, RULE_TOP + RULE_PITCH * row, RULE_SIZE.0, RULE_SIZE.1]));
        }
    }
    Some(out)
}

/// Whether the C string at `a` equals the one at `b` (as `strcmp`).
fn same_name(mem: &dyn Mem, a: u32, b: u32) -> Option<bool> {
    for i in 0..NAME_MAX {
        let (x, y) = (mem.u8(a + i)?, mem.u8(b + i)?);
        if x != y {
            return Some(false);
        }
        if x == 0 {
            return Some(true);
        }
    }
    Some(true)
}

/// The lobby's own controls: (cell kind, index, rect in its pixels, kind).
pub fn lobby(mem: &dyn Mem, form: u32) -> Option<Vec<(u32, u32, [i32; 4], ControlKind)>> {
    if mem.u32(form)? != LOBBY {
        return None;
    }
    let mut out: Vec<(u32, u32, [i32; 4], ControlKind)> = L_CELLS
        .iter()
        .filter_map(|&(i, cell, index)| Some((cell, index, inclusive(mem, form + L_RECTS + 16 * i)?, ControlKind::Button)))
        .collect();
    out.extend(games(mem, form).unwrap_or_default().into_iter().map(|(i, r)| (CELL_GAME, i, r, ControlKind::ListItem)));
    Some(out)
}

/// The lobby list's rows that select a game: (its entry, rect).
fn games(mem: &dyn Mem, form: u32) -> Option<Vec<(u32, [i32; 4])>> {
    // (Named: as the click's walk, 0x481ABD.)
    let named = |i: u32| same_name(mem, form + L_GAMES + L_GAME_SIZE * i, EMPTY_NAME).map(|same| !same);
    let mut out = Vec::new();
    let scroll = mem.i32(form + L_SCROLL)?.max(0);
    let rows = (L_LIST_Y.1 - 1 - L_LIST_Y.0) / L_ROW + 1;
    // Row `r`: the entry after `r` + scroll named ones (one walk for all
    // rows: each row's goes on from the one above).
    let (mut passed, mut i) = (0, 0);
    for r in 0..rows {
        while passed < scroll + r && i < L_GAMES_LAST {
            if named(i)? {
                passed += 1;
            }
            i += 1;
        }
        if !named(i)? {
            break;
        }
        let top = (L_LIST_Y.0 + L_ROW * r).max(L_LIST_Y.0 + 1);
        let bottom = (L_LIST_Y.0 + L_ROW * (r + 1) - 1).min(L_LIST_Y.1 - 1);
        let (x, w) = (L_LIST_X.0 + 1, L_LIST_X.1 - L_LIST_X.0 - 1);
        out.push((i, [x, top, w, bottom - top + 1]));
    }
    Some(out)
}

pub(super) fn controls(mem: &dyn Mem, form: &Form, out: &mut Vec<Control>) {
    let cells: Vec<(u32, u32, [i32; 4], ControlKind)> = match form.vtable {
        SETUP => setup(mem, form.addr).unwrap_or_default().into_iter().map(|(c, i, r)| (c, i, r, ControlKind::Button)).collect(),
        LOBBY => lobby(mem, form.addr).unwrap_or_default(),
        _ => return,
    };
    for (cell, index, r, kind) in cells {
        let rect = form.on_screen(r);
        if !empty(rect) {
            out.push(Control { rect, kind, id: cell_id(form.addr, cell, index) });
        }
    }
}

#[cfg(test)]
#[path = "multiplayer/tests.rs"]
mod tests;
