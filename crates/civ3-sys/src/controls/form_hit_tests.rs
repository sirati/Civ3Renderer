use super::*;
use crate::controls::tests::Fake;
use std::cell::RefCell;

/// Game memory that the hit test may write.
struct Game(RefCell<Fake>);

impl Mem for Game {
    fn u32(&self, addr: u32) -> Option<u32> {
        self.0.borrow().u32(addr)
    }
    fn u8(&self, addr: u32) -> Option<u8> {
        self.0.borrow().u8(addr)
    }
}

impl HitMem for Game {
    fn write_u32(&self, addr: u32, value: u32) -> Option<()> {
        self.0.borrow_mut().put(addr, value);
        Some(())
    }
}

const SITES: HitSites = HitSites {
    captures: [(0x500, true), (0x504, false), (0x508, true), (0x50C, false)],
    modal: 0x300,
    companion: 0x304,
    roots: 0x100,
    root_count: 0x200,
    on_frame: 0x600,
    screen: 0x610,
};

const HUD: u32 = 0x10_0000;
const BUTTON: u32 = 0x20_0000;
const MAP: u32 = 0x30_0000;

/// A form that receives its own events (`+0xB0`, as almost all do).
fn form(m: &mut Fake, addr: u32, status1: u32, rect: [i32; 4], parent: u32, children: &[u32]) {
    m.form(addr, super::super::tests::PANEL, status1, true, rect, parent, children);
    m.put(addr + RECEIVER, addr);
}

/// A keyed HUD over the whole screen with a button in it, over the map.
fn hud_over_map() -> Game {
    let mut m = Fake::default();
    form(&mut m, HUD, KEYED, [0, 0, 1024, 768], 0, &[BUTTON]);
    form(&mut m, BUTTON, 0, [10, 700, 74, 740], HUD, &[]);
    form(&mut m, MAP, 0, [0, 0, 1024, 768], 0, &[]);
    super::super::tests::with_roots(&mut m, &[HUD, MAP]);
    for at in [0x500, 0x504, 0x508, 0x50C] {
        m.put(at, 0);
    }
    Game(RefCell::new(m))
}

/// The HUD's drawn panel: its bottom band.
fn hud_panel(form: u32, _x: i32, y: i32) -> bool {
    form == HUD && y >= 660
}

#[test]
fn a_button_takes_its_click_in_its_own_pixels() {
    let game = hud_over_map();
    let hit = form_under(&game, &SITES, (20, 710), &hud_panel);
    assert_eq!(hit, Hit { form: BUTTON, point: (10, 10), on_frame: false });
    assert_eq!(game.u32(0x600), Some(0));
    assert_eq!((game.i32(0x610), game.i32(0x614)), (Some(20), Some(710)));
}

#[test]
fn a_keyed_form_takes_points_where_it_draws_and_passes_the_rest_on() {
    let game = hud_over_map();
    assert_eq!(form_under(&game, &SITES, (500, 720), &hud_panel).form, HUD);
    let hit = form_under(&game, &SITES, (500, 300), &hud_panel);
    assert_eq!(hit, Hit { form: MAP, point: (500, 300), on_frame: false });
    // The answer comes from the caller alone: no pixel is read.
    assert_eq!(form_under(&game, &SITES, (500, 720), &|_, _, _| false).form, MAP);
}

#[test]
fn a_rectangle_only_keyed_form_takes_its_whole_rectangle() {
    let game = hud_over_map();
    game.write_u32(HUD + STATUS1, KEYED | RECT_ONLY);
    assert_eq!(form_under(&game, &SITES, (500, 300), &hud_panel).form, HUD);
}

#[test]
fn a_point_on_no_form_hits_nothing() {
    let game = hud_over_map();
    let hit = form_under(&game, &SITES, (2000, 2000), &hud_panel);
    assert_eq!(hit.form, 0);
    assert_eq!(hit.point, (2000, 2000));
}

#[test]
fn events_go_to_the_form_s_receiver() {
    let game = hud_over_map();
    game.write_u32(BUTTON + RECEIVER, HUD);
    // The button only passes its events to the HUD: its own children are
    // still tested, it is still the one the point is on.
    let hit = form_under(&game, &SITES, (20, 710), &hud_panel);
    assert_eq!(hit, Hit { form: HUD, point: (10, 10), on_frame: false });
}

#[test]
fn a_modal_form_takes_every_click_even_beside_it() {
    let game = hud_over_map();
    let popup = 0x40_0000;
    let ok = 0x50_0000;
    let mut m = game.0.borrow_mut();
    form(&mut m, popup, 0, [300, 200, 700, 500], 0, &[ok]);
    form(&mut m, ok, 0, [20, 250, 120, 280], popup, &[]);
    m.put(0x300, popup);
    drop(m);
    assert_eq!(form_under(&game, &SITES, (330, 460), &hud_panel), Hit { form: ok, point: (10, 10), on_frame: false });
    // Beside it (over the HUD's button): still the popup, in its pixels.
    assert_eq!(form_under(&game, &SITES, (20, 710), &hud_panel), Hit { form: popup, point: (-280, 510), on_frame: false });
}

#[test]
fn the_companion_of_a_modal_form_takes_its_own_clicks() {
    let game = hud_over_map();
    let popup = 0x40_0000;
    let mut m = game.0.borrow_mut();
    form(&mut m, popup, 0, [300, 200, 700, 500], 0, &[]);
    m.put(0x300, popup);
    m.put(0x304, BUTTON);
    drop(m);
    assert_eq!(form_under(&game, &SITES, (20, 710), &hud_panel).form, BUTTON);
}

#[test]
fn a_capture_takes_everything_while_its_form_shows() {
    let game = hud_over_map();
    game.write_u32(0x508, BUTTON);
    let hit = form_under(&game, &SITES, (500, 300), &hud_panel);
    assert_eq!(hit, Hit { form: BUTTON, point: (490, -400), on_frame: false });
    // Not passed to a receiver.
    game.write_u32(BUTTON + RECEIVER, HUD);
    assert_eq!(form_under(&game, &SITES, (500, 300), &hud_panel).form, BUTTON);
    // Hidden: dropped, and the point is tested as usual.
    game.write_u32(BUTTON + STATUS2, 0);
    assert_eq!(form_under(&game, &SITES, (500, 300), &hud_panel).form, MAP);
    assert_eq!(game.u32(0x508), Some(0));
}

#[test]
fn a_framed_form_s_frame_band_is_its_frame() {
    let game = hud_over_map();
    let window = 0x60_0000;
    let mut m = game.0.borrow_mut();
    form(&mut m, window, 0, [8, 24, 408, 324], 0, &[]);
    m.rect(window + CLIENT, [100, 100, 516, 432]);
    m.put(window + STATUS2, 1 | FRAMED);
    m.put(0x200, 3);
    m.put(0x100, window);
    m.put(0x104, HUD);
    m.put(0x108, MAP);
    drop(m);
    // The title band: the frame, in the outer rectangle's pixels.
    let hit = form_under(&game, &SITES, (150, 110), &hud_panel);
    assert_eq!(hit, Hit { form: window, point: (50, 10), on_frame: true });
    assert_eq!(game.u32(0x600), Some(1));
    // Inside: the form's own pixels.
    let hit = form_under(&game, &SITES, (150, 150), &hud_panel);
    assert_eq!(hit, Hit { form: window, point: (42, 26), on_frame: false });
    assert_eq!(game.u32(0x600), Some(0));
}

#[test]
fn hidden_forms_and_forms_under_hidden_parents_take_nothing() {
    let game = hud_over_map();
    game.write_u32(HUD + STATUS2, 0);
    assert_eq!(form_under(&game, &SITES, (20, 710), &hud_panel).form, MAP);
}
