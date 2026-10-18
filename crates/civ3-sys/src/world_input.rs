//! Native map-input entry points for the renderer's virtual world pointer.
//!
//! The renderer can draw a tile that is outside the game's current camera.
//! Civ3's map form normally converts a *client* pixel to a tile at
//! [`get_tile_coords_under_mouse`] (0x4E3D20), then uses that tile for hover,
//! selection, and commands.  This module provides the narrow hook and the
//! map-form event entry points needed to keep those native rules while the
//! renderer supplies the tile.
//!
//! The caller must invoke the event functions on Civ3's game thread, while
//! the map form is the active form and no modal dialog or animation pump is
//! running.  The functions do not post Windows messages or perform that
//! scheduling check themselves.  This is intentional: calling the Base_Form
//! event entry points from another thread can corrupt Civ3's form tree.

use std::sync::atomic::{AtomicU32, Ordering};

use inproc::{self, Ptr32};

/// The PCGames/v1.22 addresses.  They are deliberately kept together so a
/// build check can reject a mismatched executable before any call is made.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sites {
    /// `Main_Screen_Form::get_tile_coords_under_mouse(this, x, y, out_x,
    /// out_y)`, a `thiscall` function with a 16-byte callee-popped stack.
    pub get_tile_coords_under_mouse: u32,
    /// `Base_Form::m84_On_Mouse_Left_Down` for Main_Screen_Form.
    pub left_down: u32,
    /// `Base_Form::m85_On_Mouse_Left_Up` for Main_Screen_Form.
    pub left_up: u32,
    /// `Base_Form::m86_On_Mouse_Right_Down` for Main_Screen_Form.
    pub right_down: u32,
    /// `Base_Form::m87_On_Mouse_Right_Up` for Main_Screen_Form.
    pub right_up: u32,
    /// Main_Screen_Form's vtable m27, `process_mouse_hover`.
    pub hover: u32,
    /// Main_Screen_Form::bring_tile_into_view.
    pub bring_tile_into_view: u32,
    /// Main_Screen_Form::handle_key_down.
    pub handle_key_down: u32,
}

/// PCGames/v1.22 sites.  GOG and Steam have different layouts and are not
/// safe to call through these addresses.
pub const PCGAMES: Sites = Sites {
    get_tile_coords_under_mouse: 0x4E_3D20,
    left_down: 0x60_89_60,
    left_up: 0x60_8A40,
    right_down: 0x60_8AF0,
    right_up: 0x60_8B70,
    hover: 0x4E_B7D0,
    bring_tile_into_view: 0x4D_FA_A0,
    handle_key_down: 0x4E_06B0,
};

/// Exact bytes checked before the call or hook.  These include complete
/// instructions only, and are long enough to reject an incompatible build.
pub const GET_TILE_PROLOGUE: &[u8] = &[0x83, 0xEC, 0x08, 0xA1, 0x2C, 0x73, 0x9C, 0x00];
pub const HOVER_PROLOGUE: &[u8] = &[0x55, 0x8B, 0x6C, 0x24, 0x0C, 0x56, 0x57];
pub const LEFT_DOWN_PROLOGUE: &[u8] = &[0x51, 0x53, 0x55, 0x56, 0x57, 0x8B, 0xF1];
pub const LEFT_UP_PROLOGUE: &[u8] = &[
    0xA1, 0x70, 0xD3, 0xCA, 0x00, 0x83, 0xEC, 0x10, 0x53, 0x56, 0x57,
];
pub const RIGHT_DOWN_PROLOGUE: &[u8] = &[
    0x53, 0x55, 0x56, 0x57, 0x8B, 0xF1, 0xE8, 0x45, 0xEF, 0xFF, 0xFF,
];
pub const RIGHT_UP_PROLOGUE: &[u8] = LEFT_UP_PROLOGUE;
/// `push ebx; mov ebx,ecx; mov ecx,0x7409c0` at 0x4DFAA0.
pub const BRING_TILE_PROLOGUE: &[u8] = &[0x53, 0x8B, 0xD9, 0xB9, 0xC0, 0x09, 0x74, 0x00];
/// `sub esp,0x0c; push ebx; push ebp; push esi; mov esi,ecx`.
pub const KEY_PROLOGUE: &[u8] = &[0x83, 0xEC, 0x0C, 0x53, 0x55, 0x56, 0x8B, 0xF1];
/// PCGames global focus-city pointer, non-zero while City_Form owns focus.
pub const FOCUS_CITY_SLOT: u32 = 0x9C_B3F4;

/// The map form's Main_Screen_Form vtable in PCGames.  The left-down slot is
/// replaced by C3X in the patched executable, so callers should use the Base
/// event wrappers below rather than hard-coding a leaf function.
pub const MAIN_SCREEN_VTABLE: u32 = 0x66_B3D8;
pub const VTABLE_HOVER: u32 = 0x6C;
pub const VTABLE_LEFT_UP: u32 = 0x74;
pub const VTABLE_RIGHT_UP: u32 = 0x7C;
pub const VTABLE_RIGHT_DOWN: u32 = 0x80;
pub const VTABLE_LEFT_DOUBLE: u32 = 0x98;

// These are configured before installing the inline hook. Atomics keep the
// hook's fast path lock-free and make clearing the virtual pointer safe when
// a renderer frame ends between game-thread calls.
static MAIN_SCREEN_FORM: AtomicU32 = AtomicU32::new(0);
static VIRTUAL_TILE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static VIRTUAL_SET: AtomicU32 = AtomicU32::new(0);
static SUPPRESS_RECENTER: AtomicU32 = AtomicU32::new(0);
static CITY_FORM_ACTIVE: AtomicU32 = AtomicU32::new(0);

/// The map's client clip rectangle, stored as left/top/right/bottom.
const MAP_CLIENT_RECT: u32 = 0x9C_B3FC;

/// A safe point inside the native map clip, including a deliberately tiny
/// parked engine viewport. The Base_Form still owns the full game window.
pub fn client_point_in(rect: Option<[i32; 4]>, size: (u32, u32)) -> (i32, i32) {
    let w = (size.0 as i32).max(1);
    let h = (size.1 as i32).max(1);
    let [left, top, right, bottom] = rect.unwrap_or([0, 0, w, h]);
    let left = left.clamp(0, w - 1);
    let top = top.clamp(0, h - 1);
    let right = right.clamp(left + 1, w);
    let bottom = bottom.clamp(top + 1, h);
    (left + (right - left) / 2, top + (bottom - top) / 2)
}

pub fn safe_client_point(size: (u32, u32)) -> (i32, i32) {
    client_point_in(Ptr32::<[i32; 4]>::new(MAP_CLIENT_RECT).read().ok(), size)
}

/// The native hover handler leaves its previous target alone when tile
/// conversion fails. Retire only the pointer target and validity flag so
/// an off-map renderer pointer cannot leave an old go-to overlay visible.
/// Must run on the game thread, after the normal virtual-input gates.
pub fn clear_pointer_target(form: u32) -> Result<(), inproc::MemError> {
    let form = Ptr32::<u8>::new(form);
    form.offset::<[i32; 2]>(crate::layout::MAIN_SCREEN_FORM_FIELD_4E80 + 12)?
        .write(&[-1, -1])?;
    form.offset::<u8>(crate::layout::MAIN_SCREEN_FORM_FIELD_2E1AC)?
        .write(&0)
}

/// Selects the Main_Screen_Form instance whose tile conversion is virtual.
pub fn set_main_screen_form(form: u32) {
    MAIN_SCREEN_FORM.store(form, Ordering::Release);
}

/// Makes native tile conversion return `tile` for the configured map form.
/// `None` restores Civ3's camera-based conversion.
pub fn set_virtual_tile(tile: Option<(i32, i32)>) {
    match tile {
        Some((x, y)) => {
            let packed = (u64::from(x as u32) << 32) | u64::from(y as u32);
            VIRTUAL_TILE.store(packed, Ordering::Relaxed);
            VIRTUAL_SET.store(1, Ordering::Release);
        }
        None => VIRTUAL_SET.store(0, Ordering::Release),
    }
}

/// Suppresses only the selection path's `bring_tile_into_view` call while a
/// virtual click is being dispatched. Keep this set for the duration of the
/// native left-click call and clear it immediately afterwards.
pub fn set_recenter_suppressed(suppressed: bool) {
    SUPPRESS_RECENTER.store(u32::from(suppressed), Ordering::Release);
}

/// Tells the guard that the city form currently owns the click. City-screen
/// opening and centering must remain native even when a virtual tile is set.
pub fn set_city_form_active(active: bool) {
    CITY_FORM_ACTIVE.store(u32::from(active), Ordering::Release);
}

/// Scoped form of [`set_recenter_suppressed`], which clears suppression on
/// every return path, including a panic in the caller.
pub struct RecenterGuard;

pub fn suppress_recenter() -> RecenterGuard {
    set_recenter_suppressed(true);
    RecenterGuard
}

impl Drop for RecenterGuard {
    fn drop(&mut self) {
        set_recenter_suppressed(false);
    }
}

/// Makes the native helper reject the current pointer as off the map.
/// This clears native targeting through the same hover path as a real
/// pointer leaving the map, rather than retaining its last destination.
pub fn set_virtual_invalid() {
    VIRTUAL_SET.store(2, Ordering::Release);
}

/// Returns the configured virtual tile, for diagnostics and tests.
pub fn virtual_tile() -> Option<(i32, i32)> {
    (VIRTUAL_SET.load(Ordering::Acquire) == 1).then(|| {
        let packed = VIRTUAL_TILE.load(Ordering::Relaxed);
        ((packed >> 32) as u32 as i32, packed as u32 as i32)
    })
}

/// Native readers of the current pointer use the same virtual tile as the
/// coordinate hook. Outer None means the override is disabled; Some(None)
/// means the renderer pointer is outside the map.
pub fn virtual_pointer_for(form: u32) -> Option<Option<(i32, i32)>> {
    if form == 0 || MAIN_SCREEN_FORM.load(Ordering::Acquire) != form { return None; }
    match VIRTUAL_SET.load(Ordering::Acquire) {
        1 => {
            let packed = VIRTUAL_TILE.load(Ordering::Relaxed);
            Some(Some(((packed >> 32) as u32 as i32, packed as u32 as i32)))
        }
        2 => Some(None),
        _ => None,
    }
}

inproc::fastcall_hook! {
    /// Inline hook for `0x4E3D20`.  A successful native conversion returns
    /// zero, so the virtual result writes both output coordinates and returns
    /// zero. Other forms and disabled frames always call the game original.
    pub mod get_tile_coords(this, edx; mouse_x, mouse_y, out_tile_x, out_tile_y) -> u32;
}

/// Verifies every direct entry point against the PCGames image. Call this once
/// during renderer installation, before dispatching events.
pub fn validate_sites() -> Result<(), inproc::hook::HookError> {
    for (address, prologue) in [
        (PCGAMES.get_tile_coords_under_mouse, GET_TILE_PROLOGUE),
        (PCGAMES.hover, HOVER_PROLOGUE),
        (PCGAMES.left_down, LEFT_DOWN_PROLOGUE),
        (PCGAMES.left_up, LEFT_UP_PROLOGUE),
        (PCGAMES.right_down, RIGHT_DOWN_PROLOGUE),
        (PCGAMES.right_up, RIGHT_UP_PROLOGUE),
        (PCGAMES.bring_tile_into_view, BRING_TILE_PROLOGUE),
        (PCGAMES.handle_key_down, KEY_PROLOGUE),
    ] {
        inproc::hook::check_function_start(address, prologue)?;
    }
    Ok(())
}

/// Installs the tile conversion hook after the caller has identified the
/// PCGames executable and its Main_Screen_Form address.
pub fn install_tile_hook(form: u32) -> Result<get_tile_coords::Installed, inproc::hook::HookError> {
    set_main_screen_form(form);
    get_tile_coords::install_inline(
        PCGAMES.get_tile_coords_under_mouse,
        GET_TILE_PROLOGUE,
        |call| {
            let expected = MAIN_SCREEN_FORM.load(Ordering::Acquire);
            let state = VIRTUAL_SET.load(Ordering::Acquire);
            if expected != 0 && call.this == expected && state == 2 {
                let _ = Ptr32::<i32>::new(call.out_tile_x).write(&-1);
                let _ = Ptr32::<i32>::new(call.out_tile_y).write(&-1);
                return 1;
            }
            if expected != 0 && call.this == expected && state == 1 {
                let packed = VIRTUAL_TILE.load(Ordering::Relaxed);
                let x = (packed >> 32) as u32 as i32;
                let y = packed as u32 as i32;
                // The native caller owns these output pointers. A failed write
                // falls back to the game so a malformed call cannot turn into a
                // false valid tile.
                if Ptr32::<i32>::new(call.out_tile_x).write(&x).is_ok()
                    && Ptr32::<i32>::new(call.out_tile_y).write(&y).is_ok()
                {
                    return 0;
                }
            }
            call.original()
        },
    )
}

inproc::fastcall_hook! {
    /// `Main_Screen_Form::bring_tile_into_view` (0x4DFAA0). The native
    /// selection handler calls this for an empty tile. Suppression is scoped
    /// by [`set_recenter_suppressed`] so ordinary city and unit centering
    /// still run.
    pub mod bring_tile_into_view_fn(screen, edx; tile_x, tile_y, param_3, always_update, param_5) -> ();
}

/// Installs the narrow camera-recentre guard. This is separate from the tile
/// conversion hook because `bring_tile_into_view` is also used by city and
/// unit screens and must remain native outside a virtual click.
pub fn install_bring_tile_hook()
-> Result<bring_tile_into_view_fn::Installed, inproc::hook::HookError> {
    bring_tile_into_view_fn::install_inline(
        PCGAMES.bring_tile_into_view,
        BRING_TILE_PROLOGUE,
        |call| {
            if SUPPRESS_RECENTER.load(Ordering::Acquire) != 0
                && CITY_FORM_ACTIVE.load(Ordering::Acquire) == 0
                && Ptr32::<u32>::new(FOCUS_CITY_SLOT)
                    .read()
                    .ok()
                    .is_some_and(|city| city == 0)
            {
                return;
            }
            let _request = crate::native_camera::begin_bring_tile(call.screen, (call.tile_x as i32, call.tile_y as i32));
            call.original();
        },
    )
}

inproc::thiscall_fn! {
    /// Main_Screen_Form::handle_key_down (0x4E06B0). Both character and
    /// virtual-key values are passed, matching Civ3's keyboard route.
    pub fn handle_key_down(screen; char_code, virtual_key_code) -> u32;
}

/// Sends a key through Civ3's own map-form handler. This is useful for G
/// (go-to), Escape, and F-key handling when the renderer owns the pointer.
pub fn key(screen: u32, char_code: u32, virtual_key_code: u32) -> u32 {
    handle_key_down::Function::at(PCGAMES.handle_key_down, KEY_PROLOGUE)
        .expect("checked PCGames key site")
        .call(screen, char_code, virtual_key_code)
}

inproc::thiscall_fn! {
    /// Main_Screen_Form::process_mouse_hover (vtable m27, 0x4EB7D0).
    pub fn process_mouse_hover(screen; local_x, local_y) -> ();
}

inproc::thiscall_fn! {
    /// Base_Form::m84_On_Mouse_Left_Down (0x608960). Four stack arguments
    /// are part of the game's event ABI: local x/y, double-click flag, and
    /// the fourth button message parameter.
    pub fn left_down_fn(screen; local_x, local_y, double_click, param_4) -> ();
}

inproc::thiscall_fn! {
    /// Base_Form::m85_On_Mouse_Left_Up (0x608A40). Three stack arguments:
    /// local x/y and the button message parameter.
    pub fn left_up_fn(screen; local_x, local_y, param_3) -> ();
}

inproc::thiscall_fn! {
    /// Base_Form::m86_On_Mouse_Right_Down (0x608AF0), with the same four
    /// message arguments as left-down.
    pub fn right_down_fn(screen; local_x, local_y, double_click, param_4) -> ();
}

inproc::thiscall_fn! {
    /// Base_Form::m87_On_Mouse_Right_Up (0x608B70), with three arguments.
    pub fn right_up_fn(screen; local_x, local_y, param_3) -> ();
}

/// Invokes the native hover path. The PCGames vtable points m27 at
/// `process_mouse_hover`; this call also updates `mouse_x/y` before asking
/// the virtual tile hook, so targeting and tile overlays update as usual.
pub fn hover(screen: u32, local_x: i32, local_y: i32) {
    process_mouse_hover::Function::at(PCGAMES.hover, HOVER_PROLOGUE)
        .expect("checked PCGames hover site")
        .call(screen, local_x as u32, local_y as u32);
}

/// Invokes the game's Base_Form left-down path. It dispatches through the
/// form's current vtable, preserving C3X's replacement of the left-down leaf.
pub fn left_down(screen: u32, local_x: i32, local_y: i32, double_click: bool, param_4: u32) {
    left_down_fn::Function::at(PCGAMES.left_down, LEFT_DOWN_PROLOGUE)
        .expect("checked PCGames left-down site")
        .call(
            screen,
            local_x as u32,
            local_y as u32,
            u32::from(double_click),
            param_4,
        );
}

/// Invokes the game's Base_Form left-up path.
pub fn left_up(screen: u32, local_x: i32, local_y: i32, param_3: u32) {
    left_up_fn::Function::at(PCGAMES.left_up, LEFT_UP_PROLOGUE)
        .expect("checked PCGames left-up site")
        .call(screen, local_x as u32, local_y as u32, param_3);
}

/// Invokes the game's Base_Form right-down path.
pub fn right_down(screen: u32, local_x: i32, local_y: i32, double_click: bool, param_4: u32) {
    right_down_fn::Function::at(PCGAMES.right_down, RIGHT_DOWN_PROLOGUE)
        .expect("checked PCGames right-down site")
        .call(
            screen,
            local_x as u32,
            local_y as u32,
            u32::from(double_click),
            param_4,
        );
}

/// Invokes the game's Base_Form right-up path.
pub fn right_up(screen: u32, local_x: i32, local_y: i32, param_3: u32) {
    right_up_fn::Function::at(PCGAMES.right_up, RIGHT_UP_PROLOGUE)
        .expect("checked PCGames right-up site")
        .call(screen, local_x as u32, local_y as u32, param_3);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_input_point_stays_inside_tiny_and_offset_map_clips() {
        for rect in [[0, 0, 5, 5], [10, 20, 15, 25], [200, 100, 1000, 700]] {
            let p = client_point_in(Some(rect), (1920, 1080));
            assert!(p.0 >= rect[0] && p.0 < rect[2] && p.1 >= rect[1] && p.1 < rect[3]);
        }
        assert_eq!(client_point_in(Some([0, 0, 5, 5]), (1920, 1080)), (2, 2));
        assert_eq!(client_point_in(None, (1920, 1080)), (960, 540));
        assert_eq!(
            client_point_in(Some([-10, -10, 5000, 5000]), (800, 600)),
            (400, 300)
        );
    }

    #[test]
    fn pcgames_sites_match_the_recovered_vtable() {
        assert_eq!(PCGAMES.get_tile_coords_under_mouse, 0x4E3D20);
        assert_eq!(PCGAMES.hover, 0x4EB7D0);
        assert_eq!(PCGAMES.left_down, 0x608960);
        assert_eq!(PCGAMES.left_up, 0x608A40);
        assert_eq!(PCGAMES.right_down, 0x608AF0);
        assert_eq!(PCGAMES.right_up, 0x608B70);
        assert_eq!(PCGAMES.bring_tile_into_view, 0x4DFAA0);
        assert_eq!(PCGAMES.handle_key_down, 0x4E06B0);
        assert_eq!(MAIN_SCREEN_VTABLE + VTABLE_LEFT_DOUBLE, 0x66B470);
    }

    #[test]
    fn virtual_tile_can_be_cleared_and_set() {
        set_virtual_tile(None);
        assert_eq!(virtual_tile(), None);
        set_main_screen_form(0x1234);
        set_virtual_tile(Some((-12, 34)));
        assert_eq!(virtual_tile(), Some((-12, 34)));
        assert_eq!(virtual_pointer_for(0x1234), Some(Some((-12, 34))));
        assert_eq!(virtual_pointer_for(0x5678), None);
        set_virtual_invalid();
        assert_eq!(virtual_pointer_for(0x1234), Some(None));
        set_virtual_tile(None);
        assert_eq!(virtual_pointer_for(0x1234), None);
        set_main_screen_form(0);
    }
}
