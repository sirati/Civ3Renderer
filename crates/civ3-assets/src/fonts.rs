//! The game's text glyphs: Lucida Sans rasterised by GDI without
//! anti-aliasing, exactly as the game's `jgl.dll` draws text.
//!
//! Inside a Windows (or Wine) process the glyphs are rendered on the spot
//! with `gdi-text`. Elsewhere they come from sheets that `font-sheet`
//! rendered under Wine beforehand (`tools/gen-civ3-fonts`), in
//! `$CIV3R_FONT_CACHE` (default `~/.cache/civ3r/fonts`).

use std::collections::BTreeMap;

use tile_render::RgbaImage;

use crate::GameFiles;

/// One glyph: its pixels (ink opaque in `color`, the rest transparent),
/// where they start relative to the pen, and the pen advance.
#[derive(Clone, Debug)]
pub struct Glyph {
    pub left: i32,
    pub advance: i32,
    pub image: RgbaImage,
}

/// A font at one size: glyphs by character, positioned on the baseline.
#[derive(Clone, Debug)]
pub struct GlyphFont {
    /// Cell top to baseline.
    pub ascent: i32,
    pub glyphs: BTreeMap<char, Glyph>,
}

/// The characters the game draws (Windows-1252 as Latin-1).
pub fn charset() -> impl Iterator<Item = char> {
    (32u32..=126).chain(160..=255).filter_map(char::from_u32)
}

fn paint(ink: &[bool], w: u32, h: u32, color: [u8; 3]) -> RgbaImage {
    let [r, g, b] = color;
    RgbaImage::from_fn(w.max(1), h.max(1), |x, y| {
        if x < w && ink[(y * w + x) as usize] { [r, g, b, 255] } else { [0, 0, 0, 0] }
    })
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
            .map(|g| (g.ch, Glyph { left: g.left, advance: g.advance, image: paint(&g.ink, g.width, g.height, color) }))
            .collect();
        Ok(GlyphFont { ascent: font.ascent, glyphs })
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

#[cfg(not(windows))]
fn from_cache(font: &FontStyle, color: [u8; 3]) -> Result<GlyphFont, String> {
    let dir = std::env::var_os("CIV3R_FONT_CACHE")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".cache/civ3r/fonts")))
        .ok_or("no font cache directory")?;
    let stem = dir.join(cache_stem(font));
    let missing = |e: &dyn std::fmt::Display| format!("{}: {e} (run tools/gen-civ3-fonts)", stem.display());
    let meta: toml::Table = std::fs::read_to_string(stem.with_extension("toml"))
        .map_err(|e| missing(&e))?
        .parse()
        .map_err(|e| missing(&e))?;
    let sheet = image::open(stem.with_extension("png")).map_err(|e| missing(&e))?.to_rgba8();
    let int = |v: Option<&toml::Value>| v.and_then(|v| v.as_integer()).unwrap_or(0);
    let cell = meta.get("cell").and_then(|c| c.as_array()).ok_or_else(|| missing(&"no cell"))?;
    let (cw, ch) = (int(cell.first()) as u32, int(cell.get(1)) as u32);
    let first = int(meta.get("first")) as u32;
    let ascent = int(meta.get("ascent")) as i32;
    let table = meta.get("glyphs").and_then(|g| g.as_table()).ok_or_else(|| missing(&"no glyphs"))?;
    let mut glyphs = BTreeMap::new();
    for (code, m) in table {
        let Ok(code) = code.parse::<u32>() else { continue };
        let Some(c) = char::from_u32(code) else { continue };
        let i = code - first;
        let (ox, oy) = ((i % 16) * cw, (i / 16) * ch);
        let w = int(m.get("width")) as u32;
        let ink: Vec<bool> = (0..ch).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| sheet.get_pixel(ox + x, oy + y)[3] > 0).collect();
        glyphs.insert(c, Glyph { left: int(m.get("left")) as i32, advance: int(m.get("advance")) as i32, image: paint(&ink, w, ch, color) });
    }
    Ok(GlyphFont { ascent, glyphs })
}
