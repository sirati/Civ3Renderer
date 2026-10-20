//! Exact glyph coverage generated on the GPU and retained across scene updates.
//!
//! The atlas contains only the integer linear RGB coverage produced by the
//! outline shader. Text colour, contrast, keyed composition, clipping and
//! destination blending remain live on every draw.
use super::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tile_render_shared::glyph_cache::{CacheGlyph, CacheJob};
use tile_render_shared::text::{self, GlyphPlace, TextGlyph, TEXT_BAR, TEXT_FIXED, TEXT_ITALIC};
use tile_render_shared::{ALL_VIEWS, FREE_EXCEPT_VIEW, shrunk, world_period, wrap_distance};

const ATLAS_SIZE: u32 = 2048;
const MAX_ENTRIES: usize = 16_384;
const MAX_PINNED_FACE_WORD_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    face: usize,
    glyph: u32,
    scale: i32,
    bold: i32,
    flags: u32,
    phase_x: i32,
    phase_y: i32,
}

struct Entry {
    // Keeping the face alive prevents pointer reuse and mutation through
    // Arc::get_mut while any of its coverage remains cached.
    _face: Arc<gpu_text::Face>,
    glyph: CacheGlyph,
    extent: (u32, u32),
}

#[derive(Default)]
struct Shelves {
    x: u32,
    y: u32,
    row_height: u32,
}
impl Shelves {
    fn allocate(&mut self, size: u32, width: u32, height: u32) -> Option<(u32, u32)> {
        if width == 0 || height == 0 || width > size || height > size { return None; }
        if self.x + width > size {
            self.x = 0;
            self.y += self.row_height;
            self.row_height = 0;
        }
        if self.y + height > size { return None; }
        let result = (self.x, self.y);
        self.x += width;
        self.row_height = self.row_height.max(height);
        Some(result)
    }
}

struct Preparation {
    metadata: wgpu::Buffer,
    jobs: wgpu::Buffer,
    group: wgpu::BindGroup,
    glyphs: Vec<CacheGlyph>,
    fills: Vec<CacheJob>,
}
#[derive(Default)]
struct Slot {
    preparations: Vec<Preparation>,
    used: usize,
    filled: HashSet<Key>,
}

/// CPU-side observations of the bounded GPU coverage cache. `fill_jobs`
/// includes warm jobs culled by GPU readiness, not just newly rasterized glyphs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphCacheStats {
    pub enabled: bool,
    pub atlas_bytes: u64,
    pub resident_bytes: u64,
    pub entry_count: usize,
    pub pinned_face_word_bytes: usize,
    pub reset_count: u64,
    pub allocation_count: u64,
    pub fill_jobs: u64,
    pub hits: u64,
    pub fallback_glyphs: u64,
}

pub(super) struct State {
    pub layout: wgpu::BindGroupLayout,
    fill_layout: wgpu::BindGroupLayout,
    fill_pipeline: Option<wgpu::RenderPipeline>,
    publish_layout: wgpu::BindGroupLayout,
    publish_pipeline: Option<wgpu::ComputePipeline>,
    ready: wgpu::Buffer,
    atlas: wgpu::Texture,
    atlas_view: wgpu::TextureView,
    size: u32,
    enabled: bool,
    entries: HashMap<Key, Entry>,
    shelves: Shelves,
    reset_after_submit: bool,
    next_tag: u32,
    pinned_faces: HashMap<usize, usize>,
    reset_count: u64,
    fill_jobs: u64,
    hits: u64,
    allocation_count: u64,
    fallback_glyphs: u64,
    slots: [Slot; CONTROL_SLOTS],
}

fn buffer(device: &wgpu::Device, label: &str, bytes: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label), size: (bytes.max(256) as u64).next_power_of_two(),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl State {
    pub fn new(device: &wgpu::Device) -> Self {
        let enabled = std::env::var("TILE_RENDER_GLYPH_CACHE").ok().as_deref() != Some("0");
        let requested_size = std::env::var("TILE_RENDER_GLYPH_CACHE_SIZE").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(ATLAS_SIZE).clamp(16, ATLAS_SIZE);
        let size = if enabled { requested_size.min(device.limits().max_texture_dimension_2d) } else { 1 };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glyph coverage cache"), entries: &[
                wgpu::BindGroupLayoutEntry { visibility: wgpu::ShaderStages::FRAGMENT, ..storage_entry(0) },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { visibility: wgpu::ShaderStages::FRAGMENT, ..storage_entry(2) },
            ],
        });
        let fill_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glyph coverage fill"), entries: &[
                storage_entry(0),
                wgpu::BindGroupLayoutEntry { visibility: wgpu::ShaderStages::FRAGMENT, ..storage_entry(1) },
                storage_entry(2),
            ],
        });
        let publish_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glyph coverage publish"), entries: &[
                wgpu::BindGroupLayoutEntry { visibility: wgpu::ShaderStages::COMPUTE, ..storage_entry(0) },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false, min_binding_size: None }, count: None },
            ],
        });
        let ready = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glyph coverage GPU readiness"), size: (MAX_ENTRIES * size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false,
        });
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("integer glyph coverage atlas"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Uint,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let atlas_view = atlas.create_view(&Default::default());
        Self { layout, fill_layout, fill_pipeline: Default::default(), publish_layout, publish_pipeline: Default::default(), ready, atlas, atlas_view, size, enabled,
            entries: HashMap::new(), shelves: Default::default(), reset_after_submit: false,
            next_tag: 1, pinned_faces: HashMap::new(), reset_count: 0, fill_jobs: 0, hits: 0, allocation_count: 0, fallback_glyphs: 0,
            slots: Default::default() }
    }

    pub fn begin(&mut self, slot: usize) {
        self.slots[slot].used = 0;
        self.slots[slot].filled.clear();
    }

    /// The renderer calls this only after all prepared commands were submitted.
    /// New atlas fills then execute after those old draws by queue ordering.
    /// No atlas rectangle is evicted while an unsubmitted snapshot may read it.
    pub fn submitted(&mut self) {
        if self.reset_after_submit {
            self.entries.clear();
            self.shelves = Default::default();
            self.pinned_faces.clear();
            self.reset_count += 1;
            self.reset_after_submit = false;
        }
    }

    pub fn bytes(&self) -> u64 {
        u64::from(self.atlas.width()) * u64::from(self.atlas.height()) * 4 + self.ready.size()
            + self.slots.iter().flat_map(|s| &s.preparations).map(|p| p.metadata.size() + p.jobs.size()).sum::<u64>()
    }

    pub fn stats(&self) -> GlyphCacheStats {
        GlyphCacheStats { enabled: self.enabled,
            atlas_bytes: u64::from(self.size) * u64::from(self.size) * 4,
            resident_bytes: self.bytes(), entry_count: self.entries.len(),
            pinned_face_word_bytes: self.pinned_faces.values().sum(),
            reset_count: self.reset_count, allocation_count: self.allocation_count,
            fill_jobs: self.fill_jobs, hits: self.hits, fallback_glyphs: self.fallback_glyphs }
    }

    pub fn flush(&self, queue: &wgpu::Queue, slot: usize) {
        let slot = &self.slots[slot];
        for p in slot.preparations.iter().take(slot.used) {
            queue.write_buffer(&p.metadata, 0, bytemuck::cast_slice(&p.glyphs));
            if !p.fills.is_empty() { queue.write_buffer(&p.jobs, 0, bytemuck::cast_slice(&p.fills)); }
        }
    }

    fn group(&self, device: &wgpu::Device, metadata: &wgpu::Buffer) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("glyph coverage instance mapping"),
            layout: &self.layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: metadata.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: self.ready.as_entire_binding() },
            ] })
    }

    fn create_fill_pipeline(&self, device: &wgpu::Device, shader: &wgpu::ShaderModule) -> wgpu::RenderPipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glyph coverage fill"), bind_group_layouts: &[Some(&self.fill_layout)], immediate_size: 0,
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glyph coverage fill"), layout: Some(&layout),
            vertex: wgpu::VertexState { module: shader, entry_point: Some("glyph_cache_vs"),
                compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("glyph_cache_fs"),
                compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Uint, blend: None, write_mask: wgpu::ColorWrites::ALL,
                })] }), multiview_mask: None, cache: None,
        })
    }

    fn create_publish_pipeline(&self, device: &wgpu::Device, shader: &wgpu::ShaderModule) -> wgpu::ComputePipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glyph coverage publish"), bind_group_layouts: &[Some(&self.publish_layout)], immediate_size: 0,
        });
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("glyph coverage publish"), layout: Some(&layout), module: shader,
            entry_point: Some("glyph_cache_publish_cs"), compilation_options: Default::default(), cache: None,
        })
    }

    /// Prepares only glyph keys, rectangles and instance metadata on the CPU.
    /// Every missing coverage texel is evaluated by the GPU in this encoder,
    /// before the matching text draws. A pending entry is filled in each
    /// prepared control snapshot so no encoder depends on an unsubmitted one.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(&mut self, device: &wgpu::Device, shader: &wgpu::ShaderModule,
        encoder: &mut wgpu::CommandEncoder, slot: usize, glyphs: &[TextGlyph],
        fonts: &[Arc<gpu_text::Face>], outlines: &wgpu::Buffer, params: &FrameParams,
        timer: Option<&crate::timing::GpuTimer>,
    ) -> wgpu::BindGroup {
        let mut bases = Vec::with_capacity(fonts.len());
        let mut offset = 0u32;
        for face in fonts { bases.push(offset); offset += face.words.len() as u32; }
        let index = self.slots[slot].used;
        let (mut metadata, mut jobs) = self.slots[slot].preparations.get_mut(index)
            .map(|p| (std::mem::take(&mut p.glyphs), std::mem::take(&mut p.fills))).unwrap_or_default();
        metadata.clear();
        jobs.clear();
        metadata.reserve(glyphs.len().max(1));
        for g in glyphs {
            let cached = self.enabled.then(|| {
                let face_index = bases.binary_search(&g.face).ok()?;
                let face = &fonts[face_index];
                let local = TextGlyph { face: 0, glyph: g.glyph.checked_sub(g.face)?, ..*g };
                let p = placement(&face.words, &local, params)?;
                if p.flags & TEXT_BAR != 0 { return None; }
                // Integer translations cannot affect the exact fixed-point
                // integration. Preserve both negative-origin phases by Euclidean
                // remainder, then use the same outline bounds as text_vs.
                let key = Key { face: Arc::as_ptr(face) as usize, glyph: p.glyph,
                    scale: p.scale, bold: p.bold, flags: p.flags & TEXT_ITALIC,
                    phase_x: p.x.rem_euclid(TEXT_FIXED), phase_y: p.y.rem_euclid(TEXT_FIXED) };
                let normalized = GlyphPlace { x: key.phase_x, y: key.phase_y, flags: key.flags, ..p };
                let (left, top, right, bottom) = text::bounds(&face.words, &normalized);
                if right <= left || bottom <= top { return None; }
                if !self.entries.contains_key(&key) {
                    // CPU ownership is bounded as well as atlas texel memory.
                    if self.entries.len() == MAX_ENTRIES || (!self.pinned_faces.contains_key(&key.face)
                        && self.pinned_faces.values().sum::<usize>() + face.words.len() * 4 > MAX_PINNED_FACE_WORD_BYTES) {
                        self.reset_after_submit = true;
                        return None;
                    }
                    // Never reuse a readiness stamp, including after eviction.
                    // At exhaustion existing entries remain useful; new shapes
                    // retain the exact procedural path.
                    if self.next_tag == u32::MAX { return None; }
                    let width = (right - left) as u32;
                    let height = (bottom - top) as u32;
                    if width > self.size || height > self.size { return None; }
                    let Some((x, y)) = self.shelves.allocate(self.size, width, height) else {
                        // Existing snapshots retain all their rectangles. Use the
                        // original shader until the next submission boundary.
                        self.reset_after_submit = true;
                        return None;
                    };
                    self.allocation_count += 1;
                    let ready_index = self.entries.len() as u32;
                    let ready_tag = self.next_tag;
                    self.next_tag += 1;
                    self.pinned_faces.entry(key.face).or_insert(face.words.len() * 4);
                    self.entries.insert(key, Entry { _face: face.clone(),
                        glyph: CacheGlyph { atlas_x: x as i32, atlas_y: y as i32, local_left: left, local_top: top,
                            ready_index, ready_tag, ..Default::default() }, extent: (width, height) });
                } else {
                    self.hits += 1;
                }
                let entry = &self.entries[&key];
                if self.slots[slot].filled.insert(key) {
                    jobs.push(CacheJob {
                        atlas_x: entry.glyph.atlas_x as u32, atlas_y: entry.glyph.atlas_y as u32,
                        width: entry.extent.0, height: entry.extent.1,
                        x: normalized.x, y: normalized.y, scale: p.scale, bold: p.bold, flags: key.flags,
                        glyph: g.glyph, face: g.face, left, top,
                        atlas_width: self.size, atlas_height: self.size, ready_index: entry.glyph.ready_index,
                        ready_tag: entry.glyph.ready_tag, ..Default::default()
                    });
                }
                Some(entry.glyph)
            }).flatten().unwrap_or_default();
            if cached.atlas_x < 0 { self.fallback_glyphs += 1; }
            metadata.push(cached);
        }
        if metadata.is_empty() { metadata.push(Default::default()); }
        self.fill_jobs += jobs.len() as u64;
        if index == self.slots[slot].preparations.len() {
            let mapping = buffer(device, "glyph coverage instance metadata", metadata.len() * size_of::<CacheGlyph>());
            let group = self.group(device, &mapping);
            self.slots[slot].preparations.push(Preparation { metadata: mapping,
                jobs: buffer(device, "glyph coverage fill jobs", jobs.len() * size_of::<CacheJob>()),
                group, glyphs: Vec::new(), fills: Vec::new() });
        }
        if self.slots[slot].preparations[index].metadata.size() < (metadata.len() * size_of::<CacheGlyph>()) as u64 {
            let mapping = buffer(device, "glyph coverage instance metadata", metadata.len() * size_of::<CacheGlyph>());
            let group = self.group(device, &mapping);
            let p = &mut self.slots[slot].preparations[index];
            p.metadata = mapping;
            p.group = group;
        }
        let p = &mut self.slots[slot].preparations[index];
        if p.jobs.size() < (jobs.len() * size_of::<CacheJob>()) as u64 {
            p.jobs = buffer(device, "glyph coverage fill jobs", jobs.len() * size_of::<CacheJob>());
        }
        p.glyphs = metadata;
        p.fills = jobs;
        self.slots[slot].used += 1;
        if !self.slots[slot].preparations[index].fills.is_empty() {
            if self.fill_pipeline.is_none() {
                self.fill_pipeline = Some(self.create_fill_pipeline(device, shader));
            }
            if self.publish_pipeline.is_none() {
                self.publish_pipeline = Some(self.create_publish_pipeline(device, shader));
            }
        }
        let p = &self.slots[slot].preparations[index];
        if !p.fills.is_empty() {
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glyph coverage fill"), layout: &self.fill_layout, entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: p.jobs.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: outlines.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: self.ready.as_entire_binding() },
                ],
            });
            let attachments = [Some(wgpu::RenderPassColorAttachment { view: &self.atlas_view,
                depth_slice: None, resolve_target: None, ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store,
                } })];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("glyph coverage fill"), color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: timer.and_then(|t| t.pass(|| "glyph cache fill".into())),
                occlusion_query_set: None, multiview_mask: None,
            });
            pass.set_pipeline(self.fill_pipeline.as_ref().expect("glyph coverage fill pipeline prepared"));
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..QUAD_VERTICES, 0..p.fills.len() as u32);
            drop(pass);
            let publish = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glyph coverage publish"), layout: &self.publish_layout, entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &p.jobs, offset: 0, size: NonZeroU64::new((p.fills.len() * size_of::<CacheJob>()) as u64),
                    }) },
                    wgpu::BindGroupEntry { binding: 2, resource: self.ready.as_entire_binding() },
                ],
            });
            let mut publish_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("glyph coverage publish"),
                timestamp_writes: timer.and_then(|t| t.pass(|| "glyph cache publish".into()))
                    .map(|q| wgpu::ComputePassTimestampWrites { query_set: q.query_set,
                        beginning_of_pass_write_index: q.beginning_of_pass_write_index,
                        end_of_pass_write_index: q.end_of_pass_write_index }),
            });
            publish_pass.set_pipeline(self.publish_pipeline.as_ref().expect("glyph coverage publish pipeline prepared"));
            publish_pass.set_bind_group(0, &publish, &[]);
            publish_pass.dispatch_workgroups((p.fills.len() as u32).div_ceil(64), 1, 1);
        }
        p.group.clone()
    }
}

impl Steps<'_> {
    pub(super) fn prepare_glyph_cache(&self, encoder: &mut wgpu::CommandEncoder,
        camera: &Camera, view: usize, art: u32, scaled_at: Option<(i32, i32)>, viewport: [f32; 4],
        steps: &std::ops::Range<usize>,
    ) -> Option<wgpu::BindGroup> {
        if !self.world.has_text || self.world.fonts.is_empty() || !matches!(art, 0 | 2) { return None; }
        let has_text_run = self.world.free_after.iter().enumerate().any(|(pass, runs)| {
            steps.contains(&(2 * pass + 1)) && runs.iter().any(|(_, kind, class)| {
                matches!(kind, FreeKind::Text | FreeKind::TextKeyed)
                    && free_run_visible(*kind, *class, view as u32, camera.cells, art)
            })
        });
        if !has_text_run { return None; }
        let params = FrameParams {
            target_x: viewport[0] as i32, target_y: viewport[1] as i32,
            ..camera.frame_params(&self.world.grid, self.world.color, self.frame,
                &Pass::default(), view as u32, art, scaled_at, self.world.reach)
        };
        let mut cache = self.renderer.glyph_cache.borrow_mut();
        let state = cache.get_or_insert_with(|| {
            let mut state = State::new(self.device);
            state.begin(self.control_slot);
            state
        });
        Some(state.prepare(self.device, &self.renderer.shader,
            encoder, self.control_slot, &self.world.text_source, &self.world.fonts,
            &self.world.outlines, &params, self.timer))
    }
}

/// The vertex shader's placement, using an individual immutable face. This
/// computes geometry only; coverage evaluation remains in the GPU shader.
fn placement(words: &[u32], g: &TextGlyph, params: &FrameParams) -> Option<GlyphPlace> {
    let except = g.flags & FREE_EXCEPT_VIEW != 0;
    let wrong_view = if except { g.view == params.view || params.world == 0 }
        else { (g.view != ALL_VIEWS && g.view != params.view) || (g.view == ALL_VIEWS && params.world == 0) };
    if !text::visible_at_art(g.flags, params.zoom_art) || wrong_view || g.category & params.hidden_categories != 0 { return None; }
    let screen = text::on_screen(g);
    let n = if screen { 1 } else { params.downscale };
    let (bx, by) = if screen { (params.origin_x, params.origin_y) } else { (params.camera_x, params.camera_y) };
    let (fx, fy) = if screen { (g.x - bx, g.y - by) } else {
        let (px, py) = world_period(&params.grid);
        (wrap_distance(shrunk(g.x, n) - bx, px), wrap_distance(shrunk(g.y, n) - by, py))
    };
    let p = text::place(words, g, fx, fy, params.zoom_num, params.zoom_den)?;
    let (mut l, mut t, mut r, mut b) = text::bounds(words, &p);
    if let Some(c) = text::clip_at_zoom(g, (bx, by), params.zoom_num, params.zoom_den, n) {
        (l, t, r, b) = (l.max(c[0]), t.max(c[1]), r.min(c[2]), b.min(c[3]));
    }
    (r > l && b > t && r > 0 && b > 0 && l < params.viewport_w as i32 && t < params.viewport_h as i32).then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shelves_never_overlap_and_refuse_overflow() {
        let mut shelves = Shelves::default();
        let mut rectangles = Vec::new();
        for (w, h) in [(7, 9), (12, 3), (11, 7), (4, 10), (8, 4), (2, 3)] {
            if let Some((x, y)) = shelves.allocate(32, w, h) {
                assert!(x + w <= 32 && y + h <= 32);
                for &(a, b, c, d) in &rectangles { assert!(x + w <= a || a + c <= x || y + h <= b || b + d <= y); }
                rectangles.push((x, y, w, h));
            }
        }
        assert_eq!(shelves.allocate(32, 33, 1), None);
        assert_eq!(shelves.allocate(32, 1, 33), None);
    }
    #[test]
    fn normalized_coverage_keeps_every_channel_at_negative_fractional_origins() {
        let face = gpu_text::Face::from_curves(1000, &[('x', vec![
            [(0.0, -110.0), (250.0, 0.0), (420.0, 720.0)],
            [(420.0, 720.0), (520.0, 680.0), (600.0, 0.0)],
            [(600.0, 0.0), (300.0, -80.0), (0.0, -110.0)],
        ], 640)]);
        for (x, y) in [(-193, -97), (-97, -1), (-1, 95), (95, 193)] {
            for flags in [0, TEXT_ITALIC] {
                let p = GlyphPlace { x, y, scale: text::scale_of(11 * 64, 3, 2, 1000),
                    bold: 17, flags, glyph: face.record(0), face: 0, ..Default::default() };
                let normalized = GlyphPlace { x: x.rem_euclid(TEXT_FIXED), y: y.rem_euclid(TEXT_FIXED), ..p };
                let (dx, dy) = (x.div_euclid(TEXT_FIXED), y.div_euclid(TEXT_FIXED));
                let (l, t, r, b) = text::bounds(&face.words, &p);
                for py in t..b { for px in l..r {
                    assert_eq!(text::linear_coverage(&face.words, &p, px, py),
                        text::linear_coverage(&face.words, &normalized, px - dx, py - dy),
                        "origin ({x},{y}), flags {flags}, pixel ({px},{py})");
                } }
            }
        }
        assert_eq!(size_of::<CacheGlyph>(), 32);
        assert_eq!(size_of::<CacheJob>(), 96);
    }
}

#[cfg(test)]
mod preparation_tests;
