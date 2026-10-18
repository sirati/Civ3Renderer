//! Text rasterised by Windows GDI — the way games of the GDI era drew their
//! text — so glyph sheets made with it match their pixels, smoothing
//! included. All `unsafe` GDI calls stay in this crate.

/// The Unicode character an outline font draws for a byte-valued game
/// glyph. Other characters pass through unchanged. Undefined Windows-1252
/// bytes retain their controls so a missing face glyph remains unsupported.
pub fn windows_1252_char(c: char) -> char {
    match c {
        '\u{80}' => '\u{20ac}', '\u{82}' => '\u{201a}', '\u{83}' => '\u{192}', '\u{84}' => '\u{201e}',
        '\u{85}' => '\u{2026}', '\u{86}' => '\u{2020}', '\u{87}' => '\u{2021}', '\u{88}' => '\u{2c6}',
        '\u{89}' => '\u{2030}', '\u{8a}' => '\u{160}', '\u{8b}' => '\u{2039}', '\u{8c}' => '\u{152}',
        '\u{8e}' => '\u{17d}', '\u{91}' => '\u{2018}', '\u{92}' => '\u{2019}', '\u{93}' => '\u{201c}',
        '\u{94}' => '\u{201d}', '\u{95}' => '\u{2022}', '\u{96}' => '\u{2013}', '\u{97}' => '\u{2014}',
        '\u{98}' => '\u{2dc}', '\u{99}' => '\u{2122}', '\u{9a}' => '\u{161}', '\u{9b}' => '\u{203a}',
        '\u{9c}' => '\u{153}', '\u{9e}' => '\u{17e}', '\u{9f}' => '\u{178}', _ => c,
    }
}


/// A font request, as `CreateFontW` takes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontSpec {
    /// Face name, e.g. "Lucida Sans".
    pub face: String,
    /// Character height in pixels (GDI's negative `lfHeight`); a negative
    /// value asks for that cell height instead (positive `lfHeight`).
    pub height: i32,
    /// 400 normal, 700 bold.
    pub weight: i32,
    pub italic: bool,
    /// `lfQuality`: 0 default (what the game uses), 3 non-antialiased.
    pub quality: u8,
}

/// One rendered glyph: `width × height` (cell height of the font), the pen
/// advance, and where the bitmap starts relative to the pen.
///
/// GDI does not blend smoothed text linearly, and with ClearType each
/// colour channel gets its own coverage, so a glyph keeps how much of the
/// text colour GDI actually puts down at each pixel, per channel, measured
/// both ways: black text on white (`dark`) and white text on black
/// (`light`). [`Glyph::paint`] gives the glyph in any colour from them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    pub left: i32,
    pub advance: i32,
    /// The width `GetTextExtentPoint32` gives the character on a memory DC
    /// with its default (monochrome) bitmap, which is how `jgl.dll`
    /// measures text for the game (docs §10.8). It is not `advance`, with
    /// which the text is drawn on the 16-bit surfaces.
    pub extent: i32,
    pub width: u32,
    pub height: u32,
    /// Row-major opacity per channel (red, green, blue) of black text over
    /// white, 0 to 255.
    pub dark: Vec<[u8; 3]>,
    /// The same of white text over black.
    pub light: Vec<[u8; 3]>,
}

impl Glyph {
    /// The glyph's opacity per channel in `color` (see [`paint`]).
    pub fn paint(&self, color: [u8; 3]) -> Vec<[u8; 4]> {
        paint(&self.dark, &self.light, color)
    }
}

/// Opacities measured as in [`Glyph`] (`dark`, `light`) for text in
/// `color`: per pixel, each channel's opacity (interpolated between the two
/// measurements by that channel's lightness, exact for black and white
/// text) in red, green and blue, their maximum in alpha — what a renderer
/// blending each channel on its own needs (the colour is drawn separately).
pub fn paint(dark: &[[u8; 3]], light: &[[u8; 3]], color: [u8; 3]) -> Vec<[u8; 4]> {
    dark.iter()
        .zip(light)
        .map(|(d, l)| {
            let a: [u8; 3] = std::array::from_fn(|i| (((255 - color[i] as u32) * d[i] as u32 + color[i] as u32 * l[i] as u32 + 127) / 255) as u8);
            [a[0], a[1], a[2], a[0].max(a[1]).max(a[2])]
        })
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum GdiError {
    #[error("adding font file {0}")]
    AddFont(String),
    #[error("GDI call {0} failed")]
    Call(&'static str),
    #[error("GDI is only available on Windows")]
    Unsupported,
}

#[cfg(windows)]
mod win;

/// Whether this Windows process runs in Wine (whose GDI draws text its own
/// way).
pub fn under_wine() -> bool {
    #[cfg(windows)]
    return win::under_wine();
    #[cfg(not(windows))]
    false
}

/// Makes the fonts in a font file available to this process.
pub fn add_font_file(path: &std::path::Path) -> Result<(), GdiError> {
    #[cfg(windows)]
    return win::add_font_file(path);
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(GdiError::Unsupported)
    }
}

/// A rendered font: its ascent (pixels from the cell top to the baseline),
/// underline and glyphs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Font {
    pub ascent: i32,
    /// Where GDI draws an underline: top row below the baseline, and
    /// thickness.
    pub underline: (i32, i32),
    pub glyphs: Vec<Glyph>,
}

/// The width `GetTextExtentPoint32A` gives `text` (Windows-1252 bytes) in
/// `font` on a memory DC with its default bitmap: the game's text measure.
pub fn text_extent(font: &FontSpec, text: &[u8]) -> Result<i32, GdiError> {
    #[cfg(windows)]
    return win::text_extent(font, text);
    #[cfg(not(windows))]
    {
        let _ = (font, text);
        Err(GdiError::Unsupported)
    }
}

/// Renders `chars` one by one with `font`. Byte-valued keys use
/// Windows-1252 when drawn through GDI's Unicode API; returned glyphs keep
/// the original keys so sheets and ANSI text retain their byte positions.
pub fn render_glyphs(font: &FontSpec, chars: impl IntoIterator<Item = char>) -> Result<Font, GdiError> {
    #[cfg(windows)]
    return win::render_glyphs(font, chars);
    #[cfg(not(windows))]
    {
        let _ = (font, chars.into_iter().count());
        Err(GdiError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn windows_1252_byte_keys_decode_punctuation_for_unicode_drawing() {
        let bytes = [0x80, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x8b, 0x8c, 0x8e, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0x9b, 0x9c, 0x9e, 0x9f];
        let decoded: String = bytes.into_iter().map(|byte| super::windows_1252_char(char::from(byte))).collect();
        assert_eq!(decoded, "€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ");
        for c in ['A', '\u{a0}', 'é', '’', '界', '\u{81}', '\u{8d}', '\u{8f}', '\u{90}', '\u{9d}'] {
            assert_eq!(super::windows_1252_char(c), c, "non-C1 and undefined characters retain their values");
        }
    }

    #[test]
    fn black_and_white_text_take_their_own_measurement() {
        let (dark, light) = ([[0; 3], [100, 120, 140], [255; 3]], [[0; 3], [180, 170, 160], [255; 3]]);
        assert_eq!(super::paint(&dark, &light, [0, 0, 0]), [[0; 4], [100, 120, 140, 140], [255; 4]]);
        assert_eq!(super::paint(&dark, &light, [255, 255, 255]), [[0; 4], [180, 170, 160, 180], [255; 4]]);
        // A colour in between: per channel in between.
        assert_eq!(super::paint(&dark, &light, [255, 0, 0])[1], [180, 120, 140, 180]);
    }
}
