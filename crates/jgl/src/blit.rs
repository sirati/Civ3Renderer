//! The sprite group (`docs/jgl-replacement.md` §6, group 4): sprites, their
//! blits onto images (sprite slots +0x38..+0xA0), the sprite state slots
//! (+0x04 init, +0x0C duplicate, +0x10..+0x18 colour table and key), the
//! global draw scale (graphics +0x9C) and the colour tables' slots, as
//! values.
//!
//! A blit carries the sprite and every image it touches as they were at
//! the call, the colour table it names, the draw scale and who set it, and
//! the call site: the return address into whoever called the slot. jgl
//! passes the call sites on as opaque numbers, as it does owners; they let
//! a consumer tell which of the game's own functions a blit came from
//! (several of them use one slot, and the scaled ones set the scale and
//! call an unscaled one).

use crate::Surface;

/// A `JGLSprite`, by its address.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpriteId(pub u32);

/// A `JGL_Color_Table`, by its address (0: none).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TableId(pub u32);

/// A colour table as a call names it: the table and its owner (+0x80C,
/// the constructor's argument), opaque to jgl. Both 0 for none.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TableRef {
    pub table: TableId,
    pub owner: u32,
}

/// A sprite as it was at a call.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SpriteRef {
    pub sprite: SpriteId,
    /// The constructor's argument (+0x0C), opaque to jgl.
    pub owner: u32,
    /// The sprite's own colour table (+0x10), used when a blit names none.
    pub table: TableId,
    /// Bits per pixel of its pixels (+0x20): 8 or 16.
    pub bits: u32,
    /// Whether jgl keeps its pixels as spans (+0x18 bit 0; for sprites
    /// made after the takeover, jgl's rule, [`compiles`]): its scaled
    /// blits place such sprites differently.
    pub compiled: bool,
    /// Whether its pixels are the placeholder (jgl made none).
    pub placeholder: bool,
}

/// Whether jgl's `init` (+0x04) keeps a sprite as spans of non-key
/// pixels [V, the 8- and 16-bit paths 0xAA60 and 0x9F50 test the same]:
/// pixels given, span compilation on (jgl's global at RVA 0x70D94), wider
/// than 4 and narrower than 256 pixels, fewer than 65536 of them.
pub fn compiles(pixels: u32, compile: bool, (w, h): (i32, i32)) -> bool {
    pixels != 0 && compile && w > 4 && w < 0x100 && (w as i64 * h as i64) < 0x10000
}

/// The global draw scale (graphics +0x9C `set_scale(num_x, num_y, den)`):
/// sprites are drawn `num/den` times their size. `set_at` is the call site
/// of the `set_scale` that set it (0: set before the takeover).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Scale {
    pub num_x: u32,
    pub num_y: u32,
    pub den: u32,
    pub set_at: u32,
}

impl Default for Scale {
    fn default() -> Scale {
        Scale { num_x: 1, num_y: 1, den: 1, set_at: 0 }
    }
}

impl Scale {
    /// The three numbers, as `set_scale` takes them.
    pub fn ratio(&self) -> [u32; 3] {
        [self.num_x, self.num_y, self.den]
    }

    /// Whether sprites are drawn at their size.
    pub fn identity(&self) -> bool {
        self.num_x == self.den && self.num_y == self.den
    }
}

/// The most stack arguments of a blit slot (+0x88: 36 bytes).
pub const MAX_BLIT_ARGS: usize = 9;

/// Where a blit slot's arguments are, by index of its stack arguments
/// [V for the slots the exe's wrappers call, from their pushes; the rest
/// not seen called].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    /// The image drawn onto.
    pub target: Option<usize>,
    /// A second image (the first argument of +0x88/+0x8C, the background
    /// of +0x50).
    pub other: Option<usize>,
    /// A second sprite (the mask of +0x50 and +0x58).
    pub mask: Option<usize>,
    /// x and y (the first of the two).
    pub at: Option<usize>,
    /// The colour table.
    pub table: Option<usize>,
}

impl Shape {
    const fn new(target: usize, at: usize, table: usize) -> Shape {
        Shape { target: Some(target), other: None, mask: None, at: Some(at), table: Some(table) }
    }

    /// The shape of blit slot `slot` (`None`: not seen called, arguments
    /// unknown).
    pub const fn of(slot: u16) -> Option<Shape> {
        Some(match slot {
            // (target, x, y, ct): Sprite::draw, draw_on_map (0x5F8210,
            // 0x5F8060), and 0x5F81A0.
            0x44 | 0x5C => Shape::new(0, 1, 3),
            // (target, x, y, a4, ct): 0x5F84A0, 0x5F8760, and draw_blend
            // 0x5F8620 (a4: the blend table).
            0x74 | 0x7C | 0x84 => Shape::new(0, 1, 4),
            // (canvas, target, x, y, blend table, ct): 0x5F87E0.
            0x8C => Shape { other: Some(0), ..Shape::new(1, 2, 5) },
            // (canvas, target, x, y, num_x, num_y, den, flags, ct): 0x5F8870.
            0x88 => Shape { other: Some(0), ..Shape::new(1, 2, 8) },
            // (target, x, y, opacity, ct, flags): 0x5F89A0.
            0x94 => Shape::new(0, 1, 4),
            // (mask, background, target, x, y, ct): 0x5F8310.
            0x50 => Shape { other: Some(1), mask: Some(0), ..Shape::new(2, 3, 5) },
            // (mask, target, x, y, ct): 0x5F8380.
            0x58 => Shape { mask: Some(0), ..Shape::new(1, 2, 4) },
            _ => return None,
        })
    }
}

/// A blit: sprite slot `slot` called on `sprite`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BlitCall {
    pub slot: u16,
    /// The call site (the return address into the caller), opaque.
    pub caller: u32,
    pub sprite: SpriteRef,
    /// The image drawn onto, where the slot's [`Shape`] is known.
    pub target: Option<Surface>,
    pub other: Option<Surface>,
    pub mask: Option<SpriteRef>,
    /// Where the sprite's stored pixels go (the caller added any crop
    /// offset).
    pub at: (i32, i32),
    /// The colour table argument (none: the sprite's own).
    pub table: TableRef,
    /// The draw scale at the call.
    pub scale: Scale,
    /// Every stack argument as passed, `args[..n]` with `n` the slot's
    /// argument count; the rest 0.
    pub args: [u32; MAX_BLIT_ARGS],
}

/// A call of a sprite's state slots.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SpriteCall {
    /// +0x04 `init(pixels, width, height, depth, a5, table)` [V, from the
    /// exe's slicer 0x62B121]: the sprite's pixels made from `pixels`
    /// (8-bit indices or 16-bit colours, `depth`). `compile`: jgl's span
    /// compilation switch at the call.
    Init { sprite: SpriteRef, pixels: u32, size: (i32, i32), depth: u32, a5: u32, table: TableId, compile: bool },
    /// +0x0C `duplicate(source)` [V, `Sprite::copy` 0x5F8A8E].
    Duplicate { sprite: SpriteRef, source: SpriteRef },
    /// +0x10 `set_color_table(table)`.
    SetTable { sprite: SpriteRef, table: TableId },
    /// +0x18 `set_key(index)`.
    SetKey { sprite: SpriteRef, key: u32 },
    /// +0x08 `clear`, and +0x00 `scalar_delete`: the pixels are gone.
    Clear { sprite: SpriteRef },
}

/// A call of a colour table's slots that changes it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TableCall {
    /// +0x04 `init`.
    Init { table: TableRef },
    /// +0x08 `reset_fade`.
    ResetFade { table: TableRef },
    /// +0x0C `animate`.
    Animate { table: TableRef },
    /// +0x14 `set_colors(first, count, colors)`: the colours are `count`
    /// `RGBQUAD`s at `colors` (not copied into the record).
    SetColors { table: TableRef, first: u32, count: u32, colors: u32 },
    /// +0x20, one argument [G: not decoded].
    Slot20 { table: TableRef, arg: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slots::{Class, Group};

    #[test]
    fn shapes_fit_their_slots() {
        for s in Class::Sprite.slots().iter().filter(|s| s.offset >= 0x38) {
            assert_eq!(s.group, Group::Blit);
            let Some(shape) = Shape::of(s.offset) else { continue };
            let n = s.stack as usize / 4;
            for i in [shape.target, shape.other, shape.mask, shape.table].into_iter().flatten() {
                assert!(i < n, "+{:#x}", s.offset);
            }
            assert!(shape.at.is_some_and(|i| i + 1 < n), "+{:#x}", s.offset);
        }
    }

    #[test]
    fn identity_scale() {
        assert!(Scale::default().identity());
        assert!(Scale { num_x: 64, num_y: 64, den: 64, set_at: 1 }.identity());
        assert!(!Scale { num_x: 32, num_y: 32, den: 64, set_at: 1 }.identity());
    }
}
