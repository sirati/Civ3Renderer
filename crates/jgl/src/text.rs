//! jgl's text state: the fonts that exist and, per image, the font and text
//! colour its device context has selected.
//!
//! jgl keeps this state in GDI (each image has its own DC, `+0x4BC`). We keep
//! it here instead, from the calls that change it, so that a text call
//! arrives at the hooks complete.

use std::collections::HashMap;
use std::rc::Rc;

use crate::{FontId, ImageId};

/// A font's glyphs as the font engine renders them for `TextOutA`, and
/// which of its bitmaps reproduce that text, decided once per font by a
/// calibration `TextOutA` of the whole set ([`FontGeometry::new`]). No text
/// or canvas is drawn or read to answer a query.
#[derive(Clone, Debug)]
pub struct FontGeometry {
    glyphs: Vec<Option<Glyph>>,
    /// Underline rows relative to the baseline (top, height), if any.
    underline: Option<(i32, i32)>,
    mode: Option<Mode>,
    /// Per grey level, the one RGB555 pixel black text of that level made
    /// over the colour key throughout the calibration, if it was one.
    on_key: [Option<u16>; 17],
}

/// One glyph as the font engine rasterises it for each mode: `GGO_BITMAP`
/// (levels 0 or 1) and `GGO_GRAY4_BITMAP` (0..=16). Each format is hinted
/// on its own, with its own box, origin and advance.
#[derive(Clone, Debug)]
pub struct Glyph {
    pub mono: Raster,
    pub gray: Raster,
}

/// A rasterised glyph: `levels` over its black box of `size`, its origin
/// (left, top above the baseline) and pen advance.
#[derive(Clone, Debug)]
pub struct Raster {
    pub origin: (i32, i32),
    pub size: (i32, i32),
    pub advance: i32,
    pub levels: Vec<u8>,
}

/// A calibration line: `text` drawn with `TextOutA` in black over the colour
/// key, its pen at `pen`; `pixels` the RGB555 result, `width` per row.
pub struct Calibration {
    pub text: Vec<u8>,
    pub pen: (i32, i32),
    pub width: i32,
    pub pixels: Vec<u16>,
}

/// How `TextOutA` draws this font, as the calibration showed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Exactly the mono bitmaps, in the text colour.
    Mono,
    /// Smoothed: no ink where the grey level is 0, the text colour where it
    /// is 16, a blend (not reproduced) between.
    Gray,
}

/// What a text run draws at a pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    None,
    /// The text colour.
    Solid,
    /// Something between what is below and the text colour: one glyph's
    /// grey level (1..=15), or none where glyphs or the underline meet.
    Partial(Option<u8>),
}

const KEY: u16 = 0x7C1F;

impl FontGeometry {
    /// `underline`: `OUTLINETEXTMETRIC`'s underscore position and size.
    pub fn new(glyphs: Vec<Option<Glyph>>, calibration: &[Calibration], underline: Option<(i32, i32)>) -> Self {
        let mut font = Self { glyphs, underline: underline.map(|(position, size)| (-position - size.max(1) / 2, size.max(1))), mode: None, on_key: [None; 17] };
        // Each line's ink painted once per mode, glyph by glyph.
        let painted = |mode: Mode| -> Option<Vec<Vec<Ink>>> {
            calibration.iter().map(|line| {
                let mut ink = vec![Ink::None; line.pixels.len()];
                let height = line.pixels.len() as i32 / line.width;
                let mut pen = line.pen.0;
                for &b in &line.text {
                    let glyph = font.glyphs.get(b as usize)?.as_ref()?;
                    let raster = match mode { Mode::Mono => &glyph.mono, Mode::Gray => &glyph.gray };
                    for gy in 0..raster.size.1 { for gx in 0..raster.size.0 {
                        let (x, y) = (pen + raster.origin.0 + gx, line.pen.1 - raster.origin.1 + gy);
                        if x < 0 || y < 0 || x >= line.width || y >= height { return None; }
                        let at = (y * line.width + x) as usize;
                        ink[at] = combine(ink[at], level_ink(mode, raster.levels[(gy * raster.size.0 + gx) as usize]));
                    }}
                    pen += raster.advance;
                }
                Some(ink)
            }).collect()
        };
        let fits = |ink: &[Vec<Ink>]| calibration.iter().zip(ink).all(|(line, ink)| line.pixels.iter().zip(ink).all(|(&px, &ink)| match ink {
            Ink::None => px == KEY,
            Ink::Solid => px == 0,
            Ink::Partial(_) => true,
        }));
        let found = [Mode::Mono, Mode::Gray].into_iter().find_map(|m| painted(m).filter(|ink| !calibration.is_empty() && fits(ink)).map(|ink| (m, ink)));
        if let Some((mode, ink)) = found {
            font.mode = Some(mode);
            let mut seen: [Option<Option<u16>>; 17] = [None; 17];
            for (line, ink) in calibration.iter().zip(&ink) {
                for (&px, &ink) in line.pixels.iter().zip(ink) {
                    if let Ink::Partial(Some(level)) = ink {
                        let s = &mut seen[level as usize];
                        *s = Some(match *s { None => Some(px), Some(v) => v.filter(|&v| v == px) });
                    }
                }
            }
            font.on_key = seen.map(Option::flatten);
        }
        font
    }

    /// The pixel black text of grey `level` makes over the colour key, as
    /// the calibration showed it for every pixel of that level.
    pub fn black_on_key(&self, level: u8) -> Option<u16> { *self.on_key.get(level as usize)? }

    /// Whether `TextOutA` of this font is reproduced (else every query is
    /// unanswered).
    pub fn calibrated(&self) -> bool { self.mode.is_some() }

    /// The ink of `text` at (x, y) relative to its pen on the baseline.
    pub fn ink(&self, text: &str, x: i32, y: i32, underline: bool) -> Option<Ink> {
        let bytes = text.chars().map(|c| u8::try_from(u32::from(c)).ok()).collect::<Option<Vec<u8>>>()?;
        let ink = self.glyph_ink(&bytes, x, y)?;
        if underline && ink != Ink::Solid {
            // The underline's own rows are not calibrated.
            let (top, height) = self.underline?;
            // Drawn over the text's extent, which follows the grey advances
            // even where the glyphs are mono.
            let pen: i32 = bytes.iter().map(|&b| self.glyphs[b as usize].as_ref().map_or(0, |g| g.mono.advance.max(g.gray.advance))).sum();
            if x >= 0 && x < pen && y >= top && y < top + height { return Some(Ink::Partial(None)); }
        }
        Some(ink)
    }

    fn glyph_ink(&self, text: &[u8], x: i32, y: i32) -> Option<Ink> {
        let mode = self.mode?;
        let mut pen = 0;
        let mut ink = Ink::None;
        for &b in text {
            let glyph = self.glyphs.get(b as usize)?.as_ref()?;
            let raster = match mode { Mode::Mono => &glyph.mono, Mode::Gray => &glyph.gray };
            let (gx, gy) = (x - pen - raster.origin.0, y + raster.origin.1);
            if gx >= 0 && gy >= 0 && gx < raster.size.0 && gy < raster.size.1 {
                ink = combine(ink, level_ink(mode, raster.levels[(gy * raster.size.0 + gx) as usize]));
            }
            pen += raster.advance;
        }
        Some(ink)
    }
}

fn level_ink(mode: Mode, level: u8) -> Ink {
    match (mode, level) {
        (_, 0) => Ink::None,
        (Mode::Mono, _) | (Mode::Gray, 16) => Ink::Solid,
        (Mode::Gray, level) => Ink::Partial(Some(level)),
    }
}

/// Glyphs meeting at a pixel: any solid one wins; two smoothed ones are a
/// blend of blends.
fn combine(a: Ink, b: Ink) -> Ink {
    match (a, b) {
        (Ink::Solid, _) | (_, Ink::Solid) => Ink::Solid,
        (Ink::Partial(_), Ink::Partial(_)) => Ink::Partial(None),
        (Ink::Partial(level), _) | (_, Ink::Partial(level)) => Ink::Partial(level),
        _ => Ink::None,
    }
}

#[cfg(test)]
mod glyph_tests {
    use super::*;

    /// 'A' (65): a 2x2 block one pixel right of its pen, top 2 above the
    /// baseline, advance 3; its grey raster has one partial pixel.
    fn glyphs() -> Vec<Option<Glyph>> {
        let raster = |levels: Vec<u8>| Raster { origin: (1, 2), size: (2, 2), advance: 3, levels };
        let mut glyphs = vec![None; 256];
        glyphs[65] = Some(Glyph { mono: raster(vec![1, 1, 1, 0]), gray: raster(vec![16, 16, 16, 7]) });
        glyphs
    }

    fn line(text: &[u8], ink: &[(i32, i32, u16)]) -> Calibration {
        let (width, pen) = (12, (2, 4));
        let mut pixels = vec![KEY; (width * 8) as usize];
        for &(x, y, v) in ink { pixels[((y + pen.1) * width + x + pen.0) as usize] = v; }
        Calibration { text: text.to_vec(), pen, width, pixels }
    }

    #[test]
    fn calibration_picks_the_bitmaps_text_out_reproduces_or_none() {
        let solid = [(1, -2, 0), (2, -2, 0), (1, -1, 0), (4, -2, 0), (5, -2, 0), (4, -1, 0)];
        let mono = FontGeometry::new(glyphs(), &[line(b"AA", &solid)], None);
        assert!(mono.calibrated());
        assert_eq!(mono.ink("AA", 4, -2, false), Some(Ink::Solid));
        assert_eq!(mono.ink("AA", 2, -1, false), Some(Ink::None));
        assert_eq!(mono.ink("AA", 3, -2, false), Some(Ink::None), "between the glyphs");
        // A smoothed pixel where the mono bitmap has none: only grey fits.
        let mut smoothed = solid.to_vec();
        smoothed.push((2, -1, 0x4986));
        let uneven = FontGeometry::new(glyphs(), &[line(b"AA", &smoothed)], None);
        assert!(uneven.calibrated());
        assert_eq!(uneven.black_on_key(7), None, "level 7 also left the key unchanged");
        smoothed.push((5, -1, 0x4986));
        let gray = FontGeometry::new(glyphs(), &[line(b"AA", &smoothed)], None);
        assert!(gray.calibrated());
        assert_eq!(gray.ink("AA", 2, -1, false), Some(Ink::Partial(Some(7))));
        assert_eq!(gray.black_on_key(7), Some(0x4986));
        assert_eq!(gray.black_on_key(8), None, "no pixel of that level was seen");
        assert_eq!(gray.ink("AA", 1, -2, false), Some(Ink::Solid));
        // Ink no bitmap explains: no answers at all.
        let mut stray = solid.to_vec();
        stray.push((7, 1, 0));
        let none = FontGeometry::new(glyphs(), &[line(b"AA", &stray)], None);
        assert!(!none.calibrated());
        assert_eq!(none.ink("AA", 1, -2, false), None);
        // A character without a glyph, or outside Latin-1, is not known.
        assert_eq!(mono.ink("AB", 1, -2, false), None);
        assert_eq!(mono.ink("A\u{2014}", 1, -2, false), None);
        // The underline's own rows are possible ink only.
        let lined = FontGeometry::new(glyphs(), &[line(b"AA", &solid)], Some((-1, 1)));
        assert_eq!(lined.ink("AA", 3, 1, true), Some(Ink::Partial(None)));
        assert_eq!(lined.ink("AA", 1, -2, true), Some(Ink::Solid));
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
