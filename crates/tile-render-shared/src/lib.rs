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
//! that offsets the layer's animation per cell, and optionally by a signed
//! pixel *offset* (x, y; two little-endian `i16`) that moves the cell's
//! sprite off its place (e.g. a unit on its way to the next cell): a layer
//! pays for what it uses only. The value indexes the layer's
//! lookup table, which yields a *sprite set*: an animation, i.e. a frame count
//! followed by sprite indices. A sprite is a rectangle in the atlas plus the
//! offset at which it is drawn relative to the cell's origin, and the part of
//! it a view showing the world smaller samples (see [`placed`]): the atlas
//! holds the art once, at full size.
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

pub mod text;

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

/// One copy of the output pass: the rectangle of the source texture at
/// (src_x, src_y) into the target at (dst_x, dst_y) (the pass's scissor
/// bounds it), scaled by `zoom_num / zoom_den` by nearest texel; `bits`: the
/// quantized colour depth to expand from, 0 to copy values as they are.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[cfg_attr(not(target_arch = "spirv"), derive(Pod, Zeroable))]
pub struct OutputParams {
    pub src_x: i32,
    pub src_y: i32,
    pub dst_x: i32,
    pub dst_y: i32,
    pub zoom_num: u32,
    pub zoom_den: u32,
    pub bits: u32,
    pub _pad: u32,
}

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
    /// The view's zoom (`FREE_ZOOMED` sprites made for it are drawn where it
    /// is drawn at that zoom, see `scaled`); it also keeps `grid` 16-byte
    /// aligned.
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

    /// Where screen-space coordinates start in the view: screen-space free
    /// sprites and clip rectangles are given in target pixels, relative to
    /// the view's origin less its screen shift.
    pub origin_x: i32,
    pub origin_y: i32,
    /// 0: the view shows no world (cells, world-space sprites), only its own
    /// sprites.
    pub world: u32,
    /// Which free sprites by their zoom art (`FREE_LOW`, `FREE_ZOOMED`):
    /// 0 all but zoomed ones; 1 all but zoomed and low ones (a view showing
    /// the zoomed ones, drawn at zoom 1); 2 all but low ones (a view showing
    /// the zoomed ones, drawn at its zoom: those at full resolution).
    pub zoom_art: u32,

    /// 1: the view is drawn at its zoom straight into the target
    /// (`viewport_*` in target pixels): every sprite scaled by nearest texel
    /// exactly as the view's picture drawn at zoom 1 and then scaled would
    /// show it, zoomed art at full resolution in its place in the order.
    pub scaled: u32,
    /// With `scaled`: 1 keeps sprites the scaling would shrink to nothing
    /// one target pixel wide (or high), e.g. the one-pixel lines of an
    /// interface shown smaller.
    pub hairline: u32,
    /// With `scaled`: the target pixel where the view's viewport starts.
    pub target_x: i32,
    pub target_y: i32,

    /// The view shows the world this many times smaller (1: as it is), as a
    /// game's zoomed-out view does, from the same art: `grid` is the grid at
    /// that scale ([`grid_at`]), the camera and the cull rectangle are in
    /// its pixels; layer offsets and world sprites' positions are divided
    /// ([`shrunk`]), and every sprite is drawn as [`placed`] says.
    /// Screen-space sprites are drawn as they are.
    pub downscale: u32,
    /// 1: the pass sweeps each row of cells from its last column to its
    /// first (rows still top to bottom), as a game drawing its units right
    /// to left does.
    pub descending: u32,
    /// The row of named palette 1 (see [`PALETTE_SIZE`]).
    pub named_palettes: u32,
    pub _pad2: u32,
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

    /// Animation ticks per frame for this layer; 0: the animation does not
    /// follow the clock, a cell shows the frame its phase says (see
    /// [`frame_of`]).
    pub ticks_per_frame: u32,
    /// Offset from the cell origin to the point tested against the
    /// camera's cull rectangle.
    pub cull_offset_x: i32,
    pub cull_offset_y: i32,
    /// Non-zero: a "scale" layer. Its sprites darken what is below to
    /// `k / scale_den` (`k` = the sprite's red channel) instead of covering
    /// it.
    pub scale_den: u32,

    /// 0 or [`OFFSET_BYTES`]: whether every record ends with the cell's
    /// pixel offset (see [`cell_offset`]).
    pub offset_bytes: u32,
    /// Non-zero: a value's bits from this one on pick the named palette its
    /// indexed sprites are drawn with (see `FreeSprite::palette`), the bits
    /// below it index the lookup table (see [`layer_value`]).
    pub palette_shift: u32,
    pub _pad1: u32,
    pub _pad2: u32,
}

/// A cell's raw value on `layer` as (lookup table index, named palette; see
/// `LayerDesc::palette_shift`).
pub fn layer_value(layer: &LayerDesc, raw: u32) -> (u32, u32) {
    let shift = layer.palette_shift;
    if shift == 0 || shift >= 32 { (raw, 0) } else { (raw & ((1u32 << shift) - 1), raw >> shift) }
}

/// Bytes of a cell's pixel offset in its record: x and y, `i16` each.
pub const OFFSET_BYTES: u32 = 4;

/// Byte offset of a cell's record in `tile_data`, and its size.
pub fn record_at(layer: &LayerDesc, grid_cols: u32, col: u32, row: u32) -> u32 {
    layer.data_byte_offset + (row * grid_cols + col) * (layer.value_bytes + layer.phase_bytes + layer.offset_bytes)
}

/// The pixel offset of the cell whose record starts at byte `record`
/// (0, 0 if the layer has none), in a view showing the world `n` times
/// smaller: divided, rounded down (the offset moves a sprite to a position,
/// and a game halving positions rounds those down; a cell origin is a
/// multiple of the steps).
pub fn cell_offset(data: &ByteReader, layer: &LayerDesc, record: u32, n: u32) -> (i32, i32) {
    if layer.offset_bytes == 0 {
        return (0, 0);
    }
    let at = record + layer.value_bytes + layer.phase_bytes;
    let x = sign_extend_16(data.uint(at, 2));
    let y = sign_extend_16(data.uint(at + 2, 2));
    (floor_div(x, n), floor_div(y, n))
}

fn sign_extend_16(v: u32) -> i32 {
    ((v << 16) as i32) >> 16
}

/// `v / n` rounded down (`n` 0 or 1: `v`).
pub fn floor_div(v: i32, n: u32) -> i32 {
    if n <= 1 {
        return v;
    }
    let d = n as i32;
    if v >= 0 { v / d } else { -((-v + d - 1) / d) }
}

/// The frame of an animation of `frames` frames a cell or sprite with
/// `phase` shows at clock `tick`: `(tick / ticks_per_frame + phase) %
/// frames`, or with `ticks_per_frame` 0 its phase alone (modulo `frames`).
pub fn frame_of(tick: u32, ticks_per_frame: u32, phase: u32, frames: u32) -> u32 {
    let clock = if ticks_per_frame == 0 { 0 } else { tick / ticks_per_frame };
    (clock + phase) % frames
}

/// The column of the visible window drawn as its `local`-th cell of a row
/// (`FrameParams::descending`: from the last column back).
pub fn window_col(params: &FrameParams, local: u32) -> i32 {
    let i = local % params.cols;
    params.first_col + if params.descending != 0 { (params.cols - 1 - i) as i32 } else { i as i32 }
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
    /// With `scale_den == SCALE_SUBPIXEL`: the colour drawn (0xRRGGBB).
    pub tint: u32,
    /// With `FREE_ZOOMED`: the zoom the art is made for (`num << 16 | den`)
    /// and where its anchor goes from its 1:1 position scaled (pixels at
    /// that zoom).
    pub zoom: u32,
    pub zoom_dx: i32,
    pub zoom_dy: i32,

    /// The named palette its indexed sprites' remapped indices are drawn
    /// with (see [`SpriteDesc::remap`]; 0: none, their own colours).
    pub palette: u32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
}

/// The free sprite's position is in viewport pixels, unaffected by the
/// camera (UI).
pub const FREE_SCREEN: u32 = 1 << 0;
/// `FreeSprite::view` of sprites drawn in every view.
pub const ALL_VIEWS: u32 = u32::MAX;
/// The free sprite is clipped to its `clip` rectangle.
pub const FREE_CLIP: u32 = 1 << 1;
/// The 1:1 twin of a `FREE_ZOOMED` sprite: left out where the view shows those.
pub const FREE_LOW: u32 = 1 << 2;
/// A sprite made for a zoom (e.g. text rasterised at that size,
/// `FreeSprite::zoom`): drawn only where a view at that zoom is drawn at its
/// zoom (`FrameParams::zoom_art` 2), unscaled, at its 1:1 position scaled
/// plus `zoom_dx, zoom_dy`.
pub const FREE_ZOOMED: u32 = 1 << 3;
/// Drawn in every view but `FreeSprite::view` (instead of only in it).
pub const FREE_EXCEPT_VIEW: u32 = 1 << 4;

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
    /// How a view showing the world smaller draws it: [`SHRINK`],
    /// [`SHRINK_KEEP_ANCHOR`] or [`KEEP_SIZE`] (see [`placed`]).
    pub scaling: u32,

    /// The rectangle of the sprite's texels a view showing the world `n`
    /// times smaller samples (x, y, width, height; e.g. the box of its
    /// non-blank pixels, for a game that keeps sprites cropped to it):
    /// every `n`-th texel of every `n`-th row from its top-left, `width / n`
    /// by `height / n` of them (rounded down), drawn from its top-left
    /// divided by `n` (rounded down) on. Width 0: nothing is drawn there.
    pub box_x: u32,
    pub box_y: u32,
    pub box_w: u32,
    pub box_h: u32,

    /// Non-zero: a view showing the world smaller draws sprite
    /// `downscaled - 1` instead (art a game keeps in a second place for its
    /// zoomed-out view).
    pub downscaled: u32,
    /// Non-zero: an *indexed* sprite, drawn from the index atlas (one byte,
    /// a palette index, per texel) through palette rows (see
    /// [`PALETTE_SIZE`]): this is the absolute row of its own colours (an
    /// art file's palette with its transparent and translucent entries).
    /// 0: a colour sprite of the RGBA atlas.
    pub palette: u32,
    /// An indexed sprite's indices in `[remap & 0xFFFF, remap >> 16)` take
    /// their colours from the palette row the drawing cell or free sprite
    /// picks (a nation's colours, say), where it picks one: so one copy of
    /// the art serves every colouring (see [`indexed_color`]).
    pub remap: u32,
    pub _pad2: u32,
}

/// Entries per palette row. Palette rows are `PALETTE_SIZE` RGBA8 colours
/// each (one `u32` per colour, red in the low byte), back to back: row 0
/// (all clear), the sprites' own rows, then the *named* rows draws pick by
/// number (1 is the first; `FrameParams::named_palettes` is its row).
pub const PALETTE_SIZE: u32 = 256;

/// `SpriteDesc::remap` of the index range `[lo, hi)`.
pub fn remap_range(lo: u32, hi: u32) -> u32 {
    lo | hi << 16
}

/// The colour (8 bits per channel) of index `index` of an indexed sprite
/// whose own row is `own` and remapped range `remap`, drawn with named
/// palette `named` (0: none; the named rows start at row `named_base`).
pub fn indexed_color(index: u32, own: u32, remap: u32, named: u32, named_base: u32, palettes: &[u32]) -> [u32; 4] {
    let remapped = named != 0 && index >= (remap & 0xFFFF) && index < (remap >> 16);
    let row = if remapped { named_base + named - 1 } else { own };
    let i = (row * PALETTE_SIZE + index) as usize;
    let c = if i < palettes.len() { palettes[i] } else { 0 };
    [c & 0xFF, (c >> 8) & 0xFF, (c >> 16) & 0xFF, c >> 24]
}

/// [`SpriteDesc::scaling`]: shrunk, its anchor divided by the factor
/// (rounded toward zero).
pub const SHRINK: u32 = 0;
/// [`SpriteDesc::scaling`]: shrunk, its anchor kept.
pub const SHRINK_KEEP_ANCHOR: u32 = 1;
/// [`SpriteDesc::scaling`]: drawn as it is, anchor and all (e.g. art meant
/// for the screen).
pub const KEEP_SIZE: u32 = 2;

/// Where and how a sprite is drawn, relative to the point it is anchored
/// at (see [`placed`]).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Placed {
    /// The sprite's top-left at the view's scale, from the anchor point.
    pub corner_x: i32,
    pub corner_y: i32,
    /// Where the drawn rectangle starts, from the corner, and its size.
    pub inset_x: u32,
    pub inset_y: u32,
    pub width: u32,
    pub height: u32,
    /// The texel (in the sprite) drawn at the rectangle's top-left; the one
    /// at (i, j) of the rectangle is `texel + step·(i, j)`.
    pub texel_x: u32,
    pub texel_y: u32,
    pub step: u32,
}

/// How `sprite` is drawn in a view showing the world `n` times smaller
/// (`FrameParams::downscale`; 1: whole, at its anchor). A shrunk sprite
/// covers its box divided by `n` (see [`SpriteDesc::box_x`]) and samples
/// every `n`-th texel of it: pixel (i, j) of the drawn rectangle shows
/// texel `(box_x + n·i, box_y + n·j)` and lies at
/// `anchor' + (box_x / n + i, box_y / n + j)`, `anchor'` being the anchor
/// divided by `n` (rounded toward zero) or kept, as `scaling` says.
pub fn placed(sprite: &SpriteDesc, n: u32) -> Placed {
    if n <= 1 || sprite.scaling == KEEP_SIZE {
        return Placed {
            corner_x: sprite.anchor_x,
            corner_y: sprite.anchor_y,
            inset_x: 0,
            inset_y: 0,
            width: sprite.width,
            height: sprite.height,
            texel_x: 0,
            texel_y: 0,
            step: 1,
        };
    }
    let keep = sprite.scaling == SHRINK_KEEP_ANCHOR;
    Placed {
        corner_x: if keep { sprite.anchor_x } else { shrunk(sprite.anchor_x, n) },
        corner_y: if keep { sprite.anchor_y } else { shrunk(sprite.anchor_y, n) },
        inset_x: sprite.box_x / n,
        inset_y: sprite.box_y / n,
        width: sprite.box_w / n,
        height: sprite.box_h / n,
        texel_x: sprite.box_x,
        texel_y: sprite.box_y,
        step: n,
    }
}

/// The sprite drawn for sprite `id` in a view showing the world `n` times
/// smaller (see [`SpriteDesc::downscaled`]).
pub fn drawn_sprite(sprites: &[SpriteDesc], id: u32, n: u32) -> u32 {
    let alt = sprites[id as usize].downscaled;
    if n > 1 && alt != 0 { alt - 1 } else { id }
}

/// A world position or offset `v` in a view showing the world `n` times
/// smaller: divided, rounded toward zero (as C divides).
pub fn shrunk(v: i32, n: u32) -> i32 {
    if n <= 1 { v } else { v / n as i32 }
}

/// `grid` as a view showing the world `n` times smaller lays it out: its
/// steps divided (rounded down).
pub fn grid_at(grid: &GridParams, n: u32) -> GridParams {
    let n = if n == 0 { 1 } else { n };
    GridParams { step_x: grid.step_x / n, step_y: grid.step_y / n, ..*grid }
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

/// A free sprite's `scale_den` that inverts what is under its opaque
/// texels (`Blend::Invert`).
pub const SCALE_INVERT: u32 = u32::MAX;
/// A free sprite's `scale_den` for ClearType-like text: its texels hold
/// coverage per colour channel (red, green, blue; alpha: any), it draws
/// `FreeSprite::tint` with that coverage per channel (`Blend::Subpixel`).
pub const SCALE_SUBPIXEL: u32 = u32::MAX - 1;
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

/// How far the world repeats along each axis in pixels (0: it does not wrap).
pub fn world_period(grid: &GridParams) -> (i32, i32) {
    let w = if grid.layout == LAYOUT_STAGGERED { 2 * grid.cols * grid.step_x } else { grid.cols * grid.step_x };
    let x = if grid.wrap & WRAP_X != 0 { w as i32 } else { 0 };
    let y = if grid.wrap & WRAP_Y != 0 { (grid.rows * grid.step_y) as i32 } else { 0 };
    (x, y)
}

/// How far before the camera a world sprite of a wrapping world is still
/// drawn there rather than one period on (its art may reach right of it).
pub const WRAP_MARGIN: i32 = 512;

/// A world sprite's distance `d` from the camera along an axis repeating
/// every `period` pixels (0: not at all): the copy of it that lies in
/// [−`WRAP_MARGIN`, `period` − `WRAP_MARGIN`).
pub fn wrap_distance(d: i32, period: i32) -> i32 {
    if period <= 2 * WRAP_MARGIN {
        return d;
    }
    let mut m = d % period;
    if m < 0 {
        m += period;
    }
    if m >= period - WRAP_MARGIN { m - period } else { m }
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
        assert_eq!(size_of::<text::TextGlyph>() % 16, 0);
    }

    #[test]
    fn shrunk_sprites_sample_their_box_from_its_corner() {
        let s = SpriteDesc { width: 128, height: 64, anchor_x: -19, anchor_y: -15, box_x: 1, box_y: 3, box_w: 126, box_h: 61, ..Default::default() };
        // Whole at 1:1, the box aside.
        assert_eq!(placed(&s, 1), Placed { corner_x: -19, corner_y: -15, width: 128, height: 64, step: 1, ..Default::default() });
        // Half size: the anchor toward zero, the box's corner and size rounded down.
        let p = placed(&s, 2);
        assert_eq!((p.corner_x, p.corner_y, p.inset_x, p.inset_y, p.width, p.height, p.texel_x, p.texel_y, p.step), (-9, -7, 0, 1, 63, 30, 1, 3, 2));
        let keep = placed(&SpriteDesc { scaling: SHRINK_KEEP_ANCHOR, ..s }, 2);
        assert_eq!((keep.corner_x, keep.corner_y), (-19, -15));
        assert_eq!(placed(&SpriteDesc { scaling: KEEP_SIZE, ..s }, 2), placed(&s, 1));
        assert_eq!((shrunk(-13, 2), shrunk(13, 2), shrunk(-13, 1)), (-6, 6, -13));
        let sprites = [s, SpriteDesc { downscaled: 1, ..s }];
        assert_eq!((drawn_sprite(&sprites, 1, 1), drawn_sprite(&sprites, 1, 2), drawn_sprite(&sprites, 0, 2)), (1, 0, 0));
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
    fn offsets_divide_rounding_down_and_frames_may_ignore_the_clock() {
        assert_eq!((floor_div(-7, 2), floor_div(7, 2), floor_div(-6, 3), floor_div(-7, 1)), (-4, 3, -2, -7));
        assert_eq!((sign_extend_16(0xFFFF), sign_extend_16(0x8000), sign_extend_16(0x7FFF)), (-1, -32768, 32767));
        assert_eq!((frame_of(10, 3, 1, 4), frame_of(10, 0, 5, 4), frame_of(u32::MAX, 0, 2, 3)), (0, 1, 2));
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
