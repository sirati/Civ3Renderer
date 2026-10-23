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

/// Straight survey lines make bevel compression and corner bending visible.
fn survey_grid() -> (tile_render::Atlas, TileWorld) {
    let mut builder = AtlasBuilder::new(256);
    let tile = RgbaImage::from_fn(128,128, |x,y| {
        if (2*y as i32 - x as i32 - 32).rem_euclid(128) <= 1 { [215,225,215,255] }
        else { [38,67,49,255] }
    });
    let mut sets = SpriteSets::default();
    let tile = sets.single(builder.add(&tile,(0,0)).unwrap());
    let grid = Grid { cols: 32, rows: 24, layout: Layout::Orthogonal, step: (128,128), wrap_x: true, wrap_y: true };
    let mut world = TileWorld::new(grid, sets);
    let layer = world.add_layer(LayerSpec::new("survey grid", vec![tile]));
    for y in 0..grid.rows { for x in 0..grid.cols { world.set(layer,x,y,0); } }
    (builder.finish(),world)
}

fn cameras(scale: u32, rounded: bool, glass: bool, background: bool) -> Vec<Camera> {
    let mut cameras = Vec::new();
    if background { cameras.push(Camera::new((1024/scale, 640/scale))); }
    cameras.push(Camera { x: 170, y: 180, zoom: (2, 1), origin: (170/scale as i32, 150/scale as i32),
        lens: Some(Lens { radius: if rounded { 25.0/scale as f32 } else { 0.0 }, rim: if glass { 14.0 } else { 2.0 } / scale as f32, glass }),
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
    let lens = Camera { cells: false, origin: (20, 30), lens: Some(Lens { radius: 24.0, rim: 14.0, glass: true }), ..Camera::new((120, 90)) };
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
                lens: glass.then_some(Lens { radius: 56.0/scale as f32, rim: 24.0/scale as f32, glass: true }),
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
                        rims.push(times.passes.iter().filter(|(label, _)| label.ends_with("lens rim")).map(|(_, ms)| *ms).sum::<f64>());
                    }
                }
            }
            spans.sort_by(f64::total_cmp); rims.sort_by(f64::total_cmp);
            eprintln!("4K scale {scale}, glass {glass}: frame median {:.4} ms, rim median {:.4} ms, {} samples", spans[spans.len()/2], if rims.is_empty() {0.0} else {rims[rims.len()/2]}, spans.len());
        }
    }
}

/// Track a whole line across the bevel at both presentation scales. The
/// face and rim must meet continuously, then gain tens of pixels of bend.
#[test]
fn straight_lines_bend_continuously_across_the_bevel_at_both_scales() {
    let gpu = Headless::new().expect("Vulkan GPU");
    let (atlas, world) = survey_grid();
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    for scale in [1, 2] { for menu in [false, true] {
        let lens = Camera { cells: false, origin: (40/scale,40/scale),
            lens: Some(Lens { radius: 56.0/scale as f32, rim: 24.0/scale as f32, glass: true }),
            ..Camera::new((600/scale as u32,200/scale as u32)) };
        let present = tile_render::Present { canvas: (700/scale as u32,300/scale as u32), target: (700,300), zoom: (scale as u32,1), at: (0,0) };
        let base = Camera::new(present.canvas);
        let image = if menu {
            let copy = tile_render::ScreenCopy { source: [100/scale,40/scale,300/scale,100/scale], at: lens.origin, zoom: (2,1), lens: lens.lens };
            gpu.render_presented(&mut renderer, &[base], &[copy], present, 0, [0,0,0,255])
        } else { gpu.render_presented(&mut renderer, &[base,lens], &[], present, 0, [0,0,0,255]) };
        let intercept = if menu { -80 + 192*scale } else { 96*scale };
        let displacement = |y| {
            let baseline = (2*y as i32 + intercept) as u32;
            let peak = (baseline-4..baseline+124).max_by_key(|&x| image.get(x,y)[0]).unwrap();
            peak as i32 - baseline as i32
        };
        let shifts: Vec<_> = [63,60,57,54,51,48,45,42,41].into_iter().map(displacement).collect();
        eprintln!("scale {scale}, menu {menu}, line displacement across 24px bevel: {shifts:?}");
        assert!(shifts[0].abs() <= 3, "scale {scale}, menu {menu}, clear face continuity: {shifts:?}");
        assert!(shifts[4] >= 12 && shifts[8] >= 85, "scale {scale}, menu {menu}, broad visible refraction: {shifts:?}");
        for pair in shifts.windows(2) { assert!(pair[1] >= pair[0], "scale {scale}, menu {menu}, continuously increasing bend: {shifts:?}"); }
        let baseline = (82 + intercept) as u32;
        let peak = |c| (baseline+70..baseline+124).max_by_key(|&x| image.get(x,41)[c]).unwrap();
        let fringe = peak(0).abs_diff(peak(2));
        assert!((2..=8).contains(&fringe), "scale {scale}, menu {menu}, resolved red/blue fringe: {fringe}px");
    } }
}

/// Empty black glass isolates lighting and physical silhouette coverage from
/// the pixelated art. Scale 2 must not turn the corner into 2x2 stair steps.
#[test]
fn presented_glass_keeps_physical_width_and_smooth_corners() {
    let gpu = Headless::new().expect("Vulkan GPU");
    let (atlas, world) = fixture(ColorModel::Direct);
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let render = |renderer: &mut TileRenderer, scale: u32| {
        let c = Camera { cells: false, origin: (40/scale as i32,40/scale as i32),
            lens: Some(Lens { radius: 56.0/scale as f32, rim: 24.0/scale as f32, glass: true }),
            ..Camera::new((600/scale,200/scale)) };
        gpu.render_presented(renderer, &[c], &[], tile_render::Present {
            canvas: (700/scale,300/scale), target: (700,300), zoom: (scale,1), at: (0,0) }, 0, [0,0,0,255])
    };
    let full = render(&mut renderer, 1);
    let boxed = render(&mut renderer, 2);
    for (i,(a,b)) in full.pixels.iter().zip(&boxed.pixels).enumerate() { for c in 0..3 {
        assert!(a[c].abs_diff(b[c]) <= 1, "window-pixel optics at {}/{}: {a:?} vs {b:?}", i%700, i/700);
    } }
    assert!((40..64).map(|y| boxed.get(300,y)[2]).max().unwrap() >= 100, "visible unseen-light streak on black");
    assert_eq!(boxed.get(300,64), [0,0,0,255], "the face stays clear");
    assert_ne!(boxed.get(56,56), boxed.get(57,56), "the corner is shaded per physical pixel");
}

/// A floating world lens sits below driver UI. Final-resolution shading
/// must preserve both opaque/translucent later pixels and cached replays.
#[test]
fn window_glass_preserves_later_interface_views_and_cached_frames() {
    let gpu = Headless::new().expect("Vulkan GPU");
    for color in [ColorModel::Direct, ColorModel::Quantized { bits: 5 }] {
        let (atlas, world) = fixture(color);
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        renderer.set_world(&gpu.device, &world);
        let lens = Camera { cells: false, origin: (20,20),
            lens: Some(Lens { radius: 28.0, rim: 12.0, glass: true }), ..Camera::new((300,100)) };
        let ui = Camera { origin: (60,20), ..Camera::new((100,24)) };
        let before = gpu.render_views(&mut renderer, &[lens], (350,150), 0, [0,0,0,255]);
        let after = gpu.render_views(&mut renderer, &[lens,ui], (350,150), 0, [0,0,0,255]);
        let present = tile_render::Present { canvas: (350,150), target: (714,318), zoom: (2,1), at: (7,9) };
        let actual = gpu.render_presented(&mut renderer, &[lens,ui], &[], present, 0, [0,0,0,255]);
        let cached = gpu.render_presented(&mut renderer, &[lens,ui], &[], present, 0, [0,0,0,255]);
        assert_eq!(actual, cached, "cached lens uniforms and snapshots");
        let mut covered = 0;
        for y in 20..32 { for x in 60..160 {
            if before.get(x,y) == after.get(x,y) { continue; }
            covered += 1;
            for dy in 0..2 { for dx in 0..2 {
                let pixel = actual.get(7+x*2+dx,9+y*2+dy);
                for c in 0..3 { assert!(pixel[c].abs_diff(after.get(x,y)[c]) <= 1, "{color:?}: later UI at {x}/{y}: {pixel:?} vs {:?}", after.get(x,y)); }
            } }
        } }
        assert!(covered > 200, "UI must actually cover the top bevel");
    }
}

/// Check the black-background requirement independently of the CPU oracle:
/// luminous angular reflections, a clear face and no repeated edge marks.
#[test]
fn black_glass_has_a_curved_luminous_rim_without_periodic_ticks() {
    let gpu = Headless::new().expect("Vulkan GPU");
    let (atlas, world) = fixture(ColorModel::Direct);
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let c = Camera { cells: false, origin: (40, 40), lens: Some(Lens { radius: 30.0, rim: 14.0, glass: true }), ..Camera::new((600, 200)) };
    let image = gpu.render_views(&mut renderer, &[c], (700, 300), 0, [0,0,0,255]);
    assert_eq!(image.get(340, 140), [0,0,0,255], "clear, untinted face");
    assert!(image.get(340, 39)[2] > 0, "scattered light outside the silhouette");
    assert_eq!(image.get(340, 35), [0,0,0,255], "halo is bounded");
    let top: Vec<_> = (40..54).map(|y| image.get(300,y)[2]).collect();
    assert!(top.iter().max().unwrap() - top.iter().min().unwrap() > 15, "curved light profile: {top:?}");
    assert!(image.get(300,40)[2] > image.get(40,140)[2], "unseen light varies with rim angle");
    // Every top pixel has the same profile apart from the one short scratch.
    let mut runs = 0;
    let mut flaw = false;
    for x in 80..600 {
        let differs = (40..54).any(|y| image.get(x,y) != image.get(100,y));
        if differs && !flaw { runs += 1; }
        flaw = differs;
    }
    assert_eq!(runs, 1, "exactly one local scratch, no repeated ticks");
}

#[test]
fn screen_copy_lenses_preserve_corners_and_clear_menu_content() {
    use tile_render::{Present, ScreenCopy};
    let gpu = Headless::new().expect("Vulkan GPU");
    for color in [ColorModel::Direct, ColorModel::Quantized { bits: 5 }] {
        let (atlas, world) = fixture(color);
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        renderer.set_world(&gpu.device, &world);
        for scale in [1, 2] { for glass in [false, true] {
            let size = (800/scale, 600/scale);
            let cameras = [Camera::new(size)];
            let copy = ScreenCopy { source: [40/scale as i32, 100/scale as i32, 160/scale as i32, 120/scale as i32],
                at: (350/scale as i32, 200/scale as i32), zoom: (2,1),
                lens: Some(Lens { radius: 56.0/scale as f32, rim: 24.0/scale as f32, glass }) };
            let actual = gpu.render_composed(&mut renderer, &cameras, &[copy], size, 0, [0,0,0,255]);
            let expected = reference::render_composed(&world, &atlas, &cameras, &[copy], size, 0, [0,0,0,255]);
            for (a,b) in actual.pixels.iter().zip(&expected.pixels) { for ch in 0..3 {
                assert!(a[ch].abs_diff(b[ch]) <= 1, "menu lens {color:?}, scale {scale}, glass {glass}: {a:?} vs {b:?}");
            } }
            let plain = gpu.render_composed(&mut renderer, &cameras, &[ScreenCopy { lens: None, ..copy }], size, 0, [0,0,0,255]);
            assert_eq!(actual.get(500/scale,300/scale), plain.get(500/scale,300/scale));
            let base = gpu.render_views(&mut renderer, &cameras, size, 0, [0,0,0,255]);
            assert_eq!(actual.get(350/scale,200/scale), base.get(350/scale,200/scale));
            // The final-window pass keeps the physical lens width when pixels are boxed.
            let present = Present { canvas: size, target: (800,600), zoom: (scale,1), at: (0,0) };
            let presented = gpu.render_presented(&mut renderer, &cameras, &[copy], present, 0, [0,0,0,255]);
            assert_eq!(presented.get(500,300), plain.get(500/scale,300/scale));
            let boxed = gpu.render_presented(&mut renderer, &[cameras[0].boxed(scale)], &[copy.boxed(scale)], Present::unscaled((800,600)), 0, [0,0,0,255]);
            // Background art is sampled differently, but the silhouette is identical.
            assert_eq!(presented.get(350,200), boxed.get(350,200));
        } }
    }
}

#[test]
#[ignore = "4K review captures; set C3R_ZOOM_SHOTS"]
fn capture_glass_review_at_4k() {
    let dir = std::path::PathBuf::from(std::env::var_os("C3R_ZOOM_SHOTS").expect("C3R_ZOOM_SHOTS"));
    std::fs::create_dir_all(&dir).unwrap();
    let gpu = Headless::new().expect("Vulkan GPU");
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let save = |name: String, image: &RgbaImage| image::save_buffer(dir.join(name), bytemuck::cast_slice(&image.pixels), image.width, image.height, image::ColorType::Rgba8).unwrap();
    for grid in [false, true] {
      let (atlas, world) = if grid { survey_grid() } else { fixture(ColorModel::Direct) };
      renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
      renderer.set_world(&gpu.device, &world);
      for scale in [1, 2] { for background in [false, true] {
        if grid && !background { continue; }
        let mut cams = Vec::new();
        if background { cams.push(Camera::new((3840/scale,2160/scale))); }
        cams.push(Camera { x: if grid { 200 } else { 170 }, y: 180, zoom: (2,1), origin: (768/scale as i32,216/scale as i32),
            lens: Some(Lens { radius: 56.0/scale as f32, rim: 24.0/scale as f32, glass: true }), ..Camera::new((2304/scale,432/scale)) });
        let image = gpu.render_presented(&mut renderer, &cams, &[], tile_render::Present { canvas: (3840/scale,2160/scale), target: (3840,2160), zoom: (scale,1), at: (0,0) }, 0, [0,0,0,255]);
        let background = if grid { "grid" } else if background { "map" } else { "black" };
        save(format!("glass3-{background}-4k-scale{scale}.png"), &image);
        let crop = RgbaImage::from_fn(420,330, |x,y| image.get(752+x/3,200+y/3));
        save(format!("glass3-{background}-corner-3x-scale{scale}.png"), &crop);
        if !grid && background == "black" {
            let mut empty = *cams.last().unwrap(); empty.cells = false;
            let image = gpu.render_presented(&mut renderer, &[empty], &[], tile_render::Present { canvas: (3840/scale,2160/scale), target: (3840,2160), zoom: (scale,1), at: (0,0) }, 0, [0,0,0,255]);
            save(format!("glass3-empty-black-4k-scale{scale}.png"), &image);
            let crop = RgbaImage::from_fn(420,330, |x,y| image.get(752+x/3,200+y/3));
            save(format!("glass3-empty-black-corner-3x-scale{scale}.png"), &crop);
        }
      } }
    }
}
