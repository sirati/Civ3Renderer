//! jgl.dll's object layouts (2004-03-17 build). Pointers and handles are
//! `u32` so that the same structs compile for the native side and in tests.
//!
//! Each field is named from what reads or writes it in jgl's code
//! (`docs/jgl-replacement.md` §3); `pad_*` fields are bytes no one outside
//! jgl touches. The tests check every offset the game, C3X or the bridge
//! uses, and (when `jgl.dll` is at hand) that jgl's code uses the same.

/// `RECT`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Rect32 {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl From<Rect32> for jgl::Rect {
    fn from(r: Rect32) -> jgl::Rect {
        jgl::Rect::new(r.left, r.top, r.right, r.bottom)
    }
}

/// `BITMAPINFOHEADER`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct BitmapInfoHeader {
    pub size: u32,
    pub width: i32,
    pub height: i32,
    pub planes: u16,
    pub bit_count: u16,
    pub compression: u32,
    pub size_image: u32,
    pub x_pels_per_meter: i32,
    pub y_pels_per_meter: i32,
    pub clr_used: u32,
    pub clr_important: u32,
}

/// The graphics object `get_graphsy_object_ptr` returns (0x14C bytes).
#[repr(C)]
pub struct JglGraphics {
    pub vtable: u32,
    pub pad_004: [u32; 74],
    /// +0x12C, +0x130: the screen size (`GetSystemMetrics` at window
    /// creation), slots +0xA8/+0xAC.
    pub screen_width: i32,
    pub screen_height: i32,
    /// +0x134: the depth slot +0xB0 returns (8).
    pub depth: i32,
    /// +0x138: the window's DC (slot +0x3C).
    pub dc: u32,
    pub pad_13c: u32,
    /// +0x140, +0x144: slots +0x24 and +0x20.
    pub hinstance: u32,
    pub hwnd: u32,
    /// +0x148: the window's owner, the back buffer (`create_window`'s third
    /// argument; the first, the controller, goes to a global); its
    /// method 0 is told each new mode (width, height, bits, 1).
    pub owner: u32,
}

/// `CRITICAL_SECTION` (24 bytes).
#[repr(C)]
pub struct CriticalSection(pub [u32; 6]);

/// `JGL_Image`, a surface (0x4D4 bytes); C3X's `Civ3Conquests.h` has the
/// same layout.
#[repr(C)]
pub struct JglImage {
    pub vtable: u32,
    pub field_04: u32,
    pub field_08: u32,
    pub lock: CriticalSection,
    /// +0x24: 8 or 16 (slot +0xE4 returns its address).
    pub bit_count: i32,
    pub field_28: u32,
    /// +0x2C..+0x37: 16-bit channel layout (5, 0xA shifts; masks).
    pub channels: [u16; 6],
    pub field_38: u32,
    pub field_3c: u32,
    /// +0x40: row width in pixels (slot +0xE0).
    pub row_width: i32,
    /// +0x44: the clip (slots +0x34 set, +0xC4 reset, +0xCC address).
    pub clip: Rect32,
    /// +0x54: the whole image (slot +0xD4; +0xD8/+0xDC width, height).
    pub image_rect: Rect32,
    pub pad_64: [u32; 6],
    pub field_7c: u32,
    /// +0x80: the game's `PCX_Image` this belongs to (constructor argument).
    pub owner: u32,
    /// +0x84: the DIB's header, then its colour table (+0xAC, 256 entries;
    /// slot +0xEC sets it).
    pub bitmap_info: BitmapInfoHeader,
    pub colors: [u32; 256],
    /// +0x4AC.
    pub region: u32,
    pub previous_bitmap: u32,
    pub bitmap: u32,
    /// +0x4B8: the DC handed out by slot +0x28.
    pub current_dc: u32,
    /// +0x4BC: the image's own memory DC (`CreateCompatibleDC` in +0x04).
    pub dc: u32,
    /// +0x4C0: the DIB section's bits (slot +0x10 lock).
    pub bits: u32,
    pub dc_locks: u32,
    pub bits_locks: u32,
    pub current_bits: u32,
    /// +0x4D0: the colour key (slots +0xBC/+0xC0).
    pub color_key: u32,
}

/// `JGLSprite` (0x50 bytes).
#[repr(C)]
pub struct JglSprite {
    pub vtable: u32,
    pub field_04: u32,
    pub field_08: u32,
    /// +0x0C: the constructor's argument, the owner: the exe passes its
    /// `Sprite`'s +4 (the field this sprite is kept in).
    pub owner: u32,
    /// +0x10: slots +0x10/+0x14.
    pub color_table: u32,
    /// +0x14: slot +0x20.
    pub field_14: u32,
    /// +0x18: flags.
    pub flags: u32,
    pub field_1c: u32,
    /// +0x20: bits per pixel, 8 or 16 (slot +0x28; the blits dispatch on it).
    pub bits: u32,
    pub field_24: u32,
    /// +0x28: the key index (0xFF; slots +0x18/+0x1C).
    pub key: u32,
    pub field_2c: u32,
    pub field_30: u32,
    pub field_34: u32,
    pub lock: CriticalSection,
}

/// `JGL_Color_Table` (0x820 bytes).
#[repr(C)]
pub struct JglColorTable {
    pub vtable: u32,
    pub field_04: u32,
    pub field_08: u32,
    /// +0x0C: 256 entries.
    pub entries: [u32; 256],
    /// +0x40C, +0x60C: the 16-bit tables (slots +0x18, +0x1C).
    pub table16_a: [u16; 256],
    pub table16_b: [u16; 256],
    /// +0x80C: the constructor's argument, the owner: the exe passes its
    /// `PCX_Color_Table`.
    pub owner: u32,
    /// +0x810/+0x814: fade state (slot +0x08 clears it).
    pub fade: [u32; 2],
    pub field_818: [u32; 2],
}

/// A jgl font (0x24 bytes).
#[repr(C)]
pub struct JglFont {
    pub vtable: u32,
    /// +0x04: metrics the font writes after creation (constructor
    /// argument: +0x08 height, +0x0C ascent, +0x10..+0x18).
    pub metrics: u32,
    /// +0x08: an embedded jgl base object (vtable and list registration).
    pub base_vtable: u32,
    pub base_field: u32,
    pub field_10: u32,
    /// +0x14: bit 0: loaded from a file (+0x04), so +0x08 keeps it.
    pub flags: u32,
    /// +0x18: the HFONT (+0xA8 of an image selects it).
    pub hfont: u32,
    pub field_1c: u32,
    /// +0x20: the font file's resource path (`AddFontResourceA`).
    pub resource: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use jgl::slots::Class;
    use std::mem::{offset_of, size_of};

    #[test]
    fn sizes_are_the_factories_allocations() {
        assert_eq!(size_of::<JglGraphics>() as u32, Class::Graphics.object_size());
        assert_eq!(size_of::<JglImage>() as u32, Class::Image.object_size());
        assert_eq!(size_of::<JglSprite>() as u32, Class::Sprite.object_size());
        assert_eq!(size_of::<JglColorTable>() as u32, Class::ColorTable.object_size());
        assert_eq!(size_of::<JglFont>() as u32, Class::Font.object_size());
    }

    #[test]
    fn the_fields_others_read() {
        // The game, C3X (injected_code.c, Civ3Conquests.h) and civ3-sys.
        assert_eq!(offset_of!(JglImage, bit_count), 0x24);
        assert_eq!(offset_of!(JglImage, row_width), 0x40);
        assert_eq!(offset_of!(JglImage, clip), 0x44);
        assert_eq!(offset_of!(JglImage, image_rect), 0x54);
        assert_eq!(offset_of!(JglImage, field_7c), 0x7C);
        assert_eq!(offset_of!(JglImage, owner), 0x80);
        assert_eq!(offset_of!(JglImage, bitmap_info), 0x84);
        assert_eq!(offset_of!(JglImage, colors), 0xAC);
        assert_eq!(offset_of!(JglImage, region), 0x4AC);
        assert_eq!(offset_of!(JglImage, current_dc), 0x4B8);
        assert_eq!(offset_of!(JglImage, dc), 0x4BC);
        assert_eq!(offset_of!(JglImage, bits), 0x4C0);
        assert_eq!(offset_of!(JglImage, current_bits), 0x4CC);
        assert_eq!(offset_of!(JglImage, color_key), 0x4D0);
        assert_eq!(offset_of!(JglSprite, owner), 0x0C);
        assert_eq!(offset_of!(JglSprite, color_table), 0x10);
        assert_eq!(offset_of!(JglSprite, bits), 0x20);
        assert_eq!(offset_of!(JglSprite, flags), 0x18);
        assert_eq!(offset_of!(JglSprite, key), 0x28);
        assert_eq!(offset_of!(JglSprite, lock), 0x38);
        assert_eq!(offset_of!(JglColorTable, entries), 0x0C);
        assert_eq!(offset_of!(JglColorTable, table16_a), 0x40C);
        assert_eq!(offset_of!(JglColorTable, table16_b), 0x60C);
        assert_eq!(offset_of!(JglColorTable, owner), 0x80C);
        assert_eq!(offset_of!(JglColorTable, fade), 0x810);
        assert_eq!(offset_of!(JglFont, flags), 0x14);
        assert_eq!(offset_of!(JglFont, hfont), 0x18);
        assert_eq!(offset_of!(JglFont, resource), 0x20);
        assert_eq!(offset_of!(JglGraphics, screen_width), 0x12C);
        assert_eq!(offset_of!(JglGraphics, depth), 0x134);
        assert_eq!(offset_of!(JglGraphics, hinstance), 0x140);
        assert_eq!(offset_of!(JglGraphics, hwnd), 0x144);
        assert_eq!(offset_of!(JglGraphics, owner), 0x148);
    }
}
