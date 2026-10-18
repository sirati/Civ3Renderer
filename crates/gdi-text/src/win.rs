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

/// A memory DC with a selected font and a 32-bit DIB to draw text on.
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
                font.quality as u32, DEFAULT_PITCH as u32, face.as_ptr(),
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
            Ok(canvas)
        }
    }

    /// `text` drawn with its pen at (x, 0) in `color` over `background`
    /// (0xRRGGBB); the pixels as (red, green, blue).
    fn draw(&mut self, text: &[u16], x: i32, color: u32, background: u32) -> Vec<[u8; 3]> {
        let n = (self.size.0 * self.size.1) as usize;
        // SAFETY: the DIB section holds n pixels and lives as long as `self`,
        // exclusive through &mut self; GDI is flushed before it is read.
        unsafe {
            std::slice::from_raw_parts_mut(self.bits, n).fill(background);
            SetTextColor(self.dc, (color & 0xFF) << 16 | (color & 0xFF00) | (color >> 16 & 0xFF));
            TextOutW(self.dc, x, 0, text.as_ptr(), text.len() as i32);
            GdiFlush();
            std::slice::from_raw_parts(self.bits, n).iter().map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8]).collect()
        }
    }

    /// The pen advance of each character of `text` as text output places
    /// them (`GetCharacterPlacementW`; text extents can differ from it).
    fn advances(&self, text: &[u16]) -> Vec<i32> {
        let mut dx = vec![0i32; text.len()];
        // SAFETY: a plain C struct, all zeroes is valid.
        let mut results: GCP_RESULTSW = unsafe { std::mem::zeroed() };
        results.lStructSize = size_of::<GCP_RESULTSW>() as u32;
        results.lpDx = dx.as_mut_ptr();
        results.nGlyphs = text.len() as u32;
        // SAFETY: `text` and `dx` (`nGlyphs` entries) outlive the call, which
        // writes at most that many advances.
        unsafe { GetCharacterPlacementW(self.dc, text.as_ptr(), text.len() as i32, 0, &mut results, 0) };
        dx
    }
}

/// A memory DC with only `font` selected: its default bitmap is one
/// monochrome pixel, as that of the DC `jgl.dll` measures text on.
struct MeasureDc {
    dc: HDC,
    font: HFONT,
}

impl MeasureDc {
    fn new(font: &FontSpec) -> Result<MeasureDc, GdiError> {
        let face: Vec<u16> = font.face.encode_utf16().chain([0]).collect();
        // SAFETY: plain GDI object creation; every handle is checked and
        // owned by the returned value.
        unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            if dc.is_null() {
                return Err(GdiError::Call("CreateCompatibleDC"));
            }
            // As the game's fonts: ANSI character set, swiss, variable pitch.
            let hfont = CreateFontW(
                -font.height, 0, 0, 0, font.weight, font.italic as u32, 0, 0,
                ANSI_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32,
                font.quality as u32, (FF_SWISS | VARIABLE_PITCH) as u32, face.as_ptr(),
            );
            if hfont.is_null() {
                DeleteDC(dc);
                return Err(GdiError::Call("CreateFontW"));
            }
            SelectObject(dc, hfont);
            Ok(MeasureDc { dc, font: hfont })
        }
    }

    /// `GetTextExtentPoint32A`'s width.
    fn extent(&self, text: &[u8]) -> Result<i32, GdiError> {
        let mut size = SIZE { cx: 0, cy: 0 };
        // SAFETY: `text` outlives the call, which writes only `size`.
        if unsafe { GetTextExtentPoint32A(self.dc, text.as_ptr(), text.len() as i32, &mut size) } == 0 {
            return Err(GdiError::Call("GetTextExtentPoint32A"));
        }
        Ok(size.cx)
    }
}

impl Drop for MeasureDc {
    fn drop(&mut self) {
        // SAFETY: handles created in `MeasureDc::new`, deleted once.
        unsafe {
            DeleteDC(self.dc);
            DeleteObject(self.font);
        }
    }
}

pub fn text_extent(font: &FontSpec, text: &[u8]) -> Result<i32, GdiError> {
    MeasureDc::new(font)?.extent(text)
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
    let measure = MeasureDc::new(font)?;
    let mut out = Vec::new();
    for ch in chars {
        let mut units = [0u16; 2];
        let units: &[u16] = crate::windows_1252_char(ch).encode_utf16(&mut units);
        // The pen advance as text output places it (of the first of two, so
        // no end-of-string adjustment applies).
        let twice: Vec<u16> = units.iter().chain(units).copied().collect();
        let advance = canvas.advances(&twice).first().copied().unwrap_or(0);
        let light = canvas.draw(units, origin, 0xFF_FFFF, 0);
        let dark: Vec<[u8; 3]> = canvas.draw(units, origin, 0, 0xFF_FFFF).into_iter().map(|p| p.map(|c| 255 - c)).collect();
        let (w, h) = canvas.size;
        let height = line.min(h);
        let at = |x: i32, y: i32| (y * w + x) as usize;
        let cols: Vec<i32> = (0..w).filter(|&x| (0..height).any(|y| dark[at(x, y)] != [0; 3] || light[at(x, y)] != [0; 3])).collect();
        let (x0, x1) = match (cols.first(), cols.last()) {
            (Some(&a), Some(&b)) => (a, b + 1),
            _ => (origin, origin),
        };
        let crop = |m: &[[u8; 3]]| (0..height).flat_map(|y| (x0..x1).map(move |x| (x, y))).map(|(x, y)| m[at(x, y)]).collect();
        // The original Windows-1252 byte, before Unicode decoding.
        let extent = u8::try_from(ch as u32).map_or(Ok(advance), |b| measure.extent(&[b]))?;
        out.push(Glyph {
            ch,
            left: x0 - origin,
            advance,
            extent,
            width: (x1 - x0) as u32,
            height: height as u32,
            dark: crop(&dark),
            light: crop(&light),
        });
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

pub fn under_wine() -> bool {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    let ntdll: Vec<u16> = "ntdll.dll".encode_utf16().chain([0]).collect();
    // SAFETY: NUL-terminated names that outlive the calls; a missing
    // module or export gives null.
    unsafe {
        let module = GetModuleHandleW(ntdll.as_ptr());
        !module.is_null() && GetProcAddress(module, c"wine_get_version".as_ptr().cast()).is_some()
    }
}
