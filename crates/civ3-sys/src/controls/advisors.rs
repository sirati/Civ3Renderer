//! Rows and cells that the advisor screens draw themselves and test in
//! their own click code, rather than through child controls. Geometry comes
//! from the form's own rectangle fields and region tables and from the
//! comparison immediates in the click handlers, read from the loaded code,
//! so scrolling, the number of cities and the shown page are the game's.
//!
//! Domestic advisor (`Advisor_Internal_Form`, vtable 0x66C270): left click
//! 0x51F850, right click 0x51FCF0.
//! - City names: the region table at +0x4020 (the layout of the form's own
//!   table at +0x328), one fixed entry per visible row with the row as its
//!   value; the city is `City_ID_List[start + row]` (+0x4080, start +0x401C,
//!   count +0x401E). Left opens the city, right its culture.
//! - Production: right click at x 0x32B..0x39C, y top+1..top+0x22 of the
//!   row's citizen rectangle (+0x3E24 + 16·row) opens the quick build
//!   chooser 0x4E7A70, a native `Context_Menu` already published as menu rows.
//! - Citizens: 0x4BECE0 inside the same rectangle (see [`citizen_slots`]).
//! - Government and draft buttons: rectangles +0x44A0 and +0x44B0; the
//!   draft button only with an advance flagged 0x20 known (0x561430).
//! - Column headers: 0x521680 sorts by the column clicked.
//! - Speech box of every advisor: 0x49E050 advances its message.
//!
//! Cultural advisor (`Advisor_Culture_Form`, vtable 0x66BBC0): left click
//! 0x4FA500, right 0x4FB920, wheel 0x4FBD10, all on the region table at
//! +0x5BD4. Values 0..7 are city rows (city `Cities_ID[start + value]`,
//! start +0x2948, count +0x294C, ids +0x2950): left selects the city,
//! right opens it in the domestic advisor. Values 8..14 are the selected
//! city's culture buildings (`int[+0x313C + 4·value]`, -1 empty): right
//! opens their Civilopedia entry. The wheel scrolls the cities left of
//! x 0x2D2 (up to +0x3178) and the buildings right of it (up to +0x317C).
//!
//! Science advisor (vtable 0x66F420): left 0x5A9090 and right 0x5A9680 find
//! the advance with 0x5A8EF0: each advance shown on the page (24-byte
//! entries at `[+0x33E4]`, byte 0 zero) at its map place (`Advance` +0x50,
//! +0x54), inside its box sprite for its state and the page's era (44-byte
//! records at +0x28E8 + 44·(16·state + `[+0x12E4]`): sprite, width
//! +0x14, height +0x18, inclusive), where the box art is not colour 255.
//!
//! Foreign advisor (vtable 0x66C930): left click 0x532120 tests its three
//! page tabs (+0x3810, +0x3800, +0x3820, inclusive) and the eight leader
//! places (+0x3830 + 16·k, inclusive) of `Report_Civ_List[k]` (+0x5D8, -1
//! none) that are the player or in contact with the player
//! (`Leader.Contacts[civ] & 1`), toggling the civ's selection.

use super::{Control, ControlKind, Form, Mem, empty, part_id, rect_at};

/// Native cells tested by a form's click code: the form above, the cell
/// kind in bits 20..27, its index below.
pub(crate) const TAG_CELL: u32 = 0x7000_0000;
pub const CELL_CITY: u32 = 0;
pub const CELL_PRODUCTION: u32 = 1;
pub const CELL_CITIZEN: u32 = 2;
pub const CELL_BUTTON: u32 = 3;
pub const CELL_HEADER: u32 = 4;
pub const CELL_SPEECH: u32 = 5;
pub const CELL_LIST: u32 = 6;
pub const CELL_BUILDING: u32 = 7;
pub const CELL_TECH: u32 = 8;
pub const CELL_CIV: u32 = 9;
pub const CELL_TAB: u32 = 10;

pub fn cell_id(form: u32, kind: u32, index: u32) -> u64 {
    part_id(form, TAG_CELL | (kind & 0xFF) << 20, index & 0xF_FFFF)
}

/// The cell kind of a control id, if it is a native cell.
pub fn cell_kind(id: u64) -> Option<u32> {
    (id >> 32 != 0 && id as u32 & 0xF000_0000 == TAG_CELL).then_some((id as u32 >> 20) & 0xFF)
}

pub const DOMESTIC: u32 = 0x66_C270;
pub const CULTURE: u32 = 0x66_BBC0;
pub const SCIENCE: u32 = 0x66_F420;
pub const FOREIGN: u32 = 0x66_C930;
/// The six advisors whose left click starts with the speech box test.
pub const ADVISORS: [u32; 6] = [DOMESTIC, 0x66_F750, 0x66_DF50, FOREIGN, CULTURE, SCIENCE];

const D_CITIZEN_RECTS: u32 = 0x3E24;
const D_START: u32 = 0x401C;
const D_COUNT: u32 = 0x401E;
const D_REGIONS: u32 = 0x4020;
const D_CITY_IDS: u32 = 0x4080;
const D_GOVERNMENT: u32 = 0x44A0;
const D_DRAFT: u32 = 0x44B0;
const D_ROWS: u32 = 9;
const D_MAX_CITIES: i32 = 512;

/// Region table object: 32-byte entries (RECT at +4, value +0x18), count.
const REGION_ENTRIES: u32 = 0x50;
const REGION_COUNT: u32 = 0x58;

/// The game's cities (`Cities`): items (8 bytes, `City* + 0x1C` at +4),
/// last index.
const CITY_ITEMS: u32 = 0xA5_2E2C;
const CITY_LAST: u32 = 0xA5_2E38;
const CITY_POPULATION: u32 = 0x138;
const CITY_CITIZENS: u32 = 0xDC;
const CITIZEN_MOOD: u32 = 0x128;
/// The head width the citizen test packs with (0x4BED25).
const HEAD_WIDTH: u32 = 0x66_8F60;

const PLAYER: u32 = 0x9F_D47C;
const LEADERS: u32 = 0xA5_2E58;
const ADVANCE_COUNT: u32 = 0x9C_3D7C;
const ADVANCES: u32 = 0x9C_72E0;
const ADVANCES_KNOWN: u32 = 0xA5_2B0C;
const DRAFT_FLAG: u32 = 0x20;

/// One comparison immediate of a click handler: `bytes` must start the
/// instruction at `at`; then a 32-bit or sign-extended 8-bit immediate.
fn immediate(mem: &dyn Mem, at: u32, opcode: &[u8], wide: bool) -> Option<i32> {
    for (i, b) in opcode.iter().enumerate() {
        if mem.u8(at + i as u32)? != *b {
            return None;
        }
    }
    let imm = at + opcode.len() as u32;
    if wide { mem.i32(imm) } else { mem.u8(imm).map(|v| v as i8 as i32) }
}

/// The production cell's comparisons in 0x51FCF0: x > lo, x < hi, and the
/// row height above its top (y > top, y < top + h).
fn production_bounds(mem: &dyn Mem) -> Option<(i32, i32, i32)> {
    Some((
        immediate(mem, 0x51_FD79, &[0x81, 0xFD], true)?,
        immediate(mem, 0x51_FD81, &[0x81, 0xFD], true)?,
        immediate(mem, 0x51_FD97, &[0x83, 0xC0], false)?,
    ))
}

/// The speech box test of 0x49E050 (inclusive): x0, x1, y0, y1.
fn speech_bounds(mem: &dyn Mem) -> Option<[i32; 4]> {
    Some([
        immediate(mem, 0x49_E064, &[0x3D], true)?,
        immediate(mem, 0x49_E06B, &[0x3D], true)?,
        immediate(mem, 0x49_E076, &[0x83, 0xF8], false)?,
        immediate(mem, 0x49_E07B, &[0x3D], true)?,
    ])
}

/// 0x521680 starts by testing the header band (y 0xF5 < y < 0x122).
const HEADER_CODE: [u8; 10] = [0x8B, 0x44, 0x24, 0x08, 0x56, 0x3D, 0xF5, 0x00, 0x00, 0x00];
/// Its column ranges (exclusive), in the order it tests them.
pub const HEADER_COLUMNS: [(i32, i32); 10] =
    [(0x60, 0xF1), (0xEA, 0x117), (0x114, 0x141), (0x145, 0x172), (0x18F, 0x1B5), (0x1B9, 0x1DF), (0x1E5, 0x20B), (0x210, 0x236), (0x290, 0x335), (0x352, 0x3CA)];
const HEADER_BAND: (i32, i32) = (0xF5, 0x122);

/// Header cells in the form's pixels: earlier ranges win where they overlap.
pub fn header_cells() -> Vec<[i32; 4]> {
    let mut out: Vec<[i32; 4]> = Vec::new();
    let (top, bottom) = (HEADER_BAND.0 + 1, HEADER_BAND.1);
    for &(lo, hi) in &HEADER_COLUMNS {
        let mut left = lo + 1;
        if let Some(prev) = out.last() {
            left = left.max(prev[0] + prev[2]);
        }
        if hi > left {
            out.push([left, top, hi - left, bottom - top]);
        }
    }
    out
}

/// The city at `id` of the game's city table (`City*`), as the handlers
/// look it up.
fn city(mem: &dyn Mem, id: i32) -> Option<u32> {
    let items = mem.u32(CITY_ITEMS).filter(|&p| p != 0)?;
    if id < 0 || id > mem.i32(CITY_LAST)? {
        return None;
    }
    let p = mem.u32(items.checked_add(8 * id as u32 + 4)?)?;
    (p != 0).then(|| p.wrapping_sub(0x1C)).filter(|&c| c != 0)
}

/// Where 0x4BECE0 finds citizen `j` of a city in `rect` (`l, t, r, b`,
/// inclusive): `size + 4` slots of the head width, packed into the width;
/// citizens by mood 0..5, one empty slot after each mood present. Each
/// citizen's slot rectangle (x, y, w, h) with its index in the list.
pub fn citizen_slots([l, t, r, b]: [i32; 4], size: i32, head: i32, moods: &[i32]) -> Vec<(usize, [i32; 4])> {
    if r < l || b < t {
        return vec![];
    }
    let width = r - l;
    let n = size.saturating_add(4);
    let mut step = head;
    if n > 1 && head.saturating_mul(n) > width {
        step = ((width - head) / (n - 1)).clamp(1, head.max(1));
    }
    let step = step.max(1);
    let mut out = Vec::new();
    let mut slot = 0;
    for mood in 0..5 {
        let mut any = false;
        for (j, _) in moods.iter().enumerate().filter(|(_, m)| **m == mood) {
            any = true;
            if slot < n {
                let x0 = l + slot * step;
                let x1 = (x0 + step - 1).min(r);
                if x1 >= x0 {
                    out.push((j, [x0, t, x1 - x0 + 1, b - t + 1]));
                }
            }
            slot += 1;
        }
        if any {
            slot += 1;
        }
    }
    out
}

fn moods(mem: &dyn Mem, city: u32) -> Vec<i32> {
    let list = city + CITY_CITIZENS;
    let (Some(items), Some(last)) = (mem.u32(list + 4), mem.i32(list + 0x10)) else { return vec![] };
    if items == 0 || last < 0 {
        return vec![];
    }
    (0..=last.min(255))
        .map(|j| {
            let p = mem.u32(items + 8 * j as u32 + 4).unwrap_or(0);
            if p == 0 { -1 } else { mem.i32(p.wrapping_sub(0x1C) + CITIZEN_MOOD).unwrap_or(-1) }
        })
        .collect()
}

fn knows_flagged_advance(mem: &dyn Mem, flag: u32) -> bool {
    let probe = || -> Option<bool> {
        let player = mem.u32(PLAYER)?;
        if player >= 32 {
            return Some(false);
        }
        let civ = mem.u32(LEADERS + crate::layout::LEADER_SIZE * player + 0x1C)? & 31;
        let (n, advances, known) = (mem.i32(ADVANCE_COUNT)?, mem.u32(ADVANCES)?, mem.u32(ADVANCES_KNOWN)?);
        if advances == 0 || known == 0 {
            return Some(false);
        }
        for i in 0..n.clamp(0, 1024) as u32 {
            if mem.u32(advances + 0x68 + 0x74 * i)? & flag == flag && mem.u32(known + 4 * i)? & (1 << civ) != 0 {
                return Some(true);
            }
        }
        Some(false)
    };
    probe().unwrap_or(false)
}

/// A visible row of the domestic advisor, in the form's pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomesticRow {
    pub row: u32,
    /// Its place in the city list (start + row).
    pub index: u32,
    pub city: u32,
    /// The city name's region, if the table has the row.
    pub name: Option<[i32; 4]>,
    pub production: Option<[i32; 4]>,
    /// The citizens (index in the city's list) and their slots.
    pub citizens: Vec<(usize, [i32; 4])>,
}

/// The domestic advisor's parts, in the form's pixels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Domestic {
    pub rows: Vec<DomesticRow>,
    pub government: Option<[i32; 4]>,
    pub draft: Option<[i32; 4]>,
    /// More cities than rows: the list scrolls with the wheel (0x5228D0).
    pub scrolls: bool,
}

fn short(mem: &dyn Mem, at: u32) -> Option<i32> {
    Some(i16::from_le_bytes([mem.u8(at)?, mem.u8(at + 1)?]) as i32)
}

fn inclusive(mem: &dyn Mem, at: u32) -> Option<[i32; 4]> {
    let [l, t, r, b] = rect_at(mem, at)?;
    (r >= l && b >= t).then(|| [l, t, r - l + 1, b - t + 1])
}

pub fn domestic(mem: &dyn Mem, form: u32) -> Option<Domestic> {
    if mem.u32(form)? != DOMESTIC {
        return None;
    }
    let start = short(mem, form + D_START)?;
    let count = short(mem, form + D_COUNT)?;
    if !(0..=D_MAX_CITIES).contains(&count) || start < 0 {
        return None;
    }
    // The name regions by their value (the row).
    let mut names = [None; D_ROWS as usize];
    let n = mem.u32(form + D_REGIONS + REGION_COUNT).unwrap_or(0).min(64);
    if let Some(base) = mem.u32(form + D_REGIONS + REGION_ENTRIES).filter(|&b| b != 0) {
        for i in 0..n {
            let entry = base + 32 * i;
            let (Some([l, t, r, b]), Some(row)) = (rect_at(mem, entry + 4), mem.u32(entry + 0x18)) else { continue };
            if row < D_ROWS && r > l && b > t && names[row as usize].is_none() {
                names[row as usize] = Some([l, t, r - l, b - t]);
            }
        }
    }
    let production = production_bounds(mem);
    let head = mem.i32(HEAD_WIDTH).filter(|h| (1..=256).contains(h));
    let mut rows = Vec::new();
    for row in 0..D_ROWS {
        let index = start + row as i32;
        if index >= count || index >= D_MAX_CITIES {
            break;
        }
        let id = short(mem, form + D_CITY_IDS + 2 * index as u32)?;
        let Some(city) = city(mem, id) else { continue };
        let Some(rect @ [l, t, r, b]) = rect_at(mem, form + D_CITIZEN_RECTS + 16 * row) else { continue };
        let citizens = match head {
            Some(head) if r >= l && b >= t => {
                let size = mem.i32(city + CITY_POPULATION).unwrap_or(0).clamp(0, 255);
                citizen_slots(rect, size, head, &moods(mem, city))
            }
            _ => vec![],
        };
        // The citizen test runs first: a packed row's heads can reach into
        // the production column, which starts after them.
        let heads_end = citizens.iter().map(|(_, s)| s[0] + s[2]).max().unwrap_or(i32::MIN);
        let production = production
            .map(|(lo, hi, h)| {
                let left = (lo + 1).max(heads_end);
                [left, t + 1, hi - left, h - 1]
            })
            .filter(|r| !empty(*r));
        rows.push(DomesticRow { row, index: index as u32, city, name: names[row as usize], production, citizens });
    }
    let mut government = inclusive(mem, form + D_GOVERNMENT);
    // The speech box (tested first) takes what overlaps the draft
    // button's right part.
    let mut draft = inclusive(mem, form + D_DRAFT).filter(|_| knows_flagged_advance(mem, DRAFT_FLAG)).map(|[x, y, w, h]| {
        let w = match speech_bounds(mem) {
            Some([x0, _, y0, y1]) if y < y1 + 1 && y + h > y0 && x + w > x0 => w.min(x0 - x),
            _ => w,
        };
        [x, y, w, h]
    });
    // A row both buttons include runs both: neither target keeps it.
    if let (Some(g), Some(d)) = (government.as_mut(), draft.as_mut()) {
        if g[1] < d[1] && g[1] + g[3] > d[1] && g[0] < d[0] + d[2] && g[0] + g[2] > d[0] {
            let shared = g[1] + g[3] - d[1];
            g[3] -= shared;
            d[1] += shared;
            d[3] -= shared;
        }
    }
    Some(Domestic {
        rows,
        government: government.filter(|r| !empty(*r)),
        draft: draft.filter(|r| !empty(*r)),
        scrolls: count > D_ROWS as i32,
    })
}

const C_START: u32 = 0x2948;
const C_COUNT: u32 = 0x294C;
const C_IDS: u32 = 0x2950;
const C_BUILDINGS: u32 = 0x313C;
const C_CITY_LAST_START: u32 = 0x3178;
const C_BUILDING_LAST_START: u32 = 0x317C;
const C_REGIONS: u32 = 0x5BD4;
const C_CITY_ROWS: u32 = 8;
const C_BUILDING_ROWS: u32 = 7;
/// The wheel scrolls the buildings at and right of this x (0x4FBD1C).
const C_WHEEL_SPLIT: i32 = 0x2D2;

/// A region table's entries (value, x, y, w, h), in the form's pixels.
fn regions(mem: &dyn Mem, table: u32) -> Vec<(u32, [i32; 4])> {
    let n = mem.u32(table + REGION_COUNT).unwrap_or(0).min(64);
    let Some(base) = mem.u32(table + REGION_ENTRIES).filter(|&b| b != 0) else { return vec![] };
    (0..n)
        .filter_map(|i| {
            let [l, t, r, b] = rect_at(mem, base + 32 * i + 4)?;
            (r > l && b > t).then_some((mem.u32(base + 32 * i + 0x18)?, [l, t, r - l, b - t]))
        })
        .collect()
}

/// The cultural advisor's parts, in the form's pixels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Culture {
    /// City rows: (list index, region).
    pub cities: Vec<(u32, [i32; 4])>,
    /// Building rows: (row, region).
    pub buildings: Vec<(u32, [i32; 4])>,
    pub cities_scroll: bool,
    pub buildings_scroll: bool,
}

pub fn culture(mem: &dyn Mem, form: u32) -> Option<Culture> {
    if mem.u32(form)? != CULTURE {
        return None;
    }
    let (start, count) = (mem.i32(form + C_START)?, mem.i32(form + C_COUNT)?);
    if !(0..=D_MAX_CITIES).contains(&count) || start < 0 {
        return None;
    }
    let mut out = Culture {
        cities_scroll: mem.i32(form + C_CITY_LAST_START).is_some_and(|m| m > 0),
        buildings_scroll: mem.i32(form + C_BUILDING_LAST_START).is_some_and(|m| m > 0),
        ..Default::default()
    };
    for (value, rect) in regions(mem, form + C_REGIONS) {
        if value < C_CITY_ROWS {
            let index = start + value as i32;
            let valid = index < count && mem.i32(form + C_IDS + 4 * index as u32).is_some_and(|id| city(mem, id).is_some());
            if valid && !out.cities.iter().any(|(i, _)| *i == index as u32) {
                out.cities.push((index as u32, rect));
            }
        } else if value < C_CITY_ROWS + C_BUILDING_ROWS {
            if mem.i32(form + C_BUILDINGS + 4 * value).is_some_and(|b| b != -1) && !out.buildings.iter().any(|(j, _)| *j == value - C_CITY_ROWS) {
                out.buildings.push((value - C_CITY_ROWS, rect));
            }
        }
    }
    Some(out)
}

const S_ERA: u32 = 0x12E4;
const S_BOXES: u32 = 0x28E8;
const S_SHOWN: u32 = 0x33E4;

/// The science advisor's advance boxes (advance, x, y, w, h) on its page.
pub fn science(mem: &dyn Mem, form: u32) -> Option<Vec<(u32, [i32; 4])>> {
    Some(science_boxes(mem, form)?.into_iter().map(|(advance, rect, _)| (advance, rect)).collect())
}

/// [`science`] with each box's `Sprite` record (`+4`: its jgl sprite).
fn science_boxes(mem: &dyn Mem, form: u32) -> Option<Vec<(u32, [i32; 4], u32)>> {
    if mem.u32(form)? != SCIENCE {
        return None;
    }
    let (n, advances, shown, era) = (mem.i32(ADVANCE_COUNT)?, mem.u32(ADVANCES)?, mem.u32(form + S_SHOWN)?, mem.i32(form + S_ERA)?);
    if advances == 0 || shown == 0 || !(0..16).contains(&era) {
        return None;
    }
    let mut out = Vec::new();
    for i in 0..n.clamp(0, 1024) as u32 {
        let entry = shown + 24 * i;
        if mem.u8(entry) != Some(0) {
            continue;
        }
        let Some(state) = mem.i32(entry + 4) else { continue };
        let kind = match state {
            ..=1 => 0,
            2..=3 => 1,
            4 => 3,
            _ => 2,
        };
        let record = form + S_BOXES + 44 * (16 * kind + era as u32);
        let (Some(sprite), Some(w), Some(h)) = (mem.u32(record), mem.i32(record + 0x14), mem.i32(record + 0x18)) else { continue };
        let (Some(x), Some(y)) = (mem.i32(advances + 0x74 * i + 0x50), mem.i32(advances + 0x74 * i + 0x54)) else { continue };
        if sprite != 0 && (0..=1024).contains(&w) && (0..=768).contains(&h) {
            out.push((i, [x, y, w + 1, h + 1], record));
        }
    }
    Some(out)
}

/// The science advisor's `advance_at(x, y)` (0x5A8EF0, thiscall, `ret 8`:
/// the advance whose box takes a point of its page, -1 for none) and its
/// first instructions. Its left and right clicks and its hover ask it.
pub fn science_hit_site(build: crate::Build) -> Option<(u32, &'static [u8])> {
    match build {
        crate::Build::PcGames => Some((0x5A_8EF0, &[0x83, 0xEC, 0x10, 0x8B, 0x15, 0x7C, 0x3D, 0x9C, 0x00])),
        crate::Build::Gog | crate::Build::Steam => None,
    }
}

/// The advance whose box takes point (x, y) of the science advisor's page,
/// decided by us as 0x5A8EF0 decides it, in the game's order: the first
/// shown advance whose box rectangle holds the point (inclusive), on the
/// box's jgl sprite. The record (the box `Sprite` from its `+4`) places
/// that sprite at `+0x1C`/`+0x20` in the box, trimmed of its transparent
/// margins, with its size at the jgl sprite's `+0x30`/`+0x34`. The game
/// then read the sprite's pixel there and let colour 255 through; that is
/// sprite pixel work this replaces (the box art's corners take the click).
pub fn science_advance_at(mem: &dyn Mem, form: u32, p: (i32, i32)) -> Option<u32> {
    ScienceBoxes::read(mem, form)?.at(p)
}

/// The science advisor's advance boxes as [`science_advance_at`] tests
/// them, read once: (advance, box rectangle, its sprite's rectangle), in
/// the game's order, in the page's pixels.
pub struct ScienceBoxes(Vec<(u32, [i32; 4], [i32; 4])>);

impl ScienceBoxes {
    pub fn read(mem: &dyn Mem, form: u32) -> Option<ScienceBoxes> {
        let boxes = science_boxes(mem, form)?;
        Some(ScienceBoxes(boxes.into_iter().filter_map(|(advance, [bx, by, w, h], record)| {
            let sprite = mem.u32(record)?;
            let (ox, oy) = (mem.i32(record + 0x1C)?, mem.i32(record + 0x20)?);
            let (sw, sh) = (mem.i32(sprite + 0x30)?, mem.i32(sprite + 0x34)?);
            Some((advance, [bx, by, w, h], [bx + ox, by + oy, sw, sh]))
        }).collect()))
    }

    /// The advance whose box takes point `p` (see [`science_advance_at`]).
    pub fn at(&self, (x, y): (i32, i32)) -> Option<u32> {
        let inside = |[rx, ry, rw, rh]: [i32; 4]| x >= rx && y >= ry && x < rx + rw && y < ry + rh;
        self.0.iter().find(|&&(_, b, s)| inside(b) && inside(s)).map(|&(advance, _, _)| advance)
    }

    /// The point of `advance`'s box nearest `near` that the advisor gives
    /// to it ([`ScienceBoxes::at`]): an earlier advance's box over part of
    /// it takes that part (the page's pixels).
    pub fn spot(&self, advance: u32, near: (i32, i32)) -> Option<(i32, i32)> {
        let &(_, [bx, by, w, h], _) = self.0.iter().find(|b| b.0 == advance)?;
        let d = |(x, y): (i32, i32)| (x - near.0).pow(2) + (y - near.1).pow(2);
        (by..by + h).flat_map(|y| (bx..bx + w).map(move |x| (x, y)))
            .filter(|&p| self.at(p) == Some(advance))
            .min_by_key(|&p| d(p))
    }
}

/// The science advisor's advance cells among `cells` (control id, its
/// rectangle and the spot the pointer takes on it, the screen's pixels)
/// whose spot the advisor gives to another advance (an earlier one's box
/// over it): each such cell's index and its spot moved to the nearest
/// point that is its advance's ([`ScienceBoxes::spot`]).
pub fn science_spots(mem: &dyn Mem, cells: &[(u64, [i32; 4], (i32, i32))]) -> Vec<(usize, (i32, i32))> {
    let mut boxes: Vec<(u32, Option<((i32, i32), ScienceBoxes)>)> = Vec::new();
    let mut out = Vec::new();
    for (k, &(id, _, spot)) in cells.iter().enumerate() {
        if cell_kind(id) != Some(CELL_TECH) {
            continue;
        }
        let (form, advance) = ((id >> 32) as u32, id as u32 & 0xF_FFFF);
        if !boxes.iter().any(|(f, _)| *f == form) {
            let read = super::form_rect(mem, form).zip(ScienceBoxes::read(mem, form)).map(|(r, b)| ((r[0], r[1]), b));
            boxes.push((form, read));
        }
        let Some(((ox, oy), b)) = boxes.iter().find(|(f, _)| *f == form).and_then(|(_, b)| b.as_ref()) else { continue };
        let page = (spot.0 - ox, spot.1 - oy);
        if b.at(page) == Some(advance) {
            continue;
        }
        if let Some((x, y)) = b.spot(advance, page) {
            out.push((k, (x + ox, y + oy)));
        }
    }
    out
}

const F_TABS: [u32; 3] = [0x3810, 0x3800, 0x3820];
const F_LEADERS: u32 = 0x3830;
const F_CIVS: u32 = 0x5D8;

/// The foreign advisor's page tabs (k, rect) and leaders (civ, rect).
pub fn foreign(mem: &dyn Mem, form: u32) -> Option<(Vec<(u32, [i32; 4])>, Vec<(u32, [i32; 4])>)> {
    if mem.u32(form)? != FOREIGN {
        return None;
    }
    let tabs = F_TABS.iter().enumerate().filter_map(|(k, &at)| Some((k as u32, inclusive(mem, form + at)?))).collect();
    let player = mem.u32(PLAYER)?;
    let contacts = LEADERS.checked_add(crate::layout::LEADER_SIZE.checked_mul(player)?)? + crate::layout::LEADER_CONTACTS;
    let mut leaders = Vec::new();
    for k in 0..8 {
        let Some(civ) = mem.i32(form + F_CIVS + 4 * k).filter(|c| (0..32).contains(c)) else { continue };
        let known = civ as u32 == player || mem.u32(contacts + 4 * civ as u32).is_some_and(|c| c & 1 != 0);
        if let (true, Some(r)) = (known, inclusive(mem, form + F_LEADERS + 16 * k)) {
            leaders.push((civ as u32, r));
        }
    }
    Some((tabs, leaders))
}

/// A wheel list over `rows` (one row a notch), if they are not empty.
fn wheel_list(form: &Form, rows: &[[i32; 4]], id: u64, out: &mut Vec<Control>) {
    let (Some(left), Some(top)) = (rows.iter().map(|r| r[0]).min(), rows.iter().map(|r| r[1]).min()) else { return };
    let right = rows.iter().map(|r| r[0] + r[2]).max().unwrap_or(left);
    let bottom = rows.iter().map(|r| r[1] + r[3]).max().unwrap_or(top);
    let mut tops: Vec<i32> = rows.iter().map(|r| r[1]).collect();
    tops.sort_unstable();
    tops.dedup();
    let step = tops.windows(2).map(|w| w[1] - w[0]).min().unwrap_or(0);
    let scroll = form.on_screen([left, top, right - left, bottom - top]);
    if !empty(scroll) {
        out.push(Control { rect: scroll, kind: ControlKind::List { scroll, step }, id });
    }
}

/// The draft button's native rectangle (inclusive), while it is active.
fn d_draft_bounds(mem: &dyn Mem, form: u32) -> Option<[i32; 4]> {
    rect_at(mem, form + D_DRAFT).filter(|_| knows_flagged_advance(mem, DRAFT_FLAG))
}

pub(super) fn controls(mem: &dyn Mem, form: &Form, out: &mut Vec<Control>) {
    if !ADVISORS.contains(&form.vtable) {
        return;
    }
    let push = |r: [i32; 4], kind: ControlKind, cell: u32, index: u32, out: &mut Vec<Control>| {
        let rect = form.on_screen(r);
        if !empty(rect) {
            out.push(Control { rect, kind, id: cell_id(form.addr, cell, index) });
        }
    };
    let domestic = (form.vtable == DOMESTIC).then(|| domestic(mem, form.addr)).flatten();
    if let Some(d) = &domestic {
        for row in &d.rows {
            if let Some(name) = row.name {
                push(name, ControlKind::ListItem, CELL_CITY, row.index, out);
            }
            if let Some(p) = row.production {
                push(p, ControlKind::Button, CELL_PRODUCTION, row.index, out);
            }
            for &(j, slot) in &row.citizens {
                push(slot, ControlKind::Button, CELL_CITIZEN, row.index << 8 | (j as u32 & 0xFF), out);
            }
        }
        for (k, r) in [d.government, d.draft].into_iter().enumerate() {
            if let Some(r) = r {
                push(r, ControlKind::Button, CELL_BUTTON, k as u32, out);
            }
        }
        if (0..HEADER_CODE.len()).all(|i| mem.u8(0x52_1680 + i as u32) == Some(HEADER_CODE[i])) {
            for (k, r) in header_cells().into_iter().enumerate() {
                push(r, ControlKind::Button, CELL_HEADER, k as u32, out);
            }
        }
        // The wheel scrolls the rows (Advisor_GUI forwards it, 0x49E870).
        if d.scrolls {
            let rows: Vec<[i32; 4]> = d.rows.iter().flat_map(|r| r.name.into_iter().chain(r.production)).collect();
            if let (Some(first), Some(last)) = (rows.iter().map(|r| r[1]).min(), rows.iter().map(|r| r[1] + r[3]).max()) {
                // The native table includes statistics to the right of
                // production and between its clickable name/citizen cells.
                // Use the same column extents as the native header, rather
                // than the union of clickable cells.
                let left = HEADER_COLUMNS.iter().map(|&(lo, _)| lo).min().unwrap_or(0);
                let right = HEADER_COLUMNS.iter().map(|&(_, hi)| hi).max().unwrap_or(0);
                // One row a notch: the citizen rectangles' spacing.
                let step = match (rect_at(mem, form.addr + D_CITIZEN_RECTS), rect_at(mem, form.addr + D_CITIZEN_RECTS + 16)) {
                    (Some(a), Some(b)) if b[1] > a[1] => b[1] - a[1],
                    _ => 0,
                };
                let scroll = form.on_screen([left, first, right - left, last - first]);
                if !empty(scroll) {
                    out.push(Control { rect: scroll, kind: ControlKind::List { scroll, step }, id: cell_id(form.addr, CELL_LIST, 0) });
                }
            }
        }
    }
    if let Some(c) = (form.vtable == CULTURE).then(|| culture(mem, form.addr)).flatten() {
        for &(index, r) in &c.cities {
            push(r, ControlKind::ListItem, CELL_CITY, index, out);
        }
        for &(row, r) in &c.buildings {
            push(r, ControlKind::ListItem, CELL_BUILDING, row, out);
        }
        // The wheel scrolls the list on its side of x 0x2D2.
        let side = |rows: &[(u32, [i32; 4])], right: bool| -> Vec<[i32; 4]> {
            rows.iter().map(|(_, r)| *r).filter(|r| (r[0] >= C_WHEEL_SPLIT) == right && (r[0] + r[2] <= C_WHEEL_SPLIT) != right).collect()
        };
        if c.cities_scroll {
            wheel_list(form, &side(&c.cities, false), cell_id(form.addr, CELL_LIST, 0), out);
        }
        if c.buildings_scroll {
            wheel_list(form, &side(&c.buildings, true), cell_id(form.addr, CELL_LIST, 1), out);
        }
    }
    for (advance, r) in (form.vtable == SCIENCE).then(|| science(mem, form.addr)).flatten().unwrap_or_default() {
        push(r, ControlKind::Button, CELL_TECH, advance, out);
    }
    if let Some((tabs, leaders)) = (form.vtable == FOREIGN).then(|| foreign(mem, form.addr)).flatten() {
        for (k, r) in tabs {
            push(r, ControlKind::Button, CELL_TAB, k, out);
        }
        for (civ, r) in leaders {
            push(r, ControlKind::Button, CELL_CIV, civ, out);
        }
    }
    if let Some([mut x0, x1, y0, y1]) = speech_bounds(mem).filter(|[x0, x1, y0, y1]| x1 >= x0 && y1 >= y0) {
        // A click where the draft button reaches into the box runs both.
        if let Some([l, t, r, b]) = domestic.as_ref().and(d_draft_bounds(mem, form.addr)) {
            if t <= y1 && b >= y0 && l <= x0 && r >= x0 {
                x0 = r + 1;
            }
        }
        if x1 >= x0 {
            push([x0, y0, x1 - x0 + 1, y1 - y0 + 1], ControlKind::Button, CELL_SPEECH, 0, out);
        }
    }
}

#[cfg(test)]
#[path = "advisors/tests.rs"]
mod tests;
