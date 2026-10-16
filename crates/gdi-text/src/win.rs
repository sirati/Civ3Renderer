//! The GDI calls.

use windows_sys::Win32::Foundation::SIZE;
use windows_sys::Win32::Graphics::Gdi::*;

use crate::{FontSpec, GdiError, Glyph};

pub fn add_font_file(path: &std::path::Path) -> Result<(), GdiError> {
    let wide: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().chain([0]).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call.
    let n = unsafe { AddFontResourceExW(wide.as_ptr(), FR_PRIVATE, std::ptr::null()) };
    if n == 0 { Err(GdiError::AddFont(path.display().to_string())) } else { Ok(()) }
}

/// A memory DC with a selected font and a white-on-black 1-bit-ish DIB.
struct Canvas {
    dc: HDC,
    font: HFONT,
    bitmap: HBITMAP,
    bits: *mut u32,
    size: (i32, i32),
}

impl Drop for Canvas {
    fn drop(&mut self) {
        // SAFETY: handles created in `Canvas::new`, deleted once.
        unsafe {
            DeleteDC(self.dc);
            DeleteObject(self.font);
            DeleteObject(self.bitmap);
        }
    }
}

/// Default quality on a 32-bit DIB may pick ClearType (colour fringes); the
/// games draw on 16-bit surfaces, where GDI smooths in grey only.
fn quality(q: u8) -> u32 {
    if q == DEFAULT_QUALITY as u8 { ANTIALIASED_QUALITY as u32 } else { q as u32 }
}

impl Canvas {
    fn new(font: &FontSpec, size: (i32, i32)) -> Result<Canvas, GdiError> {
        let face: Vec<u16> = font.face.encode_utf16().chain([0]).collect();
        // SAFETY: plain GDI object creation; every handle is checked and
        // owned by the returned Canvas.
        unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            if dc.is_null() {
                return Err(GdiError::Call("CreateCompatibleDC"));
            }
            let hfont = CreateFontW(
                -font.height, 0, 0, 0, font.weight, font.italic as u32, 0, 0,
                DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32,
                quality(font.quality), DEFAULT_PITCH as u32, face.as_ptr(),
            );
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = size.0;
            info.bmiHeader.biHeight = -size.1;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            let canvas = Canvas { dc, font: hfont, bitmap, bits: bits as *mut u32, size };
            if hfont.is_null() || bitmap.is_null() || bits.is_null() {
                return Err(GdiError::Call("CreateFontW/CreateDIBSection"));
            }
            SelectObject(dc, bitmap);
            SelectObject(dc, hfont);
            SetBkMode(dc, TRANSPARENT as i32);
            SetTextColor(dc, 0x00FF_FFFF);
            Ok(canvas)
        }
    }

    fn pixels(&self) -> &[u32] {
        // SAFETY: the DIB section holds size.0 × size.1 pixels and lives as
        // long as `self`; GDI has finished drawing (GdiFlush below).
        unsafe {
            GdiFlush();
            std::slice::from_raw_parts(self.bits, (self.size.0 * self.size.1) as usize)
        }
    }

    fn clear(&mut self) {
        // SAFETY: as in `pixels`, exclusive through &mut self.
        unsafe { std::ptr::write_bytes(self.bits, 0, (self.size.0 * self.size.1) as usize) };
    }
}

pub fn render_glyphs(font: &FontSpec, chars: impl IntoIterator<Item = char>) -> Result<crate::Font, GdiError> {
    let cell = font.height.abs() * 3;
    let mut canvas = Canvas::new(font, (cell * 2, cell))?;
    let mut metrics: TEXTMETRICW = unsafe { std::mem::zeroed() };
    // SAFETY: writes only `metrics`.
    if unsafe { GetTextMetricsW(canvas.dc, &mut metrics) } == 0 {
        return Err(GdiError::Call("GetTextMetricsW"));
    }
    let line = metrics.tmHeight;
    let origin = font.height.abs(); // pen x, leaves room for negative bearings
    let mut out = Vec::new();
    for ch in chars {
        let mut units = [0u16; 2];
        let units = ch.encode_utf16(&mut units);
        let mut size = SIZE { cx: 0, cy: 0 };
        canvas.clear();
        // SAFETY: `units` outlives the calls; output structs are local.
        unsafe {
            GetTextExtentPoint32W(canvas.dc, units.as_ptr(), units.len() as i32, &mut size);
            TextOutW(canvas.dc, origin, 0, units.as_ptr(), units.len() as i32);
        }
        let px = canvas.pixels();
        let (w, h) = canvas.size;
        // White text on black: any channel is the coverage (grey where GDI
        // smooths the edges).
        let ink_at = |x: i32, y: i32| (px[(y * w + x) as usize] >> 8) as u8;
        let cols: Vec<i32> = (0..w).filter(|&x| (0..line.min(h)).any(|y| ink_at(x, y) != 0)).collect();
        let (x0, x1) = match (cols.first(), cols.last()) {
            (Some(&a), Some(&b)) => (a, b + 1),
            _ => (origin, origin),
        };
        let height = line.min(h) as u32;
        let ink = (0..height as i32).flat_map(|y| (x0..x1).map(move |x| (x, y))).map(|(x, y)| ink_at(x, y)).collect();
        out.push(Glyph { ch, left: x0 - origin, advance: size.cx, width: (x1 - x0) as u32, height, ink });
    }
    Ok(crate::Font { ascent: metrics.tmAscent, underline: underline(&canvas, &metrics), glyphs: out })
}

/// GDI's underline: centred on `otmsUnderscorePosition` (up from the
/// baseline), `otmsUnderscoreSize` thick; one row under the baseline if
/// the font has no outline metrics.
fn underline(canvas: &Canvas, metrics: &TEXTMETRICW) -> (i32, i32) {
    let mut otm: OUTLINETEXTMETRICW = unsafe { std::mem::zeroed() };
    // SAFETY: writes at most `size_of::<OUTLINETEXTMETRICW>()` bytes into `otm`.
    let n = unsafe { GetOutlineTextMetricsW(canvas.dc, size_of::<OUTLINETEXTMETRICW>() as u32, &mut otm) };
    if n == 0 {
        return (1.min(metrics.tmDescent), 1);
    }
    let size = (otm.otmsUnderscoreSize as i32).max(1);
    (-otm.otmsUnderscorePosition - size / 2, size)
}
