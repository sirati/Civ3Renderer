//! Outline fallback must draw at 1:1 and retain its atlas/outline twin and
//! pass ordering when a view is magnified. These checks use the CPU path.

use std::sync::Arc;

use tile_render::{Atlas, AtlasBuilder, Camera, GlyphShape, Grid, LayerSpec, Layout, OutlineGlyph, PlacedSprite, RgbaImage, SpriteSetId, SpriteSets, TileWorld, ZoomArt, gpu_text::Face, reference};

const CLEAR: [u8; 4] = [0, 0, 0, 255];

fn world() -> (TileWorld, Atlas) {
    let points = [(0.0, 0.0), (0.0, 800.0), (500.0, 800.0), (500.0, 0.0)];
    let curves = (0..points.len()).map(|i| {
        let (a, b) = (points[i], points[(i + 1) % points.len()]);
        [a, ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), b]
    }).collect();
    let face = Face::from_curves(1000, &[('x', curves, 600)]);
    let mut world = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (32, 32) }, SpriteSets::default());
    world.fonts.push(Arc::new(face));
    (world, AtlasBuilder::new(64).finish())
}

fn glyph(color: [u8; 3], art: ZoomArt) -> PlacedSprite {
    let text = OutlineGlyph { font: 0, shape: GlyphShape::Glyph(0), em64: 16 * 64, advance: 10, bold: false, italic: false, color, key: None };
    PlacedSprite { screen: true, zoom_art: art, text: Some(text), ..PlacedSprite::new(SpriteSetId::EMPTY, 8, 24) }
}

#[test]
fn ordinary_outline_fallback_draws_visible_clipped_pixels_at_one_to_one() {
    let (mut world, atlas) = world();
    world.free.push(PlacedSprite { clip: Some([10, 14, 16, 22]), ..glyph([255; 3], ZoomArt::Any) });
    for zoomed_art in [false, true] {
        let camera = Camera { zoomed_art, ..Camera::new((32, 32)) };
        let picture = reference::render(&world, &atlas, &camera, 0, CLEAR);
        assert!(picture.pixels.iter().any(|pixel| pixel[0] != 0), "a valid outline fallback must actually draw at 1:1");
        for y in 0..32 {
            for x in 0..32 {
                if !(10..16).contains(&x) || !(14..22).contains(&y) { assert_eq!(picture.get(x, y), CLEAR); }
            }
        }
    }
}

#[test]
fn low_and_zoomed_text_twins_draw_once_at_their_selected_scale() {
    let (mut world, atlas) = world();
    let low = glyph([255, 0, 0], ZoomArt::Low);
    let high = glyph([0, 255, 0], ZoomArt::Zoomed { zoom: (0, 0), offset: (0, 0) });
    world.free = vec![low, high];
    for (zoom, zoomed_art, expected) in [((1, 1), false, low), ((1, 1), true, low), ((2, 1), true, high), ((2, 1), false, low)] {
        let camera = Camera { zoom, zoomed_art, ..Camera::new((64, 64)) };
        let picture = reference::render(&world, &atlas, &camera, 0, CLEAR);
        let mut single = world.clone();
        single.free = vec![expected];
        assert_eq!(picture, reference::render(&single, &atlas, &camera, 0, CLEAR), "the other twin must not blend onto this view at {zoom:?}");
        let selected = expected.text.unwrap().color;
        assert!(picture.pixels.iter().any(|pixel| pixel[..3] != [0, 0, 0]));
        for channel in 0..3 { if selected[channel] == 0 { assert!(picture.pixels.iter().all(|pixel| pixel[channel] == 0)); } }
    }
}

#[test]
fn atlas_low_twin_and_zoomed_outline_keep_their_existing_selection() {
    let (mut world, _) = world();
    let mut builder = AtlasBuilder::new(64);
    let image = builder.add(&RgbaImage::from_fn(8, 12, |_, _| [255, 0, 0, 255]), (0, -12)).unwrap();
    let set = world.sets.single(image);
    world.free = vec![
        PlacedSprite { screen: true, zoom_art: ZoomArt::Low, ..PlacedSprite::new(set, 8, 24) },
        glyph([0, 255, 0], ZoomArt::Zoomed { zoom: (0, 0), offset: (0, 0) }),
    ];
    let atlas = builder.finish();
    for (zoom, selected) in [((1, 1), 0), ((2, 1), 1)] {
        let camera = Camera { zoom, zoomed_art: true, ..Camera::new((64, 64)) };
        let picture = reference::render(&world, &atlas, &camera, 0, CLEAR);
        assert!(picture.pixels.iter().any(|pixel| pixel[selected] != 0));
        assert!(picture.pixels.iter().all(|pixel| pixel[1 - selected] == 0), "atlas and outline twins must not overlap");
    }
}

#[test]
fn fallback_text_keeps_its_position_in_separate_source_rectangles() {
    let (mut world, atlas) = world();
    let small = |color, view| {
        let mut sprite = glyph(color, ZoomArt::Any);
        sprite.x = 2;
        sprite.y = 12;
        sprite.text.as_mut().unwrap().em64 = 10 * 64;
        sprite.text.as_mut().unwrap().advance = 6;
        sprite.view = Some(view);
        sprite
    };
    world.free = vec![small([255, 0, 0], 0), small([0, 255, 0], 1)];
    let camera = Camera { zoom: (2, 1), ..Camera::new((32, 32)) };
    let cameras = [camera, Camera { origin: (32, 0), screen_shift: (32, 0), ..camera }];
    let picture = reference::render_views(&world, &atlas, &cameras, (64, 32), 0, CLEAR);
    for view in 0..2 {
        let mut single = world.clone();
        single.free = vec![PlacedSprite { view: Some(0), ..world.free[view] }];
        let expected = reference::render(&single, &atlas, &camera, 0, CLEAR);
        assert!(expected.pixels.iter().any(|pixel| pixel[..3] != [0, 0, 0]));
        for y in 0..32 { for x in 0..32 { assert_eq!(picture.get(x + 32 * view as u32, y), expected.get(x, y), "source rectangle {view}"); } }
    }
}

#[test]
fn text_stays_between_its_declared_layers_at_one_to_one_and_magnified() {
    let (mut world, _) = world();
    let mut builder = AtlasBuilder::new(64);
    let background = world.sets.single(builder.add(&RgbaImage::from_fn(32, 32, |_, _| [0, 0, 255, 255]), (0, 0)).unwrap());
    let covering = world.sets.single(builder.add(&RgbaImage::from_fn(32, 32, |_, _| [0, 255, 0, 255]), (0, 0)).unwrap());
    world.add_layer(LayerSpec::new("background", vec![background]));
    world.add_layer(LayerSpec { pass: 1, ..LayerSpec::new("covering", vec![covering]) });
    world.free = vec![PlacedSprite { after_pass: 0, ..glyph([255, 0, 0], ZoomArt::Any) }];
    let atlas = builder.finish();
    for zoom in [(1, 1), (2, 1)] {
        let camera = Camera { zoom, zoomed_art: true, ..Camera::new((32, 32)) };
        let underneath = reference::render(&world, &atlas, &camera, 0, CLEAR);
        assert!(underneath.pixels.iter().all(|pixel| *pixel == [0, 255, 0, 255]));
        let mut top = world.clone();
        top.free[0].after_pass = usize::MAX;
        let on_top = reference::render(&top, &atlas, &camera, 0, CLEAR);
        assert!(on_top.pixels.iter().any(|pixel| pixel[0] != 0), "text after the covering pass must remain visible");
    }
}
