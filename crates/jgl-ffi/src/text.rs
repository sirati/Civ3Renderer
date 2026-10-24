//! The text group (`docs/jgl-replacement.md` §6, group 1): jgl's image
//! slots +0x08 (`clear`, watched: the DC goes), +0xA8 `select_font`, +0xAC
//! `deselect_font`, +0xB0 `set_text_color16`, +0xB4 `set_text_color`,
//! +0xB8 `text_out`, and font slots +0x00 (delete), +0x08 `create`, +0x0C
//! `release`.
//!
//! Every entry point records through the game thread's [`jgl::Jgl`]. The
//! state slots then always run jgl's original (a later original
//! `text_out` draws with the DC's font and colour); `text_out` runs it only
//! where [`jgl::PixelPolicy`] wants jgl's pixels. Font +0x04 (load from a
//! file, which ends in +0x08 through the vtable) and +0x10 (measuring)
//! stay jgl's.
//!
//! The entry points read their arguments directly, without guarded reads:
//! `this` is an object of the class (only those dispatch through its
//! vtable), and every other pointer read here is one jgl's original
//! dereferences unchecked too (the font holder and its font, the string).
//! A guarded read would cost a system call per call under FEX.

#![cfg_attr(not(all(windows, target_arch = "x86")), allow(dead_code, unused_imports))]

use std::sync::atomic::AtomicU32;

use jgl::{FontDesc, FontId, ImageId, Producer};

pub use crate::state::{HooksFactory, reentrant, threads};
use crate::state::{FACTORY, STOCK, guarded, record, stock};
use crate::takeover::{Takeover, TakeoverError};

#[cfg(all(windows, target_arch = "x86"))]
pub(crate) mod entries {
    use super::*;
    use crate::layout::{JglFont, JglImage, Rect32};
    use crate::takeover::{Entry, original};
    use std::mem::offset_of;

    pub static IMAGE_CLEAR: AtomicU32 = AtomicU32::new(0);
    pub static SELECT_FONT: AtomicU32 = AtomicU32::new(0);
    pub static DESELECT_FONT: AtomicU32 = AtomicU32::new(0);
    pub static TEXT_COLOR16: AtomicU32 = AtomicU32::new(0);
    pub static TEXT_COLOR: AtomicU32 = AtomicU32::new(0);
    pub static TEXT_OUT: AtomicU32 = AtomicU32::new(0);
    pub static FONT_DELETE: AtomicU32 = AtomicU32::new(0);
    pub static FONT_CREATE: AtomicU32 = AtomicU32::new(0);
    pub static FONT_RELEASE: AtomicU32 = AtomicU32::new(0);

    pub type M0 = extern "thiscall" fn(u32) -> u32;
    pub type M1 = extern "thiscall" fn(u32, u32) -> u32;
    pub type M3 = extern "thiscall" fn(u32, u32, u32, u32) -> u32;
    pub type M4 = extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32;

    /// A `u32` field of an object.
    ///
    /// # Safety
    /// `at` must be readable: a field of the object the entry point was
    /// called on, or memory jgl's original reads unchecked.
    unsafe fn read_u32(at: u32) -> u32 {
        // SAFETY: the caller's contract.
        unsafe { std::ptr::read_unaligned(at as usize as *const u32) }
    }

    fn image_field(image: u32, offset: usize) -> u32 {
        // SAFETY: `image` is the `this` of a `JGL_Image` method, the field
        // is inside its 0x4D4 bytes.
        unsafe { read_u32(image + offset as u32) }
    }

    fn image_clip(image: u32) -> jgl::Rect {
        let at = image + offset_of!(JglImage, clip) as u32;
        // SAFETY: as `image_field`; `Rect32` is plain data.
        let r: Rect32 = unsafe { std::ptr::read_unaligned(at as usize as *const Rect32) };
        r.into()
    }

    fn desc(lf: inproc::gdi::LogFont) -> FontDesc {
        FontDesc { face: lf.face, height: lf.height, weight: lf.weight, italic: lf.italic, underline: lf.underline }
    }

    /// Gives the producer the image's DC state if it predates the takeover.
    fn ensure_dc(p: &mut Producer, image: u32) {
        let id = ImageId(image);
        if !p.needs_dc(id) {
            return;
        }
        match image_field(image, offset_of!(JglImage, dc)) {
            0 => p.image_reset(id),
            dc => {
                let font = inproc::gdi::dc_font(dc).map(desc).unwrap_or_else(stock);
                let color = match inproc::gdi::dc_text_colorref(dc) {
                    u32::MAX => 0,
                    c => c,
                };
                p.dc_state(id, &font, color);
            }
        }
    }

    fn ensure_font(p: &mut Producer, font: u32) {
        if font == 0 || !p.needs_font(FontId(font)) {
            return;
        }
        // SAFETY: a font object jgl's `select_font` dereferences unchecked.
        let hfont = unsafe { read_u32(font + offset_of!(JglFont, hfont) as u32) };
        if let Some(lf) = inproc::gdi::font(hfont) {
            p.font_defined(FontId(font), &desc(lf));
        }
    }

    pub extern "thiscall" fn image_clear(this: u32) -> u32 {
        guarded("clear", || {
            record(|p| p.image_reset(ImageId(this)));
        }, || ());
        original::<M0>(&IMAGE_CLEAR)(this)
    }

    pub extern "thiscall" fn select_font(this: u32, holder: u32) -> u32 {
        guarded("select_font", || {
            if holder != 0 {
                // SAFETY: jgl's original reads `[holder]` unchecked.
                let font = unsafe { read_u32(holder) };
                record(|p| {
                    ensure_dc(p, this);
                    ensure_font(p, font);
                    p.select_font(ImageId(this), FontId(font));
                });
            }
        }, || ());
        original::<M1>(&SELECT_FONT)(this, holder)
    }

    pub extern "thiscall" fn deselect_font(this: u32) -> u32 {
        guarded("deselect_font", || {
            record(|p| {
                ensure_dc(p, this);
                p.select_font(ImageId(this), FontId::STOCK);
            });
        }, || ());
        original::<M0>(&DESELECT_FONT)(this)
    }

    pub extern "thiscall" fn set_text_color16(this: u32, color: u32) -> u32 {
        guarded("set_text_color16", || {
            record(|p| {
                ensure_dc(p, this);
                p.text_color(ImageId(this), 0x10FF_0000 | (color & 0xFFFF));
            });
        }, || ());
        original::<M1>(&TEXT_COLOR16)(this, color)
    }

    pub extern "thiscall" fn set_text_color(this: u32, r: u32, g: u32, b: u32) -> u32 {
        guarded("set_text_color", || {
            record(|p| {
                ensure_dc(p, this);
                p.text_color(ImageId(this), (r & 0xFF) | (g & 0xFF) << 8 | (b & 0xFF) << 16);
            });
        }, || ());
        original::<M3>(&TEXT_COLOR)(this, r, g, b)
    }

    pub extern "thiscall" fn text_out(this: u32, x: u32, y: u32, text: u32, len: u32) -> u32 {
        let jgl_pixels = guarded("text_out", || {
            let n = (len as i32).clamp(0, jgl::record::MAX_TAIL as i32) as usize;
            if text == 0 && n > 0 {
                return true;
            }
            let bytes: &[u8] = if n == 0 {
                &[]
            } else {
                // SAFETY: the string the game hands jgl, which passes it
                // to TextOutA with this length unchecked.
                unsafe { std::slice::from_raw_parts(text as usize as *const u8, n) }
            };
            let owner = image_field(this, offset_of!(JglImage, owner));
            let clip = image_clip(this);
            record(|p| {
                ensure_dc(p, this);
                p.text(ImageId(this), owner, x as i32, y as i32, clip, bytes)
            })
            .unwrap_or(true)
        }, || true);
        // jgl's text_out returns 0.
        if jgl_pixels { original::<M4>(&TEXT_OUT)(this, x, y, text, len) } else { 0 }
    }

    pub extern "thiscall" fn font_delete(this: u32, flags: u32) -> u32 {
        guarded("font_delete", || {
            record(|p| p.font_released(FontId(this)));
        }, || ());
        original::<M1>(&FONT_DELETE)(this, flags)
    }

    pub extern "thiscall" fn font_create(this: u32, face: u32, height: u32, flags: u32) -> u32 {
        // jgl's create releases the old HFONT through the vtable first.
        let r = original::<M3>(&FONT_CREATE)(this, face, height, flags);
        if r == 0 && face != 0 {
            guarded("font_create", || {
                // SAFETY: the face name jgl just copied into its LOGFONT
                // (NUL-terminated, unchecked); at most 64 bytes are read.
                let name: Vec<u8> = (0..64u32).map(|i| unsafe { *((face + i) as usize as *const u8) }).take_while(|&b| b != 0).collect();
                record(|p| p.font_created(FontId(this), height as i32, flags, &name));
            }, || ());
        }
        r
    }

    pub extern "thiscall" fn font_release(this: u32) -> u32 {
        guarded("font_release", || {
            record(|p| p.font_released(FontId(this)));
        }, || ());
        original::<M0>(&FONT_RELEASE)(this)
    }

    /// The group's slots: (image, font).
    pub fn split() -> (Vec<Entry>, Vec<Entry>) {
        let image = vec![
            Entry { offset: 0x08, function: image_clear as M0 as usize as u32, original: &IMAGE_CLEAR },
            Entry { offset: 0xA8, function: select_font as M1 as usize as u32, original: &SELECT_FONT },
            Entry { offset: 0xAC, function: deselect_font as M0 as usize as u32, original: &DESELECT_FONT },
            Entry { offset: 0xB0, function: set_text_color16 as M1 as usize as u32, original: &TEXT_COLOR16 },
            Entry { offset: 0xB4, function: set_text_color as M3 as usize as u32, original: &TEXT_COLOR },
            Entry { offset: 0xB8, function: text_out as M4 as usize as u32, original: &TEXT_OUT },
        ];
        let font = vec![
            Entry { offset: 0x00, function: font_delete as M1 as usize as u32, original: &FONT_DELETE },
            Entry { offset: 0x08, function: font_create as M3 as usize as u32, original: &FONT_CREATE },
            Entry { offset: 0x0C, function: font_release as M0 as usize as u32, original: &FONT_RELEASE },
        ];
        (image, font)
    }
}

/// The taken-over text group (for the life of the process, in practice).
pub struct TextGroup {
    _image: Takeover,
    _font: Takeover,
}

/// Takes over the text group of jgl.dll at `base`; `hooks` makes each
/// drawing thread's hooks. Once per process (a second call fails with
/// `AlreadyInstalled`).
pub fn install(base: u32, hooks: HooksFactory) -> Result<TextGroup, TakeoverError> {
    install_impl(base, hooks)
}

#[cfg(all(windows, target_arch = "x86"))]
fn install_impl(base: u32, hooks: HooksFactory) -> Result<TextGroup, TakeoverError> {
    use crate::takeover::take_over;
    use jgl::slots::Class;
    if FACTORY.set(hooks).is_err() {
        return Err(inproc::hook::HookError::AlreadyInstalled.into());
    }
    set_stock_font();
    let (image, font) = entries::split();
    // Fonts first: an image may select one the moment its slots change.
    let font = take_over(base, Class::Font, &font)?;
    let image = take_over(base, Class::Image, &image)?;
    Ok(TextGroup { _image: image, _font: font })
}

/// GDI's `SYSTEM_FONT`: what a fresh DC and +0xAC select.
#[cfg(all(windows, target_arch = "x86"))]
pub(crate) fn set_stock_font() {
    if let Some(lf) = inproc::gdi::stock_font(13) {
        let _ = STOCK.set(FontDesc { face: lf.face, height: lf.height, weight: lf.weight, italic: lf.italic, underline: lf.underline });
    }
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn install_impl(_base: u32, _hooks: HooksFactory) -> Result<TextGroup, TakeoverError> {
    Err(inproc::hook::HookError::Unsupported.into())
}
