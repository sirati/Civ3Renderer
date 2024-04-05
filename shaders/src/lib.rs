#![cfg_attr(target_arch = "spirv", no_std)]
// HACK(eddyb) can't easily see warnings otherwise from `spirv-builder` builds.
//#![cfg_attr(target_arch = "spirv", deny(warnings))]

// use std::mem::swap;
// use std::usize;
//#[cfg(target_arch = "spirv")]


// use spirv_std::glam::{UVec2, Vec3};
use spirv_std::glam::{UVec3, Vec2, Vec4};
use spirv_std::{spirv};
//use civ3_renderer_shared::Params;

#[repr(C)]
pub struct Params {
    pub world_size_x: u32,
    pub world_size_y: u32,
    pub max_texture_y: u32,
    pub layer_count: u32,
}

// 
// #[repr(C)]
// pub struct Params {}

/*
const RHOMBUS_VERTEX_COUNT: usize = 6;*/

#[spirv(compute(threads(32)))]
pub fn main_generate_rhombuses(
    #[spirv(global_invocation_id)] _global_id: UVec3,
/*    #[spirv(uniform, descriptor_set = 1, binding = 0)] params: &Params,
    #[spirv(storage_buffer, descriptor_set = 1, binding = 1)] leftmost_vertices_buffer: &[UVec2],
    #[spirv(storage_buffer, descriptor_set = 1, binding = 2)] vertices_generated: &mut [Vec4],*/
) {
    /*let x_index = global_id.x as usize;
    let y_index = global_id.y as usize;
    let instance_index = y_index * params.world_size_x as usize + x_index;
    let leftmost_vertex = leftmost_vertices_buffer[instance_index as usize];
    let start_index = instance_index as usize * RHOMBUS_VERTEX_COUNT * params.layer_count as usize;
    let rhombus = Rhombus::from_fu2(leftmost_vertex);
    for l in 0..params.layer_count {
        rhombus.apply_v4s(start_index + l as usize * RHOMBUS_VERTEX_COUNT, vertices_generated, l as f32);
    }*/
}

/*
#[repr(C)]
pub struct Rhombus {
    top: Vec2,
    left: Vec2,
    right: Vec2,
    bottom: Vec2
}

impl Rhombus {
    pub fn from_fu2(left_v: UVec2) -> Self {
        Self::from_fv2(Vec2{x: left_v.x as f32, y: left_v.y as f32 * 0.8})
    }

    pub fn from_fv2(left_v: Vec2) -> Self {
        let left = left_v.x;
        let right = left + 1.0;
        let middle_x = left + 0.5;
        let middle_y = left_v.y;
        let top = left_v.y - 0.4;
        let bottom = left_v.y + 0.4;


        Self { top: Vec2{x: middle_x, y: top}, left: Vec2{x: left, y: middle_y}, right: Vec2{x: right, y: middle_y}, bottom: Vec2{x: middle_x, y: bottom}}
    }

    // ltr, lrb
    pub fn apply_v3s(&self, index: usize, v3s: &mut [Vec3], z: f32) {
        v3s[index] = self.left.extend(z);
        v3s[index + 1] = self.top.extend(z);
        v3s[index + 2] = self.right.extend(z);
        v3s[index + 3] = v3s[index];
        v3s[index + 4] = v3s[index + 2];
        v3s[index + 5] = self.bottom.extend(z);
    }


    pub fn apply_v4s(&self, index: usize, v4s: &mut [Vec4], z: f32) {
        v4s[index] = self.left.extend(z).extend(0.0);
        v4s[index + 1] = self.top.extend(z).extend(0.0);
        v4s[index + 2] = self.right.extend(z).extend(0.0);
        v4s[index + 3] = v4s[index];
        v4s[index + 4] = v4s[index + 2];
        v4s[index + 5] = self.bottom.extend(z).extend(0.0);
    }
}

const NO_SPRITE: usize = 0;*/

#[spirv(compute(threads(32)))]
pub fn main_resolve_sprite_and_mark_culled(
    #[spirv(global_invocation_id)] _global_id: UVec3,
/*    #[spirv(uniform, descriptor_set = 1, binding = 0)] params: &Params,
    #[spirv(storage_buffer, descriptor_set = 1, binding = 2)] vertices_generated: &mut [Vec4],
    #[spirv(storage_buffer, descriptor_set = 1, binding = 3)] world_data: &[u8],
    #[spirv(storage_buffer descriptor_set = 1, binding = 4)] layer_lookup_table: &[u8],
    #[spirv(storage_buffer, descriptor_set = 1, binding = 5)] texture_lookup_table: &[u8],*/
) {/*
    let frame_count: u32 = 0;
    
    let x = global_id.x as usize;
    let y_index = global_id.y as usize;
    let instance_index = y_index * params.world_size_x as usize + x;
    let start_index = instance_index as usize * RHOMBUS_VERTEX_COUNT * params.layer_count as usize;

    let is_top = x == 0;
    let is_left = y_index == 0;
    let is_right = x == params.world_size_x as usize - 1;
    let is_bottom = y_index == params.world_size_y as usize - 1;

    for l in 0usize..params.layer_count as usize {
        let layer_index = start_index + l * RHOMBUS_VERTEX_COUNT;
        let flags = world_data[l * 4];
        let bits = ((flags >> 6) + 1) as usize;
        let offset_flags = flags << 2 >> 4;
        let ignore_top = offset_flags & 1 != 0;
        let ignore_left = offset_flags & 2 != 0;
        let ignore_right = offset_flags & 4 != 0;
        let ignore_bottom = offset_flags & 8 != 0;

        let sprite_index = if (ignore_top && is_top) || (ignore_left && is_left) ||
            (ignore_right && is_right) || (ignore_bottom && is_bottom) {
            NO_SPRITE as usize
        } else {
            let world_base_addr = u32::from_be_bytes([flags << 6 >> 6, world_data[l *4+1], world_data[l *4+2], world_data[l *4+3]]) as usize;
            //+1 is for animation offset. let's consider if we can omit that in the future by a flag
            let local_sprite_index = world_base_addr + (instance_index * bits + 1);
            let local_base_addr = u32::from_be_bytes([layer_lookup_table[l *4], layer_lookup_table[l *4+1], layer_lookup_table[l *4+2], layer_lookup_table[l *4+3]]) as usize;

            let sprite_index = local_base_addr + {let mut bytes4 = [0; 4];
                                    bytes4[4-bits..].copy_from_slice(&layer_lookup_table[local_sprite_index..local_sprite_index + bits]);
                                    u32::from_be_bytes(bytes4) as usize};
            //one after sprite_index is frame offset for the animation
            let anim_frame_offset = layer_lookup_table[local_sprite_index + bits];
            let anim_frames_count = texture_lookup_table[sprite_index];

            //+1 because first entry is animation frame count
            let anim_index = sprite_index + 1 + ((frame_count+ anim_frame_offset as u32) % anim_frames_count as u32) as usize;
            u32::from_be_bytes(*texture_lookup_table[anim_index..anim_index+4].try_into().unwrap_or(&[0u8,0,0,0])) as usize
        };
        
        let was_swapped = vertices_generated[layer_index + 1].w as i32 == -1;
        let need_swapper = sprite_index == NO_SPRITE;

        for i in 0..6 {
            vertices_generated[layer_index + i].w = sprite_index as f32;
        }
        if need_swapper && !was_swapped {
            vertices_generated.swap(layer_index+1, layer_index+2);
            vertices_generated.swap(layer_index+4, layer_index+5);
            //need to last as we swapped 1 and 2
            vertices_generated[layer_index + 1].w = -1.0;
        } else if need_swapper && !was_swapped  {
            vertices_generated.swap(layer_index+1, layer_index+2);
            vertices_generated.swap(layer_index+4, layer_index+5);
        }
    }*/
}

#[spirv(vertex)]
pub fn main_apply_sprite_uv_or_culling(
    #[spirv(vertex_index)] vert_index: usize,
    #[spirv(uniform, descriptor_set = 1, binding = 0)] params: &Params,
    #[spirv(position)] out_pos: &mut Vec4,
    #[spirv(flat)] out_uv: &mut Vec2,
) {
    if out_pos.w > 0.0 {
        let sprite_index = out_pos.w as usize;

        let x = (sprite_index & 0xFF) as f32;
        let y = (sprite_index >> 8) as f32;

        // the rhombus is tiled but our graphics are not!
        //        t            |          t  r
        //       l r           |
        //        b            |          l  b
        // so we need to correctly apply the UV based on this!
        // 012  345 indices are clockwise
        // ltr, lrb
        let vi = vert_index % 6;

        *out_uv = if /*left*/ vi == 0 || vi == 3 {
            Vec2::new(
                (x + 1.0) / 256.0,
                (y + 0.0) / params.max_texture_y as f32)
        } else if /*right*/ vi == 2 || vi == 4 {
            Vec2::new(
                (x + 0.0) / 256.0,
                (y + 1.0) / params.max_texture_y as f32)
        } else if /*top*/ vi == 1 {
            Vec2::new(
                (x + 0.0) / 256.0,
                (y + 0.0) / params.max_texture_y as f32)            
        } else if /*bottom*/ vi == 5 {
            Vec2::new(
                (x + 1.0) / 256.0,
                (y + 1.0) / params.max_texture_y as f32)            
        } else {/*impossible*/ return;};
    }
}


// #[spirv(vertex)]
// pub fn main_vertex(
//     #[spirv(vertex_index)] vertex_index: u32,
//     #[spirv(uniform, descriptor_set = 1, binding = 0)] layer_data: &LayerData,
//     #[spirv(position)] out_position: &mut Vec4,
//     uv: &mut Vec2,
// ) {
//     let x = vertex_index % layer_data.tiles_x;
//     let y = vertex_index / layer_data.tiles_x;
//
//     *uv = Vec2::new(
//         x as f32 / layer_data.tiles_x as f32,
//         y as f32 / layer_data.tiles_y as f32,
//     );
//
//     if layer_data.flags.contains(LayerFlags::ISOMETRIC) {
//         *out_position = Vec4::new(
//             x as f32 - y as f32,
//             (x as f32 + y as f32) * 0.5,
//             layer_data.depth,
//             1.0,
//         );
//     } else {
//         *out_position = Vec4::new(
//             x as f32 + layer_data.offset_x,
//             y as f32 + layer_data.offset_y,
//             layer_data.depth,
//             1.0,
//         );
//     }
// }
//
// #[spirv(fragment)]
// pub fn main_fragment(
//     uv: Vec2,
//     #[spirv(uniform, descriptor_set = 1, binding = 0)] layer_data: &LayerData,
//     #[spirv(descriptor_set = 1, binding = 1)] texture_atlas: &Image2dArray,
//     #[spirv(descriptor_set = 1, binding = 2)] sampler_atlas: &spirv_std::Sampler,
//     output: &mut Vec4,
// ) {
//     let texture_index = /* Retrieving texture index logic goes here */ 0;
//     let sampled_color = unsafe {
//         texture_atlas.sample(*sampler_atlas, uv, Some(Vec3::new(0.0, 0.0, texture_index as f32)))
//     };
//     *output = sampled_color;
// }
//
//
// #[spirv(storage_buffer)]
// pub struct LookupTable {
//     data: [u8],
// }
//

//
//
// #[spirv(vertex)]
// pub fn generate_vertices(
//     #[spirv(vertex_index)] vertex_index: i32,
//     #[spirv(instance_index)] instance_index: i32,
//     #[spirv(uniform, descriptor_set = 1, binding = 0)] params: &Uniform<Params>,
//     #[spirv(position)] out_pos: &mut Vec4,
//     #[spirv(output)] uv: &mut Vec2,
// ) {
//     let x_index = instance_index % params.world_size_x as i32;
//     let y_index = instance_index / params.world_size_x as i32;
//     let layer_index = (instance_index / (params.world_size_x * params.world_size_y)) as f32;
//
//     let height_offset = layer_index * 0.001;
//
//     // Calculate the base position of the left point of each rhombus.
//     let base_x = x_index as f32 + if y_index % 2 == 0 { 0.0 } else { 0.5 };
//     let base_y = y_index as f32 * 0.4 + height_offset; // Adjust based on the isometric layout.
//
//     // Define vertices for the rhombus based on the base position.
//     let vertices = [
//         Vec3::new(base_x, base_y, 0.0), // Reused left corner for both triangles
//         Vec3::new(base_x + 0.5, base_y + 0.4, 0.0),
//         Vec3::new(base_x + 1.0, base_y, 0.0),
//         Vec3::new(base_x, base_y, 0.0), // Repeating the left corner for the second triangle
//         Vec3::new(base_x + 1.0, base_y, 0.0),
//         Vec3::new(base_x + 0.5, base_y - 0.4, 0.0),
//     ];
//
//     // Select the appropriate vertex based on `vertex_index`.
//     let vertex = vertices[vertex_index as usize];
//     *out_pos = Vec4::new(vertex.x, vertex.y, vertex.z, 1.0);
//     *uv = Vec2::new(vertex.x / 256.0, vertex.y / params.max_texture_y as f32);
// }
//
// #[repr(C)]
// pub struct Params {
//     world_size_x: u32,
//     world_size_y: u32,
//     max_texture_y: u32,
// }