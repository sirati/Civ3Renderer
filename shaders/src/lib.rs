#![cfg_attr(target_arch = "spirv", no_std)]
//#![feature(lang_items)]
// HACK(eddyb) can't easily see warnings otherwise from `spirv-builder` builds.
//#![cfg_attr(target_arch = "spirv", deny(warnings))]

// use std::mem::swap;
// use std::usize;
//#[cfg(target_arch = "spirv")]


extern crate core;
#[allow(dead_code)] mod lower_aligned;
use crate::lower_aligned::*;

use spirv_std::glam::{UVec2, Vec3};
use spirv_std::glam::{UVec3, Vec2, Vec4};
use spirv_std::spirv;
use civ3_renderer_shared::{NO_SPRITE, Params, RHOMBUS_VERTEX_COUNT};
//use civ3_renderer_shared::Params;

// 
// #[repr(C)]
// pub struct Params {}

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
named_array!(WorldData);
named_array!(LayerLookupTable);
named_array!(TextureLookupTable);

#[repr(transparent)]
pub struct NewType<'a>(&'a [u32]);
#[spirv(compute(threads(32)))]
fn _foo(#[spirv(storage_buffer, descriptor_set = 1, binding = 3)] new_type: &[u32]) {
    let _new_type = NewType(new_type);
}

#[spirv(compute(threads(32)))]
fn _foo2(#[spirv(storage_buffer, descriptor_set = 1, binding = 3)] _new_type: NewType) {
    
}


fn reorder_global_id(global_id: UVec3, params: &Params) -> (u32, u32) {
    //This branches but does not use type conversions, or division
    //The branch is always the same per workgroup though
    //todo: benchmark which is faster
    /*let mut x = global_id.y<<2;
    let alignment_branch = if global_id.y >= params.compute_size_x >> 1 {
        x -= params.compute_size_x <<1;
        if global_id.y - params.compute_size_x >> 1 >= params.compute_size_x >> 2 {
            x -= params.compute_size_x;
            3
        } else {
            2
        }
    } else if global_id.y >= params.compute_size_x >> 2 {
        x -= params.compute_size_x;
        1
    } else {
        0
    };
    (x + alignment_branch , global_id.x) //x & y switched intentionally*/


    //This is branch-less, but needs type conversion 
    let int_div = (global_id.y as f32 * params.four_over_compute_size) as u32;
    let remainder = global_id.y - int_div * (params.compute_size_x >> 2);
    ((remainder << 2) + int_div , global_id.x) //x & y switched intentionally
}

#[spirv(compute(threads(16, 4, 1)))]
pub fn main_resolve_sprite_and_mark_culled(
    #[spirv(global_invocation_id)] global_id: UVec3,
    #[spirv(uniform, descriptor_set = 1, binding = 0)] params: &Params,
    #[spirv(storage_buffer, descriptor_set = 1, binding = 2)] vertices_generated: &mut [Vec4],
    #[spirv(storage_buffer, descriptor_set = 1, binding = 3)] world_data: &[u32],
    #[spirv(storage_buffer, descriptor_set = 1, binding = 4)] layer_lookup_table: &[u32],
    #[spirv(storage_buffer, descriptor_set = 1, binding = 5)] texture_lookup_table: &[u32],

    frame_count: u32,
    /*#[spirv(storage_buffer, descriptor_set = 1, binding = 3)] world_data: &WorldData,
    #[spirv(storage_buffer, descriptor_set = 1, binding = 4)] layer_lookup_table: &LayerLookupTable,
    #[spirv(storage_buffer, descriptor_set = 1, binding = 5)] texture_lookup_table: &TextureLookupTable,*/
) {
    /*let vertices_generated: &mut [Vec4],*/
    //    let frame_count: u32 = 0;
    let world_data: WorldData = WorldData(world_data);
    let layer_lookup_table: LayerLookupTable = LayerLookupTable(layer_lookup_table);
    let texture_lookup_table: TextureLookupTable=  TextureLookupTable(texture_lookup_table);
    
    
    /*
    # because the data is packed consistently per layer, and we want to avoid branching,
    # or taking both branches within a workgroup
    # each workgroup should only ever access one layer
    # further the architecture u32 means that no matter if our data is 1, 2, 3 or 4 bytes,
    # after x advancing by 4, we are always at the same boundary within a workgroup
    # with other words if we have 4 workgroups, within a layer
    # 1234123412341234 is the pattern we want to access the data
    # 1234123412341234
    # current cpu have 64 thread per workgroup, and to not waste threads,
    # we want to minimize the width and height of the workgroup, 
    # as we need to repeat 4 times per width to get a cycle we must satisfy x*y*4=64*4 and y=x*4
    # this is given for x=4, y=16
    # now why do set threads as x=16, y=4 then? Even between workgroups there may be some branch
    # prediction, so we want to make sure this is as regular as possible. With others words,
    # we want to do all columns of the first alignment branch before we proceed to the second
    */
    let (x, y) = reorder_global_id(global_id, params);
    if x >= params.world_size_x || y >= params.world_size_y {
        return;
    }
    //yes as written above x, y here are switched on purpose
    
    let instance_index = (y * params.world_size_x + x) as usize;
    let start_index = instance_index * RHOMBUS_VERTEX_COUNT * params.layer_count as usize;

    let is_top = x == 0;
    let is_left = y == 0;
    let is_right = x == params.world_size_x - 1;
    let is_bottom = y == params.world_size_y - 1;

    for l in 0usize..params.layer_count as usize {
        //the top 6 bits are flags, the rest is the base address
        let world_base_addr = world_data.inner_as_ref()[l * 4];

        //  0btlrboo00_00000000_00000000_00000000
        let ignore_top = world_base_addr & (1 << 31) != 0;
        let ignore_left = world_base_addr & (1 << 30) != 0;
        let ignore_right = world_base_addr & (1 << 29) != 0;
        let ignore_bottom = world_base_addr & (1 << 28) != 0;
        let bits = world_base_addr << 4 >> 30;
        let mut world_base_addr = U8AlignedIndex::new_u32(world_base_addr & 0x03_FF_FF_FF, world_data);
        
        
        let layer_index = start_index + l * RHOMBUS_VERTEX_COUNT;

        let sprite_index = if (ignore_top && is_top) || (ignore_left && is_left) ||
            (ignore_right && is_right) || (ignore_bottom && is_bottom) {
            NO_SPRITE as usize
        } else {
            //+1 is for animation offset. let's consider if we can omit that in the future by a flag
            world_base_addr.skip(instance_index * (bits as usize + 1)); //layer data is continuous per layer
            let mut world_data_addr = world_base_addr;
            let layer_base_addr = layer_lookup_table.inner_as_ref()[l * 4];


            let mut layer_sprite_addr = U32AlignedIndex::new_u32(layer_base_addr + world_data_addr.read_n_bytes(world_data, bits), layer_lookup_table);
            //one after sprite_index is frame offset for the animation
            let anim_frame_offset = world_data_addr.get_u8(world_data);
            let mut sprite_addr = U8AlignedIndex::new_u32(layer_sprite_addr.read_u32(layer_lookup_table), texture_lookup_table);
            
            let anim_frames_count = sprite_addr.read_u8(texture_lookup_table);

            //+1 because first entry is animation frame count
            sprite_addr.skip(((frame_count+ anim_frame_offset) % anim_frames_count) as usize);
            sprite_addr.read_u24(texture_lookup_table) as usize
        };
        
        let was_swapped = vertices_generated[layer_index + 1].w as i32 == -1;
        let need_swapper = sprite_index == NO_SPRITE;

        for i in 0..6 {
            vertices_generated[layer_index + i].w = sprite_index as f32;
        }
        if need_swapper && !was_swapped {
            swap(vertices_generated, layer_index+1, layer_index+2);
            swap(vertices_generated, layer_index+4, layer_index+5);
            //need to last as we swapped 1 and 2
            vertices_generated[layer_index + 1].w = -1.0;
        } else if (!need_swapper) && was_swapped  {
            swap(vertices_generated, layer_index+1, layer_index+2);
            swap(vertices_generated, layer_index+4, layer_index+5);
        }
    }
}

pub fn swap<T: Sized>(arr: &mut [T], i: usize, j: usize) {
    unsafe {
        // Get raw pointers to the elements at indices i and j
        let ptr_i = &mut arr[i] as *mut T;
        let ptr_j = &mut arr[j] as *mut T;

        // Read the value at index i
        let temp = ptr_i.read();

        // Write the value at index j to index i
        ptr_i.write(ptr_j.read());

        // Write the saved value to index j
        ptr_j.write(temp);
    }
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