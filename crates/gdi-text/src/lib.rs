//! Text rasterised by Windows GDI, without anti-aliasing — the way games of
//! the GDI era drew their text — so glyph sheets made with it match their
//! pixels. All `unsafe` GDI calls stay in this crate.

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
}

/// One rendered glyph: a 1-bit bitmap of `width × height` (cell height of
/// the font), the pen advance, and where the bitmap starts relative to the
/// pen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    pub left: i32,
    pub advance: i32,
    pub width: u32,
    pub height: u32,
    /// Row-major, true = ink.
    pub ink: Vec<bool>,
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

/// Renders `chars` one by one with `font`, aliased.
pub fn render_glyphs(font: &FontSpec, chars: impl IntoIterator<Item = char>) -> Result<Vec<Glyph>, GdiError> {
    #[cfg(windows)]
    return win::render_glyphs(font, chars);
    #[cfg(not(windows))]
    {
        let _ = (font, chars.into_iter().count());
        Err(GdiError::Unsupported)
    }
}
