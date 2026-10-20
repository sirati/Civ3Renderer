//! Native city worker eligibility. Only call on the game thread.
use crate::{Build, modal::FunctionSite};

inproc::thiscall_fn! {
    /// C3X `City_controls_tile(this, neighbor_index, consider_enemy_units)`.
    /// Native PCGames 0x4BB560, ret 8. C3X redirects this entry to include
    /// work-ring limits, district and distribution-hub exclusions.
    pub fn controls_tile(city; index, consider_enemy_units) -> u32;
}

inproc::fastcall_hook! {
    /// `void City_Form::draw_yields_on_worked_tiles(this)`.
    pub mod draw_yields(form, edx;) -> ();
}

pub fn sites(build: Build) -> Option<(FunctionSite, FunctionSite)> {
    match build {
        Build::PcGames => Some(((0x4B_B560, &[0x83, 0xEC, 0x10, 0x8B, 0x44, 0x24, 0x14]),
            (0x42_0CC0, &[0x83, 0xEC, 0x24, 0x53, 0x55, 0x56, 0x57]))),
        Build::Gog | Build::Steam => None,
    }
}
