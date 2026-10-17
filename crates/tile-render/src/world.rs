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
            Blend::Alpha => 0,
            Blend::Scale { denominator } => denominator.max(1),
            Blend::Invert => tile_render_shared::SCALE_INVERT,
            Blend::Subpixel { .. } => tile_render_shared::SCALE_SUBPIXEL,
        }
    }

    /// The colour free sprites carry for it (0xRRGGBB, `FreeSprite::tint`).
    pub fn tint(self) -> u32 {
        match self {
            Blend::Subpixel { color: [r, g, b] } => (r as u32) << 16 | (g as u32) << 8 | b as u32,
            _ => 0,
        }
    }

    /// From [`Blend::scale_den`] and [`Blend::tint`].
    pub fn from_scale_den(d: u32, tint: u32) -> Blend {
        match d {
            0 => Blend::Alpha,
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
    /// A glyph drawn from its font's outline instead of the sprite (`set`,
    /// `blend` and `zoom_art` do not matter then): zoomed art for every
    /// zoom, drawn in views drawn at their zoom showing zoomed art, at its
    /// 1:1 position scaled, at that zoom's resolution.
    pub text: Option<OutlineGlyph>,
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
            clip: sprite.clip.unwrap_or_default(),
            color: rgb(self.color),
            key: self.key.map_or(0, rgb),
            bar_top: bar.0,
            bar_height: bar.1,
        })
    }
}

impl PlacedSprite {
    pub fn new(set: SpriteSetId, x: i32, y: i32) -> Self {
        Self { set, x, y, screen: false, after_pass: usize::MAX, phase: 0, ticks_per_frame: 1, blend: Blend::Alpha, clip: None, view: None, except_view: false, zoom_art: ZoomArt::Any, text: None }
    }

    /// Its `FREE_SCREEN`, `FREE_CLIP` and `FREE_EXCEPT_VIEW` flags.
    fn flags(&self) -> u32 {
        (if self.screen { tile_render_shared::FREE_SCREEN } else { 0 })
            | if self.clip.is_some() { tile_render_shared::FREE_CLIP } else { 0 }
            | if self.except_view && self.view.is_some() { tile_render_shared::FREE_EXCEPT_VIEW } else { 0 }
    }

    pub(crate) fn gpu(&self) -> tile_render_shared::FreeSprite {
        tile_render_shared::FreeSprite {
            set: self.set.0,
            phase: self.phase,
            ticks_per_frame: self.ticks_per_frame.max(1),
            flags: self.flags()
                | match self.zoom_art {
                    ZoomArt::Any => 0,
                    ZoomArt::Low => tile_render_shared::FREE_LOW,
                    ZoomArt::Zoomed { .. } => tile_render_shared::FREE_ZOOMED,
                },
            x: self.x,
            y: self.y,
            scale_den: self.blend.scale_den(),
            view: self.view.unwrap_or(tile_render_shared::ALL_VIEWS),
            clip: self.clip.unwrap_or_default(),
            tint: self.blend.tint(),
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
        }
    }

    /// Smallest value width (bytes) that can index the whole table.
    pub fn value_bytes(&self) -> u32 {
        match self.lut.len() {
            0..=0x100 => 1,
            0x101..=0x1_0000 => 2,
            0x1_0001..=0x100_0000 => 3,
            _ => 4,
        }
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
}

/// Index of a layer in a `TileWorld`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LayerId(pub usize);

impl TileWorld {
    pub fn new(grid: Grid, sets: SpriteSets) -> Self {
        Self { grid, sets, color: ColorModel::Direct, free: Vec::new(), fonts: Vec::new(), layers: Vec::new(), data: Vec::new() }
    }

    pub fn add_layer(&mut self, spec: LayerSpec) -> LayerId {
        assert!(self.layers.len() < 255);
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
        LayerId(self.layers.len() - 1)
    }

    pub fn layers(&self) -> &[LayerSpec] {
        &self.layers
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
        let spec = &self.layers[layer.0];
        let n = spec.value_bytes() as usize;
        let at = self.record_at(layer, col, row);
        self.data[layer.0][at..at + n].copy_from_slice(&value.to_le_bytes()[..n]);
    }

    pub fn get(&self, layer: LayerId, col: u32, row: u32) -> u32 {
        let n = self.layers[layer.0].value_bytes() as usize;
        let at = self.record_at(layer, col, row);
        let mut b = [0u8; 4];
        b[..n].copy_from_slice(&self.data[layer.0][at..at + n]);
        u32::from_le_bytes(b)
    }

    /// Sets the cell's animation phase. Requires `per_cell_phase`.
    pub fn set_phase(&mut self, layer: LayerId, col: u32, row: u32, phase: u8) {
        let spec = &self.layers[layer.0];
        assert!(spec.per_cell_phase, "layer {} has no per-cell phase", spec.name);
        let at = self.record_at(layer, col, row) + spec.value_bytes() as usize;
        self.data[layer.0][at] = phase;
    }

    /// Sets the cell's pixel offset (x, y): its sprite is drawn that far
    /// from where the cell and the layer put it (in a view showing the
    /// world `n` times smaller, the offset divided by `n`, rounded down).
    /// Requires `per_cell_offset`.
    pub fn set_offset(&mut self, layer: LayerId, col: u32, row: u32, (x, y): (i16, i16)) {
        let at = self.offset_at(layer, col, row);
        self.data[layer.0][at..at + 2].copy_from_slice(&x.to_le_bytes());
        self.data[layer.0][at + 2..at + 4].copy_from_slice(&y.to_le_bytes());
    }

    /// The cell's pixel offset (0, 0 on a layer without them).
    pub fn offset(&self, layer: LayerId, col: u32, row: u32) -> (i16, i16) {
        if !self.layers[layer.0].per_cell_offset {
            return (0, 0);
        }
        let at = self.offset_at(layer, col, row);
        let d = &self.data[layer.0];
        (i16::from_le_bytes([d[at], d[at + 1]]), i16::from_le_bytes([d[at + 2], d[at + 3]]))
    }

    fn offset_at(&self, layer: LayerId, col: u32, row: u32) -> usize {
        let spec = &self.layers[layer.0];
        assert!(spec.per_cell_offset, "layer {} has no per-cell offset", spec.name);
        self.record_at(layer, col, row) + (spec.value_bytes() + spec.per_cell_phase as u32) as usize
    }

    /// How far the per-cell offsets of the cells that draw something move
    /// their sprites beyond their cells' places, at most: (left, top,
    /// right, bottom), to be added to a camera's overhang (cells that far
    /// outside the viewport may reach into it).
    pub fn offset_reach(&self) -> (u32, u32, u32, u32) {
        let mut reach = (0, 0, 0, 0);
        for (i, spec) in self.layers.iter().enumerate() {
            if !spec.per_cell_offset {
                continue;
            }
            let (n, at) = (spec.value_bytes() as usize, (spec.value_bytes() + spec.per_cell_phase as u32) as usize);
            for record in self.data[i].chunks_exact(spec.record_bytes() as usize) {
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

    fn record_at(&self, layer: LayerId, col: u32, row: u32) -> usize {
        assert!(col < self.grid.cols && row < self.grid.rows, "cell ({col}, {row}) outside the grid");
        let spec = &self.layers[layer.0];
        ((row * self.grid.cols + col) * spec.record_bytes()) as usize
    }

    /// GPU layer table, lookup tables and packed data, in the layout the
    /// shader expects.
    pub(crate) fn pack(&self) -> PackedWorld {
        let mut layers = Vec::with_capacity(self.layers.len());
        let mut lut = Vec::new();
        let mut data = Vec::new();
        for (spec, bytes) in self.layers.iter().zip(&self.data) {
            layers.push(LayerDesc {
                data_byte_offset: data.len() as u32,
                value_bytes: spec.value_bytes(),
                phase_bytes: spec.per_cell_phase as u32,
                lut_offset: lut.len() as u32,
                lut_len: spec.lut.len() as u32,
                offset_x: spec.offset.0,
                offset_y: spec.offset.1,
                flags: if spec.hidden { LAYER_HIDDEN } else { 0 },
                ticks_per_frame: spec.ticks_per_frame,
                cull_offset_x: spec.cull_offset.0,
                cull_offset_y: spec.cull_offset.1,
                scale_den: match spec.blend {
                    Blend::Invert | Blend::Subpixel { .. } => 0,
                    b => b.scale_den(),
                },
                offset_bytes: if spec.per_cell_offset { OFFSET_BYTES } else { 0 },
                _pad0: 0,
                _pad1: 0,
                _pad2: 0,
            });
            lut.extend(spec.lut.iter().map(|s| s.0));
            data.extend_from_slice(bytes);
        }
        // Word-align, plus one spare word: the shader may read the word after
        // a record that ends exactly on a word boundary.
        while data.len() % 4 != 0 {
            data.push(0);
        }
        data.extend_from_slice(&[0; 4]);
        if lut.is_empty() {
            lut.push(0);
        }
        PackedWorld { layers, lut, data: bytemuck::pod_collect_to_vec(&data), sets: self.sets.words().to_vec(), reach: self.offset_reach() }
    }
}

pub(crate) struct PackedWorld {
    pub layers: Vec<LayerDesc>,
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

/// Per zoom that `free` has zoomed sprites for, the first pass they follow
/// (of `passes`; `passes`: after all of them); zoom (0, 0): text drawn from
/// outlines, zoomed art for every zoom (see [`zoomed_from`]).
pub(crate) fn first_zoomed(free: &[PlacedSprite], passes: usize) -> Vec<((u32, u32), usize)> {
    let mut out: Vec<((u32, u32), usize)> = Vec::new();
    for s in free {
        let zoom = match (s.text, s.zoom_art) {
            (Some(_), _) => (0, 0),
            (None, ZoomArt::Zoomed { zoom, .. }) => zoom,
            _ => continue,
        };
        let pass = s.after_pass.min(passes);
        match out.iter_mut().find(|(z, _)| *z == zoom) {
            Some((_, p)) => *p = (*p).min(pass),
            None => out.push((zoom, pass)),
        }
    }
    out
}

/// Of [`first_zoomed`]'s list, the first pass zoomed art for `zoom` follows.
pub(crate) fn zoomed_from(first: &[((u32, u32), usize)], zoom: (u32, u32)) -> Option<usize> {
    first.iter().filter(|(z, _)| *z == zoom || *z == (0, 0)).map(|&(_, p)| p).min()
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
