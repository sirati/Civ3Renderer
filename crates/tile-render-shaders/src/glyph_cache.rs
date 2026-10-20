//! Exact linear LCD coverage rendered once into an integer glyph atlas.
//! Placement and packing happen on the host. All outline integration stays
//! in these GPU shaders, and all later colour blending stays in text.rs.

use spirv_std::glam::{IVec4, UVec3, UVec4, Vec4};
use spirv_std::spirv;
use tile_render_shared::glyph_cache::CacheJob;
use tile_render_shared::text::{GlyphPlace, linear_coverage_all_channels};
use crate::{CULLED, quad_corner};

#[spirv(vertex(entry_point_name = "glyph_cache_vs"))]
pub fn glyph_cache_vs(
    #[spirv(vertex_index)] vertex: u32,
    #[spirv(instance_index)] instance: u32,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] jobs: &[CacheJob],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] ready: &[u32],
    #[spirv(position)] out_pos: &mut Vec4,
    #[spirv(flat)] out_place: &mut IVec4,
    #[spirv(flat)] out_glyph: &mut UVec4,
    #[spirv(flat)] out_bar: &mut IVec4,
    #[spirv(flat)] out_offset: &mut IVec4,
) {
    *out_pos = CULLED;
    *out_place = IVec4::ZERO;
    *out_glyph = UVec4::ZERO;
    *out_bar = IVec4::ZERO;
    *out_offset = IVec4::ZERO;
    let j = &jobs[instance as usize];
    if ready[j.ready_index as usize] == j.ready_tag { return; }
    let (u, v) = quad_corner(vertex);
    let (x, y) = (j.atlas_x as f32 + u * j.width as f32, j.atlas_y as f32 + v * j.height as f32);
    *out_pos = Vec4::new(x / j.atlas_width as f32 * 2.0 - 1.0, 1.0 - y / j.atlas_height as f32 * 2.0, 0.0, 1.0);
    *out_place = IVec4::new(j.x, j.y, j.scale, j.bold);
    *out_glyph = UVec4::new(j.flags, j.glyph, j.face, 0);
    *out_bar = IVec4::new(j.bar_w, j.bar_top, j.bar_h, 0);
    *out_offset = IVec4::new(j.atlas_x as i32, j.atlas_y as i32, j.left, j.top);
}

#[spirv(fragment(entry_point_name = "glyph_cache_fs"))]
pub fn glyph_cache_fs(
    #[spirv(flat)] place: IVec4,
    #[spirv(flat)] glyph: UVec4,
    #[spirv(flat)] bar: IVec4,
    #[spirv(flat)] offset: IVec4,
    #[spirv(frag_coord)] frag: Vec4,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] outlines: &[u32],
    output: &mut UVec4,
) {
    let p = GlyphPlace { x: place.x, y: place.y, scale: place.z, bold: place.w, flags: glyph.x, glyph: glyph.y, face: glyph.z, bar_w: bar.x, bar_top: bar.y, bar_h: bar.z };
    let c = linear_coverage_all_channels(outlines, &p, frag.x as i32 - offset.x + offset.z, frag.y as i32 - offset.y + offset.w);
    *output = UVec4::new(c[0], c[1], c[2], 0);
}

/// Publish only after the render pass that generated these exact texels.
/// Queue resource ordering handles visibility; the CPU never reads or
/// waits on the flags. Canceling an encoder cannot publish its entries.
#[spirv(compute(threads(64), entry_point_name = "glyph_cache_publish_cs"))]
pub fn glyph_cache_publish_cs(
    #[spirv(global_invocation_id)] invocation: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] jobs: &[CacheJob],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] ready: &mut [u32],
) {
    let i = invocation.x as usize;
    if i < jobs.len() {
        let j = &jobs[i];
        ready[j.ready_index as usize] = j.ready_tag;
    }
}
