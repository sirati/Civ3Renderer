//! The city screen's lists, as the game's data gives their rows
//! (docs/smooth-lists.md, "Bridge design"): every entry of a list, not
//! only those the game shows, so that a renderer can show the list whole
//! and scroll it itself.
//!
//! Every list box draws an entry with 0x617470 (`this` = the list object
//! G, the argument the entry's index), which calls the list's callbacks
//! with the entry's data. [`entry_draw`] hooks it: the bridge marks what is
//! drawn inside as that list's rows.
//!
//! The improvements list's rows (`City_Form.Improvement_List`, filled by
//! 0x421D30: one entry per improvement the city has, its index the data)
//! are drawn by `draw_improv_icons_on_city_screen` (0x4212C0) and the text
//! callback 0x421210; [`ImprovementLists::read`] asks the game for each
//! row's numbers exactly as those functions do (the functions their call
//! sites call, C3X's replacements included), and `civ3_ui::list_rows`
//! lays them out.
//!
//! Only the PCGames build is decoded. Everything here runs on the game's
//! thread (it calls the game's functions).

use inproc::Ptr32;

use crate::controls::{self, ListBox, Mem};
use crate::{Build, c3x_layout as X, layout as L};

inproc::fastcall_hook! {
    /// The entry draw of every list box (0x617470, thiscall, `ret 4`):
    /// `this` the list object G, `index` the entry.
    pub mod entry_draw(this, edx; index) -> ();
}

/// What of a game `Sprite` the rows use (Civ3Conquests.h `struct
/// Sprite`); the same as `civ3_ui::list_rows::SpriteInfo`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct SpriteInfo {
    pub addr: u32,
    /// `Width3` (+0x10), `Width` (+0x18), `Height` (+0x1C), `Height2` (+0x24).
    pub width3: i32,
    pub width: i32,
    pub height: i32,
    pub height2: i32,
}

/// The improvement rows' icons (`civ3_ui::list_rows::ImprovementIcons`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ImprovementIcons {
    pub upkeep: SpriteInfo,
    pub tourism: SpriteInfo,
    pub culture: SpriteInfo,
    pub happy: SpriteInfo,
}

/// One improvement's row as the game's data gives it
/// (`civ3_ui::list_rows::ImprovementRow`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImprovementRow {
    pub improvement: i32,
    pub small: SpriteInfo,
    pub name: String,
    pub upkeep: i32,
    /// The sprite of each tourism icon, in order.
    pub tourism: Vec<u32>,
    pub culture: i32,
    pub happy: i32,
}

/// 0x617470 and its first instructions (`mov eax,[esp+4]; sub esp,0x28`).
pub const ENTRY_DRAW: u32 = 0x61_7470;
pub const ENTRY_DRAW_PROLOGUE: &[u8] = &[0x8B, 0x44, 0x24, 0x04, 0x83, 0xEC, 0x28];

/// `p_city_form`.
pub const CITY_FORM: u32 = 0x74_09C0;
/// `City_Form.Improvement_List`, `Order_List` (list objects G).
pub const IMPROVEMENT_LIST: u32 = CITY_FORM + 0x8A4C;
pub const ORDER_LIST: u32 = CITY_FORM + 0x7C7C;
/// `City_Form.CurrentCity`.
const CURRENT_CITY: u32 = CITY_FORM + 0x19EC;
/// `City_Form.Improvement_Images_Small` (`Sprite *`, one a improvement).
const IMPROVEMENT_IMAGES_SMALL: u32 = CITY_FORM + 0x6548;
/// `City_Form.City_Icons_Images.Icon_14_Gold`; `Icon_k` is `k − 14`
/// sprites (44 bytes) from it.
const ICON_14: u32 = CITY_FORM + 0x5EEC;
const SPRITE_SIZE: u32 = 0x2C;
const fn icon(k: u32) -> u32 {
    ICON_14 + k * SPRITE_SIZE - 14 * SPRITE_SIZE
}
/// The leaders (`Leader[32]`, 0x20E4 bytes each); a leader's culture
/// object (0x4F8DA0's `this`) at +0x181C; its government at +0xA0.
const LEADERS: u32 = 0xA5_2E58;
const LEADER_SIZE: u32 = 0x20E4;
const LEADER_CULTURE: u32 = 0x181C;
const LEADER_GOVERNMENT: u32 = 0xA0;
/// The default font (`Canvas::line_height`, 0x5FE760, when a canvas has
/// none): its `+0x10` is the height the text routines centre.
const DEFAULT_FONT: u32 = 0xCC_38B8;
/// The BIC's resource types (`Resource_Type *`, as `mill_icons` reads them).
const RESOURCES: u32 = 0x9C_7194;
/// The BIC object (`p_bic_data`).
const BIC: u32 = 0x9C_34C8;
/// BIC improvement fields (Civ3Conquests.h `struct Improvement`).
const IMPROVEMENT_NAME: u32 = 0x44;
const IMPROVEMENT_HAPPY_FACES: u32 = 0xB4;
const IMPROVEMENT_GOVERNMENT: u32 = 0xD4;
const IMPROVEMENT_OBSOLETE: u32 = 0xE0;
const IMPROVEMENT_CHARACTERISTICS: u32 = 0xF0;
/// BIC resource yields (`struct Resource_Type`).
const RESOURCE_FOOD: u32 = 80;
const RESOURCE_SHIELD: u32 = 84;
const RESOURCE_COMMERCE: u32 = 88;
/// C3X's `MF_YIELDS`.
const MILL_YIELDS: u16 = 4;
/// The C3X state's red food icon (`is->red_food_icon`, a `Sprite`), and
/// whether it was made (`red_food_icon_state == IS_OK`): not decoded, so a
/// negative food yield is drawn as C3X falls back to without it (nothing).
const RED_FOOD: Option<u32> = None;

/// The row's call sites whose targets C3X replaces (`City_get_improv_
/// maintenance_for_ui`) or may: their targets are what is called.
const UPKEEP_SITE: u32 = 0x42_1367;
const TOURISM: u32 = 0x4B_07A0;
const TOURISM_PROLOGUE: &[u8] = &[0x8B, 0x44, 0x24, 0x04, 0x8B, 0xD0];
const CULTURE: u32 = 0x4F_8DA0;
const CULTURE_PROLOGUE: &[u8] = &[0x51, 0x53, 0x55, 0x56, 0x8B, 0x74, 0x24, 0x14];
const DOUBLES_HAPPY: u32 = 0x55_A790;
const DOUBLES_HAPPY_PROLOGUE: &[u8] = &[0xA1, 0x40, 0x3D, 0x9C, 0x00];
const HAS_TECH: u32 = 0x56_13F0;
const HAS_TECH_PROLOGUE: &[u8] = &[0x8B, 0x44, 0x24, 0x04, 0x83, 0xF8, 0xFF];
/// The game's own upkeep function (the call site's target without C3X).
const UPKEEP: u32 = 0x4A_CE80;
const UPKEEP_PROLOGUE: &[u8] = &[0x51, 0x53, 0x8B, 0xD9, 0x55];
/// How C3X's functions (built with TCC) start: `push ebp; mov ebp,esp;
/// sub esp,imm32` (`81 EC`, the frame size after it).
const C3X_START: &[u8] = &[0x55, 0x89, 0xE5, 0x81, 0xEC];
/// C3X R28's mill helpers (`gather_mill_yields`), at the addresses and
/// with the starts `mill_icons::Producer::find` audits.
const CAN_GENERATE: u32 = 0xD9_FA83;
const CAN_GENERATE_PROLOGUE: &[u8] = &[0x55, 0x89, 0xE5, 0x81, 0xEC, 4, 0, 0, 0];
const ACTIVE_BUILDING: u32 = 0xDA_7B13;
const ACTIVE_BUILDING_PROLOGUE: &[u8] = &[0x55, 0x89, 0xE5, 0x81, 0xEC, 8, 0, 0, 0];
const HAS_INPUTS: u32 = 0xDA_9632;
const HAS_INPUTS_PROLOGUE: &[u8] = &[0x55, 0x89, 0xE5, 0x81, 0xEC, 0, 0, 0, 0];

inproc::thiscall_fn! {
    /// `int City::get_improvement_maintenance(this, improvement)` as the
    /// row's call site 0x421367 calls it.
    pub fn upkeep(city; improvement) -> i32;
}
inproc::thiscall_fn! {
    /// `int City::get_tourism_amount(this, improvement)` (0x4B07A0).
    pub fn tourism(city; improvement) -> i32;
}
inproc::thiscall_fn! {
    /// The improvement's culture icons for the city's civ (0x4F8DA0,
    /// `this` the leader's culture object).
    pub fn culture(culture; improvement, city) -> i32;
}
inproc::thiscall_fn! {
    /// Whether the leader doubles the improvement's happy faces (0x55A790;
    /// only the low byte is set).
    pub fn doubles_happy(leader; improvement) -> u32;
}
inproc::thiscall_fn! {
    /// `bool Leader::has_tech(this, tech)` (0x5613F0; the low byte).
    pub fn has_tech(leader; tech) -> u32;
}
inproc::cdecl_fn! { pub fn can_generate(civ, mill) -> u32; }
inproc::cdecl_fn! { pub fn active_building(city, improvement) -> u32; }
inproc::cdecl_fn! { pub fn has_inputs(city, improvement) -> u32; }

/// C3X's mills, where its state is known (`mill_icons::Producer`).
#[derive(Copy, Clone, Debug)]
struct Mills {
    state: u32,
    can_generate: can_generate::Function,
    active_building: active_building::Function,
    has_inputs: has_inputs::Function,
}

/// The game's functions the rows need, checked once.
#[derive(Copy, Clone, Debug)]
pub struct ImprovementLists {
    upkeep: upkeep::Function,
    tourism: tourism::Function,
    culture: culture::Function,
    doubles_happy: doubles_happy::Function,
    has_tech: has_tech::Function,
    mills: Option<Mills>,
}

/// The function a `call rel32` at `site` calls.
fn call_target(site: u32) -> Option<u32> {
    let bytes = Ptr32::<u8>::new(site).read_array(5).ok()?;
    let rel = u32::from_le_bytes(bytes.get(1..5)?.try_into().ok()?);
    (bytes[0] == 0xE8).then(|| site.wrapping_add(5).wrapping_add(rel))
}

/// The improvements list of the open city screen, its rows.
#[derive(Clone, Debug)]
pub struct ImprovementList {
    pub list: ListBox,
    /// The list's canvas (`PCX_Image*`, the form's +0x274): the rows' are
    /// in its pixels.
    pub canvas: u32,
    /// Its font's `+0x10` ([`DEFAULT_FONT`] if it has none).
    pub text_cell: i32,
    pub icons: ImprovementIcons,
    /// One per entry, in the list's order (none where an entry's data is
    /// not an improvement).
    pub rows: Vec<Option<ImprovementRow>>,
}

fn sprite(mem: &dyn Mem, addr: u32) -> Option<SpriteInfo> {
    Some(SpriteInfo { addr, width3: mem.i32(addr + 0x10)?, width: mem.i32(addr + 0x18)?, height: mem.i32(addr + 0x1C)?, height2: mem.i32(addr + 0x24)? })
}

fn c_string(mem: &dyn Mem, at: u32, max: usize) -> String {
    let bytes: Vec<u8> = (0..max as u32).map_while(|i| mem.u8(at + i)).take_while(|&b| b != 0).collect();
    bytes.iter().map(|&b| b as char).collect()
}

impl ImprovementLists {
    /// The functions, if this build's are the ones decoded.
    pub fn find(build: Build) -> Result<Option<ImprovementLists>, inproc::hook::HookError> {
        if build != Build::PcGames {
            return Ok(None);
        }
        let upkeep_at = call_target(UPKEEP_SITE).ok_or(inproc::hook::HookError::Unsupported)?;
        let upkeep = if upkeep_at == UPKEEP { upkeep::Function::at(UPKEEP, UPKEEP_PROLOGUE)? } else { upkeep::Function::at(upkeep_at, C3X_START)? };
        let mills = match crate::mill_icons::Producer::find(build) {
            Ok(Some(_)) => {
                let state = Ptr32::<u32>::new(0xE2_74E0).read()?;
                Some(Mills {
                    state,
                    can_generate: can_generate::Function::at(CAN_GENERATE, CAN_GENERATE_PROLOGUE)?,
                    active_building: active_building::Function::at(ACTIVE_BUILDING, ACTIVE_BUILDING_PROLOGUE)?,
                    has_inputs: has_inputs::Function::at(HAS_INPUTS, HAS_INPUTS_PROLOGUE)?,
                })
            }
            _ => None,
        };
        Ok(Some(ImprovementLists {
            upkeep,
            tourism: tourism::Function::at(TOURISM, TOURISM_PROLOGUE)?,
            culture: culture::Function::at(CULTURE, CULTURE_PROLOGUE)?,
            doubles_happy: doubles_happy::Function::at(DOUBLES_HAPPY, DOUBLES_HAPPY_PROLOGUE)?,
            has_tech: has_tech::Function::at(HAS_TECH, HAS_TECH_PROLOGUE)?,
            mills,
        }))
    }

    /// C3X's `gather_mill_yields(city, improvement)`: the food, shields and
    /// commerce the improvement's mills with yields give the city.
    fn mill_yields(&self, mem: &dyn Mem, city: u32, civ: i32, improvement: i32) -> (i32, i32, i32) {
        let Some(m) = self.mills else { return (0, 0, 0) };
        let count = mem.i32(m.state + X::STATE_CURRENT_CONFIG_COUNT_MILLS).unwrap_or(0).clamp(0, 32768);
        let Some(mills) = mem.u32(m.state + X::STATE_CURRENT_CONFIG_MILLS).filter(|&p| p != 0 && count > 0) else { return (0, 0, 0) };
        let Some(resources) = mem.u32(RESOURCES).filter(|&p| p != 0) else { return (0, 0, 0) };
        let (mut food, mut shields, mut commerce) = (0, 0, 0);
        for i in 0..count as u32 {
            let mill = mills + i * X::MILL_SIZE;
            let short = |at: u32| Some(i16::from_le_bytes([mem.u8(mill + at)?, mem.u8(mill + at + 1)?]) as i32);
            let (Some(id), Some(resource), Some(flags)) = (short(X::MILL_IMPROV_ID), short(X::MILL_RESOURCE_ID), short(X::MILL_FLAGS)) else { continue };
            if flags as u16 & MILL_YIELDS == 0 || id != improvement || resource < 0 {
                continue;
            }
            if m.can_generate.call(civ as u32, mill) & 0xFF == 0
                || m.active_building.call(city, improvement as u32) & 0xFF == 0
                || m.has_inputs.call(city, improvement as u32) & 0xFF == 0
            {
                continue;
            }
            let r = resources + resource as u32 * L::RESOURCE_TYPE_SIZE;
            food += mem.i32(r + RESOURCE_FOOD).unwrap_or(0);
            shields += mem.i32(r + RESOURCE_SHIELD).unwrap_or(0);
            commerce += mem.i32(r + RESOURCE_COMMERCE).unwrap_or(0);
        }
        (food, shields, commerce)
    }

    /// The open city screen's improvements list and its rows, if it shows.
    pub fn read(&self) -> Option<ImprovementList> {
        let mem: &dyn Mem = &controls::Live;
        let city = mem.u32(CURRENT_CITY).filter(|&c| c != 0)?;
        let list = controls::read_list_box(mem, IMPROVEMENT_LIST).filter(|l| l.visible)?;
        let canvas = list.form + 0x274;
        let font = mem.u32(canvas + 0x5C).filter(|&f| f != 0).or_else(|| mem.u32(DEFAULT_FONT).filter(|&f| f != 0))?;
        let text_cell = mem.i32(font + 0x10)?;
        let icons = ImprovementIcons { upkeep: sprite(mem, icon(17))?, tourism: sprite(mem, icon(14))?, culture: sprite(mem, icon(18))?, happy: sprite(mem, icon(19))? };
        let civ = i32::from(mem.u8(city + 0x1C + L::CITY_BODY_CIVID)? as i8);
        let leader = LEADERS + (civ.clamp(0, 31) as u32) * LEADER_SIZE;
        let images = mem.u32(IMPROVEMENT_IMAGES_SMALL).filter(|&p| p != 0)?;
        let improvements = mem.u32(BIC + L::BIC_IMPROVEMENTS).filter(|&p| p != 0)?;
        let count = mem.i32(BIC + L::BIC_IMPROVEMENTSCOUNT)?;
        let rows = list
            .data
            .iter()
            .map(|&id| {
                if !(0..count).contains(&id) {
                    return None;
                }
                let record = improvements + id as u32 * L::IMPROVEMENT_SIZE;
                let field = |at: u32| mem.i32(record + at);
                let (food, shields, commerce) = self.mill_yields(mem, city, civ, id);
                let base = self.tourism.call(city, id as u32);
                // C3X's `patch_City_get_tourism_amount_to_draw` and
                // `patch_Sprite_draw_tourism_gold`: |food| food icons,
                // |shields| shields, then |commerce + tourism| gold (as a
                // loss when negative).
                let combined = commerce + base;
                let mut tourism = Vec::new();
                for _ in 0..food.abs() {
                    match (food >= 0, RED_FOOD) {
                        (true, _) => tourism.push(icon(15)),
                        (false, Some(red)) => tourism.push(red),
                        (false, None) => {}
                    }
                }
                tourism.extend(std::iter::repeat_n(if shields >= 0 { icon(13) } else { icon(5) }, shields.unsigned_abs() as usize));
                tourism.extend(std::iter::repeat_n(if combined < 0 { icon(17) } else { icon(14) }, combined.unsigned_abs() as usize));
                // Happy faces: a temple-like building (characteristics & 4)
                // only under its government and before it is obsolete.
                let shows_happy = field(IMPROVEMENT_CHARACTERISTICS)? & 4 == 0 || {
                    let government = field(IMPROVEMENT_GOVERNMENT)?;
                    let obsolete = field(IMPROVEMENT_OBSOLETE)?;
                    (government == -1 || government == mem.i32(leader + LEADER_GOVERNMENT)?)
                        && (obsolete == -1 || self.has_tech.call(leader, obsolete as u32) & 0xFF == 0)
                };
                let happy = if shows_happy {
                    let faces = field(IMPROVEMENT_HAPPY_FACES)?;
                    if self.doubles_happy.call(leader, id as u32) & 0xFF != 0 { faces * 2 } else { faces }
                } else {
                    0
                };
                Some(ImprovementRow {
                    improvement: id,
                    small: sprite(mem, images + id as u32 * SPRITE_SIZE)?,
                    name: c_string(mem, record + IMPROVEMENT_NAME, 32),
                    upkeep: self.upkeep.call(city, id as u32),
                    tourism,
                    culture: self.culture.call(leader + LEADER_CULTURE, id as u32, city),
                    happy,
                })
            })
            .collect();
        Some(ImprovementList { list, canvas, text_cell, icons, rows })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_checked_start_is_long_enough_to_check() {
        // `InlinePatch::check` refuses a start of fewer than 5 bytes: a
        // shorter one failed every install, and the game with it.
        for p in [ENTRY_DRAW_PROLOGUE, TOURISM_PROLOGUE, CULTURE_PROLOGUE, DOUBLES_HAPPY_PROLOGUE, HAS_TECH_PROLOGUE, UPKEEP_PROLOGUE, C3X_START, CAN_GENERATE_PROLOGUE, ACTIVE_BUILDING_PROLOGUE, HAS_INPUTS_PROLOGUE] {
            assert!(p.len() >= 5, "{p:x?}");
        }
    }

    #[test]
    fn the_icons_are_the_city_forms() {
        // Icon_14_Gold at 0x7468AC, Icon_17 0x746930, Icon_18 0x74695C,
        // Icon_19 0x746988 (the addresses draw_improv_icons_on_city_screen
        // reads).
        assert_eq!([icon(14), icon(17), icon(18), icon(19)], [0x7468AC, 0x746930, 0x74695C, 0x746988]);
        assert_eq!(IMPROVEMENT_IMAGES_SMALL, 0x746F08);
        assert_eq!(CURRENT_CITY, 0x7423AC);
        // The leaders' culture objects (0xA54674 + civ · 0x20E4).
        assert_eq!(LEADERS + LEADER_CULTURE, 0xA54674);
    }
}
