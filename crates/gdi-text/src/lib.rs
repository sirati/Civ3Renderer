//! Text rasterised by Windows GDI — the way games of the GDI era drew their
//! text — so glyph sheets made with it match their pixels, smoothing
//! included. All `unsafe` GDI calls stay in this crate.

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

/// Renders `chars` one by one with `font`.
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
    fn black_and_white_text_take_their_own_measurement() {
        let (dark, light) = ([[0; 3], [100, 120, 140], [255; 3]], [[0; 3], [180, 170, 160], [255; 3]]);
        assert_eq!(super::paint(&dark, &light, [0, 0, 0]), [[0; 4], [100, 120, 140, 140], [255; 4]]);
        assert_eq!(super::paint(&dark, &light, [255, 255, 255]), [[0; 4], [180, 170, 160, 180], [255; 4]]);
        // A colour in between: per channel in between.
        assert_eq!(super::paint(&dark, &light, [255, 0, 0])[1], [180, 120, 140, 180]);
    }
}
