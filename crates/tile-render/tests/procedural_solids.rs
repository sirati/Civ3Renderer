//! Numeric GPU goldens for commands that have no atlas or font dependency.
use tile_render::{AtlasBuilder, Camera, ColorModel, Grid, Headless, Layout, PlacedSprite, RgbaImage, SolidRect, SpriteSetId, SpriteSets, TileRenderer, TileWorld};

fn world() -> TileWorld {
    TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, step: (1, 1), wrap_x: false, wrap_y: false }, SpriteSets::default())
}

fn solid(x: i32, y: i32, width: u32, height: u32, rgba: [u8; 4]) -> PlacedSprite {
    // Deliberately invalid atlas/set metadata: solids must not read it.
    PlacedSprite { screen: true, solid: Some(SolidRect { width, height, rgba }), palette: u32::MAX,
        ..PlacedSprite::new(SpriteSetId::from_raw(u32::MAX), x, y) }
}

#[test]
fn atlas_free_solids_clip_and_zoom_with_direct_alpha() {
    let gpu = Headless::new().expect("GPU");
    eprintln!("procedural solid adapter: {:?}", gpu.adapter.get_info());
    let mut world = world();
    world.free.push(PlacedSprite { clip: Some([3, 1, 5, 5]), ..solid(2, 2, 4, 3, [255, 0, 0, 128]) });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_world(&gpu.device, &world); // No set_atlas call.
    for zoom in [(1, 1), (2, 1), (1, 2), (3, 2)] {
        let camera = Camera { zoom, ..Camera::new((20, 16)) };
        let got = gpu.render(&mut renderer, &camera, 0, [0, 0, 0, 255]);
        let empty_atlas = AtlasBuilder::new(16).finish();
        assert_eq!(got, tile_render::reference::render(&world, &empty_atlas, &camera, 0, [0, 0, 0, 255]), "reference oracle, zoom {:?}", camera.zoom);
        for y in 0..got.height {
            for x in 0..got.width {
                let (sx, sy) = (x * zoom.1 / zoom.0, y * zoom.1 / zoom.0);
                let want = if (3..5).contains(&sx) && (2..5).contains(&sy) { [128, 0, 0, 255] } else { [0, 0, 0, 255] };
                assert_eq!(got.get(x, y), want, "zoom {zoom:?}, pixel {x},{y}");
            }
        }
    }
}

#[test]
fn successive_quantized_solids_preserve_each_integer_boundary() {
    let gpu = Headless::new().expect("GPU");
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    let camera = Camera::new((32, 32));
    for k in 1..32u32 {
        let mut world = world();
        world.color = ColorModel::Quantized { bits: 5 };
        for y in 0..32u32 {
            for x in 0..32u32 {
                world.free.push(solid(x as i32, y as i32, 1, 1, [(y * 8) as u8, 0, 0, 255]));
                let overlay = solid(x as i32, y as i32, 1, 1, [(x * 8) as u8, 0, 0, ((k * 255 + 16) / 32) as u8]);
                world.free.extend([overlay, overlay]);
            }
        }
        renderer.set_world(&gpu.device, &world);
        let got = gpu.render(&mut renderer, &camera, 0, [0, 0, 0, 255]);
        let empty_atlas = AtlasBuilder::new(16).finish();
        assert_eq!(got, tile_render::reference::render(&world, &empty_atlas, &camera, 0, [0, 0, 0, 255]), "reference oracle, zoom {:?}", camera.zoom);
        for y in 0..32u32 {
            for x in 0..32u32 {
                let once = (x * k + y * (32 - k)) / 32;
                let twice = (x * k + once * (32 - k)) / 32;
                let red = ((twice << 3) | (twice >> 2)) as u8;
                assert_eq!(got.get(x, y), [red, 0, 0, 255], "k={k}, src={x}, dst={y}");
            }
        }
    }
}

#[test]
fn solids_and_atlas_commands_keep_their_original_order() {
    let gpu = Headless::new().expect("GPU");
    let mut builder = AtlasBuilder::new(16);
    let blue = builder.add(&RgbaImage::from_fn(8, 4, |_, _| [0, 0, 255, 255]), (0, 0)).unwrap();
    let atlas = builder.finish();
    let mut world = world();
    let blue = world.sets.single(blue);
    world.free = vec![solid(0, 0, 8, 4, [255, 0, 0, 255]),
        PlacedSprite { screen: true, ..PlacedSprite::new(blue, 0, 0) },
        solid(4, 0, 4, 4, [0, 255, 0, 255])];
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    // Exercise adoption after an atlas-free frame as well.
    renderer.set_world(&gpu.device, &world);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    let got = gpu.render(&mut renderer, &Camera::new((8, 4)), 0, [0, 0, 0, 255]);
    for y in 0..4 {
        for x in 0..8 {
            assert_eq!(got.get(x, y), if x < 4 { [0, 0, 255, 255] } else { [0, 255, 0, 255] });
        }
    }
}

#[test]
fn world_plates_keep_their_size_while_their_anchor_shrinks() {
    let gpu = Headless::new().expect("GPU");
    let mut world = world();
    world.free.push(PlacedSprite { screen: false, ..solid(12, 8, 7, 5, [0, 255, 0, 255]) });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_world(&gpu.device, &world);
    for downscale in [1, 2] {
        let got = gpu.render(&mut renderer, &Camera { downscale, ..Camera::new((24, 16)) }, 0, [0, 0, 0, 255]);
        for y in 0..16 {
            for x in 0..24 {
                let (left, top) = (12 / downscale, 8 / downscale);
                let inside = (left..left + 7).contains(&x) && (top..top + 5).contains(&y);
                assert_eq!(got.get(x, y), if inside { [0, 255, 0, 255] } else { [0, 0, 0, 255] }, "downscale {downscale}, pixel {x},{y}");
            }
        }
    }
}

#[test]
fn atlas_free_solids_and_outline_text_keep_composition_order() {
    use std::sync::Arc;
    use tile_render::{GlyphShape, OutlineGlyph};
    let gpu = Headless::new().expect("GPU");
    let mut world = world();
    world.fonts.push(Arc::new(tile_render::gpu_text::Face::from_curves(1000, &[])));
    let bar = OutlineGlyph { font: 0, shape: GlyphShape::Bar { top: 0, height: 4 }, em64: 10 * 64,
        advance: 10, bold: false, italic: false, color: [0, 0, 255], key: None };
    world.free = vec![solid(0, 0, 12, 8, [255, 0, 0, 255]),
        PlacedSprite { screen: true, text: Some(bar), ..PlacedSprite::new(SpriteSetId::EMPTY, 1, 2) },
        solid(7, 0, 5, 8, [0, 255, 0, 255])];
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_world(&gpu.device, &world);
    let got = gpu.render(&mut renderer, &Camera::new((12, 8)), 0, [0, 0, 0, 255]);
    assert_eq!(got.get(0, 0), [255, 0, 0, 255], "background before the text");
    assert_eq!(got.get(4, 3), [0, 0, 255, 255], "text over the background");
    assert_eq!(got.get(8, 3), [0, 255, 0, 255], "later solid over the text");
}

/// The viewer's shader module (it holds the tile renderer's shaders).
const SHADERS: tile_render::Shaders = tile_render::Shaders::new(viewer_spirv::SPIRV, viewer_spirv::SPIRV_FALLBACK);
