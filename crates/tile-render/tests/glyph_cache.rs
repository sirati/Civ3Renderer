//! GPU coverage ownership across cancellation, repacked faces and eviction.
//! The independent real-font oracle also compares cached and procedural output.
use std::sync::Arc;
use tile_render::{gpu, gpu_text::Face, *};

const CLEAR: [u8; 4] = [11, 37, 71, 255];
const SIZE: (u32, u32) = (80, 80);

fn polygon(points: &[(f64, f64)]) -> Vec<[(f64, f64); 3]> {
    (0..points.len()).map(|i| {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        [a, ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), b]
    }).collect()
}

fn face() -> Arc<Face> {
    Arc::new(Face::from_curves(1000, &[
        ('A', polygon(&[(0.0, 0.0), (0.0, 1300.0), (1500.0, 1300.0), (1500.0, 0.0)]), 1500),
        ('B', polygon(&[(0.0, 0.0), (450.0, 900.0), (900.0, 0.0)]), 900),
        ('C', polygon(&[(0.0, -150.0), (0.0, 800.0), (250.0, 800.0), (500.0, -150.0)]), 500),
    ]))
}

fn world(fonts: Vec<Arc<Face>>, font: u32, count: usize) -> TileWorld {
    let mut world = TileWorld::new(Grid {
        cols: 1, rows: 1, layout: Layout::Orthogonal, step: (1024, 512), wrap_x: false, wrap_y: false,
    }, SpriteSets::default());
    for i in 0..count {
        let glyph = OutlineGlyph {
            font, shape: GlyphShape::Glyph(fonts[font as usize].glyph(['A', 'B', 'C'][i]).unwrap()),
            em64: 19 * 64 + i as u32, advance: 24, bold: i == 1, italic: i == 2,
            color: [227, 91, 19], key: (i == 1).then_some([248, 0, 248]),
        };
        world.free.push(PlacedSprite {
            text: Some(glyph), zoom_art: ZoomArt::Any,
            ..PlacedSprite::new(SpriteSetId::EMPTY, 20 + i as i32 * 160, 45)
        });
    }
    world.fonts = fonts;
    world
}

fn renderer(gpu: &Headless, world: &TileWorld) -> TileRenderer {
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_world(&gpu.device, world);
    renderer
}

fn target(gpu: &Headless) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("glyph coverage ownership test"),
        size: wgpu::Extent3d { width: SIZE.0, height: SIZE.1, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn camera(x: i32) -> Camera {
    Camera { x, zoom: (5, 4), zoomed_art: true, ..Camera::new(SIZE) }
}

fn read(gpu: &Headless, texture: &wgpu::Texture) -> RgbaImage {
    gpu::read_texture(&gpu.device, &gpu.queue, gpu.device.create_command_encoder(&Default::default()), texture)
}

fn fresh(gpu: &Headless, world: &TileWorld, camera: &Camera) -> RgbaImage {
    gpu.render(&mut renderer(gpu, world), camera, 0, CLEAR)
}

#[test]
fn canceled_glyph_fills_are_recovered_after_another_encoder_is_submitted() {
    let gpu = Headless::new().expect("GPU");
    let world = world(vec![face()], 0, 3);
    let mut renderer = renderer(&gpu, &world);
    let targets: Vec<_> = (0..2).map(|_| target(&gpu)).collect();
    let mut canceled = gpu.device.create_command_encoder(&Default::default());
    assert!(renderer.render_layered(&gpu.device, &gpu.queue, &mut canceled, &targets[0],
        &[camera(0)], 1, &[], 0, CLEAR, Present::unscaled(SIZE)));
    drop(canceled);
    let mut submitted = gpu.device.create_command_encoder(&Default::default());
    assert!(renderer.render_layered(&gpu.device, &gpu.queue, &mut submitted, &targets[1],
        &[camera(160)], 1, &[], 0, CLEAR, Present::unscaled(SIZE)));
    gpu.queue.submit([submitted.finish()]);
    renderer.controls_submitted();
    let got = gpu.render(&mut renderer, &camera(0), 0, CLEAR);
    assert!(got.pixels.iter().any(|p| *p != CLEAR));
    assert_eq!(got.pixels, fresh(&gpu, &world, &camera(0)).pixels);
    assert!(renderer.glyph_cache_stats().hits > 0);
}

#[test]
fn three_prepared_font_repackings_keep_their_own_outline_buffers_and_live_colors() {
    let gpu = Headless::new().expect("GPU");
    let stable = face();
    let filler_a = face();
    let filler_c = face();
    let mut worlds = [
        world(vec![filler_a.clone(), stable.clone()], 1, 1),
        world(vec![stable.clone()], 0, 1),
        world(vec![filler_c, filler_a, stable], 2, 1),
    ];
    for (i, world) in worlds.iter_mut().enumerate() {
        let glyph = world.free[0].text.as_mut().unwrap();
        glyph.color = [[227, 91, 19], [11, 197, 249], [251, 251, 251]][i];
        glyph.key = (i == 1).then_some([248, 0, 248]);
    }
    let mut renderer = renderer(&gpu, &worlds[0]);
    let targets: Vec<_> = (0..3).map(|_| target(&gpu)).collect();
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    for (i, world) in worlds.iter().enumerate() {
        renderer.set_world(&gpu.device, world);
        assert!(renderer.render_layered(&gpu.device, &gpu.queue, &mut encoder, &targets[i],
            &[camera(0)], 1, &[], 0, CLEAR, Present::unscaled(SIZE)));
    }
    assert!(!renderer.control_frame_available());
    let stats = renderer.glyph_cache_stats();
    assert_eq!(stats.allocation_count, 1);
    assert!(stats.hits >= 2);
    gpu.queue.submit([encoder.finish()]);
    renderer.controls_submitted();
    for (texture, world) in targets.iter().zip(&worlds) {
        assert_eq!(read(&gpu, texture).pixels, fresh(&gpu, world, &camera(0)).pixels);
    }
}

#[test]
fn bounded_default_atlas_eviction_regenerates_a_revisited_glyph() {
    let gpu = Headless::new().expect("GPU");
    let mut world = world(vec![face()], 0, 1);
    let mut renderer = renderer(&gpu, &world);
    let texture = target(&gpu);
    let camera = Camera { zoomed_art: true, ..Camera::new(SIZE) };
    let mut first = None;
    for i in 0..240 {
        let sprite = &mut world.free[0];
        sprite.x = -17 + i % 3;
        sprite.y = 75;
        let glyph = sprite.text.as_mut().unwrap();
        glyph.em64 = 180 * 64 + i as u32 * 20;
        glyph.advance = 155;
        glyph.bold = i % 2 == 0;
        glyph.italic = i % 3 == 0;
        if i == 0 { first = Some(sprite.clone()); }
        renderer.update_free(&gpu.device, &gpu.queue, &world);
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        assert!(renderer.render_layered(&gpu.device, &gpu.queue, &mut encoder, &texture,
            &[camera], 1, &[], i as u32, CLEAR, Present::unscaled(SIZE)));
        gpu.queue.submit([encoder.finish()]);
        renderer.controls_submitted();
    }
    world.free[0] = first.unwrap();
    renderer.update_free(&gpu.device, &gpu.queue, &world);
    let got = gpu.render(&mut renderer, &camera, 0, CLEAR);
    assert_eq!(got.pixels, fresh(&gpu, &world, &camera).pixels);
    let stats = renderer.glyph_cache_stats();
    assert!(stats.enabled);
    assert_eq!(stats.atlas_bytes, 16 * 1024 * 1024);
    assert!(stats.reset_count > 0, "fixture must actually evict the default cache: {stats:?}");
    assert!(stats.entry_count <= 16_384);
    assert!(stats.pinned_face_word_bytes <= 64 * 1024 * 1024);
    assert!(stats.resident_bytes >= stats.atlas_bytes + 65_536);
}

/// The viewer's shader module (it holds the tile renderer's shaders).
const SHADERS: tile_render::Shaders = tile_render::Shaders::new(viewer_spirv::SPIRV, viewer_spirv::SPIRV_FALLBACK);
