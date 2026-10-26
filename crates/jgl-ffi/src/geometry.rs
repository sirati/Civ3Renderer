//! Font-engine metadata only. No text or canvas bitmap is drawn or read.

pub fn font(desc: &jgl::FontDesc) -> Option<jgl::text::FontGeometry> {
    #[cfg(windows)]
    return windows(desc);
    #[cfg(not(windows))]
    { let _=desc; None }
}

#[cfg(windows)]
fn windows(desc: &jgl::FontDesc) -> Option<jgl::text::FontGeometry> {
    use windows_sys::Win32::Graphics::Gdi::*;
    // SAFETY: all input structs and output buffers live through the calls.
    // The temporary DC and font are owned here and deleted after restoring
    // the original selection. GGO_METRICS requests no glyph bitmap.
    unsafe {
        let mut lf:LOGFONTA=std::mem::zeroed();
        lf.lfHeight=desc.height;
        lf.lfWeight=desc.weight;
        lf.lfItalic=desc.italic as u8;
        lf.lfUnderline=desc.underline as u8;
        lf.lfOutPrecision=7;
        for (d,s) in lf.lfFaceName.iter_mut().zip(desc.face.bytes().take(31)) { *d=s as i8; }
        let dc=CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() {return None;}
        let font=CreateFontIndirectA(&lf);
        if font.is_null() {DeleteDC(dc);return None;}
        let old=SelectObject(dc,font);
        let one=FIXED {fract:0,value:1}; let zero=FIXED {fract:0,value:0};
        let mat=MAT2 {eM11:one,eM12:zero,eM21:zero,eM22:one};
        let glyphs=(0..256u32).map(|ch| {
            let mut m:GLYPHMETRICS=std::mem::zeroed();
            (GetGlyphOutlineA(dc,ch,GGO_METRICS,&mut m,0,std::ptr::null_mut(),&mat)!=u32::MAX)
                .then(|| jgl::text::GlyphGeometry::from_metrics(m.gmptGlyphOrigin.x,m.gmptGlyphOrigin.y,m.gmBlackBoxX,m.gmBlackBoxY,i32::from(m.gmCellIncX)))
        }).collect();
        let mut metrics:OUTLINETEXTMETRICA=std::mem::zeroed();
        let underline=(GetOutlineTextMetricsA(dc,size_of::<OUTLINETEXTMETRICA>() as u32,&mut metrics)!=0)
            .then(||jgl::text::underline_geometry(metrics.otmsUnderscorePosition,metrics.otmsUnderscoreSize));
        SelectObject(dc,old);
        DeleteObject(font);
        DeleteDC(dc);
        Some(jgl::text::FontGeometry {glyphs,underline})
    }
}
