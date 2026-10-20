//! GPU-generated, colour-independent glyph coverage. No cached value includes
//! a background colour, contrast boost, colour quantization or blending.

/// One glyph instance's cached coverage rectangle. Coordinates in `local_*`
/// are relative to the integer part of its placed baseline. A negative
/// `atlas_x` selects direct outline evaluation instead.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "spirv"), derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct CacheGlyph {
    pub atlas_x: i32,
    pub atlas_y: i32,
    pub local_left: i32,
    pub local_top: i32,
    /// GPU-written readiness stamp. A mismatch selects direct outlines.
    pub ready_index: u32,
    pub ready_tag: u32,
    pub _pad0: u32,
    pub _pad1: u32,
}

impl Default for CacheGlyph {
    fn default() -> Self {
        Self { atlas_x: -1, atlas_y: 0, local_left: 0, local_top: 0, ready_index: 0, ready_tag: 0, _pad0: 0, _pad1: 0 }
    }
}

/// A GPU atlas fill. `x`/`y` are the placed baseline modulo TEXT_FIXED;
/// `left`/`top` and `width`/`height` bound every texel written by the fill.
/// Glyph and face offsets address the current immutable outline buffer.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "spirv"), derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct CacheJob {
    pub atlas_x: u32,
    pub atlas_y: u32,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale: i32,
    pub bold: i32,
    pub flags: u32,
    pub glyph: u32,
    pub face: u32,
    pub _pad0: u32,
    pub bar_w: i32,
    pub bar_top: i32,
    pub bar_h: i32,
    pub _pad1: u32,
    pub left: i32,
    pub top: i32,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub ready_index: u32,
    pub ready_tag: u32,
    pub _pad2: u32,
    pub _pad3: u32,
}
