//! The pointer the game shows, for the viewer (`scene::Cursor`): its cursor
//! sprites by their sets in the render description (`Viewer@cursors/<k>`,
//! cell `k` of `Art\cursor.pcx`), the system's busy and resize pointers as
//! such. The plain arrow, which the game leaves to the system, is shown
//! with the game's own arrow art (cell 0) unless
//! `CIV3R_SYSTEM_ARROW=1`.

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
        // Busy, resizing: the system's.
        assert_eq!(scene_cursor(Some(GameCursor::System), Some(32514), set, false), Cursor::System(SystemCursor::Wait));
        assert_eq!(scene_cursor(None, Some(32645), set, false), Cursor::System(SystemCursor::ResizeNs));
        // Unknown sprites, no art: the arrow, else the renderer's default.
        assert_eq!(scene_cursor(Some(GameCursor::OtherSprite), None, set, false), Cursor::Sprite(100));
        assert_eq!(scene_cursor(Some(GameCursor::Sprite(30)), None, set, false), Cursor::Sprite(100));
        assert_eq!(scene_cursor(Some(GameCursor::Sprite(3)), None, |_| None, false), Cursor::Default);
    }
}
