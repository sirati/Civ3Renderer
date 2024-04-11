use spirv_std::glam::{UVec2, Vec4};
// Rust code to define data structures and the function signatures
use civ3_renderer_shared::TextureAtlasMap;

const SHADER: &[u8] = include_bytes!(env!("civ3-renderer-shared.spv"));

use wgpu::{util::DeviceExt, *};
use civ3_renderer_shared::Params;

async fn setup_pipeline(device: &Device, shader: &[u8]) -> Result<(ComputePipeline, RenderPipeline), Box<dyn std::error::Error>> {
    let shader_module = device.create_shader_module(&ShaderModuleDescriptor {
        label: Some("Civ3 Renderer"),
        source: ShaderSource::SpirV(std::borrow::Cow::Borrowed(shader)),
        flags: ShaderFlags::all(),
    });

    let compute_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Compute Pipeline Layout"),
        bind_group_layouts: &[], // Define your bind group layouts here
        push_constant_ranges: &[],
    });

    let compute_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("Compute Pipeline"),
        layout: Some(&compute_pipeline_layout),
        module: &shader_module,
        entry_point: "main_resolve_sprite_and_mark_culled",
    });

    let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[], // Define your bind group layouts here
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: VertexState {
            module: &shader_module,
            entry_point: "main_apply_sprite_uv",
            buffers: &[], // Define your vertex buffers here
        },
        fragment: Some(FragmentState { // Define your fragment state here
            module: &shader_module,
            entry_point: "your_fragment_shader_entry_point",
            targets: &[ColorTargetState {
                format: TextureFormat::Bgra8UnormSrgb,
                blend: Some(BlendState::REPLACE),
                write_mask: ColorWrites::ALL,
            }],
        }),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Cw,
            cull_mode: Some(Face::Back),
            polygon_mode: PolygonMode::Fill,
            clamp_depth: false,
            conservative: false,
        },
        depth_stencil: None, // Define if you need depth-stencil
        multisample: MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
    });

    Ok((compute_pipeline, render_pipeline))
}

struct ShadersResources {
    params_buffer: Buffer,
    leftmost_vertices_buffer: Buffer,
    vertices_generated_buffer: Buffer,
    world_data_buffer: Buffer,
    layer_lookup_table_buffer: Buffer,
    texture_lookup_table_buffer: Buffer,
}

fn create_shader_resources(device: &Device) -> ShadersResources {
    let params_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Params Buffer"),
        size: std::mem::size_of::<Params>() as BufferAddress,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let leftmost_vertices_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Leftmost Vertices Buffer"),
        size: 100 * std::mem::size_of::<UVec2>() as BufferAddress, // assuming a size for demonstration
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let vertices_generated_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Vertices Generated Buffer"),
        size: 100 * std::mem::size_of::<Vec4>() as BufferAddress, // assuming a size for demonstration
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let world_data_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("World Data Buffer"),
        size: 1024, // assuming a size for demonstration
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let layer_lookup_table_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Layer Lookup Table Buffer"),
        size: 256, // assuming a size for demonstration
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let texture_lookup_table_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Texture Lookup Table Buffer"),
        size: 256, // assuming a size for demonstration
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    ShadersResources {
        params_buffer,
        leftmost_vertices_buffer,
        vertices_generated_buffer,
        world_data_buffer,
        layer_lookup_table_buffer,
        texture_lookup_table_buffer,
    }
}

async fn setup_device() -> Result<(Device, Queue), Box<dyn std::error::Error>> {
    let instance = Instance::new(Backends::all());
    let adapter = instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }).await.ok_or("Failed to find a suitable GPU adapter")?;

    let (device, queue) = adapter.request_device(
        &DeviceDescriptor {
            label: Some("Civ3 Renderer Device"),
            required_features: Features::empty(),
            required_limits: Limits::default(),
        },
        None,
    ).await?;

    Ok((device, queue))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (device, queue) = setup_device().await?;
    //let shader_bytes = include_bytes!(env!("CARGO_MANIFEST_DIR").to_owned() + "/civ3-renderer-shared.spv");
    let (compute_pipeline, render_pipeline) = setup_pipeline(&device, shader_bytes).await?;
    // The rest of your rendering or compute logic goes here
    let shader_resources = create_shader_resources(&device);

    // Dummy data and commands just for demonstration; replace them with actual drawing or compute commands.
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("Main Command Encoder"),
    });

    // Begin compute pass
    {
        let mut compute_pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("Compute Pass"),
        });
        compute_pass.set_pipeline(&compute_pipeline);
        // Set your compute bind group and dispatch parameters here
    }

    // Assuming you have a swap chain configured:
    let frame = swap_chain.get_current_frame()?.output;
    let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("Render Pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: &frame.view,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Color {
                    r: 0.1, g: 0.2, b: 0.3, a: 1.0,
                }),
                store: true,
            },
        })],
        depth_stencil_attachment: None,
    });

    render_pass.set_pipeline(&render_pipeline);
    // Set your render bind group and draw parameters here

    // Finish encoding and submit
    queue.submit(Some(encoder.finish()));

    Ok(())
}


//
// struct Renderer {
//     device: wgpu::Device,
//     queue: wgpu::Queue,
//     texture_atlas_maps: Vec<TextureAtlasMap>,
//     data: Vec<Data>,
// }
//
// impl Renderer {
//     fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
//         Renderer {
//             device,
//             queue,
//             texture_atlas_maps: Vec::new(),
//             data: Vec::new(),
//         }
//     }
//
//     fn add_layer(&mut self, layer: Layer) {
//         // Implementation to upload layer data
//     }
//
//     fn set_texture_atlas_map(&mut self, atlas_map: TextureAtlasMap) {
//         self.texture_atlas_maps.push(atlas_map);
//     }
//
//     fn set_data(&mut self, data: Data) {
//         self.data.push(data);
//     }
// }
//
// fn main() {
//     println!("Hello, world!");
//     let lookup_table: Vec<u32> = vec![/* your lookup table values here */];
//
//     let lookup_table_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
//         label: Some("Lookup Table Buffer"),
//         contents: bytemuck::cast_slice(&lookup_table),
//         usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
//     });
//
// // Create a bind group to bind the buffer to the shader
//     let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
//         entries: &[
//             wgpu::BindGroupLayoutEntry {
//                 binding: 0,
//                 visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT, // Adjust as needed
//                 ty: wgpu::BindingType::Buffer {
//                     ty: wgpu::BufferBindingType::Storage { read_only: true },
//                     has_dynamic_offset: false,
//                     min_binding_size: None,
//                 },
//                 count: None,
//             },
//         ],
//         label: Some("lookup_table_bind_group_layout"),
//     });
//
//     let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
//         layout: &bind_group_layout,
//         entries: &[wgpu::BindGroupEntry {
//             binding: 0,
//             resource: lookup_table_buffer.as_entire_binding(),
//         }],
//         label: Some("lookup_table_bind_group"),
//     });
// }
//
//
//
//
// fn generate_isometric_grid(world_size_x: u16, world_size_y: u16) -> Vec<f32> {
//     let mut vertices = vec![];
//
//     let rhombus_vertices =  [
//         -0.5, 0.0, // Left
//         0.0, 0.4, // Top
//         0.5, 0.0, // Right
//         0.0, -0.4, // Bottom
//     ];
//
//     for x in 0..world_size_x {
//         for y in 0..world_size_y {
//             let base_x = x as f32 + 0.5 * (x & 1); // Offset for isometric layout
//             let base_y = y as f32;
//
//             for v in 0..rhombus_vertices.len() / 2 {
//                 vertices.push(rhombus_vertices[2 * v] + base_x);
//                 vertices.push(rhombus_vertices[2 * v + 1] + base_y);
//             }
//         }
//     }
//
//     vertices
// }
//
// async fn setup_gpu_buffers(device: &wgpu::Device, world_size_x: u16, world_size_y: u16) {
//     let vertices = generate_isometric_grid(world_size_x, world_size_y);
//
//     let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
//         label: Some("Vertex Buffer"),
//         contents: bytemuck::cast_slice(&vertices),
//         usage: wgpu::BufferUsages::VERTEX,
//     });
//
//     let params = [world_size_x as u32, world_size_y as u32, max_texture_y as u32];
//     let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
//         label: Some("Params Buffer"),
//         contents: bytemuck::cast_slice(&params),
//         usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
//     });
//
//     // Code to create bind groups and pipeline setup goes here
// }
