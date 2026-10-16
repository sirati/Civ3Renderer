//! The game's text glyphs: Lucida Sans as Windows' GDI draws it for the
//! game's `jgl.dll` (ClearType: each colour channel with its own coverage),
//! measured per channel (`gdi_text::Glyph`).
//!
//! In a Windows process the glyphs are measured on the spot with
//! `gdi-text`. Elsewhere, and wherever `$CIV3R_FONT_CACHE` is set (Wine,
//! whose GDI draws text its own way), they come from sheets `font-sheet`
//! made on Windows beforehand (`tools/gen-civ3-fonts`), in
//! `$CIV3R_FONT_CACHE` (default `~/.cache/civ3r/fonts`).

use std::collections::BTreeMap;

use tile_render::RgbaImage;

use crate::GameFiles;

/// One glyph: per pixel its opacity in each colour channel (red, green,
/// blue; alpha: their maximum) for the colour it was loaded for, drawn with
/// that colour per channel (`tile_render::Blend::Subpixel`); where it
/// starts relative to the pen, and the pen advance. `extent` is the width the
/// game's text measure gives the character (`gdi_text::Glyph::extent`).
#[derive(Clone, Debug)]
pub struct Glyph {
    pub left: i32,
    pub advance: i32,
    pub extent: i32,
    pub image: RgbaImage,
}

/// A font at one size: glyphs by character, positioned on the baseline.
#[derive(Clone, Debug)]
pub struct GlyphFont {
    /// Cell top to baseline.
    pub ascent: i32,
    /// Underline: top row below the baseline, thickness.
    pub underline: (i32, i32),
    pub glyphs: BTreeMap<char, Glyph>,
}

/// The characters the game draws (Windows-1252 as Latin-1).
pub fn charset() -> impl Iterator<Item = char> {
    (32u32..=126).chain(160..=255).filter_map(char::from_u32)
}

/// A `w × h` glyph from its pixels (row-major RGBA).
fn image(pixels: Vec<[u8; 4]>, w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w.max(1), h.max(1), |x, y| if x < w { pixels[(y * w + x) as usize] } else { [0; 4] })
}

/// A font as the game selects it: face, height in pixels (GDI's negative
/// `lfHeight`), weight (0 or 400 normal, 700 bold), italic.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontStyle {
    pub face: String,
    pub height: i32,
    pub weight: i32,
    pub italic: bool,
}

/// Loads `font` in `color`.
pub fn load(files: &GameFiles, font: &FontStyle, color: [u8; 3]) -> Result<GlyphFont, String> {
    #[cfg(windows)]
    {
        // Wine's GDI is not Windows': there the sheets measured on Windows.
        if std::env::var_os("CIV3R_FONT_CACHE").is_some() || gdi_text::under_wine() {
            return from_cache(font, color);
        }
        // The game installs its font from LSANS.TTF; make sure we have it.
        for f in ["LSANS.TTF", "../LSANS.TTF"] {
            if let Ok(p) = files.resolve(f) {
                let _ = gdi_text::add_font_file(&p);
            }
        }
        let spec = gdi_text::FontSpec { face: font.face.clone(), height: font.height, weight: font.weight, italic: font.italic, quality: 0 };
        let font = gdi_text::render_glyphs(&spec, charset()).map_err(|e| e.to_string())?;
        let glyphs = font
            .glyphs
            .into_iter()
            .map(|g| (g.ch, Glyph { left: g.left, advance: g.advance, extent: g.extent, image: image(g.paint(color), g.width, g.height) }))
            .collect();
        Ok(GlyphFont { ascent: font.ascent, underline: font.underline, glyphs })
    }
    #[cfg(not(windows))]
    {
        let _ = files;
        from_cache(font, color)
    }
}

/// The cache file stem for a font (`font-sheet` output name): face and
/// height, then `-w<weight>` if bold, `-i` if italic.
pub fn cache_stem(font: &FontStyle) -> String {
    let mut s = format!("{}-{}", font.face.replace(' ', "_"), font.height);
    if font.weight > 400 {
        s += &format!("-w{}", font.weight);
    }
    if font.italic {
        s += "-i";
    }
    s
}

fn from_cache(font: &FontStyle, color: [u8; 3]) -> Result<GlyphFont, String> {
    let dir = cache_dir().ok_or("no font cache directory")?;
    let stem = dir.join(cache_stem(font));
    let missing = |e: &dyn std::fmt::Display| format!("{}: {e} (run tools/gen-civ3-fonts)", stem.display());
    let meta: toml::Table = std::fs::read_to_string(stem.with_extension("toml"))
        .map_err(|e| missing(&e))?
        .parse()
        .map_err(|e| missing(&e))?;
    let sheet = image::open(stem.with_extension("png")).map_err(|e| missing(&e))?.to_rgba8();
    let light_sheet = image::open(stem.with_file_name(format!("{}-light.png", cache_stem(font)))).map_err(|e| missing(&e))?.to_rgba8();
    let int = |v: Option<&toml::Value>| v.and_then(|v| v.as_integer()).unwrap_or(0);
    let cell = meta.get("cell").and_then(|c| c.as_array()).ok_or_else(|| missing(&"no cell"))?;
    let (cw, ch) = (int(cell.first()) as u32, int(cell.get(1)) as u32);
    let first = int(meta.get("first")) as u32;
    if meta.get("coverage").and_then(|c| c.as_str()) != Some("rgb") {
        return Err(missing(&"old sheet format"));
    }
    let ascent = int(meta.get("ascent")) as i32;
    let underline = match meta.get("underline").and_then(|u| u.as_array()) {
        Some(u) => (int(u.first()) as i32, int(u.get(1)) as i32),
        None => (1, 1),
    };
    let table = meta.get("glyphs").and_then(|g| g.as_table()).ok_or_else(|| missing(&"no glyphs"))?;
    let mut glyphs = BTreeMap::new();
    for (code, m) in table {
        let Ok(code) = code.parse::<u32>() else { continue };
        let Some(c) = char::from_u32(code) else { continue };
        let i = code - first;
        let (ox, oy) = ((i % 16) * cw, (i / 16) * ch);
        let w = int(m.get("width")) as u32;
        let rgb = |s: &image::RgbaImage, (x, y): (u32, u32)| { let p = s.get_pixel(ox + x, oy + y); [p[0], p[1], p[2]] };
        let cells: Vec<(u32, u32)> = (0..ch).flat_map(|y| (0..w).map(move |x| (x, y))).collect();
        let dark: Vec<[u8; 3]> = cells.iter().map(|&p| rgb(&sheet, p)).collect();
        let light: Vec<[u8; 3]> = cells.iter().map(|&p| rgb(&light_sheet, p)).collect();
        let image = image(gdi_text::paint(&dark, &light, color), w, ch);
        // Sheets from before the text measure have no extent: the advance.
        let advance = int(m.get("advance")) as i32;
        let extent = m.get("extent").and_then(|e| e.as_integer()).map_or(advance, |e| e as i32);
        glyphs.insert(c, Glyph { left: int(m.get("left")) as i32, advance, extent, image });
    }
    Ok(GlyphFont { ascent, underline, glyphs })
}

/// `$CIV3R_FONT_CACHE`, else `~/.cache/civ3r/fonts`. A Windows path to the
/// Unix side's `Z:` drive also works on Linux, and in Wine the home folder
/// is the Unix one (`WINEHOMEDIR`), so both sides of a Wine process find
/// the same sheets.
fn cache_dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Some(dir) = std::env::var_os("CIV3R_FONT_CACHE") {
        let s = dir.to_string_lossy();
        if !cfg!(windows) {
            if let Some(rest) = s.strip_prefix("Z:").or_else(|| s.strip_prefix("z:")) {
                return Some(PathBuf::from(rest.replace('\\', "/")));
            }
        }
        return Some(PathBuf::from(dir));
    }
    if cfg!(windows) {
        if let Some(home) = std::env::var_os("WINEHOMEDIR") {
            let home = home.to_string_lossy().trim_start_matches(r"\??\").to_owned();
            return Some(PathBuf::from(home).join(r".cache\civ3r\fonts"));
        }
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/civ3r/fonts"))
}
