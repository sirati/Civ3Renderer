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

/// The game's byte-valued glyph keys. Windows-1252 bytes retain their
/// numeric values in the trace, cache metrics and atlas names, including
/// the punctuation in 0x80..=0x9f. The original 191-frame order stays a
/// prefix so saved descriptions retain their frame indices.
pub fn charset() -> impl Iterator<Item = char> {
    (32u32..=126).chain(160..=255).chain(127..=159).filter_map(char::from_u32)
}

/// The Unicode character an outline font draws for a byte-valued game glyph.
pub use gdi_text::windows_1252_char as outline_char;

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

/// The width this system's text routine (`GetTextExtentPoint32A`, as the game
/// measures; under Wine, Wine's, as the game gets there) gives `text` in
/// `font`, Windows-1252 encoded (other characters as `?`); `None` where there
/// is no such routine (not Windows) or it fails.
pub fn measure(font: &FontStyle, text: &str) -> Option<i32> {
    let spec = gdi_text::FontSpec { face: font.face.clone(), height: font.height, weight: font.weight, italic: font.italic, quality: 0 };
    let bytes: Vec<u8> = text.chars().map(|c| u8::try_from(c as u32).unwrap_or(b'?')).collect();
    gdi_text::text_extent(&spec, &bytes).ok()
}

/// Of `available` heights, the one nearest `height` (of two as near, the
/// larger); `None` if there is none.
pub fn nearest_height(available: &[i32], height: i32) -> Option<i32> {
    available.iter().copied().min_by_key(|&h| ((h - height).abs(), -h))
}

/// Whether glyphs come from sheets (see [`load`]) rather than this
/// system's GDI.
fn from_sheets() -> bool {
    #[cfg(windows)]
    return std::env::var_os("CIV3R_FONT_CACHE").is_some() || gdi_text::under_wine();
    #[cfg(not(windows))]
    true
}

/// The heights there are sheets for in the font cache `dir` of `font`'s
/// face and style (any height), ascending.
fn sheet_heights(dir: Option<std::path::PathBuf>, font: &FontStyle) -> Vec<i32> {
    let probe = FontStyle { height: 0, ..font.clone() };
    let stem = cache_stem(&probe);
    let (head, tail) = stem.split_once("-0").unwrap_or((&stem, ""));
    let (head, tail) = (format!("{head}-"), format!("{tail}.toml"));
    let mut out: Vec<i32> = dir
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|n| n.strip_prefix(&head)?.strip_suffix(&tail)?.parse().ok())
        .collect();
    out.sort_unstable();
    out
}

/// The height `font` is made at when asked for `font.height`: with
/// sheets, the nearest height there is a sheet for (each size the cache
/// lacks noted once), with GDI the height asked for. Text at other zooms is
/// drawn from the face's outlines, so only the game's own sizes are needed.
pub fn available_height(font: &FontStyle) -> i32 {
    use std::collections::HashMap;
    use std::sync::Mutex;
    let wanted = font.height.abs().max(1);
    if !from_sheets() {
        return wanted;
    }
    // The sheets per face and style, listed once; the sizes reported.
    static SHEETS: Mutex<Option<HashMap<String, Vec<i32>>>> = Mutex::new(None);
    static NOTED: Mutex<Option<std::collections::HashSet<String>>> = Mutex::new(None);
    let style = cache_stem(&FontStyle { height: 0, ..font.clone() });
    let resolved = {
        let mut sheets = SHEETS.lock().unwrap_or_else(|e| e.into_inner());
        let heights = sheets.get_or_insert_with(HashMap::new).entry(style).or_insert_with(|| sheet_heights(cache_dir(), font));
        nearest_height(heights, wanted)
    };
    let asked = cache_stem(&FontStyle { height: font.height.abs(), ..font.clone() });
    if resolved != Some(font.height.abs()) && NOTED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(asked.clone()) {
        match resolved {
            Some(h) => log::info!("font {asked}: no glyph sheet of that size, drawn at the nearest, {h} pixels"),
            None => log::warn!("font {asked}: no glyph sheets of this face and style (run tools/gen-civ3-fonts)"),
        }
    }
    resolved.unwrap_or(wanted)
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
    // (`sheet_heights` relies on the height following the face as `-<height>`.)
    let mut s = format!("{}-{}", font.face.replace(' ', "_"), font.height);
    if font.weight > 400 {
        s += &format!("-w{}", font.weight);
    }
    if font.italic {
        s += "-i";
    }
    s
}

/// A font's sheet (see [`from_cache`]): its file stem, its metrics
/// (`*.toml`), and the cell size.
struct SheetMeta {
    stem: std::path::PathBuf,
    meta: toml::Table,
    cell: (u32, u32),
}

fn int(v: Option<&toml::Value>) -> i64 {
    v.and_then(|v| v.as_integer()).unwrap_or(0)
}

fn sheet_meta(font: &FontStyle) -> Result<SheetMeta, String> {
    let dir = cache_dir().ok_or("no font cache directory")?;
    let stem = dir.join(cache_stem(font));
    let missing = |e: &dyn std::fmt::Display| format!("{}: {e} (run tools/gen-civ3-fonts)", stem.display());
    let meta: toml::Table = std::fs::read_to_string(stem.with_extension("toml"))
        .map_err(|e| missing(&e))?
        .parse()
        .map_err(|e| missing(&e))?;
    let cell = meta.get("cell").and_then(|c| c.as_array()).ok_or_else(|| missing(&"no cell"))?;
    let cell = (int(cell.first()) as u32, int(cell.get(1)) as u32);
    if meta.get("coverage").and_then(|c| c.as_str()) != Some("rgb") {
        return Err(missing(&"old sheet format"));
    }
    Ok(SheetMeta { stem, meta, cell })
}

/// The size of each glyph image [`load`] makes of `font` (in any colour),
/// by character; from the sheet's metrics alone where glyphs come from
/// sheets.
pub fn glyph_sizes(files: &GameFiles, font: &FontStyle) -> Result<BTreeMap<char, (u32, u32)>, String> {
    if !from_sheets() {
        // (Rasterising them is slow, and the same in every colour: once a
        // style.)
        type Sizes = std::collections::HashMap<(Vec<std::path::PathBuf>, FontStyle), BTreeMap<char, (u32, u32)>>;
        static MADE: std::sync::Mutex<Option<Sizes>> = std::sync::Mutex::new(None);
        let key = (files.roots().to_vec(), font.clone());
        if let Some(sizes) = MADE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&key)) {
            return Ok(sizes.clone());
        }
        let sizes: BTreeMap<char, (u32, u32)> = load(files, font, [0; 3])?.glyphs.into_iter().map(|(c, g)| (c, (g.image.width, g.image.height))).collect();
        MADE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(key, sizes.clone());
        return Ok(sizes);
    }
    let font = &FontStyle { height: available_height(font), ..font.clone() };
    let SheetMeta { meta, cell: (_, ch), .. } = sheet_meta(font)?;
    let table = meta.get("glyphs").and_then(|g| g.as_table()).ok_or("no glyphs")?;
    Ok(table
        .iter()
        .filter_map(|(code, m)| {
            let c = char::from_u32(code.parse().ok()?)?;
            // (As `image` makes them: at least 1 × 1.)
            Some((c, ((int(m.get("width")) as u32).max(1), ch.max(1))))
        })
        .collect())
}

fn from_cache(font: &FontStyle, color: [u8; 3]) -> Result<GlyphFont, String> {
    // A size without a sheet: the nearest there is.
    let font = &FontStyle { height: available_height(font), ..font.clone() };
    let SheetMeta { stem, meta, cell: (cw, ch) } = sheet_meta(font)?;
    let missing = |e: &dyn std::fmt::Display| format!("{}: {e} (run tools/gen-civ3-fonts)", stem.display());
    let sheet = image::open(stem.with_extension("png")).map_err(|e| missing(&e))?.to_rgba8();
    let light_sheet = image::open(stem.with_file_name(format!("{}-light.png", cache_stem(font)))).map_err(|e| missing(&e))?.to_rgba8();
    let first = int(meta.get("first")) as u32;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_charset_preserves_all_windows_1252_byte_positions() {
        let chars: Vec<_> = charset().collect();
        assert_eq!(chars.len(), 224);
        let original: Vec<_> = (32u32..=126).chain(160..=255).filter_map(char::from_u32).collect();
        assert_eq!(&chars[..191], original, "saved font descriptions keep every original frame index");
        assert_eq!(chars[191..].iter().map(|&c| c as u32).collect::<Vec<_>>(), (127..=159).collect::<Vec<_>>());
        assert_eq!(chars[191 + 146 - 127], '\u{92}');
        assert_eq!(outline_char(chars[191 + 146 - 127]), '’');
    }

    #[test]
    fn the_nearest_height_is_taken_larger_on_a_tie() {
        let have = [5, 6, 8, 12, 48, 96];
        assert_eq!(nearest_height(&have, 8), Some(8));
        assert_eq!(nearest_height(&have, 7), Some(8));
        assert_eq!(nearest_height(&have, 10), Some(12));
        assert_eq!(nearest_height(&have, 9), Some(8));
        assert_eq!(nearest_height(&have, 2), Some(5));
        assert_eq!(nearest_height(&have, 192), Some(96));
        assert_eq!(nearest_height(&[], 12), None);
    }

    #[test]
    fn sheet_names_parse_back_to_their_height() {
        let dir = std::env::temp_dir().join(format!("civ3r-font-heights-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["Lucida_Sans-8.toml", "Lucida_Sans-12.toml", "Lucida_Sans-12-w700.toml", "Lucida_Sans-14-i.toml", "Lucida_Sans-9.png", "Lucida_Sans-10-light.png"] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        let style = |weight, italic| FontStyle { face: "Lucida Sans".into(), height: 11, weight, italic };
        let heights = |weight, italic| sheet_heights(Some(dir.clone()), &style(weight, italic));
        assert_eq!(heights(0, false), vec![8, 12]);
        assert_eq!(heights(700, false), vec![12]);
        assert_eq!(heights(0, true), vec![14]);
        assert_eq!(heights(700, true), Vec::<i32>::new());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
