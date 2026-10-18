//! The game's text (labels, status lines), as it draws it through
//! `jgl.dll`'s `TextOutA`: runs of characters at a baseline position in a
//! font and colour. Drawn 1:1 with the same GDI-rasterised glyphs
//! (`civ3_assets::fonts`), as screen-space sprites on top of everything,
//! and at other zooms from the face's outlines (see [`Fonts::run_glyphs`]).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::cell::RefCell;
use std::fmt::Write as _;

use civ3_assets::GameFiles;
use civ3_assets::fonts::{FontStyle, Glyph, GlyphFont, charset, outline_char};
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

/// Older sheets rasterised raw C1 controls through TextOutW instead of
/// their Windows-1252 characters. Their empty glyphs cannot draw the game
/// byte. Keep their keys, but use supported outlines for the entire run.
fn legacy_c1(c: char, glyph: &Glyph) -> bool {
    outline_char(c) != c && glyph.advance <= 0 && glyph.image.pixels.iter().all(|pixel| pixel[3] == 0)
}

fn legacy_run(run: &TextRun, font: &GlyphFont) -> bool {
    run.text.chars().any(|c| font.glyphs.get(&c).is_some_and(|glyph| legacy_c1(c, glyph)))
}

/// Only a legacy empty glyph needs a replacement advance. Adjacent glyphs
/// keep their measured Windows advances; this one uses its source face.
fn legacy_advance(c: char, face: &tile_render::gpu_text::Face, height: i32) -> Option<i32> {
    let units = i64::from(face.advance(outline_char(c))?);
    let em = i64::from(face.units_per_em());
    if em == 0 { return None; }
    i32::try_from((units * i64::from(height) + em / 2) / em).ok().filter(|&advance| advance > 0)
}

/// The fonts (face, size, colour) text runs need, loaded.
#[derive(Default)]
pub struct Fonts {
    fonts: BTreeMap<FontKey, Option<GlyphFont>>,
    /// Changes whenever loaded font metrics change, including a reload
    /// whose glyphs are already in the atlas.
    revision: u64,
    /// Cached glyph ink extents, invalidated when metrics are replaced.
    ink: RefCell<(u64, BTreeMap<(FontKey, char), Option<[i32; 4]>>)>,
    /// Fonts (face, size, style) that failed to load: reported once, not per
    /// colour.
    failed: BTreeSet<FontStyle>,
    /// When each font was last asked for (see [`Fonts::prune`]).
    used: BTreeMap<FontKey, std::time::Instant>,
    /// The fonts the art has (see [`Fonts::addition`]); pruning does not
    /// take them out of it.
    described: BTreeSet<FontKey>,
    /// Loading on a thread of its own (see [`Fonts::in_background`]): the
    /// requests, the fonts asked for and not loaded yet, the loaded ones.
    loader: Option<Loader>,
}

/// A thread loading fonts (rasterising their glyphs: CPU work no one
/// waits for).
struct Loader {
    requests: std::sync::mpsc::Sender<(Vec<std::path::PathBuf>, FontKey)>,
    loaded: std::sync::mpsc::Receiver<(FontKey, Result<GlyphFont, String>)>,
    pending: BTreeSet<FontKey>,
}

impl Fonts {
    /// Fonts loaded on a thread of their own: [`Fonts::add`] asks for them,
    /// [`Fonts::take_loaded`] takes them in; text whose font is not there
    /// yet is drawn from its face's outlines meanwhile (see
    /// [`Fonts::run_sprites`]).
    pub fn in_background() -> std::io::Result<Fonts> {
        let (requests, rx) = std::sync::mpsc::channel::<(Vec<std::path::PathBuf>, FontKey)>();
        let (tx, loaded) = std::sync::mpsc::channel();
        std::thread::Builder::new().name("civ3r-fonts".into()).spawn(move || {
            for (roots, key) in rx {
                let font = civ3_assets::fonts::load(&GameFiles::with_roots(roots), &key.0, key.1);
                if tx.send((key, font)).is_err() {
                    break;
                }
            }
        })?;
        Ok(Fonts { loader: Some(Loader { requests, loaded, pending: BTreeSet::new() }), ..Fonts::default() })
    }

    /// Loads the fonts `runs` use that are not loaded yet (in the
    /// background: asks for them); true if any was loaded now.
    pub fn add(&mut self, files: &GameFiles, runs: &[TextRun]) -> bool {
        let mut added = false;
        for r in runs {
            let key = key(r);
            self.used.insert(key.clone(), std::time::Instant::now());
            if self.fonts.contains_key(&key) {
                continue;
            }
            if let Some(l) = &mut self.loader {
                if l.pending.insert(key.clone()) && l.requests.send((files.roots().to_vec(), key.clone())).is_err() {
                    l.pending.remove(&key);
                }
                continue;
            }
            let font = civ3_assets::fonts::load(files, &key.0, key.1);
            self.insert(key, font);
            added = true;
        }
        added
    }

    /// Takes in the fonts loaded in the background since; true if any.
    pub fn take_loaded(&mut self) -> bool {
        let Some(l) = &mut self.loader else { return false };
        let loaded: Vec<_> = l.loaded.try_iter().collect();
        for (key, _) in &loaded {
            l.pending.remove(key);
        }
        let any = !loaded.is_empty();
        for (key, font) in loaded {
            self.used.entry(key.clone()).or_insert_with(std::time::Instant::now);
            self.insert(key, font);
        }
        any
    }

    fn insert(&mut self, key: FontKey, font: Result<GlyphFont, String>) {
        let font = font
            .inspect_err(|e| {
                if self.failed.insert(key.0.clone()) {
                    log::warn!("font {:?}: {e}", key.0);
                }
            })
            .ok();
        self.fonts.insert(key, font);
        self.revision += 1;
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
        let dropped = before - self.fonts.len();
        if dropped > 0 { self.revision += 1; }
        dropped
    }

    /// Revision of the loaded font metrics, for cached text sprites.
    pub fn revision(&self) -> u64 { self.revision }

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

    /// Whether this run still has an outstanding background metrics load.
    /// A failed font is terminal and is not reported as loading.
    pub fn run_loading(&self, run: &TextRun) -> bool {
        self.loader.as_ref().is_some_and(|loader| loader.pending.contains(&key(run)))
    }

    /// Whether every visible character can be drawn with the current art.
    /// Follows `run_sprites`: the first mapped glyph or an empty legacy
    /// C1 glyph selects the atlas path or whole-run outline fallback. Fonts with
    /// no usable draw path return false; `run_loading` distinguishes a
    /// metrics load that can still complete from a terminal failure.
    pub fn run_ready(&self, run: &TextRun, compiled: &Compiled) -> bool {
        if run.text.is_empty() || (!run.underline && run.text.chars().all(|c| outline_char(c).is_whitespace())) { return true; }
        let want = key(run);
        let exact = self.fonts.get(&want).and_then(Option::as_ref);
        let tag = Self::tag(&want);
        let first = exact.and_then(|font| run.text.chars().find(|c| font.glyphs.contains_key(c)));
        let atlas = exact.is_some_and(|font| !legacy_run(run, font)) && first.is_none_or(|c| compiled.set(&format!("Text@{tag}/{}", c as u32)).is_some());
        let present = |name: &str| compiled.set_available(name);
        if atlas {
            let font = exact.unwrap();
            let face = compiled.font(&run.face).map(|(_, face)| face);
            return run.text.chars().all(|c| {
                let Some(glyph) = font.glyphs.get(&c) else { return outline_char(c).is_whitespace() && !run.underline };
                // A loaded outline face makes ui_screen draw this path at
                // scaled zooms too. Missing cmap entries would draw glyph 0.
                let visible = !outline_char(c).is_whitespace();
                (!visible || present(&format!("Text@{tag}/{}", c as u32)))
                    && (!visible || face.is_none_or(|f| f.glyph(outline_char(c)).is_some()))
                    && (!run.underline || glyph.advance <= 0 || font.underline.1 <= 0 || present(&format!("TextLine@{tag}/{}", glyph.advance)))
            });
        }
        let Some((_, face)) = compiled.font(&run.face) else { return false };
        let metrics = exact.or_else(|| {
            self.fonts.iter().filter(|((style, _, _), font)| style.face == run.face && font.is_some())
                .min_by_key(|((style, _, _), _)| (style.height - want.0.height).abs()).and_then(|(_, font)| font.as_ref())
        });
        let Some(font) = metrics else { return false };
        run.text.chars().all(|c| {
            let Some(glyph) = font.glyphs.get(&c) else { return outline_char(c).is_whitespace() && !run.underline };
            (outline_char(c).is_whitespace() || face.glyph(outline_char(c)).is_some())
                && (!legacy_c1(c, glyph) || legacy_advance(c, face, want.0.height).is_some())
        })
    }

    /// How wide `run` is (its glyph advances) and how far its glyphs reach
    /// above and below the baseline, once its font is loaded.
    pub fn metrics(&self, run: &TextRun) -> Option<(i32, i32, i32)> {
        let Some(Some(font)) = self.fonts.get(&key(run)) else { return None };
        let width = run.text.chars().filter_map(|c| font.glyphs.get(&c)).map(|g| g.advance).sum();
        let cell = font.glyphs.values().map(|g| g.image.height as i32).max().unwrap_or(font.ascent);
        Some((width, font.ascent, (cell - font.ascent).max(0)))
    }

    /// Bounds of the visible characters at the game's measured glyph
    /// positions. Trailing spaces and glyph cell padding do not extend a
    /// text-only Alt target's preferred pointer position.
    pub fn ink_bounds(&self, run: &TextRun) -> Option<[i32; 4]> {
        let want = key(run);
        let (font, height) = self.fonts.get(&want).and_then(Option::as_ref).map(|f| (f, want.0.height)).or_else(|| {
            self.fonts.iter().filter(|((s, _, _), f)| s.face == run.face && f.is_some())
                .min_by_key(|((s, _, _), _)| (s.height - want.0.height).abs())
                .and_then(|((s, _, _), f)| f.as_ref().map(|f| (f, s.height)))
        })?;
        let scale = |v: i32| v * want.0.height / height.max(1);
        let mut cache = self.ink.borrow_mut();
        if cache.0 != self.revision { *cache = (self.revision, BTreeMap::new()); }
        let mut pen = run.x;
        let mut bounds: Option<[i32; 4]> = None;
        for c in run.text.chars() {
            let Some(glyph) = font.glyphs.get(&c) else { continue };
            if !c.is_whitespace() {
                let ink = *cache.1.entry((want.clone(), c)).or_insert_with(|| {
                    let mut ink: Option<[i32; 4]> = None;
                    for (k, _) in glyph.image.pixels.iter().enumerate().filter(|(_, p)| p[3] != 0) {
                        let x = k as i32 % glyph.image.width as i32;
                        let y = k as i32 / glyph.image.width as i32;
                        ink = Some(match ink {
                            Some([l,t,r,b]) => [l.min(x),t.min(y),r.max(x+1),b.max(y+1)],
                            None => [x,y,x+1,y+1],
                        });
                    }
                    ink
                });
                if let Some([x, y, right, bottom]) = ink {
                    let (l, t) = (pen + scale(glyph.left + x), run.y + scale(y - font.ascent));
                    let (r, b) = (pen + scale(glyph.left + right).max(scale(glyph.left + x) + 1),
                        run.y + scale(bottom - font.ascent).max(scale(y - font.ascent) + 1));
                    bounds = Some(match bounds {
                        Some([bl, bt, br, bb]) => [bl.min(l), bt.min(t), br.max(r), bb.max(b)],
                        None => [l, t, r, b],
                    });
                }
            }
            pen += scale(glyph.advance);
        }
        bounds.map(|[l, t, r, b]| [l, t, r - l, b - t])
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
        let Some(Some(font)) = self.fonts.get(&key) else {
            // Not loaded yet: from the outlines meanwhile.
            self.outline_fallback(r, clip, compiled, after_pass, out);
            return;
        };
        if legacy_run(r, font) {
            self.outline_fallback(r, clip, compiled, after_pass, out);
            return;
        }
        let tag = Self::tag(&key);
        // Its glyphs not in the art yet: from the outlines meanwhile.
        let first = r.text.chars().find(|c| font.glyphs.contains_key(c));
        if first.is_some_and(|c| compiled.set(&format!("Text@{tag}/{}", c as u32)).is_none()) {
            self.outline_fallback(r, clip, compiled, after_pass, out);
            return;
        }
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
    /// Run `r` drawn from its face's outlines while its own glyphs are not
    /// in the art (its font loading, or being added): with the advances of
    /// its font, or of the loaded font of the face nearest its height
    /// (scaled); nothing if the art has no outlines of the face.
    fn outline_fallback(&self, r: &TextRun, clip: Option<[i32; 4]>, compiled: &Compiled, after_pass: u32, out: &mut Vec<scene::FreeSprite>) {
        let Some((face, outline)) = compiled.font(&r.face) else { return };
        let want = key(r);
        let metrics = self.fonts.get(&want).and_then(Option::as_ref).map(|f| (f, want.0.height)).or_else(|| {
            self.fonts
                .iter()
                .filter(|((s, _, _), f)| s.face == r.face && f.is_some())
                .min_by_key(|((s, _, _), _)| (s.height - want.0.height).abs())
                .and_then(|((s, _, _), f)| f.as_ref().map(|f| (f, s.height)))
        });
        let Some((font, height)) = metrics else { return };
        let scale = |v: i32| v * want.0.height / height.max(1);
        let rgb = |[r, g, b]: [u8; 3]| (r as u32) << 16 | (g as u32) << 8 | b as u32;
        let mut pen = r.x;
        for c in r.text.chars() {
            let Some(glyph) = font.glyphs.get(&c) else { continue };
            let advance = if legacy_c1(c, glyph) { legacy_advance(c, outline, want.0.height).unwrap_or(0) } else { scale(glyph.advance) };
            let g = scene::Glyph {
                font: face,
                ch: outline_char(c),
                em64: r.height.unsigned_abs() * 64,
                advance,
                bold: r.weight >= 600,
                italic: r.italic,
                key: r.backdrop,
                bar: None,
            };
            out.push(scene::FreeSprite { x: pen, y: r.y, screen: true, after_pass, ticks_per_frame: 1, tint: rgb(r.color), clip, glyph: Some(g), ..Default::default() });
            if r.underline {
                let thickness = if font.underline.1 > 0 { scale(font.underline.1).max(1) } else { scale(font.underline.1) };
                let bar = scene::Glyph { bar: Some((scale(font.underline.0), thickness)), key: None, ..g };
                out.push(scene::FreeSprite { x: pen, y: r.y, screen: true, after_pass, ticks_per_frame: 1, tint: rgb(r.color), clip, glyph: Some(bar), ..Default::default() });
            }
            pen += advance;
        }
    }

    pub fn run_glyphs(
        &self,
        r: &TextRun,
        clip: Option<[i32; 4]>,
        compiled: &Compiled,
        after_pass: u32,
        out: &mut Vec<scene::FreeSprite>,
    ) -> bool {
        let want = key(r);
        let (Some(Some(font)), Some((face, outline))) = (self.fonts.get(&want), compiled.font(&r.face)) else { return false };
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
            let advance = if legacy_c1(c, glyph) { legacy_advance(c, outline, want.0.height).unwrap_or(0) } else { glyph.advance };
            let g = scene::Glyph { ch: outline_char(c), advance, ..style };
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
            pen += advance;
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

    #[test]
    fn reloading_and_pruning_change_the_revision_even_when_font_count_and_atlas_do_not() {
        let style = FontStyle { face: "Lucida Sans".into(), height: 12, weight: 400, italic: false };
        let old = (style.clone(), [1, 2, 3], None);
        let reloaded = (style.clone(), [4, 5, 6], None);
        let font = || GlyphFont { ascent: 12, underline: (1, 1), glyphs: Default::default() };
        let mut fonts = Fonts::default();
        fonts.insert(old.clone(), Ok(font()));
        fonts.described.extend([old.clone(), reloaded.clone()]);
        fonts.used.insert(old, std::time::Instant::now() - std::time::Duration::from_secs(120));
        fonts.used.insert(reloaded.clone(), std::time::Instant::now());
        let (requests, _requests_rx) = std::sync::mpsc::channel();
        let (completed, loaded) = std::sync::mpsc::channel();
        fonts.loader = Some(Loader { requests, loaded, pending: [reloaded.clone()].into_iter().collect() });
        let (count, revision) = (fonts.len(), fonts.revision());
        completed.send((reloaded.clone(), Ok(font()))).unwrap();
        assert!(fonts.take_loaded());
        assert_eq!(fonts.prune(std::time::Duration::from_secs(60)), 1);
        assert_eq!(fonts.len(), count);
        assert!(!fonts.has_new(), "the atlas already contains the reloaded font");
        assert!(fonts.fonts.get(&reloaded).is_some_and(Option::is_some));
        assert!(fonts.revision() > revision, "cached text must be rebuilt despite the unchanged count and atlas");
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

#[cfg(test)]
mod readiness_tests {
    use super::*;

    struct Plain;
    impl render_desc::ImageLoader for Plain {
        fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
            Err("readiness fixtures use generated pixels".into())
        }
    }

    fn text_run(text: &str) -> TextRun {
        TextRun { x: 10, y: 20, color: [1, 2, 3], face: "Readiness fixture".into(), height: -12, weight: 400, italic: false, underline: false, text: text.into(), backdrop: None }
    }

    fn font(chars: &str) -> GlyphFont {
        let glyphs = chars.chars().map(|c| {
            let image = tile_render::RgbaImage::from_fn(1, 1, |_, _| if c.is_whitespace() { [0; 4] } else { [255; 4] });
            (c, civ3_assets::fonts::Glyph { left: 0, advance: 6, extent: 6, image })
        }).collect();
        GlyphFont { ascent: 10, underline: (1, 1), glyphs }
    }

    fn fixture_art(names: &[String], outlines: &str) -> Compiled {
        let mut description = String::from("grid = { layout = \"orthogonal\", step = [1, 1] }\n[images.pixel]\nsource = \"solid:255,255,255,255:1x1\"\n[sheets.pixel]\nimage = \"pixel\"\ncell = [1, 1]\ngrid = [1, 1]\ngroup = \"pixel\"\n");
        for name in names {
            writeln!(description, "[sets.\"{name}\"]\nframes = [\"pixel/0\"]\n").unwrap();
        }
        let mut compiled = render_desc::Description::parse(&description).unwrap().compile(&Plain, 64).unwrap();
        if !outlines.is_empty() {
            let curves = vec![[(0.0, 0.0), (0.5, 0.0), (1.0, 0.0)], [(1.0, 0.0), (1.0, 0.5), (1.0, 1.0)], [(1.0, 1.0), (0.5, 0.5), (0.0, 0.0)]];
            let glyphs: Vec<_> = outlines.chars().map(|c| (c, curves.clone(), 6)).collect();
            let face = tile_render::gpu_text::Face::from_curves(12, &glyphs);
            compiled.fonts.push(("Readiness fixture".into(), std::sync::Arc::new(face)));
        }
        compiled
    }

    fn glyph_names(run: &TextRun) -> Vec<String> {
        let tag = Fonts::tag(&key(run));
        run.text.chars().map(|c| format!("Text@{tag}/{}", c as u32)).collect()
    }

    #[test]
    fn windows_1252_atlas_keys_and_unicode_outlines_keep_the_same_pens() {
        let run = text_run("A\u{92}\u{85}\u{80}éB");
        let mut fonts = Fonts::default();
        fonts.fonts.insert(key(&run), Some(font(&run.text)));
        let names = glyph_names(&run);
        let compiled = fixture_art(&names, "A’…€éB");
        assert!(fonts.run_ready(&run, &compiled));
        let mut raster = Vec::new();
        fonts.run_sprites(&run, None, &compiled, u32::MAX, &mut raster);
        assert_eq!(raster.len(), run.text.chars().count());
        assert!(raster.iter().all(|sprite| sprite.glyph.is_none()));
        assert_eq!(raster[1].set, compiled.set(&format!("Text@{}/146", Fonts::tag(&key(&run)))).unwrap().0.raw(), "atlas lookup retains the Windows byte");
        let mut outline = Vec::new();
        assert!(fonts.run_glyphs(&run, None, &compiled, u32::MAX, &mut outline));
        let unicode: String = outline.iter().map(|sprite| sprite.glyph.unwrap().ch).collect();
        assert_eq!(unicode, "A’…€éB");
        assert_eq!(raster.iter().map(|sprite| sprite.x).collect::<Vec<_>>(), outline.iter().map(|sprite| sprite.x).collect::<Vec<_>>());
        assert!(!fonts.run_ready(&run, &fixture_art(&names[..names.len() - 1], "A’…€éB")), "an omitted later atlas glyph still blocks a partial run");
        assert!(!fonts.run_ready(&run, &fixture_art(&names, "A…€éB")), "a missing mapped source outline is not accepted");
        let ellipsis = text_run("\u{85}");
        assert!(!Fonts::default().run_ready(&ellipsis, &fixture_art(&[], "")), "raw U+0085 is a visible Windows ellipsis, not whitespace");
    }

    #[test]
    fn negative_height_nearest_metrics_fallback_preserves_scaled_gdi_pens() {
        let mut run = text_run("A\u{92}B");
        run.underline = true;
        let old = TextRun { height: -6, color: [9, 8, 7], ..run.clone() };
        let mut metrics = font(&run.text);
        metrics.glyphs.get_mut(&'A').unwrap().advance = 5;
        metrics.glyphs.get_mut(&'B').unwrap().advance = 7;
        let apostrophe = metrics.glyphs.get_mut(&'\u{92}').unwrap();
        apostrophe.advance = 0;
        apostrophe.image = tile_render::RgbaImage::new(1, 1);
        let mut fonts = Fonts::default();
        fonts.fonts.insert(key(&old), Some(metrics));
        let compiled = fixture_art(&[], "A’B");
        assert!(fonts.run_ready(&run, &compiled));
        let mut fallback = Vec::new();
        fonts.run_sprites(&run, None, &compiled, u32::MAX, &mut fallback);
        let letters: Vec<_> = fallback.iter().filter(|sprite| sprite.glyph.unwrap().bar.is_none()).map(|sprite| (sprite.x, sprite.glyph.unwrap().ch, sprite.glyph.unwrap().advance)).collect();
        assert_eq!(letters, [(10, 'A', 10), (20, '’', 6), (26, 'B', 14)], "negative LogFont heights normalize before scaling the nearest metrics");
        let bars: Vec<_> = fallback.iter().filter_map(|sprite| sprite.glyph.unwrap().bar).collect();
        assert_eq!(bars, [(2, 2); 3]);
        assert!(!fonts.run_ready(&run, &fixture_art(&[], "AB")), "legacy empty glyphs require their actual source outlines");
        let small = TextRun { height: -10, ..run.clone() };
        let large = TextRun { height: -18, ..old.clone() };
        let metrics = fonts.fonts.remove(&key(&old)).unwrap();
        fonts.fonts.insert(key(&large), metrics);
        assert!(fonts.run_ready(&small, &compiled));
        let mut down = Vec::new();
        fonts.run_sprites(&small, None, &compiled, u32::MAX, &mut down);
        let letters: Vec<_> = down.iter().filter(|sprite| sprite.glyph.unwrap().bar.is_none()).map(|sprite| (sprite.x, sprite.glyph.unwrap().advance)).collect();
        assert_eq!(letters, [(10, 2), (12, 5), (17, 3)], "larger nearest metrics scale down; the C1 advance still comes from the source face at 10px");
        assert!(down.iter().filter_map(|sprite| sprite.glyph.unwrap().bar).all(|bar| bar == (0, 1)), "a positive source underline keeps one visible target pixel when scaled down");
    }

    #[test]
    fn legacy_c1_exact_metrics_use_source_advance_and_do_not_accept_blank_art() {
        let run = text_run("A\u{92}B");
        let mut metrics = font(&run.text);
        metrics.glyphs.get_mut(&'A').unwrap().advance = 5;
        metrics.glyphs.get_mut(&'B').unwrap().advance = 7;
        let apostrophe = metrics.glyphs.get_mut(&'\u{92}').unwrap();
        apostrophe.advance = 0;
        apostrophe.image = tile_render::RgbaImage::new(1, 1);
        let mut fonts = Fonts::default();
        fonts.fonts.insert(key(&run), Some(metrics));
        let names = glyph_names(&run);
        let compiled = fixture_art(&names, "A’B");
        assert!(fonts.run_ready(&run, &compiled));
        let mut fallback = Vec::new();
        fonts.run_sprites(&run, None, &compiled, u32::MAX, &mut fallback);
        let mut zoomed = Vec::new();
        assert!(fonts.run_glyphs(&run, None, &compiled, u32::MAX, &mut zoomed));
        let pens = |sprites: &[scene::FreeSprite]| sprites.iter().map(|sprite| (sprite.x, sprite.glyph.unwrap().ch, sprite.glyph.unwrap().advance)).collect::<Vec<_>>();
        assert_eq!(pens(&fallback), [(10, 'A', 5), (15, '’', 6), (21, 'B', 7)]);
        assert_eq!(pens(&fallback), pens(&zoomed));
        assert!(!fonts.run_ready(&run, &fixture_art(&names, "")), "a named blank atlas glyph cannot substitute for unavailable outlines");
        fonts.fonts.get_mut(&key(&run)).unwrap().as_mut().unwrap().glyphs.remove(&'B');
        assert!(!fonts.run_ready(&run, &compiled), "a complete face cannot replace missing layout metrics");
    }

    #[test]
    fn real_cached_copyright_renders_the_windows_apostrophe_and_keeps_adjacent_metrics() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        let files = GameFiles::complete(&install);
        let run = TextRun { x: 717, y: 884, color: [248; 3], face: "Lucida Sans".into(), height: -10, weight: 0, italic: false, underline: false, text: "Sid Meier\u{92}s Civilization III, 2K Games, the 2K Games logo, and Take-Two Interactive Software are all".into(), backdrop: None };
        let Ok(metrics) = civ3_assets::fonts::load(&files, &key(&run).0, run.color) else { return eprintln!("no real Lucida cache, skipped") };
        if files.read("LSANS.TTF").is_err() { return eprintln!("no LSANS.TTF, skipped"); }
        let legacy = legacy_run(&run, &metrics);
        let mut fonts = Fonts::default();
        fonts.fonts.insert(key(&run), Some(metrics.clone()));
        let description = render_desc::Description::parse(&format!("grid = {{ layout = \"orthogonal\", step = [1, 1] }}\n{}", fonts.description())).unwrap();
        let loader = civ3_assets::loader::Civ3Loader::new(files);
        let compiled = description.compile(&loader, 256).unwrap();
        assert!(compiled.set_available(&format!("Text@{}/146", Fonts::tag(&key(&run)))), "raw Windows byte146 has atlas metadata");
        assert!(fonts.run_ready(&run, &compiled));
        let mut raster = Vec::new();
        fonts.run_sprites(&run, None, &compiled, u32::MAX, &mut raster);
        assert_eq!(raster.len(), 100);
        if legacy { assert!(raster.iter().all(|sprite| sprite.glyph.is_some()), "old empty C1 sheets use outlines for the whole run"); }
        let mut outlines = Vec::new();
        assert!(fonts.run_glyphs(&run, None, &compiled, u32::MAX, &mut outlines));
        assert_eq!(outlines.len(), 100);
        let mut pen = run.x;
        for (c, sprite) in run.text.chars().zip(&outlines) {
            let glyph = sprite.glyph.unwrap();
            assert_eq!((sprite.x, glyph.ch), (pen, outline_char(c)));
            if c == '\u{92}' {
                assert!(glyph.advance > 0, "the source face supplies a real positive apostrophe advance");
            } else { assert_eq!(glyph.advance, metrics.glyphs[&c].advance, "adjacent GDI advances stay unchanged"); }
            pen += glyph.advance;
        }
        let mut apostrophe = outlines[9].clone();
        assert_eq!(apostrophe.glyph.unwrap().ch, '’');
        apostrophe.x = 4;
        apostrophe.y = 18;
        apostrophe.zoom_art = 0;
        let mut world = compiled.world(1, 1, false, false);
        world.free.push(tile_viewer::placed_sprite(&compiled, &apostrophe, None, None));
        let camera = tile_render::Camera { zoomed_art: true, ..tile_render::Camera::new((32, 32)) };
        let picture = tile_render::reference::render(&world, &compiled.atlas, &camera, 0, [0, 0, 0, 255]);
        assert!(picture.pixels.iter().any(|pixel| pixel[..3] != [0, 0, 0]), "the actual source apostrophe has visible rendered coverage");
    }

    #[test]
    fn a_partial_atlas_is_not_a_complete_run() {
        let run = text_run("AB");
        let mut fonts = Fonts::default();
        fonts.fonts.insert(key(&run), Some(font("AB")));
        let names = glyph_names(&run);
        let partial = fixture_art(&names[..1], "AB");
        let mut drawn = Vec::new();
        fonts.run_sprites(&run, None, &partial, u32::MAX, &mut drawn);
        assert_eq!(drawn.len(), 1, "the first atlas glyph prevents fallback of the whole run");
        assert!(!fonts.run_ready(&run, &partial), "one emitted glyph is not proof that the text is complete");
        let complete = fixture_art(&names, "AB");
        assert!(fonts.run_ready(&run, &complete));
        assert!(fonts.run_ready(&run, &complete.without_pixels()));
        assert!(!fonts.run_ready(&run, &fixture_art(&names, "A")), "scaled text must have every visible outline glyph");
        assert!(!fonts.run_ready(&text_run("AC"), &complete), "unsupported characters must not make a run appear complete");
    }

    #[test]
    fn unavailable_glyph_placeholders_do_not_make_text_ready() {
        let run = text_run("A");
        let mut fonts = Fonts::default();
        fonts.fonts.insert(key(&run), Some(font("A")));
        let name = &glyph_names(&run)[0];
        let description = format!("grid = {{ layout = \"orthogonal\", step = [1, 1] }}\n[images.glyph]\nsource = \"solid:255,255,255,255:8x2\"\n[sheets.glyph]\nimage = \"glyph\"\ncell = [8, 2]\ngrid = [1, 1]\n[sets.\"{name}\"]\nframes = [\"glyph/0\"]\n");
        let description = render_desc::Description::parse(&description).unwrap();
        let small = description.compile(&Plain, 4).unwrap();
        assert!(small.set(name).is_some(), "run_sprites selects the atlas path when a named placeholder exists");
        assert!(!fonts.run_ready(&run, &small));
        assert!(!fonts.run_ready(&run, &small.without_pixels()));
        let complete = description.compile(&Plain, 16).unwrap();
        assert!(fonts.run_ready(&run, &complete));
    }

    #[test]
    fn underlines_require_atlas_bars_or_complete_outline_fallback() {
        let mut run = text_run("A");
        run.underline = true;
        let mut fonts = Fonts::default();
        fonts.fonts.insert(key(&run), Some(font("A")));
        let mut names = glyph_names(&run);
        let fallback = fixture_art(&[], "A");
        assert!(fonts.run_ready(&run, &fallback));
        let mut drawn = Vec::new();
        fonts.run_sprites(&run, None, &fallback, u32::MAX, &mut drawn);
        assert_eq!(drawn.len(), 2);
        assert_eq!(drawn[1].glyph.unwrap().bar, Some((1, 1)), "fallback must include the requested visible bar");
        assert!(!fonts.run_ready(&run, &fixture_art(&names, "A")));
        names.push(format!("TextLine@{}/6", Fonts::tag(&key(&run))));
        assert!(fonts.run_ready(&run, &fixture_art(&names, "A")));
        let spaces = TextRun { text: " ".into(), ..run.clone() };
        fonts.fonts.insert(key(&spaces), Some(font(" A")));
        assert!(!fonts.run_ready(&spaces, &fixture_art(&glyph_names(&spaces), " A")), "underlined spaces still need visible bars");
    }

    #[test]
    fn pending_metrics_and_failed_fonts_are_distinct_terminal_states() {
        let run = text_run("AB");
        let mut fonts = Fonts::default();
        let (requests, _requests_rx) = std::sync::mpsc::channel();
        let (completed, loaded) = std::sync::mpsc::channel();
        fonts.loader = Some(Loader { requests, loaded, pending: [key(&run)].into_iter().collect() });
        let compiled = fixture_art(&[], "AB");
        assert!(fonts.run_loading(&run));
        assert!(!fonts.run_ready(&run, &compiled), "outlines without any metrics cannot lay out the run");
        let old = TextRun { height: -10, color: [4, 5, 6], ..run.clone() };
        fonts.fonts.insert(key(&old), Some(font("AB")));
        assert!(fonts.run_ready(&run, &compiled), "supported fallback can draw while exact metrics load");
        assert!(fonts.run_loading(&run));
        fonts.fonts.remove(&key(&old));
        completed.send((key(&run), Err("unsupported font".into()))).unwrap();
        assert!(fonts.take_loaded());
        assert!(!fonts.run_loading(&run));
        assert!(!fonts.run_ready(&run, &compiled));
        fonts.fonts.insert(key(&old), Some(font("AB")));
        assert!(fonts.run_ready(&run, &compiled), "a failed exact load can use supported nearest-height metrics");
        assert!(!fonts.run_ready(&run, &fixture_art(&[], "A")), "outline fallback must support the whole run");
    }

    #[test]
    fn exact_metrics_can_use_outlines_until_the_atlas_arrives() {
        let run = text_run("AB");
        let mut fonts = Fonts::default();
        fonts.fonts.insert(key(&run), Some(font("AB")));
        assert!(fonts.run_ready(&run, &fixture_art(&[], "AB")));
        assert!(!fonts.run_ready(&run, &fixture_art(&[], "A")));
        assert!(!fonts.run_ready(&run, &fixture_art(&[], "")));
        assert!(Fonts::default().run_ready(&text_run(""), &fixture_art(&[], "")));
        assert!(Fonts::default().run_ready(&text_run(" \t\n"), &fixture_art(&[], "")));
    }
    #[test]
    fn ink_bounds_exclude_trailing_spaces_and_empty_glyph_cell_pixels() {
        let run = text_run("A A  ");
        let mut f = font("A ");
        f.glyphs.get_mut(&'A').unwrap().image = tile_render::RgbaImage::from_fn(7, 12, |x, y| {
            if (2..5).contains(&x) && (3..9).contains(&y) { [255;4] } else { [0;4] }
        });
        f.glyphs.get_mut(&'A').unwrap().left = -1;
        let mut fonts = Fonts::default();
        fonts.fonts.insert(key(&run), Some(f));
        assert_eq!(fonts.ink_bounds(&run), Some([11,13,15,6]));
        let mut enlarged = run.clone();
        enlarged.height = -24;
        assert_eq!(fonts.ink_bounds(&enlarged), Some([12,6,30,12]), "nearest loaded metrics scale with their actual rendering");
        fonts.insert(key(&run), Ok(font("A ")));
        assert_eq!(fonts.ink_bounds(&run), Some([10,10,13,1]), "reloaded fonts invalidate cached ink bounds");
        assert_eq!(fonts.ink_bounds(&text_run("   ")), None);
        assert_eq!(Fonts::default().ink_bounds(&run), None);
    }

}
