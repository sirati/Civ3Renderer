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

pub mod glyph_cache;
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
/// `LayerDesc::flags`: value 0 draws nothing (its table's entry 0 is the
/// empty set), known without reading the table.
pub const LAYER_ZERO_EMPTY: u32 = 1 << 1;

/// Vertices per sprite quad (two triangles, no index buffer).
pub const QUAD_VERTICES: u32 = 6;
/// Vertices per cell of a pass drawn as octagons
/// ([`FrameParams::corner_cuts`]): six triangles fanned from one corner.
pub const OCTAGON_VERTICES: u32 = 18;

/// The index buffer cells are drawn with: a quad's two triangles over its
/// four corners (indices `0..6`), then an octagon's six over its eight
/// (`6..24`), so that the vertex shader runs once per corner.
pub const CELL_INDICES: [u16; 24] = [0, 1, 2, 2, 1, 3, 0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 5, 0, 5, 6, 0, 6, 7];

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
    /// 1: integer FP16 storage; 0: biased UNORM8 storage.
    pub quantized_float: u32,
}

/// One bounded destination snapshot origin for native palette composition.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[cfg_attr(not(target_arch = "spirv"), derive(Pod, Zeroable))]
pub struct NativePaletteParams {
    pub origin_x: i32,
    pub origin_y: i32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
    pub _pad3: u32,
    pub _pad4: u32,
    pub _pad5: u32,
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
    /// The render-target pixel where the viewport starts, including a
    /// source rectangle. Outline coverage uses this in every stage.
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
    /// 1: cells are drawn as octagons, their rectangles less the corners
    /// their sprites leave blank ([`SpriteDesc::cuts`]), with
    /// [`OCTAGON_VERTICES`] vertices each (where pixels map 1:1 to texels).
    pub corner_cuts: u32,
    /// Categories hidden in this camera. A zero category is always visible.
    pub hidden_categories: u32,
    /// How scaled UI backgrounds are sampled: [`BACKGROUND_DITHER`],
    /// [`BACKGROUND_INTERPOLATE`] or [`BACKGROUND_NEAREST`].
    pub background_sampling: u32,
    /// Where the view's layer table starts in `layers` (entries): the
    /// layers as its camera's binding draws them, each from its own records
    /// or from a data slot bound in their place (`LayerDesc` copies whose
    /// `data_byte_offset` points at the slot). 0: the shared table.
    pub layer_table: u32,
    /// 1: integer FP16 storage; 0: biased UNORM8 storage.
    pub quantized_float: u32,
    /// Viewer milliseconds, used only by [`HOST_CLOCK`] layers and free
    /// sprites.
    pub host_tick_ms: u32,
    pub _host_reserved0: u32,
    pub _host_reserved1: u32,
    pub _host_reserved2: u32,
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
    /// [`frame_of`]). With [`HOST_CLOCK`]: viewer milliseconds per frame
    /// (see [`layer_frame_of`]).
    pub ticks_per_frame: u32,
    /// Offset from the cell origin to the point tested against the
    /// camera's cull rectangle.
    pub cull_offset_x: i32,
    pub cull_offset_y: i32,
    /// Non-zero: a "scale" layer. Its sprites darken what is below to
    /// `k / scale_den` (`k` = the sprite's red channel) instead of covering
    /// it.
    pub scale_den: u32,
    /// Visibility category bit for per-camera clean-map filtering. Zero is
    /// always visible; nonzero categories are hidden when the camera mask
    /// contains their bit.
    pub category: u32,

    /// 0 or [`OFFSET_BYTES`]: whether every record ends with the cell's
    /// pixel offset (see [`cell_offset`]).
    pub offset_bytes: u32,
    /// Non-zero: a value's bits from this one on pick the named palette its
    /// indexed sprites are drawn with (see `FreeSprite::palette`), the bits
    /// below it index the lookup table (see [`layer_value`]).
    pub palette_shift: u32,
    pub _pad1: u32,
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

/// A free sprite period selects the viewer clock when this bit is set.
pub const HOST_CLOCK: u32 = 1 << 31;

pub fn free_frame_of(game_tick: u32, host_tick_ms: u32, ticks_per_frame: u32, phase: u32, frames: u32) -> u32 {
    let tick = if ticks_per_frame & HOST_CLOCK != 0 { host_tick_ms } else { game_tick };
    let period = (ticks_per_frame & !HOST_CLOCK).max(1);
    (tick / period).wrapping_add(phase) % frames.max(1)
}

/// The frame a layer's cell shows: with [`HOST_CLOCK`] in
/// `ticks_per_frame`, by the viewer's milliseconds (`host_tick_ms`), a
/// frame every `ticks_per_frame & !HOST_CLOCK` of them, from the cell's
/// phase on (an animation that runs whatever the driver does); else
/// [`frame_of`] by the driver's clock.
pub fn layer_frame_of(game_tick: u32, host_tick_ms: u32, ticks_per_frame: u32, phase: u32, frames: u32) -> u32 {
    if ticks_per_frame & HOST_CLOCK != 0 {
        free_frame_of(game_tick, host_tick_ms, ticks_per_frame, phase, frames)
    } else {
        frame_of(game_tick, ticks_per_frame, phase, frames)
    }
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
    /// With ordinary alpha blending: the high byte is inverse opacity.
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
    /// Visibility category bit for per-camera clean-map filtering. Zero is
    /// always visible; nonzero categories are hidden when the camera mask
    /// contains their bit.
    pub category: u32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub primitive: FreePrimitive,
}

/// The free command's explicit procedural primitive. Kind zero samples sprites.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "spirv"), derive(Pod, Zeroable))]
pub struct FreePrimitive {
    pub kind: u32,
    pub width: u32,
    pub height: u32,
    /// RGBA, red in the low byte.
    pub rgba: u32,
    /// Kind-specific words: for [`PRIMITIVE_OUTPUT_RECT`] the rectangle
    /// (see [`output_rect`]), for [`PRIMITIVE_ARROW`] and
    /// [`PRIMITIVE_ARROW_SHADOW`] the marker (see [`arrow_frame`]).
    pub a: u32,
    pub b: u32,
    pub c: u32,
    pub d: u32,
}
pub const PRIMITIVE_SPRITE: u32 = 0;
pub const PRIMITIVE_SOLID: u32 = 1;
/// A solid rectangle laid out in the pixels of the stage that draws it (a
/// view drawn at its zoom: the target's), from the sprite's position
/// scaled (see [`output_rect`]): drawn in every stage at that stage's
/// scale, every edge on a whole pixel and equal parts equally wide.
pub const PRIMITIVE_OUTPUT_RECT: u32 = 2;
/// The gilded arrow over the active unit, painted at the stage's scale
/// (see [`arrow_frame`]).
pub const PRIMITIVE_ARROW: u32 = 3;
/// The arrow's ground shadow, likewise.
pub const PRIMITIVE_ARROW_SHADOW: u32 = 4;

/// Whether primitive `kind` is laid out at the scale of the stage that
/// draws it. A view drawn at its zoom (showing zoomed art) switches to
/// drawing at it from the first such sprite on, as for art made for every
/// zoom.
pub fn output_scaled(kind: u32) -> bool {
    kind >= PRIMITIVE_OUTPUT_RECT && kind <= PRIMITIVE_ARROW_SHADOW
}

/// Sixteenths: the unit of output rectangles' and arrows' scales.
pub const SCALE_ONE: u32 = 16;

/// `len` pixels at scale `scale / SCALE_ONE`, drawn at zoom `zn / zd`, in
/// whole pixels: rounded to the nearest, halves up. (`len` and `scale` at
/// most 255 and a zoom's terms at most 4096 keep the product in 32 bits.)
pub fn output_length(len: u32, scale: u32, zn: u32, zd: u32) -> i32 {
    let (zn, zd) = (if zn == 0 { 1 } else { zn }, if zd == 0 { 1 } else { zd });
    let (len, scale) = (if len > 255 { 255 } else { len }, if scale > 255 { 255 } else { scale });
    ((2 * len * scale * zn + SCALE_ONE * zd) / (2 * SCALE_ONE * zd)) as i32
}

/// One edge of an output rectangle, `a·L1 + b·L2` (`L1`, `L2`: two lengths
/// of its axis, each rounded to whole pixels once), from `code` (`a` in its
/// low byte, `b` in the next, both signed).
fn output_edge(code: u32, l1: i32, l2: i32) -> i32 {
    let a = ((code << 24) as i32) >> 24;
    let b = ((code << 16) as i32) >> 24;
    a * l1 + b * l2
}

/// The rectangle (left, top, right, bottom) a [`PRIMITIVE_OUTPUT_RECT`]
/// covers, relative to its anchor (the sprite's position scaled to the
/// stage), in the pixels of a stage at zoom `zn / zd`. Its edges are whole
/// multiples of two lengths per axis, each scaled and rounded once
/// ([`output_length`]), so that parts of the same length come out equally
/// wide wherever they are. `c`: the lengths (x in its low two bytes, y in
/// its high two); `a`: the x edges (left in its low half, right in its
/// high half, see `output_edge`), `b` the y edges likewise; `d`: the x
/// scale in its low half, the y scale in its high half (sixteenths).
pub fn output_rect(p: &FreePrimitive, zn: u32, zd: u32) -> (i32, i32, i32, i32) {
    let (sx, sy) = (p.d & 0xFFFF, p.d >> 16);
    let length = |shift: u32, scale: u32| output_length((p.c >> shift) & 0xFF, scale, zn, zd);
    let (x1, x2, y1, y2) = (length(0, sx), length(8, sx), length(16, sy), length(24, sy));
    (output_edge(p.a & 0xFFFF, x1, x2), output_edge(p.b & 0xFFFF, y1, y2), output_edge(p.a >> 16, x1, x2), output_edge(p.b >> 16, y1, y2))
}

/// A [`PRIMITIVE_OUTPUT_RECT`] in `rgba`: `lengths` (x1, x2, y1, y2), the
/// x edges (left, right) and y edges (top, bottom) as coefficients
/// `[a, b]` of their axis's lengths, the scale per axis in sixteenths.
#[cfg(not(target_arch = "spirv"))]
pub fn output_rect_primitive(rgba: [u8; 4], lengths: [u8; 4], x: [[i8; 2]; 2], y: [[i8; 2]; 2], scale: (u16, u16)) -> FreePrimitive {
    let edge = |e: [i8; 2]| (e[0] as u8 as u32) | (e[1] as u8 as u32) << 8;
    FreePrimitive {
        kind: PRIMITIVE_OUTPUT_RECT,
        width: 0,
        height: 0,
        rgba: u32::from_le_bytes(rgba),
        a: edge(x[0]) | edge(x[1]) << 16,
        b: edge(y[0]) | edge(y[1]) << 16,
        c: u32::from_le_bytes(lengths),
        d: scale.0 as u32 | (scale.1 as u32) << 16,
    }
}

/// Where an arrow marker ([`PRIMITIVE_ARROW`], [`PRIMITIVE_ARROW_SHADOW`])
/// is in the pixels of a stage at zoom `zn / zd` showing the world `n`
/// times smaller: its scale (stage pixels per pixel of the 1:1 art) and
/// its reference point (the arrow's tip, the shadow's centre) from the
/// sprite's position scaled. `a`: its size in sixteenths; `b`: the arrow's
/// bob, in 64ths of a full-size world pixel downwards (signed; the shadow
/// stays put, shaped by it); `c`, `d`: the reference point from the
/// position, in full-size world pixels (signed).
pub fn arrow_frame(p: &FreePrimitive, zn: u32, zd: u32, n: u32) -> (f32, f32, f32) {
    let (zn, zd, n) = (if zn == 0 { 1 } else { zn }, if zd == 0 { 1 } else { zd }, if n == 0 { 1 } else { n });
    let zoom = zn as f32 / (zd * n) as f32;
    let bob = if p.kind == PRIMITIVE_ARROW { p.b as i32 as f32 / 64.0 } else { 0.0 };
    (p.a as f32 / SCALE_ONE as f32 * zoom, p.c as i32 as f32 * zoom, (p.d as i32 as f32 + bob) * zoom)
}

/// An arrow marker's bob in the pixels of its 1:1 art (what shapes its
/// shadow): `b` over its size.
pub fn marker_bob(p: &FreePrimitive) -> f32 {
    let size = if p.a == 0 { 1 } else { p.a };
    p.b as i32 as f32 / 64.0 * SCALE_ONE as f32 / size as f32
}

/// The arrow marker's 1:1 art: its size, and its tip in it.
pub const ARROW_ART: (f32, f32) = (33.0, 48.0);
pub const ARROW_TIP: (f32, f32) = (16.0, 44.0);

/// Half the width and height of the arrow's ground shadow, in pixels of
/// the 1:1 art, with the arrow `bob` of them below its rest: wider the
/// lower it is (41 by 13 at rest).
pub fn shadow_half(bob: f32) -> (f32, f32) {
    ((41.0 + bob) / 2.0, (13.0 + bob / 2.0) / 2.0)
}

/// Where an arrow marker is drawn in the pixels of a stage at zoom
/// `zn / zd` showing the world `n` times smaller, its position scaled to
/// that stage at `anchor`: the rectangle it may cover (left, top, right,
/// bottom), its scale ([`arrow_frame`]) and its reference point (the
/// arrow's tip, the shadow's centre) from the rectangle's top-left.
pub fn marker_place(p: &FreePrimitive, anchor: (i32, i32), zn: u32, zd: u32, n: u32) -> ([i32; 4], f32, (f32, f32)) {
    let floor = |v: f32| { let i = v as i32; if (i as f32) > v { i - 1 } else { i } };
    let ceil = |v: f32| { let i = v as i32; if (i as f32) < v { i + 1 } else { i } };
    let (scale, rx, ry) = arrow_frame(p, zn, zd, n);
    let (px, py) = (anchor.0 as f32 + rx, anchor.1 as f32 + ry);
    let (x0, y0, x1, y1) = if p.kind == PRIMITIVE_ARROW {
        (px - ARROW_TIP.0 * scale, py - ARROW_TIP.1 * scale, px + (ARROW_ART.0 - ARROW_TIP.0) * scale, py + (ARROW_ART.1 - ARROW_TIP.1) * scale)
    } else {
        let (hx, hy) = shadow_half(marker_bob(p));
        (px - hx * scale, py - hy * scale, px + hx * scale, py + hy * scale)
    };
    let (l, t) = (floor(x0), floor(y0));
    ([l, t, ceil(x1), ceil(y1)], scale, (px - l as f32, py - t as f32))
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

/// Categories for per-camera clean-map filtering. A category is a single bit.
pub const CATEGORY_RIVERS_FLOODPLAINS: u32 = 1 << 0;
pub const CATEGORY_MOUNTAINS: u32 = 1 << 1;
pub const CATEGORY_HILLS: u32 = 1 << 2;
pub const CATEGORY_FORESTS_JUNGLES: u32 = 1 << 3;
pub const CATEGORY_TERRAIN_BUILDINGS: u32 = 1 << 4;
pub const CATEGORY_ROADS_RAILROADS: u32 = 1 << 5;
pub const CATEGORY_IRRIGATION: u32 = 1 << 6;
pub const CATEGORY_RESOURCES: u32 = 1 << 7;
pub const CATEGORY_YIELD_ICONS: u32 = 1 << 8;
pub const CATEGORY_POLLUTION: u32 = 1 << 9;
pub const CATEGORY_TERRITORY_BOUNDARIES: u32 = 1 << 10;
pub const CATEGORY_CITIES: u32 = 1 << 11;
pub const CATEGORY_UNITS: u32 = 1 << 12;
pub const CATEGORY_ALL: u32 = (1 << 13) - 1;

/// Reserved layout categories, outside the user-selectable category mask.
/// Paired world annotations may use different layouts for full-size and
/// reduced-size worlds while retaining their ordinary visibility category.
pub const CATEGORY_FULL_RESOLUTION: u32 = 1 << 30;
pub const CATEGORY_REDUCED_RESOLUTION: u32 = 1 << 31;
/// Likewise for a layout made for the half-size world only (downscale 2,
/// the game's zoomed-out view) and one made for any other scale: e.g. a
/// world shown much smaller that is also explored at half size.
pub const CATEGORY_HALF_RESOLUTION: u32 = 1 << 29;
pub const CATEGORY_NOT_HALF_RESOLUTION: u32 = 1 << 28;

/// A camera hides the annotation layout intended for the other world scale.
pub fn categories_at_downscale(hidden: u32, downscale: u32) -> u32 {
    hidden
        | if downscale > 1 { CATEGORY_FULL_RESOLUTION } else { CATEGORY_REDUCED_RESOLUTION }
        | if downscale == 2 { CATEGORY_NOT_HALF_RESOLUTION } else { CATEGORY_HALF_RESOLUTION }
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
    /// How many columns at its left, rows at its top, columns at its right
    /// and rows at its bottom draw nothing (texels of alpha 0), at most 127
    /// each, in the low 7 bits of a byte each from the low one
    /// ([`blank_margins`]): drawn rectangles leave them out
    /// ([`drawn_part`]), so that no fragment is shaded only to be
    /// discarded. 0: none known. The top bits of the low bytes say more of
    /// its texels: [`BLANK_BINARY`], [`BLANK_OPAQUE`].
    pub blank: u32,
    /// The corners it leaves blank, as cuts along isometric diagonals (a
    /// pixel down for two across), a byte each from the low one: top left
    /// `c0` (texel centres with `u + 2v < c0` are blank), top right `c1`
    /// (`(w − u) + 2v`), bottom right `c2` (`(w − u) + 2(h − v)`), bottom
    /// left `c3` (`u + 2(h − v)`), each at most `min(w / 2, h)` so that
    /// they do not cross ([`corner_cuts`]). A diamond on a 2:1 grid leaves
    /// half its rectangle out. 0: none.
    pub cuts: u32,
    /// Explicit sampling role. Zero is exact nearest sampling. Only
    /// [`SAMPLING_UI_BACKGROUND`] screen sprites may vary their colour
    /// sample when a view is drawn at a different size than its source art.
    pub sampling: u32,
    /// The sprite's top-left in its original source picture. The spatial
    /// dither follows that picture across atlas repacking, clipping and
    /// copied/cropped regions, rather than a window or animation clock.
    pub sampling_origin_x: u32,
    pub sampling_origin_y: u32,
}

/// A pure interface background texture, with controls and text drawn
/// separately. Never use for art with baked labels, icons or borders.
pub const SAMPLING_UI_BACKGROUND: u32 = 1;

/// Pick one source texel inside a downsampled background pixel's footprint.
/// `nearest` and `bounds` are relative to the sprite; `origin` anchors the
/// pattern in the original image. This changes only spatial sampling, with
/// no clock or viewport coordinate, and never reads outside the source crop.
pub fn background_texel(nearest: (u32, u32), footprint: (u32, u32), bounds: (u32, u32), origin: (u32, u32)) -> (u32, u32) {
    let width = bounds.0.max(1);
    let height = bounds.1.max(1);
    let x = nearest.0.min(width - 1);
    let y = nearest.1.min(height - 1);
    let span_x = footprint.0.max(1).min(width - x);
    let span_y = footprint.1.max(1).min(height - y);
    let seed = x.wrapping_add(origin.0).wrapping_mul(0x9e37_79b9)
        ^ y.wrapping_add(origin.1).wrapping_mul(0x85eb_ca6b);
    (x + background_hash(seed) % span_x, y + background_hash(seed ^ 0xc2b2_ae35) % span_y)
}

/// A magnified pixel's two source neighbours and right-hand weight in
/// 64ths, sampled at its centre. Clamp to the actual source crop, never
/// neighbouring atlas art. Clipping changes coverage, not this phase.
pub fn background_linear_axis(pixel: i32, base: i32, bound: u32, n: u32, d: u32) -> (u32, u32, u32) {
    let (n, d) = (n.max(1) as i32, d.max(1) as i32);
    let denominator = 2 * n;
    let position = (pixel * d - base * n) * 2 + d - n;
    let first = if position >= 0 { position / denominator } else { -((-position + denominator - 1) / denominator) };
    let fraction = (position - first * denominator) as u32;
    let last = bound.max(1) as i32 - 1;
    let left = first.max(0).min(last) as u32;
    let right = (first + 1).max(0).min(last) as u32;
    (left, right, (fraction * 64 + denominator as u32 / 2) / denominator as u32)
}

/// Choose one of the four bilinear neighbours with their interpolation
/// weights. The fixed pattern follows source coordinates and subpixel
/// phases, so a stationary magnified background cannot flicker.
pub fn background_linear_texel(x: (u32, u32, u32), y: (u32, u32, u32), origin: (u32, u32)) -> (u32, u32) {
    let seed = x.0.wrapping_add(origin.0).wrapping_mul(0x9e37_79b9)
        ^ y.0.wrapping_add(origin.1).wrapping_mul(0x85eb_ca6b)
        ^ x.2.wrapping_mul(0x27d4_eb2d) ^ y.2.wrapping_mul(0x1656_67b1);
    (if background_hash(seed) & 63 < x.2 { x.1 } else { x.0 },
        if background_hash(seed ^ 0xc2b2_ae35) & 63 < y.2 { y.1 } else { y.0 })
}

/// [`FrameParams::background_sampling`]: how a scaled
/// [`SAMPLING_UI_BACKGROUND`] sprite picks its colour. Dither selects one
/// source texel, from the footprint when shrinking or weighted bilinear
/// neighbours when magnifying. Nominal alpha stays unchanged.
pub const BACKGROUND_DITHER: u32 = 0;
/// Smooth colour: an area-weighted box filter when shrinking, bilinear
/// interpolation when magnifying. Transparent neighbours are omitted.
pub const BACKGROUND_INTERPOLATE: u32 = 1;
/// The ordinary nearest texel, like any other sprite.
pub const BACKGROUND_NEAREST: u32 = 2;
/// The most texels per axis a box-filtered pixel averages (a footprint
/// wider than this, a downscale beyond 16×, averages its first 16).
pub const BACKGROUND_MAX_TAPS: i32 = 16;

/// The source footprint of target pixel `pixel` (from the view's scaled
/// origin) along one axis of a sprite whose zoom-1 rectangle starts at
/// `base`, drawn at zoom `n / d`: `(first, end, a, b)`, texels
/// `first..end` of the sprite (from `lo`, its first drawn one, below
/// `bound`, its size), the pixel covering `[a, b)` in `1/n` texels.
pub fn background_footprint(pixel: i32, base: i32, lo: i32, bound: u32, n: u32, d: u32) -> (i32, i32, i32, i32) {
    let (n, d) = (n.max(1) as i32, d.max(1) as i32);
    let a = pixel * d - base * n;
    let b = a + d;
    let floor = if a >= 0 { a / n } else { -((-a + n - 1) / n) };
    let ceil = if b >= 0 { (b + n - 1) / n } else { -(-b / n) };
    let first = if floor > lo { floor } else { lo };
    let mut end = ceil;
    if end > bound as i32 { end = bound as i32; }
    if end > first + BACKGROUND_MAX_TAPS { end = first + BACKGROUND_MAX_TAPS; }
    (first, end, a, b)
}

/// The weight of texel `j` in a footprint `[a, b)` (in `1/n` texels, see
/// [`background_footprint`]): its overlap, exact up to `n = 64`, else
/// rescaled to 64ths, so that weighted sums of 8-bit colours stay well
/// inside 32 bits.
pub fn background_weight(j: i32, a: i32, b: i32, n: u32) -> u32 {
    let n = n.max(1) as i32;
    let lo = if a > j * n { a } else { j * n };
    let hi = if b < (j + 1) * n { b } else { (j + 1) * n };
    if hi <= lo {
        return 0;
    }
    let w = (hi - lo) as u32;
    if n <= 64 { w } else { (w * 64 + n as u32 / 2) / n as u32 }
}

/// One channel of a box-filtered colour: `sum / weight`, rounded.
pub fn background_mean(sum: u32, weight: u32) -> u32 {
    if weight == 0 { 0 } else { (sum + weight / 2) / weight }
}

fn background_hash(mut value: u32) -> u32 {
    value = (value ^ (value >> 16)).wrapping_mul(0x7feb_352d);
    value = (value ^ (value >> 15)).wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

/// [`SpriteDesc::cuts`] of the four corner cuts (top left, top right,
/// bottom right, bottom left) of a `w × h` sprite: each kept to
/// `min(w / 2, h, 255)`.
pub fn corner_cuts(c: [u32; 4], w: u32, h: u32) -> u32 {
    let most = (w / 2).min(h).min(255);
    c[0].min(most) | c[1].min(most) << 8 | c[2].min(most) << 16 | c[3].min(most) << 24
}

/// [`SpriteDesc::blank`]: every texel has alpha 0 or 255 (drawn over
/// itself, the sprite shows what it showed once).
pub const BLANK_BINARY: u32 = 1 << 7;
/// [`SpriteDesc::blank`]: every texel has alpha 255.
pub const BLANK_OPAQUE: u32 = 1 << 15;
/// Every sampled texel is proven transparent, independent of saturated margins.
pub const BLANK_EMPTY: u32 = 1 << 23;

/// [`SpriteDesc::blank`] of blank margins (left, top, right, bottom),
/// each cut to 127.
pub fn blank_margins(left: u32, top: u32, right: u32, bottom: u32) -> u32 {
    left.min(127) | top.min(127) << 8 | right.min(127) << 16 | bottom.min(127) << 24
}

/// The part of the rectangle `p` draws `sprite` in (pixels `0..p.width` ×
/// `0..p.height` from its corner) that shows texels of the sprite outside
/// its blank margins ([`SpriteDesc::blank`]): left, top, right, bottom,
/// relative to the rectangle (empty if right ≤ left or bottom ≤ top).
/// Pixel `i` of the rectangle shows texel `p.texel + p.step·i`.
pub fn drawn_part(sprite: &SpriteDesc, p: &Placed) -> (u32, u32, u32, u32) {
    let m = sprite.blank;
    let (x0, x1) = drawn_span(m & 0x7F, (m >> 16) & 0x7F, sprite.width, p.texel_x, p.step, p.width);
    let (y0, y1) = drawn_span((m >> 8) & 0x7F, (m >> 24) & 0x7F, sprite.height, p.texel_y, p.step, p.height);
    (x0, y0, x1, y1)
}

/// Of `count` pixels showing texels `texel + step·i` of a row (column) of
/// `size` texels with `lo` and `hi` blank at its ends, the first and past
/// the last that show one that is not.
fn drawn_span(lo: u32, hi: u32, size: u32, texel: u32, step: u32, count: u32) -> (u32, u32) {
    let step = if step == 0 { 1 } else { step };
    let end = if size > hi { size - hi } else { 0 };
    let first = if lo > texel { (lo - texel + step - 1) / step } else { 0 };
    let past = if end > texel { (end - texel + step - 1) / step } else { 0 };
    (if first < count { first } else { count }, if past < count { past } else { count })
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
///
/// That is the rule of a game's own half-size view. Smaller still, a
/// shrunk sprite (not one keeping its anchor) samples the texels of its box
/// a multiple of `n` from the anchor point instead, each drawn at its
/// distance divided by `n`: the samples of neighbouring cells line up (from
/// each box's own corner, tiles cropped to different boxes leave pixels
/// between them that none of them covers).
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
    if n > 2 && !keep {
        let (tx, ty) = (on_grid(sprite.anchor_x, sprite.box_x, n), on_grid(sprite.anchor_y, sprite.box_y, n));
        return Placed {
            corner_x: (sprite.anchor_x + tx as i32) / n as i32,
            corner_y: (sprite.anchor_y + ty as i32) / n as i32,
            inset_x: 0,
            inset_y: 0,
            width: samples(tx, sprite.box_x + sprite.box_w, n),
            height: samples(ty, sprite.box_y + sprite.box_h, n),
            texel_x: tx,
            texel_y: ty,
            step: n,
        };
    }
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

/// The first texel from `from` on a multiple of `n` from the anchor point
/// (the sprite's top-left `anchor` from it).
fn on_grid(anchor: i32, from: u32, n: u32) -> u32 {
    let n = n as i32;
    let r = ((anchor + from as i32) % n + n) % n;
    from + ((n - r) % n) as u32
}

/// How many texels every `n`-th from `first` on lie before `end`.
fn samples(first: u32, end: u32, n: u32) -> u32 {
    if first >= end { 0 } else { (end - first + n - 1) / n }
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
/// Integer-valued FP16 storage on the measured DX12 software adapter.
pub const FLOAT_QUANTIZED_BIAS: u32 = 1536;

/// A free sprite's `scale_den` that inverts what is under its opaque
/// texels (`Blend::Invert`).
pub const SCALE_INVERT: u32 = u32::MAX;
/// A free sprite's `scale_den` for ClearType-like text: its texels hold
/// coverage per colour channel (red, green, blue; alpha: any), it draws
/// `FreeSprite::tint` with that coverage per channel (`Blend::Subpixel`).
pub const SCALE_SUBPIXEL: u32 = u32::MAX - 1;
/// Dedicated native RGB555 palette-alpha composition pipeline.
pub const SCALE_PALETTE_ALPHA256: u32 = u32::MAX - 2;
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
    div255(a8 * ALPHA_STEPS + 127)
}

/// `x / 255` for `x` below 65535, without a division (GPUs have none for
/// integers; it is a long sequence of instructions).
pub fn div255(x: u32) -> u32 {
    (x + 1 + (x >> 8)) >> 8
}

/// What a translucent texel with `k` alpha steps adds to its premultiplied
/// colour: `S·k/n + D·(1 − k/n)` has a fractional part that is a multiple
/// of `1/n` (n = [`ALPHA_STEPS`]), so this offset makes rounding to the
/// nearest integer give the floor (see [`mix_stored`]).
pub fn mix_offset() -> f32 {
    -0.5 + 0.5 / ALPHA_STEPS as f32
}

/// The premultiplied source of a quantized alpha blend (`k` in
/// `1..=ALPHA_STEPS`); an opaque draw emits `stored / 255` directly.
/// Translucent RGB444 and RGB555 sources use a binary fraction that
/// survives packed FP16 fragment exports exactly. Its value is strictly
/// between `(stored*k/32 - 1/2)/255` and that plus `1/(32*255)`, so rounding
/// the resulting blend still gives [`mix_stored`] for every destination.
/// Larger colour spaces retain the floating-point source calculation.
pub fn mixed_source(stored: u32, k: u32, bits: u32) -> f32 {
    if k >= ALPHA_STEPS {
        return stored as f32 / 255.0;
    }
    let source = stored as f32 / 255.0 * (k as f32 / ALPHA_STEPS as f32) + mix_offset() / 255.0;
    if bits <= 5 {
        // Select the nearest representable FP16 before a driver's packed
        // export can truncate it. These positive values are all normal
        // FP16 numbers, so rounding the mantissa is sufficient.
        let word = source.to_bits();
        return f32::from_bits((word + 0xFFF + ((word >> 13) & 1)) & !0x1FFF);
    }
    source
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
    fn magnified_background_neighbours_match_centres_and_stay_inside_source_crops() {
        for (n, d) in [(2, 1), (3, 1), (3, 2), (5, 4), (7, 3), (129, 64)] {
            for base in [-11, 0, 17] { for bound in [1, 2, 31] { for pixel in -12..100 {
                let got = background_linear_axis(pixel, base, bound, n, d);
                let coordinate = (pixel as f64 + 0.5) * d as f64 / n as f64 - base as f64 - 0.5;
                let first = coordinate.floor();
                let expected = (first.max(0.0).min((bound - 1) as f64) as u32,
                    (first + 1.0).max(0.0).min((bound - 1) as f64) as u32,
                    ((coordinate - first) * 64.0).round() as u32);
                assert_eq!(got, expected, "pixel {pixel}, base {base}, bound {bound}, zoom {n}/{d}");
                assert!(got.0 < bound && got.1 < bound && got.2 <= 64);
                assert_eq!(got, background_linear_axis(pixel + n as i32 * 3, base + d as i32 * 3, bound, n, d), "moving the destination must not reset source phase");
            } } }
        }
    }

    #[test]
    fn magnified_dither_uses_weighted_neighbours_without_clock_or_out_of_crop_samples() {
        let mut selected = [0u32; 4];
        for y in 0..256 { for x in 0..256 {
            let horizontal = (x, x + 1, 16);
            let vertical = (y, y + 1, 48);
            let sample = background_linear_texel(horizontal, vertical, (17, 29));
            assert!(sample.0 == x || sample.0 == x+1);
            assert!(sample.1 == y || sample.1 == y+1);
            selected[usize::from(sample.0 != x) + 2 * usize::from(sample.1 != y)] += 1;
            assert_eq!(background_linear_texel((x, x+1, 0), (y, y+1, 64), (0,0)), (x,y+1));
        } }
        for (count, expected) in selected.into_iter().zip([12_288, 4_096, 36_864, 12_288]) {
            assert!(count.abs_diff(expected) < 700, "weighted dither distribution {count}, expected about {expected}");
        }
    }

    #[test]
    fn div255_divides() {
        for x in 0..65535 {
            assert_eq!(div255(x), x / 255, "{x}");
        }
    }

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
        // A quarter size and smaller: on the cells' grid. Texel 3 is 16 from
        // the anchor point (-19 + 3 = -16), drawn at -4; to texel 123, 31
        // of them; rows from 3 (-15 + 3 = -12, at -3), 16 of them.
        let p = placed(&s, 4);
        assert_eq!((p.corner_x, p.corner_y, p.inset_x, p.inset_y, p.width, p.height, p.texel_x, p.texel_y, p.step), (-4, -3, 0, 0, 31, 16, 3, 3, 4));
        // Two tiles a cell apart cropped to different boxes: their samples
        // lie on one grid, the world pixels a multiple of 8 apart.
        let other = SpriteDesc { box_x: 6, box_y: 0, box_w: 120, box_h: 64, ..s };
        for t in [s, other] {
            let p = placed(&t, 8);
            assert_eq!((t.anchor_x + p.texel_x as i32).rem_euclid(8), 0);
            assert_eq!(p.corner_x * 8, t.anchor_x + p.texel_x as i32);
        }
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
    fn packed_sources_preserve_every_rgb444_and_rgb555_blend() {
        for bits in [4, 5] {
            for k in 1..ALPHA_STEPS {
                let a = k as f32 / ALPHA_STEPS as f32;
                for qs in 0..1 << bits {
                    let src = mixed_source(qs + QUANTIZED_BIAS, k, bits);
                    assert_eq!(src.to_bits() & 0x1FFF, 0, "the source must survive an FP16 export exactly");
                    for qd in 0..1 << bits {
                        let dst = (qd + QUANTIZED_BIAS) as f32 / 255.0;
                        let got = ((src + dst * (1.0 - a)) * 255.0).round() as u32;
                        assert_eq!(got, mix_stored(qs + QUANTIZED_BIAS, qd + QUANTIZED_BIAS, k), "bits {bits}, src {qs}, dst {qd}, k {k}");
                    }
                }
            }
        }
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
    fn a_host_clock_layer_runs_on_viewer_milliseconds_whatever_the_driver_tick() {
        // 30 frames of 83 ms: the driver's tick frozen, the viewer's running.
        for game_tick in [0, 7, u32::MAX] {
            assert_eq!(layer_frame_of(game_tick, 0, HOST_CLOCK | 83, 0, 30), 0);
            assert_eq!(layer_frame_of(game_tick, 82, HOST_CLOCK | 83, 0, 30), 0);
            assert_eq!(layer_frame_of(game_tick, 83, HOST_CLOCK | 83, 0, 30), 1);
            assert_eq!(layer_frame_of(game_tick, 29 * 83, HOST_CLOCK | 83, 0, 30), 29);
            assert_eq!(layer_frame_of(game_tick, 30 * 83, HOST_CLOCK | 83, 0, 30), 0);
            assert_eq!(layer_frame_of(game_tick, 83, HOST_CLOCK | 83, 5, 30), 6, "from the cell's phase on");
        }
        // Without the flag: as before (the phase alone with 0 ticks).
        assert_eq!(layer_frame_of(10, 999, 3, 1, 4), frame_of(10, 3, 1, 4));
        assert_eq!(layer_frame_of(10, 999, 0, 5, 4), 1);
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
