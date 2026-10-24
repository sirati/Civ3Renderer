//! The game functions we hook, with their exact signatures (from C3X's
//! civ_prog_objects.csv) and per-build locations.

use inproc::Ptr32;

use crate::Build;

inproc::fastcall_hook! {
    /// `int Sprite::draw_on_map(Sprite *this, int edx, Map_Renderer *mr,
    /// int pixel_x, int pixel_y, int p4, int p5, int p6, int p7)`
    pub mod sprite_draw_on_map(sprite, edx; map_renderer, pixel_x, pixel_y, p4, p5, p6, p7) -> i32;
}

inproc::fastcall_hook! {
    /// A sprite drawn through a blend table (resource shadows):
    /// `Sprite *this, Map_Renderer *mr, x, y, table, p5, p6, zoom, p8` (8 stack args).
    pub mod sprite_draw_on_map_blend(sprite, edx; map_renderer, pixel_x, pixel_y, table, p5, p6, p7, p8) -> i32;
}

inproc::fastcall_hook! {
    /// A sprite recoloured with a civ colour table (borders, unit flags):
    /// `Sprite *this, canvas, canvas2, x, y, table, p6` (meaning of the
    /// second canvas and p6 not decoded).
    pub mod sprite_draw_with_color_table(sprite, edx; canvas, canvas2, pixel_x, pixel_y, table, p6) -> i32;
}

inproc::fastcall_hook! {
    /// `void Map_Renderer::m71_Draw_Tiles(Map_Renderer *this, int edx,
    /// int p1, int p2, int p3)` — draws the whole visible map.
    pub mod draw_tiles(map_renderer, edx; p1, p2, p3) -> ();
}

/// Where `Sprite_draw_on_map` starts, and its first instructions (checked
/// before patching). Only verified on the builds listed.
pub fn sprite_draw_on_map_site(build: Build) -> Option<(u32, &'static [u8])> {
    match build {
        // sub esp,0Ch / push esi / push edi
        Build::PcGames => Some((0x5F_8060, &[0x83, 0xEC, 0x0C, 0x56, 0x57])),
        Build::Gog | Build::Steam => None,
    }
}

/// `Sprite_draw_on_map` with a blend table (8 stack args).
pub fn sprite_draw_on_map_blend_site(build: Build) -> Option<(u32, &'static [u8])> {
    match build {
        // sub esp,0Ch / lea edx,[esp+0]
        Build::PcGames => Some((0x5F_8560, &[0x83, 0xEC, 0x0C, 0x8D, 0x54, 0x24, 0x00])),
        Build::Gog | Build::Steam => None,
    }
}

/// `Sprite_draw_with_color_table` (6 stack args).
pub fn sprite_draw_with_color_table_site(build: Build) -> Option<(u32, &'static [u8])> {
    match build {
        // mov eax,[ecx+8] / push esi / test eax,eax
        Build::PcGames => Some((0x5F_87E0, &[0x8B, 0x41, 0x08, 0x56, 0x85, 0xC0])),
        Build::Gog | Build::Steam => None,
    }
}

/// The global per-tech table `Leader_has_tech` reads: a pointer to one
/// dword per tech, bit `civ` set if that civ knows it; and the "no tech"
/// sentinel id that is never known.
pub fn tech_table(build: Build) -> Option<(Ptr32<Ptr32<u32>>, Ptr32<i32>)> {
    match build {
        Build::PcGames => Some((Ptr32::new(0xA5_2B0C), Ptr32::new(0x9C_3D7C))),
        Build::Gog | Build::Steam => None,
    }
}

/// The `Map_Renderer` vtable slot of `m71_Draw_Tiles`.
pub fn draw_tiles_slot(build: Build) -> Ptr32<u32> {
    Ptr32::new(match build {
        Build::Gog | Build::PcGames => 0x66_A624,
        Build::Steam => 0x68_7730,
    })
}

/// How near its view's edge the game's pointer scrolls the map. The game
/// scrolls on a 66 ms timer (`Main_Screen_Form` at 0x4DE4F0 in PcGames)
/// while a window of its own has the keyboard focus (the executable's only
/// `GetFocus` call) and its pointer — as `jgl.dll` gets it from
/// `GetCursorPos` (that library's only call; its other users place tooltips
/// and repeat held buttons), relative to the game's window — is within this
/// many pixels of an edge of the view, faster nearer; it never looks at the
/// mouse messages it is sent.
pub const EDGE_SCROLL_ZONE: i32 = 16;
