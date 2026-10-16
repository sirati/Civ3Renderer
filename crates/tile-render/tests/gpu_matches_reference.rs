//! The GPU renderer must produce exactly the reference renderer's pixels.

use tile_render::{
    Atlas, AtlasBuilder, Camera, Grid, Headless, LayerSpec, Layout, RgbaImage, SpriteSets, TileRenderer, TileWorld,
    reference,
};

const CLEAR: [u8; 4] = [10, 20, 30, 255];

/// A diamond tile with a 1-pixel border, coloured by `seed`.
fn diamond(seed: u8) -> RgbaImage {
    RgbaImage::from_fn(128, 64, |x, y| {
        let dx = (x as i32 * 2 - 127).abs() as f32 / 128.0;
        let dy = (y as i32 * 2 - 63).abs() as f32 / 64.0;
        let d = dx + dy;
        if d > 1.0 {
            [0, 0, 0, 0]
        } else if d > 0.95 {
            [0, 0, 0, 255]
        } else {
            [seed.wrapping_mul(37), (x as u8).wrapping_add(seed), (y as u8).wrapping_mul(3), 255]
        }
    })
}

/// A sprite taller than a cell (88 px, like Civ III hills): overlaps the row behind.
fn tall(seed: u8) -> RgbaImage {
    RgbaImage::from_fn(60, 88, |x, y| {
        if (x + y) % 7 == 0 { [0, 0, 0, 0] } else { [200, seed.wrapping_add(x as u8), y as u8 * 2, 255] }
    })
}

fn scene(grid: Grid) -> (TileWorld, Atlas) {
    let mut atlas = AtlasBuilder::new(512);
    let mut sets = SpriteSets::default();
    let terrain: Vec<_> = (0..5u8).map(|i| sets.single(atlas.add(&diamond(i * 40), (0, 0)).unwrap())).collect();
    let anim_frames: Vec<_> = (0..4u8).map(|i| atlas.add(&tall(i * 60), (34, -40)).unwrap()).collect();
    let anim = sets.add(&anim_frames);

    let mut world = TileWorld::new(grid, sets);
    let base = world.add_layer(LayerSpec::new("terrain", terrain));
    let mut overlay_spec = LayerSpec::new("overlay", vec![tile_render::SpriteSetId::EMPTY, anim]);
    overlay_spec.per_cell_phase = true;
    overlay_spec.ticks_per_frame = 3;
    let overlay = world.add_layer(overlay_spec);
    // Enough distinct values to need 2-byte cells; all map to one set.
    let wide = world.add_layer(LayerSpec::new("wide", {
        let mut lut = vec![tile_render::SpriteSetId::EMPTY; 300];
        lut[299] = anim;
        lut
    }));
    for r in 0..grid.rows {
        for c in 0..grid.cols {
            world.set(base, c, r, (c * 3 + r * 7) % 5);
            if (c + 2 * r) % 3 == 0 {
                world.set(overlay, c, r, 1);
                world.set_phase(overlay, c, r, ((c * r) % 4) as u8);
            }
            if (c * r) % 11 == 5 {
                world.set(wide, c, r, 299);
            }
        }
    }
    (world, atlas.finish())
}

fn assert_same(gpu: &RgbaImage, cpu: &RgbaImage, what: &str) {
    assert_eq!((gpu.width, gpu.height), (cpu.width, cpu.height));
    let diff: Vec<_> = gpu.pixels.iter().zip(&cpu.pixels).enumerate().filter(|(_, (a, b))| a != b).collect();
    if let Some((i, (g, c))) = diff.first() {
        let (x, y) = (*i as u32 % gpu.width, *i as u32 / gpu.width);
        panic!("{what}: {} pixels differ, first at ({x}, {y}): gpu {g:?} cpu {c:?}", diff.len());
    }
}

#[test]
fn staggered_wrapping_world_is_pixel_exact() {
    let gpu = Headless::new().expect("GPU");
    eprintln!("adapter: {:?}", gpu.adapter.get_info());
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let (world, atlas) = scene(grid);
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);

    // Cameras: origin, inside, across the horizontal wrap, past the top
    // and bottom edges, odd offsets.
    for (i, (x, y)) in [(0, 0), (333, 101), (-517, 40), (1400, -50), (77, 400)].into_iter().enumerate() {
        let mut camera = Camera::new((640, 360));
        camera.x = x;
        camera.y = y;
        camera.overhang = (0, 40, 0, 0);
        for frame in [0, 5, 13] {
            let got = gpu.render(&renderer, &camera, frame, CLEAR);
            let want = reference::render(&world, &atlas, &camera, frame, CLEAR);
            assert_same(&got, &want, &format!("camera {i} ({x}, {y}) frame {frame}"));
        }
    }
}

#[test]
fn orthogonal_world_is_pixel_exact() {
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 9, rows: 7, layout: Layout::Orthogonal, wrap_x: false, wrap_y: true, step: (128, 64) };
    let (world, atlas) = scene(grid);
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let mut camera = Camera::new((500, 300));
    camera.x = -60;
    camera.y = 250;
    camera.overhang = (0, 40, 0, 0);
    let got = gpu.render(&renderer, &camera, 2, CLEAR);
    let want = reference::render(&world, &atlas, &camera, 2, CLEAR);
    assert_same(&got, &want, "orthogonal");
    // Something was actually drawn.
    assert!(got.pixels.iter().any(|p| *p != CLEAR));
}
