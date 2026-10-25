//! What jgl's objects compute, for our objects (`jgl-ffi::own`): image
//! geometry, clipping, colour tables, font metrics and the byte operations
//! on index images. Values in, values out; `jgl-ffi` reads and writes the
//! objects.
//!
//! From jgl.dll disassembled [V], RVAs in each item's doc.

use crate::Rect;

/// The row width in pixels of an image `width` wide at `bits` per pixel
/// (image +0x04 `create`, RVA 0x1800: rows are 4-byte aligned), `None` for
/// a depth jgl has no rows for (it answers 0x17 then).
pub fn row_width(width: i32, bits: i32) -> Option<i32> {
    Some(match bits {
        8 => (width + 3) & !3,
        16 => (width + 1) & !1,
        24 => width + (width & 1),
        32 => width,
        _ => return None,
    })
}

/// The bytes an image's pixels take (`row` pixels a row).
pub fn image_bytes(row: i32, height: i32, bits: i32) -> usize {
    (row.max(0) as usize) * (height.max(0) as usize) * (bits.max(0) as usize / 8)
}

/// `IntersectRect`: the common part of `a` and `b`, `None` if empty (the
/// result is then all zeroes).
pub fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let r = Rect::new(a.left.max(b.left), a.top.max(b.top), a.right.min(b.right), a.bottom.min(b.bottom));
    (r.left < r.right && r.top < r.bottom).then_some(r)
}

/// Image +0x0C `pixel_at(x, y)` (RVA 0x1BC0): the byte offset of the pixel
/// from the image's bits, `None` past the right or bottom edge (jgl tests
/// `x >= width`, `y >= height` signed; negative ones are not tested and
/// give an offset before the bits, as jgl's would).
pub fn pixel_offset(x: i32, y: i32, width: i32, height: i32, row: i32, bits: i32) -> Option<i32> {
    if x >= width || y >= height {
        return None;
    }
    let bytes = match bits {
        8 | 16 | 24 | 32 => bits / 8,
        _ => return None,
    };
    Some(bytes.wrapping_mul(row.wrapping_mul(y).wrapping_add(x)))
}

/// The pixel format words image +0x04 writes at +0x28..+0x36: 0 (555),
/// then the shifts 10, 5, 0 and the masks 0x7C00, 0x3E0, 0x1F.
pub const FORMAT_555: [u16; 6] = [10, 5, 0, 0x7C00, 0x3E0, 0x1F];

/// The colour key a new image gets (constructor, RVA 0x15F0): graphics
/// +0xB4 answers 0, so 555's magenta with the high word's marker.
pub const COLOR_KEY: u32 = 0x8000_7C1F;

/// jgl's error answers.
pub mod error {
    pub const GDI: u32 = 1;
    pub const ARGUMENT: u32 = 3;
    pub const NULL: u32 = 0x10;
    pub const DEPTH: u32 = 0x17;
    pub const NOT_IMPLEMENTED: u32 = 0x18;
    pub const FONT: u32 = 0xD;
    pub const FONT_RESOURCE: u32 = 0x1C;
    pub const MEMORY: u32 = 4;
}

/// The system colours at both ends of jgl's palettes (RVA 0x688E8,
/// 0x68910), as 0x00RRGGBB.
pub const SYSTEM_LOW: [u32; 10] = [0x0, 0x800000, 0x8000, 0x808000, 0x80, 0x800080, 0x8080, 0xc0c0c0, 0xc0dcc0, 0xa6caf0];
pub const SYSTEM_HIGH: [u32; 10] = [0xfffbf0, 0xa0a0a4, 0x808080, 0xff0000, 0xff00, 0xffff00, 0xff, 0xff00ff, 0xffff, 0xffffff];

/// A colour table's entries after its +0x04 `init` (RVA 0x3D090): (R, G,
/// B, flags) each; the system colours with flags 4 at the ends, greys
/// (i, i, i) with flags 5 between, 8, 9, 246, 247 as `PC_EXPLICIT` (2)
/// indices 8, 9, 0xF5, 0xF6.
pub fn table_init() -> [[u8; 4]; 256] {
    let rgb = |c: u32, flags: u8| [(c >> 16) as u8, (c >> 8) as u8, c as u8, flags];
    let mut e = [[0u8; 4]; 256];
    for (i, entry) in e.iter_mut().enumerate().take(246).skip(10) {
        *entry = [i as u8, i as u8, i as u8, 5];
    }
    for i in 0..10 {
        e[i] = rgb(SYSTEM_LOW[i], 4);
        e[246 + i] = rgb(SYSTEM_HIGH[i], 4);
    }
    e[8] = [8, 0, 0, 2];
    e[9] = [9, 0, 0, 2];
    e[246] = [0xF5, 0, 0, 2];
    e[247] = [0xF6, 0, 0, 2];
    e
}

/// A colour table's 16-bit tables (+0x40C 555, +0x60C 565) of its entries
/// (RVA 0x3D010).
pub fn table16(entries: &[[u8; 4]; 256]) -> ([u16; 256], [u16; 256]) {
    let mut a = [0u16; 256];
    let mut b = [0u16; 256];
    for (i, [r, g, bl, _]) in entries.iter().map(|e| e.map(u16::from)).enumerate() {
        a[i] = (r >> 3) << 10 | (g >> 3) << 5 | bl >> 3;
        b[i] = (r >> 3) << 11 | (g >> 2) << 5 | bl >> 3;
    }
    (a, b)
}

/// The next of jgl's change stamps (colour table +0x810, RVA 0x3CFB0): a
/// number in 1..=0xFFFF from a linear congruential generator seeded with
/// the performance counter's low word. Returns (stamp, next seed).
pub fn stamp(seed: u32) -> (u32, u32) {
    let mut s = seed;
    loop {
        s = s.wrapping_mul(0x41C6_4E6D).wrapping_add(0x3039);
        let v = (s >> 16) % 0xFFFF;
        if v != 0 {
            return (v, s);
        }
    }
}

/// The `LOGFONTA` fields font +0x08 `create(face, height, flags)` sets
/// (RVA 0x3C270): `lfHeight` −height, bold (700) for flag 1, italic for
/// flag 2, underline for flag 4, `OUT_TT_ONLY_PRECIS` (7), the rest 0.
pub fn logfont(height: i32, flags: u32) -> (i32, i32, bool, bool) {
    (height.wrapping_neg(), if flags & 1 != 0 { 700 } else { 0 }, flags & 2 != 0, flags & 4 != 0)
}

/// What font +0x08 writes into the game's metrics (words 2..=6 of the
/// structure the constructor was given) from `GetTextMetricsA`: height plus
/// external leading, ascent less internal leading, internal leading,
/// ascent, descent.
pub fn font_metrics(height: i32, ascent: i32, descent: i32, internal: i32, external: i32) -> [i32; 5] {
    [height.wrapping_add(external), ascent.wrapping_sub(internal), internal, ascent, descent]
}

/// The bytes of an index (8-bit) image as rows: `row` bytes each.
pub struct Indexed<'a> {
    pub bytes: &'a mut [u8],
    pub row: usize,
}

impl Indexed<'_> {
    fn rows(&mut self, r: Rect) -> impl Iterator<Item = &mut [u8]> {
        let row = self.row;
        let (x0, x1) = (r.left.max(0) as usize, r.right.max(0) as usize);
        self.bytes
            .chunks_mut(row.max(1))
            .skip(r.top.max(0) as usize)
            .take((r.bottom - r.top).max(0) as usize)
            .filter_map(move |line| line.get_mut(x0.min(line.len())..x1.min(line.len())))
    }

    /// Image +0x44 `fill(rect, color)` on an index image: every byte of
    /// `r` (already clipped) `color`.
    pub fn fill(&mut self, r: Rect, color: u8) {
        for line in self.rows(r) {
            line.fill(color);
        }
    }

    /// Image +0x58 `replace_color(rect, from, to)` (RVA 0x2070) on an
    /// index image: every byte of `r` (already clipped) equal to `from`
    /// becomes `to`.
    pub fn replace(&mut self, r: Rect, from: u8, to: u8) {
        for line in self.rows(r) {
            for b in line.iter_mut().filter(|b| **b == from) {
                *b = to;
            }
        }
    }
}

/// The part of `rect` (or the whole image for `None`) inside the clip
/// that jgl's fills and `replace_color` touch.
pub fn clipped(rect: Option<Rect>, clip: Rect) -> Option<Rect> {
    match rect {
        Some(r) => intersect(r, clip),
        None => (clip.left < clip.right && clip.top < clip.bottom).then_some(clip),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_and_pixels_as_jgl_has_them() {
        assert_eq!(row_width(5, 8), Some(8));
        assert_eq!(row_width(5, 16), Some(6));
        assert_eq!(row_width(5, 24), Some(6));
        assert_eq!(row_width(5, 32), Some(5));
        assert_eq!(row_width(5, 15), None);
        assert_eq!(pixel_offset(3, 2, 5, 4, 6, 16), Some(2 * (6 * 2 + 3)));
        assert_eq!(pixel_offset(5, 0, 5, 4, 6, 16), None);
        assert_eq!(pixel_offset(0, 4, 5, 4, 6, 16), None);
        assert_eq!(pixel_offset(-1, 0, 5, 4, 8, 8), Some(-1), "negative coordinates are not tested");
        assert_eq!(image_bytes(6, 4, 16), 48);
    }

    #[test]
    fn intersections() {
        assert_eq!(intersect(Rect::new(0, 0, 10, 10), Rect::new(5, 5, 20, 20)), Some(Rect::new(5, 5, 10, 10)));
        assert_eq!(intersect(Rect::new(0, 0, 10, 10), Rect::new(10, 0, 20, 10)), None);
        assert_eq!(clipped(None, Rect::new(1, 2, 3, 4)), Some(Rect::new(1, 2, 3, 4)));
    }

    #[test]
    fn colour_tables() {
        let e = table_init();
        assert_eq!(e[1], [0x80, 0, 0, 4]);
        assert_eq!(e[100], [100, 100, 100, 5]);
        assert_eq!(e[9], [9, 0, 0, 2]);
        assert_eq!(e[247], [0xF6, 0, 0, 2]);
        assert_eq!(e[255], [0xFF, 0xFF, 0xFF, 4]);
        let (a, b) = table16(&e);
        assert_eq!(a[255], 0x7FFF);
        assert_eq!(b[255], 0xFFFF);
        assert_eq!(a[1], 0x10 << 10);
        let (s, _) = stamp(12345);
        assert!((1..=0xFFFF).contains(&s));
    }

    #[test]
    fn fonts() {
        assert_eq!(logfont(14, 5), (-14, 700, false, true));
        assert_eq!(font_metrics(16, 13, 3, 2, 1), [17, 11, 2, 13, 3]);
    }

    #[test]
    fn index_operations() {
        let mut bytes = vec![0u8; 4 * 3];
        let mut img = Indexed { bytes: &mut bytes, row: 4 };
        img.fill(Rect::new(1, 1, 3, 3), 7);
        img.replace(Rect::new(0, 0, 4, 3), 7, 9);
        assert_eq!(bytes, [0, 0, 0, 0, 0, 9, 9, 0, 0, 9, 9, 0]);
    }
}
