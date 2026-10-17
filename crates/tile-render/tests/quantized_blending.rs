use tile_render::{
    AtlasBuilder, Camera, ColorModel, Grid, Headless, Layout, PlacedSprite, RgbaImage, SpriteSets,
    TileRenderer, TileWorld, reference,
};

#[test]
fn every_five_bit_colour_and_alpha_step_blends_exactly() {
    let gpu = Headless::new().expect("AMD GPU");
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
    let mut atlas = builder.finish();
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
    world.free.extend([
        PlacedSprite::new(base_set, 0, 0),
        PlacedSprite::new(over_set, 0, 0),
    ]);
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let camera = Camera::new((32, 32));
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
