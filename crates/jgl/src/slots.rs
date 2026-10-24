//! Every virtual method of jgl.dll's five classes: its slot offset, where
//! jgl of 2004-03-17 has it (RVA), how many argument bytes it pops, a name
//! where its body says what it does, and the method group that takes it
//! over (`docs/jgl-replacement.md` §6).
//!
//! Generated from `jgl.dll` (first `ret` of each body; names from the
//! bodies and `docs/rerender-roadmap.md` §1.2, `slot_xx` where unnamed).
//! `jgl-ffi`'s tests check the RVAs and sizes against the binary.

/// One of jgl's object classes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Graphics,
    Image,
    Sprite,
    ColorTable,
    Font,
}

impl Class {
    pub const ALL: [Class; 5] = [Class::Graphics, Class::Image, Class::Sprite, Class::ColorTable, Class::Font];

    /// The class's vtable, relative to jgl.dll's base.
    pub const fn vtable_rva(self) -> u32 {
        match self {
            Class::Graphics => 0x6_85F8,
            Class::Image => 0x6_8238,
            Class::Sprite => 0x6_8440,
            Class::ColorTable => 0x6_8938,
            Class::Font => 0x6_88AC,
        }
    }

    /// The size of one object (what jgl's factory allocates).
    pub const fn object_size(self) -> u32 {
        match self {
            Class::Graphics => 0x14C,
            Class::Image => 0x4D4,
            Class::Sprite => 0x50,
            Class::ColorTable => 0x820,
            Class::Font => 0x24,
        }
    }

    pub fn slots(self) -> &'static [Slot] {
        match self {
            Class::Graphics => GRAPHICS,
            Class::Image => IMAGE,
            Class::Sprite => SPRITE,
            Class::ColorTable => COLOR_TABLE,
            Class::Font => FONT,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Class::Graphics => "graphics",
            Class::Image => "image",
            Class::Sprite => "sprite",
            Class::ColorTable => "color_table",
            Class::Font => "font",
        }
    }

    /// The slot at byte offset `offset` of the vtable.
    pub fn slot(self, offset: u16) -> Option<&'static Slot> {
        self.slots().get(offset as usize / 4).filter(|s| s.offset == offset)
    }
}

/// The method group a slot belongs to: groups are taken over as a whole.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    /// jgl's own "not implemented" bodies (`mov eax, 0x18; ret n`).
    Stub,
    /// Fonts, text colour, text drawing.
    Text,
    /// Fills, shading, lines, frames.
    Fill,
    /// Image to image copies.
    Copy,
    /// Sprites and their blits.
    Blit,
    /// Colour tables.
    Palette,
    /// Invalidate and present.
    Present,
    /// Object creation, pixel access, clip, device contexts, getters.
    Surface,
    /// The window, message pumps, pointer, display modes.
    Window,
}

/// One virtual method.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    /// Byte offset in the vtable.
    pub offset: u16,
    /// Where jgl.dll of 2004-03-17 has the body, relative to its base.
    pub rva: u32,
    /// Argument bytes the method pops (`ret n`), `this` (ecx) not counted.
    pub stack: u8,
    pub name: &'static str,
    pub group: Group,
}

pub const GRAPHICS: &[Slot] = &[
    Slot { offset: 0x000, rva: 0x3b360, stack: 4, name: "scalar_delete", group: Group::Window },
    Slot { offset: 0x004, rva: 0x3b400, stack: 16, name: "create_window", group: Group::Window },
    Slot { offset: 0x008, rva: 0x3b390, stack: 0, name: "destroy_window", group: Group::Window },
    Slot { offset: 0x00c, rva: 0x3b2c0, stack: 0, name: "release", group: Group::Window },
    Slot { offset: 0x010, rva: 0x3bea0, stack: 0, name: "show_window_a", group: Group::Window },
    Slot { offset: 0x014, rva: 0x3beb0, stack: 0, name: "show_window_b", group: Group::Window },
    Slot { offset: 0x018, rva: 0x3bec0, stack: 0, name: "show_window_c", group: Group::Window },
    Slot { offset: 0x01c, rva: 0x3bed0, stack: 0, name: "show_maximized", group: Group::Window },
    Slot { offset: 0x020, rva: 0x3be80, stack: 0, name: "hwnd", group: Group::Window },
    Slot { offset: 0x024, rva: 0x3be90, stack: 0, name: "hinstance", group: Group::Window },
    Slot { offset: 0x028, rva: 0x3bc10, stack: 16, name: "set_window_pos", group: Group::Window },
    Slot { offset: 0x02c, rva: 0x3bbd0, stack: 8, name: "cursor_pos", group: Group::Window },
    Slot { offset: 0x030, rva: 0x3bbb0, stack: 4, name: "invalidate", group: Group::Present },
    Slot { offset: 0x034, rva: 0x3bb90, stack: 8, name: "screen_size", group: Group::Window },
    Slot { offset: 0x038, rva: 0x3bb70, stack: 4, name: "window_rect", group: Group::Window },
    Slot { offset: 0x03c, rva: 0x3b7a0, stack: 0, name: "dc", group: Group::Window },
    Slot { offset: 0x040, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x044, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x048, rva: 0x3bf10, stack: 0, name: "zero_48", group: Group::Window },
    Slot { offset: 0x04c, rva: 0x3c6b0, stack: 0, name: "nop_4c", group: Group::Window },
    Slot { offset: 0x050, rva: 0x3b7b0, stack: 12, name: "set_mode_fastest", group: Group::Window },
    Slot { offset: 0x054, rva: 0x3b9b0, stack: 4, name: "set_depth", group: Group::Window },
    Slot { offset: 0x058, rva: 0x3b870, stack: 12, name: "set_mode", group: Group::Window },
    Slot { offset: 0x05c, rva: 0x3b900, stack: 20, name: "set_mode_refresh", group: Group::Window },
    Slot { offset: 0x060, rva: 0x3b810, stack: 16, name: "set_mode_keep", group: Group::Window },
    Slot { offset: 0x064, rva: 0x3b9e0, stack: 4, name: "set_mode_index", group: Group::Window },
    Slot { offset: 0x068, rva: 0x3b560, stack: 0, name: "display_modes", group: Group::Window },
    Slot { offset: 0x06c, rva: 0x3bc40, stack: 8, name: "set_display_size", group: Group::Window },
    Slot { offset: 0x070, rva: 0x3bcb0, stack: 0, name: "restore_display_mode", group: Group::Window },
    Slot { offset: 0x074, rva: 0x3bf20, stack: 4, name: "new_font", group: Group::Surface },
    Slot { offset: 0x078, rva: 0x3bf80, stack: 4, name: "new_color_table", group: Group::Surface },
    Slot { offset: 0x07c, rva: 0x3bff0, stack: 8, name: "new_image", group: Group::Surface },
    Slot { offset: 0x080, rva: 0x3c060, stack: 8, name: "new_sprite", group: Group::Surface },
    Slot { offset: 0x084, rva: 0x3bde0, stack: 4, name: "zero_84", group: Group::Surface },
    Slot { offset: 0x088, rva: 0x3c0c0, stack: 4, name: "delete_font", group: Group::Surface },
    Slot { offset: 0x08c, rva: 0x3c0c0, stack: 4, name: "delete_color_table", group: Group::Surface },
    Slot { offset: 0x090, rva: 0x3c0c0, stack: 4, name: "delete_image", group: Group::Surface },
    Slot { offset: 0x094, rva: 0x3c0c0, stack: 4, name: "delete_sprite", group: Group::Surface },
    Slot { offset: 0x098, rva: 0x3be50, stack: 4, name: "nop_98", group: Group::Window },
    Slot { offset: 0x09c, rva: 0x3c0e0, stack: 12, name: "set_scale", group: Group::Surface },
    Slot { offset: 0x0a0, rva: 0x3c100, stack: 12, name: "get_scale", group: Group::Surface },
    Slot { offset: 0x0a4, rva: 0x3baa0, stack: 4, name: "present", group: Group::Present },
    Slot { offset: 0x0a8, rva: 0x3bee0, stack: 0, name: "screen_width", group: Group::Window },
    Slot { offset: 0x0ac, rva: 0x3bef0, stack: 0, name: "screen_height", group: Group::Window },
    Slot { offset: 0x0b0, rva: 0x3bf00, stack: 0, name: "screen_depth", group: Group::Window },
    Slot { offset: 0x0b4, rva: 0x3bf10, stack: 0, name: "pixel_format", group: Group::Window },
    Slot { offset: 0x0b8, rva: 0x3c130, stack: 0, name: "create_compatible_dc", group: Group::Window },
    Slot { offset: 0x0bc, rva: 0x3c140, stack: 0, name: "delete_dc", group: Group::Window },
    Slot { offset: 0x0c0, rva: 0x3c150, stack: 8, name: "create_palette", group: Group::Window },
    Slot { offset: 0x0c4, rva: 0x3c170, stack: 0, name: "delete_palette", group: Group::Window },
    Slot { offset: 0x0c8, rva: 0x3bdf0, stack: 0, name: "stub", group: Group::Stub },
    Slot { offset: 0x0cc, rva: 0x3bdf0, stack: 0, name: "stub", group: Group::Stub },
    Slot { offset: 0x0d0, rva: 0x3bdf0, stack: 0, name: "stub", group: Group::Stub },
    Slot { offset: 0x0d4, rva: 0x3be10, stack: 8, name: "stub", group: Group::Stub },
    Slot { offset: 0x0d8, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x0dc, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x0e0, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x0e4, rva: 0x3be00, stack: 12, name: "stub", group: Group::Stub },
    Slot { offset: 0x0e8, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x0ec, rva: 0x3be10, stack: 8, name: "stub", group: Group::Stub },
    Slot { offset: 0x0f0, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x0f4, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x0f8, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x0fc, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x100, rva: 0x3be30, stack: 24, name: "stub", group: Group::Stub },
    Slot { offset: 0x104, rva: 0x3be40, stack: 16, name: "stub", group: Group::Stub },
    Slot { offset: 0x108, rva: 0x3be10, stack: 8, name: "stub", group: Group::Stub },
    Slot { offset: 0x10c, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x110, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x114, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x118, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x11c, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x120, rva: 0x3d850, stack: 0, name: "pump_120", group: Group::Window },
    Slot { offset: 0x124, rva: 0x3d430, stack: 0, name: "wait_message", group: Group::Window },
    Slot { offset: 0x128, rva: 0x3d450, stack: 0, name: "pump_128", group: Group::Window },
    Slot { offset: 0x12c, rva: 0x3d4c0, stack: 0, name: "pump_12c", group: Group::Window },
    Slot { offset: 0x130, rva: 0x3d5a0, stack: 0, name: "slot_130", group: Group::Window },
    Slot { offset: 0x134, rva: 0x3d5e0, stack: 0, name: "slot_134", group: Group::Window },
    Slot { offset: 0x138, rva: 0x3d750, stack: 0, name: "slot_138", group: Group::Window },
    Slot { offset: 0x13c, rva: 0x3d7a0, stack: 0, name: "slot_13c", group: Group::Window },
    Slot { offset: 0x140, rva: 0x3d630, stack: 0, name: "slot_140", group: Group::Window },
    Slot { offset: 0x144, rva: 0x3d680, stack: 0, name: "slot_144", group: Group::Window },
    Slot { offset: 0x148, rva: 0x3d7f0, stack: 0, name: "pump_148", group: Group::Window },
    Slot { offset: 0x14c, rva: 0x3d820, stack: 0, name: "pump_14c", group: Group::Window },
    Slot { offset: 0x150, rva: 0x3d8a0, stack: 0, name: "slot_150", group: Group::Window },
    Slot { offset: 0x154, rva: 0x3c180, stack: 4, name: "set_global_154", group: Group::Window },
];

pub const IMAGE: &[Slot] = &[
    Slot { offset: 0x000, rva: 0x015d0, stack: 4, name: "scalar_delete", group: Group::Surface },
    Slot { offset: 0x004, rva: 0x01800, stack: 16, name: "create", group: Group::Surface },
    Slot { offset: 0x008, rva: 0x01780, stack: 0, name: "clear", group: Group::Surface },
    Slot { offset: 0x00c, rva: 0x01bc0, stack: 8, name: "pixel_at", group: Group::Surface },
    Slot { offset: 0x010, rva: 0x01b70, stack: 0, name: "lock", group: Group::Surface },
    Slot { offset: 0x014, rva: 0x02630, stack: 0, name: "unlock_14", group: Group::Surface },
    Slot { offset: 0x018, rva: 0x02620, stack: 0, name: "pixel_at_18", group: Group::Surface },
    Slot { offset: 0x01c, rva: 0x02630, stack: 0, name: "unlock_1c", group: Group::Surface },
    Slot { offset: 0x020, rva: 0x02620, stack: 0, name: "pixel_at_20", group: Group::Surface },
    Slot { offset: 0x024, rva: 0x01b90, stack: 4, name: "read_pixel", group: Group::Surface },
    Slot { offset: 0x028, rva: 0x01b20, stack: 0, name: "acquire_dc", group: Group::Surface },
    Slot { offset: 0x02c, rva: 0x01b40, stack: 4, name: "release_dc", group: Group::Surface },
    Slot { offset: 0x030, rva: 0x3be50, stack: 4, name: "nop_30", group: Group::Surface },
    Slot { offset: 0x034, rva: 0x01a40, stack: 4, name: "set_clip", group: Group::Surface },
    Slot { offset: 0x038, rva: 0x01e20, stack: 32, name: "copy_38", group: Group::Copy },
    Slot { offset: 0x03c, rva: 0x01fa0, stack: 28, name: "copy_3c", group: Group::Copy },
    Slot { offset: 0x040, rva: 0x01ec0, stack: 12, name: "copy", group: Group::Copy },
    Slot { offset: 0x044, rva: 0x02270, stack: 8, name: "fill", group: Group::Fill },
    Slot { offset: 0x048, rva: 0x02320, stack: 12, name: "shade_rect", group: Group::Fill },
    Slot { offset: 0x04c, rva: 0x02360, stack: 8, name: "line_4c", group: Group::Fill },
    Slot { offset: 0x050, rva: 0x023d0, stack: 12, name: "line_50", group: Group::Fill },
    Slot { offset: 0x054, rva: 0x022c0, stack: 16, name: "line_54", group: Group::Fill },
    Slot { offset: 0x058, rva: 0x02070, stack: 12, name: "replace_color", group: Group::Fill },
    Slot { offset: 0x05c, rva: 0x02110, stack: 8, name: "frame_rect", group: Group::Fill },
    Slot { offset: 0x060, rva: 0x021e0, stack: 36, name: "line_60", group: Group::Fill },
    Slot { offset: 0x064, rva: 0x02160, stack: 24, name: "line", group: Group::Fill },
    Slot { offset: 0x068, rva: 0x3be40, stack: 16, name: "stub", group: Group::Stub },
    Slot { offset: 0x06c, rva: 0x3be40, stack: 16, name: "stub", group: Group::Stub },
    Slot { offset: 0x070, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x074, rva: 0x07bb0, stack: 16, name: "draw_74", group: Group::Copy },
    Slot { offset: 0x078, rva: 0x025d0, stack: 4, name: "set_78", group: Group::Surface },
    Slot { offset: 0x07c, rva: 0x3be20, stack: 4, name: "stub", group: Group::Stub },
    Slot { offset: 0x080, rva: 0x3be40, stack: 16, name: "stub", group: Group::Stub },
    Slot { offset: 0x084, rva: 0x02410, stack: 12, name: "keyed_draw_onto_84", group: Group::Copy },
    Slot { offset: 0x088, rva: 0x3be40, stack: 16, name: "stub", group: Group::Stub },
    Slot { offset: 0x08c, rva: 0x024f0, stack: 16, name: "keyed_draw_onto_8c", group: Group::Copy },
    Slot { offset: 0x090, rva: 0x3be40, stack: 16, name: "stub", group: Group::Stub },
    Slot { offset: 0x094, rva: 0x3be40, stack: 16, name: "stub", group: Group::Stub },
    Slot { offset: 0x098, rva: 0x3be40, stack: 16, name: "stub", group: Group::Stub },
    Slot { offset: 0x09c, rva: 0x3be40, stack: 16, name: "stub", group: Group::Stub },
    Slot { offset: 0x0a0, rva: 0x3be40, stack: 16, name: "stub", group: Group::Stub },
    Slot { offset: 0x0a4, rva: 0x3be40, stack: 16, name: "stub", group: Group::Stub },
    Slot { offset: 0x0a8, rva: 0x01d10, stack: 4, name: "select_font", group: Group::Text },
    Slot { offset: 0x0ac, rva: 0x01d40, stack: 0, name: "deselect_font", group: Group::Text },
    Slot { offset: 0x0b0, rva: 0x01db0, stack: 4, name: "set_text_color16", group: Group::Text },
    Slot { offset: 0x0b4, rva: 0x01d70, stack: 12, name: "set_text_color", group: Group::Text },
    Slot { offset: 0x0b8, rva: 0x01de0, stack: 16, name: "text_out", group: Group::Text },
    Slot { offset: 0x0bc, rva: 0x02600, stack: 4, name: "set_color_key", group: Group::Surface },
    Slot { offset: 0x0c0, rva: 0x02610, stack: 0, name: "color_key", group: Group::Surface },
    Slot { offset: 0x0c4, rva: 0x02640, stack: 0, name: "reset_clip", group: Group::Surface },
    Slot { offset: 0x0c8, rva: 0x02650, stack: 4, name: "get_c8", group: Group::Surface },
    Slot { offset: 0x0cc, rva: 0x02670, stack: 0, name: "clip_rect", group: Group::Surface },
    Slot { offset: 0x0d0, rva: 0x02680, stack: 4, name: "get_d0", group: Group::Surface },
    Slot { offset: 0x0d4, rva: 0x026a0, stack: 0, name: "image_rect", group: Group::Surface },
    Slot { offset: 0x0d8, rva: 0x026b0, stack: 0, name: "width", group: Group::Surface },
    Slot { offset: 0x0dc, rva: 0x026c0, stack: 0, name: "height", group: Group::Surface },
    Slot { offset: 0x0e0, rva: 0x026d0, stack: 0, name: "row_width", group: Group::Surface },
    Slot { offset: 0x0e4, rva: 0x026e0, stack: 0, name: "format", group: Group::Surface },
    Slot { offset: 0x0e8, rva: 0x026f0, stack: 0, name: "field_7c", group: Group::Surface },
    Slot { offset: 0x0ec, rva: 0x01ca0, stack: 4, name: "set_color_table", group: Group::Surface },
];

pub const SPRITE: &[Slot] = &[
    Slot { offset: 0x000, rva: 0x07e60, stack: 4, name: "scalar_delete", group: Group::Blit },
    Slot { offset: 0x004, rva: 0x07fc0, stack: 24, name: "init", group: Group::Blit },
    Slot { offset: 0x008, rva: 0x07f40, stack: 0, name: "clear", group: Group::Blit },
    Slot { offset: 0x00c, rva: 0x09eb0, stack: 4, name: "duplicate", group: Group::Blit },
    Slot { offset: 0x010, rva: 0x09710, stack: 4, name: "set_color_table", group: Group::Blit },
    Slot { offset: 0x014, rva: 0x09720, stack: 0, name: "color_table", group: Group::Blit },
    Slot { offset: 0x018, rva: 0x09730, stack: 4, name: "set_key", group: Group::Blit },
    Slot { offset: 0x01c, rva: 0x09740, stack: 0, name: "key", group: Group::Blit },
    Slot { offset: 0x020, rva: 0x098a0, stack: 0, name: "field_14", group: Group::Blit },
    Slot { offset: 0x024, rva: 0x3be50, stack: 4, name: "nop_24", group: Group::Blit },
    Slot { offset: 0x028, rva: 0x098b0, stack: 0, name: "field_20", group: Group::Blit },
    Slot { offset: 0x02c, rva: 0x098c0, stack: 0, name: "field_2c", group: Group::Blit },
    Slot { offset: 0x030, rva: 0x098d0, stack: 0, name: "field_30", group: Group::Blit },
    Slot { offset: 0x034, rva: 0x098e0, stack: 0, name: "field_34", group: Group::Blit },
    Slot { offset: 0x038, rva: 0x08030, stack: 8, name: "slot_38", group: Group::Blit },
    Slot { offset: 0x03c, rva: 0x08b10, stack: 20, name: "slot_3c", group: Group::Blit },
    Slot { offset: 0x040, rva: 0x089d0, stack: 20, name: "slot_40", group: Group::Blit },
    Slot { offset: 0x044, rva: 0x08180, stack: 16, name: "slot_44", group: Group::Blit },
    Slot { offset: 0x048, rva: 0x09950, stack: 24, name: "slot_48", group: Group::Blit },
    Slot { offset: 0x04c, rva: 0x099c0, stack: 24, name: "slot_4c", group: Group::Blit },
    Slot { offset: 0x050, rva: 0x09aa0, stack: 24, name: "slot_50", group: Group::Blit },
    Slot { offset: 0x054, rva: 0x09a30, stack: 20, name: "slot_54", group: Group::Blit },
    Slot { offset: 0x058, rva: 0x09b20, stack: 20, name: "slot_58", group: Group::Blit },
    Slot { offset: 0x05c, rva: 0x08050, stack: 16, name: "slot_5c", group: Group::Blit },
    Slot { offset: 0x060, rva: 0x08db0, stack: 24, name: "slot_60", group: Group::Blit },
    Slot { offset: 0x064, rva: 0x08ca0, stack: 24, name: "slot_64", group: Group::Blit },
    Slot { offset: 0x068, rva: 0x08be0, stack: 24, name: "slot_68", group: Group::Blit },
    Slot { offset: 0x06c, rva: 0x08860, stack: 20, name: "slot_6c", group: Group::Blit },
    Slot { offset: 0x070, rva: 0x08750, stack: 20, name: "slot_70", group: Group::Blit },
    Slot { offset: 0x074, rva: 0x08600, stack: 20, name: "slot_74", group: Group::Blit },
    Slot { offset: 0x078, rva: 0x098f0, stack: 20, name: "slot_78", group: Group::Blit },
    Slot { offset: 0x07c, rva: 0x08ee0, stack: 20, name: "slot_7c", group: Group::Blit },
    Slot { offset: 0x080, rva: 0x08e30, stack: 20, name: "slot_80", group: Group::Blit },
    Slot { offset: 0x084, rva: 0x08fc0, stack: 20, name: "slot_84", group: Group::Blit },
    Slot { offset: 0x088, rva: 0x090e0, stack: 36, name: "slot_88", group: Group::Blit },
    Slot { offset: 0x08c, rva: 0x090a0, stack: 24, name: "slot_8c", group: Group::Blit },
    Slot { offset: 0x090, rva: 0x09140, stack: 20, name: "slot_90", group: Group::Blit },
    Slot { offset: 0x094, rva: 0x09220, stack: 24, name: "slot_94", group: Group::Blit },
    Slot { offset: 0x098, rva: 0x09750, stack: 20, name: "slot_98", group: Group::Blit },
    Slot { offset: 0x09c, rva: 0x08310, stack: 28, name: "slot_9c", group: Group::Blit },
    Slot { offset: 0x0a0, rva: 0x08490, stack: 24, name: "slot_a0", group: Group::Blit },
];

pub const COLOR_TABLE: &[Slot] = &[
    Slot { offset: 0x000, rva: 0x3cf10, stack: 4, name: "scalar_delete", group: Group::Palette },
    Slot { offset: 0x004, rva: 0x3d090, stack: 0, name: "init", group: Group::Palette },
    Slot { offset: 0x008, rva: 0x3cfa0, stack: 0, name: "reset_fade", group: Group::Palette },
    Slot { offset: 0x00c, rva: 0x3d170, stack: 0, name: "animate", group: Group::Palette },
    Slot { offset: 0x010, rva: 0x3d190, stack: 12, name: "get_colors", group: Group::Palette },
    Slot { offset: 0x014, rva: 0x3d1d0, stack: 12, name: "set_colors", group: Group::Palette },
    Slot { offset: 0x018, rva: 0x3d410, stack: 0, name: "table16_a", group: Group::Palette },
    Slot { offset: 0x01c, rva: 0x3d420, stack: 0, name: "table16_b", group: Group::Palette },
    Slot { offset: 0x020, rva: 0x3d370, stack: 4, name: "slot_20", group: Group::Palette },
];

pub const FONT: &[Slot] = &[
    Slot { offset: 0x000, rva: 0x3c6f0, stack: 4, name: "scalar_delete", group: Group::Text },
    Slot { offset: 0x004, rva: 0x3c3e0, stack: 16, name: "load_file", group: Group::Text },
    Slot { offset: 0x008, rva: 0x3c270, stack: 12, name: "create", group: Group::Text },
    Slot { offset: 0x00c, rva: 0x3c5b0, stack: 0, name: "release", group: Group::Text },
    Slot { offset: 0x010, rva: 0x3c600, stack: 8, name: "measure", group: Group::Text },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_have_every_slot_in_order() {
        for (class, n) in [(Class::Graphics, 86), (Class::Image, 60), (Class::Sprite, 41), (Class::ColorTable, 9), (Class::Font, 5)] {
            let slots = class.slots();
            assert_eq!(slots.len(), n, "{class:?}");
            assert!(slots.iter().enumerate().all(|(i, s)| s.offset as usize == i * 4), "{class:?}");
        }
    }

    #[test]
    fn jgl_stubs_24_graphics_and_12_image_slots() {
        let stubs = |c: Class| c.slots().iter().filter(|s| s.group == Group::Stub).count();
        assert_eq!(stubs(Class::Graphics), 24);
        assert_eq!(stubs(Class::Image), 12);
    }

    #[test]
    fn the_text_slots() {
        let s = Class::Image.slot(0xB8).unwrap();
        assert_eq!((s.name, s.stack, s.group), ("text_out", 16, Group::Text));
        assert_eq!(Class::Font.slot(0x08).unwrap().stack, 12);
        assert_eq!(Class::Image.slot(0xB9), None);
    }
}
