//! Text drawn from glyph outlines: the GPU draws exactly the reference's
//! pixels (integer coverage, the same blending as ClearType-like sprites).

use std::sync::Arc;

use tile_render::gpu_text::Face;
use tile_render::{
    AtlasBuilder, Blend, Camera, ColorModel, GlyphShape, Grid, Headless, LayerSpec, Layout, OutlineGlyph, PlacedSprite, RgbaImage, SpriteSetId, SpriteSets, TileRenderer, TileWorld,
    ZoomArt, reference,
};

const CLEAR: [u8; 4] = [10, 20, 30, 255];

fn assert_same(gpu: &RgbaImage, cpu: &RgbaImage, what: &str) {
    assert_eq!((gpu.width, gpu.height), (cpu.width, cpu.height));
    let diff: Vec<_> = gpu.pixels.iter().zip(&cpu.pixels).enumerate().filter(|(_, (a, b))| a != b).collect();
    if let Some((i, (g, c))) = diff.first() {
        let (x, y) = (*i as u32 % gpu.width, *i as u32 / gpu.width);
        panic!("{what}: {} pixels differ, first at ({x}, {y}): gpu {g:?} cpu {c:?}", diff.len());
    }
}

/// A line from `a` to `b` as a quadratic.
fn line(a: (f64, f64), b: (f64, f64)) -> [(f64, f64); 3] {
    [a, ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), b]
}

/// A closed polygon, clockwise (y up) as given.
fn polygon(points: &[(f64, f64)]) -> Vec<[(f64, f64); 3]> {
    (0..points.len()).map(|i| line(points[i], points[(i + 1) % points.len()])).collect()
}

/// A ring of quadratic arcs around (`cx`, `cy`), `n` of them; reversed:
/// counter-clockwise (a hole).
fn ring(cx: f64, cy: f64, r: f64, n: usize, reversed: bool) -> Vec<[(f64, f64); 3]> {
    let step = std::f64::consts::TAU / n as f64;
    let at = |a: f64, k: f64| (cx + k * r * a.cos(), cy + k * r * a.sin());
    let mut out: Vec<_> = (0..n)
        .map(|i| {
            // Clockwise: decreasing angle.
            let (a0, a1) = (-(i as f64) * step, -((i + 1) as f64) * step);
            [at(a0, 1.0), at((a0 + a1) / 2.0, 1.0 / (step / 2.0).cos()), at(a1, 1.0)]
        })
        .collect();
    if reversed {
        out.reverse();
        for q in &mut out {
            q.swap(0, 2);
        }
    }
    out
}

/// A face of test shapes: a square, an "o" (ring with a hole), a slanted
/// bar, a triangle; 1000 units per em.
fn shapes() -> Face {
    let mut o = ring(300.0, 350.0, 300.0, 8, false);
    o.extend(ring(300.0, 350.0, 170.0, 6, true));
    Face::from_curves(
        1000,
        &[
            ('s', polygon(&[(50.0, 0.0), (50.0, 700.0), (600.0, 700.0), (600.0, 0.0)]), 650),
            ('o', o, 640),
            ('/', polygon(&[(0.0, -150.0), (380.0, 760.0), (480.0, 760.0), (100.0, -150.0)]), 480),
            ('v', polygon(&[(20.0, 520.0), (150.0, 520.0), (300.0, 80.0), (450.0, 520.0), (580.0, 520.0), (360.0, 0.0), (240.0, 0.0)]), 600),
        ],
    )
}

/// The game's font, where there is a Civ III installation.
fn lucida() -> Option<Face> {
    let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    let data = std::fs::read(std::path::Path::new(&install).join("LSANS.TTF")).ok()?;
    Some(Face::parse(&data, 0).unwrap())
}

/// A text run's glyphs at (`x`, `y`) (pen on the baseline), advances as
/// given, with an underline.
fn run(face: &Face, font: u32, text: &str, (x, y): (i32, i32), em: u32, color: [u8; 3], style: (bool, bool, Option<[u8; 3]>), base: PlacedSprite) -> Vec<PlacedSprite> {
    let (bold, italic, key) = style;
    let mut out = Vec::new();
    let mut pen = x;
    for c in text.chars() {
        let advance = (em as i32 * 6 / 10 + if bold { 1 } else { 0 }).max(1);
        let g = OutlineGlyph { font, shape: GlyphShape::Glyph(face.glyph(c).unwrap_or(0)), em64: em * 64, advance, bold, italic, color, key };
        out.push(PlacedSprite { x: pen, y, text: Some(g), ..base });
        out.push(PlacedSprite { x: pen, y, text: Some(OutlineGlyph { shape: GlyphShape::Bar { top: 1, height: 1 }, ..g }), ..base });
        pen += advance;
    }
    out
}

/// A world with a darkening pass (the fog; in the quantized model only,
/// where scaling is exact), text under it and on top, screen and world
/// text, clipped, in every style.
fn world(color: ColorModel, faces: Vec<Arc<Face>>, spread: i32) -> (TileWorld, tile_render::Atlas) {
    let mut atlas = AtlasBuilder::new(512);
    let mut sets = SpriteSets::default();
    let ground = sets.single(atlas.add(&RgbaImage::from_fn(128, 64, |x, y| [(x * 2) as u8, 120 + (y % 64) as u8, 200, 255]), (0, 0)).unwrap());
    let fog = sets.single(atlas.add(&RgbaImage::from_fn(128, 64, |x, y| [((x / 8 + y / 8) % 16) as u8, 0, 0, 255]), (0, 0)).unwrap());
    let plate = sets.single(atlas.add(&RgbaImage::from_fn(90, 20, |_, _| [240, 230, 200, 255]), (0, 0)).unwrap());
    let grid = Grid { cols: 8, rows: 12, layout: Layout::Orthogonal, wrap_x: true, wrap_y: false, step: (128, 64) };
    let mut world = TileWorld::new(grid, sets);
    world.color = color;
    let terrain = world.add_layer(LayerSpec::new("terrain", vec![ground]));
    let shading = world.add_layer(LayerSpec { pass: 1, blend: Blend::Scale { denominator: 16 }, ..LayerSpec::new("fog", vec![SpriteSetId::EMPTY, fog]) });
    for r in 0..grid.rows {
        for c in 0..grid.cols {
            world.set(terrain, c, r, 0);
            if color != ColorModel::Direct {
                world.set(shading, c, r, (c + r) % 2);
            }
        }
    }
    let texts = ["sovs/", "o/vs", "vos"];
    for (font, face) in faces.iter().enumerate() {
        let font = font as u32;
        let text = |i: usize| if font == 0 { texts[i % 3].to_owned() } else { ["Rome 12", "Ägypt: wavy", "Kyōto"][i % 3].to_owned() };
        let mut i = 0;
        for (bold, italic, key) in [(false, false, None), (true, false, None), (false, true, None), (true, true, Some([255, 0, 255]))] {
            for (em, color) in [(10, [0, 0, 0]), (12, [255, 255, 255]), (16, [250, 200, 40])] {
                let y = 20 + 22 * (i % 8) as i32 + 180 * font as i32;
                let x = 10 + 140 * (i / 8) as i32;
                // A plate under (sprites and text keep their order), the
                // text after pass 0 (under the fog), world-space.
                let world_text = PlacedSprite { after_pass: 0, ..PlacedSprite::new(plate, x, y) };
                world.free.push(PlacedSprite { y: y - 14, ..world_text });
                world.free.extend(run(face, font, &text(i), (x, y), em, color, (bold, italic, key), world_text));
                // Screen text on top, clipped.
                let screen = PlacedSprite { screen: true, view: Some(0), clip: Some([x + 5, y - 30, x + 60, y + 3]), ..PlacedSprite::new(plate, x + 400, y) };
                world.free.extend(run(face, font, &text(i + 1), (x + 400, y), em, color, (bold, italic, key), screen));
                i += 1;
            }
        }
    }
    world.fonts = faces;
    // These are the zoomed twins of the fixture's plates. Ordinary
    // outline fallback is tested separately and also draws at 1:1.
    for sprite in world.free.iter_mut().filter(|sprite| sprite.text.is_some()) {
        sprite.zoom_art = ZoomArt::Zoomed { zoom: (0, 0), offset: (0, 0) };
    }
    // World sprites laid out `spread` times apart (for a view showing the
    // world that many times smaller, as a game's zoomed-out labels are).
    for s in world.free.iter_mut().filter(|s| !s.screen) {
        (s.x, s.y) = (s.x * spread, s.y * spread);
    }
    // A 1:1 twin, left out where the text is drawn.
    world.free.push(PlacedSprite { screen: true, view: Some(0), zoom_art: ZoomArt::Low, ..PlacedSprite::new(plate, 5, 5) });
    (world, atlas.finish())
}

/// The largest difference of any colour channel per pixel.
fn differences(gpu: &RgbaImage, cpu: &RgbaImage) -> Vec<i32> {
    gpu.pixels.iter().zip(&cpu.pixels).map(|(a, b)| (0..3).map(|c| (a[c] as i32 - b[c] as i32).abs()).max().unwrap()).collect()
}

#[test]
fn outline_fallback_and_twins_match_reference_at_one_and_magnified() {
    let gpu = Headless::new().expect("GPU");
    let mut world = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (64, 64) }, SpriteSets::default());
    world.fonts.push(Arc::new(shapes()));
    let glyph = |x, color, zoom_art| PlacedSprite {
        screen: true, zoom_art,
        text: Some(OutlineGlyph { font: 0, shape: GlyphShape::Glyph(0), em64: 16 * 64, advance: 10, bold: false, italic: false, color, key: None }),
        ..PlacedSprite::new(SpriteSetId::EMPTY, x, 20)
    };
    world.free = vec![
        glyph(5, [255, 0, 0], ZoomArt::Low),
        glyph(5, [0, 255, 0], ZoomArt::Zoomed { zoom: (0, 0), offset: (0, 0) }),
        glyph(20, [0, 0, 255], ZoomArt::Any),
    ];
    let atlas = AtlasBuilder::new(64).finish();
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    for (zoom, selected) in [((1, 1), 0), ((2, 1), 1)] {
        let camera = Camera { zoom, zoomed_art: true, ..Camera::new((64, 64)) };
        let actual = gpu.render_views(&mut renderer, &[camera], (64, 64), 0, [0, 0, 0, 255]);
        let expected = reference::render(&world, &atlas, &camera, 0, [0, 0, 0, 255]);
        assert_same(&actual, &expected, &format!("fallback and twins at {zoom:?}"));
        assert!(actual.pixels.iter().any(|pixel| pixel[selected] != 0), "the selected twin draws");
        assert!(actual.pixels.iter().all(|pixel| pixel[1 - selected] == 0), "the other twin does not draw");
        assert!(actual.pixels.iter().any(|pixel| pixel[2] != 0), "ordinary fallback draws at every scale");
    }
}

#[test]
fn outline_fallback_matches_reference_in_offset_source_rectangles() {
    let gpu = Headless::new().expect("GPU");
    let mut world = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (64, 64) }, SpriteSets::default());
    world.fonts.push(Arc::new(shapes()));
    for (view, color) in [(0, [255, 0, 0]), (1, [0, 255, 0])] {
        world.free.push(PlacedSprite {
            screen: true, view: Some(view), zoom_art: ZoomArt::Any,
            text: Some(OutlineGlyph { font: 0, shape: GlyphShape::Glyph(0), em64: 10 * 64, advance: 6, bold: false, italic: false, color, key: None }),
            ..PlacedSprite::new(SpriteSetId::EMPTY, 2, 12)
        });
    }
    let atlas = AtlasBuilder::new(64).finish();
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    for zoom in [(1, 1), (2, 1)] {
        let camera = Camera { zoom, ..Camera::new((32, 32)) };
        let cameras = [camera, Camera { origin: (32, 0), screen_shift: (32, 0), ..camera }];
        let actual = gpu.render_views(&mut renderer, &cameras, (64, 32), 0, [0, 0, 0, 255]);
        let expected = reference::render_views(&world, &atlas, &cameras, (64, 32), 0, [0, 0, 0, 255]);
        assert_same(&actual, &expected, &format!("offset fallback at {zoom:?}"));
        assert!(actual.pixels.iter().any(|pixel| pixel[0] != 0));
        assert!(actual.pixels.iter().any(|pixel| pixel[1] != 0), "the second viewport uses its actual render-target origin");
    }
}

#[test]
fn outline_coverage_is_bit_exact() {
    // Glyphs apart from each other, white on black in the direct model:
    // every pixel is the coverage itself (one blend, with nothing under
    // it), so the GPU's must be the reference's to the bit; and drawn onto
    // a colour key (opaque where drawn), in both models. Screen glyphs, and
    // world glyphs in views showing the world as it is and half size (at
    // twice the positions, so they land in the same places).
    let gpu = Headless::new().expect("GPU");
    let mut faces = vec![Arc::new(shapes())];
    faces.extend(lucida().map(Arc::new));
    for (color, key) in [(ColorModel::Direct, None), (ColorModel::Direct, Some([255, 0, 255])), (ColorModel::Quantized { bits: 5 }, Some([255, 0, 255]))] {
        for (screen, downscale) in [(true, 1), (false, 1), (false, 2)] {
            let mut world = TileWorld::new(Grid { cols: 30, rows: 30, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (64, 64) }, SpriteSets::default());
            world.color = color;
            for (font, face) in faces.iter().enumerate() {
                for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                    for em in [10u32, 12, 16] {
                        let base = PlacedSprite { screen, ..PlacedSprite::new(SpriteSetId::EMPTY, 0, 0) };
                        let text = if font == 0 { "sovs/" } else { "Rome Ägypt" };
                        world.free.extend(run(face, font as u32, text, (0, 0), em, [255, 255, 255], (bold, italic, key), base));
                    }
                }
            }
            // Each glyph (and its underline) on its own.
            let k = downscale as i32;
            for (i, s) in world.free.iter_mut().enumerate() {
                let i = i as i32 / 2;
                (s.x, s.y) = (k * (5 + (i % 14) * 45), k * (20 + (i / 14) * 34 + if matches!(s.text.unwrap().shape, GlyphShape::Bar { .. }) { 6 } else { 0 }));
            }
            world.fonts = faces.clone();
            let atlas = AtlasBuilder::new(64).finish();
            let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
            renderer.set_atlas(&gpu.device, &gpu.queue, &trimmed(&atlas));
            renderer.set_world(&gpu.device, &world);
            for zoom in [(1, 1), (2, 1), (3, 2), (3, 1), (2, 3), (5, 4)] {
                let camera = Camera { zoom, downscale, zoomed_art: true, ..Camera::new((640, 600)) };
                let got = gpu.render_views(&mut renderer, &[camera], (640, 600), 1, [0, 0, 0, 255]);
                let want = reference::render_views(&world, &atlas, &[camera], (640, 600), 1, [0, 0, 0, 255]);
                let what = format!("glyphs at {zoom:?}, {color:?}, key {key:?}, screen {screen}, downscale {downscale}");
                assert_same(&got, &want, &what);
                assert!(got.pixels.iter().filter(|p| p[0] > 0).count() > 3000, "{what}: drawn");
            }
        }
    }
}

#[test]
fn outline_text_matches_the_reference() {
    // Labels on plates, under the fog and on top, screen and world text,
    // clipped, in every style, overlapping, in two views. Coverage is exact
    // (see `outline_coverage_is_bit_exact`); where translucent pixels are
    // blended the GPU's blender rounds its own way: in the direct model by
    // up to one 8-bit step anywhere; in the quantized model, whose rounding
    // trick leaves the blender a margin of 1/64 of a step, one 5-bit step
    // in a handful of pixels (seen: at most 9 of 256 000). Also in views
    // showing the world half size.
    let gpu = Headless::new().expect("GPU");
    let mut faces = vec![Arc::new(shapes())];
    faces.extend(lucida().map(Arc::new));
    // (And in views showing the world half size, as a game's zoomed-out
    // view: world text at its positions halved, at its own size.)
    for (color, downscale) in [(ColorModel::Quantized { bits: 5 }, 1), (ColorModel::Direct, 1), (ColorModel::Quantized { bits: 5 }, 2), (ColorModel::Direct, 2)] {
        let (world, atlas) = world(color, faces.clone(), downscale as i32);
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
        renderer.set_atlas(&gpu.device, &gpu.queue, &trimmed(&atlas));
        renderer.set_world(&gpu.device, &world);
        let zooms: &[(u32, u32)] = if downscale == 1 { &[(2, 1), (3, 2), (3, 1), (2, 3), (5, 4)] } else { &[(2, 1), (3, 2), (3, 1)] };
        for &zoom in zooms {
            let main = Camera { x: -20, y: 0, zoom, downscale, zoomed_art: true, screen_shift: (3, -2), overhang: (0, 40, 0, 0), ..Camera::new((640, 400)) };
            let inset = Camera { x: 900 / downscale as i32, y: 30, zoom, downscale, zoomed_art: true, origin: (500, 250), ..Camera::new((140, 150)) };
            let cameras = [main, inset];
            let got = gpu.render_views(&mut renderer, &cameras, (640, 400), 1, CLEAR);
            let want = reference::render_views(&world, &atlas, &cameras, (640, 400), 1, CLEAR);
            let d = differences(&got, &want);
            let (count, most) = (d.iter().filter(|&&v| v > 0).count(), *d.iter().max().unwrap());
            match color {
                ColorModel::Direct => assert!(most <= 1, "text at {zoom:?} (downscale {downscale}), {color:?}: {count} pixels differ, by up to {most}"),
                _ => assert!(most <= 9 && count <= 26, "text at {zoom:?} (downscale {downscale}), {color:?}: {count} pixels differ, by up to {most}"),
            }
            // Some text is drawn: it differs from the 1:1 twins' picture.
            let pictures = cameras.map(|c| Camera { zoomed_art: false, ..c });
            let low = reference::render_views(&world, &atlas, &pictures, (640, 400), 1, CLEAR);
            let changed = got.pixels.iter().zip(&low.pixels).filter(|(a, b)| a != b).count();
            assert!(changed > 2000, "text at {zoom:?} (downscale {downscale}): {changed} pixels drawn");
        }
    }
}

#[test]
fn ordinary_styled_layered_text_matches_reference_and_is_visible_at_one_to_one() {
    let gpu = Headless::new().expect("GPU");
    let mut faces = vec![Arc::new(shapes())];
    faces.extend(lucida().map(Arc::new));
    for (color, downscale) in [(ColorModel::Direct, 1), (ColorModel::Quantized { bits: 5 }, 1), (ColorModel::Direct, 2), (ColorModel::Quantized { bits: 5 }, 2)] {
        let (mut world, atlas) = world(color, faces.clone(), downscale as i32);
        for sprite in world.free.iter_mut().filter(|sprite| sprite.text.is_some()) {
            sprite.zoom_art = ZoomArt::Any;
            // This variant places the topmost screen runs inside their
            // own clips, 400px to the right of the world-space runs.
            if sprite.screen {
                sprite.clip = sprite.clip.map(|[left, top, right, bottom]| [left + 400, top, right + 400, bottom]);
            }
        }
        let mut bare = world.clone();
        bare.free.retain(|sprite| sprite.text.is_none());
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
        renderer.set_atlas(&gpu.device, &gpu.queue, &trimmed(&atlas));
        renderer.set_world(&gpu.device, &world);
        for zoomed_art in [false, true] {
            let cameras = [
                Camera { x: -20, downscale, zoomed_art, screen_shift: (3, -2), overhang: (0, 40, 0, 0), ..Camera::new((640, 400)) },
                Camera { x: 900 / downscale as i32, y: 30, downscale, zoomed_art, origin: (500, 250), ..Camera::new((140, 150)) },
            ];
            let actual = gpu.render_views(&mut renderer, &cameras, (640, 400), 1, CLEAR);
            let expected = reference::render_views(&world, &atlas, &cameras, (640, 400), 1, CLEAR);
            let difference = differences(&actual, &expected);
            let (count, most) = (difference.iter().filter(|&&value| value != 0).count(), *difference.iter().max().unwrap());
            // The same blending allowance as the magnified layered test:
            // one 8-bit step in direct colour, or one 5-bit colour step.
            let allowed = if color == ColorModel::Direct { 1 } else { 9 };
            assert!(most <= allowed, "ordinary styled text at 1:1, {color:?}, downscale {downscale}, zoomed_art {zoomed_art}: {count} pixels differ, by up to {most}");
            let no_text = reference::render_views(&bare, &atlas, &cameras, (640, 400), 1, CLEAR);
            let visible = actual.pixels.iter().zip(&no_text.pixels).filter(|(text, bare)| text != bare).count();
            assert!(visible > 1000, "ordinary styled/layered text must draw visible pixels at 1:1, {color:?}, downscale {downscale}, zoomed_art {zoomed_art}: {visible} pixels");
        }
    }
}

#[test]
fn text_is_left_out_where_zoomed_art_is_not_shown() {
    let gpu = Headless::new().expect("GPU");
    let (world, atlas) = world(ColorModel::Quantized { bits: 5 }, vec![Arc::new(shapes())], 1);
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_atlas(&gpu.device, &gpu.queue, &trimmed(&atlas));
    renderer.set_world(&gpu.device, &world);
    let mut bare = world.clone();
    bare.free.retain(|s| s.text.is_none());
    for camera in [Camera::new((640, 400)), Camera { zoom: (2, 1), ..Camera::new((640, 400)) }] {
        let got = gpu.render_views(&mut renderer, &[camera], (640, 400), 1, CLEAR);
        let want = reference::render_views(&bare, &atlas, &[camera], (640, 400), 1, CLEAR);
        assert_same(&got, &want, &format!("no text at {:?}", camera.zoom));
    }
}



/// Screen text in a stacked view of a scaled group (the driver's whole
/// screen fitted into a pane) shows wherever the group is: in a pane at the
/// window's corner and in one off it.
#[test]
fn screen_text_in_a_fitted_screen_shows_off_the_corner_too() {
    let gpu = Headless::new().expect("GPU");
    let faces = vec![Arc::new(shapes())];
    let (mut world, atlas) = world(ColorModel::Direct, faces, 1);
    // Only the screen text (view 1: the stacked interface view).
    world.free.retain(|s| s.screen);
    for s in &mut world.free {
        s.view = Some(1);
    }
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_atlas(&gpu.device, &gpu.queue, &trimmed(&atlas));
    renderer.set_world(&gpu.device, &world);
    let mut picture = |origin: (i32, i32), zoomed_art: bool| {
        let head = Camera { x: 0, y: 0, zoom: (1, 2), origin, viewport: (640, 400), zoomed_art, ..Camera::new((640, 400)) };
        let ui = Camera { cells: false, stacked: true, hairline: true, zoomed_art, ..Camera::new((640, 400)) };
        let cameras = [head, ui];
        (gpu.render_views(&mut renderer, &cameras, (1280, 400), 1, CLEAR), reference::render_views(&world, &atlas, &cameras, (1280, 400), 1, CLEAR))
    };
    for zoomed_art in [false, true] {
        let (a, a_ref) = picture((0, 0), zoomed_art);
        let (b, b_ref) = picture((640, 0), zoomed_art);
        let at = |img: &RgbaImage, x0: u32| (0..200).flat_map(|y| (0..320).map(move |x| (x, y))).filter(|&(x, y)| img.get(x0 + x, y) != CLEAR).count();
        let (in_a, in_b) = (at(&a, 0), at(&b, 640));
        assert!(in_a > 100, "zoomed_art {zoomed_art}: text in the pane at the corner: {in_a}");
        assert_eq!(in_b, in_a, "zoomed_art {zoomed_art}: the same text in the pane off the corner");
        assert_eq!((at(&a_ref, 0), at(&b_ref, 640)), (in_a, in_a), "zoomed_art {zoomed_art}: the reference");
    }
}

/// `atlas` with its sprites' blank margins found, as compiled descriptions
/// have them: the GPU leaves them out of the rectangles it draws, which
/// must not change a pixel.
fn trimmed(atlas: &tile_render::Atlas) -> tile_render::Atlas {
    let mut a = atlas.clone();
    a.trim_blank();
    a
}

#[test]
fn clean_map_masks_outline_glyphs_per_camera() {
    let gpu = Headless::new().expect("GPU");
    let mut world = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (8, 8) }, SpriteSets::default());
    world.fonts.push(Arc::new(shapes()));
    let glyph = |x: i32, category: u32| PlacedSprite {
        x,
        y: 20,
        screen: true,
        category,
        text: Some(OutlineGlyph { font: 0, shape: GlyphShape::Glyph(0), em64: 12 * 64, advance: 10, bold: false, italic: false, color: [255, 255, 255], key: None }),
        ..PlacedSprite::new(SpriteSetId::EMPTY, x, 20)
    };
    world.free.push(glyph(1, 1));
    world.free.push(glyph(14, 2));
    let atlas = AtlasBuilder::new(64).finish();
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let mut camera = Camera::new((24, 24));
    camera.hidden_categories = 1;
    let got = gpu.render(&mut renderer, &camera, 0, CLEAR);
    let want = reference::render(&world, &atlas, &camera, 0, CLEAR);
    assert_same(&got, &want, "outline glyph clean-map mask");
    let mut left = 0;
    let mut right = 0;
    for y in 0..24 {
        for x in 0..24 {
            if got.get(x, y) == CLEAR { continue; }
            if x < 12 { left += 1; } else { right += 1; }
        }
    }
    assert_eq!(left, 0, "hidden glyph category must not draw");
    assert!(right > 0, "visible glyph category must draw");
}

/// Cropped named panels keep text in driver coordinates while their
/// stacked camera moves the panel into a pane at its own scale.
#[test]
fn cropped_panel_text_twins_and_fallback_keep_visible_pixels_at_every_scale() {
    let gpu = Headless::new().expect("GPU");
    let mut atlas = AtlasBuilder::new(64);
    let mut sets = SpriteSets::default();
    let red = sets.single(atlas.add(&RgbaImage::from_fn(8, 12, |_, _| [255, 0, 0, 255]), (0, -12)).unwrap());
    let atlas = atlas.finish();
    let mut world = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, step: (32, 32), wrap_x: false, wrap_y: false }, sets);
    world.fonts.push(Arc::new(shapes()));
    for (x, key) in [(470, None), (500, Some([255, 0, 255]))] {
        let base = PlacedSprite { screen: true, view: Some(1), clip: Some([448, 156, 1472, 924]), ..PlacedSprite::new(red, x, 200) };
        world.free.push(PlacedSprite { zoom_art: ZoomArt::Low, ..base });
        world.free.push(PlacedSprite {
            zoom_art: ZoomArt::Zoomed { zoom: (0, 0), offset: (0, 0) },
            text: Some(OutlineGlyph { font: 0, shape: GlyphShape::Glyph(0), em64: 16 * 64, advance: 10, bold: false, italic: false, color: [0, 255, 0], key }),
            ..base
        });
    }
    world.free.push(PlacedSprite {
        x: 540, screen: true, view: Some(1), clip: Some([448, 156, 1472, 924]), zoom_art: ZoomArt::Any,
        text: Some(OutlineGlyph { font: 0, shape: GlyphShape::Glyph(0), em64: 16 * 64, advance: 10, bold: false, italic: false, color: [0, 0, 255], key: None }),
        ..PlacedSprite::new(SpriteSetId::EMPTY, 540, 200)
    });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    for zoom in [(239, 256), (957, 1024), (4095, 4096), (1, 1), (3, 2), (2, 1)] {
        let viewport = ((1024u32 * zoom.0).div_ceil(zoom.1), (768u32 * zoom.0).div_ceil(zoom.1));
        let origin = (100, 100);
        let cameras = [
            Camera { origin, viewport, zoom, cells: false, hairline: true, ..Camera::new(viewport) },
            Camera { origin: (0, 0), screen_shift: (-448, -156), cells: false, hairline: true, stacked: true, zoomed_art: zoom.0 != zoom.1, ..Camera::new((1024, 768)) },
        ];
        let actual = gpu.render_views(&mut renderer, &cameras, (2300, 1800), 0, [0, 0, 0, 255]);
        let expected = reference::render_views(&world, &atlas, &cameras, (2300, 1800), 0, [0, 0, 0, 255]);
        assert_same(&actual, &expected, &format!("cropped panel text at {zoom:?}"));
        let selected = if zoom.0 == zoom.1 { 0 } else { 1 };
        let counts: Vec<_> = (0..3).map(|c| actual.pixels.iter().filter(|p| p[c] > 0).count()).collect();
        assert!(counts[selected] > 30, "selected text twin draws at {zoom:?}: {counts:?}");
        assert!(actual.pixels.iter().filter(|p| p[2] > 0).count() > 30, "ordinary outline fallback draws at {zoom:?}");
    }
}


#[test]
fn world_annotation_layouts_keep_plates_and_text_together_across_native_zoom_switches() {
    use tile_render::{CATEGORY_ALL, CATEGORY_CITIES, CATEGORY_FULL_RESOLUTION, CATEGORY_REDUCED_RESOLUTION, KEEP_SIZE};
    let gpu = Headless::new().unwrap();
    let mut builder = AtlasBuilder::new(128);
    let plate = builder.add(&RgbaImage::from_fn(80, 20, |_, _| [30, 40, 50, 255]), (0, 0)).unwrap();
    let mut atlas = builder.finish();
    atlas.sprites[plate.0 as usize].scaling = KEEP_SIZE;
    let mut sets = SpriteSets::default();
    let plate_set = sets.add(&[plate]);
    let mut world = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (64, 64) }, sets);
    world.color = ColorModel::Quantized { bits: 5 };
    world.fonts.push(Arc::new(shapes()));
    let full = CATEGORY_CITIES | CATEGORY_FULL_RESOLUTION;
    let reduced = CATEGORY_CITIES | CATEGORY_REDUCED_RESOLUTION;
    for (category, x, y, step) in [(full, 80, 140, 1), (reduced, 40, 180, 2)] {
        world.free.push(PlacedSprite { category, ..PlacedSprite::new(plate_set, x, y) });
        let mut glyphs = run(&world.fonts[0], 0, "sos", (x + 10 * step, y + 15 * step), 12, [230, 240, 250], (false, false, None), PlacedSprite { category, zoom_art: ZoomArt::Any, ..PlacedSprite::new(SpriteSetId::EMPTY, 0, 0) });
        if step == 2 {
            for g in glyphs.iter_mut().skip(2) {
                // Reduced layout anchors are doubled, while glyph metrics
                // and the plate's own pixels keep their native size.
                let start = x + 10 * step;
                g.x = start + (g.x - start) * 2;
            }
        }
        world.free.extend(glyphs);
    }
    assert_eq!(CATEGORY_ALL & (CATEGORY_FULL_RESOLUTION | CATEGORY_REDUCED_RESOLUTION), 0);
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    for downscale in [1, 2, 1, 2] {
        for zoom in [(1, 1), (2, 1), (3, 2), (239, 256)] {
            let camera = Camera { downscale, zoom, zoomed_art: true, ..Camera::new((640, 480)) };
            let selected = if downscale == 1 { CATEGORY_FULL_RESOLUTION } else { CATEGORY_REDUCED_RESOLUTION };
            let mut expected_world = world.clone();
            expected_world.free.retain(|s| s.category & selected != 0);
            for s in &mut expected_world.free { s.category = 0; }
            let expected = reference::render_views(&expected_world, &atlas, &[camera], (640, 480), 0, CLEAR);
            let actual = gpu.render_views(&mut renderer, &[camera], (640, 480), 0, CLEAR);
            assert_same(&actual, &expected, &format!("annotation layout downscale{downscale} zoom{zoom:?}"));
            let cpu = reference::render_views(&world, &atlas, &[camera], (640, 480), 0, CLEAR);
            assert_same(&cpu, &expected, "CPU scale-layout selection");
            let (px, py) = if downscale == 1 { (80, 140) } else { (20, 90) };
            let scaled = |v: i32| (v as i64 * zoom.0 as i64).div_euclid(zoom.1 as i64) as i32;
            let plate_bounds = [scaled(px), scaled(py), scaled(px + 80), scaled(py + 20)];
            let ink: Vec<_> = actual.pixels.iter().enumerate().filter(|(_, p)| p[0] > 100 && p[1] > 100).collect();
            assert!(ink.len() > 25, "text must draw inside its native plate");
            assert!(ink.iter().all(|(at, _)| {
                let (x, y) = ((*at % 640) as i32, (*at / 640) as i32);
                x >= plate_bounds[0] && x <= plate_bounds[2] && y >= plate_bounds[1] && y <= plate_bounds[3]
            }), "letters and underline must remain inside their plate at downscale{downscale} zoom{zoom:?}");
            let hidden = Camera { hidden_categories: CATEGORY_CITIES, ..camera };
            let actual = gpu.render_views(&mut renderer, &[hidden], (640, 480), 0, CLEAR);
            expected_world.free.clear();
            let expected = reference::render_views(&expected_world, &atlas, &[hidden], (640, 480), 0, CLEAR);
            assert_same(&actual, &expected, "city filtering must hide both layouts");
        }
    }
}

/// The viewer's shader module (it holds the tile renderer's shaders).
const SHADERS: tile_render::Shaders = tile_render::Shaders::new(viewer_spirv::SPIRV, viewer_spirv::SPIRV_FALLBACK);
