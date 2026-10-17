//! Runtime atlas uploads keep the active picture until an atomic adoption.
use std::num::NonZeroU64;

use tile_render::{
    Atlas, AtlasBuilder, AtlasGrowth, Blit, BlitPixels, Camera, ColorModel, Grid, Headless,
    IndexImage, Layout, PlacedSprite, RgbaImage, SpriteId, SpriteSets, TileRenderer, TileWorld,
    atlas::sprite_at, reference,
    renderer::{AtlasTransferError, AtlasTransferSource, ATLAS_TRANSFER_STAGING_RESERVE_BYTES},
};

const CLEAR: [u8; 4] = [16, 24, 32, 255];

fn world(sprites: &[SpriteId]) -> TileWorld {
    let mut sets = SpriteSets::default();
    let ids: Vec<_> = sprites.iter().map(|&id| sets.single(id)).collect();
    let mut world = TileWorld::new(
        Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (8, 8) },
        sets,
    );
    world.color = ColorModel::Quantized { bits: 5 };
    for (i, set) in ids.into_iter().enumerate() {
        world.free.push(PlacedSprite::new(set, i as i32 * 12, 0));
    }
    world
}

fn base() -> (Atlas, SpriteId) {
    let mut b = AtlasBuilder::new(16);
    let id = b.add(&RgbaImage::from_fn(8, 8, |_, _| [240, 40, 8, 255]), (0, 0)).unwrap();
    let mut atlas = b.finish();
    atlas.trim_blank();
    (atlas, id)
}

fn renderer(gpu: &Headless, atlas: &Atlas, world: &TileWorld) -> TileRenderer {
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, atlas);
    renderer.set_world(&gpu.device, world);
    renderer
}

fn check(gpu: &Headless, renderer: &mut TileRenderer, atlas: &Atlas, world: &TileWorld) {
    let camera = Camera::new((48, 24));
    let got = gpu.render(renderer, &camera, 0, CLEAR);
    let want = reference::render(world, atlas, &camera, 0, CLEAR);
    assert_eq!(got.pixels, want.pixels);
}

fn step(gpu: &Headless, renderer: &mut TileRenderer, source: AtlasTransferSource<'_>, budget: u64) -> bool {
    let progress = renderer.advance_atlas_transfer(&gpu.device, &gpu.queue, source, budget).unwrap();
    assert!(progress.bytes <= budget, "{} submitted with a {budget}-byte budget", progress.bytes);
    // Tests drive callbacks deterministically. The runtime API itself only
    // polls, and does not perform this blocking test synchronization.
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    progress.ready
}

fn finish(gpu: &Headless, renderer: &mut TileRenderer, source: AtlasTransferSource<'_>, budget: u64) {
    for _ in 0..10_000 {
        if step(gpu, renderer, source, budget) {
            assert!(renderer.commit_atlas_transfer(&gpu.device, &gpu.queue));
            return;
        }
    }
    panic!("atlas transfer did not finish");
}

#[test]
fn copied_growth_preserves_late_patches_and_new_alias_bounds() {
    let gpu = Headless::new().expect("GPU");
    let (mut atlas, old) = base();
    let old_world = world(&[old]);
    let mut renderer = renderer(&gpu, &atlas, &old_world);
    let growth = AtlasGrowth {
        first: 1, pages: 2, index_pages: 0,
        sprites: vec![sprite_at((1, 0, 0), (8, 8), (0, 0)), sprite_at((0, 2, 2), (4, 4), (0, 0))],
        blits: vec![Blit { page: 1, x: 0, y: 0, pixels: BlitPixels::Rgba(RgbaImage::from_fn(8, 8, |_, _| [8, 40, 240, 255])) }],
        palettes: Vec::new(),
    };
    let source = AtlasTransferSource::Growth(&growth);
    renderer.begin_atlas_transfer(&gpu.device, source, (0, 0)).unwrap();
    assert!(!renderer.commit_atlas_transfer(&gpu.device, &gpu.queue));
    // The old 16x16 RGBA page is copied by now, but the entire transaction
    // is still uploading. This patch must reach an already-copied prefix.
    for _ in 0..4 { assert!(!step(&gpu, &mut renderer, source, 269)); }
    let image = RgbaImage::from_fn(8, 8, |x, y| if x >= 3 && y >= 3 { [40, 240, 8, 128] } else { [0; 4] });
    assert!(renderer.patch_sprite(&gpu.queue, old, &image));
    assert!(atlas.patch(old, &image));
    check(&gpu, &mut renderer, &atlas, &old_world);
    let mut ready = false;
    for _ in 0..10_000 {
        if step(&gpu, &mut renderer, source, 269) { ready = true; break; }
    }
    assert!(ready);
    // Immediate mirroring permits adoption after a new mutation without
    // requiring a quiet frame after the background work was submitted.
    let image = RgbaImage::from_fn(8, 8, |x, _| if x >= 4 { [240, 8, 40, 128] } else { [0; 4] });
    assert!(renderer.patch_sprite(&gpu.queue, old, &image));
    assert!(renderer.commit_atlas_transfer(&gpu.device, &gpu.queue));
    atlas.grow(&growth);
    assert!(atlas.patch(old, &image));
    let world = world(&[old, SpriteId(1), SpriteId(2)]);
    renderer.set_world(&gpu.device, &world);
    check(&gpu, &mut renderer, &atlas, &world);
}

#[test]
fn cancellation_prevents_a_second_candidate_until_retirement() {
    let gpu = Headless::new().expect("GPU");
    let (atlas, id) = base();
    let world = world(&[id]);
    let mut renderer = renderer(&gpu, &atlas, &world);
    let mut replacement = atlas.clone();
    assert!(replacement.patch(id, &RgbaImage::from_fn(8, 8, |_, _| [8, 40, 240, 128])));
    let source = AtlasTransferSource::Whole(&replacement);
    renderer.begin_atlas_transfer(&gpu.device, source, (0, 0)).unwrap();
    step(&gpu, &mut renderer, source, 269);
    assert_eq!(renderer.begin_atlas_transfer(&gpu.device, source, (0, 0)), Err(AtlasTransferError::Busy));
    renderer.cancel_atlas_transfer(&gpu.queue);
    assert!(!renderer.atlas_transfer_idle());
    check(&gpu, &mut renderer, &atlas, &world);
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    renderer.poll_atlas_transfers(&gpu.device);
    assert!(renderer.atlas_transfer_idle());
    renderer.begin_atlas_transfer(&gpu.device, source, (0, 0)).unwrap();
    finish(&gpu, &mut renderer, source, 269);
    check(&gpu, &mut renderer, &replacement, &world);
    // Adoption itself retains the old allocation until a completion poll.
    assert!(!renderer.atlas_transfer_idle());
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    renderer.poll_atlas_transfers(&gpu.device);
    assert!(renderer.atlas_transfer_idle());
}

#[test]
fn reservations_reduce_to_required_capacity_under_the_peak_limit() {
    let gpu = Headless::new().expect("GPU");
    let (mut atlas, id) = base();
    let world = world(&[id]);
    let mut renderer = renderer(&gpu, &atlas, &world);
    let desc = size_of::<tile_render_shared::SpriteDesc>() as u64;
    let active = 16 * 16 * 4 + 1 + desc + 256 * 4;
    renderer.set_atlas_transfer_limit(NonZeroU64::new(active).unwrap());
    renderer.begin_atlas_transfer(&gpu.device, AtlasTransferSource::Reserve, (1024, 1024)).unwrap();
    finish(&gpu, &mut renderer, AtlasTransferSource::Reserve, 269);
    assert_eq!(renderer.atlas_room(), (1, 0));
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    renderer.poll_atlas_transfers(&gpu.device);
    let growth = AtlasGrowth {
        first: 1, pages: 2, index_pages: 0,
        sprites: vec![sprite_at((1, 0, 0), (8, 8), (0, 0))],
        blits: vec![Blit { page: 1, x: 0, y: 0, pixels: BlitPixels::Rgba(RgbaImage::from_fn(8, 8, |_, _| [8, 40, 240, 255])) }],
        palettes: Vec::new(),
    };
    let source = AtlasTransferSource::Growth(&growth);
    assert!(matches!(renderer.begin_atlas_transfer(&gpu.device, source, (1024, 1024)), Err(AtlasTransferError::BudgetExceeded { .. })));
    let minimum_peak = active + 16 * 16 * 2 * 4 + 2 * desc + 256 * 4 + ATLAS_TRANSFER_STAGING_RESERVE_BYTES;
    renderer.set_atlas_transfer_limit(NonZeroU64::new(minimum_peak).unwrap());
    renderer.begin_atlas_transfer(&gpu.device, source, (1024, 1024)).unwrap();
    finish(&gpu, &mut renderer, source, 269);
    assert_eq!(renderer.atlas_room(), (2, 0));
    atlas.grow(&growth);
    check(&gpu, &mut renderer, &atlas, &world);
}

#[test]
fn palette_mutations_during_growth_preserve_alpha_classification() {
    let gpu = Headless::new().expect("GPU");
    let mut b = AtlasBuilder::new(16);
    let id = b.add_indexed(&IndexImage::from_fn(8, 8, |_, _| 1), (0, 0), 1, 0).unwrap();
    let mut atlas = b.finish();
    atlas.palettes = vec![0; 512];
    atlas.palettes[257] = u32::from_le_bytes([240, 40, 8, 255]);
    atlas.trim_blank();
    let mut world = world(&[id]);
    // Identical opaque draws can be deduplicated. Palette translucency
    // during the transfer must restore both draws at adoption.
    world.free.push(world.free[0]);
    let mut renderer = renderer(&gpu, &atlas, &world);
    let growth = AtlasGrowth { first: 1, pages: 0, index_pages: 2, ..Default::default() };
    let source = AtlasTransferSource::Growth(&growth);
    renderer.begin_atlas_transfer(&gpu.device, source, (0, 0)).unwrap();
    let mut ready = false;
    for _ in 0..10_000 {
        if step(&gpu, &mut renderer, source, 269) { ready = true; break; }
    }
    assert!(ready);
    atlas.palettes[257] = u32::from_le_bytes([8, 40, 240, 128]);
    renderer.set_palettes(&gpu.device, &gpu.queue, &atlas.palettes);
    atlas.trim_blank();
    assert!(renderer.commit_atlas_transfer(&gpu.device, &gpu.queue));
    atlas.grow(&growth);
    check(&gpu, &mut renderer, &atlas, &world);
}

#[test]
fn changed_palette_prefix_lengths_permanently_reject_pending_growth() {
    let gpu = Headless::new().expect("GPU");
    for rows in [1, 3] {
        let mut b = AtlasBuilder::new(16);
        let id = b.add_indexed(&IndexImage::from_fn(4, 4, |_, _| 1), (0, 0), 1, 0).unwrap();
        let mut atlas = b.finish();
        atlas.palettes = vec![0; 512];
        atlas.palettes[257] = u32::from_le_bytes([240, 40, 8, 255]);
        atlas.trim_blank();
        let world = world(&[id]);
        let mut renderer = renderer(&gpu, &atlas, &world);
        let mut alias = atlas.sprites[0];
        alias.palette = 2;
        let growth = AtlasGrowth {
            first: 1, pages: 0, index_pages: 1, sprites: vec![alias],
            palettes: vec![u32::from_le_bytes([8, 40, 240, 255]); 256], ..Default::default()
        };
        let source = AtlasTransferSource::Growth(&growth);
        renderer.begin_atlas_transfer(&gpu.device, source, (0, 0)).unwrap();
        step(&gpu, &mut renderer, source, 269);
        let previous = atlas.palettes.clone();
        atlas.palettes.resize(rows * 256, 0);
        renderer.set_palettes(&gpu.device, &gpu.queue, &atlas.palettes);
        assert_eq!(renderer.advance_atlas_transfer(&gpu.device, &gpu.queue, source, 269), Err(AtlasTransferError::PaletteLayoutChanged));
        assert!(!renderer.commit_atlas_transfer(&gpu.device, &gpu.queue));
        atlas.trim_blank();
        check(&gpu, &mut renderer, &atlas, &world);
        // Once invalid, the candidate cannot be resurrected by another
        // mutation. The caller must supply newly compatible art.
        renderer.set_palettes(&gpu.device, &gpu.queue, &previous);
        assert_eq!(renderer.advance_atlas_transfer(&gpu.device, &gpu.queue, source, 269), Err(AtlasTransferError::PaletteLayoutChanged));
        renderer.cancel_atlas_transfer(&gpu.queue);
    }
}

#[test]
fn narrow_index_uploads_charge_padded_rows_and_reject_tiny_budgets() {
    let gpu = Headless::new().expect("GPU");
    let mut b = AtlasBuilder::new(128);
    let id = b.add_indexed(&IndexImage::from_fn(1, 1, |_, _| 1), (0, 0), 1, 0).unwrap();
    let mut atlas = b.finish();
    atlas.palettes = vec![0; 512];
    atlas.palettes[257] = u32::from_le_bytes([40, 240, 8, 255]);
    atlas.trim_blank();
    let old_world = world(&[id]);
    let mut renderer = renderer(&gpu, &atlas, &old_world);
    let mut sprite = sprite_at((0, 2, 0), (1, 64), (0, 0));
    sprite.palette = 1;
    let growth = AtlasGrowth {
        first: 1, pages: 0, index_pages: 1, sprites: vec![sprite],
        blits: vec![Blit { page: 0, x: 2, y: 0, pixels: BlitPixels::Index(IndexImage::from_fn(1, 64, |_, _| 1)) }],
        ..Default::default()
    };
    let source = AtlasTransferSource::Growth(&growth);
    renderer.begin_atlas_transfer(&gpu.device, source, (0, 0)).unwrap();
    assert_eq!(renderer.advance_atlas_transfer(&gpu.device, &gpu.queue, source, 255), Err(AtlasTransferError::BudgetTooSmall { minimum: 256, provided: 255 }));
    let progress = renderer.advance_atlas_transfer(&gpu.device, &gpu.queue, source, 269).unwrap();
    assert_eq!(progress.bytes, 256, "one payload byte occupies a padded staging row");
    assert!(!progress.ready);
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    finish(&gpu, &mut renderer, source, 269);
    atlas.grow(&growth);
    let world = world(&[id, SpriteId(1)]);
    renderer.set_world(&gpu.device, &world);
    check(&gpu, &mut renderer, &atlas, &world);
}

#[test]
fn continuously_patched_sprites_do_not_starve_growth_adoption() {
    let gpu = Headless::new().expect("GPU");
    let (mut atlas, id) = base();
    let old_world = world(&[id]);
    let mut renderer = renderer(&gpu, &atlas, &old_world);
    let growth = AtlasGrowth {
        first: 1, pages: 2, index_pages: 0,
        sprites: vec![sprite_at((1, 0, 0), (8, 8), (0, 0))],
        blits: vec![Blit { page: 1, x: 0, y: 0, pixels: BlitPixels::Rgba(RgbaImage::from_fn(8, 8, |_, _| [8, 40, 240, 255])) }],
        ..Default::default()
    };
    let source = AtlasTransferSource::Growth(&growth);
    renderer.begin_atlas_transfer(&gpu.device, source, (0, 0)).unwrap();
    let mut committed = false;
    let mut latest = RgbaImage::new(8, 8);
    for frame in 0..128 {
        latest = RgbaImage::from_fn(8, 8, |x, _| if x >= frame % 5 { [frame as u8, 240, 40, 128] } else { [0; 4] });
        assert!(renderer.patch_sprite(&gpu.queue, id, &latest));
        assert!(atlas.patch(id, &latest));
        let progress = renderer.advance_atlas_transfer(&gpu.device, &gpu.queue, source, 269).unwrap();
        assert!(progress.bytes <= 269);
        if progress.ready {
            // Commit immediately after submission, without a quiet frame
            // or waiting for the final transfer's completion callback.
            assert!(renderer.commit_atlas_transfer(&gpu.device, &gpu.queue));
            committed = true;
            break;
        }
        gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    }
    assert!(committed, "a live patch every iteration must not restart background replay");
    atlas.grow(&growth);
    assert!(atlas.patch(id, &latest));
    let world = world(&[id, SpriteId(1)]);
    renderer.set_world(&gpu.device, &world);
    check(&gpu, &mut renderer, &atlas, &world);
}

#[test]
fn legacy_adoption_rejects_overlap_without_changing_the_runtime_source() {
    let gpu = Headless::new().expect("GPU");
    let (atlas, id) = base();
    let world = world(&[id]);
    let mut renderer = renderer(&gpu, &atlas, &world);
    let mut replacement = atlas.clone();
    replacement.patch(id, &RgbaImage::from_fn(8, 8, |_, _| [8, 40, 240, 128]));
    let legacy = renderer.stage_atlas(&gpu.device, &replacement);
    renderer.begin_atlas_transfer(&gpu.device, AtlasTransferSource::Reserve, (2, 0)).unwrap();
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        renderer.adopt_atlas(&gpu.device, legacy, &replacement);
    }));
    assert!(rejected.is_err());
    check(&gpu, &mut renderer, &atlas, &world);
    finish(&gpu, &mut renderer, AtlasTransferSource::Reserve, 269);
    assert_eq!(renderer.atlas_room(), (2, 0));
    check(&gpu, &mut renderer, &atlas, &world);
}

#[test]
fn first_runtime_atlas_retains_its_upload_workspace_until_retirement() {
    let gpu = Headless::new().expect("GPU");
    let (atlas, _) = base();
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let source = AtlasTransferSource::Whole(&atlas);
    renderer.begin_atlas_transfer(&gpu.device, source, (0, 0)).unwrap();
    finish(&gpu, &mut renderer, source, 269);
    assert!(!renderer.atlas_transfer_idle(), "the upload buffer is still an allocation even without an old atlas");
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    renderer.poll_atlas_transfers(&gpu.device);
    assert!(renderer.atlas_transfer_idle());
}

fn draw_live_old(gpu: &Headless, renderer: &mut TileRenderer, atlas: &mut Atlas, world: &TileWorld, frame: u32) {
    let image = RgbaImage::from_fn(8, 8, |_, _| [(frame as u8).wrapping_mul(3), 40, 240, 128]);
    assert!(renderer.patch_sprite(&gpu.queue, SpriteId(0), &image));
    assert!(atlas.patch(SpriteId(0), &image));
    let marker = PlacedSprite::new(world.free[0].set, frame as i32 % 24, 10);
    renderer.set_markers(&gpu.device, &gpu.queue, &[marker]);
    let mut marked = world.clone();
    marked.free.push(marker);
    check(gpu, renderer, atlas, &marked);
}

#[test]
fn replacement_and_required_growth_keep_the_drawable_atlas_until_adoption() {
    let gpu = Headless::new().expect("GPU");
    let (mut old, id) = base();
    let old_world = world(&[id]);
    let mut renderer = renderer(&gpu, &old, &old_world);
    let mut replacement = old.clone();
    replacement.patch(id, &RgbaImage::from_fn(8, 8, |_, _| [8, 40, 240, 255]));
    let growth = AtlasGrowth {
        first: 1, pages: 2, index_pages: 0,
        sprites: vec![sprite_at((1, 0, 0), (8, 8), (0, 0))],
        blits: vec![Blit { page: 1, x: 0, y: 0, pixels: BlitPixels::Rgba(RgbaImage::from_fn(8, 8, |_, _| [40, 240, 8, 255])) }],
        ..Default::default()
    };
    let mut prepared = renderer.atlas_preparer();
    let remaining = renderer.atlas_transfer_limit().get() - renderer.atlas_resident_bytes();
    prepared.set_atlas_transfer_limit(NonZeroU64::new(remaining).unwrap());
    let whole = AtlasTransferSource::Whole(&replacement);
    prepared.begin_atlas_transfer_with_capacity(&gpu.device, whole, (2, 0), (0, 0)).unwrap();
    let mut frame = 0;
    while !step(&gpu, &mut prepared, whole, 269) {
        draw_live_old(&gpu, &mut renderer, &mut old, &old_world, frame);
        frame += 1;
    }
    assert!(prepared.commit_atlas_transfer(&gpu.device, &gpu.queue));
    assert_eq!(prepared.atlas_room(), (2, 0));
    draw_live_old(&gpu, &mut renderer, &mut old, &old_world, frame);
    let source = AtlasTransferSource::Growth(&growth);
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    prepared.poll_atlas_transfers(&gpu.device);
    prepared.begin_atlas_transfer(&gpu.device, source, (0, 0)).unwrap();
    assert_eq!(renderer.adopt_prepared_atlas(&gpu.device, &gpu.queue, &mut prepared), Err(AtlasTransferError::Busy));
    while !step(&gpu, &mut prepared, source, 269) {
        // The old atlas remains usable for a real draw on every transfer
        // iteration, rather than only a snapshot of its last composition.
        frame += 1;
        draw_live_old(&gpu, &mut renderer, &mut old, &old_world, frame);
    }
    assert!(prepared.commit_atlas_transfer(&gpu.device, &gpu.queue));
    assert!(!prepared.atlas_transfer_idle());
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    prepared.poll_atlas_transfers(&gpu.device);
    renderer.set_markers(&gpu.device, &gpu.queue, &[]);
    renderer.adopt_prepared_atlas(&gpu.device, &gpu.queue, &mut prepared).unwrap();
    replacement.grow(&growth);
    let new_world = world(&[id, SpriteId(1)]);
    renderer.set_world(&gpu.device, &new_world);
    check(&gpu, &mut renderer, &replacement, &new_world);
    assert!(!renderer.atlas_transfer_idle());
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    renderer.poll_atlas_transfers(&gpu.device);
    assert!(renderer.atlas_transfer_idle());
    assert_eq!(prepared.atlas_resident_bytes(), 0);
}

#[test]
fn mandatory_final_capacity_avoids_three_atlas_peak_without_exceeding_the_cap() {
    let gpu = Headless::new().expect("GPU");
    let (old, id) = base();
    let old_world = world(&[id]);
    let mut renderer = renderer(&gpu, &old, &old_world);
    let growth = AtlasGrowth {
        first: 1, pages: 2, index_pages: 0,
        sprites: vec![sprite_at((1, 0, 0), (8, 8), (0, 0))],
        blits: vec![Blit { page: 1, x: 0, y: 0, pixels: BlitPixels::Rgba(RgbaImage::from_fn(8, 8, |_, _| [40, 240, 8, 255])) }],
        ..Default::default()
    };
    let desc = size_of::<tile_render_shared::SpriteDesc>() as u64;
    let base_tables = desc + 256 * 4;
    let final_tables = 2 * desc + 256 * 4;
    let final_arrays = 16 * 16 * 2 * 4 + 1;
    let remainder = final_arrays + base_tables + final_tables + ATLAS_TRANSFER_STAGING_RESERVE_BYTES;
    let old_resident = renderer.atlas_resident_bytes();
    renderer.set_atlas_transfer_limit(NonZeroU64::new(old_resident + remainder).unwrap());
    let whole = AtlasTransferSource::Whole(&old);
    let source = AtlasTransferSource::Growth(&growth);
    let mut prepared = renderer.atlas_preparer();
    prepared.set_atlas_transfer_limit(NonZeroU64::new(remainder).unwrap());
    prepared.begin_atlas_transfer(&gpu.device, whole, (0, 0)).unwrap();
    finish(&gpu, &mut prepared, whole, 269);
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    prepared.poll_atlas_transfers(&gpu.device);
    assert!(matches!(prepared.begin_atlas_transfer(&gpu.device, source, (0, 0)), Err(AtlasTransferError::BudgetExceeded { .. })));
    assert!(prepared.atlas_transfer_idle());
    // No GPU draw has ever referenced this preparer. Idle means its
    // intermediate allocations can now be dropped before restarting.
    drop(prepared);
    let mut prepared = renderer.atlas_preparer();
    prepared.set_atlas_transfer_limit(NonZeroU64::new(remainder).unwrap());
    prepared.begin_atlas_transfer_with_capacity(&gpu.device, whole, (2, 0), (0, 0)).unwrap();
    assert!(old_resident + prepared.atlas_resident_bytes() <= renderer.atlas_transfer_limit().get());
    finish(&gpu, &mut prepared, whole, 269);
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    prepared.poll_atlas_transfers(&gpu.device);
    prepared.begin_atlas_transfer(&gpu.device, source, (0, 0)).unwrap();
    assert!(old_resident + prepared.atlas_resident_bytes() <= renderer.atlas_transfer_limit().get());
    finish(&gpu, &mut prepared, source, 269);
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    prepared.poll_atlas_transfers(&gpu.device);
    renderer.adopt_prepared_atlas(&gpu.device, &gpu.queue, &mut prepared).unwrap();
    assert!(renderer.atlas_resident_bytes() <= renderer.atlas_transfer_limit().get());
    let mut final_atlas = old;
    final_atlas.grow(&growth);
    let world = world(&[id, SpriteId(1)]);
    renderer.set_world(&gpu.device, &world);
    check(&gpu, &mut renderer, &final_atlas, &world);
}

#[test]
fn superseded_preparation_releases_its_candidate_before_replacement_allocation() {
    let gpu = Headless::new().expect("GPU");
    let (mut old, id) = base();
    let old_world = world(&[id]);
    let mut renderer = renderer(&gpu, &old, &old_world);
    let mut replacement = old.clone();
    replacement.patch(id, &RgbaImage::from_fn(8, 8, |_, _| [40, 240, 8, 255]));
    let source = AtlasTransferSource::Whole(&replacement);
    let limit = renderer.atlas_transfer_limit().get();
    let remaining = limit - renderer.atlas_resident_bytes();
    let mut prepared = renderer.atlas_preparer();
    prepared.set_atlas_transfer_limit(NonZeroU64::new(remaining).unwrap());
    prepared.begin_atlas_transfer(&gpu.device, source, (0, 0)).unwrap();
    prepared.advance_atlas_transfer(&gpu.device, &gpu.queue, source, 269).unwrap();
    prepared.cancel_atlas_transfer(&gpu.queue);
    assert!(!prepared.atlas_transfer_idle());
    assert!(prepared.atlas_resident_bytes() > 0);
    assert!(renderer.atlas_resident_bytes() + prepared.atlas_resident_bytes() <= limit);
    draw_live_old(&gpu, &mut renderer, &mut old, &old_world, 1);
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    prepared.poll_atlas_transfers(&gpu.device);
    assert!(prepared.atlas_transfer_idle());
    assert_eq!(prepared.atlas_resident_bytes(), 0);
    drop(prepared);
    // Allocate the successor only after the first candidate's fence has
    // released its atlas, tables, and upload workspace.
    let mut successor = renderer.atlas_preparer();
    successor.set_atlas_transfer_limit(NonZeroU64::new(remaining).unwrap());
    successor.begin_atlas_transfer(&gpu.device, source, (0, 0)).unwrap();
    assert!(renderer.atlas_resident_bytes() + successor.atlas_resident_bytes() <= limit);
    finish(&gpu, &mut successor, source, 269);
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    successor.poll_atlas_transfers(&gpu.device);
    renderer.set_markers(&gpu.device, &gpu.queue, &[]);
    renderer.adopt_prepared_atlas(&gpu.device, &gpu.queue, &mut successor).unwrap();
    renderer.set_world(&gpu.device, &old_world);
    check(&gpu, &mut renderer, &replacement, &old_world);
}
