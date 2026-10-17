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

/// The font `r` is drawn with: its style at the nearest size there are
/// glyphs for (zoomed text asks for sizes the game never uses, see
/// [`zoomed`] and `civ3_assets::fonts::available_height`).
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

    /// Drops the fonts not asked for in the last `age` (zooms and screens
    /// come and go; every font is a sheet of glyphs in the art); how many.
    pub fn prune(&mut self, age: std::time::Duration) -> usize {
        let now = std::time::Instant::now();
        let before = self.fonts.len();
        let used = &self.used;
        self.fonts.retain(|k, _| used.get(k).is_some_and(|t| now.duration_since(*t) < age));
        let fonts = &self.fonts;
        self.used.retain(|k, _| fonts.contains_key(k));
        before - self.fonts.len()
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
    fn tag((style, [r, g, b], backdrop): &FontKey) -> String {
        let on = backdrop.map_or(String::new(), |[r, g, b]| format!("~{r:02x}{g:02x}{b:02x}"));
        format!("{}#{r:02x}{g:02x}{b:02x}{on}", civ3_assets::fonts::cache_stem(style))
    }

    /// Render description entries: glyph sprites `Text@<font>/<code>`
    /// anchored at the pen on the baseline.
    pub fn description(&self) -> String {
        let mut out = String::from("\n# Text glyphs (generated).\n");
        for (n, ((style, [r, g, b], backdrop), font)) in self.fonts.iter().enumerate() {
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
            // Over a backdrop the glyphs are the finished pixels.
            let glyph_blend = if r.backdrop.is_some() { tile_render::Blend::Alpha } else { tile_render::Blend::Subpixel { color: r.color } };
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

    /// The heights `tools/gen-civ3-fonts` makes by default.
    fn generated_heights() -> Vec<i32> {
        let script = include_str!("../../../tools/gen-civ3-fonts");
        let line = script.lines().find_map(|l| l.strip_prefix("default_sizes=\"")).expect("default_sizes line");
        let (seq, rest) = line.strip_prefix("$(seq -s ' ' ").and_then(|l| l.split_once(')')).expect("$(seq ...) first");
        let (from, to) = seq.split_once(' ').expect("seq FROM TO");
        let mut out: Vec<i32> = (from.parse().unwrap()..=to.parse().unwrap()).collect();
        out.extend(rest.trim_end_matches('"').split_whitespace().map(|v| v.parse::<i32>().unwrap()));
        out
    }

    /// Every size the viewer's zooms make of the game's text sizes has its
    /// glyph sheet (up to the largest made, which larger ones are capped to).
    #[test]
    fn the_generated_sheets_cover_every_zoomed_size() {
        use civ3_assets::fonts::{MAX_HEIGHT, MIN_HEIGHT};
        let have = generated_heights();
        assert_eq!((have.iter().min(), have.iter().max()), (Some(&MIN_HEIGHT), Some(&MAX_HEIGHT)));
        let run = |height| TextRun { x: 0, y: 0, color: [0; 3], face: "Lucida Sans".into(), height, weight: 0, italic: false, underline: false, text: String::new(), backdrop: None };
        for base in [8, 10, 12, 14, 16, 18, 24] {
            for &zoom in &tile_viewer::layout::ZOOMS {
                let h = zoomed(&run(-base), zoom).height.abs().clamp(MIN_HEIGHT, MAX_HEIGHT);
                assert!(have.contains(&h), "{base} px at {zoom:?}: {h} px has no sheet");
            }
        }
    }
}

/// Run `r` as the game would draw it at `zoom` (`(num, den)`): its font
/// that many times the size (rounded), the rest as it is.
pub fn zoomed(r: &TextRun, (n, d): (u32, u32)) -> TextRun {
    let height = ((r.height.abs() as i64 * n as i64 + d as i64 / 2) / d.max(1) as i64) as i32;
    TextRun { height: height * r.height.signum(), ..r.clone() }
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
}
