//! Rare destination-dependent native palette composition. All pixels stay GPU-side.
use super::*;
use tile_render_shared::{NativePaletteParams, FREE_SCREEN, FREE_ZOOMED, FREE_LOW, FREE_EXCEPT_VIEW, FREE_CLIP, ALL_VIEWS};

pub(super) struct State {
    buffers: [wgpu::Buffer; CONTROL_SLOTS],
    uploads: [ControlBytes; CONTROL_SLOTS],
    draws: usize,
    scratch: Option<Scratch>,
}
impl State {
    pub fn new(device: &wgpu::Device, draws: usize) -> Self {
        Self {
            buffers: std::array::from_fn(|_| device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("native palette origins"), size: PARAMS_STRIDE * draws.max(1) as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
            })),
            uploads: Default::default(), draws: draws.max(1), scratch: None,
        }
    }
    pub fn begin(&mut self, device: &wgpu::Device, slot: usize, draws: usize) {
        if draws > self.draws { *self = Self::new(device, draws); }
        self.uploads[slot].bytes.borrow_mut().clear();
    }
    pub fn bytes(&self) -> u64 {
        self.buffers.iter().map(wgpu::Buffer::size).sum::<u64>() + self.scratch.as_ref().map_or(0, |s| {
            u64::from(s.main.width()) * u64::from(s.main.height()) * if s.main.format() == wgpu::TextureFormat::Rgba16Float { 8 } else { 4 }
        })
    }
    pub fn flush(&self, queue: &wgpu::Queue, slot: usize) {
        let mut bytes = self.uploads[slot].bytes.borrow_mut();
        if !bytes.is_empty() { queue.write_buffer(&self.buffers[slot], 0, &bytes); }
        bytes.clear();
    }
    pub fn capture(&mut self, renderer: &TileRenderer, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture, slot: usize, [x, y, w, h]: [u32; 4]) -> (wgpu::BindGroup, u32) {
        let scratch = scratch_for(&mut self.scratch, device, &renderer.output_layout, &self.buffers, (w, h), false, source.format());
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo { origin: wgpu::Origin3d { x, y, z: 0 }, ..source.as_image_copy() },
            scratch.main.as_image_copy(), wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        let upload = &self.uploads[slot];
        let offset = upload.bytes.borrow().len().next_multiple_of(PARAMS_STRIDE as usize) as u64;
        assert!(offset + size_of::<NativePaletteParams>() as u64 <= self.buffers[slot].size(), "native palette origin bound");
        control_bytes(upload, offset, bytemuck::bytes_of(&NativePaletteParams { origin_x: x as i32, origin_y: y as i32, ..Default::default() }));
        (scratch.output[slot].clone(), offset as u32)
    }
}

/// Exact quad coverage bounds, using the same placement and clip rules as free_vs.
/// This computes geometry only. It never reads or composes image pixels.
pub(super) fn bounds(f: &FreeSprite, world: &WorldBuffers, sprites: &[SpriteDesc], p: &FrameParams,
    viewport: [f32; 4], scissor: [u32; 4], texture: &wgpu::Texture) -> Option<[u32; 4]> {
    use tile_render_shared::{drawn_sprite, placed, drawn_part, shrunk, world_period, wrap_distance};
    let scaled = |v: i32, n: u32, d: u32| (i64::from(v) * i64::from(n)).div_euclid(i64::from(d.max(1))) as i32;
    let zoomed = f.flags & FREE_ZOOMED != 0;
    let except = f.flags & FREE_EXCEPT_VIEW != 0;
    if f.set == 0 || f.category & p.hidden_categories != 0 ||
        (if zoomed { p.zoom_art != 2 || f.zoom != (p.zoom_num << 16 | p.zoom_den) } else { p.zoom_art != 0 && f.flags & FREE_LOW != 0 }) ||
        (if except { f.view == p.view || p.world == 0 } else { (f.view != ALL_VIEWS && f.view != p.view) || (f.view == ALL_VIEWS && p.world == 0) }) { return None; }
    let words = world.sets_source.words();
    let frames = *words.get(f.set as usize)?;
    if frames == 0 { return None; }
    let frame = tile_render_shared::free_frame_of(p.frame, p.host_tick_ms, f.ticks_per_frame, f.phase, frames);
    let id = *words.get((f.set + 1 + frame) as usize)?;
    let screen = f.flags & FREE_SCREEN != 0;
    let n = if screen { 1 } else { p.downscale.max(1) };
    let id = drawn_sprite(sprites, id, n);
    let sprite = sprites.get(id as usize)?;
    if sprite.blank & tile_render_shared::BLANK_EMPTY != 0 { return None; }
    let q = placed(sprite, n);
    let (bx, by) = if screen { (p.origin_x, p.origin_y) } else { (p.camera_x, p.camera_y) };
    let (zn, zd) = if zoomed { (p.zoom_num, p.zoom_den) } else { (1, 1) };
    let (fx, fy) = if screen { (f.x - bx, f.y - by) } else {
        let (px, py) = world_period(&p.grid);
        (wrap_distance(shrunk(f.x, n) - bx, px), wrap_distance(shrunk(f.y, n) - by, py))
    };
    let x0 = scaled(fx, zn, zd) + if zoomed { f.zoom_dx } else { 0 } + q.corner_x + q.inset_x as i32;
    let y0 = scaled(fy, zn, zd) + if zoomed { f.zoom_dy } else { 0 } + q.corner_y + q.inset_y as i32;
    let (l, t, r, b) = if p.hairline != 0 { (0, 0, q.width, q.height) } else { drawn_part(sprite, &q) };
    let (mut l, mut t, mut r, mut b) = (x0 + l as i32, y0 + t as i32, x0 + r as i32, y0 + b as i32);
    if f.flags & FREE_CLIP != 0 {
        let (ox, oy, cn) = if zoomed { (bx, by, n) } else { (p.origin_x, p.origin_y, 1) };
        l = l.max(scaled(shrunk(f.clip[0], cn) - ox, zn, zd));
        t = t.max(scaled(shrunk(f.clip[1], cn) - oy, zn, zd));
        r = r.min(scaled(shrunk(f.clip[2], cn) - ox, zn, zd));
        b = b.min(scaled(shrunk(f.clip[3], cn) - oy, zn, zd));
    }
    if r <= l || b <= t { return None; }
    if !zoomed && p.scaled != 0 {
        let span = |lo: i32, hi: i32| {
            let ceil = |v: i32| ((i64::from(v) * i64::from(p.zoom_num) + i64::from(p.zoom_den) - 1).div_euclid(i64::from(p.zoom_den))) as i32;
            let (a, b) = (ceil(lo), ceil(hi));
            if b > a { Some((a, b)) } else if p.hairline != 0 { let a = scaled(lo, p.zoom_num, p.zoom_den); Some((a, a + 1)) } else { None }
        };
        (l, r) = span(l, r)?; (t, b) = span(t, b)?;
    }
    let [sx, sy, sw, sh] = scissor;
    let x0 = (i64::from(l) + viewport[0] as i64).max(i64::from(sx)).max(0);
    let y0 = (i64::from(t) + viewport[1] as i64).max(i64::from(sy)).max(0);
    let x1 = (i64::from(r) + viewport[0] as i64).min(i64::from(sx) + i64::from(sw)).min(i64::from(texture.width()));
    let y1 = (i64::from(b) + viewport[1] as i64).min(i64::from(sy) + i64::from(sh)).min(i64::from(texture.height()));
    (x1 > x0 && y1 > y0).then_some([x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
}

impl Steps<'_> {
    pub(super) fn native_step_visible(&self, i: usize, params: &FrameParams, geometry: ([f32; 4], [u32; 4]), texture: &wgpu::Texture, (view, cells): (u32, bool)) -> bool {
        let sprites = self.renderer.atlas.as_ref().map_or(&[][..], |a| a.sprites.as_slice());
        self.world.free_after.get(i).into_iter().flatten().any(|(range, kind, class)| {
            free_run_visible(*kind, *class, view, cells, params.zoom_art) && (*kind != FreeKind::NativePaletteAlpha || range.clone().any(|instance| {
                self.world.scene_free.get(instance as usize).or_else(|| self.renderer.markers.get(instance as usize - self.world.scene_free.len()))
                    .and_then(|f| bounds(f, self.world, sprites, params, geometry.0, geometry.1, texture)).is_some()
            }))
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_native_free(&self, encoder: &mut wgpu::CommandEncoder,
        render: &mut Option<wgpu::RenderPass<'static>>, into: &wgpu::TextureView, texture: &wgpu::Texture,
        geometry: ([f32; 4], [u32; 4]), params: &FrameParams, params_offset: u32, i: usize,
        pipelines: FreePipelines<'_>, (view, cells): (u32, bool), glyph_cache: Option<&wgpu::BindGroup>) {
        let sprites = self.renderer.atlas.as_ref().map_or(&[][..], |a| a.sprites.as_slice());
        for (range, kind, class) in self.world.free_after.get(i).into_iter().flatten() {
            if !free_run_visible(*kind, *class, view, cells, params.zoom_art) { continue; }
            if *kind != FreeKind::NativePaletteAlpha {
                let pass = render.get_or_insert_with(|| begin_view(encoder, into, geometry, self.bind_group, params_offset, None).forget_lifetime());
                if matches!(kind, FreeKind::Text | FreeKind::TextKeyed) { pass.set_bind_group(1, glyph_cache.expect("text cache prepared"), &[]); }
                for pipeline in pipelines.of(*kind) { pass.set_pipeline(pipeline); pass.draw(0..QUAD_VERTICES, range.clone()); }
                continue;
            }
            for instance in range.clone() {
                let command = self.world.scene_free.get(instance as usize).or_else(|| self.renderer.markers.get(instance as usize - self.world.scene_free.len()));
                let Some(rect) = command.and_then(|f| bounds(f, self.world, sprites, params, geometry.0, geometry.1, texture)) else { continue };
                // Only visible, nonempty smoke ends a pass. Every instance
                // captures after all earlier overlapping commands finished.
                drop(render.take());
                let (snapshot, offset) = self.renderer.smoke.borrow_mut().as_mut().expect("native palette controls prepared")
                    .capture(self.renderer, self.device, encoder, texture, self.control_slot, rect);
                let mut pass = begin_view(encoder, into, geometry, self.bind_group, params_offset, None).forget_lifetime();
                pass.set_bind_group(1, &snapshot, &[offset]);
                pass.set_pipeline(self.renderer.pipeline(self.device, P::NativePaletteAlpha(pipelines.into)));
                pass.draw(0..QUAD_VERTICES, instance..instance + 1);
                *render = Some(pass);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtlasBuilder, Blend, Headless, Layout, PlacedSprite, RgbaImage, SolidRect, SpriteSets};
    fn world() -> TileWorld {
        let mut w = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, step: (1, 1), wrap_x: false, wrap_y: false }, SpriteSets::default());
        w.color = ColorModel::Quantized { bits: 5 }; w
    }
    fn expand(q: u32) -> u8 { ((q << 3) | (q >> 2)) as u8 }
    fn native(src: [u8; 4], dst: [u32; 3]) -> [u32; 3] {
        std::array::from_fn(|c| (u32::from(src[c]) * u32::from(src[3]) + 8 * dst[c] * (256 - u32::from(src[3]))) / 2048)
    }
    #[test]
    fn native_palette_all_raw_alpha_and_source_bytes_match_integer_oracle() {
        let gpu = Headless::new().expect("GPU");
        eprintln!("native palette adapter: {:?}", gpu.adapter.get_info());
        let mut builder = AtlasBuilder::new(512);
        let image = RgbaImage::from_fn(256, 256, |x, y| [x as u8, (x * 7 + 1) as u8, 255 - x as u8, y as u8]);
        let id = builder.add(&image, (0, 0)).unwrap();
        let atlas = builder.finish(); let mut world = world(); let set = world.sets.single(id);
        world.free.push(PlacedSprite { screen: true, blend: Blend::NativePaletteAlpha, ..PlacedSprite::new(set, 0, 0) });
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, TEST_SHADERS);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas); renderer.set_world(&gpu.device, &world);
        for d in 0..32 {
            let dst = [d, 31 - d, (d * 3) % 32];
            let alpha = (d * 7 + 13) as u8;
            let clear = [(dst[0] * 8) as u8, (dst[1] * 8) as u8, (dst[2] * 8) as u8, alpha];
            let got = gpu.render(&mut renderer, &Camera::new((256, 256)), d, clear);
            for y in 0..256 { for x in 0..256 {
                let q = native(image.get(x, y), dst);
                assert_eq!(got.get(x, y), [expand(q[0]), expand(q[1]), expand(q[2]), alpha], "dst={dst:?} src={x} alpha={y}");
            }}
            let state = renderer.smoke.borrow(); let state = state.as_ref().unwrap();
            let snapshot = &state.scratch.as_ref().unwrap().main;
            assert!(snapshot.width() <= 256 && snapshot.height() <= 256, "only affected rectangle may be retained");
        }
    }
    #[test]
    fn native_palette_overlaps_clip_and_packed_view_origins_stay_ordered() {
        let gpu = Headless::new().expect("GPU");
        let mut builder = AtlasBuilder::new(64);
        let image = RgbaImage::from_fn(13, 9, |x, y| [(x * 19 + 3) as u8, (y * 27 + 7) as u8, 173, [0, 1, 33, 129, 255][((x + y) % 5) as usize]]);
        let id = builder.add(&image, (0, 0)).unwrap(); let atlas = builder.finish();
        let mut world = world(); let set = world.sets.single(id);
        for view in 0..2 {
            world.free.push(PlacedSprite { screen: true, view: Some(view), blend: Blend::NativePaletteAlpha, clip: Some([0, 3, 10, 10]), ..PlacedSprite::new(set, -3, 2) });
            world.free.push(PlacedSprite { screen: true, view: Some(view), solid: Some(SolidRect { width: 3, height: 5, rgba: [0, 248, 0, 255] }), ..PlacedSprite::new(crate::SpriteSetId::EMPTY, 5, 3) });
            let twice = PlacedSprite { screen: true, view: Some(view), blend: Blend::NativePaletteAlpha, clip: Some([2, 0, 15, 11]), ..PlacedSprite::new(set, 4, 4) };
            world.free.extend([twice, twice]); // alpha255 must still compose twice.
        }
        let cameras = [Camera { origin: (7, 5), screen_shift: (7, 5), zoom: (3, 2), ..Camera::new((27, 18)) }, Camera { origin: (43, 4), screen_shift: (43, 4), zoom: (2, 1), ..Camera::new((32, 24)) }];
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, TEST_SHADERS);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas); renderer.set_world(&gpu.device, &world);
        let got = gpu.render_views(&mut renderer, &cameras, (80, 36), 0, [80, 120, 160, 255]);
        for (view, camera) in cameras.iter().enumerate() {
            for ty in 0..camera.viewport.1 { for tx in 0..camera.viewport.0 {
                let x = tx * camera.zoom.1 / camera.zoom.0; let y = ty * camera.zoom.1 / camera.zoom.0;
                let mut q = [10, 15, 20];
                if (0..10).contains(&(x as i32)) && (3..10).contains(&(y as i32)) && x + 3 < 13 && y >= 2 && y - 2 < 9 { q = native(image.get(x + 3, y - 2), q); }
                if (5..8).contains(&x) && (3..8).contains(&y) { q = [0, 31, 0]; }
                if (4..15).contains(&x) && (4..11).contains(&y) { let src = image.get(x - 4, y - 4); q = native(src, native(src, q)); }
                assert_eq!(got.get(tx + camera.origin.0 as u32, ty + camera.origin.1 as u32), [expand(q[0]), expand(q[1]), expand(q[2]), 255], "view={view} local={tx},{ty} source={x},{y}");
            }}
        }
    }
    #[test]
    fn native_palette_resources_stay_absent_for_ordinary_and_offscreen_draws() {
        let gpu = Headless::new().expect("GPU"); let mut builder = AtlasBuilder::new(512);
        let id = builder.add(&RgbaImage::from_fn(4, 4, |_, _| [91, 73, 55, 255]), (0, 0)).unwrap();
        let empty = builder.add(&RgbaImage::new(400, 300), (0, 0)).unwrap();
        let mut atlas = builder.finish(); atlas.trim_blank();
        let mut world = world(); let set = world.sets.single(id);
        world.free.push(PlacedSprite { screen: true, ..PlacedSprite::new(set, 0, 0) });
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, TEST_SHADERS);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas); renderer.set_world(&gpu.device, &world);
        let _ = gpu.render(&mut renderer, &Camera::new((16, 16)), 0, [0, 0, 0, 255]);
        assert!(renderer.smoke.borrow().is_none(), "ordinary frames allocate no smoke resources");
        world.free[0].blend = Blend::NativePaletteAlpha; world.free[0].x = -100;
        renderer.update_free(&gpu.device, &gpu.queue, &world);
        let _ = gpu.render(&mut renderer, &Camera::new((16, 16)), 1, [0, 0, 0, 255]);
        assert!(renderer.smoke.borrow().as_ref().unwrap().scratch.is_none(), "offscreen smoke captures no destination rectangle");
        world.free[0].x = 0; world.free[0].set = world.sets.single(empty);
        renderer.update_free(&gpu.device, &gpu.queue, &world);
        let _ = gpu.render(&mut renderer, &Camera::new((512, 384)), 2, [0, 0, 0, 255]);
        assert!(renderer.smoke.borrow().as_ref().unwrap().scratch.is_none(), "large proven empty smoke captures nothing despite saturated margins");

    }
}

#[cfg(test)]
mod native_capture_test {
    use super::*;
    use crate::{AtlasBuilder, Blend, Headless, Layout, PlacedSprite, RgbaImage, SpriteSets};
    #[test]
    #[ignore = "requires explicit captured native RGB555 tables and original ExplosionPalette PCX"]
    fn native_palette_matches_all_captured_game_table_entries() {
        let tables = std::fs::read(std::env::var("C3R_NATIVE_SMOKE_TABLES").expect("C3R_NATIVE_SMOKE_TABLES")).unwrap();
        assert_eq!(tables.len(), 31 * 32768 * 2);
        let pcx = std::fs::read(std::env::var("C3R_NATIVE_EXPLOSION_PALETTE").expect("C3R_NATIVE_EXPLOSION_PALETTE")).unwrap();
        let palette_at = pcx.len() - 769; assert_eq!(pcx[palette_at], 12);
        let smoke: Vec<_> = pcx[palette_at + 1..].chunks_exact(3).enumerate().filter(|(_, rgb)| *rgb != [0, 0, 255]).take(16).collect();
        assert_eq!(smoke.len(), 16);
        let gpu = Headless::new().expect("GPU"); let mut builder = AtlasBuilder::new(1024);
        let expand = |q: u32| ((q << 3) | (q >> 2)) as u8;
        let base = builder.add(&RgbaImage::from_fn(256, 128, |x, y| { let p = y * 256 + x; [expand((p >> 10) & 31), expand((p >> 5) & 31), expand(p & 31), 255] }), (0, 0)).unwrap();
        let ids: Vec<_> = smoke.iter().map(|(a, rgb)| builder.add(&RgbaImage::from_fn(256, 128, |_, _| [rgb[0], rgb[1], rgb[2], *a as u8]), (0, 0)).unwrap()).collect();
        let atlas = builder.finish(); let mut world = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal, step: (1, 1), wrap_x: false, wrap_y: false }, SpriteSets::default());
        world.color = ColorModel::Quantized { bits: 5 };
        let base = world.sets.single(base); let sets: Vec<_> = ids.into_iter().map(|id| world.sets.single(id)).collect();
        world.free.push(PlacedSprite { screen: true, ..PlacedSprite::new(base, 0, 0) });
        world.free.push(PlacedSprite { screen: true, blend: Blend::NativePaletteAlpha, ..PlacedSprite::new(sets[0], 0, 0) });
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, TEST_SHADERS);
        renderer.set_atlas(&gpu.device, &gpu.queue, &atlas); renderer.set_world(&gpu.device, &world);
        for (j, set) in sets.into_iter().enumerate() {
            world.free[1].set = set; renderer.update_free(&gpu.device, &gpu.queue, &world);
            let got = gpu.render(&mut renderer, &Camera::new((256, 128)), j as u32, [0, 0, 0, 255]);
            for p in 0..32768usize {
                let at = ((15 + j) * 32768 + p) * 2;
                let native = u32::from(u16::from_le_bytes([tables[at], tables[at + 1]])) & 0x7fff;
                let want = [expand((native >> 10) & 31), expand((native >> 5) & 31), expand(native & 31), 255];
                assert_eq!(got.get((p % 256) as u32, (p / 256) as u32), want, "captured table level={} RGB555={p:#06x}", 15 + j);
            }
        }
    }
}
