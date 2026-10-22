//! The pointer the game shows, for the viewer (`scene::Cursor`): its cursor
//! sprites by their sets in the render description (`Viewer@cursors/<k>`,
//! cell `k` of `Art\cursor.pcx`), the system's resize pointers as such.
//! The system's busy pointers (`IDC_WAIT`, `IDC_APPSTARTING`) stay the
//! semantic busy states: for those the viewer shows the wait cursor's
//! hourglass, turning (`scene::loading_cursor`), as its own pointer. Cell
//! 12 is the "NEW UNIT" arrow, not a busy icon. The plain
//! arrow, which the game leaves to the system, is shown with the game's
//! own arrow art (cell 0) unless `CIV3R_SYSTEM_ARROW=1`.

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
