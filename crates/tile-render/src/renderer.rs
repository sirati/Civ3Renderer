//! The wgpu side: pipeline, GPU buffers and the draw call.

use std::num::NonZeroU64;

use tile_render_shared::{FrameParams, LayerDesc, QUAD_VERTICES, SpriteDesc};
use wgpu::util::DeviceExt;

use crate::atlas::Atlas;
use crate::camera::Camera;
use crate::world::{Blend, Grid, Pass, TileWorld};

const SPV: &[u8] = include_bytes!(env!("TILE_RENDER_SPV"));

/// Per-pass `FrameParams` live in one buffer at this stride (the minimum
/// uniform offset alignment wgpu guarantees).
const PARAMS_STRIDE: u64 = 256;
/// Most passes a world may have.
pub const MAX_PASSES: usize = 16;

/// Draws one `TileWorld` with one `Atlas`.
pub struct TileRenderer {
    /// For `Blend::Alpha` passes.
    pipeline: wgpu::RenderPipeline,
    /// For `Blend::Scale` passes: blending off, reads the backdrop.
    scale_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    backdrop_layout: wgpu::BindGroupLayout,
    /// Copy of the target taken before a `Blend::Scale` pass.
    backdrop: Option<Backdrop>,
    params: wgpu::Buffer,
    atlas: Option<wgpu::TextureView>,
    sprites: Option<wgpu::Buffer>,
    world: Option<WorldBuffers>,
    bind_group: Option<wgpu::BindGroup>,
}

struct Backdrop {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

struct WorldBuffers {
    grid: Grid,
    passes: Vec<Pass>,
    layers: wgpu::Buffer,
    tile_data: wgpu::Buffer,
    lut: wgpu::Buffer,
    sets: wgpu::Buffer,
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
        let backdrop_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tile-render backdrop"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline = |name: &str, entry: &str, groups: &[Option<&wgpu::BindGroupLayout>], blend| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(name),
                bind_group_layouts: groups,
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(name),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("tile_vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: target_format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let alpha = pipeline("tile-render", "tile_fs", &[Some(&layout)], Some(wgpu::BlendState::ALPHA_BLENDING));
        let scale = pipeline("tile-render scale", "tile_fs_scale", &[Some(&layout), Some(&backdrop_layout)], None);
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-render params"),
            size: PARAMS_STRIDE * MAX_PASSES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline: alpha,
            scale_pipeline: scale,
            layout,
            backdrop_layout,
            backdrop: None,
            params,
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
        self.world = Some(WorldBuffers {
            grid: world.grid,
            passes: world.passes(),
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
            Some(w) if w.grid == world.grid && w.tile_data.size() == (packed.data.len() * 4) as u64 => {
                queue.write_buffer(&w.tile_data, 0, bytemuck::cast_slice(&packed.data));
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
            ],
        }));
    }

    /// Renders the world into `target` as seen by `camera` at animation
    /// tick `frame`, clearing it to `clear` first. Only draws once both an
    /// atlas and a world are set. Worlds with `Blend::Scale` passes need a
    /// target with `COPY_SRC` usage; without it those passes are skipped.
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
        // Clear on its own, so every pass can load.
        drop(begin_pass(encoder, &view, wgpu::LoadOp::Clear(crate::gpu::color(clear))));
        let (Some(bind_group), Some(world)) = (&self.bind_group, &self.world) else { return };
        let copyable = target.usage().contains(wgpu::TextureUsages::COPY_SRC);
        for (i, p) in world.passes.iter().enumerate().take(MAX_PASSES) {
            let params = camera.frame_params(&world.grid, frame, p);
            let offset = i as u64 * PARAMS_STRIDE;
            queue.write_buffer(&self.params, offset, bytemuck::bytes_of(&params));
            let backdrop = match p.blend {
                Blend::Alpha => None,
                Blend::Scale { .. } if !copyable => {
                    log::warn!("skipping a scale-blend pass: the target is not COPY_SRC");
                    continue;
                }
                Blend::Scale { .. } => {
                    let b = backdrop_for(&mut self.backdrop, device, &self.backdrop_layout, target);
                    encoder.copy_texture_to_texture(target.as_image_copy(), b.texture.as_image_copy(), target.size());
                    Some(&b.bind_group)
                }
            };
            let mut pass = begin_pass(encoder, &view, wgpu::LoadOp::Load);
            pass.set_pipeline(if backdrop.is_some() { &self.scale_pipeline } else { &self.pipeline });
            pass.set_bind_group(0, bind_group, &[offset as u32]);
            if let Some(b) = backdrop {
                pass.set_bind_group(1, b, &[]);
            }
            pass.draw(0..QUAD_VERTICES, 0..params.cols * params.rows * p.count);
        }
    }
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

/// The backdrop texture for `target`, (re)created when its size or format
/// changed.
fn backdrop_for<'a>(
    slot: &'a mut Option<Backdrop>,
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    target: &wgpu::Texture,
) -> &'a Backdrop {
    let fits = slot.as_ref().is_some_and(|b| b.texture.size() == target.size() && b.texture.format() == target.format());
    if !fits {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tile-render backdrop"),
            size: target.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: target.format(),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tile-render backdrop"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture.create_view(&Default::default())),
            }],
        });
        *slot = Some(Backdrop { texture, bind_group });
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
