//! The engine's Clean Map mode and its thirteen checkbox categories.
//!
//! The checkbox bits are not the renderer's layer bits. In the PCGames
//! build, the dialog at 0x4EC660 translates them to the flag word at
//! 0x9C783C and saves that word at 0xA52C04. The command at 0x4EBA30
//! copies the saved word to the active word, then toggles bit 0 of
//! `p_debug_mode_bits`. A selected preference therefore hides nothing
//! until Clean Map mode itself is enabled.

use inproc::Ptr32;

use crate::{Build, Civ3, Civ3Error};

/// Engine layer masks in the order of the Clean Map Preferences dialog:
/// rivers/flood plains, mountains, hills, forests/jungles, terrain
/// buildings, roads/railroads, irrigation, natural resources, yield
/// icons, pollution, territory boundaries, cities, and units.
pub const ENGINE_MASKS: [u32; 13] = [
    0x000E, 0x0010, 0x4000, 0x0020, 0x0080, 0x0400, 0x0800,
    0x1000, 0x0040, 0x8000, 0x0200, 0x2000, 0x1_0000,
];

/// The game's independently stored selection and enable switch.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub enabled: bool,
    /// One bit per checkbox, in [`ENGINE_MASKS`] order.
    pub selected: u32,
}

impl State {
    /// Hidden categories for a view that follows the game's Clean Map mode.
    pub fn hidden(self) -> u32 {
        if self.enabled { self.selected } else { 0 }
    }
}

/// The flag word lives immediately after the BIC's associated map globals.
/// PCGames' address is decoded directly; the other builds use their BIC's
/// address from C3X's global table and the same relative data layout.
pub const fn flags_address(build: Build) -> u32 {
    build.addresses().bic_data + 0x4374
}

/// Saved `Default_Game_Settings.Clean_Map`, next to the other preferences.
/// The live flag word can differ while the preferences dialog is open.
pub const fn saved_flags_address(build: Build) -> u32 {
    build.addresses().preferences + 0x5CC
}

/// Convert the game's layer mask to the dialog's consecutive checkbox bits.
pub fn categories(engine_flags: u32) -> u32 {
    ENGINE_MASKS.iter().enumerate().fold(0, |out, (bit, &mask)| {
        out | (u32::from(engine_flags & mask != 0) << bit)
    })
}

impl Civ3 {
    /// Selected Clean Map categories and the game's separate enable switch.
    /// Reads only the game's globals and never changes a preference.
    pub fn clean_map_state(&self) -> Result<State, Civ3Error> {
        let enabled = Ptr32::<u32>::new(self.addrs.debug_mode_bits).read()? & 1 != 0;
        let flags = Ptr32::<u32>::new(flags_address(self.build)).read()?;
        Ok(State { enabled, selected: categories(flags) })
    }

    /// Effective hidden categories for views following the game's mode.
    pub fn clean_map(&self) -> Result<u32, Civ3Error> {
        Ok(self.clean_map_state()?.hidden())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_dialog_checkbox_maps_to_its_own_category() {
        for (bit, mask) in ENGINE_MASKS.into_iter().enumerate() {
            assert_eq!(categories(mask), 1 << bit, "checkbox {bit}");
            for engine_bit in 0..32 {
                if mask & (1 << engine_bit) != 0 {
                    assert_eq!(categories(1 << engine_bit), 1 << bit);
                }
            }
        }
        assert_eq!(categories(u32::MAX), 0x1FFF);
        assert_eq!(categories(0), 0);
        assert_eq!(categories(0xFFFF_0001 & !ENGINE_MASKS[12]), 0,
            "unrelated layer bits never become Clean Map preferences");
    }

    #[test]
    fn checked_preferences_hide_nothing_until_mode_is_enabled() {
        let state = State { enabled: false, selected: (1 << 5) | (1 << 12) };
        assert_eq!(state.hidden(), 0);
        assert_eq!(State { enabled: true, ..state }.hidden(), (1 << 5) | (1 << 12));
    }

    #[test]
    fn globals_follow_the_supported_build_data_tables() {
        for (build, active, saved) in [
            (Build::PcGames, 0x9C_783C, 0xA5_2C04),
            (Build::Gog, 0x9C_787C, 0xA5_2C44),
            (Build::Steam, 0x9E_A07C, 0xA7_543C),
        ] {
            assert_eq!(flags_address(build), active);
            assert_eq!(saved_flags_address(build), saved);
        }
    }
}
