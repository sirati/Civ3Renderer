//! A CPU implementation of exactly what the shaders draw.
//!
//! It is the oracle for the GPU path (tests compare both byte for byte) and
//! a fallback for tools that have no GPU.

use tile_render_shared::{ByteReader, cell_origin, wrap_cell};

use crate::atlas::{Atlas, RgbaImage};
use crate::camera::Camera;
use crate::world::TileWorld;

/// Renders like `TileRenderer::draw` onto a `clear`ed image. Only zoom 1/1 is
/// supported (that is what pixel-exact comparisons use).
pub fn render(world: &TileWorld, atlas: &Atlas, camera: &Camera, frame: u32, clear: [u8; 4]) -> RgbaImage {
    assert_eq!(camera.zoom, (1, 1), "the reference renderer only implements zoom 1");
    let (w, h) = camera.viewport;
    let mut out = RgbaImage::from_fn(w, h, |_, _| clear);
    let packed = world.pack();
    let data = ByteReader::new(&packed.data);
    for (first, count) in world.passes() {
    let params = camera.frame_params(&world.grid, frame, (first, count));
    for local in 0..params.cols * params.rows {
        let col = params.first_col + (local % params.cols) as i32;
        let row = params.first_row + (local / params.cols) as i32;
        let Some((c, r)) = wrap_cell(&params.grid, col, row) else { continue };
        let (ox, oy) = cell_origin(&params.grid, col, row);
        for layer in &packed.layers[first as usize..(first + count) as usize] {
            if layer.flags & tile_render_shared::LAYER_HIDDEN != 0 {
                continue;
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
            let sprite = atlas.sprites[packed.sets[set + 1 + f as usize] as usize];
            let page = &atlas.pages[sprite.page as usize];
            let left = ox + layer.offset_x + sprite.anchor_x - camera.x;
            let top = oy + layer.offset_y + sprite.anchor_y - camera.y;
            for sy in 0..sprite.height as i32 {
                for sx in 0..sprite.width as i32 {
                    let (px, py) = (left + sx, top + sy);
                    if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
                        continue;
                    }
                    let src = page.get(sprite.atlas_x + sx as u32, sprite.atlas_y + sy as u32);
                    if src[3] == 0 {
                        continue;
                    }
                    let dst = out.get(px as u32, py as u32);
                    out.put(px as u32, py as u32, blend(src, dst));
                }
            }
        }
    }
    }
    out
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
