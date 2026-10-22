use super::*;
use crate::controls::tests::{list, sites, with_roots, Fake, BUTTON, PANEL, ROWS};

const SCREEN: u32 = 0x10_0000;
const SCREEN_VTABLE: u32 = 0x74_0000;

/// A scroll bar at `rect` (in its parent's pixels), its range and value.
fn bar(m: &mut Fake, f: u32, parent: u32, rect: [i32; 4], range: (i32, i32), value: i32) {
    m.form(f, SCROLL_BAR, 0, true, rect, parent, &[]);
    m.put(f + BAR_MIN, range.0 as u32);
    m.put(f + BAR_MAX, range.1 as u32);
    m.put(f + BAR_VALUE, value as u32);
}

/// A screen of 1024×768 (its wheel slot `wheel`) with `children`.
fn screen(m: &mut Fake, wheel: u32, children: &[u32]) {
    m.form(SCREEN, SCREEN_VTABLE, 0, true, [0, 0, 1024, 768], 0, children);
    m.put(SCREEN_VTABLE + WHEEL_SLOT, wheel);
    with_roots(m, &[SCREEN]);
}

#[test]
fn the_inventory_tells_forms_that_take_the_wheel_from_those_that_drop_it() {
    let mut m = Fake::default();
    screen(&mut m, NO_WHEEL, &[]);
    assert_eq!(classify(&m, SCREEN), Some(Scrolling::Drops { bars: 0, own_bar: false }));
    // A screen with its own handler (F1 Domestic's 0x5228D0).
    m.put(SCREEN_VTABLE + WHEEL_SLOT, 0x52_28D0);
    assert_eq!(classify(&m, SCREEN), Some(Scrolling::Own));
    // Scroll bars by their shape and range.
    bar(&mut m, 0x20_0000, SCREEN, [10, 10, 30, 300], (0, 40), 0);
    assert_eq!(classify(&m, 0x20_0000), Some(Scrolling::Bar { vertical: true, scrollable: true }));
    bar(&mut m, 0x20_0000, SCREEN, [10, 10, 300, 30], (0, 40), 0);
    assert_eq!(classify(&m, 0x20_0000), Some(Scrolling::Bar { vertical: false, scrollable: true }));
    bar(&mut m, 0x20_0000, SCREEN, [10, 10, 30, 300], (0, 0), 0);
    assert_eq!(classify(&m, 0x20_0000), Some(Scrolling::Bar { vertical: true, scrollable: false }));
    // A list box's handler acts only with its bar (Status1 & 8).
    list(&mut m, 0x30_0000, WHEEL, ROWS, 30, 0, 8);
    m.put(0x70_0000 + WHEEL_SLOT, 0x61_8F70);
    assert_eq!(classify(&m, 0x30_0000), Some(Scrolling::Own));
    list(&mut m, 0x30_0000, 0, ROWS, 30, 0, 8);
    m.put(0x70_0000 + WHEEL_SLOT, 0x61_8F70);
    assert_eq!(classify(&m, 0x30_0000), Some(Scrolling::Drops { bars: 0, own_bar: false }));
    // Diplomacy paints its arrows.
    m.form(0x40_0000, DIPLOMACY, 0, true, [0, 0, 1024, 768], 0, &[]);
    assert_eq!(classify(&m, 0x40_0000), Some(Scrolling::PaintedArrows));
    // A text form with its own bar (+0x26C), a text box with its (+0x574).
    m.form(0x50_0000, PANEL, WHEEL, true, [0, 0, 400, 300], 0, &[0x50_1000]);
    m.put(0x50_0000 + OWN_BAR, 0x50_1000);
    bar(&mut m, 0x50_1000, 0x50_0000, [380, 0, 400, 300], (0, 12), 3);
    assert_eq!(classify(&m, 0x50_0000), Some(Scrolling::Drops { bars: 1, own_bar: true }));
    m.form(0x60_0000, TEXT_BOX, 0, true, [0, 0, 400, 300], 0, &[0x60_0000 + TEXT_BOX_BAR]);
    bar(&mut m, 0x60_0000 + TEXT_BOX_BAR, 0x60_0000, [380, 0, 400, 300], (0, 12), 3);
    assert_eq!(classify(&m, 0x60_0000), Some(Scrolling::Drops { bars: 1, own_bar: true }));
    // A hidden bar or one with nowhere to go is not counted.
    m.put(0x60_0000 + TEXT_BOX_BAR + BAR_MAX, 0);
    assert_eq!(classify(&m, 0x60_0000), Some(Scrolling::Drops { bars: 0, own_bar: false }));
}

#[test]
fn the_wheel_steps_the_bar_beside_the_pointer_in_its_direction() {
    // Two lists side by side (the Civilopedia's): bars A (x 500..520,
    // rows 100..400) and B (x 1000..1020, rows 100..700).
    let mut m = Fake::default();
    let (a, b) = (0x20_0000, 0x21_0000);
    screen(&mut m, NO_WHEEL, &[a, b]);
    bar(&mut m, a, SCREEN, [500, 100, 520, 400], (0, 50), 10);
    bar(&mut m, b, SCREEN, [1000, 100, 1020, 700], (0, 50), 10);
    let s = sites();
    let up = |bar| Action::Arrow { bar, command: 1, steps: 1 };
    assert_eq!(route(&m, &s, SCREEN, (300, 200), 120), up(a));
    assert_eq!(route(&m, &s, SCREEN, (700, 200), 120), up(b));
    assert_eq!(route(&m, &s, SCREEN, (300, 500), 120), up(b));
    // Towards the user: down, a step a notch.
    assert_eq!(route(&m, &s, SCREEN, (300, 200), -240), Action::Arrow { bar: a, command: 2, steps: 2 });
    // On a bar itself (the game's hit form is the bar).
    assert_eq!(route(&m, &s, a, (510, 200), 120), up(a));
    // Above or right of every list: nothing.
    assert_eq!(route(&m, &s, SCREEN, (300, 50), 120), Action::Native);
    assert_eq!(route(&m, &s, SCREEN, (1021, 200), 120), Action::Native);
    // A horizontal bar (a slider) keeps the game's wheel: none.
    bar(&mut m, a, SCREEN, [500, 100, 800, 120], (0, 50), 10);
    assert_eq!(route(&m, &s, a, (510, 110), 120), Action::Native);
    // No delta, no hit form: nothing.
    assert_eq!(route(&m, &s, SCREEN, (700, 200), 0), Action::Native);
    assert_eq!(route(&m, &s, 0, (700, 200), 120), Action::Native);
}

#[test]
fn a_list_without_its_own_bar_scrolls_with_its_dialog_s_bar() {
    // The load/save file dialog: a list box without a bar of its own, the
    // dialog's bar right of it.
    let mut m = Fake::default();
    let (l, b) = (0x30_0000, 0x20_0000);
    list(&mut m, l, 0, ROWS, 30, 0, 8);
    m.put(0x70_0000 + WHEEL_SLOT, 0x61_8F70);
    m.put(l + PARENT, SCREEN);
    m.put(l + super::super::STATUS1, super::super::RELATIVE);
    screen(&mut m, NO_WHEEL, &[l, b]);
    bar(&mut m, b, SCREEN, [300, 100, 320, 260], (0, 22), 0);
    let s = sites();
    assert_eq!(route(&m, &s, l, (150, 150), -120), Action::Arrow { bar: b, command: 2, steps: 1 });
    // With its bar the list takes the wheel itself.
    m.put(l + super::super::STATUS1, super::super::RELATIVE | WHEEL);
    assert_eq!(route(&m, &s, l, (150, 150), -120), Action::Native);
}

#[test]
fn a_form_s_own_bar_scrolls_it_anywhere_on_it() {
    let mut m = Fake::default();
    let (t, b) = (0x50_0000, 0x50_1000);
    m.form(t, PANEL, WHEEL, true, [100, 100, 500, 400], 0, &[b]);
    m.put(t + OWN_BAR, b);
    bar(&mut m, b, t, [380, 0, 400, 300], (0, 12), 3);
    with_roots(&mut m, &[t]);
    let s = sites();
    assert_eq!(route(&m, &s, t, (120, 390), 120), Action::Arrow { bar: b, command: 1, steps: 1 });
    // Its button: the same (the walk goes up to it).
    m.form(0x50_2000, BUTTON, 0, true, [10, 10, 60, 30], t, &[]);
    assert_eq!(route(&m, &s, 0x50_2000, (120, 115), 120), Action::Arrow { bar: b, command: 1, steps: 1 });
}

#[test]
fn a_screen_that_scrolls_itself_gets_the_wheel_over_its_buttons_but_the_map_does_not() {
    let mut m = Fake::default();
    let button = 0x20_0000;
    screen(&mut m, 0x57_5F10, &[button]);
    m.form(button, BUTTON, 0, true, [200, 300, 260, 330], SCREEN, &[]);
    let s = sites();
    // F3 Military's own handler, at the pointer in the screen's pixels.
    assert_eq!(route(&m, &s, button, (210, 310), 120), Action::Forward { form: SCREEN, x: 210, y: 310 });
    // On the screen itself the game gives it the message anyway.
    assert_eq!(route(&m, &s, SCREEN, (210, 310), 120), Action::Native);
    // The map: over its interface panel's buttons the wheel does nothing.
    let main_screen = s.main_gui - MAIN_GUI_IN_SCREEN;
    m.form(main_screen, SCREEN_VTABLE, 0, true, [0, 0, 1024, 768], 0, &[s.main_gui]);
    m.form(s.main_gui, PANEL, 0, true, [0, 0, 1024, 768], main_screen, &[button]);
    m.form(button, BUTTON, 0, true, [200, 300, 260, 330], s.main_gui, &[]);
    // (Even with a bar beside it.)
    bar(&mut m, 0x21_0000, main_screen, [900, 0, 920, 768], (0, 10), 0);
    assert_eq!(route(&m, &s, button, (210, 310), 120), Action::Native);
    assert_eq!(route(&m, &s, main_screen, (210, 310), 120), Action::Native);
}

#[test]
fn diplomacy_clicks_the_painted_arrow_of_the_trade_table_under_the_pointer() {
    let mut m = Fake::default();
    let d = 0x40_0000;
    m.form(d, DIPLOMACY, 0, true, [0, 0, 1024, 768], 0, &[]);
    m.put(d + DIPLO_MODE, 2);
    m.put(d + DIPLO_PAGE, 1);
    with_roots(&mut m, &[d]);
    let entries = 0x41_0000;
    let regions: [(u32, [i32; 4]); 5] = [
        (1, [10, 10, 50, 50]),
        (3, [743, 566, 763, 586]),
        (4, [743, 590, 763, 610]),
        (5, [261, 566, 281, 586]),
        (6, [261, 590, 281, 610]),
    ];
    for (i, (id, r)) in regions.iter().enumerate() {
        let e = entries + 32 * i as u32;
        m.rect(e + 4, *r);
        m.put(e + 0x18, *id);
    }
    m.put(d + DIPLO_REGIONS + 0x50, entries);
    m.put(d + DIPLO_REGIONS + 0x58, regions.len() as u32);
    let s = sites();
    assert_eq!(route(&m, &s, d, (100, 300), 120), Action::Click { form: d, x: 271, y: 576, times: 1 });
    assert_eq!(route(&m, &s, d, (100, 300), -360), Action::Click { form: d, x: 271, y: 600, times: 3 });
    assert_eq!(route(&m, &s, d, (900, 300), 120), Action::Click { form: d, x: 753, y: 576, times: 1 });
    // The talk lines between the tables, the leader above them: nothing.
    assert_eq!(route(&m, &s, d, (500, 300), 120), Action::Native);
    assert_eq!(route(&m, &s, d, (100, 100), 120), Action::Native);
    // Below the trees: nothing.
    assert_eq!(route(&m, &s, d, (100, 0x267), 120), Action::Native);
    // Another page (no trees shown), a table without its arrow regions:
    // nothing.
    m.put(d + DIPLO_PAGE, 2);
    assert_eq!(route(&m, &s, d, (100, 300), 120), Action::Native);
    m.put(d + DIPLO_PAGE, 1);
    m.put(d + DIPLO_REGIONS + 0x58, 3);
    assert_eq!(route(&m, &s, d, (100, 300), 120), Action::Native);
}
