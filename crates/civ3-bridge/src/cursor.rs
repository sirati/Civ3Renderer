//! The pointer the game shows, for the viewer (`scene::Cursor`): its cursor
//! sprites by their sets in the render description (`Viewer@cursors/<k>`,
//! cell `k` of `Art\cursor.pcx`), the system's resize pointers as such.
//! The system's busy pointers (`IDC_WAIT`, `IDC_APPSTARTING`) stay the
//! semantic busy states: for those the viewer shows the wait cursor's
//! hourglass, turning (`scene::loading_cursor`), as its own pointer. Cell
//! 12 is the "NEW UNIT" arrow, not a busy icon. The plain
//! arrow, which the game leaves to the system, is shown with the game's
//! own arrow art (cell 0) unless `CIV3R_SYSTEM_ARROW=1`.
//!
//! The cells of `Art\cursor.pcx` (PCGames 1.22 with C3X R21/R28, which
//! adds no cursor of its own), all shown as the viewer's pointer:
//! 0 the arrow; 1 and 18 blank (go-to, stack go-to: the marker on the map
//! shows the target); 2 road to, 3 railroad to; 4 air drop (parachute);
//! 5 "not here" ([`NOT_HERE`]); 6 bombard, 7 air bombard, 8 airlift,
//! 9 recon (the viewer's capture pointer too), 10 rebase, 11 precision
//! strike; 12 "NEW UNIT"; 13 the Civilopedia book ([`CIVILOPEDIA`]);
//! 14 build colony; 15 to 17 the automatic bombard, air bombard and
//! precision strike; 19 stack rebase; 20 and 21 the map's modes 0x14 and
//! 0x15 (0x4E9E62, 0x4E9EB5); 22 empty, never used. Cells 1 to 12 and 14
//! to 21 are the map form's, by `Unit_Mode_Actions` (0x4DC540, 0x4D9520,
//! ...); 13 is the only one the screens set.

use civ3_sys::cursor::GameCursor;
use scene::{Cursor, SystemCursor};

/// The description's set of cursor sprite `k`.
pub fn set_name(k: u32) -> String {
    format!("Viewer@cursors/{k}")
}

/// The scene's cursor for what the game shows (`None`: not known) and
/// the system pointer it set last (`IDC_*`, `None`: none seen); `set` finds
/// a description set's raw id.
pub fn scene_cursor(game: Option<GameCursor>, standard: Option<u16>, set: impl Fn(&str) -> Option<u32>, system_arrow: bool) -> Cursor {
    let arrow = || if system_arrow { Cursor::System(SystemCursor::Arrow) } else { set(&set_name(0)).map_or(Cursor::Default, Cursor::Sprite) };
    match game {
        Some(GameCursor::Sprite(k)) => set(&set_name(k)).map_or_else(arrow, Cursor::Sprite),
        // (A sprite the description does not have: the arrow rather than
        // nothing.)
        Some(GameCursor::OtherSprite) => arrow(),
        Some(GameCursor::System) | None => match standard {
            Some(32514) => Cursor::System(SystemCursor::Wait),
            Some(32650) => Cursor::System(SystemCursor::Progress),
            Some(32515) => Cursor::System(SystemCursor::Crosshair),
            Some(32513) => Cursor::System(SystemCursor::Text),
            Some(32649) => Cursor::System(SystemCursor::Hand),
            Some(32648) => Cursor::System(SystemCursor::NotAllowed),
            Some(32646) => Cursor::System(SystemCursor::Move),
            Some(32645) => Cursor::System(SystemCursor::ResizeNs),
            Some(32644) => Cursor::System(SystemCursor::ResizeEw),
            Some(32642) => Cursor::System(SystemCursor::ResizeNwse),
            Some(32643) => Cursor::System(SystemCursor::ResizeNesw),
            _ => arrow(),
        },
    }
}

/// The Civilopedia book (cell 13: the arrow's tip over a book marked
/// "C", hot spot (1, 1)): over what a click opens the Civilopedia on, when
/// the preference `P_SHOW_PEDIA_BOOK_CURSOR` (0x80000 of 0xA52638) is on.
/// Each screen's mouse move sets it on its form with 0x604350 (applied at
/// once by 0x606A70) and the arrow (0x6043A0 `IDC_ARROW`) when the pointer
/// leaves: the wonders of F7 (0x5D14F0: six inclusive RECTs at +0x19B8,
/// one with a wonder, +0x1F34), the city screen's buildings, units and
/// production (0x41E340, 0x4151A0), the domestic (0x51FF10), foreign
/// (0x532770), military (0x56FB30) and cultural (0x4FBAC0) advisors, the
/// espionage screen (0x52B3C0), diplomacy (0x50C230), the trade advisor
/// (0x5AD690, its parts 0x5AD830..0x5AE890), the spaceship (F10, 0x5A45C0)
/// and the wonder-completion splash (0x5D0140). C3X changes none of these.
pub const CIVILOPEDIA: u32 = 13;

/// The "not here" pointer (cell 5 of `Art\cursor.pcx`, a red circle with a
/// slash).
pub const NOT_HERE: u32 = 5;

/// The pointer while a unit command waits for its tile (`mode`, a
/// `Unit_Mode_Actions` value) and the viewer's pointer is on the world
/// (`on_world`): the game's own rule (0x4DC540), from whether the tile
/// under that pointer takes the command (`target_ok`, `field_2E1AC`, which
/// the game's hover sets for the tile handed to it, 0x4DC360). Cell
/// `mode` where it does, [`NOT_HERE`] where it does not (out of range, no
/// city or airfield to rebase to, nothing to bombard, ...). The go-to modes
/// take every tile (the game never refuses one) and keep `game`.
///
/// The game switches the cell itself only from its animator's tick
/// (0x4EF1AC) and after commands, not from the hover the viewer's moves
/// call (0x4EB7D0 skips 0x4DC540 for a tile it converts), and applies it
/// through the form under its own last client pointer (0x606A70), which
/// rests on the interface while the viewer's pointer is on the world; so
/// its cursor id is not relied on here.
pub fn targeting(game: Option<GameCursor>, mode: i32, target_ok: bool, on_world: bool) -> Option<GameCursor> {
    if !on_world || !crate::targeting::active(mode) || crate::targeting::go_to(mode) {
        return game;
    }
    Some(GameCursor::Sprite(if target_ok { mode as u32 } else { NOT_HERE }))
}

/// Whether the plain arrow stays the system's (`CIV3R_SYSTEM_ARROW=1`).
pub fn system_arrow() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("CIV3R_SYSTEM_ARROW").is_ok_and(|v| v == "1" || v == "on"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(name: &str) -> Option<u32> {
        name.strip_prefix("Viewer@cursors/").and_then(|k| k.parse::<u32>().ok()).filter(|&k| k < 23).map(|k| 100 + k)
    }

    #[test]
    fn the_civilopedia_book_is_its_own_pointer() {
        // (The F7 screen over a wonder: the game's pointer sprite, cell 13.)
        let s = civ3_sys::cursor::sites(civ3_sys::Build::PcGames).unwrap();
        let book = civ3_sys::cursor::read(&Word(s.sprites + CIVILOPEDIA * s.stride), &s);
        assert_eq!(book, Some(GameCursor::Sprite(CIVILOPEDIA)));
        assert_eq!(set_name(CIVILOPEDIA), "Viewer@cursors/13");
        // Its own art, whatever the last system pointer, the arrow's art
        // or the system's.
        for standard in [None, Some(32512)] {
            for system_arrow in [false, true] {
                assert_eq!(scene_cursor(book, standard, set, system_arrow), Cursor::Sprite(113));
            }
        }
        // Over the world while a command waits on the interface: kept.
        assert_eq!(targeting(book, 0, false, false), book);
        // Left (the game sets the arrow): the arrow again.
        assert_eq!(scene_cursor(Some(GameCursor::System), Some(32512), set, false), Cursor::Sprite(100));
    }

    struct Word(u32);
    impl civ3_sys::controls::Mem for Word {
        fn u32(&self, addr: u32) -> Option<u32> {
            (addr == 0xCA_D2D0).then_some(self.0)
        }
        fn u8(&self, _: u32) -> Option<u8> {
            None
        }
    }

    #[test]
    fn the_games_cursor_becomes_its_sprite_or_a_system_pointer() {
        assert_eq!(scene_cursor(Some(GameCursor::Sprite(9)), None, set, false), Cursor::Sprite(109));
        // The system's arrow: the game's arrow art, or the system's.
        assert_eq!(scene_cursor(Some(GameCursor::System), Some(32512), set, false), Cursor::Sprite(100));
        assert_eq!(scene_cursor(Some(GameCursor::System), None, set, true), Cursor::System(SystemCursor::Arrow));
        // Busy: the system's own pointers, art or not.
        assert_eq!(scene_cursor(Some(GameCursor::System), Some(32514), set, false), Cursor::System(SystemCursor::Wait));
        assert_eq!(scene_cursor(None, Some(32650), set, true), Cursor::System(SystemCursor::Progress));
        assert_eq!(scene_cursor(None, Some(32514), |_| None, false), Cursor::System(SystemCursor::Wait));
        // Resizing: the system's.
        assert_eq!(scene_cursor(None, Some(32645), set, false), Cursor::System(SystemCursor::ResizeNs));
        // Unknown sprites, no art: the arrow, else the renderer's default.
        assert_eq!(scene_cursor(Some(GameCursor::OtherSprite), None, set, false), Cursor::Sprite(100));
        assert_eq!(scene_cursor(Some(GameCursor::Sprite(30)), None, set, false), Cursor::Sprite(100));
        assert_eq!(scene_cursor(Some(GameCursor::Sprite(3)), None, |_| None, false), Cursor::Default);
    }

    /// `Unit_Mode_Actions`: rebase, bombard (land), air drop, airlift.
    const REBASE: i32 = 0xA;
    const BOMBARD: i32 = 0x7;
    const AIR_DROP: i32 = 0x4;
    const AIRLIFT: i32 = 0x8;

    #[test]
    fn a_tile_the_command_does_not_take_shows_not_here() {
        // Whatever the game's own pointer shows meanwhile (its arrow: its
        // pointer rests on the order buttons).
        let shown = Some(GameCursor::System);
        for mode in [REBASE, BOMBARD, AIR_DROP, AIRLIFT, 0x13, 0xB, 0x9] {
            // Out of range, no city or airfield, nothing to bombard.
            assert_eq!(targeting(shown, mode, false, true), Some(GameCursor::Sprite(NOT_HERE)), "{mode:#x}");
            assert_eq!(scene_cursor(targeting(shown, mode, false, true), None, set, false), Cursor::Sprite(100 + NOT_HERE));
            // A tile it takes: the command's own pointer.
            assert_eq!(targeting(shown, mode, true, true), Some(GameCursor::Sprite(mode as u32)), "{mode:#x}");
            // The pointer on the interface: the game's.
            assert_eq!(targeting(shown, mode, false, false), shown);
        }
        // Go-to takes every tile (its blank pointer stays).
        for mode in [crate::targeting::GO_TO, crate::targeting::STACK_GO_TO] {
            assert_eq!(targeting(Some(GameCursor::Sprite(mode as u32)), mode, false, true), Some(GameCursor::Sprite(mode as u32)));
        }
        // No command waiting (0, or the arrow `IDC_ARROW`).
        for mode in [0, 0x7F00] {
            assert_eq!(targeting(shown, mode, false, true), shown);
        }
    }
}

/// Which of the pointer moves the bridge posts the game has handled, so
/// that the scene's cursor tells where it is the game's answer for
/// (`scene::Scene::cursor_at`).
///
/// The game takes its messages through jgl's `PeekMessageA`
/// (`crate::heartbeat`): a `WM_MOUSEMOVE` it removes there is dispatched
/// (its form's mouse move setting the cursor, 0x604350 -> 0x606A70) before
/// it peeks again, so the next peek marks it handled. Each move posted is
/// kept with the viewer's position it came from ([`posted`]), the last
/// [`SLOTS`] of them; a handled move not among them (one the bridge made
/// for the world, or the system's) is no known position. Lock-free: the
/// game's thread writes on its peeks, the forwarder on posting, the
/// publisher reads.
pub mod handled {
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::SeqCst};

    /// The moves remembered.
    pub const SLOTS: usize = 16;
    const WM_MOUSEMOVE: u32 = 0x200;
    const PM_REMOVE: u32 = 1;
    /// A known message: its `lParam` with this bit above it (0: none).
    const SET: u64 = 1 << 32;

    static GAME: [AtomicU64; SLOTS] = [const { AtomicU64::new(0) }; SLOTS];
    static INPUT: [AtomicU32; SLOTS] = [const { AtomicU32::new(0) }; SLOTS];
    static NEXT: AtomicU32 = AtomicU32::new(0);
    /// The move removed by the last peek, until the next one.
    static PENDING: AtomicU64 = AtomicU64::new(0);
    /// The last move handled.
    static HANDLED: AtomicU64 = AtomicU64::new(0);

    /// `lParam` of a point (x low, y high, 16 bits each).
    fn pack((x, y): (i32, i32)) -> u32 {
        (x as u16 as u32) | ((y as u16 as u32) << 16)
    }

    fn unpack(v: u32) -> (i32, i32) {
        (v as u16 as i16 as i32, (v >> 16) as u16 as i16 as i32)
    }

    /// A move to client position `game` was posted for the viewer's
    /// position `input`.
    pub fn posted(game: (i32, i32), input: (i32, i32)) {
        let k = NEXT.fetch_add(1, SeqCst) as usize % SLOTS;
        INPUT[k].store(pack(input), SeqCst);
        GAME[k].store(SET | pack(game) as u64, SeqCst);
    }

    /// The game's thread peeks again: the move it removed before is handled.
    pub fn peeking() {
        let p = PENDING.swap(0, SeqCst);
        if p != 0 {
            HANDLED.store(p, SeqCst);
        }
    }

    /// The peek answered `got` for `remove`, `message` and `lparam` the
    /// message it gave.
    pub fn peeked(got: bool, remove: u32, message: u32, lparam: u32) {
        if got && remove & PM_REMOVE != 0 && message == WM_MOUSEMOVE {
            PENDING.store(SET | lparam as u64, SeqCst);
        }
    }

    /// The viewer's position of the last move the game handled, if it is
    /// one of those posted. (Read before the cursor: the cursor is then at
    /// least as new as this.)
    pub fn at() -> Option<(i32, i32)> {
        let h = HANDLED.load(SeqCst);
        if h == 0 {
            return None;
        }
        let next = NEXT.load(SeqCst) as usize;
        (1..=SLOTS).map(|back| (next + SLOTS * 2 - back) % SLOTS).find(|&k| GAME[k].load(SeqCst) == h).map(|k| unpack(INPUT[k].load(SeqCst)))
    }

    #[cfg(test)]
    pub(super) fn reset() {
        for k in 0..SLOTS {
            GAME[k].store(0, SeqCst);
            INPUT[k].store(0, SeqCst);
        }
        PENDING.store(0, SeqCst);
        HANDLED.store(0, SeqCst);
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_move_counts_once_the_game_peeks_after_taking_it() {
            reset();
            assert_eq!(at(), None);
            // (The city screen's map: the game's position is not the viewer's.)
            posted((100, 200), (130, 260));
            posted((640, 380), (640, 380));
            // Taken from the queue, not dispatched yet.
            peeked(true, PM_REMOVE, WM_MOUSEMOVE, pack((100, 200)));
            assert_eq!(at(), None);
            peeking();
            assert_eq!(at(), Some((130, 260)));
            // Looked at without taking it, or another message: nothing.
            peeked(true, 0, WM_MOUSEMOVE, pack((640, 380)));
            peeked(true, PM_REMOVE, 0x201, pack((640, 380)));
            peeked(false, PM_REMOVE, WM_MOUSEMOVE, pack((640, 380)));
            peeking();
            assert_eq!(at(), Some((130, 260)));
            peeked(true, PM_REMOVE, WM_MOUSEMOVE, pack((640, 380)));
            peeking();
            assert_eq!(at(), Some((640, 380)));
            // A move the bridge did not post for the viewer: unknown.
            peeked(true, PM_REMOVE, WM_MOUSEMOVE, pack((7, 9)));
            peeking();
            assert_eq!(at(), None);
            // Negative positions keep their sign.
            posted((-2, 5), (-2, 5));
            peeked(true, PM_REMOVE, WM_MOUSEMOVE, pack((-2, 5)));
            peeking();
            assert_eq!(at(), Some((-2, 5)));
        }
    }
}
