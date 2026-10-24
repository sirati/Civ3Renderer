//! Where the game starts to quit, before it frees anything.
//!
//! `WinMain` (0x56BF10) runs `BIC::run` (0x4C9190): its setup, the main
//! loop `run_game` (0x4F6270), then the teardown of the program's objects;
//! `WinMain` then returns into the C runtime, whose `exit` runs the
//! `atexit` handlers and calls `ExitProcess` (0x64CA7A).
//!
//! The main loop shows the title menu (0x54F580: nonzero is its Quit),
//! then plays a game until the "leave" flag (0xCC377C) is set: by the
//! confirmed Quit (`REALLYQUIT`, 0x4DFF8F), the end of a game, a load or
//! "new game". Leaving, it ends the game (0x4F6857: `call 0x5B0200` and
//! what follows), and goes round again if the "restart" byte (0xA2839C)
//! says so (to the title menu, a new or loaded game); else it returns
//! (0x4F68AD: `call 0x535F80`, then the loop's own end).
//!
//! So the two call sites below see every quit before the game frees its
//! world (cities, units, players, the map): [`game_loop_left`] with the
//! restart byte 0 when a running game quits, [`main_loop_left`] when the
//! main loop ends, from the title menu too. Both callees take no
//! arguments (plain `call rel32`, nothing pushed, no `this`).
//!
//! What of the teardown is kept: the teardown calls (`BIC::run` after
//! `run_game`, `WinMain` after that, the C runtime's `atexit` handlers,
//! its stream flush) were read for file, registry and network writes, and
//! a quit's changes to the game folder and prefix compared with and
//! without them. The game persists two things only:
//! [`save_scroll_speed`] (0x4E2BA0, `BIC::run`'s teardown: the map's
//! scroll speed, changed by keys in play, written to `conquests.ini` only
//! here) and [`delete_scenario_temp`] (0x599F60, an `atexit` handler:
//! deletes `bic__in_.tmp` and `bic__out.tmp`). Everything else the
//! teardown does is freeing memory and handles.
//!
//! Only the PCGames build is decoded; the others have no sites (`None`).

use inproc::Ptr32;

use crate::Build;
use crate::modal::{CallSite, FunctionSite};

inproc::call_site_hook! {
    /// The main loop leaving a running game (0x4F6857, `call 0x5B0200`),
    /// before it ends the game: to quit, or to go round again.
    pub mod game_loop_left() -> ();
}

inproc::call_site_hook! {
    /// The main loop's last call before it returns (0x4F68AD,
    /// `call 0x535F80`): the program quits.
    pub mod main_loop_left() -> ();
}

inproc::thiscall_fn! {
    /// `Main_Screen_Form::save_scroll_speed(this)`: writes `ScrollSpeed`
    /// (this +0x4DB8) to `conquests.ini`.
    pub fn save_scroll_speed(main_screen;) -> ();
}

inproc::cdecl_fn! {
    /// Deletes the scenario loader's temporary files (`bic__in_.tmp`,
    /// `bic__out.tmp`, names at 0x72D9A0/0x72D9A4).
    pub fn delete_scenario_temp() -> ();
}

/// The call sites, the main loop's "restart" byte (1: it goes round
/// again instead of returning), and what the teardown persists.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sites {
    pub game_loop_left: CallSite,
    pub main_loop_left: CallSite,
    pub restart: Ptr32<u8>,
    pub save_scroll_speed: FunctionSite,
    pub delete_scenario_temp: FunctionSite,
}

pub fn sites(build: Build) -> Option<Sites> {
    match build {
        Build::PcGames => Some(Sites {
            game_loop_left: (0x4F_6857, [0xE8, 0xA4, 0x99, 0x0B, 0x00]),
            main_loop_left: (0x4F_68AD, [0xE8, 0xCE, 0xF6, 0x03, 0x00]),
            restart: Ptr32::new(0xA2_839C),
            // mov eax,[ecx+4DB8h]
            save_scroll_speed: (0x4E_2BA0, &[0x8B, 0x81, 0xB8, 0x4D, 0x00, 0x00]),
            // mov eax,[72D9A0h] / push esi
            delete_scenario_temp: (0x59_9F60, &[0xA1, 0xA0, 0xD9, 0x72, 0x00, 0x56]),
        }),
        Build::Gog | Build::Steam => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sites_call_what_the_disassembly_says() {
        let s = sites(Build::PcGames).unwrap();
        // `call rel32`: the target is the next instruction plus rel32.
        let target = |(at, bytes): CallSite| at.wrapping_add(5).wrapping_add(u32::from_le_bytes(bytes[1..].try_into().unwrap()));
        assert_eq!(target(s.game_loop_left), 0x5B_0200);
        assert_eq!(target(s.main_loop_left), 0x53_5F80);
        assert!(sites(Build::Gog).is_none() && sites(Build::Steam).is_none());
    }
}
