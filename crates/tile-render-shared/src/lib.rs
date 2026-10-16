//! GPU-visible data layout of the tile renderer.
//!
//! Everything here is `repr(C)`, made of 32-bit scalars only, and padded to
//! 16 bytes, so the same definition is valid as a uniform/storage buffer
//! element on the host (via `bytemuck`) and in the rust-gpu shaders.
//!
//! # Data model
//!
//! A world is a grid of cells. Every cell has one value per *layer*. A layer's
//! values are packed back to back in the `tile_data` buffer, `value_bytes`
//! bytes each (1–4, little endian), optionally followed by one *phase* byte
//! that offsets the layer's animation per cell. The value indexes the layer's
//! lookup table, which yields a *sprite set*: an animation, i.e. a frame count
//! followed by sprite indices. A sprite is a rectangle in the atlas plus the
//! offset at which it is drawn relative to the cell's origin.
//!
//! ```text
//! tile_data[layer, cell] --value--> layer_lut[layer.lut_offset + value]
//!     --set offset--> sprite_sets[set] = [n, sprite_0, .., sprite_n-1]
//!     --(frame + phase) % n--> sprites[sprite] = atlas rect + anchor
//! ```
//!
//! Set offset 0 is reserved for the empty set (`n == 0`): such cells draw
//! nothing.
#![cfg_attr(target_arch = "spirv", no_std)]

#[cfg(not(target_arch = "spirv"))]
use bytemuck::{Pod, Zeroable};

/// Cells laid out on an orthogonal grid: cell (c, r) is at (c·step_x, r·step_y).
pub const LAYOUT_ORTHOGONAL: u32 = 0;
/// Odd rows shifted right by `step_x`, cells two steps apart horizontally:
/// cell (c, r) is at ((2c + (r & 1))·step_x, r·step_y). This is the classic
/// isometric "diamond" map where only tiles with even x + y exist (Civ III).
pub const LAYOUT_STAGGERED: u32 = 1;

/// The layer is not drawn.
pub const LAYER_HIDDEN: u32 = 1 << 0;

/// Vertices per sprite quad (two triangles, no index buffer).
pub const QUAD_VERTICES: u32 = 6;

/// Per-frame parameters, one uniform buffer.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[cfg_attr(not(target_arch = "spirv"), derive(Pod, Zeroable))]
pub struct FrameParams {
    /// Render target size in pixels.
    pub viewport_w: u32,
    pub viewport_h: u32,
    /// World pixel shown at the viewport's top-left corner, before zoom.
    pub camera_x: i32,
    pub camera_y: i32,

    /// Screen pixels per world pixel = zoom_num / zoom_den.
    pub zoom_num: u32,
    pub zoom_den: u32,
    /// Animation clock in ticks.
    pub frame: u32,
    /// Layers drawn by this draw call (one render pass): `layer_count`
    /// layers starting at `layer_first`.
    pub layer_count: u32,

    /// Window of cells to draw: `cols × rows` cells starting at
    /// (first_col, first_row). May extend past the world edge when it wraps.
    pub first_col: i32,
    pub first_row: i32,
    pub cols: u32,
    pub rows: u32,

    pub grid: GridParams,

    pub layer_first: u32,
    /// 1: only draw cells whose logical anchor (origin + the layer's
    /// `cull_offset`) lies in `[cull_x0, cull_x1) × [cull_y0, cull_y1)`
    /// (world pixels, unwrapped). For games that clip by tile range rather
    /// than by sprite bounds.
    pub cull: u32,
    /// 0: colours are composed as they are. 4..=8: the world composes in
    /// channels of this many bits (see `quantize`); the target holds such
    /// channel values.
    pub color_bits: u32,
    /// Index of the view being drawn (free sprites of other views are
    /// skipped).
    pub view: u32,

    pub cull_x0: i32,
    pub cull_y0: i32,
    pub cull_x1: i32,
    pub cull_y1: i32,

    /// Where the view sits in the render target: screen-space free sprites
    /// are given in target pixels.
    pub origin_x: i32,
    pub origin_y: i32,
    /// 0: the view shows no world (cells, world-space sprites), only its own
    /// sprites.
    pub world: u32,
    pub _pad3: u32,
}

/// Shape of the world's cell grid.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[cfg_attr(not(target_arch = "spirv"), derive(Pod, Zeroable))]
pub struct GridParams {
    /// Cells per row and number of rows.
    pub cols: u32,
    pub rows: u32,
    /// One of the `LAYOUT_*` constants.
    pub layout: u32,
    /// Bit 0: wrap horizontally, bit 1: wrap vertically.
    pub wrap: u32,
    /// Distance between neighbouring cell origins (see `LAYOUT_*`).
    pub step_x: u32,
    pub step_y: u32,
    // Scalars, not an array: std140 would align an array to 16 bytes.
    pub _pad0: u32,
    pub _pad1: u32,
}

pub const WRAP_X: u32 = 1 << 0;
pub const WRAP_Y: u32 = 1 << 1;

/// Describes one layer. Layers are grouped into *passes* (consecutive layer
/// ranges); each pass sweeps the visible cells in row-major order, drawing
/// its layers in index order within each cell, so a cell's tall sprites
/// overlap the rows behind. Passes are drawn one after the other.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[cfg_attr(not(target_arch = "spirv"), derive(Pod, Zeroable))]
pub struct LayerDesc {
    /// Byte offset of this layer's first record in `tile_data`.
    pub data_byte_offset: u32,
    /// Bytes per value: 1, 2, 3 or 4.
    pub value_bytes: u32,
    /// 0 or 1: whether every value is followed by an animation phase byte.
    pub phase_bytes: u32,
    /// Index of this layer's first entry in `layer_lut`.
    pub lut_offset: u32,

    /// Number of entries in this layer's lookup table; larger values are empty.
    pub lut_len: u32,
    /// Pixel offset of the layer relative to the cell origin.
    pub offset_x: i32,
    pub offset_y: i32,
    /// `LAYER_*` flags.
    pub flags: u32,

    /// Animation ticks per frame for this layer (>= 1).
    pub ticks_per_frame: u32,
    /// Offset from the cell origin to the point tested against the
    /// camera's cull rectangle.
    pub cull_offset_x: i32,
    pub cull_offset_y: i32,
    /// Non-zero: a "scale" layer. Its sprites darken what is below to
    /// `k / scale_den` (`k` = the sprite's red channel) instead of covering
    /// it.
    pub scale_den: u32,
}

/// A sprite placed freely rather than on a grid cell (units, effects, UI).
/// Drawn by instance index from its own buffer.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[cfg_attr(not(target_arch = "spirv"), derive(Pod, Zeroable))]
pub struct FreeSprite {
    /// Offset of its sprite set (animation) in `sprite_sets`; 0 = nothing.
    pub set: u32,
    /// Shown frame: `(frame / ticks_per_frame + phase) % frames`.
    pub phase: u32,
    pub ticks_per_frame: u32,
    /// `FREE_*` flags.
    pub flags: u32,

    /// Where the sprites' anchor offsets are measured from: world pixels, or
    /// viewport pixels with `FREE_SCREEN`.
    pub x: i32,
    pub y: i32,
    /// Non-zero: darkens like a scale layer (see `LayerDesc::scale_den`).
    pub scale_den: u32,
    /// Drawn only in this view (`ALL_VIEWS`: in every view).
    pub view: u32,
    /// With `FREE_CLIP`: only the part inside this viewport rectangle
    /// (left, top, right, bottom; right and bottom exclusive) is drawn.
    pub clip: [i32; 4],
}

/// The free sprite's position is in viewport pixels, unaffected by the
/// camera (UI).
pub const FREE_SCREEN: u32 = 1 << 0;
/// `FreeSprite::view` of sprites drawn in every view.
pub const ALL_VIEWS: u32 = u32::MAX;
/// The free sprite is clipped to its `clip` rectangle.
pub const FREE_CLIP: u32 = 1 << 1;

/// One sprite: a rectangle of an atlas page and where to draw it.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[cfg_attr(not(target_arch = "spirv"), derive(Pod, Zeroable))]
pub struct SpriteDesc {
    /// Top-left texel in the atlas page.
    pub atlas_x: u32,
    pub atlas_y: u32,
    /// Size in texels (= world pixels).
    pub width: u32,
    pub height: u32,

    /// Offset of the sprite's top-left corner from the cell origin.
    pub anchor_x: i32,
    pub anchor_y: i32,
    /// Atlas array layer.
    pub page: u32,
    pub _pad: u32,
}

/// Little-endian byte addressing on top of a `u32` buffer.
///
/// Storage buffers are arrays of 32-bit words, but tile data is packed at
/// byte granularity so that a layer with 200 distinct values costs one byte
/// per cell, not four.
pub struct ByteReader<'a> {
    words: &'a [u32],
}

impl<'a> ByteReader<'a> {
    pub fn new(words: &'a [u32]) -> Self {
        Self { words }
    }

    pub fn u8(&self, byte: u32) -> u32 {
        (self.words[(byte >> 2) as usize] >> ((byte & 3) << 3)) & 0xff
    }

    /// Reads `n` (1..=4) bytes starting at `byte`, little endian.
    pub fn uint(&self, byte: u32, n: u32) -> u32 {
        let word = byte >> 2;
        let shift = (byte & 3) << 3;
        let lo = self.words[word as usize] >> shift;
        // Bytes spilling into the next word; `shift == 0` would mean a
        // 32-bit shift, which is undefined, and is never needed then.
        let value = if shift == 0 || shift + (n << 3) <= 32 {
            lo
        } else {
            lo | (self.words[word as usize + 1] << (32 - shift))
        };
        if n >= 4 { value } else { value & ((1u32 << (n << 3)) - 1) }
    }
}

/// In a quantized colour model the target stores channel value `q` as the
/// 8-bit value `q + QUANTIZED_BIAS`. The bias lets a "scale" blit round
/// down with ordinary blending (see `scale_offset`).
pub const QUANTIZED_BIAS: u32 = 16;
/// Largest scale denominator the bias supports (`2·QUANTIZED_BIAS`).
pub const MAX_SCALE_DENOMINATOR: u32 = 32;

/// What a scale blit by `num / den` adds after multiplying the stored value
/// `S = q + B` by `num / den` (B = `QUANTIZED_BIAS`): with
/// `S' = S·num/den + B·(1 − num/den) − 1/2 + 1/(2·den)`, rounding `S'` to
/// the nearest integer — what writing to an 8-bit target does — gives
/// exactly `B + floor(q·num/den)`: `q·num/den` is a multiple of `1/den`, so
/// the result lies within `1/(2·den)` of the middle of the right interval.
/// Needs `num < den` (`num == den` changes nothing and is not drawn) and
/// `den <= MAX_SCALE_DENOMINATOR` (so the offset is not negative).
pub fn scale_offset(num: u32, den: u32) -> f32 {
    QUANTIZED_BIAS as f32 * (1.0 - num as f32 / den as f32) - 0.5 + 0.5 / den as f32
}

/// The exact result of a scale blit on a stored quantized value.
pub fn scale_stored(stored: u32, num: u32, den: u32) -> u32 {
    QUANTIZED_BIAS + (stored - QUANTIZED_BIAS) * num / den
}

/// Translucent texels in a quantized colour model blend in steps of
/// `1/ALPHA_STEPS` and round down, like a game mixing two RGB555 values
/// with integer arithmetic (50 % = `(src + dst) >> 1`).
pub const ALPHA_STEPS: u32 = 32;

/// A texel's 8-bit alpha as a number of [`ALPHA_STEPS`].
pub fn alpha_steps(a8: u32) -> u32 {
    (a8 * ALPHA_STEPS + 127) / 255
}

/// What a translucent texel with `k` alpha steps adds to its premultiplied
/// colour: `S·k/n + D·(1 − k/n)` has a fractional part that is a multiple
/// of `1/n` (n = [`ALPHA_STEPS`]), so this offset makes rounding to the
/// nearest integer give the floor (see [`mix_stored`]).
pub fn mix_offset() -> f32 {
    -0.5 + 0.5 / ALPHA_STEPS as f32
}

/// The exact result of drawing stored value `src` with `k` alpha steps
/// over stored value `dst`.
pub fn mix_stored(src: u32, dst: u32, k: u32) -> u32 {
    QUANTIZED_BIAS + ((src - QUANTIZED_BIAS) * k + (dst - QUANTIZED_BIAS) * (ALPHA_STEPS - k)) / ALPHA_STEPS
}

/// An 8-bit channel value in a `bits`-bit colour space (4..=8): the top
/// bits, as games that compose in e.g. RGB555 store it.
pub fn quantize(c8: u32, bits: u32) -> u32 {
    c8 >> (8 - bits)
}

/// A `bits`-bit channel value back to 8 bits by bit replication, as such
/// games' colours reach the screen (31 -> 255, 16 -> 132 for 5 bits).
pub fn expand(q: u32, bits: u32) -> u32 {
    let shift = 8 - bits;
    if shift == 0 { q } else { (q << shift) | (q >> (bits - shift)) }
}

/// World-pixel origin of a cell. `col`/`row` are already wrapped.
pub fn cell_origin(grid: &GridParams, col: i32, row: i32) -> (i32, i32) {
    let sx = grid.step_x as i32;
    let sy = grid.step_y as i32;
    if grid.layout == LAYOUT_STAGGERED {
        ((2 * col + (row & 1)) * sx, row * sy)
    } else {
        (col * sx, row * sy)
    }
}

/// Wraps or rejects a cell coordinate. Returns `None` outside a non-wrapping
/// world.
pub fn wrap_cell(grid: &GridParams, col: i32, row: i32) -> Option<(u32, u32)> {
    let c = if grid.wrap & WRAP_X != 0 {
        col.rem_euclid(grid.cols as i32)
    } else if col < 0 || col >= grid.cols as i32 {
        return None;
    } else {
        col
    };
    let r = if grid.wrap & WRAP_Y != 0 {
        row.rem_euclid(grid.rows as i32)
    } else if row < 0 || row >= grid.rows as i32 {
        return None;
    } else {
        row
    };
    Some((c as u32, r as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_are_16_byte_multiples() {
        assert_eq!(size_of::<FreeSprite>() % 16, 0);
        assert_eq!(size_of::<FrameParams>() % 16, 0);
        assert_eq!(size_of::<GridParams>() % 16, 0);
        assert_eq!(size_of::<LayerDesc>() % 16, 0);
        assert_eq!(size_of::<SpriteDesc>() % 16, 0);
    }

    #[test]
    fn mix_offset_rounds_to_the_floor() {
        for k in 1..ALPHA_STEPS {
            for qs in 0..=127u32 {
                for qd in (0..=127u32).step_by(3) {
                    let (s, d) = (qs + QUANTIZED_BIAS, qd + QUANTIZED_BIAS);
                    let a = k as f32 / ALPHA_STEPS as f32;
                    let v = s as f32 * a + d as f32 * (1.0 - a) + mix_offset();
                    assert_eq!(v.round() as u32, mix_stored(s, d, k), "src {qs} dst {qd} k {k}");
                }
            }
        }
        // 50 %: the average, rounded down.
        assert_eq!(mix_stored(28 + QUANTIZED_BIAS, 16 + QUANTIZED_BIAS, 16), 22 + QUANTIZED_BIAS);
        assert_eq!(mix_stored(1 + QUANTIZED_BIAS, 6 + QUANTIZED_BIAS, 16), 3 + QUANTIZED_BIAS);
    }

    #[test]
    fn scale_offset_rounds_to_the_floor() {
        for den in [2, 10, 16, 32] {
            for num in 0..den {
                for q in 0..=127u32 {
                    let s = (q + QUANTIZED_BIAS) as f32 * num as f32 / den as f32 + scale_offset(num, den);
                    assert_eq!(s.round() as u32, scale_stored(q + QUANTIZED_BIAS, num, den), "q {q} num {num}/{den}");
                    assert!(s >= 0.0);
                }
            }
        }
    }

    #[test]
    fn quantize_expand_round_trip_rgb555() {
        for q in 0..32 {
            assert_eq!(quantize(expand(q, 5), 5), q);
        }
        assert_eq!(expand(31, 5), 255);
        assert_eq!(expand(20, 5), 165);
        assert_eq!(expand(200, 8), 200);
    }

    #[test]
    fn byte_reader_reads_every_alignment_and_width() {
        let bytes: Vec<u8> = (1..=16).collect();
        let words: Vec<u32> = bytes
            .chunks(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        let r = ByteReader::new(&words);
        for start in 0..12u32 {
            for n in 1..=4u32 {
                let mut want = 0u32;
                for i in 0..n {
                    want |= (bytes[(start + i) as usize] as u32) << (8 * i);
                }
                assert_eq!(r.uint(start, n), want, "start {start} n {n}");
            }
            assert_eq!(r.u8(start), bytes[start as usize] as u32);
        }
    }

    #[test]
    fn staggered_origins_match_civ3_diamond_coordinates() {
        let grid = GridParams { cols: 50, rows: 100, layout: LAYOUT_STAGGERED, step_x: 64, step_y: 32, ..Default::default() };
        // Civ III tile (x, y) with x + y even sits at (x·64, y·32): row y, col x/2.
        for (x, y) in [(0, 0), (2, 0), (1, 1), (3, 1), (97, 99)] {
            assert_eq!(cell_origin(&grid, x / 2, y), (x * 64, y * 32));
        }
    }
}
