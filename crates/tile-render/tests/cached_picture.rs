//! A retained canvas can be presented while its replacement art has no matching scene.
use tile_render::{
    Atlas, AtlasBuilder, Camera, ColorModel, Grid, Headless, Layout, PlacedSprite, Present,
    RgbaImage, ScreenCopy, SpriteSets, TileRenderer, TileWorld,
};

const SOURCE: (u32, u32) = (16, 12);
const CLEAR: [u8; 4] = [24, 40, 56, 255];
const SENTINEL: [u8; 4] = [3, 7, 11, 255];

fn fixture(color: ColorModel) -> (Atlas, TileWorld) {
    let mut builder = AtlasBuilder::new(16);
    let id = builder
        .add(
            &RgbaImage::from_fn(8, 8, |x, y| {
                if (x + y) % 5 == 0 {
                    [0; 4]
                } else {
                    [(x * 24 + 32) as u8, (y * 24 + 32) as u8, 176, 255]
                }
            }),
            (0, 0),
        )
        .unwrap();
    let mut atlas = builder.finish();
    atlas.trim_blank();
    let mut sets = SpriteSets::default();
    let set = sets.single(id);
    let grid = Grid {
        cols: 1,
        rows: 1,
        layout: Layout::Orthogonal,
        wrap_x: false,
        wrap_y: false,
        step: (8, 8),
    };
    let mut world = TileWorld::new(grid, sets);
    world.color = color;
    world.free.push(PlacedSprite::new(set, 2, 1));
    (atlas, world)
}

fn renderer(gpu: &Headless, atlas: &Atlas, world: &TileWorld) -> TileRenderer {
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_atlas(&gpu.device, &gpu.queue, atlas);
    renderer.set_world(&gpu.device, world);
    renderer
}

fn cached(
    gpu: &Headless,
    renderer: &TileRenderer,
    size: (u32, u32),
    copies: &[ScreenCopy],
    present: Present,
) -> (bool, RgbaImage) {
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cached picture test"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    // A false result must leave the caller's target untouched.
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &target.create_view(&Default::default()),
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(tile_render::gpu::color(SENTINEL)),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    }));
    let shown = renderer.render_cached_picture(
        &gpu.device,
        &gpu.queue,
        &mut encoder,
        &target,
        copies,
        CLEAR,
        present,
    );
    let image = tile_render::gpu::read_texture(&gpu.device, &gpu.queue, encoder, &target);
    renderer.controls_submitted();
    (shown, image)
}

fn assert_same(got: &RgbaImage, want: &RgbaImage, what: &str) {
    assert_eq!((got.width, got.height), (want.width, want.height));
    let width = want.width as usize;
    for (i, (got, want)) in got.pixels.iter().zip(&want.pixels).enumerate() {
        assert_eq!(got, want, "{what}: pixel ({}, {})", i % width, i / width);
    }
}

#[test]
fn cached_presentations_survive_incompatible_art_and_match_normal_scaling_and_copies() {
    let gpu = Headless::new().expect("AMD GPU");
    let copies = [
        ScreenCopy {
            source: [1, 0, 7, 6],
            at: (8, 3),
            zoom: (3, 2),
            lens: None,
        },
        ScreenCopy {
            source: [-2, 2, 8, 6],
            at: (-1, 5),
            zoom: (2, 1),
            lens: None,
        },
        ScreenCopy {
            source: [4, -1, 8, 7],
            at: (5, -2),
            zoom: (2, 3),
            lens: None,
        },
        ScreenCopy {
            source: [0, 0, 2, 2],
            at: (12, 9),
            zoom: (1, 1),
            lens: None,
        },
        // The public method must apply the same MAX_COPIES limit.
        ScreenCopy {
            source: [0, 0, 8, 8],
            at: (0, 0),
            zoom: (4, 1),
            lens: None,
        },
    ];
    for color in [ColorModel::Direct, ColorModel::Quantized { bits: 5 }] {
        let (atlas, world) = fixture(color);
        let mut retained = renderer(&gpu, &atlas, &world);
        let mut fresh = renderer(&gpu, &atlas, &world);
        let camera = Camera::new(SOURCE);
        // A copy forces composition for direct colour too. Copies are
        // presented from the canvas and must never become part of it.
        let _ = gpu.render_composed(&mut retained, &[camera], &copies[..1], SOURCE, 0, CLEAR);
        let incompatible = AtlasBuilder::new(16).finish();
        retained.set_atlas(&gpu.device, &gpu.queue, &incompatible);

        for (size, zoom, at) in [
            (SOURCE, (1, 1), (0, 0)),
            ((32, 24), (1, 1), (0, 0)),
            ((40, 30), (2, 1), (3, 2)),
            ((31, 23), (3, 2), (-1, 3)),
            ((13, 11), (2, 3), (2, -1)),
            ((8, 6), (1, 1), (-3, -2)),
            ((20, 15), (1, 1), (100, 100)),
        ] {
            let normal = Present {
                canvas: SOURCE,
                zoom,
                at,
                target: size,
            };
            let expected = gpu.render_presented(&mut fresh, &[camera], &copies, normal, 0, CLEAR);
            // Deliberately stale canvas/target metadata. Texture dimensions
            // and the retained source must determine clipping and borders.
            let replay = Present {
                canvas: size,
                target: SOURCE,
                ..normal
            };
            let (shown, got) = cached(&gpu, &retained, size, &copies, replay);
            assert!(shown);
            assert_same(
                &got,
                &expected,
                &format!("{color:?}, {size:?}, {zoom:?}, {at:?}"),
            );
        }
        let (shown, got) = cached(&gpu, &retained, SOURCE, &[], Present::unscaled(SOURCE));
        assert!(shown);
        assert_same(
            &got,
            &tile_render::reference::render(&world, &atlas, &camera, 0, CLEAR),
            "replays leave the source unchanged",
        );
    }
}

#[test]
fn missing_and_direct_pictures_return_false_without_drawing() {
    let gpu = Headless::new().expect("AMD GPU");
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    let (shown, got) = cached(&gpu, &renderer, SOURCE, &[], Present::unscaled(SOURCE));
    assert!(!shown);
    assert!(got.pixels.iter().all(|p| *p == SENTINEL));

    let (atlas, mut world) = fixture(ColorModel::Quantized { bits: 5 });
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    renderer.set_world(&gpu.device, &world);
    let camera = Camera::new(SOURCE);
    let _ = gpu.render(&mut renderer, &camera, 0, CLEAR);
    assert!(cached(&gpu, &renderer, SOURCE, &[], Present::unscaled(SOURCE)).0);
    world.color = ColorModel::Direct;
    renderer.set_world(&gpu.device, &world);
    let _ = gpu.render(&mut renderer, &camera, 0, CLEAR);
    let (shown, got) = cached(&gpu, &renderer, SOURCE, &[], Present::unscaled(SOURCE));
    assert!(
        !shown,
        "an older composed picture must not survive a direct frame"
    );
    assert!(got.pixels.iter().all(|p| *p == SENTINEL));
}

#[test]
fn uploaded_world_colour_changes_do_not_redecode_the_retained_canvas() {
    let gpu = Headless::new().expect("AMD GPU");
    let colors = [ColorModel::Quantized { bits: 5 }, ColorModel::Quantized { bits: 6 }, ColorModel::Direct];
    let camera = Camera::new(SOURCE);
    // A copy also forces direct colour through the retained canvas path.
    let copies = [ScreenCopy { source: [1, 0, 7, 6], at: (8, 3), zoom: (1, 1), lens: None }];
    for old in colors {
        for new in colors {
            let (atlas, mut world) = fixture(old);
            let mut renderer = renderer(&gpu, &atlas, &world);
            let old_picture = gpu.render_composed(&mut renderer, &[camera], &copies, SOURCE, 0, CLEAR);
            // A compatible world can be uploaded before surface acquisition
            // fails. No commands for its replacement picture were encoded.
            world.color = new;
            renderer.set_world(&gpu.device, &world);
            let (shown, replay) = cached(&gpu, &renderer, SOURCE, &copies, Present::unscaled(SOURCE));
            assert!(shown);
            assert_same(&replay, &old_picture, &format!("uploaded {new:?}, retained {old:?}"));
            // The next successful composition adopts its own decoding.
            let new_picture = gpu.render_composed(&mut renderer, &[camera], &copies, SOURCE, 0, CLEAR);
            let (shown, replay) = cached(&gpu, &renderer, SOURCE, &copies, Present::unscaled(SOURCE));
            assert!(shown);
            assert_same(&replay, &new_picture, &format!("drawn {new:?}"));
        }
    }
}

/// The viewer's shader module (it holds the tile renderer's shaders).
const SHADERS: tile_render::Shaders = tile_render::Shaders::new(viewer_spirv::SPIRV, viewer_spirv::SPIRV_FALLBACK);
