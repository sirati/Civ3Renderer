//! A CPU implementation of exactly what the shaders draw.
//!
//! It is the oracle for the GPU path (tests compare both byte for byte) and
//! a fallback for tools that have no GPU.

use tile_render_shared::text;
use tile_render_shared::{ByteReader, Placed, cell_offset, frame_of, record_at, window_col, QUANTIZED_BIAS, alpha_steps, cell_origin, drawn_sprite, expand, mix_stored, placed, quantize, scale_stored, shrunk, wrap_cell};

use crate::atlas::{Atlas, RgbaImage};
use crate::camera::Camera;
use crate::renderer::ScreenCopy;
use crate::world::{Blend, ColorModel, TileWorld};

/// One sprite draw: which sprite, and where its top-left lands in the
/// viewport (screen pixels at zoom 1).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Draw {
    /// The sprite drawn (in a view showing the world smaller, its
    /// `downscaled` twin if it has one).
    pub sprite: crate::SpriteId,
    /// Where the sprite's top-left lands (at the view's scale).
    pub x: i32,
    pub y: i32,
    /// Index of the render pass, and the layer's blend.
    pub pass: usize,
    pub blend: Blend,
    /// Viewport rectangle the draw is limited to.
    pub clip: Option<[i32; 4]>,
    /// Which texels are drawn where, from (`x`, `y`) (its corner left
    /// aside): the whole sprite, or in a view showing the world smaller
    /// what `tile_render_shared::placed` says.
    pub placed: Placed,
    /// Text drawn from outlines instead of the sprite: the glyph's pixels
    /// from (`x`, `y`) on.
    pub text: Option<TextPixels>,
}

impl Draw {
    /// The drawn rectangle's top-left.
    fn rect_x(&self) -> i32 {
        self.x + self.placed.inset_x as i32
    }

    fn rect_y(&self) -> i32 {
        self.y + self.placed.inset_y as i32
    }

    /// The atlas texel at pixel (i, j) of the drawn rectangle.
    fn texel(&self, sprite: &tile_render_shared::SpriteDesc, i: u32, j: u32) -> (u32, u32) {
        let p = &self.placed;
        (sprite.atlas_x + p.texel_x + p.step * i, sprite.atlas_y + p.texel_y + p.step * j)
    }
}

/// A glyph drawn from its outline (see `tile_render_shared::text`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TextPixels {
    pub place: tile_render_shared::text::GlyphPlace,
    /// How many pixels it may cover from the draw's position.
    pub size: (i32, i32),
    /// The target pixel the place's coordinates start at.
    pub origin: (i32, i32),
    /// 0xRRGGBB; the colour key, if it is drawn onto one.
    pub color: u32,
    pub key: Option<u32>,
}

/// Calls `f` for every pixel of text draw `d` (`t`) inside the viewport and
/// its clip with what it draws there: coverage per channel (alpha their
/// maximum), or with a colour key the opaque colour.
fn each_text_pixel(words: &[u32], d: &Draw, t: &TextPixels, (w, h): (u32, u32), mut f: impl FnMut(u32, u32, [u8; 4])) {
    let clip = d.clip.unwrap_or([i32::MIN, i32::MIN, i32::MAX, i32::MAX]);
    let (x0, y0) = (d.x.max(clip[0]).max(0), d.y.max(clip[1]).max(0));
    let (x1, y1) = ((d.x + t.size.0).min(clip[2]).min(w as i32), (d.y + t.size.1).min(clip[3]).min(h as i32));
    for py in y0..y1 {
        for px in x0..x1 {
            let c = tile_render_shared::text::coverage(words, &t.place, px - t.origin.0, py - t.origin.1, t.color);
            let src = match t.key {
                Some(key) => match tile_render_shared::text::keyed(c, t.color, key) {
                    Some([r, g, b]) => [r as u8, g as u8, b as u8, 255],
                    None => continue,
                },
                None if c == [0; 3] => continue,
                None => [c[0] as u8, c[1] as u8, c[2] as u8, c[0].max(c[1]).max(c[2]) as u8],
            };
            f(px as u32, py as u32, src);
        }
    }
}

/// The sprites `TileRenderer::draw` draws, in drawing order (without
/// clipping to the viewport).
pub fn draw_list(world: &TileWorld, atlas: &Atlas, camera: &Camera, frame: u32) -> Vec<Draw> {
    let mut out = Vec::new();
    visit(world, atlas, camera, 0, frame, 0, 0..usize::MAX, false, &mut |d, _| out.push(d));
    out
}

/// Renders like `TileRenderer::render`.
pub fn render(world: &TileWorld, atlas: &Atlas, camera: &Camera, frame: u32, clear: [u8; 4]) -> RgbaImage {
    render_views(world, atlas, std::slice::from_ref(camera), camera.viewport, frame, clear)
}

/// Renders like `TileRenderer::render_views` into a target of `size`.
pub fn render_views(world: &TileWorld, atlas: &Atlas, cameras: &[Camera], size: (u32, u32), frame: u32, clear: [u8; 4]) -> RgbaImage {
    render_composed(world, atlas, cameras, &[], size, frame, clear)
}

/// Renders like `TileRenderer::render_composed` into a target of `size`.
pub fn render_composed(
    world: &TileWorld,
    atlas: &Atlas,
    cameras: &[Camera],
    copies: &[ScreenCopy],
    size: (u32, u32),
    frame: u32,
    clear: [u8; 4],
) -> RgbaImage {
    let (w, h) = size;
    let copies = &copies[..copies.len().min(crate::renderer::MAX_COPIES)];
    let bits = match world.color {
        ColorModel::Direct => {
            let mut out = compose(world, atlas, cameras, size, frame, clear, |dst: &mut [u8; 4], d, src| match d.blend {
                Blend::Alpha => *dst = blend(src, *dst),
                // Each channel with its own coverage (the GPU draws one channel
                // at a time).
                Blend::Subpixel { color } => {
                    for c in 0..3 {
                        let a = src[c] as f32 / 255.0;
                        dst[c] = (color[c] as f32 * a + dst[c] as f32 * (1.0 - a)).round() as u8;
                    }
                }
                Blend::Invert => *dst = [255 - dst[0], 255 - dst[1], 255 - dst[2], dst[3]],
                // Black at alpha 1 − k/den, as the shader blends it.
                Blend::Scale { denominator } if (src[0] as u32) < denominator => {
                    let keep = src[0] as f32 / denominator as f32;
                    let a = 1.0 - keep;
                    let [r, g, b, da] = dst.map(|c| c as f32);
                    *dst = [(r * keep).round() as u8, (g * keep).round() as u8, (b * keep).round() as u8, (a * 255.0 + da * keep).round() as u8];
                }
                Blend::Scale { .. } => {}
            });
            apply_copies(&mut out, size, copies);
            return RgbaImage::from_fn(w, h, |x, y| out[(y * w + x) as usize]);
        }
        ColorModel::Quantized { bits } => bits,
    };
    // Stored values (q + QUANTIZED_BIAS), like the GPU's internal target.
    let stored = |c: u8| quantize(c.into(), bits) + QUANTIZED_BIAS;
    let background = [stored(clear[0]), stored(clear[1]), stored(clear[2])];
    let mut out = compose(world, atlas, cameras, size, frame, background, |dst: &mut [u32; 3], d, src| match d.blend {
        Blend::Scale { denominator } if (src[0] as u32) < denominator => {
            for c in dst.iter_mut() {
                *c = scale_stored(*c, src[0] as u32, denominator);
            }
        }
        Blend::Scale { .. } => {}
        // Translucent texels mix in whole alpha steps, rounded down.
        // ClearType-like: each channel with its own coverage.
        Blend::Subpixel { color } => {
            for (c, i) in dst.iter_mut().zip(0..3) {
                let k = alpha_steps(src[i].into());
                if k > 0 {
                    *c = mix_stored(stored(color[i]), *c, k);
                }
            }
        }
        // (Inverting: the highest stored value less what is there.)
        Blend::Invert => {
            let top = (1 << bits) - 1 + 2 * QUANTIZED_BIAS;
            for c in dst.iter_mut() {
                *c = top - *c;
            }
        }
        Blend::Alpha => {
            let k = alpha_steps(src[3].into());
            if k > 0 {
                for (c, s) in dst.iter_mut().zip(src) {
                    *c = mix_stored(stored(s), *c, k);
                }
            }
        }
    });
    apply_copies(&mut out, size, copies);
    RgbaImage::from_fn(w, h, |x, y| {
        let p = out[(y * w + x) as usize];
        let c = |v: u32| expand(v.saturating_sub(QUANTIZED_BIAS), bits) as u8;
        [c(p[0]), c(p[1]), c(p[2]), clear[3]]
    })
}

/// Copies rectangles of the finished picture `out` on top of it.
fn apply_copies<P: Copy>(out: &mut [P], (w, h): (u32, u32), copies: &[ScreenCopy]) {
    let finished = out.to_vec();
    for c in copies {
        let Some([x, y, cw, ch]) = c.scissor((w, h)) else { continue };
        let (n, d) = (c.zoom.0.max(1), c.zoom.1.max(1));
        for py in y..y + ch {
            for px in x..x + cw {
                let sx = c.source[0] + ((px as i32 - c.at.0) as u32 * d / n) as i32;
                let sy = c.source[1] + ((py as i32 - c.at.1) as u32 * d / n) as i32;
                out[(py * w + px) as usize] = finished[(sy as u32 * w + sx as u32) as usize];
            }
        }
    }
}

/// The views composed into a `size` canvas of pixels `P` starting as
/// `background`, `put` blending one texel. A zoomed view is drawn at zoom 1
/// on its own `background`, then magnified (or shrunk) into its rectangle
/// by nearest texel, replacing what is there; later views draw over it.
/// From the first zoomed sprite of a view showing them on (see
/// `camera::Placement`), its group is drawn at its zoom straight into the
/// canvas: every target pixel blended with the texels its zoom-1 pixel
/// gets, zoomed art at full resolution.
fn compose<P: Copy>(
    world: &TileWorld,
    atlas: &Atlas,
    cameras: &[Camera],
    (w, h): (u32, u32),
    frame: u32,
    background: P,
    mut put: impl FnMut(&mut P, &Draw, [u8; 4]),
) -> Vec<P> {
    let mut out = vec![background; (w * h) as usize];
    let cameras = &cameras[..cameras.len().min(crate::renderer::MAX_VIEWS)];
    let first_zoomed = crate::world::first_zoomed(&world.free, world.passes().len().min(crate::renderer::MAX_PASSES));
    let placements = crate::camera::placements(cameras, |z| crate::world::zoomed_from(&first_zoomed, z));
    let (words, _) = crate::world::outline_words(&world.fonts);
    // The source texture: every scaled group's picture, laid out the same.
    let (sw, sh) = placements.iter().filter_map(|p| p.source).fold((1, 1), |(sw, sh), r| (sw.max(r[0] + r[2]), sh.max(r[1] + r[3])));
    let mut source = vec![background; (sw * sh) as usize];
    for (v, place) in placements.iter().enumerate() {
        // Draws come in target pixels from the drawn camera's origin; they
        // land in the view's rectangle, clipped to it (and to its group's
        // picture or the target).
        let drawn = place.drawn;
        let [rx, ry, rw, rh] = place.rect;
        let (dx, dy) = (rx - drawn.origin.0, ry - drawn.origin.1);
        let bounds = match place.source {
            Some(r) => [r[0] as i32, r[1] as i32, (r[0] + r[2]) as i32, (r[1] + r[3]) as i32],
            None => [0, 0, w as i32, h as i32],
        };
        let rect = [rx.max(bounds[0]), ry.max(bounds[1]), (rx + rw).min(bounds[2]), (ry + rh).min(bounds[3])];
        let art = u32::from(place.art);
        let (buffer, bw, size) = match place.source {
            Some(_) => (&mut source, sw, (sw, sh)),
            None => (&mut out, w, (w, h)),
        };
        visit(world, atlas, &drawn, v as u32, frame, art, place.steps_at_one(), false, &mut |d, _| {
            let clip = d.clip.unwrap_or([i32::MIN, i32::MIN, i32::MAX, i32::MAX]);
            let clip = [clip[0].saturating_add(dx), clip[1].saturating_add(dy), clip[2].saturating_add(dx), clip[3].saturating_add(dy)];
            let clip = [clip[0].max(rect[0]), clip[1].max(rect[1]), clip[2].min(rect[2]), clip[3].min(rect[3])];
            let d = Draw { x: d.x + dx, y: d.y + dy, clip: Some(clip), ..d };
            each_texel(atlas, &d, size, |px, py, src| put(&mut buffer[(py * bw + px) as usize], &d, src));
        });
        // The group's picture scaled into its rectangle.
        if let Some((group, r)) = place.blit {
            for (px, py) in group.target_rect((w, h)) {
                let (sx, sy) = group.source_texel((px, py));
                out[(py * w + px) as usize] = source[((r[1] + sy) * sw + r[0] + sx) as usize];
            }
        }
        // The rest at its zoom: draws in the camera's zoom-1 pixels from its
        // origin (zoomed art in target pixels), each target pixel in the
        // scissor showing its zoom-1 pixel.
        if place.scaled_from.is_some() {
            let c = place.scaled;
            let s = place.scaled_scissor;
            let scissor = [s[0].max(0), s[1].max(0), s[2].min(w as i32), s[3].min(h as i32)];
            let (n, dd) = (c.zoom.0.max(1), c.zoom.1.max(1));
            let (ox, oy) = c.origin;
            let art = if place.art { 2 } else { 0 };
            visit(world, atlas, &c, v as u32, frame, art, place.steps_scaled(), true, &mut |d, zoomed| {
                let clip = d.clip.unwrap_or([i32::MIN, i32::MIN, i32::MAX, i32::MAX]);
                if zoomed {
                    let clip = [clip[0].max(scissor[0]), clip[1].max(scissor[1]), clip[2].min(scissor[2]), clip[3].min(scissor[3])];
                    let d = Draw { clip: Some(clip), ..d };
                    match &d.text {
                        Some(t) => each_text_pixel(&words, &d, t, (w, h), |px, py, src| put(&mut out[(py * w + px) as usize], &d, src)),
                        None => each_texel(atlas, &d, (w, h), |px, py, src| put(&mut out[(py * w + px) as usize], &d, src)),
                    }
                    return;
                }
                let sprite = atlas.sprites[d.sprite.0 as usize];
                // The sprite's zoom-1 rectangle from the origin, clipped.
                let (x0, y0) = (d.rect_x() - ox, d.rect_y() - oy);
                let l = x0.max(clip[0].saturating_sub(ox));
                let t = y0.max(clip[1].saturating_sub(oy));
                let r = (x0 + d.placed.width as i32).min(clip[2].saturating_sub(ox));
                let b = (y0 + d.placed.height as i32).min(clip[3].saturating_sub(oy));
                if r <= l || b <= t {
                    return;
                }
                let (Some((tl, tr)), Some((tt, tb))) = (scaled_span(l, r, n, dd, c.hairline), scaled_span(t, b, n, dd, c.hairline)) else { return };
                let page = &atlas.pages[sprite.page as usize];
                for py in (oy + tt).max(scissor[1])..(oy + tb).min(scissor[3]) {
                    for px in (ox + tl).max(scissor[0])..(ox + tr).min(scissor[2]) {
                        let sx = ((px - ox) as i64 * dd as i64).div_euclid(n as i64) as i32;
                        let sy = ((py - oy) as i64 * dd as i64).div_euclid(n as i64) as i32;
                        let (tx, ty) = d.texel(&sprite, (sx.max(l) - x0) as u32, (sy.max(t) - y0) as u32);
                        let src = page.get(tx, ty);
                        if src[3] != 0 {
                            put(&mut out[(py as u32 * w + px as u32) as usize], &d, src);
                        }
                    }
                }
            });
        }
    }
    out
}

/// The pixels `p` whose zoom-1 pixel `floor(p·d/n)` is in `[lo, hi)`, as in
/// the shaders: from `ceil(lo·n/d)` to `ceil(hi·n/d)`; with `hair`, at least
/// the pixel holding `lo·n/d`.
fn scaled_span(lo: i32, hi: i32, n: u32, d: u32, hair: bool) -> Option<(i32, i32)> {
    let floor = |v: i32| (v as i64 * n as i64).div_euclid(d as i64) as i32;
    let (a, b) = (-floor(-lo), -floor(-hi));
    if b > a {
        Some((a, b))
    } else if hair {
        Some((floor(lo), floor(lo) + 1))
    } else {
        None
    }
}

/// Calls `f` for every opaque texel of `d` inside the viewport.
fn each_texel(atlas: &Atlas, d: &Draw, (w, h): (u32, u32), mut f: impl FnMut(u32, u32, [u8; 4])) {
    let sprite = atlas.sprites[d.sprite.0 as usize];
    let page = &atlas.pages[sprite.page as usize];
    for sy in 0..d.placed.height {
        for sx in 0..d.placed.width {
            let (px, py) = (d.rect_x() + sx as i32, d.rect_y() + sy as i32);
            if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
                continue;
            }
            if d.clip.is_some_and(|[x0, y0, x1, y1]| px < x0 || py < y0 || px >= x1 || py >= y1) {
                continue;
            }
            let (tx, ty) = d.texel(&sprite, sx, sy);
            let src = page.get(tx, ty);
            if src[3] != 0 {
                f(px as u32, py as u32, src);
            }
        }
    }
}

/// Calls `draw` for every sprite view `view` (seen by `camera`) shows in
/// `steps` (see `camera::Placement`), in order, in target pixels, clipped to
/// the view; and whether it is zoomed art.
/// `art`: which free sprites by their zoom art (`FrameParams::zoom_art`).
/// `scaled`: the view is drawn at its zoom, its draws come in its zoom-1
/// pixels from its origin (zoomed art in target pixels), clipped only to
/// their own clip rectangles.
#[allow(clippy::too_many_arguments)]
fn visit(world: &TileWorld, atlas: &Atlas, camera: &Camera, view: u32, frame: u32, art: u32, steps: std::ops::Range<usize>, scaled: bool, draw: &mut dyn FnMut(Draw, bool)) {
    // Zoomed art only for a view at its zoom, drawn at it.
    let zoom = (camera.zoom.0.max(1), camera.zoom.1.max(1));
    let zoomed_for = |s: &crate::PlacedSprite| match s.zoom_art {
        crate::ZoomArt::Zoomed { zoom: z, offset } => Some((z, offset)),
        _ => None,
    };
    let art_fits = |s: &crate::PlacedSprite| match zoomed_for(s) {
        _ if s.text.is_some() => art == 2,
        Some((z, _)) => art == 2 && z == zoom,
        None => !(art != 0 && s.zoom_art == crate::ZoomArt::Low),
    };
    let (vx, vy) = camera.origin;
    let rect = [vx, vy, vx + camera.viewport.0 as i32, vy + camera.viewport.1 as i32];
    let within = |clip: Option<[i32; 4]>| {
        if scaled {
            return clip;
        }
        let c = clip.unwrap_or(rect);
        Some([c[0].max(rect[0]), c[1].max(rect[1]), c[2].min(rect[2]), c[3].min(rect[3])])
    };
    let packed = world.pack();
    let (words, bases) = crate::world::outline_words(&world.fonts);
    let data = ByteReader::new(&packed.data);
    let passes = world.passes();
    // The world at the view's scale (see `Camera::downscale`).
    let dn = camera.downscale.max(1);
    let period = tile_render_shared::world_period(&tile_render_shared::grid_at(&world.grid.params(), dn));
    // Free sprites following pass `p`, in list order.
    let free_after = |p: usize, draw: &mut dyn FnMut(Draw, bool)| {
        if !steps.contains(&(2 * p + 1)) {
            return;
        }
        for s in world.free.iter().filter(|s| s.after_pass.min(passes.len()) == p && s.view.map_or(camera.cells, |sv| if s.except_view { camera.cells && sv != view } else { sv == view }) && art_fits(s)) {
            if let Some(t) = s.text {
                // Zoomed art for every zoom: at its 1:1 position (from the
                // screen origin or the camera; a world glyph's at the view's
                // scale, its copy near the camera) scaled, its clip
                // likewise; at its own size.
                let Some(g) = t.gpu(s, &world.fonts, &bases) else { continue };
                let n = if s.screen { 1 } else { dn };
                let (sx, sy) = camera.screen_shift;
                let (bx, by) = if s.screen { (vx - sx, vy - sy) } else { (camera.x, camera.y) };
                let (rx, ry) = if s.screen {
                    (s.x - bx, s.y - by)
                } else {
                    (tile_render_shared::wrap_distance(shrunk(s.x, n) - bx, period.0), tile_render_shared::wrap_distance(shrunk(s.y, n) - by, period.1))
                };
                let Some(place) = text::place(&words, &g, rx, ry, zoom.0, zoom.1) else { continue };
                let (l, t_, r, b) = text::bounds(&words, &place);
                let clip = text::clip_at_zoom(&g, (bx, by), zoom.0, zoom.1, n).map(|[x0, y0, x1, y1]| [vx + x0, vy + y0, vx + x1, vy + y1]);
                draw(Draw {
                    sprite: crate::SpriteId(0),
                    x: vx + l,
                    y: vy + t_,
                    pass: p,
                    blend: if t.key.is_some() { crate::Blend::Alpha } else { crate::Blend::Subpixel { color: t.color } },
                    clip: within(clip),
                    placed: Placed::default(),
                    text: Some(TextPixels { place, size: (r - l, b - t_), origin: (vx, vy), color: g.color, key: t.key.map(|_| g.key) }),
                }, true);
                continue;
            }
            let set = s.set.0 as usize;
            let frames = packed.sets[set];
            if set == 0 || frames == 0 {
                continue;
            }
            let f = (frame / s.ticks_per_frame.max(1) + s.phase) % frames;
            // World sprites at the view's scale; screen sprites as they are.
            let n = if s.screen { 1 } else { dn };
            let sprite_id = drawn_sprite(&atlas.sprites, packed.sets[set + 1 + f as usize], n);
            let placed = placed(&atlas.sprites[sprite_id as usize], n);
            // In a wrapping world, the copy of a world sprite near the camera.
            let s = &crate::PlacedSprite {
                x: if s.screen { s.x } else { camera.x + tile_render_shared::wrap_distance(shrunk(s.x, n) - camera.x, period.0) },
                y: if s.screen { s.y } else { camera.y + tile_render_shared::wrap_distance(shrunk(s.y, n) - camera.y, period.1) },
                ..*s
            };
            // Screen sprites are given in target pixels (moved by the shift).
            let (sx, sy) = camera.screen_shift;
            let (cx, cy) = if s.screen { (-sx, -sy) } else { (camera.x - vx, camera.y - vy) };
            if let Some(((zn, zd), (dx, dy))) = zoomed_for(s) {
                // At its 1:1 position (from the screen origin or the camera)
                // scaled, plus its offset; its clip likewise (a world
                // sprite's at the view's scale).
                let (bx, by) = if s.screen { (vx - sx, vy - sy) } else { (camera.x, camera.y) };
                let sc = |v: i32, b: i32| ((v - b) as i64 * zn as i64).div_euclid(zd as i64) as i32;
                let cl = |v: i32, b: i32| sc(shrunk(v, n), b);
                draw(Draw {
                    sprite: crate::SpriteId(sprite_id),
                    x: vx + sc(s.x, bx) + dx + placed.corner_x,
                    y: vy + sc(s.y, by) + dy + placed.corner_y,
                    pass: p,
                    blend: s.blend,
                    clip: within(s.clip.map(|[x0, y0, x1, y1]| [vx + cl(x0, bx), vy + cl(y0, by), vx + cl(x1, bx), vy + cl(y1, by)])),
                    placed,
                    text: None,
                }, true);
                continue;
            }
            draw(Draw {
                sprite: crate::SpriteId(sprite_id),
                x: s.x + placed.corner_x - cx,
                y: s.y + placed.corner_y - cy,
                pass: p,
                blend: s.blend,
                clip: within(s.clip.map(|[x0, y0, x1, y1]| [x0 + sx, y0 + sy, x1 + sx, y1 + sy])),
                placed,
                text: None,
            }, false);
        }
    };
    for (pass_index, pass) in passes.iter().enumerate() {
        let (first, count) = (pass.first, pass.count);
        // (Its grid is the grid at the view's scale.)
        let params = camera.frame_params(&world.grid, world.color, frame, pass, view, 0, None, packed.reach);
        for local in 0..if camera.cells && steps.contains(&(2 * pass_index)) { params.cols * params.rows } else { 0 } {
            let col = window_col(&params, local);
            let row = params.first_row + (local / params.cols) as i32;
            let Some((c, r)) = wrap_cell(&params.grid, col, row) else { continue };
            let (ox, oy) = cell_origin(&params.grid, col, row);
            for layer in &packed.layers[first as usize..(first + count) as usize] {
                if layer.flags & tile_render_shared::LAYER_HIDDEN != 0 {
                    continue;
                }
                if let Some((x0, y0, x1, y1)) = camera.cull {
                    let (ax, ay) = (ox + shrunk(layer.cull_offset_x, dn), oy + shrunk(layer.cull_offset_y, dn));
                    if ax < x0 || ax >= x1 || ay < y0 || ay >= y1 {
                        continue;
                    }
                }
                let byte = record_at(layer, params.grid.cols, c, r);
                let value = data.uint(byte, layer.value_bytes);
                if value >= layer.lut_len {
                    continue;
                }
                let phase = if layer.phase_bytes != 0 { data.u8(byte + layer.value_bytes) } else { 0 };
                let set = packed.lut[(layer.lut_offset + value) as usize] as usize;
                let frames = packed.sets[set];
                if frames == 0 {
                    continue;
                }
                let f = frame_of(frame, layer.ticks_per_frame, phase, frames);
                let (dx, dy) = cell_offset(&data, layer, byte, dn);
                let sprite_id = drawn_sprite(&atlas.sprites, packed.sets[set + 1 + f as usize], dn);
                let placed = placed(&atlas.sprites[sprite_id as usize], dn);
                draw(Draw {
                    sprite: crate::SpriteId(sprite_id),
                    x: ox + shrunk(layer.offset_x, dn) + dx + placed.corner_x - camera.x + vx,
                    y: oy + shrunk(layer.offset_y, dn) + dy + placed.corner_y - camera.y + vy,
                    pass: pass_index,
                    blend: if layer.scale_den == 0 { Blend::Alpha } else { Blend::Scale { denominator: layer.scale_den } },
                    clip: within(None),
                    placed,
                    text: None,
                }, false);
            }
        }
        free_after(pass_index, draw);
    }
    free_after(passes.len(), draw);
}

/// `BlendState::ALPHA_BLENDING` in 8-bit: colour = src·a + dst·(1−a),
/// alpha = src.a + dst.a·(1−src.a). Exact for a = 0 and a = 255, which is all
/// that pixel-exact content uses.
fn blend(src: [u8; 4], dst: [u8; 4]) -> [u8; 4] {
    if src[3] == 255 {
        return src;
    }
    let a = src[3] as f32 / 255.0;
    let mix = |s: u8, d: u8| (s as f32 * a + d as f32 * (1.0 - a)).round() as u8;
    [
        mix(src[0], dst[0]),
        mix(src[1], dst[1]),
        mix(src[2], dst[2]),
        (src[3] as f32 + dst[3] as f32 * (1.0 - a)).round() as u8,
    ]
}
