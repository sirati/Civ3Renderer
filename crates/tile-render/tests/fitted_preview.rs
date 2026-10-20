//! A world fitted small into a pane (a scenario preview beside a menu) is
//! shown whole, at any pixel scale: its picture at zoom 1 can be taller than
//! a picture may be (`MAX_SOURCE`), and then it was cut there, its last row
//! repeated down the pane.

use tile_render::{AtlasBuilder, Camera, ColorModel, Grid, Headless, LayerSpec, Layout, RgbaImage, SpriteSets, TileRenderer, TileWorld, reference};

const CLEAR: [u8; 4] = [10, 20, 30, 255];

/// A diamond tile with a 1-pixel border, coloured by `seed`.
fn diamond(seed: u8) -> RgbaImage {
    RgbaImage::from_fn(128, 64, |x, y| {
        let dx = (x as i32 * 2 - 127).abs() as f32 / 128.0;
        let dy = (y as i32 * 2 - 63).abs() as f32 / 64.0;
        let d = dx + dy;
        if d > 1.0 {
            [0, 0, 0, 0]
        } else if d > 0.9 {
            [0, 0, 0, 255]
        } else {
            [seed.wrapping_mul(37), (x as u8).wrapping_add(seed), (y as u8).wrapping_mul(3), 255]
        }
    })
}

/// A tall non-wrapping world whose rows differ all the way down (a smear
/// repeating one row shows).
fn tall_world() -> (TileWorld, tile_render::Atlas) {
    let grid = Grid { cols: 19, rows: 300, layout: Layout::Staggered, wrap_x: false, wrap_y: false, step: (64, 32) };
    let mut atlas = AtlasBuilder::new(512);
    let mut sets = SpriteSets::default();
    let terrain: Vec<_> = (0..7u8).map(|i| sets.single(atlas.add(&diamond(i * 35 + 3), (0, 0)).unwrap())).collect();
    let mut world = TileWorld::new(grid, sets);
    let base = world.add_layer(LayerSpec::new("terrain", terrain));
    for r in 0..grid.rows {
        for c in 0..grid.cols {
            world.set(base, c, r, (c + r / 3 + (r * r) % 5) % 7);
        }
    }
    world.color = ColorModel::Quantized { bits: 5 };
    (world, atlas.finish())
}

/// What `camera` (zoomed out, its world larger than a picture may be)
/// shows: its world drawn at zoom 1 whole (by the reference, which has no
/// size limit), each pixel the zoom-1 pixel under it.
fn expected(world: &TileWorld, atlas: &tile_render::Atlas, camera: &Camera) -> RgbaImage {
    let (w, h) = camera.world_size();
    let flat = Camera { zoom: (1, 1), origin: (0, 0), viewport: (w, h), target_clip: None, ..*camera };
    let picture = reference::render(world, atlas, &flat, 0, CLEAR);
    let (n, d) = (camera.zoom.0 as u64, camera.zoom.1 as u64);
    RgbaImage::from_fn(camera.viewport.0, camera.viewport.1, |x, y| picture.get((x as u64 * d / n) as u32, (y as u64 * d / n) as u32))
}

fn assert_same(got: &RgbaImage, want: &RgbaImage, what: &str) {
    assert_eq!((got.width, got.height), (want.width, want.height), "{what}");
    let diff: Vec<_> = (0..got.height).flat_map(|y| (0..got.width).map(move |x| (x, y))).filter(|&(x, y)| got.get(x, y) != want.get(x, y)).collect();
    if let Some(&(x, y)) = diff.first() {
        let rows: std::collections::BTreeSet<u32> = diff.iter().map(|p| p.1).collect();
        panic!("{what}: {} pixels differ (rows {:?}..={:?}), first at ({x}, {y}): got {:?} want {:?}", diff.len(), rows.first(), rows.last(), got.get(x, y), want.get(x, y));
    }
}

#[test]
fn a_fitted_preview_taller_than_a_picture_is_shown_whole_at_pixel_scale_two() {
    let gpu = Headless::new().expect("GPU");
    let (world, atlas) = tall_world();
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    // The world shown half size (the bridge's downscale), fitted into a
    // narrow pane beside a menu at 1/12: its zoom-1 picture is 1200×4800,
    // more than `MAX_SOURCE` tall. At pixel scale 2 the frame is composed
    // at the window's resolution (`Camera::boxed`): the pane twice as large
    // at 1/6, the same world.
    let fitted = Camera { x: 0, y: 0, zoom: (1, 12), origin: (60, 0), downscale: 2, ..Camera::new((100, 400)) };
    assert!(fitted.world_size().1 > 4096, "{:?}", fitted.world_size());
    // A zoomed-in inset beside it, drawn from a picture of its own (laid
    // out after the preview's).
    let inset = Camera { x: 300, y: 200, zoom: (2, 1), origin: (0, 0), downscale: 2, ..Camera::new((60, 120)) };
    for k in [1, 2] {
        let (fitted, inset) = (fitted.boxed(k), inset.boxed(k));
        let size = (160 * k, 400 * k);
        let got = gpu.render_views(&mut renderer, &[fitted, inset], size, 0, CLEAR);
        let pane = RgbaImage::from_fn(fitted.viewport.0, fitted.viewport.1, |x, y| got.get(fitted.origin.0 as u32 + x, fitted.origin.1 as u32 + y));
        let want = expected(&world, &atlas, &fitted);
        let repeated = gpu.render_views(&mut renderer, &[fitted, inset], size, 0, CLEAR);
        assert!(renderer.reused(), "a static fitted preview reuses its composed frame");
        assert_same(&repeated, &got, "retained fitted preview");
        // (The world fills the pane: a cut would show.)
        let rows: std::collections::BTreeSet<Vec<[u8; 4]>> = (0..want.height).map(|y| (0..want.width).map(|x| want.get(x, y)).collect()).collect();
        assert!(rows.len() > want.height as usize / 2, "{} distinct rows", rows.len());
        assert_same(&pane, &want, &format!("fitted preview at pixel scale {k}"));
        let all = reference::render_views(&world, &atlas, &[fitted, inset], size, 0, CLEAR);
        assert_same(&got, &all, &format!("fitted preview and inset at pixel scale {k}, against the reference"));
        let alone = gpu.render_views(&mut renderer, &[inset], size, 0, CLEAR);
        let inset_of = |img: &RgbaImage| RgbaImage::from_fn(inset.viewport.0, inset.viewport.1, |x, y| img.get(x, y));
        assert_same(&inset_of(&got), &inset_of(&alone), &format!("inset beside the preview at pixel scale {k}"));
    }
}

#[test]
fn a_view_zoomed_out_a_little_past_the_picture_limit_is_shown_whole() {
    // At 4/5 a 5000-pixel-tall view shows 6250 world rows: more than a
    // picture holds, though zoomed out less than half.
    let gpu = Headless::new().expect("GPU");
    let (world, atlas) = tall_world();
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let camera = Camera { x: 40, y: 100, zoom: (4, 5), origin: (6, 0), ..Camera::new((30, 5000)) };
    assert!(camera.world_size().1 > 4096 && camera.drawn_at_zoom());
    let got = gpu.render_views(&mut renderer, &[camera], (36, 5000), 0, CLEAR);
    let pane = RgbaImage::from_fn(30, 5000, |x, y| got.get(6 + x, y));
    assert_same(&pane, &expected(&world, &atlas, &camera), "a tall view at 4/5");
}

#[test]
fn a_clipped_preview_preserves_its_fractional_zoom_phase() {
    let gpu = Headless::new().expect("GPU");
    let (world, atlas) = tall_world();
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    for zoom in [(1, 12), (2, 7)] {
        let camera = Camera {
            x: -13, y: 127, zoom, origin: (-7, -11), downscale: 2,
            target_clip: Some([9, 17, 91, 383]),
            ..Camera::new((110, 420))
        };
        let want = expected(&world, &atlas, &camera);
        let got = gpu.render_views(&mut renderer, &[camera], (100, 400), 0, CLEAR);
        for y in 0..400 {
            for x in 0..100 {
                let pixel = if (9..91).contains(&x) && (17..383).contains(&y) {
                    want.get(x + 7, y + 11)
                } else {
                    let q = |c: u8| tile_render_shared::expand(tile_render_shared::quantize(c.into(), 5), 5) as u8;
                    [q(CLEAR[0]), q(CLEAR[1]), q(CLEAR[2]), CLEAR[3]]
                };
                assert_eq!(got.get(x, y), pixel, "zoom {zoom:?} at ({x}, {y})");
            }
        }
    }
}

#[test]
fn direct_scaled_quads_keep_their_full_extent() {
    let gpu = Headless::new().expect("GPU");
    let image = RgbaImage::from_fn(16, 8, |x, y| [x as u8 * 10, y as u8 * 25, 71, 255]);
    let mut builder = AtlasBuilder::new(64);
    let sprite = builder.add(&image, (0, 0)).unwrap();
    let atlas = builder.finish();
    let mut sets = SpriteSets::default();
    let set = sets.single(sprite);
    let grid = Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, step: (1, 1), wrap_x: false, wrap_y: false };
    let mut world = TileWorld::new(grid, sets);
    world.free.push(tile_render::PlacedSprite { screen: true, view: Some(0), ..tile_render::PlacedSprite::new(set, 0, 0) });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    for position in [(0, 0), (-3, -2)] {
        (world.free[0].x, world.free[0].y) = position;
        for clip in [None, Some([-2, -1, 13, 7]), Some([1, -1, 15, 6])] {
            world.free[0].clip = clip;
            renderer.set_world(&gpu.device, &world);
            for (n, d) in [(2, 1), (1, 2), (2, 3), (3, 7)] {
                let camera = Camera { cells: false, hairline: true, zoom: (n, d), ..Camera::new((32, 16)) };
                let got = gpu.render(&mut renderer, &camera, 0, CLEAR);
                // Independent of placements and the reference renderer's own
                // quad bounds: each pixel samples its nominal source texel.
                for y in 0..16 { for x in 0..32 {
                    let (sx, sy) = ((x * d / n) as i32 - position.0, (y * d / n) as i32 - position.1);
                    let (qx, qy) = ((x * d / n) as i32, (y * d / n) as i32);
                    let inside_clip = clip.is_none_or(|[l, t, r, b]| qx >= l && qx < r && qy >= t && qy < b);
                    let want = if inside_clip && (0..16).contains(&sx) && (0..8).contains(&sy) { image.get(sx as u32, sy as u32) } else { CLEAR };
                    assert_eq!(got.get(x, y), want, "position {position:?}, clip {clip:?}, zoom {n}/{d}, pixel ({x}, {y})");
                } }
                assert_same(&got, &reference::render(&world, &atlas, &camera, 0, CLEAR), "direct scaled quad against the CPU reference");
            }
        }
    }
}
