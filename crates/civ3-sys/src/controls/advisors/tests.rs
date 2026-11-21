use super::*;
use crate::controls::{ControlKind, Sites, controls};
use std::collections::HashMap;

#[derive(Default)]
struct Fake(HashMap<u32, u8>);

impl Fake {
    fn put(&mut self, at: u32, v: u32) {
        self.bytes(at, &v.to_le_bytes());
    }
    fn short(&mut self, at: u32, v: i16) {
        self.bytes(at, &v.to_le_bytes());
    }
    fn bytes(&mut self, at: u32, bytes: &[u8]) {
        for (i, b) in bytes.iter().enumerate() {
            self.0.insert(at + i as u32, *b);
        }
    }
    fn rect(&mut self, at: u32, r: [i32; 4]) {
        for (i, v) in r.into_iter().enumerate() {
            self.put(at + 4 * i as u32, v as u32);
        }
    }
}

impl Mem for Fake {
    fn u32(&self, addr: u32) -> Option<u32> {
        let b: Vec<u8> = (0..4).map(|i| self.0.get(&(addr + i)).copied()).collect::<Option<_>>()?;
        Some(u32::from_le_bytes(b.try_into().unwrap()))
    }
    fn u8(&self, addr: u32) -> Option<u8> {
        self.0.get(&addr).copied()
    }
}

/// The loaded game's code and constants this module reads: from the
/// installed executable when present (mapped by its section table), else
/// the same PCGames bytes.
fn load_code(m: &mut Fake) -> bool {
    let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    let ranges: [(u32, usize); 6] = [(0x51_FD79, 0x24), (0x49_E064, 0x20), (0x52_1680, 16), (0x66_8F60, 4), (0x51_F850, 0x4A0), (0x52_1680, 0x400)];
    if let Ok(pe) = std::fs::read(std::path::Path::new(&install).join("Conquests/Civ3Conquests.exe")) {
        let u32_at = |o: usize| u32::from_le_bytes(pe[o..o + 4].try_into().unwrap());
        let nt = u32_at(0x3C) as usize;
        let sections = u16::from_le_bytes(pe[nt + 6..nt + 8].try_into().unwrap()) as usize;
        let table = nt + 24 + u16::from_le_bytes(pe[nt + 20..nt + 22].try_into().unwrap()) as usize;
        let map = |va: u32| -> Option<usize> {
            let rva = va.checked_sub(0x40_0000)?;
            (0..sections).find_map(|s| {
                let h = table + 40 * s;
                let (start, size, raw) = (u32_at(h + 12), u32_at(h + 16), u32_at(h + 20));
                (rva >= start && rva - start < size).then(|| (raw + rva - start) as usize)
            })
        };
        for (va, len) in ranges {
            let o = map(va).expect("mapped");
            m.bytes(va, &pe[o..o + len]);
        }
        return true;
    }
    m.bytes(0x51_FD79, &[0x81, 0xFD, 0x2A, 0x03, 0, 0, 0x7E, 0x42, 0x81, 0xFD, 0x9D, 0x03, 0, 0]);
    m.bytes(0x51_FD97, &[0x83, 0xC0, 0x23]);
    m.bytes(0x49_E064, &[0x3D, 0x2C, 0x03, 0, 0, 0x7C, 0x64, 0x3D, 0xEA, 0x03, 0, 0, 0x7F, 0x5D, 0x8B, 0x44, 0x24, 0x0C, 0x83, 0xF8, 0x75, 0x7C, 0x54, 0x3D, 0xEF, 0, 0, 0]);
    m.bytes(0x52_1680, &HEADER_CODE);
    m.put(0x66_8F60, 0x31);
    false
}

const FORM: u32 = 0xA3_8BD8;
const ORIGIN: (i32, i32) = (448, 156);
const REGIONS: u32 = 0x60_0000;
const CITIES: u32 = 0x61_0000;
const CITY_OBJECTS: u32 = 0x62_0000;
const CITIZENS: u32 = 0x70_0000;

/// A city with citizens of these moods at `CITY_OBJECTS + 0x1000·id`.
fn add_city(m: &mut Fake, id: u32, moods: &[i32]) {
    let city = CITY_OBJECTS + 0x1000 * id;
    m.put(CITIES + 8 * id + 4, city + 0x1C);
    m.put(city + CITY_POPULATION, moods.len() as u32);
    let items = CITIZENS + 0x1000 * id;
    m.put(city + CITY_CITIZENS + 4, items);
    m.put(city + CITY_CITIZENS + 0x10, (moods.len() as i32 - 1) as u32);
    for (j, mood) in moods.iter().enumerate() {
        let citizen = items + 0x800 + 0x200 * j as u32;
        m.put(items + 8 * j as u32 + 4, citizen + 0x1C);
        m.put(citizen + CITIZEN_MOOD, *mood as u32);
    }
}

/// The domestic advisor as its init (0x51F3E0) lays it out, on the screen
/// at the captured 1920x1080 place, listing `cities` from `start`.
fn domestic_fixture(cities: &[&[i32]], start: i16, draft: bool) -> Fake {
    let mut m = Fake::default();
    load_code(&mut m);
    m.put(FORM, DOMESTIC);
    m.put(FORM + 0x9C, KEYED_STATUS);
    m.put(FORM + 0xA0, 1);
    m.put(FORM + 0x130, 0);
    m.rect(FORM + 0x1AC, [ORIGIN.0, ORIGIN.1, ORIGIN.0 + 1024, ORIGIN.1 + 768]);
    m.rect(FORM + 0x1BC, [0; 4]);
    m.rect(FORM + 0x1DC, [0, 0, 1920, 1080]);
    m.put(FORM + 0x22C, 0);
    m.put(FORM + 0x380, 0);
    m.put(FORM + D_REGIONS + REGION_ENTRIES, REGIONS);
    m.put(FORM + D_REGIONS + REGION_COUNT, 9);
    for row in 0..9u32 {
        let y = 0x131 + 0x2D * row as i32;
        m.rect(REGIONS + 32 * row + 4, [0x60, y, 0x60 + 0x91, y + 0x25]);
        m.put(REGIONS + 32 * row + 0x14, 0);
        m.put(REGIONS + 32 * row + 0x18, row);
        m.rect(FORM + D_CITIZEN_RECTS + 16 * row, [0x258, y, 0x366, y + 0x25]);
    }
    m.rect(FORM + D_GOVERNMENT, [0x28A, 0xB7, 0x31C, 0xD1]);
    m.rect(FORM + D_DRAFT, [0x28A, 0xD1, 0x352, 0xE7]);
    // The rate tracks' ends (0x59A8, 0x59B8), outside every cell.
    m.rect(FORM + 0x59A8, [0x233, 0x57, 0x23F, 0x6E]);
    m.rect(FORM + 0x59B8, [0x233, 0x86, 0x23F, 0x9D]);
    m.short(FORM + D_START, start);
    m.short(FORM + D_COUNT, cities.len() as i16);
    for (k, moods) in cities.iter().enumerate() {
        // City ids differ from list places, as the sorted list makes them.
        let id = 3 + 2 * k as u32;
        add_city(&mut m, id, moods);
        m.short(FORM + D_CITY_IDS + 2 * k as u32, id as i16);
    }
    m.put(CITY_ITEMS, CITIES);
    m.put(CITY_LAST, 3 + 2 * cities.len() as u32);
    // Advances: the second has the draft flag; player 1, civ 5.
    m.put(PLAYER, 1);
    m.put(LEADERS + crate::layout::LEADER_SIZE + 0x1C, 5);
    m.put(ADVANCE_COUNT, 2);
    m.put(ADVANCES, 0x71_0000);
    m.put(0x71_0000 + 0x68, 0);
    m.put(0x71_0000 + 0x74 + 0x68, DRAFT_FLAG | 1);
    m.put(ADVANCES_KNOWN, 0x72_0000);
    m.put(0x72_0000, 1 << 5);
    m.put(0x72_0004, if draft { 1 << 5 } else { 1 << 4 });
    // Roots: only the advisor.
    m.put(0x100, FORM);
    m.put(0x200, 1);
    m.put(0x300, 0);
    m.put(0x304, 0);
    m
}
const KEYED_STATUS: u32 = 0x100_0000;

fn sites() -> Sites {
    Sites { roots: 0x100, root_count: 0x200, modal: 0x300, companion: 0x304, main_gui: 0x9F_DB10, advisor: 0x9A_FD58 }
}

/// What the domestic advisor's own click code does at a point of its
/// form, ported from the handlers branch by branch.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Native {
    Speech,
    Government,
    Draft,
    OpenCity(u32),
    Citizen(u32, usize),
    Sort(usize),
    CityCulture(u32),
    Production(u32),
    CitizenMenu(u32, usize),
}

fn city_at(m: &Fake, row: u32) -> Option<u32> {
    let start = short(m, FORM + D_START)?;
    let id = short(m, FORM + D_CITY_IDS + 2 * (start + row as i32) as u32)?;
    city(m, id)
}

/// 0x4BECE0: the citizen (list index) hit in `rect` (inclusive), by index
/// = (x − l) / step, then counting citizens by mood with a gap after each
/// mood present.
fn citizen_hit(m: &Fake, city: u32, rect: [i32; 4], x: i32, y: i32) -> Option<usize> {
    let [l, t, r, b] = rect;
    if x < l || x > r || y > b || y < t {
        return None;
    }
    let mut step = m.i32(HEAD_WIDTH)?;
    let width = r - l;
    let n = m.i32(city + CITY_POPULATION)? + 4;
    if n > 1 && step * n > width {
        let packed = (width - step) / (n - 1);
        step = if step < 1 || packed < 1 { 1 } else { packed.min(step) };
    }
    let mut index = (x - l) / step;
    if index >= n || index < 0 {
        return None;
    }
    let moods = moods(m, city);
    for mood in 0..5 {
        let mut any = false;
        for (j, &cm) in moods.iter().enumerate() {
            if cm == mood {
                let was = index;
                index -= 1;
                any = true;
                if was == 0 {
                    return Some(j);
                }
            }
        }
        if any {
            index -= 1;
        }
    }
    None
}

fn region_row(m: &Fake, x: i32, y: i32) -> Option<u32> {
    // 0x60F170: last entry first, half-open rectangles.
    let base = m.u32(FORM + D_REGIONS + REGION_ENTRIES)?;
    (0..m.u32(FORM + D_REGIONS + REGION_COUNT)?).rev().find_map(|i| {
        let [l, t, r, b] = rect_at(m, base + 32 * i + 4)?;
        (x >= l && x < r && y >= t && y < b).then(|| m.u32(base + 32 * i + 0x18)).flatten()
    })
}

fn inside([l, t, r, b]: [i32; 4], x: i32, y: i32) -> bool {
    x >= l && x <= r && y >= t && y <= b
}

/// 0x51F850 (left button).
fn native_left(m: &Fake, x: i32, y: i32) -> Vec<Native> {
    let mut out = vec![];
    if (0x32C..=0x3EA).contains(&x) && (0x75..=0xEF).contains(&y) {
        out.push(Native::Speech);
    }
    if inside(rect_at(m, FORM + D_GOVERNMENT).unwrap(), x, y) {
        out.push(Native::Government);
    }
    if inside(rect_at(m, FORM + D_DRAFT).unwrap(), x, y) && knows_flagged_advance(m, DRAFT_FLAG) {
        out.push(Native::Draft);
    }
    match region_row(m, x, y) {
        Some(row) => {
            if city_at(m, row).is_some() {
                out.push(Native::OpenCity(row));
            }
        }
        None => {
            for row in 0..9 {
                let Some(city) = city_at(m, row) else { continue };
                if let Some(j) = citizen_hit(m, city, rect_at(m, FORM + D_CITIZEN_RECTS + 16 * row).unwrap(), x, y) {
                    out.push(Native::Citizen(row, j));
                }
            }
        }
    }
    for track in [0x59A8, 0x59B8] {
        if inside(rect_at(m, FORM + track).unwrap(), x, y) {
            return out;
        }
    }
    // 0x521680.
    if y > HEADER_BAND.0 && y < HEADER_BAND.1 {
        if let Some(col) = HEADER_COLUMNS.iter().position(|&(lo, hi)| x > lo && x < hi) {
            out.push(Native::Sort(col));
        }
    }
    out
}

/// 0x51FCF0 (right button).
fn native_right(m: &Fake, x: i32, y: i32) -> Vec<Native> {
    for row in 0..9 {
        let Some(city) = city_at(m, row) else { break };
        let rect = rect_at(m, FORM + D_CITIZEN_RECTS + 16 * row).unwrap();
        if let Some(j) = citizen_hit(m, city, rect, x, y) {
            return vec![Native::CitizenMenu(row, j)];
        }
        if x > 0x32A && x < 0x39D {
            if rect[1] < y && rect[1] + 0x23 > y {
                return vec![Native::Production(row)];
            }
            continue;
        }
        if let Some(row) = region_row(m, x, y) {
            if city_at(m, row).is_some() {
                return vec![Native::CityCulture(row)];
            }
        }
    }
    vec![]
}

/// Every edge pixel of a rectangle and every third one inside it.
fn points([x, y, w, h]: [i32; 4]) -> impl Iterator<Item = (i32, i32)> {
    (y..y + h).flat_map(move |py| (x..x + w).map(move |px| (px, py)))
        .filter(move |&(px, py)| px == x || py == y || px == x + w - 1 || py == y + h - 1 || ((px - x) % 3 == 0 && (py - y) % 3 == 0))
}

/// The edges and a dense grid of every published cell runs exactly the cell's action in
/// the game's own handler logic, for each mouse button.
fn check_every_pixel(m: &Fake) -> usize {
    let found = controls(m, &sites());
    let start = short(m, FORM + D_START).unwrap() as u32;
    let mut cells = 0;
    for c in found.iter().filter(|c| cell_kind(c.id).is_some()) {
        let kind = cell_kind(c.id).unwrap();
        let index = c.id as u32 & 0xF_FFFF;
        let (left, right): (Vec<Native>, Vec<Native>) = match kind {
            CELL_CITY => (vec![Native::OpenCity(index - start)], vec![Native::CityCulture(index - start)]),
            CELL_PRODUCTION => (vec![], vec![Native::Production(index - start)]),
            CELL_CITIZEN => {
                let (row, j) = ((index >> 8) - start, (index & 0xFF) as usize);
                (vec![Native::Citizen(row, j)], vec![Native::CitizenMenu(row, j)])
            }
            CELL_BUTTON => (vec![if index == 0 { Native::Government } else { Native::Draft }], vec![]),
            CELL_HEADER => (vec![Native::Sort(index as usize)], vec![]),
            CELL_SPEECH => (vec![Native::Speech], vec![]),
            CELL_LIST => continue,
            _ => panic!("unknown cell {kind}"),
        };
        cells += 1;
        for (px, py) in points(c.rect) {
            {
                let (lx, ly) = (px - ORIGIN.0, py - ORIGIN.1);
                assert_eq!(native_left(m, lx, ly), left, "left at {lx},{ly} in {c:?}");
                // A right click on the advisor's other parts is not ours.
                if kind != CELL_BUTTON && kind != CELL_HEADER && kind != CELL_SPEECH {
                    assert_eq!(native_right(m, lx, ly), right, "right at {lx},{ly} in {c:?}");
                }
            }
        }
    }
    cells
}

#[test]
fn the_real_f1_screen_cells_hit_their_own_rows_in_the_native_handlers() {
    // The captured F1 screen (linux-ui-live f1-right): Rome, one content
    // citizen, producing Wealth, at 1920x1080 with the advisor at (448, 156).
    let real = load_code(&mut Fake::default());
    let m = domestic_fixture(&[&[1]], 0, false);
    let found = controls(&m, &sites());
    let cell = |kind, index| found.iter().find(|c| c.id == cell_id(FORM, kind, index)).map(|c| c.rect);
    let rome = cell(CELL_CITY, 0).expect("city row");
    // The drawn name "Rome" (559, 485), its head (1055, 459, 36x41), the
    // production icon (1312, 465, 27x26) and text (1343, 479).
    let has = |[x, y, w, h]: [i32; 4], (px, py)| px >= x && py >= y && px < x + w && py < y + h;
    assert!(has(rome, (559, 485)));
    assert_eq!(found.iter().find(|c| c.id == cell_id(FORM, CELL_CITY, 0)).unwrap().kind, ControlKind::ListItem);
    let head = cell(CELL_CITIZEN, 0).expect("citizen");
    assert!(has(head, (1055 + 18, 459 + 20)) && has(head, (1055 + 35, 459 + 37)), "{head:?}");
    let production = cell(CELL_PRODUCTION, 0).expect("production");
    assert!(has(production, (1312, 465)) && has(production, (1343 + 20, 479)), "{production:?}");
    assert_eq!(production, [448 + 0x32B, 156 + 0x131 + 1, 0x39D - 0x32B, 0x22]);
    assert!(cell(CELL_BUTTON, 0).is_some_and(|g| has(g, (1105, 356))), "Government button");
    assert!(cell(CELL_BUTTON, 1).is_none(), "no draft advance known");
    assert!(cell(CELL_SPEECH, 0).is_some_and(|s| has(s, (1417, 388))), "the speech box's More");
    assert!(cell(CELL_HEADER, 0).is_some_and(|s| has(s, (564, 423))), "Cities header");
    assert!(cell(CELL_LIST, 0).is_none(), "one city does not scroll");
    assert!(cell(CELL_CITY, 1).is_none() && cell(CELL_PRODUCTION, 1).is_none(), "empty rows have no cells");
    assert!(check_every_pixel(&m) >= 15);
    eprintln!("checked against the {} executable", if real { "installed" } else { "embedded" });
}

#[test]
fn scrolled_lists_follow_the_start_and_publish_a_wheel_list() {
    let moods: Vec<Vec<i32>> = (0..14).map(|k| (0..(k % 7 + 1)).map(|j| j % 3).collect()).collect();
    let cities: Vec<&[i32]> = moods.iter().map(|v| v.as_slice()).collect();
    let m = domestic_fixture(&cities, 3, true);
    let d = domestic(&m, FORM).unwrap();
    assert_eq!(d.rows.iter().map(|r| r.index).collect::<Vec<_>>(), (3..12).collect::<Vec<_>>());
    assert_eq!(d.rows[0].city, CITY_OBJECTS + 0x1000 * (3 + 2 * 3));
    assert!(d.draft.is_some() && d.scrolls);
    let found = controls(&m, &sites());
    let list = found.iter().find(|c| c.id == cell_id(FORM, CELL_LIST, 0)).unwrap();
    let ControlKind::List { scroll, step } = list.kind else { panic!() };
    assert_eq!(step, 0x2D);
    assert_eq!(scroll[0], ORIGIN.0 + HEADER_COLUMNS.iter().map(|&(lo, _)| lo).min().unwrap());
    assert_eq!(scroll[0] + scroll[2], ORIGIN.0 + HEADER_COLUMNS.iter().map(|&(_, hi)| hi).max().unwrap());
    for r in found.iter().filter(|c| cell_kind(c.id) == Some(CELL_CITY)) {
        let (cx, cy) = (r.rect[0] + r.rect[2] / 2, r.rect[1] + r.rect[3] / 2);
        assert!(cx >= scroll[0] && cy >= scroll[1] && cx < scroll[0] + scroll[2] && cy < scroll[1] + scroll[3]);
    }
    assert!(check_every_pixel(&m) > 9 * 3);
}

#[test]
fn the_draft_button_stops_at_the_speech_box_tested_before_it() {
    let m = domestic_fixture(&[&[0, 0, 1, 2]], 0, true);
    let draft = domestic(&m, FORM).unwrap().draft.unwrap();
    // Below the Government button's last row, which it shares.
    assert_eq!(draft, [0x28A, 0xD2, 0x32C - 0x28A, 0xE7 - 0xD2 + 1]);
    check_every_pixel(&m);
}

#[test]
fn packed_citizens_end_before_the_production_column() {
    // 50 citizens in 54 slots pack to a 4 pixel step and reach x 0x258 + 54·4.
    let moods: Vec<i32> = (0..50).map(|j| (j % 5) as i32).collect();
    let m = domestic_fixture(&[&moods, &[1, 1]], 0, false);
    let d = domestic(&m, FORM).unwrap();
    assert_eq!(d.rows[0].citizens.len(), 50);
    let end = d.rows[0].citizens.iter().map(|(_, s)| s[0] + s[2]).max().unwrap();
    assert!(end > 0x32B);
    assert_eq!(d.rows[0].production.unwrap()[0], end);
    assert_eq!(d.rows[1].production.unwrap()[0], 0x32B);
    check_every_pixel(&m);
}

#[test]
fn citizen_slots_follow_mood_groups_with_a_gap_after_each() {
    // Content, happy, content, unhappy: happy first, then the two content.
    let slots = citizen_slots([600, 10, 1000, 47], 4, 0x31, &[1, 0, 1, 2]);
    let x = |s: &[(usize, [i32; 4])]| s.iter().map(|(j, r)| (*j, r[0], r[2])).collect::<Vec<_>>();
    assert_eq!(x(&slots), [(1, 600, 49), (0, 698, 49), (2, 747, 49), (3, 845, 49)]);
    assert!(slots.iter().all(|(_, r)| r[1] == 10 && r[3] == 38));
    // Packed: 8 slots into 270 pixels.
    let packed = citizen_slots([600, 0, 870, 37], 4, 0x31, &[0, 0, 0, 0]);
    assert_eq!(x(&packed), [(0, 600, 31), (1, 631, 31), (2, 662, 31), (3, 693, 31)]);
    assert!(citizen_slots([10, 0, 5, 37], 1, 0x31, &[0]).is_empty());
    assert!(citizen_slots([0, 0, 100, 37], 0, 0x31, &[]).is_empty());
    // Unknown moods take no slot (0x4BECE0 only counts 0..5).
    assert_eq!(x(&citizen_slots([0, 0, 1000, 37], 2, 10, &[7, 0])), [(1, 0, 10)]);
}

#[test]
fn header_cells_follow_the_native_test_order_and_do_not_overlap() {
    let cells = header_cells();
    assert_eq!(cells.len(), 10);
    for pair in cells.windows(2) {
        assert!(pair[0][0] + pair[0][2] <= pair[1][0]);
    }
    assert_eq!(cells[1][0], 0xF1);
}

#[test]
fn changed_code_or_other_forms_publish_nothing() {
    let mut m = domestic_fixture(&[&[1]], 0, false);
    m.bytes(0x51_FD79, &[0x90, 0x90]);
    m.bytes(0x49_E064, &[0x90]);
    m.bytes(0x52_1680, &[0x90]);
    let found = controls(&m, &sites());
    assert!(found.iter().all(|c| !matches!(cell_kind(c.id), Some(CELL_PRODUCTION | CELL_SPEECH | CELL_HEADER))));
    assert!(found.iter().any(|c| cell_kind(c.id) == Some(CELL_CITY)));
    m.put(FORM, 0x66_5000);
    assert!(controls(&m, &sites()).iter().all(|c| cell_kind(c.id).is_none()));
    assert!(domestic(&m, FORM).is_none());
    // A hidden advisor publishes nothing either.
    let mut hidden = domestic_fixture(&[&[1]], 0, false);
    hidden.put(FORM + 0xA0, 0);
    assert!(controls(&hidden, &sites()).is_empty());
}

#[test]
fn cells_are_content_rows_but_the_wheel_list_is_not() {
    let c = |kind| Control { rect: [0, 0, 10, 10], kind: ControlKind::Button, id: cell_id(FORM, kind, 0) };
    assert!(c(CELL_PRODUCTION).content_row() && c(CELL_CITIZEN).content_row());
    assert!(!c(CELL_LIST).content_row() && !c(CELL_CITY).text_link());
    assert_eq!(cell_kind(cell_id(FORM, CELL_HEADER, 9)), Some(CELL_HEADER));
    assert_eq!(cell_kind(FORM as u64), None);
}

const CULTURE_FORM: u32 = 0xA2_A7B0;

/// The cultural advisor as its init (0x4F9297..0x4F932D) fills its region
/// table: eight city rows and seven building rows.
fn culture_fixture(cities: u32, start: i32, buildings: &[i32]) -> Fake {
    let mut m = Fake::default();
    load_code(&mut m);
    let f = CULTURE_FORM;
    m.put(f, CULTURE);
    m.put(f + 0x9C, KEYED_STATUS);
    m.put(f + 0xA0, 1);
    m.put(f + 0x130, 0);
    m.rect(f + 0x1AC, [ORIGIN.0, ORIGIN.1, ORIGIN.0 + 1024, ORIGIN.1 + 768]);
    m.rect(f + 0x1BC, [0; 4]);
    m.rect(f + 0x1DC, [0, 0, 1920, 1080]);
    m.put(f + 0x22C, 0);
    m.put(f + 0x380, 0);
    m.put(f + C_REGIONS + REGION_ENTRIES, REGIONS);
    m.put(f + C_REGIONS + REGION_COUNT, 15);
    for i in 0..8u32 {
        let y = 0x13A + 0x2D * i as i32;
        m.rect(REGIONS + 32 * i + 4, [0x72, y, 0x72 + 0x22C, y + 0x2D]);
        m.put(REGIONS + 32 * i + 0x18, i);
    }
    for j in 0..7u32 {
        let (e, y) = (REGIONS + 32 * (8 + j), 0x136 + 0x37 * j as i32);
        m.rect(e + 4, [0x2D4, y, 0x2D4 + 0x11D, y + 0x37]);
        m.put(e + 0x18, 8 + j);
        m.put(f + C_BUILDINGS + 4 * (8 + j), buildings.get(j as usize).copied().unwrap_or(-1) as u32);
    }
    m.put(f + C_START, start as u32);
    m.put(f + C_COUNT, cities);
    for k in 0..512u32 {
        m.put(f + C_IDS + 4 * k, if k < cities { 3 + 2 * k } else { u32::MAX });
    }
    for k in 0..cities {
        add_city(&mut m, 3 + 2 * k, &[0]);
    }
    m.put(CITY_ITEMS, CITIES);
    m.put(CITY_LAST, 3 + 2 * cities);
    m.put(f + C_CITY_LAST_START, (cities as i32 - 8).max(0) as u32);
    m.put(f + C_BUILDING_LAST_START, 0);
    m.put(0x100, f);
    m.put(0x200, 1);
    m.put(0x300, 0);
    m.put(0x304, 0);
    m
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CultureNative {
    Select(u32),
    Domestic,
    Pedia(u32),
    CulturePedia,
}

fn culture_region(m: &Fake, x: i32, y: i32) -> Option<u32> {
    let base = m.u32(CULTURE_FORM + C_REGIONS + REGION_ENTRIES)?;
    (0..m.u32(CULTURE_FORM + C_REGIONS + REGION_COUNT)?).rev().find_map(|i| {
        let [l, t, r, b] = rect_at(m, base + 32 * i + 4)?;
        (x >= l && x < r && y >= t && y < b).then(|| m.u32(base + 32 * i + 0x18)).flatten()
    })
}

/// 0x4FA500: the speech box and the headers (y 0xF6 < y < 0x120) lie
/// elsewhere; then the region table.
fn culture_left(m: &Fake, x: i32, y: i32) -> Option<CultureNative> {
    assert!(!(y > 0xF6 && y < 0x120) && !((0x32C..=0x3EA).contains(&x) && (0x75..=0xEF).contains(&y)));
    let v = culture_region(m, x, y)?;
    let index = m.i32(CULTURE_FORM + C_START)? + v as i32;
    (v < 8 && city(m, m.i32(CULTURE_FORM + C_IDS + 4 * index as u32)?).is_some()).then_some(CultureNative::Select(index as u32))
}

/// 0x4FB920.
fn culture_right(m: &Fake, x: i32, y: i32) -> Option<CultureNative> {
    if ((0x1ED..0x29D).contains(&x) && (0x63..0xA4).contains(&y)) || ((0x175..0x1B1).contains(&x) && (0xF3..0x2D6).contains(&y)) {
        return Some(CultureNative::CulturePedia);
    }
    let v = culture_region(m, x, y)?;
    if v < 8 {
        return (m.i32(CULTURE_FORM + C_IDS + 4 * v)? != -1).then_some(CultureNative::Domestic);
    }
    (v - 8 < 7 && m.i32(CULTURE_FORM + C_BUILDINGS + 4 * v)? != -1).then_some(CultureNative::Pedia(v - 8))
}

#[test]
fn cultural_city_and_building_rows_hit_their_entries_in_the_native_handlers() {
    let m = culture_fixture(11, 2, &[5, 9, 12]);
    let c = culture(&m, CULTURE_FORM).unwrap();
    assert_eq!(c.cities.iter().map(|(i, _)| *i).collect::<Vec<_>>(), (2..10).collect::<Vec<_>>());
    assert_eq!(c.buildings.iter().map(|(j, _)| *j).collect::<Vec<_>>(), [0, 1, 2]);
    let found = controls(&m, &sites());
    let lists: Vec<_> = found.iter().filter(|c| cell_kind(c.id) == Some(CELL_LIST)).map(|c| c.kind).collect();
    assert_eq!(lists.len(), 1, "only the city list scrolls");
    assert!(matches!(lists[0], ControlKind::List { step: 0x2D, .. }));
    let mut checked = 0;
    for c in found.iter().filter(|c| matches!(cell_kind(c.id), Some(CELL_CITY | CELL_BUILDING))) {
        let index = c.id as u32 & 0xF_FFFF;
        assert_eq!(c.kind, ControlKind::ListItem);
        for (px, py) in points(c.rect) {
            {
                let (lx, ly) = (px - ORIGIN.0, py - ORIGIN.1);
                if cell_kind(c.id) == Some(CELL_CITY) {
                    assert_eq!(culture_left(&m, lx, ly), Some(CultureNative::Select(index)), "{lx},{ly}");
                    let right = culture_right(&m, lx, ly);
                    assert!(matches!(right, Some(CultureNative::Domestic | CultureNative::CulturePedia)), "{lx},{ly}: {right:?}");
                } else {
                    assert_eq!(culture_left(&m, lx, ly), None);
                    assert_eq!(culture_right(&m, lx, ly), Some(CultureNative::Pedia(index)));
                }
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 11);
    // Few cities: no scrolling, empty rows publish nothing.
    let few = culture_fixture(3, 0, &[]);
    let found = controls(&few, &sites());
    assert_eq!(found.iter().filter(|c| cell_kind(c.id) == Some(CELL_CITY)).count(), 3);
    assert!(found.iter().all(|c| !matches!(cell_kind(c.id), Some(CELL_LIST | CELL_BUILDING))));
}

#[test]
fn science_boxes_use_each_advance_place_and_its_state_box_on_the_era_page() {
    let mut m = Fake::default();
    let f = 0xC9_1CB0;
    m.put(f, SCIENCE);
    m.put(ADVANCE_COUNT, 4);
    m.put(ADVANCES, 0x71_0000);
    m.put(f + S_SHOWN, 0x73_0000);
    m.put(f + S_ERA, 1);
    // Advance 0 known (state 1), 1 researchable (3), 2 on another page,
    // 3 the current research (4).
    for (i, (shown, state, x, y)) in [(0u8, 1, 100, 200), (0, 3, 300, 200), (1, 1, 500, 200), (0, 4, 100, 400)].into_iter().enumerate() {
        m.bytes(0x73_0000 + 24 * i as u32, &[shown]);
        m.put(0x73_0000 + 24 * i as u32 + 4, state);
        m.put(0x71_0000 + 0x74 * i as u32 + 0x50, x);
        m.put(0x71_0000 + 0x74 * i as u32 + 0x54, y);
    }
    for kind in 0..4u32 {
        let record = f + S_BOXES + 44 * (16 * kind + 1);
        m.put(record, 0x5000 + kind);
        m.put(record + 0x14, 130 + kind);
        m.put(record + 0x18, 40);
    }
    assert_eq!(science(&m, f).unwrap(), [(0, [100, 200, 131, 41]), (1, [300, 200, 132, 41]), (3, [100, 400, 134, 41])]);
    // A missing box sprite publishes nothing for that state.
    m.put(f + S_BOXES + 44 * (16 * 3 + 1), 0);
    assert_eq!(science(&m, f).unwrap().len(), 2);
    m.put(f, CULTURE);
    assert!(science(&m, f).is_none());
}

#[test]
fn foreign_leaders_need_contact_and_tabs_keep_their_native_order() {
    let mut m = Fake::default();
    let f = 0xA4_6940;
    m.put(f, FOREIGN);
    for (k, at) in F_TABS.iter().enumerate() {
        m.rect(f + at, [10 + 100 * k as i32, 20, 90 + 100 * k as i32, 40]);
    }
    m.put(PLAYER, 1);
    let contacts = LEADERS + crate::layout::LEADER_SIZE + crate::layout::LEADER_CONTACTS;
    for k in 0..8u32 {
        m.put(f + F_CIVS + 4 * k, [1, 2, 3, u32::MAX, 5, 6, 7, 8][k as usize]);
        m.rect(f + F_LEADERS + 16 * k, [100 * k as i32, 300, 100 * k as i32 + 80, 400]);
    }
    for civ in 0..32 {
        m.put(contacts + 4 * civ, u32::from(civ == 2 || civ == 6));
    }
    let (tabs, leaders) = foreign(&m, f).unwrap();
    assert_eq!(tabs, [(0, [10, 20, 81, 21]), (1, [110, 20, 81, 21]), (2, [210, 20, 81, 21])]);
    assert_eq!(leaders, [(1, [0, 300, 81, 101]), (2, [100, 300, 81, 101]), (6, [500, 300, 81, 101])]);
}

#[test]
fn an_advance_takes_the_points_of_its_box_rectangle_in_the_games_order() {
    let mut m = Fake::default();
    let form = 0x10_0000;
    let (advances, shown) = (0x20_0000, 0x30_0000);
    m.put(form, SCIENCE);
    m.put(ADVANCE_COUNT, 3);
    m.put(ADVANCES, advances);
    m.put(form + S_SHOWN, shown);
    m.put(form + S_ERA, 0);
    // Advance 0 shown at (100, 50), 1 hidden, 2 shown at (100, 120); both
    // drawn with the state-0 box of era 0, 80 x 40 (inclusive: 81 x 41).
    for (i, shown_byte, at) in [(0u32, 0u8, (100, 50)), (1, 1, (300, 50)), (2, 0, (100, 120))] {
        m.bytes(shown + 24 * i, &[shown_byte]);
        m.put(shown + 24 * i + 4, 0);
        m.put(advances + 0x74 * i + 0x50, at.0);
        m.put(advances + 0x74 * i + 0x54, at.1);
    }
    let (record, sprite) = (form + S_BOXES, 0x40_0000);
    m.put(record, sprite);
    m.put(record + 0x14, 80);
    m.put(record + 0x18, 40);
    // Its jgl sprite fills the box.
    m.put(record + 0x1C, 0);
    m.put(record + 0x20, 0);
    m.put(sprite + 0x30, 81);
    m.put(sprite + 0x34, 41);
    // Every point of the box, its corners and edges too (the game's test is
    // inclusive), and none outside.
    for (x, y) in [(100, 50), (180, 90), (140, 70), (100, 90), (180, 50)] {
        assert_eq!(science_advance_at(&m, form, (x, y)), Some(0), "({x}, {y})");
    }
    assert_eq!(science_advance_at(&m, form, (140, 130)), Some(2));
    for (x, y) in [(99, 70), (181, 70), (140, 91), (320, 60)] {
        assert_eq!(science_advance_at(&m, form, (x, y)), None, "({x}, {y})");
    }
    // A sprite trimmed of its transparent margins, placed at (6, 2) in the
    // box: the margins take nothing, as in the game.
    m.put(record + 0x1C, 6);
    m.put(record + 0x20, 2);
    m.put(sprite + 0x30, 70);
    m.put(sprite + 0x34, 36);
    for (x, y) in [(106, 52), (175, 87), (140, 70)] {
        assert_eq!(science_advance_at(&m, form, (x, y)), Some(0), "({x}, {y})");
    }
    for (x, y) in [(100, 70), (105, 70), (176, 70), (140, 51), (140, 88)] {
        assert_eq!(science_advance_at(&m, form, (x, y)), None, "({x}, {y})");
    }
    // Another form is not the science advisor.
    m.put(form, 0x66_C270);
    assert_eq!(science_advance_at(&m, form, (140, 70)), None);
}
