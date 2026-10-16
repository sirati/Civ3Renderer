//! A CPU implementation of exactly what the shaders draw.
//!
//! It is the oracle for the GPU path (tests compare both byte for byte) and
//! a fallback for tools that have no GPU.

use tile_render_shared::{ByteReader, cell_origin, expand, quantize, wrap_cell};

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
    // Raw channel values, like the GPU's float target.
    let q = |c: u8| quantize(c.into(), bits) as f32;
    let mut out: Vec<[f32; 4]> = vec![[q(clear[0]), q(clear[1]), q(clear[2]), clear[3] as f32 / 255.0]; (w * h) as usize];
    let mut pass = 0;
    let floor = |out: &mut Vec<[f32; 4]>| {
        for p in out.iter_mut() {
            for c in &mut p[..3] {
                *c = c.floor();
            }
        }
    };
    visit(world, atlas, camera, frame, |d| {
        if d.pass != pass {
            floor(&mut out);
            pass = d.pass;
        }
        each_texel(atlas, &d, (w, h), |px, py, src| {
            let dst = &mut out[(py * w + px) as usize];
            let (rgb, a) = match d.blend {
                Blend::Scale { denominator } => ([0.0; 3], 1.0 - src[0] as f32 / denominator as f32),
                Blend::Alpha => ([q(src[0]), q(src[1]), q(src[2])], src[3] as f32 / 255.0),
            };
            for i in 0..3 {
                dst[i] = rgb[i] * a + dst[i] * (1.0 - a);
            }
            dst[3] = a + dst[3] * (1.0 - a);
        });
    });
    floor(&mut out);
    RgbaImage::from_fn(w, h, |x, y| {
        let p = out[(y * w + x) as usize];
        let c = |v: f32| expand(v as u32, bits) as u8;
        [c(p[0]), c(p[1]), c(p[2]), (p[3] * 255.0).round() as u8]
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
    for (pass_index, pass) in world.passes().iter().enumerate() {
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
                });
            }
        }
    }
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
