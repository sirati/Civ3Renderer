//! Tile renderer shaders.
//!
//! One instanced draw per render pass covers the whole visible window:
//! instance `i` is layer `layer_first + i % layer_count` of visible cell
//! `i / layer_count`, so all of the pass's layers of a cell are drawn before
//! the next cell, in row-major order. That is the order an isometric renderer
//! needs for tall sprites (hills, buildings, units) to overlap the rows
//! behind them.
//!
//! The vertex shader pulls everything from storage buffers (see
//! `tile_render_shared` for the data model) and emits a quad per instance, or
//! a degenerate one when the cell has nothing to draw on that layer. The
//! fragment shader fetches atlas texels 1:1, so sprites stay pixel exact.
#![cfg_attr(target_arch = "spirv", no_std)]

use spirv_std::glam::{IVec2, Vec2, Vec4};
use spirv_std::num_traits::Float;
use spirv_std::{Image, spirv};
use tile_render_shared::{
    ByteReader, FrameParams, LAYER_HIDDEN, LayerDesc, QUAD_VERTICES, SpriteDesc, cell_origin,
    expand, quantize, wrap_cell,
};

/// Outside every clip volume: all six vertices of a culled quad land here,
/// so the triangles have zero area and are dropped before rasterisation.
const CULLED: Vec4 = Vec4::new(2.0, 2.0, 2.0, 1.0);

/// Corner (0 or 1 per axis) of each of the quad's six vertices.
fn quad_corner(vertex: u32) -> (f32, f32) {
    match vertex % QUAD_VERTICES {
        0 => (0.0, 0.0),
        1 | 4 => (1.0, 0.0),
        2 | 3 => (0.0, 1.0),
        _ => (1.0, 1.0),
    }
}

/// Resolves the sprite drawn by `layer` in the cell at the given visible
/// window position, together with the cell's world-pixel origin.
fn resolve_sprite(
    params: &FrameParams,
    layer: &LayerDesc,
    local_cell: u32,
    tile_data: &[u32],
    layer_lut: &[u32],
    sprite_sets: &[u32],
) -> Option<(u32, i32, i32)> {
    if layer.flags & LAYER_HIDDEN != 0 {
        return None;
    }
    let col = params.first_col + (local_cell % params.cols) as i32;
    let row = params.first_row + (local_cell / params.cols) as i32;
    let (c, r) = wrap_cell(&params.grid, col, row)?;
    // Position from the unwrapped coordinates, so a wrapped-around column
    // is drawn next to the edge it continues, not on top of its original.
    let (origin_x, origin_y) = cell_origin(&params.grid, col, row);
    if params.cull != 0 {
        let (ax, ay) = (origin_x + layer.cull_offset_x, origin_y + layer.cull_offset_y);
        if ax < params.cull_x0 || ax >= params.cull_x1 || ay < params.cull_y0 || ay >= params.cull_y1 {
            return None;
        }
    }

    let record = layer.value_bytes + layer.phase_bytes;
    let byte = layer.data_byte_offset + (r * params.grid.cols + c) * record;
    let data = ByteReader::new(tile_data);
    let value = data.uint(byte, layer.value_bytes);
    if value >= layer.lut_len {
        return None;
    }
    let phase = if layer.phase_bytes != 0 { data.u8(byte + layer.value_bytes) } else { 0 };

    let set = layer_lut[(layer.lut_offset + value) as usize];
    let frames = sprite_sets[set as usize];
    if frames == 0 {
        return None;
    }
    let frame = (params.frame / layer.ticks_per_frame + phase) % frames;
    let sprite = sprite_sets[(set + 1 + frame) as usize];
    Some((sprite, origin_x + layer.offset_x, origin_y + layer.offset_y))
}

#[spirv(vertex)]
pub fn tile_vs(
    #[spirv(vertex_index)] vertex: u32,
    #[spirv(instance_index)] instance: u32,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] layers: &[LayerDesc],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] tile_data: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] layer_lut: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] sprite_sets: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] sprites: &[SpriteDesc],
    #[spirv(position)] out_pos: &mut Vec4,
    out_local: &mut Vec2,
    #[spirv(flat)] out_origin: &mut IVec2,
    #[spirv(flat)] out_page: &mut u32,
    #[spirv(flat)] out_scale_den: &mut u32,
) {
    let layer_index = params.layer_first + instance % params.layer_count;
    let local_cell = instance / params.layer_count;
    let layer = &layers[layer_index as usize];

    *out_local = Vec2::ZERO;
    *out_origin = IVec2::ZERO;
    *out_page = 0;
    *out_scale_den = layer.scale_den;
    let Some((sprite_index, x, y)) =
        resolve_sprite(params, layer, local_cell, tile_data, layer_lut, sprite_sets)
    else {
        *out_pos = CULLED;
        return;
    };
    let sprite = &sprites[sprite_index as usize];

    let (u, v) = quad_corner(vertex);
    let world_x = (x + sprite.anchor_x - params.camera_x) as f32 + u * sprite.width as f32;
    let world_y = (y + sprite.anchor_y - params.camera_y) as f32 + v * sprite.height as f32;
    let zoom = params.zoom_num as f32 / params.zoom_den as f32;
    let ndc_x = world_x * zoom / params.viewport_w as f32 * 2.0 - 1.0;
    let ndc_y = 1.0 - world_y * zoom / params.viewport_h as f32 * 2.0;

    *out_pos = Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
    // Texel = flat atlas origin + interpolated offset inside the sprite. Small
    // interpolated values keep float precision exact for big atlases, and the
    // IVec2 varying also gives naga's SPIR-V frontend the ivec2 type it needs
    // for integer fetches from an arrayed image.
    *out_local = Vec2::new(u * sprite.width as f32, v * sprite.height as f32);
    *out_origin = IVec2::new(sprite.atlas_x as i32, sprite.atlas_y as i32);
    *out_page = sprite.page;
}

/// Draws a sprite texel. In a quantized colour space (`color_bits != 0`)
/// the target holds raw channel values (0..2^bits - 1), and a scale layer's
/// texel becomes black with alpha `1 - k / den`, so ordinary alpha blending
/// computes `dst · k / den` exactly; `quantize_fs` rounds it down later.
#[spirv(fragment)]
pub fn tile_fs(
    local: Vec2,
    #[spirv(flat)] origin: IVec2,
    #[spirv(flat)] page: u32,
    #[spirv(flat)] scale_den: u32,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(descriptor_set = 0, binding = 6)] atlas: &Image!(2D, type = f32, sampled, arrayed),
    output: &mut Vec4,
) {
    let xy = origin + IVec2::new(local.x.floor() as i32, local.y.floor() as i32);
    let color: Vec4 = atlas.fetch(xy.extend(page as i32));
    if color.w == 0.0 {
        spirv_std::arch::kill();
    }
    let bits = params.color_bits;
    *output = if scale_den != 0 {
        Vec4::new(0.0, 0.0, 0.0, 1.0 - to_u8(color.x) as f32 / scale_den as f32)
    } else if bits != 0 {
        Vec4::new(
            quantize(to_u8(color.x), bits) as f32,
            quantize(to_u8(color.y), bits) as f32,
            quantize(to_u8(color.z), bits) as f32,
            color.w,
        )
    } else {
        color
    };
}

/// A triangle covering the viewport, for the full-screen passes.
#[spirv(vertex)]
pub fn fullscreen_vs(#[spirv(vertex_index)] vertex: u32, #[spirv(position)] out_pos: &mut Vec4) {
    let x = if vertex == 1 { 3.0 } else { -1.0 };
    let y = if vertex == 2 { 3.0 } else { -1.0 };
    *out_pos = Vec4::new(x, y, 0.0, 1.0);
}

/// Rounds the quantized channel values in `source` down to integers, as the
/// game does after every blit (end of each pass).
#[spirv(fragment)]
pub fn quantize_fs(
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(descriptor_set = 0, binding = 0)] source: &Image!(2D, type = f32, sampled),
    output: &mut Vec4,
) {
    let v: Vec4 = source.fetch(IVec2::new(frag.x as i32, frag.y as i32));
    *output = Vec4::new(v.x.floor(), v.y.floor(), v.z.floor(), v.w);
}

/// Quantized channel values to the 8-bit output.
#[spirv(fragment)]
pub fn output_fs(
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 1)] bits: &u32,
    #[spirv(descriptor_set = 0, binding = 0)] source: &Image!(2D, type = f32, sampled),
    output: &mut Vec4,
) {
    let v: Vec4 = source.fetch(IVec2::new(frag.x as i32, frag.y as i32));
    *output = Vec4::new(to_unorm(v.x, *bits), to_unorm(v.y, *bits), to_unorm(v.z, *bits), v.w);
}

fn to_unorm(q: f32, bits: u32) -> f32 {
    expand(q.floor() as u32, bits) as f32 / 255.0
}

fn to_u8(v: f32) -> u32 {
    (v * 255.0 + 0.5) as u32
}
