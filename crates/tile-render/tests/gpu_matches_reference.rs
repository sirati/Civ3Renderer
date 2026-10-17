//! The GPU renderer must produce exactly the reference renderer's pixels.

use tile_render::{
    Atlas, AtlasBuilder, Blend, Camera, ColorModel, Grid, PlacedSprite, Headless, LayerSpec, Layout, Present, RgbaImage, ScreenCopy, SpriteSets, TileRenderer, TileWorld, ZoomArt,
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
fn world_sprites_show_across_the_wrap() {
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    // (Sets are made in the same order either way; without shading the
    // direct-colour renders match.)
    let anim = scene_with(grid, true).0.free[0].set;
    let (mut world, atlas) = scene(grid);
    // Just before the seam (the world is 1536 pixels wide): seen from a
    // camera at 0 it is at −36, from one past the seam further on.
    world.free.push(PlacedSprite::new(anim, 1500, 150));
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    for x in [0, 1300, 1536 + 20, -200] {
        let mut camera = Camera::new((640, 360));
        camera.x = x;
        let got = gpu.render(&mut renderer, &camera, 3, CLEAR);
        let want = reference::render(&world, &atlas, &camera, 3, CLEAR);
        assert_same(&got, &want, &format!("camera at {x}"));
    }
    let camera = Camera::new((640, 360));
    let draws = reference::draw_list(&world, &atlas, &camera, 3);
    assert!(draws.iter().any(|d| d.y == 150 - 40 && d.x == 1500 - 1536 + 34), "the sprite near the seam is drawn left of the camera");
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

#[test]
fn several_views_share_one_target() {
    // The whole window, and an inset view of another part of the world;
    // screen sprites belong to one view each, world sprites to both.
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let (mut world, atlas) = scene_with(grid, true);
    world.color = ColorModel::Quantized { bits: 5 };
    let screen_sprite = world.free.iter().find(|s| s.screen).cloned().unwrap();
    world.free.push(PlacedSprite { view: Some(1), x: 200, y: 140, clip: Some([180, 130, 260, 170]), ..screen_sprite.clone() });
    world.free.push(PlacedSprite { view: Some(0), x: 220, y: 150, ..screen_sprite });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let mut main = Camera::new((400, 240));
    main.x = 50;
    main.overhang = (0, 40, 0, 0);
    let mut inset = Camera::new((150, 90));
    inset.x = 333;
    inset.y = 101;
    inset.origin = (170, 120);
    inset.overhang = (0, 40, 0, 0);
    let cameras = [main, inset];
    let got = gpu.render_views(&mut renderer, &cameras, (400, 240), 5, CLEAR);
    let want = reference::render_views(&world, &atlas, &cameras, (400, 240), 5, CLEAR);
    assert_same(&got, &want, "two views");
}

#[test]
fn a_scaled_interface_shows_what_is_below_it() {
    // A world view shown 2× and an interface over it at 3/2 (drawn straight
    // at its zoom, `hairline`): the interface's sprites over the world,
    // the world where it has none; the same on the GPU.
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let (mut world, atlas) = scene_with(grid, true);
    world.color = ColorModel::Quantized { bits: 5 };
    let screen_sprite = world.free.iter().find(|s| s.screen && s.clip.is_none()).cloned().unwrap();
    world.free.retain(|s| !s.screen);
    world.free.push(PlacedSprite { view: Some(1), x: 40, y: 30, ..screen_sprite });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let main = Camera { x: 30, y: 10, zoom: (2, 1), overhang: (0, 40, 0, 0), ..Camera::new((400, 240)) };
    let interface = Camera { zoom: (3, 2), cells: false, hairline: true, ..Camera::new((400, 240)) };
    let got = gpu.render_views(&mut renderer, &[main, interface], (400, 240), 1, CLEAR);
    let want = reference::render_views(&world, &atlas, &[main, interface], (400, 240), 1, CLEAR);
    assert_same(&got, &want, "scaled interface");
    let alone = gpu.render_views(&mut renderer, &[main], (400, 240), 1, CLEAR);
    let same = got.pixels.iter().zip(&alone.pixels).filter(|(a, b)| a == b).count();
    assert!(same > got.pixels.len() / 2, "the world shows around the interface's sprite ({same} of {} pixels)", got.pixels.len());
    assert!(same < got.pixels.len(), "the interface's sprite shows");
}

#[test]
fn world_sprites_can_skip_one_view() {
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let (mut world, atlas) = scene_with(grid, true);
    world.color = ColorModel::Quantized { bits: 5 };
    let world_sprite = world.free.iter().find(|s| !s.screen).cloned().unwrap();
    // Seen by both cameras, drawn only by the second.
    world.free.push(PlacedSprite { view: Some(0), except_view: true, x: 120, y: 100, ..world_sprite });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let left = Camera::new((200, 200));
    let right = Camera { origin: (200, 0), ..Camera::new((200, 200)) };
    let cameras = [left, right];
    let got = gpu.render_views(&mut renderer, &cameras, (400, 200), 2, CLEAR);
    assert_same(&got, &reference::render_views(&world, &atlas, &cameras, (400, 200), 2, CLEAR), "except view");
    // The two halves show the same world but for that sprite.
    let half = |x0: u32| (0..200).flat_map(|y| (x0..x0 + 200).map(move |x| (x, y))).map(|(x, y)| got.pixels[(y * 400 + x) as usize]).collect::<Vec<_>>();
    let differ = half(0).iter().zip(&half(200)).filter(|(a, b)| a != b).count();
    assert!(differ > 0, "the sprite shows in the second view only");
}

#[test]
fn interface_views_show_only_their_own_sprites() {
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let (mut world, atlas) = scene_with(grid, true);
    world.color = ColorModel::Quantized { bits: 5 };
    let screen_sprite = world.free.iter().find(|s| s.screen).cloned().unwrap();
    world.free.push(PlacedSprite { view: Some(1), x: 60, y: 40, ..screen_sprite });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let main = Camera::new((300, 200));
    let interface = Camera { cells: false, ..Camera::new((300, 200)) };
    let cameras = [main, interface];
    let got = gpu.render_views(&mut renderer, &cameras, (300, 200), 2, CLEAR);
    assert_same(&got, &reference::render_views(&world, &atlas, &cameras, (300, 200), 2, CLEAR), "interface view");
    // The interface adds only its sprite to what the main view shows.
    let alone = gpu.render_views(&mut renderer, &cameras[..1], (300, 200), 2, CLEAR);
    let changed = got.pixels.iter().zip(&alone.pixels).filter(|(a, b)| a != b).count();
    assert!(changed > 0 && changed < 64 * 64, "{changed} pixels changed");
}

#[test]
fn zoomed_views_scale_by_nearest_texel() {
    // A main view zoomed in 3/2, a magnifier at 3/1, a zoomed-out inset at
    // 1/2, and an interface at zoom 1 blending over all of them; in the
    // quantized and the direct colour model (without blends there: the
    // GPU's 8-bit blending rounds its own way).
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    for color in [ColorModel::Quantized { bits: 5 }, ColorModel::Direct] {
        let (mut world, atlas) = scene_with(grid, color != ColorModel::Direct);
        world.color = color;
        if let Some(screen_sprite) = world.free.iter().find(|s| s.screen).cloned() {
            world.free.push(PlacedSprite { view: Some(1), x: 10, y: 12, ..screen_sprite.clone() });
            world.free.push(PlacedSprite { view: Some(3), x: 300, y: 20, ..screen_sprite.clone() });
            world.free.push(PlacedSprite { view: Some(0), x: 90, y: 60, clip: Some([95, 50, 140, 90]), ..screen_sprite });
        }
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        renderer.set_world(&gpu.device, &world);
        let overhang = (0, 40, 0, 0);
        let main = Camera { x: 50, y: 7, zoom: (3, 2), overhang, screen_shift: (12, -7), ..Camera::new((400, 240)) };
        let magnifier = Camera { x: 140, y: 90, zoom: (3, 1), origin: (250, 10), overhang, ..Camera::new((121, 80)) };
        let inset = Camera { x: 20, y: 30, zoom: (1, 2), origin: (-30, 150), overhang, ..Camera::new((160, 100)) };
        let interface = Camera { cells: false, ..Camera::new((400, 240)) };
        let cameras = [main, magnifier, inset, interface];
        let got = gpu.render_views(&mut renderer, &cameras, (400, 240), 3, CLEAR);
        let want = reference::render_views(&world, &atlas, &cameras, (400, 240), 3, CLEAR);
        assert_same(&got, &want, &format!("zoomed views, {color:?}"));
    }
}

#[test]
fn a_clip_meaning_everywhere_hides_nothing_in_a_view_off_the_corner() {
    // A screen sprite clipped to "everywhere" (i32::MIN..i32::MAX, as a
    // mirrored interface draws straight onto the screen) in a view whose
    // origin is off the window's corner (a popup drawn as a view of its
    // own, perhaps moved): moving the clip by the origin must not wrap.
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let (mut world, atlas) = scene_with(grid, true);
    world.color = ColorModel::Quantized { bits: 5 };
    let sprite = world.free.iter().find(|s| s.screen).cloned().expect("a screen sprite");
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    let main = Camera { x: 30, y: 10, ..Camera::new((320, 200)) };
    let part = Camera { cells: false, origin: (60, 40), ..Camera::new((120, 90)) };
    let moved = Camera { cells: false, origin: (160, 90), screen_shift: (100, 50), ..Camera::new((120, 90)) };
    renderer.set_world(&gpu.device, &world);
    let without = gpu.render_views(&mut renderer, &[main, part, moved], (320, 200), 1, CLEAR);
    let everywhere = Some([i32::MIN, i32::MIN, i32::MAX, i32::MAX]);
    world.free.push(PlacedSprite { view: Some(1), x: 70, y: 50, clip: everywhere, ..sprite.clone() });
    world.free.push(PlacedSprite { view: Some(2), x: 75, y: 45, clip: everywhere, ..sprite });
    renderer.set_world(&gpu.device, &world);
    let cameras = [main, part, moved];
    let got = gpu.render_views(&mut renderer, &cameras, (320, 200), 1, CLEAR);
    let want = reference::render_views(&world, &atlas, &cameras, (320, 200), 1, CLEAR);
    assert_same(&got, &want, "unbounded clip in views off the corner");
    let changed: Vec<(u32, u32)> = (0..200).flat_map(|y| (0..320).map(move |x| (x, y))).filter(|&(x, y)| got.get(x, y) != without.get(x, y)).collect();
    assert!(changed.iter().any(|&(x, y)| x < 150 && y < 130), "the sprite in the view at (60, 40) is drawn");
    assert!(changed.iter().any(|&(x, y)| x >= 160 && y >= 90), "the sprite in the moved view is drawn, moved");
}

#[test]
fn zoom_repeats_texels() {
    // At 2/1 every texel of the zoom-1 picture becomes a 2×2 block.
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let (mut world, atlas) = scene_with(grid, true);
    world.color = ColorModel::Quantized { bits: 5 };
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let flat = Camera { x: 64, y: 32, ..Camera::new((100, 60)) };
    let zoomed = Camera { zoom: (2, 1), viewport: (200, 120), ..flat };
    let small = gpu.render_views(&mut renderer, &[flat], (100, 60), 0, CLEAR);
    let big = gpu.render_views(&mut renderer, &[zoomed], (200, 120), 0, CLEAR);
    for y in 0..120 {
        for x in 0..200 {
            assert_eq!(big.get(x, y), small.get(x / 2, y / 2), "({x}, {y})");
        }
    }
}

#[test]
fn screen_copies_magnify_the_finished_picture() {
    // A magnifier over the corner (partly outside the target) and one over
    // the middle at 5/2, in both colour models.
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    for color in [ColorModel::Quantized { bits: 5 }, ColorModel::Direct] {
        let (mut world, atlas) = scene_with(grid, color != ColorModel::Direct);
        world.color = color;
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        renderer.set_world(&gpu.device, &world);
        let main = Camera { x: 30, y: 10, overhang: (0, 40, 0, 0), ..Camera::new((320, 200)) };
        let copies = [
            ScreenCopy { source: [-10, -5, 40, 30], at: (250, 140), zoom: (3, 1) },
            ScreenCopy { source: [150, 90, 33, 21], at: (-7, 3), zoom: (5, 2) },
        ];
        let got = gpu.render_composed(&mut renderer, &[main], &copies, (320, 200), 1, CLEAR);
        let want = reference::render_composed(&world, &atlas, &[main], &copies, (320, 200), 1, CLEAR);
        assert_same(&got, &want, &format!("screen copies, {color:?}"));
        let plain = gpu.render_views(&mut renderer, &[main], (320, 200), 1, CLEAR);
        assert_ne!(got.pixels, plain.pixels);
    }
}

#[test]
fn a_presented_picture_is_the_canvas_with_each_pixel_doubled() {
    // A 160x100 picture shown 2x in a larger target, off its corner: each
    // target pixel is the canvas pixel it covers, the rest is clear; in
    // both colour models (the quantized one is composed internally anyway,
    // the direct one only for this).
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    for color in [ColorModel::Quantized { bits: 5 }, ColorModel::Direct] {
        let (mut world, atlas) = scene_with(grid, color != ColorModel::Direct);
        world.color = color;
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        renderer.set_world(&gpu.device, &world);
        let camera = Camera { x: 30, y: 10, overhang: (0, 40, 0, 0), ..Camera::new((160, 100)) };
        let small = gpu.render_views(&mut renderer, &[camera], (160, 100), 1, CLEAR);
        let present = Present { canvas: (160, 100), zoom: (2, 1), at: (5, 10), target: (330, 220) };
        let big = gpu.render_presented(&mut renderer, &[camera], &[], present, 1, CLEAR);
        for y in 0..220 {
            for x in 0..330 {
                let (cx, cy) = ((x as i32 - 5).div_euclid(2), (y as i32 - 10).div_euclid(2));
                let inside = (0..160).contains(&cx) && (0..100).contains(&cy);
                let want = if inside { small.get(cx as u32, cy as u32) } else { CLEAR };
                assert_eq!(big.get(x, y), want, "({x}, {y}), {color:?}");
            }
        }
        // As large as fits, not a whole number of times: every target pixel
        // shows the canvas pixel ⌊(p − at)·den/num⌋.
        let present = Present { canvas: (160, 100), zoom: (5, 3), at: (2, 1), target: (270, 170) };
        let odd = gpu.render_presented(&mut renderer, &[camera], &[], present, 1, CLEAR);
        for y in 0..170 {
            for x in 0..270 {
                let (cx, cy) = ((x as i32 - 2) * 3 / 5, (y as i32 - 1) * 3 / 5);
                let inside = x >= 2 && y >= 1 && cx < 160 && cy < 100;
                let want = if inside { small.get(cx as u32, cy as u32) } else { CLEAR };
                assert_eq!(odd.get(x, y), want, "5/3: ({x}, {y}), {color:?}");
            }
        }
        let present = Present { canvas: (160, 100), zoom: (2, 1), at: (5, 10), target: (330, 220) };
        // The pointer over it, back in the canvas's pixels; beside the
        // picture (on the target), at its edge.
        assert_eq!(present.to_canvas((5.0, 10.0)), (0, 0));
        assert_eq!(present.to_canvas((324.9, 209.9)), (159, 99));
        assert_eq!(present.to_canvas((0.0, 219.0)), (0, 99));
        assert_eq!(present.to_canvas((-6.0, 10.0)), (-6, 0));
    }
}

#[test]
fn inverting_sprites_invert_what_is_under_them() {
    // Rings in inverted colours over the world and over each other, between
    // ordinary sprites (the pipeline switches back and forth).
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    for color in [ColorModel::Quantized { bits: 5 }, ColorModel::Direct] {
        let (mut world, atlas) = scene_with(grid, color != ColorModel::Direct);
        world.color = color;
        // (Sprite sets are made before the shading, the same in both scenes.)
        let some = scene_with(grid, true).0.free[0];
        for (i, blend) in [Blend::Invert, Blend::Alpha, Blend::Invert, Blend::Invert].into_iter().enumerate() {
            world.free.push(PlacedSprite { x: 60 + 23 * i as i32, y: 40 + 11 * i as i32, blend, after_pass: usize::MAX, ..some });
        }
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        renderer.set_world(&gpu.device, &world);
        let camera = Camera { overhang: (0, 40, 0, 0), ..Camera::new((320, 200)) };
        let got = gpu.render_views(&mut renderer, &[camera], (320, 200), 2, CLEAR);
        let want = reference::render_views(&world, &atlas, &[camera], (320, 200), 2, CLEAR);
        assert_same(&got, &want, &format!("inverting sprites, {color:?}"));
    }
}

#[test]
fn subpixel_sprites_blend_each_channel_on_its_own() {
    // The art's colours read as coverage per channel, drawn in a tint over
    // the world and over each other, between ordinary and inverting ones.
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    // (The quantized model: the direct one rounds as the GPU's blender does.)
    for color in [ColorModel::Quantized { bits: 5 }] {
        let (mut world, atlas) = scene_with(grid, color != ColorModel::Direct);
        world.color = color;
        let some = scene_with(grid, true).0.free[0];
        let blends = [Blend::Subpixel { color: [250, 30, 90] }, Blend::Alpha, Blend::Subpixel { color: [0, 0, 0] }, Blend::Invert, Blend::Subpixel { color: [255, 255, 255] }];
        for (i, blend) in blends.into_iter().enumerate() {
            world.free.push(PlacedSprite { x: 50 + 19 * i as i32, y: 30 + 13 * i as i32, blend, after_pass: usize::MAX, ..some });
        }
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        renderer.set_world(&gpu.device, &world);
        let camera = Camera { overhang: (0, 40, 0, 0), ..Camera::new((320, 200)) };
        let got = gpu.render_views(&mut renderer, &[camera], (320, 200), 2, CLEAR);
        let want = reference::render_views(&world, &atlas, &[camera], (320, 200), 2, CLEAR);
        assert_same(&got, &want, &format!("subpixel sprites, {color:?}"));
    }
}

#[test]
fn zoomed_art_replaces_its_low_twin_in_a_scaled_view() {
    // A label as a low (1:1) sprite and a zoomed one (art at the view's
    // zoom): a scaled view showing zoomed art draws the zoomed one after
    // scaling, other views the low one; the zoomed one follows the view's
    // screen shift scaled.
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let (mut world, atlas) = scene_with(grid, true);
    world.color = ColorModel::Quantized { bits: 5 };
    let s = world.free.iter().find(|s| s.screen).cloned().unwrap();
    world.free.push(PlacedSprite { view: Some(0), x: 40, y: 30, zoom_art: ZoomArt::Low, clip: Some([30, 20, 120, 90]), ..s });
    world.free.push(PlacedSprite { view: Some(0), x: 40, y: 30, zoom_art: ZoomArt::Zoomed { zoom: (2, 1), offset: (5, -2) }, clip: Some([30, 20, 120, 90]), ..s });
    // World-space: a twin for this zoom, and one for another zoom (unused).
    let w = PlacedSprite { screen: false, view: None, x: 200, y: 140, after_pass: usize::MAX, ..s };
    world.free.push(PlacedSprite { zoom_art: ZoomArt::Low, ..w });
    world.free.push(PlacedSprite { zoom_art: ZoomArt::Zoomed { zoom: (2, 1), offset: (-3, 4) }, ..w });
    world.free.push(PlacedSprite { zoom_art: ZoomArt::Zoomed { zoom: (3, 1), offset: (0, 0) }, ..w });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let main = Camera { x: 30, y: 10, zoom: (2, 1), screen_shift: (5, -3), zoomed_art: true, overhang: (0, 40, 0, 0), ..Camera::new((320, 200)) };
    for cameras in [[main], [Camera { zoomed_art: false, ..main }], [Camera { zoom: (1, 1), ..main }]] {
        let got = gpu.render_views(&mut renderer, &cameras, (320, 200), 1, CLEAR);
        let want = reference::render_views(&world, &atlas, &cameras, (320, 200), 1, CLEAR);
        assert_same(&got, &want, &format!("zoom art, {:?}", cameras[0].zoomed_art));
    }
}

#[test]
fn stacked_views_scale_with_their_group() {
    // A game's screen fitted into a pane: its map view scaled to 3/4 with
    // its interface stacked on it (one picture, scaled once; the
    // interface's zoomed text at full resolution), a second view beside it;
    // and the same unscaled.
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let (mut world, atlas) = scene_with(grid, true);
    world.color = ColorModel::Quantized { bits: 5 };
    let s = world.free.iter().find(|s| s.screen).cloned().unwrap();
    world.free.push(PlacedSprite { view: Some(1), x: 30, y: 40, zoom_art: ZoomArt::Low, ..s });
    world.free.push(PlacedSprite { view: Some(1), x: 30, y: 40, zoom_art: ZoomArt::Zoomed { zoom: (3, 4), offset: (2, 1) }, ..s });
    world.free.push(PlacedSprite { view: Some(1), x: 150, y: 90, clip: Some([140, 80, 200, 120]), ..s });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let game = Camera { x: 40, y: 20, zoom: (3, 4), origin: (10, 5), overhang: (0, 40, 0, 0), ..Camera::new((240, 150)) };
    let interface = Camera { cells: false, stacked: true, zoomed_art: true, ..Camera::new((320, 200)) };
    let other = Camera { x: 300, y: 200, origin: (260, 0), overhang: (0, 40, 0, 0), ..Camera::new((140, 200)) };
    for cameras in [[game, interface, other], [Camera { zoom: (1, 1), viewport: (250, 200), ..game }, interface, other]] {
        let got = gpu.render_views(&mut renderer, &cameras, (400, 240), 2, CLEAR);
        let want = reference::render_views(&world, &atlas, &cameras, (400, 240), 2, CLEAR);
        assert_same(&got, &want, &format!("stacked views, zoom {:?}", cameras[0].zoom));
    }
}

#[test]
fn drawing_at_the_zoom_shows_what_the_scaled_picture_does() {
    // From a zoomed sprite on (here one for a view not drawn, after the
    // first pass) views showing zoomed art are drawn at their zoom straight
    // into the target: pixel for pixel what their picture drawn at zoom 1
    // and scaled shows. A game's screen fitted into a pane (its interface
    // stacked on its map) and an inset, zoomed in and out, both colour
    // models (the direct one against the GPU's own scaled picture only: its
    // 8-bit blending rounds its own way).
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let zooms = [(3, 2), (1, 2), (2, 3), (3, 1), (1, 3)];
    for color in [ColorModel::Quantized { bits: 5 }, ColorModel::Direct] {
        let (mut world, atlas) = scene_with(grid, true);
        world.color = color;
        let s = world.free.iter().find(|s| s.screen).cloned().unwrap();
        world.free.push(PlacedSprite { view: Some(1), x: 150, y: 90, clip: Some([140, 80, 200, 120]), ..s });
        world.free.push(PlacedSprite { view: Some(2), x: 10, y: 15, ..s });
        for zoom in zooms {
            world.free.push(PlacedSprite { view: Some(9), after_pass: 0, zoom_art: ZoomArt::Zoomed { zoom, offset: (0, 0) }, ..s });
        }
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        renderer.set_world(&gpu.device, &world);
        let overhang = (0, 40, 0, 0);
        for zoom in zooms {
            let game = Camera { x: 40, y: 20, zoom, origin: (10, 5), screen_shift: (3, -2), overhang, zoomed_art: true, ..Camera::new((240, 150)) };
            let interface = Camera { cells: false, stacked: true, zoomed_art: true, ..Camera::new((320, 200)) };
            let inset = Camera { x: 300, y: 200, zoom, origin: (262, 31), overhang, zoomed_art: true, ..Camera::new((131, 170)) };
            let at_zoom = [game, interface, inset];
            let pictures = at_zoom.map(|c| Camera { zoomed_art: false, ..c });
            let got = gpu.render_views(&mut renderer, &at_zoom, (400, 240), 2, CLEAR);
            if color != ColorModel::Direct {
                let want = reference::render_views(&world, &atlas, &at_zoom, (400, 240), 2, CLEAR);
                assert_same(&got, &want, &format!("drawn at zoom {zoom:?}"));
                let want = reference::render_views(&world, &atlas, &pictures, (400, 240), 2, CLEAR);
                assert_same(&got, &want, &format!("drawn at zoom {zoom:?}, against the reference's scaled picture"));
            }
            let scaled = gpu.render_views(&mut renderer, &pictures, (400, 240), 2, CLEAR);
            assert_same(&got, &scaled, &format!("drawn at zoom {zoom:?} against the scaled picture, {color:?}"));
        }
    }
}

#[test]
fn zoomed_art_is_drawn_in_its_pass() {
    // A label under a darkening pass (as the map's labels are under the fog
    // of war), as a 1:1 sprite and one made for the view's zoom: the
    // darkening goes over the zoomed one too, in a view and in a stacked
    // one; on top of it, it is not darkened.
    let gpu = Headless::new().expect("GPU");
    let mut atlas = AtlasBuilder::new(512);
    let mut sets = SpriteSets::default();
    let ground = sets.single(atlas.add(&diamond(3), (0, 0)).unwrap());
    let fog = sets.single(atlas.add(&shade(), (0, 0)).unwrap());
    let low = sets.single(atlas.add(&tall(40), (0, 0)).unwrap());
    let zoomed = sets.single(atlas.add(&RgbaImage::from_fn(120, 176, |x, y| [250, 200 + (x % 50) as u8, (y % 200) as u8, 255]), (0, 0)).unwrap());
    let grid = Grid { cols: 8, rows: 12, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let label = |after_pass: usize| {
        let mut world = TileWorld::new(grid, sets.clone());
        let terrain = world.add_layer(LayerSpec::new("terrain", vec![ground]));
        let shading = world.add_layer(LayerSpec { pass: 1, blend: Blend::Scale { denominator: 16 }, ..LayerSpec::new("fog", vec![tile_render::SpriteSetId::EMPTY, fog]) });
        for r in 0..grid.rows {
            for c in 0..grid.cols {
                world.set(terrain, c, r, 0);
                world.set(shading, c, r, 1);
            }
        }
        world.color = ColorModel::Quantized { bits: 5 };
        for view in [0, 1] {
            let (x, y) = (60 + 90 * view as i32, 50);
            let s = PlacedSprite { screen: true, view: Some(view), ..PlacedSprite::new(low, x, y) };
            world.free.push(PlacedSprite { after_pass: 0, zoom_art: ZoomArt::Low, ..s });
            world.free.push(PlacedSprite { set: zoomed, after_pass, zoom_art: ZoomArt::Zoomed { zoom: (2, 1), offset: (1, -1) }, ..s });
        }
        world
    };
    let atlas = atlas.finish();
    let main = Camera { x: 30, y: 10, zoom: (2, 1), zoomed_art: true, ..Camera::new((400, 240)) };
    let interface = Camera { cells: false, stacked: true, zoomed_art: true, ..Camera::new((200, 120)) };
    let cameras = [main, interface];
    let mut pictures = Vec::new();
    for after_pass in [0, usize::MAX] {
        let world = label(after_pass);
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        renderer.set_world(&gpu.device, &world);
        let got = gpu.render_views(&mut renderer, &cameras, (400, 240), 1, CLEAR);
        let want = reference::render_views(&world, &atlas, &cameras, (400, 240), 1, CLEAR);
        assert_same(&got, &want, &format!("zoomed label after pass {after_pass}"));
        pictures.push(got);
    }
    // Under the darkening pass it is darker than on top of it, nothing else
    // changes.
    let luma = |p: &[u8; 4]| p[0] as u32 * 3 + p[1] as u32 * 6 + p[2] as u32;
    let (mut darker, mut other) = (0, 0);
    for (under, over) in pictures[0].pixels.iter().zip(&pictures[1].pixels) {
        match luma(under).cmp(&luma(over)) {
            std::cmp::Ordering::Less => darker += 1,
            std::cmp::Ordering::Greater => other += 1,
            _ => {}
        }
    }
    assert!(darker > 10_000 && other == 0, "{darker} pixels darker, {other} lighter");
}

#[test]
fn hairlines_stay_when_an_interface_is_scaled_down() {
    // One-pixel lines of an interface stacked on a game view shown at 1/2
    // and 1/3 keep one screen pixel (at odd and even positions); without
    // `hairline` some vanish. Nothing else changes.
    let gpu = Headless::new().expect("GPU");
    let mut atlas = AtlasBuilder::new(256);
    let mut sets = SpriteSets::default();
    let ground = sets.single(atlas.add(&diamond(3), (0, 0)).unwrap());
    let yellow = [255, 255, 0, 255];
    let across = sets.single(atlas.add(&RgbaImage::from_fn(16, 1, |_, _| yellow), (0, 0)).unwrap());
    let down = sets.single(atlas.add(&RgbaImage::from_fn(1, 16, |_, _| yellow), (0, 0)).unwrap());
    let box_ = sets.single(atlas.add(&diamond(7), (0, 0)).unwrap());
    let grid = Grid { cols: 8, rows: 12, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let mut world = TileWorld::new(grid, sets);
    let terrain = world.add_layer(LayerSpec::new("terrain", vec![ground]));
    for r in 0..grid.rows {
        for c in 0..grid.cols {
            world.set(terrain, c, r, 0);
        }
    }
    world.color = ColorModel::Quantized { bits: 5 };
    let mut lines = Vec::new();
    for i in 0..6 {
        let at = (40 + 7 * i, 30 + 5 * i);
        world.free.push(PlacedSprite { screen: true, view: Some(1), ..PlacedSprite::new(across, at.0, at.1) });
        world.free.push(PlacedSprite { screen: true, view: Some(1), ..PlacedSprite::new(down, 200 + at.1, at.0) });
        lines.push(at);
    }
    // A clipped one, and ordinary sprites.
    world.free.push(PlacedSprite { screen: true, view: Some(1), clip: Some([0, 0, 300, 101]), ..PlacedSprite::new(across, 60, 100) });
    world.free.push(PlacedSprite { screen: true, view: Some(1), ..PlacedSprite::new(box_, 90, 120) });
    let atlas = atlas.finish();
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    for zoom in [(1, 2), (1, 3)] {
        let (w, h) = (300 * zoom.0 / zoom.1, 200 * zoom.0 / zoom.1);
        let game = Camera { x: 20, y: 10, zoom, origin: (5, 3), ..Camera::new((w, h)) };
        let interface = Camera { cells: false, stacked: true, hairline: true, ..Camera::new((300, 200)) };
        let cameras = [game, interface];
        let got = gpu.render_views(&mut renderer, &cameras, (160, 120), 0, CLEAR);
        let want = reference::render_views(&world, &atlas, &cameras, (160, 120), 0, CLEAR);
        assert_same(&got, &want, &format!("hairlines at {zoom:?}"));
        let yellow_at = |img: &RgbaImage, (x0, y0, x1, y1): (u32, u32, u32, u32)| (y0..y1).any(|y| (x0..x1).any(|x| img.get(x, y) == yellow));
        let near = |v: i32, o: i32| ((v as u32 * zoom.0) / zoom.1 + o as u32).saturating_sub(1);
        for &(x, y) in &lines {
            let (px, py) = (near(x, 5), near(y, 3));
            assert!(yellow_at(&got, (px, py, px + 8, py + 3)), "line across at ({x}, {y}), {zoom:?}");
            let (px, py) = (near(200 + y, 5), near(x, 3));
            assert!(yellow_at(&got, (px, py, px + 3, py + 8)), "line down at ({}, {x}), {zoom:?}", 200 + y);
        }
        let thin = [game, Camera { hairline: false, ..interface }];
        let plain = gpu.render_views(&mut renderer, &thin, (160, 120), 0, CLEAR);
        let lost = lines.iter().filter(|&&(x, y)| {
            let (px, py) = (near(x, 5), near(y, 3));
            !yellow_at(&plain, (px, py, px + 8, py + 3))
        });
        assert!(lost.count() > 0, "without hairline some lines vanish at {zoom:?}");
        let changed = got.pixels.iter().zip(&plain.pixels).filter(|(a, b)| a != b).count();
        assert!(changed > 0 && changed < 120, "{changed} pixels changed at {zoom:?}");
    }
}

#[test]
fn a_viewers_most_views_are_all_drawn() {
    // The game's map and city views, a second pane, three insets with their
    // frames, a magnifier with its frame and the interface on top: 12 views,
    // all drawn (the interface's sprite, last, too).
    assert!(tile_render::MAX_VIEWS >= 12);
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let (mut world, atlas) = scene_with(grid, true);
    world.color = ColorModel::Quantized { bits: 5 };
    let s = world.free.iter().find(|s| s.screen).cloned().unwrap();
    world.free.push(PlacedSprite { view: Some(11), x: 300, y: 180, ..s });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let overhang = (0, 40, 0, 0);
    let frame = |x: i32, y: i32| Camera { zoom: (2, 1), origin: (x - 2, y - 2), cells: false, ..Camera::new((64, 44)) };
    let mut cameras = vec![
        Camera { x: 10, y: 20, overhang, ..Camera::new((200, 240)) },
        Camera { x: 400, y: 30, origin: (120, 160), overhang, ..Camera::new((70, 60)) },
        Camera { x: 200, y: 100, zoom: (2, 1), origin: (204, 0), overhang, ..Camera::new((196, 240)) },
    ];
    for (i, (x, y)) in [(20, 20), (90, 40), (260, 120)].into_iter().enumerate() {
        cameras.push(frame(x, y));
        cameras.push(Camera { x: 100 * i as i32, y: 50, zoom: (1, 2), origin: (x, y), overhang, ..Camera::new((60, 40)) });
    }
    cameras.push(frame(150, 170));
    cameras.push(Camera { x: 333, y: 77, zoom: (3, 1), origin: (150, 170), overhang, ..Camera::new((60, 40)) });
    cameras.push(Camera { cells: false, ..Camera::new((400, 240)) });
    assert_eq!(cameras.len(), 12);
    let got = gpu.render_views(&mut renderer, &cameras, (400, 240), 1, CLEAR);
    let want = reference::render_views(&world, &atlas, &cameras, (400, 240), 1, CLEAR);
    assert_same(&got, &want, "12 views");
    let without = gpu.render_views(&mut renderer, &cameras[..11], (400, 240), 1, CLEAR);
    assert_ne!(got.pixels, without.pixels, "the last view is drawn");
}

/// `scene_with`'s world and atlas, its sprites cut for views showing the
/// world smaller (`Camera::downscale`): sample boxes with odd corners and
/// sizes, each `scaling`, a twin drawn instead, an empty box; layers with
/// odd and negative offsets.
fn downscaled_scene(grid: Grid, shading: bool) -> (TileWorld, Atlas) {
    use tile_render_shared::{KEEP_SIZE, SHRINK_KEEP_ANCHOR};
    let (mut world, atlas) = scene_with(grid, shading);
    // (Rebuilt: the builder keeps the sprites' tables.)
    let mut b = AtlasBuilder::new(512);
    let shade_id = b.add(&shade(), (0, 0)).unwrap();
    let shadow_id = b.add(&shade(), (20, -30)).unwrap();
    let terrain: Vec<_> = (0..5u8).map(|i| b.add(&diamond(i * 40), (0, 0)).unwrap()).collect();
    let anim: Vec<_> = (0..4u8).map(|i| b.add(&tall(i * 60), (34, -40)).unwrap()).collect();
    // The terrain's diamonds: column 0 is clear, so the box starts at 1
    // (odd: odd columns are sampled) and is 126 wide (Civ III's terrain).
    for (i, &t) in terrain.iter().enumerate() {
        *b.sprite_mut(t) = tile_render_shared::SpriteDesc { box_x: 1, box_y: i as u32 % 2, box_w: 126, box_h: 64 - i as u32 % 2, ..*b.sprite(t) };
    }
    // The animation: odd boxes, one frame keeping its anchor, one its size,
    // one drawing another sprite (the second terrain diamond) instead, one
    // with an empty box.
    let boxes = [(3, 5, 51, 77), (0, 1, 60, 87), (7, 2, 49, 80), (2, 2, 0, 0)];
    for (k, (&a, (x, y, w, h))) in anim.iter().zip(boxes).enumerate() {
        let s = b.sprite_mut(a);
        (s.box_x, s.box_y, s.box_w, s.box_h) = (x, y, w, h);
        s.scaling = [0, SHRINK_KEEP_ANCHOR, KEEP_SIZE, 0][k];
    }
    b.sprite_mut(anim[1]).downscaled = terrain[1].0 + 1;
    b.sprite_mut(shadow_id).box_x = 5;
    b.sprite_mut(shadow_id).box_w = 117;
    let _ = shade_id;
    let rebuilt = b.finish();
    assert_eq!(rebuilt.sprites.len(), atlas.sprites.len(), "same sprites as scene_with");
    for l in 0..world.layers().len() {
        let spec = world.layer_mut(tile_render::LayerId(l));
        spec.offset = [(0, 0), (-5, -13), (3, 7), (-1, 1), (7, -3), (0, -9), (-11, 0)][l % 7];
        spec.cull_offset = (1, -33);
    }
    // World sprites at odd and negative positions.
    world.free.push(PlacedSprite { after_pass: 0, ..PlacedSprite::new(tile_render::SpriteSetId::from_raw(world.free.first().map_or(0, |s| s.set.raw())), -37, 121) });
    world.free.retain(|s| s.set != tile_render::SpriteSetId::EMPTY);
    (world, rebuilt)
}

#[test]
fn downscaled_views_are_pixel_exact() {
    // Views showing the world 2 and 3 times smaller, from the same atlas:
    // wrapping, past the edges, culled, with world, screen, clipped and
    // darkening sprites, in both colour models (direct: no blends, as
    // elsewhere).
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    for color in [ColorModel::Quantized { bits: 5 }, ColorModel::Direct] {
        let (mut world, atlas) = downscaled_scene(grid, color != ColorModel::Direct);
        world.color = color;
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        let mut drawn = 0;
        renderer.set_world(&gpu.device, &world);
        for n in [2, 3] {
            for (x, y) in [(0, 0), (167, 51), (-259, 20), (700, -25), (39, 200)] {
                let mut camera = Camera { x, y, downscale: n, overhang: (0, 40, 0, 0), ..Camera::new((400, 240)) };
                for cull in [None, Some((x + 40, y + 20, x + 300, y + 200))] {
                    camera.cull = cull;
                    let got = gpu.render(&mut renderer, &camera, 5, CLEAR);
                    let want = reference::render(&world, &atlas, &camera, 5, CLEAR);
                    assert_same(&got, &want, &format!("1/{n} at ({x}, {y}), cull {cull:?}, {color:?}"));
                    drawn += got.pixels.iter().any(|p| *p != CLEAR) as u32;
                }
            }
        }
        assert!(drawn >= 15, "{drawn} of 20 views show something");
    }
}

#[test]
fn downscaled_views_compose_with_zoom() {
    // A half-size view shown at 2/1 and 3/2 in a pane (copied from its
    // zoom-1 picture, or drawn at the zoom straight into the target once it
    // shows zoomed art), next to a 1:1 view of the same world and an
    // interface on top.
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let (mut world, atlas) = downscaled_scene(grid, true);
    world.color = ColorModel::Quantized { bits: 5 };
    let s = world.free.iter().find(|s| s.screen).cloned().unwrap();
    world.free.push(PlacedSprite { view: Some(2), x: 20, y: 30, ..s });
    for zoom in [(2, 1), (3, 2)] {
        world.free.push(PlacedSprite { after_pass: 0, zoom_art: ZoomArt::Zoomed { zoom, offset: (1, -1) }, ..PlacedSprite::new(s.set, 90, 70) });
    }
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let overhang = (0, 40, 0, 0);
    for zoom in [(2, 1), (3, 2)] {
        for zoomed_art in [false, true] {
            let pane = Camera { x: 31, y: 17, zoom, downscale: 2, overhang, zoomed_art, ..Camera::new((260, 240)) };
            let plain = Camera { x: 70, y: 40, origin: (260, 0), overhang, ..Camera::new((140, 240)) };
            let interface = Camera { cells: false, ..Camera::new((400, 240)) };
            let cameras = [pane, plain, interface];
            let got = gpu.render_views(&mut renderer, &cameras, (400, 240), 2, CLEAR);
            let want = reference::render_views(&world, &atlas, &cameras, (400, 240), 2, CLEAR);
            assert_same(&got, &want, &format!("half size at zoom {zoom:?}, zoomed art {zoomed_art}"));
        }
        // At zoom 2/1 every pixel of the half-size picture is a 2×2 block.
        if zoom == (2, 1) {
            let flat = Camera { x: 31, y: 17, downscale: 2, overhang, ..Camera::new((130, 120)) };
            let small = gpu.render_views(&mut renderer, &[flat], (130, 120), 2, CLEAR);
            let big = gpu.render_views(&mut renderer, &[Camera { zoom, viewport: (260, 240), ..flat }], (260, 240), 2, CLEAR);
            for y in 0..240 {
                for x in 0..260 {
                    assert_eq!(big.get(x, y), small.get(x / 2, y / 2), "({x}, {y})");
                }
            }
        }
    }
}

#[test]
fn downscaled_sprites_sample_their_box() {
    // One sprite, 1/2: pixel (i, j) of its drawn rectangle is texel
    // (box + 2·(i, j)), from the anchor halved plus the box's corner halved.
    let gpu = Headless::new().expect("GPU");
    let mut b = AtlasBuilder::new(64);
    let img = RgbaImage::from_fn(9, 7, |x, y| [x as u8 * 20, y as u8 * 30, 7, 255]);
    let id = b.add(&img, (-5, 3)).unwrap();
    let s = b.sprite_mut(id);
    (s.box_x, s.box_y, s.box_w, s.box_h) = (3, 1, 5, 6);
    let mut sets = SpriteSets::default();
    let set = sets.single(id);
    let atlas = b.finish();
    let grid = Grid { cols: 4, rows: 4, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (16, 16) };
    let mut world = TileWorld::new(grid, sets);
    world.add_layer(LayerSpec::new("none", vec![tile_render::SpriteSetId::EMPTY]));
    world.free.push(PlacedSprite::new(set, 21, 9));
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let camera = Camera { downscale: 2, ..Camera::new((20, 20)) };
    let got = gpu.render(&mut renderer, &camera, 0, CLEAR);
    assert_same(&got, &reference::render(&world, &atlas, &camera, 0, CLEAR), "one sprite");
    // At (21/2 + -5/2 + 3/2, 9/2 + 3/2 + 1/2) = (9, 5): 5/2 × 6/2 pixels.
    for y in 0..20 {
        for x in 0..20 {
            let inside = (9..11).contains(&x) && (5..8).contains(&y);
            let want = if inside { img.get(3 + 2 * (x - 9), 1 + 2 * (y - 5)) } else { CLEAR };
            assert_eq!(got.get(x, y), want, "({x}, {y})");
        }
    }
}

/// `downscaled_scene`'s world plus a pass drawn right to left whose cells
/// are moved by per-cell offsets (odd, negative, larger than a cell, some
/// across the wrap), its animation following the cells' phases alone
/// (`ticks_per_frame` 0), one of its layers darkening.
fn offset_scene(grid: Grid, shading: bool) -> (TileWorld, Atlas) {
    let (mut world, atlas) = downscaled_scene(grid, shading);
    let anim = world.layers()[1].lut[1];
    let tile = world.layers()[0].lut[2];
    let shadow = world.layers().iter().find(|l| l.blend != Blend::Alpha).map(|l| l.lut[1]);
    let last = world.layers().last().map_or(0, |l| l.pass) + 1;
    let moved = |name: &str, lut, blend| LayerSpec {
        pass: last,
        descending: true,
        per_cell_offset: true,
        per_cell_phase: true,
        ticks_per_frame: 0,
        offset: (64, 32),
        blend,
        ..LayerSpec::new(name, lut)
    };
    let base = world.add_layer(moved("under", vec![tile_render::SpriteSetId::EMPTY, tile], Blend::Alpha));
    let mut layers = vec![base];
    if let Some(shadow) = shadow {
        layers.push(world.add_layer(moved("shadow", vec![tile_render::SpriteSetId::EMPTY, shadow], Blend::Scale { denominator: 16 })));
    }
    layers.push(world.add_layer(moved("unit", vec![tile_render::SpriteSetId::EMPTY, anim], Blend::Alpha)));
    let offsets: [(i16, i16); 7] = [(0, 0), (-37, 11), (64, -32), (-129, 70), (5, -3), (200, 1), (-1, -1)];
    for r in 0..grid.rows {
        for c in 0..grid.cols {
            if (c + r) % 3 == 0 {
                for (k, &l) in layers.iter().enumerate() {
                    world.set(l, c, r, 1);
                    world.set_offset(l, c, r, offsets[((c * 5 + r + k as u32) % 7) as usize]);
                    world.set_phase(l, c, r, ((c * 7 + r) % 9) as u8);
                }
            }
        }
    }
    (world, atlas)
}

#[test]
fn per_cell_offsets_are_pixel_exact() {
    // Cells moved by per-cell offsets, swept right to left, in both colour
    // models (direct: no blends), 1:1 and shown 2 and 3 times smaller,
    // across the wrap, past the edges, culled, and in zoomed views.
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    for color in [ColorModel::Quantized { bits: 5 }, ColorModel::Direct] {
        let (mut world, atlas) = offset_scene(grid, color != ColorModel::Direct);
        world.color = color;
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        renderer.set_world(&gpu.device, &world);
        for n in [1, 2, 3] {
            for (x, y) in [(0, 0), (167, 51), (-259, 20), (1400, -25), (39, 200)] {
                let mut camera = Camera { x, y, downscale: n, overhang: (0, 40, 0, 0), ..Camera::new((400, 240)) };
                for cull in [None, Some((x + 40, y + 20, x + 300, y + 200))] {
                    camera.cull = cull;
                    for frame in [0, 7] {
                        let got = gpu.render(&mut renderer, &camera, frame, CLEAR);
                        let want = reference::render(&world, &atlas, &camera, frame, CLEAR);
                        assert_same(&got, &want, &format!("1/{n} at ({x}, {y}), cull {cull:?}, frame {frame}, {color:?}"));
                    }
                }
            }
        }
        let overhang = (0, 40, 0, 0);
        let main = Camera { x: 50, y: 7, zoom: (3, 2), overhang, ..Camera::new((400, 240)) };
        let inset = Camera { x: -20, y: 30, zoom: (1, 2), downscale: 2, origin: (-30, 150), overhang, ..Camera::new((160, 100)) };
        let magnifier = Camera { x: 1500, y: 90, zoom: (2, 1), origin: (250, 10), overhang, ..Camera::new((121, 80)) };
        let cameras = [main, inset, magnifier];
        let got = gpu.render_views(&mut renderer, &cameras, (400, 240), 3, CLEAR);
        let want = reference::render_views(&world, &atlas, &cameras, (400, 240), 3, CLEAR);
        assert_same(&got, &want, &format!("zoomed views, {color:?}"));
    }
}

#[test]
fn per_cell_offsets_move_cells_in_their_order() {
    // Two cells of one row in a pass swept right to left: the right one is
    // drawn first; offsets move them (halved and rounded down at 1/2), the
    // frame is the phase whatever the clock, and a cell moved in from outside
    // the viewport is drawn.
    let mut b = AtlasBuilder::new(64);
    let frames: Vec<_> = (0..3u8).map(|i| b.add(&RgbaImage::from_fn(4, 4, |_, _| [i * 80, 0, 0, 255]), (0, 0)).unwrap()).collect();
    let mut sets = SpriteSets::default();
    let set = sets.add(&frames);
    let atlas = b.finish();
    let grid = Grid { cols: 8, rows: 2, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (16, 16) };
    let mut world = TileWorld::new(grid, sets);
    let l = world.add_layer(LayerSpec { descending: true, per_cell_offset: true, per_cell_phase: true, ticks_per_frame: 0, ..LayerSpec::new("units", vec![tile_render::SpriteSetId::EMPTY, set]) });
    for (c, off, phase) in [(1, (3, -5), 2), (2, (-7, 9), 1), (7, (-70, 0), 0)] {
        world.set(l, c, 0, 1);
        world.set_offset(l, c, 0, off);
        world.set_phase(l, c, 0, phase);
    }
    assert_eq!(world.offset(l, 2, 0), (-7, 9));
    assert_eq!(world.offset_reach(), (70, 5, 3, 9));
    let camera = Camera::new((64, 32));
    let draws = reference::draw_list(&world, &atlas, &camera, 1000);
    let at: Vec<_> = draws.iter().map(|d| (d.sprite, d.x, d.y)).collect();
    // Cell 7 (origin 112, moved to 42) is outside the viewport's cells but
    // drawn, first: the row is swept from its right.
    assert_eq!(at, vec![(frames[0], 42, 0), (frames[1], 25, 9), (frames[2], 19, -5)]);
    let half = reference::draw_list(&world, &atlas, &Camera { downscale: 2, ..camera }, 1000);
    let at: Vec<_> = half.iter().map(|d| (d.x, d.y)).collect();
    assert_eq!(at, vec![(56 - 35, 0), (16 - 4, 4), (8 + 1, -3)]);
}

/// An atlas uploaded in parts (the first sprites, then the rest appended,
/// over several texture replacements) draws what the whole atlas does.
#[test]
fn a_grown_atlas_draws_what_the_whole_one_does() {
    let gpu = Headless::new().expect("GPU");
    let grid = Grid { cols: 10, rows: 14, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let (mut world, whole) = scene_with(grid, true);
    world.color = ColorModel::Quantized { bits: 5 };
    // The same sprites again on small pages: the first three in one atlas,
    // the others appended one at a time.
    let image = |s: &tile_render_shared::SpriteDesc| whole.pages[s.page as usize].crop(s.atlas_x, s.atlas_y, s.width, s.height);
    let mut first = AtlasBuilder::new(160);
    for s in &whole.sprites[..3] {
        let id = first.add(&image(s), (s.anchor_x, s.anchor_y)).unwrap();
        let d = first.sprite_mut(id);
        (d.scaling, d.box_x, d.box_y, d.box_w, d.box_h, d.downscaled) = (s.scaling, s.box_x, s.box_y, s.box_w, s.box_h, s.downscaled);
    }
    let mut shelves = first.shelves().0.clone();
    let mut atlas = first.finish();
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    for (i, s) in whole.sprites.iter().enumerate().skip(3) {
        let at = shelves.place(s.width, s.height).unwrap();
        let sprite = tile_render_shared::SpriteDesc { page: at.0, atlas_x: at.1, atlas_y: at.2, ..*s };
        let growth = tile_render::AtlasGrowth {
            first: i as u32,
            pages: shelves.pages,
            index_pages: 0,
            sprites: vec![sprite],
            blits: vec![tile_render::Blit { page: at.0, x: at.1, y: at.2, pixels: tile_render::BlitPixels::Rgba(image(s)) }],
            palettes: Vec::new(),
        };
        renderer.grow_atlas(&gpu.device, &gpu.queue, &growth);
        atlas.grow(&growth);
    }
    assert!(atlas.pages.len() > 4, "the test should span several pages ({})", atlas.pages.len());
    renderer.set_world(&gpu.device, &world);
    let mut camera = Camera::new((480, 300));
    camera.x = 100;
    camera.y = 37;
    for frame in [0, 5] {
        let got = gpu.render(&mut renderer, &camera, frame, CLEAR);
        assert_same(&got, &reference::render(&world, &whole, &camera, frame, CLEAR), &format!("frame {frame}, against the whole atlas"));
        assert_same(&got, &reference::render(&world, &atlas, &camera, frame, CLEAR), &format!("frame {frame}, against the grown atlas"));
    }
}
