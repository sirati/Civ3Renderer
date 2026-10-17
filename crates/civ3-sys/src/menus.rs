//! The game's title menus: what the Load Scenario screen reads when a
//! scenario is highlighted in its list.
//!
//! The screen (its form's code around 0x59F600, PCGames) builds the
//! highlighted file's path and calls `BIC_load_scenario_preview`
//! (0x59B1A0) on the global `BIC`, which calls `load_scenario` (0x59A870)
//! with the file: the game loads the whole scenario — rules, map, cities,
//! units — into its tables to show the players, description and map size.
//! C3X redirects `load_scenario` and tells this load from a real one by its
//! return address (`ADDR_LOAD_SCENARIO_PREVIEW_RETURN`, 0x59B1B9), so the
//! hook goes on the screen's call of the preview function, one level up.
//!
//! Only the PCGames build is decoded; the others have no sites (`None`).

use crate::Build;
use crate::modal::CallSite;

inproc::fastcall_hook! {
    /// `int BIC::load_scenario_preview(BIC *this, int edx, char *path,
    /// int *out)` (`ret 8`): 0 when the file loaded.
    pub mod scenario_preview(bic, edx; path, out) -> i32;
}

/// The Load Scenario screen's call of `BIC::load_scenario_preview` (its
/// only caller).
pub fn scenario_preview_site(build: Build) -> Option<CallSite> {
    match build {
        // call 0x59B1A0
        Build::PcGames => Some((0x59_F72A, [0xE8, 0x71, 0xBA, 0xFF, 0xFF])),
        Build::Gog | Build::Steam => None,
    }
}

/// The class (vtable) of the Load Scenario screen's form (the title menu's
/// Load Scenario, or Conquests; a 1024×768 dialog in the middle of the
/// screen).
pub fn load_scenario_form(build: Build) -> Option<u32> {
    match build {
        Build::PcGames => Some(0x66_EAB0),
        Build::Gog | Build::Steam => None,
    }
}
