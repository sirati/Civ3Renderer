//! Pure UI backgrounds opt into spatial sampling; every other pixel keeps
//! the renderer's ordinary exact nearest behaviour.

use tile_render::{Atlas, AtlasBuilder, BackgroundSampling, Blend, Camera, Grid, Headless, IndexImage, Layout, PlacedSprite, RgbaImage, SpriteSets, TileRenderer, TileWorld, reference};
use tile_render_shared::SAMPLING_UI_BACKGROUND;

const CLEAR: [u8; 4] = [17, 23, 31, 255];

fn pattern(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let value = if (x + y) % 2 == 0 { 0 } else { 255 };
        [value, value, value, 255]
    })
}

fn scene(image: &RgbaImage, origin: (u32, u32), tagged: bool) -> (TileWorld, Atlas) {
    let mut builder = AtlasBuilder::new(256);
    let id = builder.add(image, (0, 0)).unwrap();
    let desc = builder.sprite_mut(id);
    desc.sampling = if tagged { SAMPLING_UI_BACKGROUND } else { 0 };
    (desc.sampling_origin_x, desc.sampling_origin_y) = origin;
    let mut sets = SpriteSets::default();
    let set = sets.single(id);
    let grid = Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, step: (1, 1), wrap_x: false, wrap_y: false };
    let mut world = TileWorld::new(grid, sets);
    world.free.push(PlacedSprite { screen: true, view: Some(0), ..PlacedSprite::new(set, 0, 0) });
    (world, builder.finish())
}

fn camera(size: (u32, u32), zoom: (u32, u32)) -> Camera {
    Camera { cells: false, hairline: true, zoom, ..Camera::new(size) }
}

fn render(gpu: &Headless, world: &TileWorld, atlas: &Atlas, camera: &Camera, frame: u32) -> RgbaImage {
    render_within(gpu, world, atlas, camera, frame, 0)
}

/// GPU and CPU reference agree, each channel within `tolerance`: the
/// reference's blend is exact only for alpha 0 and 255, so translucent
/// draws may round one step apart.
fn render_within(gpu: &Headless, world: &TileWorld, atlas: &Atlas, camera: &Camera, frame: u32, tolerance: u8) -> RgbaImage {
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, atlas);
    renderer.set_world(&gpu.device, world);
    let actual = gpu.render(&mut renderer, camera, frame, CLEAR);
    let expected = reference::render(world, atlas, camera, frame, CLEAR);
    let differ: Vec<_> = (0..actual.pixels.len()).filter(|&i| (0..4).any(|k| actual.pixels[i][k].abs_diff(expected.pixels[i][k]) > tolerance)).collect();
    if let Some(&i) = differ.first() {
        let (x, y) = (i as u32 % actual.width, i as u32 / actual.width);
        panic!("GPU and CPU background sampling differ at {} pixels, first ({x}, {y}): GPU {:?}, CPU {:?}; camera {camera:?}", differ.len(), actual.pixels[i], expected.pixels[i]);
    }
    actual
}

#[test]
fn background_downsampling_breaks_pattern_aliasing_without_touching_controls() {
    let gpu = Headless::new().expect("GPU");
    let image = pattern(64, 64);
    let (mut world, atlas) = scene(&image, (0, 0), true);
    // A nearest-sampled icon and bitmap text use the exact same patterned
    // source art, declared separately without the background role.
    let mut atlas = atlas;
    let mut control = atlas.sprites[0];
    control.sampling = 0;
    let control_id = tile_render::SpriteId(atlas.sprites.len() as u32);
    atlas.sprites.push(control);
    let set = world.sets.single(control_id);
    world.free.push(PlacedSprite { screen: true, view: Some(0), clip: Some([0, 0, 16, 16]), ..PlacedSprite::new(set, 0, 0) });
    world.free.push(PlacedSprite { screen: true, view: Some(0), clip: Some([48, 0, 64, 16]), ..PlacedSprite::new(set, 0, 0) });
    let camera = camera((32, 32), (1, 2));
    let got = render(&gpu, &world, &atlas, &camera, 0);
    let mut nearest = atlas.clone();
    nearest.sprites[0].sampling = 0;
    let ordinary = render(&gpu, &world, &nearest, &camera, 0);
    for y in 0..8 {
        for x in (0..8).chain(24..32) {
            assert_eq!(got.get(x, y), ordinary.get(x, y), "a control was dithered");
        }
    }
    let white = (8..32).flat_map(|y| (0..32).map(move |x| (x, y))).filter(|&(x, y)| got.get(x, y)[0] == 255).count();
    assert!((280..490).contains(&white), "checkerboard should retain both colours: {white} of 768 white");
    assert!((8..32).all(|y| (0..32).all(|x| ordinary.get(x, y)[0] == 0)), "ordinary nearest must alias to black");
    // A frozen small sample guards the algorithm independently of the CPU
    // reference implementation and exposes changes in dither phase.
    let rows: Vec<String> = (8..16).map(|y| (0..8).map(|x| if got.get(x, y)[0] == 255 { '1' } else { '0' }).collect()).collect();
    assert_eq!(rows, ["00011001", "10110001", "01100101", "01010000", "10011110", "10100010", "00111000", "01000100"]);
}

#[test]
fn background_dither_keeps_native_scale_and_world_art_exact() {
    let gpu = Headless::new().expect("GPU");
    let (world, atlas) = scene(&pattern(32, 32), (17, 29), true);
    let mut nearest = atlas.clone();
    nearest.sprites[0].sampling = 0;
    for zoom in [(1, 1)] {
        let camera = camera((48, 48), zoom);
        assert_eq!(render(&gpu, &world, &atlas, &camera, 0).pixels, render(&gpu, &world, &nearest, &camera, 0).pixels, "zoom {zoom:?}");
    }
    let mut world_art = world.clone();
    world_art.free[0].screen = false;
    let camera = Camera { cells: true, ..camera((16, 16), (1, 2)) };
    assert_eq!(render(&gpu, &world_art, &atlas, &camera, 0).pixels, render(&gpu, &world_art, &nearest, &camera, 0).pixels);
}

#[test]
fn clipped_cropped_moved_and_stacked_backgrounds_keep_their_source_phase() {
    let gpu = Headless::new().expect("GPU");
    let image = pattern(32, 32);
    let (world, atlas) = scene(&image, (0, 0), true);
    let base_camera = camera((32, 32), (1, 2));
    let full = render(&gpu, &world, &atlas, &base_camera, 0);
    let stable = render(&gpu, &world, &atlas, &base_camera, 9_999);
    assert_eq!(full.pixels, stable.pixels, "a static background must not flicker");
    let mut clipped = world.clone();
    clipped.free[0].clip = Some([8, 8, 24, 24]);
    let clipped_image = render(&gpu, &clipped, &atlas, &base_camera, 0);
    let (mut cropped, crop_atlas) = scene(&image.crop(8, 8, 16, 16), (8, 8), true);
    (cropped.free[0].x, cropped.free[0].y) = (8, 8);
    let cropped_image = render(&gpu, &cropped, &crop_atlas, &base_camera, 0);
    assert_eq!(clipped_image.pixels, cropped_image.pixels, "a copied source crop changed the pattern");
    for y in 4..12 { for x in 4..12 { assert_eq!(clipped_image.get(x, y), full.get(x, y)); } }
    let moved_camera = Camera { origin: (3, 5), screen_shift: (3, 5), viewport: (16, 16), ..base_camera };
    let moved = render(&gpu, &world, &atlas, &moved_camera, 0);
    // Headless render uses the camera viewport as its target, so inspect
    // the visible common rectangle after the destination origin changes.
    for y in 0..11 { for x in 0..13 { assert_eq!(moved.get(x + 3, y + 5), full.get(x, y)); } }
    let mut stacked_world = world.clone();
    stacked_world.free[0].view = Some(1);
    let head = Camera { cells: false, ..base_camera };
    let child = Camera { stacked: true, zoom: (1, 1), viewport: (64, 64), ..camera((64, 64), (1, 1)) };
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &stacked_world);
    let stacked = gpu.render_views(&mut renderer, &[head, child], (32, 32), 0, CLEAR);
    assert_eq!(stacked.pixels, full.pixels);
    assert_eq!(stacked.pixels, reference::render_views(&stacked_world, &atlas, &[head, child], (32, 32), 0, CLEAR).pixels);
}

#[test]
fn fractional_downsampling_preserves_nearest_alpha_masks_and_source_crop_bounds() {
    let gpu = Headless::new().expect("GPU");
    let image = RgbaImage::from_fn(31, 29, |x, y| {
        // RGB encodes the source coordinate, exposing an out-of-crop read.
        [x as u8, y as u8, 71, match (x + 2 * y) % 5 { 0 => 0, 1 => 117, _ => 255 }]
    });
    let (mut world, atlas) = scene(&image, (91, 37), true);
    world.free[0].clip = Some([3, 5, 30, 28]);
    let mut nearest = atlas.clone();
    nearest.sprites[0].sampling = 0;
    for zoom in [(2, 3), (3, 7), (1, 4)] {
        let camera = camera((24, 24), zoom);
        for blend in [Blend::Alpha, Blend::AlphaOpacity { opacity: 137 }] {
            world.free[0].blend = blend;
            let got = render(&gpu, &world, &atlas, &camera, 0);
            let ordinary = render(&gpu, &world, &nearest, &camera, 0);
            for y in 0..24 { for x in 0..24 {
                let a = got.get(x, y);
                let b = ordinary.get(x, y);
                assert_eq!(a == CLEAR, b == CLEAR, "mask changed at ({x}, {y}) zoom {zoom:?}");
                // Every source texel has the same blue channel. Its exact
                // blend therefore proves that both partial source alpha
                // and uniform opacity stay at the nominal nearest sample.
                assert_eq!((a[2], a[3]), (b[2], b[3]), "partial alpha changed at ({x}, {y}) zoom {zoom:?}");
                if a != CLEAR && blend == Blend::Alpha && b[2] == 71 {
                    assert!(a[0] < 31 && a[1] < 29 && a[2] == 71, "source crop leaked: {a:?}");
                }
            } }
        }
    }
}

fn with_mode(camera: Camera, mode: BackgroundSampling) -> Camera {
    Camera { background_sampling: mode, ..camera }
}

#[test]
fn interpolated_background_is_the_box_mean_and_nearest_mode_is_plain_nearest() {
    let gpu = Headless::new().expect("GPU");
    let (world, atlas) = scene(&pattern(64, 64), (5, 9), true);
    let mut plain = atlas.clone();
    plain.sprites[0].sampling = 0;
    let base = camera((32, 32), (1, 2));
    // Every target pixel covers two black and two white texels exactly.
    let smooth = render(&gpu, &world, &atlas, &with_mode(base, BackgroundSampling::Interpolate), 0);
    assert!(smooth.pixels.iter().all(|p| *p == [128, 128, 128, 255]), "2x2 box mean of a checkerboard");
    let nearest = render(&gpu, &world, &atlas, &with_mode(base, BackgroundSampling::Nearest), 0);
    assert_eq!(nearest.pixels, render(&gpu, &world, &plain, &base, 0).pixels);
    // At 1:1 every mode is nearest; explicit nearest stays exact at any scale.
    for zoom in [(1, 1), (2, 1), (3, 2)] {
        let camera = camera((48, 48), zoom);
        let ordinary = render(&gpu, &world, &plain, &camera, 0);
        assert_eq!(render(&gpu, &world, &atlas, &with_mode(camera, BackgroundSampling::Nearest), 0).pixels, ordinary.pixels, "nearest at {zoom:?}");
        if zoom == (1, 1) {
            assert_eq!(render(&gpu, &world, &atlas, &with_mode(camera, BackgroundSampling::Interpolate), 0).pixels, ordinary.pixels);
        }
    }
    // An untagged sprite ignores the mode.
    assert_eq!(render(&gpu, &world, &plain, &with_mode(base, BackgroundSampling::Interpolate), 0).pixels, render(&gpu, &world, &plain, &base, 0).pixels);
}


/// The exact area-weighted mean red and green of the opaque texels of
/// `image` under target pixel (`x`, `y`) at `zoom`, from texel `first` on.
fn box_mean(image: &RgbaImage, (n, d): (u32, u32), (x, y): (u32, u32), first: (u32, u32)) -> [u8; 2] {
    let span = |p: u32| (p as f64 * d as f64 / n as f64, (p + 1) as f64 * d as f64 / n as f64);
    let ((x0, x1), (y0, y1)) = (span(x), span(y));
    let (mut sum, mut weight) = ([0.0f64; 2], 0.0f64);
    for ty in first.1..image.height { for tx in first.0..image.width {
        let w = ((tx + 1) as f64).min(x1) - (tx as f64).max(x0);
        let h = ((ty + 1) as f64).min(y1) - (ty as f64).max(y0);
        let texel = image.get(tx, ty);
        if w > 0.0 && h > 0.0 && texel[3] != 0 {
            sum[0] += w * h * texel[0] as f64;
            sum[1] += w * h * texel[1] as f64;
            weight += w * h;
        }
    } }
    [(sum[0] / weight).round() as u8, (sum[1] / weight).round() as u8]
}
#[test]
fn interpolation_is_exact_at_fractional_zooms_without_mask_bleed_or_crop_leaks() {
    let gpu = Headless::new().expect("GPU");
    // Opaque texels encode 8× their coordinate in red/green, blue 71.
    // Cleared (alpha 0) texels carry red 255 / blue 0, which must never
    // bleed into the mean. Partial alpha, where present, stays the nominal
    // sample's; the reference blends it within one step of the GPU.
    for partial in [false, true] {
        let image = RgbaImage::from_fn(31, 29, |x, y| match (x + 2 * y) % 5 {
            0 => [255, 255, 0, 0],
            1 if partial => [x as u8, y as u8, 71, 117],
            _ => [8 * x as u8, 8 * y as u8, 71, 255],
        });
        let (mut world, atlas) = scene(&image, (91, 37), true);
        world.free[0].clip = Some([3, 5, 30, 28]);
        let mut plain = atlas.clone();
        plain.sprites[0].sampling = 0;
        for zoom in [(2, 3), (3, 7), (1, 4), (5, 8), (1, 3), (2, 1), (3, 2), (5, 4)] {
            let base = camera((24, 24), zoom);
            for blend in [Blend::Alpha, Blend::AlphaOpacity { opacity: 137 }] {
                world.free[0].blend = blend;
                let tolerance = (partial || blend != Blend::Alpha) as u8;
                let ordinary = render_within(&gpu, &world, &plain, &base, 0, tolerance);
                let smooth = render_within(&gpu, &world, &atlas, &with_mode(base, BackgroundSampling::Interpolate), 0, tolerance);
                let mut averaged = 0;
                for y in 0..24 { for x in 0..24 {
                    let (a, b) = (smooth.get(x, y), ordinary.get(x, y));
                    assert_eq!(a == CLEAR, b == CLEAR, "mask changed at ({x}, {y}) zoom {zoom:?}");
                    if a != CLEAR && blend == Blend::Alpha && b[2] == 71 {
                        assert_eq!(a[2], 71, "a cleared texel bled into ({x}, {y}) zoom {zoom:?}: {a:?}");
                        assert!(a[0] <= 240 && a[1] <= 224, "source crop leaked: {a:?}");
                        averaged += (a != b) as u32;
                        if !partial {
                            let expected = if zoom.0 > zoom.1 { bilinear_mean(&image, zoom, (x, y)) } else { box_mean(&image, zoom, (x, y), (3, 5)) };
                            assert_eq!([a[0], a[1]], expected, "not the independent filtered mean at ({x}, {y}) zoom {zoom:?}");
                        }
                    }
                } }
                if blend == Blend::Alpha { assert!(averaged > 20, "zoom {zoom:?}: only {averaged} pixels averaged"); }
            }
        }
    }
}

#[test]
fn interpolation_resolves_indexed_palettes_before_averaging() {
    let gpu = Headless::new().expect("GPU");
    let mut builder = AtlasBuilder::new(64);
    // Index 0 transparent; indices 1 and 2 opaque black and white.
    let id = builder.add_indexed(&IndexImage::from_fn(40, 40, |x, y| if x % 7 == 3 { 0 } else { 1 + ((x + y) % 2) as u8 }), (0, 0), 1, 0).unwrap();
    let desc = builder.sprite_mut(id);
    desc.sampling = SAMPLING_UI_BACKGROUND;
    let mut atlas = builder.finish();
    atlas.palettes = vec![0; 512];
    atlas.palettes[256] = tile_render::atlas::palette_entry([255, 0, 0, 0]);
    atlas.palettes[257] = tile_render::atlas::palette_entry([0, 0, 0, 255]);
    atlas.palettes[258] = tile_render::atlas::palette_entry([250, 250, 250, 255]);
    let mut sets = SpriteSets::default();
    let set = sets.single(id);
    let grid = Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, step: (1, 1), wrap_x: false, wrap_y: false };
    let mut world = TileWorld::new(grid, sets);
    world.free.push(PlacedSprite { screen: true, view: Some(0), ..PlacedSprite::new(set, 0, 0) });
    for zoom in [(1, 2), (2, 5), (3, 8), (2, 1), (3, 2)] {
        let base = camera((20, 20), zoom);
        let smooth = render(&gpu, &world, &atlas, &with_mode(base, BackgroundSampling::Interpolate), 0);
        let grey = smooth.pixels.iter().filter(|p| **p != CLEAR && p[0] > 0 && p[0] < 250).count();
        assert!(grey > 50, "zoom {zoom:?}: indexed background was not averaged ({grey} grey pixels)");
        assert!(smooth.pixels.iter().all(|p| *p == CLEAR || (p[0] == p[1] && p[1] == p[2])), "transparent palette colour bled at {zoom:?}");
    }
}


/// Independent centre-based bilinear oracle. Quantize weights to 64ths,
/// then average only nontransparent neighbours, preserving nominal alpha.
fn bilinear_mean(image: &RgbaImage, zoom: (u32, u32), pixel: (u32, u32)) -> [u8; 2] {
    let axis = |p: u32, bound: u32| {
        let source = (p as f64 + 0.5) * zoom.1 as f64 / zoom.0 as f64 - 0.5;
        let left = source.floor();
        let right_weight = ((source - left) * 64.0).round() as u32;
        [(left.max(0.0).min((bound - 1) as f64) as u32, 64 - right_weight),
            ((left + 1.0).max(0.0).min((bound - 1) as f64) as u32, right_weight)]
    };
    let (mut sum, mut total) = ([0u32; 2], 0u32);
    for (sy, wy) in axis(pixel.1, image.height) { for (sx, wx) in axis(pixel.0, image.width) {
        let t = image.get(sx, sy); let weight = wx * wy;
        if t[3] != 0 { for k in 0..2 { sum[k] += weight * u32::from(t[k]); } total += weight; }
    } }
    assert!(total > 0);
    [((sum[0] + total / 2) / total) as u8, ((sum[1] + total / 2) / total) as u8]
}

#[test]
fn magnified_background_modes_preserve_separate_controls_and_change_texture_at_integer_scales() {
    let gpu = Headless::new().expect("GPU");
    let (mut world, mut atlas) = scene(&pattern(32, 32), (17, 29), true);
    let mut control = atlas.sprites[0]; control.sampling = 0;
    let control_id = tile_render::SpriteId(atlas.sprites.len() as u32); atlas.sprites.push(control);
    let control_set = world.sets.single(control_id);
    world.free.push(PlacedSprite { screen: true, view: Some(0), clip: Some([0, 0, 12, 8]), ..PlacedSprite::new(control_set, 0, 0) });
    let mut plain = atlas.clone(); plain.sprites[0].sampling = 0;
    for zoom in [(2, 1), (3, 1), (3, 2), (5, 4), (7, 3)] {
        let base = camera((48, 48), zoom);
        let nearest = render(&gpu, &world, &plain, &base, 0);
        for mode in [BackgroundSampling::Dither, BackgroundSampling::Interpolate] {
            let c = with_mode(base, mode); let got = render(&gpu, &world, &atlas, &c, 0);
            assert_eq!(got.pixels, render(&gpu, &world, &atlas, &c, 9_999).pixels, "magnified background flickers");
            let mut changed = 0;
            for y in 0..48 { for x in 0..48 {
                if x * zoom.1 / zoom.0 < 12 && y * zoom.1 / zoom.0 < 8 {
                    assert_eq!(got.get(x,y), nearest.get(x,y), "{mode:?} blurred a control at ({x},{y}), {zoom:?}");
                } else if got.get(x,y) != nearest.get(x,y) { changed += 1; }
            } }
            assert!(changed > 50, "{mode:?} stayed nearest at {zoom:?}, changed {changed}");
            assert_eq!(render(&gpu, &world, &plain, &c, 0).pixels, nearest.pixels, "untagged art changed");
            eprintln!("magnified sampling {mode:?}, zoom{zoom:?}: {changed} background pixels changed; controls exact");
        }
    }
}

#[test]
fn integer_magnification_has_known_bilinear_colours_instead_of_replicated_nearest_texels() {
    let gpu = Headless::new().expect("GPU");
    let (mut world, atlas) = scene(&pattern(2,2), (0,0), true);
    let c = with_mode(camera((4,4), (2,1)), BackgroundSampling::Interpolate);
    for (model, colours) in [(tile_render::ColorModel::Direct, [0,64,191,96,159]),
        (tile_render::ColorModel::Quantized { bits: 5 }, [0,66,189,99,156])] {
        world.color = model;
        let got = render(&gpu, &world, &atlas, &c, 0);
        for ((x,y), value) in [(0,0),(1,0),(2,0),(1,1),(2,1)].into_iter().zip(colours) {
            assert_eq!(got.get(x,y), [value,value,value,255], "known bilinear colour at ({x},{y}), {model:?}");
        }
    }
}

#[test]
fn scaled_background_colour_keeps_outline_text_and_opaque_ui_plate_exact() {
    let gpu = Headless::new().expect("GPU");
    let (mut world, atlas) = scene(&pattern(64,64), (0,0), true);
    let bytes = std::fs::read("/mnt/datasets/winvm/civ3-src/Civilization III Complete/LSANS.TTF").expect("existing real font fixture");
    let face = std::sync::Arc::new(tile_render::gpu_text::Face::parse(&bytes,0).unwrap());
    let shape = tile_render::GlyphShape::Glyph(face.glyph('G').unwrap()); world.fonts.push(face);
    world.free.push(PlacedSprite { screen: true, view: Some(0), solid: Some(tile_render::SolidRect { width: 36, height: 24, rgba: [249,244,211,255] }),
        ..PlacedSprite::new(tile_render::SpriteSetId::EMPTY,2,8) });
    world.free.push(PlacedSprite { screen: true, view: Some(0), zoom_art: tile_render::ZoomArt::Any,
        text: Some(tile_render::OutlineGlyph { font: 0, shape, em64: 14*64, advance: 16, bold: true, italic: false, color: [27,39,51], key: None }),
        ..PlacedSprite::new(tile_render::SpriteSetId::EMPTY,6,25) });
    for zoom in [(2,3),(1,1),(3,2),(2,1)] {
        let base = Camera { zoomed_art: true, ..camera((128,96), zoom) };
        let nearest = render_within(&gpu, &world, &atlas, &with_mode(base, BackgroundSampling::Nearest), 0, 1);
        for mode in [BackgroundSampling::Dither, BackgroundSampling::Interpolate] {
            let got = render_within(&gpu, &world, &atlas, &with_mode(base, mode), 0, 1);
            let mut ink = 0;
            for y in 0..96 { for x in 0..128 {
                let (sx,sy) = (x*zoom.1/zoom.0,y*zoom.1/zoom.0);
                if (2..38).contains(&sx) && (8..32).contains(&sy) {
                    assert_eq!(got.get(x,y), nearest.get(x,y), "{mode:?} changed outlined text or its UI plate at ({x},{y}), {zoom:?}");
                    ink += usize::from(nearest.get(x,y)[0] < 200);
                }
            } }
            assert!(ink > 3, "outline text fixture must contain visible ink");
        }
    }
}
