//! Rounded magnifiers, bounded glass refraction and review images.
use tile_render::{AtlasBuilder, Camera, ColorModel, Grid, Headless, LayerSpec, Lens, Layout, RgbaImage, SpriteSets, TileRenderer, TileWorld, reference};

fn fixture(color: ColorModel) -> (tile_render::Atlas, TileWorld) {
    let mut atlas = AtlasBuilder::new(256);
    let mut sets = SpriteSets::default();
    let tiles: Vec<_> = (0..4).map(|kind| {
        let tile = RgbaImage::from_fn(64, 32, |x, y| {
            let dx = (x as i32 - 32).abs();
            let dy = (y as i32 - 16).abs();
            if dx + 2*dy > 32 { return [0; 4]; }
            let grain = ((x*13+y*37) % 13) as u8;
            let base: [u8; 3] = match kind { 0 => [47, 93, 128], 1 => [78, 109, 47], 2 => [133, 130, 67], _ => [63, 97, 43] };
            if kind != 0 && (y as i32 - x as i32 / 2).abs() < 2 { [177, 157, 104, 255] }
            else { [base[0]+grain, base[1]+grain, base[2]+grain, 255] }
        });
        sets.single(atlas.add(&tile, (0, 0)).unwrap())
    }).collect();
    let grid = Grid { cols: 32, rows: 48, layout: Layout::Staggered, step: (32, 16), wrap_x: true, wrap_y: true };
    let mut world = TileWorld::new(grid, sets);
    world.color = color;
    let layer = world.add_layer(LayerSpec::new("terrain", tiles));
    for y in 0..grid.rows { for x in 0..grid.cols { world.set(layer, x, y, if x % 9 < 2 { 0 } else { 1+(x*3+y/3)%3 }); } }
    (atlas.finish(), world)
}

fn cameras(scale: u32, rounded: bool, glass: bool, background: bool) -> Vec<Camera> {
    let mut cameras = Vec::new();
    if background { cameras.push(Camera::new((1024/scale, 640/scale))); }
    cameras.push(Camera { x: 170, y: 180, zoom: (2, 1), origin: (170/scale as i32, 150/scale as i32),
        lens: Some(Lens { radius: if rounded { 25.0/scale as f32 } else { 0.0 }, rim: if glass { 7.0 } else { 2.0 } / scale as f32, glass }),
        ..Camera::new((684/scale, 330/scale)) });
    cameras
}

#[test]
fn rounded_corners_preserve_the_backdrop_and_glass_lights_black() {
    let gpu = Headless::new().expect("Vulkan GPU");
    eprintln!("lens adapter: {:?}", gpu.adapter.get_info());
    for color in [ColorModel::Direct, ColorModel::Quantized { bits: 5 }] {
        let (atlas, world) = fixture(color);
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        renderer.set_world(&gpu.device, &world);
        for scale in [1, 2] { for rounded in [false, true] { for glass in [false, true] { for background in [false, true] {
            let cameras = cameras(scale, rounded, glass, background);
            let size = (1024/scale, 640/scale);
            let expected = reference::render_views(&world, &atlas, &cameras, size, 0, [0, 0, 0, 255]);
            let actual = gpu.render_views(&mut renderer, &cameras, size, 0, [0, 0, 0, 255]);
            let mut differences = 0;
            let mut worst = 0;
            for (g, c) in actual.pixels.iter().zip(&expected.pixels) { for channel in 0..3 {
                worst = worst.max(g[channel].abs_diff(c[channel]));
                if g[channel].abs_diff(c[channel]) > if color == ColorModel::Direct { 1 } else { 9 } { differences += 1; }
            } }
            assert_eq!(differences, 0, "{color:?} scale {scale} rounded {rounded} glass {glass} background {background}, worst {worst}");
            let lens = cameras.last().unwrap();
            let (x, y) = (lens.origin.0 as u32, lens.origin.1 as u32);
            if rounded && !background { assert_eq!(actual.get(x, y), [0, 0, 0, 255]); }
            if glass && !background { assert!(actual.get(x + lens.viewport.0/2, y)[0] > 0); }
            let mut square = cameras.clone(); square.last_mut().unwrap().lens = None;
            let unframed = reference::render_views(&world, &atlas, &square, size, 0, [0, 0, 0, 255]);
            assert_eq!(actual.get(x + lens.viewport.0/2, y + lens.viewport.1/2), unframed.get(x + lens.viewport.0/2, y + lens.viewport.1/2));
            if color == ColorModel::Direct && rounded && glass {
                if let Some(dir) = std::env::var_os("C3R_ZOOM_SHOTS") {
                    std::fs::create_dir_all(&dir).unwrap();
                    let path = std::path::Path::new(&dir).join(format!("lens-{}-scale{scale}.png", if background { "map" } else { "black" }));
                    let image = if scale == 1 { actual.clone() } else { RgbaImage::from_fn(1024, 640, |x, y| actual.get(x/scale, y/scale)) };
                    image::save_buffer(path, bytemuck::cast_slice(&image.pixels), image.width, image.height, image::ColorType::Rgba8).unwrap();
                }
            }
        } } } }
    }
}

#[test]
fn clipped_moving_lenses_keep_their_shape_and_casting_marks() {
    let gpu = Headless::new().expect("Vulkan GPU");
    let (atlas, world) = fixture(ColorModel::Direct);
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let lens = Camera { cells: false, origin: (20, 30), lens: Some(Lens { radius: 12.0, rim: 7.0, glass: true }), ..Camera::new((120, 90)) };
    let first = gpu.render_views(&mut renderer, &[lens], (240, 180), 0, [0, 0, 0, 255]);
    let moved = Camera { origin: (70, 60), ..lens };
    let second = gpu.render_views(&mut renderer, &[moved], (240, 180), 0, [0, 0, 0, 255]);
    for y in 0..90 { for x in 0..120 { assert_eq!(first.get(x+20, y+30), second.get(x+70, y+60)); } }
    for origin in [(-25, -20), (180, 140)] {
        let clipped = Camera { origin, target_clip: Some([5, 8, 235, 175]), ..lens };
        let expected = reference::render_views(&world, &atlas, &[clipped], (240, 180), 0, [0, 0, 0, 255]);
        let actual = gpu.render_views(&mut renderer, &[clipped], (240, 180), 0, [0, 0, 0, 255]);
        for (a, b) in actual.pixels.iter().zip(expected.pixels) { for c in 0..3 { assert!(a[c].abs_diff(b[c]) <= 1); } }
    }
}

/// Reproducible opt-in measurement. Frame timestamps include the backdrop
/// copy; the separate rim timestamp measures the eight bounded edge draws.
#[test]
#[ignore = "GPU cost measurement; run without concurrent GPU tests"]
fn measure_lens_gpu_cost_at_4k() {
    let gpu = Headless::new().expect("Vulkan GPU");
    eprintln!("lens benchmark adapter: {:?}", gpu.adapter.get_info());
    let (atlas, world) = fixture(ColorModel::Quantized { bits: 5 });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    assert!(renderer.enable_timing(&gpu.device, &gpu.queue));
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("4K lens timing"), size: wgpu::Extent3d { width: 3840, height: 2160, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
    });
    for scale in [1, 2] {
        for glass in [false, true] {
            let c = Camera { origin: (768/scale, 216/scale), zoom: (2, 1), x: 170, y: 180,
                lens: glass.then_some(Lens { radius: 32.0/scale as f32, rim: 7.0/scale as f32, glass: true }),
                ..Camera::new((2304/scale as u32, 432/scale as u32)) };
            let cams = [Camera::new((3840/scale as u32, 2160/scale as u32)), c];
            let present = tile_render::Present { canvas: (3840/scale as u32, 2160/scale as u32), target: (3840, 2160), zoom: (scale as u32, 1), at: (0, 0) };
            let (mut spans, mut rims) = (Vec::new(), Vec::new());
            for frame in 0..44 {
                let mut encoder = gpu.device.create_command_encoder(&Default::default());
                renderer.render_presented(&gpu.device, &gpu.queue, &mut encoder, &target, &cams, &[], frame, [0,0,0,255], present);
                renderer.timer().unwrap().finish(&mut encoder);
                gpu.queue.submit([encoder.finish()]);
                renderer.controls_submitted();
                renderer.timer().unwrap().submitted();
                gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                for times in renderer.timer().unwrap().collect(&gpu.device) {
                    if frame >= 4 {
                        spans.push(times.span);
                        rims.extend(times.passes.iter().filter(|(label, _)| label.ends_with("lens rim")).map(|(_, ms)| *ms));
                    }
                }
            }
            spans.sort_by(f64::total_cmp); rims.sort_by(f64::total_cmp);
            eprintln!("4K scale {scale}, glass {glass}: frame median {:.4} ms, rim median {:.4} ms, {} samples", spans[spans.len()/2], if rims.is_empty() {0.0} else {rims[rims.len()/2]}, spans.len());
        }
    }
}
