//! Font-engine glyph data for `jgl::text::FontGeometry`: each glyph's mono
//! and grey bitmaps, and one calibration `TextOutA` of the whole set into a
//! small 16-bit DIB of our own (as jgl's images: baseline aligned,
//! transparent background), from which the safe side decides which bitmaps
//! reproduce the game's text. No game text or canvas is drawn or read.

pub fn font(desc: &jgl::FontDesc) -> Option<jgl::text::FontGeometry> {
    #[cfg(windows)]
    return windows(desc);
    #[cfg(not(windows))]
    { let _=desc; None }
}

#[cfg(windows)]
fn windows(desc: &jgl::FontDesc) -> Option<jgl::text::FontGeometry> {
    use windows_sys::Win32::Graphics::Gdi::*;
    let logfont = |underline: bool| {
        // SAFETY: LOGFONTA is plain data; all zero is a valid value.
        let mut lf: LOGFONTA = unsafe { std::mem::zeroed() };
        lf.lfHeight = desc.height;
        lf.lfWeight = desc.weight;
        lf.lfItalic = desc.italic as u8;
        lf.lfUnderline = underline as u8;
        lf.lfOutPrecision = 7;
        for (d, s) in lf.lfFaceName.iter_mut().zip(desc.face.bytes().take(31)) { *d = s as i8; }
        lf
    };
    // SAFETY: every struct and buffer passed lives through its call; the
    // sizes given are those of the buffers. The DC, DIB and fonts are owned
    // here and deleted after their selections are restored.
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() { return None; }
        let plain = CreateFontIndirectA(&logfont(false));
        if plain.is_null() { DeleteDC(dc); return None; }
        let old_font = SelectObject(dc, plain);
        let one = FIXED { fract: 0, value: 1 };
        let zero = FIXED { fract: 0, value: 0 };
        let mat = MAT2 { eM11: one, eM12: zero, eM21: zero, eM22: one };
        // Each format with its own metrics: mono and grey are hinted apart.
        let raster = |ch: u32, format: GET_GLYPH_OUTLINE_FORMAT, gray: bool| -> Option<jgl::text::Raster> {
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
        };
        let glyphs: Vec<Option<jgl::text::Glyph>> = (0..256u32).map(|ch| {
            Some(jgl::text::Glyph { mono: raster(ch, GGO_BITMAP, false)?, gray: raster(ch, GGO_GRAY4_BITMAP, true)? })
        }).collect();
        // Calibration: the glyph set in lines, as jgl draws text.
        let mut tm: TEXTMETRICA = std::mem::zeroed();
        GetTextMetricsA(dc, &mut tm);
        let present: Vec<u8> = (32..=255u8).filter(|&b| glyphs[b as usize].is_some()).collect();
        let pad = tm.tmMaxCharWidth.max(8) * 2;
        let (pen_y, height) = (tm.tmAscent + pad, tm.tmHeight + 2 * pad);
        let mut calibration = Vec::new();
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
            if dib.is_null() || bits.is_null() { break; }
            let old = SelectObject(dc, dib);
            // A 16-bit DIB's rows are padded to four bytes.
            let row = (width as usize).div_ceil(2) * 2;
            let pixels = std::slice::from_raw_parts_mut(bits as *mut u16, row * height as usize);
            pixels.fill(0x7C1F);
            SetBkMode(dc, TRANSPARENT as i32);
            SetTextAlign(dc, TA_BASELINE);
            SetTextColor(dc, 0);
            TextOutA(dc, pad, pen_y, text.as_ptr(), text.len() as i32);
            GdiFlush();
            let pixels = (0..height as usize).flat_map(|y| pixels[y * row..y * row + width as usize].iter().copied()).collect();
            calibration.push(jgl::text::Calibration { text: text.to_vec(), pen: (pad, pen_y), width, pixels });
            SelectObject(dc, old);
            DeleteObject(dib);
        }
        let mut underline = None;
        if desc.underline {
            let lined = CreateFontIndirectA(&logfont(true));
            if !lined.is_null() {
                SelectObject(dc, lined);
                let mut otm: OUTLINETEXTMETRICA = std::mem::zeroed();
                if GetOutlineTextMetricsA(dc, size_of::<OUTLINETEXTMETRICA>() as u32, &mut otm) != 0 {
                    underline = Some((otm.otmsUnderscorePosition, otm.otmsUnderscoreSize));
                }
                SelectObject(dc, plain);
                DeleteObject(lined);
            }
        }
        SelectObject(dc, old_font);
        DeleteObject(plain);
        DeleteDC(dc);
        Some(jgl::text::FontGeometry::new(glyphs, &calibration, underline))
    }
}
