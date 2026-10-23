//! Tile renderer shaders.
//!
//! One instanced draw per render pass covers the whole visible window:
//! instance `i` is layer `layer_first + i % layer_count` of visible cell
//! `i / layer_count`, so all of the pass's layers of a cell are drawn before
//! the next cell, in row-major order. That is the order an isometric renderer
//! needs for tall sprites (hills, buildings, units) to overlap the rows
//! behind them. A pass may sweep each row from its last column back
//! (`FrameParams::descending`), as a game drawing its units right to left
//! does.
//!
//! The vertex shader pulls everything from storage buffers (see
//! `tile_render_shared` for the data model) and emits a quad per instance, or
//! a degenerate one when the cell has nothing to draw on that layer. The
//! fragment shader fetches atlas texels 1:1, so sprites stay pixel exact. In
//! a view showing the world `n` times smaller (`FrameParams::downscale`) it
//! fetches every `n`-th texel of each sprite's sample box from the same
//! atlas (`tile_render_shared::placed`): the art is held once, at full size.
#![cfg_attr(target_arch = "spirv", no_std)]

pub mod text;
pub mod glyph_cache;
mod lens_math;

use spirv_std::glam::{IVec2, UVec2, UVec3, UVec4, Vec2, Vec4};
use spirv_std::num_traits::Float;
use spirv_std::{Image, spirv};
use tile_render_shared::{
    ALL_VIEWS, ByteReader, cell_offset, frame_of, record_at, window_col, FREE_CLIP, FREE_EXCEPT_VIEW, FREE_LOW, FREE_SCREEN, FREE_ZOOMED, FrameParams, FreeSprite, LAYER_HIDDEN, LayerDesc, OutputParams, QUAD_VERTICES, SpriteDesc, cell_origin,
    ALPHA_STEPS, QUANTIZED_BIAS, SCALE_INVERT, SCALE_SUBPIXEL, alpha_steps, drawn_part, drawn_sprite, expand, indexed_color, layer_value, mixed_source, placed, quantize, scale_offset, shrunk, world_period, wrap_cell, wrap_distance,
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

/// Corner `vertex` (0..4: left top, right top, left bottom, right bottom)
/// of a cell's quad (see `tile_render_shared::CELL_INDICES`).
fn cell_corner(vertex: u32) -> (f32, f32) {
    ((vertex & 1) as f32, ((vertex >> 1) & 1) as f32)
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
) -> Option<(u32, u32, i32, i32)> {
    if layer.flags & LAYER_HIDDEN != 0 || layer.category & params.hidden_categories != 0 {
        return None;
    }
    let col = window_col(params, local_cell);
    let row = params.first_row + (local_cell / params.cols) as i32;
    let (c, r) = wrap_cell(&params.grid, col, row)?;
    // Position from the unwrapped coordinates, so a wrapped-around column
    // is drawn next to the edge it continues, not on top of its original.
    // (`grid` is the grid at the view's scale, offsets are shrunk to it.)
    let (origin_x, origin_y) = cell_origin(&params.grid, col, row);
    let n = params.downscale;
    // (The cell's anchor, not moved by a per-cell offset: a game that
    // clips by tile range draws what is on its tiles wherever it is.)
    if params.cull != 0 {
        let (ax, ay) = (origin_x + shrunk(layer.cull_offset_x, n), origin_y + shrunk(layer.cull_offset_y, n));
        if ax < params.cull_x0 || ax >= params.cull_x1 || ay < params.cull_y0 || ay >= params.cull_y1 {
            return None;
        }
    }

    let byte = record_at(layer, params.grid.cols, c, r);
    let data = ByteReader::new(tile_data);
    let (value, palette) = layer_value(layer, data.uint(byte, layer.value_bytes));
    // (Most cells of most layers: nothing, without reading the tables.)
    if value >= layer.lut_len || (value == 0 && layer.flags & tile_render_shared::LAYER_ZERO_EMPTY != 0) {
        return None;
    }
    let phase = if layer.phase_bytes != 0 { data.u8(byte + layer.value_bytes) } else { 0 };

    let set = layer_lut[(layer.lut_offset + value) as usize];
    let frames = sprite_sets[set as usize];
    if frames == 0 {
        return None;
    }
    let frame = frame_of(params.frame, layer.ticks_per_frame, phase, frames);
    let sprite = sprite_sets[(set + 1 + frame) as usize];
    // The cell's own offset on top of the layer's.
    let (dx, dy) = cell_offset(&data, layer, byte, n);
    Some((sprite, palette, origin_x + shrunk(layer.offset_x, n) + dx, origin_y + shrunk(layer.offset_y, n) + dy))
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
    #[spirv(flat)] out_step: &mut u32,
    #[spirv(flat)] out_palette: &mut UVec3,
    #[spirv(flat)] out_background: &mut UVec4,
    #[spirv(flat)] out_primitive: &mut UVec2,
) {
    let layer_index = params.layer_first + instance % params.layer_count;
    let local_cell = instance / params.layer_count;
    // (From the view's layer table: its binding's data slots.)
    let layer = &layers[(params.layer_table + layer_index) as usize];

    *out_local = Vec2::ZERO;
    *out_origin = IVec2::ZERO;
    *out_page = 0;
    *out_scale_den = layer.scale_den;
    *out_tint = 0;
    *out_base = IVec2::ZERO;
    *out_lo = IVec2::ZERO;
    *out_scaled = 0;
    *out_step = 1;
    *out_palette = UVec3::ZERO;
    *out_background = UVec4::ZERO;
    *out_primitive = UVec2::ZERO;
    let Some((sprite_index, palette, x, y)) =
        resolve_sprite(params, layer, local_cell, tile_data, layer_lut, sprite_sets)
    else {
        *out_pos = CULLED;
        return;
    };
    let n = params.downscale;
    let sprite = &sprites[drawn_sprite(sprites, sprite_index, n) as usize];
    let p = placed(sprite, n);

    // (Cells are drawn indexed, `CELL_INDICES`: `vertex` is a corner.)
    let (u, v) = cell_corner(vertex);
    // The sprite's rectangle at zoom 1, from the viewport's corner (at the
    // view's scale: the part of its box it covers).
    let x0 = x + p.corner_x + p.inset_x as i32 - params.camera_x;
    let y0 = y + p.corner_y + p.inset_y as i32 - params.camera_y;
    // Without the sprite's blank margins (see `trimmed`).
    let (dx0, dy0, dx1, dy1) = trimmed(sprite, &p, params);
    let rect = (x0 + dx0 as i32, y0 + dy0 as i32, x0 + dx1 as i32, y0 + dy1 as i32);
    let Some((l, t, r, b)) = on_target(params, rect) else {
        *out_pos = CULLED;
        return;
    };
    let (sx, sy) = if params.corner_cuts != 0 {
        // (The cuts are in texels: where pixels are texels, 1:1.)
        let cuts = if params.scaled == 0 && p.step == 1 { sprite.cuts } else { 0 };
        octagon_corner(vertex, (l, t, r, b), (x0, y0), (sprite.width, sprite.height), cuts)
    } else {
        (l as f32 + u * (r - l) as f32, t as f32 + v * (b - t) as f32)
    };
    *out_pos = Vec4::new(sx / params.viewport_w as f32 * 2.0 - 1.0, 1.0 - sy / params.viewport_h as f32 * 2.0, 0.0, 1.0);
    // Texel = flat atlas origin + interpolated offset inside the sprite. Small
    // interpolated values keep float precision exact for big atlases, and the
    // IVec2 varying also gives naga's SPIR-V frontend the ivec2 type it needs
    // for integer fetches from an arrayed image.
    *out_local = Vec2::new(sx - x0 as f32, sy - y0 as f32);
    *out_origin = IVec2::new((sprite.atlas_x + p.texel_x) as i32, (sprite.atlas_y + p.texel_y) as i32);
    *out_page = sprite.page;
    *out_base = IVec2::new(x0, y0);
    *out_scaled = params.scaled;
    *out_step = p.step;
    *out_palette = UVec3::new(sprite.palette, sprite.remap, palette);
}

/// The part of the rectangle `p` places `sprite` in that is drawn: all of
/// it but the sprite's blank margins (`tile_render_shared::drawn_part`),
/// whose fragments would all be discarded. Not in a hairline view, whose
/// sprites keep a pixel of their rectangle however small it is shown.
fn trimmed(sprite: &SpriteDesc, p: &tile_render_shared::Placed, params: &FrameParams) -> (u32, u32, u32, u32) {
    if params.hairline != 0 { (0, 0, p.width, p.height) } else { drawn_part(sprite, p) }
}

/// Corner `corner` (0..8, clockwise from the top edge's left end; see
/// `tile_render_shared::CELL_INDICES`) of the octagon a sprite of `size`
/// texels whose texel (0, 0) is at pixel `origin` covers in rectangle (l,
/// t, r, b): the rectangle less its corners `cuts` leaves blank
/// (`SpriteDesc::cuts`, which keeps every texel centre that is not blank
/// strictly inside; `cuts` 0: the rectangle).
fn octagon_corner(corner: u32, (l, t, r, b): (i32, i32, i32, i32), (x0, y0): (i32, i32), (w, h): (u32, u32), cuts: u32) -> (f32, f32) {
    let c = |k: u32| ((cuts >> (8 * k)) & 0xFF) as f32;
    let (w, h) = (w as f32, h as f32);
    // The rectangle in the sprite's texels.
    let (ul, ur, vt, vb) = ((l - x0) as f32, (r - x0) as f32, (t - y0) as f32, (b - y0) as f32);
    let cu = |u: f32| if u < ul { ul } else if u > ur { ur } else { u };
    let cv = |v: f32| if v < vt { vt } else if v > vb { vb } else { v };
    if cuts == 0 {
        // (The rectangle, as it is: also where it is not in texels.)
        let (x, y) = match corner {
            0 | 7 => (l, t),
            1 | 2 => (r, t),
            3 | 4 => (r, b),
            _ => (l, b),
        };
        return (x as f32, y as f32);
    }
    let (u, v) = match corner {
        0 => (cu(c(0) - 2.0 * vt), vt),
        1 => (cu(w - c(1) + 2.0 * vt), vt),
        2 => (ur, cv((c(1) - (w - ur)) * 0.5)),
        3 => (ur, cv(h - (c(2) - (w - ur)) * 0.5)),
        4 => (cu(w - c(2) + 2.0 * (h - vb)), vb),
        5 => (cu(c(3) - 2.0 * (h - vb)), vb),
        6 => (ul, cv(h - (c(3) - ul) * 0.5)),
        _ => (ul, cv((c(0) - ul) * 0.5)),
    };
    (x0 as f32 + u, y0 as f32 + v)
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
    // Compute ceil directly. The negated-floor form collapses both
    // endpoints on the DX12 compiler used by Microsoft Basic Render Driver,
    // leaving only the hairline fallback pixel instead of the whole quad.
    let ceil = |v: i32| {
        let p = v * n as i32;
        let d = d as i32;
        if p >= 0 { (p + d - 1) / d } else { -((-p) / d) }
    };
    let (a, b) = (ceil(lo), ceil(hi));
    if b > a {
        Some((a, b))
    } else if hair {
        let a = scaled(lo, n, d);
        Some((a, a + 1))
    } else {
        None
    }
}

/// The atlas offset from the sprite's first drawn texel a fragment shows:
/// `step` times the pixel of the sprite's rectangle under it, which is the
/// interpolated `local`, or (`scaled` views) the zoom-1 pixel under the
/// fragment less the rectangle's corner `base`, at least `lo` (where a clip
/// or a hairline starts).
fn texel_offset(local: Vec2, base: IVec2, lo: IVec2, scaled_view: u32, step: u32, frag: Vec4, params: &FrameParams) -> IVec2 {
    let s = step as i32;
    if scaled_view == 0 {
        // (Pixel centres: `local` is a whole number and a half.)
        return IVec2::new(pixel(local.x) as i32 * s, pixel(local.y) as i32 * s);
    }
    let (n, d) = (params.zoom_num, params.zoom_den);
    let sx = scaled(pixel(frag.x) as i32 - params.target_x, d, n) - base.x;
    let sy = scaled(pixel(frag.y) as i32 - params.target_y, d, n) - base.y;
    IVec2::new(if sx < lo.x { lo.x } else { sx } * s, if sy < lo.y { lo.y } else { sy } * s)
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
    #[spirv(flat)] out_step: &mut u32,
    #[spirv(flat)] out_palette: &mut UVec3,
    #[spirv(flat)] out_background: &mut UVec4,
    #[spirv(flat)] out_primitive: &mut UVec2,
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
    *out_step = 1;
    *out_palette = UVec3::ZERO;
    *out_background = UVec4::ZERO;
    *out_primitive = UVec2::ZERO;
    let solid = f.primitive.kind == tile_render_shared::PRIMITIVE_SOLID;
    let frames = if solid { 1 } else { sprite_sets[f.set as usize] };
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
    if (!solid && f.set == 0) || frames == 0 || wrong_art || wrong_view || f.category & params.hidden_categories != 0 {
        *out_pos = CULLED;
        return;
    }
    let frame = tile_render_shared::free_frame_of(params.frame, params.host_tick_ms, f.ticks_per_frame, f.phase, frames);
    let screen = f.flags & FREE_SCREEN != 0;
    // A view showing the world smaller shows world sprites smaller, at
    // their positions shrunk; screen sprites as they are.
    let n = if screen { 1 } else { params.downscale };
    // Procedural commands never dereference an atlas sprite or set.
    let solid_desc = SpriteDesc { scaling: tile_render_shared::KEEP_SIZE, width: f.primitive.width, height: f.primitive.height,
        box_w: f.primitive.width, box_h: f.primitive.height, ..SpriteDesc::default() };
    let sprite = if solid { solid_desc } else { sprites[drawn_sprite(sprites, sprite_sets[(f.set + 1 + frame) as usize], n) as usize] };
    let p = placed(&sprite, n);
    let (u, v) = quad_corner(vertex);
    // Where 1:1 positions start in the viewport: screen sprites are given in
    // target pixels from the view's screen origin, world sprites in world
    // pixels from the camera. Zoomed art is placed at its position scaled to
    // the view's zoom, plus its offset, and drawn unscaled.
    let (bx, by) = if screen { (params.origin_x, params.origin_y) } else { (params.camera_x, params.camera_y) };
    let (zn, zd) = if zoomed { (params.zoom_num, params.zoom_den) } else { (1, 1) };
    let (dx, dy) = if zoomed { (f.zoom_dx, f.zoom_dy) } else { (0, 0) };
    // In a wrapping world, the copy of a world sprite near the camera.
    let (fx, fy) = if screen {
        (f.x - bx, f.y - by)
    } else {
        let (px, py) = world_period(&params.grid);
        (wrap_distance(shrunk(f.x, n) - bx, px), wrap_distance(shrunk(f.y, n) - by, py))
    };
    let x0 = scaled(fx, zn, zd) + dx + p.corner_x + p.inset_x as i32;
    let y0 = scaled(fy, zn, zd) + dy + p.corner_y + p.inset_y as i32;
    // Without the sprite's blank margins (ClearType-like sprites draw
    // where their coverage is, whatever their alpha: all of them).
    let (dx0, dy0, dx1, dy1) = if solid || f.scale_den == SCALE_SUBPIXEL { (0, 0, p.width, p.height) } else { trimmed(&sprite, &p, params) };
    let (mut l, mut t) = (x0 + dx0 as i32, y0 + dy0 as i32);
    let (mut r, mut b) = (x0 + dx1 as i32, y0 + dy1 as i32);
    // Clipping shrinks the quad (sprites are axis-aligned rectangles).
    if f.flags & FREE_CLIP != 0 {
        // (Target pixels from the screen origin, like screen sprites; for
        // zoomed art in its 1:1 coordinates, scaled; world ones shrunk.)
        let (ox, oy, cn) = if zoomed { (bx, by, n) } else { (params.origin_x, params.origin_y, 1) };
        l = max(l, scaled(shrunk(f.clip[0], cn) - ox, zn, zd));
        t = max(t, scaled(shrunk(f.clip[1], cn) - oy, zn, zd));
        r = min(r, scaled(shrunk(f.clip[2], cn) - ox, zn, zd));
        b = min(b, scaled(shrunk(f.clip[3], cn) - oy, zn, zd));
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
    if solid {
        *out_primitive = UVec2::new(tile_render_shared::PRIMITIVE_SOLID, f.primitive.rgba);
        return;
    }
    *out_origin = IVec2::new((sprite.atlas_x + p.texel_x) as i32, (sprite.atlas_y + p.texel_y) as i32);
    *out_page = sprite.page;
    *out_base = IVec2::new(x0, y0);
    *out_lo = IVec2::new(l - x0, t - y0);
    *out_scaled = if zoomed { 0 } else { params.scaled };
    *out_step = p.step;
    *out_palette = UVec3::new(sprite.palette, sprite.remap, f.palette);
    if screen && f.scale_den == 0 && sprite.sampling == tile_render_shared::SAMPLING_UI_BACKGROUND {
        *out_background = UVec4::new(sprite.width, sprite.height, sprite.sampling_origin_x, sprite.sampling_origin_y);
    }
}

fn max(a: i32, b: i32) -> i32 {
    if a > b { a } else { b }
}

fn min(a: i32, b: i32) -> i32 {
    if a < b { a } else { b }
}

/// The selected software adapter preserves float blend inputs but truncates
/// FP16 target writes. Its integer values use a unit-spaced FP16 interval.
fn quantized_opaque(q: u32, params: &FrameParams) -> f32 {
    if params.quantized_float != 0 { (tile_render_shared::FLOAT_QUANTIZED_BIAS + q) as f32 }
    else { (QUANTIZED_BIAS + q) as f32 / 255.0 }
}
fn quantized_mixed(q: u32, k: u32, params: &FrameParams) -> f32 {
    if params.quantized_float != 0 {
        let stored = (tile_render_shared::FLOAT_QUANTIZED_BIAS + q) as f32;
        if k >= ALPHA_STEPS { stored }
        else { stored * (k as f32 / ALPHA_STEPS as f32) + 0.5 / ALPHA_STEPS as f32 }
    } else { mixed_source(q + QUANTIZED_BIAS, k, params.color_bits) }
}

/// Terrain-cell RGB555 composition. Tile vertices always emit zero tint,
/// background and procedural-command flags, so these modes can be fixed
/// when compiling this entry. Generic tile_fs remains the fallback.
#[spirv(fragment)]
pub fn tile555_fs(
    local: Vec2,
    #[spirv(flat)] origin: IVec2,
    #[spirv(flat)] page: u32,
    #[spirv(flat)] scale_den: u32,
    #[spirv(flat)] tint: u32,
    #[spirv(flat)] base: IVec2,
    #[spirv(flat)] lo: IVec2,
    #[spirv(flat)] scaled_view: u32,
    #[spirv(flat)] step: u32,
    #[spirv(flat)] palette: UVec3,
    #[spirv(flat)] background: UVec4,
    #[spirv(flat)] primitive: UVec2,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(descriptor_set = 0, binding = 6)] atlas: &Image!(2D, type = f32, sampled, arrayed),
    #[spirv(storage_buffer, descriptor_set = 0, binding = 10)] palettes: &[u32],
    #[spirv(descriptor_set = 0, binding = 11)] index_atlas: &Image!(2D, type = f32, sampled, arrayed),
    output: &mut Vec4,
) {
    let offset = texel_offset(local, base, lo, scaled_view, step, frag, params);
    let xy = origin + offset;
    let c = if palette.x != 0 {
        let index: Vec4 = index_atlas.fetch(xy.extend(page as i32));
        let c = indexed_color(to_u8(index.x), palette.x, palette.y, palette.z, params.named_palettes, palettes);
        UVec4::new(c[0], c[1], c[2], c[3])
    } else {
        let c: Vec4 = atlas.fetch(xy.extend(page as i32));
        UVec4::new(to_u8(c.x), to_u8(c.y), to_u8(c.z), to_u8(c.w))
    };
    if c.w == 0 { spirv_std::arch::kill(); }
    if scale_den == SCALE_INVERT {
        let highest = (31 + 2 * QUANTIZED_BIAS) as f32 / 255.0;
        *output = Vec4::new(highest, highest, highest, 0.0);
    } else if scale_den != 0 {
        let k = c.x;
        if k >= scale_den { spirv_std::arch::kill(); }
        let a = 1.0 - k as f32 / scale_den as f32;
        let value = scale_offset(k, scale_den) / 255.0;
        *output = Vec4::new(value, value, value, a);
    } else {
        let k = alpha_steps(c.w);
        if k == 0 { spirv_std::arch::kill(); }
        let a = k as f32 / ALPHA_STEPS as f32;
        *output = Vec4::new(
            mixed_source((c.x >> 3) + QUANTIZED_BIAS, k, 5),
            mixed_source((c.y >> 3) + QUANTIZED_BIAS, k, 5),
            mixed_source((c.z >> 3) + QUANTIZED_BIAS, k, 5),
            a,
        );
    }
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
    #[spirv(flat)] tint: u32,
    #[spirv(flat)] base: IVec2,
    #[spirv(flat)] lo: IVec2,
    #[spirv(flat)] scaled_view: u32,
    #[spirv(flat)] step: u32,
    #[spirv(flat)] palette: UVec3,
    #[spirv(flat)] background: UVec4,
    #[spirv(flat)] primitive: UVec2,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(descriptor_set = 0, binding = 6)] atlas: &Image!(2D, type = f32, sampled, arrayed),
    #[spirv(storage_buffer, descriptor_set = 0, binding = 10)] palettes: &[u32],
    #[spirv(descriptor_set = 0, binding = 11)] index_atlas: &Image!(2D, type = f32, sampled, arrayed),
    output: &mut Vec4,
) {
    let offset = texel_offset(local, base, lo, scaled_view, step, frag, params);
    let mut color = if primitive.x == tile_render_shared::PRIMITIVE_SOLID {
        let c = primitive.y;
        Vec4::new((c & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, ((c >> 16) & 255) as f32 / 255.0, (c >> 24) as f32 / 255.0)
    } else { texel(origin + offset, page, palette, params, atlas, index_atlas, palettes) };
    let background_mode = params.background_sampling;
    let scaled_background = background.x != 0 && scaled_view != 0 && params.zoom_num != params.zoom_den && color.w != 0.0;
    if scaled_background && params.zoom_num > params.zoom_den && background_mode != tile_render_shared::BACKGROUND_NEAREST {
        // Magnification needs reconstruction between source texel centres.
        // A minification footprint alone would still be nearest at 2x/3x.
        let (n, d) = (params.zoom_num, params.zoom_den);
        let x = tile_render_shared::background_linear_axis(pixel(frag.x) as i32 - params.target_x, base.x, (background.x + step - 1) / step, n, d);
        let y = tile_render_shared::background_linear_axis(pixel(frag.y) as i32 - params.target_y, base.y, (background.y + step - 1) / step, n, d);
        let sampled = if background_mode == tile_render_shared::BACKGROUND_INTERPOLATE {
            linear_mean(x, y, step as i32, origin, page, palette, params, atlas, index_atlas, palettes)
        } else {
            let (sx, sy) = tile_render_shared::background_linear_texel(x, y, (background.z, background.w));
            texel(origin + IVec2::new(sx as i32, sy as i32) * step as i32, page, palette, params, atlas, index_atlas, palettes)
        };
        if sampled.w != 0.0 { color = Vec4::new(sampled.x, sampled.y, sampled.z, color.w); }
    } else if scaled_background && background_mode == tile_render_shared::BACKGROUND_INTERPOLATE {
        // Box filter: the area-weighted mean colour of the source texels
        // this target pixel covers, transparent ones (the other side of an
        // alpha mask) left out. The nominal sample's alpha is kept, as for
        // the dither, so coverage and keyed shapes stay exact.
        let (n, d) = (params.zoom_num, params.zoom_den);
        let s = step as i32;
        let (bound_x, bound_y) = ((background.x + step - 1) / step, (background.y + step - 1) / step);
        let (x0, x1, ax, bx) = tile_render_shared::background_footprint(pixel(frag.x) as i32 - params.target_x, base.x, lo.x, bound_x, n, d);
        let (y0, y1, ay, by) = tile_render_shared::background_footprint(pixel(frag.y) as i32 - params.target_y, base.y, lo.y, bound_y, n, d);
        let footprint = [x0, x1, ax, bx, y0, y1, ay, by];
        let mean = box_mean(footprint, n, s, origin, page, palette, params, atlas, index_atlas, palettes);
        if mean.w != 0.0 {
            color = Vec4::new(mean.x, mean.y, mean.z, color.w);
        }
    } else if scaled_background && background_mode == tile_render_shared::BACKGROUND_DITHER {
        // Jitter only the background colour within the source area covered
        // by this target pixel. Keep the ordinary sample's alpha so keyed
        // shapes and alpha masks keep their existing coverage exactly.
        let (n, d) = (params.zoom_num, params.zoom_den);
        let end_x = scaled(pixel(frag.x) as i32 + 1 - params.target_x, d, n) - base.x;
        let end_y = scaled(pixel(frag.y) as i32 + 1 - params.target_y, d, n) - base.y;
        let span_x = max(1, end_x * step as i32 - offset.x) as u32;
        let span_y = max(1, end_y * step as i32 - offset.y) as u32;
        let (x, y) = tile_render_shared::background_texel(
            (offset.x as u32, offset.y as u32), (span_x, span_y),
            (background.x, background.y), (background.z, background.w),
        );
        let sampled = texel(origin + IVec2::new(x as i32, y as i32), page, palette, params, atlas, index_atlas, palettes);
        if sampled.w != 0.0 {
            color = Vec4::new(sampled.x, sampled.y, sampled.z, color.w);
        }
    }
    if scale_den == 0 && tint >> 24 != 0 {
        // The high byte is inverse opacity: existing zero tints stay opaque.
        let opacity = 255 - (tint >> 24);
        color.w = tile_render_shared::div255(to_u8(color.w) * opacity) as f32 / 255.0;
    }
    if color.w == 0.0 {
        spirv_std::arch::kill();
    }
    let bits = params.color_bits;
    if scale_den == SCALE_INVERT {
        // The pipeline subtracts what is there from this: the highest
        // stored value (direct: 1), so the result is the inverse.
        let k = if bits != 0 { if params.quantized_float != 0 { (((1u32 << bits) - 1) + 2 * tile_render_shared::FLOAT_QUANTIZED_BIAS) as f32 } else { (((1u32 << bits) - 1) + 2 * QUANTIZED_BIAS) as f32 / 255.0 } } else { 1.0 };
        *output = Vec4::new(k, k, k, 0.0);
    } else if scale_den != 0 {
        let k = to_u8(color.x);
        if k >= scale_den {
            spirv_std::arch::kill();
        }
        let a = 1.0 - k as f32 / scale_den as f32;
        *output = if bits != 0 {
            let o = if params.quantized_float != 0 { tile_render_shared::FLOAT_QUANTIZED_BIAS as f32 * a + 0.5 / scale_den as f32 } else { scale_offset(k, scale_den) / 255.0 };
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
        let channel = |c| quantized_mixed(quantize(to_u8(c), bits), k, params);
        *output = Vec4::new(channel(color.x), channel(color.y), channel(color.z), a);
    } else {
        *output = color;
    }
}

/// Native RGB555 smoke composition. This entry is isolated from tile_fs.
#[spirv(fragment)]
pub fn native_palette_alpha_fs(
    local: Vec2,
    #[spirv(flat)] origin: IVec2,
    #[spirv(flat)] page: u32,
    #[spirv(flat)] scale_den: u32,
    #[spirv(flat)] tint: u32,
    #[spirv(flat)] base: IVec2,
    #[spirv(flat)] lo: IVec2,
    #[spirv(flat)] scaled_view: u32,
    #[spirv(flat)] step: u32,
    #[spirv(flat)] palette: UVec3,
    #[spirv(flat)] background: UVec4,
    #[spirv(flat)] primitive: UVec2,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(descriptor_set = 0, binding = 6)] atlas: &Image!(2D, type = f32, sampled, arrayed),
    #[spirv(storage_buffer, descriptor_set = 0, binding = 10)] palettes: &[u32],
    #[spirv(descriptor_set = 0, binding = 11)] index_atlas: &Image!(2D, type = f32, sampled, arrayed),
    #[spirv(descriptor_set = 1, binding = 0)] snapshot: &Image!(2D, type = f32, sampled),
    #[spirv(uniform, descriptor_set = 1, binding = 1)] smoke: &tile_render_shared::NativePaletteParams,
    output: &mut Vec4,
) {
    let offset = texel_offset(local, base, lo, scaled_view, step, frag, params);
    let source = texel(origin + offset, page, palette, params, atlas, index_atlas, palettes);
    let a = to_u8(source.w);
    if a == 0 { spirv_std::arch::kill(); }
    let pos = IVec2::new(pixel(frag.x) as i32 - smoke.origin_x, pixel(frag.y) as i32 - smoke.origin_y);
    let dst: Vec4 = snapshot.fetch(pos);
    let channel = |v: f32, stored: f32| {
        let dst5 = if params.color_bits == 5 {
            if params.quantized_float != 0 { (stored + 0.5) as u32 - tile_render_shared::FLOAT_QUANTIZED_BIAS }
            else { to_u8(stored) - QUANTIZED_BIAS }
        } else if params.color_bits == 0 { quantize(to_u8(stored), 5) }
        else {
            let q = if params.quantized_float != 0 { (stored + 0.5) as u32 - tile_render_shared::FLOAT_QUANTIZED_BIAS } else { to_u8(stored) - QUANTIZED_BIAS };
            quantize(expand(q, params.color_bits), 5)
        };
        let q = (to_u8(v) * a + 8 * dst5 * (256 - a)) / 2048;
        if params.color_bits == 0 { expand(q, 5) as f32 / 255.0 }
        else if params.color_bits == 5 { quantized_opaque(q, params) }
        else { quantized_opaque(quantize(expand(q, 5), params.color_bits), params) }
    };
    *output = Vec4::new(channel(source.x, dst.x), channel(source.y, dst.y), channel(source.z, dst.z), dst.w);
}


/// Four-tap magnification. Indexed palettes are resolved before averaging;
/// nominal alpha is retained by the caller, so masks and controls stay sharp.
#[allow(clippy::too_many_arguments)]
fn linear_mean(
    x: (u32, u32, u32), y: (u32, u32, u32), s: i32,
    origin: IVec2, page: u32, palette: UVec3, params: &FrameParams,
    atlas: &Image!(2D, type = f32, sampled, arrayed),
    index_atlas: &Image!(2D, type = f32, sampled, arrayed), palettes: &[u32],
) -> Vec4 {
    // Explicit taps avoid the SPIR-V-to-MSL loop-phi issue described below.
    let a = texel(origin + IVec2::new(x.0 as i32 * s, y.0 as i32 * s), page, palette, params, atlas, index_atlas, palettes);
    let b = texel(origin + IVec2::new(x.1 as i32 * s, y.0 as i32 * s), page, palette, params, atlas, index_atlas, palettes);
    let c = texel(origin + IVec2::new(x.0 as i32 * s, y.1 as i32 * s), page, palette, params, atlas, index_atlas, palettes);
    let d = texel(origin + IVec2::new(x.1 as i32 * s, y.1 as i32 * s), page, palette, params, atlas, index_atlas, palettes);
    let wa = if a.w != 0.0 { (64 - x.2) * (64 - y.2) } else { 0 };
    let wb = if b.w != 0.0 { x.2 * (64 - y.2) } else { 0 };
    let wc = if c.w != 0.0 { (64 - x.2) * y.2 } else { 0 };
    let wd = if d.w != 0.0 { x.2 * y.2 } else { 0 };
    let weight = wa + wb + wc + wd;
    let rgb = |t: Vec4| UVec3::new(to_u8(t.x), to_u8(t.y), to_u8(t.z));
    let sum = rgb(a) * wa + rgb(b) * wb + rgb(c) * wc + rgb(d) * wd;
    if weight == 0 { Vec4::ZERO } else { Vec4::new(
        tile_render_shared::background_mean(sum.x, weight) as f32 / 255.0,
        tile_render_shared::background_mean(sum.y, weight) as f32 / 255.0,
        tile_render_shared::background_mean(sum.z, weight) as f32 / 255.0, 1.0) }
}

/// The interpolated background's box filter: the weighted mean colour of the
/// opaque texels in `footprint` (`[x0, x1, ax, bx, y0, y1, ay, by]`, see
/// `tile_render_shared::background_footprint`), alpha 1, or alpha 0 if none
/// is opaque.
///
/// One flat loop with a trailing iteration that adds nothing. Naga's
/// SPIR-V-to-MSL translation (Metal) of these loops drops the last
/// iteration: it updates the loop's phis before evaluating the exit test
/// on them, or exports the sums from before the last iteration. Either way
/// the dropped one is the empty extra iteration; Vulkan runs it as a no-op.
#[allow(clippy::too_many_arguments)]
fn box_mean(
    footprint: [i32; 8],
    n: u32,
    s: i32,
    origin: IVec2,
    page: u32,
    palette: UVec3,
    params: &FrameParams,
    atlas: &Image!(2D, type = f32, sampled, arrayed),
    index_atlas: &Image!(2D, type = f32, sampled, arrayed),
    palettes: &[u32],
) -> Vec4 {
    let [x0, x1, ax, bx, y0, y1, ay, by] = footprint;
    if x1 <= x0 || y1 <= y0 {
        return Vec4::ZERO;
    }
    let columns = (x1 - x0) as u32;
    let total = columns * (y1 - y0) as u32;
    let (mut r, mut g, mut b, mut weight) = (0u32, 0u32, 0u32, 0u32);
    let mut k = 0u32;
    loop {
        if k < total {
            let (i, j) = (x0 + (k % columns) as i32, y0 + (k / columns) as i32);
            let w = tile_render_shared::background_weight(j, ay, by, n) * tile_render_shared::background_weight(i, ax, bx, n);
            if w != 0 {
                let t = texel(origin + IVec2::new(i * s, j * s), page, palette, params, atlas, index_atlas, palettes);
                if t.w != 0.0 {
                    r += w * to_u8(t.x);
                    g += w * to_u8(t.y);
                    b += w * to_u8(t.z);
                    weight += w;
                }
            }
        }
        k += 1;
        if k > total {
            return if weight == 0 {
                Vec4::ZERO
            } else {
                Vec4::new(
                    tile_render_shared::background_mean(r, weight) as f32 / 255.0,
                    tile_render_shared::background_mean(g, weight) as f32 / 255.0,
                    tile_render_shared::background_mean(b, weight) as f32 / 255.0,
                    1.0,
                )
            };
        }
    }
}

/// The colour of atlas texel `xy` of page `page`: of a colour sprite, or of
/// an indexed one (`palette`: its own row, remapped range and the named
/// palette it is drawn with, see `tile_render_shared::indexed_color`)
/// through its palette rows.
fn texel(
    xy: IVec2,
    page: u32,
    palette: UVec3,
    params: &FrameParams,
    atlas: &Image!(2D, type = f32, sampled, arrayed),
    index_atlas: &Image!(2D, type = f32, sampled, arrayed),
    palettes: &[u32],
) -> Vec4 {
    if palette.x == 0 {
        return atlas.fetch(xy.extend(page as i32));
    }
    let index: Vec4 = index_atlas.fetch(xy.extend(page as i32));
    let c = indexed_color(to_u8(index.x), palette.x, palette.y, palette.z, params.named_palettes, palettes);
    Vec4::new(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0)
}

/// One colour channel (0 red, 1 green, 2 blue) of a ClearType-like sprite
/// (`SCALE_SUBPIXEL`): its texels hold that channel's coverage in the same
/// channel, `tint` is the colour. Drawn three times, the pipeline writing
/// one channel each time, so each channel blends with its own alpha (in
/// whole alpha steps, rounded down, in a quantized model).
#[allow(clippy::too_many_arguments)]
fn subpixel(
    offset: IVec2,
    origin: IVec2,
    page: u32,
    palette: UVec3,
    tint: u32,
    params: &FrameParams,
    atlas: &Image!(2D, type = f32, sampled, arrayed),
    index_atlas: &Image!(2D, type = f32, sampled, arrayed),
    palettes: &[u32],
    channel: u32,
) -> Vec4 {
    let texel = texel(origin + offset, page, palette, params, atlas, index_atlas, palettes);
    let coverage = if channel == 0 { texel.x } else if channel == 1 { texel.y } else { texel.z };
    let c8 = (tint >> (16 - 8 * channel)) & 0xFF;
    let bits = params.color_bits;
    if bits != 0 {
        let k = alpha_steps(to_u8(coverage));
        if k == 0 {
            spirv_std::arch::kill();
        }
        let a = k as f32 / ALPHA_STEPS as f32;
        let v = quantized_mixed(quantize(c8, bits), k, params);
        Vec4::new(v, v, v, a)
    } else {
        if coverage == 0.0 {
            spirv_std::arch::kill();
        }
        let c = c8 as f32 / 255.0;
        Vec4::new(c, c, c, coverage)
    }
}

/// Fetch a subpixel sprite once and blend all RGB channels independently.
/// The build helper assigns both outputs to Location 0 with source indices
/// 0 and 1. The pipeline keeps the three-pass fallback's target alpha.
#[spirv(fragment)]
pub fn subpixel_dual_fs(
    local: Vec2,
    #[spirv(flat)] origin: IVec2,
    #[spirv(flat)] page: u32,
    #[spirv(flat)] _scale_den: u32,
    #[spirv(flat)] tint: u32,
    #[spirv(flat)] base: IVec2,
    #[spirv(flat)] lo: IVec2,
    #[spirv(flat)] scaled_view: u32,
    #[spirv(flat)] step: u32,
    #[spirv(flat)] palette: UVec3,
    #[spirv(flat)] _background: UVec4,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(descriptor_set = 0, binding = 6)] atlas: &Image!(2D, type = f32, sampled, arrayed),
    #[spirv(storage_buffer, descriptor_set = 0, binding = 10)] palettes: &[u32],
    #[spirv(descriptor_set = 0, binding = 11)] index_atlas: &Image!(2D, type = f32, sampled, arrayed),
    subpixel_dual_source: &mut Vec4,
    subpixel_dual_coverage: &mut Vec4,
) {
    let offset = texel_offset(local, base, lo, scaled_view, step, frag, params);
    let texel = texel(origin + offset, page, palette, params, atlas, index_atlas, palettes);
    let (r, ar) = text::dual_channel_out(to_u8(texel.x), tint, 0, params);
    let (g, ag) = text::dual_channel_out(to_u8(texel.y), tint, 1, params);
    let (b, ab) = text::dual_channel_out(to_u8(texel.z), tint, 2, params);
    // Direct colour keeps the fetched UNORM coverage, exactly as the
    // three-pass shader; quantized colour uses whole alpha steps.
    let coverage = if params.color_bits == 0 {
        Vec4::new(texel.x, texel.y, texel.z, 0.0)
    } else {
        Vec4::new(ar, ag, ab, 0.0)
    };
    if coverage.x == 0.0 && coverage.y == 0.0 && coverage.z == 0.0 {
        spirv_std::arch::kill();
    }
    *subpixel_dual_source = Vec4::new(r, g, b, 0.0);
    *subpixel_dual_coverage = coverage;
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
    #[spirv(flat)] step: u32,
    #[spirv(flat)] palette: UVec3,
    #[spirv(flat)] _background: UVec4,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(descriptor_set = 0, binding = 6)] atlas: &Image!(2D, type = f32, sampled, arrayed),
    #[spirv(storage_buffer, descriptor_set = 0, binding = 10)] palettes: &[u32],
    #[spirv(descriptor_set = 0, binding = 11)] index_atlas: &Image!(2D, type = f32, sampled, arrayed),
    output: &mut Vec4,
) {
    *output = subpixel(texel_offset(local, base, lo, scaled_view, step, frag, params), origin, page, palette, tint, params, atlas, index_atlas, palettes, 0);
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
    #[spirv(flat)] step: u32,
    #[spirv(flat)] palette: UVec3,
    #[spirv(flat)] _background: UVec4,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(descriptor_set = 0, binding = 6)] atlas: &Image!(2D, type = f32, sampled, arrayed),
    #[spirv(storage_buffer, descriptor_set = 0, binding = 10)] palettes: &[u32],
    #[spirv(descriptor_set = 0, binding = 11)] index_atlas: &Image!(2D, type = f32, sampled, arrayed),
    output: &mut Vec4,
) {
    *output = subpixel(texel_offset(local, base, lo, scaled_view, step, frag, params), origin, page, palette, tint, params, atlas, index_atlas, palettes, 1);
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
    #[spirv(flat)] step: u32,
    #[spirv(flat)] palette: UVec3,
    #[spirv(flat)] _background: UVec4,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(descriptor_set = 0, binding = 6)] atlas: &Image!(2D, type = f32, sampled, arrayed),
    #[spirv(storage_buffer, descriptor_set = 0, binding = 10)] palettes: &[u32],
    #[spirv(descriptor_set = 0, binding = 11)] index_atlas: &Image!(2D, type = f32, sampled, arrayed),
    output: &mut Vec4,
) {
    *output = subpixel(texel_offset(local, base, lo, scaled_view, step, frag, params), origin, page, palette, tint, params, atlas, index_atlas, palettes, 2);
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
    if params.lens_width > 0.0 {
        let p = tile_render_shared::LensParams { x: params.dst_x as f32, y: params.dst_y as f32,
            width: params.lens_width, height: params.lens_height, radius: params.lens_radius,
            rim: params.lens_rim, ..Default::default() };
        let s = lens_math::surface(&p, frag.x, frag.y);
        if s.distance >= -p.rim.max(0.5) {
            let size = UVec2::new(params.canvas_width, params.canvas_height);
            let ratio = params.canvas_den as f32 / params.canvas_num as f32;
            let bounds = [0, 0, size.x as i32, size.y as i32];
            let zoom = params.zoom_den as f32 / params.zoom_num as f32;
            let sample = |channel: f32| {
                let bend = s.bend + s.dispersion * channel;
                // Preserve the clear face's nearest-pixel phase, then bend
                // in window pixels before converting to source texels.
                let x = params.src_x as f32 + ((frag.x - p.x) * zoom).floor() + 0.5 - s.nx * bend * zoom;
                let y = params.src_y as f32 + ((frag.y - p.y) * zoom).floor() + 0.5 - s.ny * bend * zoom;
                lens_sample(source, x, y, bounds, params.bits, params.quantized_float)
            };
            let base = lens_nearest(source, (frag.x - params.canvas_x as f32) * ratio,
                    (frag.y - params.canvas_y as f32) * ratio,
                    bounds, params.bits, params.quantized_float);
            let backdrop = if params.lens_glass != 0 { lens_math::halo(&s, base, p.rim) } else { base };
            let glass = if params.lens_glass != 0 {
                let (r, g, b) = (sample(1.0), sample(0.0), sample(-1.0));
                lens_math::shade(&s, lens_math::dispersed(r, g, b))
            } else { [if s.distance > -p.rim * 0.5 { 0.78 } else { 0.08 }; 3] };
            let rgb = lens_math::composite(&s, glass, backdrop);
            *output = Vec4::new(rgb[0], rgb[1], rgb[2], 1.0);
            return;
        }
    }
    let (ox, oy) = (offset(frag.x, params.dst_x), offset(frag.y, params.dst_y));
    // (A copy 1:1 divides nothing.)
    let (ox, oy) = if params.zoom_den == params.zoom_num { (ox, oy) } else { (ox * params.zoom_den / params.zoom_num, oy * params.zoom_den / params.zoom_num) };
    let (sx, sy) = (params.src_x + ox as i32, params.src_y + oy as i32);
    let v: Vec4 = source.fetch(IVec2::new(sx, sy));
    let bits = params.bits;
    *output = if bits == 0 { v } else { Vec4::new(to_unorm(v.x, bits, params.quantized_float), to_unorm(v.y, bits, params.quantized_float), to_unorm(v.z, bits, params.quantized_float), v.w) };
}

/// `v · num / den`, rounded down.
fn scaled(v: i32, num: u32, den: u32) -> i32 {
    let p = v * num as i32;
    let d = den as i32;
    if p >= 0 { p / d } else { -((-p + d - 1) / d) }
}

/// How far pixel coordinate `p` lies past `o` (0 before it).
fn offset(p: f32, o: i32) -> u32 {
    let r = pixel(p) as i32 - o;
    if r < 0 { 0 } else { r as u32 }
}

fn to_unorm(stored: f32, bits: u32, float_encoding: u32) -> f32 {
    let s = if float_encoding != 0 { stored as u32 } else { to_u8(stored) };
    let bias = if float_encoding != 0 { tile_render_shared::FLOAT_QUANTIZED_BIAS } else { QUANTIZED_BIAS };
    let q = if s > bias { s - bias } else { 0 };
    expand(q, bits) as f32 / 255.0
}

fn to_u8(v: f32) -> u32 {
    round(v * 255.0)
}

/// `v` (in `[0, 2^23)`) rounded to the nearest whole number (half to even),
/// from the bits of `v + 2^23`: no saturating conversion (each `as u32` of
/// a float is several instructions, and a fragment makes a dozen).
fn round(v: f32) -> u32 {
    (v + 8_388_608.0).to_bits() - 0x4B00_0000
}

/// The pixel `c` (at least 0) lies in: `c` rounded down.
fn pixel(c: f32) -> u32 {
    round(c.floor())
}

/// Restores cut-off corners and shades only the lens's narrow border.
#[spirv(fragment)]
pub fn lens_fs(
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 1)] p: &tile_render_shared::LensParams,
    #[spirv(descriptor_set = 0, binding = 0)] source: &Image!(2D, type = f32, sampled),
    #[spirv(descriptor_set = 0, binding = 2)] face: &Image!(2D, type = f32, sampled),
    #[spirv(descriptor_set = 0, binding = 3)] composed: &Image!(2D, type = f32, sampled),
    #[spirv(descriptor_set = 0, binding = 4)] shaded: &Image!(2D, type = f32, sampled),
    output: &mut Vec4,
) {
    let s = lens_math::surface(p, frag.x, frag.y);
    // Restore a source-pixel guard inside the bevel when re-shading boxed
    // pixels, so no old coarse highlight leaks onto the clear face.
    let guard = if p.sample_scale != 1.0 { 1.0 / p.sample_scale } else { 0.0 };
    if s.distance < -p.rim.max(0.5) - guard { spirv_std::arch::kill(); }
    let bounds = [p.snapshot_x, p.snapshot_y, p.snapshot_w, p.snapshot_h];
    let (x, y) = (frag.x * p.sample_scale + p.sample_x, frag.y * p.sample_scale + p.sample_y);
    if p.check_occlusion != 0 {
        // Compare stored texels exactly, before colour decoding. Later UI or
        // another camera can cover a rim, including with translucent art.
        let xy = IVec2::new(x.floor() as i32, y.floor() as i32);
        let before: Vec4 = shaded.fetch(IVec2::new((xy.x-p.snapshot_x).clamp(0,p.snapshot_w-1), (xy.y-p.snapshot_y).clamp(0,p.snapshot_h-1)));
        let after: Vec4 = composed.fetch(xy);
        if before.x != after.x || before.y != after.y || before.z != after.z { spirv_std::arch::kill(); }
    }
    // Backdrop art retains nearest boxed pixels, even when the rim is shaded
    // at final window resolution. Only the refracted face is interpolated.
    let base = lens_nearest(source, x, y, bounds, p.bits, p.quantized_float);
    let backdrop = if p.glass != 0 { lens_math::halo(&s, base, p.rim) } else { base };
    let glass = if s.distance < -p.rim.max(0.5) {
        lens_nearest(face, x, y, bounds, p.bits, p.quantized_float)
    } else if p.glass != 0 {
        let fetch = |channel: f32| {
            let bend = (s.bend + s.dispersion * channel) * p.sample_scale;
            lens_sample(face, x - s.nx * bend, y - s.ny * bend, bounds, p.bits, p.quantized_float)
        };
        let (r, g, b) = (fetch(1.0), fetch(0.0), fetch(-1.0));
        lens_math::shade(&s, lens_math::dispersed(r, g, b))
    } else { [if s.distance > -p.rim * 0.5 { 0.78 } else { 0.08 }; 3] };
    let rgb = lens_math::composite(&s, glass, backdrop);
    let encode = |v: f32| if p.output_bits == 0 { v } else {
        let q = quantize((v * 255.0 + 0.5) as u32, p.output_bits);
        if p.quantized_float != 0 { (q + tile_render_shared::FLOAT_QUANTIZED_BIAS) as f32 }
        else { (q + QUANTIZED_BIAS) as f32 / 255.0 }
    };
    *output = Vec4::new(encode(rgb[0]), encode(rgb[1]), encode(rgb[2]), 1.0);
}

/// Backdrop and clear-face guards keep boxed art. Fetch a single texel rather
/// than running four-tap interpolation with three zero weights on every rim pixel.
fn lens_nearest(source: &Image!(2D, type = f32, sampled), x: f32, y: f32,
    bounds: [i32; 4], bits: u32, float_encoding: u32) -> [f32; 3] {
    let v: Vec4 = source.fetch(IVec2::new(
        (x.floor() as i32 - bounds[0]).clamp(0, bounds[2]-1),
        (y.floor() as i32 - bounds[1]).clamp(0, bounds[3]-1)));
    if bits == 0 { [v.x, v.y, v.z] }
    else { [to_unorm(v.x, bits, float_encoding), to_unorm(v.y, bits, float_encoding), to_unorm(v.z, bits, float_encoding)] }
}

/// Bilinear refraction avoids bright nearest-sample ticks along straight edges.
/// Decode each quantized texel before interpolation, never interpolate its code.
fn lens_sample(source: &Image!(2D, type = f32, sampled), x: f32, y: f32,
    bounds: [i32; 4], bits: u32, float_encoding: u32) -> [f32; 3] {
    let (x, y) = (x - 0.5 - bounds[0] as f32, y - 0.5 - bounds[1] as f32);
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let fetch = |dx: i32, dy: i32| {
        let v: Vec4 = source.fetch(IVec2::new((ix+dx).clamp(0, bounds[2]-1), (iy+dy).clamp(0, bounds[3]-1)));
        if bits == 0 { v } else { Vec4::new(to_unorm(v.x, bits, float_encoding), to_unorm(v.y, bits, float_encoding), to_unorm(v.z, bits, float_encoding), 1.0) }
    };
    let top = fetch(0,0) * (1.0-fx) + fetch(1,0) * fx;
    let bottom = fetch(0,1) * (1.0-fx) + fetch(1,1) * fx;
    let v = top * (1.0-fy) + bottom * fy;
    [v.x, v.y, v.z]
}
