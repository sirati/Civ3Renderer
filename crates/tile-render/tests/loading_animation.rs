//! GPU viewer-clock animation without publishing or uploading another world.
use tile_render::{AtlasBuilder, Camera, Grid, Headless, Layout, PlacedSprite, Present, RgbaImage, SpriteSets, TileRenderer, TileWorld};
use tile_render_shared::HOST_CLOCK;

#[test]
fn gpu_waiting_animation_uses_host_clock_and_preserves_game_clock_and_replay() {
    let gpu = Headless::new().expect("GPU animation correctness lease");
    eprintln!("loading animation adapter: {:?}", gpu.adapter.get_info());
    let mut atlas = AtlasBuilder::new(64);
    let color = |frame: u32| [(frame * 7) as u8, (250 - frame * 3) as u8, (frame * 5 + 11) as u8, 255];
    let frames: Vec<_> = (0..30).map(|frame| atlas.add(&RgbaImage::from_fn(3, 3, |_, _| color(frame)), (0, 0)).unwrap()).collect();
    let mut sets = SpriteSets::default();
    let set = sets.add(&frames);
    let mut world = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (1, 1) }, sets);
    world.free.push(PlacedSprite { ticks_per_frame: HOST_CLOCK | 83, screen: true, ..PlacedSprite::new(set, 4, 5) });
    world.free.push(PlacedSprite { ticks_per_frame: 1, screen: true, ..PlacedSprite::new(set, 14, 5) });
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas.finish());
    renderer.set_world(&gpu.device, &world);
    let camera = Camera::new((20, 10));
    let present = Present { canvas: (20, 10), target: (40, 20), zoom: (2, 1), at: (0, 0) };
    let mut pixels_checked = 0;
    for frame in 0..30 {
        // The game tick and every world command remain unchanged. Only the
        // existing scalar frame uniform differs, including a replay case.
        renderer.set_host_tick_ms(frame * 83);
        let first = gpu.render_presented(&mut renderer, &[camera], &[], present, 9, [0; 4]);
        assert_eq!(first.get(8, 10), color(frame), "host frame {frame}");
        assert_eq!(first.get(28, 10), color(9), "game animation remains authoritative");
        renderer.set_host_tick_ms(frame * 83 + 82);
        let replay = gpu.render_presented(&mut renderer, &[camera], &[], present, 9, [0; 4]);
        assert_eq!(first.pixels, replay.pixels);
        assert!(renderer.reused(), "within the same83ms frame the retained picture remains valid");
        pixels_checked += replay.pixels.len();
    }
    renderer.set_host_tick_ms(2490);
    let wrapped = gpu.render_presented(&mut renderer, &[camera], &[], present, 9, [0; 4]);
    assert_eq!(wrapped.get(8, 10), color(0));
    assert_eq!(wrapped.get(28, 10), color(9));
    assert!(!renderer.reused(), "the changed host frame must invalidate the retained picture");
    assert_eq!(pixels_checked, 24000);
    eprintln!("loading animation exact replay comparisons: {pixels_checked} pixels, all30frames and wrap; gameclock preserved");
}

/// The viewer's shader module (it holds the tile renderer's shaders).
const SHADERS: tile_render::Shaders = tile_render::Shaders::new(viewer_spirv::SPIRV, viewer_spirv::SPIRV_FALLBACK);
