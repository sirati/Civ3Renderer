//! Text drawn from glyph outlines (`tile_render_shared::text`): one quad
//! per glyph instance, covering the pixels the glyph may touch at the
//! view's zoom; the fragment shaders compute each pixel's coverage per
//! colour channel from the curves and blend like ClearType-like sprites
//! (`subpixel_*_fs`), or (`text_keyed_fs`) draw the text onto a colour key.

use spirv_std::glam::{IVec4, UVec4, Vec4};
use spirv_std::spirv;
use tile_render_shared::text::{GlyphPlace, TextGlyph, bounds, clip_at_zoom, coverage, keyed, on_screen, place, visible_at_art};
use tile_render_shared::{ALL_VIEWS, ALPHA_STEPS, FREE_EXCEPT_VIEW, FrameParams, QUANTIZED_BIAS, alpha_steps, mixed_source, quantize, shrunk, world_period, wrap_distance};

use crate::{CULLED, max, min, quad_corner};

#[spirv(vertex(entry_point_name = "text_vs"))]
pub fn text_vs(
    #[spirv(vertex_index)] vertex: u32,
    #[spirv(instance_index)] instance: u32,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 8)] text: &[TextGlyph],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 9)] outlines: &[u32],
    #[spirv(position)] out_pos: &mut Vec4,
    #[spirv(flat)] out_place: &mut IVec4,
    #[spirv(flat)] out_glyph: &mut UVec4,
    #[spirv(flat)] out_bar: &mut IVec4,
    #[spirv(flat)] out_face: &mut u32,
) {
    let g = &text[instance as usize];
    *out_place = IVec4::ZERO;
    *out_glyph = UVec4::ZERO;
    *out_bar = IVec4::ZERO;
    *out_face = 0;
    *out_pos = CULLED;
    // Ordinary/Low text at 1:1, ordinary/Zoomed text where a scaled view
    // shows outline art; in its own view, or in the other world views
    // (`FREE_EXCEPT_VIEW`).
    let except = g.flags & FREE_EXCEPT_VIEW != 0;
    let wrong_view = if except { g.view == params.view || params.world == 0 } else { (g.view != ALL_VIEWS && g.view != params.view) || (g.view == ALL_VIEWS && params.world == 0) };
    if !visible_at_art(g.flags, params.zoom_art) || wrong_view || g.category & params.hidden_categories != 0 {
        return;
    }
    // From the screen origin (screen text) or the camera (world text at the
    // view's scale, the copy near the camera in a wrapping world), 1:1; at
    // its own size.
    let screen = on_screen(g);
    let n = if screen { 1 } else { params.downscale };
    let (bx, by) = if screen { (params.origin_x, params.origin_y) } else { (params.camera_x, params.camera_y) };
    let (fx, fy) = if screen {
        (g.x - bx, g.y - by)
    } else {
        let (px, py) = world_period(&params.grid);
        (wrap_distance(shrunk(g.x, n) - bx, px), wrap_distance(shrunk(g.y, n) - by, py))
    };
    let (zn, zd) = (params.zoom_num, params.zoom_den);
    let Some(p) = place(outlines, g, fx, fy, zn, zd) else { return };
    let (mut l, mut t, mut r, mut b) = bounds(outlines, &p);
    if let Some(c) = clip_at_zoom(g, (bx, by), zn, zd, n) {
        (l, t, r, b) = (max(l, c[0]), max(t, c[1]), min(r, c[2]), min(b, c[3]));
    }
    if r <= l || b <= t {
        return;
    }
    let (u, v) = quad_corner(vertex);
    let (sx, sy) = (l as f32 + u * (r - l) as f32, t as f32 + v * (b - t) as f32);
    *out_pos = Vec4::new(sx / params.viewport_w as f32 * 2.0 - 1.0, 1.0 - sy / params.viewport_h as f32 * 2.0, 0.0, 1.0);
    *out_place = IVec4::new(p.x, p.y, p.scale, p.bold);
    *out_glyph = UVec4::new(p.flags, p.glyph, g.color, g.key);
    *out_bar = IVec4::new(p.bar_w, p.bar_top, p.bar_h, 0);
    *out_face = p.face;
}

/// The coverage per channel of the fragment's pixel (the view is drawn at
/// its zoom from `target_x, target_y` on).
fn pixel_coverage(place: IVec4, glyph: UVec4, bar: IVec4, face: u32, frag: Vec4, params: &FrameParams, outlines: &[u32]) -> [u32; 3] {
    let p = GlyphPlace { x: place.x, y: place.y, scale: place.z, bold: place.w, flags: glyph.x, glyph: glyph.y, face, bar_w: bar.x, bar_top: bar.y, bar_h: bar.z };
    coverage(outlines, &p, frag.x as i32 - params.target_x, frag.y as i32 - params.target_y, glyph.z)
}

/// Channel `channel` of text in `color` with coverage `c` (of 255), as
/// `subpixel` draws a ClearType-like sprite's texel.
fn channel_out(c: u32, color: u32, channel: u32, params: &FrameParams) -> Vec4 {
    let c8 = (color >> (16 - 8 * channel)) & 0xFF;
    let bits = params.color_bits;
    if bits != 0 {
        let k = alpha_steps(c);
        if k == 0 {
            spirv_std::arch::kill();
        }
        let a = k as f32 / ALPHA_STEPS as f32;
        let v = mixed_source(quantize(c8, bits) + QUANTIZED_BIAS, k, bits);
        Vec4::new(v, v, v, a)
    } else {
        if c == 0 {
            spirv_std::arch::kill();
        }
        let v = c8 as f32 / 255.0;
        Vec4::new(v, v, v, c as f32 / 255.0)
    }
}

#[spirv(fragment(entry_point_name = "text_r_fs"))]
pub fn text_r_fs(
    #[spirv(flat)] place: IVec4,
    #[spirv(flat)] glyph: UVec4,
    #[spirv(flat)] bar: IVec4,
    #[spirv(flat)] face: u32,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 9)] outlines: &[u32],
    output: &mut Vec4,
) {
    let c = pixel_coverage(place, glyph, bar, face, frag, params, outlines);
    *output = channel_out(c[0], glyph.z, 0, params);
}

#[spirv(fragment(entry_point_name = "text_g_fs"))]
pub fn text_g_fs(
    #[spirv(flat)] place: IVec4,
    #[spirv(flat)] glyph: UVec4,
    #[spirv(flat)] bar: IVec4,
    #[spirv(flat)] face: u32,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 9)] outlines: &[u32],
    output: &mut Vec4,
) {
    let c = pixel_coverage(place, glyph, bar, face, frag, params, outlines);
    *output = channel_out(c[1], glyph.z, 1, params);
}

#[spirv(fragment(entry_point_name = "text_b_fs"))]
pub fn text_b_fs(
    #[spirv(flat)] place: IVec4,
    #[spirv(flat)] glyph: UVec4,
    #[spirv(flat)] bar: IVec4,
    #[spirv(flat)] face: u32,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 9)] outlines: &[u32],
    output: &mut Vec4,
) {
    let c = pixel_coverage(place, glyph, bar, face, frag, params, outlines);
    *output = channel_out(c[2], glyph.z, 2, params);
}

/// Text drawn onto its colour key: the opaque colour where that differs
/// from the key, as a sprite's opaque texel is drawn.
#[spirv(fragment(entry_point_name = "text_keyed_fs"))]
pub fn text_keyed_fs(
    #[spirv(flat)] place: IVec4,
    #[spirv(flat)] glyph: UVec4,
    #[spirv(flat)] bar: IVec4,
    #[spirv(flat)] face: u32,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &FrameParams,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 9)] outlines: &[u32],
    output: &mut Vec4,
) {
    let c = pixel_coverage(place, glyph, bar, face, frag, params, outlines);
    let Some(rgb) = keyed(c, glyph.z, glyph.w) else { spirv_std::arch::kill() };
    let bits = params.color_bits;
    let v = |c8: u32| if bits != 0 { (quantize(c8, bits) + QUANTIZED_BIAS) as f32 / 255.0 } else { c8 as f32 / 255.0 };
    *output = Vec4::new(v(rgb[0]), v(rgb[1]), v(rgb[2]), 1.0);
}
