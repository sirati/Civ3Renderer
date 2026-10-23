use super::*;
use crate::controls::{Sites, controls};
use std::collections::HashMap;

#[derive(Default)]
struct Fake(HashMap<u32, u8>);

impl Fake {
    fn put(&mut self, at: u32, v: u32) {
        for (i, b) in v.to_le_bytes().iter().enumerate() {
            self.0.insert(at + i as u32, *b);
        }
    }
    fn rect(&mut self, at: u32, r: [i32; 4]) {
        for (i, v) in r.into_iter().enumerate() {
            self.put(at + 4 * i as u32, v as u32);
        }
    }
    /// A shown form `w`×`h` at (x, y) (in its parent's pixels with
    /// `status1` 0x20), with `children`.
    fn form(&mut self, f: u32, vtable: u32, status1: u32, parent: u32, (x, y): (i32, i32), (w, h): (i32, i32), children: &[u32]) {
        self.put(f, vtable);
        self.put(f + 0x9C, status1);
        self.put(f + 0xA0, 1);
        self.put(f + 0x130, parent);
        self.rect(f + 0x1AC, [x, y, x + w, y + h]);
        self.rect(f + 0x1BC, [0; 4]);
        self.rect(f + 0x1DC, [0, 0, 1920, 1080]);
        self.put(f + 0x224, if children.is_empty() { 0 } else { f + 0x1_0000 });
        self.put(f + 0x22C, children.len() as u32);
        for (i, &c) in children.iter().enumerate() {
            self.put(f + 0x1_0000 + 4 * i as u32, c);
        }
        self.put(f + 0x380, 0);
    }
    /// `roots` the root forms, `modal` the modal one (0: none).
    fn roots(&mut self, roots: &[u32], modal: u32) {
        for (i, &r) in roots.iter().enumerate() {
            self.put(0x100 + 4 * i as u32, r);
        }
        self.put(0x200, roots.len() as u32);
        self.put(0x300, modal);
        self.put(0x304, 0);
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

fn sites() -> Sites {
    Sites { roots: 0x100, root_count: 0x200, modal: 0x300, companion: 0x304, main_gui: 0x9F_DB10, advisor: 0x9A_FD58 }
}

const BUTTON: u32 = 0x66_56C0;
/// The main screen form (its children are the screens after a game).
const MSF: u32 = 0x9F_86C0;
const MSF_VTABLE: u32 = 0x66_B3D8;
/// `Main_GUI`: keyed, its corner buttons among its children.
const MAIN_GUI: u32 = 0x9F_DB10;
const MAIN_GUI_VTABLE: u32 = 0x66_B570;

/// The summary replay as a trace of a retired game composed it
/// (`control_form` lines, 1920×1080): the form at (448, 156) relative to
/// the main screen form, its close button, the event list, the check box
/// and the combo box (its arrow a button) as forms of their own; the
/// playback buttons as 0x58C4F0 sets them.
fn replay() -> Fake {
    let mut m = Fake::default();
    let replay = 0xC8_8588;
    m.form(MSF, MSF_VTABLE, 0, 0, (0, 0), (1920, 1080), &[replay]);
    m.form(replay, REPLAY, 0x20, MSF, (448, 156), (1024, 768), &[0xC8_C980, 0xC8_AF44]);
    m.form(0xC8_C980, BUTTON, 0x100_0220, replay, (952, 718), (72, 48), &[]);
    m.form(0xC8_AF44, 0x67_2328, 0xA0, replay, (386, 438), (100, 24), &[0xC8_B4CC]);
    m.form(0xC8_B4CC, BUTTON, 0x100_0220, 0xC8_AF44, (78, 2), (22, 20), &[]);
    let (t, b) = (0x188, 0x1A5);
    for (at, l, r) in [(0x5160, 0x180, 0x1B4), (0x5170, 0x1CC, 0x1F7), (0x5180, 0x204, 0x230), (0x5190, 0x25A, 0x290), (0x51B0, 0x127, 0x164)] {
        m.rect(replay + at, [l, t, r, b]);
    }
    // The map's rectangle, which takes no click.
    m.rect(replay + 0x51A0, [0x127, 0x8E, 0x2D1, 0x158]);
    m.roots(&[replay], replay);
    m
}

#[test]
fn the_replays_painted_playback_buttons_take_the_pointer_beside_its_forms() {
    let m = replay();
    let found = controls(&m, &sites());
    let rects: Vec<[i32; 4]> = found.iter().map(|c| c.rect).collect();
    // The five buttons left to right, where the game draws them (the
    // retired game's screenshot: 773, 857, 929, 987 and 1077 across at
    // 1920×1080, all at y 563), inclusive of their right and bottom.
    let playback = [[743, 548, 62, 30], [832, 548, 53, 30], [908, 548, 44, 30], [964, 548, 45, 30], [1050, 548, 55, 30]];
    for r in playback {
        assert!(rects.contains(&r), "{r:?} in {rects:?}");
    }
    // Still the forms: the close button and the combo box's arrow.
    assert!(rects.contains(&[1400, 874, 72, 48]) && rects.contains(&[912, 596, 22, 20]), "{rects:?}");
    assert!(found.iter().all(|c| c.kind == ControlKind::Button));
    // Each is a control of its own, the same while the screen is open.
    let mut ids: Vec<u64> = found.iter().map(|c| c.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), found.len());
    assert_eq!(controls(&m, &sites()), found);
    // Nothing for the map.
    assert!(!rects.iter().any(|r| r[2] > 100 && r[3] > 100), "{rects:?}");
}

/// The replay's click handler 0x58CEF0 (in its own pixels): which button.
fn replay_click(m: &Fake, form: u32, x: i32, y: i32) -> Option<usize> {
    REPLAY_BUTTONS.iter().position(|&at| {
        let [l, t, r, b] = rect_at(m, form + at).unwrap();
        x >= l && x <= r && y >= t && y <= b
    })
}

#[test]
fn every_pixel_of_a_playback_target_is_that_button_to_the_game() {
    let m = replay();
    let form = 0xC8_8588;
    for (cell, index, [x, y, w, h]) in targets(&m, form) {
        assert_eq!(cell, CELL_BUTTON);
        for py in y..y + h {
            for px in x..x + w {
                assert_eq!(replay_click(&m, form, px, py), Some(index as usize), "({px}, {py})");
            }
        }
        // And not a pixel more.
        for (px, py) in [(x - 1, y), (x + w, y), (x, y - 1), (x, y + h)] {
            assert_ne!(replay_click(&m, form, px, py), Some(index as usize));
        }
    }
}

#[test]
fn the_rankings_close_button_is_as_large_as_its_sprite() {
    let mut m = Fake::default();
    let ranking = 0xE0_9F34;
    m.form(ranking, RANKING, 0, 0, (448, 156), (1024, 768), &[]);
    m.put(0xB3_F384, 72);
    m.put(0xB3_F388, 48);
    m.roots(&[ranking], 0);
    let found = controls(&m, &sites());
    assert_eq!(found.iter().map(|c| c.rect).collect::<Vec<_>>(), [[1400, 874, 72, 48]]);
    // The press handler 0x58A4E0: [0x3B8, 0x3B8 + w) × [0x2CE, 0x2CE + h).
    let [x, y, w, h] = targets(&m, ranking)[0].2;
    assert_eq!((x, y, x + w, y + h), (0x3B8, 0x2CE, 0x3B8 + 72, 0x2CE + 48));
}

#[test]
fn the_victory_screens_close_button_is_at_its_boards_corner() {
    let mut m = Fake::default();
    let victory = 0x22_9638;
    for (screen, board) in [((1920, 1080), (1024, 768)), ((1024, 768), (1024, 768)), ((1280, 1024), (1024, 768))] {
        m.form(victory, VICTORY, 0, 0, (0, 0), screen, &[]);
        m.put(0x9C_72F8, screen.0 as u32);
        m.put(0x9C_72FC, screen.1 as u32);
        m.put(victory + 0x6D8, board.0 as u32);
        m.put(victory + 0x6DC, board.1 as u32);
        m.roots(&[victory], 0);
        let (bx, by) = ((screen.0 - board.0) / 2, (screen.1 - board.1) / 2);
        assert_eq!(controls(&m, &sites()).iter().map(|c| c.rect).collect::<Vec<_>>(), [[bx + 0x3CC, by + 0xF, 42, 42]], "{screen:?}");
    }
}

#[test]
fn the_hall_of_fames_names_titles_and_all_leaders_box_take_the_pointer() {
    let mut m = Fake::default();
    let hof = 0xB2_C638;
    m.form(hof, HALL_OF_FAME, 0x20, 0, (0, 0), (1024, 768), &[0xB2_E490]);
    m.form(0xB2_E490, BUTTON, 0x100_0220, hof, (990, 730), (16, 20), &[]);
    // 32 races with the barbarians: 31 names (0x53F988), four a column.
    m.put(0x9C_3D74, 32);
    for i in 0..32i32 {
        let (l, t) = (0xDA + 95 * (i / 4), 0x7E + 16 * (i % 4));
        // Past the races: what the form's memory holds there.
        let r = if i < 31 { [l, t, l + 60, t + 16] } else { [0, 0, -1, -1] };
        m.rect(hof + 0x3904 + 16 * i as u32, r);
    }
    for (i, l) in [0x101, 0x13E, 0x216, 0x272, 0x2CE].into_iter().enumerate() {
        m.rect(hof + 0x3B04 + 16 * i as u32, [l, 0xE1, l + 40, 0xE1 + 14]);
    }
    m.rect(hof + 0x3B54, [0x26E, 0x62, 0x27E, 0x72]);
    m.rect(hof + 0x3B74, [0x2DA, 0x16, 0x34C, 0x66]);
    m.roots(&[hof], hof);
    let found = controls(&m, &sites());
    let kind = |c: &Control| crate::controls::advisors::cell_kind(c.id).unwrap_or(u32::MAX);
    assert_eq!(found.iter().filter(|c| kind(c) == CELL_CIV).count(), 31);
    assert_eq!(found.iter().filter(|c| kind(c) == CELL_HEADER).count(), 5);
    let rects: Vec<[i32; 4]> = found.iter().map(|c| c.rect).collect();
    // "Romans" at the top left of the names, "All Leaders On", the close
    // button (a form); not the empty place right of the title.
    assert!(rects.contains(&[0xDA, 0x7E, 61, 17]) && rects.contains(&[0x26E, 0x62, 17, 17]) && rects.contains(&[990, 730, 16, 20]), "{rects:?}");
    assert!(!rects.contains(&[0x2DA, 0x16, 0x34C - 0x2DA + 1, 0x66 - 0x16 + 1]));
    assert_eq!(found.len(), 31 + 5 + 1 + 1);
}

#[test]
fn an_opaque_screen_hides_the_buttons_of_later_forms_it_covers() {
    // The victory screen as the main screen form's first child, Main_GUI
    // (keyed: its buttons take the mouse, not its transparent pixels)
    // after it: the game's hit test tries the children in order and the
    // victory screen takes every click.
    let mut m = Fake::default();
    let victory = 0x22_9638;
    m.form(MSF, MSF_VTABLE, 0, 0, (0, 0), (1920, 1080), &[victory, MAIN_GUI]);
    m.form(victory, VICTORY, 0, MSF, (0, 0), (1920, 1080), &[]);
    m.form(MAIN_GUI, MAIN_GUI_VTABLE, 0x100_0020, MSF, (0, 0), (1920, 1080), &[MAIN_GUI + 0x8B30]);
    m.form(MAIN_GUI + 0x8B30, BUTTON, 0x100_0220, MAIN_GUI, (21, 12), (36, 30), &[]);
    m.put(0x9C_72F8, 1920);
    m.put(0x9C_72FC, 1080);
    m.put(victory + 0x6D8, 1024);
    m.put(victory + 0x6DC, 768);
    m.roots(&[MSF], 0);
    assert_eq!(controls(&m, &sites()).iter().map(|c| c.rect).collect::<Vec<_>>(), [[1420, 171, 42, 42]]);
    // Hidden, it covers nothing: the menu button is there again.
    m.put(victory + 0xA0, 0);
    assert_eq!(controls(&m, &sites()).iter().map(|c| c.rect).collect::<Vec<_>>(), [[21, 12, 36, 30]]);
    // A keyed form before them covers nothing either.
    m.put(victory + 0xA0, 1);
    m.put(victory + 0x9C, 0x100_0000);
    assert_eq!(controls(&m, &sites()).len(), 2);
}
