//! Text drawn from glyph outlines: the layout of the outline buffer and of
//! the glyph instances, and the coverage computation the text shaders and
//! the CPU reference share.
//!
//! # Outline buffer
//!
//! One block of `u32` words per font face (all faces back to back in one
//! storage buffer), independent of the size text is drawn at:
//!
//! ```text
//! face header   [FACE_MAGIC, units per em, glyph count, 0]
//! glyph records GLYPH_WORDS each: xmin, ymin, xmax, ymax, advance (font
//!               units), band count, band height (font units), bands offset
//! bands         per band: curve list offset, curve count
//! curve lists   word offsets of curves
//! curves        CURVE_WORDS each: three points (x low 16 bits, y high 16
//!               bits, signed font units, y up)
//! ```
//!
//! (Offsets are in words from the face header.) Every curve is a quadratic
//! Bézier that is monotonic in y (split at its y extremum), with its contour
//! running clockwise (TrueType's orientation: a curve going up enters the
//! glyph, seen from the left). A glyph's bands cut its height
//! `[ymin, ymin + bands · band height]` into equal horizontal strips, each
//! listing the curves whose y range meets it (Lengyel's Slug bands), so a
//! sample only looks at the curves near its row.
//!
//! # Coverage
//!
//! Everything is integer arithmetic, so the GPU and the CPU reference give
//! the same bits. Positions are fixed point, [`TEXT_FIXED`] units per pixel.
//! A pixel is cut into three subpixels (red, green, blue, left to right) and
//! [`TEXT_ROWS`] rows. Along each row the outline's crossings give the exact
//! covered length of every subpixel (the winding number integrated over the
//! subpixel's width); the rows are summed, the middle ones weighted three
//! times the outer ones (crisper horizontal strokes than a box). Each
//! channel then takes the five subpixels around its own through a
//! colour-balanced LCD filter (`{a, b, c, b, a}` with `a + b = c`,
//! `2a + 2b + c = 1`; FreeType's "light" one, a = 0, the sharpest of them),
//! and a contrast boost that depends on the colour (light text gets more,
//! as Windows' ClearType draws it) turns that into the 8-bit coverage
//! drawn per channel, like a measured glyph's.

use crate::{FREE_CLIP, FREE_SCREEN};

/// Fixed-point units per pixel.
pub const TEXT_FIXED: i32 = 96;
/// Fixed-point units per subpixel (a third of a pixel).
const SUB: i32 = TEXT_FIXED / 3;
/// Rows sampled per pixel.
pub const TEXT_ROWS: i32 = 4;
/// Halvings of a curve before its crossing with a row is taken on its chord.
const BISECT: u32 = 6;
/// Font units scale to fixed-point units by `scale / 2^SCALE_SHIFT`.
const SCALE_SHIFT: u32 = 12;
/// The largest em drawn, in pixels (larger text is drawn at this size).
pub const TEXT_MAX_EM: i32 = 256;
/// Text smaller than this em (pixels at the view's zoom) is darkened: its
/// stems widened by `(SMALL_EM − em) / 24` pixels (a quarter pixel at 6),
/// as hinting keeps Windows' small text dark (stems at least a pixel wide),
/// where an outline's exact coverage turns them grey.
pub const SMALL_EM: i32 = 12;

/// How far a glyph of `em64` 64ths of a pixel at zoom `n/d` is widened (see
/// [`SMALL_EM`]), in fixed-point units.
pub fn darkening(em64: u32, n: u32, d: u32) -> i32 {
    let em = (if em64 > 1 << 14 { 1 << 14 } else { em64 }) as i32;
    let (n, d) = (n as i32, if d == 0 { 1 } else { d as i32 });
    let at_zoom = em * n * TEXT_FIXED / (64 * d);
    let short = SMALL_EM * TEXT_FIXED - at_zoom;
    if short > 0 { short / 24 } else { 0 }
}

/// First word of a face block.
pub const FACE_MAGIC: u32 = 0x3158_5447;
pub const FACE_HEADER_WORDS: u32 = 4;
pub const GLYPH_WORDS: u32 = 8;
pub const CURVE_WORDS: u32 = 3;

/// `TextGlyph::flags` (beside `FREE_SCREEN`, `FREE_CLIP` and
/// `FREE_EXCEPT_VIEW`): emboldened by a pixel, as Windows makes bold of a
/// regular face.
pub const TEXT_BOLD: u32 = 1 << 8;
/// Slanted by a third of the height, as Windows makes italics of a regular
/// face.
pub const TEXT_ITALIC: u32 = 1 << 9;
/// Drawn onto `TextGlyph::key` in 15 bits, opaque where that differs from
/// the key, nothing where not (text a game draws onto a canvas cleared to
/// its colour key).
pub const TEXT_KEYED: u32 = 1 << 10;
/// A bar instead of a glyph (an underline): `advance` wide, `bar_height`
/// high, `bar_top` below the baseline.
pub const TEXT_BAR: u32 = 1 << 11;

/// The contrast boost of black text and of white text (others in between
/// per channel), in 256ths: coverage `c` (of 255) becomes
/// `c + c·(255 − c)·boost / (255·256)`. Measured against the glyph sheets
/// Windows draws (Lucida Sans, letters and digits, `tools/gen-civ3-fonts`):
/// the boosts at which the outlines put down as much ink per glyph as
/// ClearType does, black text on white and white on black, are 16 and 120
/// at 24 pixels (at 10..16 pixels, where hinting thins stems, 0..56 and
/// 8..160). White text gets less, 72: compared with the city labels Windows
/// draws at each zoomed size (white names), the outlines with 120 came out
/// 7 to 12 % heavier at 2x, with 72 within -6..+7 % at 1.5x, 2x and 3x.
pub const BOOST_DARK: u32 = 16;
pub const BOOST_LIGHT: u32 = 72;

/// One glyph of text drawn from its outline: zoomed art for every zoom,
/// drawn only where a view is drawn at its zoom showing zoomed art
/// (`FrameParams::zoom_art` 2), at its 1:1 position scaled.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[cfg_attr(not(target_arch = "spirv"), derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct TextGlyph {
    /// Word offset of the glyph's record in the outline buffer.
    pub glyph: u32,
    /// Word offset of its face's header.
    pub face: u32,
    /// `FREE_SCREEN`, `FREE_CLIP`, `FREE_EXCEPT_VIEW` and `TEXT_*` flags.
    pub flags: u32,
    /// As `FreeSprite::view`.
    pub view: u32,

    /// The pen on the baseline at 1:1: world pixels, or viewport pixels with
    /// `FREE_SCREEN`.
    pub x: i32,
    pub y: i32,
    /// The em at 1:1, in 64ths of a pixel.
    pub em: u32,
    /// The pen advance at 1:1 in pixels: the glyph is centred in it scaled
    /// (so text keeps its 1:1 layout); a bar's width.
    pub advance: i32,

    /// With `FREE_CLIP`: as `FreeSprite::clip`, at 1:1.
    pub clip: [i32; 4],

    /// 0xRRGGBB.
    pub color: u32,
    /// With `TEXT_KEYED`: the colour key, 0xRRGGBB.
    pub key: u32,
    /// With `TEXT_BAR`: its top below the baseline and its height, at 1:1.
    pub bar_top: i32,
    pub bar_height: i32,
}

/// Where and how big a glyph is drawn: all in fixed-point units of the
/// viewport at the view's zoom (see [`place`]).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct GlyphPlace {
    /// The pen on the baseline.
    pub x: i32,
    pub y: i32,
    /// Font units to fixed-point units, `· scale / 2^12`.
    pub scale: i32,
    /// How far crossings leaving the glyph move right (emboldening: bold,
    /// and small text darkened, [`darkening`]).
    pub bold: i32,
    /// `TEXT_*` flags.
    pub flags: u32,
    /// The glyph record and its face (word offsets; the face's own offsets
    /// are from its start).
    pub glyph: u32,
    pub face: u32,
    /// A bar's rectangle from the pen: width, top, height (y down).
    pub bar_w: i32,
    pub bar_top: i32,
    pub bar_h: i32,
}

fn min(a: i32, b: i32) -> i32 {
    if a < b { a } else { b }
}

fn max(a: i32, b: i32) -> i32 {
    if a > b { a } else { b }
}

fn clamp(v: i32, lo: i32, hi: i32) -> i32 {
    if v < lo { lo } else if v > hi { hi } else { v }
}

/// `a / b` rounded down (`b > 0`).
pub fn floor_div(a: i32, b: i32) -> i32 {
    if a >= 0 { a / b } else { -((-a + b - 1) / b) }
}

/// A 1:1 length `v` (pixels) at zoom `n/d` in fixed-point units, rounded
/// down; `None` if it is that far (65536 pixels) off.
pub fn fixed_at_zoom(v: i32, n: u32, d: u32) -> Option<i32> {
    let (n, d) = (n as i32, d as i32);
    if v > 1 << 20 || v < -(1 << 20) || n > 255 || d > 255 || n < 1 || d < 1 {
        return None;
    }
    let p = v * n;
    let q = floor_div(p, d);
    if q > 1 << 16 || q < -(1 << 16) {
        return None;
    }
    Some(q * TEXT_FIXED + (p - q * d) * TEXT_FIXED / d)
}

/// The font-units-to-fixed-point factor of an em of `em64` 64ths of a
/// pixel at zoom `n/d` with `upem` units per em (at most [`TEXT_MAX_EM`]
/// pixels per em).
pub fn scale_of(em64: u32, n: u32, d: u32, upem: u32) -> i32 {
    let upem = if upem == 0 { 1 } else { upem };
    // em64·F·64 = em pixels · F · 2^12; then · n / (upem · d).
    let em64 = if em64 > 1 << 14 { 1 << 14 } else { em64 };
    let a = em64 * TEXT_FIXED as u32 * 64;
    let (q, r) = (a / upem, a % upem);
    let s = (q * n + r * n / upem) / d;
    let most = (TEXT_MAX_EM as u32 * TEXT_FIXED as u32) << SCALE_SHIFT;
    let most = most / upem;
    (if s > most { most } else { s }) as i32
}

/// Font units to fixed-point units.
fn units(v: i32, scale: i32) -> i32 {
    (v * scale) >> SCALE_SHIFT
}

fn word(words: &[u32], at: u32) -> u32 {
    words[at as usize]
}

fn signed(words: &[u32], at: u32) -> i32 {
    words[at as usize] as i32
}

/// Places glyph instance `g` whose pen lies at (`rel_x`, `rel_y`) 1:1
/// pixels from where the view's coordinates start, in a view at zoom `n/d`:
/// scaled, centred in its scaled 1:1 advance. `None`: nowhere near.
pub fn place(words: &[u32], g: &TextGlyph, rel_x: i32, rel_y: i32, n: u32, d: u32) -> Option<GlyphPlace> {
    let x = fixed_at_zoom(rel_x, n, d)?;
    let y = fixed_at_zoom(rel_y, n, d)?;
    let advance = fixed_at_zoom(g.advance, n, d)?;
    let flags = g.flags & (TEXT_BOLD | TEXT_ITALIC | TEXT_KEYED | TEXT_BAR);
    if flags & TEXT_BAR != 0 {
        let top = fixed_at_zoom(g.bar_top, n, d)?;
        let h = fixed_at_zoom(g.bar_height, n, d)?;
        return Some(GlyphPlace { x, y, scale: 0, bold: 0, flags, glyph: 0, face: 0, bar_w: advance, bar_top: top, bar_h: h });
    }
    if word(words, g.face) != FACE_MAGIC {
        return None;
    }
    let scale = scale_of(g.em, n, d, word(words, g.face + 1));
    let bold = if flags & TEXT_BOLD != 0 { TEXT_FIXED } else { 0 } + darkening(g.em, n, d);
    let own = units(signed(words, g.glyph + 4), scale) + bold;
    Some(GlyphPlace { x: x + floor_div(advance - own, 2), y, scale, bold, flags, glyph: g.glyph, face: g.face, bar_w: 0, bar_top: 0, bar_h: 0 })
}

/// The italic slant at height `y` (fixed point, up).
fn slant(y: i32, flags: u32) -> i32 {
    if flags & TEXT_ITALIC != 0 { floor_div(y, 3) } else { 0 }
}

/// The viewport pixels (left, top, right, bottom; exclusive) a placed glyph
/// may cover.
pub fn bounds(words: &[u32], p: &GlyphPlace) -> (i32, i32, i32, i32) {
    let f = TEXT_FIXED;
    if p.flags & TEXT_BAR != 0 {
        let (l, t) = (floor_div(p.x, f), floor_div(p.y + p.bar_top, f));
        return (l, t, floor_div(p.x + p.bar_w - 1, f) + 1, floor_div(p.y + p.bar_top + p.bar_h - 1, f) + 1);
    }
    let g = p.glyph;
    let (x0, y0) = (units(signed(words, g), p.scale), units(signed(words, g + 1), p.scale));
    let (x1, y1) = (units(signed(words, g + 2), p.scale), units(signed(words, g + 3), p.scale));
    let (s0, s1) = (slant(y0, p.flags), slant(y1, p.flags));
    // The filter reaches two subpixels out.
    let margin = 2 * SUB + 1;
    let left = p.x + x0 + min(s0, s1) - margin;
    let right = p.x + x1 + max(s0, s1) + p.bold + margin;
    (floor_div(left, f), floor_div(p.y - y1 - 1, f), floor_div(right, f) + 1, floor_div(p.y - y0 + 1, f) + 1)
}

/// The covered length of each of seven subpixels along one row.
struct Row {
    c: [i32; 7],
    /// Crossings right of every window: each adds a window's width (or takes
    /// it away) from all.
    full: i32,
}

impl Row {
    /// A crossing at `x` (from the first window's left edge) entering
    /// (`enter`) or leaving the glyph.
    fn cross(&mut self, x: i32, enter: bool) {
        let mut j = 0;
        while j < 7 {
            let v = clamp(x - j as i32 * SUB, 0, SUB);
            self.c[j] += if enter { -v } else { v };
            j += 1;
        }
    }
}

/// Where a y-monotonic quadratic from `(x0, y0)` up to `(x2, y2)` (`y0 <= y
/// < y2`) crosses height `y`: by halving the curve, then on the chord.
fn crossing(mut x0: i32, mut y0: i32, mut x1: i32, mut y1: i32, mut x2: i32, mut y2: i32, y: i32) -> i32 {
    let mut i = 0;
    while i < BISECT {
        let (ax, ay) = ((x0 + x1) >> 1, (y0 + y1) >> 1);
        let (bx, by) = ((x1 + x2) >> 1, (y1 + y2) >> 1);
        let (mx, my) = ((ax + bx) >> 1, (ay + by) >> 1);
        if y < my {
            (x1, y1, x2, y2) = (ax, ay, mx, my);
        } else {
            (x0, y0, x1, y1) = (mx, my, bx, by);
        }
        i += 1;
    }
    x0 + (y - y0) * (x2 - x0) / (y2 - y0)
}

/// One curve's point `i` in fixed point from the pen (y up, slanted).
fn point(words: &[u32], curve: u32, i: u32, scale: i32, flags: u32) -> (i32, i32) {
    let w = word(words, curve + i);
    let (x, y) = (((w << 16) as i32) >> 16, (w as i32) >> 16);
    let (x, y) = (units(x, scale), units(y, scale));
    (x + slant(y, flags), y)
}

/// The row at height `y` (fixed point, up from the pen) across the seven
/// subpixels whose first starts at `left` (from the pen).
fn row(words: &[u32], p: &GlyphPlace, y: i32, left: i32, out: &mut Row) {
    let g = p.glyph;
    let bands = word(words, g + 5);
    let height = signed(words, g + 6);
    let ymin = signed(words, g + 1);
    if bands == 0 || y < units(ymin, p.scale) {
        return;
    }
    // The band holding the row: the first whose top is above it.
    let mut b = 0;
    while b < bands && y >= units(ymin + (b as i32 + 1) * height, p.scale) {
        b += 1;
    }
    if b == bands {
        return;
    }
    let band = p.face + word(words, g + 7) + 2 * b;
    let (list, count) = (p.face + word(words, band), word(words, band + 1));
    let right = left + 7 * SUB;
    let mut i = 0;
    while i < count {
        let c = p.face + word(words, list + i);
        let (mut x0, mut y0) = point(words, c, 0, p.scale, p.flags);
        let (x1, y1) = point(words, c, 1, p.scale, p.flags);
        let (mut x2, mut y2) = point(words, c, 2, p.scale, p.flags);
        i += 1;
        // Going up: entering the glyph; down: leaving it (taken upwards).
        let enter = y2 > y0;
        if !enter {
            (x0, y0, x2, y2) = (x2, y2, x0, y0);
        }
        if y < y0 || y >= y2 {
            continue;
        }
        let shift = if enter { 0 } else { p.bold };
        let lo = min(min(x0, x1), x2) + shift;
        let hi = max(max(x0, x1), x2) + shift;
        if hi <= left {
            continue;
        }
        if lo >= right {
            out.full += if enter { -1 } else { 1 };
            continue;
        }
        out.cross(crossing(x0, y0, x1, y1, x2, y2, y) + shift - left, enter);
    }
}

/// The seven subpixels' coverage of viewport pixel (`px`, `py`), summed
/// over the rows (each of [`TEXT_ROWS`] at most `SUB`); subpixel 0 is two
/// left of the pixel's red one.
fn subpixels(words: &[u32], p: &GlyphPlace, px: i32, py: i32) -> [i32; 7] {
    let mut total = [0i32; 7];
    let left = px * TEXT_FIXED - 2 * SUB - p.x;
    let mut r = 0;
    while r < TEXT_ROWS {
        // Rows at the middles of equal strips, y up from the pen.
        let below = py * TEXT_FIXED + (2 * r + 1) * TEXT_FIXED / (2 * TEXT_ROWS) - p.y;
        let mut row_cov = Row { c: [0; 7], full: 0 };
        if p.flags & TEXT_BAR != 0 {
            if below >= p.bar_top && below < p.bar_top + p.bar_h {
                row_cov.cross(-left, true);
                row_cov.cross(p.bar_w - left, false);
            }
        } else {
            row(words, p, -below, left, &mut row_cov);
        }
        let mut j = 0;
        while j < 7 {
            total[j] += clamp(row_cov.c[j] + row_cov.full * SUB, 0, SUB) * row_weight(r);
            j += 1;
        }
        r += 1;
    }
    total
}

/// Row `r`'s weight in a pixel's vertical filter (of [`ROW_TOTAL`]).
fn row_weight(r: i32) -> i32 {
    if r == 0 || r == TEXT_ROWS - 1 { ROW_EDGE } else { ROW_MIDDLE }
}

/// (Weights of the outer rows and the middle ones.)
const ROW_EDGE: i32 = 1;
const ROW_MIDDLE: i32 = 3;
const ROW_TOTAL: i32 = 2 * ROW_EDGE + (TEXT_ROWS - 2) * ROW_MIDDLE;

/// The LCD filter's tap `k` (0..5), in 96ths.
fn tap(k: usize) -> i32 {
    match k {
        0 | 4 => 0,
        1 | 3 => 32,
        _ => 32,
    }
}

/// Coverage `c` (of 255) boosted by `boost` 256ths (at most 256, see
/// [`BOOST_DARK`]).
pub fn boosted(c: u32, boost: u32) -> u32 {
    let v = c + c * (255 - c) * boost / (255 * 256);
    if v > 255 { 255 } else { v }
}

/// The boost of a channel of text whose colour has value `c8` there.
pub fn boost_of(c8: u32) -> u32 {
    (BOOST_DARK * (255 - c8) + BOOST_LIGHT * c8) / 255
}

/// The coverage of viewport pixel (`px`, `py`) per channel (red, green,
/// blue), 0..=255, before the contrast boost.
pub fn linear_coverage(words: &[u32], p: &GlyphPlace, px: i32, py: i32) -> [u32; 3] {
    let s = subpixels(words, p, px, py);
    let mut out = [0u32; 3];
    let full = 96 * ROW_TOTAL * SUB;
    let mut c = 0;
    while c < 3 {
        let mut v = 0;
        let mut k = 0;
        while k < 5 {
            v += tap(k) * s[c + k];
            k += 1;
        }
        out[c] = ((v * 255 + full / 2) / full) as u32;
        c += 1;
    }
    out
}

/// The coverage of viewport pixel (`px`, `py`) per channel (red, green,
/// blue), 0..=255, for text in `color` (0xRRGGBB).
pub fn coverage(words: &[u32], p: &GlyphPlace, px: i32, py: i32, color: u32) -> [u32; 3] {
    let linear = linear_coverage(words, p, px, py);
    let mut out = [0u32; 3];
    let mut c = 0;
    while c < 3 {
        out[c] = boosted(linear[c], boost_of((color >> (16 - 8 * c as u32)) & 0xFF));
        c += 1;
    }
    out
}

/// Keyed text (`TEXT_KEYED`): the colour drawn at a pixel of `coverage`,
/// text in `color` blended onto `key` and kept in 15 bits, if it differs
/// from the key there (else nothing is drawn).
pub fn keyed(coverage: [u32; 3], color: u32, key: u32) -> Option<[u32; 3]> {
    let mut out = [0u32; 3];
    let mut differs = false;
    let mut c = 0;
    while c < 3 {
        let shift = 16 - 8 * c as u32;
        let (k, t) = ((key >> shift) & 0xFF, (color >> shift) & 0xFF);
        let five = (k * (255 - coverage[c]) + t * coverage[c]) / 255 >> 3;
        differs |= five != k >> 3;
        out[c] = (five << 3) | (five >> 2);
        c += 1;
    }
    if differs { Some(out) } else { None }
}

/// How far a glyph instance's clip rectangle reaches at zoom `n/d`, in
/// viewport pixels from where its coordinates start (`base`: that start in
/// its 1:1 coordinates at the view's scale), if it has one; `downscale`:
/// a world glyph's in a view showing the world that many times smaller
/// (its clip shrunk with it, `crate::shrunk`).
pub fn clip_at_zoom(g: &TextGlyph, base: (i32, i32), n: u32, d: u32, downscale: u32) -> Option<[i32; 4]> {
    if g.flags & FREE_CLIP == 0 {
        return None;
    }
    let s = |v: i32, b: i32| floor_div((crate::shrunk(v, downscale) - b) * n as i32, d as i32);
    Some([s(g.clip[0], base.0), s(g.clip[1], base.1), s(g.clip[2], base.0), s(g.clip[3], base.1)])
}

/// Whether the glyph's position is in viewport pixels.
pub fn on_screen(g: &TextGlyph) -> bool {
    g.flags & FREE_SCREEN != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lcd_filter_is_normalized_and_colour_balanced() {
        let (a, b, c) = (tap(0), tap(1), tap(2));
        assert_eq!((tap(4), tap(3)), (a, b));
        assert_eq!(a + b, c);
        assert_eq!(2 * a + 2 * b + c, 96);
    }

    #[test]
    fn lengths_scale_to_fixed_point_rounding_down() {
        assert_eq!(fixed_at_zoom(10, 3, 2), Some(15 * TEXT_FIXED));
        assert_eq!(fixed_at_zoom(1, 1, 3), Some(TEXT_FIXED / 3));
        assert_eq!(fixed_at_zoom(-1, 2, 3), Some(-2 * TEXT_FIXED / 3));
        assert_eq!(fixed_at_zoom(1 << 21, 1, 1), None);
        // 12 px per em at 3/2, 2048 units per em: 18 px.
        assert_eq!(units(2048, scale_of(12 * 64, 3, 2, 2048)), 18 * TEXT_FIXED);
    }

    #[test]
    fn small_text_is_darkened() {
        assert_eq!(darkening(6 * 64, 1, 1), TEXT_FIXED / 4);
        assert_eq!(darkening(10 * 64, 1, 2), (12 * 96 - 5 * 96) / 24);
        assert_eq!((darkening(12 * 64, 1, 1), darkening(10 * 64, 2, 1)), (0, 0));
    }

    #[test]
    fn boosting_keeps_the_ends() {
        for boost in [0, BOOST_DARK, BOOST_LIGHT, 256] {
            assert_eq!((boosted(0, boost), boosted(255, boost)), (0, 255));
            assert!(boosted(128, boost) >= 128);
        }
    }

    #[test]
    fn keyed_text_is_nothing_where_it_stays_the_key() {
        assert_eq!(keyed([0; 3], 0, 0xFF00FF), None);
        assert_eq!(keyed([255; 3], 0, 0xFF00FF), Some([0, 0, 0]));
        assert_eq!(keyed([128, 128, 128], 0, 0xFF00FF), Some([123, 0, 123]));
    }
}
