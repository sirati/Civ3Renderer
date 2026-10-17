//! Font outlines packed for drawing text on the GPU, at any size from the
//! same data.
//!
//! A TrueType or OpenType (CFF) face is read with `ttf-parser`; every
//! glyph's outline becomes quadratic Bézier curves (lines as straight
//! quadratics, cubics split into quadratics), each cut where it turns in y,
//! oriented the TrueType way, and listed in horizontal bands of its glyph
//! (Eric Lengyel's Slug bands). [`Face::words`] is the whole face as one
//! block of words in the layout `tile_render_shared::text` documents, which
//! the tile renderer's text shaders read.

use std::collections::BTreeMap;

use tile_render_shared::text::{CURVE_WORDS, FACE_HEADER_WORDS, FACE_MAGIC, GLYPH_WORDS};

#[derive(Debug, thiserror::Error)]
pub enum FontError {
    #[error("font file: {0}")]
    Parse(#[from] ttf_parser::FaceParsingError),
    #[error("font file has no glyphs")]
    Empty,
}

/// A face's outlines, packed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Face {
    /// See `tile_render_shared::text`.
    pub words: Vec<u32>,
    /// Glyph index by character.
    cmap: BTreeMap<char, u32>,
    glyphs: u32,
}

impl Face {
    /// Face `index` of a font file (0 unless it is a collection).
    pub fn parse(data: &[u8], index: u32) -> Result<Face, FontError> {
        let face = ttf_parser::Face::parse(data, index)?;
        let glyphs = u32::from(face.number_of_glyphs());
        if glyphs == 0 {
            return Err(FontError::Empty);
        }
        // Coordinates are kept in fractions of a font unit where they fit
        // 16 bits.
        let bbox = face.global_bounding_box();
        let extent = [bbox.x_min, bbox.y_min, bbox.x_max, bbox.y_max].iter().map(|v| i32::from(*v).abs()).max().unwrap_or(0) + face.units_per_em() as i32;
        let fraction = [4, 2, 1].into_iter().find(|f| extent * f < 30_000).unwrap_or(1);
        let mut packer = Packer::new(u32::from(face.units_per_em()) * fraction as u32, glyphs);
        for g in 0..glyphs {
            let id = ttf_parser::GlyphId(g as u16);
            let mut outline = Outline { fraction: fraction as f64, curves: Vec::new(), start: (0.0, 0.0), at: (0.0, 0.0) };
            face.outline_glyph(id, &mut outline);
            let advance = i32::from(face.glyph_hor_advance(id).unwrap_or(0)) * fraction;
            packer.glyph(g, outline.packed(), advance);
        }
        let mut cmap = BTreeMap::new();
        if let Some(table) = face.tables().cmap {
            for sub in table.subtables.into_iter().filter(|s| s.is_unicode()) {
                sub.codepoints(|cp| {
                    if let (Some(c), Some(g)) = (char::from_u32(cp), sub.glyph_index(cp)) {
                        cmap.entry(c).or_insert(u32::from(g.0));
                    }
                });
            }
        }
        Ok(Face { words: packer.words, cmap, glyphs })
    }

    /// A face of glyphs given as curves (quadratics in font units, y up,
    /// each glyph's contours closed) and advances; `chars[i]` is glyph
    /// `i`'s character. For tests and generated shapes.
    pub fn from_curves(units_per_em: u32, glyphs: &[(char, Vec<[(f64, f64); 3]>, i32)]) -> Face {
        let mut packer = Packer::new(units_per_em, glyphs.len() as u32);
        let mut cmap = BTreeMap::new();
        for (i, (c, curves, advance)) in glyphs.iter().enumerate() {
            let outline = Outline { fraction: 1.0, curves: curves.clone(), start: (0.0, 0.0), at: (0.0, 0.0) };
            packer.glyph(i as u32, outline.packed(), *advance);
            cmap.insert(*c, i as u32);
        }
        Face { words: packer.words, cmap, glyphs: glyphs.len() as u32 }
    }

    /// The glyph `c` is drawn with, if the face has one.
    pub fn glyph(&self, c: char) -> Option<u32> {
        self.cmap.get(&c).copied()
    }

    pub fn glyph_count(&self) -> u32 {
        self.glyphs
    }

    /// Word offset of glyph `index`'s record from the start of the face.
    pub fn record(&self, index: u32) -> u32 {
        FACE_HEADER_WORDS + index.min(self.glyphs - 1) * GLYPH_WORDS
    }

    /// Font units per em, in the fractions of a unit the curves are in.
    pub fn units_per_em(&self) -> u32 {
        self.words[1]
    }
}

/// A quadratic curve in font units (y up).
type Quad = [(f64, f64); 3];

/// Collects a glyph's contours as quadratics.
struct Outline {
    fraction: f64,
    curves: Vec<Quad>,
    start: (f64, f64),
    at: (f64, f64),
}

impl Outline {
    fn point(&self, x: f32, y: f32) -> (f64, f64) {
        (f64::from(x) * self.fraction, f64::from(y) * self.fraction)
    }

    fn quad(&mut self, c: (f64, f64), to: (f64, f64)) {
        let from = self.at;
        if from != to || c != to {
            self.curves.push([from, c, to]);
        }
        self.at = to;
    }

    fn line(&mut self, to: (f64, f64)) {
        let from = self.at;
        self.quad(((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0), to);
    }

    /// The curves, cut where they turn in y, in whole units (of the
    /// fraction), clockwise.
    fn packed(&self) -> Vec<[(i32, i32); 3]> {
        let mut out = Vec::new();
        for q in &self.curves {
            for part in split_at_y_turn(q) {
                let [a, b, c] = part.map(|(x, y)| (x.round() as i32, y.round() as i32));
                if a.1 == c.1 && a.1 == b.1 {
                    // Flat: crosses no row (still part of the outline's
                    // bounds).
                    out.push([a, b, c]);
                    continue;
                }
                // Rounding must not turn it: the control point between the
                // ends in y.
                let (lo, hi) = (a.1.min(c.1), a.1.max(c.1));
                out.push([a, (b.0, b.1.clamp(lo, hi)), c]);
            }
        }
        // Clockwise (TrueType): a counter-clockwise outline (CFF) turned.
        let area: f64 = self.curves.iter().map(|[a, b, c]| cross(*a, *b) + cross(*b, *c)).sum();
        if area > 0.0 {
            for q in &mut out {
                q.swap(0, 2);
            }
        }
        out
    }
}

fn cross(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.0 * b.1 - b.0 * a.1
}

/// `q` cut at the parameter where it turns in y, if it does.
fn split_at_y_turn(q: &Quad) -> Vec<Quad> {
    let [a, b, c] = *q;
    let den = a.1 - 2.0 * b.1 + c.1;
    let t = if den.abs() > 1e-12 { (a.1 - b.1) / den } else { -1.0 };
    if t <= 1e-9 || t >= 1.0 - 1e-9 {
        return vec![*q];
    }
    let lerp = |p: (f64, f64), r: (f64, f64)| (p.0 + (r.0 - p.0) * t, p.1 + (r.1 - p.1) * t);
    let (ab, bc) = (lerp(a, b), lerp(b, c));
    let m = lerp(ab, bc);
    // (The turning point's y is exact, so both halves are monotonic.)
    vec![[a, (ab.0, m.1), m], [m, (bc.0, m.1), c]]
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.start = self.point(x, y);
        self.at = self.start;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let to = self.point(x, y);
        self.line(to);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (c, to) = (self.point(x1, y1), self.point(x, y));
        self.quad(c, to);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        // Split into quadratics close enough: the error of approximating a
        // cubic piece by one quadratic is √3/36 · |p3 − 3·p2 + 3·p1 − p0|,
        // shrinking with the cube of the number of pieces.
        let p0 = self.at;
        let (p1, p2, p3) = (self.point(x1, y1), self.point(x2, y2), self.point(x, y));
        let d = ((p3.0 - 3.0 * p2.0 + 3.0 * p1.0 - p0.0).powi(2) + (p3.1 - 3.0 * p2.1 + 3.0 * p1.1 - p0.1).powi(2)).sqrt();
        let tolerance = 0.25 * self.fraction;
        let n = ((3f64.sqrt() / 36.0 * d / tolerance).cbrt().ceil() as usize).clamp(1, 16);
        let at = |t: f64| {
            let u = 1.0 - t;
            let (a, b, c, e) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            (a * p0.0 + b * p1.0 + c * p2.0 + e * p3.0, a * p0.1 + b * p1.1 + c * p2.1 + e * p3.1)
        };
        let tangent = |t: f64| {
            let u = 1.0 - t;
            let (a, b, c) = (3.0 * u * u, 6.0 * u * t, 3.0 * t * t);
            (a * (p1.0 - p0.0) + b * (p2.0 - p1.0) + c * (p3.0 - p2.0), a * (p1.1 - p0.1) + b * (p2.1 - p1.1) + c * (p3.1 - p2.1))
        };
        for i in 0..n {
            let (t0, t1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
            let (s, e) = (at(t0), at(t1));
            // The quadratic's control point: where the end tangents meet,
            // approximated by the midpoint of the two tangent estimates.
            let h = (t1 - t0) / 2.0;
            let (d0, d1) = (tangent(t0), tangent(t1));
            let c = ((s.0 + d0.0 * h + e.0 - d1.0 * h) / 2.0, (s.1 + d0.1 * h + e.1 - d1.1 * h) / 2.0);
            self.quad(c, e);
        }
    }

    fn close(&mut self) {
        if self.at != self.start {
            let start = self.start;
            self.line(start);
        }
    }
}

/// Builds the face block: header, glyph records, then each glyph's bands,
/// curve lists and curves.
struct Packer {
    words: Vec<u32>,
}

impl Packer {
    fn new(units_per_em: u32, glyphs: u32) -> Self {
        let mut words = vec![FACE_MAGIC, units_per_em, glyphs, 0];
        words.resize((FACE_HEADER_WORDS + glyphs * GLYPH_WORDS) as usize, 0);
        Packer { words }
    }

    fn glyph(&mut self, index: u32, curves: Vec<[(i32, i32); 3]>, advance: i32) {
        let record = (FACE_HEADER_WORDS + index * GLYPH_WORDS) as usize;
        self.words[record + 4] = advance as u32;
        if curves.is_empty() {
            return;
        }
        let points = || curves.iter().flatten();
        let (x0, y0) = (points().map(|p| p.0).min().unwrap(), points().map(|p| p.1).min().unwrap());
        let (x1, y1) = (points().map(|p| p.0).max().unwrap(), points().map(|p| p.1).max().unwrap());
        // About four curves a band, at most 16 bands.
        let crossing: Vec<&[(i32, i32); 3]> = curves.iter().filter(|q| q[0].1 != q[2].1).collect();
        let bands = (crossing.len() as u32 / 4).clamp(1, 16);
        let height = ((y1 - y0) as u32 / bands + 1) as i32;
        // The curves first, then the bands' lists of them.
        let mut at = Vec::with_capacity(crossing.len());
        for q in &crossing {
            at.push(self.words.len() as u32);
            self.words.extend(q.iter().map(|&(x, y)| (x as i16 as u16 as u32) | ((y as i16 as u16 as u32) << 16)));
        }
        debug_assert!(CURVE_WORDS == 3);
        let band_table = self.words.len();
        self.words.resize(band_table + 2 * bands as usize, 0);
        for b in 0..bands as i32 {
            let (lo, hi) = (y0 + b * height, y0 + (b + 1) * height);
            let list = self.words.len() as u32;
            for (q, &offset) in crossing.iter().zip(&at) {
                let (cy0, cy1) = (q[0].1.min(q[2].1), q[0].1.max(q[2].1));
                if cy0 <= hi && cy1 >= lo {
                    self.words.push(offset);
                }
            }
            let count = self.words.len() as u32 - list;
            self.words[band_table + 2 * b as usize] = list;
            self.words[band_table + 2 * b as usize + 1] = count;
        }
        let r = &mut self.words[record..record + GLYPH_WORDS as usize];
        r[0] = x0 as u32;
        r[1] = y0 as u32;
        r[2] = x1 as u32;
        r[3] = y1 as u32;
        r[5] = bands;
        r[6] = height as u32;
        r[7] = band_table as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tile_render_shared::text::{GlyphPlace, TextGlyph, bounds, coverage, place};

    /// The game's font, where there is a Civ III installation.
    fn lucida() -> Option<Vec<u8>> {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        std::fs::read(std::path::Path::new(&install).join("LSANS.TTF")).ok()
    }

    #[test]
    fn cubics_split_and_curves_stay_monotonic() {
        let mut o = Outline { fraction: 4.0, curves: Vec::new(), start: (0.0, 0.0), at: (0.0, 0.0) };
        use ttf_parser::OutlineBuilder;
        o.move_to(0.0, 0.0);
        o.curve_to(0.0, 100.0, 100.0, 100.0, 100.0, 0.0);
        o.close();
        assert!(o.curves.len() >= 3, "{} quadratics", o.curves.len());
        for q in o.packed() {
            let (lo, hi) = (q[0].1.min(q[2].1), q[0].1.max(q[2].1));
            assert!(q[1].1 >= lo && q[1].1 <= hi, "{q:?}");
        }
    }

    /// A glyph drawn big covers its inside fully and its outside not at all.
    #[test]
    fn a_square_glyph_covers_its_inside() {
        // A 0..1000 square, as a TrueType contour (clockwise, y up).
        let mut p = Packer::new(1000, 1);
        let sq = |a: (i32, i32), b: (i32, i32)| [a, ((a.0 + b.0) / 2, (a.1 + b.1) / 2), b];
        p.glyph(0, vec![sq((0, 0), (0, 1000)), sq((0, 1000), (1000, 1000)), sq((1000, 1000), (1000, 0)), sq((1000, 0), (0, 0))], 1000);
        let words = p.words;
        // 10 px per em at zoom 1: the square is 10 × 10 pixels above the pen.
        let g = TextGlyph { glyph: FACE_HEADER_WORDS, face: 0, em: 640, advance: 10, color: 0, ..Default::default() };
        let place: GlyphPlace = place(&words, &g, 5, 20, 1, 1).unwrap();
        let (l, t, r, b) = bounds(&words, &place);
        assert!(l <= 4 && r >= 16 && t <= 10 && b >= 20, "{:?}", (l, t, r, b));
        assert_eq!(coverage(&words, &place, 10, 15, 0), [255; 3]);
        assert_eq!(coverage(&words, &place, 2, 15, 0), [0; 3]);
        assert_eq!(coverage(&words, &place, 10, 21, 0), [0; 3]);
        // Just inside the left edge: the filter reaches out, red the furthest.
        let edge = coverage(&words, &place, 5, 15, 0);
        assert!(edge[0] < edge[1] && edge[1] < edge[2], "{edge:?}");
        let out = coverage(&words, &place, 4, 15, 0);
        assert!(out[0] < out[1] && out[1] < out[2] && out[2] < 255, "{out:?}");
    }

    #[test]
    fn lucida_sans_packs_its_glyphs() {
        let Some(data) = lucida() else { return eprintln!("no LSANS.TTF, skipped") };
        let face = Face::parse(&data, 0).unwrap();
        let a = face.glyph('a').expect("a");
        assert!(face.glyph('é').is_some());
        let r = face.record(a) as usize;
        assert!(face.words[r + 5] > 0, "bands");
        eprintln!("{} glyphs, {} KiB, upem {}", face.glyph_count(), face.words.len() * 4 / 1024, face.units_per_em());
    }
}
