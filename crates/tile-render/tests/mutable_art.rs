//! Mutable art and cell-only updates must preserve the full scene's pixels.
use tile_render::{
    Atlas, AtlasBuilder, AtlasGrowth, Camera, ColorModel, Dirty, Grid, Headless, IndexImage,
    LayerSpec, Layout, PlacedSprite, RgbaImage, SpriteId, SpriteSetId, SpriteSets, TileRenderer,
    TileWorld, ZoomArt, reference,
};

const CLEAR: [u8; 4] = [16, 24, 32, 255];

fn world(sprites: &[SpriteId]) -> TileWorld {
    let grid = Grid {
        cols: 1,
        rows: 1,
        layout: Layout::Orthogonal,
        wrap_x: false,
        wrap_y: false,
        step: (8, 8),
    };
    let mut sets = SpriteSets::default();
    let ids: Vec<_> = sprites.iter().map(|&id| sets.single(id)).collect();
    let mut world = TileWorld::new(grid, sets);
    world.color = ColorModel::Quantized { bits: 5 };
    for set in ids {
        world.free.extend([PlacedSprite::new(set, 0, 0); 2]);
    }
    world
}

fn renderer(gpu: &Headless, atlas: &Atlas, world: &TileWorld) -> TileRenderer {
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, atlas);
    renderer.set_world(&gpu.device, world);
    renderer
}

fn check(
    gpu: &Headless,
    renderer: &mut TileRenderer,
    atlas: &Atlas,
    world: &TileWorld,
    camera: &Camera,
) {
    let got = gpu.render(renderer, camera, 0, CLEAR);
    let want = reference::render(world, atlas, camera, 0, CLEAR);
    assert_eq!((got.width, got.height), (want.width, want.height));
    for (i, (got, want)) in got.pixels.iter().zip(&want.pixels).enumerate() {
        assert_eq!(
            got,
            want,
            "pixel ({}, {})",
            i % camera.viewport.0 as usize,
            i / camera.viewport.0 as usize
        );
    }
}

#[test]
fn downscaled_twins_keep_repeated_alpha_blending() {
    let gpu = Headless::new().expect("AMD GPU");
    let mut b = AtlasBuilder::new(64);
    let original = b
        .add(&RgbaImage::from_fn(8, 8, |_, _| [240, 40, 8, 255]), (0, 0))
        .unwrap();
    let twin = b
        .add(&RgbaImage::from_fn(8, 8, |_, _| [240, 40, 8, 128]), (0, 0))
        .unwrap();
    b.sprite_mut(original).downscaled = twin.0 + 1;
    let mut atlas = b.finish();
    atlas.trim_blank();
    let world = world(&[original]);
    let mut renderer = renderer(&gpu, &atlas, &world);
    check(
        &gpu,
        &mut renderer,
        &atlas,
        &world,
        &Camera {
            downscale: 2,
            ..Camera::new((32, 32))
        },
    );
}

#[test]
fn patches_refresh_aliases_and_restore_covered_draws() {
    let gpu = Headless::new().expect("AMD GPU");
    let mut b = AtlasBuilder::new(64);
    let large = b
        .add(
            &RgbaImage::from_fn(16, 16, |x, y| {
                if (6..10).contains(&x) && (6..10).contains(&y) {
                    [240, 40, 8, 255]
                } else {
                    [0; 4]
                }
            }),
            (0, 0),
        )
        .unwrap();
    let desc = *b.sprite(large);
    let patch = b.add_desc(tile_render_shared::SpriteDesc {
        atlas_x: desc.atlas_x + 4,
        atlas_y: desc.atlas_y + 4,
        width: 8,
        height: 8,
        box_w: 8,
        box_h: 8,
        ..desc
    });
    let alias = b.alias(patch, (0, 0));
    let mut atlas = b.finish();
    atlas.trim_blank();
    let mut world = world(&[alias]);
    let large_set = world.sets.single(large);
    world.free.push(PlacedSprite::new(large_set, 12, 0));
    let mut renderer = renderer(&gpu, &atlas, &world);
    let camera = Camera::new((32, 32));
    check(&gpu, &mut renderer, &atlas, &world, &camera);
    for alpha in [128, 255] {
        let pixels = RgbaImage::from_fn(8, 8, |_, _| [40, 200, 80, alpha]);
        assert!(atlas.patch(patch, &pixels));
        assert!(renderer.patch_sprite(&gpu.queue, patch, &pixels));
        check(&gpu, &mut renderer, &atlas, &world, &camera);
    }
}

#[test]
fn own_palette_alpha_changes_expose_margins_and_restore_draws() {
    let gpu = Headless::new().expect("AMD GPU");
    let mut b = AtlasBuilder::new(64);
    let id = b
        .add_indexed(
            &IndexImage::from_fn(8, 8, |x, y| {
                u8::from((2..6).contains(&x) && (2..6).contains(&y))
            }),
            (0, 0),
            1,
            0,
        )
        .unwrap();
    let mut atlas = b.finish();
    atlas.palettes = vec![0; 512];
    atlas.palettes[257] = u32::from_le_bytes([240, 40, 8, 255]);
    atlas.named_palettes = 2;
    atlas.trim_blank();
    let world = world(&[id]);
    let mut renderer = renderer(&gpu, &atlas, &world);
    let camera = Camera::new((32, 32));
    check(&gpu, &mut renderer, &atlas, &world, &camera);
    atlas.palettes[256] = u32::from_le_bytes([8, 40, 240, 255]);
    atlas.palettes[257] = u32::from_le_bytes([240, 40, 8, 128]);
    renderer.set_palettes(&gpu.device, &gpu.queue, &atlas.palettes);
    check(&gpu, &mut renderer, &atlas, &world, &camera);
}

#[test]
fn named_palette_alpha_survives_changes_growth_and_removed_rows() {
    let gpu = Headless::new().expect("AMD GPU");
    let mut b = AtlasBuilder::new(64);
    let id = b
        .add_indexed(
            &IndexImage::from_fn(8, 8, |_, _| 1),
            (0, 0),
            1,
            tile_render::remap_range(1, 2),
        )
        .unwrap();
    let mut atlas = b.finish();
    atlas.palettes = vec![0; 256];
    atlas
        .palettes
        .extend(vec![u32::from_le_bytes([240, 40, 8, 255]); 512]);
    atlas.named_palettes = 2;
    atlas.trim_blank();
    let mut world = world(&[id]);
    for s in &mut world.free {
        s.palette = 1;
    }
    let mut renderer = renderer(&gpu, &atlas, &world);
    let camera = Camera::new((32, 32));
    check(&gpu, &mut renderer, &atlas, &world, &camera);
    atlas.palettes[513] = u32::from_le_bytes([40, 200, 80, 128]);
    renderer.set_palettes(&gpu.device, &gpu.queue, &atlas.palettes);
    check(&gpu, &mut renderer, &atlas, &world, &camera);
    let growth = AtlasGrowth {
        first: atlas.sprites.len() as u32,
        pages: 0,
        index_pages: 1,
        palettes: vec![0xFF00_0000; 256],
        ..Default::default()
    };
    atlas.grow(&growth);
    renderer.grow_atlas(&gpu.device, &gpu.queue, &growth);
    check(&gpu, &mut renderer, &atlas, &world, &camera);
    atlas.palettes.truncate(512);
    renderer.set_palettes(&gpu.device, &gpu.queue, &atlas.palettes);
    check(&gpu, &mut renderer, &atlas, &world, &camera);
}

#[test]
fn replacing_an_atlas_restores_previously_covered_draws() {
    let gpu = Headless::new().expect("AMD GPU");
    let mut b = AtlasBuilder::new(64);
    let id = b
        .add(&RgbaImage::from_fn(8, 8, |_, _| [240, 40, 8, 255]), (0, 0))
        .unwrap();
    let mut atlas = b.finish();
    atlas.trim_blank();
    let world = world(&[id]);
    let mut renderer = renderer(&gpu, &atlas, &world);
    let camera = Camera::new((32, 32));
    check(&gpu, &mut renderer, &atlas, &world, &camera);
    assert!(atlas.patch(id, &RgbaImage::from_fn(8, 8, |_, _| [240, 40, 8, 128])));
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    check(&gpu, &mut renderer, &atlas, &world, &camera);
}

#[test]
fn cell_updates_rebuild_changed_tables_even_when_record_size_is_unchanged() {
    let gpu = Headless::new().expect("AMD GPU");
    let mut b = AtlasBuilder::new(64);
    let red = b
        .add(&RgbaImage::from_fn(8, 8, |_, _| [240, 40, 8, 255]), (0, 0))
        .unwrap();
    let blue = b
        .add(&RgbaImage::from_fn(8, 8, |_, _| [8, 40, 240, 255]), (0, 0))
        .unwrap();
    let atlas = b.finish();
    let mut world = world(&[]);
    let red_set = world.sets.single(red);
    let blue_set = world.sets.single(blue);
    let first = world.add_layer(LayerSpec::new("first", vec![SpriteSetId::EMPTY, red_set]));
    world.set(first, 0, 0, 1);
    let mut renderer = renderer(&gpu, &atlas, &world);
    let camera = Camera::new((32, 32));
    let mut top = LayerSpec::new("second", vec![SpriteSetId::EMPTY, blue_set]);
    top.pass = 1;
    let second = world.add_layer(top);
    world.set(second, 0, 0, 1);
    renderer.update_cells(&gpu.device, &gpu.queue, &world, &Dirty::All);
    check(&gpu, &mut renderer, &atlas, &world, &camera);
    let mut swapped = SpriteSets::default();
    assert_eq!(swapped.single(blue), red_set);
    assert_eq!(swapped.single(red), blue_set);
    world.sets = swapped;
    renderer.update_cells(&gpu.device, &gpu.queue, &world, &Dirty::None);
    check(&gpu, &mut renderer, &atlas, &world, &camera);
}

#[test]
fn marker_only_zoomed_art_is_drawn_and_removed() {
    let gpu = Headless::new().expect("AMD GPU");
    let mut b = AtlasBuilder::new(64);
    let id = b
        .add(&RgbaImage::from_fn(8, 8, |_, _| [240, 40, 8, 255]), (0, 0))
        .unwrap();
    let atlas = b.finish();
    let mut world = world(&[]);
    let set = world.sets.single(id);
    let marker = PlacedSprite {
        zoom_art: ZoomArt::Zoomed {
            zoom: (2, 1),
            offset: (0, 0),
        },
        ..PlacedSprite::new(set, 2, 2)
    };
    let mut renderer = renderer(&gpu, &atlas, &world);
    let camera = Camera {
        zoom: (2, 1),
        zoomed_art: true,
        ..Camera::new((32, 32))
    };
    renderer.set_markers(&gpu.device, &gpu.queue, &[marker]);
    let mut marked = world.clone();
    marked.free.push(marker);
    check(&gpu, &mut renderer, &atlas, &marked, &camera);
    renderer.set_markers(&gpu.device, &gpu.queue, &[]);
    check(&gpu, &mut renderer, &atlas, &world, &camera);
    renderer.set_markers(&gpu.device, &gpu.queue, &[marker]);
    renderer.set_world(&gpu.device, &world);
    check(&gpu, &mut renderer, &atlas, &marked, &camera);
}

#[test]
fn palette_growth_restores_draws_for_newly_available_named_rows() {
    let gpu = Headless::new().expect("AMD GPU");
    let mut b = AtlasBuilder::new(64);
    let id = b
        .add_indexed(
            &IndexImage::from_fn(8, 8, |_, _| 1),
            (0, 0),
            1,
            tile_render::remap_range(1, 2),
        )
        .unwrap();
    let mut atlas = b.finish();
    atlas.palettes = vec![0; 256];
    atlas.palettes.extend(vec![0xFF00_0000; 512]);
    atlas.named_palettes = 2;
    atlas.trim_blank();
    let mut world = world(&[id]);
    for s in &mut world.free {
        s.palette = 2;
    }
    let mut renderer = renderer(&gpu, &atlas, &world);
    let camera = Camera::new((32, 32));
    check(&gpu, &mut renderer, &atlas, &world, &camera);
    let growth = AtlasGrowth {
        first: atlas.sprites.len() as u32,
        pages: 0,
        index_pages: 1,
        palettes: vec![u32::from_le_bytes([240, 40, 8, 128]); 256],
        ..Default::default()
    };
    atlas.grow(&growth);
    renderer.grow_atlas(&gpu.device, &gpu.queue, &growth);
    check(&gpu, &mut renderer, &atlas, &world, &camera);
}

#[test]
fn changed_font_faces_rebuild_even_without_dirty_cells() {
    use std::sync::Arc;
    use tile_render::{GlyphShape, OutlineGlyph, gpu_text::Face};
    let gpu = Headless::new().expect("AMD GPU");
    let face = |width: f64| {
        let points = [(0.0, 0.0), (0.0, 700.0), (width, 700.0), (width, 0.0)];
        let curves = (0..4)
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % 4]);
                [a, ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5), b]
            })
            .collect();
        Face::from_curves(1000, &[('x', curves, 1000)])
    };
    let atlas = AtlasBuilder::new(64).finish();
    let mut world = world(&[]);
    world.fonts.push(Arc::new(face(200.0)));
    world.free.push(PlacedSprite {
        text: Some(OutlineGlyph {
            font: 0,
            shape: GlyphShape::Glyph(0),
            em64: 16 * 64,
            advance: 16,
            bold: false,
            italic: false,
            color: [240, 240, 240],
            key: None,
        }),
        ..PlacedSprite::new(SpriteSetId::EMPTY, 4, 20)
    });
    let mut renderer = renderer(&gpu, &atlas, &world);
    let camera = Camera::new((32, 32));
    check(&gpu, &mut renderer, &atlas, &world, &camera);
    *Arc::make_mut(&mut world.fonts[0]) = face(800.0);
    renderer.update_cells(&gpu.device, &gpu.queue, &world, &Dirty::None);
    check(&gpu, &mut renderer, &atlas, &world, &camera);
}

#[test]
fn staged_upload_keeps_the_cached_picture_until_adoption() {
    let gpu = Headless::new().expect("AMD GPU");
    let make = |color| {
        let mut b = AtlasBuilder::new(8);
        let id = b
            .add(&RgbaImage::from_fn(8, 8, |_, _| color), (0, 0))
            .unwrap();
        // Two pages let a one-byte budget upload only part of the atlas.
        b.add(&RgbaImage::from_fn(8, 8, |_, _| [40, 200, 80, 255]), (0, 0))
            .unwrap();
        let mut atlas = b.finish();
        atlas.trim_blank();
        (atlas, id)
    };
    let (before, id) = make([240, 40, 8, 255]);
    let (after, next_id) = make([8, 40, 240, 255]);
    assert_eq!(id, next_id);
    let world = world(&[id]);
    let mut renderer = renderer(&gpu, &before, &world);
    let camera = Camera::new((32, 32));
    check(&gpu, &mut renderer, &before, &world, &camera);
    assert!(!renderer.reused());
    check(&gpu, &mut renderer, &before, &world, &camera);
    assert!(renderer.reused());

    let mut staged = renderer.stage_atlas(&gpu.device, &after);
    check(&gpu, &mut renderer, &before, &world, &camera);
    assert!(
        renderer.reused(),
        "staging keeps the current atlas and cached picture"
    );
    assert!(!renderer.upload_staged(&gpu.queue, &mut staged, &after, 1));
    check(&gpu, &mut renderer, &before, &world, &camera);
    assert!(
        renderer.reused(),
        "partial uploads keep the current picture"
    );
    assert!(renderer.upload_staged(&gpu.queue, &mut staged, &after, 1));
    check(&gpu, &mut renderer, &before, &world, &camera);
    assert!(renderer.reused(), "completed uploads wait for adoption");

    renderer.adopt_atlas(&gpu.device, staged, &after);
    check(&gpu, &mut renderer, &after, &world, &camera);
    assert!(
        !renderer.reused(),
        "adoption invalidates the old cached picture"
    );
    check(&gpu, &mut renderer, &after, &world, &camera);
    assert!(renderer.reused());
}

#[test]
fn staged_adoption_restores_translucent_draws_and_changed_bounds() {
    let gpu = Headless::new().expect("AMD GPU");
    let make = |filled: bool| {
        let mut b = AtlasBuilder::new(16);
        let id = b
            .add(
                &RgbaImage::from_fn(8, 8, |x, y| {
                    if filled {
                        [8, 40, 240, 128]
                    } else if (2..6).contains(&x) && (2..6).contains(&y) {
                        [240, 40, 8, 255]
                    } else {
                        [0; 4]
                    }
                }),
                (0, 0),
            )
            .unwrap();
        let mut atlas = b.finish();
        atlas.trim_blank();
        (atlas, id)
    };
    let (before, id) = make(false);
    let (after, next_id) = make(true);
    assert_eq!(id, next_id);
    let world = world(&[id]);
    let mut renderer = renderer(&gpu, &before, &world);
    let camera = Camera::new((32, 32));
    check(&gpu, &mut renderer, &before, &world, &camera);
    let mut staged = renderer.stage_atlas(&gpu.device, &after);
    assert!(renderer.upload_staged(&gpu.queue, &mut staged, &after, u64::MAX));
    renderer.adopt_atlas(&gpu.device, staged, &after);
    // Moving the camera forces drawing independently of cache invalidation,
    // so stale duplicate masks cannot be hidden by a stale cached picture.
    let moved = Camera { x: 1, ..camera };
    check(&gpu, &mut renderer, &after, &world, &moved);
}
