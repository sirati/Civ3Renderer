//! Cache-preparation policy, global glyph indices, and queued control ownership.
use super::*;
use crate::{AtlasBuilder, Blend, Headless, Layout, SpriteSetId, PlacedSprite, RgbaImage, SpriteSets, ZoomArt, OutlineGlyph, GlyphShape};
const SIZE: (u32, u32) = (96, 80);
const CLEAR: [u8; 4] = [11, 37, 71, 255];

/// Exercise the adapter's supported optional path, then its portable fallback.
/// Adapter selection follows Headless's production policy on every platform.
fn for_each_gpu(mut test: impl FnMut(&Headless)) {
    let mut gpu = Headless::new().expect("GPU for glyph preparation regression");
    eprintln!("GLYPH_PREPARATION_GPU native {:?} features={:?}",
        gpu.adapter.get_info(), gpu.device.features());
    test(&gpu);
    (gpu.device, gpu.queue) = pollster::block_on(gpu.adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("glyph preparation without optional GPU features"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits {
            max_texture_array_layers: gpu.adapter.limits().max_texture_array_layers,
            ..wgpu::Limits::default().using_resolution(gpu.adapter.limits())
        },
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    })).expect("GPU without optional features for glyph preparation regression");
    assert!(gpu.device.features().is_empty());
    eprintln!("GLYPH_PREPARATION_GPU fallback {:?} features={:?}",
        gpu.adapter.get_info(), gpu.device.features());
    test(&gpu);
}

fn fixture(smoke: bool) -> (TileWorld, crate::Atlas) {
    let mut builder = AtlasBuilder::new(32);
    let mut sets = SpriteSets::default();
    let ordinary = sets.single(builder.add(&RgbaImage::from_fn(30, 20, |_, _| [91, 143, 57, 255]), (0, 0)).unwrap());
    let native = sets.single(builder.add(&RgbaImage::from_fn(12, 12, |_, _| [217, 81, 37, 97]), (0, 0)).unwrap());
    let points = [(0.0, 0.0), (0.0, 900.0), (650.0, 900.0), (650.0, 0.0)];
    let curves = (0..4).map(|i| { let a = points[i]; let b = points[(i+1)%4];
        [a, ((a.0+b.0)/2.0, (a.1+b.1)/2.0), b] }).collect();
    let face = Arc::new(gpu_text::Face::from_curves(1000, &[('A', curves, 700)]));
    let mut world = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal,
        step: (256, 128), wrap_x: false, wrap_y: false }, sets);
    world.fonts.push(face.clone());
    world.free.push(PlacedSprite { screen: true, view: Some(0), ..PlacedSprite::new(ordinary, 4, 4) });
    for (i, view) in [Some(1), None, Some(0), Some(0)].into_iter().enumerate() {
        world.free.push(PlacedSprite { screen: true, view, zoom_art: ZoomArt::Any,
            text: Some(OutlineGlyph { font: 0, shape: GlyphShape::Glyph(face.glyph('A').unwrap()),
                em64: (17 + i as u32) * 64, advance: 18, bold: i == 2, italic: i == 3,
                color: [227, 91, 249], key: (i == 3).then_some([248, 0, 248]) }),
            ..PlacedSprite::new(SpriteSetId::EMPTY, 8 + i as i32 * 12, 42) });
    }
    if smoke { world.free.push(PlacedSprite { screen: true, view: Some(0), blend: Blend::NativePaletteAlpha,
        ..PlacedSprite::new(native, 10, 8) }); }
    (world, builder.finish())
}

fn renderer(gpu: &Headless, world: &TileWorld, atlas: &crate::Atlas) -> TileRenderer {
    let mut r = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    r.set_atlas(&gpu.device, &gpu.queue, atlas);
    r.set_world(&gpu.device, world);
    r
}
fn target(gpu: &Headless) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor { label: Some("glyph preparation skip correctness"),
        size: wgpu::Extent3d { width: SIZE.0, height: SIZE.1, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[] })
}
fn camera() -> Camera { Camera { cells: false, zoomed_art: true, ..Camera::new(SIZE) } }
fn stage(gpu: &Headless, r: &TileRenderer, encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture, slot: usize, camera: &Camera, view: usize, art: u32,
    selected: std::ops::Range<usize>) {
    let world = r.world.as_ref().unwrap();
    if world.smoke_count != 0 && r.smoke.borrow().is_none() {
        *r.smoke.borrow_mut() = Some(smoke::State::new(&gpu.device, 32));
    }
    let steps = Steps { renderer: r, device: &gpu.device, control_slot: slot,
        params: &r.control_uploads[slot].params, world, bind_group: &r.bind_group.as_ref().unwrap()[slot],
        frame: 0, named_palettes: r.named_palettes, timer: None,
        quantized_float: r.quantized_float, cell_indices: &r.cell_indices };
    steps.draw(encoder, &texture.create_view(&Default::default()), texture,
        ([0.0, 0.0, SIZE.0 as f32, SIZE.1 as f32], [0, 0, SIZE.0, SIZE.1]), camera, view,
        (art, (art == 2).then_some(camera.origin)), selected, 0,
        (r.pipeline(&gpu.device, P::Tiles), r.free_pipelines(&gpu.device, 0)));
}
fn clear(gpu: &Headless, encoder: &mut wgpu::CommandEncoder, texture: &wgpu::Texture) {
    drop(begin_pass(encoder, &texture.create_view(&Default::default()), wgpu::LoadOp::Clear(crate::gpu::color(CLEAR)), None));
    let _ = gpu;
}
fn submit(gpu: &Headless, r: &TileRenderer, slot: usize, encoder: wgpu::CommandEncoder) {
    r.flush_controls(&gpu.queue, slot); gpu.queue.submit([encoder.finish()]); r.controls_submitted();
}
fn no_preparation(label: &str, gpu: &Headless, mut world: TileWorld, atlas: &crate::Atlas,
    view: usize, art: u32, selected: std::ops::Range<usize>) {
    // All glyphs are made coarsely drawable for exactly view 0.
    for s in &mut world.free { if s.text.is_some() { s.view = Some(0); } }
    let r = renderer(gpu, &world, atlas); let target = target(gpu);
    let slot = r.claim_control_frame().unwrap();
    let mut encoder = gpu.device.create_command_encoder(&Default::default()); clear(gpu, &mut encoder, &target);
    stage(gpu, &r, &mut encoder, &target, slot, &camera(), view, art, selected);
    let bytes = r.glyph_cache.borrow().as_ref().map_or(0, State::bytes);
    eprintln!("NO_PREPARATION {label} resident_cache_bytes={bytes} stats={:?}", r.glyph_cache_stats());
    submit(gpu, &r, slot, encoder);
    // Execute every encoded draw under validation, including native smoke.
    let _ = crate::gpu::read_texture(&gpu.device, &gpu.queue, gpu.device.create_command_encoder(&Default::default()), &target);
    assert_eq!(bytes, 0, "{label}: a stage without drawable text must not allocate cache preparation");
}

#[test]
fn no_matching_view_allocates_no_glyph_preparation() {
    for_each_gpu(|gpu| {
        let (world, atlas) = fixture(false);
        no_preparation("other-view", gpu, world, &atlas, 7, 0, 0..usize::MAX);
    });
}
#[test]
fn no_selected_text_step_allocates_no_glyph_preparation() {
    for_each_gpu(|gpu| {
        let (world, atlas) = fixture(false);
        no_preparation("excluded-free-step", gpu, world, &atlas, 0, 0, 0..1);
    });
}
#[test]
fn source_art1_ordinary_path_skips_preparation_and_invisible_text_draws() {
    for_each_gpu(|gpu| {
        let (world, atlas) = fixture(false);
        no_preparation("art1-ordinary", gpu, world, &atlas, 0, 1, 0..usize::MAX);
    });
}
#[test]
fn source_art1_native_smoke_path_skips_preparation_and_invisible_text_draws() {
    for_each_gpu(|gpu| {
        let (world, atlas) = fixture(true);
        no_preparation("art1-native-smoke", gpu, world, &atlas, 0, 1, 0..usize::MAX);
    });
}
#[test]
fn world_text_without_cells_allocates_no_glyph_preparation() {
    for_each_gpu(|gpu| {
        let (mut world, atlas) = fixture(false);
        for s in &mut world.free { if s.text.is_some() { s.view = None; } }
        let r = renderer(gpu, &world, &atlas); let texture = target(gpu);
        let slot = r.claim_control_frame().unwrap(); let mut encoder = gpu.device.create_command_encoder(&Default::default());
        clear(gpu, &mut encoder, &texture);
        stage(gpu, &r, &mut encoder, &texture, slot, &camera(), 0, 0, 0..usize::MAX);
        let bytes = r.glyph_cache.borrow().as_ref().map_or(0, State::bytes);
        eprintln!("NO_PREPARATION all-views-without-cells resident_cache_bytes={bytes}");
        submit(gpu, &r, slot, encoder);
        let _ = crate::gpu::read_texture(&gpu.device, &gpu.queue, gpu.device.create_command_encoder(&Default::default()), &texture);
        assert_eq!(bytes, 0);
    });
}
#[test]
fn warm_skipped_stages_leave_preparations_and_upload_metadata_untouched() {
    for_each_gpu(|gpu| {
        let (world, atlas) = fixture(true); let r = renderer(gpu, &world, &atlas);
        let texture = target(gpu); let slot = r.claim_control_frame().unwrap();
        let mut encoder = gpu.device.create_command_encoder(&Default::default()); clear(gpu, &mut encoder, &texture);
        stage(gpu, &r, &mut encoder, &texture, slot, &camera(), 0, 0, 0..usize::MAX);
        let before = r.glyph_cache_stats();
        let before_used = r.glyph_cache.borrow().as_ref().unwrap().slots[slot].used;
        for (view, art, selected) in [(0, 1, 0..usize::MAX), (7, 0, 0..usize::MAX), (0, 0, 0..1)] {
            stage(gpu, &r, &mut encoder, &texture, slot, &camera(), view, art, selected);
        }
        let after = r.glyph_cache_stats(); let cache = r.glyph_cache.borrow(); let state = cache.as_ref().unwrap();
        let used = state.slots[slot].used;
        let metadata_bytes: usize = state.slots[slot].preparations.iter().take(used).map(|p| p.glyphs.len()*size_of::<CacheGlyph>()).sum();
        eprintln!("WARM_PREPARATION before_used={before_used} after_used={used} metadata_upload_bytes={metadata_bytes} before={before:?} after={after:?}");
        drop(cache); submit(gpu, &r, slot, encoder);
        let _ = crate::gpu::read_texture(&gpu.device, &gpu.queue, gpu.device.create_command_encoder(&Default::default()), &texture);
        assert_eq!(used, before_used); assert_eq!(metadata_bytes, 4*32); assert_eq!(before, after);
    });
}
#[test]
fn eligible_text_keeps_global_indices_and_three_prepared_slots() {
    for_each_gpu(|gpu| {
        for smoke in [false, true] {
           let (world, atlas) = fixture(smoke); let r = renderer(gpu, &world, &atlas);
           let textures: Vec<_> = (0..3).map(|_| target(gpu)).collect();
           let mut encoder = gpu.device.create_command_encoder(&Default::default());
            for (i, zoom) in [(1,1),(5,4),(3,2)].into_iter().enumerate() {
               let slot = r.claim_control_frame().unwrap(); assert_eq!(slot, i);
                clear(gpu, &mut encoder, &textures[i]);
                stage(gpu, &r, &mut encoder, &textures[i], slot, &Camera { zoom, ..camera() }, 0, if i==0 {0} else {2}, 0..usize::MAX);
               let cache = r.glyph_cache.borrow(); let state = cache.as_ref().unwrap();
                assert_eq!(state.slots[slot].used, 1);
               let metadata = &state.slots[slot].preparations[0].glyphs;
                assert_eq!(metadata.len(), 4, "metadata remains globally indexed");
                assert!(metadata[0].atlas_x < 0 && metadata[1].atlas_x < 0);
                assert!(metadata[2].atlas_x >= 0 && metadata[3].atlas_x >= 0);
                assert!(r.world.as_ref().unwrap().free_after.iter().flatten().any(|(range, kind, class)|
                    matches!(kind, FreeKind::Text | FreeKind::TextKeyed) && *class == 0 && range.end == 4));
                eprintln!("GLOBAL_METADATA smoke={smoke} slot={slot} length={} indices2and3=ready", metadata.len());
                drop(cache); r.flush_controls(&gpu.queue, slot);
            }
            assert!(!r.control_frame_available()); assert!(r.claim_control_frame().is_none());
            gpu.queue.submit([encoder.finish()]); r.controls_submitted(); assert!(r.control_frame_available());
            for texture in &textures {
               let got = crate::gpu::read_texture(&gpu.device, &gpu.queue, gpu.device.create_command_encoder(&Default::default()), texture);
                assert!(got.pixels.iter().any(|p| p[2] > 150 && p[0] > 100), "text ink must be visible");
            }
        }
    });
}
