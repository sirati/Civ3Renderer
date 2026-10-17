//! The wgpu side: pipeline, GPU buffers and the draw call.

use std::num::NonZeroU64;

use tile_render_shared::{FrameParams, FreeSprite, LayerDesc, OutputParams, QUAD_VERTICES, SpriteDesc};
use wgpu::util::DeviceExt;

use crate::atlas::Atlas;
use crate::camera::Camera;
use crate::world::{ColorModel, Grid, Pass, TileWorld};

const SPV: &[u8] = include_bytes!(env!("TILE_RENDER_SPV"));

/// Per-pass `FrameParams` live in one buffer at this stride (the minimum
/// uniform offset alignment wgpu guarantees).
const PARAMS_STRIDE: u64 = 256;
/// Views [`TileRenderer::render_views`] draws at most (a viewer's most: a
/// game's few views, a second pane, insets with their frames, a magnifier
/// with its frame).
pub const MAX_VIEWS: usize = 16;
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
    layout: wgpu::BindGroupLayout,
    output_layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    /// `OutputParams` of each copy (one slot per view, then the final one).
    output_params: wgpu::Buffer,
    scratch: Option<Scratch>,
    /// The pictures of zoomed views at zoom 1 (see `Camera::source_size`).
    sources: Option<Scratch>,
    /// The atlas on the GPU.
    atlas: Option<AtlasTextures>,
    /// The row of named palette 1 (`Atlas::named_palettes`).
    named_palettes: u32,
    sprites: Option<wgpu::Buffer>,
    /// The palette rows indexed sprites are drawn through (`Atlas::palettes`).
    palettes: Option<wgpu::Buffer>,
    world: Option<WorldBuffers>,
    bind_group: Option<wgpu::BindGroup>,
    /// Whether more views than drawn were asked for (warned once).
    too_many_views: bool,
}

/// The atlas on the GPU: its colour and index pages, its sprites and
/// palette rows.
struct AtlasTextures {
    rgba: PageArray,
    index: PageArray,
    sprites: Vec<SpriteDesc>,
    palettes: Vec<u32>,
}

/// Atlas pages of one format: a texture array with room for `capacity`
/// pages of `size`² texels (none: a single texel, so that it can be bound).
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
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
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
    output: wgpu::BindGroup,
}

struct WorldBuffers {
    grid: Grid,
    color: ColorModel,
    passes: Vec<Pass>,
    /// How far per-cell offsets move sprites (`TileWorld::offset_reach`).
    reach: (u32, u32, u32, u32),
    /// Free sprites, ordered by the pass they follow, and per pass the
    /// range of them to draw after it (the last entry: after all passes).
    free: wgpu::Buffer,
    free_after: Vec<FreeRuns>,
    /// Text drawn from outlines (`tile_render_shared::text::TextGlyph`), in
    /// drawing order (`free_after`'s text runs index it).
    text: wgpu::Buffer,
    /// The fonts' outlines, and which fonts they are (`Arc` pointers).
    outlines: wgpu::Buffer,
    fonts: Vec<usize>,
    /// Per zoom with zoomed sprites, the first pass they follow.
    first_zoomed: Vec<((u32, u32), usize)>,
    layers: wgpu::Buffer,
    tile_data: wgpu::Buffer,
    lut: wgpu::Buffer,
    sets: wgpu::Buffer,
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tile-render shaders"),
            source: wgpu::util::make_spirv(SPV),
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
        let output_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-render output params"),
            size: PARAMS_STRIDE * (MAX_VIEWS + 1 + MAX_COPIES) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-render params"),
            size: PARAMS_STRIDE * (VIEW_SLOTS * MAX_VIEWS) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipelines: std::array::from_fn(|_| std::sync::OnceLock::new()),
            shader,
            target_format,
            layout,
            output_layout,
            params,
            output_params,
            scratch: None,
            sources: None,
            atlas: None,
            named_palettes: 1,
            sprites: None,
            palettes: None,
            world: None,
            bind_group: None,
            too_many_views: false,
        }
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
            let (stages, layout, format, blend, mask) = match p {
                P::Tiles => (tiles, &self.layout, target, alpha, all),
                P::Free => (free, &self.layout, target, alpha, all),
                P::TilesQuantized => (tiles, &self.layout, internal, premultiplied, all),
                P::FreeQuantized => (free, &self.layout, internal, premultiplied, all),
                P::TilesSource => (tiles, &self.layout, internal, alpha, all),
                P::FreeSource => (free, &self.layout, internal, alpha, all),
                P::FreeInvert => (free, &self.layout, target, invert, all),
                P::FreeInvertInternal => (free, &self.layout, internal, invert, all),
                P::Subpixel(into, channel) => {
                    let (fs, mask) = [("subpixel_r_fs", wgpu::ColorWrites::RED), ("subpixel_g_fs", wgpu::ColorWrites::GREEN), ("subpixel_b_fs", wgpu::ColorWrites::BLUE)][channel as usize];
                    let (format, blend) = [(target, alpha), (internal, premultiplied), (internal, alpha)][into as usize];
                    (("free_vs", fs), &self.layout, format, blend, mask)
                }
                P::Text(into, channel) => {
                    let (fs, mask) = [("text_r_fs", wgpu::ColorWrites::RED), ("text_g_fs", wgpu::ColorWrites::GREEN), ("text_b_fs", wgpu::ColorWrites::BLUE)][channel as usize];
                    let (format, blend) = [(target, alpha), (internal, premultiplied), (internal, alpha)][into as usize];
                    (("text_vs", fs), &self.layout, format, blend, mask)
                }
                P::TextKeyed(into) => {
                    let (format, blend) = [(target, alpha), (internal, premultiplied), (internal, alpha)][into as usize];
                    (("text_vs", "text_keyed_fs"), &self.layout, format, blend, all)
                }
                P::Output => (copy, &self.output_layout, target, None, all),
                P::Blit => (copy, &self.output_layout, internal, None, all),
            };
            let name = format!("tile-render {p:?}");
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(&name),
                bind_group_layouts: &[Some(layout)],
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

    /// The pipelines free sprites are drawn with into `into` (0 the
    /// caller's target, 1 the quantized internal one, 2 a direct internal
    /// one), made as runs need them.
    fn free_pipelines<'a>(&'a self, device: &'a wgpu::Device, into: u8) -> FreePipelines<'a> {
        FreePipelines { renderer: self, device, into }
    }

    /// Replaces the pixels of atlas sprite `sprite` (sprites whose content
    /// changes at run time, e.g. a minimap). `image` must have the sprite's
    /// size; returns whether it was written.
    pub fn patch_sprite(&self, queue: &wgpu::Queue, sprite: crate::SpriteId, image: &crate::RgbaImage) -> bool {
        let Some(a) = &self.atlas else { return false };
        let Some(s) = a.sprites.get(sprite.0 as usize) else { return false };
        if (s.width, s.height) != (image.width, image.height) || s.width == 0 || s.height == 0 || s.palette != 0 {
            return false;
        }
        a.rgba.write(queue, s.page, s.atlas_x, s.atlas_y, (image.width, image.height), image.as_bytes());
        true
    }

    /// Uploads the atlas pages (colour and index), sprite and palette tables.
    pub fn set_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, atlas: &Atlas) {
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
        self.rebuild_bind_group(device);
    }

    /// Appends sprites to the atlas (`growth`, made for the atlas as
    /// uploaded so far): only the new pixels are written, and the new
    /// palette rows. Pages without room in their texture array go into a
    /// bigger one, the old pages copied on the GPU, with room for half as
    /// many again (see [`TileRenderer::reserve_atlas`]). Without an atlas,
    /// does nothing.
    pub fn grow_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, growth: &crate::AtlasGrowth) {
        let Some(mut a) = self.atlas.take() else { return };
        if growth.first as usize != a.sprites.len() {
            log::error!("atlas growth for {} sprites, the atlas has {}", growth.first, a.sprites.len());
        }
        let max = device.limits().max_texture_array_layers;
        if growth.pages.max(growth.index_pages) > max {
            log::error!("atlas: {} colour and {} index pages, the device has room for {max} of each: sprites past them are not drawn", growth.pages, growth.index_pages);
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
        a.sprites.extend_from_slice(&growth.sprites);
        let empty = [SpriteDesc::default()];
        let sprites: &[SpriteDesc] = if a.sprites.is_empty() { &empty } else { &a.sprites };
        self.sprites = Some(storage(device, "tile-render sprites", bytemuck::cast_slice(sprites)));
        if !growth.palettes.is_empty() {
            a.palettes.extend_from_slice(&growth.palettes);
            self.palettes = Some(storage(device, "tile-render palettes", bytemuck::cast_slice(&palette_words(&a.palettes))));
        }
        self.atlas = Some(a);
        self.rebuild_bind_group(device);
    }

    /// Makes room in the atlas for `pages` colour and `index_pages` index
    /// pages (e.g. as many as art expected soon will need), so that growing
    /// it that far does not move it again.
    pub fn reserve_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pages: u32, index_pages: u32) {
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

    /// Bytes the atlas takes on the GPU: colour pages, index pages (as
    /// allocated).
    pub fn atlas_bytes(&self) -> (u64, u64) {
        self.atlas.as_ref().map_or((0, 0), |a| (a.rgba.bytes(), a.index.bytes()))
    }

    /// Replaces the palette rows (see `Atlas::palettes`), e.g. a driver's
    /// colours changing at run time: no atlas is uploaded again.
    pub fn set_palettes(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, palettes: &[u32]) {
        let words = palette_words(palettes);
        match &self.palettes {
            Some(b) if b.size() >= (words.len() * 4) as u64 => queue.write_buffer(b, 0, bytemuck::cast_slice(&words)),
            _ => {
                self.palettes = Some(storage(device, "tile-render palettes", bytemuck::cast_slice(&words)));
                self.rebuild_bind_group(device);
            }
        }
    }

    /// Uploads a whole world. Call again (or `update_world`) when it changes.
    pub fn set_world(&mut self, device: &wgpu::Device, world: &TileWorld) {
        let packed = world.pack();
        let empty = [LayerDesc::default()];
        let layers: &[LayerDesc] = if packed.layers.is_empty() { &empty } else { &packed.layers };
        let (outlines, bases) = crate::world::outline_words(&world.fonts);
        let (free, text, free_after) = free_sprites(world, &bases);
        self.world = Some(WorldBuffers {
            grid: world.grid,
            color: world.color,
            passes: world.passes(),
            reach: packed.reach,
            free: storage(device, "tile-render free sprites", bytemuck::cast_slice(&free)),
            free_after,
            text: storage(device, "tile-render text", bytemuck::cast_slice(&text)),
            outlines: storage(device, "tile-render outlines", bytemuck::cast_slice(&outlines)),
            fonts: font_ids(world),
            first_zoomed: crate::world::first_zoomed(&world.free, world.passes().len().min(MAX_PASSES)),
            layers: storage(device, "tile-render layers", bytemuck::cast_slice(layers)),
            tile_data: storage(device, "tile-render tile data", bytemuck::cast_slice(&packed.data)),
            lut: storage(device, "tile-render lut", bytemuck::cast_slice(&packed.lut)),
            sets: storage(device, "tile-render sprite sets", bytemuck::cast_slice(&packed.sets)),
        });
        self.rebuild_bind_group(device);
    }

    /// Re-uploads the cell values of an already uploaded world of the same
    /// shape (same grid and layer tables). Cheap enough to do every frame.
    pub fn update_world(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &TileWorld) {
        let packed = world.pack();
        match &self.world {
            Some(w) if w.grid == world.grid && w.color == world.color && w.tile_data.size() == (packed.data.len() * 4) as u64 => {
                queue.write_buffer(&w.tile_data, 0, bytemuck::cast_slice(&packed.data));
                if let Some(w) = self.world.as_mut() {
                    w.reach = packed.reach;
                }
                self.update_free(device, queue, world);
            }
            _ => self.set_world(device, world),
        }
    }

    /// Re-uploads only the free sprites (and text) of an already uploaded
    /// world whose cells did not change (e.g. markers moving every frame).
    pub fn update_free(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &TileWorld) {
        let Some(w) = &mut self.world else { return self.set_world(device, world) };
        let fonts = font_ids(world);
        let (outlines, bases) = crate::world::outline_words(&world.fonts);
        let (free, text, free_after) = free_sprites(world, &bases);
        w.first_zoomed = crate::world::first_zoomed(&world.free, w.passes.len().min(MAX_PASSES));
        w.free_after = free_after;
        let mut rebind = false;
        // Into the buffer if it fits, else a new one.
        let mut write = |buffer: &mut wgpu::Buffer, label: &str, bytes: &[u8]| {
            if buffer.size() >= bytes.len() as u64 {
                queue.write_buffer(buffer, 0, bytes);
            } else {
                *buffer = storage(device, label, bytes);
                rebind = true;
            }
        };
        write(&mut w.free, "tile-render free sprites", bytemuck::cast_slice(&free));
        write(&mut w.text, "tile-render text", bytemuck::cast_slice(&text));
        if w.fonts != fonts {
            w.outlines = storage(device, "tile-render outlines", bytemuck::cast_slice(&outlines));
            w.fonts = fonts;
            rebind = true;
        }
        if rebind {
            self.rebuild_bind_group(device);
        }
    }

    fn rebuild_bind_group(&mut self, device: &wgpu::Device) {
        self.bind_group = None;
        let (Some(atlas), Some(sprites), Some(palettes), Some(world)) = (&self.atlas, &self.sprites, &self.palettes, &self.world) else { return };
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tile-render"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.params,
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
        }));
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
        let view = target.create_view(&Default::default());
        let (Some(bind_group), Some(world)) = (&self.bind_group, &self.world) else {
            drop(begin_pass(encoder, &view, wgpu::LoadOp::Clear(crate::gpu::color(clear))));
            return;
        };
        if cameras.len() > MAX_VIEWS && !self.too_many_views {
            log::warn!("{} views, only the first {MAX_VIEWS} are drawn (not repeated)", cameras.len());
        }
        self.too_many_views |= cameras.len() > MAX_VIEWS;
        let cameras = &cameras[..cameras.len().min(MAX_VIEWS)];
        let bits = world.color.bits();
        let target_size = (target.width(), target.height());
        let unscaled = present == Present::unscaled(target_size);
        // What is drawn: the canvas, shown on the target afterwards.
        let (canvas_w, canvas_h) = (present.canvas.0.max(1), present.canvas.1.max(1));
        let size = wgpu::Extent3d { width: canvas_w, height: canvas_h, depth_or_array_layers: 1 };
        // Background in the canvas's values.
        let background = if bits == 0 {
            crate::gpu::color(clear)
        } else {
            let q = |c: u8| f64::from(tile_render_shared::quantize(c.into(), bits) + tile_render_shared::QUANTIZED_BIAS) / 255.0;
            wgpu::Color { r: q(clear[0]), g: q(clear[1]), b: q(clear[2]), a: f64::from(clear[3]) / 255.0 }
        };
        let write_output = |slot: usize, params: OutputParams| {
            let offset = slot as u64 * PARAMS_STRIDE;
            queue.write_buffer(&self.output_params, offset, bytemuck::bytes_of(&params));
            offset as u32
        };
        // Scaled groups' pictures, side by side in rows.
        let placements = crate::camera::placements(cameras, |z| crate::world::zoomed_from(&world.first_zoomed, z));
        let source_extent = placements.iter().filter_map(|p| p.source).fold((1, 1), |(w, h), r| (w.max(r[0] + r[2]), h.max(r[1] + r[3])));
        if placements.iter().any(|p| p.source.is_some()) {
            let s = scratch_for(&mut self.sources, device, &self.output_layout, &self.output_params, source_extent, true);
            drop(begin_pass(encoder, &s.main.create_view(&Default::default()), wgpu::LoadOp::Clear(background)));
        }
        let copies = &copies[..copies.len().min(MAX_COPIES)];
        // Copies read the finished picture, and a scaled one is shown from
        // it: then it is composed internally.
        let direct = bits == 0 && copies.is_empty() && unscaled;
        let canvas = if direct {
            drop(begin_pass(encoder, &view, wgpu::LoadOp::Clear(background)));
            view.clone()
        } else {
            let s = scratch_for(&mut self.scratch, device, &self.output_layout, &self.output_params, (size.width, size.height), false);
            let main = s.main.create_view(&Default::default());
            drop(begin_pass(encoder, &main, wgpu::LoadOp::Clear(background)));
            main
        };
        let source_view = self.sources.as_ref().map(|s| s.main.create_view(&Default::default()));
        // The part of rectangle (x, y, w, h) inside `bounds` (x, y, w, h).
        let clipped = |[x, y, w, h]: [i32; 4], [bx, by, bw, bh]: [i32; 4]| {
            let (x0, y0) = (x.max(bx).max(0), y.max(by).max(0));
            let (x1, y1) = ((x + w).min(bx + bw), (y + h).min(by + bh));
            (x1 > x0 && y1 > y0).then_some([x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
        };
        let target_rect = [0, 0, size.width as i32, size.height as i32];
        // Pipelines into the canvas, and into the source texture.
        let (canvas_tiles, canvas_free) = match () {
            _ if direct => (self.pipeline(device, P::Tiles), self.free_pipelines(device, 0)),
            _ if bits == 0 => (self.pipeline(device, P::TilesSource), self.free_pipelines(device, 2)),
            _ => (self.pipeline(device, P::TilesQuantized), self.free_pipelines(device, 1)),
        };
        let source_pipelines = if bits == 0 {
            (self.pipeline(device, P::TilesSource), self.free_pipelines(device, 2))
        } else {
            (self.pipeline(device, P::TilesQuantized), self.free_pipelines(device, 1))
        };
        let steps = Steps { queue, params: &self.params, world, bind_group, frame, named_palettes: self.named_palettes };
        for (v, place) in placements.iter().enumerate() {
            // Its steps at zoom 1.
            let rect = place.rect;
            let scissor = match place.source {
                Some(r) => clipped(rect, [r[0] as i32, r[1] as i32, r[2] as i32, r[3] as i32]),
                None => clipped(rect, target_rect),
            };
            let (into, pipelines) = match place.source {
                None => (&canvas, (canvas_tiles, canvas_free)),
                Some(_) => (source_view.as_ref().expect("created above"), source_pipelines),
            };
            // Which zoom art the view draws (see `FrameParams::zoom_art`).
            let art = u32::from(place.art);
            if let Some(scissor) = scissor {
                let viewport = [rect[0] as f32, rect[1] as f32, rect[2] as f32, rect[3] as f32];
                steps.draw(encoder, into, (viewport, scissor), &place.drawn, v, (art, None), place.steps_at_one(), 0, pipelines);
            }
            // The group's picture, scaled into its rectangle.
            if let Some((group, r)) = place.blit {
                let group_rect = [group.origin.0, group.origin.1, group.viewport.0 as i32, group.viewport.1 as i32];
                if let Some(on_target) = clipped(group_rect, target_rect) {
                    let params = OutputParams {
                        src_x: r[0] as i32,
                        src_y: r[1] as i32,
                        dst_x: group.origin.0,
                        dst_y: group.origin.1,
                        zoom_num: group.zoom.0.max(1),
                        zoom_den: group.zoom.1.max(1),
                        bits: 0,
                        _pad: 0,
                    };
                    let offset = write_output(v, params);
                    let mut pass = begin_pass(encoder, &canvas, wgpu::LoadOp::Load);
                    pass.set_pipeline(self.pipeline(device, if direct { P::Output } else { P::Blit }));
                    pass.set_scissor_rect(on_target[0], on_target[1], on_target[2], on_target[3]);
                    pass.set_bind_group(0, &self.sources.as_ref().expect("created above").output, &[offset]);
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
                steps.draw(encoder, &canvas, (viewport, scissor), c, v, (art, Some(c.origin)), place.steps_scaled(), MAX_PASSES + 1, (canvas_tiles, canvas_free));
            }
        }
        if direct {
            return;
        }
        let scratch = self.scratch.as_ref().expect("created above");
        let (n, d) = (present.zoom.0.max(1), present.zoom.1.max(1));
        let (ax, ay) = present.at;
        let params = OutputParams { dst_x: ax, dst_y: ay, zoom_num: n, zoom_den: d, bits, ..Default::default() };
        let offset = write_output(MAX_VIEWS, params);
        // Around a scaled picture: clear.
        let load = if unscaled { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(crate::gpu::color(clear)) };
        let mut pass = begin_pass(encoder, &view, load);
        // Canvas pixels as target pixels: the first target pixel showing
        // canvas pixel `v` (target pixel p shows canvas pixel ⌊(p − at)·d/n⌋).
        let to_target = |v: u32| (v as u64 * n as u64).div_ceil(d as u64) as i32;
        // A canvas rectangle (x, y, w, h) as shown on the target, clipped to it.
        let shown = |[x, y, w, h]: [u32; 4]| {
            let (x0, y0) = (ax + to_target(x), ay + to_target(y));
            let r = [x0, y0, ax + to_target(x + w) - x0, ay + to_target(y + h) - y0];
            clipped(r, [0, 0, target_size.0 as i32, target_size.1 as i32])
        };
        let Some(picture) = shown([0, 0, canvas_w, canvas_h]) else { return };
        pass.set_pipeline(self.pipeline(device, P::Output));
        pass.set_scissor_rect(picture[0], picture[1], picture[2], picture[3]);
        pass.set_bind_group(0, &scratch.output, &[offset]);
        pass.draw(0..3, 0..1);
        for (i, c) in copies.iter().enumerate() {
            // (A copy of a scaled picture samples it at the target's pixels,
            // not the canvas's.)
            let Some(on_target) = c.scissor((size.width, size.height)).and_then(shown) else { continue };
            let (zoom_num, zoom_den) = (c.zoom.0.max(1) * n, c.zoom.1.max(1) * d);
            let [src_x, src_y, _, _] = c.source;
            let at = |v: i32, a: i32| a + (v as i64 * n as i64).div_euclid(d as i64) as i32;
            let (dst_x, dst_y) = (at(c.at.0, ax), at(c.at.1, ay));
            let params = OutputParams { src_x, src_y, dst_x, dst_y, zoom_num, zoom_den, bits, _pad: 0 };
            let offset = write_output(MAX_VIEWS + 1 + i, params);
            pass.set_scissor_rect(on_target[0], on_target[1], on_target[2], on_target[3]);
            pass.set_bind_group(0, &scratch.output, &[offset]);
            pass.draw(0..3, 0..1);
        }
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

    /// The canvas pixel under target point `p` (e.g. the pointer). Points
    /// on the target beside the picture count as on its nearest edge pixel
    /// (the target's edge is the picture's); points off the target are
    /// mapped as they are.
    pub fn to_canvas(&self, (x, y): (f64, f64)) -> (i32, i32) {
        let s = self.factor();
        let axis = |v: f64, at: i32, len: u32, t: u32| {
            let c = ((v - at as f64) / s).floor() as i64;
            let on_target = v >= 0.0 && v < t as f64;
            (if on_target { c.clamp(0, len.max(1) as i64 - 1) } else { c }) as i32
        };
        (axis(x, self.at.0, self.canvas.0, self.target.0), axis(y, self.at.1, self.canvas.1, self.target.1))
    }
}

/// Most [`ScreenCopy`]s a frame may have.
pub const MAX_COPIES: usize = 4;

/// A rectangle of the finished picture copied on top of it, scaled by
/// nearest pixel (a magnifier).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ScreenCopy {
    /// Source rectangle in target pixels: x, y, width, height.
    pub source: [i32; 4],
    /// Top-left of the copy in the target.
    pub at: (i32, i32),
    /// Target pixels per source pixel, as a fraction.
    pub zoom: (u32, u32),
}

impl ScreenCopy {
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
    fn of(&self, kind: FreeKind) -> Vec<&wgpu::RenderPipeline> {
        let get = |p| self.renderer.pipeline(self.device, p);
        match (kind, self.into) {
            (FreeKind::Normal, 0) => vec![get(P::Free)],
            (FreeKind::Normal, 1) => vec![get(P::FreeQuantized)],
            (FreeKind::Normal, _) => vec![get(P::FreeSource)],
            (FreeKind::Invert, 0) => vec![get(P::FreeInvert)],
            (FreeKind::Invert, _) => vec![get(P::FreeInvertInternal)],
            (FreeKind::Subpixel, into) => (0..3).map(|c| get(P::Subpixel(into, c))).collect(),
            (FreeKind::Text, into) => (0..3).map(|c| get(P::Text(into, c))).collect(),
            (FreeKind::TextKeyed, into) => vec![get(P::TextKeyed(into))],
        }
    }
}

/// What drawing a view's steps (see `camera::Placement`) needs.
struct Steps<'a> {
    queue: &'a wgpu::Queue,
    params: &'a wgpu::Buffer,
    world: &'a WorldBuffers,
    bind_group: &'a wgpu::BindGroup,
    frame: u32,
    named_palettes: u32,
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
        bounds: ([f32; 4], [u32; 4]),
        camera: &Camera,
        v: usize,
        (art, scaled_at): (u32, Option<(i32, i32)>),
        steps: std::ops::Range<usize>,
        slot: usize,
        (tiles, free): (&wgpu::RenderPipeline, FreePipelines<'_>),
    ) {
        let world = self.world;
        let passes = world.passes.len().min(MAX_PASSES);
        for i in 0..=passes {
            let cells = i < passes && camera.cells && steps.contains(&(2 * i));
            let sprites = steps.contains(&(2 * i + 1)) && world.free_after.get(i).is_some_and(|r| !r.is_empty());
            if !cells && !sprites {
                continue;
            }
            let pass = world.passes.get(i).copied().filter(|_| i < passes).unwrap_or_default();
            let params = FrameParams { named_palettes: self.named_palettes, ..camera.frame_params(&world.grid, world.color, self.frame, &pass, v as u32, art, scaled_at, world.reach) };
            // Each (view, step) has its own parameter slot: buffer writes all
            // land before the commands run.
            let offset = (v * VIEW_SLOTS + slot + i) as u64 * PARAMS_STRIDE;
            self.queue.write_buffer(self.params, offset, bytemuck::bytes_of(&params));
            let mut render = begin_view(encoder, into, bounds, self.bind_group, offset as u32);
            if cells {
                render.set_pipeline(tiles);
                render.draw(0..QUAD_VERTICES, 0..params.cols * params.rows * pass.count);
            }
            if sprites {
                draw_free(&mut render, world, i, free);
            }
        }
    }
}

/// Free sprites that follow pass `i`, into `pass` (whose bind group is
/// already set), each run with its pipelines.
fn draw_free(pass: &mut wgpu::RenderPass<'_>, world: &WorldBuffers, i: usize, pipelines: FreePipelines<'_>) {
    for (range, kind) in world.free_after.get(i).into_iter().flatten() {
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
type FreeRuns = Vec<(std::ops::Range<u32>, FreeKind)>;

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
        let (kind, i) = match s.text {
            Some(t) => {
                let Some(g) = t.gpu(s, &world.fonts, bases) else { continue };
                text.push(g);
                (if t.key.is_some() { FreeKind::TextKeyed } else { FreeKind::Text }, text.len() as u32 - 1)
            }
            None => {
                gpu.push(s.gpu());
                let kind = match s.blend {
                    crate::Blend::Invert => FreeKind::Invert,
                    crate::Blend::Subpixel { .. } => FreeKind::Subpixel,
                    _ => FreeKind::Normal,
                };
                (kind, gpu.len() as u32 - 1)
            }
        };
        let p = s.after_pass.min(passes);
        match runs[p].last_mut() {
            Some((range, k)) if *k == kind && range.end == i => range.end = i + 1,
            _ => runs[p].push((i..i + 1, kind)),
        }
    }
    if gpu.is_empty() {
        gpu.push(FreeSprite::default());
    }
    if text.is_empty() {
        text.push(Default::default());
    }
    (gpu, text, runs)
}

/// Which fonts `world` has (to tell when its outlines change).
fn font_ids(world: &TileWorld) -> Vec<usize> {
    world.fonts.iter().map(|f| std::sync::Arc::as_ptr(f) as usize).collect()
}

/// A pass drawing into a rectangle of `canvas`: viewport (x, y, w, h) and
/// the part of it to touch (scissor).
fn begin_view<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    canvas: &wgpu::TextureView,
    ([vx, vy, vw, vh], [x, y, w, h]): ([f32; 4], [u32; 4]),
    bind_group: &wgpu::BindGroup,
    params_offset: u32,
) -> wgpu::RenderPass<'e> {
    let mut pass = begin_pass(encoder, canvas, wgpu::LoadOp::Load);
    pass.set_viewport(vx, vy, vw, vh, 0.0, 1.0);
    pass.set_scissor_rect(x, y, w, h);
    pass.set_bind_group(0, bind_group, &[params_offset]);
    pass
}
fn begin_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
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
        timestamp_writes: None,
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
    output_params: &wgpu::Buffer,
    (w, h): (u32, u32),
    grow: bool,
) -> &'a Scratch {
    let fits = |s: &Scratch| {
        let have = s.main.size();
        if grow { have.width >= w && have.height >= h } else { (have.width, have.height) == (w, h) }
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
            format: QUANTIZED_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let output = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tile-render output"),
            layout: output_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&main.create_view(&Default::default())) },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: output_params,
                        offset: 0,
                        size: NonZeroU64::new(size_of::<OutputParams>() as u64),
                    }),
                },
            ],
        });
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
    /// Into the internal target (quantized colour models).
    TilesQuantized,
    FreeQuantized,
    /// Into an internal target in direct colour (zoomed views' pictures,
    /// pictures screen copies read).
    TilesSource,
    FreeSource,
    /// Inverting free sprites, into the caller's target and internally.
    FreeInvert,
    FreeInvertInternal,
    /// ClearType-like free sprites: into (as `free_pipelines`), channel.
    Subpixel(u8, u8),
    /// Text drawn from outlines: into, channel; onto a colour key: into.
    Text(u8, u8),
    TextKeyed(u8),
    /// Copies (scaled) into the caller's target, expanding quantized values.
    Output,
    /// Copies (scaled) into the internal target.
    Blit,
}

impl P {
    const COUNT: usize = 31;

    fn index(self) -> usize {
        match self {
            P::Tiles => 0,
            P::Free => 1,
            P::TilesQuantized => 2,
            P::FreeQuantized => 3,
            P::TilesSource => 4,
            P::FreeSource => 5,
            P::FreeInvert => 6,
            P::FreeInvertInternal => 7,
            P::Output => 8,
            P::Blit => 9,
            P::Subpixel(into, channel) => 10 + 3 * into.min(2) as usize + channel.min(2) as usize,
            P::Text(into, channel) => 19 + 3 * into.min(2) as usize + channel.min(2) as usize,
            P::TextKeyed(into) => 28 + into.min(2) as usize,
        }
    }
}
