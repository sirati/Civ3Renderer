//! A grid layer animated on the viewer's clock (`HOST_CLOCK` in its
//! `ticks_per_frame`): its cells run through their frames with the driver's
//! tick frozen and no world uploaded, from each cell's phase on, while a
//! layer of driver frames keeps the frame its cell's phase says.
use tile_render::{AtlasBuilder, Camera, Grid, Headless, LayerSpec, Layout, Present, RgbaImage, SpriteSets, TileRenderer, TileWorld};
use tile_render_shared::HOST_CLOCK;

#[test]
fn a_viewer_clock_layer_animates_with_the_driver_frozen() {
    let gpu = Headless::new().expect("GPU host-clock layer lease");
    let mut atlas = AtlasBuilder::new(64);
    let color = |frame: u32| [(frame * 7) as u8, (250 - frame * 3) as u8, (frame * 5 + 11) as u8, 255];
    let frames: Vec<_> = (0..30).map(|frame| atlas.add(&RgbaImage::from_fn(4, 4, |_, _| color(frame)), (0, 0)).unwrap()).collect();
    let mut sets = SpriteSets::default();
    let set = sets.add(&frames);
    let grid = Grid { cols: 2, rows: 1, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (4, 4) };
    let mut world = TileWorld::new(grid, sets);
    let lut = vec![tile_render::SpriteSetId::EMPTY, set];
    let driver = world.add_layer(LayerSpec { ticks_per_frame: 0, per_cell_phase: true, ..LayerSpec::new("driver", lut.clone()) });
    let viewer = world.add_layer(LayerSpec { ticks_per_frame: HOST_CLOCK | 83, per_cell_phase: true, ..LayerSpec::new("viewer", lut) });
    world.set(driver, 0, 0, 1);
    world.set_phase(driver, 0, 0, 9);
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas.finish());
    renderer.set_world(&gpu.device, &world);
    let camera = Camera::new((8, 4));
    // (Shown scaled: a retained picture, which only a changed frame redraws.)
    let present = Present { canvas: (8, 4), target: (16, 8), zoom: (2, 1), at: (0, 0) };
    // Nothing on the viewer-clock layer yet: its clock changes nothing, the
    // picture is kept.
    renderer.set_host_tick_ms(0);
    gpu.render_presented(&mut renderer, &[camera], &[], present, 3, [0; 4]);
    renderer.set_host_tick_ms(500);
    gpu.render_presented(&mut renderer, &[camera], &[], present, 3, [0; 4]);
    assert!(renderer.reused(), "an empty viewer-clock layer does not redraw with the clock");
    // A cell on it from phase 4 (only its records uploaded).
    world.set(viewer, 1, 0, 1);
    world.set_phase(viewer, 1, 0, 4);
    let dirty = world.take_dirty();
    renderer.update_cells(&gpu.device, &gpu.queue, &world, &dirty);
    for k in 0..31u32 {
        renderer.set_host_tick_ms(k * 83);
        let picture = gpu.render_presented(&mut renderer, &[camera], &[], present, 3, [0; 4]);
        assert_eq!(picture.get(10, 2), color((4 + k) % 30), "viewer frame {k}, from the cell's phase on");
        assert_eq!(picture.get(2, 2), color(9), "the driver's frame stays the cell's phase");
        renderer.set_host_tick_ms(k * 83 + 82);
        gpu.render_presented(&mut renderer, &[camera], &[], present, 3, [0; 4]);
        assert!(renderer.reused(), "within one 83 ms frame the picture is kept");
    }
    // The cell gone: the clock no longer redraws.
    world.set(viewer, 1, 0, 0);
    let dirty = world.take_dirty();
    renderer.update_cells(&gpu.device, &gpu.queue, &world, &dirty);
    renderer.set_host_tick_ms(10_000);
    gpu.render_presented(&mut renderer, &[camera], &[], present, 3, [0; 4]);
    renderer.set_host_tick_ms(20_000);
    gpu.render_presented(&mut renderer, &[camera], &[], present, 3, [0; 4]);
    assert!(renderer.reused());
}

/// The viewer's shader module (it holds the tile renderer's shaders).
const SHADERS: tile_render::Shaders = tile_render::Shaders::new(viewer_spirv::SPIRV, viewer_spirv::SPIRV_FALLBACK);
