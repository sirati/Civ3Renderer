//! CPU-side description of a tile world: grid, layers and their cell values.

use tile_render_shared::{GridParams, LAYER_HIDDEN, LAYOUT_ORTHOGONAL, LAYOUT_STAGGERED, LayerDesc, OFFSET_BYTES, WRAP_X, WRAP_Y};

use crate::atlas::SpriteId;

/// How cells are positioned (see `tile_render_shared::LAYOUT_*`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Layout {
    Orthogonal,
    /// Odd rows shifted half a cell: the isometric diamond map.
    Staggered,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    pub cols: u32,
    pub rows: u32,
    pub layout: Layout,
    pub wrap_x: bool,
    pub wrap_y: bool,
    /// Distance between neighbouring cell origins in pixels.
    pub step: (u32, u32),
}

impl Grid {
    pub fn cells(&self) -> u32 {
        self.cols * self.rows
    }

    pub(crate) fn params(&self) -> GridParams {
        GridParams {
            cols: self.cols,
            rows: self.rows,
            layout: match self.layout {
                Layout::Orthogonal => LAYOUT_ORTHOGONAL,
                Layout::Staggered => LAYOUT_STAGGERED,
            },
            wrap: if self.wrap_x { WRAP_X } else { 0 } | if self.wrap_y { WRAP_Y } else { 0 },
            step_x: self.step.0,
            step_y: self.step.1,
            _pad0: 0,
            _pad1: 0,
        }
    }

    /// World-pixel origin of cell (`col`, `row`).
    pub fn origin(&self, col: i32, row: i32) -> (i32, i32) {
        tile_render_shared::cell_origin(&self.params(), col, row)
    }
}

/// An animation: the frames a cell cycles through. One frame = static.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SpriteSetId(pub(crate) u32);

impl SpriteSetId {
    /// The id as a plain number (for passing through game-agnostic data
    /// such as a scene); `from_raw` turns it back.
    pub fn raw(self) -> u32 {
        self.0
    }

    /// An id from `raw`. An id that does not belong to the world's sets
    /// draws nothing.
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

impl SpriteSetId {
    /// The set that draws nothing.
    pub const EMPTY: SpriteSetId = SpriteSetId(0);
}

/// All sprite sets, flattened as `[n, sprite_0, .., sprite_n-1]` records.
#[derive(Clone, Debug)]
pub struct SpriteSets {
    words: Vec<u32>,
}

impl Default for SpriteSets {
    fn default() -> Self {
        // Offset 0 is the empty set.
        Self { words: vec![0] }
    }
}

impl SpriteSets {
    /// Whether every existing numeric set record is unchanged in `other`,
    /// including records whose former names now resolve to newer sets.
    pub fn is_prefix_of(&self, other: &Self) -> bool {
        other.words.starts_with(&self.words)
    }

    /// Frame `i` of `set`, if it has one.
    pub fn frame(&self, set: SpriteSetId, i: u32) -> Option<SpriteId> {
        let at = set.0 as usize;
        let n = *self.words.get(at).filter(|_| set.0 != 0)?;
        (i < n).then(|| self.words.get(at + 1 + i as usize).map(|&s| SpriteId(s))).flatten()
    }

    pub fn add(&mut self, frames: &[SpriteId]) -> SpriteSetId {
        if frames.is_empty() {
            return SpriteSetId::EMPTY;
        }
        let id = SpriteSetId(self.words.len() as u32);
        self.words.push(frames.len() as u32);
        self.words.extend(frames.iter().map(|s| s.0));
        id
    }

    pub fn single(&mut self, sprite: SpriteId) -> SpriteSetId {
        self.add(&[sprite])
    }

    pub(crate) fn words(&self) -> &[u32] {
        &self.words
    }
}

/// How a layer's sprites combine with what is already drawn.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Blend {
    /// Standard "over" alpha blending.
    #[default]
    Alpha,
    /// Alpha blending with uniform sprite opacity; atlas pixels are shared.
    AlphaOpacity { opacity: u8 },
    /// Exact native RGB555 destination composition with raw alpha / 256.
    /// Free sprites only. A cell layer draws it as ordinary alpha.
    NativePaletteAlpha,
    /// Darkens what is underneath to `k / denominator` (`k` = the sprite's
    /// red channel; `denominator` at most 32) — how games darken with blend
    /// tables (fog of war, shadows). Exact (rounded down per channel after
    /// every sprite) in a quantized [`ColorModel`]; plain alpha blending
    /// with black otherwise.
    Scale { denominator: u32 },
    /// Inverts what is underneath its opaque texels (their colour does not
    /// matter): `c' = max − c` per channel, exact in a quantized model.
    /// Free sprites only (a layer draws it as `Alpha`).
    Invert,
    /// ClearType-like text: the texels hold coverage per colour channel
    /// (red, green, blue), `color` is drawn with it, each channel blending
    /// with its own alpha (in whole steps, rounded down, in a quantized
    /// model). Free sprites only (a layer draws it as `Alpha`).
    Subpixel { color: [u8; 3] },
}

impl Blend {
    /// As free sprites and scenes carry it: 0 alpha, the denominator of a
    /// scale, [`tile_render_shared::SCALE_INVERT`].
    pub fn scale_den(self) -> u32 {
        match self {
            Blend::Alpha | Blend::AlphaOpacity { .. } => 0,
            Blend::Scale { denominator } => denominator.max(1),
            Blend::NativePaletteAlpha => tile_render_shared::SCALE_PALETTE_ALPHA256,
            Blend::Invert => tile_render_shared::SCALE_INVERT,
            Blend::Subpixel { .. } => tile_render_shared::SCALE_SUBPIXEL,
        }
    }

    /// Subpixel colour (low 24 bits), or inverse alpha opacity (high byte).
    pub fn tint(self) -> u32 {
        match self {
            Blend::AlphaOpacity { opacity } => (255 - u32::from(opacity)) << 24,
            Blend::Subpixel { color: [r, g, b] } => (r as u32) << 16 | (g as u32) << 8 | b as u32,
            _ => 0,
        }
    }

    /// From [`Blend::scale_den`] and [`Blend::tint`].
    pub fn from_scale_den(d: u32, tint: u32) -> Blend {
        match d {
            0 if tint >> 24 != 0 => Blend::AlphaOpacity { opacity: (255 - (tint >> 24)) as u8 },
            0 => Blend::Alpha,
            tile_render_shared::SCALE_PALETTE_ALPHA256 => Blend::NativePaletteAlpha,
            tile_render_shared::SCALE_INVERT => Blend::Invert,
            tile_render_shared::SCALE_SUBPIXEL => Blend::Subpixel { color: [(tint >> 16) as u8, (tint >> 8) as u8, tint as u8] },
            d => Blend::Scale { denominator: d },
        }
    }
}

/// The colour space the world is composed in.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ColorModel {
    /// 8-bit channels as they are.
    #[default]
    Direct,
    /// Channels of `bits` bits (4..=7), like a game drawing into an RGB555
    /// frame buffer: sprite colours keep their top bits, blending happens on
    /// those values and is rounded down after every pass, and the result is
    /// expanded to 8 bits by bit replication.
    Quantized { bits: u32 },
}

impl ColorModel {
    pub(crate) fn bits(self) -> u32 {
        match self {
            ColorModel::Direct => 0,
            ColorModel::Quantized { bits } => bits,
        }
    }
}

/// Which zoom a sprite's art is made for. A driver may give sprites made for
/// a zoom (`Zoomed`, e.g. text rasterised at that size) next to their 1:1
/// twins (`Low`): a view at that zoom showing zoomed art
/// (`Camera::zoomed_art`) draws the zoomed ones after scaling, at full
/// resolution, other views the low ones.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ZoomArt {
    #[default]
    Any,
    Low,
    /// For views at `zoom` (a reduced fraction): drawn at the sprite's 1:1
    /// position scaled, plus `offset` (pixels at that zoom).
    Zoomed { zoom: (u32, u32), offset: (i32, i32) },
}

/// A sprite (animation) placed freely, see [`TileWorld::free`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PlacedSprite {
    pub set: SpriteSetId,
    /// Anchor position: world pixels, or viewport pixels if `screen`.
    pub x: i32,
    pub y: i32,
    pub screen: bool,
    /// Drawn after this render pass (index into [`TileWorld::passes`];
    /// past the last pass: on top of everything).
    pub after_pass: usize,
    pub phase: u32,
    pub ticks_per_frame: u32,
    pub blend: Blend,
    /// Drawn only inside this viewport rectangle (left, top, right,
    /// bottom; exclusive).
    pub clip: Option<[i32; 4]>,
    /// Drawn only in this view (`None`: in every view; see
    /// `TileRenderer::render_views`).
    pub view: Option<u32>,
    /// With a `view`: drawn in every view showing the world but that one.
    pub except_view: bool,
    /// Art for a zoom (screen sprites of a view; see [`ZoomArt`]).
    pub zoom_art: ZoomArt,
    /// The named palette its indexed sprites' remapped indices are drawn
    /// with (see `tile_render_shared::SpriteDesc::remap`; 0: none).
    pub palette: u32,
    /// Visibility category bit for per-camera clean-map filtering. Zero is
    /// always visible.
    pub category: u32,
    /// A glyph drawn from its font's outline instead of the sprite (`set`
    /// and `blend` are ignored). `zoom_art` keeps its twin selection. At
    /// 1:1, `Any` and `Low` draw even when `Camera::zoomed_art` is true.
    /// A scaled view's zoomed-art stage draws `Any` and `Zoomed`. Outlines
    /// use the view's current scale; `Zoomed`'s specific `zoom` and `offset`
    /// are ignored.
    pub text: Option<OutlineGlyph>,
    /// Procedural rectangle, independent of atlas and sprite set.
    pub solid: Option<SolidRect>,
    /// A shape laid out at the scale of the stage drawing it (instead of
    /// the sprite, a solid or text): crisp in a view drawn at its zoom.
    pub shape: Option<Shape>,
}

/// What a [`PlacedSprite::shape`] draws, from the sprite's position (its
/// world pixel, or its screen pixel, scaled to the stage).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    Rect(OutputRect),
    Painted(PaintedShape),
}

/// A shape painted per pixel at the scale of the stage drawing it, by the
/// renderer's painter (`Shaders::painter`) and its CPU oracle's
/// (`reference::Painter`) (`tile_render_shared::PRIMITIVE_PAINTED`).
#[derive(Copy, Clone, Debug)]
pub struct PaintedShape {
    /// Which shape the painter paints.
    pub shape: u32,
    /// Its size, in sixteenths of its 1:1 art.
    pub size: u32,
    /// Its reference point, in full-size world pixels from the position.
    pub at: (f32, f32),
    /// The rectangle it may cover (left, top, right, bottom), in pixels of
    /// its 1:1 art from its reference point.
    pub extent: [f32; 4],
    /// A word for the painter, and the colour it paints with.
    pub param: f32,
    pub rgba: [u8; 4],
}

impl PaintedShape {
    fn bits(&self) -> (u32, u32, [u32; 2], [u32; 4], u32, [u8; 4]) {
        (self.shape, self.size, [self.at.0.to_bits(), self.at.1.to_bits()], self.extent.map(f32::to_bits), self.param.to_bits(), self.rgba)
    }
}

/// The same bits: the same shape.
impl PartialEq for PaintedShape {
    fn eq(&self, other: &Self) -> bool {
        self.bits() == other.bits()
    }
}

impl Eq for PaintedShape {}

/// A solid rectangle laid out in the pixels of the stage drawing it
/// (`tile_render_shared::output_rect`): its edges whole multiples of two
/// lengths per axis (`lengths`: x1, x2, y1, y2, at 1:1), each scaled by
/// `scale` (sixteenths, per axis) and the stage's zoom and rounded once.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct OutputRect {
    pub rgba: [u8; 4],
    pub lengths: [u8; 4],
    /// Left and right edges, as coefficients of x1 and x2.
    pub x: [[i8; 2]; 2],
    /// Top and bottom edges, as coefficients of y1 and y2.
    pub y: [[i8; 2]; 2],
    pub scale: (u16, u16),
}

impl Shape {
    pub(crate) fn primitive(&self) -> tile_render_shared::FreePrimitive {
        match *self {
            Shape::Rect(r) => tile_render_shared::output_rect_primitive(r.rgba, r.lengths, r.x, r.y, r.scale),
            Shape::Painted(p) => {
                let [e, f, g, h] = p.extent.map(f32::to_bits);
                tile_render_shared::FreePrimitive { kind: tile_render_shared::PRIMITIVE_PAINTED, width: p.shape, rgba: u32::from_le_bytes(p.rgba),
                    a: p.size, b: p.param.to_bits(), c: p.at.0.to_bits(), d: p.at.1.to_bits(), e, f, g, h, ..Default::default() }
            }
        }
    }
}

/// A solid rectangle with unpremultiplied RGBA, retaining its dimensions
/// under world downscale like UI sprite art; camera zoom still applies.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct SolidRect {
    pub width: u32,
    pub height: u32,
    pub rgba: [u8; 4],
}

/// One glyph of text drawn from outlines (see `tile_render_shared::text`),
/// its pen at the sprite's position (on the baseline).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct OutlineGlyph {
    /// Index into [`TileWorld::fonts`].
    pub font: u32,
    pub shape: GlyphShape,
    /// The em at 1:1, in 64ths of a pixel.
    pub em64: u32,
    /// The pen advance at 1:1 in pixels: the glyph is centred in it, scaled
    /// (text keeps its 1:1 layout at every zoom); a bar's width.
    pub advance: i32,
    /// Emboldened by a pixel, slanted by a third (a regular face made bold
    /// or italic, as Windows does).
    pub bold: bool,
    pub italic: bool,
    /// Drawn ClearType-like: each colour channel with its own coverage.
    pub color: [u8; 3],
    /// Drawn onto this colour key instead, in 15 bits, opaque where that
    /// differs from the key (see `tile_render_shared::text::TEXT_KEYED`).
    pub key: Option<[u8; 3]>,
}

/// What an [`OutlineGlyph`] draws.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GlyphShape {
    /// The glyph of this index in its face.
    Glyph(u32),
    /// A bar (an underline) `top` pixels below the baseline, `height`
    /// high, at 1:1.
    Bar { top: i32, height: i32 },
}

impl OutlineGlyph {
    /// The GPU instance of `sprite` drawing it; `bases`: where each font's
    /// words start in the outline buffer.
    pub(crate) fn gpu(&self, sprite: &PlacedSprite, fonts: &[std::sync::Arc<gpu_text::Face>], bases: &[u32]) -> Option<tile_render_shared::text::TextGlyph> {
        use tile_render_shared::text::{TEXT_BAR, TEXT_BOLD, TEXT_ITALIC, TEXT_KEYED};
        let rgb = |[r, g, b]: [u8; 3]| (r as u32) << 16 | (g as u32) << 8 | b as u32;
        let base = *bases.get(self.font as usize)?;
        let (glyph, bar) = match self.shape {
            GlyphShape::Glyph(g) => (base + fonts.get(self.font as usize)?.record(g), (0, 0)),
            GlyphShape::Bar { top, height } => (base, (top, height)),
        };
        let free = sprite.flags();
        Some(tile_render_shared::text::TextGlyph {
            glyph,
            face: base,
            flags: free
                | if self.bold { TEXT_BOLD } else { 0 }
                | if self.italic { TEXT_ITALIC } else { 0 }
                | if self.key.is_some() { TEXT_KEYED } else { 0 }
                | if matches!(self.shape, GlyphShape::Bar { .. }) { TEXT_BAR } else { 0 },
            view: sprite.view.unwrap_or(tile_render_shared::ALL_VIEWS),
            x: sprite.x,
            y: sprite.y,
            em: self.em64,
            advance: self.advance,
            clip: sprite.clip.map_or([0; 4], bound_clip),
            color: rgb(self.color),
            key: self.key.map_or(0, rgb),
            bar_top: bar.0,
            bar_height: bar.1,
            category: sprite.category,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        })
    }
}

/// How far from the screen origin a clip rectangle's sides are kept: the
/// shaders move them by a view's origin (and scale them), which must not
/// overflow. A clip meaning "everywhere" (`i32::MIN`, `i32::MAX`) moved by
/// a view off the window's corner wrapped round and hid the sprite.
pub const CLIP_LIMIT: i32 = 1 << 24;

/// A clip rectangle (x0, y0, x1, y1) with its sides within [`CLIP_LIMIT`].
pub fn bound_clip(clip: [i32; 4]) -> [i32; 4] {
    clip.map(|v| v.clamp(-CLIP_LIMIT, CLIP_LIMIT))
}

impl PlacedSprite {
    pub fn new(set: SpriteSetId, x: i32, y: i32) -> Self {
        Self { set, x, y, screen: false, after_pass: usize::MAX, phase: 0, ticks_per_frame: 1, blend: Blend::Alpha, clip: None, view: None, except_view: false, zoom_art: ZoomArt::Any, text: None, solid: None, shape: None, palette: 0, category: 0 }
    }

    /// Its view, clipping and Low/Zoomed twin selection flags.
    pub(crate) fn flags(&self) -> u32 {
        (if self.screen { tile_render_shared::FREE_SCREEN } else { 0 })
            | if self.clip.is_some() { tile_render_shared::FREE_CLIP } else { 0 }
            | if self.except_view && self.view.is_some() { tile_render_shared::FREE_EXCEPT_VIEW } else { 0 }
            | match self.zoom_art {
                ZoomArt::Any => 0,
                ZoomArt::Low => tile_render_shared::FREE_LOW,
                ZoomArt::Zoomed { .. } => tile_render_shared::FREE_ZOOMED,
            }
    }

    pub(crate) fn gpu(&self) -> tile_render_shared::FreeSprite {
        tile_render_shared::FreeSprite {
            set: self.set.0,
            phase: self.phase,
            ticks_per_frame: self.ticks_per_frame.max(1),
            flags: self.flags(),
            x: self.x,
            y: self.y,
            scale_den: if self.solid.is_some() || self.shape.is_some() { 0 } else { self.blend.scale_den() },
            view: self.view.unwrap_or(tile_render_shared::ALL_VIEWS),
            clip: self.clip.map_or([0; 4], bound_clip),
            tint: if self.solid.is_some() || self.shape.is_some() { 0 } else { self.blend.tint() },
            zoom: match self.zoom_art {
                ZoomArt::Zoomed { zoom: (n, d), .. } => n << 16 | d,
                _ => 0,
            },
            zoom_dx: match self.zoom_art {
                ZoomArt::Zoomed { offset, .. } => offset.0,
                _ => 0,
            },
            zoom_dy: match self.zoom_art {
                ZoomArt::Zoomed { offset, .. } => offset.1,
                _ => 0,
            },
            palette: self.palette,
            category: self.category,
            primitive: match (self.shape, self.solid) {
                (Some(shape), _) => shape.primitive(),
                (None, Some(solid)) => tile_render_shared::FreePrimitive {
                    kind: tile_render_shared::PRIMITIVE_SOLID, width: solid.width, height: solid.height,
                    rgba: u32::from_le_bytes(solid.rgba), ..Default::default()
                },
                (None, None) => Default::default(),
            },
            _pad0: 0,
            _pad1: 0,
        }
    }
}

/// A range of layers drawn together, see [`TileWorld::passes`].
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Pass {
    pub first: u32,
    pub count: u32,
    /// Each row is swept from its last column back (see
    /// [`LayerSpec::descending`]).
    pub descending: bool,
}

/// Static configuration of one layer.
#[derive(Clone, Debug)]
pub struct LayerSpec {
    pub name: String,
    /// Value → sprite set. Values outside the table draw nothing.
    pub lut: Vec<SpriteSetId>,
    /// Store a per-cell animation phase next to every value.
    pub per_cell_phase: bool,
    /// Store a per-cell pixel offset next to every value (and phase): the
    /// cell's sprite is drawn that far from its place (e.g. a unit on its
    /// way to the next cell). See [`TileWorld::set_offset`].
    pub per_cell_offset: bool,
    /// Pixel offset of the whole layer relative to cell origins.
    pub offset: (i32, i32),
    /// Animation ticks per frame; 0: a cell shows the frame its phase says,
    /// whatever the clock.
    pub ticks_per_frame: u32,
    pub hidden: bool,
    /// Render pass. A world's layers must be ordered by pass.
    pub pass: u32,
    /// The pass sweeps each row of cells from its last column to its first
    /// (all layers of a pass must agree), as a game drawing right to left
    /// does.
    pub descending: bool,
    /// Point tested against `Camera::cull`, relative to the cell origin.
    pub cull_offset: (i32, i32),
    pub blend: Blend,
    /// Bits of a value above those indexing `lut` that pick the named
    /// palette of its indexed sprites (0: the layer picks none;
    /// see [`LayerSpec::value_in`]).
    pub palette_bits: u32,
    /// Bits indexing `lut` in a value that picks a palette, at least (0:
    /// as many as the table needs): a table that grows keeps its values.
    pub index_bits: u32,
    /// Visibility category bit for per-camera clean-map filtering. Zero is
    /// always visible.
    pub category: u32,
}

impl LayerSpec {
    pub fn new(name: impl Into<String>, lut: Vec<SpriteSetId>) -> Self {
        Self {
            name: name.into(),
            lut,
            per_cell_phase: false,
            per_cell_offset: false,
            offset: (0, 0),
            ticks_per_frame: 1,
            hidden: false,
            pass: 0,
            descending: false,
            cull_offset: (0, 0),
            blend: Blend::Alpha,
            palette_bits: 0,
            index_bits: 0,
            category: 0,
        }
    }

    /// Smallest value width (bytes) that can index the whole table.
    pub fn value_bytes(&self) -> u32 {
        let bits = self.palette_shift() + self.palette_bits;
        if self.palette_bits == 0 {
            return match self.lut.len() {
                0..=0x100 => 1,
                0x101..=0x1_0000 => 2,
                0x1_0001..=0x100_0000 => 3,
                _ => 4,
            };
        }
        bits.div_ceil(8).clamp(1, 4)
    }

    /// Where the palette row starts in a value (the bits indexing `lut`;
    /// 0 without palettes).
    pub fn palette_shift(&self) -> u32 {
        if self.palette_bits == 0 { 0 } else { (usize::BITS - self.lut.len().saturating_sub(1).leading_zeros()).max(1).max(self.index_bits) }
    }

    /// The value drawing `lut` entry `index` with palette row `row` (rows
    /// past `palette_bits` are not drawn as asked: `None`).
    pub fn value_in(&self, index: u32, row: u32) -> Option<u32> {
        if row == 0 {
            return Some(index);
        }
        (self.palette_bits > 0 && row >> self.palette_bits == 0).then(|| index | row << self.palette_shift())
    }

    fn record_bytes(&self) -> u32 {
        self.value_bytes() + self.per_cell_phase as u32 + if self.per_cell_offset { OFFSET_BYTES } else { 0 }
    }
}

/// A tile world: grid + layers + one value (and phase) per layer and cell.
#[derive(Clone, Debug)]
pub struct TileWorld {
    pub grid: Grid,
    pub sets: SpriteSets,
    pub color: ColorModel,
    /// Sprites not on the grid, drawn in list order after the pass each
    /// names.
    pub free: Vec<PlacedSprite>,
    /// The font faces of text drawn from outlines ([`OutlineGlyph::font`]).
    pub fonts: Vec<std::sync::Arc<gpu_text::Face>>,
    layers: Vec<LayerSpec>,
    /// Each layer's records, packed, little endian.
    data: Vec<Vec<u8>>,
    /// Data slots ([`SlotId`]): records of a layer kept apart from its own,
    /// which the cameras of a binding draw instead ([`TileWorld::bind`]). A
    /// freed slot keeps its region for the next slot of that size.
    slots: Vec<Slot>,
    /// Per binding but the shared one (by key, ascending), the layers it
    /// binds to slots (by layer, ascending).
    bindings: Vec<(u32, Vec<(usize, SlotId)>)>,
    /// The records changed since [`TileWorld::take_dirty`] last took them.
    dirty: Dirty,
}

/// A data slot of a [`TileWorld`]: a region holding one layer's records
/// for every cell, apart from the layer's own (see [`TileWorld::add_slot`]).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SlotId(pub u32);

/// The binding every camera has unless it names another
/// (`Camera::binding`): every layer drawn from its own records.
pub const SHARED_BINDING: u32 = 0;

/// [`Dirty::Records`]' cell index for all of a region's records.
pub const ALL_CELLS: u32 = u32::MAX;

#[derive(Clone, Debug)]
struct Slot {
    layer: usize,
    live: bool,
    data: Vec<u8>,
}

/// Which cells' records changed (see [`TileWorld::take_dirty`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Dirty {
    #[default]
    None,
    /// These (data region, cell index) records, maybe more than once: a
    /// layer's own records are region `layer`, a slot's region
    /// [`TileWorld::slot_region`]; cell [`ALL_CELLS`]: all of the region's.
    /// Empty: only which slots the bindings draw changed.
    Records(Vec<(u32, u32)>),
    /// Too many to list: all of them.
    All,
}

/// More changed records than this are [`Dirty::All`].
const MOST_DIRTY: usize = 1 << 14;

/// Index of a layer in a `TileWorld`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LayerId(pub usize);

impl TileWorld {
    pub fn new(grid: Grid, sets: SpriteSets) -> Self {
        Self { grid, sets, color: ColorModel::Direct, free: Vec::new(), fonts: Vec::new(), layers: Vec::new(), data: Vec::new(), slots: Vec::new(), bindings: Vec::new(), dirty: Dirty::All }
    }

    pub fn add_layer(&mut self, spec: LayerSpec) -> LayerId {
        assert!(self.layers.len() < 255);
        assert!(self.slots.is_empty(), "layer {} added after data slots: add every layer first", spec.name);
        if let Some(prev) = self.layers.last() {
            assert!(spec.pass >= prev.pass, "layer {} (pass {}) after pass {}: layers must be ordered by pass", spec.name, spec.pass, prev.pass);
            assert!(
                spec.pass != prev.pass || spec.descending == prev.descending,
                "layer {} sweeps its pass otherwise than layer {}: the layers of a pass must agree",
                spec.name,
                prev.name
            );
        }
        let bytes = (spec.record_bytes() * self.grid.cells()) as usize;
        self.layers.push(spec);
        self.data.push(vec![0; bytes]);
        self.dirty = Dirty::All;
        LayerId(self.layers.len() - 1)
    }

    pub fn layers(&self) -> &[LayerSpec] {
        &self.layers
    }

    /// The viewer-clock periods (`ticks_per_frame` with
    /// [`tile_render_shared::HOST_CLOCK`]) of the layers that draw a cell
    /// now (in their own records or a live data slot): what makes a
    /// picture of this world change with the viewer's clock alone.
    pub fn host_clock_layers(&self) -> impl Iterator<Item = u32> + '_ {
        self.layers.iter().enumerate().filter(|(_, spec)| spec.ticks_per_frame & tile_render_shared::HOST_CLOCK != 0).filter_map(move |(i, spec)| {
            let (n, stride) = (spec.value_bytes() as usize, spec.record_bytes() as usize);
            let drawn = |bytes: &[u8]| bytes.chunks_exact(stride).any(|r| r[..n].iter().any(|&b| b != 0));
            let in_slot = || self.slots.iter().any(|s| s.live && s.layer == i && drawn(&s.data));
            (drawn(&self.data[i]) || in_slot()).then_some(spec.ticks_per_frame)
        })
    }

    /// (first layer, layer count) of every non-empty pass, in drawing order.
    pub fn passes(&self) -> Vec<Pass> {
        let mut out: Vec<Pass> = Vec::new();
        let mut last_pass = None;
        for (i, l) in self.layers.iter().enumerate() {
            if last_pass == Some(l.pass) {
                out.last_mut().unwrap().count += 1;
            } else {
                out.push(Pass { first: i as u32, count: 1, descending: l.descending });
                last_pass = Some(l.pass);
            }
        }
        out
    }

    pub fn layer_mut(&mut self, layer: LayerId) -> &mut LayerSpec {
        &mut self.layers[layer.0]
    }

    /// Sets the value of a cell (the index into the layer's table).
    pub fn set(&mut self, layer: LayerId, col: u32, row: u32, value: u32) {
        self.set_region(layer.0, col, row, value);
    }

    /// Replaces one layer's complete cell records. Missing values, phases
    /// and offsets are zero. Pack directly into the already allocated
    /// upload records and note one region change, rather than dispatching
    /// individual setters and dirty tracking for every field of every cell.
    pub fn set_layer_cells(&mut self, layer: LayerId, values: &[u32], phases: Option<&[u8]>, offsets: Option<&[(i16, i16)]>) {
        let spec = &self.layers[layer.0];
        let (n, stride) = (spec.value_bytes() as usize, spec.record_bytes() as usize);
        let (phase, offset) = (spec.per_cell_phase, spec.per_cell_offset);
        let bytes = &mut self.data[layer.0];
        bytes.fill(0);
        for (record, value) in bytes.chunks_exact_mut(stride).zip(values) {
            record[..n].copy_from_slice(&value.to_le_bytes()[..n]);
        }
        if let (true, Some(phases)) = (phase, phases) {
            for (record, &value) in bytes.chunks_exact_mut(stride).zip(phases) { record[n] = value; }
        }
        if let (true, Some(offsets)) = (offset, offsets) {
            let at = n + usize::from(phase);
            for (record, &(x, y)) in bytes.chunks_exact_mut(stride).zip(offsets) {
                record[at..at + 2].copy_from_slice(&x.to_le_bytes());
                record[at + 2..at + 4].copy_from_slice(&y.to_le_bytes());
            }
        }
        self.touch_cell(layer.0, ALL_CELLS);
    }

    /// [`TileWorld::set`] in data slot `slot` (a live one).
    pub fn set_in(&mut self, slot: SlotId, col: u32, row: u32, value: u32) {
        self.set_region(self.live_region(slot), col, row, value);
    }

    fn set_region(&mut self, region: usize, col: u32, row: u32, value: u32) {
        let n = self.region_spec(region).value_bytes() as usize;
        let at = self.region_record_at(region, col, row);
        self.region_bytes_mut(region)[at..at + n].copy_from_slice(&value.to_le_bytes()[..n]);
        self.touch(region, col, row);
    }

    /// Notes that the record of (`col`, `row`) in data region `region`
    /// changed.
    fn touch(&mut self, region: usize, col: u32, row: u32) {
        self.touch_cell(region, row * self.grid.cols + col);
    }

    fn touch_cell(&mut self, region: usize, cell: u32) {
        match &mut self.dirty {
            Dirty::All => {}
            Dirty::None => self.dirty = Dirty::Records(vec![(region as u32, cell)]),
            Dirty::Records(r) if r.len() >= MOST_DIRTY => self.dirty = Dirty::All,
            Dirty::Records(r) => r.push((region as u32, cell)),
        }
    }

    /// Notes that which slots bindings draw changed (no records did).
    fn touch_bindings(&mut self) {
        if self.dirty == Dirty::None {
            self.dirty = Dirty::Records(Vec::new());
        }
    }

    /// A new data slot for `layer`'s records, all zero (drawing what value
    /// 0 draws): a region of the cell data the cameras of a binding draw
    /// in place of the layer's own records once bound to it
    /// ([`TileWorld::bind`]). It reuses a freed slot's region of its size,
    /// so slots that come and go cost no new memory. Slots follow all
    /// layers: add every layer first.
    pub fn add_slot(&mut self, layer: LayerId) -> SlotId {
        let bytes = (self.layers[layer.0].record_bytes() * self.grid.cells()) as usize;
        let i = match self.slots.iter().position(|s| !s.live && s.data.len() == bytes) {
            Some(i) => {
                let s = &mut self.slots[i];
                (s.layer, s.live) = (layer.0, true);
                s.data.fill(0);
                i
            }
            None => {
                self.slots.push(Slot { layer: layer.0, live: true, data: vec![0; bytes] });
                self.slots.len() - 1
            }
        };
        self.touch_cell(self.layers.len() + i, ALL_CELLS);
        SlotId(i as u32)
    }

    /// Frees `slot`: no binding draws it any more (they draw the layer's
    /// own records again). Its region is kept for the next slot of its
    /// size, unless no slot after it is live (then it is given back).
    pub fn free_slot(&mut self, slot: SlotId) {
        let Some(s) = self.slots.get_mut(slot.0 as usize).filter(|s| s.live) else { return };
        s.live = false;
        for (_, b) in &mut self.bindings {
            b.retain(|&(_, s)| s != slot);
        }
        self.bindings.retain(|(_, b)| !b.is_empty());
        while self.slots.last().is_some_and(|s| !s.live) {
            self.slots.pop();
        }
        self.touch_bindings();
    }

    /// The live data slots and the layer each holds records of.
    pub fn slots(&self) -> impl Iterator<Item = (SlotId, LayerId)> + '_ {
        self.slots.iter().enumerate().filter(|(_, s)| s.live).map(|(i, s)| (SlotId(i as u32), LayerId(s.layer)))
    }

    /// The data region of `slot` in [`Dirty::Records`].
    pub fn slot_region(&self, slot: SlotId) -> u32 {
        (self.layers.len() + slot.0 as usize) as u32
    }

    /// Bytes the slots' regions take (freed ones kept for reuse included).
    pub fn slot_bytes(&self) -> usize {
        self.slots.iter().map(|s| s.data.len()).sum()
    }

    /// Makes the cameras of `binding` (`Camera::binding`) draw `layer`
    /// from `slot` (`None`: from the layer's own records again). Binding
    /// [`SHARED_BINDING`] always draws the layers' own records. Binding the
    /// same slot to several bindings shares it: nothing is copied.
    pub fn bind(&mut self, binding: u32, layer: LayerId, slot: Option<SlotId>) {
        if binding == SHARED_BINDING {
            debug_assert!(slot.is_none(), "the shared binding draws the layers' own records");
            return;
        }
        if let Some(s) = slot {
            let held = self.slots.get(s.0 as usize).filter(|d| d.live).map(|d| d.layer);
            assert_eq!(held, Some(layer.0), "slot {s:?} does not hold records of layer {}", self.layers[layer.0].name);
        }
        let i = match self.bindings.binary_search_by_key(&binding, |b| b.0) {
            Ok(i) => i,
            Err(_) if slot.is_none() => return,
            Err(i) => {
                self.bindings.insert(i, (binding, Vec::new()));
                i
            }
        };
        let b = &mut self.bindings[i].1;
        let before = b.clone();
        b.retain(|&(l, _)| l != layer.0);
        if let Some(s) = slot {
            b.push((layer.0, s));
            b.sort_unstable();
        }
        let changed = *b != before;
        if b.is_empty() {
            self.bindings.remove(i);
        }
        if changed {
            self.touch_bindings();
        }
    }

    /// The slot the cameras of `binding` draw `layer` from, if not its own
    /// records.
    pub fn bound(&self, binding: u32, layer: LayerId) -> Option<SlotId> {
        let i = self.bindings.binary_search_by_key(&binding, |b| b.0).ok()?;
        self.bindings[i].1.iter().find(|(l, _)| *l == layer.0).map(|&(_, s)| s)
    }

    /// The bindings that draw some layer from a slot.
    pub fn bindings(&self) -> impl Iterator<Item = u32> + '_ {
        self.bindings.iter().map(|b| b.0)
    }

    /// The layer whose records data region `region` holds, if any.
    fn region_layer(&self, region: usize) -> Option<usize> {
        match region.checked_sub(self.layers.len()) {
            None => Some(region),
            Some(s) => self.slots.get(s).map(|s| s.layer),
        }
    }

    fn region_spec(&self, region: usize) -> &LayerSpec {
        &self.layers[self.region_layer(region).expect("a data region of the world")]
    }

    fn region_bytes(&self, region: usize) -> &[u8] {
        match region.checked_sub(self.layers.len()) {
            None => &self.data[region],
            Some(s) => &self.slots[s].data,
        }
    }

    fn region_bytes_mut(&mut self, region: usize) -> &mut [u8] {
        match region.checked_sub(self.layers.len()) {
            None => &mut self.data[region],
            Some(s) => &mut self.slots[s].data,
        }
    }

    /// The data region of `slot`, which must be live.
    fn live_region(&self, slot: SlotId) -> usize {
        assert!(self.slots.get(slot.0 as usize).is_some_and(|s| s.live), "{slot:?} is not a live data slot");
        self.layers.len() + slot.0 as usize
    }

    /// Every data region's bytes, in packed order: the layers', then the
    /// slots'.
    fn regions(&self) -> impl Iterator<Item = &[u8]> + '_ {
        self.data.iter().map(Vec::as_slice).chain(self.slots.iter().map(|s| s.data.as_slice()))
    }

    /// The pending cell changes without consuming them. Reading an already
    /// clean shared world must not require copying its immutable records.
    pub fn dirty(&self) -> &Dirty { &self.dirty }

    /// The records changed since the last call (all of them in a world
    /// just made), to upload only those (`TileRenderer::update_cells`).
    pub fn take_dirty(&mut self) -> Dirty {
        std::mem::take(&mut self.dirty)
    }

    /// Where the record of `cell` in data region `region` (see
    /// [`Dirty::Records`]; [`ALL_CELLS`]: all of them) lies in the packed
    /// data the GPU reads (byte offset, length; see `pack`).
    pub fn packed_record(&self, region: u32, cell: u32) -> Option<(usize, usize)> {
        let region = region as usize;
        let spec = &self.layers[self.region_layer(region)?];
        let len = spec.record_bytes() as usize;
        let start: usize = self.regions().take(region).map(<[u8]>::len).sum();
        let bytes = self.region_bytes(region).len();
        if cell == ALL_CELLS {
            return Some((start, bytes));
        }
        let at = cell as usize * len;
        (at + len <= bytes).then_some((start + at, len))
    }

    /// The packed data's bytes `range` (zeros past the records, as `pack`
    /// pads them).
    pub fn packed_bytes(&self, range: std::ops::Range<usize>) -> Vec<u8> {
        let mut out = vec![0; range.len()];
        let mut start = 0;
        for d in self.regions() {
            let (lo, hi) = (range.start.max(start), range.end.min(start + d.len()));
            if lo < hi {
                out[lo - range.start..hi - range.start].copy_from_slice(&d[lo - start..hi - start]);
            }
            start += d.len();
        }
        out
    }

    /// Whether the records of data region `region` (a layer's, or a
    /// slot's: see [`Dirty::Records`]) store per-cell offsets.
    pub fn has_offsets(&self, region: u32) -> bool {
        self.region_layer(region as usize).and_then(|l| self.layers.get(l)).is_some_and(|l| l.per_cell_offset)
    }

    pub fn get(&self, layer: LayerId, col: u32, row: u32) -> u32 {
        self.get_region(layer.0, col, row)
    }

    /// [`TileWorld::get`] in data slot `slot` (a live one).
    pub fn get_in(&self, slot: SlotId, col: u32, row: u32) -> u32 {
        self.get_region(self.live_region(slot), col, row)
    }

    fn get_region(&self, region: usize, col: u32, row: u32) -> u32 {
        let n = self.region_spec(region).value_bytes() as usize;
        let at = self.region_record_at(region, col, row);
        let mut b = [0u8; 4];
        b[..n].copy_from_slice(&self.region_bytes(region)[at..at + n]);
        u32::from_le_bytes(b)
    }

    /// Sets the cell's animation phase. Requires `per_cell_phase`.
    pub fn set_phase(&mut self, layer: LayerId, col: u32, row: u32, phase: u8) {
        self.set_phase_region(layer.0, col, row, phase);
    }

    /// [`TileWorld::set_phase`] in data slot `slot` (a live one).
    pub fn set_phase_in(&mut self, slot: SlotId, col: u32, row: u32, phase: u8) {
        self.set_phase_region(self.live_region(slot), col, row, phase);
    }

    fn set_phase_region(&mut self, region: usize, col: u32, row: u32, phase: u8) {
        let spec = self.region_spec(region);
        assert!(spec.per_cell_phase, "layer {} has no per-cell phase", spec.name);
        let at = self.region_record_at(region, col, row) + spec.value_bytes() as usize;
        self.region_bytes_mut(region)[at] = phase;
        self.touch(region, col, row);
    }

    /// Sets the cell's pixel offset (x, y): its sprite is drawn that far
    /// from where the cell and the layer put it (in a view showing the
    /// world `n` times smaller, the offset divided by `n`, rounded down).
    /// Requires `per_cell_offset`.
    pub fn set_offset(&mut self, layer: LayerId, col: u32, row: u32, offset: (i16, i16)) {
        self.set_offset_region(layer.0, col, row, offset);
    }

    /// [`TileWorld::set_offset`] in data slot `slot` (a live one).
    pub fn set_offset_in(&mut self, slot: SlotId, col: u32, row: u32, offset: (i16, i16)) {
        self.set_offset_region(self.live_region(slot), col, row, offset);
    }

    fn set_offset_region(&mut self, region: usize, col: u32, row: u32, (x, y): (i16, i16)) {
        let at = self.offset_at(region, col, row);
        let d = self.region_bytes_mut(region);
        d[at..at + 2].copy_from_slice(&x.to_le_bytes());
        d[at + 2..at + 4].copy_from_slice(&y.to_le_bytes());
        self.touch(region, col, row);
    }

    /// The cell's pixel offset (0, 0 on a layer without them).
    pub fn offset(&self, layer: LayerId, col: u32, row: u32) -> (i16, i16) {
        self.offset_region(layer.0, col, row)
    }

    /// [`TileWorld::offset`] in data slot `slot` (a live one).
    pub fn offset_in(&self, slot: SlotId, col: u32, row: u32) -> (i16, i16) {
        self.offset_region(self.live_region(slot), col, row)
    }

    fn offset_region(&self, region: usize, col: u32, row: u32) -> (i16, i16) {
        if !self.region_spec(region).per_cell_offset {
            return (0, 0);
        }
        let at = self.offset_at(region, col, row);
        let d = self.region_bytes(region);
        (i16::from_le_bytes([d[at], d[at + 1]]), i16::from_le_bytes([d[at + 2], d[at + 3]]))
    }

    fn offset_at(&self, region: usize, col: u32, row: u32) -> usize {
        let spec = self.region_spec(region);
        assert!(spec.per_cell_offset, "layer {} has no per-cell offset", spec.name);
        self.region_record_at(region, col, row) + (spec.value_bytes() + spec.per_cell_phase as u32) as usize
    }

    /// How far the per-cell offsets of the cells that draw something move
    /// their sprites beyond their cells' places, at most: (left, top,
    /// right, bottom), to be added to a camera's overhang (cells that far
    /// outside the viewport may reach into it).
    pub fn offset_reach(&self) -> (u32, u32, u32, u32) {
        let mut reach = (0, 0, 0, 0);
        // (A slot's records reach as far as the layer's would.)
        let regions = self.data.iter().enumerate().chain(self.slots.iter().filter(|s| s.live).map(|s| (s.layer, &s.data)));
        for (i, data) in regions {
            let spec = &self.layers[i];
            if !spec.per_cell_offset {
                continue;
            }
            let (n, at) = (spec.value_bytes() as usize, (spec.value_bytes() + spec.per_cell_phase as u32) as usize);
            for record in data.chunks_exact(spec.record_bytes() as usize) {
                if record[..n].iter().all(|&b| b == 0) {
                    continue;
                }
                let x = i16::from_le_bytes([record[at], record[at + 1]]) as i32;
                let y = i16::from_le_bytes([record[at + 2], record[at + 3]]) as i32;
                reach.0 = reach.0.max((-x).max(0) as u32);
                reach.1 = reach.1.max((-y).max(0) as u32);
                reach.2 = reach.2.max(x.max(0) as u32);
                reach.3 = reach.3.max(y.max(0) as u32);
            }
        }
        reach
    }

    fn region_record_at(&self, region: usize, col: u32, row: u32) -> usize {
        assert!(col < self.grid.cols && row < self.grid.rows, "cell ({col}, {row}) outside the grid");
        let spec = self.region_spec(region);
        ((row * self.grid.cols + col) * spec.record_bytes()) as usize
    }

    /// The GPU's layer tables: the layers drawn from their own records
    /// (the shared binding's), then per binding (ascending) the same with
    /// the layers it binds to slots drawn from those; and where each
    /// binding's table starts (binding, entries; the shared one at 0, not
    /// listed). Small next to the cell data: a binding costs a table, not
    /// a copy of the cells.
    pub(crate) fn layer_tables(&self) -> (Vec<LayerDesc>, Vec<(u32, u32)>) {
        let (shared, _) = self.shared_table();
        let mut starts = Vec::with_capacity(self.slots.len());
        let mut at: usize = self.data.iter().map(Vec::len).sum();
        for s in &self.slots {
            starts.push(at as u32);
            at += s.data.len();
        }
        let mut table = shared.clone();
        let mut tables = Vec::with_capacity(self.bindings.len());
        for (key, bound) in &self.bindings {
            tables.push((*key, table.len() as u32));
            table.extend(shared.iter().enumerate().map(|(l, d)| match bound.iter().find(|(bl, _)| *bl == l) {
                Some((_, s)) => LayerDesc { data_byte_offset: starts[s.0 as usize], ..*d },
                None => *d,
            }));
        }
        (table, tables)
    }

    /// Words of the packed cell data (see `pack`).
    pub(crate) fn packed_words(&self) -> usize {
        self.regions().map(<[u8]>::len).sum::<usize>().div_ceil(4) + 1
    }

    /// GPU layer tables, lookup tables and packed data, in the layout the
    /// shader expects.
    pub(crate) fn pack(&self) -> PackedWorld {
        let (layers, tables) = self.layer_tables();
        let (_, lut) = self.shared_table();
        let mut data: Vec<u8> = Vec::with_capacity(self.packed_words() * 4);
        for bytes in self.regions() {
            data.extend_from_slice(bytes);
        }
        // Word-align, plus one spare word: the shader may read the word after
        // a record that ends exactly on a word boundary.
        while data.len() % 4 != 0 {
            data.push(0);
        }
        data.extend_from_slice(&[0; 4]);
        PackedWorld { layers, tables, lut, data: bytemuck::pod_collect_to_vec(&data), sets: self.sets.words().to_vec(), reach: self.offset_reach() }
    }

    /// The layers drawn from their own records, and their lookup tables.
    fn shared_table(&self) -> (Vec<LayerDesc>, Vec<u32>) {
        let mut layers = Vec::with_capacity(self.layers.len());
        let mut lut = Vec::new();
        let mut at = 0usize;
        for (spec, bytes) in self.layers.iter().zip(&self.data) {
            layers.push(LayerDesc {
                data_byte_offset: at as u32,
                value_bytes: spec.value_bytes(),
                phase_bytes: spec.per_cell_phase as u32,
                lut_offset: lut.len() as u32,
                lut_len: spec.lut.len() as u32,
                offset_x: spec.offset.0,
                offset_y: spec.offset.1,
                flags: if spec.hidden { LAYER_HIDDEN } else { 0 }
                    | if spec.lut.first().is_none_or(|s| *s == SpriteSetId::EMPTY) { tile_render_shared::LAYER_ZERO_EMPTY } else { 0 },
                ticks_per_frame: spec.ticks_per_frame,
                cull_offset_x: spec.cull_offset.0,
                cull_offset_y: spec.cull_offset.1,
                scale_den: match spec.blend {
                    Blend::NativePaletteAlpha | Blend::Invert | Blend::Subpixel { .. } => 0,
                    b => b.scale_den(),
                },
                category: spec.category,
                offset_bytes: if spec.per_cell_offset { OFFSET_BYTES } else { 0 },
                palette_shift: spec.palette_shift(),
                _pad1: 0,
            });
            lut.extend(spec.lut.iter().map(|s| s.0));
            at += bytes.len();
        }
        if lut.is_empty() {
            lut.push(0);
        }
        (layers, lut)
    }
}

/// Where the layer table of `binding` starts (`FrameParams::layer_table`),
/// of [`TileWorld::layer_tables`]' list: 0, the shared table, for a binding
/// that binds nothing.
pub(crate) fn table_of(tables: &[(u32, u32)], binding: u32) -> u32 {
    tables.binary_search_by_key(&binding, |t| t.0).map_or(0, |i| tables[i].1)
}

pub(crate) struct PackedWorld {
    /// The layer tables (see [`TileWorld::layer_tables`]).
    pub layers: Vec<LayerDesc>,
    pub tables: Vec<(u32, u32)>,
    pub lut: Vec<u32>,
    pub data: Vec<u32>,
    pub sets: Vec<u32>,
    /// See [`TileWorld::offset_reach`].
    pub reach: (u32, u32, u32, u32),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tile_render_shared::ByteReader;

    #[test]
    fn outline_instances_keep_their_low_and_zoomed_twin_flags() {
        let face = std::sync::Arc::new(gpu_text::Face::from_curves(1000, &[('A', Vec::new(), 600)]));
        let glyph = OutlineGlyph { font: 0, shape: GlyphShape::Glyph(0), em64: 12 * 64, advance: 7, bold: false, italic: false, color: [255; 3], key: None };
        for (zoom_art, flag, at_one, at_zoom) in [
            (ZoomArt::Any, 0, true, true),
            (ZoomArt::Low, tile_render_shared::FREE_LOW, true, false),
            (ZoomArt::Zoomed { zoom: (0, 0), offset: (0, 0) }, tile_render_shared::FREE_ZOOMED, false, true),
        ] {
            let sprite = PlacedSprite { screen: true, zoom_art, text: Some(glyph), ..PlacedSprite::new(SpriteSetId::EMPTY, 3, 12) };
            let packed = glyph.gpu(&sprite, std::slice::from_ref(&face), &[0]).unwrap();
            assert_eq!(packed.flags & (tile_render_shared::FREE_LOW | tile_render_shared::FREE_ZOOMED), flag);
            assert_eq!(tile_render_shared::text::visible_at_art(packed.flags, 0), at_one);
            assert_eq!(tile_render_shared::text::visible_at_art(packed.flags, 2), at_zoom);
            assert!(!tile_render_shared::text::visible_at_art(packed.flags, 1));
        }
    }

    #[test]
    fn uniform_alpha_survives_the_scene_and_gpu_sprite_encoding() {
        for opacity in 0..255 {
            let blend = Blend::AlphaOpacity { opacity };
            let s = PlacedSprite { blend, ..PlacedSprite::new(SpriteSetId(1), 0, 0) };
            let gpu = s.gpu();
            assert_eq!(Blend::from_scale_den(gpu.scale_den, gpu.tint), blend);
        }
        assert_eq!(Blend::from_scale_den(0, 0), Blend::Alpha);
    }

    #[test]
    fn slots_are_allocated_reused_freed_and_bound_without_copies() {
        let grid = Grid { cols: 6, rows: 4, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
        let mut sets = SpriteSets::default();
        let a = sets.single(SpriteId(3));
        let mut w = TileWorld::new(grid, sets);
        let terrain = w.add_layer(LayerSpec::new("terrain", vec![a; 300]));
        let fog = w.add_layer(LayerSpec::new("fog", vec![SpriteSetId::EMPTY, a, a]));
        let mut edge_spec = LayerSpec::new("fog_edge", vec![SpriteSetId::EMPTY, a]);
        edge_spec.per_cell_offset = true;
        let edge = w.add_layer(edge_spec);
        w.set(fog, 1, 1, 1);
        let base_bytes: u32 = (2 + 1 + 5) * 24;
        // No slot, no binding: one table, every camera the shared one.
        let (table, tables) = w.layer_tables();
        assert_eq!((table.len(), tables.len(), w.slot_bytes()), (3, 0, 0));
        let _ = w.take_dirty();
        let s = w.add_slot(fog);
        assert_eq!(w.take_dirty(), Dirty::Records(vec![(3, ALL_CELLS)]), "a new slot uploads its region");
        assert_eq!(w.packed_record(3, ALL_CELLS), Some((base_bytes as usize, 24)));
        assert_eq!(w.get_in(s, 1, 1), 0, "a slot starts empty");
        w.set_in(s, 2, 3, 2);
        assert_eq!((w.get_in(s, 2, 3), w.get(fog, 2, 3), w.get(fog, 1, 1)), (2, 0, 1));
        assert_eq!(w.take_dirty(), Dirty::Records(vec![(w.slot_region(s), 3 * 6 + 2)]));
        assert_eq!(w.packed_record(w.slot_region(s), 3 * 6 + 2), Some((base_bytes as usize + 20, 1)));
        // Bound to two bindings: one region, two tables pointing at it.
        w.bind(1, fog, Some(s));
        w.bind(5, fog, Some(s));
        w.bind(5, fog, Some(s));
        assert_eq!(w.take_dirty(), Dirty::Records(Vec::new()), "binding moves no records");
        let (table, tables) = w.layer_tables();
        assert_eq!(tables, [(1, 3), (5, 6)]);
        assert_eq!((table[4].data_byte_offset, table[7].data_byte_offset), (base_bytes, base_bytes));
        assert_eq!((table[3], table[5]), (table[0], table[2]), "unbound layers draw their own records");
        assert_eq!((table_of(&tables, 5), table_of(&tables, 0), table_of(&tables, 2)), (6, 0, 0));
        // The packed data reads the slot back where the table says.
        let p = w.pack();
        let reader = tile_render_shared::ByteReader::new(&p.data);
        assert_eq!(reader.uint(tile_render_shared::record_at(&p.layers[4], 6, 2, 3), 1), 2);
        assert_eq!(reader.uint(tile_render_shared::record_at(&p.layers[1], 6, 1, 1), 1), 1);
        assert_eq!(p.data.len(), w.packed_words());
        // An offset slot reaches as far as its records do.
        let e = w.add_slot(edge);
        w.set_in(e, 0, 1, 1);
        w.set_offset_in(e, 0, 1, (-128, 0));
        assert_eq!((w.offset_reach(), w.offset_in(e, 0, 1), w.offset(edge, 0, 1)), ((128, 0, 0, 0), (-128, 0), (0, 0)));
        w.bind(1, edge, Some(e));
        // Freeing the first slot unbinds it everywhere and keeps its region
        // (a slot after it is live) for the next one of its size.
        w.free_slot(s);
        assert_eq!((w.bound(1, fog), w.bound(5, fog), w.bound(1, edge)), (None, None, Some(e)));
        assert_eq!(w.bindings().collect::<Vec<_>>(), [1]);
        assert_eq!(w.slot_bytes(), 24 + 5 * 24);
        let again = w.add_slot(fog);
        assert_eq!((again, w.slot_bytes(), w.get_in(again, 2, 3)), (s, 24 + 5 * 24, 0), "reused, cleared");
        // Freed from the end: given back.
        w.free_slot(e);
        assert_eq!((w.slot_bytes(), w.offset_reach()), (24, (0, 0, 0, 0)));
        w.free_slot(again);
        assert_eq!((w.slot_bytes(), w.slots().count(), w.layer_tables().0.len()), (0, 0, 3));
        assert_eq!(w.packed_record(3, 0), None, "a freed region is no record");
        let _ = terrain;
    }

    #[test]
    fn packed_records_read_back_through_the_shader_reader() {
        let grid = Grid { cols: 7, rows: 5, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
        let mut sets = SpriteSets::default();
        let a = sets.single(SpriteId(3));
        let mut w = TileWorld::new(grid, sets);
        let small = w.add_layer(LayerSpec::new("small", vec![SpriteSetId::EMPTY, a]));
        let mut big_spec = LayerSpec::new("big", vec![a; 70_000]);
        big_spec.per_cell_phase = true;
        let big = w.add_layer(big_spec);
        for r in 0..5 {
            for c in 0..7 {
                w.set(small, c, r, (c + r) % 2);
                w.set(big, c, r, c * 9_000 + r);
                w.set_phase(big, c, r, (c * r) as u8);
            }
        }
        let mut moved_spec = LayerSpec::new("moved", vec![SpriteSetId::EMPTY, a]);
        (moved_spec.per_cell_phase, moved_spec.per_cell_offset) = (true, true);
        let moved = w.add_layer(moved_spec);
        w.set(moved, 3, 2, 1);
        w.set_phase(moved, 3, 2, 9);
        w.set_offset(moved, 3, 2, (-300, 7));
        w.set_offset(moved, 4, 2, (-500, -9));
        // (Cells drawing nothing do not reach.)
        assert_eq!(w.offset_reach(), (300, 0, 0, 7));
        let p = w.pack();
        let reader = ByteReader::new(&p.data);
        let l = &p.layers[2];
        let at = tile_render_shared::record_at(l, 7, 3, 2);
        assert_eq!((reader.uint(at, 1), reader.u8(at + 1)), (1, 9));
        assert_eq!(tile_render_shared::cell_offset(&reader, l, at, 1), (-300, 7));
        assert_eq!(tile_render_shared::cell_offset(&reader, l, at, 2), (-150, 3));
        assert_eq!(tile_render_shared::cell_offset(&reader, &p.layers[0], 0, 1), (0, 0));
        for r in 0..5u32 {
            for c in 0..7u32 {
                let l = &p.layers[1];
                let byte = l.data_byte_offset + (r * 7 + c) * (l.value_bytes + l.phase_bytes);
                assert_eq!(l.value_bytes, 3);
                assert_eq!(reader.uint(byte, 3), c * 9_000 + r);
                assert_eq!(reader.u8(byte + 3), c * r);
                let l = &p.layers[0];
                assert_eq!(reader.uint(l.data_byte_offset + r * 7 + c, 1), (c + r) % 2);
            }
        }
    }
}

/// A view's first sprite requiring rendering at the target resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ZoomedStart {
    pub zoom: (u32, u32),
    pub pass: usize,
    pub view: Option<u32>,
    pub except_view: bool,
    pub category: u32,
}

/// Keep the visibility of each zoomed sprite. Text in a separate interface
/// must not switch unrelated world views to full-resolution rendering.
pub(crate) fn first_zoomed(free: &[PlacedSprite], passes: usize) -> Vec<ZoomedStart> {
    let mut out: Vec<ZoomedStart> = Vec::new();
    for s in free {
        let zoom = match (s.text, s.zoom_art) {
            (Some(_), _) => (0, 0),
            // (Shapes laid out at every stage's scale: as text.)
            (None, _) if s.shape.is_some() => (0, 0),
            (None, ZoomArt::Zoomed { zoom, .. }) => zoom,
            _ => continue,
        };
        let pass = s.after_pass.min(passes);
        if let Some(start) = out.iter_mut().find(|start| start.zoom == zoom && start.view == s.view && start.except_view == s.except_view && start.category == s.category) {
            start.pass = start.pass.min(pass);
        } else {
            out.push(ZoomedStart { zoom, pass, view: s.view, except_view: s.except_view, category: s.category });
        }
    }
    out
}

pub(crate) fn zoomed_from(first: &[ZoomedStart], zoom: (u32, u32), view: u32, camera: &crate::Camera) -> Option<usize> {
    first.iter().filter(|s| {
        let visible = if s.except_view {
            s.view != Some(view) && camera.cells
        } else {
            s.view.map_or(camera.cells, |v| v == view)
        };
        visible && s.category & camera.effective_hidden_categories() == 0 && (s.zoom == zoom || s.zoom == (0, 0))
    }).map(|s| s.pass).min()
}

/// The words of `fonts` back to back (one outline buffer), and where each
/// font's start.
pub(crate) fn outline_words(fonts: &[std::sync::Arc<gpu_text::Face>]) -> (Vec<u32>, Vec<u32>) {
    let mut words = Vec::new();
    let mut bases = Vec::with_capacity(fonts.len());
    for f in fonts {
        bases.push(words.len() as u32);
        words.extend_from_slice(&f.words);
    }
    if words.is_empty() {
        words.push(0);
    }
    (words, bases)
}

#[cfg(test)]
mod zoomed_start_tests {
    use super::*;

    #[test]
    fn unrelated_or_hidden_art_does_not_scale_world_passes() {
        let sprite = |view, after_pass| PlacedSprite {
            view,
            after_pass,
            zoom_art: ZoomArt::Zoomed { zoom: (2, 1), offset: (0, 0) },
            ..PlacedSprite::new(SpriteSetId::EMPTY, 0, 0)
        };
        let first = first_zoomed(&[
            sprite(Some(1), 0),
            PlacedSprite { category: 4, ..sprite(Some(0), 1) },
            PlacedSprite { except_view: true, ..sprite(Some(0), 2) },
            sprite(None, 3),
        ], 4);
        let world = crate::Camera { hidden_categories: 4, ..crate::Camera::new((100, 100)) };
        assert_eq!(zoomed_from(&first, (2, 1), 0, &world), Some(3));
        assert_eq!(zoomed_from(&first, (2, 1), 1, &world), Some(0));
        assert_eq!(zoomed_from(&first, (2, 1), 2, &world), Some(2));
        assert_eq!(zoomed_from(&first, (3, 1), 0, &world), None);
        let interface = crate::Camera { cells: false, ..world };
        assert_eq!(zoomed_from(&first, (2, 1), 2, &interface), None);
        assert_eq!(zoomed_from(&first, (2, 1), 1, &interface), Some(0));
    }
}

#[cfg(test)]
mod bulk_cell_tests {
    use super::*;
    #[test]
    fn whole_layer_packing_matches_individual_setters_for_every_record_width() {
        for n in 1..=4 {
            for phase in [false, true] { for offset in [false, true] {
                for len in [0, 7, 15, 21] {
                    let grid = Grid { cols: 5, rows: 3, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (8, 8) };
                    let mut spec = LayerSpec::new("cells", vec![SpriteSetId::EMPTY; 2]);
                    if n > 1 { spec.index_bits = 8; spec.palette_bits = 8 * (n - 1); }
                    spec.per_cell_phase = phase; spec.per_cell_offset = offset;
                    assert_eq!(spec.value_bytes(), n);
                    let mut expected = TileWorld::new(grid, SpriteSets::default());
                    let mut actual = TileWorld::new(grid, SpriteSets::default());
                    expected.add_layer(spec.clone()); actual.add_layer(spec);
                    let values: Vec<_> = (0..len).map(|i| i * 0x1020304 + 0x12).collect();
                    let phases: Vec<_> = (0..len).map(|i| (i * 17) as u8).collect();
                    let offsets: Vec<_> = (0..len).map(|i| (i as i16 - 9, 4 - i as i16)).collect();
                    for i in 0..len.min(15) {
                        expected.set(LayerId(0), i % 5, i / 5, values[i as usize]);
                        if phase { expected.set_phase(LayerId(0), i % 5, i / 5, phases[i as usize]); }
                        if offset { expected.set_offset(LayerId(0), i % 5, i / 5, offsets[i as usize]); }
                    }
                    actual.set_layer_cells(LayerId(0), &values, Some(&phases), Some(&offsets));
                    assert_eq!(actual.data, expected.data, "width={n}, phase={phase}, offset={offset}, len={len}");
                    actual.take_dirty();
                    actual.set_layer_cells(LayerId(0), &[], None, None);
                    assert!(actual.data[0].iter().all(|&v| v == 0));
                    assert_eq!(actual.take_dirty(), Dirty::Records(vec![(0, ALL_CELLS)]));
                }
            } }
        }
    }
}
