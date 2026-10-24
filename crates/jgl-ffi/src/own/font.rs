//! Our jgl font (0x24 bytes, jgl's layout): a GDI font, made and measured
//! in the memory DC of graphics +0xB8, as jgl's [V, jgl.dll disassembled,
//! RVAs in each body's doc].
//!
//! The text's measure is GDI's: the game lays its text out by these
//! numbers (font +0x10 `measure`, 2.2 million calls a session; the metrics
//! +0x08 writes into the game's font). GDI is Windows' text engine, not
//! jgl, and the viewer's glyphs come from GDI too (`gdi-text`), with the
//! same faces and sizes: what is laid out and what is shown agree. The DC
//! is one memory DC with no surface of ours: nothing is drawn in it.

use std::mem::offset_of;

use jgl::slots::Class;
use windows_sys::Win32::Graphics::Gdi as gdi;

use super::call::{self, get, put};
use super::stubs;
use crate::layout::JglFont;

const METRICS: usize = offset_of!(JglFont, metrics);
const FLAGS: usize = offset_of!(JglFont, flags);
const HFONT: usize = offset_of!(JglFont, hfont);
const RESOURCE: usize = offset_of!(JglFont, resource);

fn field(this: u32, at: usize) -> u32 {
    // SAFETY: `this` is a font of ours (0x24 bytes).
    unsafe { get(this + at as u32) }
}

fn set(this: u32, at: usize, v: u32) {
    // SAFETY: as `field`.
    unsafe { put(this + at as u32, v) }
}

/// graphics +0x74 `new_font(metrics)`'s original (0x3BF20, constructor
/// 0x3C190): `metrics` is the game's font +4.
pub extern "thiscall" fn new(_graphics: u32, metrics: u32) -> u32 {
    let this = call::alloc::<JglFont>(super::table_of(Class::Font));
    if this == 0 {
        return 0;
    }
    set(this, METRICS, metrics);
    super::registry::made(this);
    this
}

/// +0x00 `scalar_delete(flags)`'s original (0x3C6F0): the game's font lets
/// go of it, the font is released, freed with flag 1.
pub extern "thiscall" fn scalar_delete(this: u32, flags: u32) -> u32 {
    let metrics = field(this, METRICS);
    if metrics != 0 {
        // SAFETY: the game's font +4, the field this font is kept in.
        unsafe { put(metrics, 0) };
    }
    release(this);
    super::registry::gone(this);
    if flags & 1 != 0 {
        // SAFETY: a font of ours, from `alloc::<JglFont>`.
        unsafe { call::free::<JglFont>(this) };
    }
    this
}

/// Deletes the GDI font of `this` (the compare mode's copies).
pub(crate) fn forget(this: u32) {
    let font = field(this, HFONT);
    if font != 0 {
        // SAFETY: the HFONT our `create` made for the copy.
        unsafe { gdi::DeleteObject(font as usize as gdi::HGDIOBJ) };
    }
}

/// The NUL-terminated string at `at`.
fn c_str(at: u32) -> Vec<u8> {
    // SAFETY: the game's string, read as jgl reads it (unchecked, to its
    // NUL); at most 260 bytes.
    (0..260u32).map(|i| unsafe { *((at + i) as usize as *const u8) }).take_while(|&b| b != 0).collect()
}

/// +0x04 `load_file(path, face, height, flags)` (0x3C3E0): the font file
/// added to the system's fonts (a `.fot` resource made beside it), then
/// +0x08.
extern "thiscall" fn load_file(this: u32, path: u32, face: u32, height: u32, flags: u32) -> u32 {
    // SAFETY: our font's +0x0C, no arguments (through the table, as jgl).
    unsafe { call::method(this, 0x0C, &[]) };
    if path == 0 || face == 0 {
        return jgl::object::error::NULL;
    }
    let file = c_str(path);
    let mut fot = file.clone();
    let n = fot.len().saturating_sub(4);
    fot.truncate(n);
    fot.extend_from_slice(b".fot\0");
    let cwd = std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    let mut full: Vec<u8> = cwd.into_bytes();
    full.push(b'\\');
    full.extend_from_slice(&file);
    full.push(0);
    // SAFETY: GDI calls with NUL-terminated strings of ours.
    unsafe {
        use windows_sys::Win32::Foundation::GetLastError;
        if gdi::CreateScalableFontResourceA(0, fot.as_ptr(), full.as_ptr(), std::ptr::null()) == 0 {
            let e = GetLastError();
            if e != 0 && e != 0x50 {
                return jgl::object::error::FONT;
            }
        }
        if gdi::AddFontResourceA(fot.as_ptr()) == 0 {
            return jgl::object::error::FONT_RESOURCE;
        }
        windows_sys::Win32::UI::WindowsAndMessaging::PostMessageA(windows_sys::Win32::UI::WindowsAndMessaging::HWND_BROADCAST, 0x1D, 0, 0);
    }
    // Kept for its release (jgl's malloc'd copy).
    let kept: &'static mut [u8] = Box::leak(fot.into_boxed_slice());
    set(this, RESOURCE, kept.as_ptr() as usize as u32);
    set(this, FLAGS, field(this, FLAGS) | 1);
    // SAFETY: our font's +0x08, three words (through the table, as jgl).
    unsafe { call::method(this, 0x08, &[face, height, flags]) }
}

/// +0x08 `create(face, height, flags)`'s original (0x3C270): the GDI font,
/// its metrics into the game's font; 0, 3 for no face, 0xD if GDI made
/// none.
pub extern "thiscall" fn create(this: u32, face: u32, height: u32, flags: u32) -> u32 {
    if face == 0 {
        return jgl::object::error::ARGUMENT;
    }
    let metrics = field(this, METRICS);
    if field(this, FLAGS) & 1 == 0 {
        release(this);
    } else {
        for word in [2, 3, 5, 6] {
            // SAFETY: the game's font metrics (words 2..=6 of it).
            unsafe { put(metrics + 4 * word, 0) };
        }
        let old = field(this, HFONT);
        if old != 0 {
            // SAFETY: the HFONT this font made.
            unsafe { gdi::DeleteObject(old as usize as gdi::HGDIOBJ) };
        }
    }
    let (lf_height, weight, italic, underline) = jgl::object::logfont(height as i32, flags);
    // SAFETY: LOGFONTA is plain data.
    let mut lf: gdi::LOGFONTA = unsafe { std::mem::zeroed() };
    lf.lfHeight = lf_height;
    lf.lfWeight = weight;
    lf.lfItalic = italic as u8;
    lf.lfUnderline = underline as u8;
    lf.lfOutPrecision = 7;
    // jgl's strcpy, unbounded: the game's names fit.
    for (d, s) in lf.lfFaceName.iter_mut().zip(c_str(face).into_iter().take(31)) {
        *d = s as i8;
    }
    // SAFETY: GDI calls on a LOGFONT of ours and graphics +0xB8's DC.
    unsafe {
        let font = gdi::CreateFontIndirectA(&lf);
        set(this, HFONT, font as usize as u32);
        if font.is_null() {
            return jgl::object::error::FONT;
        }
        let dc = super::graphics::memory_dc() as usize as gdi::HDC;
        gdi::SelectObject(dc, font as gdi::HGDIOBJ);
        let mut tm: gdi::TEXTMETRICA = std::mem::zeroed();
        gdi::GetTextMetricsA(dc, &mut tm);
        let m = jgl::object::font_metrics(tm.tmHeight, tm.tmAscent, tm.tmDescent, tm.tmInternalLeading, tm.tmExternalLeading);
        for (i, v) in m.into_iter().enumerate() {
            put(metrics + 4 * (2 + i as u32), v as u32);
        }
        gdi::SelectObject(dc, gdi::GetStockObject(gdi::SYSTEM_FONT));
    }
    0
}

/// +0x0C `release`'s original (0x3C5B0): the GDI font, and a file's
/// resource, go.
pub extern "thiscall" fn release(this: u32) -> u32 {
    let font = field(this, HFONT);
    if font != 0 {
        // SAFETY: the HFONT this font made.
        unsafe { gdi::DeleteObject(font as usize as gdi::HGDIOBJ) };
        set(this, HFONT, 0);
    }
    let resource = field(this, RESOURCE);
    if resource != 0 {
        // SAFETY: the `.fot` name `load_file` kept (NUL-terminated).
        unsafe {
            gdi::RemoveFontResourceA(resource as usize as *const u8);
            windows_sys::Win32::UI::WindowsAndMessaging::PostMessageA(windows_sys::Win32::UI::WindowsAndMessaging::HWND_BROADCAST, 0x1D, 0, 0);
        }
        set(this, RESOURCE, 0);
    }
    0
}

/// +0x10 `measure(text, len)` (0x3C600): the text's width in this font
/// (`GetTextExtentPoint32A` in the memory DC); 0 for no text.
extern "thiscall" fn measure(this: u32, text: u32, len: u32) -> u32 {
    if text == 0 {
        return 0;
    }
    let dc = super::graphics::memory_dc() as usize as gdi::HDC;
    let mut size = windows_sys::Win32::Foundation::SIZE { cx: 0, cy: 0 };
    // SAFETY: GDI calls; `text` is the game's string of `len` bytes, read
    // by GDI as jgl passes it.
    unsafe {
        gdi::SelectObject(dc, field(this, HFONT) as usize as gdi::HGDIOBJ);
        gdi::GetTextExtentPoint32A(dc, text as usize as *const u8, len as i32, &mut size);
        gdi::SelectObject(dc, gdi::GetStockObject(gdi::SYSTEM_FONT));
    }
    size.cx as u32
}

type M0 = extern "thiscall" fn(u32) -> u32;
type M1 = extern "thiscall" fn(u32, u32) -> u32;
type M2 = extern "thiscall" fn(u32, u32, u32) -> u32;
type M3 = extern "thiscall" fn(u32, u32, u32, u32) -> u32;
type M4 = extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32;

/// Our implementation of font slot `offset` (0: none).
pub fn slot(offset: u16) -> u32 {
    let f: usize = match offset {
        0x00 => scalar_delete as M1 as usize,
        0x04 => load_file as M4 as usize,
        0x08 => create as M3 as usize,
        0x0C => release as M0 as usize,
        0x10 => measure as M2 as usize,
        _ => {
            return match Class::Font.slot(offset) {
                Some(s) if s.group == jgl::slots::Group::Stub => stubs::by_stack(s.stack),
                _ => 0,
            };
        }
    };
    f as u32
}
