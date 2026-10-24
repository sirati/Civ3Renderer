//! Indexed sprites (one byte a texel, drawn through palette rows) next to
//! colour sprites, in the same passes: the GPU renderer must produce exactly
//! the reference renderer's pixels, on tile layers and as free sprites,
//! with every blend, in both colour models, shrunk and zoomed.

use tile_render::{
    Atlas, AtlasBuilder, Blend, Camera, ColorModel, Grid, Headless, IndexImage, LayerSpec, Layout, PlacedSprite, RgbaImage, SpriteSetId, SpriteSets, TileRenderer, TileWorld, reference,
};
use tile_render_shared::{PALETTE_SIZE, remap_range};

const CLEAR: [u8; 4] = [10, 20, 30, 255];
/// Named palettes after the own rows.
const NAMED: u32 = 3;

/// A colour diamond (the colour sprites' terrain).
fn diamond(seed: u8) -> RgbaImage {
    RgbaImage::from_fn(128, 64, |x, y| {
        let d = (x as i32 * 2 - 127).abs() as f32 / 128.0 + (y as i32 * 2 - 63).abs() as f32 / 64.0;
        if d > 1.0 { [0; 4] } else { [seed.wrapping_mul(37), (x as u8).wrapping_add(seed), (y as u8).wrapping_mul(3), 255] }
    })
}

/// An indexed figure taller than a cell: index 255 around it (clear), a
/// border of 1, its body striped through every range a palette row has:
/// 2..64 (remapped: a nation's colours), 64..240 (its own), 240..248
/// (translucent), 248..255 (shadow strengths).
fn figure(seed: u32) -> IndexImage {
    IndexImage::from_fn(52, 90, |x, y| {
        let (cx, cy) = (x as i32 - 26, y as i32 - 45);
        let r = cx * cx * 4 + cy * cy;
        if r > 2000 {
            255
        } else if r > 1700 {
            1
        } else {
            (match (x + 2 * y + seed) % 9 {
                0 | 1 => 2 + (x * 7 + y + seed) % 62,
                2..=5 => 64 + (x * 3 + y * 5 + seed) % 176,
                6 => 240 + (x + y) % 8,
                _ => 248 + (x + seed) % 7,
            }) as u8
        }
    })
}

/// A sprite's own colours: clear 0 and 255, black 1, 2..240 colours from
/// `seed`, 240..248 at 50 % (the translucency a quantized model mixes
/// exactly), 248..255 as a scale blend reads them
/// (shadow `k = index − 239` in red), or (`shadow`) clear but those.
fn own_row(seed: u32, shadow: bool) -> Vec<u32> {
    (0..PALETTE_SIZE)
        .map(|i| {
            let c: [u8; 4] = match i {
                248..=254 => [(i - 239) as u8, 0, 0, 255],
                _ if shadow => [0; 4],
                0 | 255 => [0; 4],
                1 => [0, 0, 0, 255],
                240..=247 => [(i * 13 + seed) as u8, (i * 7) as u8, 200, 128],
                _ => [(i * 5 + seed) as u8, (i * 11 + 2 * seed) as u8, (i * 3 + 90) as u8, 255],
            };
            u32::from_le_bytes(c)
        })
        .collect()
}

/// A named palette (a nation's colours) for remapped indices.
fn named_row(k: u32) -> Vec<u32> {
    (0..PALETTE_SIZE).map(|i| u32::from_le_bytes([(40 + 70 * k) as u8, (i * 4) as u8, 255 - ((i * 3 + 30 * k) % 256) as u8, 255])).collect()
}

struct Sets {
    terrain: Vec<SpriteSetId>,
    /// A colour sprite of what `single` draws with named palette 2.
    twin: SpriteSetId,
    /// Indexed: an animation, its shadow (the same pixels), one frame.
    figure: SpriteSetId,
    shadow: SpriteSetId,
    single: SpriteSetId,
}

/// Colour terrain under indexed figures (remapped through the cells' named
/// palettes) and their shadows, over a colour layer in the same pass; with
/// `free`, indexed and colour free sprites in every blend.
fn scene(grid: Grid, free: bool) -> (TileWorld, Atlas, Sets) {
    let mut b = AtlasBuilder::new(512);
    let mut sets = SpriteSets::default();
    let terrain: Vec<_> = (0..4u8).map(|i| sets.single(b.add(&diamond(i * 50), (0, 0)).unwrap())).collect();
    // Own rows 1 and 2 (figures), 3 (shadows).
    let frames: Vec<_> = (0..4u32).map(|k| b.add_indexed(&figure(k * 3), (38, -50), 1 + k % 2, remap_range(2, 64)).unwrap()).collect();
    let shadows: Vec<_> = frames.iter().map(|&f| b.alias(f, (44, -48))).collect();
    for &s in &shadows {
        b.sprite_mut(s).palette = 3;
        b.sprite_mut(s).remap = 0;
    }
    let figure_set = sets.add(&frames);
    let shadow_set = sets.add(&shadows);
    let single = sets.single(frames[2]);
    let (own, named) = (own_row(0, false), named_row(1));
    let resolved = RgbaImage::from_fn(52, 90, |x, y| {
        let i = figure(6).get(x, y) as usize;
        (if (2..64).contains(&i) { named[i] } else { own[i] }).to_le_bytes()
    });
    let twin = sets.single(b.add(&resolved, (38, -50)).unwrap());
    let mut atlas = b.finish();
    let mut palettes = tile_render::atlas::no_palettes();
    palettes.extend(own_row(0, false));
    palettes.extend(own_row(77, false));
    palettes.extend(own_row(0, true));
    atlas.named_palettes = (palettes.len() / PALETTE_SIZE as usize) as u32;
    for k in 0..NAMED {
        palettes.extend(named_row(k));
    }
    atlas.palettes = palettes;

    let mut world = TileWorld::new(grid, sets);
    let ground = world.add_layer(LayerSpec::new("terrain", terrain.clone()));
    // A colour layer and an indexed one in one pass, cells picking named
    // palettes (0: the figures' own colours).
    let over = world.add_layer(LayerSpec::new("over", vec![SpriteSetId::EMPTY, terrain[3]]));
    let mut shadow = LayerSpec { pass: 1, blend: Blend::Scale { denominator: 16 }, ..LayerSpec::new("shadow", vec![SpriteSetId::EMPTY, shadow_set]) };
    (shadow.per_cell_phase, shadow.ticks_per_frame) = (true, 0);
    let shadow = world.add_layer(shadow);
    let mut figures = LayerSpec { pass: 1, ..LayerSpec::new("figures", vec![SpriteSetId::EMPTY, figure_set]) };
    (figures.per_cell_phase, figures.ticks_per_frame, figures.palette_bits) = (true, 0, 2);
    let figures = world.add_layer(figures);
    let colour = world.add_layer(LayerSpec { pass: 1, ..LayerSpec::new("colour", vec![SpriteSetId::EMPTY, terrain[1]]) });
    let spec = world.layers()[figures.0].clone();
    for r in 0..grid.rows {
        for c in 0..grid.cols {
            world.set(ground, c, r, (c * 3 + r * 7) % 4);
            if (c + r) % 5 == 1 {
                world.set(over, c, r, 1);
            }
            if (c + 2 * r) % 3 == 0 {
                let phase = ((c * r) % 4) as u8;
                world.set(shadow, c, r, 1);
                world.set_phase(shadow, c, r, phase);
                world.set(figures, c, r, spec.value_in(1, (c + r) % (NAMED + 1)).unwrap());
                world.set_phase(figures, c, r, phase);
            } else if (c * r) % 7 == 3 {
                world.set(colour, c, r, 1);
            }
        }
    }
    if free {
        let fig = |x, y, palette, blend, after_pass| PlacedSprite { palette, blend, after_pass, ..PlacedSprite::new(single, x, y) };
        world.free.push(fig(150, 70, 1, Blend::Alpha, 0));
        world.free.push(fig(190, 80, 0, Blend::Alpha, 1));
        world.free.push(PlacedSprite { after_pass: 1, blend: Blend::Scale { denominator: 16 }, ..PlacedSprite::new(shadow_set, 120, 60) });
        world.free.push(fig(220, 95, 3, Blend::Invert, usize::MAX));
        world.free.push(PlacedSprite { screen: true, palette: 2, ..PlacedSprite::new(single, 40, 30) });
        world.free.push(PlacedSprite { screen: true, ..PlacedSprite::new(terrain[2], 90, 40) });
        world.free.push(PlacedSprite { screen: true, palette: 1, clip: Some([60, 40, 110, 90]), ..PlacedSprite::new(figure_set, 70, 50) });
        // (Past the named palettes: its own colours, as row 0 would not.)
        world.free.push(PlacedSprite { screen: true, palette: 9, ..PlacedSprite::new(single, 170, 120) });
    }
    (world, atlas, Sets { terrain, twin, figure: figure_set, shadow: shadow_set, single })
}

/// For the direct colour model, opaque art only (as the colour sprites'
/// tests do: the GPU's 8-bit blender rounds translucency its own way): the
/// shadow layer hidden, translucent palette entries made opaque.
fn direct_only(world: &mut TileWorld, atlas: &mut Atlas) {
    world.layer_mut(tile_render::LayerId(2)).hidden = true;
    for c in &mut atlas.palettes {
        if *c >> 24 != 0 {
            *c |= 0xFF << 24;
        }
    }
}

fn assert_same(gpu: &RgbaImage, cpu: &RgbaImage, what: &str) {
    assert_eq!((gpu.width, gpu.height), (cpu.width, cpu.height));
    let diff: Vec<_> = gpu.pixels.iter().zip(&cpu.pixels).enumerate().filter(|(_, (a, b))| a != b).collect();
    if let Some((i, (g, c))) = diff.first() {
        let (x, y) = (*i as u32 % gpu.width, *i as u32 / gpu.width);
        panic!("{what}: {} pixels differ, first at ({x}, {y}): gpu {g:?} cpu {c:?}", diff.len());
    }
}

const GRID: Grid = Grid { cols: 12, rows: 20, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };

#[test]
fn indexed_and_colour_sprites_are_pixel_exact_together() {
    // Tile layers and free sprites, both kinds in one pass, every blend; in
    // the quantized model (blends exact) and the direct one (without the
    // blends the GPU's 8-bit blender rounds its own way).
    let gpu = Headless::new().expect("GPU");
    for (color, free) in [(ColorModel::Quantized { bits: 5 }, true), (ColorModel::Direct, false)] {
        let (mut world, mut atlas, _) = scene(GRID, free);
        world.color = color;
        if color == ColorModel::Direct {
            direct_only(&mut world, &mut atlas);
        }
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
        renderer.set_atlas(&gpu.device, &gpu.queue, &trimmed(&atlas));
        renderer.set_world(&gpu.device, &world);
        let mut drawn = 0;
        for (x, y) in [(0, 0), (333, 101), (-517, 40), (77, 400)] {
            let camera = Camera { x, y, overhang: (0, 64, 0, 0), ..Camera::new((400, 240)) };
            for frame in [0, 5] {
                let got = gpu.render(&mut renderer, &camera, frame, CLEAR);
                let want = reference::render(&world, &atlas, &camera, frame, CLEAR);
                assert_same(&got, &want, &format!("({x}, {y}) frame {frame}, {color:?}"));
                drawn += 1;
            }
        }
        assert_eq!(drawn, 8);
    }
}

#[test]
fn indexed_sprites_pick_their_colours_by_cell_and_sprite() {
    // The remapped indices take the named palette the cell or sprite picks,
    // the others the sprite's own row; picking none draws them all in its
    // own colours. (Checked on the reference, which the GPU matches.)
    let (mut world, atlas, sets) = scene(GRID, false);
    world.color = ColorModel::Direct;
    let draw = |world: &TileWorld, palette: u32| {
        let mut w = world.clone();
        for l in 0..w.layers().len() {
            w.layer_mut(tile_render::LayerId(l)).hidden = true;
        }
        w.free = vec![PlacedSprite { screen: true, palette, ..PlacedSprite::new(sets.single, -38, 50) }];
        reference::render(&w, &atlas, &Camera::new((52, 90)), 0, [0, 0, 0, 255])
    };
    let image = figure(6);
    let own = own_row(0, false);
    for palette in 0..=NAMED {
        let got = draw(&world, palette);
        for (i, p) in image.pixels.iter().enumerate() {
            let (x, y) = (i as u32 % 52, i as u32 / 52);
            let index = *p as usize;
            let row = if palette > 0 && (2..64).contains(&index) { named_row(palette - 1) } else { own.clone() };
            let [r, g, b, a] = row[index].to_le_bytes();
            if a == 255 {
                assert_eq!(got.get(x, y), [r, g, b, 255], "palette {palette}, index {index} at ({x}, {y})");
            }
        }
    }
    let _ = (&sets.terrain, sets.figure, sets.shadow);
}

#[test]
fn indexed_sprites_shrink_and_zoom_like_colour_ones() {
    // Views showing the world 2 times smaller (sample boxes, kept sizes and
    // anchors, a downscaled twin of the other kind), zoomed in and out, and
    // drawn at their zoom: both kinds, both colour models.
    use tile_render_shared::{KEEP_SIZE, SHRINK_KEEP_ANCHOR};
    let gpu = Headless::new().expect("GPU");
    for (color, free) in [(ColorModel::Quantized { bits: 5 }, true), (ColorModel::Direct, false)] {
        let (mut world, mut atlas, sets) = scene(GRID, free);
        world.color = color;
        if color == ColorModel::Direct {
            direct_only(&mut world, &mut atlas);
        }
        // Boxes at odd places, one frame keeping its anchor, one its size,
        // one drawing a colour sprite instead (and a colour terrain tile an
        // indexed one).
        for (k, (x, y, w, h)) in [(3, 5, 45, 77), (0, 1, 52, 87), (7, 2, 41, 80), (2, 2, 47, 81)].into_iter().enumerate() {
            let id = tile_render::SpriteId(4 + k as u32);
            let s = &mut atlas.sprites[id.0 as usize];
            (s.box_x, s.box_y, s.box_w, s.box_h) = (x, y, w, h);
            s.scaling = [0, SHRINK_KEEP_ANCHOR, KEEP_SIZE, 0][k];
        }
        atlas.sprites[5].downscaled = 1 + 1;
        atlas.sprites[3].downscaled = 6 + 1;
        for t in 0..4 {
            let s = &mut atlas.sprites[t];
            (s.box_x, s.box_w) = (1, 126);
        }
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
        renderer.set_atlas(&gpu.device, &gpu.queue, &trimmed(&atlas));
        renderer.set_world(&gpu.device, &world);
        for (x, y) in [(0, 0), (167, 51), (-259, 20)] {
            let camera = Camera { x, y, downscale: 2, overhang: (0, 64, 0, 0), ..Camera::new((400, 240)) };
            let got = gpu.render(&mut renderer, &camera, 5, CLEAR);
            let want = reference::render(&world, &atlas, &camera, 5, CLEAR);
            assert_same(&got, &want, &format!("1/2 at ({x}, {y}), {color:?}"));
        }
        // Zoomed: a main view at 3/2, an inset at 1/2 shown shrunk, a
        // magnifier at 3/1 drawn at its zoom (hairlines kept).
        let overhang = (0, 64, 0, 0);
        let main = Camera { x: 50, y: 7, zoom: (3, 2), overhang, ..Camera::new((400, 240)) };
        let inset = Camera { x: 20, y: 30, zoom: (1, 2), downscale: 2, origin: (-30, 150), overhang, ..Camera::new((160, 100)) };
        let magnifier = Camera { x: 140, y: 90, zoom: (3, 1), origin: (250, 10), overhang, hairline: true, ..Camera::new((121, 80)) };
        let cameras = [main, inset, magnifier];
        let got = gpu.render_views(&mut renderer, &cameras, (400, 240), 3, CLEAR);
        let want = reference::render_views(&world, &atlas, &cameras, (400, 240), 3, CLEAR);
        assert_same(&got, &want, &format!("zoomed views, {color:?}"));
        let _ = (sets.figure, sets.shadow, sets.single);
    }
}

#[test]
fn indexed_sprites_blend_as_their_colour_twins() {
    // Subpixel (and every other) blend of an indexed sprite draws what the
    // colour sprite of its resolved texels draws, on the GPU and in the
    // reference, over the world and over each other.
    let gpu = Headless::new().expect("GPU");
    let (mut world, atlas, sets) = scene(GRID, true);
    world.color = ColorModel::Quantized { bits: 5 };
    let blends = [Blend::Subpixel { color: [250, 30, 90] }, Blend::Alpha, Blend::Invert, Blend::Subpixel { color: [0, 0, 0] }, Blend::Scale { denominator: 32 }];
    let drawn = |set: SpriteSetId, palette: u32| {
        let mut w = world.clone();
        for (i, blend) in blends.into_iter().enumerate() {
            w.free.push(PlacedSprite { screen: true, palette, blend, ..PlacedSprite::new(set, 60 + 31 * i as i32, 40 + 17 * i as i32) });
        }
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
        renderer.set_atlas(&gpu.device, &gpu.queue, &trimmed(&atlas));
        renderer.set_world(&gpu.device, &w);
        let camera = Camera { overhang: (0, 64, 0, 0), ..Camera::new((400, 240)) };
        (gpu.render(&mut renderer, &camera, 2, CLEAR), reference::render(&w, &atlas, &camera, 2, CLEAR))
    };
    let (gpu_indexed, ref_indexed) = drawn(sets.single, 2);
    let (gpu_twin, ref_twin) = drawn(sets.twin, 0);
    assert_same(&gpu_indexed, &gpu_twin, "GPU: indexed and colour twin");
    assert_same(&ref_indexed, &ref_twin, "reference: indexed and colour twin");
    // Over a plain background the subpixel blend is exact too.
    let (mut world, atlas, sets) = scene(GRID, false);
    world.color = ColorModel::Quantized { bits: 5 };
    for l in 0..world.layers().len() {
        world.layer_mut(tile_render::LayerId(l)).hidden = true;
    }
    world.free.push(PlacedSprite { screen: true, palette: 2, blend: Blend::Subpixel { color: [250, 30, 90] }, ..PlacedSprite::new(sets.single, 50, 60) });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_atlas(&gpu.device, &gpu.queue, &trimmed(&atlas));
    renderer.set_world(&gpu.device, &world);
    let camera = Camera::new((200, 160));
    assert_same(&gpu.render(&mut renderer, &camera, 0, CLEAR), &reference::render(&world, &atlas, &camera, 0, CLEAR), "subpixel indexed sprite");
}


/// `atlas` with its sprites' blank margins found, as compiled descriptions
/// have them: the GPU leaves them out of the rectangles it draws, which
/// must not change a pixel.
fn trimmed(atlas: &tile_render::Atlas) -> tile_render::Atlas {
    let mut a = atlas.clone();
    a.trim_blank();
    a
}

/// The viewer's shader module (it holds the tile renderer's shaders).
const SHADERS: tile_render::Shaders = tile_render::Shaders::new(viewer_spirv::SPIRV, viewer_spirv::SPIRV_FALLBACK);
