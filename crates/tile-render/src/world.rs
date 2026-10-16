//! CPU-side description of a tile world: grid, layers and their cell values.

use tile_render_shared::{GridParams, LAYER_HIDDEN, LAYOUT_ORTHOGONAL, LAYOUT_STAGGERED, LayerDesc, WRAP_X, WRAP_Y};

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

/// Which zoom a screen sprite's art is made for. A driver may give a view
/// that it knows to be scaled (`Camera::zoomed_art`) sprites made for its
/// zoom (`Zoomed`, e.g. text rasterised at that size) next to their 1:1
/// twins (`Low`): such a view draws the zoomed ones after scaling, at
/// full resolution, other views the low ones.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ZoomArt {
    #[default]
    Any,
    Low,
    /// Positioned in target pixels from where the view's screen sprites
    /// start (its origin less its screen shift), scaled by its zoom.
    Zoomed,
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
    /// Art for a zoom (screen sprites of a view; see [`ZoomArt`]).
    pub zoom_art: ZoomArt,
}

impl PlacedSprite {
    pub fn new(set: SpriteSetId, x: i32, y: i32) -> Self {
        Self { set, x, y, screen: false, after_pass: usize::MAX, phase: 0, ticks_per_frame: 1, blend: Blend::Alpha, clip: None, view: None, zoom_art: ZoomArt::Any }
    }

    pub(crate) fn gpu(&self) -> tile_render_shared::FreeSprite {
        tile_render_shared::FreeSprite {
            set: self.set.0,
            phase: self.phase,
            ticks_per_frame: self.ticks_per_frame.max(1),
            flags: if self.screen { tile_render_shared::FREE_SCREEN } else { 0 }
                | if self.clip.is_some() { tile_render_shared::FREE_CLIP } else { 0 }
                | match self.zoom_art {
                    ZoomArt::Any => 0,
                    ZoomArt::Low => tile_render_shared::FREE_LOW,
                    ZoomArt::Zoomed => tile_render_shared::FREE_ZOOMED,
                },
            x: self.x,
            y: self.y,
            scale_den: self.blend.scale_den(),
            view: self.view.unwrap_or(tile_render_shared::ALL_VIEWS),
            clip: self.clip.unwrap_or_default(),
            tint: self.blend.tint(),
            _pad: [0; 3],
        }
    }
}

/// A range of layers drawn together, see [`TileWorld::passes`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Pass {
    pub first: u32,
    pub count: u32,
}

/// Static configuration of one layer.
#[derive(Clone, Debug)]
pub struct LayerSpec {
    pub name: String,
    /// Value → sprite set. Values outside the table draw nothing.
    pub lut: Vec<SpriteSetId>,
    /// Store a per-cell animation phase next to every value.
    pub per_cell_phase: bool,
    /// Pixel offset of the whole layer relative to cell origins.
    pub offset: (i32, i32),
    pub ticks_per_frame: u32,
    pub hidden: bool,
    /// Render pass. A world's layers must be ordered by pass.
    pub pass: u32,
    /// Point tested against `Camera::cull`, relative to the cell origin.
    pub cull_offset: (i32, i32),
    pub blend: Blend,
}

impl LayerSpec {
    pub fn new(name: impl Into<String>, lut: Vec<SpriteSetId>) -> Self {
        Self { name: name.into(), lut, per_cell_phase: false, offset: (0, 0), ticks_per_frame: 1, hidden: false, pass: 0, cull_offset: (0, 0), blend: Blend::Alpha }
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
        self.value_bytes() + self.per_cell_phase as u32
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
    layers: Vec<LayerSpec>,
    /// Each layer's records, packed, little endian.
    data: Vec<Vec<u8>>,
}

/// Index of a layer in a `TileWorld`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LayerId(pub usize);

impl TileWorld {
    pub fn new(grid: Grid, sets: SpriteSets) -> Self {
        Self { grid, sets, color: ColorModel::Direct, free: Vec::new(), layers: Vec::new(), data: Vec::new() }
    }

    pub fn add_layer(&mut self, spec: LayerSpec) -> LayerId {
        assert!(self.layers.len() < 255);
        if let Some(prev) = self.layers.last() {
            assert!(spec.pass >= prev.pass, "layer {} (pass {}) after pass {}: layers must be ordered by pass", spec.name, spec.pass, prev.pass);
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
                out.push(Pass { first: i as u32, count: 1 });
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
                ticks_per_frame: spec.ticks_per_frame.max(1),
                cull_offset_x: spec.cull_offset.0,
                cull_offset_y: spec.cull_offset.1,
                scale_den: match spec.blend {
                    Blend::Invert | Blend::Subpixel { .. } => 0,
                    b => b.scale_den(),
                },
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
        PackedWorld { layers, lut, data: bytemuck::pod_collect_to_vec(&data), sets: self.sets.words().to_vec() }
    }
}

pub(crate) struct PackedWorld {
    pub layers: Vec<LayerDesc>,
    pub lut: Vec<u32>,
    pub data: Vec<u32>,
    pub sets: Vec<u32>,
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
        let p = w.pack();
        let reader = ByteReader::new(&p.data);
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
