//! Independent byte oracle for the final GPU output pass, including values
//! outside the ordinary biased RGB555 range and the integer-float fallback.
use tile_render::{Headless, RgbaImage};
use tile_render_shared::OutputParams;
use wgpu::util::DeviceExt;

const SOURCE: (u32, u32) = (256, 32);
const ALPHAS: [u8; 8] = [0, 1, 31, 32, 127, 128, 254, 255];

fn source_pixel(x: u32, y: u32) -> [u8; 4] {
    [x as u8, (255 - x) as u8, (x as u8).wrapping_add((y * 37) as u8), ALPHAS[y as usize % 8]]
}

fn decoded(value: u8, bits: u32, float_encoding: bool) -> u8 {
    if bits == 0 { return value; }
    // Encoded float sources are populated as B + value, unlike UNORM8.
    let q = if float_encoding { u32::from(value) } else { u32::from(value.saturating_sub(16)) };
    let shift = 8 - bits;
    ((q << shift) | (q >> (bits - shift))).min(255) as u8
}

fn draw(gpu: &Headless, pipeline: &wgpu::RenderPipeline, layout: &wgpu::BindGroupLayout, source: &wgpu::Texture, params: OutputParams, size: (u32, u32)) -> RgbaImage {
    let uniform = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("output decode controls"), contents: bytemuck::bytes_of(&params), usage: wgpu::BufferUsages::UNIFORM,
    });
    let source_view = source.create_view(&Default::default());
    let bindings = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("output decode source"), layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&source_view) },
            wgpu::BindGroupEntry { binding: 1, resource: uniform.as_entire_binding() },
        ],
    });
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("output decode target"), size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("output decode oracle"), color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view, depth_slice: None, resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })], ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bindings, &[]);
        pass.draw(0..3, 0..1);
    }
    tile_render::gpu::read_texture(&gpu.device, &gpu.queue, encoder, &target)
}

#[test]
fn output_decoder_matches_manual_bytes_across_depths_encodings_zooms_and_origins() {
    let gpu = Headless::new().expect("GPU");
    eprintln!("numeric oracle adapter: {:?}", gpu.adapter.get_info());
    let bytes: &[u8] = if gpu.device.features().contains(wgpu::Features::DUAL_SOURCE_BLENDING) {
        include_bytes!(env!("TILE_RENDER_SPV"))
    } else {
        include_bytes!(env!("TILE_RENDER_SPV_FALLBACK"))
    };
    let shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("output decode shader"), source: wgpu::util::make_spirv(bytes),
    });
    let layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("output decode layout"), entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
        ],
    });
    let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("output decode pipeline layout"), bind_group_layouts: &[Some(&layout)], immediate_size: 0,
    });
    let pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("output decode pipeline"), layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("fullscreen_vs"), compilation_options: Default::default(), buffers: &[] },
        primitive: Default::default(), depth_stencil: None, multisample: Default::default(),
        fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("output_fs"), compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
        multiview_mask: None, cache: None,
    });
    let mut pixels_checked = 0usize;
    for float_encoding in [false, true] {
        let source = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("all output channel bytes"), size: wgpu::Extent3d { width: SOURCE.0, height: SOURCE.1, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            // Every tested integer is also exactly representable in FP16.
            format: if float_encoding { wgpu::TextureFormat::Rgba32Float } else { wgpu::TextureFormat::Rgba8Unorm },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
        });
        let bytes: Vec<u8> = if float_encoding {
            let words: Vec<f32> = (0..SOURCE.1).flat_map(|y| (0..SOURCE.0).flat_map(move |x| {
                let p = source_pixel(x, y);
                [1536.0 + f32::from(p[0]), 1536.0 + f32::from(p[1]), 1536.0 + f32::from(p[2]), f32::from(p[3]) / 255.0]
            })).collect();
            bytemuck::cast_slice(&words).to_vec()
        } else {
            (0..SOURCE.1).flat_map(|y| (0..SOURCE.0).flat_map(move |x| source_pixel(x, y))).collect()
        };
        gpu.queue.write_texture(source.as_image_copy(), &bytes, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(SOURCE.0 * if float_encoding { 16 } else { 4 }), rows_per_image: Some(SOURCE.1) }, source.size());
        for bits in [0, 4, 5, 6, 7, 8] {
            if float_encoding && bits == 0 { continue; }
            for (size, zoom, src, dst) in [
                ((256, 8), (1, 1), (0, 0), (0, 0)),
                ((160, 12), (2, 1), (31, 5), (3, 2)),
                ((160, 12), (3, 2), (7, 3), (-3, -1)),
                ((160, 12), (2, 3), (11, 4), (5, 1)),
            ] {
                let params = OutputParams { src_x: src.0, src_y: src.1, dst_x: dst.0, dst_y: dst.1,
                    zoom_num: zoom.0, zoom_den: zoom.1, bits, quantized_float: u32::from(float_encoding), ..Default::default() };
                let got = draw(&gpu, &pipeline, &layout, &source, params, size);
                for y in 0..size.1 { for x in 0..size.0 {
                    let sx = src.0 as u32 + (x as i32 - dst.0).max(0) as u32 * zoom.1 / zoom.0;
                    let sy = src.1 as u32 + (y as i32 - dst.1).max(0) as u32 * zoom.1 / zoom.0;
                    assert!(sx < SOURCE.0 && sy < SOURCE.1);
                    let p = source_pixel(sx, sy);
                    let want = [decoded(p[0], bits, float_encoding), decoded(p[1], bits, float_encoding), decoded(p[2], bits, float_encoding), p[3]];
                    assert_eq!(got.get(x, y), want, "bits={bits}, float={float_encoding}, zoom={zoom:?}, src={src:?}, dst={dst:?}, pixel=({x}, {y})");
                    pixels_checked += 1;
                } }
            }
        }
    }
    eprintln!("output byte oracle checked {pixels_checked} pixels");
}
