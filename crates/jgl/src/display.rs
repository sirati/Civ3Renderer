//! The display mode: jgl's graphics slots that set it.
//!
//! The game picks its screen size at start (`KeepRes=1`: the screen's
//! size, at least 1024×768; or a fixed size from its `Video Mode`
//! setting) and asks jgl for that mode at 16 bits:
//!
//! - +0x58 `set_mode(width, height, bits)` and +0x5C
//!   `set_mode_refresh(width, height, bits, keep, refresh)` look the mode
//!   up in the display's list (`EnumDisplaySettingsA`, slot +0x68) and
//!   switch to it (`ChangeDisplaySettingsA`, slot +0x64). Without such a
//!   mode they answer 1, and the game quits: its start-up (exe 0x56C3A0
//!   and 0x56C3BA) returns 0, and the process exits with code 0 and no
//!   message. A display shorter than 768 lines has no mode the game takes.
//! - +0x60 `set_mode_keep(width, height, bits, keep)` with `keep` set
//!   takes the size without touching the display: it sets the screen
//!   fields (+0x12C, +0x130, +0x134) and tells the window's owner
//!   (+0x148, its method 0) the new mode. With `keep` clear it is +0x58.
//!
//! The game's window is not what is shown (the viewer draws), so the
//! display is never switched: every mode the game asks for becomes its
//! screen the way +0x60 with `keep` does it. The movie slots +0x6C (a
//! 640×480 switch) and +0x70 (back) and the list itself are left to jgl.

/// A display mode as the game asks for it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Mode {
    pub width: i32,
    pub height: i32,
    /// Bits per pixel (the game asks for 16).
    pub bits: i32,
}

/// Larger than any screen; what `GetSystemMetrics` lets the game take.
const MOST: i32 = 0x2000;

/// The screen the game gets for a mode it asks for: the mode itself, the
/// display left as it is. `None` for a request that is no mode (sizes
/// outside 1..=8192, a depth GDI has no surfaces for): then jgl answers
/// it as it always did.
pub fn screen_for(asked: Mode) -> Option<Mode> {
    let size = 1..=MOST;
    let ok = size.contains(&asked.width) && size.contains(&asked.height) && matches!(asked.bits, 8 | 15 | 16 | 24 | 32);
    ok.then_some(asked)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(width: i32, height: i32, bits: i32) -> Mode {
        Mode { width, height, bits }
    }

    #[test]
    fn every_mode_the_game_asks_for_is_its_screen() {
        // The least the game takes, on a display 729 lines high.
        assert_eq!(screen_for(mode(1280, 768, 16)), Some(mode(1280, 768, 16)));
        assert_eq!(screen_for(mode(1024, 768, 16)), Some(mode(1024, 768, 16)));
        // As large as the display, or a Video Mode setting's.
        assert_eq!(screen_for(mode(3274, 2048, 16)), Some(mode(3274, 2048, 16)));
        assert_eq!(screen_for(mode(1600, 1200, 16)), Some(mode(1600, 1200, 16)));
        assert_eq!(screen_for(mode(1920, 1080, 32)), Some(mode(1920, 1080, 32)));
    }

    #[test]
    fn what_is_no_mode_stays_jgls() {
        assert_eq!(screen_for(mode(0, 768, 16)), None);
        assert_eq!(screen_for(mode(1024, -1, 16)), None);
        assert_eq!(screen_for(mode(9000, 768, 16)), None);
        assert_eq!(screen_for(mode(1024, 768, 0)), None);
        assert_eq!(screen_for(mode(1024, 768, 12)), None);
    }
}
