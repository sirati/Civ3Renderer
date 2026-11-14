//! Palette bytes quantize before each ordered RGB555 alpha blend.
use tile_render::{AtlasBuilder, Camera, ColorModel, Grid, Headless, IndexImage, LayerSpec, Layout, PlacedSprite, RgbaImage, SpriteSets, TileRenderer, TileWorld};

#[test]
fn all_palette_channel_bytes_and_critical_alpha_preserve_integer_order() {
    let gpu = Headless::new().expect("GPU");
    eprintln!("numeric oracle adapter: {:?}", gpu.adapter.get_info());
    let mut builder = AtlasBuilder::new(512);
    let base = builder.add(&RgbaImage::from_fn(256, 32, |_, y| [(y * 8) as u8, ((31 - y) * 8) as u8, ((y * 3 % 32) * 8) as u8, 255]), (0, 0)).unwrap();
    let over = builder.add_indexed(&IndexImage::from_fn(256, 32, |x, _| x as u8), (0, 0), 1, 0).unwrap();
    let mut atlas = builder.finish();
    atlas.palettes = tile_render::atlas::no_palettes();
    atlas.palettes.extend([0; 256]);
    let mut sets = SpriteSets::default();
    let base = sets.single(base);
    let over = sets.single(over);
    let mut world = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, step: (1, 1), wrap_x: false, wrap_y: false }, sets);
    world.color = ColorModel::Quantized { bits: 5 };
    world.free = vec![PlacedSprite { screen: true, ..PlacedSprite::new(base, 0, 0) },
        PlacedSprite { screen: true, ..PlacedSprite::new(over, 0, 0) },
        PlacedSprite { screen: true, ..PlacedSprite::new(over, 0, 0) }];
    let mut cells = world.clone();
    cells.free.clear();
    cells.add_layer(LayerSpec::new("base", vec![base]));
    cells.add_layer(LayerSpec::new("first", vec![over]));
    cells.add_layer(LayerSpec::new("second", vec![over]));
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    let camera = Camera::new((256, 32));
    let mut checked = 0;
    // Return from opaque to translucent too: deduplicated opaque runs must
    // regain their original ordered instances after a palette change.
    for (kind, world) in [("free", &world), ("cells", &cells)] {
    renderer.set_world(&gpu.device, world);
    for alpha in [0u8, 1, 31, 32, 127, 128, 254, 255, 128, 0] {
        for x in 0..256usize {
            atlas.palettes[256 + x] = u32::from_le_bytes([x as u8, (255 - x) as u8, (x as u8).wrapping_mul(13).wrapping_add(5), alpha]);
        }
        renderer.set_palettes(&gpu.device, &gpu.queue, &atlas.palettes);
        let got = gpu.render(&mut renderer, &camera, 0, [13, 23, 37, 255]);
        let k = (u32::from(alpha) * 32 + 127) / 255;
        for y in 0..32u32 { for x in 0..256u32 {
            let src = [x, 255 - x, u32::from((x as u8).wrapping_mul(13).wrapping_add(5))];
            let dst = [y, 31 - y, y * 3 % 32];
            let mut want = [0; 4];
            for channel in 0..3 {
                let src = src[channel] >> 3;
                let first = (src * k + dst[channel] * (32 - k)) / 32;
                let second = (src * k + first * (32 - k)) / 32;
                want[channel] = ((second << 3) | (second >> 2)) as u8;
            }
            want[3] = 255;
            assert_eq!(got.get(x, y), want, "{kind}, alpha={alpha}, sourcebyte={x}, destinationlevel={y}");
            checked += 1;
        } }
    }
    }
    eprintln!("palette byte/order oracle checked {checked} pixels");
}

/// The tile renderer's independent GPU-test shader module.
const SHADERS: tile_render::Shaders = tile_render::Shaders::new(tile_render_test_spirv::SPIRV, tile_render_test_spirv::SPIRV_FALLBACK);
