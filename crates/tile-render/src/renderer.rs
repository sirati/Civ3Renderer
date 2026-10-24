//! The wgpu side: pipeline, GPU buffers and the draw call.

use std::num::NonZeroU64;
mod smoke;
mod lens;
mod glyph_cache;
pub use glyph_cache::GlyphCacheStats;

use tile_render_shared::{FrameParams, FreeSprite, LayerDesc, OutputParams, QUAD_VERTICES, SpriteDesc};
use wgpu::util::DeviceExt;

use crate::atlas::Atlas;
use crate::camera::Camera;
use crate::world::{ColorModel, Grid, Pass, TileWorld};

const SPV: &[u8] = include_bytes!(env!("TILE_RENDER_SPV"));
const FALLBACK_SPV: &[u8] = include_bytes!(env!("TILE_RENDER_SPV_FALLBACK"));

/// Per-pass `FrameParams` live in one buffer at this stride (the minimum
/// uniform offset alignment wgpu guarantees).
const PARAMS_STRIDE: u64 = 256;
/// Only draw controls are tripled, never world buffers or atlas textures.
const CONTROL_SLOTS: usize = 3;

#[derive(Default)]
struct ControlFrames {
    next: std::cell::Cell<usize>,
    prepared: std::cell::Cell<usize>,
    active: std::cell::Cell<usize>,
}

impl ControlFrames {
    fn claim(&self) -> Option<usize> {
        if self.prepared.get() == CONTROL_SLOTS { return None }
        let slot = self.next.get();
        self.next.set((slot + 1) % CONTROL_SLOTS);
        self.prepared.set(self.prepared.get() + 1);
        self.active.set(slot);
        Some(slot)
    }
}
#[derive(Default)]
struct ControlUpload {
    params: ControlBytes,
    output: ControlBytes,
}

#[derive(Default)]
struct ControlBytes {
    bytes: std::cell::RefCell<Vec<u8>>,
    start: std::cell::Cell<usize>,
}

fn control_bytes(upload: &ControlBytes, offset: u64, data: &[u8]) {
    let mut bytes = upload.bytes.borrow_mut();
    let offset = offset as usize;
    upload.start.set(upload.start.get().min(offset));
    if bytes.len() < offset + data.len() { bytes.resize(offset + data.len(), 0); }
    bytes[offset..offset + data.len()].copy_from_slice(data);
}

/// Views [`TileRenderer::render_views`] draws at most. A split needs
/// divider fills and lines as well as the driver's views, anchored HUD
/// panels, three framed insets, a framed magnifier, cursor and FPS.
/// The fixed uniform buffers use 140 KiB more than the former 16 views.
pub const MAX_VIEWS: usize = 32;
/// Most passes a world may have.
pub const MAX_PASSES: usize = 16;
/// `FrameParams` slots per view: one per pass and one for after all, drawn
/// at zoom 1 and again at the view's zoom.
const VIEW_SLOTS: usize = 2 * (MAX_PASSES + 1);

/// Format of the internal target of quantized colour models: holds
/// `q + QUANTIZED_BIAS` per channel (see `tile_render_shared::scale_offset`).
const QUANTIZED_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Draws one `TileWorld` with one `Atlas`.
pub struct TileRenderer {
    /// The pipelines ([`P`]), made when first used: a software renderer
    /// compiles each one, at a cost in memory, and a frame needs a few only.
    pipelines: [std::sync::OnceLock<wgpu::RenderPipeline>; P::COUNT],
    shader: wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
    quantized_float: bool,
    /// Selected once for numeric/performance A/B; unsupported devices keep three draws.
    subpixel_dual: bool,
    /// RGB555 cell specialization; generic shader remains available for paired A/B.
    tiles555: bool,
    layout: wgpu::BindGroupLayout,
    output_layout: wgpu::BindGroupLayout,
    params: [wgpu::Buffer; CONTROL_SLOTS],
    controls: ControlFrames,
    control_uploads: [ControlUpload; CONTROL_SLOTS],
    /// `OutputParams` of each copy (one slot per view, then the final one).
    output_params: [wgpu::Buffer; CONTROL_SLOTS],
    scratch: Option<Scratch>,
    smoke: std::cell::RefCell<Option<smoke::State>>,
    lens: std::cell::RefCell<Option<lens::State>>,
    glyph_cache: std::cell::RefCell<Option<glyph_cache::State>>,
    /// Decoding of the retained canvas, independent of later world uploads.
    picture_bits: u32,
    /// The pictures of zoomed views at zoom 1 (see `Camera::source_size`).
    sources: Option<Scratch>,
    /// The atlas on the GPU.
    atlas: Option<AtlasTextures>,
    atlas_transfer: Option<AtlasTransfer>,
    atlas_retired: Option<RetiredAtlas>,
    atlas_transfer_limit: NonZeroU64,
    /// The row of named palette 1 (`Atlas::named_palettes`).
    named_palettes: u32,
    sprites: Option<wgpu::Buffer>,
    /// The palette rows indexed sprites are drawn through (`Atlas::palettes`).
    palettes: Option<wgpu::Buffer>,
    world: Option<WorldBuffers>,
    bind_group: Option<[wgpu::BindGroup; CONTROL_SLOTS]>,
    /// Whether more views than drawn were asked for (warned once).
    too_many_views: bool,
    /// `tile_render_shared::CELL_INDICES`.
    cell_indices: wgpu::Buffer,
    /// GPU time per pass, when profiling (see [`TileRenderer::enable_timing`]).
    timer: Option<crate::timing::GpuTimer>,
    /// The markers (see [`TileRenderer::set_markers`]) and how they are
    /// drawn (runs from 0).
    markers: Vec<FreeSprite>,
    marker_sources: Vec<crate::PlacedSprite>,
    marker_runs: FreeRuns,
    marker_smoke_count: usize,
    /// Bumped whenever what the views draw changes (anything uploaded but
    /// markers): a frame of the same views, markers and tick is the same.
    generation: u64,
    host_tick_ms: u32,
    /// What the internal canvas shows (see `render_layered`), whether its
    /// overlay views are drawn on it, and a copy of it without them.
    drawn: Option<FrameKey>,
    overlaid: bool,
    base: Option<wgpu::Texture>,
    /// Whether the last frame showed the picture drawn before.
    reused: bool,
}

/// What a frame's views (but the overlay ones) were drawn from.
#[derive(Clone, Debug, PartialEq)]
struct FrameKey {
    generation: u64,
    /// The markers drawn in them.
    markers: Vec<FreeSprite>,
    marker_text: Vec<crate::PlacedSprite>,
    cameras: Vec<Camera>,
    frame: u32,
    host_frame: u32,
    clear: [u8; 4],
    size: (u32, u32),
}

/// Whether a frame may show the picture drawn before where nothing it
/// shows changed (`TILE_RENDER_REUSE=0`: every frame is drawn whole).
fn reuse_frames() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("TILE_RENDER_REUSE").map_or(true, |v| v != "0"))
}

/// An atlas being uploaded a page at a time ([`TileRenderer::stage_atlas`]).
pub struct StagedAtlas {
    textures: AtlasTextures,
    /// The pages still to write, the next last: (colour page, index).
    pending: Vec<(bool, u32)>,
}

/// Maximum explicit copy or aligned upload work in one runtime batch.
/// Implicit texture initialization and immediate live mutations are separate.
pub const ATLAS_TRANSFER_MAX_BATCH_BYTES: u64 = 4 << 20;
/// Peak headroom for the reusable upload buffer, queue staging, and the
/// bounded CPU scratch slice that prepares a texture or palette upload.
pub const ATLAS_TRANSFER_STAGING_RESERVE_BYTES: u64 = 3 * ATLAS_TRANSFER_MAX_BATCH_BYTES;

/// Pixels supplied by the caller while a runtime atlas transfer advances.
#[derive(Clone, Copy)]
pub enum AtlasTransferSource<'a> {
    Whole(&'a Atlas),
    Growth(&'a crate::AtlasGrowth),
    Reserve,
}

/// Why a runtime atlas transfer could not start or advance.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AtlasTransferError {
    #[error("another atlas allocation is uploading or retiring")]
    Busy,
    #[error("there is no active atlas to grow or reserve")]
    NoAtlas,
    #[error("no atlas transfer has started")]
    NoTransfer,
    #[error("the source does not match the pending atlas transfer")]
    SourceMismatch,
    #[error("atlas transfer needs {required} resident bytes, above the limit of {limit}")]
    BudgetExceeded { required: u64, limit: u64 },
    #[error("atlas {resource} needs {requested}, above the device limit of {limit}")]
    DeviceLimit { resource: &'static str, requested: u64, limit: u64 },
    #[error("the atlas transfer requires a different allocation; restart it")]
    Invalidated,
    #[error("the live palette prefix changed length; the pending growth needs a newly compiled source")]
    PaletteLayoutChanged,
    #[error("atlas transfer budget {provided} is below the next operation's minimum of {minimum} bytes")]
    BudgetTooSmall { minimum: u64, provided: u64 },
}

/// Explicit texture-copy payload and aligned upload staging bytes charged
/// by this call, and whether subsequent draws can use the queued result.
/// Driver allocation/initialization and immediate live mutations are excluded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtlasTransferProgress {
    pub bytes: u64,
    pub ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransferKind {
    Whole,
    Growth,
    Reserve,
}

#[derive(Clone, Copy, Debug, Default)]
struct PixelCursor {
    x: u32,
    y: u32,
}

impl PixelCursor {
    /// A contiguous rectangle costing no more than `budget` bytes. A
    /// partial row always has height one, so source strides add no padding.
    fn chunk(&self, width: u32, height: u32, texel_bytes: u32, budget: u64) -> Option<(u32, u32)> {
        let pixels = budget / u64::from(texel_bytes);
        if self.y >= height || width == 0 || pixels == 0 {
            return None;
        }
        if self.x == 0 && pixels >= u64::from(width) {
            Some((width, u64::from(height - self.y).min(pixels / u64::from(width)) as u32))
        } else {
            Some((u64::from(width - self.x).min(pixels) as u32, 1))
        }
    }

    /// CPU uploads use an explicit staging buffer with WebGPU's known
    /// 256-byte row alignment, rather than a backend-dependent queue pitch.
    fn staged_chunk(&self, width: u32, height: u32, texel_bytes: u32, budget: u64) -> Option<(u32, u32)> {
        let alignment = u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        if self.y >= height || width == 0 || budget < alignment { return None }
        let row = aligned_texture_row(width, texel_bytes);
        if self.x == 0 && row <= budget {
            Some((width, u64::from(height - self.y).min(budget / row) as u32))
        } else {
            let pixels = (budget / alignment * alignment) / u64::from(texel_bytes);
            Some((u64::from(width - self.x).min(pixels) as u32, 1))
        }
    }

    fn advance(&mut self, width: u32, w: u32, h: u32) {
        self.x += w;
        if self.x == width {
            self.x = 0;
            self.y += h;
        }
    }
}

#[derive(Clone, Copy)]
enum PixelSource {
    Copy,
    WholeRgba(usize),
    WholeIndex(usize),
    Blit(usize),
}

struct PixelTransfer {
    source: PixelSource,
    rgba: bool,
    page: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    cursor: PixelCursor,
}

impl PixelTransfer {
    fn copy(rgba: bool, page: u32, x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { source: PixelSource::Copy, rgba, page, x, y, width, height, cursor: PixelCursor::default() }
    }
}

struct AtlasTransfer {
    kind: TransferKind,
    textures: AtlasTextures,
    named_palettes: u32,
    /// Growth and reserve retain sprite ids. Full replacements do not.
    prefix_sprites: usize,
    palette_tail: Vec<u32>,
    palette_prefix: usize,
    replace_rgba: bool,
    tasks: std::collections::VecDeque<PixelTransfer>,
    upload_buffer: Option<wgpu::Buffer>,
    sprite_buffer: Option<wgpu::Buffer>,
    palette_buffer: Option<wgpu::Buffer>,
    sprite_cursor: u64,
    palette_cursor: u64,
    fence: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    palette_layout_changed: bool,
}

impl AtlasTransfer {
    fn ready(&self) -> bool {
        !self.palette_layout_changed
            && self.tasks.is_empty()
            && self.sprite_buffer.as_ref().is_none_or(|b| self.sprite_cursor == b.size())
            && self.palette_buffer.as_ref().is_none_or(|b| self.palette_cursor == b.size())
    }
}

struct RetiredAtlas {
    _textures: Option<AtlasTextures>,
    _sprites: Option<wgpu::Buffer>,
    _palettes: Option<wgpu::Buffer>,
    _bind_group: Option<[wgpu::BindGroup; CONTROL_SLOTS]>,
    _upload: Option<wgpu::Buffer>,
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

fn atlas_fence(queue: &wgpu::Queue) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let callback = done.clone();
    queue.on_submitted_work_done(move || callback.store(true, std::sync::atomic::Ordering::Release));
    done
}

fn atlas_transfer_limit() -> NonZeroU64 {
    let default_mib = if cfg!(target_pointer_width = "32") { 2048 } else { 4096 };
    let bytes = std::env::var("TILE_RENDER_ATLAS_PEAK_MIB")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .and_then(|mib| mib.checked_mul(1 << 20))
        .and_then(NonZeroU64::new);
    bytes.unwrap_or_else(|| NonZeroU64::new(default_mib << 20).unwrap())
}

fn aligned_texture_row(width: u32, texel_bytes: u32) -> u64 {
    let alignment = u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    (u64::from(width) * u64::from(texel_bytes)).div_ceil(alignment) * alignment
}

/// Only materialize the requested palette slice and its implicit zeros.
fn palette_slice(palettes: &[u32], offset: u64, bytes: u64) -> Vec<u32> {
    let first = offset as usize / 4;
    let mut words = vec![0; bytes as usize / 4];
    let count = words.len().min(palettes.len().saturating_sub(first));
    if count > 0 { words[..count].copy_from_slice(&palettes[first..first + count]); }
    words
}

fn atlas_storage(device: &wgpu::Device, label: &str, bytes: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn atlas_table_bytes(sprites: usize, palettes: usize) -> (u64, u64) {
    (sprites.max(1) as u64 * size_of::<SpriteDesc>() as u64, palettes.max(tile_render_shared::PALETTE_SIZE as usize) as u64 * 4)
}

fn array_bytes(size: u32, pages: u32, texel_bytes: u32) -> u64 {
    if pages == 0 { u64::from(texel_bytes) } else { u64::from(size) * u64::from(size) * u64::from(pages) * u64::from(texel_bytes) }
}

/// Prefer spare room, but never let an optional reservation prevent a
/// required growth. An oversized reservation falls back to required room.
fn transfer_room(
    size: u32,
    active: (u32, u32),
    required: (u32, u32),
    desired: (u32, u32),
    whole: bool,
    resident: u64,
    extra_bytes: u64,
    limit: u64,
) -> Result<(u32, u32), AtlasTransferError> {
    let peak = |room: (u32, u32)| {
        let rgba = if whole || room.0 > active.0 { array_bytes(size, room.0, 4) } else { 0 };
        let index = if whole || room.1 > active.1 { array_bytes(size, room.1, 1) } else { 0 };
        resident.saturating_add(extra_bytes).saturating_add(rgba).saturating_add(index)
    };
    if peak(desired) <= limit {
        return Ok(desired);
    }
    let needed = peak(required);
    if needed <= limit { Ok(required) } else { Err(AtlasTransferError::BudgetExceeded { required: needed, limit }) }
}

/// The atlas on the GPU: its colour and index pages, its sprites and
/// palette rows.
struct AtlasTextures {
    rgba: PageArray,
    index: PageArray,
    rgba_pages: u32,
    index_pages: u32,
    sprites: Vec<SpriteDesc>,
    palettes: Vec<u32>,
}

/// Atlas pages of one format: a texture array with room for `capacity`
/// pages of `size`² texels (none: a single texel, so that it can be bound).
#[derive(Clone)]
struct PageArray {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
    size: u32,
    capacity: u32,
}

impl PageArray {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, size: u32, capacity: u32) -> Self {
        let (side, layers) = if capacity == 0 { (1, 1) } else { (size, capacity) };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tile-render atlas"),
            size: wgpu::Extent3d { width: side, height: side, depth_or_array_layers: layers },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            // (Copied from when the atlas grows into a bigger one.)
            // R8Unorm and Rgba8Unorm are renderable. This also permits
            // wgpu to initialize untouched layers with attachment clears,
            // instead of copying a full layer from a zero-filled buffer.
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        Self { texture, view, format, size, capacity }
    }

    /// The same pages in a new array with room for `capacity` (at least as
    /// many as this one), copied on the GPU. The old array goes once the
    /// copy is done.
    fn grown(self, device: &wgpu::Device, queue: &wgpu::Queue, capacity: u32) -> Self {
        let new = Self::new(device, self.format, self.size, capacity.max(self.capacity));
        if self.capacity > 0 {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tile-render atlas growth") });
            let extent = wgpu::Extent3d { width: self.size, height: self.size, depth_or_array_layers: self.capacity };
            encoder.copy_texture_to_texture(self.texture.as_image_copy(), new.texture.as_image_copy(), extent);
            queue.submit([encoder.finish()]);
        }
        new
    }

    fn texel_bytes(&self) -> u32 {
        if self.format == wgpu::TextureFormat::R8Unorm { 1 } else { 4 }
    }

    fn bytes(&self) -> u64 {
        self.size as u64 * self.size as u64 * self.capacity as u64 * self.texel_bytes() as u64
    }

    fn write(&self, queue: &wgpu::Queue, page: u32, x: u32, y: u32, (w, h): (u32, u32), bytes: &[u8]) {
        if w == 0 || h == 0 || page >= self.capacity || x + w > self.size || y + h > self.size {
            return;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: page }, aspect: wgpu::TextureAspect::All },
            bytes,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * self.texel_bytes()), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }

}

/// An internal texture and the bind group reading it in the output pass.
struct Scratch {
    main: wgpu::Texture,
    output: [wgpu::BindGroup; CONTROL_SLOTS],
}

struct WorldBuffers {
    grid: Grid,
    color: ColorModel,
    passes: Vec<Pass>,
    /// How far per-cell offsets move sprites (`TileWorld::offset_reach`).
    reach: (u32, u32, u32, u32),
    /// Free sprites, ordered by the pass they follow, then the markers
    /// (see [`TileRenderer::set_markers`]), and per pass the range of them
    /// to draw after it (the last entry: after all passes).
    free: wgpu::Buffer,
    free_after: Vec<FreeRuns>,
    host_period_ms: u32,
    /// [`host_period`] of the viewer-clock layers drawing a cell
    /// ([`TileWorld::host_clock_layers`]).
    layer_period_ms: u32,
    smoke_count: usize,
    scene_smoke_count: usize,
    /// The world's free sprites as uploaded, and the runs of them after
    /// all passes (the markers' runs follow).
    scene_free: Vec<FreeSprite>,
    /// Runs before duplicate draws are removed. All instances stay in the
    /// buffer so mutable art can restore draws without allocating buffers.
    source_runs: Vec<FreeRuns>,
    last_runs: FreeRuns,
    /// The exact layer tables and sprite sets used by the GPU. Cell-only
    /// updates compare these, rather than just the padded record size.
    layer_specs: Vec<crate::LayerSpec>,
    sets_source: crate::SpriteSets,
    /// Per pass, whether its cells are drawn as octagons (`corner_cuts`).
    octagons: Vec<bool>,
    /// Text drawn from outlines (`tile_render_shared::text::TextGlyph`), in
    /// drawing order (`free_after`'s text runs index it).
    text: wgpu::Buffer,
    text_source: Vec<tile_render_shared::text::TextGlyph>,
    scene_text_len: usize,
    scene_has_text: bool,
    font_bases: Vec<u32>,
    has_text: bool,
    /// The fonts' outlines, and shared ownership of their source faces.
    /// Holding the faces makes pointer identity stable until replacement.
    outlines: wgpu::Buffer,
    fonts: Vec<std::sync::Arc<gpu_text::Face>>,
    /// Per zoom with zoomed sprites, the first pass they follow.
    first_zoomed: Vec<crate::world::ZoomedStart>,
    scene_first_zoomed: Vec<crate::world::ZoomedStart>,
    /// The layer tables as uploaded (`TileWorld::layer_tables`): the
    /// shared one, then one per binding; and where each binding's starts.
    layer_table: Vec<LayerDesc>,
    tables: Vec<(u32, u32)>,
    layers: wgpu::Buffer,
    /// The cell data: the layers' records, then the data slots'. With room
    /// to grow, so that a slot allocated writes only its records.
    tile_data: wgpu::Buffer,
    lut: wgpu::Buffer,
    sets: wgpu::Buffer,
}

impl WorldBuffers {
    fn same_shape(&self, world: &TileWorld) -> bool {
        self.grid == world.grid && self.color == world.color && self.sets_source.words() == world.sets.words()
            && self.layer_specs.len() == world.layers().len()
            && self.layer_specs.iter().zip(world.layers()).all(|(a, b)| {
                a.lut == b.lut && a.per_cell_phase == b.per_cell_phase && a.per_cell_offset == b.per_cell_offset
                    && a.offset == b.offset && a.ticks_per_frame == b.ticks_per_frame && a.hidden == b.hidden
                    && a.pass == b.pass && a.descending == b.descending && a.cull_offset == b.cull_offset
                    && a.blend == b.blend && a.palette_bits == b.palette_bits && a.index_bits == b.index_bits && a.category == b.category
            })
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn texture_array_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}

fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

impl TileRenderer {
    /// `target_format` should be a non-sRGB format (e.g. `Rgba8Unorm`,
    /// `Bgra8Unorm`): atlas colours are written unchanged, which is what
    /// makes the output byte-identical to the source art.
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let subpixel_dual = device.features().contains(wgpu::Features::DUAL_SOURCE_BLENDING)
            && std::env::var("TILE_RENDER_SUBPIXEL_DUAL").as_deref() != Ok("0");
        let tiles555 = std::env::var("TILE_RENDER_TILES555").as_deref() != Ok("0");
        let adapter = device.adapter_info();
        let quantized_float = adapter.backend == wgpu::Backend::Dx12 && adapter.device_type == wgpu::DeviceType::Cpu
            && adapter.vendor == 0x1414 && adapter.device == 0x8c && adapter.name == "Microsoft Basic Render Driver";
        if quantized_float { log::info!("integer FP16 composition for the measured Microsoft software adapter"); }
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tile-render shaders"),
            source: wgpu::util::make_spirv(if device.features().contains(wgpu::Features::DUAL_SOURCE_BLENDING) { SPV } else { FALLBACK_SPV }),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tile-render"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: NonZeroU64::new(size_of::<FrameParams>() as u64),
                    },
                    count: None,
                },
                storage_entry(1),
                storage_entry(2),
                storage_entry(3),
                storage_entry(4),
                storage_entry(5),
                storage_entry(7),
                storage_entry(8),
                wgpu::BindGroupLayoutEntry { visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ..storage_entry(9) },
                wgpu::BindGroupLayoutEntry { visibility: wgpu::ShaderStages::FRAGMENT, ..storage_entry(10) },
                wgpu::BindGroupLayoutEntry { binding: 11, ..texture_array_entry(6) },
                texture_array_entry(6),
            ],
        });
        let output_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tile-render output"),
            entries: &[
                texture_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: NonZeroU64::new(size_of::<OutputParams>() as u64),
                    },
                    count: None,
                },
            ],
        });
        let output_params = std::array::from_fn(|_| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-render output params"),
            size: PARAMS_STRIDE * (MAX_VIEWS + 1 + MAX_COPIES) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        let params = std::array::from_fn(|_| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-render params"),
            size: PARAMS_STRIDE * (VIEW_SLOTS * MAX_VIEWS) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        Self {
            pipelines: std::array::from_fn(|_| std::sync::OnceLock::new()),
            shader,
            target_format,
            quantized_float,
            subpixel_dual,
            tiles555,
            layout,
            output_layout,
            params,
            controls: ControlFrames::default(),
            control_uploads: std::array::from_fn(|_| ControlUpload::default()),
            output_params,
            scratch: None,
            smoke: Default::default(),
            lens: Default::default(),
            glyph_cache: Default::default(),
            picture_bits: 0,
            sources: None,
            atlas: None,
            atlas_transfer: None,
            atlas_retired: None,
            atlas_transfer_limit: atlas_transfer_limit(),
            named_palettes: 1,
            sprites: None,
            palettes: None,
            world: None,
            bind_group: None,
            too_many_views: false,
            cell_indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("tile-render cell indices"),
                contents: bytemuck::cast_slice(&tile_render_shared::CELL_INDICES),
                usage: wgpu::BufferUsages::INDEX,
            }),
            timer: None,
            markers: Vec::new(),
            marker_sources: Vec::new(),
            marker_runs: Vec::new(),
            marker_smoke_count: 0,
            generation: 0,
            host_tick_ms: 0,
            drawn: None,
            overlaid: false,
            base: None,
            reused: false,
        }
    }

    /// Whether another immutable control snapshot fits before submission.
    /// Latest scene/input updates can remain pending when all three are full.
    pub fn control_frame_available(&self) -> bool {
        self.controls.prepared.get() < CONTROL_SLOTS
    }

    /// GPU coverage cache diagnostics for numeric and retention checks.
    pub fn glyph_cache_stats(&self) -> GlyphCacheStats {
        self.glyph_cache.borrow().as_ref().map_or_else(GlyphCacheStats::default, glyph_cache::State::stats)
    }

    /// Call immediately after submitting all commands encoded with this
    /// renderer. This recycles control slots at the CPU submission boundary,
    /// without waiting for GPU completion. Queue ordering places subsequent
    /// writes after the preceding submitted draws. Never call before submit.
    /// If omitted, a fourth frame is refused instead of overwriting controls.
    ///
    /// These slots snapshot controls, not mutable world/atlas storage. Submit
    /// already encoded draws before calling queue-writing updates such as
    /// `update_free`, `update_cells`, `set_markers` or `patch_sprite`: a write
    /// queued before submission would also affect those earlier draws.
    pub fn controls_submitted(&self) {
        self.controls.prepared.set(0);
        if let Some(cache) = self.glyph_cache.borrow_mut().as_mut() { cache.submitted(); }
    }

    /// The last prepared snapshot's upload bytes and queue-write calls.
    /// Controls are assembled on the CPU and uploaded once per nonempty
    /// uniform buffer, rather than once per camera/pass/copy.
    pub fn control_upload_stats(&self) -> (usize, usize) {
        let upload = &self.control_uploads[self.controls.active.get()];
        let lengths = [&upload.params, &upload.output].map(|u| u.bytes.borrow().len().saturating_sub(u.start.get()));
        (lengths.iter().sum(), lengths.iter().filter(|&&n| n != 0).count())
    }

    fn claim_control_frame(&self) -> Option<usize> {
        let slot = self.controls.claim()?;
        if let Some(cache) = self.glyph_cache.borrow_mut().as_mut() { cache.begin(slot); }
        for upload in [&self.control_uploads[slot].params, &self.control_uploads[slot].output] {
            upload.bytes.borrow_mut().clear();
            upload.start.set(usize::MAX);
        }
        Some(slot)
    }

    fn flush_controls(&self, queue: &wgpu::Queue, slot: usize) {
        if let Some(cache) = self.glyph_cache.borrow().as_ref() { cache.flush(queue, slot); }
        if let Some(smoke) = self.smoke.borrow().as_ref() { smoke.flush(queue, slot); }
        let upload = &self.control_uploads[slot];
        for (buffer, upload) in [(&self.params[slot], &upload.params), (&self.output_params[slot], &upload.output)] {
            let bytes = upload.bytes.borrow();
            let start = upload.start.get();
            if start < bytes.len() { queue.write_buffer(buffer, start as u64, &bytes[start..]); }
        }
    }

    /// Times every render pass on the GPU from now on (where the device has
    /// timestamp queries); whether it does. Each frame's encoder is then
    /// ended with the timer's `finish` (see [`crate::timing::GpuTimer`]).
    pub fn enable_timing(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        self.timer = crate::timing::GpuTimer::new(device, queue);
        self.timer.is_some()
    }

    /// Stops recording timing queries; pending readbacks are discarded.
    pub fn disable_timing(&mut self) {
        self.timer = None;
    }

    /// Whether the last frame showed the picture drawn before (only its
    /// overlays drawn anew, see [`TileRenderer::render_layered`]).
    pub fn reused(&self) -> bool {
        self.reused
    }

    /// The size of the picture frames are composed in before they are shown
    /// on the target (`Present::canvas`), once one has been: what the views
    /// are rendered at. (None: none yet, or every frame drawn straight
    /// into the target.)
    pub fn composed_size(&self) -> Option<(u32, u32)> {
        self.scratch.as_ref().map(|s| (s.main.width(), s.main.height()))
    }

    /// The GPU timer, if timing is on.
    pub fn timer(&mut self) -> Option<&mut crate::timing::GpuTimer> {
        self.timer.as_mut()
    }

    /// The timestamp writes of a pass, when timing.
    fn timed(&self, label: impl FnOnce() -> String) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        self.timer.as_ref().and_then(|t| t.pass(label))
    }

    /// Pipeline `p`, made the first time it is asked for.
    fn pipeline(&self, device: &wgpu::Device, p: P) -> &wgpu::RenderPipeline {
        self.pipelines[p.index()].get_or_init(|| {
            let alpha = Some(wgpu::BlendState::ALPHA_BLENDING);
            let premultiplied = Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
            // What is there subtracted from the shader's maximum; alpha kept.
            let invert = Some(wgpu::BlendState {
                color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Subtract },
                alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            });
            let (tiles, free, copy) = (("tile_vs", "tile_fs"), ("free_vs", "tile_fs"), ("fullscreen_vs", "output_fs"));
            let all = wgpu::ColorWrites::ALL;
            let (target, internal) = (self.target_format, QUANTIZED_FORMAT);
            let quantized = self.quantized_format();
            let (stages, layout, format, blend, mask) = match p {
                P::NativePaletteAlpha(into) => {
                    let format = [target, quantized, internal][into as usize];
                    (("free_vs", "native_palette_alpha_fs"), &self.layout, format, None, wgpu::ColorWrites::RED | wgpu::ColorWrites::GREEN | wgpu::ColorWrites::BLUE)
                }
                P::Tiles => (tiles, &self.layout, target, alpha, all),
                P::Free => (free, &self.layout, target, alpha, all),
                P::TilesQuantized => (tiles, &self.layout, quantized, premultiplied, all),
                P::Tiles555 => (("tile_vs", "tile555_fs"), &self.layout, quantized, premultiplied, all),
                P::FreeQuantized => (free, &self.layout, quantized, premultiplied, all),
                P::TilesSource => (tiles, &self.layout, internal, alpha, all),
                P::FreeSource => (free, &self.layout, internal, alpha, all),
                P::FreeInvert => (free, &self.layout, target, invert, all),
                P::FreeInvertInternal => (free, &self.layout, internal, invert, all),
                P::FreeInvertQuantized => (free, &self.layout, quantized, invert, all),
                P::Subpixel(into, channel) => {
                    let (fs, mask) = [("subpixel_r_fs", wgpu::ColorWrites::RED), ("subpixel_g_fs", wgpu::ColorWrites::GREEN), ("subpixel_b_fs", wgpu::ColorWrites::BLUE)][channel as usize];
                    let (format, blend) = [(target, alpha), (quantized, premultiplied), (internal, alpha)][into as usize];
                    (("free_vs", fs), &self.layout, format, blend, mask)
                }
                P::Text(into, channel) => {
                    let (fs, mask) = [("text_r_fs", wgpu::ColorWrites::RED), ("text_g_fs", wgpu::ColorWrites::GREEN), ("text_b_fs", wgpu::ColorWrites::BLUE)][channel as usize];
                    let (format, blend) = [(target, alpha), (quantized, premultiplied), (internal, alpha)][into as usize];
                    (("text_vs", fs), &self.layout, format, blend, mask)
                }
                P::TextDual(into) | P::SubpixelDual(into) => {
                    let format = [target, quantized, internal][into as usize];
                    let blend = Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: if into == 1 { wgpu::BlendFactor::One } else { wgpu::BlendFactor::Src1 },
                            dst_factor: wgpu::BlendFactor::OneMinusSrc1,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    });
                    let stages = if matches!(p, P::SubpixelDual(_)) { ("free_vs", "subpixel_dual_fs") } else { ("text_vs", "text_dual_fs") };
                    (stages, &self.layout, format, blend,
                        wgpu::ColorWrites::RED | wgpu::ColorWrites::GREEN | wgpu::ColorWrites::BLUE)
                }
                P::TextKeyed(into) => {
                    let (format, blend) = [(target, alpha), (quantized, premultiplied), (internal, alpha)][into as usize];
                    (("text_vs", "text_keyed_fs"), &self.layout, format, blend, all)
                }
                P::Output => (copy, &self.output_layout, target, None, all),
                P::Blit => (copy, &self.output_layout, internal, None, all),
                P::BlitQuantized => (copy, &self.output_layout, quantized, None, all),
            };
            let name = format!("tile-render {p:?}");
            let ordinary_layouts = [Some(layout)];
            let native_layouts = [Some(layout), Some(&self.output_layout)];
            let cache = self.glyph_cache.borrow();
            let text_layouts = cache.as_ref().map(|cache| [Some(layout), Some(&cache.layout)]);
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(&name),
                bind_group_layouts: if matches!(p, P::NativePaletteAlpha(_)) { &native_layouts } else if matches!(p, P::Text(..) | P::TextDual(..) | P::TextKeyed(..)) { text_layouts.as_ref().expect("text cache prepared") } else { &ordinary_layouts },
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&name),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState { module: &self.shader, entry_point: Some(stages.0), compilation_options: Default::default(), buffers: &[] },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some(stages.1),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: mask })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })
    }

    fn quantized_format(&self) -> wgpu::TextureFormat {
        if self.quantized_float { wgpu::TextureFormat::Rgba16Float } else { QUANTIZED_FORMAT }
    }

    /// The pipelines free sprites are drawn with into `into` (0 the
    /// caller's target, 1 the quantized internal one, 2 a direct internal
    /// one), made as runs need them.
    fn free_pipelines<'a>(&'a self, device: &'a wgpu::Device, into: u8) -> FreePipelines<'a> {
        FreePipelines { renderer: self, device, into }
    }

    /// Replaces the pixels of atlas sprite `sprite` (sprites whose content
    /// changes at run time, e.g. a minimap). `image` must have the sprite's
    /// size; returns whether it was written. Live mutations and their
    /// candidate mirrors are immediate and outside the background-art
    /// transfer allowance; callers control their frequency and image size.
    pub fn patch_sprite(&mut self, queue: &wgpu::Queue, sprite: crate::SpriteId, image: &crate::RgbaImage) -> bool {
        let Some(a) = &mut self.atlas else { return false };
        let Some(s) = a.sprites.get(sprite.0 as usize).copied() else { return false };
        if (s.width, s.height) != (image.width, image.height) || s.width == 0 || s.height == 0 || s.palette != 0 {
            return false;
        }
        self.generation += 1;
        a.rgba.write(queue, s.page, s.atlas_x, s.atlas_y, (image.width, image.height), image.as_bytes());
        let changed = crate::atlas::patch_bounds(&mut a.sprites, &s, image);
        if let Some(buffer) = &self.sprites {
            for i in &changed {
                queue.write_buffer(buffer, (*i * size_of::<SpriteDesc>()) as u64, bytemuck::bytes_of(&a.sprites[*i]));
            }
        }
        if let Some(job) = self.atlas_transfer.as_mut().filter(|j| j.kind != TransferKind::Whole) {
            // Live patches already upload immediately. Mirror them outside
            // the background-art allowance so continuous updates cannot
            // restart a bounded replay forever. Queue order preserves both
            // already-copied regions and regions copied on a later frame.
            if job.replace_rgba {
                job.textures.rgba.write(queue, s.page, s.atlas_x, s.atlas_y, (image.width, image.height), image.as_bytes());
            }
            if job.kind == TransferKind::Growth {
                for i in crate::atlas::patch_bounds(&mut job.textures.sprites, &s, image) {
                    if let Some(buffer) = &job.sprite_buffer {
                        queue.write_buffer(buffer, (i * size_of::<SpriteDesc>()) as u64, bytemuck::bytes_of(&job.textures.sprites[i]));
                    }
                }
            }
        }
        if !changed.is_empty() {
            self.refresh_free_runs();
        }
        true
    }

    /// Uploads the atlas pages (colour and index), sprite and palette tables.
    pub fn set_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, atlas: &Atlas) {
        self.finish_atlas_transfer_sync(device, queue);
        self.generation += 1;
        // Release the old atlas first: two at once may not fit (32-bit
        // processes, software renderers).
        self.bind_group = None;
        self.atlas = None;
        self.sprites = None;
        self.palettes = None;
        // Dropped resources are only freed once the GPU is done with them:
        // wait for that, so the old atlas is really gone.
        if let Err(e) = device.poll(wgpu::PollType::wait_indefinitely()) {
            log::warn!("waiting for the GPU before a new atlas: {e}");
        }
        // (A 1×1 page stands for none.)
        let rgba_pages = atlas.pages.iter().filter(|p| p.width == atlas.page_size).count() as u32;
        let index_pages = atlas.index_pages.len() as u32;
        let textures = AtlasTextures {
            rgba: PageArray::new(device, wgpu::TextureFormat::Rgba8Unorm, atlas.page_size, rgba_pages),
            index: PageArray::new(device, wgpu::TextureFormat::R8Unorm, atlas.page_size, index_pages),
            rgba_pages,
            index_pages,
            sprites: atlas.sprites.clone(),
            palettes: atlas.palettes.clone(),
        };
        for (i, page) in atlas.pages.iter().enumerate().filter(|(_, p)| p.width == atlas.page_size) {
            textures.rgba.write(queue, i as u32, 0, 0, (page.width, page.height), page.as_bytes());
        }
        for (i, page) in atlas.index_pages.iter().enumerate() {
            textures.index.write(queue, i as u32, 0, 0, (page.width, page.height), &page.pixels);
        }
        self.atlas = Some(textures);
        self.named_palettes = atlas.named_palettes;
        let empty = [SpriteDesc::default()];
        let sprites: &[SpriteDesc] = if atlas.sprites.is_empty() { &empty } else { &atlas.sprites };
        self.sprites = Some(storage(device, "tile-render sprites", bytemuck::cast_slice(sprites)));
        self.palettes = Some(storage(device, "tile-render palettes", bytemuck::cast_slice(&palette_words(&atlas.palettes))));
        self.refresh_free_runs();
        self.rebuild_bind_group(device);
    }

    /// Appends sprites to the atlas (`growth`, made for the atlas as
    /// uploaded so far): only the new pixels are written, and the new
    /// palette rows. Pages without room in their texture array go into a
    /// bigger one, the old pages copied on the GPU, with room for half as
    /// many again (see [`TileRenderer::reserve_atlas`]). Without an atlas,
    /// does nothing.
    pub fn grow_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, growth: &crate::AtlasGrowth) {
        self.finish_atlas_transfer_sync(device, queue);
        self.generation += 1;
        let Some(mut a) = self.atlas.take() else { return };
        if growth.first as usize != a.sprites.len() {
            log::error!("atlas growth for {} sprites, the atlas has {}", growth.first, a.sprites.len());
            render_errors::report(render_errors::Kind::Atlas, "a growth for another sprite count", format_args!("growth for {} sprites, the atlas has {}", growth.first, a.sprites.len()));
        }
        let max = device.limits().max_texture_array_layers;
        if growth.pages.max(growth.index_pages) > max {
            log::error!("atlas: {} colour and {} index pages, the device has room for {max} of each: sprites past them are not drawn", growth.pages, growth.index_pages);
            render_errors::report(render_errors::Kind::Atlas, "more pages than the device has", format_args!("{} colour and {} index pages, room for {max}", growth.pages, growth.index_pages));
        }
        let room = |pages: u32| (pages + (pages / 2).max(4)).min(max);
        let mut resized = false;
        if growth.pages > a.rgba.capacity {
            a.rgba = a.rgba.grown(device, queue, room(growth.pages));
            resized = true;
        }
        if growth.index_pages > a.index.capacity {
            a.index = a.index.grown(device, queue, room(growth.index_pages));
            resized = true;
        }
        if resized {
            // (The old arrays are gone once the copies are done.)
            self.bind_group = None;
            if let Err(e) = device.poll(wgpu::PollType::wait_indefinitely()) {
                log::warn!("waiting for the GPU after growing the atlas: {e}");
            }
            log::info!("atlas: room for {} colour and {} index pages", a.rgba.capacity, a.index.capacity);
        }
        for b in &growth.blits {
            match &b.pixels {
                crate::BlitPixels::Rgba(i) => a.rgba.write(queue, b.page, b.x, b.y, (i.width, i.height), i.as_bytes()),
                crate::BlitPixels::Index(i) => a.index.write(queue, b.page, b.x, b.y, (i.width, i.height), &i.pixels),
            }
        }
        a.rgba_pages = growth.pages.min(a.rgba.capacity);
        a.index_pages = growth.index_pages.min(a.index.capacity);
        let old_sprites = a.sprites.len();
        a.sprites.extend_from_slice(&growth.sprites);
        if !growth.palettes.is_empty() {
            let mut palettes = a.palettes.clone();
            palettes.extend_from_slice(&growth.palettes);
            crate::atlas::palette_bounds(&mut a.sprites[..old_sprites], &a.palettes, &palettes, self.named_palettes);
            a.palettes = palettes;
            self.palettes = Some(storage(device, "tile-render palettes", bytemuck::cast_slice(&palette_words(&a.palettes))));
        }
        // Palette growth may change existing descriptors' alpha flags.
        let empty = [SpriteDesc::default()];
        let sprites: &[SpriteDesc] = if a.sprites.is_empty() { &empty } else { &a.sprites };
        self.sprites = Some(storage(device, "tile-render sprites", bytemuck::cast_slice(sprites)));
        self.atlas = Some(a);
        self.refresh_free_runs();
        self.rebuild_bind_group(device);
    }

    /// Makes room in the atlas for `pages` colour and `index_pages` index
    /// pages (e.g. as many as art expected soon will need), so that growing
    /// it that far does not move it again.
    pub fn reserve_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pages: u32, index_pages: u32) {
        self.finish_atlas_transfer_sync(device, queue);
        let max = device.limits().max_texture_array_layers;
        let Some(a) = &mut self.atlas else { return };
        if pages.min(max) <= a.rgba.capacity && index_pages.min(max) <= a.index.capacity {
            return;
        }
        let Some(mut a) = self.atlas.take() else { return };
        if pages.min(max) > a.rgba.capacity {
            a.rgba = a.rgba.grown(device, queue, pages.min(max));
        }
        if index_pages.min(max) > a.index.capacity {
            a.index = a.index.grown(device, queue, index_pages.min(max));
        }
        self.bind_group = None;
        if let Err(e) = device.poll(wgpu::PollType::wait_indefinitely()) {
            log::warn!("waiting for the GPU after making room in the atlas: {e}");
        }
        self.atlas = Some(a);
        self.rebuild_bind_group(device);
    }

    /// Pages the atlas has room for: colour pages, index pages.
    pub fn atlas_room(&self) -> (u32, u32) {
        self.atlas.as_ref().map_or((0, 0), |a| (a.rgba.capacity, a.index.capacity))
    }

    /// An atlas-only preparer sharing immutable programs and draw uniforms.
    /// Use its transfer API, then `adopt_prepared_atlas`; do not give it a
    /// world or render with it. No shader compilation or GPU allocation is
    /// needed to construct it. Its budget must exclude other live atlases.
    pub fn atlas_preparer(&self) -> Self {
        Self {
            pipelines: std::array::from_fn(|_| std::sync::OnceLock::new()),
            shader: self.shader.clone(),
            target_format: self.target_format,
            quantized_float: self.quantized_float,
            subpixel_dual: self.subpixel_dual,
            tiles555: self.tiles555,
            layout: self.layout.clone(),
            output_layout: self.output_layout.clone(),
            params: self.params.clone(),
            controls: ControlFrames::default(),
            control_uploads: std::array::from_fn(|_| ControlUpload::default()),
            output_params: self.output_params.clone(),
            scratch: None,
            smoke: Default::default(),
            lens: Default::default(),
            glyph_cache: Default::default(),
            picture_bits: 0,
            sources: None,
            atlas: None,
            atlas_transfer: None,
            atlas_retired: None,
            atlas_transfer_limit: self.atlas_transfer_limit,
            named_palettes: 1,
            sprites: None,
            palettes: None,
            world: None,
            bind_group: None,
            too_many_views: false,
            cell_indices: self.cell_indices.clone(),
            timer: None,
            markers: Vec::new(),
            marker_sources: Vec::new(),
            marker_runs: Vec::new(),
            marker_smoke_count: 0,
            generation: 0,
            host_tick_ms: 0,
            drawn: None,
            overlaid: false,
            base: None,
            reused: false,
        }
    }

    /// The configured combined allocation ceiling. A preparer receives the
    /// remainder after subtracting the renderer that is still being drawn.
    pub fn atlas_transfer_limit(&self) -> NonZeroU64 {
        self.atlas_transfer_limit
    }

    /// Resident art and composition textures, including candidate/retirement
    /// resources and reserved upload workspaces. Shared arrays and buffers
    /// are counted once. Driver metadata and live patch uploads are separate.
    pub fn atlas_resident_bytes(&self) -> u64 {
        let mut arrays: Vec<&PageArray> = Vec::with_capacity(6);
        let mut buffers: Vec<&wgpu::Buffer> = Vec::with_capacity(9);
        if let Some(a) = &self.atlas { arrays.extend([&a.rgba, &a.index]); }
        buffers.extend(self.sprites.iter());
        buffers.extend(self.palettes.iter());
        let mut workspaces = 0;
        if let Some(job) = &self.atlas_transfer {
            arrays.extend([&job.textures.rgba, &job.textures.index]);
            buffers.extend(job.sprite_buffer.iter());
            buffers.extend(job.palette_buffer.iter());
            buffers.extend(job.upload_buffer.iter());
            if job.upload_buffer.is_some() { workspaces += 2 * ATLAS_TRANSFER_MAX_BATCH_BYTES; }
        }
        if let Some(old) = &self.atlas_retired {
            if let Some(a) = &old._textures { arrays.extend([&a.rgba, &a.index]); }
            buffers.extend(old._sprites.iter());
            buffers.extend(old._palettes.iter());
            buffers.extend(old._upload.iter());
            if old._upload.is_some() { workspaces += 2 * ATLAS_TRANSFER_MAX_BATCH_BYTES; }
        }
        let mut bytes = workspaces + self.smoke.borrow().as_ref().map_or(0, smoke::State::bytes)
            + self.glyph_cache.borrow().as_ref().map_or(0, glyph_cache::State::bytes)
            + self.lens.borrow().as_ref().map_or(0, lens::State::bytes);
        for texture in self.scratch.iter().map(|s| &s.main).chain(self.sources.iter().map(|s| &s.main)).chain(self.base.iter()) {
            let texel_bytes = if texture.format() == wgpu::TextureFormat::Rgba16Float { 8 } else { 4 };
            bytes += u64::from(texture.width()) * u64::from(texture.height()) * texel_bytes;
        }
        for (i, a) in arrays.iter().enumerate() {
            if !arrays[..i].iter().any(|b| a.texture == b.texture) {
                bytes += array_bytes(a.size, a.capacity, a.texel_bytes());
            }
        }
        for (i, b) in buffers.iter().enumerate() {
            if !buffers[..i].contains(b) { bytes += b.size(); }
        }
        bytes
    }

    /// Installs prepared art only after its uploads and retirements are
    /// complete. Programs, worlds and caches stay with the drawing renderer.
    /// The caller must install a scene compatible with this art before its
    /// next draw. Until then, the donor never replaces the drawable atlas.
    pub fn adopt_prepared_atlas(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &mut Self,
    ) -> Result<(), AtlasTransferError> {
        self.poll_atlas_transfers(device);
        prepared.poll_atlas_transfers(device);
        if !self.atlas_transfer_idle() || !prepared.atlas_transfer_idle() {
            return Err(AtlasTransferError::Busy);
        }
        if prepared.world.is_some() { return Err(AtlasTransferError::SourceMismatch) }
        if prepared.atlas.is_none() { return Err(AtlasTransferError::NoAtlas) }
        let old_resident = self.atlas_resident_bytes();
        let required = old_resident.saturating_add(prepared.atlas_resident_bytes());
        if required > self.atlas_transfer_limit.get() {
            return Err(AtlasTransferError::BudgetExceeded { required, limit: self.atlas_transfer_limit.get() });
        }
        let old = self.atlas.replace(prepared.atlas.take().unwrap());
        let old_sprites = std::mem::replace(&mut self.sprites, prepared.sprites.take());
        let old_palettes = std::mem::replace(&mut self.palettes, prepared.palettes.take());
        let old_bind_group = self.bind_group.take();
        self.named_palettes = prepared.named_palettes;
        self.generation += 1;
        if old.is_some() || old_sprites.is_some() || old_palettes.is_some() || old_bind_group.is_some() {
            queue.submit([]);
            self.atlas_retired = Some(RetiredAtlas {
                _textures: old, _sprites: old_sprites, _palettes: old_palettes,
                _bind_group: old_bind_group, _upload: None, done: atlas_fence(queue),
            });
        }
        self.refresh_free_runs();
        self.rebuild_bind_group(device);
        Ok(())
    }

    /// Sets the finite peak allocation limit for runtime atlas transfers.
    /// Existing allocations remain valid; the limit applies to new ones.
    pub fn set_atlas_transfer_limit(&mut self, bytes: NonZeroU64) {
        self.atlas_transfer_limit = bytes;
    }

    /// Services completion callbacks without waiting for GPU work.
    pub fn poll_atlas_transfers(&mut self, device: &wgpu::Device) {
        // Idle frames have no atlas callbacks to service. Completed flags
        // can also be reaped after another subsystem polled this device.
        let pending = self.atlas_retired.as_ref().is_some_and(|r| !r.done.load(std::sync::atomic::Ordering::Acquire))
            || self.atlas_transfer.as_ref().and_then(|j| j.fence.as_ref()).is_some_and(|f| !f.load(std::sync::atomic::Ordering::Acquire));
        if pending { let _ = device.poll(wgpu::PollType::Poll); }
        if self.atlas_retired.as_ref().is_some_and(|r| r.done.load(std::sync::atomic::Ordering::Acquire)) {
            self.atlas_retired = None;
        }
    }

    /// Another candidate can be allocated only after retirement finishes.
    pub fn atlas_transfer_idle(&self) -> bool {
        self.atlas_transfer.is_none() && self.atlas_retired.is_none()
    }

    /// Allocates one candidate atlas while retaining the atlas being drawn.
    /// `reserve` is optional spare capacity. If it exceeds the peak memory
    /// limit, required capacity is tried instead. Non-reserve transfers also
    /// reserve 12 MiB for bounded upload-buffer, queue-staging, and CPU-scratch
    /// workspaces. Immediate live mutations remain outside that allowance.
    /// The default peak limit is
    /// 2 GiB on 32-bit targets and 4 GiB otherwise, configurable with
    /// `TILE_RENDER_ATLAS_PEAK_MIB` or `set_atlas_transfer_limit`.
    pub fn begin_atlas_transfer(
        &mut self,
        device: &wgpu::Device,
        source: AtlasTransferSource<'_>,
        reserve: (u32, u32),
    ) -> Result<(), AtlasTransferError> {
        self.begin_atlas_transfer_with_capacity(device, source, (0, 0), reserve)
    }

    /// Like `begin_atlas_transfer`, with mandatory capacity for known art
    /// that will follow this source. Optional spare room may be reduced;
    /// `minimum_room` never is. This lets a replacement and its required
    /// growths share one allocation while the previous atlas remains live.
    pub fn begin_atlas_transfer_with_capacity(
        &mut self,
        device: &wgpu::Device,
        source: AtlasTransferSource<'_>,
        minimum_room: (u32, u32),
        reserve: (u32, u32),
    ) -> Result<(), AtlasTransferError> {
        self.poll_atlas_transfers(device);
        if !self.atlas_transfer_idle() {
            return Err(AtlasTransferError::Busy);
        }
        let active = self.atlas.as_ref();
        let active_room = active.map_or((0, 0), |a| (a.rgba.capacity, a.index.capacity));
        let resident = self.atlas_resident_bytes();
        let (kind, size, used, sprites, palettes, named_palettes, prefix_sprites, palette_tail) = match source {
            AtlasTransferSource::Whole(a) => {
                let pages = a.pages.iter().filter(|p| p.width == a.page_size).count() as u32;
                if pages as usize != a.pages.len() && !(a.pages.len() == 1 && a.pages[0].width == 1 && a.pages[0].height == 1) {
                    return Err(AtlasTransferError::SourceMismatch);
                }
                for page in a.pages.iter().filter(|p| p.width == a.page_size) {
                    if page.height != a.page_size || page.pixels.len() as u64 != u64::from(page.width) * u64::from(page.height) {
                        return Err(AtlasTransferError::SourceMismatch);
                    }
                }
                for page in &a.index_pages {
                    if (page.width, page.height) != (a.page_size, a.page_size)
                        || page.pixels.len() as u64 != u64::from(page.width) * u64::from(page.height) {
                        return Err(AtlasTransferError::SourceMismatch);
                    }
                }
                (TransferKind::Whole, a.page_size, (pages, a.index_pages.len() as u32), a.sprites.clone(), a.palettes.clone(), a.named_palettes, 0, Vec::new())
            }
            AtlasTransferSource::Growth(g) => {
                let a = active.ok_or(AtlasTransferError::NoAtlas)?;
                if g.first as usize != a.sprites.len() || g.pages < a.rgba_pages || g.index_pages < a.index_pages {
                    return Err(AtlasTransferError::SourceMismatch);
                }
                let mut sprites = a.sprites.clone();
                let mut palettes = a.palettes.clone();
                palettes.extend_from_slice(&g.palettes);
                crate::atlas::palette_bounds(&mut sprites, &a.palettes, &palettes, self.named_palettes);
                sprites.extend_from_slice(&g.sprites);
                for b in &g.blits {
                    let (w, h, len, pages) = match &b.pixels {
                        crate::BlitPixels::Rgba(i) => (i.width, i.height, i.pixels.len() as u64, g.pages),
                        crate::BlitPixels::Index(i) => (i.width, i.height, i.pixels.len() as u64, g.index_pages),
                    };
                    if b.page >= pages || b.x.checked_add(w).is_none_or(|r| r > a.rgba.size)
                        || b.y.checked_add(h).is_none_or(|r| r > a.rgba.size) || len != u64::from(w) * u64::from(h) {
                        return Err(AtlasTransferError::SourceMismatch);
                    }
                }
                (TransferKind::Growth, a.rgba.size, (g.pages, g.index_pages), sprites, palettes, self.named_palettes, a.sprites.len(), g.palettes.clone())
            }
            AtlasTransferSource::Reserve => {
                let a = active.ok_or(AtlasTransferError::NoAtlas)?;
                (TransferKind::Reserve, a.rgba.size, (a.rgba_pages, a.index_pages), a.sprites.clone(), a.palettes.clone(), self.named_palettes, a.sprites.len(), Vec::new())
            }
        };
        let limits = device.limits();
        let check = |resource, requested, limit| {
            if requested > limit { Err(AtlasTransferError::DeviceLimit { resource, requested, limit }) } else { Ok(()) }
        };
        check("page size", u64::from(size), u64::from(limits.max_texture_dimension_2d))?;
        check("colour pages", u64::from(used.0), u64::from(limits.max_texture_array_layers))?;
        check("index pages", u64::from(used.1), u64::from(limits.max_texture_array_layers))?;
        if size == 0 {
            return Err(AtlasTransferError::SourceMismatch);
        }
        let whole = kind == TransferKind::Whole;
        let required = if whole { used } else { (active_room.0.max(used.0), active_room.1.max(used.1)) };
        let required = (required.0.max(minimum_room.0), required.1.max(minimum_room.1));
        check("required colour pages", u64::from(required.0), u64::from(limits.max_texture_array_layers))?;
        check("required index pages", u64::from(required.1), u64::from(limits.max_texture_array_layers))?;
        let extra = |used: u32, current: u32| {
            if kind == TransferKind::Growth && used > current { used.saturating_add((used / 2).max(4)) } else { used }
        };
        let desired = (
            required.0.max(reserve.0).max(extra(used.0, active_room.0)).min(limits.max_texture_array_layers),
            required.1.max(reserve.1).max(extra(used.1, active_room.1)).min(limits.max_texture_array_layers),
        );
        let (sprite_bytes, palette_bytes) = atlas_table_bytes(sprites.len(), palettes.len());
        if kind != TransferKind::Reserve {
            check("upload buffer bytes", ATLAS_TRANSFER_MAX_BATCH_BYTES, limits.max_buffer_size)?;
            let max = limits.max_buffer_size.min(u64::from(limits.max_storage_buffer_binding_size));
            check("sprite table bytes", sprite_bytes, max)?;
            check("palette table bytes", palette_bytes, max)?;
        }
        let table_bytes = if kind == TransferKind::Reserve { 0 } else {
            sprite_bytes + palette_bytes + ATLAS_TRANSFER_STAGING_RESERVE_BYTES
        };
        let room = transfer_room(size, active_room, required, desired, whole, resident, table_bytes, self.atlas_transfer_limit.get())?;
        let replace_rgba = whole || room.0 > active_room.0;
        let replace_index = whole || room.1 > active_room.1;
        let rgba = if replace_rgba { PageArray::new(device, wgpu::TextureFormat::Rgba8Unorm, size, room.0) } else { active.unwrap().rgba.clone() };
        let index = if replace_index { PageArray::new(device, wgpu::TextureFormat::R8Unorm, size, room.1) } else { active.unwrap().index.clone() };
        let mut tasks = std::collections::VecDeque::new();
        if !whole {
            let a = active.unwrap();
            if replace_rgba {
                tasks.extend((0..a.rgba_pages).map(|page| PixelTransfer::copy(true, page, 0, 0, size, size)));
            }
            if replace_index {
                tasks.extend((0..a.index_pages).map(|page| PixelTransfer::copy(false, page, 0, 0, size, size)));
            }
        }
        match source {
            AtlasTransferSource::Whole(a) => {
                for (i, page) in a.pages.iter().enumerate().filter(|(_, p)| p.width == size) {
                    tasks.push_back(PixelTransfer { source: PixelSource::WholeRgba(i), rgba: true, page: i as u32, x: 0, y: 0, width: page.width, height: page.height, cursor: PixelCursor::default() });
                }
                for (i, page) in a.index_pages.iter().enumerate() {
                    tasks.push_back(PixelTransfer { source: PixelSource::WholeIndex(i), rgba: false, page: i as u32, x: 0, y: 0, width: page.width, height: page.height, cursor: PixelCursor::default() });
                }
            }
            AtlasTransferSource::Growth(g) => {
                for (i, b) in g.blits.iter().enumerate() {
                    let (rgba, w, h) = match &b.pixels {
                        crate::BlitPixels::Rgba(i) => (true, i.width, i.height),
                        crate::BlitPixels::Index(i) => (false, i.width, i.height),
                    };
                    if w > 0 && h > 0 {
                        tasks.push_back(PixelTransfer { source: PixelSource::Blit(i), rgba, page: b.page, x: b.x, y: b.y, width: w, height: h, cursor: PixelCursor::default() });
                    }
                }
            }
            AtlasTransferSource::Reserve => {}
        }
        self.atlas_transfer = Some(AtlasTransfer {
            kind,
            textures: AtlasTextures { rgba, index, rgba_pages: used.0, index_pages: used.1, sprites, palettes },
            named_palettes,
            prefix_sprites,
            palette_tail,
            palette_prefix: active.map_or(0, |a| a.palettes.len()),
            replace_rgba,
            tasks,
            upload_buffer: (kind != TransferKind::Reserve).then(|| device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tile-render candidate upload buffer"),
                size: ATLAS_TRANSFER_MAX_BATCH_BYTES,
                usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })),
            sprite_buffer: (kind != TransferKind::Reserve).then(|| atlas_storage(device, "tile-render candidate sprites", sprite_bytes)),
            palette_buffer: (kind != TransferKind::Reserve).then(|| atlas_storage(device, "tile-render candidate palettes", palette_bytes)),
            sprite_cursor: 0,
            palette_cursor: 0,
            fence: None,
            palette_layout_changed: false,
        });
        Ok(())
    }

    /// Submits at most `min(budget, ATLAS_TRANSFER_MAX_BATCH_BYTES)` charged
    /// bytes. Texture uploads charge 256-byte-aligned staging rows, texture
    /// copies charge payload, and table uploads charge their byte count.
    /// Driver allocation/zero-initialization and immediate live mutations
    /// are separate work. Subsequent batches use callbacks, never a blocking
    /// poll. Ready means all commands were queued, so a later draw can use
    /// queue ordering; retirement still waits for GPU completion callbacks.
    /// Source pixels must remain unchanged until adoption or cancellation.
    pub fn advance_atlas_transfer(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: AtlasTransferSource<'_>,
        budget: u64,
    ) -> Result<AtlasTransferProgress, AtlasTransferError> {
        self.poll_atlas_transfers(device);
        let Some(job) = &mut self.atlas_transfer else { return Err(AtlasTransferError::NoTransfer) };
        let matches = match source {
            AtlasTransferSource::Whole(a) => job.kind == TransferKind::Whole && a.page_size == job.textures.rgba.size && a.sprites.len() == job.textures.sprites.len(),
            AtlasTransferSource::Growth(g) => job.kind == TransferKind::Growth && g.first as usize == job.prefix_sprites
                && g.sprites.len() + job.prefix_sprites == job.textures.sprites.len(),
            AtlasTransferSource::Reserve => job.kind == TransferKind::Reserve,
        };
        if !matches { return Err(AtlasTransferError::SourceMismatch) }
        if job.palette_layout_changed { return Err(AtlasTransferError::PaletteLayoutChanged) }
        if job.ready() { return Ok(AtlasTransferProgress { bytes: 0, ready: true }) }
        let budget = budget.min(ATLAS_TRANSFER_MAX_BATCH_BYTES);
        if job.fence.as_ref().is_some_and(|f| !f.load(std::sync::atomic::Ordering::Acquire)) {
            return Ok(AtlasTransferProgress::default());
        }
        job.fence = None;
        let mut written = 0;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tile-render bounded atlas transfer") });
        let mut staging_offset = 0;
        loop {
            let Some(task) = job.tasks.front_mut() else { break };
            let is_copy = matches!(task.source, PixelSource::Copy);
            // Texture copies and staged uploads use this encoder, so their
            // ordering follows the transaction even within one submission.
            let texel_bytes = if task.rgba { 4 } else { 1 };
            let chunk = if is_copy { task.cursor.chunk(task.width, task.height, texel_bytes, budget - written) }
                else { task.cursor.staged_chunk(task.width, task.height, texel_bytes, budget - written) };
            let Some((w, h)) = chunk else {
                if written == 0 {
                    return Err(AtlasTransferError::BudgetTooSmall {
                        minimum: if is_copy { u64::from(texel_bytes) } else { u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) }, provided: budget,
                    });
                }
                break;
            };
            let target = if task.rgba { &job.textures.rgba } else { &job.textures.index };
            let origin = wgpu::Origin3d { x: task.x + task.cursor.x, y: task.y + task.cursor.y, z: task.page };
            let extent = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
            match task.source {
                PixelSource::Copy => {
                    let active = self.atlas.as_ref().ok_or(AtlasTransferError::NoAtlas)?;
                    let from = if task.rgba { &active.rgba } else { &active.index };
                    encoder.copy_texture_to_texture(
                        wgpu::TexelCopyTextureInfo { origin, ..from.texture.as_image_copy() },
                        wgpu::TexelCopyTextureInfo { origin, ..target.texture.as_image_copy() },
                        extent,
                    );
                }
                _ => {
                    let bytes: &[u8] = match (task.source, source) {
                        (PixelSource::WholeRgba(i), AtlasTransferSource::Whole(a)) => a.pages[i].as_bytes(),
                        (PixelSource::WholeIndex(i), AtlasTransferSource::Whole(a)) => &a.index_pages[i].pixels,
                        (PixelSource::Blit(i), AtlasTransferSource::Growth(g)) => match &g.blits[i].pixels {
                            crate::BlitPixels::Rgba(i) => i.as_bytes(),
                            crate::BlitPixels::Index(i) => &i.pixels,
                        },
                        _ => return Err(AtlasTransferError::SourceMismatch),
                    };
                    let offset = (u64::from(task.cursor.y) * u64::from(task.width) + u64::from(task.cursor.x)) * u64::from(texel_bytes);
                    let pitch = aligned_texture_row(w, texel_bytes);
                    let stage_size = pitch * u64::from(h);
                    let mut padded = vec![0; stage_size as usize];
                    let source_pitch = u64::from(task.width) * u64::from(texel_bytes);
                    let row_bytes = (w * texel_bytes) as usize;
                    for row in 0..u64::from(h) {
                        let from = (offset + row * source_pitch) as usize;
                        let into = (row * pitch) as usize;
                        padded[into..into + row_bytes].copy_from_slice(&bytes[from..from + row_bytes]);
                    }
                    let staging = job.upload_buffer.as_ref().unwrap();
                    queue.write_buffer(staging, staging_offset, &padded);
                    encoder.copy_buffer_to_texture(
                        wgpu::TexelCopyBufferInfo { buffer: staging, layout: wgpu::TexelCopyBufferLayout {
                            offset: staging_offset, bytes_per_row: Some(pitch as u32), rows_per_image: Some(h),
                        } },
                        wgpu::TexelCopyTextureInfo { origin, ..target.texture.as_image_copy() }, extent,
                    );
                    staging_offset += stage_size;
                }
            }
            written += if is_copy { u64::from(w) * u64::from(h) * u64::from(texel_bytes) }
                else { aligned_texture_row(w, texel_bytes) * u64::from(h) };
            task.cursor.advance(task.width, w, h);
            if task.cursor.y == task.height {
                job.tasks.pop_front();
            }
        }
        if job.tasks.is_empty() {
            if let Some(buffer) = &job.sprite_buffer {
                let empty = [SpriteDesc::default()];
                let descriptors: &[SpriteDesc] = if job.textures.sprites.is_empty() { &empty } else { &job.textures.sprites };
                let bytes: &[u8] = bytemuck::cast_slice(descriptors);
                let count = (budget - written).min(buffer.size() - job.sprite_cursor) & !3;
                if count > 0 {
                    queue.write_buffer(buffer, job.sprite_cursor, &bytes[job.sprite_cursor as usize..(job.sprite_cursor + count) as usize]);
                    job.sprite_cursor += count;
                    written += count;
                }
            }
            if let Some(buffer) = &job.palette_buffer {
                let count = (budget - written).min(buffer.size() - job.palette_cursor) & !3;
                if count > 0 {
                    let words = palette_slice(&job.textures.palettes, job.palette_cursor, count);
                    queue.write_buffer(buffer, job.palette_cursor, bytemuck::cast_slice(&words));
                    job.palette_cursor += count;
                    written += count;
                }
            }
        }
        if written == 0 && !job.ready() && budget < 4 {
            return Err(AtlasTransferError::BudgetTooSmall { minimum: 4, provided: budget });
        }
        if written > 0 {
            queue.submit([encoder.finish()]);
            job.fence = Some(atlas_fence(queue));
        }
        Ok(AtlasTransferProgress { bytes: written, ready: job.ready() })
    }

    /// Adopts a completed candidate, retaining old allocations until their
    /// submitted draws finish. Returns false while any work is incomplete.
    pub fn commit_atlas_transfer(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        self.poll_atlas_transfers(device);
        if self.atlas_transfer.as_ref().is_none_or(|j| !j.ready()) { return false }
        let mut job = self.atlas_transfer.take().unwrap();
        if job.kind == TransferKind::Reserve {
            // Reservation changes neither ids nor table contents. Runtime
            // palette buffer replacements remain owned by the live atlas.
            let active = self.atlas.as_ref().unwrap();
            job.textures.sprites = active.sprites.clone();
            job.textures.palettes = active.palettes.clone();
        }
        let old = self.atlas.replace(job.textures);
        let old_bind_group = self.bind_group.take();
        let (old_sprites, old_palettes) = if job.kind == TransferKind::Reserve { (None, None) } else {
            self.named_palettes = job.named_palettes;
            self.generation += 1;
            (std::mem::replace(&mut self.sprites, job.sprite_buffer), std::mem::replace(&mut self.palettes, job.palette_buffer))
        };
        let upload = job.upload_buffer.take();
        if old.is_some() || old_bind_group.is_some() || old_sprites.is_some() || old_palettes.is_some() || upload.is_some() {
            queue.submit([]);
            self.atlas_retired = Some(RetiredAtlas { _textures: old, _sprites: old_sprites, _palettes: old_palettes, _bind_group: old_bind_group, _upload: upload, done: atlas_fence(queue) });
        }
        if job.kind != TransferKind::Reserve { self.refresh_free_runs(); }
        self.rebuild_bind_group(device);
        true
    }

    /// Cancels work without freeing its allocations early or allocating a
    /// replacement. Call `poll_atlas_transfers` until the renderer is idle.
    pub fn cancel_atlas_transfer(&mut self, queue: &wgpu::Queue) {
        let Some(job) = self.atlas_transfer.take() else { return };
        queue.submit([]);
        self.atlas_retired = Some(RetiredAtlas {
            _textures: Some(job.textures), _sprites: job.sprite_buffer, _palettes: job.palette_buffer,
            _bind_group: None, _upload: job.upload_buffer, done: atlas_fence(queue),
        });
    }

    fn finish_atlas_transfer_sync(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.cancel_atlas_transfer(queue);
        if self.atlas_retired.is_some() {
            if let Err(e) = device.poll(wgpu::PollType::wait_indefinitely()) {
                log::warn!("waiting for synchronous atlas transfer retirement: {e}");
            }
            self.poll_atlas_transfers(device);
        }
    }

    /// Startup/offscreen compatibility API, outside the bounded runtime
    /// allocation policy. Do not retain legacy staging objects while using
    /// runtime transfers. Staging and adoption panic if a runtime candidate
    /// or retirement is present. Live viewers use `begin_atlas_transfer`.
    ///
    /// Starts uploading `atlas` to replace the one drawn, a part at a time
    /// ([`TileRenderer::upload_staged`]) while frames go on being drawn with
    /// the old one; [`TileRenderer::adopt_atlas`] swaps it in. Unlike
    /// [`TileRenderer::set_atlas`] nothing waits for the GPU: the old atlas
    /// goes once the GPU is done with it, so both take memory meanwhile.
    pub fn stage_atlas(&self, device: &wgpu::Device, atlas: &Atlas) -> StagedAtlas {
        assert!(self.atlas_transfer_idle(), "legacy atlas staging cannot overlap a runtime atlas transfer");
        let rgba_pages: Vec<u32> = atlas.pages.iter().enumerate().filter(|(_, p)| p.width == atlas.page_size).map(|(i, _)| i as u32).collect();
        let index_pages = atlas.index_pages.len() as u32;
        let textures = AtlasTextures {
            rgba: PageArray::new(device, wgpu::TextureFormat::Rgba8Unorm, atlas.page_size, rgba_pages.len() as u32),
            index: PageArray::new(device, wgpu::TextureFormat::R8Unorm, atlas.page_size, index_pages),
            rgba_pages: rgba_pages.len() as u32,
            index_pages,
            sprites: atlas.sprites.clone(),
            palettes: atlas.palettes.clone(),
        };
        let mut pending: Vec<(bool, u32)> = rgba_pages.into_iter().map(|i| (true, i)).collect();
        pending.extend((0..index_pages).map(|i| (false, i)));
        pending.reverse();
        StagedAtlas { textures, pending }
    }

    /// Startup/offscreen compatibility upload, outside the runtime budget.
    /// Writes pages of `staged` (made by [`TileRenderer::stage_atlas`] from
    /// `atlas`) until about `budget` bytes were written this time (at least
    /// one page); whether every page is there now.
    pub fn upload_staged(&self, queue: &wgpu::Queue, staged: &mut StagedAtlas, atlas: &Atlas, budget: u64) -> bool {
        let mut written = 0u64;
        while let Some(&(rgba, i)) = staged.pending.last() {
            if written > 0 && written >= budget {
                break;
            }
            staged.pending.pop();
            if rgba {
                let page = &atlas.pages[i as usize];
                staged.textures.rgba.write(queue, i, 0, 0, (page.width, page.height), page.as_bytes());
                written += page.as_bytes().len() as u64;
            } else {
                let page = &atlas.index_pages[i as usize];
                staged.textures.index.write(queue, i, 0, 0, (page.width, page.height), &page.pixels);
                written += page.pixels.len() as u64;
            }
        }
        staged.pending.is_empty()
    }

    /// Startup/offscreen compatibility adoption. Panics if runtime atlas
    /// work is uploading or retiring; complete or cancel that work first.
    /// Draws with the atlas `staged` from now on (all of it uploaded, see
    /// [`TileRenderer::upload_staged`]; `atlas` the one it was made from).
    pub fn adopt_atlas(&mut self, device: &wgpu::Device, staged: StagedAtlas, atlas: &Atlas) {
        assert!(self.atlas_transfer_idle(), "legacy atlas adoption cannot overlap a runtime atlas transfer");
        self.generation += 1;
        self.bind_group = None;
        self.atlas = Some(staged.textures);
        self.named_palettes = atlas.named_palettes;
        let empty = [SpriteDesc::default()];
        let sprites: &[SpriteDesc] = if atlas.sprites.is_empty() { &empty } else { &atlas.sprites };
        self.sprites = Some(storage(device, "tile-render sprites", bytemuck::cast_slice(sprites)));
        self.palettes = Some(storage(device, "tile-render palettes", bytemuck::cast_slice(&palette_words(&atlas.palettes))));
        // The staged descriptors may change both sprite bounds and the
        // alpha properties used to remove covered draws. Rebuild the runs
        // only on adoption, while uploads leave the old picture intact.
        self.refresh_free_runs();
        self.rebuild_bind_group(device);
    }

    /// Bytes the atlas takes on the GPU: colour pages, index pages (as
    /// allocated).
    pub fn atlas_bytes(&self) -> (u64, u64) {
        self.atlas.as_ref().map_or((0, 0), |a| (a.rgba.bytes(), a.index.bytes()))
    }

    /// Replaces the palette rows (see `Atlas::palettes`), e.g. a driver's
    /// colours changing at run time: no atlas is uploaded again. Live
    /// palette edits and candidate mirrors are immediate, outside the
    /// background transfer allowance. Changing the prefix length makes a
    /// pending growth permanently incompatible (`PaletteLayoutChanged`).
    pub fn set_palettes(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, palettes: &[u32]) {
        self.generation += 1;
        let words = palette_words(palettes);
        let mut changed = false;
        if let Some(a) = &mut self.atlas {
            let descriptors = crate::atlas::palette_bounds(&mut a.sprites, &a.palettes, palettes, self.named_palettes);
            if let Some(buffer) = &self.sprites {
                for i in &descriptors {
                    queue.write_buffer(buffer, (*i * size_of::<SpriteDesc>()) as u64, bytemuck::bytes_of(&a.sprites[*i]));
                }
            }
            changed = !descriptors.is_empty();
            a.palettes = palettes.to_vec();
        }
        match &self.palettes {
            Some(b) if b.size() >= (words.len() * 4) as u64 => {
                queue.write_buffer(b, 0, bytemuck::cast_slice(&words));
                // The shader sees the whole allocation, including a removed
                // palette row. Do not leave its previous colours there.
                if b.size() > (words.len() * 4) as u64 {
                    queue.write_buffer(b, (words.len() * 4) as u64, &vec![0; b.size() as usize - words.len() * 4]);
                }
            }
            _ => {
                self.palettes = Some(storage(device, "tile-render palettes", bytemuck::cast_slice(&words)));
                self.rebuild_bind_group(device);
            }
        }
        if let Some(job) = self.atlas_transfer.as_mut().filter(|j| j.kind == TransferKind::Growth) {
            // Growth palette ids were compiled after this exact prefix.
            // Reallocating a table cannot repair a changed row namespace.
            if palettes.len() != job.palette_prefix {
                job.palette_layout_changed = true;
            } else if !job.palette_layout_changed {
                let mut candidate = palettes.to_vec();
                candidate.extend_from_slice(&job.palette_tail);
                for i in crate::atlas::palette_bounds(&mut job.textures.sprites, &job.textures.palettes, &candidate, job.named_palettes) {
                    if let Some(buffer) = &job.sprite_buffer {
                        queue.write_buffer(buffer, (i * size_of::<SpriteDesc>()) as u64, bytemuck::bytes_of(&job.textures.sprites[i]));
                    }
                }
                job.textures.palettes = candidate;
                // Like the live update above, this mirror is immediate and
                // outside the allowance for loading background art. Future
                // initial slices read the current CPU palette, so neither a
                // completed prefix nor an unfinished suffix can go stale.
                if let Some(buffer) = &job.palette_buffer {
                    if !job.textures.palettes.is_empty() {
                        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&job.textures.palettes));
                    }
                }
            }
        }
        if changed {
            self.refresh_free_runs();
        }
    }

    /// Rebuilds only which free instances are drawn. Keeping the original
    /// instances and runs lets alpha changes restore previously covered
    /// draws without keeping atlas pixels or another copy of the world.
    fn refresh_free_runs(&mut self) {
        let sprites = self.atlas.as_ref().map_or(&[][..], |a| &a.sprites[..]);
        let Some(w) = &mut self.world else { return };
        let mut runs = dedup_free(&w.source_runs, &w.scene_free, &w.sets_source, sprites);
        w.last_runs = runs.last().cloned().unwrap_or_default();
        if let Some(last) = runs.last_mut() {
            last.extend(shifted_markers(&self.marker_runs, w.scene_free.len() as u32, w.scene_text_len as u32));
        }
        w.smoke_count = smoke_instances(&runs);
        w.free_after = runs;
    }

    /// Uploads a whole world. Call again (or `update_world`) when it changes.
    pub fn set_world(&mut self, device: &wgpu::Device, world: &TileWorld) {
        // Solid commands and outline text can draw before any art arrives.
        // Empty GPU resources satisfy bindings without generating CPU pixels.
        if self.atlas.is_none() {
            self.atlas = Some(AtlasTextures {
                rgba: PageArray::new(device, wgpu::TextureFormat::Rgba8Unorm, 1, 0),
                index: PageArray::new(device, wgpu::TextureFormat::R8Unorm, 1, 0),
                rgba_pages: 0, index_pages: 0, sprites: Vec::new(), palettes: Vec::new(),
            });
            self.sprites = Some(storage(device, "tile-render empty sprites", bytemuck::bytes_of(&SpriteDesc::default())));
            self.palettes = Some(storage(device, "tile-render empty palettes", &[0; 4]));
        }

        self.generation += 1;
        let packed = world.pack();
        let empty = [LayerDesc::default()];
        let layers: &[LayerDesc] = if packed.layers.is_empty() { &empty } else { &packed.layers };
        let layer_table = packed.layers.clone();
        let tables = packed.tables.clone();
        let (outlines, bases) = crate::world::outline_words(&world.fonts);
        let sprites = self.atlas.as_ref().map_or(&[][..], |a| &a.sprites[..]);
        let (free, mut text, source_runs) = free_sprites(world, &bases);
        let scene_text_len = text.len();
        let (markers, marker_text, marker_runs) = pack_markers(&self.marker_sources, &world.fonts, &bases);
        self.markers = markers;
        self.marker_runs = marker_runs;
        let mut free_after = dedup_free(&source_runs, &free, &world.sets, sprites);
        let last_runs = free_after.last().cloned().unwrap_or_default();
        if let Some(last) = free_after.last_mut() {
            last.extend(shifted_markers(&self.marker_runs, free.len() as u32, text.len() as u32));
        }
        let mut all = free.clone();
        all.extend_from_slice(&self.markers);
        let sprite_table = self.atlas.as_ref().map_or(&[][..], |a| &a.sprites[..]);
        let octagons = world.passes().iter().map(|p| octagon_pass(world, p, sprite_table)).collect();
        let scene_has_text = source_runs.iter().flatten().any(|(_, kind, _)| matches!(kind, FreeKind::Text | FreeKind::TextKeyed));
        let has_text = scene_has_text || !marker_text.is_empty();
        text.extend(marker_text);
        self.world = Some(WorldBuffers {
            octagons,
            grid: world.grid,
            color: world.color,
            passes: world.passes(),
            reach: packed.reach,
            free: storage_with_room(device, "tile-render free sprites", bytemuck::cast_slice(&all)),
            scene_smoke_count: smoke_instances(&source_runs),
            smoke_count: smoke_instances(&free_after),
            free_after,
            host_period_ms: host_period(world.free.iter().map(|s| s.ticks_per_frame)),
            layer_period_ms: host_period(world.host_clock_layers()),
            scene_free: free,
            source_runs,
            last_runs,
            layer_specs: world.layers().to_vec(),
            sets_source: world.sets.clone(),
            text: storage(device, "tile-render text", bytemuck::cast_slice(&text)),
            text_source: text,
            scene_text_len,
            scene_has_text,
            font_bases: bases,
            has_text,
            outlines: storage(device, "tile-render outlines", bytemuck::cast_slice(&outlines)),
            fonts: world.fonts.clone(),
            first_zoomed: with_markers(crate::world::first_zoomed(&world.free, world.passes().len().min(MAX_PASSES)), &self.markers, world.passes().len().min(MAX_PASSES)),
            scene_first_zoomed: crate::world::first_zoomed(&world.free, world.passes().len().min(MAX_PASSES)),
            layer_table,
            tables,
            layers: storage_with_room(device, "tile-render layers", bytemuck::cast_slice(layers)),
            tile_data: storage_with_room(device, "tile-render tile data", bytemuck::cast_slice(&packed.data)),
            lut: storage(device, "tile-render lut", bytemuck::cast_slice(&packed.lut)),
            sets: storage(device, "tile-render sprite sets", bytemuck::cast_slice(&packed.sets)),
        });
        self.rebuild_bind_group(device);
    }

    /// Re-uploads the cell values of an already uploaded world of the same
    /// shape (same grid and layer tables). Cheap enough to do every frame.
    pub fn update_world(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &TileWorld) {
        match &self.world {
            Some(w) if w.same_shape(world) => {
                if !self.sync_tables(device, queue, world) {
                    let packed = world.pack();
                    let w = self.world.as_mut().expect("matched above");
                    queue.write_buffer(&w.tile_data, 0, bytemuck::cast_slice(&packed.data));
                    w.reach = packed.reach;
                }
                self.update_free(device, queue, world);
            }
            _ => self.set_world(device, world),
        }
    }

    /// Brings the uploaded layer tables and the room for the cell data up
    /// to `world`'s (data slots allocated or freed, bindings changed), into
    /// the buffers where they fit (queued writes: the GPU never waits for
    /// them, nor they for it), else into new ones with room to grow.
    /// Whether the cell data moved to a new buffer (then all of it is
    /// written, and nothing else needs to be).
    fn sync_tables(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &TileWorld) -> bool {
        let Some(w) = self.world.as_mut() else { return false };
        let mut rebind = false;
        let (table, tables) = world.layer_tables();
        if table != w.layer_table {
            let empty = [LayerDesc::default()];
            let bytes: &[u8] = bytemuck::cast_slice(if table.is_empty() { &empty[..] } else { &table[..] });
            if w.layers.size() >= bytes.len() as u64 {
                queue.write_buffer(&w.layers, 0, bytes);
            } else {
                w.layers = storage_with_room(device, "tile-render layers", bytes);
                rebind = true;
            }
            w.layer_table = table;
            // (A slot freed or bound may reach less or further.)
            w.reach = world.offset_reach();
            self.generation += 1;
        }
        w.tables = tables;
        let moved = (w.tile_data.size() as usize) < world.packed_words() * 4;
        if moved {
            let packed = world.pack();
            w.tile_data = storage_with_room(device, "tile-render tile data", bytemuck::cast_slice(&packed.data));
            w.reach = packed.reach;
            self.generation += 1;
            rebind = true;
        }
        if rebind {
            self.rebuild_bind_group(device);
        }
        moved
    }

    /// Re-uploads only the free sprites (and text) of an already uploaded
    /// world whose cells did not change (e.g. markers moving every frame).
    pub fn update_free(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &TileWorld) {
        self.generation += 1;
        if self.world.as_ref().is_none_or(|w| !w.same_shape(world)) {
            return self.set_world(device, world);
        }
        let w = self.world.as_mut().expect("checked above");
        // (The fonts' outlines are put together only when they change.)
        let bases = font_bases(&world.fonts);
        let sprites = self.atlas.as_ref().map_or(&[][..], |a| &a.sprites[..]);
        let (free, mut text, source_runs) = free_sprites(world, &bases);
        let scene_text_len = text.len();
        let (markers, marker_text, marker_runs) = pack_markers(&self.marker_sources, &world.fonts, &bases);
        self.markers = markers;
        self.marker_runs = marker_runs;
        let mut free_after = dedup_free(&source_runs, &free, &world.sets, sprites);
        w.host_period_ms = host_period(world.free.iter().map(|s| s.ticks_per_frame));
        w.scene_first_zoomed = crate::world::first_zoomed(&world.free, w.passes.len().min(MAX_PASSES));
        w.first_zoomed = with_markers(w.scene_first_zoomed.clone(), &self.markers, w.passes.len().min(MAX_PASSES));
        w.scene_has_text = source_runs.iter().flatten().any(|(_, kind, _)| matches!(kind, FreeKind::Text | FreeKind::TextKeyed));
        w.has_text = w.scene_has_text || !marker_text.is_empty();
        w.scene_text_len = scene_text_len;
        w.font_bases = bases;
        w.source_runs = source_runs;
        w.last_runs = free_after.last().cloned().unwrap_or_default();
        if let Some(last) = free_after.last_mut() {
            last.extend(shifted_markers(&self.marker_runs, free.len() as u32, text.len() as u32));
        }
        w.scene_smoke_count = smoke_instances(&w.source_runs);
        w.smoke_count = smoke_instances(&free_after);
        w.free_after = free_after;
        let mut all = free.clone();
        all.extend_from_slice(&self.markers);
        w.scene_free = free;
        let mut rebind = false;
        if w.free.size() >= (all.len() * size_of::<FreeSprite>()) as u64 {
            queue.write_buffer(&w.free, 0, bytemuck::cast_slice(&all));
        } else {
            w.free = storage_with_room(device, "tile-render free sprites", bytemuck::cast_slice(&all));
            rebind = true;
        }
        // Into the buffer if it fits, else a new one.
        let mut write = |buffer: &mut wgpu::Buffer, label: &str, bytes: &[u8]| {
            if buffer.size() >= bytes.len() as u64 {
                queue.write_buffer(buffer, 0, bytes);
            } else {
                *buffer = storage(device, label, bytes);
                rebind = true;
            }
        };
        text.extend(marker_text);
        write(&mut w.text, "tile-render text", bytemuck::cast_slice(&text));
        w.text_source = text;
        if !same_fonts(&w.fonts, &world.fonts) {
            let (outlines, _) = crate::world::outline_words(&world.fonts);
            w.outlines = storage(device, "tile-render outlines", bytemuck::cast_slice(&outlines));
            w.fonts = world.fonts.clone();
            rebind = true;
        }
        if rebind {
            self.rebuild_bind_group(device);
        }
    }

    /// Sprites drawn after everything else, in list order, that change
    /// more often than the world (a viewer's cursor, markers, a frame
    /// counter): uploaded on their own, after the world's free sprites,
    /// which are not touched. Outline text uses the uploaded world's fonts.
    /// Only bounded marker records are packed and written here; scene text
    /// and immutable font outlines are retained in their existing GPU buffers.
    pub fn set_markers(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, markers: &[crate::PlacedSprite]) {
        self.marker_sources.clear();
        self.marker_sources.extend_from_slice(markers);
        let Some(w) = &mut self.world else { return };
        let (free, text, runs) = pack_markers(markers, &w.fonts, &w.font_bases);
        self.markers = free;
        self.marker_runs = runs;
        self.marker_smoke_count = self.marker_runs.iter().filter(|(_, kind, _)| *kind == FreeKind::NativePaletteAlpha).map(|(range, _, _)| range.len()).sum();
        let first_zoomed = with_markers(w.scene_first_zoomed.clone(), &self.markers, w.passes.len().min(MAX_PASSES));
        if w.first_zoomed != first_zoomed {
            w.first_zoomed = first_zoomed;
            self.generation += 1;
        }
        w.smoke_count = w.scene_smoke_count + self.marker_smoke_count;
        let base = w.scene_free.len();
        if let Some(last) = w.free_after.last_mut() {
            *last = w.last_runs.clone();
            last.extend(shifted_markers(&self.marker_runs, base as u32, w.scene_text_len as u32));
        }
        let mut rebind = false;
        let size = size_of::<FreeSprite>();
        if w.free.size() >= ((base + self.markers.len()) * size) as u64 {
            if !self.markers.is_empty() {
                queue.write_buffer(&w.free, (base * size) as u64, bytemuck::cast_slice(&self.markers));
            }
        } else {
            let mut all = w.scene_free.clone();
            all.extend_from_slice(&self.markers);
            w.free = storage_with_room(device, "tile-render free sprites", bytemuck::cast_slice(&all));
            rebind = true;
        }
        w.has_text = w.scene_has_text || !text.is_empty();
        w.text_source.truncate(w.scene_text_len);
        w.text_source.extend_from_slice(&text);
        let size = size_of::<tile_render_shared::text::TextGlyph>();
        if w.text.size() >= (w.text_source.len() * size) as u64 {
            if !text.is_empty() {
                queue.write_buffer(&w.text, (w.scene_text_len * size) as u64, bytemuck::cast_slice(&text));
            }
        } else {
            // Growth is occasional; steady animation writes only its tail.
            w.text = storage_with_room(device, "tile-render text", bytemuck::cast_slice(&w.text_source));
            rebind = true;
        }
        if rebind { self.rebuild_bind_group(device); }
    }

    /// Uploads the records of `world`'s cells that `dirty` lists (taken
    /// from `TileWorld::take_dirty`), into a world of the same shape
    /// uploaded before; all of them (or a new world) otherwise. Its free
    /// sprites are not uploaded (see [`TileRenderer::update_free`]).
    pub fn update_cells(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &TileWorld, dirty: &crate::world::Dirty) {
        self.update_cell_records(device, queue, world, dirty);
        if !matches!(dirty, crate::world::Dirty::None) && let Some(w) = self.world.as_mut() {
            // (A viewer-clock layer drawing a cell now, or no longer.)
            w.layer_period_ms = host_period(world.host_clock_layers());
        }
    }

    fn update_cell_records(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &TileWorld, dirty: &crate::world::Dirty) {
        if self.world.as_ref().is_none_or(|w| !w.same_shape(world) || !same_fonts(&w.fonts, &world.fonts)) {
            return self.set_world(device, world);
        }
        let records = match dirty {
            crate::world::Dirty::None => return,
            crate::world::Dirty::Records(r) => r,
            crate::world::Dirty::All => {
                self.generation += 1;
                if self.sync_tables(device, queue, world) {
                    return;
                }
                let packed = world.pack();
                let w = self.world.as_mut().expect("checked above");
                queue.write_buffer(&w.tile_data, 0, bytemuck::cast_slice(&packed.data));
                w.reach = packed.reach;
                return;
            }
        };
        self.generation += 1;
        // (Slots allocated or freed, bindings changed: the tables first.)
        if self.sync_tables(device, queue, world) {
            return;
        }
        let w = self.world.as_mut().expect("checked above");
        // The byte ranges, word aligned, merged where they touch.
        let mut ranges: Vec<(usize, usize)> = records
            .iter()
            .filter_map(|&(layer, cell)| world.packed_record(layer, cell))
            .map(|(at, len)| (at & !3, (at + len + 3) & !3))
            .collect();
        ranges.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
        for (a, b) in ranges {
            match merged.last_mut() {
                Some(last) if a <= last.1 + 64 => last.1 = last.1.max(b),
                _ => merged.push((a, b)),
            }
        }
        let size = w.tile_data.size() as usize;
        for (a, b) in merged {
            let b = b.min(size);
            if a < b {
                queue.write_buffer(&w.tile_data, a as u64, &world.packed_bytes(a..b));
            }
        }
        if records.iter().any(|&(layer, _)| world.has_offsets(layer)) {
            w.reach = world.offset_reach();
        }
    }

    fn rebuild_bind_group(&mut self, device: &wgpu::Device) {
        self.bind_group = None;
        let (Some(atlas), Some(sprites), Some(palettes), Some(world)) = (&self.atlas, &self.sprites, &self.palettes, &self.world) else { return };
        self.bind_group = Some(std::array::from_fn(|slot| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tile-render"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.params[slot],
                        offset: 0,
                        size: wgpu::BufferSize::new(size_of::<FrameParams>() as u64),
                    }),
                },
                wgpu::BindGroupEntry { binding: 1, resource: world.layers.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: world.tile_data.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: world.lut.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: world.sets.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: sprites.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&atlas.rgba.view) },
                wgpu::BindGroupEntry { binding: 11, resource: wgpu::BindingResource::TextureView(&atlas.index.view) },
                wgpu::BindGroupEntry { binding: 7, resource: world.free.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 8, resource: world.text.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 9, resource: world.outlines.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 10, resource: palettes.as_entire_binding() },
            ],
        })));
    }

    /// Renders the world into `target` (`RENDER_ATTACHMENT`, of the format
    /// given to `new`) as seen by `camera` at animation tick `frame`, on a
    /// background of `clear`. Only clears until both an atlas and a world
    /// are set.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        camera: &Camera,
        frame: u32,
        clear: [u8; 4],
    ) {
        self.render_views(device, queue, encoder, target, std::slice::from_ref(camera), frame, clear);
    }

    /// Renders several views of the world into `target`: view `i` as
    /// `cameras[i]` sees it, into the rectangle at `origin` of the camera's
    /// viewport size (later views over earlier ones). Free sprites of view
    /// `i` (`PlacedSprite::view`) are drawn in it only; the others in every
    /// view. A zoomed view is drawn at zoom 1 into a texture of its own and
    /// copied into its rectangle by nearest texel; from its first zoomed
    /// sprite on (`Camera::zoomed_art`; `Camera::hairline`: from the start)
    /// it is drawn at its zoom straight into the target instead, which shows
    /// the same pixels, but zoomed art at full resolution in its place (under
    /// later passes). At most [`MAX_VIEWS`] views.
    #[allow(clippy::too_many_arguments)]
    pub fn render_views(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        cameras: &[Camera],
        frame: u32,
        clear: [u8; 4],
    ) {
        self.render_composed(device, queue, encoder, target, cameras, &[], frame, clear);
    }

    /// [`TileRenderer::render_views`], then `copies` of the finished
    /// picture on top (at most [`MAX_COPIES`]), e.g. a magnifier.
    #[allow(clippy::too_many_arguments)]
    pub fn render_composed(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        cameras: &[Camera],
        copies: &[ScreenCopy],
        frame: u32,
        clear: [u8; 4],
    ) {
        let present = Present::unscaled((target.width(), target.height()));
        self.render_presented(device, queue, encoder, target, cameras, copies, frame, clear, present);
    }

    /// [`TileRenderer::render_composed`] into a picture of `present.canvas`
    /// (cameras and copies in its pixels), shown on `target` as `present`
    /// says: `zoom` times larger, by nearest pixel.
    #[allow(clippy::too_many_arguments)]
    pub fn render_presented(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        cameras: &[Camera],
        copies: &[ScreenCopy],
        frame: u32,
        clear: [u8; 4],
        present: Present,
    ) {
        self.render_layered(device, queue, encoder, target, cameras, cameras.len(), copies, frame, clear, present);
    }

    /// [`TileRenderer::render_presented`], where the views from
    /// `cameras[overlay_from]` on are *overlays* that draw only markers
    /// over everything (a cursor, a frame counter): a frame whose other
    /// views, the markers in them, the world and the tick are those of the
    /// last frame shows the picture drawn then (of a world composed
    /// internally: a quantized one, one with copies, or one shown scaled),
    /// with the overlays drawn anew over it. (`TILE_RENDER_REUSE=0`: every
    /// frame drawn whole.)
    #[allow(clippy::too_many_arguments)]
    pub fn render_layered(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        cameras: &[Camera],
        overlay_from: usize,
        copies: &[ScreenCopy],
        frame: u32,
        clear: [u8; 4],
        present: Present,
    ) -> bool {
        let view = target.create_view(&Default::default());
        let (Some(bind_group), Some(world)) = (&self.bind_group, &self.world) else {
            drop(begin_pass(encoder, &view, wgpu::LoadOp::Clear(crate::gpu::color(clear)), None));
            return true;
        };
        let Some(control_slot) = self.claim_control_frame() else { return false };
        let bind_group = &bind_group[control_slot];
        if cameras.len() > MAX_VIEWS && !self.too_many_views {
            log::warn!("{} views, only the first {MAX_VIEWS} are drawn (not repeated)", cameras.len());
            render_errors::report(render_errors::Kind::ViewsLeftOut, "renderer cameras", format_args!("{} views, only the first {MAX_VIEWS} drawn", cameras.len()));
        }
        self.too_many_views |= cameras.len() > MAX_VIEWS;
        let cameras = &cameras[..cameras.len().min(MAX_VIEWS)];
        let bits = world.color.bits();
        let internal_format = if bits != 0 { self.quantized_format() } else { QUANTIZED_FORMAT };
        let float_encoding = u32::from(bits != 0 && self.quantized_float);
        let target_size = (target.width(), target.height());
        let unscaled = present == Present::unscaled(target_size);
        // What is drawn: the canvas, shown on the target afterwards.
        let (canvas_w, canvas_h) = (present.canvas.0.max(1), present.canvas.1.max(1));
        let size = wgpu::Extent3d { width: canvas_w, height: canvas_h, depth_or_array_layers: 1 };
        // Background in the canvas's values.
        let background = if bits == 0 {
            crate::gpu::color(clear)
        } else {
            let q = |c: u8| if float_encoding != 0 { f64::from(tile_render_shared::quantize(c.into(), bits) + tile_render_shared::FLOAT_QUANTIZED_BIAS) } else { f64::from(tile_render_shared::quantize(c.into(), bits) + tile_render_shared::QUANTIZED_BIAS) / 255.0 };
            wgpu::Color { r: q(clear[0]), g: q(clear[1]), b: q(clear[2]), a: f64::from(clear[3]) / 255.0 }
        };
        let write_output = |slot: usize, params: OutputParams| {
            let offset = slot as u64 * PARAMS_STRIDE;
            control_bytes(&self.control_uploads[control_slot].output, offset, bytemuck::bytes_of(&params));
            offset as u32
        };
        // Scaled groups' pictures, side by side in rows.
        let placements = crate::camera::placements(cameras, |z, view, camera| crate::world::zoomed_from(&world.first_zoomed, z, view, camera));
        if world.smoke_count != 0 {
            let draws = world.smoke_count.checked_mul(cameras.len().max(1)).and_then(|n| n.checked_mul(2)).expect("native palette control bound");
            let mut smoke = self.smoke.borrow_mut();
            smoke.get_or_insert_with(|| smoke::State::new(device, draws)).begin(device, control_slot, draws);
        }
        let copies = &copies[..copies.len().min(MAX_COPIES)];
        // Copies read the finished picture, and a scaled one is shown from
        // it: then it is composed internally. Lenses only need a bounded
        // backdrop copy; use the caller's target directly when copyable.
        let direct = bits == 0 && copies.is_empty() && unscaled && world.smoke_count == 0 && (cameras.iter().all(|c| c.lens.is_none()) || target.usage().contains(wgpu::TextureUsages::COPY_SRC));
        // The picture of the views but the overlays drawn last, if this
        // frame's are the same (then only the overlays are drawn, over a
        // copy of it without the last frame's).
        // The views drawn onto the target after the picture
        // (`Camera::on_target`; with the others where the target is the
        // picture). They neither touch the canvas nor make it be kept.
        let on_target = if direct { cameras.len() } else { cameras.iter().position(|c| c.on_target).unwrap_or(cameras.len()) };
        debug_assert!(cameras[on_target..].iter().all(|c| c.on_target && !c.cells), "on-target views come last and show no world");
        let overlay_from = overlay_from.min(on_target);
        let overlays = overlay_from < on_target;
        let markers = self.markers.iter().filter(|m| m.view == tile_render_shared::ALL_VIEWS || (m.view as usize) < overlay_from || m.flags & tile_render_shared::FREE_EXCEPT_VIEW != 0);
        let markers: Vec<_> = markers.copied().collect();
        let period = host_period(markers.iter().map(|s| s.ticks_per_frame)
            .chain((world.host_period_ms != 0).then_some(world.host_period_ms | tile_render_shared::HOST_CLOCK))
            .chain((world.layer_period_ms != 0).then_some(world.layer_period_ms | tile_render_shared::HOST_CLOCK)));
        let host_frame = if period == 0 { 0 } else { self.host_tick_ms / period };
        let key = FrameKey { generation: self.generation, markers, marker_text: self.marker_sources.iter().filter(|m|
            m.text.is_some() && (m.view.is_none_or(|v| (v as usize) < overlay_from) || m.except_view)).copied().collect(), cameras: cameras[..overlay_from].to_vec(), frame, host_frame, clear, size: (size.width, size.height) };
        // A scaled overlay may share its source picture with a preceding
        // view. Restoring the canvas alone cannot restore that group's
        // unfinished source picture, so draw these arrangements in full.
        // (One drawn at its zoom from its start has an empty picture.)
        let cacheable = !direct && placements[overlay_from..on_target].iter().all(|p| p.source.is_none_or(|r| r[2] == 0 || r[3] == 0));
        let same = cacheable && reuse_frames() && self.drawn.as_ref() == Some(&key) && self.scratch.is_some();
        let reuse = same && (!self.overlaid || self.base.is_some());
        // An overlay can return after a newer picture was drawn without
        // overlays. Keep that current picture, even when it was reused.
        let capture_base = cacheable && overlays && (!reuse || !self.overlaid);
        self.reused = reuse;
        if capture_base {
            let fits = self.base.as_ref().is_some_and(|b| (b.width(), b.height()) == (size.width, size.height) && b.format() == internal_format);
            if !fits {
                self.base = Some(device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("tile-render canvas without overlays"),
                    size: wgpu::Extent3d { width: size.width, height: size.height, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: internal_format,
                    usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                }));
            }
        }
        // The views drawn through a source texture need it even when the
        // picture is reused: its overlays are drawn again, and one may have
        // an empty source (which a reused picture allows) on a renderer that
        // never made the texture yet.
        if placements.iter().any(|p| p.source.is_some()) {
            let source_extent = placements.iter().filter_map(|p| p.source).fold((1, 1), |(w, h), r| (w.max(r[0] + r[2]), h.max(r[1] + r[3])));
            let s = scratch_for(&mut self.sources, device, &self.output_layout, &self.output_params, source_extent, true, internal_format);
            if !reuse {
                let timed = self.timer.as_ref().and_then(|t| t.pass(|| "clear sources".into()));
                drop(begin_pass(encoder, &s.main.create_view(&Default::default()), wgpu::LoadOp::Clear(background), timed));
            }
        }
        let canvas = if direct {
            drop(begin_pass(encoder, &view, wgpu::LoadOp::Clear(background), self.timed(|| "clear".into())));
            view.clone()
        } else {
            let s = scratch_for(&mut self.scratch, device, &self.output_layout, &self.output_params, (size.width, size.height), false, internal_format);
            let main = s.main.create_view(&Default::default());
            if !reuse {
                let timed = self.timer.as_ref().and_then(|t| t.pass(|| "clear".into()));
                drop(begin_pass(encoder, &main, wgpu::LoadOp::Clear(background), timed));
            } else if self.overlaid {
                // (The last frame's overlays off.)
                let (base, canvas) = (self.base.as_ref().expect("checked above"), &s.main);
                encoder.copy_texture_to_texture(base.as_image_copy(), canvas.as_image_copy(), canvas.size());
            }
            main
        };
        self.drawn = cacheable.then_some(key);
        self.overlaid = cacheable && overlays;
        let source_view = self.sources.as_ref().map(|s| s.main.create_view(&Default::default()));
        // The part of rectangle (x, y, w, h) inside `bounds` (x, y, w, h).
        let clipped = |[x, y, w, h]: [i32; 4], [bx, by, bw, bh]: [i32; 4]| {
            let (x0, y0) = (x.max(bx).max(0), y.max(by).max(0));
            let (x1, y1) = ((x + w).min(bx + bw), (y + h).min(by + bh));
            (x1 > x0 && y1 > y0).then_some([x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
        };
        let target_rect = [0, 0, size.width as i32, size.height as i32];
        // Pipelines into the canvas, and into the source texture.
        // tile_vs emits no tint, background sampling or procedural mode.
        // Only RGB555 cells in UNORM8 composition use tile555_fs; all free
        // sprites and other cell formats retain their generic pipelines.
        let (canvas_tiles, canvas_free) = match () {
            _ if direct => (self.pipeline(device, P::Tiles), self.free_pipelines(device, 0)),
            _ if bits == 0 => (self.pipeline(device, P::TilesSource), self.free_pipelines(device, 2)),
            _ => (self.pipeline(device, if self.tiles555 && bits == 5 && !self.quantized_float { P::Tiles555 } else { P::TilesQuantized }), self.free_pipelines(device, 1)),
        };
        let source_pipelines = if bits == 0 {
            (self.pipeline(device, P::TilesSource), self.free_pipelines(device, 2))
        } else {
            (self.pipeline(device, if self.tiles555 && bits == 5 && !self.quantized_float { P::Tiles555 } else { P::TilesQuantized }), self.free_pipelines(device, 1))
        };
        let steps = Steps { renderer: self, device, control_slot, params: &self.control_uploads[control_slot].params, world, bind_group, frame, named_palettes: self.named_palettes, timer: self.timer.as_ref(), quantized_float: self.quantized_float, cell_indices: &self.cell_indices, onto_target: false };
        if let Some(lens) = self.lens.borrow_mut().as_mut() {
            lens.begin_frame(if reuse { overlay_from } else { 0 });
        }
        for (v, place) in placements.iter().enumerate().take(on_target) {
            if reuse && v < overlay_from {
                continue;
            }
            // The picture without the overlays, kept.
            if capture_base && v == overlay_from {
                let (base, canvas) = (self.base.as_ref().expect("made above"), &self.scratch.as_ref().expect("made above").main);
                encoder.copy_texture_to_texture(canvas.as_image_copy(), base.as_image_copy(), canvas.size());
            }
            // Snapshot only this lens's bounds before its camera draws.
            let lens_params = if cameras[v].lens.is_some() {
                let texture = if direct { target } else { &self.scratch.as_ref().expect("internal canvas").main };
                let mut lens = self.lens.borrow_mut();
                if lens.as_ref().is_some_and(|state| state.format != texture.format()) { *lens = None; }
                lens.get_or_insert_with(|| lens::State::new(device, &self.shader, texture.format()))
                    .snapshot(device, queue, encoder, texture, &cameras[v], control_slot, v, bits, float_encoding)
            } else { None };
            // Its steps at zoom 1.
            let rect = place.rect;
            let scissor = match place.source {
                Some(r) => clipped(place.scissor, [r[0] as i32, r[1] as i32, r[2] as i32, r[3] as i32]),
                None => clipped(place.scissor, target_rect),
            };
            let (into, texture, pipelines) = match place.source {
                None => (&canvas, if direct { target } else { &self.scratch.as_ref().expect("created above").main }, (canvas_tiles, canvas_free)),
                Some(_) => (source_view.as_ref().expect("created above"), &self.sources.as_ref().expect("created above").main, source_pipelines),
            };
            // Which zoom art the view draws (see `FrameParams::zoom_art`).
            let art = u32::from(place.art);
            if let Some(scissor) = scissor {
                let viewport = [rect[0] as f32, rect[1] as f32, rect[2] as f32, rect[3] as f32];
                steps.draw(encoder, into, texture, (viewport, scissor), &place.drawn, v, (art, None), place.steps_at_one(), 0, pipelines);
            }
            // The group's picture, scaled into its rectangle.
            if let Some((group, r)) = place.blit {
                let group_rect = [group.origin.0, group.origin.1, group.viewport.0 as i32, group.viewport.1 as i32];
                let clip = group.target_clip.map_or(target_rect, |[x0, y0, x1, y1]| {
                    let (x, y) = (x0.max(target_rect[0]), y0.max(target_rect[1]));
                    [x, y, (x1.min(target_rect[0] + target_rect[2]) - x).max(0), (y1.min(target_rect[1] + target_rect[3]) - y).max(0)]
                });
                if let Some(on_target) = clipped(group_rect, clip) {
                    let params = OutputParams {
                        src_x: r[0] as i32,
                        src_y: r[1] as i32,
                        dst_x: group.origin.0,
                        dst_y: group.origin.1,
                        zoom_num: group.picture_zoom().0,
                        zoom_den: group.picture_zoom().1,
                        bits: 0,
                        quantized_float: 0,
                        ..Default::default()
                    };
                    let offset = write_output(v, params);
                    let mut pass = begin_pass(encoder, &canvas, wgpu::LoadOp::Load, self.timed(|| format!("v{v} blit")));
                    pass.set_pipeline(self.pipeline(device, if direct { P::Output } else if bits != 0 { P::BlitQuantized } else { P::Blit }));
                    pass.set_scissor_rect(on_target[0], on_target[1], on_target[2], on_target[3]);
                    pass.set_bind_group(0, &self.sources.as_ref().expect("created above").output[control_slot], &[offset]);
                    pass.draw(0..3, 0..1);
                }
            }
            // Its steps at its zoom, straight into the canvas (zoomed art at
            // full resolution).
            let s = place.scaled_scissor;
            if let (true, Some(scissor)) = (place.scaled_from.is_some(), clipped([s[0], s[1], s[2] - s[0], s[3] - s[1]], target_rect)) {
                let c = &place.scaled;
                let viewport = [c.origin.0 as f32, c.origin.1 as f32, c.viewport.0 as f32, c.viewport.1 as f32];
                let art = if place.art { 2 } else { 0 };
                steps.draw(encoder, &canvas, if direct { target } else { &self.scratch.as_ref().expect("canvas").main }, (viewport, scissor), c, v, (art, Some(c.origin)), place.steps_scaled(), MAX_PASSES + 1, (canvas_tiles, canvas_free));
            }
            if let Some(params) = lens_params {
                let lens = self.lens.borrow();
                let lens = lens.as_ref().unwrap();
                let texture = if direct { target } else { &self.scratch.as_ref().unwrap().main };
                lens.capture(encoder, texture, params, v, false);
                lens.draw(device, encoder, &canvas, &cameras[v], params, control_slot, v, None, self.timed(|| format!("v{v} lens rim")));
                lens.capture(encoder, texture, params, v, true);
            }
        }
        if direct {
            // This frame was drawn into the caller's texture, so a previous
            // internal canvas no longer represents the current picture.
            self.scratch = None;
            self.flush_controls(queue, control_slot);
            return true;
        }
        self.picture_bits = bits;
        let shown = self.output_picture(device, queue, encoder, target, copies, clear, present, self.timed(|| "output".into()));
        debug_assert!(shown, "a composed frame has an internal canvas");
        // The views on the target, in its pixels and colours.
        if on_target < cameras.len() {
            let onto = Steps { renderer: self, device, control_slot, params: &self.control_uploads[control_slot].params, world, bind_group, frame, named_palettes: self.named_palettes, timer: self.timer.as_ref(), quantized_float: false, cell_indices: &self.cell_indices, onto_target: true };
            let whole = [0, 0, target_size.0 as i32, target_size.1 as i32];
            let pipelines = (self.pipeline(device, P::Tiles), self.free_pipelines(device, 0));
            for (v, place) in placements.iter().enumerate().skip(on_target) {
                let rect = place.rect;
                if let Some(scissor) = clipped(place.scissor, whole) {
                    let viewport = [rect[0] as f32, rect[1] as f32, rect[2] as f32, rect[3] as f32];
                    onto.draw(encoder, &view, target, (viewport, scissor), &place.drawn, v, (u32::from(place.art), None), place.steps_at_one(), 0, pipelines);
                }
            }
        }
        self.flush_controls(queue, control_slot);
        true
    }

    /// Presents the most recently composed picture without drawing its
    /// world again. This can be called after replacing the atlas while
    /// the matching scene is still being prepared: it reads no atlas or
    /// sprite tables and leaves the retained canvas unchanged.
    ///
    /// The retained canvas supplies its own source size (`present.canvas`
    /// is ignored). `present.zoom`, `present.at`, copies and clear colour
    /// work as in [`TileRenderer::render_presented`], clipped to the actual
    /// target size. Colour decoding stays with the retained canvas even
    /// when a replacement world has already been uploaded.
    ///
    /// Returns false, without recording commands, before a composed frame
    /// exists or after a frame drawn directly into the caller's target.
    #[allow(clippy::too_many_arguments)]
    pub fn render_cached_picture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        copies: &[ScreenCopy],
        clear: [u8; 4],
        present: Present,
    ) -> bool {
        if self.scratch.is_none() { return false }
        let Some(slot) = self.claim_control_frame() else { return false };
        let shown = self.output_picture(device, queue, encoder, target, copies, clear, present, None);
        self.flush_controls(queue, slot);
        shown
    }

    /// Shared output path. Replays do not allocate timer queries because
    /// they need no timing-frame lifecycle; normal rendering supplies them.
    #[allow(clippy::too_many_arguments)]
    fn output_picture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        copies: &[ScreenCopy],
        clear: [u8; 4],
        present: Present,
        timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) -> bool {
        let Some(scratch) = &self.scratch else { return false };
        let control_slot = self.controls.active.get();
        let bits = self.picture_bits;
        let target_size = (target.width(), target.height());
        let (canvas_w, canvas_h) = (scratch.main.width(), scratch.main.height());
        // A window may resize while another art generation is prepared.
        // The retained source still has its previous dimensions.
        let present = Present { canvas: (canvas_w, canvas_h), ..present };
        let unscaled = present == Present::unscaled(target_size);
        let view = target.create_view(&Default::default());
        let write_output = |slot: usize, params: OutputParams| {
            let offset = slot as u64 * PARAMS_STRIDE;
            control_bytes(&self.control_uploads[control_slot].output, offset, bytemuck::bytes_of(&params));
            offset as u32
        };
        let clipped = |[x, y, w, h]: [i32; 4], [bx, by, bw, bh]: [i32; 4]| {
            let (x0, y0) = (x.max(bx).max(0), y.max(by).max(0));
            let (x1, y1) = ((x + w).min(bx + bw), (y + h).min(by + bh));
            (x1 > x0 && y1 > y0).then_some([x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
        };
        let copies = &copies[..copies.len().min(MAX_COPIES)];
        let (n, d) = (present.zoom.0.max(1), present.zoom.1.max(1));
        let (ax, ay) = present.at;
        let params = OutputParams { dst_x: ax, dst_y: ay, zoom_num: n, zoom_den: d, bits, quantized_float: u32::from(self.quantized_float && bits != 0), ..Default::default() };
        let offset = write_output(MAX_VIEWS, params);
        // Around a scaled picture: clear.
        let load = if unscaled { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(crate::gpu::color(clear)) };
        let mut pass = begin_pass(encoder, &view, load, timestamp_writes);
        // Canvas pixels as target pixels: the first target pixel showing
        // canvas pixel `v` (target pixel p shows canvas pixel ⌊(p − at)·d/n⌋).
        let to_target = |v: u32| (v as u64 * n as u64).div_ceil(d as u64) as i32;
        // A canvas rectangle (x, y, w, h) as shown on the target, clipped to it.
        let shown = |[x, y, w, h]: [u32; 4]| {
            let (x0, y0) = (ax + to_target(x), ay + to_target(y));
            let r = [x0, y0, ax + to_target(x + w) - x0, ay + to_target(y + h) - y0];
            clipped(r, [0, 0, target_size.0 as i32, target_size.1 as i32])
        };
        let Some(picture) = shown([0, 0, canvas_w, canvas_h]) else { return true };
        pass.set_pipeline(self.pipeline(device, P::Output));
        pass.set_scissor_rect(picture[0], picture[1], picture[2], picture[3]);
        pass.set_bind_group(0, &scratch.output[control_slot], &[offset]);
        pass.draw(0..3, 0..1);
        drop(pass);
        if let Some(lens) = self.lens.borrow().as_ref() {
            lens.draw_presented(device, queue, encoder, target, &scratch.main, present, control_slot, self.timer.as_ref());
        }
        if copies.is_empty() { return true; }
        let mut pass = begin_pass(encoder, &view, wgpu::LoadOp::Load, self.timed(|| "screen copies".into()));
        pass.set_pipeline(self.pipeline(device, P::Output));
        for (i, c) in copies.iter().enumerate() {
            // (A copy of a scaled picture samples it at the target's pixels,
            // not the canvas's.)
            let Some(mut on_target) = c.scissor((canvas_w, canvas_h)).and_then(shown) else { continue };
            let (zoom_num, zoom_den) = (c.zoom.0.max(1) * n, c.zoom.1.max(1) * d);
            let [src_x, src_y, _, _] = c.source;
            let at = |v: i32, a: i32| a + (v as i64 * n as i64).div_euclid(d as i64) as i32;
            let (dst_x, dst_y) = (at(c.at.0, ax), at(c.at.1, ay));
            let mut params = OutputParams { src_x, src_y, dst_x, dst_y, zoom_num, zoom_den, bits, quantized_float: u32::from(self.quantized_float && bits != 0), ..Default::default() };
            if let Some(lens) = c.lens {
                let factor = n as f32 / d as f32;
                params.lens_width = (c.source[2] as u32 * c.zoom.0).div_ceil(c.zoom.1) as f32 * factor;
                params.lens_height = (c.source[3] as u32 * c.zoom.0).div_ceil(c.zoom.1) as f32 * factor;
                params.lens_radius = lens.radius * factor;
                params.lens_rim = lens.rim * factor;
                params.lens_glass = u32::from(lens.glass);
                params.canvas_x = ax;
                params.canvas_y = ay;
                params.canvas_num = n;
                params.canvas_den = d;
                params.canvas_width = canvas_w;
                params.canvas_height = canvas_h;
                let halo = if lens.glass { (params.lens_rim * 0.28).ceil() as i32 } else { 0 };
                let [x, y, w, h] = on_target.map(|v| v as i32);
                let Some(expanded) = clipped([x-halo, y-halo, w+2*halo, h+2*halo], picture.map(|v| v as i32)) else { continue };
                on_target = expanded;
            }
            let offset = write_output(MAX_VIEWS + 1 + i, params);
            pass.set_scissor_rect(on_target[0], on_target[1], on_target[2], on_target[3]);
            pass.set_bind_group(0, &scratch.output[control_slot], &[offset]);
            pass.draw(0..3, 0..1);
        }
        true
    }
}

/// How a finished picture goes onto its target
/// ([`TileRenderer::render_presented`]): drawn at `canvas` size, shown
/// `zoom` (num, den) times larger by nearest pixel (no smoothing: at a whole
/// number each pixel is a crisp block), its top-left at target pixel `at`;
/// the rest of the target cleared. `target` is the target's size.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Present {
    pub canvas: (u32, u32),
    pub zoom: (u32, u32),
    pub at: (i32, i32),
    pub target: (u32, u32),
}

impl Present {
    /// 1:1 onto a target of `size`, filling it.
    pub fn unscaled(size: (u32, u32)) -> Self {
        Present { canvas: size, zoom: (1, 1), at: (0, 0), target: size }
    }

    /// Target pixels per canvas pixel.
    pub fn factor(&self) -> f64 {
        self.zoom.0.max(1) as f64 / self.zoom.1.max(1) as f64
    }

    /// Target pixels per canvas pixel where that is a whole number above 1
    /// (each canvas pixel shown as a `k`×`k` box).
    pub fn pixel_box(&self) -> Option<u32> {
        let (n, d) = (self.zoom.0.max(1), self.zoom.1.max(1));
        (n % d == 0 && n / d > 1).then_some(n / d)
    }

    /// The same presentation of a canvas composed at the target's
    /// resolution: [`Present::pixel_box`] times larger each way, shown 1:1
    /// where the boxes were (its cameras [`Camera::boxed`], its copies
    /// [`ScreenCopy::boxed`]). As it is without whole boxes.
    ///
    /// [`Camera::boxed`]: crate::Camera::boxed
    pub fn at_target(&self) -> Present {
        let Some(k) = self.pixel_box() else { return *self };
        Present { canvas: (self.canvas.0.saturating_mul(k), self.canvas.1.saturating_mul(k)), zoom: (1, 1), ..*self }
    }

    /// The canvas pixel under target point `p` (e.g. the pointer). Points
    /// on the target beside the picture count as on its nearest edge pixel
    /// (the target's edge is the picture's), its far boundary too (macOS
    /// reports the pointer resting on the bottom row as `y == height`);
    /// points further off the target are mapped as they are.
    pub fn to_canvas(&self, (x, y): (f64, f64)) -> (i32, i32) {
        let s = self.factor();
        let axis = |v: f64, at: i32, len: u32, t: u32| {
            let c = ((v - at as f64) / s).floor() as i64;
            let on_target = v >= 0.0 && v <= t as f64;
            (if on_target { c.clamp(0, len.max(1) as i64 - 1) } else { c }) as i32
        };
        (axis(x, self.at.0, self.canvas.0, self.target.0), axis(y, self.at.1, self.canvas.1, self.target.1))
    }
}

/// Most [`ScreenCopy`]s a frame may have.
pub const MAX_COPIES: usize = 4;

/// A rectangle of the finished picture copied on top of it, scaled by
/// nearest pixel (a magnifier).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ScreenCopy {
    /// Source rectangle in target pixels: x, y, width, height.
    pub source: [i32; 4],
    /// Top-left of the copy in the target.
    pub at: (i32, i32),
    /// Target pixels per source pixel, as a fraction.
    pub zoom: (u32, u32),
    /// Optional rounded glass edge, in canvas pixels.
    pub lens: Option<crate::Lens>,
}

impl ScreenCopy {
    /// This copy of a canvas `k` times as large each way (see
    /// `Camera::boxed`): the same part of the picture, at the same zoom.
    pub fn boxed(&self, k: u32) -> ScreenCopy {
        let k = k.max(1) as i32;
        ScreenCopy { source: self.source.map(|v| v.saturating_mul(k)), at: (self.at.0.saturating_mul(k), self.at.1.saturating_mul(k)), zoom: self.zoom, lens: self.lens.map(|l| crate::Lens { radius: l.radius * k as f32, rim: l.rim * k as f32, ..l }) }
    }

    /// Target pixel `p` of the copy shows source pixel `source + (p − at)·den/num`
    /// (per axis). The target pixels (x, y, w, h) that are inside a target of
    /// `size` and show a source pixel inside it, if any.
    pub fn scissor(&self, (w, h): (u32, u32)) -> Option<[u32; 4]> {
        let (n, d) = (self.zoom.0.max(1) as i64, self.zoom.1.max(1) as i64);
        // Offsets o ≥ 0 from `at` with s + floor(o·d/n) in [max(s, 0), min(s + len, limit)).
        let axis = |s: i64, len: i64, at: i64, limit: i64| {
            let lo = (-s).max(0); // first source offset
            let hi = len.min(limit - s); // past the last
            if hi <= lo {
                return None;
            }
            // floor(o·d/n) ≥ lo ⇔ o ≥ ceil(lo·n/d); < hi ⇔ o < ceil(hi·n/d).
            let (o0, o1) = ((lo * n + d - 1) / d, (hi * n + d - 1) / d);
            let (t0, t1) = ((at + o0).max(0), (at + o1).min(limit));
            (t1 > t0).then_some((t0 as u32, (t1 - t0) as u32))
        };
        let [sx, sy, sw, sh] = self.source.map(i64::from);
        let (x, cw) = axis(sx, sw, self.at.0 as i64, w as i64)?;
        let (y, ch) = axis(sy, sh, self.at.1 as i64, h as i64)?;
        Some([x, y, cw, ch])
    }
}


/// Where free sprites are drawn, to get their pipelines from.
#[derive(Copy, Clone)]
struct FreePipelines<'a> {
    renderer: &'a TileRenderer,
    device: &'a wgpu::Device,
    into: u8,
}

impl FreePipelines<'_> {
    /// The pipelines a run of `kind` is drawn with, in order (ClearType-like
    /// sprites once per colour channel).
    fn of(&self, kind: FreeKind) -> impl Iterator<Item = &wgpu::RenderPipeline> {
        let get = |p| self.renderer.pipeline(self.device, p);
        let one = |p| [Some(get(p)), None, None];
        let pipelines = match (kind, self.into) {
            (FreeKind::NativePaletteAlpha, into) => one(P::NativePaletteAlpha(into)),
            (FreeKind::Normal, 0) => one(P::Free),
            (FreeKind::Normal, 1) => one(P::FreeQuantized),
            (FreeKind::Normal, _) => one(P::FreeSource),
            (FreeKind::Invert, 0) => one(P::FreeInvert),
            (FreeKind::Invert, 1) => one(P::FreeInvertQuantized),
            (FreeKind::Invert, _) => one(P::FreeInvertInternal),
            (FreeKind::Subpixel, into) if self.renderer.subpixel_dual => one(P::SubpixelDual(into)),
            (FreeKind::Subpixel, into) => std::array::from_fn(|c| Some(get(P::Subpixel(into, c as u8)))),
            (FreeKind::Text, into) if self.device.features().contains(wgpu::Features::DUAL_SOURCE_BLENDING) => one(P::TextDual(into)),
            (FreeKind::Text, into) => std::array::from_fn(|c| Some(get(P::Text(into, c as u8)))),
            (FreeKind::TextKeyed, into) => one(P::TextKeyed(into)),
        };
        pipelines.into_iter().flatten()
    }
}

/// Recomputed only when command metadata changes. Its GCD supplies a
/// conservative retained-picture boundary for every flagged animation.
fn host_period(periods: impl IntoIterator<Item = u32>) -> u32 {
    let gcd = |mut a: u32, mut b: u32| { while b != 0 { (a, b) = (b, a % b); } a };
    periods.into_iter().filter(|p| p & tile_render_shared::HOST_CLOCK != 0)
        .map(|p| (p & !tile_render_shared::HOST_CLOCK).max(1)).fold(0, gcd)
}

impl TileRenderer {
    /// A scalar uniform clock. It never changes or uploads scene commands.
    pub fn set_host_tick_ms(&mut self, milliseconds: u32) { self.host_tick_ms = milliseconds; }
}

/// What drawing a view's steps (see `camera::Placement`) needs.
struct Steps<'a> {
    renderer: &'a TileRenderer,
    device: &'a wgpu::Device,
    control_slot: usize,
    params: &'a ControlBytes,
    world: &'a WorldBuffers,
    bind_group: &'a wgpu::BindGroup,
    frame: u32,
    named_palettes: u32,
    timer: Option<&'a crate::timing::GpuTimer>,
    quantized_float: bool,
    cell_indices: &'a wgpu::Buffer,
    /// Drawing onto the caller's target (`Camera::on_target`): in direct
    /// colour whatever the world's colour model, no native palette steps.
    onto_target: bool,
}

impl Steps<'_> {
    /// Draws the `steps` of view `v` seen by `camera` into `into` (its
    /// viewport and the part to touch), with `art` (`FrameParams::zoom_art`)
    /// and `scaled_at` (`Camera::frame_params`), its parameters in the view's
    /// slots from `slot`.
    #[allow(clippy::too_many_arguments)]
    fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        into: &wgpu::TextureView,
        texture: &wgpu::Texture,
        bounds: ([f32; 4], [u32; 4]),
        camera: &Camera,
        v: usize,
        (art, scaled_at): (u32, Option<(i32, i32)>),
        steps: std::ops::Range<usize>,
        slot: usize,
        (tiles, free): (&wgpu::RenderPipeline, FreePipelines<'_>),
    ) {
        let glyph_cache = self.prepare_glyph_cache(encoder, camera, v, art, scaled_at, bounds.0, &steps);
        let world = self.world;
        let passes = world.passes.len().min(MAX_PASSES);
        // One render pass for all the steps (draws blend in order within
        // it; a pass begun and ended per step costs the GPU), unless each
        // step is timed on its own (`TILE_RENDER_GPU_TIMING=passes`).
        let split = self.timer.is_some() && crate::timing::per_pass();
        let mut render: Option<wgpu::RenderPass<'static>> = None;
        for i in 0..=passes {
            let cells = i < passes && camera.cells && steps.contains(&(2 * i));
            let drawable = |(_, kind, class): &(std::ops::Range<u32>, FreeKind, u32)| free_run_visible(*kind, *class, v as u32, camera.cells, art);
            let sprites = steps.contains(&(2 * i + 1)) && world.free_after.get(i).is_some_and(|r| r.iter().any(drawable));
            if !cells && !sprites {
                continue;
            }
            let pass = world.passes.get(i).copied().filter(|_| i < passes).unwrap_or_default();
            let octagons = i < passes && world.octagons.get(i).copied().unwrap_or(false);
            let corner_cuts = octagons as u32;
            let params = FrameParams {
                named_palettes: self.named_palettes, corner_cuts,
                host_tick_ms: self.renderer.host_tick_ms,
                quantized_float: u32::from(self.quantized_float && world.color.bits() != 0 && !self.onto_target),
                // Text coverage is viewport-local even when this ordinary
                // stage draws into an offset source or target rectangle.
                target_x: bounds.0[0] as i32, target_y: bounds.0[1] as i32,
                // The layers as the view's binding draws them.
                layer_table: crate::world::table_of(&world.tables, camera.binding),
                ..camera.frame_params(&world.grid, if self.onto_target { crate::ColorModel::Direct } else { world.color }, self.frame, &pass, v as u32, art, scaled_at, world.reach)
            };
            let sprites = sprites && (world.smoke_count == 0 || self.onto_target || self.native_step_visible(i, &params, bounds, texture, (v as u32, camera.cells)));
            if !cells && !sprites { continue; }
            // Pack only steps that actually draw, with one unique aligned
            // block per draw. This keeps the single upload small even when
            // the view only uses its final free-sprite step.
            let offset = self.params.bytes.borrow().len().next_multiple_of(PARAMS_STRIDE as usize) as u64;
            control_bytes(self.params, offset, bytemuck::bytes_of(&params));
            if split {
                drop(render.take());
            }
            let active = match &mut render {
                Some(r) => {
                    r.set_bind_group(0, self.bind_group, &[offset as u32]);
                    r
                }
                None => {
                    let timed = self.timer.and_then(|t| {
                        let zoomed = if slot == 0 { "" } else { " zoomed" };
                        let what = if cells && sprites { "cells+free" } else if cells { "cells" } else { "free" };
                        t.pass(|| if split { format!("v{v}{zoomed} pass {i} {what}") } else { format!("v{v}{zoomed}") })
                    });
                    // (The encoder is locked while the pass lives, until it is dropped.)
                    render.insert(begin_view(encoder, into, bounds, self.bind_group, offset as u32, timed).forget_lifetime())
                }
            };
            if cells {
                active.set_pipeline(tiles);
                // (Indexed: the vertex shader runs once per corner.)
                let indices = if octagons { 6..6 + tile_render_shared::OCTAGON_VERTICES } else { 0..QUAD_VERTICES };
                active.set_index_buffer(self.cell_indices.slice(..), wgpu::IndexFormat::Uint16);
                active.draw_indexed(indices, 0, 0..params.cols * params.rows * pass.count);
            }
            if sprites {
                if world.smoke_count == 0 || self.onto_target {
                    draw_free(active, world, i, free, (v as u32, camera.cells, art), glyph_cache.as_ref());
                } else {
                    self.draw_native_free(encoder, &mut render, into, texture, bounds, &params, offset as u32, i, free, (v as u32, camera.cells), glyph_cache.as_ref());
                }
            }
        }
    }
}

/// Free sprites that follow pass `i`, into `pass` (whose bind group is
/// already set), each run with its pipelines.
fn draw_free(pass: &mut wgpu::RenderPass<'_>, world: &WorldBuffers, i: usize, pipelines: FreePipelines<'_>, (view, cells, art): (u32, bool, u32), glyph_cache: Option<&wgpu::BindGroup>) {
    for (range, kind, class) in world.free_after.get(i).into_iter().flatten() {
        // (A run for other views draws nothing here: not drawn at all.)
        if !free_run_visible(*kind, *class, view, cells, art) {
            continue;
        }
        if matches!(kind, FreeKind::Text | FreeKind::TextKeyed) { pass.set_bind_group(1, glyph_cache.expect("text cache prepared"), &[]); }
        for p in pipelines.of(*kind) {
            pass.set_pipeline(p);
            pass.draw(0..QUAD_VERTICES, range.clone());
        }
    }
}

/// How a run of free sprites is drawn.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum FreeKind {
    Normal,
    NativePaletteAlpha,
    Invert,
    Subpixel,
    /// Text drawn from outlines, ClearType-like (its range indexes the text
    /// buffer).
    Text,
    /// Text drawn from outlines onto a colour key (likewise).
    TextKeyed,
}

/// Runs of free sprites in drawing order: which instances, and how they
/// are drawn.
/// (A run's view class, `view_class`.)
type FreeRuns = Vec<(std::ops::Range<u32>, FreeKind, u32)>;

/// Coarse draw eligibility shared by cache preparation and both draw paths.
/// Text's source-art stage emits only culled vertices (visible_at_art).
fn free_run_visible(kind: FreeKind, class: u32, view: u32, cells: bool, art: u32) -> bool {
    (class == view || (class == tile_render_shared::ALL_VIEWS && cells))
        && (!matches!(kind, FreeKind::Text | FreeKind::TextKeyed) || matches!(art, 0 | 2))
}

/// Which views free sprite `s` may be drawn in: its own one only (its
/// index), or (`ALL_VIEWS`) those showing the world, maybe but one.
fn view_class(s: &crate::PlacedSprite) -> u32 {
    match s.view {
        Some(v) if !s.except_view => v,
        _ => tile_render_shared::ALL_VIEWS,
    }
}

/// Pack a small overlay command list without touching canonical world data.
fn pack_markers(markers: &[crate::PlacedSprite], fonts: &[std::sync::Arc<gpu_text::Face>], bases: &[u32])
    -> (Vec<FreeSprite>, Vec<tile_render_shared::text::TextGlyph>, FreeRuns)
{
    let (mut free, mut text, mut runs) = (Vec::new(), Vec::new(), Vec::new());
    for marker in markers {
        let (kind, index) = match marker.text.filter(|_| marker.solid.is_none()) {
            Some(glyph) => {
                let Some(gpu) = glyph.gpu(marker, fonts, bases) else { continue };
                text.push(gpu);
                (if glyph.key.is_some() { FreeKind::TextKeyed } else { FreeKind::Text }, text.len() as u32 - 1)
            }
            None => {
                free.push(marker.gpu());
                (free_kind(marker), free.len() as u32 - 1)
            }
        };
        push_run(&mut runs, index, kind, view_class(marker));
    }
    (free, text, runs)
}

/// The world's free sprites in drawing order (by the pass they follow,
/// then list order), its text drawn from outlines likewise (`bases`: where
/// each font starts in the outline buffer), and per pass the runs of them
/// that follow it; the last entry follows all passes.
fn free_sprites(world: &TileWorld, bases: &[u32]) -> (Vec<FreeSprite>, Vec<tile_render_shared::text::TextGlyph>, Vec<FreeRuns>) {
    let passes = world.passes().len().min(MAX_PASSES);
    let mut order: Vec<&crate::PlacedSprite> = world.free.iter().collect();
    order.sort_by_key(|s| s.after_pass.min(passes));
    let mut runs: Vec<FreeRuns> = vec![Vec::new(); passes + 1];
    let mut gpu: Vec<FreeSprite> = Vec::with_capacity(order.len());
    let mut text = Vec::new();
    for s in order {
        let (kind, i) = match s.text.filter(|_| s.solid.is_none()) {
            Some(t) => {
                let Some(g) = t.gpu(s, &world.fonts, bases) else { continue };
                text.push(g);
                (if t.key.is_some() { FreeKind::TextKeyed } else { FreeKind::Text }, text.len() as u32 - 1)
            }
            None => {
                gpu.push(s.gpu());
                (free_kind(s), gpu.len() as u32 - 1)
            }
        };
        push_run(&mut runs[s.after_pass.min(passes)], i, kind, view_class(s));
    }
    if gpu.is_empty() {
        gpu.push(FreeSprite::default());
    }
    if text.is_empty() {
        text.push(Default::default());
    }
    (gpu, text, runs)
}

/// Whether `pass`'s cells are better drawn as octagons
/// (`FrameParams::corner_cuts`): it has few layers (each cell costs three
/// times the vertices) whose sprites leave much of their corners blank
/// (a fifth of their rectangles or more).
fn octagon_pass(world: &TileWorld, pass: &crate::Pass, sprites: &[SpriteDesc]) -> bool {
    if pass.count > 4 {
        return false;
    }
    let (mut cut, mut all) = (0u64, 0u64);
    let layers = &world.layers()[pass.first as usize..(pass.first + pass.count) as usize];
    for set in layers.iter().flat_map(|l| l.lut.iter()) {
        let Some(id) = world.sets.frame(*set, 0) else { continue };
        let Some(s) = sprites.get(id.0 as usize) else { continue };
        all += (s.width * s.height) as u64;
        cut += (0..4).map(|k| ((s.cuts >> (8 * k)) & 0xFF) as u64).map(|c| c * c / 4).sum::<u64>();
    }
    all > 0 && cut * 5 >= all
}

/// Removes covered binary draws from each pass's runs, retaining the
/// original instances. A downscaled twin must also be binary because any
/// view can draw it instead of its original sprite.
fn dedup_free(source: &[FreeRuns], free: &[FreeSprite], sets: &crate::SpriteSets, sprites: &[SpriteDesc]) -> Vec<FreeRuns> {
    let mut binary = std::collections::HashMap::new();
    let mut is_binary = |set: u32| {
        *binary.entry(set).or_insert_with(|| {
            let mut frames = (0..).map_while(|i| sets.frame(crate::SpriteSetId::from_raw(set), i)).peekable();
            frames.peek().is_some() && frames.all(|id| {
                sprites.get(id.0 as usize).is_some_and(|s| {
                    s.blank & tile_render_shared::BLANK_BINARY != 0 && (s.downscaled == 0
                        || sprites.get((s.downscaled - 1) as usize).is_some_and(|t| t.blank & tile_render_shared::BLANK_BINARY != 0))
                })
            })
        })
    };
    let mut out = Vec::with_capacity(source.len());
    for runs in source {
        let mut later = std::collections::HashSet::<[u32; size_of::<FreeSprite>() / 4]>::new();
        let mut hidden = std::collections::HashSet::new();
        for (range, kind, _) in runs.iter().rev() {
            if *kind != FreeKind::Normal {
                continue;
            }
            for i in range.clone().rev() {
                let s = free[i as usize];
                if s.primitive.kind == tile_render_shared::PRIMITIVE_SPRITE && s.scale_den == 0 && !later.insert(bytemuck::cast(s)) && is_binary(s.set) {
                    hidden.insert(i);
                }
            }
        }
        let mut shown = Vec::with_capacity(runs.len());
        for (range, kind, class) in runs {
            if *kind == FreeKind::Normal {
                for i in range.clone().filter(|i| !hidden.contains(i)) {
                    push_run(&mut shown, i, *kind, *class);
                }
            } else {
                shown.push((range.clone(), *kind, *class));
            }
        }
        out.push(shown);
    }
    out
}

/// The first zoomed step of the scene or of its markers. Markers follow
/// every cell pass, regardless of the pass given on their input sprites.
fn with_markers(mut first: Vec<crate::world::ZoomedStart>, markers: &[FreeSprite], passes: usize) -> Vec<crate::world::ZoomedStart> {
    for m in markers.iter().filter(|m| m.flags & tile_render_shared::FREE_ZOOMED != 0 || tile_render_shared::output_scaled(m.primitive.kind)) {
        let zoom = if tile_render_shared::output_scaled(m.primitive.kind) { (0, 0) } else { (m.zoom >> 16, m.zoom & 0xFFFF) };
        first.push(crate::world::ZoomedStart {
            zoom,
            pass: passes,
            view: (m.view != tile_render_shared::ALL_VIEWS).then_some(m.view),
            except_view: m.flags & tile_render_shared::FREE_EXCEPT_VIEW != 0,
            category: m.category,
        });
    }
    first
}

fn smoke_instances(runs: &[FreeRuns]) -> usize {
    runs.iter().flatten().filter(|(_, kind, _)| *kind == FreeKind::NativePaletteAlpha).map(|(range, _, _)| range.len()).sum()
}

/// How free sprite `s` (not text) is drawn.
fn free_kind(s: &crate::PlacedSprite) -> FreeKind {
    if s.solid.is_some() || s.shape.is_some() { return FreeKind::Normal; }
    match s.blend {
        crate::Blend::NativePaletteAlpha => FreeKind::NativePaletteAlpha,
        crate::Blend::Invert => FreeKind::Invert,
        crate::Blend::Subpixel { .. } => FreeKind::Subpixel,
        _ => FreeKind::Normal,
    }
}

/// Adds instance `i`, drawn as `kind` in views of `class`, to `runs`.
fn push_run(runs: &mut FreeRuns, i: u32, kind: FreeKind, class: u32) {
    match runs.last_mut() {
        Some((range, k, c)) if *k == kind && *c == class && range.end == i => range.end = i + 1,
        _ => runs.push((i..i + 1, kind, class)),
    }
}

/// Marker text and ordinary sprites index independent GPU buffers.
fn shifted_markers(runs: &FreeRuns, free_by: u32, text_by: u32) -> impl Iterator<Item = (std::ops::Range<u32>, FreeKind, u32)> + '_ {
    runs.iter().map(move |(range, kind, class)| {
        let by = if matches!(kind, FreeKind::Text | FreeKind::TextKeyed) { text_by } else { free_by };
        (range.start + by..range.end + by, *kind, *class)
    })
}

/// Where each font's words start in the outline buffer (see
/// `world::outline_words`).
fn font_bases(fonts: &[std::sync::Arc<gpu_text::Face>]) -> Vec<u32> {
    let mut at = 0;
    fonts
        .iter()
        .map(|f| {
            let base = at;
            at += f.words.len() as u32;
            base
        })
        .collect()
}

/// A storage buffer of `contents` with room for a quarter more (and a few
/// markers), so that what grows a little is written into it.
fn storage_with_room(device: &wgpu::Device, label: &str, contents: &[u8]) -> wgpu::Buffer {
    let size = (contents.len() + contents.len() / 4 + 64 * size_of::<FreeSprite>()).next_multiple_of(4) as u64;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: true,
    });
    buffer.slice(..contents.len() as u64).get_mapped_range_mut().expect("mapped at creation").copy_from_slice(contents);
    buffer.unmap();
    buffer
}

/// Whether both lists share the same immutable faces, in the same order.
fn same_fonts(a: &[std::sync::Arc<gpu_text::Face>], b: &[std::sync::Arc<gpu_text::Face>]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| std::sync::Arc::ptr_eq(a, b))
}

/// A pass drawing into a rectangle of `canvas`: viewport (x, y, w, h) and
/// the part of it to touch (scissor).
fn begin_view<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    canvas: &wgpu::TextureView,
    ([vx, vy, vw, vh], [x, y, w, h]): ([f32; 4], [u32; 4]),
    bind_group: &wgpu::BindGroup,
    params_offset: u32,
    timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
) -> wgpu::RenderPass<'e> {
    let mut pass = begin_pass(encoder, canvas, wgpu::LoadOp::Load, timestamp_writes);
    pass.set_viewport(vx, vy, vw, vh, 0.0, 1.0);
    pass.set_scissor_rect(x, y, w, h);
    pass.set_bind_group(0, bind_group, &[params_offset]);
    pass
}
fn begin_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
    timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("tile-render"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// An internal texture of `size` (at least `size` if `grow`), (re)created
/// when it does not fit.
fn scratch_for<'a>(
    slot: &'a mut Option<Scratch>,
    device: &wgpu::Device,
    output_layout: &wgpu::BindGroupLayout,
    output_params: &[wgpu::Buffer; CONTROL_SLOTS],
    (w, h): (u32, u32),
    grow: bool,
    format: wgpu::TextureFormat,
) -> &'a Scratch {
    let fits = |s: &Scratch| {
        let have = s.main.size();
        s.main.format() == format && if grow { have.width >= w && have.height >= h } else { (have.width, have.height) == (w, h) }
    };
    if !slot.as_ref().is_some_and(fits) {
        let size = match slot.as_ref().filter(|_| grow) {
            Some(s) => wgpu::Extent3d { width: s.main.width().max(w), height: s.main.height().max(h), depth_or_array_layers: 1 },
            None => wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        };
        let main = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tile-render internal target"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let output = std::array::from_fn(|slot| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tile-render output"),
            layout: output_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&main.create_view(&Default::default())) },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &output_params[slot],
                        offset: 0,
                        size: NonZeroU64::new(size_of::<OutputParams>() as u64),
                    }),
                },
            ],
        }));
        *slot = Some(Scratch { main, output });
    }
    slot.as_ref().unwrap()
}

/// A palette table as uploaded: at least row 0.
fn palette_words(palettes: &[u32]) -> Vec<u32> {
    let mut words = palettes.to_vec();
    words.resize(words.len().max(tile_render_shared::PALETTE_SIZE as usize), 0);
    words
}

fn storage(device: &wgpu::Device, label: &str, contents: &[u8]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    })
}

/// The pipelines [`TileRenderer`] draws with.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum P {
    /// Cells and free sprites into the caller's target (direct colour).
    Tiles,
    Free,
    NativePaletteAlpha(u8),
    /// Into the internal target (quantized colour models).
    TilesQuantized,
    Tiles555,
    FreeQuantized,
    /// Into an internal target in direct colour (zoomed views' pictures,
    /// pictures screen copies read).
    TilesSource,
    FreeSource,
    /// Inverting free sprites, into the caller's target and internally.
    FreeInvert,
    FreeInvertInternal,
    FreeInvertQuantized,
    /// ClearType-like free sprites: into (as `free_pipelines`), channel.
    Subpixel(u8, u8),
    SubpixelDual(u8),
    /// Text drawn from outlines: into, channel; onto a colour key: into.
    Text(u8, u8),
    TextDual(u8),
    TextKeyed(u8),
    /// Copies (scaled) into the caller's target, expanding quantized values.
    Output,
    /// Copies (scaled) into the internal target.
    Blit,
    BlitQuantized,
}

impl P {
    const COUNT: usize = 43;

    fn index(self) -> usize {
        match self {
            P::NativePaletteAlpha(into) => 33 + into.min(2) as usize,
            P::Tiles => 0,
            P::Free => 1,
            P::TilesQuantized => 2,
            P::Tiles555 => 42,
            P::FreeQuantized => 3,
            P::TilesSource => 4,
            P::FreeSource => 5,
            P::FreeInvert => 6,
            P::FreeInvertInternal => 7,
            P::Output => 8,
            P::Blit => 9,
            P::BlitQuantized => 31,
            P::FreeInvertQuantized => 32,
            P::Subpixel(into, channel) => 10 + 3 * into.min(2) as usize + channel.min(2) as usize,
            P::TextDual(into) => 36 + into.min(2) as usize,
            P::SubpixelDual(into) => 39 + into.min(2) as usize,
            P::Text(into, channel) => 19 + 3 * into.min(2) as usize + channel.min(2) as usize,
            P::TextKeyed(into) => 28 + into.min(2) as usize,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_text_keeps_painter_order_and_uses_its_own_buffer_offset() {
        let face = std::sync::Arc::new(gpu_text::Face::from_curves(1000, &[]));
        let sprite = crate::PlacedSprite::new(crate::SpriteSetId::EMPTY, 0, 0);
        let text = crate::PlacedSprite { text: Some(crate::OutlineGlyph { font:0,
            shape:crate::GlyphShape::Bar {top:0,height:3},em64:640,advance:8,
            bold:false,italic:false,color:[10,20,30],key:None }), ..sprite };
        let (free,glyphs,runs)=pack_markers(&[sprite,text,sprite], &[face], &[0]);
        assert_eq!((free.len(),glyphs.len()),(2,1));
        assert_eq!(runs.iter().map(|(_,kind,_)|*kind).collect::<Vec<_>>(),vec![FreeKind::Normal,FreeKind::Text,FreeKind::Normal]);
        let shifted=shifted_markers(&runs,100,7).collect::<Vec<_>>();
        assert_eq!(shifted.iter().map(|(range,_,_)|range.clone()).collect::<Vec<_>>(),vec![100..101,7..8,101..102]);
        let (free,glyphs,runs)=pack_markers(&[sprite,text,sprite], &[], &[]);
        assert_eq!((free.len(),glyphs.len(),runs.len()),(2,0,1));
    }

    #[test]
    fn source_art_text_is_rejected_while_other_free_kinds_stay_drawable() {
        for kind in [FreeKind::Text, FreeKind::TextKeyed] {
            for cells in [false, true] {
                assert!(free_run_visible(kind, 2, 2, cells, 0));
                assert!(!free_run_visible(kind, 2, 2, cells, 1));
                assert!(free_run_visible(kind, 2, 2, cells, 2));
            }
        }
        for kind in [FreeKind::Normal, FreeKind::NativePaletteAlpha, FreeKind::Invert, FreeKind::Subpixel] {
            for art in 0..=2 {
                assert!(free_run_visible(kind, 2, 2, false, art));
            }
        }
    }

    #[test]
    fn free_run_view_eligibility_keeps_world_text_out_of_ui_only_cameras() {
        for kind in [FreeKind::Normal, FreeKind::NativePaletteAlpha, FreeKind::Invert,
            FreeKind::Subpixel, FreeKind::Text, FreeKind::TextKeyed] {
            for art in [0, 2] {
                for cells in [false, true] {
                    assert!(free_run_visible(kind, 3, 3, cells, art));
                    assert!(!free_run_visible(kind, 4, 3, cells, art));
                }
                assert!(!free_run_visible(kind, tile_render_shared::ALL_VIEWS, 3, false, art));
                assert!(free_run_visible(kind, tile_render_shared::ALL_VIEWS, 3, true, art));
            }
        }
    }

    #[test]
    fn retained_fp16_texture_bytes_are_reserved_before_atlas_transfer() {
        let gpu = crate::Headless::new().expect("GPU");
        let mut builder = crate::AtlasBuilder::new(16);
        builder.add(&crate::RgbaImage::from_fn(1, 1, |_, _| [255; 4]), (0, 0)).unwrap();
        let atlas = builder.finish();
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        let before = renderer.atlas_resident_bytes();
        renderer.set_atlas_transfer_limit(NonZeroU64::new(1).unwrap());
        let required = match renderer.begin_atlas_transfer(&gpu.device, AtlasTransferSource::Whole(&atlas), (0, 0)) {
            Err(AtlasTransferError::BudgetExceeded { required, .. }) => required,
            other => panic!("expected initial budget rejection, got {other:?}"),
        };
        scratch_for(&mut renderer.scratch, &gpu.device, &renderer.output_layout, &renderer.output_params, (4, 5), false, wgpu::TextureFormat::Rgba16Float);
        assert_eq!(renderer.atlas_resident_bytes(), before + 4 * 5 * 8);
        renderer.set_atlas_transfer_limit(NonZeroU64::new(required).unwrap());
        assert_eq!(renderer.begin_atlas_transfer(&gpu.device, AtlasTransferSource::Whole(&atlas), (0, 0)),
            Err(AtlasTransferError::BudgetExceeded { required: required + 4 * 5 * 8, limit: required }));
        assert!(renderer.atlas_transfer_idle());
    }

    #[test]
    fn mutable_binary_art_restores_and_removes_duplicate_draws_without_losing_text_order() {
        let mut sets = crate::SpriteSets::default();
        let set = sets.single(crate::SpriteId(0));
        let f = crate::PlacedSprite::new(set, 2, 3).gpu();
        let free = [f, f];
        let source = vec![vec![(0..1, FreeKind::Normal, 0), (0..1, FreeKind::Text, 0), (1..2, FreeKind::Normal, 0)]];
        let mut sprites = [SpriteDesc { blank: tile_render_shared::BLANK_BINARY, ..Default::default() }];
        let culled = vec![vec![(0..1, FreeKind::Text, 0), (1..2, FreeKind::Normal, 0)]];
        assert_eq!(dedup_free(&source, &free, &sets, &sprites), culled);
        sprites[0].blank = 0;
        assert_eq!(dedup_free(&source, &free, &sets, &sprites), source);
        sprites[0].blank = tile_render_shared::BLANK_BINARY;
        assert_eq!(dedup_free(&source, &free, &sets, &sprites), culled);
    }

    #[test]
    fn a_translucent_downscaled_twin_keeps_both_draws() {
        let mut sets = crate::SpriteSets::default();
        let set = sets.single(crate::SpriteId(0));
        let f = crate::PlacedSprite::new(set, 0, 0).gpu();
        let source = vec![vec![(0..2, FreeKind::Normal, 0)]];
        let sprites = [SpriteDesc { blank: tile_render_shared::BLANK_BINARY, downscaled: 2, ..Default::default() }, SpriteDesc::default()];
        assert_eq!(dedup_free(&source, &[f, f], &sets, &sprites), source);
    }
}

#[cfg(test)]
mod atlas_transfer_planning {
    use super::*;

    #[test]
    fn aligned_staging_budgets_cover_narrow_and_wide_images() {
        for texel_bytes in [1, 4] {
            for width in [1, 11, 129, 2048] {
                for budget in [255, 256, 269, 511, 512, 1025] {
                    let height = 3;
                    let mut cursor = PixelCursor::default();
                    let mut covered = vec![false; (width * height) as usize];
                    while let Some((w, h)) = cursor.staged_chunk(width, height, texel_bytes, budget) {
                        assert!(aligned_texture_row(w, texel_bytes) * u64::from(h) <= budget);
                        for y in cursor.y..cursor.y + h {
                            for x in cursor.x..cursor.x + w {
                                assert!(!std::mem::replace(&mut covered[(y * width + x) as usize], true));
                            }
                        }
                        cursor.advance(width, w, h);
                    }
                    assert!(covered.iter().all(|p| *p == (budget >= 256)));
                }
            }
        }
    }

    #[test]
    fn palette_upload_slices_bound_scratch_and_pad_only_the_slice() {
        let palettes: Vec<_> = (0..1_000_000).collect();
        assert_eq!(palette_slice(&palettes, 400, 16), vec![100, 101, 102, 103]);
        assert_eq!(palette_slice(&palettes, 3_999_992, 16), vec![999_998, 999_999, 0, 0]);
        assert_eq!(palette_slice(&palettes, 4_000_004, 8), vec![0, 0]);
    }

    #[test]
    fn subrow_budgets_cover_every_texel_once() {
        for texel_bytes in [1, 4] {
            for budget in 1..=47 {
                let (width, height) = (11, 7);
                let mut cursor = PixelCursor::default();
                let mut covered = vec![false; (width * height) as usize];
                while let Some((w, h)) = cursor.chunk(width, height, texel_bytes, budget) {
                    assert!(u64::from(w) * u64::from(h) * u64::from(texel_bytes) <= budget);
                    assert!(w == width || h == 1);
                    for y in cursor.y..cursor.y + h {
                        for x in cursor.x..cursor.x + w {
                            assert!(!std::mem::replace(&mut covered[(y * width + x) as usize], true));
                        }
                    }
                    cursor.advance(width, w, h);
                }
                if budget >= u64::from(texel_bytes) { assert!(covered.iter().all(|p| *p)); }
                else { assert!(covered.iter().all(|p| !*p)); }
            }
        }
    }

    #[test]
    fn peak_limit_discards_spare_room_before_required_room() {
        // Only the colour array moves; the shared index array is counted
        // in resident bytes rather than charged as another allocation.
        let resident = 1024;
        let tables = 128;
        let limit = resident + tables + 16 * 16 * 2 * 4;
        assert_eq!(transfer_room(16, (1, 3), (2, 3), (8, 9), false, resident, tables, limit), Ok((2, 3)));
        assert!(matches!(transfer_room(16, (1, 3), (2, 3), (8, 9), false, resident, tables, limit - 1), Err(AtlasTransferError::BudgetExceeded { .. })));
        assert_eq!(transfer_room(16, (1, 3), (1, 3), (8, 9), false, resident, 0, resident), Ok((1, 3)));
    }

    #[test]
    fn whole_replacement_charges_both_new_arrays() {
        let required = 1024 + 128 + 16 * 16 * 4 + 16 * 16;
        assert_eq!(transfer_room(16, (1, 1), (1, 1), (1, 1), true, 1024, 128, required), Ok((1, 1)));
        assert!(matches!(transfer_room(16, (1, 1), (1, 1), (1, 1), true, 1024, 128, required - 1), Err(AtlasTransferError::BudgetExceeded { .. })));
    }
}
