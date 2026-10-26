//! Font-engine glyph data for `jgl::text::FontGeometry`: each glyph's mono
//! and grey rasters, and calibration `TextOutA` lines of the whole set into
//! a small 16-bit DIB of our own (as jgl's images: baseline aligned,
//! transparent background, over the colour key), from which the safe side
//! decides which rasters reproduce the game's text. No game text or canvas
//! is drawn or read.

/// The glyphs of `desc` and how `TextOutA` draws them in black.
pub fn font(desc: &jgl::FontDesc) -> Option<jgl::text::FontGeometry> {
    #[cfg(windows)]
    return windows::font(desc);
    #[cfg(not(windows))]
    { let _=desc; None }
}

/// For `desc`'s text in `rgb`: the pixel each grey level makes over the
/// colour key ([`jgl::text::FontGeometry::key_levels`]).
pub fn key_levels(desc: &jgl::FontDesc, geometry: &jgl::text::FontGeometry, rgb: [u8; 3]) -> Option<[Option<u16>; 17]> {
    #[cfg(windows)]
    return windows::key_levels(desc, geometry, rgb);
    #[cfg(not(windows))]
    { let _=(desc, geometry, rgb); None }
}

#[cfg(windows)]
mod windows {
    use windows_sys::Win32::Graphics::Gdi::*;

    fn logfont(desc: &jgl::FontDesc, underline: bool) -> LOGFONTA {
        // SAFETY: LOGFONTA is plain data; all zero is a valid value.
        let mut lf: LOGFONTA = unsafe { std::mem::zeroed() };
        lf.lfHeight = desc.height;
        lf.lfWeight = desc.weight;
        lf.lfItalic = desc.italic as u8;
        lf.lfUnderline = underline as u8;
        lf.lfOutPrecision = 7;
        for (d, s) in lf.lfFaceName.iter_mut().zip(desc.face.bytes().take(31)) { *d = s as i8; }
        lf
    }

    /// `f` on a memory DC with `desc` (without its underline) selected.
    fn with_font<T>(desc: &jgl::FontDesc, f: impl FnOnce(HDC) -> T) -> Option<T> {
        // SAFETY: the DC and font are owned here and deleted after the
        // font's selection is undone; `f` gets the DC only for the call.
        unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            if dc.is_null() { return None; }
            let font = CreateFontIndirectA(&logfont(desc, false));
            if font.is_null() { DeleteDC(dc); return None; }
            let old = SelectObject(dc, font);
            let out = f(dc);
            SelectObject(dc, old);
            DeleteObject(font);
            DeleteDC(dc);
            Some(out)
        }
    }

    /// Each glyph's rasters, each format with its own metrics: mono and
    /// grey are hinted apart.
    fn glyphs(dc: HDC) -> Vec<Option<jgl::text::Glyph>> {
        let one = FIXED { fract: 0, value: 1 };
        let zero = FIXED { fract: 0, value: 0 };
        let mat = MAT2 { eM11: one, eM12: zero, eM21: zero, eM22: one };
        let raster = |ch: u32, format: GET_GLYPH_OUTLINE_FORMAT, gray: bool| -> Option<jgl::text::Raster> {
            // SAFETY: the metrics and buffer outlive the calls; the size
            // passed is the buffer's.
            unsafe {
                let mut m: GLYPHMETRICS = std::mem::zeroed();
                let n = GetGlyphOutlineA(dc, ch, format, &mut m, 0, std::ptr::null_mut(), &mat);
                if n == u32::MAX { return None; }
                let mut buf = vec![0u8; n as usize];
                if n > 0 && GetGlyphOutlineA(dc, ch, format, &mut m, n, buf.as_mut_ptr().cast(), &mat) == u32::MAX { return None; }
                // An empty glyph reports a 1x1 box and no bytes.
                let (w, h) = if n == 0 { (0, 0) } else { (m.gmBlackBoxX as usize, m.gmBlackBoxY as usize) };
                let row = if gray { w.div_ceil(4) * 4 } else { w.div_ceil(32) * 4 };
                let at = |i: usize| buf.get(i).copied().unwrap_or(0);
                let levels = (0..h * w).map(|i| {
                    let (x, y) = (i % w, i / w);
                    if gray { at(y * row + x) } else { at(y * row + x / 8) >> (7 - x % 8) & 1 }
                }).collect();
                Some(jgl::text::Raster { origin: (m.gmptGlyphOrigin.x, m.gmptGlyphOrigin.y), size: (w as i32, h as i32), advance: i32::from(m.gmCellIncX), levels })
            }
        };
        (0..256u32).map(|ch| {
            Some(jgl::text::Glyph { mono: raster(ch, GGO_BITMAP, false)?, gray: raster(ch, GGO_GRAY4_BITMAP, true)? })
        }).collect()
    }

    /// The glyph set in lines of 32, drawn in `rgb` as jgl draws text.
    fn calibrate(dc: HDC, glyphs: &[Option<jgl::text::Glyph>], rgb: [u8; 3]) -> Vec<jgl::text::Calibration> {
        let present: Vec<u8> = (32..=255u8).filter(|&b| glyphs[b as usize].is_some()).collect();
        let color = u16::from(rgb[0] >> 3) << 10 | u16::from(rgb[1] >> 3) << 5 | u16::from(rgb[2] >> 3);
        let mut calibration = Vec::new();
        // SAFETY: the DIB is owned here and deleted after its selection is
        // undone; its pixels are read only while it exists, within the size
        // it was created with.
        unsafe {
            let mut tm: TEXTMETRICA = std::mem::zeroed();
            GetTextMetricsA(dc, &mut tm);
            let pad = tm.tmMaxCharWidth.max(8) * 2;
            let (pen_y, height) = (tm.tmAscent + pad, tm.tmHeight + 2 * pad);
            for text in present.chunks(32) {
                let width: i32 = text.iter().map(|&b| glyphs[b as usize].as_ref().map_or(0, |g| g.mono.advance.max(g.gray.advance))).sum::<i32>() + 2 * pad;
                let mut info: BITMAPINFO = std::mem::zeroed();
                info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
                info.bmiHeader.biWidth = width;
                info.bmiHeader.biHeight = -height;
                info.bmiHeader.biPlanes = 1;
                info.bmiHeader.biBitCount = 16;
                info.bmiHeader.biCompression = BI_RGB;
                let mut bits = std::ptr::null_mut();
                let dib = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
                if dib.is_null() || bits.is_null() { return Vec::new(); }
                let old = SelectObject(dc, dib);
                // A 16-bit DIB's rows are padded to four bytes.
                let row = (width as usize).div_ceil(2) * 2;
                let pixels = std::slice::from_raw_parts_mut(bits as *mut u16, row * height as usize);
                pixels.fill(0x7C1F);
                SetBkMode(dc, TRANSPARENT as i32);
                SetTextAlign(dc, TA_BASELINE);
                SetTextColor(dc, u32::from(rgb[0]) | u32::from(rgb[1]) << 8 | u32::from(rgb[2]) << 16);
                TextOutA(dc, pad, pen_y, text.as_ptr(), text.len() as i32);
                GdiFlush();
                let pixels = (0..height as usize).flat_map(|y| pixels[y * row..y * row + width as usize].iter().copied()).collect();
                calibration.push(jgl::text::Calibration { color, text: text.to_vec(), pen: (pad, pen_y), width, pixels });
                SelectObject(dc, old);
                DeleteObject(dib);
            }
        }
        calibration
    }

    pub fn font(desc: &jgl::FontDesc) -> Option<jgl::text::FontGeometry> {
        let (glyphs, calibration) = with_font(desc, |dc| {
            let glyphs = glyphs(dc);
            let calibration = calibrate(dc, &glyphs, [0; 3]);
            (glyphs, calibration)
        })?;
        let underline = if desc.underline { underline(desc) } else { None };
        Some(jgl::text::FontGeometry::new(glyphs, &calibration, underline))
    }

    pub fn key_levels(desc: &jgl::FontDesc, geometry: &jgl::text::FontGeometry, rgb: [u8; 3]) -> Option<[Option<u16>; 17]> {
        let calibration = with_font(desc, |dc| calibrate(dc, &glyphs(dc), rgb))?;
        geometry.key_levels(&calibration)
    }

    /// The underlined font's underscore position and size.
    fn underline(desc: &jgl::FontDesc) -> Option<(i32, i32)> {
        // SAFETY: as `with_font`, for the underlined font; the metrics
        // buffer is the size passed.
        unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            if dc.is_null() { return None; }
            let font = CreateFontIndirectA(&logfont(desc, true));
            if font.is_null() { DeleteDC(dc); return None; }
            let old = SelectObject(dc, font);
            let mut otm: OUTLINETEXTMETRICA = std::mem::zeroed();
            let got = GetOutlineTextMetricsA(dc, size_of::<OUTLINETEXTMETRICA>() as u32, &mut otm) != 0;
            SelectObject(dc, old);
            DeleteObject(font);
            DeleteDC(dc);
            got.then_some((otm.otmsUnderscorePosition, otm.otmsUnderscoreSize))
        }
    }
}
