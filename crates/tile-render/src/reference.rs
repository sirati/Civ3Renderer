//! A CPU implementation of exactly what the shaders draw.
//!
//! It is the oracle for the GPU path (tests compare both byte for byte) and
//! a fallback for tools that have no GPU.

use tile_render_shared::{ByteReader, QUANTIZED_BIAS, alpha_steps, cell_origin, expand, mix_stored, quantize, scale_stored, wrap_cell};

use crate::atlas::{Atlas, RgbaImage};
use crate::camera::Camera;
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
    visit(world, atlas, camera, frame, |d| out.push(d));
    out
}

/// Renders like `TileRenderer::render`. Only zoom 1/1 is supported (that is
/// what pixel-exact comparisons use).
pub fn render(world: &TileWorld, atlas: &Atlas, camera: &Camera, frame: u32, clear: [u8; 4]) -> RgbaImage {
    assert_eq!(camera.zoom, (1, 1), "the reference renderer only implements zoom 1");
    let (w, h) = camera.viewport;
    let bits = match world.color {
        ColorModel::Direct => return render_direct(world, atlas, camera, frame, clear),
        ColorModel::Quantized { bits } => bits,
    };
    // Stored values (q + QUANTIZED_BIAS), like the GPU's internal target.
    let stored = |c: u8| quantize(c.into(), bits) + QUANTIZED_BIAS;
    let mut out: Vec<[u32; 3]> = vec![[stored(clear[0]), stored(clear[1]), stored(clear[2])]; (w * h) as usize];
    visit(world, atlas, camera, frame, |d| {
        each_texel(atlas, &d, (w, h), |px, py, src| {
            let dst = &mut out[(py * w + px) as usize];
            match d.blend {
                Blend::Scale { denominator } if (src[0] as u32) < denominator => {
                    for c in dst.iter_mut() {
                        *c = scale_stored(*c, src[0] as u32, denominator);
                    }
                }
                Blend::Scale { .. } => {}
                // Translucent texels mix in whole alpha steps, rounded down.
                Blend::Alpha => {
                    let k = alpha_steps(src[3].into());
                    if k > 0 {
                        for (c, s) in dst.iter_mut().zip(src) {
                            *c = mix_stored(stored(s), *c, k);
                        }
                    }
                }
            }
        });
    });
    RgbaImage::from_fn(w, h, |x, y| {
        let p = out[(y * w + x) as usize];
        let c = |v: u32| expand(v.saturating_sub(QUANTIZED_BIAS), bits) as u8;
        [c(p[0]), c(p[1]), c(p[2]), clear[3]]
    })
}

fn render_direct(world: &TileWorld, atlas: &Atlas, camera: &Camera, frame: u32, clear: [u8; 4]) -> RgbaImage {
    let (w, h) = camera.viewport;
    let mut out = RgbaImage::from_fn(w, h, |_, _| clear);
    visit(world, atlas, camera, frame, |d| {
        each_texel(atlas, &d, (w, h), |px, py, src| {
            let src = match d.blend {
                Blend::Alpha => src,
                Blend::Scale { denominator } => [0, 0, 0, (255 - src[0] as u32 * 255 / denominator) as u8],
            };
            out.put(px, py, blend(src, out.get(px, py)));
        });
    });
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

/// Calls `draw` for every sprite the renderer would draw, in order.
fn visit(world: &TileWorld, atlas: &Atlas, camera: &Camera, frame: u32, mut draw: impl FnMut(Draw)) {
    let packed = world.pack();
    let data = ByteReader::new(&packed.data);
    let passes = world.passes();
    // Free sprites following pass `p`, in list order.
    let free_after = |p: usize, draw: &mut dyn FnMut(Draw)| {
        for s in world.free.iter().filter(|s| s.after_pass.min(passes.len()) == p) {
            let set = s.set.0 as usize;
            let frames = packed.sets[set];
            if set == 0 || frames == 0 {
                continue;
            }
            let f = (frame / s.ticks_per_frame.max(1) + s.phase) % frames;
            let sprite_id = packed.sets[set + 1 + f as usize];
            let sprite = atlas.sprites[sprite_id as usize];
            let (cx, cy) = if s.screen { (0, 0) } else { (camera.x, camera.y) };
            draw(Draw { sprite: crate::SpriteId(sprite_id), x: s.x + sprite.anchor_x - cx, y: s.y + sprite.anchor_y - cy, pass: p, blend: s.blend, clip: s.clip });
        }
    };
    for (pass_index, pass) in passes.iter().enumerate() {
        let (first, count) = (pass.first, pass.count);
        let params = camera.frame_params(&world.grid, world.color, frame, pass);
        for local in 0..params.cols * params.rows {
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
                    x: ox + layer.offset_x + sprite.anchor_x - camera.x,
                    y: oy + layer.offset_y + sprite.anchor_y - camera.y,
                    pass: pass_index,
                    blend: if layer.scale_den == 0 { Blend::Alpha } else { Blend::Scale { denominator: layer.scale_den } },
                    clip: None,
                });
            }
        }
        free_after(pass_index, &mut draw);
    }
    free_after(passes.len(), &mut draw);
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
