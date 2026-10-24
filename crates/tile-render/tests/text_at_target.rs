//! A canvas shown in k×k boxes (a pixel scale) composed at the target's own
//! resolution instead (`Present::at_target`, `Camera::boxed`): art shows
//! exactly as the boxes showed it, and text drawn from outlines is drawn at
//! the size it is seen at, never scaled as a bitmap.

use std::sync::Arc;

use tile_render::gpu_text::Face;
use tile_render::{
    Atlas, AtlasBuilder, Camera, ColorModel, GlyphShape, Grid, Headless, LayerSpec, Layout, OutlineGlyph, PlacedSprite, Present, RgbaImage, ScreenCopy, SpriteSetId, SpriteSets,
    TileRenderer, TileWorld, ZoomArt, reference,
};

const CLEAR: [u8; 4] = [10, 20, 30, 255];
const ZOOMED: ZoomArt = ZoomArt::Zoomed { zoom: (0, 0), offset: (0, 0) };

fn assert_same(got: &RgbaImage, want: &RgbaImage, what: &str) {
    assert_eq!((got.width, got.height), (want.width, want.height), "{what}");
    let diff: Vec<_> = got.pixels.iter().zip(&want.pixels).enumerate().filter(|(_, (a, b))| a != b).collect();
    if let Some((i, (g, w))) = diff.first() {
        let (x, y) = (*i as u32 % got.width, *i as u32 / got.width);
        panic!("{what}: {} pixels differ, first at ({x}, {y}): {g:?}, wanted {w:?}", diff.len());
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

/// 's': a square 550 × 700 units from x 50 (advance 650), 'v' a chevron;
/// 1000 units per em.
fn shapes() -> Face {
    Face::from_curves(
        1000,
        &[
            ('s', polygon(&[(50.0, 0.0), (50.0, 700.0), (600.0, 700.0), (600.0, 0.0)]), 650),
            ('v', polygon(&[(20.0, 520.0), (150.0, 520.0), (300.0, 80.0), (450.0, 520.0), (580.0, 520.0), (360.0, 0.0), (240.0, 0.0)]), 600),
        ],
    )
}

/// A real TrueType font, where the environment names one (`TEST_FONT`).
fn test_font() -> Option<Face> {
    let data = std::fs::read(std::env::var_os("TEST_FONT")?).ok()?;
    Some(Face::parse(&data, 0).unwrap())
}

/// A world of tiles, world sprites, interface sprites per view (opaque,
/// with holes, translucent, clipped) and 1:1 text twins (raster plates
/// beside zoomed outline glyphs).
fn art_world(color: ColorModel) -> (TileWorld, Atlas) {
    let mut atlas = AtlasBuilder::new(256);
    let mut sets = SpriteSets::default();
    let ground = sets.single(atlas.add(&RgbaImage::from_fn(32, 16, |x, y| [(x * 7) as u8, (y * 13) as u8, 90 + (x ^ y) as u8, 255]), (0, 0)).unwrap());
    let holes = sets.single(atlas.add(&RgbaImage::from_fn(7, 5, |x, y| if (x + y) % 3 == 0 { [0; 4] } else { [250, 40, 20, 255] }), (-3, -4)).unwrap());
    let glass = sets.single(atlas.add(&RgbaImage::from_fn(20, 9, |x, _| [20, 200, 240, 60 + 8 * x as u8]), (0, 0)).unwrap());
    let plate = sets.single(atlas.add(&RgbaImage::from_fn(11, 13, |x, y| [200, 180 + y as u8, 40 + 9 * x as u8, 255]), (0, -10)).unwrap());
    let grid = Grid { cols: 8, rows: 10, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (32, 16) };
    let mut world = TileWorld::new(grid, sets);
    world.color = color;
    let terrain = world.add_layer(LayerSpec::new("terrain", vec![ground]));
    for r in 0..grid.rows {
        for c in 0..grid.cols {
            world.set(terrain, c, r, 0);
        }
    }
    world.fonts.push(Arc::new(shapes()));
    for (i, (x, y)) in [(13, 9), (70, 33), (140, 77), (33, 120)].into_iter().enumerate() {
        world.free.push(PlacedSprite { after_pass: 0, ..PlacedSprite::new(if i % 2 == 0 { holes } else { glass }, x, y) });
    }
    // Per view: screen sprites, a clipped one, and a 1:1 text twin.
    for view in 0..6u32 {
        let o = 3 * view as i32;
        let screen = |set, x, y| PlacedSprite { screen: true, view: Some(view), ..PlacedSprite::new(set, x, y) };
        world.free.push(screen(holes, 12 + o, 15));
        world.free.push(screen(glass, 30 + o, 22));
        world.free.push(PlacedSprite { clip: Some([40 + o, 30, 52 + o, 36]), ..screen(glass, 38 + o, 28) });
        world.free.push(PlacedSprite { zoom_art: ZoomArt::Low, ..screen(plate, 60 + o, 40) });
        let text = OutlineGlyph { font: 0, shape: GlyphShape::Glyph(0), em64: 12 * 64, advance: 8, bold: false, italic: false, color: [255; 3], key: None };
        world.free.push(PlacedSprite { zoom_art: ZOOMED, text: Some(text), ..screen(SpriteSetId::EMPTY, 60 + o, 40) });
    }
    (world, atlas.finish())
}

/// The canvas's views (160 × 120): the world at 1:1 and an inset at 2:1
/// with an interface stacked on it, an interface over the window, a menu
/// at 2:1 drawn straight (as `overlaid` scaled interfaces are) and the
/// viewer's own overlay (a cursor camera).
fn art_cameras() -> Vec<Camera> {
    vec![
        Camera { x: 4, y: 3, ..Camera::new((100, 120)) },
        Camera { x: 60, y: 40, zoom: (2, 1), origin: (100, 0), ..Camera::new((60, 60)) },
        Camera { origin: (3, 4), screen_shift: (3, 4), cells: false, stacked: true, ..Camera::new((20, 24)) },
        Camera { cells: false, target_clip: Some([0, 0, 150, 110]), ..Camera::new((160, 120)) },
        Camera { zoom: (2, 1), origin: (20, 60), screen_shift: (-20, 30), cells: false, hairline: true, ..Camera::new((80, 50)) },
        Camera { cells: false, ..Camera::new((160, 120)) },
    ]
}

/// Without text drawn at a zoom, a picture shown in k×k boxes and the same
/// cameras composed at the target's resolution are the same, pixel for
/// pixel, in both colour models, with screen copies and a picture kept
/// under the overlays.
#[test]
fn boxed_cameras_show_art_exactly_as_boxes_do() {
    let gpu = Headless::new().expect("GPU");
    let copies = [ScreenCopy { source: [10, 12, 30, 20], at: (96, 70), zoom: (2, 1), lens: None }, ScreenCopy { source: [50, 20, 40, 40], at: (5, 70), zoom: (3, 4), lens: None }];
    for color in [ColorModel::Direct, ColorModel::Quantized { bits: 5 }] {
        let (world, atlas) = art_world(color);
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
        renderer.set_world(&gpu.device, &world);
        for k in [2u32, 3] {
            for at in [(0, 0), (1, 2)] {
                let cameras = art_cameras();
                let present = Present { canvas: (160, 120), zoom: (k, 1), at, target: (160 * k + 2 * at.0 as u32 + 1, 120 * k + 2 * at.1 as u32) };
                assert_eq!(present.pixel_box(), Some(k));
                let boxed: Vec<Camera> = cameras.iter().map(|c| c.boxed(k)).collect();
                let boxed_copies: Vec<ScreenCopy> = copies.iter().map(|c| c.boxed(k)).collect();
                let target = present.at_target();
                assert_eq!((target.canvas, target.zoom, target.at, target.target), ((160 * k, 120 * k), (1, 1), at, present.target));
                let what = format!("{color:?} at {k}x, at {at:?}");
                // Twice: the second time the picture under the overlay is reused.
                for frame in 0..2 {
                    let marker = PlacedSprite { screen: true, view: Some(5), ..PlacedSprite::new(world.free[1].set, 70 + 9 * frame, 50) };
                    renderer.set_markers(&gpu.device, &gpu.queue, &[marker]);
                    let old = gpu.render_layered(&mut renderer, &cameras, 5, &copies, present, 0, CLEAR);
                    let new = gpu.render_layered(&mut renderer, &boxed, 5, &boxed_copies, target, 0, CLEAR);
                    assert_same(&new, &old, &format!("{what}, frame {frame}"));
                    let again = gpu.render_layered(&mut renderer, &boxed, 5, &boxed_copies, target, 0, CLEAR);
                    assert!(renderer.reused(), "{what}: the same frame again draws only the overlay");
                    assert_same(&again, &old, &format!("{what}, frame {frame} again"));
                }
            }
        }
    }
}

/// At a whole pixel scale of 1 nothing changes: the boxes are no boxes.
#[test]
fn one_to_one_is_left_as_it_is() {
    let gpu = Headless::new().expect("GPU");
    let (world, atlas) = art_world(ColorModel::Quantized { bits: 5 });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let present = Present::unscaled((160, 120));
    assert_eq!((present.pixel_box(), present.at_target()), (None, present));
    let cameras = art_cameras();
    for c in &cameras {
        assert_eq!(c.boxed(1), *c);
    }
    let a = gpu.render_layered(&mut renderer, &cameras, 5, &[], present, 0, CLEAR);
    let b = gpu.render_layered(&mut renderer, &cameras.iter().map(|c| c.boxed(1)).collect::<Vec<_>>(), 5, &[], present.at_target(), 0, CLEAR);
    assert_same(&b, &a, "1:1");
    // The 1:1 raster twin is what shows at 1:1, its outline twin is not.
    let plate = (60 + 9, 40 - 5);
    // (Red 200 in 5 bits per channel, expanded: 206.)
    assert_eq!(a.get(plate.0, plate.1)[0], 206, "the 1:1 raster twin, not white outline text");
}

/// Runs of `text` from pen (`x`, `y`) at an em of `em` 1:1 pixels, each
/// glyph's advance `advance(c)` 1:1 pixels, in a view, with a raster 1:1
/// twin under each.
fn text_world(faces: &[Arc<Face>], runs: &[(u32, &str, (i32, i32), u32)], plate: SpriteSetId, sets: SpriteSets) -> TileWorld {
    let mut world = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (8, 8) }, sets);
    world.fonts = faces.to_vec();
    for &(font, text, (x, y), em) in runs {
        let face = &faces[font as usize];
        let mut pen = x;
        for c in text.chars() {
            let advance = ((face.advance(c).unwrap_or(500) as i64 * em as i64 + face.units_per_em() as i64 / 2) / face.units_per_em() as i64) as i32;
            world.free.push(PlacedSprite { screen: true, view: Some(0), zoom_art: ZoomArt::Low, ..PlacedSprite::new(plate, pen, y) });
            let g = OutlineGlyph { font, shape: GlyphShape::Glyph(face.glyph(c).unwrap_or(0)), em64: em * 64, advance, bold: false, italic: false, color: [255; 3], key: None };
            world.free.push(PlacedSprite { screen: true, view: Some(0), zoom_art: ZOOMED, text: Some(g), ..PlacedSprite::new(SpriteSetId::EMPTY, pen, y) });
            pen += advance;
        }
    }
    world
}

/// The ink bounds (left, top, right, bottom; exclusive) of the pixels in
/// `r` brighter than the background.
fn ink(image: &RgbaImage, [x0, y0, x1, y1]: [i32; 4]) -> Option<[i32; 4]> {
    let mut b: Option<[i32; 4]> = None;
    for y in y0.max(0)..y1.min(image.height as i32) {
        for x in x0.max(0)..x1.min(image.width as i32) {
            let p = image.get(x as u32, y as u32);
            if (0..3).any(|c| p[c] != CLEAR[c]) {
                b = Some(b.map_or([x, y, x + 1, y + 1], |[l, t, r, bb]| [l.min(x), t.min(y), r.max(x + 1), bb.max(y + 1)]));
            }
        }
    }
    b
}

/// Whether every `k`×`k` box of `r` (from the target's corner) is one
/// colour: a picture drawn at 1/k of the resolution and scaled.
fn boxed_picture(image: &RgbaImage, [x0, y0, x1, y1]: [i32; 4], k: i32) -> bool {
    (y0..y1).all(|y| (x0..x1).all(|x| image.get(x as u32, y as u32) == image.get((x - x.rem_euclid(k)) as u32, (y - y.rem_euclid(k)) as u32)))
}

/// Text in an interface on a canvas shown at 2×, 3×, 0.5× and 3× (a menu
/// at 3:2 on a canvas in 2×2 boxes) is drawn at the target's resolution:
/// the GPU draws the reference's pixels, the 1:1 raster twins do not show,
/// glyphs are as large as the em at that scale says (not a multiple of the
/// box), each where the 1:1 layout scaled puts it, within a pixel.
#[test]
fn interface_text_is_drawn_at_the_target_resolution() {
    let gpu = Headless::new().expect("GPU");
    let mut faces = vec![Arc::new(shapes())];
    faces.extend(test_font().map(Arc::new));
    let mut atlas = AtlasBuilder::new(64);
    let mut sets = SpriteSets::default();
    let plate = sets.single(atlas.add(&RgbaImage::from_fn(6, 9, |_, _| [255, 0, 0, 255]), (0, -9)).unwrap());
    let atlas = atlas.finish();
    // (Square glyphs alone, at 1:1 pens far apart, for their bounds.)
    let mut runs: Vec<(u32, &str, (i32, i32), u32)> = vec![(0, "s", (10, 30), 13), (0, "s", (40, 30), 16), (0, "s", (70, 30), 9), (0, "svs", (10, 60), 12)];
    if faces.len() > 1 {
        runs.extend([(1, "New Game", (10, 90), 14), (1, "Preferences", (80, 90), 11)]);
    }
    let world = text_world(&faces, &runs, plate, sets);
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    // (Canvas zoom, pixel box): 2×, 3×, 0.5×, 3×.
    for (zoom, k) in [((1, 1), 2u32), ((1, 1), 3), ((1, 4), 2), ((3, 2), 2)] {
        let scale = zoom.0 as f64 * k as f64 / zoom.1 as f64;
        let origin = (4, 6);
        // An interface camera of the canvas, as the layout makes it (in
        // driver pixels from its origin), showing outline text where its
        // scale on the target is not 1.
        let canvas_view = (180 * zoom.0 / zoom.1, 120 * zoom.0 / zoom.1);
        let camera = Camera { zoom, origin, cells: false, zoomed_art: true, hairline: zoom != (1, 1), ..Camera::new(canvas_view) };
        let canvas = (canvas_view.0 + 8, canvas_view.1 + 12);
        let present = Present { canvas, zoom: (k, 1), at: (0, 0), target: (canvas.0 * k, canvas.1 * k) };
        let boxed = [camera.boxed(k)];
        assert_eq!(boxed[0].zoom.0 as f64 / boxed[0].zoom.1 as f64, scale);
        assert_eq!((boxed[0].origin, boxed[0].origin.0 - boxed[0].screen_shift.0), ((4 * k as i32, 6 * k as i32), 4), "screen sprites keep their coordinates");
        let got = gpu.render_presented(&mut renderer, &boxed, &[], present.at_target(), 0, CLEAR);
        // (The canvas's own picture shown in boxes: its text in boxes too.)
        let boxes = gpu.render_presented(&mut renderer, &[camera], &[], present, 0, CLEAR);
        let want = reference::render_views(&world, &atlas, &boxed, present.target, 0, CLEAR);
        let d: Vec<i32> = got.pixels.iter().zip(&want.pixels).map(|(a, b)| (0..3).map(|c| (a[c] as i32 - b[c] as i32).abs()).max().unwrap()).collect();
        let what = format!("canvas zoom {zoom:?} in {k}x{k} boxes ({scale}x)");
        assert!(d.iter().all(|&v| v <= 1), "{what}: the GPU draws the reference's text ({} pixels differ)", d.iter().filter(|&&v| v > 0).count());
        assert!(got.pixels.iter().all(|p| !(p[0] == 255 && p[1] == 0 && p[2] == 0)), "{what}: no 1:1 raster twin");
        // Where 1:1 pixel p of the driver's interface shows on the target.
        let at = |p: (i32, i32)| -> (f64, f64) { ((origin.0 as f64 + (p.0 - origin.0) as f64 * zoom.0 as f64 / zoom.1 as f64) * k as f64, (origin.1 as f64 + (p.1 - origin.1) as f64 * zoom.0 as f64 / zoom.1 as f64) * k as f64) };
        for &(font, text, (x, y), em) in &runs[..3] {
            assert_eq!((font, text), (0, "s"));
            // The square: 0.55 em wide, 0.7 em high, centred in its scaled
            // 1:1 advance (650 units of the em, rounded), on the baseline.
            let advance = ((650 * em + 500) / 1000) as f64 * scale;
            let em_px = em as f64 * scale;
            let dark = if em_px < 12.0 { (12.0 - em_px) / 24.0 } else { 0.0 };
            let (pen_x, base) = at((x, y));
            let left = pen_x + (advance - 0.65 * em_px - dark) / 2.0 + 0.05 * em_px;
            let (right, top) = (left + 0.55 * em_px + dark, base - 0.7 * em_px);
            let around = [(pen_x - 4.0) as i32, (top - 4.0) as i32, (pen_x + advance + 4.0) as i32, (base + 4.0) as i32];
            let [l, t, r, b] = ink(&got, around).unwrap_or_else(|| panic!("{what}: {text} at {em}px drawn"));
            let what = format!("{what}: square of em {em} ({em_px} px)");
            assert!((l as f64 - left.floor()).abs() <= 1.0 && (r as f64 - right.ceil()).abs() <= 1.0, "{what}: ink x {l}..{r}, layout {left:.2}..{right:.2}");
            assert!((t as f64 - top.floor()).abs() <= 1.0 && (b as f64 - base.ceil()).abs() <= 1.0, "{what}: ink y {t}..{b}, layout {top:.2}..{base:.2}");
            if k > 1 && scale >= 1.0 {
                assert!(!boxed_picture(&got, [l, t, r, b], k as i32), "{what}: drawn at the target's resolution, not in {k}x{k} boxes");
                assert!(boxed_picture(&boxes, [l, t, r, b], k as i32), "{what}: (as the canvas shown in boxes draws it)");
            }
        }
        // A run spans its scaled 1:1 advances.
        let (_, _, (x, y), em) = runs[3];
        let width: i32 = "svs".chars().map(|c| { let a = faces[0].advance(c).unwrap() as i64; ((a * em as i64 + 500) / 1000) as i32 }).sum();
        let ((x0, base), (x1, _)) = (at((x, y)), at((x + width, y)));
        let [l, _, r, _] = ink(&got, [x0 as i32 - 4, (base - em as f64 * scale) as i32 - 4, x1 as i32 + 4, base as i32 + 4]).unwrap();
        assert!(l as f64 >= x0 - 1.0 && r as f64 <= x1 + 1.0, "{what}: run ink {l}..{r} within its scaled layout {x0}..{x1}");
    }
}
