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
    pub _pad1: u32,
    pub _pad2: u32,

    pub cull_x0: i32,
    pub cull_y0: i32,
    pub cull_x1: i32,
    pub cull_y1: i32,
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
    pub _pad: u32,
}

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
        assert_eq!(size_of::<FrameParams>() % 16, 0);
        assert_eq!(size_of::<GridParams>() % 16, 0);
        assert_eq!(size_of::<LayerDesc>() % 16, 0);
        assert_eq!(size_of::<SpriteDesc>() % 16, 0);
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
        for (x, y) in [(0, 0), (2, 0), (1, 1), (3, 1), (98, 99)] {
            assert_eq!(cell_origin(&grid, x / 2, y), (x * 64, y * 32));
        }
    }
}
