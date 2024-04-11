#![no_std]

/*use bitflags::bitflags;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct LayerData {
    pub offset_x: f32,
    pub offset_y: f32,
    pub depth: f32,
    pub tiles_x: u8,
    pub tiles_y: u8,
    pub flags: LayerFlags,
    pub texture_atlas_map_id: u8,
    pub data_id: u8,
}


bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    pub struct LayerFlags: u8 {
        const ISOMETRIC = 0x01;
    }
}

pub struct Data {
    pub tiles: [[u8; 256]; 256],
}

pub struct TextureCoord {
    pub u: u8,
    pub v: u8,
}



pub struct TextureAtlasMap {
    pub indices: [TextureCoord; 256],
}
*/
/*#[repr(C)]
pub struct Params {
    pub world_size_x: u16,
    pub world_size_y: u16,
    pub max_texture_y: u32,
    pub layer_count: u8,
}*/

#[repr(C)]
pub struct Params {
    pub world_size_x: u32,
    pub world_size_y: u32,
    pub compute_size_x: u32,
    pub max_texture_y: u32,
    pub layer_count: u32,
    pub four_over_compute_size: f32, 
}


pub const RHOMBUS_VERTEX_COUNT: usize = 6;

pub const NO_SPRITE: usize = 0;
