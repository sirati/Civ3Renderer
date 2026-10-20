//! Per-view layer data (data slots and bindings, see docs/per-view-layers.md):
//! the GPU draws every view's layers from the slots its camera's binding
//! names exactly as the reference renderer does, and slots come and go
//! incrementally.

use tile_render::{
    Atlas, AtlasBuilder, Blend, Camera, ColorModel, Dirty, Grid, Headless, LayerId, LayerSpec, Layout, RgbaImage, SlotId, SpriteSetId, SpriteSets, TileRenderer, TileWorld, reference,
};

const CLEAR: [u8; 4] = [10, 20, 30, 255];
const SIZE: (u32, u32) = (640, 240);
/// The binding of view A (a screen of its own over the world, say).
const A: u32 = 1;
/// A second one (another player's world, say).
const B: u32 = 7;

fn diamond(seed: u8) -> RgbaImage {
    RgbaImage::from_fn(128, 64, |x, y| {
        let dx = (x as i32 * 2 - 127).abs() as f32 / 128.0;
        let dy = (y as i32 * 2 - 63).abs() as f32 / 64.0;
        if dx + dy > 1.0 { [0, 0, 0, 0] } else { [seed.wrapping_mul(37), (x as u8).wrapping_add(seed), (y as u8).wrapping_mul(3), 255] }
    })
}

/// Darkens to `k / 16` (`k` the red channel), as a fog of war does.
fn shade(k: u8) -> RgbaImage {
    RgbaImage::from_fn(128, 64, |x, y| {
        let dx = (x as i32 * 2 - 127).abs() as f32 / 128.0;
        let dy = (y as i32 * 2 - 63).abs() as f32 / 64.0;
        if dx + dy > 1.0 { [0, 0, 0, 0] } else { [k.min(15), 0, 0, 255] }
    })
}

struct Fixture {
    world: TileWorld,
    atlas: Atlas,
    terrain: LayerId,
    fog: LayerId,
    /// Its records carry offsets (as a fog layer along a map's edge does).
    edge: LayerId,
}

fn fixture() -> Fixture {
    let mut atlas = AtlasBuilder::new(512);
    let mut sets = SpriteSets::default();
    let ground: Vec<_> = (0..4u8).map(|i| sets.single(atlas.add(&diamond(i * 50), (0, 0)).unwrap())).collect();
    let mut fog_lut = vec![SpriteSetId::EMPTY];
    fog_lut.extend((1..4u8).map(|k| sets.single(atlas.add(&shade(k * 4), (0, 0)).unwrap())));
    let grid = Grid { cols: 10, rows: 16, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
    let mut world = TileWorld::new(grid, sets);
    let terrain = world.add_layer(LayerSpec::new("terrain", ground));
    let fog = world.add_layer(LayerSpec { pass: 1, blend: Blend::Scale { denominator: 16 }, ..LayerSpec::new("fog", fog_lut.clone()) });
    let edge = world.add_layer(LayerSpec { pass: 1, per_cell_offset: true, blend: Blend::Scale { denominator: 16 }, ..LayerSpec::new("fog_edge", fog_lut) });
    for r in 0..grid.rows {
        for c in 0..grid.cols {
            world.set(terrain, c, r, (c * 3 + r) % 4);
            // The player's fog: a band of explored cells.
            world.set(fog, c, r, if (r / 4) % 2 == 1 { 2 } else { 0 });
        }
    }
    world.set(edge, 0, 3, 1);
    world.set_offset(edge, 0, 3, (-64, 0));
    Fixture { world, atlas: atlas.finish(), terrain, fog, edge }
}

/// A fog of its own (a city screen's: lit around one cell only).
fn city_fog(world: &mut TileWorld, slot: SlotId, centre: (u32, u32)) {
    for r in 0..world.grid.rows {
        for c in 0..world.grid.cols {
            let near = c.abs_diff(centre.0) <= 1 && r.abs_diff(centre.1) <= 2;
            world.set_in(slot, c, r, if near { 0 } else { 3 });
        }
    }
}

/// Two views side by side, A's with `a`, B's with `b` as their bindings.
fn cameras(a: u32, b: u32) -> [Camera; 2] {
    let left = Camera { x: 40, y: 20, overhang: (0, 0, 64, 0), binding: a, ..Camera::new((320, 240)) };
    let right = Camera { x: 40, y: 20, origin: (320, 0), overhang: (0, 0, 64, 0), binding: b, ..Camera::new((320, 240)) };
    [left, right]
}

fn assert_same(gpu: &RgbaImage, cpu: &RgbaImage, what: &str) {
    assert_eq!((gpu.width, gpu.height), (cpu.width, cpu.height));
    let diff: Vec<_> = gpu.pixels.iter().zip(&cpu.pixels).enumerate().filter(|(_, (a, b))| a != b).collect();
    if let Some((i, (g, c))) = diff.first() {
        let (x, y) = (*i as u32 % gpu.width, *i as u32 / gpu.width);
        panic!("{what}: {} pixels differ, first at ({x}, {y}): gpu {g:?} cpu {c:?}", diff.len());
    }
}

/// The pixels of the half of `image` from column `x0` on, `w` wide.
fn half(image: &RgbaImage, x0: u32, w: u32) -> Vec<[u8; 4]> {
    (0..image.height).flat_map(|y| (x0..x0 + w).map(move |x| (x, y))).map(|(x, y)| image.pixels[(y * image.width + x) as usize]).collect()
}

fn renderer(gpu: &Headless, f: &Fixture) -> TileRenderer {
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &f.atlas);
    renderer
}

#[test]
fn two_views_draw_different_fog_slots_and_the_same_one_alike() {
    let gpu = Headless::new().expect("GPU");
    eprintln!("adapter: {:?}", gpu.adapter.get_info());
    // (Darkening is exact in a quantized model; direct colour rounds it as
    // blending hardware does.)
    for color in [ColorModel::Quantized { bits: 5 }, ColorModel::Quantized { bits: 6 }] {
        let mut f = fixture();
        f.world.color = color;
        let shared_only = f.world.clone();
        let slot = f.world.add_slot(f.fog);
        city_fog(&mut f.world, slot, (4, 7));
        let edge = f.world.add_slot(f.edge);
        f.world.set_in(edge, 0, 5, 2);
        f.world.set_offset_in(edge, 0, 5, (-64, 0));
        f.world.bind(A, f.fog, Some(slot));
        f.world.bind(A, f.edge, Some(edge));
        // B shares A's fog: one slot, two bindings, nothing copied.
        f.world.bind(B, f.fog, Some(slot));
        assert_eq!(f.world.bindings().collect::<Vec<_>>(), [A, B]);
        let mut renderer = renderer(&gpu, &f);
        renderer.set_world(&gpu.device, &f.world);
        // What each binding draws, as a world of its own: the shared one
        // with the bound slots' records as its layers' own.
        let flat = |binding: u32| {
            let mut w = shared_only.clone();
            for layer in [f.fog, f.edge] {
                let Some(s) = f.world.bound(binding, layer) else { continue };
                for r in 0..w.grid.rows {
                    for c in 0..w.grid.cols {
                        w.set(layer, c, r, f.world.get_in(s, c, r));
                        if layer == f.edge {
                            w.set_offset(layer, c, r, f.world.offset_in(s, c, r));
                        }
                    }
                }
            }
            gpu.render_views(&mut renderer_for(&gpu, &f, &w), &cameras(0, 0), SIZE, 0, CLEAR)
        };
        let plain = flat(0);
        assert_ne!(half(&flat(A), 0, 320), half(&plain, 0, 320), "A's slots draw otherwise");
        for (a, b, what) in [(A, 0, "A its own fog, B the shared"), (0, A, "A the shared, B A's"), (A, B, "the same fog slot"), (B, B, "both the same binding"), (0, 0, "both shared"), (99, 0, "an unknown binding is the shared one")] {
            let cams = cameras(a, b);
            let got = gpu.render_views(&mut renderer, &cams, SIZE, 0, CLEAR);
            let want = reference::render_views(&f.world, &f.atlas, &cams, SIZE, 0, CLEAR);
            assert_same(&got, &want, &format!("{what}, {color:?}"));
            // Each half is what its binding's slots draw.
            for (binding, x0) in [(a, 0), (b, 320)] {
                assert_eq!(half(&got, x0, 320), half(&flat(binding), x0, 320), "{what}: the view of binding {binding} at {x0}, {color:?}");
            }
        }
        // Slots cost their regions only; bindings a layer table each.
        assert_eq!(f.world.slot_bytes(), (f.world.grid.cells() + 5 * f.world.grid.cells()) as usize);
    }
}

fn renderer_for(gpu: &Headless, f: &Fixture, world: &TileWorld) -> TileRenderer {
    let mut r = renderer(gpu, f);
    r.set_world(&gpu.device, world);
    r
}

#[test]
fn slots_come_and_go_incrementally() {
    let gpu = Headless::new().expect("GPU");
    let mut f = fixture();
    f.world.color = ColorModel::Quantized { bits: 5 };
    let mut renderer = renderer(&gpu, &f);
    renderer.set_world(&gpu.device, &f.world);
    let _ = f.world.take_dirty();
    let check = |renderer: &mut TileRenderer, world: &TileWorld, what: &str| {
        for (a, b) in [(A, 0), (0, A)] {
            let cams = cameras(a, b);
            let got = gpu.render_views(renderer, &cams, SIZE, 0, CLEAR);
            assert_same(&got, &reference::render_views(world, &f.atlas, &cams, SIZE, 0, CLEAR), what);
        }
    };
    let plain = gpu.render_views(&mut renderer, &cameras(A, 0), SIZE, 0, CLEAR);
    // A screen opens over view A: a slot of its own, bound, filled.
    let slot = f.world.add_slot(f.fog);
    f.world.bind(A, f.fog, Some(slot));
    city_fog(&mut f.world, slot, (3, 6));
    let dirty = f.world.take_dirty();
    assert!(matches!(&dirty, Dirty::Records(r) if r.contains(&(f.world.slot_region(slot), tile_render::ALL_CELLS))), "{dirty:?}");
    renderer.update_cells(&gpu.device, &gpu.queue, &f.world, &dirty);
    check(&mut renderer, &f.world, "slot added");
    let opened = gpu.render_views(&mut renderer, &cameras(A, 0), SIZE, 0, CLEAR);
    assert_ne!(half(&opened, 0, 320), half(&plain, 0, 320), "A shows its own fog");
    assert_eq!(half(&opened, 320, 320), half(&plain, 320, 320), "B keeps the player's fog");
    // Its city moves: only the slot's changed records travel.
    city_fog(&mut f.world, slot, (6, 9));
    // The player's own fog changes meanwhile, under both.
    f.world.set(f.fog, 2, 2, 1);
    f.world.set(f.terrain, 5, 5, 0);
    let dirty = f.world.take_dirty();
    renderer.update_cells(&gpu.device, &gpu.queue, &f.world, &dirty);
    check(&mut renderer, &f.world, "slot and shared records changed");
    // The screen closes: unbound (nothing else changes), then freed.
    f.world.bind(A, f.fog, None);
    let dirty = f.world.take_dirty();
    assert_eq!(dirty, Dirty::Records(Vec::new()), "only the binding changed");
    renderer.update_cells(&gpu.device, &gpu.queue, &f.world, &dirty);
    check(&mut renderer, &f.world, "unbound");
    let closed = gpu.render_views(&mut renderer, &cameras(A, 0), SIZE, 0, CLEAR);
    assert_eq!(half(&closed, 0, 320), half(&closed, 320, 320), "both views alike again");
    f.world.free_slot(slot);
    assert_eq!((f.world.slots().count(), f.world.slot_bytes()), (0, 0), "the last slot's region is given back");
    // Opened again, with the edge slot first: freed regions are reused.
    let edge = f.world.add_slot(f.edge);
    let slot = f.world.add_slot(f.fog);
    f.world.bind(A, f.fog, Some(slot));
    f.world.bind(A, f.edge, Some(edge));
    city_fog(&mut f.world, slot, (1, 1));
    f.world.set_in(edge, 0, 9, 3);
    f.world.set_offset_in(edge, 0, 9, (-64, 0));
    let dirty = f.world.take_dirty();
    renderer.update_cells(&gpu.device, &gpu.queue, &f.world, &dirty);
    check(&mut renderer, &f.world, "slots added again");
    f.world.free_slot(edge);
    let dirty = f.world.take_dirty();
    renderer.update_cells(&gpu.device, &gpu.queue, &f.world, &dirty);
    check(&mut renderer, &f.world, "the edge slot freed, the fog slot kept");
    assert_eq!(f.world.bound(A, f.edge), None);
    // A new world of the same shape uploads whole and draws the same.
    let mut fresh = renderer_for(&gpu, &f, &f.world);
    let cams = cameras(A, 0);
    assert_same(&gpu.render_views(&mut fresh, &cams, SIZE, 0, CLEAR), &gpu.render_views(&mut renderer, &cams, SIZE, 0, CLEAR), "incremental against whole");
    // (`update_world` repacks everything, slots included.)
    renderer.update_world(&gpu.device, &gpu.queue, &f.world);
    check(&mut renderer, &f.world, "update_world");
}
