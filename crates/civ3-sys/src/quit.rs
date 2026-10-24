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
//! Only the PCGames build is decoded; the others have no sites (`None`).

use inproc::Ptr32;

use crate::Build;
use crate::modal::CallSite;

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

/// The call sites, and the main loop's "restart" byte (1: it goes round
/// again instead of returning).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sites {
    pub game_loop_left: CallSite,
    pub main_loop_left: CallSite,
    pub restart: Ptr32<u8>,
}

pub fn sites(build: Build) -> Option<Sites> {
    match build {
        Build::PcGames => Some(Sites {
            game_loop_left: (0x4F_6857, [0xE8, 0xA4, 0x99, 0x0B, 0x00]),
            main_loop_left: (0x4F_68AD, [0xE8, 0xCE, 0xF6, 0x03, 0x00]),
            restart: Ptr32::new(0xA2_839C),
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
