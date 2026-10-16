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
/// Views [`TileRenderer::render_views`] draws at most.
pub const MAX_VIEWS: usize = 8;
/// Most passes a world may have.
pub const MAX_PASSES: usize = 16;

/// Format of the internal target of quantized colour models: holds
/// `q + QUANTIZED_BIAS` per channel (see `tile_render_shared::scale_offset`).
const QUANTIZED_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Draws one `TileWorld` with one `Atlas`.
pub struct TileRenderer {
    /// Draws into the caller's target (direct colour model).
    pipeline: wgpu::RenderPipeline,
    /// Draws into the internal target (quantized colour models).
    quantized_pipeline: wgpu::RenderPipeline,
    /// The same for free sprites.
    free_pipeline: wgpu::RenderPipeline,
    free_quantized_pipeline: wgpu::RenderPipeline,
    /// Zoomed views of direct-colour worlds, into their source texture.
    source_pipeline: wgpu::RenderPipeline,
    free_source_pipeline: wgpu::RenderPipeline,
    /// Inverting free sprites, into the caller's target and internally.
    free_invert_pipeline: wgpu::RenderPipeline,
    free_invert_internal_pipeline: wgpu::RenderPipeline,
    /// Copies (scaled) into the caller's target, expanding quantized values.
    output_pipeline: wgpu::RenderPipeline,
    /// Copies (scaled) into the internal target.
    blit_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    output_layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    /// `OutputParams` of each copy (one slot per view, then the final one).
    output_params: wgpu::Buffer,
    scratch: Option<Scratch>,
    /// The pictures of zoomed views at zoom 1 (see `Camera::source_size`).
    sources: Option<Scratch>,
    atlas: Option<wgpu::TextureView>,
    /// The atlas texture and its sprites (for run-time pixel updates).
    atlas_texture: Option<(wgpu::Texture, Vec<SpriteDesc>)>,
    sprites: Option<wgpu::Buffer>,
    world: Option<WorldBuffers>,
    bind_group: Option<wgpu::BindGroup>,
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
    /// Free sprites, ordered by the pass they follow, and per pass the
    /// range of them to draw after it (the last entry: after all passes).
    free: wgpu::Buffer,
    free_after: Vec<FreeRuns>,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
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
        let pipeline = |name: &str, (vs, fs): (&str, &str), group: &wgpu::BindGroupLayout, format, blend| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(name),
                bind_group_layouts: &[Some(group)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(name),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let tiles = ("tile_vs", "tile_fs");
        let alpha = Some(wgpu::BlendState::ALPHA_BLENDING);
        let direct = pipeline("tile-render", tiles, &layout, target_format, alpha);
        let frees = ("free_vs", "tile_fs");
        let free_direct = pipeline("tile-render free", frees, &layout, target_format, alpha);
        let premultiplied = Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let quantized = pipeline("tile-render quantized", tiles, &layout, QUANTIZED_FORMAT, premultiplied);
        let free_quantized = pipeline("tile-render free quantized", frees, &layout, QUANTIZED_FORMAT, premultiplied);
        let source = pipeline("tile-render source", tiles, &layout, QUANTIZED_FORMAT, alpha);
        let free_source = pipeline("tile-render free source", frees, &layout, QUANTIZED_FORMAT, alpha);
        // What is there subtracted from the shader's maximum; alpha kept.
        let invert = Some(wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Subtract },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        });
        let free_invert = pipeline("tile-render free invert", frees, &layout, target_format, invert);
        let free_invert_internal = pipeline("tile-render free invert internal", frees, &layout, QUANTIZED_FORMAT, invert);
        let copy = ("fullscreen_vs", "output_fs");
        let output = pipeline("tile-render output", copy, &output_layout, target_format, None);
        let blit = pipeline("tile-render blit", copy, &output_layout, QUANTIZED_FORMAT, None);
        let output_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-render output params"),
            size: PARAMS_STRIDE * (MAX_VIEWS + 1 + MAX_COPIES) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-render params"),
            size: PARAMS_STRIDE * ((MAX_PASSES + 1) * MAX_VIEWS) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline: direct,
            quantized_pipeline: quantized,
            free_pipeline: free_direct,
            free_quantized_pipeline: free_quantized,
            source_pipeline: source,
            free_source_pipeline: free_source,
            free_invert_pipeline: free_invert,
            free_invert_internal_pipeline: free_invert_internal,
            output_pipeline: output,
            blit_pipeline: blit,
            layout,
            output_layout,
            params,
            output_params,
            scratch: None,
            sources: None,
            atlas: None,
            atlas_texture: None,
            sprites: None,
            world: None,
            bind_group: None,
        }
    }

    /// Replaces the pixels of atlas sprite `sprite` (sprites whose content
    /// changes at run time, e.g. a minimap). `image` must have the sprite's
    /// size; returns whether it was written.
    pub fn patch_sprite(&self, queue: &wgpu::Queue, sprite: crate::SpriteId, image: &crate::RgbaImage) -> bool {
        let Some((texture, sprites)) = &self.atlas_texture else { return false };
        let Some(s) = sprites.get(sprite.0 as usize) else { return false };
        if (s.width, s.height) != (image.width, image.height) || s.width == 0 || s.height == 0 {
            return false;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: s.atlas_x, y: s.atlas_y, z: s.page },
                aspect: wgpu::TextureAspect::All,
            },
            image.as_bytes(),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(image.width * 4), rows_per_image: Some(image.height) },
            wgpu::Extent3d { width: image.width, height: image.height, depth_or_array_layers: 1 },
        );
        true
    }

    /// Uploads the atlas pages and sprite table.
    pub fn set_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, atlas: &Atlas) {
        // Release the old atlas first: two at once may not fit (32-bit
        // processes, software renderers).
        self.bind_group = None;
        self.atlas = None;
        self.atlas_texture = None;
        self.sprites = None;
        // Dropped resources are only freed once the GPU is done with them:
        // wait for that, so the old atlas is really gone.
        if let Err(e) = device.poll(wgpu::PollType::wait_indefinitely()) {
            log::warn!("waiting for the GPU before a new atlas: {e}");
        }
        let size = wgpu::Extent3d {
            width: atlas.page_size.max(atlas.pages[0].width),
            height: atlas.page_size.max(atlas.pages[0].height),
            depth_or_array_layers: atlas.pages.len() as u32,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tile-render atlas"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (i, page) in atlas.pages.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: i as u32 },
                    aspect: wgpu::TextureAspect::All,
                },
                page.as_bytes(),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(page.width * 4), rows_per_image: Some(page.height) },
                wgpu::Extent3d { width: page.width, height: page.height, depth_or_array_layers: 1 },
            );
        }
        self.atlas = Some(texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        }));
        self.atlas_texture = Some((texture, atlas.sprites.clone()));
        let empty = [SpriteDesc::default()];
        let sprites: &[SpriteDesc] = if atlas.sprites.is_empty() { &empty } else { &atlas.sprites };
        self.sprites = Some(storage(device, "tile-render sprites", bytemuck::cast_slice(sprites)));
        self.rebuild_bind_group(device);
    }

    /// Uploads a whole world. Call again (or `update_world`) when it changes.
    pub fn set_world(&mut self, device: &wgpu::Device, world: &TileWorld) {
        let packed = world.pack();
        let empty = [LayerDesc::default()];
        let layers: &[LayerDesc] = if packed.layers.is_empty() { &empty } else { &packed.layers };
        let (free, free_after) = free_sprites(world);
        self.world = Some(WorldBuffers {
            grid: world.grid,
            color: world.color,
            passes: world.passes(),
            free: storage(device, "tile-render free sprites", bytemuck::cast_slice(&free)),
            free_after,
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
            Some(w) if w.grid == world.grid
                && w.color == world.color
                && w.tile_data.size() == (packed.data.len() * 4) as u64
                && w.free.size() >= (world.free.len().max(1) * size_of::<FreeSprite>()) as u64 =>
            {
                queue.write_buffer(&w.tile_data, 0, bytemuck::cast_slice(&packed.data));
                let (free, free_after) = free_sprites(world);
                queue.write_buffer(&w.free, 0, bytemuck::cast_slice(&free));
                self.world.as_mut().unwrap().free_after = free_after;
            }
            _ => self.set_world(device, world),
        }
    }

    /// Re-uploads only the free sprites of an already uploaded world whose
    /// cells did not change (e.g. markers moving every frame).
    pub fn update_free(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &TileWorld) {
        let (free, free_after) = free_sprites(world);
        let bytes: &[u8] = bytemuck::cast_slice(&free);
        let Some(w) = &mut self.world else { return self.set_world(device, world) };
        if w.free.size() >= bytes.len() as u64 {
            queue.write_buffer(&w.free, 0, bytes);
            w.free_after = free_after;
            return;
        }
        w.free = storage(device, "tile-render free sprites", bytes);
        w.free_after = free_after;
        self.rebuild_bind_group(device);
    }

    fn rebuild_bind_group(&mut self, device: &wgpu::Device) {
        self.bind_group = None;
        let (Some(atlas), Some(sprites), Some(world)) = (&self.atlas, &self.sprites, &self.world) else { return };
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
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(atlas) },
                wgpu::BindGroupEntry { binding: 7, resource: world.free.as_entire_binding() },
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
    /// copied into its rectangle by nearest texel. At most [`MAX_VIEWS`]
    /// views.
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
        let view = target.create_view(&Default::default());
        let (Some(bind_group), Some(world)) = (&self.bind_group, &self.world) else {
            drop(begin_pass(encoder, &view, wgpu::LoadOp::Clear(crate::gpu::color(clear))));
            return;
        };
        if cameras.len() > MAX_VIEWS {
            log::warn!("{} views, only the first {MAX_VIEWS} are drawn", cameras.len());
        }
        let cameras = &cameras[..cameras.len().min(MAX_VIEWS)];
        let bits = world.color.bits();
        let size = target.size();
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
        // Zoomed views' pictures, side by side in rows.
        let sources = source_rects(cameras);
        let source_extent = sources.iter().flatten().fold((1, 1), |(w, h), r| (w.max(r[0] + r[2]), h.max(r[1] + r[3])));
        if sources.iter().any(Option::is_some) {
            let s = scratch_for(&mut self.sources, device, &self.output_layout, &self.output_params, source_extent, true);
            drop(begin_pass(encoder, &s.main.create_view(&Default::default()), wgpu::LoadOp::Clear(background)));
        }
        let copies = &copies[..copies.len().min(MAX_COPIES)];
        // Copies read the finished picture: then it is composed internally.
        let direct = bits == 0 && copies.is_empty();
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
        let passes = world.passes.len().min(MAX_PASSES);
        for (v, camera) in cameras.iter().enumerate() {
            // The view's rectangle, inside the target.
            let (x0, y0) = (camera.origin.0.max(0) as u32, camera.origin.1.max(0) as u32);
            let x1 = (camera.origin.0 + camera.viewport.0 as i32).clamp(0, size.width as i32) as u32;
            let y1 = (camera.origin.1 + camera.viewport.1 as i32).clamp(0, size.height as i32) as u32;
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let on_target = [x0, y0, x1 - x0, y1 - y0];
            // Where it is drawn at zoom 1: its rectangle, or its source picture.
            let (drawn, into, rect, (tiles, free)) = match sources[v] {
                None => {
                    let viewport = [camera.origin.0 as f32, camera.origin.1 as f32, camera.viewport.0 as f32, camera.viewport.1 as f32];
                    let pipelines = if direct {
                        (&self.pipeline, (&self.free_pipeline, &self.free_invert_pipeline))
                    } else if bits == 0 {
                        (&self.source_pipeline, (&self.free_source_pipeline, &self.free_invert_internal_pipeline))
                    } else {
                        (&self.quantized_pipeline, (&self.free_quantized_pipeline, &self.free_invert_internal_pipeline))
                    };
                    (*camera, &canvas, (viewport, on_target), pipelines)
                }
                Some(r) => {
                    let flat = Camera { zoom: (1, 1), viewport: (r[2], r[3]), ..*camera };
                    let viewport = [r[0] as f32, r[1] as f32, r[2] as f32, r[3] as f32];
                    let pipelines =
                        if bits == 0 { (&self.source_pipeline, (&self.free_source_pipeline, &self.free_invert_internal_pipeline)) } else { (&self.quantized_pipeline, (&self.free_quantized_pipeline, &self.free_invert_internal_pipeline)) };
                    (flat, source_view.as_ref().expect("created above"), (viewport, r), pipelines)
                }
            };
            // Each (view, pass) has its own parameter slot: buffer writes all
            // land before the commands run.
            let write_params = |slot: usize, params: FrameParams| {
                let offset = (v * (MAX_PASSES + 1) + slot) as u64 * PARAMS_STRIDE;
                queue.write_buffer(&self.params, offset, bytemuck::bytes_of(&params));
                offset as u32
            };
            for (i, p) in world.passes.iter().enumerate().take(MAX_PASSES) {
                let params = drawn.frame_params(&world.grid, world.color, frame, p, v as u32);
                let offset = write_params(i, params);
                let mut pass = begin_view(encoder, into, rect, bind_group, offset);
                if drawn.cells {
                    pass.set_pipeline(tiles);
                    pass.draw(0..QUAD_VERTICES, 0..params.cols * params.rows * p.count);
                }
                draw_free(&mut pass, world, i, free);
            }
            if world.free_after.get(passes).is_some_and(|r| !r.is_empty()) {
                let params = drawn.frame_params(&world.grid, world.color, frame, &Pass { first: 0, count: 0 }, v as u32);
                let offset = write_params(MAX_PASSES, params);
                let mut pass = begin_view(encoder, into, rect, bind_group, offset);
                draw_free(&mut pass, world, passes, free);
            }
            if let Some(r) = sources[v] {
                let params = OutputParams {
                    src_x: r[0] as i32,
                    src_y: r[1] as i32,
                    dst_x: camera.origin.0,
                    dst_y: camera.origin.1,
                    zoom_num: camera.zoom.0.max(1),
                    zoom_den: camera.zoom.1.max(1),
                    bits: 0,
                    _pad: 0,
                };
                let offset = write_output(v, params);
                let mut pass = begin_pass(encoder, &canvas, wgpu::LoadOp::Load);
                pass.set_pipeline(if direct { &self.output_pipeline } else { &self.blit_pipeline });
                pass.set_scissor_rect(on_target[0], on_target[1], on_target[2], on_target[3]);
                pass.set_bind_group(0, &self.sources.as_ref().expect("created above").output, &[offset]);
                pass.draw(0..3, 0..1);
            }
        }
        if direct {
            return;
        }
        let scratch = self.scratch.as_ref().expect("created above");
        let params = OutputParams { zoom_num: 1, zoom_den: 1, bits, ..Default::default() };
        let offset = write_output(MAX_VIEWS, params);
        let mut pass = begin_pass(encoder, &view, wgpu::LoadOp::Load);
        pass.set_pipeline(&self.output_pipeline);
        pass.set_bind_group(0, &scratch.output, &[offset]);
        pass.draw(0..3, 0..1);
        for (i, c) in copies.iter().enumerate() {
            let Some(on_target) = c.scissor((size.width, size.height)) else { continue };
            let (zoom_num, zoom_den) = (c.zoom.0.max(1), c.zoom.1.max(1));
            let [src_x, src_y, _, _] = c.source;
            let params = OutputParams { src_x, src_y, dst_x: c.at.0, dst_y: c.at.1, zoom_num, zoom_den, bits, _pad: 0 };
            let offset = write_output(MAX_VIEWS + 1 + i, params);
            pass.set_scissor_rect(on_target[0], on_target[1], on_target[2], on_target[3]);
            pass.set_bind_group(0, &scratch.output, &[offset]);
            pass.draw(0..3, 0..1);
        }
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

/// Where each zoomed view's picture goes in the source texture (x, y, w,
/// h), in rows at most `MAX_SOURCE` wide; `None` for views at zoom 1.
fn source_rects(cameras: &[Camera]) -> Vec<Option<[u32; 4]>> {
    let (mut x, mut y, mut row) = (0, 0, 0);
    cameras
        .iter()
        .map(|c| {
            if !c.is_scaled() {
                return None;
            }
            let (w, h) = c.source_size();
            if x + w > crate::camera::MAX_SOURCE {
                (x, y, row) = (0, y + row, 0);
            }
            let r = [x, y, w, h];
            x += w;
            row = row.max(h);
            Some(r)
        })
        .collect()
}

/// Free sprites that follow pass `i`, into `pass` (whose bind group is
/// already set): runs of ordinary ones with `pipelines.0`, of inverting
/// ones with `pipelines.1`.
fn draw_free(pass: &mut wgpu::RenderPass<'_>, world: &WorldBuffers, i: usize, pipelines: (&wgpu::RenderPipeline, &wgpu::RenderPipeline)) {
    for (range, invert) in world.free_after.get(i).into_iter().flatten() {
        pass.set_pipeline(if *invert { pipelines.1 } else { pipelines.0 });
        pass.draw(0..QUAD_VERTICES, range.clone());
    }
}

/// Runs of free sprites in drawing order: which instances, and whether
/// they invert ([`crate::Blend::Invert`], drawn with their own pipeline).
type FreeRuns = Vec<(std::ops::Range<u32>, bool)>;

/// The world's free sprites in drawing order (by the pass they follow,
/// then list order) and per pass the runs of them that follow it; the last
/// entry follows all passes.
fn free_sprites(world: &TileWorld) -> (Vec<FreeSprite>, Vec<FreeRuns>) {
    let passes = world.passes().len().min(MAX_PASSES);
    let mut order: Vec<&crate::PlacedSprite> = world.free.iter().collect();
    order.sort_by_key(|s| s.after_pass.min(passes));
    let mut runs: Vec<FreeRuns> = vec![Vec::new(); passes + 1];
    for (i, s) in order.iter().enumerate() {
        let (p, invert, i) = (s.after_pass.min(passes), s.blend == crate::Blend::Invert, i as u32);
        match runs[p].last_mut() {
            Some((range, inv)) if *inv == invert && range.end == i => range.end = i + 1,
            _ => runs[p].push((i..i + 1, invert)),
        }
    }
    let mut gpu: Vec<FreeSprite> = order.iter().map(|s| s.gpu()).collect();
    if gpu.is_empty() {
        gpu.push(FreeSprite::default());
    }
    (gpu, runs)
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

fn storage(device: &wgpu::Device, label: &str, contents: &[u8]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    })
}
