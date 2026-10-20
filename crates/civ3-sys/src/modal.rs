//! The game's modal dialogs, and what it takes to act on the map while one
//! is open (docs/modal-input.md): the modal loop's message pump call, the
//! pumps of the human-turn loop and of animations, the globals that say
//! which dialog is open, and the map functions to call.
//!
//! Only the PCGames build is decoded; the others have no sites (`None`).

use inproc::Ptr32;

use crate::{Build, Civ3, Civ3Error, layout as L};

inproc::call_site_hook! {
    /// The message pump call in `Base_Form::m68_Show_Dialog`'s loop, once
    /// per pass while a modal form is open.
    pub mod modal_pump() -> u32;
}

inproc::call_site_hook! {
    /// The human-turn loop's pump while the selected unit walks its go-to.
    pub mod human_pump_goto() -> u32;
}

inproc::call_site_hook! {
    /// The human-turn loop's pump while it waits for the player.
    pub mod human_pump_idle() -> u32;
}

inproc::call_site_hook! {
    /// A pump in the middle of an animation (the game is re-entrant there).
    pub mod animation_pump_a() -> u32;
}

inproc::call_site_hook! {
    /// The other mid-animation pump.
    pub mod animation_pump_b() -> u32;
}

inproc::thiscall_fn! {
    /// `Unit *Main_Screen_Form::find_visible_unit(this, tile_x, tile_y,
    /// Unit *excluded)` (C3X redirects it).
    pub fn find_visible_unit(main_screen; tile_x, tile_y, excluded) -> u32;
}

inproc::thiscall_fn! {
    /// `void Unit::select(this)`, as the map's left-click handler calls it.
    pub fn unit_select(unit;) -> ();
}

inproc::thiscall_fn! {
    /// `void Main_Screen_Form::perform_action_on_tile(this, enum
    /// Unit_Mode_Actions action, x, y)` for the selected unit (C3X
    /// redirects it).
    pub fn perform_action_on_tile(main_screen; action, tile_x, tile_y) -> ();
}

inproc::thiscall_fn! {
    /// `char Main_Screen_Form::issue_command(this, int command, Unit *unit)`
    /// (only the low byte of the result is set).
    pub fn issue_command(main_screen; command, unit) -> u32;
}

inproc::thiscall_fn! {
    /// `bool Unit::can_perform_command(this, int unit_command_value)` (C3X
    /// redirects it; only the low byte of the result is set).
    pub fn unit_can_perform_command(unit; command) -> u32;
}

inproc::thiscall_fn! {
    /// `void City_Form::draw(this)` (City_Form vtable +0x58, 0x419770;
    /// C3X redirects it). The city screen calls it itself after giving a
    /// unit in the city an order (0x4228BE).
    pub fn city_form_draw(city_form;) -> ();
}

/// A `call rel32` site and its exact bytes (checked before patching).
pub type CallSite = (u32, [u8; 5]);

/// The pump call sites (all call the pump at 0x624B90).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PumpSites {
    /// In the modal loop (0x6052F0).
    pub modal: CallSite,
    /// In the human-turn loop (0x4DFC50): go-to stepping, waiting.
    pub human_goto: CallSite,
    pub human_idle: CallSite,
    /// In the middle of animations.
    pub animation: [CallSite; 2],
}

pub fn pump_sites(build: Build) -> Option<PumpSites> {
    match build {
        Build::PcGames => Some(PumpSites {
            modal: (0x60_53B4, [0xE8, 0xD7, 0xF7, 0x01, 0x00]),
            human_goto: (0x4D_FE24, [0xE8, 0x67, 0x4D, 0x14, 0x00]),
            human_idle: (0x4D_FE5C, [0xE8, 0x2F, 0x4D, 0x14, 0x00]),
            animation: [(0x4D_BF29, [0xE8, 0x62, 0x8C, 0x14, 0x00]), (0x4D_7AE8, [0xE8, 0xA3, 0xD0, 0x14, 0x00])],
        }),
        Build::Gog | Build::Steam => None,
    }
}

/// A function's address and first bytes (checked before calling; a mod's
/// `jmp rel32` there is accepted too).
pub type FunctionSite = (u32, &'static [u8]);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct FunctionSites {
    pub find_visible_unit: FunctionSite,
    pub unit_select: FunctionSite,
    pub perform_action_on_tile: FunctionSite,
    pub issue_command: FunctionSite,
    pub unit_can_perform_command: FunctionSite,
    pub city_form_draw: FunctionSite,
}

pub fn function_sites(build: Build) -> Option<FunctionSites> {
    match build {
        Build::PcGames => Some(FunctionSites {
            // sub esp,1Ch / push ebx / push ebp
            find_visible_unit: (0x4E_3F50, &[0x83, 0xEC, 0x1C, 0x53, 0x55]),
            // mov eax,fs:[0] / push -1
            unit_select: (0x5C_5FA0, &[0x64, 0xA1, 0x00, 0x00, 0x00, 0x00, 0x6A, 0xFF]),
            // push ebx / push ebp / mov edx,[esp+0Ch]
            perform_action_on_tile: (0x4E_6E70, &[0x53, 0x55, 0x8B, 0x54, 0x24, 0x0C]),
            // push ebx / push ebp / push esi / mov esi,[esp+14h]
            issue_command: (0x4D_AB30, &[0x53, 0x55, 0x56, 0x8B, 0x74, 0x24, 0x14]),
            // push ecx / push ebx / push ebp / push esi / push edi
            unit_can_perform_command: (0x5C_17E0, &[0x51, 0x53, 0x55, 0x56, 0x57]),
            // sub esp,2B4h
            city_form_draw: (0x41_9770, &[0x81, 0xEC, 0xB4, 0x02, 0x00, 0x00]),
        }),
        Build::Gog | Build::Steam => None,
    }
}

/// Globals of the dialog machinery and the turn.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Globals {
    /// The modal form (`Base_Form *`, 0: none).
    pub modal_form: u32,
    /// How many dialogs are open (at most 4).
    pub dialog_depth: u32,
    /// The game type (`is_online_game`: 4 and 5 are online).
    pub game_type: u32,
    /// `p_is_pbem_game` (a byte).
    pub pbem: u32,
    /// The forms acting is allowed under: `Advisor_GUI`, the Civilopedia.
    pub advisor_form: u32,
    pub civilopedia_form: u32,
    /// The city screen (`p_city_form`), also a form acting is allowed under.
    pub city_form: u32,
    /// The diplomacy screen (acting is not allowed under it).
    pub diplo_form: u32,
}

/// The human-turn loop's flag (a byte of `Main_Screen_Form`, set while
/// 0x4DFC50 runs).
const MAIN_SCREEN_FORM_TURN_FLAG: u32 = 0x2_E198;

pub fn globals(build: Build) -> Option<Globals> {
    match build {
        Build::PcGames => Some(Globals {
            modal_form: 0xCA_D384,
            dialog_depth: 0xCA_D388,
            game_type: 0x9A_FD34,
            pbem: 0xA5_2774,
            advisor_form: 0x9A_FD58,
            civilopedia_form: 0x9E_85B0,
            city_form: 0x74_09C0,
            diplo_form: 0xA3_51C0,
        }),
        Build::Gog | Build::Steam => None,
    }
}

/// `Unit_Mode_Actions::UMA_Go_To`.
pub const ACTION_GO_TO: u32 = 1;

/// `UnitStateType::UnitState_Go_To`: a go-to order the unit walks on
/// later turns (`perform_action_on_tile` sets it for a far tile, 0x4E6EED).
pub const UNIT_STATE_GO_TO: i32 = 0x10;

/// `Unit_Command_Values` the map's keys give.
pub mod command {
    pub const SKIP_TURN: u32 = 0x1;
    pub const WAIT: u32 = 0x2;
    pub const FORTIFY: u32 = 0x4;
    /// Arms go-to: the next map click picks the destination (the map's G,
    /// and what holding the button on the unit does, 0x4EE81A).
    pub const GO_TO: u32 = 0x10;
    pub const SENTRY: u32 = 0x40;
}

/// What the dialog machinery and the turn are doing now.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct DialogState {
    pub modal_form: u32,
    pub depth: i32,
    /// The human-turn loop's flag.
    pub turn_flag: bool,
    pub online: bool,
    pub pbem: bool,
}

/// A coherent bounded snapshot of the game's modal stack, oldest first.
/// Sample on the game thread together with the UI's presented command frame.
pub fn modal_stack(mem: &dyn crate::controls::Mem, build: Build) -> Option<[u32; 4]> {
    if build != Build::PcGames { return None; }
    let depth = mem.u32(0xCA_D388)?;
    if depth > 4 { return None; }
    let mut forms = [0; 4];
    for index in 0..depth as usize {
        let form = mem.u32(0xCA_CD20 + index as u32 * 4)?;
        if form == 0 || forms[..index].contains(&form) { return None; }
        forms[index] = form;
    }
    let top = if depth == 0 { 0 } else { forms[depth as usize - 1] };
    (mem.u32(0xCA_D384)? == top).then_some(forms)
}

impl Civ3 {
    /// The open modal form (0: none), or `None` for an undecoded build.
    pub fn modal_form(&self) -> Result<Option<u32>, Civ3Error> {
        let Some(g) = globals(self.build) else { return Ok(None) };
        Ok(Some(Ptr32::<u32>::new(g.modal_form).read()?))
    }

    pub fn dialog_state(&self) -> Result<Option<DialogState>, Civ3Error> {
        let Some(g) = globals(self.build) else { return Ok(None) };
        let game_type = Ptr32::<u32>::new(g.game_type).read()?;
        Ok(Some(DialogState {
            modal_form: Ptr32::<u32>::new(g.modal_form).read()?,
            depth: Ptr32::<i32>::new(g.dialog_depth).read()?,
            turn_flag: Ptr32::<u8>::new(self.addrs.main_screen_form + MAIN_SCREEN_FORM_TURN_FLAG).read()? != 0,
            online: game_type == 4 || game_type == 5,
            pbem: Ptr32::<u8>::new(g.pbem).read()? != 0,
        }))
    }

    /// The selected unit (`Unit *`, 0: none).
    pub fn current_unit_ptr(&self) -> Result<u32, Civ3Error> {
        Ok(Ptr32::<u32>::new(self.addrs.main_screen_form + L::MAIN_SCREEN_FORM_CURRENT_UNIT).read()?)
    }

    /// The viewing player.
    pub fn player_civ(&self) -> Result<i32, Civ3Error> {
        Ok(Ptr32::<i32>::new(self.addrs.main_screen_form + L::MAIN_SCREEN_FORM_PLAYER_CIVID).read()?)
    }

    /// Id, tile and owner of the unit at `unit` (a `Unit *`).
    pub fn unit_at_ptr(&self, unit: u32) -> Result<UnitPlace, Civ3Error> {
        let body = Ptr32::<u8>::new(unit).offset::<u8>(L::UNIT_BODY)?;
        let f = |at| -> Result<i32, Civ3Error> { Ok(body.offset::<i32>(at)?.read()?) };
        Ok(UnitPlace { id: f(L::UNIT_BODY_ID)?, x: f(L::UNIT_BODY_X)?, y: f(L::UNIT_BODY_Y)?, civ: f(L::UNIT_BODY_CIVID)?, state: f(L::UNIT_BODY_UNITSTATE)? })
    }

    /// Every unit on tile (x, y).
    pub fn units_on(&self, x: i32, y: i32) -> Result<Vec<UnitPlace>, Civ3Error> {
        let list = Ptr32::<u8>::new(self.addrs.units);
        let items: Ptr32<u8> = list.offset::<Ptr32<u8>>(L::CITIES_CITIES)?.read()?;
        let last: i32 = list.offset::<i32>(L::CITIES_LASTINDEX)?.read()?;
        let mut out = Vec::new();
        if items.is_null() {
            return Ok(out);
        }
        for i in 0..last.clamp(-1, 0xFFFF).saturating_add(1) as u32 {
            let body: Ptr32<u8> = items.offset::<Ptr32<u8>>(i * L::CITYITEM_SIZE + L::CITYITEM_CITY)?.read()?;
            if body.is_null() {
                continue;
            }
            let f = |at| -> Result<i32, Civ3Error> { Ok(body.offset::<i32>(at)?.read()?) };
            if (f(L::UNIT_BODY_X)?, f(L::UNIT_BODY_Y)?) == (x, y) {
                out.push(UnitPlace { id: f(L::UNIT_BODY_ID)?, x, y, civ: f(L::UNIT_BODY_CIVID)?, state: f(L::UNIT_BODY_UNITSTATE)? });
            }
        }
        Ok(out)
    }
}

/// Where a unit is and whose it is.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct UnitPlace {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub civ: i32,
    pub state: i32,
}

/// The `Unit *` around a unit body pointer is `UNIT_BODY` before it.
pub fn unit_of_body(body: u32) -> u32 {
    body.wrapping_sub(L::UNIT_BODY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pump_sites_call_the_pump() {
        // Every site is a `call rel32` to the pump at 0x624B90.
        let s = pump_sites(Build::PcGames).unwrap();
        for (at, bytes) in [s.modal, s.human_goto, s.human_idle, s.animation[0], s.animation[1]] {
            assert_eq!(bytes[0], 0xE8);
            let rel = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
            assert_eq!(at.wrapping_add(5).wrapping_add(rel as u32), 0x62_4B90, "{at:#x}");
        }
        assert!(pump_sites(Build::Gog).is_none());
    }

    #[test]
    fn the_screens_are_the_recovered_forms() {
        let g = globals(Build::PcGames).unwrap();
        assert_eq!((g.city_form, g.advisor_form, g.civilopedia_form, g.diplo_form), (0x74_09C0, 0x9A_FD58, 0x9E_85B0, 0xA3_51C0));
        // The city screen's draw is its vtable's +0x58 (0x665908 holds it).
        assert_eq!(function_sites(Build::PcGames).unwrap().city_form_draw.0, 0x41_9770);
    }

    #[test]
    fn turn_flag_is_the_documented_global() {
        assert_eq!(Build::PcGames.addresses().main_screen_form + MAIN_SCREEN_FORM_TURN_FLAG, 0xA2_6858);
    }
}

#[cfg(test)]
mod modal_stack_tests {
    use super::*;
    use std::collections::BTreeMap;
    struct Memory(BTreeMap<u32, u32>);
    impl crate::controls::Mem for Memory {
        fn u32(&self, address: u32) -> Option<u32> { self.0.get(&address).copied() }
        fn u8(&self, address: u32) -> Option<u8> { self.u32(address).map(|v| v as u8) }
    }
    #[test]
    fn modal_stack_is_bounded_ordered_and_rejects_torn_or_invalid_native_state() {
        let mut mem = Memory(BTreeMap::from([(0xCA_D388, 0), (0xCA_D384, 0)]));
        assert_eq!(modal_stack(&mem, Build::PcGames), Some([0; 4]));
        for depth in 1..=4 {
            mem.0.insert(0xCA_CD20 + (depth - 1) * 4, 0x1000 * depth);
            mem.0.insert(0xCA_D388, depth);
            mem.0.insert(0xCA_D384, 0x1000 * depth);
            let stack = modal_stack(&mem, Build::PcGames).unwrap();
            assert_eq!(stack[depth as usize - 1], 0x1000 * depth);
        }
        mem.0.insert(0xCA_D388, 5);
        assert_eq!(modal_stack(&mem, Build::PcGames), None);
        mem.0.insert(0xCA_D388, 4);
        mem.0.insert(0xCA_D384, 0x9999);
        assert_eq!(modal_stack(&mem, Build::PcGames), None);
        mem.0.insert(0xCA_D384, 0x4000);
        mem.0.insert(0xCA_CD20 + 4, 0x1000);
        assert_eq!(modal_stack(&mem, Build::PcGames), None);
        assert_eq!(modal_stack(&mem, Build::Steam), None);
    }
}
