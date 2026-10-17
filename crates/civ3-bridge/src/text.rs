//! The game's text (labels, status lines), as it draws it through
//! `jgl.dll`'s `TextOutA`: runs of characters at a baseline position in a
//! font and colour. Drawn 1:1 with the same GDI-rasterised glyphs
//! (`civ3_assets::fonts`), as screen-space sprites on top of everything,
//! and at other zooms from the face's outlines (see [`Fonts::run_glyphs`]).

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
    /// Drawn onto this colour, then put where it differs from it only: text
    /// the game draws on a canvas cleared to its colour key (F1's panel),
    /// whose edges Windows blends with the key, and the canvas, put on the
    /// screen keyed, shows them so (magenta fringes, measured).
    pub backdrop: Option<[u8; 3]>,
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
            backdrop: None,
        })
    }
}

type FontKey = (FontStyle, [u8; 3], Option<[u8; 3]>);

/// The font `r` is drawn with at 1:1: its style at the nearest size there
/// are glyphs for (see `civ3_assets::fonts::available_height`).
fn key(r: &TextRun) -> FontKey {
    let style = FontStyle { face: r.face.clone(), height: r.height.abs(), weight: r.weight, italic: r.italic };
    (FontStyle { height: civ3_assets::fonts::available_height(&style), ..style }, r.color, r.backdrop)
}

/// The fonts (face, size, colour) text runs need, loaded.
#[derive(Default)]
pub struct Fonts {
    fonts: BTreeMap<FontKey, Option<GlyphFont>>,
    /// Fonts (face, size, style) that failed to load: reported once, not per
    /// colour.
    failed: BTreeSet<FontStyle>,
    /// When each font was last asked for (see [`Fonts::prune`]).
    used: BTreeMap<FontKey, std::time::Instant>,
    /// The fonts the art has (see [`Fonts::addition`]); pruning does not
    /// take them out of it.
    described: BTreeSet<FontKey>,
}

impl Fonts {
    /// Loads the fonts `runs` use that are not loaded yet; true if any.
    pub fn add(&mut self, files: &GameFiles, runs: &[TextRun]) -> bool {
        let mut added = false;
        for r in runs {
            let key = key(r);
            self.used.insert(key.clone(), std::time::Instant::now());
            if !self.fonts.contains_key(&key) {
                let font = civ3_assets::fonts::load(files, &key.0, key.1)
                    .inspect_err(|e| {
                        if self.failed.insert(key.0.clone()) {
                            log::warn!("font {:?}: {e}", key.0);
                        }
                    })
                    .ok();
                self.fonts.insert(key, font);
                added = true;
            }
        }
        added
    }

    /// Loads the font of `style` in `color` (on `backdrop`) if it is not
    /// loaded yet, as text drawn with it would (art made ahead); true if it
    /// was new.
    pub fn add_style(&mut self, files: &GameFiles, style: FontStyle, color: [u8; 3], backdrop: Option<[u8; 3]>) -> bool {
        let run = TextRun {
            x: 0,
            y: 0,
            color,
            face: style.face.clone(),
            height: -style.height,
            weight: style.weight,
            italic: style.italic,
            underline: false,
            text: String::new(),
            backdrop,
        };
        let new = self.add(files, std::slice::from_ref(&run));
        // (Not used yet: as old as can be, for pruning.)
        if let Some(t) = self.used.get_mut(&key(&run)) {
            *t = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(3600)).unwrap_or(*t);
        }
        new
    }

    /// The fonts the art has, as lines of an art list
    /// (`crate::preload::font_line`).
    pub fn lines(&self) -> String {
        self.described.iter().map(|(style, color, backdrop)| crate::preload::font_line(style, *color, *backdrop) + "\n").collect()
    }

    /// Whether the font of `style` in `color` (on `backdrop`) is loaded.
    pub fn has_style(&self, style: &FontStyle, color: [u8; 3], backdrop: Option<[u8; 3]>) -> bool {
        let style = FontStyle { height: civ3_assets::fonts::available_height(style), ..style.clone() };
        self.fonts.get(&(style.clone(), color, backdrop)).is_some_and(Option::is_some) || self.described.iter().any(|(s, c, b)| *s == style && *c == color && *b == backdrop)
    }

    /// Whether a font is loaded that the art does not have yet.
    pub fn has_new(&self) -> bool {
        self.fonts.iter().any(|(k, f)| f.is_some() && !self.described.contains(k))
    }

    /// The description entries of the fonts loaded that the art does not
    /// have yet, as an addition to it (see [`Fonts::description`]); from now
    /// on the art has them.
    pub fn addition(&mut self) -> String {
        let new: BTreeMap<FontKey, Option<GlyphFont>> =
            self.fonts.iter().filter(|(k, f)| f.is_some() && !self.described.contains(*k)).map(|(k, f)| (k.clone(), f.clone())).collect();
        self.described.extend(new.keys().cloned());
        Self::entries(&new, false)
    }

    /// Glyph pixels (RGBA bytes) of the fonts the art does not have yet.
    pub fn new_bytes(&self) -> u64 {
        self.fonts
            .iter()
            .filter(|(k, _)| !self.described.contains(*k))
            .filter_map(|(_, f)| f.as_ref())
            .map(|f| f.glyphs.values().map(|g| g.image.width as u64 * g.image.height as u64 * 4).sum::<u64>())
            .sum()
    }

    /// Drops the fonts not asked for in the last `age` (screens come and
    /// go; every font is a sheet of glyphs in the art); how many.
    pub fn prune(&mut self, age: std::time::Duration) -> usize {
        let now = std::time::Instant::now();
        let before = self.fonts.len();
        let used = &self.used;
        self.fonts.retain(|k, _| used.get(k).is_some_and(|t| now.duration_since(*t) < age));
        let fonts = &self.fonts;
        self.used.retain(|k, _| fonts.contains_key(k));
        before - self.fonts.len()
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

    /// How wide `run` is (its glyph advances) and how far its glyphs reach
    /// above and below the baseline, once its font is loaded.
    pub fn metrics(&self, run: &TextRun) -> Option<(i32, i32, i32)> {
        let Some(Some(font)) = self.fonts.get(&key(run)) else { return None };
        let width = run.text.chars().filter_map(|c| font.glyphs.get(&c)).map(|g| g.advance).sum();
        let cell = font.glyphs.values().map(|g| g.image.height as i32).max().unwrap_or(font.ascent);
        Some((width, font.ascent, (cell - font.ascent).max(0)))
    }

    /// The name of `key`'s glyph sets: the font itself, not its place among
    /// the fonts (which moves as fonts are added, before the art is rebuilt).
    fn tag((style, [r, g, b], backdrop): &FontKey) -> String {
        let on = backdrop.map_or(String::new(), |[r, g, b]| format!("~{r:02x}{g:02x}{b:02x}"));
        format!("{}#{r:02x}{g:02x}{b:02x}{on}", civ3_assets::fonts::cache_stem(style))
    }

    /// Render description entries: glyph sprites `Text@<font>/<code>`
    /// anchored at the pen on the baseline.
    pub fn description(&mut self) -> String {
        self.described.extend(self.fonts.iter().filter(|(_, f)| f.is_some()).map(|(k, _)| k.clone()));
        Self::entries(&self.fonts, true)
    }

    /// The entries of `fonts`, with the faces drawn from outlines
    /// (`outlines`) or without.
    fn entries(fonts: &BTreeMap<FontKey, Option<GlyphFont>>, outlines: bool) -> String {
        let mut out = String::from("\n# Text glyphs (generated).\n");
        // The faces text is drawn with from outlines at other zooms.
        for (face, file) in OUTLINE_FACES.iter().filter(|_| outlines) {
            let _ = writeln!(out, "[fonts.\"{face}\"]\nsource = \"{file}\"\n");
        }
        for (n, ((style, [r, g, b], backdrop), font)) in fonts.iter().enumerate() {
            let Some(font) = font else { continue };
            let tag = Self::tag(&(style.clone(), [*r, *g, *b], *backdrop));
            let FontStyle { face, height, weight, italic } = style;
            let on = backdrop.map_or(String::new(), |[r, g, b]| format!("/on={r},{g},{b}"));
            let _ = writeln!(out, "[images.text{n}]\nsource = \"font:{face}/{height}/{r},{g},{b}/{weight}/{}{on}\"\n", *italic as u8);
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

    /// Glyph sprites of one run (1:1, from the glyph sheets), clipped to
    /// `clip` (screen rectangle), drawn after render pass `after_pass`.
    pub fn run_sprites(
        &self,
        r: &TextRun,
        clip: Option<[i32; 4]>,
        compiled: &Compiled,
        after_pass: u32,
        out: &mut Vec<scene::FreeSprite>,
    ) {
        let key = key(r);
        let Some(Some(font)) = self.fonts.get(&key) else { return };
        let tag = Self::tag(&key);
        let mut pen = r.x;
        for c in r.text.chars() {
            let Some(glyph) = font.glyphs.get(&c) else { continue };
            // Glyphs hold coverage per colour channel (ClearType); the
            // underline is a plain bar.
            // Over a backdrop the glyphs are the finished pixels.
            let glyph_blend = if r.backdrop.is_some() { tile_render::Blend::Alpha } else { tile_render::Blend::Subpixel { color: r.color } };
            let line = r.underline.then(|| (format!("TextLine@{tag}/{}", glyph.advance), tile_render::Blend::Alpha));
            for (name, blend) in [Some((format!("Text@{tag}/{}", c as u32), glyph_blend)), line].into_iter().flatten() {
                let Some((set, _)) = compiled.set(&name) else { continue };
                out.push(scene::FreeSprite {
                    set: set.raw(),
                    x: pen,
                    y: r.y,
                    screen: true,
                    after_pass,
                    ticks_per_frame: 1,
                    scale_den: blend.scale_den(),
                    tint: blend.tint(),
                    clip,
                    ..Default::default()
                });
            }
            pen += glyph.advance;
        }
    }

    /// The glyphs of one run drawn from its face's outlines at every zoom
    /// (zoomed art, `scene::FreeSprite::glyph`), each where its 1:1 glyph
    /// is and centred in its 1:1 advance, so the run keeps its 1:1 layout;
    /// clipped and drawn after a pass as [`Fonts::run_sprites`]. False (and
    /// nothing) if `compiled` has no outlines of the run's face or the run's
    /// 1:1 font is not loaded.
    pub fn run_glyphs(
        &self,
        r: &TextRun,
        clip: Option<[i32; 4]>,
        compiled: &Compiled,
        after_pass: u32,
        out: &mut Vec<scene::FreeSprite>,
    ) -> bool {
        let (Some(Some(font)), Some((face, _))) = (self.fonts.get(&key(r)), compiled.font(&r.face)) else { return false };
        let rgb = |[r, g, b]: [u8; 3]| (r as u32) << 16 | (g as u32) << 8 | b as u32;
        // Windows makes bold of a regular face from weight 600 on.
        let style = scene::Glyph {
            font: face,
            ch: ' ',
            em64: r.height.unsigned_abs() * 64,
            advance: 0,
            bold: r.weight >= 600,
            italic: r.italic,
            key: r.backdrop,
            bar: None,
        };
        let mut pen = r.x;
        for c in r.text.chars() {
            let Some(glyph) = font.glyphs.get(&c) else { continue };
            let g = scene::Glyph { ch: c, advance: glyph.advance, ..style };
            let bar = r.underline.then_some(scene::Glyph { bar: Some(font.underline), key: None, ..g });
            for (g, color) in [Some((g, r.color)), bar.map(|b| (b, r.color))].into_iter().flatten() {
                out.push(scene::FreeSprite {
                    x: pen,
                    y: r.y,
                    screen: true,
                    after_pass,
                    ticks_per_frame: 1,
                    tint: rgb(color),
                    zoom_art: 2,
                    clip,
                    glyph: Some(g),
                    ..Default::default()
                });
            }
            pen += glyph.advance;
        }
        true
    }
}

/// The font files the game's faces are drawn with from outlines (the
/// game installs Lucida Sans from its folder; bold and italic it gets from
/// Windows, made of the regular face).
pub const OUTLINE_FACES: [(&str, &str); 1] = [("Lucida Sans", "LSANS.TTF")];

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

#[cfg(test)]
mod prune_tests {
    use super::*;

    #[test]
    fn fonts_not_asked_for_are_pruned() {
        let mut fonts = Fonts::default();
        let style = FontStyle { face: "Lucida Sans".into(), height: 12, weight: 400, italic: false };
        let (old, new) = ((style.clone(), [1, 2, 3], None), (style, [4, 5, 6], None));
        fonts.fonts.insert(old.clone(), None);
        fonts.fonts.insert(new.clone(), None);
        fonts.used.insert(old.clone(), std::time::Instant::now() - std::time::Duration::from_secs(120));
        fonts.used.insert(new.clone(), std::time::Instant::now());
        assert_eq!(fonts.prune(std::time::Duration::from_secs(60)), 1);
        assert!(fonts.fonts.contains_key(&new) && !fonts.fonts.contains_key(&old));
    }

    /// Outline glyphs go where the 1:1 glyphs go, with their advances; the
    /// underline as bars; nothing without the face's outlines.
    #[test]
    fn outline_glyphs_follow_the_one_to_one_pens() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        if !std::path::Path::new(&install).join("LSANS.TTF").is_file() {
            return eprintln!("no Civ III installation, skipped");
        }
        let mut fonts = Fonts::default();
        let run = TextRun { x: 10, y: 40, color: [1, 2, 3], face: "Lucida Sans".into(), height: -12, weight: 700, italic: false, underline: true, text: "Ab".into(), backdrop: None };
        let glyph = |advance| civ3_assets::fonts::Glyph { left: 0, advance, extent: advance, image: tile_render::RgbaImage::new(1, 1) };
        let font = GlyphFont { ascent: 12, underline: (1, 1), glyphs: [('A', glyph(8)), ('b', glyph(7))].into_iter().collect() };
        fonts.fonts.insert(key(&run), Some(font));
        let loader = civ3_assets::loader::Civ3Loader::new(civ3_assets::GameFiles::complete(&install));
        let desc = |fonts: &str| render_desc::Description::parse(&format!("[grid]\nlayout = \"staggered\"\nstep = [64, 32]\n{fonts}")).unwrap().compile_layout(&loader, 256).unwrap();
        assert!(fonts.description().contains("[fonts.\"Lucida Sans\"]\nsource = \"LSANS.TTF\""));
        let compiled = desc("[fonts.\"Lucida Sans\"]\nsource = \"LSANS.TTF\"\n");
        let mut out = Vec::new();
        assert!(fonts.run_glyphs(&run, Some([0, 0, 100, 100]), &compiled, 3, &mut out));
        let at: Vec<(i32, Option<char>, Option<(i32, i32)>)> = out.iter().map(|s| (s.x, s.glyph.map(|g| g.ch), s.glyph.and_then(|g| g.bar))).collect();
        assert_eq!(at, [(10, Some('A'), None), (10, Some('A'), Some((1, 1))), (18, Some('b'), None), (18, Some('b'), Some((1, 1)))]);
        let g = out[2].glyph.unwrap();
        assert_eq!((g.em64, g.advance, g.bold, out[2].tint, out[2].zoom_art, out[2].after_pass), (12 * 64, 7, true, 0x010203, 2, 3));
        assert!(!fonts.run_glyphs(&run, None, &desc(""), 3, &mut out));
    }
}
