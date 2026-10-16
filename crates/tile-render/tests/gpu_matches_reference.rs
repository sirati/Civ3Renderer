//! The GPU renderer must produce exactly the reference renderer's pixels.

use tile_render::{
    Atlas, AtlasBuilder, Blend, Camera, ColorModel, Grid, PlacedSprite, Headless, LayerSpec, Layout, RgbaImage, SpriteSets, TileRenderer, TileWorld,
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

/// A diamond that darkens what is below, in 16 steps by position (the red
/// channel is the scale numerator, as for `Blend::Scale`).
fn shade() -> RgbaImage {
    RgbaImage::from_fn(128, 64, |x, y| {
        let dx = (x as i32 * 2 - 127).abs() as f32 / 128.0;
        let dy = (y as i32 * 2 - 63).abs() as f32 / 64.0;
        if dx + dy > 1.0 { [0, 0, 0, 0] } else { [((x + y) % 16) as u8, 0, 0, 255] }
    })
}

fn scene(grid: Grid) -> (TileWorld, Atlas) {
    scene_with(grid, false)
}

/// `shading`: add darkening layers, one in the middle of pass 1 and one
/// in a pass of its own.
fn scene_with(grid: Grid, shading: bool) -> (TileWorld, Atlas) {
    let mut atlas = AtlasBuilder::new(512);
    let mut sets = SpriteSets::default();
    let shade_set = sets.single(atlas.add(&shade(), (0, 0)).unwrap());
    let shadow_set = sets.single(atlas.add(&shade(), (20, -30)).unwrap());
    let terrain: Vec<_> = (0..5u8).map(|i| sets.single(atlas.add(&diamond(i * 40), (0, 0)).unwrap())).collect();
    let anim_frames: Vec<_> = (0..4u8).map(|i| atlas.add(&tall(i * 60), (34, -40)).unwrap()).collect();
    let anim = sets.add(&anim_frames);

    let mut world = TileWorld::new(grid, sets);
    let base = world.add_layer(LayerSpec::new("terrain", terrain.clone()));
    let mut overlay_spec = LayerSpec::new("overlay", vec![tile_render::SpriteSetId::EMPTY, anim]);
    overlay_spec.per_cell_phase = true;
    overlay_spec.ticks_per_frame = 3;
    let overlay = world.add_layer(overlay_spec);
    // Enough distinct values to need 2-byte cells; all map to one set.
    // A second render pass: drawn after the whole first pass, so its tall
    // sprites are not covered by the terrain of later rows.
    let wide = world.add_layer(LayerSpec {
        pass: 1,
        ..LayerSpec::new("wide", {
            let mut lut = vec![tile_render::SpriteSetId::EMPTY; 300];
            lut[299] = anim;
            lut
        })
    });
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
    if shading {
        // Free sprites: an animated one in the world after pass 0, a
        // darkening one, and a screen-space one on top.
        world.free.push(PlacedSprite { after_pass: 0, ticks_per_frame: 2, phase: 1, ..PlacedSprite::new(anim, 200, 90) });
        world.free.push(PlacedSprite { after_pass: 1, blend: Blend::Scale { denominator: 16 }, ..PlacedSprite::new(shade_set, 150, 70) });
        world.free.push(PlacedSprite { screen: true, ..PlacedSprite::new(terrain[2], 30, 20) });
        world.free.push(PlacedSprite { screen: true, ..PlacedSprite::new(anim, -10, 300) });
        world.free.push(PlacedSprite { screen: true, clip: Some([40, 25, 90, 60]), ..PlacedSprite::new(terrain[1], 30, 20) });
        world.free.push(PlacedSprite { clip: Some([100, 0, 130, 400]), ..PlacedSprite::new(anim, 180, 120) });
        world.free.push(PlacedSprite { screen: true, clip: Some([5, 5, 5, 50]), ..PlacedSprite::new(terrain[0], 0, 0) });
        let shadow = world.add_layer(LayerSpec {
            pass: 1,
            blend: Blend::Scale { denominator: 16 },
            ..LayerSpec::new("shadow", vec![tile_render::SpriteSetId::EMPTY, shadow_set])
        });
        let after = world.add_layer(LayerSpec { pass: 1, ..LayerSpec::new("after", vec![tile_render::SpriteSetId::EMPTY, anim]) });
        let shaded = world.add_layer(LayerSpec {
            pass: 2,
            blend: Blend::Scale { denominator: 16 },
            ..LayerSpec::new("shade", vec![tile_render::SpriteSetId::EMPTY, shade_set])
        });
        // Overlaps the previous layer's sprites: darkening twice must round
        // down after each blit.
        let shaded_again = world.add_layer(LayerSpec {
            pass: 2,
            blend: Blend::Scale { denominator: 16 },
            ..LayerSpec::new("shade again", vec![tile_render::SpriteSetId::EMPTY, shadow_set])
        });
        for r in 0..grid.rows {
            for c in 0..grid.cols {
                if (c * 3 + r) % 5 == 2 {
                    world.set(shadow, c, r, 1);
                }
                if (c + r) % 7 == 3 {
                    world.set(after, c, r, 1);
                }
                if (c + r) % 4 != 1 {
                    world.set(shaded, c, r, 1);
                }
                if (c + 2 * r) % 3 != 0 {
                    world.set(shaded_again, c, r, 1);
                }
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
            let got = gpu.render(&mut renderer, &camera, frame, CLEAR);
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
    let got = gpu.render(&mut renderer, &camera, 2, CLEAR);
    let want = reference::render(&world, &atlas, &camera, 2, CLEAR);
    assert_same(&got, &want, "orthogonal");
    // Something was actually drawn.
    assert!(got.pixels.iter().any(|p| *p != CLEAR));
}

#[test]
fn quantized_world_with_scale_layers_is_pixel_exact() {
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let (mut world, atlas) = scene_with(grid, true);
    world.color = ColorModel::Quantized { bits: 5 };
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    for (x, y) in [(0, 0), (333, 101), (-517, 40)] {
        let mut camera = Camera::new((640, 360));
        camera.x = x;
        camera.y = y;
        camera.overhang = (0, 40, 0, 0);
        let got = gpu.render(&mut renderer, &camera, 5, CLEAR);
        let want = reference::render(&world, &atlas, &camera, 5, CLEAR);
        assert_same(&got, &want, &format!("quantized camera ({x}, {y})"));
        // Every channel is a 5-bit value expanded.
        assert!(got.pixels.iter().all(|p| p[..3].iter().all(|&c| c == ((c >> 3) << 3 | c >> 5))));
    }
}

#[test]
fn quantized_scale_rounds_down_after_every_blit() {
    // Every 5-bit value (column) darkened by every k/16 (row), twice.
    let gpu = Headless::new().expect("GPU");
    let mut atlas = AtlasBuilder::new(64);
    let mut sets = SpriteSets::default();
    let expand = |q: u32| ((q << 3) | (q >> 2)) as u8;
    let base = sets.single(atlas.add(&RgbaImage::from_fn(32, 16, |x, _| [expand(x), expand(31 - x), expand(x / 2), 255]), (0, 0)).unwrap());
    let once = sets.single(atlas.add(&RgbaImage::from_fn(32, 16, |_, y| [y as u8, 0, 0, 255]), (0, 0)).unwrap());
    let twice = sets.single(atlas.add(&RgbaImage::from_fn(32, 16, |x, y| [((y * 7 + x) % 16) as u8, 0, 0, 255]), (0, 0)).unwrap());
    let grid = Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (32, 16) };
    let mut world = TileWorld::new(grid, sets);
    world.color = ColorModel::Quantized { bits: 5 };
    let scale = |name: &str, set| LayerSpec { blend: Blend::Scale { denominator: 16 }, ..LayerSpec::new(name, vec![tile_render::SpriteSetId::EMPTY, set]) };
    let layers = [
        world.add_layer(LayerSpec::new("base", vec![tile_render::SpriteSetId::EMPTY, base])),
        world.add_layer(scale("once", once)),
        world.add_layer(scale("twice", twice)),
    ];
    for l in layers {
        world.set(l, 0, 0, 1);
    }
    let atlas = atlas.finish();
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let camera = Camera::new((32, 16));
    let got = gpu.render(&mut renderer, &camera, 0, CLEAR);
    assert_same(&got, &reference::render(&world, &atlas, &camera, 0, CLEAR), "exhaustive scale");
    // And the reference is the game's arithmetic.
    for y in 0..16u32 {
        for x in 0..32u32 {
            let k2 = (y * 7 + x) % 16;
            let want = expand(x * y / 16 * k2 / 16);
            assert_eq!(got.get(x, y)[0], want, "q {x} k {y} then {k2}");
        }
    }
}

#[test]
fn quantized_translucency_mixes_and_rounds_down() {
    // Every 5-bit source value (column) at 50 % over every destination
    // value (row), then the same again: the game's `(src + dst) >> 1`.
    let gpu = Headless::new().expect("GPU");
    let mut atlas = AtlasBuilder::new(64);
    let mut sets = SpriteSets::default();
    let expand = |q: u32| ((q << 3) | (q >> 2)) as u8;
    let base = sets.single(atlas.add(&RgbaImage::from_fn(32, 32, |_, y| [expand(y), expand(31 - y), expand(y / 3), 255]), (0, 0)).unwrap());
    let half = sets.single(atlas.add(&RgbaImage::from_fn(32, 32, |x, _| [expand(x), expand(x / 2), expand(31 - x), 128]), (0, 0)).unwrap());
    let grid = Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (32, 32) };
    let mut world = TileWorld::new(grid, sets);
    world.color = ColorModel::Quantized { bits: 5 };
    let layers = [
        world.add_layer(LayerSpec::new("base", vec![tile_render::SpriteSetId::EMPTY, base])),
        world.add_layer(LayerSpec::new("once", vec![tile_render::SpriteSetId::EMPTY, half])),
        world.add_layer(LayerSpec::new("twice", vec![tile_render::SpriteSetId::EMPTY, half])),
    ];
    for l in layers {
        world.set(l, 0, 0, 1);
    }
    let atlas = atlas.finish();
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let camera = Camera::new((32, 32));
    let got = gpu.render(&mut renderer, &camera, 0, CLEAR);
    assert_same(&got, &reference::render(&world, &atlas, &camera, 0, CLEAR), "exhaustive 50 %");
    for y in 0..32u32 {
        for x in 0..32u32 {
            let want = expand((x + (x + y) / 2) / 2);
            assert_eq!(got.get(x, y)[0], want, "src {x} dst {y}");
        }
    }
}
