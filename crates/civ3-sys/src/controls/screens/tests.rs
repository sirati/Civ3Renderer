use super::*;
use crate::controls::advisors::cell_kind;
use crate::controls::controls;
use std::collections::HashMap;

#[derive(Default)]
struct Fake(HashMap<u32, u8>);

impl Fake {
    fn put(&mut self, at: u32, v: u32) {
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
    /// A shown root form at the screen's (ox, oy), `w`×`h`.
    fn form(&mut self, f: u32, vtable: u32, (ox, oy): (i32, i32), (w, h): (i32, i32)) {
        self.put(f, vtable);
        self.put(f + 0x9C, 0x100_0000);
        self.put(f + 0xA0, 1);
        self.put(f + 0x130, 0);
        self.rect(f + 0x1AC, [ox, oy, ox + w, oy + h]);
        self.rect(f + 0x1BC, [0; 4]);
        self.rect(f + 0x1DC, [0, 0, 1920, 1080]);
        self.put(f + 0x22C, 0);
        self.put(f + 0x380, 0);
        self.put(0x100, f);
        self.put(0x200, 1);
        self.put(0x300, 0);
        self.put(0x304, 0);
    }
    /// A region table: entries (value, x, y, w, h) as 0x60F080 adds them.
    fn regions(&mut self, table: u32, base: u32, entries: &[(u32, [i32; 4])]) {
        self.put(table + 0x50, base);
        self.put(table + 0x58, entries.len() as u32);
        for (i, (v, [x, y, w, h])) in entries.iter().enumerate() {
            self.rect(base + 32 * i as u32 + 4, [*x, *y, x + w, y + h]);
            self.put(base + 32 * i as u32 + 0x14, 0);
            self.put(base + 32 * i as u32 + 0x18, *v);
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

fn sites() -> super::super::Sites {
    super::super::Sites { roots: 0x100, root_count: 0x200, modal: 0x300, companion: 0x304, main_gui: 0x9F_DB10, advisor: 0x9A_FD58 }
}

fn region_hit(m: &Fake, table: u32, x: i32, y: i32) -> Option<u32> {
    let base = m.u32(table + 0x50)?;
    (0..m.u32(table + 0x58)?).rev().find_map(|i| {
        let [l, t, r, b] = rect_at(m, base + 32 * i + 4)?;
        (x >= l && x < r && y >= t && y < b).then(|| m.u32(base + 32 * i + 0x18)).flatten()
    })
}

fn inside([l, t, r, b]: [i32; 4], x: i32, y: i32) -> bool {
    x >= l && x <= r && y >= t && y <= b
}

/// Every edge pixel of a rectangle and every third one inside it.
fn points([x, y, w, h]: [i32; 4]) -> impl Iterator<Item = (i32, i32)> {
    (y..y + h)
        .flat_map(move |py| (x..x + w).map(move |px| (px, py)))
        .filter(move |&(px, py)| px == x || py == y || px == x + w - 1 || py == y + h - 1 || ((px - x) % 3 == 0 && (py - y) % 3 == 0))
}

const ORIGIN: (i32, i32) = (448, 156);

// ---- Military ----

const MIL: u32 = 0xB4_50B8;

fn military_fixture(by_city: bool, cities: u32, scroll: i32) -> Fake {
    let mut m = Fake::default();
    m.form(MIL, MILITARY, ORIGIN, (1024, 768));
    // The init method 0x56DA00 and the row layout 0x56DEE0.
    m.regions(MIL + M_REGIONS, 0x60_0000, &[
        (0, [0x9D, 0x141, 0x18, 0xC]), (1, [0xC2, 0x141, 0x18, 0xC]), (2, [0x25A, 0x141, 0x18, 0xC]), (3, [0x27F, 0x141, 0x18, 0xC]),
        (4, [0x230, 0xF9, 0x18B, 0x28]), (5, [0x184, 0x5C, 0x96, 0x2E]), (6, [0x184, 0x8E, 0x96, 0x2E]), (7, [0x224, 0x6E, 0xFC, 0x62]),
    ]);
    for i in 0..6u32 {
        let (t, b) = (0x165 + 0x2D * i as i32, 0x192 + 0x2D * i as i32);
        m.rect(MIL + 0xFD58 + 16 * i, [0x67, t, 0x211, b]);
        m.rect(MIL + M_NAMES[0] + 16 * i, [0x67, t, 0xB7, b]);
        m.rect(MIL + M_UNITS[0] + 16 * i, [0xB7, t, 0x211, b]);
        m.rect(MIL + 0xFE78 + 16 * i, [0x225, t, 0x3B0, b]);
        m.rect(MIL + M_NAMES[1] + 16 * i, [0x22C, t, 0x27C, b]);
        m.rect(MIL + M_UNITS[1] + 16 * i, [0x27C, t, 0x3B0, b]);
    }
    // The two lists' label rectangles; the first one's right splits them.
    m.rect(MIL + 0xFFD8, [0x67, 0xF9, 0x211, 0x121]);
    m.rect(MIL + 0xFFE8, [0x230, 0xF9, 0x3BB, 0x121]);
    m.rect(MIL + 0x1_0018, [0x26E, 0x90, 0x322, 0xC5]);
    m.bytes(MIL + M_BY_CITY[0], &[by_city as u8, 0]);
    m.put(MIL + M_SCROLL_CITIES, scroll as u32);
    m.put(MIL + M_SELECTED_CIV, 2);
    m.put(MIL + M_CITY_ROWS[0], cities.saturating_sub(scroll as u32).min(6));
    m.put(MIL + M_TYPE_ROWS[0], 3);
    m.put(MIL + M_CITY_ROWS[1], 0);
    m.put(MIL + M_TYPE_ROWS[1], 2);
    // The player (1) owns cities 0, 2, 4…; 1, 3, … are another civ's.
    m.put(PLAYER, 1);
    m.put(CITY_ITEMS, 0x61_0000);
    m.put(CITY_LAST, 2 * cities);
    for id in 0..=2 * cities {
        let city = 0x62_0000 + 0x100 * id;
        m.put(0x61_0000 + 8 * id + 4, city + 0x1C);
        m.bytes(city + 0x28, &[if id % 2 == 0 { 1 } else { 2 }]);
    }
    m
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MilNative {
    Tab(u32),
    City(u32),
    Units(u32, u32, i32),
}

/// 0x571C70 up to its list test (the leader box returns first), then
/// 0x570130 and 0x5705F0's column for unit cells.
fn military_left(m: &Fake, x: i32, y: i32) -> Option<MilNative> {
    if inside(rect_at(m, MIL + 0x1_0018).unwrap(), x, y) {
        return None;
    }
    if let Some(v) = region_hit(m, MIL + M_REGIONS, x, y) {
        return (v <= 4).then_some(MilNative::Tab(v));
    }
    let f = |at| m.i32(MIL + at).unwrap();
    if x < f(0xFDC0) {
        if m.u8(MIL + 0x35F2) == Some(0) {
            return None;
        }
        let i = (0..6).find(|i| inside(rect_at(m, MIL + 0xFDB8 + 16 * i).unwrap(), x, y))?;
        let n = (f(0x35D8) + i as i32) as usize;
        return (n < own_cities(m).len()).then_some(MilNative::City(n as u32));
    }
    if x < f(0xFEE0) && x >= f(0xFFE8) {
        return None;
    }
    let (side, rows, base) = if x < f(0xFFE0) {
        (0, if m.u8(MIL + 0x35F2) != Some(0) { f(0x35E0) } else { f(0x35D0) }, 0xFE18)
    } else {
        if f(0x35C4) < 1 {
            return None;
        }
        (1, if m.u8(MIL + 0x35F3) != Some(0) { f(0x35E4) } else { f(0x35D4) }, 0xFF38)
    };
    let i = (0..6).take_while(|i| *i < rows).find(|i| inside(rect_at(m, MIL + base + 16 * *i as u32).unwrap(), x, y))?;
    let left = f(base + 16 * i as u32);
    let mut k = 0;
    while x > left + 32 * (k + 1) {
        k += 1;
    }
    Some(MilNative::Units(side, i as u32, k))
}

#[test]
fn military_tabs_city_names_and_unit_columns_hit_the_native_tests() {
    for (by_city, cities, scroll) in [(true, 9, 2), (true, 3, 0), (false, 5, 0)] {
        let m = military_fixture(by_city, cities, scroll);
        let found = controls(&m, &sites());
        let mut names = 0;
        for c in found.iter().filter(|c| cell_kind(c.id).is_some()) {
            let index = c.id as u32 & 0xF_FFFF;
            let expect = match cell_kind(c.id).unwrap() {
                CELL_TAB => MilNative::Tab(index),
                CELL_CITY => {
                    names += 1;
                    MilNative::City(index)
                }
                k => panic!("cell {k}"),
            };
            for (px, py) in points(c.rect) {
                assert_eq!(military_left(&m, px - ORIGIN.0, py - ORIGIN.1), Some(expect.clone()), "{px},{py} {c:?}");
            }
        }
        let own = (cities as i32 + 1 - scroll).clamp(0, 6);
        assert_eq!(names, if by_city { own } else { 0 }, "{by_city} {cities} {scroll}");
        // Units drawn at their columns: the icons' columns become targets.
        let strips = military_unit_strips(&m, &found);
        let rows = if by_city { (cities as i32 - scroll).clamp(0, 6) } else { 3 };
        assert_eq!(strips.iter().filter(|s| s.side == 0).count() as i32, rows);
        assert_eq!(strips.iter().filter(|s| s.side == 1).count(), 2, "the selected civ's type rows");
        for s in &strips {
            // Icons at columns 0, 1 and 3, drawn 32 wide from L + 32k + 2.
            let icons = [0, 1, 3].map(|k| [s.left + 32 * k + 2, s.rect[1] + 4, 32, 32]);
            let cells = unit_columns(s, icons);
            assert_eq!(cells.len(), 3);
            for c in &cells {
                let k = (c.id as u32 & 0xFF) as i32;
                for (px, py) in points(c.rect) {
                    assert_eq!(military_left(&m, px - ORIGIN.0, py - ORIGIN.1), Some(MilNative::Units(s.side, s.row, k)), "{px},{py} {c:?}");
                }
            }
        }
    }
}

// ---- Wonders ----

#[test]
fn wonder_boxes_follow_the_six_native_boxes_and_their_filled_slots() {
    let f = 0xCA_7C90;
    let mut m = Fake::default();
    m.form(f, WONDERS, ORIGIN, (1024, 768));
    // 0x5D0440: two columns of three boxes.
    for i in 0..6u32 {
        let (col, row) = ((i & 1) as i32, (i >> 1) as i32);
        m.rect(f + W_BOXES + 16 * i, [0x68 + 0x192 * col, 0x64 + 0xCA * row, 0x1DA + 0x192 * col, 0x12C + 0xCA * row]);
        m.put(f + W_SLOTS + 4 * i, [12, 30, -1i32 as u32, 0, 45, 61][i as usize]);
    }
    m.put(f + W_SCROLL, 1);
    m.put(f + W_COUNT, 11);
    let found = controls(&m, &sites());
    let boxes: Vec<_> = found.iter().filter(|c| cell_kind(c.id) == Some(CELL_BUILDING)).collect();
    assert_eq!(boxes.iter().map(|c| c.id as u32 & 0xFFFF).collect::<Vec<_>>(), [2, 3, 6, 7]);
    // 0x5D13C0: the first box (inclusive) whose slot is positive.
    let native = |x: i32, y: i32| (0..6u32).find(|i| inside(rect_at(&m, f + W_BOXES + 16 * i).unwrap(), x, y)).filter(|i| m.i32(f + W_SLOTS + 4 * i).unwrap() > 0);
    for c in boxes {
        let slot = (c.id as u32 & 0xFFFF) - 2;
        for (px, py) in points(c.rect) {
            assert_eq!(native(px - ORIGIN.0, py - ORIGIN.1), Some(slot));
        }
    }
    let list = found.iter().find(|c| cell_kind(c.id) == Some(CELL_LIST)).unwrap();
    assert!(matches!(list.kind, ControlKind::List { step: 0xCA, .. }));
    m.put(f + W_COUNT, 6);
    assert!(controls(&m, &sites()).iter().all(|c| cell_kind(c.id) != Some(CELL_LIST)));
}

// ---- City governor ----

#[test]
fn governor_tabs_ok_and_cancel_hit_its_native_tests() {
    let f = 0xA9_5ACC;
    let mut m = Fake::default();
    m.form(f, GOVERNOR, ORIGIN, (677, 620));
    // (left, top, right, bottom), inclusive: the two tabs, OK, Cancel.
    let rects = [[20, 40, 160, 60], [170, 40, 330, 60], [440, 580, 520, 605], [540, 580, 640, 605]];
    for (i, r) in rects.into_iter().enumerate() {
        m.rect(f + G_RECTS + 16 * i as u32, r);
    }
    let found: Vec<_> = controls(&m, &sites()).into_iter().filter(|c| c.id >> 32 == f as u64).collect();
    assert_eq!(found.iter().map(|c| (cell_kind(c.id), c.id as u32 & 0xF_FFFF)).collect::<Vec<_>>(),
        [(Some(CELL_TAB), 0), (Some(CELL_TAB), 1), (Some(CELL_BUTTON), 0), (Some(CELL_BUTTON), 1)]);
    assert!(found.iter().all(|c| c.kind == ControlKind::Button));
    // 0x53BD80: each rectangle in turn, inclusive, in the form's pixels.
    let native = |x: i32, y: i32| (0..4).find(|&i| inside(rects[i], x, y));
    for (i, c) in found.iter().enumerate() {
        for (px, py) in points(c.rect) {
            assert_eq!(native(px - ORIGIN.0, py - ORIGIN.1), Some(i), "{c:?} at ({px}, {py})");
        }
        let [x, y, w, h] = c.rect;
        assert_eq!(native(x + w - ORIGIN.0, y - ORIGIN.1).filter(|&j| j == i), None, "nothing past its right edge");
        assert_eq!(native(x - ORIGIN.0, y + h - ORIGIN.1).filter(|&j| j == i), None, "nothing below it");
    }
    // Another form class: none of these.
    m.put(f, 0x66_6408);
    assert!(controls(&m, &sites()).iter().all(|c| cell_kind(c.id).is_none()));
}

// ---- Palace and Spaceship masks ----

/// An 8-bit screen-sized mask PCX at `pcx` whose pixels are `paint(x, y)`.
fn mask(m: &mut Fake, pcx: u32, (w, h): (i32, i32), paint: impl Fn(i32, i32) -> u8) {
    let (image, pixels, stride) = (pcx + 0x1000, 0x80_0000, w + 3);
    m.put(pcx + 4, image);
    m.put(image + 0x24, 8);
    m.rect(image + 0x54, [0, 0, w, h]);
    m.put(image + 0x40, stride as u32);
    m.put(image + 0x4C0, pixels);
    for y in 0..h {
        let row: Vec<u8> = (0..stride).map(|x| if x < w { paint(x, y) } else { 7 }).collect();
        m.bytes(pixels + (y * stride) as u32, &row);
    }
}

#[test]
fn mask_cells_lie_wholly_in_their_colour_inside_the_background() {
    let (sw, sh) = (320, 200);
    let mut m = Fake::default();
    let f = 0xB6_DDF8;
    m.form(f, PALACE, (0, 0), (sw, sh));
    m.put(SCREEN_W, sw as u32);
    m.put(SCREEN_H, sh as u32);
    m.put(f + 0x312C, 200);
    m.put(f + 0x3130, 120);
    // An L shaped item 3, a ring item 5, item 9 outside the background,
    // and 0x40 above the Palace's item colours.
    let paint = |x: i32, y: i32| match (x, y) {
        (70..=100, 50..=60) | (70..=80, 61..=110) => 3,
        (150..=200, 70..=120) if !(160..=190).contains(&x) || !(80..=110).contains(&y) => 5,
        (5..=30, 5..=30) => 9,
        (230..=250, 50..=60) => 0x40,
        _ => 0xFF,
    };
    mask(&mut m, f + 0x3154, (sw, sh), paint);
    let found = controls(&m, &sites());
    let cells: Vec<_> = found.iter().filter(|c| cell_kind(c.id) == Some(CELL_MASK)).collect();
    assert_eq!(cells.iter().map(|c| c.id as u32 & 0xFF).collect::<Vec<_>>(), [3, 5]);
    for c in cells {
        // 0x600220 at every pixel gives the item's own colour.
        for (px, py) in points(c.rect) {
            assert_eq!(paint(px, py) as u32, c.id as u32 & 0xFF, "{px},{py}");
        }
    }
    // The Spaceship: parts below the part count, and its three buttons.
    let s = 0xC9_0E40;
    let mut m = Fake::default();
    m.form(s, SPACESHIP, (0, 0), (sw, sh));
    m.put(SCREEN_W, sw as u32);
    m.put(SCREEN_H, sh as u32);
    m.put(s + 0x84C, 300);
    m.put(s + 0x850, 180);
    m.put(0x9C_7268, 10);
    mask(&mut m, s + 0x574, (sw, sh), |x, y| match (x, y) {
        (20..=60, 20..=40) => 2,
        (70..=90, 20..=40) => 12,
        (100..=150, 100..=120) => 0xFE,
        (200..=240, 100..=120) => 0xFC,
        _ => 0xFF,
    });
    let colours: Vec<u32> = controls(&m, &sites()).iter().filter(|c| cell_kind(c.id) == Some(CELL_MASK)).map(|c| c.id as u32 & 0xFF).collect();
    assert_eq!(colours, [2, 0xFC, 0xFE]);
}

// ---- Diplomacy ----

#[test]
fn semantic_mask_targets_work_without_native_pixels_and_obey_window_and_index_limits() {
    struct Semantic;
    impl Mem for Semantic {
        fn u32(&self, _: u32) -> Option<u32> { panic!("semantic targets must not read placeholder memory") }
        fn u8(&self, _: u32) -> Option<u8> { panic!("semantic targets must not read placeholder memory") }
        fn index_regions(&self, pcx: u32) -> Option<Vec<(u8, [i32; 4])>> {
            assert_eq!(pcx, 123);
            Some(vec![(3, [8, 12, 4, 4]), (4, [20, 20, 1, 1]), (0xFF, [15, 15, 1, 1])])
        }
    }
    assert_eq!(mask_cells(&Semantic, 123, [10, 10, 10, 10], 0x3F), [(3, [10, 12, 2, 4])]);
}

const DIP: u32 = 0xA3_51C0;
const LH: i32 = 14;

struct Deal {
    ai: bool,
    mode: u32,
    page: u32,
    options: Vec<i32>,
    expanded: [bool; 11],
}

fn diplomacy_fixture(d: &Deal) -> Fake {
    let mut m = Fake::default();
    m.form(DIP, DIPLOMACY, ORIGIN, (1024, 768));
    // The canvas font: height 12 plus 2 leading.
    let canvas = DIP + crate::layout::BASE_FORM_DATA_CANVAS + 4;
    m.put(canvas + 0x5C, 0x7F_0000);
    m.put(0x7F_0000 + 8, 12);
    m.put(0x7F_0000 + 0x10, 2);
    // The partner, civ 3, is race 6; humans are races 0 (and 6 if human).
    m.put(DIP + D_PARTNER, 3);
    m.put(LEADERS + crate::layout::LEADER_SIZE * 3 + 0x1C, 6);
    m.put(HUMANS, 1 | if d.ai { 0 } else { 1 << 6 });
    m.put(DIP + D_MODE, d.mode);
    m.put(DIP + D_PAGE, d.page);
    m.put(DIP + 0xEAC, 0);
    let mut node = 0;
    for (k, lines) in d.options.iter().enumerate().rev() {
        let at = 0x7E_0000 + 0x40 * k as u32;
        m.put(at + 0xC, *lines as u32);
        m.put(at + 0x10, node);
        node = at;
    }
    m.put(DIP + D_OPTIONS, node);
    // Trees: categories 0, 2 and 5 on both sides, 5 empty.
    for c in 0..11u32 {
        let present = matches!(c, 0 | 2 | 5) as u8;
        m.bytes(DIP + 0xF0E + c, &[present]);
        m.bytes(DIP + 0xF19 + c, &[present]);
        m.bytes(DIP + 0xF03 + c, &[d.expanded[c as usize] as u8]);
        let count = match c { 0 => 3, 2 => 12, _ => 0 };
        m.put(DIP + 0xF24 + 4 * c, count);
        m.put(DIP + 0xF50 + 4 * c, count);
    }
    m.put(DIP + 0xEC4, 2);
    m.put(DIP + 0xEC8, 0);
    // The table: theirs 2 entries, ours 7 scrolled by 1.
    m.put(DIP + 0xF80 + 16 * 3 + 4, 2);
    m.put(DIP + 0x1180 + 16 * 3 + 4, 7);
    m.put(DIP + 0xECC, 0);
    m.put(DIP + 0xED0, 1);
    // 0x504A70's region table.
    m.regions(DIP + D_REGIONS, 0x60_0000, &[
        (0, [0x16E, 0x2D1, 0x61, 0x18]), (1, [0x1CF, 0x2D1, 0x61, 0x18]), (2, [0x230, 0x2D1, 0x61, 0x18]),
        (3, [0x2E7, 0x236, 0x1A, 0x18]), (4, [0x2E7, 0x24E, 0x1A, 0x18]), (5, [0x105, 0x236, 0x1A, 0x18]), (6, [0x105, 0x24E, 0x1A, 0x18]),
        (7, [0x13C, 0x1CC, 0x192, 0x50]), (8, [0x119, 0x1DC, 0x1A, 0x18]), (9, [0x119, 0x1F4, 0x1A, 0x18]),
        (10, [0x2D6, 0x1DC, 0x1A, 0x18]), (11, [0x2D6, 0x1F4, 0x1A, 0x18]), (12, [0x352, 0, 0xB6, 0x6B]), (13, [0x32F, 0x6C, 0xC1, 0x5C]),
        (14, [0x220, 0x294, 0x55, 0x18]), (15, [0x27A, 0x294, 0x55, 0x18]),
    ]);
    m
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DipNative {
    Option(u32),
    Tree(u32, u32, u32),
    Region(u32),
    Table(u32, i32),
}

/// 0x5076A0: options, trees, then the region table and its table rows.
fn diplomacy_left(m: &Fake, x: i32, y: i32) -> Option<DipNative> {
    let f = |at| m.i32(DIP + at).unwrap();
    let (mode, page) = (f(0xED8), f(0xEEC));
    let human = m.u32(HUMANS).unwrap() & (1 << (m.u32(LEADERS + crate::layout::LEADER_SIZE * f(0xE98) as u32 + 0x1C).unwrap() & 31)) != 0;
    let off = if mode == 2 || mode == 3 { 0x8A } else { 0 };
    if (0x130..=0x2D8).contains(&x) && y >= 0x15E + off + 3 * LH && y <= 0x15E + off + 12 * LH && !human {
        if mode == 2 && page != 1 {
            return None;
        }
        let mut lines = vec![];
        let mut node = m.u32(DIP + 0x1388).unwrap();
        while node != 0 {
            lines.push(m.i32(node + 0xC).unwrap());
            node = m.u32(node + 0x10).unwrap();
        }
        let row = (y - 3 * LH - off - 0x15E) / LH;
        let mut end = 9 - lines.iter().sum::<i32>();
        for (k, n) in lines.iter().enumerate() {
            end += n;
            if row >= end - n && row < end {
                return Some(DipNative::Option(k as u32));
            }
        }
        return None;
    }
    let (right, left) = ((0x319..=0x3DE).contains(&x), (0x2C..=0xF1).contains(&x));
    if (right || left) && y >= 0xE4 && y <= 0x266 {
        if !(mode == 2 && page == 1) || (human && !right) {
            return None;
        }
        let (flags, counts, scroll) = if right { (0xF0E, 0xF24, 0xEC4) } else { (0xF19, 0xF50, 0xEC8) };
        let mut r = (y - 0xE4) / LH + f(scroll);
        for c in 0..11u32 {
            if m.u8(DIP + flags + c) == Some(0) {
                continue;
            }
            let count = f(counts + 4 * c);
            if r == 0 {
                return (count > 0).then_some(DipNative::Tree(u32::from(!right), c, 0));
            }
            if m.u8(DIP + 0xF03 + c) != Some(0) {
                if r <= count {
                    return Some(DipNative::Tree(u32::from(!right), c, r as u32));
                }
                r -= 1 + count;
            } else {
                r -= 1;
            }
        }
        return None;
    }
    let v = region_hit(m, DIP + D_REGIONS, x, y)?;
    match v {
        7 if mode == 3 => (x <= 0x2CE && y <= 0x21C && f(0xEAC) == 0).then_some(DipNative::Region(7)),
        7 if mode == 2 && page == 1 => {
            let (column, list, scroll) = if (0x13C..=0x205).contains(&x) { (0, 0xF80, 0xECC) } else { (1, 0x1180, 0xED0) };
            if column == 0 && human || x > 0x2CE || y > 0x21C {
                return None;
            }
            let row = (y - 0x1CC) / LH + f(scroll);
            (row < f(list + 16 * f(0xE98) as u32 + 4)).then_some(DipNative::Table(column, row))
        }
        0 | 2 => (mode == 2 && page == 1).then_some(DipNative::Region(v)),
        1 => (mode == 2 && page == 2).then_some(DipNative::Region(v)),
        5 | 6 => (!human).then_some(DipNative::Region(v)),
        3 | 4 | 8..=13 => Some(DipNative::Region(v)),
        _ => None,
    }
}

#[test]
fn diplomacy_options_trees_and_table_hit_their_native_entries() {
    let mut expanded = [false; 11];
    expanded[2] = true;
    let cases = [
        Deal { ai: true, mode: 2, page: 1, options: vec![1, 2, 1], expanded },
        Deal { ai: false, mode: 2, page: 1, options: vec![1], expanded },
        Deal { ai: true, mode: 1, page: 0, options: vec![2, 1, 1, 1], expanded: [true; 11] },
        Deal { ai: true, mode: 3, page: 1, options: vec![1], expanded },
    ];
    for d in &cases {
        let m = diplomacy_fixture(d);
        let found = controls(&m, &sites());
        let mut kinds = HashMap::new();
        for c in found.iter().filter(|c| cell_kind(c.id).is_some()) {
            let (kind, index) = (cell_kind(c.id).unwrap(), c.id as u32 & 0xF_FFFF);
            *kinds.entry(kind).or_insert(0) += 1;
            let expect = match kind {
                CELL_OPTION => DipNative::Option(index),
                CELL_TREE => DipNative::Tree(index >> 19, index >> 15 & 0xF, index & 0x7FFF),
                CELL_BUTTON => DipNative::Region(index),
                CELL_TABLE => DipNative::Table(index >> 16, (index & 0xFFFF) as i32),
                k => panic!("cell {k}"),
            };
            for (px, py) in points(c.rect) {
                assert_eq!(diplomacy_left(&m, px - ORIGIN.0, py - ORIGIN.1), Some(expect.clone()), "{px},{py} {c:?} mode {}", d.mode);
            }
        }
        let n = |k| kinds.get(&k).copied().unwrap_or(0);
        match (d.ai, d.mode) {
            (true, 2) => {
                assert_eq!(n(CELL_OPTION), 3);
                // Rows: header 0 (collapsed), header 2, its 12 items, and
                // the empty header 5 (inert). Right scrolled by 2: 12; left: 14.
                assert_eq!(n(CELL_TREE), 12 + 14);
                // Ours shows rows 1..6, the last under the options band.
                assert_eq!(n(CELL_TABLE), 2 + 5);
            }
            (false, 2) => {
                assert_eq!(n(CELL_OPTION), 0);
                assert_eq!(n(CELL_TREE), 12);
                assert_eq!(n(CELL_TABLE), 6);
            }
            (true, 1) => assert!(n(CELL_OPTION) == 4 && n(CELL_TREE) == 0 && n(CELL_TABLE) == 0),
            (true, 3) => assert!(n(CELL_OPTION) == 1 && kinds.contains_key(&CELL_BUTTON)),
            _ => unreachable!(),
        }
    }
}
