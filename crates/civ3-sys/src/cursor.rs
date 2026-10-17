//! The mouse cursor the game shows (docs §16.11): one of the sprites it cut
//! from `Art\cursor.pcx`, which it draws itself, or a system pointer.
//!
//! The pointer is set per form as the mouse moves over it (0x606A70): a
//! form with a cursor sprite (`Base_Form+0x208`, hot spot +0x20C/+0x210)
//! makes it the screen's (0xCAD2D0) and hides the system pointer; any other
//! sets 0xCAD2D0 to 0 and shows a system pointer (its own at +0x218, else
//! the arrow; resize arrows on frames). The map form picks its sprite by
//! the action the player is choosing a tile for (0x604350 with
//! `&Sprite[k]`; the plain pointer, k = 0, is the system arrow, 0x6043A0).
//! While it works, the game shows the system's busy pointer (`SetCursor`).
//!
//! Only the PCGames build is decoded; the others have no sites (`None`).

use crate::Build;
use crate::controls::Mem;

/// Where the game keeps its cursor.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sites {
    /// `Sprite*` drawn as the cursor (0: the system pointer shows).
    pub sprite: u32,
    /// `Sprite[23]` cut from `Art\cursor.pcx`, and the size of a `Sprite`.
    pub sprites: u32,
    pub stride: u32,
    pub count: u32,
}

pub fn sites(build: Build) -> Option<Sites> {
    match build {
        Build::PcGames => Some(Sites { sprite: 0xCA_D2D0, sprites: 0xA5_2200, stride: 0x2C, count: 23 }),
        Build::Gog | Build::Steam => None,
    }
}

/// What the game shows as the pointer.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GameCursor {
    /// Its cursor sprite `k` (cell `k` of `Art\cursor.pcx`).
    Sprite(u32),
    /// A sprite that is none of those.
    OtherSprite,
    /// A system pointer (which one: the last `SetCursor`).
    System,
}

/// The pointer the game shows now; `None` if its memory cannot be read.
pub fn read(mem: &dyn Mem, s: &Sites) -> Option<GameCursor> {
    let sprite = mem.u32(s.sprite)?;
    if sprite == 0 {
        return Some(GameCursor::System);
    }
    let k = sprite.wrapping_sub(s.sprites);
    Some(if sprite >= s.sprites && k % s.stride == 0 && k / s.stride < s.count { GameCursor::Sprite(k / s.stride) } else { GameCursor::OtherSprite })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct One(u32);
    impl Mem for One {
        fn u32(&self, addr: u32) -> Option<u32> {
            (addr == 0xCA_D2D0).then_some(self.0)
        }
        fn u8(&self, _: u32) -> Option<u8> {
            None
        }
    }

    #[test]
    fn the_cursor_sprite_is_a_cell_of_cursor_pcx() {
        let s = sites(Build::PcGames).unwrap();
        assert_eq!(read(&One(0), &s), Some(GameCursor::System));
        assert_eq!(read(&One(0xA5_2200), &s), Some(GameCursor::Sprite(0)));
        assert_eq!(read(&One(0xA5_2200 + 9 * 0x2C), &s), Some(GameCursor::Sprite(9)));
        assert_eq!(read(&One(0xA5_2200 + 23 * 0x2C), &s), Some(GameCursor::OtherSprite));
        assert_eq!(read(&One(0xA5_2200 + 3), &s), Some(GameCursor::OtherSprite));
        assert_eq!(read(&One(0x10), &s), Some(GameCursor::OtherSprite));
        assert_eq!(sites(Build::Gog), None);
    }
}
