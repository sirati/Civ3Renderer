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
    ALL_VIEWS, ByteReader, FREE_CLIP, FREE_EXCEPT_VIEW, FREE_LOW, FREE_SCREEN, FREE_ZOOMED, FrameParams, FreeSprite, LAYER_HIDDEN, LayerDesc, OutputParams, QUAD_VERTICES, SpriteDesc, cell_origin,
    ALPHA_STEPS, QUANTIZED_BIAS, SCALE_INVERT, alpha_steps, expand, mix_offset, quantize, scale_offset, world_period, wrap_cell, wrap_distance,
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
    #[spirv(flat)] out_tint: &mut u32,
    #[spirv(flat)] out_base: &mut IVec2,
    #[spirv(flat)] out_lo: &mut IVec2,
    #[spirv(flat)] out_scaled: &mut u32,
) {
    let layer_index = params.layer_first + instance % params.layer_count;
    let local_cell = instance / params.layer_count;
    let layer = &layers[layer_index as usize];

    *out_local = Vec2::ZERO;
    *out_origin = IVec2::ZERO;
    *out_page = 0;
    *out_scale_den = layer.scale_den;
    *out_tint = 0;
    *out_base = IVec2::ZERO;
    *out_lo = IVec2::ZERO;
    *out_scaled = 0;
    let Some((sprite_index, x, y)) =
        resolve_sprite(params, layer, local_cell, tile_data, layer_lut, sprite_sets)
    else {
        *out_pos = CULLED;
        return;
    };
    let sprite = &sprites[sprite_index as usize];

    let (u, v) = quad_corner(vertex);
    // The sprite's rectangle at zoom 1, from the viewport's corner.
    let (x0, y0) = (x + sprite.anchor_x - params.camera_x, y + sprite.anchor_y - params.camera_y);
    let rect = (x0, y0, x0 + sprite.width as i32, y0 + sprite.height as i32);
    let Some((l, t, r, b)) = on_target(params, rect) else {
        *out_pos = CULLED;
        return;
    };
    let (sx, sy) = (l as f32 + u * (r - l) as f32, t as f32 + v * (b - t) as f32);
    *out_pos = Vec4::new(sx / params.viewport_w as f32 * 2.0 - 1.0, 1.0 - sy / params.viewport_h as f32 * 2.0, 0.0, 1.0);
    // Texel = flat atlas origin + interpolated offset inside the sprite. Small
    // interpolated values keep float precision exact for big atlases, and the
    // IVec2 varying also gives naga's SPIR-V frontend the ivec2 type it needs
    // for integer fetches from an arrayed image.
    *out_local = Vec2::new(sx - x0 as f32, sy - y0 as f32);
    *out_origin = IVec2::new(sprite.atlas_x as i32, sprite.atlas_y as i32);
    *out_page = sprite.page;
    *out_base = IVec2::new(x0, y0);
    *out_scaled = params.scaled;
}

/// Where a sprite whose (clipped) rectangle at zoom 1 is (left, top, right,
/// bottom; from the viewport's corner) lands in the viewport: there, or
/// with `FrameParams::scaled` the viewport pixels whose zoom-1 pixel
/// (`scaled(p, den, num)`) is in it; `None` if nowhere.
fn on_target(params: &FrameParams, (l, t, r, b): (i32, i32, i32, i32)) -> Option<(i32, i32, i32, i32)> {
    if r <= l || b <= t {
        return None;
    }
    if params.scaled == 0 {
        return Some((l, t, r, b));
    }
    let (n, d, hair) = (params.zoom_num, params.zoom_den, params.hairline != 0);
    let (l, r) = scaled_span(l, r, n, d, hair)?;
    let (t, b) = scaled_span(t, b, n, d, hair)?;
    Some((l, t, r, b))
}

/// The pixels `p` with `scaled(p, d, n)` in `[lo, hi)`: from `ceil(lo·n/d)`
/// to `ceil(hi·n/d)`; with `hair`, at least the pixel holding `lo·n/d`.
fn scaled_span(lo: i32, hi: i32, n: u32, d: u32, hair: bool) -> Option<(i32, i32)> {
    let (a, b) = (-scaled(-lo, n, d), -scaled(-hi, n, d));
    if b > a {
        Some((a, b))
    } else if hair {
        let a = scaled(lo, n, d);
        Some((a, a + 1))
    } else {
        None
    }
}

/// The atlas offset inside the sprite a fragment shows: the interpolated
/// `local`, or (`scaled` views) the zoom-1 pixel under the fragment less the
/// sprite's corner `base`, at least `lo` (where a clip or a hairline starts).
fn texel_offset(local: Vec2, base: IVec2, lo: IVec2, scaled_view: u32, frag: Vec4, params: &FrameParams) -> IVec2 {
    if scaled_view == 0 {
        return IVec2::new(local.x.floor() as i32, local.y.floor() as i32);
    }
    let (n, d) = (params.zoom_num, params.zoom_den);
    let sx = scaled(frag.x as i32 - params.target_x, d, n) - base.x;
    let sy = scaled(frag.y as i32 - params.target_y, d, n) - base.y;
    IVec2::new(if sx < lo.x { lo.x } else { sx }, if sy < lo.y { lo.y } else { sy })
}

/// Free sprites: instance `i` is `free[i]` (draw calls select a range with
/// their first instance).
#[spirv(vertex)]
pub fn free_vs(
    #[spirv(vertex_index)] vertex: u32,
    #[spirv(instance_index)] instance: u32,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] sprite_sets: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] sprites: &[SpriteDesc],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 7)] free: &[FreeSprite],
    #[spirv(position)] out_pos: &mut Vec4,
    out_local: &mut Vec2,
    #[spirv(flat)] out_origin: &mut IVec2,
    #[spirv(flat)] out_page: &mut u32,
    #[spirv(flat)] out_scale_den: &mut u32,
    #[spirv(flat)] out_tint: &mut u32,
    #[spirv(flat)] out_base: &mut IVec2,
    #[spirv(flat)] out_lo: &mut IVec2,
    #[spirv(flat)] out_scaled: &mut u32,
) {
    let f = &free[instance as usize];
    *out_tint = f.tint;
    *out_local = Vec2::ZERO;
    *out_origin = IVec2::ZERO;
    *out_page = 0;
    *out_scale_den = f.scale_den;
    *out_base = IVec2::ZERO;
    *out_lo = IVec2::ZERO;
    *out_scaled = 0;
    let frames = sprite_sets[f.set as usize];
    // Zoom art: zoomed sprites only where a view at their zoom is drawn at
    // it, low ones not where zoomed ones are shown.
    let zoomed = f.flags & FREE_ZOOMED != 0;
    let wrong_art = if zoomed {
        params.zoom_art != 2 || f.zoom != (params.zoom_num << 16 | params.zoom_den)
    } else {
        params.zoom_art != 0 && f.flags & FREE_LOW != 0
    };
    // In its view only, or (`FREE_EXCEPT_VIEW`) in every view showing the
    // world but that one.
    let except = f.flags & FREE_EXCEPT_VIEW != 0;
    let wrong_view = if except { f.view == params.view || params.world == 0 } else { (f.view != ALL_VIEWS && f.view != params.view) || (f.view == ALL_VIEWS && params.world == 0) };
    if f.set == 0 || frames == 0 || wrong_art || wrong_view {
        *out_pos = CULLED;
        return;
    }
    let ticks = if f.ticks_per_frame == 0 { 1 } else { f.ticks_per_frame };
    let frame = (params.frame / ticks + f.phase) % frames;
    let sprite = &sprites[sprite_sets[(f.set + 1 + frame) as usize] as usize];
    let (u, v) = quad_corner(vertex);
    // Where 1:1 positions start in the viewport: screen sprites are given in
    // target pixels from the view's screen origin, world sprites in world
    // pixels from the camera. Zoomed art is placed at its position scaled to
    // the view's zoom, plus its offset, and drawn unscaled.
    let (bx, by) = if f.flags & FREE_SCREEN != 0 { (params.origin_x, params.origin_y) } else { (params.camera_x, params.camera_y) };
    let (zn, zd) = if zoomed { (params.zoom_num, params.zoom_den) } else { (1, 1) };
    let (dx, dy) = if zoomed { (f.zoom_dx, f.zoom_dy) } else { (0, 0) };
    // In a wrapping world, the copy of a world sprite near the camera.
    let (fx, fy) = if f.flags & FREE_SCREEN != 0 {
        (f.x - bx, f.y - by)
    } else {
        let (px, py) = world_period(&params.grid);
        (wrap_distance(f.x - bx, px), wrap_distance(f.y - by, py))
    };
    let x0 = scaled(fx, zn, zd) + dx + sprite.anchor_x;
    let y0 = scaled(fy, zn, zd) + dy + sprite.anchor_y;
    let (mut l, mut t) = (x0, y0);
    let (mut r, mut b) = (x0 + sprite.width as i32, y0 + sprite.height as i32);
    // Clipping shrinks the quad (sprites are axis-aligned rectangles).
    if f.flags & FREE_CLIP != 0 {
        // (Target pixels from the screen origin, like screen sprites; for
        // zoomed art in its 1:1 coordinates, scaled.)
        let (ox, oy) = if zoomed { (bx, by) } else { (params.origin_x, params.origin_y) };
        l = max(l, scaled(f.clip[0] - ox, zn, zd));
        t = max(t, scaled(f.clip[1] - oy, zn, zd));
        r = min(r, scaled(f.clip[2] - ox, zn, zd));
        b = min(b, scaled(f.clip[3] - oy, zn, zd));
    }
    // (Zoomed art is at the target's resolution already.)
    let rect = if zoomed {
        if r <= l || b <= t { None } else { Some((l, t, r, b)) }
    } else {
        on_target(params, (l, t, r, b))
    };
    let Some((ql, qt, qr, qb)) = rect else {
        *out_pos = CULLED;
        return;
    };
    let (sx, sy) = (ql as f32 + u * (qr - ql) as f32, qt as f32 + v * (qb - qt) as f32);
    *out_pos = Vec4::new(sx / params.viewport_w as f32 * 2.0 - 1.0, 1.0 - sy / params.viewport_h as f32 * 2.0, 0.0, 1.0);
    *out_local = Vec2::new(sx - x0 as f32, sy - y0 as f32);
    *out_origin = IVec2::new(sprite.atlas_x as i32, sprite.atlas_y as i32);
    *out_page = sprite.page;
    *out_base = IVec2::new(x0, y0);
    *out_lo = IVec2::new(l - x0, t - y0);
    *out_scaled = if zoomed { 0 } else { params.scaled };
}

fn max(a: i32, b: i32) -> i32 {
    if a > b { a } else { b }
}

fn min(a: i32, b: i32) -> i32 {
    if a < b { a } else { b }
}

/// Draws a sprite texel. In a quantized colour model (`color_bits != 0`)
/// the target holds `q + QUANTIZED_BIAS` per channel and blending is
/// premultiplied: a scale layer's texel (`k` = its red channel) becomes
/// `scale_offset(k, den)` with alpha `1 − k/den`, so the blend computes
/// `S·k/den + offset`, which the 8-bit target rounds to the exact result.
#[spirv(fragment)]
pub fn tile_fs(
    local: Vec2,
    #[spirv(flat)] origin: IVec2,
    #[spirv(flat)] page: u32,
    #[spirv(flat)] scale_den: u32,
    #[spirv(flat)] _tint: u32,
    #[spirv(flat)] base: IVec2,
    #[spirv(flat)] lo: IVec2,
    #[spirv(flat)] scaled_view: u32,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(descriptor_set = 0, binding = 6)] atlas: &Image!(2D, type = f32, sampled, arrayed),
    output: &mut Vec4,
) {
    let xy = origin + texel_offset(local, base, lo, scaled_view, frag, params);
    let color: Vec4 = atlas.fetch(xy.extend(page as i32));
    if color.w == 0.0 {
        spirv_std::arch::kill();
    }
    let bits = params.color_bits;
    if scale_den == SCALE_INVERT {
        // The pipeline subtracts what is there from this: the highest
        // stored value (direct: 1), so the result is the inverse.
        let k = if bits != 0 { (((1u32 << bits) - 1) + 2 * QUANTIZED_BIAS) as f32 / 255.0 } else { 1.0 };
        *output = Vec4::new(k, k, k, 0.0);
    } else if scale_den != 0 {
        let k = to_u8(color.x);
        if k >= scale_den {
            spirv_std::arch::kill();
        }
        let a = 1.0 - k as f32 / scale_den as f32;
        *output = if bits != 0 {
            let o = scale_offset(k, scale_den) / 255.0;
            Vec4::new(o, o, o, a)
        } else {
            Vec4::new(0.0, 0.0, 0.0, a)
        };
    } else if bits != 0 {
        // Translucent texels: in whole alpha steps, rounded down.
        let k = alpha_steps(to_u8(color.w));
        if k == 0 {
            spirv_std::arch::kill();
        }
        let a = k as f32 / ALPHA_STEPS as f32;
        let o = if k < ALPHA_STEPS { mix_offset() / 255.0 } else { 0.0 };
        *output = Vec4::new(stored(color.x, bits) * a + o, stored(color.y, bits) * a + o, stored(color.z, bits) * a + o, a);
    } else {
        *output = color;
    }
}

/// One colour channel (0 red, 1 green, 2 blue) of a ClearType-like sprite
/// (`SCALE_SUBPIXEL`): its texels hold that channel's coverage in the same
/// channel, `tint` is the colour. Drawn three times, the pipeline writing
/// one channel each time, so each channel blends with its own alpha (in
/// whole alpha steps, rounded down, in a quantized model).
fn subpixel(offset: IVec2, origin: IVec2, page: u32, tint: u32, params: &FrameParams, atlas: &Image!(2D, type = f32, sampled, arrayed), channel: u32) -> Vec4 {
    let xy = origin + offset;
    let texel: Vec4 = atlas.fetch(xy.extend(page as i32));
    let coverage = if channel == 0 { texel.x } else if channel == 1 { texel.y } else { texel.z };
    let c8 = (tint >> (16 - 8 * channel)) & 0xFF;
    let bits = params.color_bits;
    if bits != 0 {
        let k = alpha_steps(to_u8(coverage));
        if k == 0 {
            spirv_std::arch::kill();
        }
        let a = k as f32 / ALPHA_STEPS as f32;
        let o = if k < ALPHA_STEPS { mix_offset() / 255.0 } else { 0.0 };
        let v = (quantize(c8, bits) + QUANTIZED_BIAS) as f32 / 255.0 * a + o;
        Vec4::new(v, v, v, a)
    } else {
        if coverage == 0.0 {
            spirv_std::arch::kill();
        }
        let c = c8 as f32 / 255.0;
        Vec4::new(c, c, c, coverage)
    }
}

#[spirv(fragment)]
pub fn subpixel_r_fs(
    local: Vec2,
    #[spirv(flat)] origin: IVec2,
    #[spirv(flat)] page: u32,
    #[spirv(flat)] _scale_den: u32,
    #[spirv(flat)] tint: u32,
    #[spirv(flat)] base: IVec2,
    #[spirv(flat)] lo: IVec2,
    #[spirv(flat)] scaled_view: u32,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(descriptor_set = 0, binding = 6)] atlas: &Image!(2D, type = f32, sampled, arrayed),
    output: &mut Vec4,
) {
    *output = subpixel(texel_offset(local, base, lo, scaled_view, frag, params), origin, page, tint, params, atlas, 0);
}

#[spirv(fragment)]
pub fn subpixel_g_fs(
    local: Vec2,
    #[spirv(flat)] origin: IVec2,
    #[spirv(flat)] page: u32,
    #[spirv(flat)] _scale_den: u32,
    #[spirv(flat)] tint: u32,
    #[spirv(flat)] base: IVec2,
    #[spirv(flat)] lo: IVec2,
    #[spirv(flat)] scaled_view: u32,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(descriptor_set = 0, binding = 6)] atlas: &Image!(2D, type = f32, sampled, arrayed),
    output: &mut Vec4,
) {
    *output = subpixel(texel_offset(local, base, lo, scaled_view, frag, params), origin, page, tint, params, atlas, 1);
}

#[spirv(fragment)]
pub fn subpixel_b_fs(
    local: Vec2,
    #[spirv(flat)] origin: IVec2,
    #[spirv(flat)] page: u32,
    #[spirv(flat)] _scale_den: u32,
    #[spirv(flat)] tint: u32,
    #[spirv(flat)] base: IVec2,
    #[spirv(flat)] lo: IVec2,
    #[spirv(flat)] scaled_view: u32,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(descriptor_set = 0, binding = 6)] atlas: &Image!(2D, type = f32, sampled, arrayed),
    output: &mut Vec4,
) {
    *output = subpixel(texel_offset(local, base, lo, scaled_view, frag, params), origin, page, tint, params, atlas, 2);
}

/// A triangle covering the viewport, for the full-screen pass.
#[spirv(vertex)]
pub fn fullscreen_vs(#[spirv(vertex_index)] vertex: u32, #[spirv(position)] out_pos: &mut Vec4) {
    let x = if vertex == 1 { 3.0 } else { -1.0 };
    let y = if vertex == 2 { 3.0 } else { -1.0 };
    *out_pos = Vec4::new(x, y, 0.0, 1.0);
}

/// Copies a rectangle of `source` into the target, scaled by nearest
/// texel, and turns stored quantized values into 8-bit ones (`bits` 0: as
/// they are).
#[spirv(fragment)]
pub fn output_fs(
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 1)] params: &OutputParams,
    #[spirv(descriptor_set = 0, binding = 0)] source: &Image!(2D, type = f32, sampled),
    output: &mut Vec4,
) {
    let sx = params.src_x + (offset(frag.x, params.dst_x) * params.zoom_den / params.zoom_num) as i32;
    let sy = params.src_y + (offset(frag.y, params.dst_y) * params.zoom_den / params.zoom_num) as i32;
    let v: Vec4 = source.fetch(IVec2::new(sx, sy));
    let bits = params.bits;
    *output = if bits == 0 { v } else { Vec4::new(to_unorm(v.x, bits), to_unorm(v.y, bits), to_unorm(v.z, bits), v.w) };
}

/// `v · num / den`, rounded down.
fn scaled(v: i32, num: u32, den: u32) -> i32 {
    let p = v * num as i32;
    let d = den as i32;
    if p >= 0 { p / d } else { -((-p + d - 1) / d) }
}

/// How far pixel coordinate `p` lies past `o` (0 before it).
fn offset(p: f32, o: i32) -> u32 {
    let r = p as i32 - o;
    if r < 0 { 0 } else { r as u32 }
}

/// An atlas channel as the stored quantized value (normalized).
fn stored(c: f32, bits: u32) -> f32 {
    (quantize(to_u8(c), bits) + QUANTIZED_BIAS) as f32 / 255.0
}

fn to_unorm(stored: f32, bits: u32) -> f32 {
    let s = to_u8(stored);
    let q = if s > QUANTIZED_BIAS { s - QUANTIZED_BIAS } else { 0 };
    expand(q, bits) as f32 / 255.0
}

fn to_u8(v: f32) -> u32 {
    (v * 255.0 + 0.5) as u32
}
