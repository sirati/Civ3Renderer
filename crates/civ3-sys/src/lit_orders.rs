//! The order buttons that show lit while their command waits for a tile
//! (docs §10.15, §16.12), from the game's targeting mode rather than from
//! how the command was given.
//!
//! The game lights a stack movement button only when it is clicked: the
//! click (0x557CA0 "all units of the stack", 0x557D2E "all units of this
//! type") sets all three of its `Images` to its highlighted sprite and the
//! other one's back to normal, then gives the stack go-to (0x4ED390). The
//! same command from a key (0x46B5FD) leaves the buttons as they are, and
//! nothing but the move itself (0x557E20) puts them back. The unit's
//! command buttons (`Command_Button`) never stay lit: they show pressed
//! only while the mouse holds them. Here the state follows the mode the
//! game is in (`Main_Screen_Form::Mode_Action`), whatever gave it.
//!
//! Only the PCGames build is decoded (`crate::controls::sites`).

use crate::controls::Mem;
use crate::layout as L;

/// `Main_GUI` lies at `Main_Screen_Form + 0x5450`.
const MAIN_GUI_IN_SCREEN: u32 = 0x5450;
/// The stack go-to's "this type only" flag (`Main_Screen_Form+0x4E96`,
/// set by 0x4ED390 with the command).
const TYPE_ONLY: u32 = 0x4E96;
/// The two stack movement buttons (`Button`s in `Main_GUI`, before the
/// right panel's seven) and their sprites (`Sprite`s in `Main_GUI`):
/// normal, rollover, highlighted.
const STACK_ALL: u32 = 0xC8A4;
const STACK_TYPE: u32 = 0xCF78;
const STACK_ALL_SPRITES: u32 = 0x143C4;
const STACK_TYPE_SPRITES: u32 = 0x14474;
/// The unit's 42 command buttons (`Command_Button`, 0x6DC bytes: a
/// `Button` and its `Unit_Command_Values` value at +0x6D4).
const ORDERS: u32 = 0x1_6CE4;
const ORDER_STRIDE: u32 = 0x6DC;
const ORDER_COUNT: u32 = 42;
const ORDER_COMMAND: u32 = 0x6D4;
/// `Unit_Mode_Actions` values are below this (one pointer of
/// `Art/Cursor.pcx` each); 0 and `IDC_ARROW` are none.
const MODES: i32 = 23;

/// `Unit_Mode_Actions`: the go-to modes, one unit or a stack (land and
/// sea units; air units: their rebase, 0x4D94B0).
pub const GO_TO: i32 = crate::targeting::GO_TO;
pub const STACK_GO_TO: i32 = crate::targeting::STACK_GO_TO;
pub const AIR_GO_TO: i32 = 0xA;
pub const AIR_STACK_GO_TO: i32 = 0x13;

/// The mode a command button's command waits in for its tile, where
/// `Main_Screen_Form::issue_command` (0x4DAB30) sets one: go to (0x4DABC4),
/// the class 1 commands at 0x4DAD8C, 0x4DAE2E, 0x4DB02B, the class 3 ones
/// (0x4D9280, 0x4D9380, 0x4D94B0, 0x4D95B0) and 0x4D96E0.
pub fn mode_of(command: u32) -> Option<i32> {
    Some(match command {
        0x10 => GO_TO,
        0x1000_0004 => 0x8,
        0x1000_0010 => 0x6,
        0x1000_8000 => 0x16,
        0x3000_0001 => 0x7,
        0x3000_0002 => 0x9,
        0x3000_0008 => AIR_GO_TO,
        0x3000_0010 => 0xB,
        0x4000_0002 => 0xF,
        _ => return None,
    })
}

/// What a button is to show: of the `sprites` it may be drawn from, the
/// one it is drawn from now goes for `show`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lit {
    /// The button (its form; its canvas at `+ BASE_FORM_DATA_CANVAS + 4`).
    pub button: u32,
    pub sprites: Vec<u32>,
    pub show: u32,
}

/// The targeting mode as the order buttons go by it: the mode, whether a
/// go-to was given (not a unit dragged), and a stack go-to's type flag.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Mode {
    pub mode: i32,
    pub armed: bool,
    pub type_only: bool,
}

impl Mode {
    /// The mode of the main screen whose `Main_GUI` is at `main_gui`.
    pub fn read(mem: &dyn Mem, main_gui: u32) -> Option<Mode> {
        let main = main_gui.checked_sub(MAIN_GUI_IN_SCREEN)?;
        Some(Mode {
            mode: mem.i32(main + L::MAIN_SCREEN_FORM_MODE_ACTION)?,
            armed: mem.u8(main + L::MAIN_SCREEN_FORM_FIELD_4E80)? != 0,
            type_only: mem.u8(main + TYPE_ONLY)? != 0,
        })
    }

    /// A go-to mode counts once given (0x4DAC07), not while a unit is
    /// dragged with the right button.
    fn waits(&self, mode: i32) -> bool {
        self.mode == mode && (self.armed || !matches!(mode, GO_TO | STACK_GO_TO))
    }

    fn stack(&self) -> bool {
        self.waits(STACK_GO_TO) || self.waits(AIR_STACK_GO_TO)
    }
}

/// The buttons whose look the mode decides, and the sprite each is to be
/// drawn from: the stack buttons lit while their stack go-to waits (and
/// put back where the game left one lit without it), the command button
/// whose command waits for its tile drawn highlighted (its third image).
pub fn lit(mem: &dyn Mem, main_gui: u32, mode: Mode) -> Vec<Lit> {
    let image = |button: u32, s: u32| mem.u32(button + L::BUTTON_IMAGES + 4 * s);
    let mut out = Vec::new();
    for (field, sprites, type_only) in [(STACK_ALL, STACK_ALL_SPRITES, false), (STACK_TYPE, STACK_TYPE_SPRITES, true)] {
        let button = main_gui + field;
        let [normal, rollover, highlighted] = [0, 1, 2].map(|s| main_gui + sprites + s * L::SPRITE_SIZE);
        let all = vec![normal, rollover, highlighted];
        if mode.stack() && mode.type_only == type_only {
            out.push(Lit { button, sprites: all, show: highlighted });
        } else if image(button, 0) == Some(highlighted) {
            // (Lit by a click whose order is gone.)
            out.push(Lit { button, sprites: all, show: normal });
        }
    }
    // (No command waits: none of them is read.)
    let waiting = (1..MODES).contains(&mode.mode);
    for k in (0..ORDER_COUNT).filter(|_| waiting) {
        let button = main_gui + ORDERS + k * ORDER_STRIDE;
        let Some(command) = mem.u32(button + ORDER_COMMAND) else { continue };
        if !mode_of(command).is_some_and(|m| mode.waits(m)) {
            continue;
        }
        let Some(images) = [0, 1, 2].map(|s| image(button, s)).into_iter().collect::<Option<Vec<u32>>>() else { continue };
        if images[2] != 0 {
            out.push(Lit { button, show: images[2], sprites: images.into_iter().filter(|&s| s != 0).collect() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const GUI: u32 = 0x9F_DB10;
    const MAIN: u32 = GUI - MAIN_GUI_IN_SCREEN;

    #[derive(Default)]
    struct Fake(HashMap<u32, u32>);

    impl Mem for Fake {
        fn u32(&self, addr: u32) -> Option<u32> {
            Some(self.0.get(&addr).copied().unwrap_or(0))
        }
        fn u8(&self, addr: u32) -> Option<u8> {
            Some(self.0.get(&addr).copied().unwrap_or(0) as u8)
        }
    }

    fn sprite(base: u32, s: u32) -> u32 {
        GUI + base + s * L::SPRITE_SIZE
    }

    /// A unit with Go To (button 3) and Bombard (button 5).
    fn game() -> Fake {
        let mut m = Fake::default();
        for (k, command) in [(3, 0x10), (5, 0x3000_0001)] {
            let b = GUI + ORDERS + k * ORDER_STRIDE;
            m.0.insert(b + ORDER_COMMAND, command);
            for s in 0..3 {
                m.0.insert(b + L::BUTTON_IMAGES + 4 * s, 0x5000 + 0x100 * k + s);
            }
        }
        for (field, sprites) in [(STACK_ALL, STACK_ALL_SPRITES), (STACK_TYPE, STACK_TYPE_SPRITES)] {
            for s in 0..3 {
                m.0.insert(GUI + field + L::BUTTON_IMAGES + 4 * s, sprite(sprites, s));
            }
        }
        m
    }

    fn set_mode(m: &mut Fake, mode: i32, armed: bool, type_only: bool) {
        m.0.insert(MAIN + L::MAIN_SCREEN_FORM_MODE_ACTION, mode as u32);
        m.0.insert(MAIN + L::MAIN_SCREEN_FORM_FIELD_4E80, armed as u32);
        m.0.insert(MAIN + TYPE_ONLY, type_only as u32);
    }

    fn read(m: &Fake) -> Vec<Lit> {
        lit(m, GUI, Mode::read(m, GUI).unwrap())
    }

    #[test]
    fn no_command_waiting_lights_nothing() {
        assert_eq!(read(&game()), []);
    }

    #[test]
    fn a_stack_go_to_lights_its_button_however_it_was_given() {
        // A key gives it (the buttons' images untouched): the mode decides.
        let mut m = game();
        set_mode(&mut m, STACK_GO_TO, true, false);
        let all = vec![sprite(STACK_ALL_SPRITES, 0), sprite(STACK_ALL_SPRITES, 1), sprite(STACK_ALL_SPRITES, 2)];
        assert_eq!(read(&m), [Lit { button: GUI + STACK_ALL, sprites: all, show: sprite(STACK_ALL_SPRITES, 2) }]);
        set_mode(&mut m, STACK_GO_TO, true, true);
        assert_eq!(read(&m).iter().map(|l| (l.button, l.show)).collect::<Vec<_>>(), [(GUI + STACK_TYPE, sprite(STACK_TYPE_SPRITES, 2))]);
        // An air stack's rebase too.
        set_mode(&mut m, AIR_STACK_GO_TO, false, false);
        assert_eq!(read(&m).iter().map(|l| (l.button, l.show)).collect::<Vec<_>>(), [(GUI + STACK_ALL, sprite(STACK_ALL_SPRITES, 2))]);
    }

    #[test]
    fn a_stack_button_a_click_left_lit_goes_out_with_its_order() {
        // The game's click: all three images highlighted, the order gone.
        let mut m = game();
        for s in 0..3 {
            m.0.insert(GUI + STACK_TYPE + L::BUTTON_IMAGES + 4 * s, sprite(STACK_TYPE_SPRITES, 2));
        }
        assert_eq!(read(&m).iter().map(|l| (l.button, l.show)).collect::<Vec<_>>(), [(GUI + STACK_TYPE, sprite(STACK_TYPE_SPRITES, 0))]);
        // While it waits: lit (as the game shows it anyway).
        set_mode(&mut m, STACK_GO_TO, true, true);
        assert_eq!(read(&m).iter().map(|l| (l.button, l.show)).collect::<Vec<_>>(), [(GUI + STACK_TYPE, sprite(STACK_TYPE_SPRITES, 2))]);
    }

    #[test]
    fn the_command_button_waiting_for_its_tile_shows_highlighted() {
        let mut m = game();
        let go_to = GUI + ORDERS + 3 * ORDER_STRIDE;
        set_mode(&mut m, GO_TO, true, false);
        assert_eq!(read(&m), [Lit { button: go_to, sprites: vec![0x5300, 0x5301, 0x5302], show: 0x5302 }]);
        // A unit dragged with the right button: no command given.
        set_mode(&mut m, GO_TO, false, false);
        assert_eq!(read(&m), []);
        set_mode(&mut m, 0x7, false, false);
        assert_eq!(read(&m).iter().map(|l| l.button).collect::<Vec<_>>(), [GUI + ORDERS + 5 * ORDER_STRIDE]);
        // Cancelled (Esc: no mode, or the arrow).
        set_mode(&mut m, 0x7F00, false, false);
        assert_eq!(read(&m), []);
    }

    #[test]
    fn every_mapped_command_waits_in_a_targeting_mode() {
        for command in [0x10, 0x1000_0004, 0x1000_0010, 0x1000_8000, 0x3000_0001, 0x3000_0002, 0x3000_0008, 0x3000_0010, 0x4000_0002] {
            assert!(mode_of(command).is_some_and(|m| (1..23).contains(&m)), "{command:#x}");
        }
        assert_eq!(mode_of(0x1), None);
    }
}
