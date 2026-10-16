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
/// GDI does not blend smoothed text linearly (Wine, for one, bends the
/// coverage through a gamma ramp, differently for text darker and lighter
/// than what is under it), so a glyph keeps how much of the text colour
/// GDI actually puts down at each pixel, measured both ways: black text on
/// white (`dark`) and white text on black (`light`). [`Glyph::paint`] gives
/// the glyph in any colour from them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    pub left: i32,
    pub advance: i32,
    pub width: u32,
    pub height: u32,
    /// Row-major opacity of black text over white, 0 to 255.
    pub dark: Vec<u8>,
    /// Row-major opacity of white text over black, 0 to 255.
    pub light: Vec<u8>,
}

impl Glyph {
    /// The glyph in `color`: row-major RGBA, straight alpha.
    pub fn paint(&self, color: [u8; 3]) -> Vec<[u8; 4]> {
        paint(&self.dark, &self.light, color)
    }
}

/// Opacities measured as in [`Glyph`] (`dark`, `light`) in `color`: each
/// channel interpolates between the two by its own lightness (exact for
/// black and white text), and the alpha is their mean.
pub fn paint(dark: &[u8], light: &[u8], color: [u8; 3]) -> Vec<[u8; 4]> {
    dark.iter()
        .zip(light)
        .map(|(&d, &l)| {
            let sum: u32 = color.iter().map(|&c| ((255 - c as u32) * d as u32 + c as u32 * l as u32 + 127) / 255).sum();
            let a = ((sum + 1) / 3) as u8;
            if a == 0 { [0, 0, 0, 0] } else { [color[0], color[1], color[2], a] }
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
        let (dark, light) = ([0, 100, 255], [0, 180, 255]);
        let black = super::paint(&dark, &light, [0, 0, 0]);
        let white = super::paint(&dark, &light, [255, 255, 255]);
        assert_eq!(black, [[0; 4], [0, 0, 0, 100], [0, 0, 0, 255]]);
        assert_eq!(white, [[0; 4], [255, 255, 255, 180], [255, 255, 255, 255]]);
        // A colour in between: per channel in between.
        assert_eq!(super::paint(&dark, &light, [255, 0, 0])[1], [255, 0, 0, 127]);
    }
}
