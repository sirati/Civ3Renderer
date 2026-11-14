//! Shapes laid out at the scale of the stage drawing them
//! (`tile_render::Shape`): output rectangles exactly as the reference
//! oracle draws them (the painted selection marker's are the marker crate's).
use tile_render::{AtlasBuilder, Camera, Grid, Headless, Layout, OutputRect, PlacedSprite, RgbaImage, Shape, SpriteSetId, SpriteSets, TileRenderer, TileWorld};

fn world() -> TileWorld {
    TileWorld::new(Grid { cols: 4, rows: 4, layout: Layout::Orthogonal, step: (32, 32), wrap_x: false, wrap_y: false }, SpriteSets::default())
}

fn shape(x: i32, y: i32, shape: Shape) -> PlacedSprite {
    // Deliberately invalid atlas/set metadata: shapes must not read it.
    PlacedSprite { shape: Some(shape), palette: u32::MAX, ..PlacedSprite::new(SpriteSetId::from_raw(u32::MAX), x, y) }
}

/// A health bar's background and segments as a piece's HUD lays them out:
/// x1 1, x2 2, y1 1, y2 4.
fn bar(x: i32, y: i32, scale: u16, segments: i8) -> Vec<PlacedSprite> {
    let lengths = [1, 2, 1, 4];
    let r = |rgba, ey| shape(x, y, Shape::Rect(OutputRect { rgba, lengths, x: [[-1, 0], [-1, 1]], y: ey, scale: (scale, scale) }));
    let mut out = vec![r([0, 0, 0, 255], [[-2, -3], [0, 0]])];
    out.extend((0..segments).map(|k| r([0, 255, 0, 255], [[-k, -k - 1], [-k, -k]])));
    out
}

fn compare(gpu: &Headless, world: &TileWorld, cameras: &[Camera], tolerance: u8) {
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_world(&gpu.device, world);
    let atlas = AtlasBuilder::new(16).finish();
    for camera in cameras {
        let got = gpu.render(&mut renderer, camera, 0, [40, 60, 80, 255]);
        let want = tile_render::reference::render(world, &atlas, camera, 0, [40, 60, 80, 255]);
        let mut off = 0;
        for y in 0..got.height {
            for x in 0..got.width {
                let (a, b) = (got.get(x, y), want.get(x, y));
                let diff = (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap();
                assert!(diff <= tolerance, "zoom {:?}, zoomed art {}, downscale {}: pixel {x},{y} {a:?}, reference {b:?}", camera.zoom, camera.zoomed_art, camera.downscale);
                off += usize::from(diff != 0);
            }
        }
        assert!(off * 50 <= (got.width * got.height) as usize, "zoom {:?}: {off} pixels differ", camera.zoom);
    }
}

fn cameras(size: (u32, u32)) -> Vec<Camera> {
    let mut out = Vec::new();
    for zoom in [(1, 1), (2, 1), (3, 1), (3, 2), (5, 2)] {
        for zoomed_art in [false, true] {
            for downscale in [1, 2] {
                out.push(Camera { x: 3, y: -5, zoom, zoomed_art, downscale, ..Camera::new(size) });
            }
        }
    }
    out
}

#[test]
fn output_rectangles_match_the_reference_at_every_stage() {
    let gpu = Headless::new().expect("GPU");
    let mut world = world();
    for (i, scale) in [16, 24, 32, 48].into_iter().enumerate() {
        world.free.extend(bar(14 + 15 * i as i32, 40, scale, 3));
    }
    // Clipped, and on the screen.
    world.free.push(PlacedSprite { clip: Some([0, 0, 70, 37]), ..bar(75, 44, 24, 2).remove(0) });
    world.free.push(PlacedSprite { screen: true, ..bar(10, 12, 32, 1).remove(1) });
    compare(&gpu, &world, &cameras((180, 120)), 0);
}

/// The widths of the runs of each colour along column `x` of `image`.
fn runs(image: &RgbaImage, x: u32) -> Vec<([u8; 4], u32)> {
    let mut out: Vec<([u8; 4], u32)> = Vec::new();
    for y in 0..image.height {
        let c = image.get(x, y);
        match out.last_mut() {
            Some((last, n)) if *last == c => *n += 1,
            _ => out.push((c, 1)),
        }
    }
    out
}

#[test]
fn segments_and_gaps_keep_one_width_at_the_views_resolution() {
    let atlas = AtlasBuilder::new(16).finish();
    for scale in [24u16, 32, 48] {
        for zoom in [(2, 1), (3, 1), (3, 2), (5, 2)] {
            let mut world = world();
            world.free.extend(bar(40, 60, scale, 3));
            let camera = Camera { zoom, zoomed_art: true, ..Camera::new((120 * zoom.0 / zoom.1, 80 * zoom.0 / zoom.1)) };
            let image = tile_render::reference::render(&world, &atlas, &camera, 0, [40, 60, 80, 255]);
            let e = f64::from(scale) / 16.0 * f64::from(zoom.0) / f64::from(zoom.1);
            let column = (0..image.width).find(|&x| (0..image.height).any(|y| image.get(x, y) == [0, 255, 0, 255])).unwrap();
            let runs: Vec<_> = runs(&image, column).into_iter().filter(|(c, _)| *c != [40, 60, 80, 255]).collect();
            // Three segments a gap apart, the bar full (no gap above).
            let segment = (4.0 * e + 0.5).floor() as u32;
            let colours: Vec<_> = runs.iter().map(|r| r.0).collect();
            assert_eq!(colours, [[0, 255, 0, 255], [0, 0, 0, 255], [0, 255, 0, 255], [0, 0, 0, 255], [0, 255, 0, 255]], "scale {scale}, zoom {zoom:?}");
            let widths: Vec<_> = runs.iter().map(|r| r.1).collect();
            assert!(widths[0] == widths[2] && widths[2] == widths[4], "scale {scale}, zoom {zoom:?}: segments {widths:?}");
            assert_eq!(widths[1], widths[3], "scale {scale}, zoom {zoom:?}: gaps {widths:?}");
            assert_eq!(widths[0], segment.max(1), "scale {scale}, zoom {zoom:?}: a segment {e} times 4 pixels");
            assert_eq!(widths[1], ((e + 0.5).floor() as u32).max(1), "scale {scale}, zoom {zoom:?}: a gap {e} pixels");
            // Across: the bar is 2·e wide.
            let row = (0..image.height).find(|&y| image.get(column, y) == [0, 255, 0, 255]).unwrap();
            let across = (0..image.width).filter(|&x| image.get(x, row) == [0, 255, 0, 255]).count() as f64;
            assert_eq!(across, (2.0 * e + 0.5).floor(), "scale {scale}, zoom {zoom:?}");
        }
    }
}

/// The tile renderer's independent GPU-test shader module.
const SHADERS: tile_render::Shaders = tile_render::Shaders::new(tile_render_test_spirv::SPIRV, tile_render_test_spirv::SPIRV_FALLBACK);
