//! Camera/output controls remain immutable across encoded, unsubmitted frames.
use tile_render::{AtlasBuilder, Camera, ColorModel, Grid, Headless, Layout, PlacedSprite, Present, RgbaImage, SpriteSets, TileRenderer, TileWorld, reference};
const CLEAR: [u8; 4] = [11, 21, 31, 255];

fn target(gpu: &Headless, size: (u32, u32)) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("control snapshot target"), size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[],
    })
}

fn fixture(gpu: &Headless, color: ColorModel) -> (TileRenderer, tile_render::Atlas, TileWorld) {
    let mut builder = AtlasBuilder::new(64);
    let sprite = builder.add(&RgbaImage::from_fn(64, 16, |x, y| [x as u8 * 3, y as u8 * 9, 71, 255]), (0, 0)).unwrap();
    let atlas = builder.finish();
    let mut sets = SpriteSets::default();
    let set = sets.single(sprite);
    let mut world = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, step: (8, 8), wrap_x: false, wrap_y: false }, sets);
    world.color = color;
    world.free.push(PlacedSprite::new(set, 0, 0));
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    (renderer, atlas, world)
}

#[test]
fn three_prepared_frames_survive_one_submit_and_recycle_without_a_completion_wait() {
    let gpu = Headless::new().expect("GPU");
    for color in [ColorModel::Direct, ColorModel::Quantized { bits: 5 }] {
        let (mut renderer, atlas, world) = fixture(&gpu, color);
        let size = (16, 8);
        let textures: Vec<_> = (0..4).map(|_| target(&gpu, size)).collect();
        let cameras: Vec<_> = (0..4).map(|i| Camera { x: i * 8, ..Camera::new(size) }).collect();
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        for i in 0..3 {
            assert!(renderer.render_layered(&gpu.device, &gpu.queue, &mut encoder, &textures[i], &[cameras[i]], 1, &[], 0, CLEAR, Present::unscaled(size)));
        }
        let (bytes, writes) = renderer.control_upload_stats();
        assert!(bytes > 0);
        assert_eq!(writes, if color == ColorModel::Direct { 1 } else { 2 });
        eprintln!("{color:?} control snapshot: {bytes} bytes, {writes} queue writes");
        assert!(!renderer.control_frame_available());
        assert!(!renderer.render_layered(&gpu.device, &gpu.queue, &mut encoder, &textures[3], &[cameras[3]], 1, &[], 0, CLEAR, Present::unscaled(size)));
        gpu.queue.submit([encoder.finish()]);
        renderer.controls_submitted();
        assert!(renderer.control_frame_available());
        // No device poll/readback between submissions: queue order protects the
        // first three snapshots when the fourth recycles their control storage.
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        assert!(renderer.render_layered(&gpu.device, &gpu.queue, &mut encoder, &textures[3], &[cameras[3]], 1, &[], 0, CLEAR, Present::unscaled(size)));
        gpu.queue.submit([encoder.finish()]);
        renderer.controls_submitted();
        for (texture, camera) in textures.iter().zip(&cameras) {
            let encoder = gpu.device.create_command_encoder(&Default::default());
            let got = tile_render::gpu::read_texture(&gpu.device, &gpu.queue, encoder, texture);
            let want = reference::render(&world, &atlas, camera, 0, CLEAR);
            assert_eq!(got.pixels, want.pixels, "camera x={}", camera.x);
        }
    }
}

#[test]
fn cached_output_snapshots_keep_distinct_offsets_before_submission() {
    let gpu = Headless::new().expect("GPU");
    let (mut renderer, _, _) = fixture(&gpu, ColorModel::Quantized { bits: 5 });
    let original = gpu.render(&mut renderer, &Camera::new((16, 8)), 0, CLEAR);
    let size = (32, 16);
    let textures: Vec<_> = (0..3).map(|_| target(&gpu, size)).collect();
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    for (i, texture) in textures.iter().enumerate() {
        let present = Present { canvas: (16, 8), zoom: (1, 1), at: (i as i32 * 4, 2), target: size };
        assert!(renderer.render_cached_picture(&gpu.device, &gpu.queue, &mut encoder, texture, &[], CLEAR, present));
    }
    let (bytes, writes) = renderer.control_upload_stats();
    assert_eq!(writes, 1, "a replay uploads only its output controls");
    eprintln!("cached control snapshot: {bytes} bytes, {writes} queue write");
    assert!(!renderer.control_frame_available());
    gpu.queue.submit([encoder.finish()]);
    renderer.controls_submitted();
    for (i, texture) in textures.iter().enumerate() {
        let got = tile_render::gpu::read_texture(&gpu.device, &gpu.queue, gpu.device.create_command_encoder(&Default::default()), texture);
        for y in 0..size.1 { for x in 0..size.0 {
            let ox = i as u32 * 4;
            let want = if x >= ox && x < ox + 16 && y >= 2 && y < 10 { original.get(x - ox, y - 2) } else { CLEAR };
            assert_eq!(got.get(x, y), want, "cached {i}, pixel ({x}, {y})");
        } }
    }
}

#[test]
fn several_views_batch_controls_into_two_uploads() {
    let gpu = Headless::new().expect("GPU");
    let (mut renderer, atlas, world) = fixture(&gpu, ColorModel::Quantized { bits: 5 });
    let cameras: Vec<_> = (0..3).map(|i| Camera { x: i * 8, origin: (i * 16, 0), ..Camera::new((16, 8)) }).collect();
    let got = gpu.render_views(&mut renderer, &cameras, (48, 8), 0, CLEAR);
    let want = reference::render_views(&world, &atlas, &cameras, (48, 8), 0, CLEAR);
    assert_eq!(got.pixels, want.pixels);
    let (bytes, writes) = renderer.control_upload_stats();
    assert_eq!(writes, 2, "three camera blocks and output controls use two uploads");
    eprintln!("three-view control snapshot: {bytes} bytes, {writes} queue writes; formerly 4 writes");
}

/// The tile renderer's independent GPU-test shader module.
const SHADERS: tile_render::Shaders = tile_render::Shaders::new(tile_render_test_spirv::SPIRV, tile_render_test_spirv::SPIRV_FALLBACK);
