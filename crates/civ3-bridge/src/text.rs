//! The game's text (labels, status lines), as it draws it through
//! `jgl.dll`'s `TextOutA`: runs of characters at a baseline position in a
//! font and colour. Drawn with the same GDI-rasterised glyphs
//! (`civ3_assets::fonts`), as screen-space sprites on top of everything.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;

use civ3_assets::GameFiles;
use civ3_assets::fonts::{FontStyle, GlyphFont, charset};
use render_desc::Compiled;

/// One `TextOutA` call: `text` with its pen at (x, baseline y).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextRun {
    pub x: i32,
    pub y: i32,
    pub color: [u8; 3],
    pub face: String,
    /// GDI `lfHeight` (negative: character height).
    pub height: i32,
    /// GDI `lfWeight` (0 or 400 normal, 700 bold), `lfItalic`, `lfUnderline`.
    pub weight: i32,
    pub italic: bool,
    pub underline: bool,
    pub text: String,
}

impl TextRun {
    /// A trace line `text dc D at X Y color R G B font "Face"/H/WEIGHT/ITALIC/UNDERLINE str TEXT`.
    pub fn parse(line: &str) -> Option<TextRun> {
        let rest = line.strip_prefix("text dc ")?;
        let (_, rest) = rest.split_once(" at ")?;
        let (head, text) = rest.split_once(" str ")?;
        let f: Vec<&str> = head.split_whitespace().collect();
        let font = head.split_once(" font ")?.1;
        let (face, metrics) = font.strip_prefix('"')?.split_once("\"/")?;
        let mut m = metrics.split('/');
        let height = m.next()?.parse().ok()?;
        let weight = m.next().and_then(|w| w.parse().ok()).unwrap_or(0);
        let italic = m.next() == Some("1");
        let underline = m.next() == Some("1");
        Some(TextRun {
            x: f.first()?.parse().ok()?,
            y: f.get(1)?.parse().ok()?,
            color: [f.get(3)?.parse().ok()?, f.get(4)?.parse().ok()?, f.get(5)?.parse().ok()?],
            face: face.to_owned(),
            height,
            weight,
            italic,
            underline,
            text: text.to_owned(),
        })
    }
}

type FontKey = (FontStyle, [u8; 3]);

fn key(r: &TextRun) -> FontKey {
    (FontStyle { face: r.face.clone(), height: r.height.abs(), weight: r.weight, italic: r.italic }, r.color)
}

/// The fonts (face, size, colour) text runs need, loaded.
#[derive(Default)]
pub struct Fonts {
    fonts: BTreeMap<FontKey, Option<GlyphFont>>,
}

impl Fonts {
    /// Loads the fonts `runs` use that are not loaded yet; true if any.
    pub fn add(&mut self, files: &GameFiles, runs: &[TextRun]) -> bool {
        let mut added = false;
        for r in runs {
            let key = key(r);
            if !self.fonts.contains_key(&key) {
                let font = civ3_assets::fonts::load(files, &key.0, key.1)
                    .inspect_err(|e| log::warn!("font {key:?}: {e}"))
                    .ok();
                self.fonts.insert(key, font);
                added = true;
            }
        }
        added
    }

    /// Whether `compiled` has the glyphs of `run` (its art is built).
    pub fn compiled(&self, run: &TextRun, compiled: &Compiled) -> bool {
        let key = key(run);
        let first = run.text.chars().next().map_or(32, |c| c as u32);
        self.has(run) && compiled.set(&format!("Text@{}/{first}", Self::tag(&key))).is_some()
    }

    /// How many fonts (style and colour) are loaded.
    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    /// Whether the font of `run` is loaded.
    pub fn has(&self, run: &TextRun) -> bool {
        self.fonts.get(&key(run)).is_some_and(Option::is_some)
    }

    /// The name of `key`'s glyph sets: the font itself, not its place among
    /// the fonts (which moves as fonts are added, before the art is rebuilt).
    fn tag((style, [r, g, b]): &FontKey) -> String {
        format!("{}#{r:02x}{g:02x}{b:02x}", civ3_assets::fonts::cache_stem(style))
    }

    /// Render description entries: glyph sprites `Text@<font>/<code>`
    /// anchored at the pen on the baseline.
    pub fn description(&self) -> String {
        let mut out = String::from("\n# Text glyphs (generated).\n");
        for (n, ((style, [r, g, b]), font)) in self.fonts.iter().enumerate() {
            let Some(font) = font else { continue };
            let tag = Self::tag(&(style.clone(), [*r, *g, *b]));
            let FontStyle { face, height, weight, italic } = style;
            let _ = writeln!(out, "[images.text{n}]\nsource = \"font:{face}/{height}/{r},{g},{b}/{weight}/{}\"\n", *italic as u8);
            for (k, c) in charset().enumerate() {
                let Some(glyph) = font.glyphs.get(&c) else { continue };
                let _ = writeln!(
                    out,
                    "[sheets.text{n}_{k}]\nimage = \"text{n}\"\nframe = {k}\ncell = [{}, {}]\nanchor = [{}, {}]\ngroup = \"Text@{tag}\"\nfirst_index = {}\nscaling = \"keep\"\n",
                    glyph.image.width,
                    glyph.image.height,
                    glyph.left,
                    -font.ascent,
                    c as u32
                );
            }
            // Underline segments, one per glyph advance.
            let (top, thick) = font.underline;
            let advances: BTreeSet<i32> = font.glyphs.values().map(|g| g.advance).filter(|&a| a > 0).collect();
            for a in advances {
                let _ = writeln!(
                    out,
                    "[images.text{n}_line{a}]\nsource = \"solid:{r},{g},{b},255:{a}x{thick}\"\n\n[sheets.text{n}_line{a}]\nimage = \"text{n}_line{a}\"\ncell = [{a}, {thick}]\ngrid = [1, 1]\nanchor = [0, {top}]\ngroup = \"TextLine@{tag}\"\nfirst_index = {a}\nscaling = \"keep\"\n"
                );
            }
        }
        out
    }

    /// Glyph sprites for `runs`, in order, on top of everything.
    pub fn sprites(&self, runs: &[TextRun], compiled: &Compiled) -> Vec<scene::FreeSprite> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for r in runs {
            // The game redraws some lines several times per frame.
            if seen.insert(r) {
                self.run_sprites(r, None, compiled, u32::MAX, &mut out);
            }
        }
        out
    }

    /// Glyph sprites of one run, clipped to `clip` (screen rectangle), drawn
    /// after render pass `after_pass`.
    pub fn run_sprites(
        &self,
        r: &TextRun,
        clip: Option<[i32; 4]>,
        compiled: &Compiled,
        after_pass: u32,
        out: &mut Vec<scene::FreeSprite>,
    ) {
        self.run_sprites_at(r, clip, compiled, after_pass, None, out);
    }

    /// Like [`Fonts::run_sprites`]; with a `zoom`, the run rasterised at that
    /// zoom (its font [`zoomed`] that many times the size) as zoomed art: at
    /// the run's position, each glyph offset by its pen at that zoom (see
    /// `scene::FreeSprite::zoom_art`).
    pub fn run_sprites_at(
        &self,
        r: &TextRun,
        clip: Option<[i32; 4]>,
        compiled: &Compiled,
        after_pass: u32,
        zoom: Option<(u32, u32)>,
        out: &mut Vec<scene::FreeSprite>,
    ) {
        let key = key(&zoom.map_or_else(|| r.clone(), |z| zoomed(r, z)));
        let Some(Some(font)) = self.fonts.get(&key) else { return };
        let tag = Self::tag(&key);
        // 1:1: the pen from the run's x; zoomed: the offset from it.
        let mut pen = if zoom.is_some() { 0 } else { r.x };
        for c in r.text.chars() {
            let Some(glyph) = font.glyphs.get(&c) else { continue };
            // Glyphs hold coverage per colour channel (ClearType); the
            // underline is a plain bar.
            let glyph_blend = tile_render::Blend::Subpixel { color: r.color };
            let line = r.underline.then(|| (format!("TextLine@{tag}/{}", glyph.advance), tile_render::Blend::Alpha));
            for (name, blend) in [Some((format!("Text@{tag}/{}", c as u32), glyph_blend)), line].into_iter().flatten() {
                let Some((set, _)) = compiled.set(&name) else { continue };
                let (x, zoom_art, z, zoom_offset) = match zoom {
                    None => (pen, 0, (0, 0), (0, 0)),
                    Some((zn, zd)) => (r.x, 2, (zn as u16, zd as u16), (pen, 0)),
                };
                out.push(scene::FreeSprite {
                    set: set.raw(),
                    x,
                    y: r.y,
                    screen: true,
                    after_pass,
                    phase: 0,
                    ticks_per_frame: 1,
                    scale_den: blend.scale_den(),
                    tint: blend.tint(),
                    zoom_art,
                    zoom: z,
                    zoom_offset,
                    clip,
                    not_in_view: None,
                });
            }
            pen += glyph.advance;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_lines_parse_with_and_without_style() {
        let r = TextRun::parse("text dc 0 at 10 20 color 0 0 0 font \"Lucida Sans\"/-24/700/1/1 str Rome").unwrap();
        assert_eq!((r.x, r.y, r.height, r.weight, r.italic, r.underline, r.text.as_str()), (10, 20, -24, 700, true, true, "Rome"));
        let old = TextRun::parse("text dc 0 at 1 2 color 3 4 5 font \"Lucida Sans\"/-12 str a b").unwrap();
        assert_eq!((old.color, old.weight, old.italic, old.underline, old.text.as_str()), ([3, 4, 5], 0, false, false, "a b"));
    }
}

/// Run `r` as the game would draw it at `zoom` (`(num, den)`): its font
/// that many times the size (rounded), the rest as it is.
pub fn zoomed(r: &TextRun, (n, d): (u32, u32)) -> TextRun {
    let height = ((r.height.abs() as i64 * n as i64 + d as i64 / 2) / d.max(1) as i64) as i32;
    TextRun { height: height * r.height.signum(), ..r.clone() }
}
