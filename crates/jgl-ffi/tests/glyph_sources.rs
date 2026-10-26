//! The glyph source of the keyed hit queries against `TextOutA` itself:
//! runs drawn as jgl draws text (16-bit DIB, baseline aligned, transparent
//! background) in the game's own font. Every pixel the source decides must
//! match; undecided pixels are counted, never guessed.
#![cfg(all(windows,target_arch="x86"))]
use jgl::text::Ink;
use windows_sys::Win32::Graphics::Gdi::*;

#[test]
fn glyph_ink_matches_text_out() {
    let font_file=std::ffi::CString::new(std::env::var("GLYPH_FONT").expect("the game's LSANS.TTF")).unwrap();
    // SAFETY: a NUL-terminated path.
    unsafe { assert!(AddFontResourceExA(font_file.as_ptr().cast(),FR_PRIVATE,std::ptr::null())>0); }
    let texts=["-25: Science","Income: 65","From other civs: +0","Expenses: 73","Tenochtitlan (Aztec) : 3","Ünïcödé àçcénts — ÿ","WWW iii ,;:'\"!?"];
    let (mut decided,mut partial,mut contradictions,mut fonts,mut by_level)=(0,0,0,0,0);
    for height in [-10,-12,-14,-16,-18,-24] { for weight in [0,700] { for italic in [false,true] { for underline in [false,true] {
        let desc=jgl::FontDesc {face:"Lucida Sans".into(),height,weight,italic,underline};
        let geometry=jgl_ffi::geometry::font(&desc).expect("geometry");
        assert!(geometry.calibrated(),"{desc:?} is not reproduced");
        fonts+=1;
        for (text,black) in texts.iter().flat_map(|&t|[(t,false),(t,true)]) {
            let bytes:Vec<u8>=text.chars().map(|c|u8::try_from(u32::from(c)).unwrap_or(b'?')).collect();
            let text:String=bytes.iter().map(|&b|b as char).collect();
            let (w,h,x0,y0)=(700i32,90i32,20i32,50i32);
            // SAFETY: GDI objects owned here, buffers sized to the DIB.
            unsafe {
                let dc=CreateCompatibleDC(std::ptr::null_mut());
                let mut info:BITMAPINFO=std::mem::zeroed();
                info.bmiHeader.biSize=size_of::<BITMAPINFOHEADER>() as u32; info.bmiHeader.biWidth=w; info.bmiHeader.biHeight=-h; info.bmiHeader.biPlanes=1; info.bmiHeader.biBitCount=16; info.bmiHeader.biCompression=BI_RGB;
                let mut bits=std::ptr::null_mut();
                let dib=CreateDIBSection(dc,&info,DIB_RGB_COLORS,&mut bits,std::ptr::null_mut(),0);
                let old=SelectObject(dc,dib);
                let px=std::slice::from_raw_parts_mut(bits as *mut u16,(w*h) as usize);
                px.fill(0x7C1F);
                let mut lf:LOGFONTA=std::mem::zeroed();
                lf.lfHeight=height; lf.lfWeight=weight; lf.lfItalic=italic as u8; lf.lfUnderline=underline as u8; lf.lfOutPrecision=7;
                for (d,s) in lf.lfFaceName.iter_mut().zip(b"Lucida Sans") {*d=*s as i8;}
                let font=CreateFontIndirectA(&lf);
                let old_font=SelectObject(dc,font);
                SetBkMode(dc,TRANSPARENT as i32); SetTextAlign(dc,TA_BASELINE); SetTextColor(dc,if black {0} else {0x00_30_60_90});
                TextOutA(dc,x0,y0,bytes.as_ptr(),bytes.len() as i32);
                GdiFlush();
                let color=if black {0} else {(0x90u16>>3)<<10 | (0x60u16>>3)<<5 | 0x30>>3};
                for y in 0..h { for x in 0..w {
                    let p=px[(y*w+x) as usize];
                    match geometry.ink(&text,x-x0,y-y0,underline) {
                        Some(Ink::None) if p==0x7C1F => decided+=1,
                        Some(Ink::Solid) if p==color => decided+=1,
                        // Over the key, black text's level fixes the pixel.
                        Some(Ink::Partial(Some(level))) if black && geometry.black_on_key(level).is_some() => {
                            if geometry.black_on_key(level)==Some(p) {decided+=1; by_level+=1;}
                            else { contradictions+=1; if contradictions<=10 {println!("{desc:?} {text:?} level {level}: {:?} vs {p:#x}",geometry.black_on_key(level));} }
                        }
                        Some(Ink::Partial(_)) | None => partial+=1,
                        other => { contradictions+=1; if contradictions<=10 {println!("{desc:?} {text:?} ({},{}) {other:?} pixel {p:#x}",x-x0,y-y0);} }
                    }
                }}
                SelectObject(dc,old_font); DeleteObject(font); SelectObject(dc,old); DeleteObject(dib); DeleteDC(dc);
            }
        }
    }}}}
    println!("glyph ink: {fonts} fonts, {decided} decided pixels ({by_level} smoothed black-on-key by level), {partial} undecided, {contradictions} contradictions");
    assert_eq!(contradictions,0);
}
