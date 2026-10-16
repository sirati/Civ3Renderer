//! A CPU implementation of exactly what the shaders draw.
//!
//! It is the oracle for the GPU path (tests compare both byte for byte) and
//! a fallback for tools that have no GPU.

use tile_render_shared::{ByteReader, QUANTIZED_BIAS, alpha_steps, cell_origin, expand, mix_stored, quantize, scale_stored, wrap_cell};

use crate::atlas::{Atlas, RgbaImage};
use crate::camera::Camera;
use crate::renderer::ScreenCopy;
use crate::world::{Blend, ColorModel, TileWorld};

/// One sprite draw: which sprite, and where its top-left lands in the
/// viewport (screen pixels at zoom 1).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Draw {
    pub sprite: crate::SpriteId,
    pub x: i32,
    pub y: i32,
    /// Index of the render pass, and the layer's blend.
    pub pass: usize,
    pub blend: Blend,
    /// Viewport rectangle the draw is limited to.
    pub clip: Option<[i32; 4]>,
}

/// The sprites `TileRenderer::draw` draws, in drawing order (without
/// clipping to the viewport).
pub fn draw_list(world: &TileWorld, atlas: &Atlas, camera: &Camera, frame: u32) -> Vec<Draw> {
    let mut out = Vec::new();
    visit(world, atlas, camera, 0, frame, 0, &mut |d| out.push(d));
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
    for (v, camera) in cameras.iter().enumerate().take(crate::renderer::MAX_VIEWS) {
        if !camera.is_scaled() {
            visit(world, atlas, camera, v as u32, frame, 0, &mut |d| {
                each_texel(atlas, &d, (w, h), |px, py, src| put(&mut out[(py * w + px) as usize], &d, src));
            });
            continue;
        }
        let (sw, sh) = camera.source_size();
        let flat = Camera { zoom: (1, 1), viewport: (sw, sh), ..*camera };
        let (ox, oy) = camera.origin;
        let mut source = vec![background; (sw * sh) as usize];
        let zoomed_art = camera.zoomed_art;
        visit(world, atlas, &flat, v as u32, frame, u32::from(zoomed_art), &mut |d| {
            let d = Draw { x: d.x - ox, y: d.y - oy, clip: d.clip.map(|[x0, y0, x1, y1]| [x0 - ox, y0 - oy, x1 - ox, y1 - oy]), ..d };
            each_texel(atlas, &d, (sw, sh), |px, py, src| put(&mut source[(py * sw + px) as usize], &d, src));
        });
        for (px, py) in camera.target_rect((w, h)) {
            let (sx, sy) = camera.source_texel((px, py));
            out[(py * w + px) as usize] = source[(sy * sw + sx) as usize];
        }
        // Sprites made for the zoom, at full resolution: from the view's
        // scaled screen origin.
        if zoomed_art {
            let (n, d) = (camera.zoom.0.max(1) as i32, camera.zoom.1.max(1) as i32);
            let shift = (ox + camera.screen_shift.0 * n / d, oy + camera.screen_shift.1 * n / d);
            let post = Camera { screen_shift: shift, ..*camera };
            visit(world, atlas, &post, v as u32, frame, 2, &mut |d| {
                each_texel(atlas, &d, (w, h), |px, py, src| put(&mut out[(py * w + px) as usize], &d, src));
            });
        }
    }
    out
}

/// Calls `f` for every opaque texel of `d` inside the viewport.
fn each_texel(atlas: &Atlas, d: &Draw, (w, h): (u32, u32), mut f: impl FnMut(u32, u32, [u8; 4])) {
    let sprite = atlas.sprites[d.sprite.0 as usize];
    let page = &atlas.pages[sprite.page as usize];
    for sy in 0..sprite.height as i32 {
        for sx in 0..sprite.width as i32 {
            let (px, py) = (d.x + sx, d.y + sy);
            if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
                continue;
            }
            if d.clip.is_some_and(|[x0, y0, x1, y1]| px < x0 || py < y0 || px >= x1 || py >= y1) {
                continue;
            }
            let src = page.get(sprite.atlas_x + sx as u32, sprite.atlas_y + sy as u32);
            if src[3] != 0 {
                f(px as u32, py as u32, src);
            }
        }
    }
}

/// Calls `draw` for every sprite view `view` (seen by `camera`) shows, in
/// order, in target pixels, clipped to the view.
/// `art`: which free sprites by their zoom art (`FrameParams::zoom_art`).
fn visit(world: &TileWorld, atlas: &Atlas, camera: &Camera, view: u32, frame: u32, art: u32, draw: &mut dyn FnMut(Draw)) {
    let art_fits = |s: &crate::PlacedSprite| {
        let zoomed = s.zoom_art == crate::ZoomArt::Zoomed;
        (art == 2) == zoomed && !(art == 1 && s.zoom_art == crate::ZoomArt::Low)
    };
    let (vx, vy) = camera.origin;
    let rect = [vx, vy, vx + camera.viewport.0 as i32, vy + camera.viewport.1 as i32];
    let within = |clip: Option<[i32; 4]>| {
        let c = clip.unwrap_or(rect);
        Some([c[0].max(rect[0]), c[1].max(rect[1]), c[2].min(rect[2]), c[3].min(rect[3])])
    };
    let packed = world.pack();
    let data = ByteReader::new(&packed.data);
    let passes = world.passes();
    // Free sprites following pass `p`, in list order.
    let free_after = |p: usize, draw: &mut dyn FnMut(Draw)| {
        for s in world.free.iter().filter(|s| s.after_pass.min(passes.len()) == p && s.view.map_or(camera.cells, |sv| sv == view) && art_fits(s)) {
            let set = s.set.0 as usize;
            let frames = packed.sets[set];
            if set == 0 || frames == 0 {
                continue;
            }
            let f = (frame / s.ticks_per_frame.max(1) + s.phase) % frames;
            let sprite_id = packed.sets[set + 1 + f as usize];
            let sprite = atlas.sprites[sprite_id as usize];
            // Screen sprites are given in target pixels (moved by the shift).
            let (sx, sy) = camera.screen_shift;
            let (cx, cy) = if s.screen { (-sx, -sy) } else { (camera.x - vx, camera.y - vy) };
            draw(Draw {
                sprite: crate::SpriteId(sprite_id),
                x: s.x + sprite.anchor_x - cx,
                y: s.y + sprite.anchor_y - cy,
                pass: p,
                blend: s.blend,
                clip: within(s.clip.map(|[x0, y0, x1, y1]| [x0 + sx, y0 + sy, x1 + sx, y1 + sy])),
            });
        }
    };
    for (pass_index, pass) in passes.iter().enumerate() {
        let (first, count) = (pass.first, pass.count);
        let params = camera.frame_params(&world.grid, world.color, frame, pass, view, 0);
        for local in 0..if camera.cells && art != 2 { params.cols * params.rows } else { 0 } {
            let col = params.first_col + (local % params.cols) as i32;
            let row = params.first_row + (local / params.cols) as i32;
            let Some((c, r)) = wrap_cell(&params.grid, col, row) else { continue };
            let (ox, oy) = cell_origin(&params.grid, col, row);
            for layer in &packed.layers[first as usize..(first + count) as usize] {
                if layer.flags & tile_render_shared::LAYER_HIDDEN != 0 {
                    continue;
                }
                if let Some((x0, y0, x1, y1)) = camera.cull {
                    let (ax, ay) = (ox + layer.cull_offset_x, oy + layer.cull_offset_y);
                    if ax < x0 || ax >= x1 || ay < y0 || ay >= y1 {
                        continue;
                    }
                }
                let byte = layer.data_byte_offset + (r * params.grid.cols + c) * (layer.value_bytes + layer.phase_bytes);
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
                let f = (frame / layer.ticks_per_frame.max(1) + phase) % frames;
                let sprite_id = packed.sets[set + 1 + f as usize];
                let sprite = atlas.sprites[sprite_id as usize];
                draw(Draw {
                    sprite: crate::SpriteId(sprite_id),
                    x: ox + layer.offset_x + sprite.anchor_x - camera.x + vx,
                    y: oy + layer.offset_y + sprite.anchor_y - camera.y + vy,
                    pass: pass_index,
                    blend: if layer.scale_den == 0 { Blend::Alpha } else { Blend::Scale { denominator: layer.scale_den } },
                    clip: within(None),
                });
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
