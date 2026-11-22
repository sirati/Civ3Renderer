//! Rows and cells of the military advisor, the F7 Wonders, F9 Palace and
//! F10 Spaceship screens and the diplomacy screen, tested in their own
//! click code. As in [`super::advisors`], the geometry comes from the
//! forms' fields and region tables and the handlers' constants.
//!
//! Military advisor (vtable 0x66DF50): left 0x571C70, right 0x571040.
//! - Region table +0x100A8: values 0..3 the two lists' sort tabs, 4 the
//!   target civ's label (a menu of civs).
//! - 0x570130: with the left list by city (+0x35F2), row `i`'s name cell
//!   (+0xFDB8 + 16·i, inclusive, x < the first name cell's right) is the
//!   player's (+0x35D8 + i)-th city: left opens it, right its quick build
//!   chooser. Unit cells (+0xFE18 / +0xFF38 + 16·i, inclusive, rows below
//!   +0x35E0/+0x35D0 and +0x35E4/+0x35D4) are 32-pixel columns,
//!   `L + 32k < x <= L + 32(k + 1)` (0x5705F0); which hold a unit is
//!   only drawn, so the bridge places them at the drawn unit icons
//!   ([`military_unit_strips`]).
//!
//! Wonders (vtable 0x66FF98): left 0x5D13C0 tests six boxes (+0x19B8 +
//! 16·i, inclusive) holding improvement `[+0x1F34 + 4·i] > 0` (Civilopedia);
//! the wheel 0x5D1490 scrolls a row of two (+0x1F2C, entries +0x1F30).
//!
//! Palace (F9, vtable 0x66E0F8) and Spaceship (F10, vtable 0x66EE48) test
//! an 8-bit mask (0x600220) the game paints with each choosable item in its
//! own colour (Palace +0x3154, rebuilt by 0x57C8A0: unbuilt items whose
//! prerequisites are met; Spaceship +0x574, 0x5A3820: incomplete parts
//! with their advance known, info 0xFC, close 0xFD, launch 0xFE). One
//! target per colour, the largest rectangle of that colour.
//!
//! Diplomacy (`DiploForm`, vtable 0x66C0D8): left click 0x5076A0.
//! - Response options (AI partner): x 0x130..0x2D8, rows of the canvas
//!   line height from 0x15E + off + 3·lh (off 0x8A in modes 2 and 3),
//!   the options' lines (list +0x1388: lines +0xC, next +0x10) bottom
//!   aligned in nine rows. The band takes every click with an AI partner.
//! - Trade trees (mode 2, page 1; 0x508D00): right x 0x319..0x3DE (flags
//!   +0xF0E, counts +0xF24, scroll +0xEC4), left x 0x2C..0xF1 (AI partner;
//!   +0xF19, +0xF50, +0xEC8), y 0xE4..0x266, rows of lh from the scroll
//!   over 11 categories: a header (toggles, if it has items), then its
//!   items when expanded (+0xF03).
//! - Region table +0x13E0 and the negotiation table (value 7): mode 2,
//!   page 1, columns x 0x13C..0x205 (theirs, AI partner; list +0xF80 +
//!   16·civ, count +4, scroll +0xECC) and 0x206..0x2CD (ours, +0x1180,
//!   +0xED0), y 0x1CC..0x21C, rows of lh.
//!
//! City governor (`Governor_Form`, vtable 0x66CE90): `On_Left_Click`
//! (vtable +0x74) 0x53BD80 tests four rectangles (+0x94398 + 16·i,
//! inclusive, the form's pixels): the Common and Production tabs (page
//! +0x31E0), OK (keeps the policies) and Cancel, which both close it. Its
//! combo boxes and check box are forms of their own.

use super::advisors::{CELL_BUILDING, CELL_BUTTON, CELL_CITY, CELL_LIST, CELL_TAB, cell_id};
use super::{Control, ControlKind, Form, Mem, empty, rect_at};

pub const CELL_MASK: u32 = 11;
pub const CELL_OPTION: u32 = 12;
pub const CELL_TREE: u32 = 13;
pub const CELL_TABLE: u32 = 14;
pub const CELL_UNIT: u32 = 15;

pub const MILITARY: u32 = 0x66_DF50;
pub const WONDERS: u32 = 0x66_FF98;
pub const PALACE: u32 = 0x66_E0F8;
pub const SPACESHIP: u32 = 0x66_EE48;
pub const DIPLOMACY: u32 = 0x66_C0D8;
pub const GOVERNOR: u32 = 0x66_CE90;

/// The city governor's tabs, OK and Cancel (`Governor_Form` +0x94398).
const G_RECTS: u32 = 0x9_4398;
/// What each of those rectangles is: (cell kind, index).
const G_CELLS: [(u32, u32); 4] = [(CELL_TAB, 0), (CELL_TAB, 1), (CELL_BUTTON, 0), (CELL_BUTTON, 1)];

const PLAYER: u32 = 0x9F_D47C;
const CITY_ITEMS: u32 = 0xA5_2E2C;
const CITY_LAST: u32 = 0xA5_2E38;
const SCREEN_W: u32 = 0x9C_72F8;
const SCREEN_H: u32 = 0x9C_72FC;
const HUMANS: u32 = 0xA5_267C;
const LEADERS: u32 = 0xA5_2E58;

fn inclusive(mem: &dyn Mem, at: u32) -> Option<[i32; 4]> {
    let [l, t, r, b] = rect_at(mem, at)?;
    (r >= l && b >= t).then(|| [l, t, r - l + 1, b - t + 1])
}

fn regions(mem: &dyn Mem, table: u32) -> Vec<(u32, [i32; 4])> {
    let n = mem.u32(table + 0x58).unwrap_or(0).min(64);
    let Some(base) = mem.u32(table + 0x50).filter(|&b| b != 0) else { return vec![] };
    (0..n)
        .filter_map(|i| {
            let [l, t, r, b] = rect_at(mem, base + 32 * i + 4)?;
            (r > l && b > t).then_some((mem.u32(base + 32 * i + 0x18)?, [l, t, r - l, b - t]))
        })
        .collect()
}

fn intersect([ax, ay, aw, ah]: [i32; 4], [bx, by, bw, bh]: [i32; 4]) -> [i32; 4] {
    super::intersect([ax, ay, aw, ah], [bx, by, bw, bh])
}

/// Row `i` of inclusive row rectangles at `rows` (16 bytes each), less the
/// line it shares with the row above, which the first match takes.
fn below(mem: &dyn Mem, rows: u32, i: u32) -> Option<[i32; 4]> {
    let [x, y, w, h] = inclusive(mem, rows + 16 * i)?;
    let above = if i == 0 { None } else { inclusive(mem, rows + 16 * (i - 1)) };
    let top = match above {
        Some([_, ay, _, ah]) if ay + ah > y => ay + ah,
        _ => y,
    };
    (y + h > top).then(|| [x, top, w, y + h - top])
}

/// The player's cities in the game's table order.
fn own_cities(mem: &dyn Mem) -> Vec<u32> {
    let (Some(items), Some(last), Some(player)) = (mem.u32(CITY_ITEMS), mem.i32(CITY_LAST), mem.u32(PLAYER)) else { return vec![] };
    if items == 0 {
        return vec![];
    }
    (0..=last.clamp(-1, 4095))
        .filter_map(|id| {
            let p = mem.u32(items + 8 * id as u32 + 4).filter(|&p| p != 0)?;
            let city = p.wrapping_sub(0x1C);
            (mem.u8(city + 0x28)? as u32 == player).then_some(city)
        })
        .collect()
}

// Military advisor.
const M_SCROLL_CITIES: u32 = 0x35D8;
const M_TYPE_ROWS: [u32; 2] = [0x35D0, 0x35D4];
const M_CITY_ROWS: [u32; 2] = [0x35E0, 0x35E4];
const M_BY_CITY: [u32; 2] = [0x35F2, 0x35F3];
const M_SELECTED_CIV: u32 = 0x35C4;
const M_NAMES: [u32; 2] = [0xFDB8, 0xFED8];
const M_UNITS: [u32; 2] = [0xFE18, 0xFF38];
const M_RIGHT_SPLIT: u32 = 0xFFE0;
const M_REGIONS: u32 = 0x1_00A8;

/// The military advisor's parts: tabs (value, rect) and the left list's
/// city name cells (city place, rect), in the form's pixels.
pub fn military(mem: &dyn Mem, form: u32) -> Option<(Vec<(u32, [i32; 4])>, Vec<(u32, [i32; 4])>)> {
    if mem.u32(form)? != MILITARY {
        return None;
    }
    let tabs = regions(mem, form + M_REGIONS).into_iter().filter(|(v, _)| *v <= 4).collect();
    let mut names = Vec::new();
    if mem.u8(form + M_BY_CITY[0])? != 0 {
        let cut = mem.i32(form + M_NAMES[0] + 8)?;
        let (scroll, count) = (mem.i32(form + M_SCROLL_CITIES)?.max(0), own_cities(mem).len() as i32);
        for i in 0..6 {
            let Some([x, y, w, h]) = below(mem, form + M_NAMES[0], i) else { continue };
            let w = w.min(cut - x);
            if scroll + (i as i32) < count && w > 0 {
                names.push(((scroll + i as i32) as u32, [x, y, w, h]));
            }
        }
    }
    Some((tabs, names))
}

/// A strip of 32-pixel unit columns of the military advisor, on the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitStrip {
    pub form: u32,
    /// The form's canvas (its draws).
    pub canvas: u32,
    /// 0: the player's list, 1: the selected civ's.
    pub side: u32,
    pub row: u32,
    /// Where it takes clicks, x, y, w, h.
    pub rect: [i32; 4],
    /// The columns' origin `L` (column k: L + 32k < x <= L + 32(k + 1)).
    pub left: i32,
}

/// The unit strips of a shown military advisor among `controls` (its
/// tabs identify it).
pub fn military_unit_strips(mem: &dyn Mem, controls: &[Control]) -> Vec<UnitStrip> {
    let mut forms: Vec<u32> = controls
        .iter()
        .filter(|c| super::advisors::cell_kind(c.id) == Some(CELL_TAB))
        .map(|c| (c.id >> 32) as u32)
        .filter(|&f| mem.u32(f) == Some(MILITARY))
        .collect();
    forms.dedup();
    let mut out = Vec::new();
    for f in forms {
        let Some(form) = Form::read(mem, f) else { continue };
        let (Some(name_cut), Some(split)) = (mem.i32(f + M_NAMES[0] + 8), mem.i32(f + M_RIGHT_SPLIT)) else { continue };
        let right_name = (mem.i32(f + 0xFFE8), mem.i32(f + M_NAMES[1] + 8));
        for side in 0..2u32 {
            if side == 1 && mem.i32(f + M_SELECTED_CIV).is_none_or(|c| c < 1) {
                continue;
            }
            let by_city = mem.u8(f + M_BY_CITY[side as usize]).unwrap_or(0) != 0;
            let rows = mem.i32(f + if by_city { M_CITY_ROWS[side as usize] } else { M_TYPE_ROWS[side as usize] }).unwrap_or(0).clamp(0, 6);
            for i in 0..rows as u32 {
                let Some([x, y, w, h]) = below(mem, f + M_UNITS[side as usize], i) else { continue };
                // The x ranges 0x570130 sends to this side's units.
                let (lo, hi) = if side == 0 {
                    (name_cut, split)
                } else {
                    (match right_name { (Some(_), Some(r)) => split.max(r), _ => split }, i32::MAX)
                };
                let (l, r) = (x.max(lo), (x + w).min(hi));
                let rect = form.on_screen([l, y, r - l, h]);
                if r > l && !empty(rect) {
                    out.push(UnitStrip {
                        form: f,
                        canvas: f + crate::layout::BASE_FORM_DATA_CANVAS + 4,
                        side,
                        row: i,
                        rect,
                        left: form.origin.0 + x,
                    });
                }
            }
        }
    }
    out
}

/// Unit columns at drawn unit icons: column k of the icon's middle, its
/// 32-pixel column within the strip.
pub fn unit_columns(strip: &UnitStrip, icons: impl IntoIterator<Item = [i32; 4]>) -> Vec<Control> {
    let mut out: Vec<Control> = Vec::new();
    for [x, _, w, _] in icons {
        let middle = x + w / 2;
        let k = ((middle - strip.left - 1).max(0)) / 32;
        let column = [strip.left + 32 * k + 1, strip.rect[1], 32, strip.rect[3]];
        let rect = intersect(column, strip.rect);
        let id = cell_id(strip.form, CELL_UNIT, strip.side << 12 | strip.row << 8 | (k as u32 & 0xFF));
        if !empty(rect) && !out.iter().any(|c| c.id == id) {
            out.push(Control { rect, kind: ControlKind::Button, id });
        }
    }
    out
}

// Wonders.
const W_BOXES: u32 = 0x19B8;
const W_SCROLL: u32 = 0x1F2C;
const W_COUNT: u32 = 0x1F30;
const W_SLOTS: u32 = 0x1F34;

/// The filled Wonders boxes (entry number, rect) and whether they scroll.
pub fn wonders(mem: &dyn Mem, form: u32) -> Option<(Vec<(u32, [i32; 4])>, bool)> {
    if mem.u32(form)? != WONDERS {
        return None;
    }
    let (scroll, count) = (mem.i32(form + W_SCROLL)?.max(0), mem.i32(form + W_COUNT)?);
    let boxes = (0..6u32)
        .filter(|i| mem.i32(form + W_SLOTS + 4 * i).is_some_and(|v| v > 0))
        .filter_map(|i| Some(((2 * scroll) as u32 + i, inclusive(mem, form + W_BOXES + 16 * i)?)))
        .collect();
    Some((boxes, count > 6))
}

/// The colour regions of an 8-bit mask image (a `PCX_Image` at `pcx`)
/// within `window` (its pixels), each colour's largest rectangle.
pub fn mask_cells(mem: &dyn Mem, pcx: u32, window: [i32; 4], last: u8) -> Vec<(u8, [i32; 4])> {
    if let Some(cells) = mem.index_regions(pcx) {
        return cells.into_iter().filter_map(|(index, r)| {
            let r = intersect(r, window);
            (index <= last && !empty(r)).then_some((index, r))
        }).collect();
    }
    let read = || -> Option<Vec<(u8, [i32; 4])>> {
        let image = mem.u32(pcx.checked_add(4)?).filter(|&i| i != 0)?;
        if mem.u32(image + 0x24)? != 8 {
            return None;
        }
        let width = mem.i32(image + 0x5C)?.checked_sub(mem.i32(image + 0x54)?)?;
        let height = mem.i32(image + 0x60)?.checked_sub(mem.i32(image + 0x58)?)?;
        let stride = mem.i32(image + 0x40)?;
        let pixels = mem.u32(image + 0x4C0).filter(|&p| p != 0)?;
        if !(1..=8192).contains(&width) || !(1..=8192).contains(&height) || stride < width || stride > 16384 {
            return None;
        }
        let [x, y, w, h] = intersect(window, [0, 0, width, height]);
        if w <= 0 || h <= 0 {
            return None;
        }
        let mut crop = Vec::with_capacity((w * h) as usize);
        for row in y..y + h {
            let at = pixels.checked_add(u32::try_from(row * stride + x).ok()?)?;
            crop.extend(mem.read_bytes(at, w as usize)?);
        }
        Some(super::setup::regions(&crop, w as usize, h as usize, w as usize, last).into_iter().map(|(c, [rx, ry, rw, rh])| (c, [rx + x, ry + y, rw, rh])).collect())
    };
    read().unwrap_or_default()
}

/// The background's place on the screen-sized mask: centred.
fn centred(mem: &dyn Mem, sprite_size: u32) -> Option<[i32; 4]> {
    let (sw, sh) = (mem.i32(SCREEN_W)?, mem.i32(SCREEN_H)?);
    let (w, h) = (mem.i32(sprite_size)?, mem.i32(sprite_size + 4)?);
    (w > 0 && h > 0).then(|| [sw / 2 - w / 2, sh / 2 - h / 2, w, h])
}

/// The Palace's choosable items or the Spaceship's parts and buttons
/// (colour, rect in the form's pixels).
pub fn masked(mem: &dyn Mem, form: u32) -> Option<Vec<(u8, [i32; 4])>> {
    match mem.u32(form)? {
        PALACE => {
            let (sw, sh) = (mem.i32(SCREEN_W)?, mem.i32(SCREEN_H)?);
            let (w, h) = (mem.i32(form + 0x312C)?, mem.i32(form + 0x3130)?);
            let window = [(sw - w) / 2, (sh - h) / 2, w, h];
            Some(mask_cells(mem, form + 0x3154, window, 0x3F))
        }
        SPACESHIP => {
            let window = centred(mem, form + 0x84C)?;
            let parts = mem.i32(0x9C_7268).unwrap_or(0);
            Some(mask_cells(mem, form + 0x574, window, 0xFE).into_iter().filter(|(c, _)| (*c as i32) < parts || *c >= 0xFC).collect())
        }
        _ => None,
    }
}

// Diplomacy.
const D_PARTNER: u32 = 0xE98;
const D_MODE: u32 = 0xED8;
const D_PAGE: u32 = 0xEEC;
const D_OPTIONS: u32 = 0x1388;
const D_REGIONS: u32 = 0x13E0;

/// The diplomacy screen's cells (kind, index, rect in form pixels).
pub fn diplomacy(mem: &dyn Mem, form: u32) -> Option<Vec<(u32, u32, [i32; 4])>> {
    if mem.u32(form)? != DIPLOMACY {
        return None;
    }
    let lh = super::canvas_line_height(mem, form + crate::layout::BASE_FORM_DATA_CANVAS + 4).filter(|h| (1..=64).contains(h))?;
    let partner = mem.u32(form + D_PARTNER)?;
    let race = mem.u32(LEADERS + crate::layout::LEADER_SIZE.checked_mul(partner.min(31))? + 0x1C)? & 31;
    let ai = mem.u32(HUMANS)? & (1 << race) == 0;
    let (mode, page) = (mem.i32(form + D_MODE)?, mem.i32(form + D_PAGE)?);
    let trading = mode == 2 && page == 1;
    let off = if mode == 2 || mode == 3 { 0x8A } else { 0 };
    let band_top = 0x15E + off + 3 * lh;
    let band = [0x130, band_top, 0x2D8 - 0x130 + 1, 9 * lh + 1];
    let mut out = Vec::new();
    // Response options, bottom aligned in nine rows.
    if ai && (mode != 2 || page == 1) {
        let mut lines = Vec::new();
        let mut node = mem.u32(form + D_OPTIONS).unwrap_or(0);
        while node != 0 && lines.len() < 32 {
            lines.push(mem.i32(node + 0xC)?.clamp(0, 9));
            node = mem.u32(node + 0x10)?;
        }
        let mut row = 9 - lines.iter().sum::<i32>();
        for (k, n) in lines.iter().enumerate() {
            let (first, end) = (row.max(0), (row + n).min(10));
            if end > first {
                let r = intersect([0x130, band_top + first * lh, band[2], (end - first) * lh], band);
                if !empty(r) {
                    out.push((CELL_OPTION, k as u32, r));
                }
            }
            row += n;
        }
    }
    // Everything else under the band is the band's while the partner is an AI.
    let free = |r: [i32; 4]| -> [i32; 4] {
        if ai && r[0] < band[0] + band[2] && r[0] + r[2] > band[0] && r[1] + r[3] > band[1] {
            [r[0], r[1], r[2], (band[1] - r[1]).min(r[3])]
        } else {
            r
        }
    };
    if trading {
        let bottom = 0x266;
        for (side, x0, x1, flags, counts, scroll) in [(0u32, 0x319, 0x3DE, 0xF0E, 0xF24, 0xEC4), (1, 0x2C, 0xF1, 0xF19, 0xF50, 0xEC8)] {
            if side == 1 && !ai {
                continue;
            }
            let shown = (bottom - 0xE4) / lh + 1;
            let scroll = mem.i32(form + scroll)?.max(0);
            let mut logical = 0;
            for c in 0..11u32 {
                if mem.u8(form + flags + c)? == 0 {
                    continue;
                }
                let count = mem.i32(form + counts + 4 * c)?.clamp(0, 4096);
                let expanded = mem.u8(form + 0xF03 + c)? != 0;
                let items = if expanded { count } else { 0 };
                for item in 0..=items {
                    let place = logical - scroll;
                    logical += 1;
                    // A header without items does nothing.
                    if !(0..shown).contains(&place) || (item == 0 && count == 0) {
                        continue;
                    }
                    let top = 0xE4 + place * lh;
                    let r = [x0, top, x1 - x0 + 1, lh.min(bottom + 1 - top)];
                    out.push((CELL_TREE, side << 19 | c << 15 | item as u32, r));
                }
            }
        }
    }
    // The region table: page tabs, scroll arrows, the portrait, the advisor.
    for (value, r) in regions(mem, form + D_REGIONS) {
        let ok = match value {
            0 | 2 => trading,
            1 => mode == 2 && page == 2,
            3 | 4 | 12 | 13 => true,
            5 | 6 => ai,
            7 => mode == 3 && mem.u32(form + 0xEAC)? == 0,
            8..=11 => mode == 2,
            _ => false,
        };
        let r = if value == 7 { intersect(r, [0x13C, 0x1CC, 0x2CE - 0x13C + 1, 0x21C - 0x1CC + 1]) } else { r };
        if ok && !empty(free(r)) {
            out.push((CELL_BUTTON, value, free(r)));
        }
    }
    // The negotiation table's rows.
    if trading {
        let partner = partner.min(31);
        for (column, x0, x1, list, scroll) in [(0u32, 0x13C, 0x205, 0xF80, 0xECC), (1, 0x206, 0x2CD, 0x1180, 0xED0)] {
            if column == 0 && !ai {
                continue;
            }
            let count = mem.i32(form + list + 16 * partner + 4)?;
            let scroll = mem.i32(form + scroll)?;
            let mut place = 0;
            // Inside value 7's half-open region as well as the inclusive test.
            while 0x1CC + place * lh < 0x21C {
                let row = place + scroll;
                if (0..count).contains(&row) {
                    let top = 0x1CC + place * lh;
                    let r = free([x0, top, x1 - x0 + 1, lh.min(0x21C - top)]);
                    if !empty(r) {
                        out.push((CELL_TABLE, column << 16 | row as u32, r));
                    }
                }
                place += 1;
            }
        }
    }
    Some(out)
}

/// The city governor's painted controls (cell kind, index, rect in the
/// form's pixels): its two page tabs, OK and Cancel.
pub fn governor(mem: &dyn Mem, form: u32) -> Option<Vec<(u32, u32, [i32; 4])>> {
    if mem.u32(form)? != GOVERNOR {
        return None;
    }
    Some(G_CELLS.iter().zip(0..).filter_map(|(&(cell, index), i)| Some((cell, index, inclusive(mem, form + G_RECTS + 16 * i)?))).collect())
}

pub(super) fn controls(mem: &dyn Mem, form: &Form, out: &mut Vec<Control>) {
    let push = |r: [i32; 4], kind: ControlKind, cell: u32, index: u32, out: &mut Vec<Control>| {
        let rect = form.on_screen(r);
        if !empty(rect) {
            out.push(Control { rect, kind, id: cell_id(form.addr, cell, index) });
        }
    };
    match form.vtable {
        MILITARY => {
            if let Some((tabs, names)) = military(mem, form.addr) {
                for (v, r) in tabs {
                    push(r, ControlKind::Button, CELL_TAB, v, out);
                }
                for (city, r) in names {
                    push(r, ControlKind::ListItem, CELL_CITY, city, out);
                }
            }
        }
        WONDERS => {
            if let Some((boxes, scrolls)) = wonders(mem, form.addr) {
                let rows: Vec<[i32; 4]> = boxes.iter().map(|(_, r)| *r).collect();
                for (entry, r) in boxes {
                    push(r, ControlKind::ListItem, CELL_BUILDING, entry, out);
                }
                if scrolls {
                    // A wheel notch moves a row: the boxes' row pitch.
                    let pitch = match (rect_at(mem, form.addr + W_BOXES), rect_at(mem, form.addr + W_BOXES + 32)) {
                        (Some(a), Some(b)) if b[1] > a[1] => b[1] - a[1],
                        _ => 0,
                    };
                    let all: Vec<[i32; 4]> = (0..6).filter_map(|i| inclusive(mem, form.addr + W_BOXES + 16 * i)).collect();
                    if let (Some(l), Some(t)) = (all.iter().map(|r| r[0]).min(), all.iter().map(|r| r[1]).min()) {
                        let r = all.iter().map(|r| r[0] + r[2]).max().unwrap_or(l);
                        let b = all.iter().map(|r| r[1] + r[3]).max().unwrap_or(t);
                        let scroll = form.on_screen([l, t, r - l, b - t]);
                        if !empty(scroll) && !rows.is_empty() {
                            out.push(Control { rect: scroll, kind: ControlKind::List { scroll, step: pitch }, id: cell_id(form.addr, CELL_LIST, 0) });
                        }
                    }
                }
            }
        }
        GOVERNOR => {
            for (cell, index, r) in governor(mem, form.addr).unwrap_or_default() {
                push(r, ControlKind::Button, cell, index, out);
            }
        }
        PALACE | SPACESHIP => {
            for (colour, r) in masked(mem, form.addr).unwrap_or_default() {
                push(r, ControlKind::Button, CELL_MASK, colour as u32, out);
            }
        }
        DIPLOMACY => {
            for (cell, index, r) in diplomacy(mem, form.addr).unwrap_or_default() {
                let kind = if matches!(cell, CELL_OPTION | CELL_TREE | CELL_TABLE) { ControlKind::ListItem } else { ControlKind::Button };
                push(r, kind, cell, index, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "screens/tests.rs"]
mod tests;
