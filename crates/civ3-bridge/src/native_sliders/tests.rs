use super::*;
use std::collections::HashMap;
use civ3_sys::controls::Mem;

#[derive(Default)]
struct Memory(HashMap<u32, u32>);
impl Mem for Memory {
    fn u32(&self, address: u32) -> Option<u32> { self.0.get(&address).copied() }
    fn u8(&self, address: u32) -> Option<u8> { self.u32(address).map(|v| v as u8) }
}
fn slider_in_canvas(address: u32, exact: i32, canvas: u32) -> Slider {
    let mut mem = Memory::default();
    for (offset, value) in [
        (0, civ3_sys::sliders::VTABLE), (0x130, 0x2000), (0x278, canvas),
        (0x1AC, 0), (0x1B0, 0), (0x1B4, 406), (0x1B8, 20),
        (0x574, 0), (0x578, 2), (0x57C, 0), (0x580, 0),
        (0x584, 2_000_003), (0x588, 0), (0x58C, exact as u32),
        (0x5AC, 1), (0x5B0, 1), (0x5B4, 19), (0x5B8, 19),
    ] { mem.0.insert(address + offset, value); }
    mem.0.insert(0x2000, 0x66E628);
    Slider::read(&mem, address).unwrap()
}
fn slider(address: u32, exact: i32) -> Slider { slider_in_canvas(address, exact, 0x3000) }

struct Fake {
    current: Option<Slider>,
    accessible: bool,
    writes: Vec<i32>,
}
impl Fake {
    fn new() -> Self { Self { current: Some(slider(0x10000, 1_000_001)), accessible: true, writes: vec![] } }
}
impl Driver for Fake {
    fn read(&self, id: u64, accessible: bool) -> Option<Slider> {
        self.current.filter(|slider| id == u64::from(slider.identity.address) && (!accessible || self.accessible))
    }
    fn set(&mut self, original: Slider, exact: i32) {
        if let Some(current) = &mut self.current && current.same_control(original) {
            current.exact = exact;
            self.writes.push(exact);
        }
    }
}
fn request(action: SliderAction, value: i32) -> Request { Request { id: 0x10000, action, value } }

#[test]
fn entering_and_committing_without_motion_never_changes_or_rounds_a_native_value() {
    let mut driver = Fake::new();
    let mut transaction = Transaction::default();
    transaction.apply(request(SliderAction::Begin, 0), &mut driver);
    assert_eq!(driver.current.unwrap().exact, 1_000_001);
    assert!(driver.writes.is_empty());
    transaction.apply(request(SliderAction::Commit, 1_000_000), &mut driver);
    assert!(driver.writes.is_empty());
    assert!(transaction.original.is_none());
}

#[test]
fn cancel_restores_the_exact_integer_instead_of_its_rounded_wire_position() {
    let mut driver = Fake::new();
    let mut transaction = Transaction::default();
    transaction.apply(request(SliderAction::Begin, 0), &mut driver);
    transaction.apply(request(SliderAction::Preview, i32::MAX), &mut driver);
    assert_eq!(driver.current.unwrap().exact, 2_000_003);
    transaction.apply(request(SliderAction::Cancel, 0), &mut driver);
    assert_eq!(driver.writes, [2_000_003, 1_000_001]);
    assert!(transaction.original.is_none());
}

#[test]
fn left_commit_keeps_the_latest_preview_and_stale_requests_do_nothing() {
    let mut driver = Fake::new();
    let mut transaction = Transaction::default();
    transaction.apply(request(SliderAction::Preview, 0), &mut driver);
    assert!(driver.writes.is_empty());
    transaction.apply(request(SliderAction::Begin, 0), &mut driver);
    transaction.apply(request(SliderAction::Preview, 250_000), &mut driver);
    transaction.apply(request(SliderAction::Commit, 0), &mut driver);
    transaction.apply(request(SliderAction::Cancel, 0), &mut driver);
    assert_eq!(driver.writes, [500_001]);
    assert_eq!(driver.current.unwrap().exact, 500_001);
}

#[test]
fn hiding_or_disabling_a_live_control_rolls_back_before_another_preview() {
    let mut driver = Fake::new();
    let mut transaction = Transaction::default();
    transaction.apply(request(SliderAction::Begin, 0), &mut driver);
    transaction.apply(request(SliderAction::Preview, -99), &mut driver);
    driver.accessible = false;
    transaction.apply(request(SliderAction::Preview, 1_000_000), &mut driver);
    assert_eq!(driver.writes, [0, 1_000_001]);
    assert!(transaction.original.is_none());
}

#[test]
fn replacing_or_destroying_the_control_cannot_restore_into_another_object() {
    for replacement in [None, Some(slider(0x20000, 99)), Some(slider_in_canvas(0x10000, 99, 0x4000))] {
        let mut driver = Fake::new();
        let mut transaction = Transaction::default();
        transaction.apply(request(SliderAction::Begin, 0), &mut driver);
        transaction.apply(request(SliderAction::Preview, 0), &mut driver);
        driver.current = replacement;
        transaction.cancel(&mut driver);
        assert_eq!(driver.writes, [0]);
        assert_eq!(driver.current, replacement);
    }
}

#[test]
fn begin_on_another_slider_restores_the_first_before_editing_the_second() {
    let mut driver = Fake::new();
    let mut transaction = Transaction::default();
    transaction.apply(request(SliderAction::Begin, 0), &mut driver);
    transaction.apply(request(SliderAction::Preview, 0), &mut driver);
    transaction.apply(Request { id: 0x20000, ..request(SliderAction::Begin, 0) }, &mut driver);
    assert_eq!(driver.writes, [0, 1_000_001]);
    assert!(transaction.original.is_none());
}

#[test]
fn pending_previews_coalesce_without_crossing_a_commit_or_foreign_input() {
    let mut queue = Requests::default();
    let (first, post) = queue.push(request(SliderAction::Preview, 0));
    assert!(post);
    for value in 1..10_000 { assert_eq!(queue.push(request(SliderAction::Preview, value)), (first, false)); }
    queue.barrier();
    let (after_key, post) = queue.push(request(SliderAction::Preview, 20_000));
    assert!(post && after_key != first);
    let (commit, _) = queue.push(request(SliderAction::Commit, 0));
    let (after_commit, post) = queue.push(request(SliderAction::Preview, 30_000));
    assert!(post && after_commit != after_key);
    assert_eq!(queue.take(first).unwrap().value, 9_999);
    assert_eq!(queue.take(after_key).unwrap().value, 20_000);
    assert_eq!(queue.take(commit).unwrap().action, SliderAction::Commit);
    assert_eq!(queue.take(after_commit).unwrap().value, 30_000);
    assert!(queue.take(first).is_none());
}
