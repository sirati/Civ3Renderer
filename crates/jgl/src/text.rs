//! jgl's text state: the fonts that exist and, per image, the font and text
//! colour its device context has selected.
//!
//! jgl keeps this state in GDI (each image has its own DC, `+0x4BC`). We keep
//! it here instead, from the calls that change it, so that a text call
//! arrives at the hooks complete.

use std::collections::HashMap;
use std::rc::Rc;

use crate::{FontId, ImageId};

/// Font-engine ink boxes and pen advances, captured once when a face is
/// selected. Boxes describe possible ink, never opaque glyph pixels.
#[derive(Clone, Debug)]
pub struct FontGeometry {
    pub glyphs: Vec<Option<GlyphGeometry>>,
    pub underline: Option<(i32, i32)>,
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphGeometry {
    pub bounds: [i32; 4],
    pub advance: i32,
}

impl GlyphGeometry {
    pub fn from_metrics(x:i32,y:i32,w:u32,h:u32,advance:i32)->Self {
        Self {bounds:[x,-y,x+w as i32,-y+h as i32],advance}
    }
}

pub fn underline_geometry(position:i32,size:i32)->(i32,i32) {
    let size=size.max(1);
    (-position-size/2,size)
}

impl FontGeometry {
    pub fn may_cover(&self, text: &str, x: i32, y: i32, underline: bool) -> Option<bool> {
        let mut pen=0;
        let mut ink=false;
        for ch in text.chars() {
            let glyph=self.glyphs.get(usize::try_from(u32::from(ch)).ok()?)?.as_ref()?;
            let [l,t,r,b]=glyph.bounds;
            ink |= x>=pen+l && x<pen+r && y>=t && y<b;
            pen+=glyph.advance;
        }
        if underline {
            let (t,h)=self.underline?;
            ink |= x>=0 && x<pen && y>=t && y<t+h;
        }
        Some(ink)
    }
}

/// A font as GDI creates it (the `LOGFONTA` fields text drawing depends on).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontDesc {
    /// The face name, at most 31 bytes (`LF_FACESIZE` - 1), Windows-1252.
    pub face: String,
    /// `lfHeight`: negative for a character height.
    pub height: i32,
    /// `lfWeight`: 0 (default) or 700 for jgl's fonts.
    pub weight: i32,
    pub italic: bool,
    pub underline: bool,
}

/// `LF_FACESIZE`: GDI keeps at most 31 face bytes and a terminator.
const FACE_MAX: usize = 31;

impl FontDesc {
    /// What font slot +0x08 `create(face, height, flags)` asks
    /// `CreateFontIndirectA` for [V, jgl RVA 0x3C270]: `lfHeight = -height`,
    /// weight 700 for flag bit 0, italic bit 1, underline bit 2.
    pub fn from_create(face: &[u8], height: i32, flags: u32) -> FontDesc {
        FontDesc {
            face: latin1(&face[..face.len().min(FACE_MAX)]),
            height: height.wrapping_neg(),
            weight: if flags & 1 != 0 { 700 } else { 0 },
            italic: flags & 2 != 0,
            underline: flags & 4 != 0,
        }
    }

    /// Flag bits for [`FontDesc::italic`] (1) and [`FontDesc::underline`]
    /// (2), as records carry them.
    pub fn style_bits(&self) -> u32 {
        u32::from(self.italic) | u32::from(self.underline) << 1
    }
}

/// Windows-1252 bytes as text; read as Latin-1, which the glyph sets cover
/// with the same codes.
pub fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// A text colour as `SetTextColor` gets it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TextColor {
    Rgb([u8; 3]),
    /// `DIBINDEX(i)`, what image slot +0xB0 `set_text_color16(c)` sets
    /// (`0x10FF0000 | c`). On the 16-bit canvases, which have no colour
    /// table, GDI draws it as pixel value 0: black.
    Index(u16),
}

impl TextColor {
    pub const BLACK: TextColor = TextColor::Rgb([0, 0, 0]);

    pub fn from_colorref(c: u32) -> TextColor {
        if c >> 16 == 0x10FF {
            TextColor::Index(c as u16)
        } else {
            TextColor::Rgb([c as u8, (c >> 8) as u8, (c >> 16) as u8])
        }
    }

    pub fn colorref(self) -> u32 {
        match self {
            TextColor::Rgb([r, g, b]) => u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16,
            TextColor::Index(i) => 0x10FF_0000 | u32::from(i),
        }
    }

    /// The colour GDI draws on a 16-bit DIB without colour table.
    pub fn on_16_bit(self) -> [u8; 3] {
        match self {
            TextColor::Rgb(c) => c,
            TextColor::Index(_) => [0, 0, 0],
        }
    }
}

/// What an image's DC has selected.
#[derive(Clone, Debug)]
pub struct DcText {
    /// As it was when selected: GDI's DC keeps the font handle it was
    /// given, whatever happens to the font object afterwards.
    pub font: Rc<FontDesc>,
    pub color: TextColor,
}

/// The fonts and the per-image selections.
#[derive(Debug)]
pub struct TextState {
    stock: Rc<FontDesc>,
    fonts: HashMap<FontId, Rc<FontDesc>>,
    images: HashMap<ImageId, DcText>,
}

impl TextState {
    /// `stock`: the stock system font (`GetStockObject(SYSTEM_FONT)`), which a
    /// fresh DC has selected and image slot +0xAC selects.
    pub fn new(stock: FontDesc) -> TextState {
        TextState { stock: Rc::new(stock), fonts: HashMap::new(), images: HashMap::new() }
    }

    pub fn define_font(&mut self, font: FontId, desc: FontDesc) {
        if font == FontId::STOCK {
            self.stock = Rc::new(desc);
        } else {
            self.fonts.insert(font, Rc::new(desc));
        }
    }

    pub fn release_font(&mut self, font: FontId) {
        self.fonts.remove(&font);
    }

    pub fn font(&self, font: FontId) -> Option<&Rc<FontDesc>> {
        if font == FontId::STOCK { Some(&self.stock) } else { self.fonts.get(&font) }
    }

    /// The image's DC was (re)created: GDI's defaults.
    pub fn reset(&mut self, image: ImageId) {
        self.images.insert(image, DcText { font: self.stock.clone(), color: TextColor::BLACK });
    }

    /// The image's state as read from its DC (for images whose DC existed
    /// before the takeover).
    pub fn set_dc(&mut self, image: ImageId, font: FontDesc, color: TextColor) {
        self.images.insert(image, DcText { font: Rc::new(font), color });
    }

    /// Returns false if the font is unknown (nothing changes then).
    pub fn select(&mut self, image: ImageId, font: FontId) -> bool {
        let Some(desc) = self.font(font).cloned() else { return false };
        let stock = self.stock.clone();
        self.images.entry(image).or_insert_with(|| DcText { font: stock, color: TextColor::BLACK }).font = desc;
        true
    }

    pub fn set_color(&mut self, image: ImageId, color: TextColor) {
        let stock = self.stock.clone();
        self.images.entry(image).or_insert_with(|| DcText { font: stock, color }).color = color;
    }

    pub fn dc(&self, image: ImageId) -> Option<&DcText> {
        self.images.get(&image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_flags_as_jgl_builds_the_logfont() {
        let f = FontDesc::from_create(b"Times New Roman", 14, 1);
        assert_eq!(f, FontDesc { face: "Times New Roman".into(), height: -14, weight: 700, italic: false, underline: false });
        let f = FontDesc::from_create(b"A", -3, 6);
        assert_eq!((f.height, f.weight, f.italic, f.underline), (3, 0, true, true));
        // GDI keeps 31 face bytes.
        assert_eq!(FontDesc::from_create(&[b'x'; 40], 1, 0).face.len(), 31);
    }

    #[test]
    fn colorrefs() {
        assert_eq!(TextColor::from_colorref(0x0030_2010), TextColor::Rgb([0x10, 0x20, 0x30]));
        assert_eq!(TextColor::from_colorref(0x10FF_7C1F), TextColor::Index(0x7C1F));
        assert_eq!(TextColor::Index(0x7C1F).on_16_bit(), [0, 0, 0]);
        assert_eq!(TextColor::Rgb([1, 2, 3]).colorref(), 0x0003_0201);
    }

    #[test]
    fn a_selected_font_stays_as_it_was_selected() {
        let mut s = TextState::new(FontDesc::from_create(b"System", -16, 1));
        let (img, font) = (ImageId(0x100), FontId(0x200));
        assert!(!s.select(img, font));
        s.define_font(font, FontDesc::from_create(b"Arial", 12, 0));
        assert!(s.select(img, font));
        s.release_font(font);
        s.define_font(font, FontDesc::from_create(b"Other", 12, 0));
        assert_eq!(s.dc(img).unwrap().font.face, "Arial");
        s.reset(img);
        assert_eq!(s.dc(img).unwrap().font.face, "System");
        assert_eq!(s.dc(img).unwrap().color, TextColor::BLACK);
    }
}
