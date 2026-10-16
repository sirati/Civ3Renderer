//! Renders a font's glyphs with GDI into a sheet for a render description.
//!
//!   font-sheet FONT.TTF "Face Name" HEIGHT WEIGHT OUT.png [FIRST LAST [QUALITY]]
//!
//! Writes OUT.png (per channel, the opacity of black text over white),
//! OUT-light.png (of white text over black; see `gdi_text::Glyph`), each
//! with one cell per character FIRST..=LAST, 16 per row, cell = widest
//! glyph × line height, and OUT.toml with each glyph's left bearing and
//! advance.

use std::fmt::Write as _;
use std::process::ExitCode;

fn main() -> ExitCode {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 5 {
        eprintln!("usage: font-sheet FONT.TTF FACE HEIGHT WEIGHT OUT.png [FIRST LAST [QUALITY [ITALIC]]]");
        return ExitCode::from(2);
    }
    let num = |i: usize, d: i64| a.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let (first, last) = (num(5, 32) as u32, num(6, 255) as u32);
    let font = gdi_text::FontSpec { face: a[1].clone(), height: num(2, 12) as i32, weight: num(3, 400) as i32, italic: num(8, 0) != 0, quality: num(7, 0) as u8 };
    if let Err(e) = gdi_text::add_font_file(std::path::Path::new(&a[0])) {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }
    let chars: Vec<char> = (first..=last).filter_map(char::from_u32).collect();
    let rendered = match gdi_text::render_glyphs(&font, chars) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let glyphs = &rendered.glyphs;
    let cw = glyphs.iter().map(|g| g.width).max().unwrap_or(1).max(1);
    let ch = glyphs.iter().map(|g| g.height).max().unwrap_or(1).max(1);
    let rows = glyphs.len().div_ceil(16) as u32;
    let (mut dark, mut light) = (image::RgbaImage::new(cw * 16, ch * rows), image::RgbaImage::new(cw * 16, ch * rows));
    let mut toml = format!("# {} {}px weight {}: cell {cw}x{ch}, 16 per row, from {first}\ncell = [{cw}, {ch}]\nfirst = {first}\nascent = {}\nunderline = [{}, {}]\ncoverage = \"rgb\"\n[glyphs]\n", font.face, font.height, font.weight, rendered.ascent, rendered.underline.0, rendered.underline.1);
    for (i, g) in glyphs.iter().enumerate() {
        let (ox, oy) = ((i as u32 % 16) * cw, (i as u32 / 16) * ch);
        for y in 0..g.height {
            for x in 0..g.width {
                let i = (y * g.width + x) as usize;
                let opaque = |[r, g, b]: [u8; 3]| image::Rgba([r, g, b, r.max(g).max(b)]);
                dark.put_pixel(ox + x, oy + y, opaque(g.dark[i]));
                light.put_pixel(ox + x, oy + y, opaque(g.light[i]));
            }
        }
        let _ = writeln!(toml, "{} = {{ left = {}, advance = {}, width = {} }}", g.ch as u32, g.left, g.advance, g.width);
    }
    let out = std::path::Path::new(&a[4]);
    let light_path = out.with_file_name(format!("{}-light.png", out.file_stem().unwrap_or_default().to_string_lossy()));
    if let Err(e) = dark.save(out).and_then(|()| light.save(&light_path)) {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }
    let _ = std::fs::write(out.with_extension("toml"), toml);
    println!("{}", out.display());
    ExitCode::SUCCESS
}
