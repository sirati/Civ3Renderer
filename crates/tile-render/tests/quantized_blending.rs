use tile_render::{
    AtlasBuilder, Camera, ColorModel, Grid, Headless, Layout, PlacedSprite, RgbaImage, SpriteSets,
    TileRenderer, TileWorld, reference,
};

fn setup(overlays: usize) -> (Headless, tile_render::Atlas, TileWorld, tile_render::SpriteId, TileRenderer, Camera) {
    let gpu = Headless::new().expect("GPU");
    let mut builder = AtlasBuilder::new(64);
    let base = builder
        .add(
            &RgbaImage::from_fn(32, 32, |_, y| {
                [(y * 8) as u8, (y * 8) as u8, (y * 8) as u8, 255]
            }),
            (0, 0),
        )
        .unwrap();
    let over = builder
        .add(
            &RgbaImage::from_fn(32, 32, |x, _| {
                [(x * 8) as u8, (x * 8) as u8, (x * 8) as u8, 255]
            }),
            (0, 0),
        )
        .unwrap();
    let atlas = builder.finish();
    let mut sets = SpriteSets::default();
    let (base_set, over_set) = (sets.single(base), sets.single(over));
    let mut world = TileWorld::new(
        Grid {
            cols: 1,
            rows: 1,
            layout: Layout::Orthogonal,
            step: (1, 1),
            wrap_x: false,
            wrap_y: false,
        },
        sets,
    );
    world.color = ColorModel::Quantized { bits: 5 };
    world.free.push(PlacedSprite::new(base_set, 0, 0));
    world.free.extend((0..overlays).map(|_| PlacedSprite::new(over_set, 0, 0)));
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let camera = Camera::new((32, 32));
    (gpu, atlas, world, over, renderer, camera)
}

#[test]
fn every_five_bit_colour_and_alpha_step_blends_exactly() {
    let (gpu, mut atlas, world, over, mut renderer, camera) = setup(1);
    for k in 1..32u32 {
        let alpha = ((k * 255 + 16) / 32) as u8;
        let image = RgbaImage::from_fn(32, 32, |x, _| {
            [(x * 8) as u8, (x * 8) as u8, (x * 8) as u8, alpha]
        });
        assert!(atlas.patch(over, &image));
        assert!(renderer.patch_sprite(&gpu.queue, over, &image));
        let got = gpu.render(&mut renderer, &camera, 0, [0, 0, 0, 255]);
        let want = reference::render(&world, &atlas, &camera, 0, [0, 0, 0, 255]);
        for (i, (got, want)) in got.pixels.iter().zip(&want.pixels).enumerate() {
            assert_eq!(got, want, "k {k}, src {}, dst {}", i % 32, i / 32);
        }
    }
}

/// Each draw must round to the game's integer colour before the next mix.
/// This oracle uses only integer arithmetic, independent of shared blending
/// helpers and the CPU renderer.
#[test]
fn successive_blends_preserve_every_five_bit_integer_boundary() {
    let (gpu, mut atlas, _world, over, mut renderer, camera) = setup(2);
    for k in 1..32u32 {
        let alpha = ((k * 255 + 16) / 32) as u8;
        let image = RgbaImage::from_fn(32, 32, |x, _| [(x * 8) as u8, (x * 8) as u8, (x * 8) as u8, alpha]);
        assert!(atlas.patch(over, &image));
        assert!(renderer.patch_sprite(&gpu.queue, over, &image));
        let got = gpu.render(&mut renderer, &camera, 0, [0, 0, 0, 255]);
        for y in 0..32u32 {
            for x in 0..32u32 {
                let once = (x * k + y * (32 - k)) / 32;
                let twice = (x * k + once * (32 - k)) / 32;
                let channel = ((twice << 3) | (twice >> 2)) as u8;
                assert_eq!(got.get(x, y), [channel, channel, channel, 255], "k {k}, src {x}, dst {y}");
            }
        }
    }
}
