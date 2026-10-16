//! The wgpu side: pipeline, GPU buffers and the draw call.

use std::num::NonZeroU64;

use tile_render_shared::{FrameParams, FreeSprite, LayerDesc, QUAD_VERTICES, SpriteDesc};
use wgpu::util::DeviceExt;

use crate::atlas::Atlas;
use crate::camera::Camera;
use crate::world::{ColorModel, Grid, Pass, TileWorld};

const SPV: &[u8] = include_bytes!(env!("TILE_RENDER_SPV"));

/// Per-pass `FrameParams` live in one buffer at this stride (the minimum
/// uniform offset alignment wgpu guarantees).
const PARAMS_STRIDE: u64 = 256;
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
    /// Expands the internal target into the caller's target.
    output_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    output_layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    /// The colour depth for `output_fs`.
    bits: wgpu::Buffer,
    scratch: Option<Scratch>,
    atlas: Option<wgpu::TextureView>,
    sprites: Option<wgpu::Buffer>,
    world: Option<WorldBuffers>,
    bind_group: Option<wgpu::BindGroup>,
}

/// The internal target of quantized colour models and the bind group
/// reading it.
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
    free_after: Vec<std::ops::Range<u32>>,
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
                        has_dynamic_offset: false,
                        min_binding_size: None,
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
        let output = pipeline("tile-render output", ("fullscreen_vs", "output_fs"), &output_layout, target_format, None);
        let bits = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-render colour bits"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-render params"),
            size: PARAMS_STRIDE * MAX_PASSES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline: direct,
            quantized_pipeline: quantized,
            free_pipeline: free_direct,
            free_quantized_pipeline: free_quantized,
            output_pipeline: output,
            layout,
            output_layout,
            params,
            bits,
            scratch: None,
            atlas: None,
            sprites: None,
            world: None,
            bind_group: None,
        }
    }

    /// Uploads the atlas pages and sprite table.
    pub fn set_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, atlas: &Atlas) {
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
        let view = target.create_view(&Default::default());
        let (Some(bind_group), Some(world)) = (&self.bind_group, &self.world) else {
            drop(begin_pass(encoder, &view, wgpu::LoadOp::Clear(crate::gpu::color(clear))));
            return;
        };
        let bits = world.color.bits();
        if bits == 0 {
            drop(begin_pass(encoder, &view, wgpu::LoadOp::Clear(crate::gpu::color(clear))));
            for (i, p) in world.passes.iter().enumerate().take(MAX_PASSES) {
                let params = camera.frame_params(&world.grid, world.color, frame, p);
                let offset = i as u64 * PARAMS_STRIDE;
                queue.write_buffer(&self.params, offset, bytemuck::bytes_of(&params));
                let mut pass = begin_pass(encoder, &view, wgpu::LoadOp::Load);
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, bind_group, &[offset as u32]);
                pass.draw(0..QUAD_VERTICES, 0..params.cols * params.rows * p.count);
                draw_free(&mut pass, world, i, &self.free_pipeline);
            }
            draw_free_last(&self.params, queue, encoder, &view, world, camera, frame, bind_group, &self.free_pipeline);
            return;
        }

        queue.write_buffer(&self.bits, 0, bytemuck::bytes_of(&[bits, 0, 0, 0]));
        let scratch = scratch_for(&mut self.scratch, device, &self.output_layout, &self.bits, target.size());
        let main = scratch.main.create_view(&Default::default());
        let q = |c: u8| f64::from(tile_render_shared::quantize(c.into(), bits) + tile_render_shared::QUANTIZED_BIAS) / 255.0;
        let background = wgpu::Color { r: q(clear[0]), g: q(clear[1]), b: q(clear[2]), a: f64::from(clear[3]) / 255.0 };
        drop(begin_pass(encoder, &main, wgpu::LoadOp::Clear(background)));
        for (i, p) in world.passes.iter().enumerate().take(MAX_PASSES) {
            let params = camera.frame_params(&world.grid, world.color, frame, p);
            let offset = i as u64 * PARAMS_STRIDE;
            queue.write_buffer(&self.params, offset, bytemuck::bytes_of(&params));
            let mut pass = begin_pass(encoder, &main, wgpu::LoadOp::Load);
            pass.set_pipeline(&self.quantized_pipeline);
            pass.set_bind_group(0, bind_group, &[offset as u32]);
            pass.draw(0..QUAD_VERTICES, 0..params.cols * params.rows * p.count);
            draw_free(&mut pass, world, i, &self.free_quantized_pipeline);
        }
        draw_free_last(&self.params, queue, encoder, &main, world, camera, frame, bind_group, &self.free_quantized_pipeline);
        let mut pass = begin_pass(encoder, &view, wgpu::LoadOp::Load);
        pass.set_pipeline(&self.output_pipeline);
        pass.set_bind_group(0, &scratch.output, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Free sprites that follow pass `i`, into `pass` (whose bind group is
/// already set).
fn draw_free(pass: &mut wgpu::RenderPass<'_>, world: &WorldBuffers, i: usize, pipeline: &wgpu::RenderPipeline) {
        let Some(range) = world.free_after.get(i).filter(|r| !r.is_empty()) else { return };
        pass.set_pipeline(pipeline);
        pass.draw(0..QUAD_VERTICES, range.clone());
    }

/// Free sprites after the last pass, in a pass of their own.
#[allow(clippy::too_many_arguments)]
fn draw_free_last(
        params_buffer: &wgpu::Buffer,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        world: &WorldBuffers,
        camera: &Camera,
        frame: u32,
        bind_group: &wgpu::BindGroup,
        pipeline: &wgpu::RenderPipeline,
    ) {
        let last = world.passes.len().min(MAX_PASSES);
        let Some(range) = world.free_after.get(last).filter(|r| !r.is_empty()) else { return };
        let params = camera.frame_params(&world.grid, world.color, frame, &Pass { first: 0, count: 0 });
        let offset = last.min(MAX_PASSES - 1) as u64 * PARAMS_STRIDE;
        if last == MAX_PASSES {
            log::warn!("free sprites after the last of {MAX_PASSES} passes share its parameters");
        } else {
            queue.write_buffer(params_buffer, offset, bytemuck::bytes_of(&params));
        }
        let mut pass = begin_pass(encoder, view, wgpu::LoadOp::Load);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[offset as u32]);
        pass.draw(0..QUAD_VERTICES, range.clone());
}

/// The world's free sprites in drawing order (by the pass they follow,
/// then list order) and per pass the range of them that follows it; the
/// last range follows all passes.
fn free_sprites(world: &TileWorld) -> (Vec<FreeSprite>, Vec<std::ops::Range<u32>>) {
    let passes = world.passes().len().min(MAX_PASSES);
    let mut order: Vec<&crate::PlacedSprite> = world.free.iter().collect();
    order.sort_by_key(|s| s.after_pass.min(passes));
    let mut ranges = vec![0..0; passes + 1];
    for (i, s) in order.iter().enumerate() {
        let p = s.after_pass.min(passes);
        if ranges[p].is_empty() {
            ranges[p] = i as u32..i as u32;
        }
        ranges[p].end = i as u32 + 1;
    }
    let mut gpu: Vec<FreeSprite> = order.iter().map(|s| s.gpu()).collect();
    if gpu.is_empty() {
        gpu.push(FreeSprite::default());
    }
    (gpu, ranges)
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

/// The scratch texture for a target of `size`, (re)created when it changed.
fn scratch_for<'a>(
    slot: &'a mut Option<Scratch>,
    device: &wgpu::Device,
    output_layout: &wgpu::BindGroupLayout,
    bits: &wgpu::Buffer,
    size: wgpu::Extent3d,
) -> &'a Scratch {
    if slot.as_ref().is_none_or(|s| s.main.size() != size) {
        let main = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tile-render quantized target"),
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
                wgpu::BindGroupEntry { binding: 1, resource: bits.as_entire_binding() },
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
