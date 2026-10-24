//! The game functions we hook, with their exact signatures (from C3X's
//! civ_prog_objects.csv) and per-build locations.

use inproc::Ptr32;

use crate::Build;

inproc::fastcall_hook! {
    /// `void Map_Renderer::m71_Draw_Tiles(Map_Renderer *this, int edx,
    /// int p1, int p2, int p3)` — draws the whole visible map.
    pub mod draw_tiles(map_renderer, edx; p1, p2, p3) -> ();
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
