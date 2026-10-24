//! Our graphics object (`get_graphsy_object_ptr`, 0x14C bytes) and its 86
//! slots: jgl's behaviour [V, jgl.dll disassembled: RVAs in each body's
//! doc], on our window.
//!
//! The window and the pumps are jgl's, with what the host did to jgl's
//! imports done here directly: the screen size the game is told
//! ([`jgl::WindowHooks::screen`]), the window kept hidden
//! (`inproc::hidden::show_command`) and painting while hidden
//! (`inproc::paint`), the pointer under an override
//! (`inproc::pointer::overridden`), the heartbeat around peeks and waits
//! ([`jgl::WindowHooks`]), and the display never switched
//! ([`jgl::display`]).

use std::mem::offset_of;
use std::sync::atomic::{AtomicU32, Ordering};

use jgl::window::{self as w, Controller, Pump, Route};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi as gdi;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::UI::WindowsAndMessaging as wm;

use super::call::{self, get, put};
use super::constant;
use super::stubs;
use crate::layout::JglGraphics;
use jgl::slots::Class;

/// The graphics object (jgl's global at RVA 0x70D30: the image and sprite
/// code and the window procedure call it).
static GRAPHICS: AtomicU32 = AtomicU32::new(0);
/// The controller `create_window` was given (RVA 0x70F3C).
static CONTROLLER: AtomicU32 = AtomicU32::new(0);
/// The `WM_COPYDATA` callback of +0x154 (RVA 0x70F38), cdecl (wParam, lParam).
static COPY_DATA: AtomicU32 = AtomicU32::new(0);
/// The memory DC of +0xB8 (RVA 0x70F34): fonts are made and measured in it.
static MEMORY_DC: AtomicU32 = AtomicU32::new(0);
/// The palette of +0xC0 (RVA 0x70F44).
static PALETTE: AtomicU32 = AtomicU32::new(0);
/// The colour tables +0xC0 was given (RVA 0x70F48, 0x70F4C).
static TABLES: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
/// The draw scale (RVA 0x6C0FC..: num_x, num_y, den), at first 1, 1, 1.
pub(crate) static SCALE: [AtomicU32; 3] = [AtomicU32::new(1), AtomicU32::new(1), AtomicU32::new(1)];

/// The graphics object, once made.
pub fn object() -> u32 {
    GRAPHICS.load(Ordering::Acquire)
}

/// The palette of graphics +0xC0 (0: none).
pub fn palette() -> u32 {
    PALETTE.load(Ordering::Acquire)
}

/// The memory DC fonts are made and measured in (graphics +0xB8).
pub fn memory_dc() -> u32 {
    MEMORY_DC.load(Ordering::Acquire)
}

fn field(this: u32, offset: usize) -> u32 {
    // SAFETY: `this` is our graphics object (0x14C bytes).
    unsafe { get(this + offset as u32) }
}

fn set_field(this: u32, offset: usize, v: u32) {
    // SAFETY: as `field`.
    unsafe { put(this + offset as u32, v) }
}

const WIDTH: usize = offset_of!(JglGraphics, screen_width);
const HEIGHT: usize = offset_of!(JglGraphics, screen_height);
const DEPTH: usize = offset_of!(JglGraphics, depth);
const DC: usize = offset_of!(JglGraphics, dc);
const HINSTANCE: usize = offset_of!(JglGraphics, hinstance);
const HWND_AT: usize = offset_of!(JglGraphics, hwnd);
const OWNER: usize = offset_of!(JglGraphics, owner);

fn hwnd(this: u32) -> HWND {
    field(this, HWND_AT) as usize as HWND
}

/// `get_graphsy_object_ptr` (RVA 0x3B290, constructor 0x3B2E0): the
/// object zeroed, our table; it is the graphics object from now on.
pub fn new() -> u32 {
    let g = call::alloc::<JglGraphics>(super::table_of(Class::Graphics));
    GRAPHICS.store(g, Ordering::Release);
    g
}

/// Tells the owner (the back buffer) the screen's mode: its method 0
/// with (width, height, bits, 1); its answer.
fn tell_owner(this: u32, width: u32, height: u32, bits: u32) -> u32 {
    let owner = field(this, OWNER);
    if owner == 0 {
        return 0;
    }
    // SAFETY: the game's `PCX_Image` create_window was given; its method 0
    // (0x414990) takes four words, as jgl calls it.
    unsafe { call::method(owner, 0x00, &[width, height, bits, 1]) }
}

/// The screen's size, as the game is told it.
fn screen() -> (i32, i32) {
    // SAFETY: plain queries.
    let display = unsafe { (wm::GetSystemMetrics(wm::SM_CXSCREEN), wm::GetSystemMetrics(wm::SM_CYSCREEN)) };
    super::window_hooks().screen(display)
}

/// +0x00 (0x3B360): the destructor only resets the vtable; freed with 1.
extern "thiscall" fn scalar_delete(this: u32, flags: u32) -> u32 {
    if flags & 1 != 0 {
        let _ = GRAPHICS.compare_exchange(this, 0, Ordering::AcqRel, Ordering::Acquire);
        // SAFETY: our graphics object, from `alloc::<JglGraphics>`; the
        // game lets go of it with this call.
        unsafe { call::free::<JglGraphics>(this) };
    }
    this
}

/// +0x04 `create_window(controller, title, owner, icon)` (0x3B400).
extern "thiscall" fn create_window(this: u32, controller: u32, title: u32, owner: u32, icon: u32) -> u32 {
    // SAFETY: the calls jgl makes, with the arguments it passes; `title` is
    // the game's string (passed on unread).
    unsafe {
        let instance = GetModuleHandleA(std::ptr::null());
        set_field(this, HINSTANCE, instance as usize as u32);
        CONTROLLER.store(controller, Ordering::Release);
        set_field(this, OWNER, owner);
        let mut icon_handle = wm::LoadIconA(instance, (icon & 0xFFFF) as usize as *const u8);
        if icon_handle.is_null() {
            icon_handle = wm::LoadIconA(std::ptr::null_mut(), wm::IDI_APPLICATION as usize as *const u8);
        }
        let class = wm::WNDCLASSA {
            style: w::CLASS_STYLE,
            lpfnWndProc: Some(window_procedure),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: icon_handle,
            hCursor: std::ptr::null_mut(),
            hbrBackground: gdi::GetStockObject(gdi::BLACK_BRUSH),
            lpszMenuName: std::ptr::null(),
            lpszClassName: w::CLASS.as_ptr(),
        };
        if wm::RegisterClassA(&class) == 0 {
            return 1;
        }
        let (width, height) = screen();
        let window = wm::CreateWindowExA(
            w::EX_STYLE,
            w::CLASS.as_ptr(),
            title as usize as *const u8,
            w::STYLE,
            0,
            0,
            width,
            height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        set_field(this, HWND_AT, window as usize as u32);
        if window.is_null() {
            return 1;
        }
        set_field(this, WIDTH, width as u32);
        set_field(this, HEIGHT, height as u32);
        set_field(this, DEPTH, 8);
        let dc = gdi::GetDC(window);
        set_field(this, DC, dc as usize as u32);
        gdi::SetBkMode(dc, gdi::TRANSPARENT as i32);
        super::window_hooks().window_made(window as usize as u32);
        tell_owner(this, width as u32, height as u32, 8);
    }
    0
}

/// +0x08 `destroy_window` (0x3B390): jgl's shutdown. Its
/// `ChangeDisplaySettingsA` back to the mode it saved is answered as the
/// display group answers it: the display was never switched.
extern "thiscall" fn destroy_window(this: u32) -> u32 {
    log::info!("our jgl: its shutdown restores a mode it never saved (the display was never switched); nothing to do");
    // SAFETY: the calls jgl makes on its own window and owner.
    unsafe {
        let window = hwnd(this);
        if !window.is_null() {
            wm::DestroyWindow(window);
            set_field(this, HWND_AT, 0);
        }
        let owner = field(this, OWNER);
        if owner != 0 {
            // The owner's method +0x08 (`PCX_Image` clear, no arguments).
            call::method(owner, 0x08, &[]);
            set_field(this, OWNER, 0);
        }
        let r = wm::UnregisterClassA(w::CLASS.as_ptr(), field(this, HINSTANCE) as usize as _);
        set_field(this, HINSTANCE, 0);
        r as u32
    }
}

/// +0x0C `release` (0x3B2C0): every object deletes itself (the last made
/// first), then the graphics object.
extern "thiscall" fn release(_this: u32) -> u32 {
    while let Some(o) = super::registry::last() {
        // SAFETY: an object of ours alive; its slot 0 (scalar_delete) takes
        // one word and takes it out of the registry.
        unsafe { call::method(o, 0x00, &[1]) };
        super::registry::gone(o);
    }
    let g = object();
    if g != 0 {
        // SAFETY: our graphics object; scalar_delete with 1 frees it.
        unsafe { call::method(g, 0x00, &[1]) };
    }
    0
}

fn show(this: u32, command: i32) -> u32 {
    let window = hwnd(this);
    let command = inproc::hidden::show_command(window as usize as u32, command as u32);
    // SAFETY: jgl's call on its own window.
    unsafe { wm::ShowWindow(window, command as i32) as u32 }
}

/// +0x10..+0x1C (0x3BEA0..): `ShowWindow` with 1, 0, 6, 3.
extern "thiscall" fn show_normal(this: u32) -> u32 {
    show(this, 1)
}
extern "thiscall" fn show_hide(this: u32) -> u32 {
    show(this, 0)
}
extern "thiscall" fn show_minimized(this: u32) -> u32 {
    show(this, 6)
}
extern "thiscall" fn show_maximized(this: u32) -> u32 {
    show(this, 3)
}

/// +0x20, +0x24: the window, the instance.
extern "thiscall" fn get_hwnd(this: u32) -> u32 {
    field(this, HWND_AT)
}
extern "thiscall" fn get_hinstance(this: u32) -> u32 {
    field(this, HINSTANCE)
}

/// +0x28 (0x3BC10): `SetWindowPos(window, HWND_TOP, x, y, w, h, 0)`.
extern "thiscall" fn set_window_pos(this: u32, x: u32, y: u32, width: u32, height: u32) -> u32 {
    // SAFETY: jgl's call on its own window (no SWP_SHOWWINDOW).
    unsafe { wm::SetWindowPos(hwnd(this), std::ptr::null_mut(), x as i32, y as i32, width as i32, height as i32, 0) as u32 }
}

/// +0x2C `cursor_pos(&x, &y)` (0x3BBD0): the pointer in the window's
/// client coordinates (under an override, the override's).
extern "thiscall" fn cursor_pos(this: u32, x: u32, y: u32) -> u32 {
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: jgl's calls; `x` and `y` are the game's out arguments.
    unsafe {
        match inproc::pointer::overridden() {
            Some((px, py)) => p = POINT { x: px, y: py },
            None => {
                wm::GetCursorPos(&mut p);
            }
        }
        gdi::ScreenToClient(hwnd(this), &mut p);
        put(x, p.x as u32);
        put(y, p.y as u32);
    }
    p.y as u32
}

/// +0x30 `invalidate(rect)`'s original (0x3BBB0): `InvalidateRect(window,
/// rect, FALSE)`, kept for a hidden window's paint.
pub extern "thiscall" fn invalidate(this: u32, rect: u32) -> u32 {
    let window = hwnd(this);
    // SAFETY: jgl's call; `rect` is the game's (or null).
    let r = unsafe { gdi::InvalidateRect(window, rect as usize as *const RECT, 0) };
    inproc::paint::invalidated(window as usize as u32, rect);
    r as u32
}

/// +0x34 (0x3BB90): the screen's size into the two out arguments.
extern "thiscall" fn screen_size(this: u32, width: u32, height: u32) -> u32 {
    // SAFETY: the game's out arguments.
    unsafe {
        put(width, field(this, WIDTH));
        put(height, field(this, HEIGHT));
    }
    field(this, HEIGHT)
}

/// +0x38 (0x3BB70): `GetWindowRect(window, rect)`.
extern "thiscall" fn window_rect(this: u32, rect: u32) -> u32 {
    // SAFETY: jgl's call; `rect` is the game's out argument.
    unsafe { wm::GetWindowRect(hwnd(this), rect as usize as *mut RECT) as u32 }
}

/// +0x3C: the window's DC.
extern "thiscall" fn get_dc(this: u32) -> u32 {
    field(this, DC)
}

constant!(zero_0, 0,);
constant!(zero_1, 0, a);

/// +0x50 `set_mode_fastest(w, h, bits)` (0x3B7B0): jgl picks the mode of
/// its list with the highest frequency and sets it (+0x64), answering 0
/// whatever happened. The display is never switched: the mode is the
/// screen, as +0x60 with `keep` makes it.
extern "thiscall" fn set_mode_fastest(this: u32, width: u32, height: u32, bits: u32) -> u32 {
    let mode = jgl::display::Mode { width: width as i32, height: height as i32, bits: bits as i32 };
    if let Some(m) = jgl::display::screen_for(mode) {
        set_mode_keep(this, m.width as u32, m.height as u32, m.bits as u32, 1);
    }
    0
}

/// +0x54 `set_depth(bits)` (0x3B9B0): +0x58 with the screen's size.
extern "thiscall" fn set_depth(this: u32, bits: u32) -> u32 {
    let (width, height) = screen();
    // SAFETY: our graphics object's +0x58, three words.
    unsafe { call::method(this, 0x58, &[width as u32, height as u32, bits]) }
}

/// +0x58 and +0x5C's original, for a mode `jgl::display::screen_for`
/// refuses: no mode of that kind (jgl's answer when its list has none).
extern "thiscall" fn set_mode(_this: u32, _width: u32, _height: u32, _bits: u32) -> u32 {
    1
}
extern "thiscall" fn set_mode_refresh(_this: u32, _width: u32, _height: u32, _bits: u32, _keep: u32, _refresh: u32) -> u32 {
    1
}

/// +0x60 `set_mode_keep(w, h, bits, keep)` (0x3B810): with `keep`, the
/// screen fields and the owner told (its answer); else +0x58.
pub extern "thiscall" fn set_mode_keep(this: u32, width: u32, height: u32, bits: u32, keep: u32) -> u32 {
    if keep & 0xFF == 0 {
        // SAFETY: our graphics object's +0x58, three words.
        return unsafe { call::method(this, 0x58, &[width, height, field(this, DEPTH)]) };
    }
    set_field(this, WIDTH, width);
    set_field(this, HEIGHT, height);
    set_field(this, DEPTH, bits);
    tell_owner(this, width, height, bits)
}

/// +0x64 `set_mode_index(i)` (0x3B9E0): no mode of jgl's list is ever
/// set; its list is empty here (+0x68), and jgl answers an index past its
/// end with 3.
extern "thiscall" fn set_mode_index(_this: u32, _index: u32) -> u32 {
    3
}

/// jgl's list of display modes (+0x68): {vtable, data, capacity, count,
/// growth}, empty.
static MODES: [u32; 5] = [0, 0, 0, 0, 1];

/// +0x68 `display_modes` (0x3B560): the list, empty (the display is never
/// switched, so no mode of it is offered).
extern "thiscall" fn display_modes(_this: u32) -> u32 {
    MODES.as_ptr() as usize as u32
}

/// +0x6C `set_display_size(w, h)` (0x3BC40): a movie's switch to 640×480;
/// +0x70 (0x3BCB0) back. The display is left as it is; jgl answers 0 for
/// a switch made.
extern "thiscall" fn set_display_size(_this: u32, width: u32, height: u32) -> u32 {
    log::info!("our jgl: a movie's {width}×{height} switch; the display is left as it is");
    0
}
extern "thiscall" fn restore_display_mode(_this: u32) -> u32 {
    0
}

/// +0x88..+0x94's original (0x3C0C0): the object deletes itself (its slot
/// 0 with 1), answering what that answers.
pub extern "thiscall" fn delete(_this: u32, object: u32) -> u32 {
    if object == 0 {
        return 0;
    }
    // SAFETY: an object of ours (the game deletes only what it was given);
    // slot 0 is its scalar_delete, one word.
    unsafe { call::method(object, 0x00, &[1]) }
}

/// +0x9C `set_scale(num_x, num_y, den)`'s original (0x3C0E0).
pub extern "thiscall" fn set_scale(_this: u32, num_x: u32, num_y: u32, den: u32) -> u32 {
    for (cell, v) in SCALE.iter().zip([num_x, num_y, den]) {
        cell.store(v, Ordering::Relaxed);
    }
    num_x
}

/// +0xA0 `get_scale(&num_x, &num_y, &den)` (0x3C100).
extern "thiscall" fn get_scale(_this: u32, num_x: u32, num_y: u32, den: u32) -> u32 {
    let v = SCALE.each_ref().map(|c| c.load(Ordering::Relaxed));
    // SAFETY: the game's out arguments.
    unsafe {
        put(num_x, v[0]);
        put(num_y, v[1]);
        put(den, v[2]);
    }
    v[2]
}

/// +0xA4 `present(rect)`'s original: our objects have no pixels to put on
/// the window (the viewer shows the game); jgl's answer, 0.
pub extern "thiscall" fn present(_this: u32, _rect: u32) -> u32 {
    0
}

/// +0xA8, +0xAC, +0xB0: the screen's width, height, depth.
extern "thiscall" fn screen_width(this: u32) -> u32 {
    field(this, WIDTH)
}
extern "thiscall" fn screen_height(this: u32) -> u32 {
    field(this, HEIGHT)
}
extern "thiscall" fn screen_depth(this: u32) -> u32 {
    field(this, DEPTH)
}

/// +0xB8 (0x3C660): a new memory DC for fonts; 0 if made, else 2.
extern "thiscall" fn create_compatible_dc(this: u32) -> u32 {
    delete_dc(this);
    // SAFETY: a plain GDI call.
    let dc = unsafe { gdi::CreateCompatibleDC(std::ptr::null_mut()) } as usize as u32;
    MEMORY_DC.store(dc, Ordering::Release);
    if dc != 0 { 0 } else { 2 }
}

/// A memory DC for fonts if there is none (the compare mode, without our
/// graphics object).
pub(crate) fn ensure_memory_dc() {
    if memory_dc() == 0 {
        // SAFETY: a plain GDI call.
        let dc = unsafe { gdi::CreateCompatibleDC(std::ptr::null_mut()) } as usize as u32;
        MEMORY_DC.store(dc, Ordering::Release);
    }
}

/// +0xBC (0x3C680): the memory DC goes.
extern "thiscall" fn delete_dc(_this: u32) -> u32 {
    let dc = MEMORY_DC.swap(0, Ordering::AcqRel);
    if dc != 0 {
        // SAFETY: the DC +0xB8 made.
        unsafe { gdi::DeleteDC(dc as usize as gdi::HDC) };
    }
    dc
}

/// The system colours jgl puts at both ends of its palette (RVA 0x688E8,
/// 0x68910), 0x00RRGGBB.
const LOW: [u32; 10] = [0x0, 0x800000, 0x8000, 0x808000, 0x80, 0x800080, 0x8080, 0xc0c0c0, 0xc0dcc0, 0xa6caf0];
const HIGH: [u32; 10] = [0xfffbf0, 0xa0a0a4, 0x808080, 0xff0000, 0xff00, 0xffff00, 0xff, 0xff00ff, 0xffff, 0xffffff];

/// The palette jgl makes (0x3D220): the system's twenty colours at the
/// ends, 8, 9, 246 and 247 as `PC_EXPLICIT` indices, the rest reserved.
/// (jgl leaves the colours of 10..245 as its allocation had them; 0 here.)
pub fn palette_entries() -> [[u8; 4]; 256] {
    let rgb = |c: u32| [(c >> 16) as u8, (c >> 8) as u8, c as u8, 0];
    let mut e = [[0, 0, 0, 5]; 256];
    for i in 0..10 {
        e[i] = rgb(LOW[i]);
        e[246 + i] = rgb(HIGH[i]);
    }
    e[8] = [8, 0, 0, 2];
    e[9] = [9, 0, 0, 2];
    e[246] = [0xF5, 0, 0, 2];
    e[247] = [0xF6, 0, 0, 2];
    e
}

/// +0xC0 `create_palette(table_a, table_b)` (0x3D220): jgl's palette; then
/// each table's method 0, and they are kept. 0, or 1 if no palette.
extern "thiscall" fn create_palette(this: u32, a: u32, b: u32) -> u32 {
    delete_palette(this);
    let mut bytes = vec![0u8; 4 + 4 * 256];
    bytes[0..2].copy_from_slice(&0x300u16.to_le_bytes());
    bytes[2..4].copy_from_slice(&256u16.to_le_bytes());
    for (i, e) in palette_entries().iter().enumerate() {
        bytes[4 + 4 * i..8 + 4 * i].copy_from_slice(e);
    }
    // SAFETY: `bytes` is a LOGPALETTE of 256 entries.
    let palette = unsafe { gdi::CreatePalette(bytes.as_ptr() as *const gdi::LOGPALETTE) } as usize as u32;
    PALETTE.store(palette, Ordering::Release);
    if palette == 0 {
        return 1;
    }
    // SAFETY: the game's two colour tables (exe objects): their method 0
    // takes no arguments, as jgl calls it.
    unsafe {
        call::method(a, 0x00, &[]);
        call::method(b, 0x00, &[]);
    }
    TABLES[1].store(b, Ordering::Release);
    TABLES[0].store(a, Ordering::Release);
    0
}

/// +0xC4 (0x3D350): the palette goes.
extern "thiscall" fn delete_palette(_this: u32) -> u32 {
    let p = PALETTE.swap(0, Ordering::AcqRel);
    if p != 0 {
        // SAFETY: the palette +0xC0 made.
        unsafe { gdi::DeleteObject(p as usize as gdi::HGDIOBJ) };
    }
    p
}

/// The palette realized in the window's DC (0x3CC10); 1.
fn realize(window: HWND) -> isize {
    // SAFETY: jgl's calls on its own window.
    unsafe {
        let dc = gdi::GetDC(window);
        gdi::SelectPalette(dc, PALETTE.load(Ordering::Acquire) as usize as gdi::HPALETTE, 0);
        gdi::RealizePalette(dc);
        gdi::ReleaseDC(window, dc);
    }
    1
}

/// +0x154 (0x3C180): the `WM_COPYDATA` callback.
extern "thiscall" fn set_copy_data(_this: u32, callback: u32) -> u32 {
    COPY_DATA.store(callback, Ordering::Release);
    callback
}

/// Tells the controller (`C->vt[slot](args)`); its answer.
fn tell(c: Controller, rect: Option<&RECT>) -> u32 {
    let controller = CONTROLLER.load(Ordering::Acquire);
    if controller == 0 {
        return 0;
    }
    let mut args = c.args();
    if let Some(r) = rect {
        args = vec![r as *const RECT as usize as u32];
    }
    // SAFETY: the exe's controller (vtable 0x67138C) and the arguments
    // jgl passes to that method (`jgl::window::Controller`).
    unsafe { call::method(controller, c.slot(), &args) }
}

/// jgl's window procedure (0x3C730).
unsafe extern "system" fn window_procedure(window: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: what jgl calls, with the window procedure's arguments.
    let def = || unsafe { wm::DefWindowProcA(window, msg, wparam, lparam) };
    match w::route(window as usize, msg, wparam, lparam) {
        Route::Default => def(),
        Route::Answer(a) => a,
        Route::Tell(c) => {
            tell(c, None);
            0
        }
        Route::Ask(c) => tell(c, None) as i32 as isize,
        Route::TellThenDefault(c) => {
            tell(c, None);
            def()
        }
        Route::Key { vk, down } => {
            tell(Controller::Key(vk, down), None);
            if down {
                key_chars(window, vk);
            }
            0
        }
        Route::Paint => {
            // SAFETY: a paint of jgl's window, as jgl makes it.
            unsafe {
                let mut ps: gdi::PAINTSTRUCT = std::mem::zeroed();
                let dc = gdi::BeginPaint(window, &mut ps);
                inproc::paint::begun(window as usize as u32, &mut ps as *mut gdi::PAINTSTRUCT as usize as u32);
                if !dc.is_null() {
                    let r = ps.rcPaint;
                    tell(Controller::Paint, Some(&r));
                    gdi::EndPaint(window, &ps);
                }
            }
            0
        }
        Route::Activate { realize: r, tell: c } => {
            // SAFETY: jgl's call on its own window.
            unsafe { gdi::InvalidateRect(window, std::ptr::null(), 0) };
            inproc::paint::invalidated(window as usize as u32, 0);
            if r {
                realize(window);
            }
            tell(c, None);
            def();
            0
        }
        Route::Realize(answer) => {
            realize(window);
            answer
        }
        Route::CopyData => match COPY_DATA.load(Ordering::Acquire) {
            0 => 0,
            f => {
                type Callback = extern "C" fn(usize, isize) -> isize;
                // SAFETY: the cdecl callback the game set through +0x154.
                let f: Callback = unsafe { std::mem::transmute::<usize, Callback>(f as usize) };
                f(wparam, lparam)
            }
        },
    }
}

/// After a key went down (0x3CD30): the characters it queued, dropped for
/// the number pad's keys, else dispatched now through graphics +0x138.
fn key_chars(window: HWND, vk: u32) {
    if w::drops_chars(vk) {
        // SAFETY: a plain peek; the messages are thrown away.
        unsafe {
            let mut m: wm::MSG = std::mem::zeroed();
            while wm::PeekMessageA(&mut m, window, wm::WM_CHAR, wm::WM_CHAR, wm::PM_REMOVE) != 0 {}
        }
        return;
    }
    let g = object();
    // SAFETY: our graphics object's +0x138, no arguments (the pump of
    // characters), called through its table as jgl does.
    while g != 0 && unsafe { call::method(g, 0x138, &[]) } != 0 {}
}

/// `PeekMessageA` with the host told; whether one was there.
fn peek(m: &mut wm::MSG, window: HWND, filter: w::Filter, remove: u32) -> bool {
    let hooks = super::window_hooks();
    hooks.peeking();
    // SAFETY: a plain peek into `m`.
    let got = unsafe { wm::PeekMessageA(m, window, filter.first, filter.last, remove) } != 0;
    if got && remove & w::PM_REMOVE != 0 {
        hooks.peeked(remove, m.message, m.lParam as u32);
    }
    got
}

fn dispatch(m: &wm::MSG) {
    // SAFETY: a message just taken off the queue.
    unsafe {
        wm::TranslateMessage(m);
        wm::DispatchMessageA(m);
    }
}

const ALL: w::Filter = w::Filter { first: 0, last: 0 };

/// A pump's body (0x3D430..0x3D8A0).
fn pump(this: u32, which: Pump) -> u32 {
    // SAFETY: an empty MSG is plain data.
    let mut m: wm::MSG = unsafe { std::mem::zeroed() };
    let none = std::ptr::null_mut();
    match which {
        Pump::One(filter) => {
            if !peek(&mut m, none, filter, w::PM_REMOVE) {
                return 0;
            }
            dispatch(&m);
            1
        }
        Pump::Wait => {
            // SAFETY: our graphics object's +0x128, called through its
            // table as jgl does.
            if unsafe { call::method(this, 0x128, &[]) } != 0 {
                return 1;
            }
            let hooks = super::window_hooks();
            hooks.waiting(true);
            // SAFETY: a plain wait.
            let r = unsafe { wm::WaitMessage() };
            hooks.waiting(false);
            r as u32
        }
        Pump::Any => {
            if !peek(&mut m, none, ALL, w::PM_REMOVE) {
                return 0;
            }
            if m.message == w::MESSAGE_402 {
                tell(Controller::Message402, None);
            } else {
                dispatch(&m);
            }
            1
        }
        Pump::Oldest => {
            let mut found = [false; 3];
            for (i, f) in w::OLDEST_FILTERS.iter().enumerate() {
                if peek(&mut m, none, *f, 0) {
                    found[i] = true;
                    break;
                }
            }
            let Some(_) = w::oldest(found) else { return 0 };
            let (window, message) = (m.hwnd, m.message);
            peek(&mut m, window, w::Filter { first: message, last: message }, w::PM_REMOVE);
            dispatch(&m);
            1
        }
        Pump::Click => {
            if !peek(&mut m, none, w::MOUSE, w::PM_REMOVE) {
                return 0;
            }
            dispatch(&m);
            if let Some(up) = w::button_up(m.message) {
                let window = m.hwnd;
                if peek(&mut m, window, w::Filter { first: up, last: up }, w::PM_REMOVE) {
                    dispatch(&m);
                }
            }
            1
        }
        Pump::Discard(filter) => {
            while peek(&mut m, none, filter, w::PM_REMOVE) {}
            0
        }
        Pump::Drain(filter) => {
            while peek(&mut m, none, filter, w::PM_REMOVE) {
                dispatch(&m);
            }
            0
        }
    }
}

macro_rules! pump_slot {
    ($name:ident, $slot:expr) => {
        extern "thiscall" fn $name(this: u32) -> u32 {
            match Pump::at($slot) {
                Some(p) => pump(this, p),
                None => 0,
            }
        }
    };
}

pump_slot!(pump_120, 0x120);
pump_slot!(pump_124, 0x124);
pump_slot!(pump_128, 0x128);
pump_slot!(pump_12c, 0x12C);
pump_slot!(pump_130, 0x130);
pump_slot!(pump_134, 0x134);
pump_slot!(pump_138, 0x138);
pump_slot!(pump_13c, 0x13C);
pump_slot!(pump_140, 0x140);
pump_slot!(pump_144, 0x144);
pump_slot!(pump_148, 0x148);
pump_slot!(pump_14c, 0x14C);
pump_slot!(pump_150, 0x150);

type M0 = extern "thiscall" fn(u32) -> u32;
type M1 = extern "thiscall" fn(u32, u32) -> u32;
type M2 = extern "thiscall" fn(u32, u32, u32) -> u32;
type M3 = extern "thiscall" fn(u32, u32, u32, u32) -> u32;
type M4 = extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32;
type M5 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32) -> u32;

/// Our implementation of graphics slot `offset` (0: none).
pub fn slot(offset: u16) -> u32 {
    let f: usize = match offset {
        0x00 => scalar_delete as M1 as usize,
        0x04 => create_window as M4 as usize,
        0x08 => destroy_window as M0 as usize,
        0x0C => release as M0 as usize,
        0x10 => show_normal as M0 as usize,
        0x14 => show_hide as M0 as usize,
        0x18 => show_minimized as M0 as usize,
        0x1C => show_maximized as M0 as usize,
        0x20 => get_hwnd as M0 as usize,
        0x24 => get_hinstance as M0 as usize,
        0x28 => set_window_pos as M4 as usize,
        0x2C => cursor_pos as M2 as usize,
        0x30 => invalidate as M1 as usize,
        0x34 => screen_size as M2 as usize,
        0x38 => window_rect as M1 as usize,
        0x3C => get_dc as M0 as usize,
        0x48 | 0x4C | 0xB4 => zero_0 as M0 as usize,
        0x50 => set_mode_fastest as M3 as usize,
        0x54 => set_depth as M1 as usize,
        0x58 => set_mode as M3 as usize,
        0x5C => set_mode_refresh as M5 as usize,
        0x60 => set_mode_keep as M4 as usize,
        0x64 => set_mode_index as M1 as usize,
        0x68 => display_modes as M0 as usize,
        0x6C => set_display_size as M2 as usize,
        0x70 => restore_display_mode as M0 as usize,
        0x74 => super::font::new as M1 as usize,
        0x78 => super::table::new as M1 as usize,
        0x7C => super::image::new as M2 as usize,
        0x80 => super::sprite::new as M2 as usize,
        0x84 | 0x98 => zero_1 as M1 as usize,
        0x88..=0x94 => delete as M1 as usize,
        0x9C => set_scale as M3 as usize,
        0xA0 => get_scale as M3 as usize,
        0xA4 => present as M1 as usize,
        0xA8 => screen_width as M0 as usize,
        0xAC => screen_height as M0 as usize,
        0xB0 => screen_depth as M0 as usize,
        0xB8 => create_compatible_dc as M0 as usize,
        0xBC => delete_dc as M0 as usize,
        0xC0 => create_palette as M2 as usize,
        0xC4 => delete_palette as M0 as usize,
        0x120 => pump_120 as M0 as usize,
        0x124 => pump_124 as M0 as usize,
        0x128 => pump_128 as M0 as usize,
        0x12C => pump_12c as M0 as usize,
        0x130 => pump_130 as M0 as usize,
        0x134 => pump_134 as M0 as usize,
        0x138 => pump_138 as M0 as usize,
        0x13C => pump_13c as M0 as usize,
        0x140 => pump_140 as M0 as usize,
        0x144 => pump_144 as M0 as usize,
        0x148 => pump_148 as M0 as usize,
        0x14C => pump_14c as M0 as usize,
        0x150 => pump_150 as M0 as usize,
        0x154 => set_copy_data as M1 as usize,
        _ => {
            return match Class::Graphics.slot(offset) {
                Some(s) if s.group == jgl::slots::Group::Stub => stubs::by_stack(s.stack),
                _ => 0,
            };
        }
    };
    f as u32
}
