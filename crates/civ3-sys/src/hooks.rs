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

/// The `Map_Renderer` vtable slot of `m71_Draw_Tiles`.
pub fn draw_tiles_slot(build: Build) -> Ptr32<u32> {
    Ptr32::new(match build {
        Build::Gog | Build::PcGames => 0x66_A624,
        Build::Steam => 0x68_7730,
    })
}
