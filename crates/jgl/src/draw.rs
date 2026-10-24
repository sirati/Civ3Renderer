//! The drawing calls of the fill group (image slots +0x44..+0x64: fills,
//! shading, lines, frames) and the copy group (+0x38, +0x40, +0x84,
//! +0x8C), as values.
//!
//! Each call carries what it draws onto (and, for a copy, what it copies
//! from) as a [`Surface`]: the image, its owner and its clip and bounds as
//! they were at the call. Until the surface group is ours, nothing else
//! can tell them later.

use crate::{ImageId, Rect};

/// An image a call draws onto or copies from, as it was at the call.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Surface {
    pub image: ImageId,
    /// The image's back pointer to its owner (+0x80), opaque to jgl.
    pub owner: u32,
    /// The clip rectangle (+0x44).
    pub clip: Rect,
    /// The whole image (+0x54).
    pub bounds: Rect,
}

impl Surface {
    /// Width and height.
    pub fn size(&self) -> (i32, i32) {
        (self.bounds.right - self.bounds.left, self.bounds.bottom - self.bounds.top)
    }
}

/// The most arguments a slot of the two groups takes (+0x60: 36 bytes).
pub const MAX_ARGS: usize = 9;

/// A call of the fill group. Colours are as the slot gets them: a 16-bit
/// pixel value (bit 31 set: a direct colour) or a palette index.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FillCall {
    /// +0x44 `fill(rect, color)` [V]: the rectangle (`None`: a null
    /// pointer, the whole image) in one colour.
    Fill { target: Surface, rect: Option<Rect>, color: u32 },
    /// +0x48 `shade_rect(rect, color, percent)` [V]: `color` mixed into
    /// the rectangle (`None`: the whole image), keeping `percent` of what
    /// is there; 16-bit images only.
    Shade { target: Surface, rect: Option<Rect>, color: u32, percent: u32 },
    /// +0x58 `replace_color(rect, from, to)` [V, jgl RVA 0x2740]: within
    /// the rectangle and the clip, every pixel of value `from` becomes
    /// `to`; 16-bit images only. It reads the pixels.
    ReplaceColor { target: Surface, rect: Rect, from: u32, to: u32 },
    /// +0x64 `line(x1, y1, x2, y2, color, flag)` [V]: the exe's line,
    /// horizontal and vertical line functions pass `flag` 1.
    Line { target: Surface, from: (i32, i32), to: (i32, i32), color: u32, flag: u32 },
    /// +0x4C, +0x50, +0x54, +0x5C, +0x60: not seen called (counting mode,
    /// `docs/jgl-replacement.md` §9). The raw arguments, `args[..n]` with
    /// `n` the slot's argument count.
    Other { slot: u16, target: Surface, args: [u32; MAX_ARGS] },
}

impl FillCall {
    /// The image drawn onto.
    pub fn target(&self) -> &Surface {
        match self {
            FillCall::Fill { target, .. }
            | FillCall::Shade { target, .. }
            | FillCall::ReplaceColor { target, .. }
            | FillCall::Line { target, .. }
            | FillCall::Other { target, .. } => target,
        }
    }
}

/// A call of the copy group. `source` is the image whose method it is
/// (`this`), `target` the image its first argument names.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CopyCall {
    /// +0x40 `copy(target, src, dst)` [V]: `StretchBlt` of the source's
    /// `src` rectangle onto the target's `dst` rectangle.
    Stretch { source: Surface, target: Surface, src: Rect, dst: Rect },
    /// +0x84 `keyed_draw_onto(target, x, y)` [V, the exe's
    /// `PCX_Image::draw_onto`]: the whole source onto the target at (x, y),
    /// its colour key left out.
    Keyed { source: Surface, target: Surface, x: i32, y: i32 },
    /// +0x38 `(target, sx, sy, x, y, w, h, mode)` [V, jgl RVA 0x6A90]:
    /// `w`×`h` of an 8-bit source at (sx, sy) onto the target (8 or 16
    /// bits) at (x, y), within the target's clip, through the source's
    /// colour table (+0x7C, else jgl's default). What `mode` selects is
    /// not known [G].
    Indexed { source: Surface, target: Surface, src: (i32, i32), at: (i32, i32), size: (i32, i32), mode: u32 },
    /// +0x8C: not seen called. `this` and the raw arguments (the first is
    /// presumably the target image [G]).
    Other { slot: u16, this: Surface, args: [u32; 4] },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_size_is_the_bounds() {
        let s = Surface { bounds: Rect::new(2, 3, 12, 33), ..Surface::default() };
        assert_eq!(s.size(), (10, 30));
    }
}
