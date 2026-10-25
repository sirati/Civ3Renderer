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
//! screen the way +0x60 with `keep` does it.
//!
//! jgl calls `ChangeDisplaySettingsA` in four places, each through its one
//! import: +0x64 (a mode of its list, saving the display's mode at +0x04
//! first), +0x6C (a movie's 640×480 switch, saving the mode at +0x98),
//! +0x70 (back from the movie) and +0x08, its shutdown, which restores the
//! mode saved at +0x04. With +0x58..+0x60 taken over, +0x64 never runs and
//! +0x04 stays zero, so the shutdown asks for no mode at all and Wine
//! answers `DISP_CHANGE_BADMODE` (-2): the "Changing (null) display
//! settings returned -2" in the Wine log of a game that is ending. The
//! import is answered too ([`answer_change`]): the display is left as it
//! is and every caller is told it changed.

/// A display mode as the game asks for it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Mode {
    pub width: i32,
    pub height: i32,
    /// Bits per pixel (the game asks for 16).
    pub bits: i32,
}

/// One 20-byte entry of graphics +68's public mode list. The binary stores
/// width twice; callers of +50 compare width, height, depth and frequency.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ModeInfo {
    pub width: u32,
    pub height: u32,
    pub width_again: u32,
    pub bits: u32,
    pub frequency: u32,
}

/// The mode-list contents and allocation growth, independent of Win32. The
/// OS adapter owns its stable ABI header and supplies EnumDisplaySettingsA.
#[derive(Default)]
pub struct Modes(Vec<ModeInfo>);

impl Modes {
    /// Re-enumerate every time, starting at mode zero. Keep every entry in
    /// the OS order, including duplicates, just as graphics +68 does.
    pub fn enumerate(mut next: impl FnMut(u32) -> Option<ModeInfo>) -> Vec<ModeInfo> {
        (0..).map_while(&mut next).collect()
    }

    /// The original list grows by one and retains its capacity on refresh.
    pub fn replace(&mut self, modes: impl IntoIterator<Item = ModeInfo>) {
        self.0.clear();
        for mode in modes {
            if self.0.len() == self.0.capacity() { self.0.reserve_exact(1); }
            self.0.push(mode);
        }
    }

    pub fn entries(&self) -> &[ModeInfo] { &self.0 }
    pub fn capacity(&self) -> usize { self.0.capacity() }

    /// +64's checked index and original bad-argument status.
    pub fn indexed(&self, index: u32) -> Result<ModeInfo, u32> {
        self.0.get(index as usize).copied().ok_or(3)
    }

    /// +50's strict signed-frequency maximum. Equal frequencies retain the
    /// first index. No match is passed to virtual +64 as -1 by the original.
    pub fn fastest(&self, width: u32, height: u32, bits: u32) -> Option<usize> {
        fastest(&self.0, width, height, bits)
    }
}

/// +50 also accepts a list returned by an overridden virtual +68.
pub fn fastest(modes: &[ModeInfo], width: u32, height: u32, bits: u32) -> Option<usize> {
    let mut best = None;
    let mut frequency = -1;
    for (index, mode) in modes.iter().enumerate() {
        if mode.width == width && mode.height == height && mode.bits == bits && (mode.frequency as i32) > frequency {
            best = Some(index);
            frequency = mode.frequency as i32;
        }
    }
    best
}

/// Raw +58's scan: keep the first strict maximum, but stop at a later
/// higher frequency above 75 once any match has been selected. This is
/// intentionally sensitive to OS order, including a first match above 75.
pub fn default_frequency(modes: &[ModeInfo], width: u32, height: u32, bits: u32) -> Option<usize> {
    let mut best = None;
    let mut frequency = -1;
    for (index, mode) in modes.iter().enumerate() {
        if mode.width == width && mode.height == height && mode.bits == bits && (mode.frequency as i32) > frequency {
            if mode.frequency as i32 > 75 && best.is_some() { break; }
            best = Some(index);
            frequency = mode.frequency as i32;
        }
    }
    best
}

/// Raw +5C with keep clear: first exact refresh match, otherwise the first
/// strict signed maximum, without +58's early stop above 75.
pub fn preferred_frequency(modes: &[ModeInfo], width: u32, height: u32, bits: u32, wanted: u32) -> Option<usize> {
    modes.iter().position(|m| m.width == width && m.height == height && m.bits == bits && m.frequency == wanted)
        .or_else(|| fastest(modes, width, height, bits))
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

/// `ChangeDisplaySettingsA`'s answer for a change made (or nothing to do).
pub const DISP_CHANGE_SUCCESSFUL: i32 = 0;

/// `DEVMODEA.dmFields` bits for the fields jgl sets.
pub const DM_BITSPERPEL: u32 = 0x0004_0000;
pub const DM_PELSWIDTH: u32 = 0x0008_0000;
pub const DM_PELSHEIGHT: u32 = 0x0010_0000;

/// A display change jgl asks for: the `DEVMODEA` it passes (`None`: a null
/// pointer, the registry's mode) and the flags.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub mode: Option<ChangeMode>,
    pub flags: u32,
}

/// The fields of a `DEVMODEA` that say the mode.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ChangeMode {
    /// `dmFields`: which of the others are set.
    pub fields: u32,
    pub width: u32,
    pub height: u32,
    pub bits: u32,
}

/// What a display change is, in the words of the log.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    /// A null mode: back to the registry's.
    Registry,
    /// A mode with nothing set: jgl's shutdown restoring a mode it never
    /// saved (+0x04 is filled by +0x64 only).
    Unsaved,
    /// +0x6C: a movie's 640×480.
    Movie,
    /// Any other mode (+0x64, +0x70, or the shutdown's saved mode).
    Mode(ChangeMode),
}

impl ChangeKind {
    pub fn of(change: &Change) -> ChangeKind {
        match change.mode {
            None => ChangeKind::Registry,
            Some(m) if m.fields & (DM_PELSWIDTH | DM_PELSHEIGHT | DM_BITSPERPEL) == 0 || m.width == 0 || m.height == 0 => ChangeKind::Unsaved,
            Some(m) if (m.width, m.height) == (640, 480) => ChangeKind::Movie,
            Some(m) => ChangeKind::Mode(m),
        }
    }
}

/// jgl's `ChangeDisplaySettingsA`: the display is never changed (the viewer
/// shows the game, not the game's window), and every change is answered as
/// made, so no caller sees a failure. What it was, for the log.
pub fn answer_change(change: &Change) -> (i32, ChangeKind) {
    (DISP_CHANGE_SUCCESSFUL, ChangeKind::of(change))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_list_keeps_os_order_capacity_and_first_fastest_tie() {
        let mut queried = Vec::new();
        let modes = Modes::enumerate(|index| {
            queried.push(index);
            [60, 75, 75].get(index as usize).map(|&frequency| ModeInfo {
                width: 1024, height: 768, width_again: 1024, bits: 16, frequency,
            })
        });
        assert_eq!(queried, [0, 1, 2, 3]);
        let mut list = Modes::default();
        list.replace(modes.clone());
        assert_eq!(list.entries(), modes);
        assert_eq!(list.capacity(), 3);
        assert_eq!(list.fastest(1024, 768, 16), Some(1));
        assert_eq!(list.fastest(1024, 768, 32), None);
        assert_eq!(list.indexed(0), Ok(modes[0]));
        assert_eq!(list.indexed(3), Err(3));
        let with_frequency = |frequencies: &[u32]| frequencies.iter().map(|&frequency| ModeInfo { frequency, ..modes[0] }).collect::<Vec<_>>();
        assert_eq!(default_frequency(&with_frequency(&[60, 85, 75]), 1024, 768, 16), Some(0));
        assert_eq!(default_frequency(&with_frequency(&[85, 60, 75]), 1024, 768, 16), Some(0));
        assert_eq!(preferred_frequency(&modes, 1024, 768, 16, 60), Some(0));
        assert_eq!(preferred_frequency(&modes, 1024, 768, 16, 85), Some(1));
        list.replace([]);
        assert!(list.entries().is_empty());
        assert_eq!(list.capacity(), 3);
    }

    fn change(fields: u32, width: u32, height: u32, bits: u32, flags: u32) -> Change {
        Change { mode: Some(ChangeMode { fields, width, height, bits }), flags }
    }

    #[test]
    fn a_movies_mode_switch_and_its_way_back_succeed_without_a_switch() {
        // +0x6C: the display's mode with 640×480 (dmFields 0x1C0000), CDS_FULLSCREEN.
        let movie = change(DM_BITSPERPEL | DM_PELSWIDTH | DM_PELSHEIGHT, 640, 480, 32, 4);
        assert_eq!(answer_change(&movie), (DISP_CHANGE_SUCCESSFUL, ChangeKind::Movie));
        // +0x70: the mode saved before it.
        let back = change(0x005C_0000, 3274, 2048, 32, 4);
        assert_eq!(answer_change(&back), (DISP_CHANGE_SUCCESSFUL, ChangeKind::Mode(back.mode.unwrap())));
    }

    #[test]
    fn the_shutdowns_restore_of_a_mode_never_saved_succeeds() {
        // +0x08 with +0x04 all zero: what Wine answered -2 on TS Neo.
        assert_eq!(answer_change(&change(0, 0, 0, 0, 0)), (DISP_CHANGE_SUCCESSFUL, ChangeKind::Unsaved));
        assert_eq!(answer_change(&Change { mode: None, flags: 0 }), (DISP_CHANGE_SUCCESSFUL, ChangeKind::Registry));
    }

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
