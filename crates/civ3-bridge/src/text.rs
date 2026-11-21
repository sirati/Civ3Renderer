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

pub use civ3_ui::text::TextRun;

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
    ink: RefCell<(u64, BTreeMap<FontKey, BTreeMap<char, Option<[i32; 4]>>>)>,
    /// How wide the game measures each run (see [`Fonts::game_width`]),
    /// and whether to ask the system (the game's fonts, in its process).
    widths: RefCell<std::collections::HashMap<(FontStyle, String), Option<i32>>>,
    measures: bool,
    layouts: RefCell<std::collections::HashMap<(FontStyle, String), Option<gdi_text::TextLayout>>>,
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
            // (A bounded wait, not a parked receive: `crate::BOUNDED_WAIT`.)
            loop {
                let (roots, key) = match rx.recv_timeout(crate::BOUNDED_WAIT) {
                    Ok(request) => request,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                };
                let font = civ3_assets::fonts::load(&GameFiles::with_roots(roots), &key.0, key.1);
                if tx.send((key, font)).is_err() {
                    break;
                }
            }
        })?;
        Ok(Fonts { loader: Some(Loader { requests, loaded, pending: BTreeSet::new() }), measures: true, ..Fonts::default() })
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
            // Async prewarm must survive until its first description/upload,
            // even when loading took longer than the normal retention time.
            self.used.insert(key.clone(), std::time::Instant::now());
            self.insert(key, font);
        }
        any
    }

    fn insert(&mut self, key: FontKey, font: Result<GlyphFont, String>) {
        let font = font
            .inspect_err(|e| {
                if self.failed.insert(key.0.clone()) {
                    log::warn!("font {:?}: {e}", key.0);
                    render_errors::report(render_errors::Kind::MissingFont, &format!("font {}", key.0.face), format_args!("{:?}: {e}", key.0));
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
        // A warmed font gets the normal finite retention window. Backdating
        // it allowed the next art rebuild to prune it before first upload.
        self.add(files, std::slice::from_ref(&run))
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
        if let (Some((_, face)), Some(layout)) = (compiled.font(&run.face), self.game_layout(run)) {
            return layout.advances.len() == run.text.chars().count()
                && run.text.chars().all(|c| outline_char(c).is_whitespace() || face.glyph(outline_char(c)).is_some());
        }
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
        if let Some(layout) = self.game_layout(run) {
            return Some((layout.advances.iter().sum(), layout.ascent, layout.descent));
        }
        let Some(Some(font)) = self.fonts.get(&key(run)) else { return None };
        let width = run.text.chars().filter_map(|c| font.glyphs.get(&c)).map(|g| g.advance).sum();
        let cell = font.glyphs.values().map(|g| g.image.height as i32).max().unwrap_or(font.ascent);
        let scale = |v: i32| (v * run.height.abs() + key(run).0.height/2) / key(run).0.height.max(1);
        Some((scale(width), scale(font.ascent), scale((cell - font.ascent).max(0))))
    }

    /// How wide the game measures `run`: its own text routine
    /// (`GetTextExtentPoint32A` with the run's font, as the game sizes a
    /// tooltip's or popup's background from it), in the game's process;
    /// `None` elsewhere (and for fonts not loaded in the background, as the
    /// game's are). Under Wine that is Wine's measure, which is not the
    /// Windows one the glyph sheets were made with.
    pub fn game_width(&self, run: &TextRun) -> Option<i32> {
        // TextRun stores lfHeight; FontStyle/FontSpec negate character height for GDI.
        let style = FontStyle { face: run.face.clone(), height: -run.height, weight: run.weight, italic: run.italic };
        let key = (style, run.text.clone());
        if let Some(&w) = self.widths.borrow().get(&key) {
            return w;
        }
        if !self.measures {
            return None;
        }
        let w = self.game_layout(run).map(|layout| layout.advances.iter().sum()).or_else(|| civ3_assets::fonts::measure(&key.0, &key.1)).filter(|&w| w > 0);
        let mut widths = self.widths.borrow_mut();
        if widths.len() >= 16384 {
            widths.clear();
        }
        widths.insert(key, w);
        w
    }

    /// Final character pens from the game's GDI, independent of glyph pixels.
    fn game_layout(&self, run: &TextRun) -> Option<gdi_text::TextLayout> {
        let style = FontStyle { face: run.face.clone(), height: -run.height, weight: run.weight, italic: run.italic };
        let key = (style, run.text.clone());
        if let Some(layout) = self.layouts.borrow().get(&key) { return layout.clone(); }
        if !self.measures { return None; }
        let spec = gdi_text::FontSpec { face: key.0.face.clone(), height: key.0.height, weight: key.0.weight, italic: key.0.italic, quality: 0 };
        let bytes: Vec<u8> = run.text.chars().map(gdi_text::windows_1252_byte).collect();
        let layout = gdi_text::text_layout(&spec, &bytes).ok();
        let mut layouts = self.layouts.borrow_mut();
        if layouts.len() >= 16384 { layouts.clear(); }
        layouts.insert(key, layout.clone());
        layout
    }

    #[cfg(test)]
    pub(crate) fn set_game_layout(&self, run: &TextRun, layout: gdi_text::TextLayout) {
        let style = FontStyle { face: run.face.clone(), height: -run.height, weight: run.weight, italic: run.italic };
        self.layouts.borrow_mut().insert((style, run.text.clone()), Some(layout));
    }

    /// Takes `width` as the game's measure of `run` (see
    /// [`Fonts::game_width`]; for tests and other systems' measures).
    pub fn set_game_width(&self, run: &TextRun, width: i32) {
        // TextRun stores lfHeight; FontStyle/FontSpec negate character height for GDI.
        let style = FontStyle { face: run.face.clone(), height: -run.height, weight: run.weight, italic: run.italic };
        self.widths.borrow_mut().insert((style, run.text.clone()), Some(width).filter(|&w| w > 0));
    }

    /// `advances` (of `run`'s characters, in order) spread over the game's
    /// measure of the run, so that the run ends where the game's does: its
    /// pens are where the game's would be, wherever the advances of the
    /// glyphs drawn differ from the game's.
    fn fit(&self, run: &TextRun, advances: &mut [i32]) {
        if let Some(layout) = self.game_layout(run).filter(|layout| layout.advances.len() == advances.len()) {
            advances.copy_from_slice(&layout.advances);
            return;
        }
        let total: i64 = advances.iter().map(|&a| i64::from(a)).sum();
        let Some(width) = self.game_width(run).map(i64::from) else { return };
        if total <= 0 || width == total {
            return;
        }
        let (mut sum, mut at) = (0i64, 0i64);
        for a in advances.iter_mut() {
            sum += i64::from(*a);
            let next = (sum * width + total / 2) / total;
            *a = (next - at) as i32;
            at = next;
        }
    }

    /// The em (64ths of a pixel) to draw `run`'s outlines at so that its
    /// glyphs, as wide as the face makes them at the run's size (and
    /// emboldened), fit in `width` pixels at 1:1: the run's own size, or
    /// smaller where the face's glyphs are wider than the game's.
    fn fitting_em(run: &TextRun, outline: &tile_render::gpu_text::Face, chars: impl Iterator<Item = char>, width: i32) -> u32 {
        let em64 = run.height.unsigned_abs() * 64;
        let upem = i64::from(outline.units_per_em());
        if upem == 0 || width <= 0 {
            return em64;
        }
        let (mut units, mut glyphs) = (0i64, 0i64);
        for c in chars {
            units += i64::from(outline.advance(outline_char(c)).unwrap_or(0));
            glyphs += i64::from(!outline_char(c).is_whitespace());
        }
        // (Emboldening widens each glyph by a pixel of the view: two thirds
        // of one at 1:1 in views scaled by 1.5, less in larger ones.)
        let bold = if run.weight >= 600 { glyphs * 43 } else { 0 };
        let natural = units * i64::from(em64) / upem + bold;
        if natural <= i64::from(width) * 64 || units == 0 {
            return em64;
        }
        let room = (i64::from(width) * 64 - bold).max(1);
        (room * upem / units).clamp(64, i64::from(em64)) as u32
    }

    /// Where the glyphs of `chars` go, each centred in its cell (pen and
    /// width at 1:1) at the pens of `advances` from `run.x`, but the first
    /// and the last visible glyph, where the face's glyph at `em64` is wider
    /// than its cell, in a cell as wide as it starting at the run's start
    /// and ending at its end: the run's ink stays within the game's measure
    /// (where there is one, see [`Fonts::game_width`]).
    fn cells(&self, run: &TextRun, outline: &tile_render::gpu_text::Face, em64: u32, chars: &[char], advances: &[i32]) -> Vec<(i32, i32)> {
        let mut pen = run.x;
        let mut cells: Vec<(i32, i32)> = advances.iter().map(|&a| { let cell = (pen, a); pen += a; cell }).collect();
        if self.game_width(run).is_none() {
            return cells;
        }
        let upem = i64::from(outline.units_per_em()).max(1);
        let bold = if run.weight >= 600 { 43 } else { 0 };
        let own = |c: char| ((i64::from(outline.advance(outline_char(c)).unwrap_or(0)) * i64::from(em64) / upem + bold + 63) / 64) as i32;
        let visible: Vec<usize> = (0..chars.len().min(cells.len())).filter(|&i| !outline_char(chars[i]).is_whitespace()).collect();
        if let Some(&first) = visible.first() {
            let (at, cell) = cells[first];
            cells[first] = (at, cell.max(own(chars[first])));
        }
        if let Some(&last) = visible.last() {
            let (at, cell) = cells[last];
            let wide = own(chars[last]).max(cell);
            cells[last] = (at + cell - wide, wide);
        }
        cells
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
        let scale = |v: i32| v * run.height.abs() / height.max(1);
        let mut cache = self.ink.borrow_mut();
        if cache.0 != self.revision { *cache = (self.revision, BTreeMap::new()); }
        // (One key per run: the font, then its characters.)
        if !cache.1.contains_key(&want) {
            cache.1.insert(want.clone(), BTreeMap::new());
        }
        let per_font = cache.1.get_mut(&want).expect("inserted above");
        let mut advances: Vec<i32> = run.text.chars().filter_map(|c| font.glyphs.get(&c)).map(|glyph| scale(glyph.advance)).collect();
        self.fit(run, &mut advances);
        let mut pen = run.x;
        let mut bounds: Option<[i32; 4]> = None;
        for (c, advance) in run.text.chars().filter(|c| font.glyphs.contains_key(c)).zip(advances) {
            let Some(glyph) = font.glyphs.get(&c) else { continue };
            if !c.is_whitespace() {
                let ink = *per_font.entry(c).or_insert_with(|| {
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
            pen += advance;
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

    /// Glyph sprites for `runs`, in order, on top of everything: 1:1, and
    /// drawn from outlines where views show them at other zooms.
    pub fn sprites(&self, runs: &[TextRun], compiled: &Compiled) -> Vec<scene::FreeSprite> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for r in runs {
            // The game redraws some lines several times per frame.
            if seen.insert(r) {
                // 1:1, and from outlines at other zooms (as `ui_screen::sprites`).
                let start = out.len();
                self.run_sprites(r, None, compiled, u32::MAX, &mut out);
                let low = out.len();
                if self.run_glyphs(r, None, compiled, u32::MAX, &mut out) {
                    for s in &mut out[start..low] {
                        s.zoom_art = 1;
                    }
                }
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
        if self.measured_glyphs(r, clip, compiled, after_pass, out, 0) { return; }
        if self.sized_glyphs(r, clip, compiled, after_pass, out, 0) { return; }
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
        // (At the game's pens: the run ends where the game measured it.)
        let mut advances: Vec<i32> = r.text.chars().filter_map(|c| font.glyphs.get(&c)).map(|g| g.advance).collect();
        self.fit(r, &mut advances);
        let mut pen = r.x;
        for (glyph, advance) in r.text.chars().filter_map(|c| font.glyphs.get(&c).map(|g| (c, g))).zip(advances) {
            let (c, glyph) = glyph;
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
            pen += advance;
        }
    }

    /// Emits outline commands using final measured pens even before any
    /// glyph sheet or alternate-size metrics exist. No pixels are rendered here.
    fn measured_glyphs(&self, r: &TextRun, clip: Option<[i32; 4]>, compiled: &Compiled, after_pass: u32, out: &mut Vec<scene::FreeSprite>, zoom_art: u8) -> bool {
        let (Some((face, outline)), Some(layout)) = (compiled.font(&r.face), self.game_layout(r)) else { return false };
        let chars: Vec<char> = r.text.chars().collect();
        if chars.len() != layout.advances.len() { return false; }
        let em64 = Self::fitting_em(r, outline, chars.iter().copied(), layout.advances.iter().sum());
        let tint = (r.color[0] as u32) << 16 | (r.color[1] as u32) << 8 | r.color[2] as u32;
        for (&c, (pen, advance)) in chars.iter().zip(self.cells(r, outline, em64, &chars, &layout.advances)) {
            let glyph = scene::Glyph { font: face, ch: outline_char(c), em64, advance, bold: r.weight >= 600, italic: r.italic, key: r.backdrop, bar: None };
            // Missing cmap entries are independent missing glyphs, not
            // glyph zero boxes. Their measured advance still positions later text.
            if outline.glyph(glyph.ch).is_none() && !c.is_whitespace() && !c.is_control() {
                render_errors::report(render_errors::Kind::TextShape, &format!("font {}", r.face), format_args!("{c:?} in {:?}", r.text));
            }
            if outline.glyph(glyph.ch).is_some() {
                out.push(scene::FreeSprite { x: pen, y: r.y, screen: true, after_pass, ticks_per_frame: 1, tint, clip, glyph: Some(glyph), zoom_art, ..Default::default() });
            }
            if r.underline {
                let glyph = scene::Glyph { bar: Some(layout.underline), key: None, ..glyph };
                out.push(scene::FreeSprite { x: pen, y: r.y, screen: true, after_pass, ticks_per_frame: 1, tint, clip, glyph: Some(glyph), zoom_art, ..Default::default() });
            }
        }
        true
    }

    fn outline_fallback(&self, r: &TextRun, clip: Option<[i32; 4]>, compiled: &Compiled, after_pass: u32, out: &mut Vec<scene::FreeSprite>) {
        if self.measured_glyphs(r, clip, compiled, after_pass, out, 0) { return; }
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
        let glyphs: Vec<(char, &Glyph)> = r.text.chars().filter_map(|c| font.glyphs.get(&c).map(|g| (c, g))).collect();
        let mut advances: Vec<i32> = glyphs.iter().map(|&(c, glyph)| if legacy_c1(c, glyph) { legacy_advance(c, outline, want.0.height).unwrap_or(0) } else { scale(glyph.advance) }).collect();
        // (At the game's pens, the glyphs as wide as there is room for.)
        self.fit(r, &mut advances);
        let em64 = Self::fitting_em(r, outline, glyphs.iter().map(|&(c, _)| c), advances.iter().sum());
        let chars: Vec<char> = glyphs.iter().map(|&(c, _)| c).collect();
        for (&c, (pen, advance)) in chars.iter().zip(self.cells(r, outline, em64, &chars, &advances)) {
            let g = scene::Glyph {
                font: face,
                ch: outline_char(c),
                em64,
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
        if self.measured_glyphs(r, clip, compiled, after_pass, out, 2) { return true; }
        if self.sized_glyphs(r, clip, compiled, after_pass, out, 2) { return true; }
        let want = key(r);
        let (Some(Some(font)), Some((face, outline))) = (self.fonts.get(&want), compiled.font(&r.face)) else { return false };
        let rgb = |[r, g, b]: [u8; 3]| (r as u32) << 16 | (g as u32) << 8 | b as u32;
        // Windows makes bold of a regular face from weight 600 on.
        let glyphs: Vec<(char, &Glyph)> = r.text.chars().filter_map(|c| font.glyphs.get(&c).map(|g| (c, g))).collect();
        let mut advances: Vec<i32> = glyphs.iter().map(|&(c, glyph)| if legacy_c1(c, glyph) { legacy_advance(c, outline, want.0.height).unwrap_or(0) } else { glyph.advance }).collect();
        // At the game's pens (the run ends where the game measured it, its
        // background around it), the glyphs as wide as there is room for.
        self.fit(r, &mut advances);
        let style = scene::Glyph {
            font: face,
            ch: ' ',
            em64: Self::fitting_em(r, outline, glyphs.iter().map(|&(c, _)| c), advances.iter().sum()),
            advance: 0,
            bold: r.weight >= 600,
            italic: r.italic,
            key: r.backdrop,
            bar: None,
        };
        let chars: Vec<char> = glyphs.iter().map(|&(c, _)| c).collect();
        for (&c, (pen, advance)) in chars.iter().zip(self.cells(r, outline, style.em64, &chars, &advances)) {
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
        }
        true
    }

    /// Linux and Wine may have only the game's native font sizes cached.
    /// Lay out that sheet, then scale its outline glyphs to the requested
    /// size. This also keeps the 1:1 path in step with enlarged HUD plates.
    fn sized_glyphs(&self, run: &TextRun, clip: Option<[i32; 4]>, compiled: &Compiled,
        after_pass: u32, out: &mut Vec<scene::FreeSprite>, zoom_art: u8) -> bool {
        let height = key(run).0.height;
        if height == run.height.abs() { return false; }
        let native = TextRun { height: -height, ..run.clone() };
        let mut glyphs = Vec::new();
        if !self.run_glyphs(&native, clip, compiled, after_pass, &mut glyphs) { return false; }
        let scale = |v: i32| (v as f64 * run.height.abs() as f64 / height.max(1) as f64).round() as i32;
        for mut sprite in glyphs {
            sprite.x = run.x + scale(sprite.x-run.x);
            sprite.y = run.y + scale(sprite.y-run.y);
            sprite.zoom_art = zoom_art;
            if let Some(glyph) = &mut sprite.glyph {
                glyph.em64 = scale(glyph.em64 as i32).max(64) as u32;
                glyph.advance = scale(glyph.advance);
                if let Some((offset, thickness)) = &mut glyph.bar {
                    *offset = scale(*offset);
                    *thickness = scale(*thickness);
                }
            }
            out.push(sprite);
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
    fn a_prewarmed_font_survives_pruning_before_its_first_upload() {
        let style = FontStyle { face: "Lucida Sans".into(), height: 12, weight: 400, italic: false };
        let mut fonts = Fonts::default();
        let (requests, requested) = std::sync::mpsc::channel();
        let (completed, loaded) = std::sync::mpsc::channel();
        fonts.loader = Some(Loader { requests, loaded, pending: BTreeSet::new() });
        fonts.add_style(&GameFiles::with_roots(Vec::new()), style, [1, 2, 3], None);
        let (_, key) = requested.try_recv().unwrap();
        assert!(fonts.used[&key].elapsed() < std::time::Duration::from_secs(60), "admission cannot make prewarm immediately stale");
        // Completion after a long load must start a fresh bounded lifetime.
        fonts.used.insert(key.clone(), std::time::Instant::now() - std::time::Duration::from_secs(120));
        completed.send((key.clone(), Ok(GlyphFont { ascent: 12, underline: (1, 1), glyphs: Default::default() }))).unwrap();
        assert!(fonts.take_loaded());
        assert_eq!(fonts.prune(std::time::Duration::from_secs(60)), 0);
        assert!(fonts.has_new());
        assert!(fonts.addition().contains("source = \"font:Lucida Sans/12/1,2,3"));
        assert!(fonts.described.contains(&key));
        // Prewarming does not retain unused glyph pixels forever.
        fonts.used.insert(key.clone(), std::time::Instant::now() - std::time::Duration::from_secs(120));
        assert_eq!(fonts.prune(std::time::Duration::from_secs(60)), 1);
        assert!(!fonts.fonts.contains_key(&key));
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
        assert_eq!((g.advance, g.bold, out[2].tint, out[2].zoom_art, out[2].after_pass), (7, true, 0x010203, 2, 3));
        // (Smaller than the run's em where the face's bold glyphs are wider
        // than these advances: they stay within them.)
        assert!(g.em64 <= 12 * 64 && g.em64 > 9 * 64, "{}", g.em64);
        assert!(!fonts.run_glyphs(&run, None, &desc(""), 3, &mut out));
    }
}

#[cfg(test)]
pub(crate) mod readiness_tests {
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

    pub(super) fn font(chars: &str) -> GlyphFont {
        let glyphs = chars.chars().map(|c| {
            let image = tile_render::RgbaImage::from_fn(1, 1, |_, _| if c.is_whitespace() { [0; 4] } else { [255; 4] });
            (c, civ3_assets::fonts::Glyph { left: 0, advance: 6, extent: 6, image })
        }).collect();
        GlyphFont { ascent: 10, underline: (1, 1), glyphs }
    }

    pub(crate) fn fixture_art(names: &[String], outlines: &str) -> Compiled {
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

    #[test]
    fn requested_sizes_scale_cached_glyphs_and_metrics_together() {
        let run = TextRun { face: "Lucida Sans".into(), height: -97, text: "x3".into(), ..text_run("") };
        let cached = key(&run).0.height;
        if cached == run.height.abs() { return; } // A system GDI can supply the exact size.
        let mut fonts = Fonts::default();
        fonts.fonts.insert(key(&run), Some(font("x3")));
        let mut compiled = fixture_art(&[], "x3");
        compiled.fonts[0].0 = run.face.clone();
        let native = TextRun { height: -cached, ..run.clone() };
        let mut before = Vec::new();
        let mut after = Vec::new();
        assert!(fonts.run_glyphs(&native, None, &compiled, 3, &mut before));
        fonts.run_sprites(&run, None, &compiled, 3, &mut after);
        let scale = |v: i32| (v as f64*97.0/cached as f64).round() as i32;
        assert_eq!(after.len(), 2);
        for (native, sized) in before.iter().zip(&after) {
            assert_eq!(sized.x-run.x, scale(native.x-run.x));
            assert_eq!(sized.glyph.unwrap().em64 as i32, scale(native.glyph.unwrap().em64 as i32));
            assert_eq!(sized.glyph.unwrap().advance, scale(native.glyph.unwrap().advance));
            assert_eq!(sized.zoom_art, 0, "the enlarged text must also render at 1:1");
        }
        assert_eq!(fonts.metrics(&run).unwrap().0, scale(12));
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
        fonts.set_game_width(&run, 15);
        assert_eq!(fonts.ink_bounds(&run), Some([11,13,9,6]), "anchors follow fitted pens");
        fonts.set_game_width(&run, 0);
        let mut enlarged = run.clone();
        enlarged.height = -24;
        assert_eq!(fonts.ink_bounds(&enlarged), Some([12,6,30,12]), "nearest loaded metrics scale with their actual rendering");
        fonts.insert(key(&run), Ok(font("A ")));
        assert_eq!(fonts.ink_bounds(&run), Some([10,10,13,1]), "reloaded fonts invalidate cached ink bounds");
        assert_eq!(fonts.ink_bounds(&text_run("   ")), None);
        assert_eq!(Fonts::default().ink_bounds(&run), None);
    }

}

#[cfg(test)]
mod window_resolution_tests {
    use super::*;
    use crate::ui_screen::Item;

    /// A screen sprite's rectangle as the viewer measures it (a glyph: its
    /// em box).
    fn rect_of(compiled: &Compiled, s: &scene::FreeSprite) -> Option<[i32; 4]> {
        if let Some(g) = s.glyph {
            let em = (g.em64 as i32 + 63) / 64;
            return Some([s.x, s.y - em, g.advance.max(1), em + em / 3]);
        }
        let id = compiled.sets.frame(tile_render::SpriteSetId::from_raw(s.set), 0)?;
        let sprite = compiled.atlas.sprites.get(id.0 as usize)?;
        Some([s.x + sprite.anchor_x, s.y + sprite.anchor_y, sprite.width as i32, sprite.height as i32])
    }

    /// The ink bounds (left, top, right, bottom; exclusive) in `r`.
    fn ink(image: &tile_render::RgbaImage, [x0, y0, x1, y1]: [i32; 4]) -> Option<[i32; 4]> {
        let mut b: Option<[i32; 4]> = None;
        for y in y0.max(0)..y1.min(image.height as i32) {
            for x in x0.max(0)..x1.min(image.width as i32) {
                if image.get(x as u32, y as u32)[..3] != [0, 0, 0] {
                    b = Some(b.map_or([x, y, x + 1, y + 1], |[l, t, r, bb]| [l.min(x), t.min(y), r.max(x + 1), bb.max(y + 1)]));
                }
            }
        }
        b
    }

    /// The user's case: the game's title rows and Preferences labels
    /// (Lucida Sans, the game's sizes and glyph metrics) on TS Neo's
    /// 3274×2048 screen, in a 3258×1904 window at desktop scale 2 and the
    /// default pixel scale ×1 (every window pixel rendered), menus scaled
    /// to 100 % and 75 % of the window. Every run is
    /// drawn from the face's outlines at the window's resolution (not in
    /// 2×2 boxes), at its em times its scale on the window, within its
    /// scaled 1:1 layout (pens and advances), and nothing of it comes from
    /// the 1:1 glyph sheets.
    #[test]
    fn title_and_preferences_text_is_drawn_at_the_window_resolution() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        let files = GameFiles::complete(&install);
        if files.read("LSANS.TTF").is_err() {
            return eprintln!("no Civ III installation, skipped");
        }
        let screen = (3274u32, 2048u32);
        let (mx, my) = ((screen.0 as i32 - 1024) / 2, (screen.1 as i32 - 768) / 2);
        let run = |x, y, height: i32, color, text: &str| TextRun { x, y, color, face: "Lucida Sans".into(), height: -height, weight: 0, italic: false, underline: false, text: text.into(), backdrop: None };
        let title: Vec<TextRun> = ["New Game", "Quick Start", "Load Game", "Load Scenario", "Hall of Fame", "Preferences", "Credits", "Exit"]
            .iter().enumerate().map(|(row, label)| run(mx + 213, my + 311 + 38 * row as i32, 18, [248; 3], label)).collect();
        let prefs: Vec<TextRun> = [(16, "Preferences"), (14, "Display"), (13, "Pixel scale"), (12, "Menu scale 100%"), (11, "Smooth menu scrolling")]
            .iter().enumerate().map(|(i, &(h, text))| run(mx + 64, my + 90 + 44 * i as i32, h, [250, 236, 200], text)).collect();
        let mut fonts = Fonts::default();
        for r in title.iter().chain(&prefs) {
            let key = key(r);
            if fonts.fonts.contains_key(&key) { continue; }
            let Ok(metrics) = civ3_assets::fonts::load(&files, &key.0, r.color) else { return eprintln!("no Lucida Sans glyph sheets of {:?}, skipped", key.0) };
            fonts.fonts.insert(key, Some(metrics));
        }
        let description = render_desc::Description::parse(&format!("grid = {{ layout = \"orthogonal\", step = [1, 1] }}\n{}", fonts.description())).unwrap();
        let compiled = description.compile(&civ3_assets::loader::Civ3Loader::new(files), 1024).unwrap();
        assert_eq!(crate::ui_screen::outline_zooms(&compiled), [(0, 0)]);
        for (name, runs) in [("title", &title), ("Preferences", &prefs)] {
            let items: Vec<Item> = runs.iter().map(|r| Item::Text(r.clone(), [0, 0, screen.0 as i32, screen.1 as i32], false)).collect();
            let sprites = crate::ui_screen::sprites(&items, &fonts, &compiled, u32::MAX, true);
            assert!(sprites.iter().any(|s| s.glyph.is_none() && s.zoom_art == 1) && sprites.iter().all(|s| s.glyph.is_none() || s.zoom_art == 2), "{name}: 1:1 sheets and outline twins");
            let prefs_screen = name == "Preferences";
            let view = scene::View { interface_only: true, menu: !prefs_screen, origin: if prefs_screen { (mx, my) } else { (0, 0) }, viewport: if prefs_screen { (1024, 768) } else { (0, 0) }, ..Default::default() };
            let scene = scene::Scene {
                screen_size: screen,
                zooms: vec![(0, 0)],
                interface: scene::Interface { menu_fit: true, screen: prefs_screen.then(|| "Menu".into()), screen_role: if prefs_screen { scene::ScreenRole::Menu } else { scene::ScreenRole::Other }, screen_view: prefs_screen.then_some(258), ..Default::default() },
                views: vec![scene::ViewState { id: if prefs_screen { 258 } else { 255 }, view, overlay: sprites }],
                ..Default::default()
            };
            let rect_of = |s: &scene::FreeSprite| rect_of(&compiled, s);
            for menu_scale_percent in [100.0, 75.0] {
                let settings = viewer_settings::Resolution { scale_menus: true, menu_scale_percent, ..Default::default() };
                let plan = tile_viewer::scaling::plan((3258, 1904), screen, &settings).for_scene(&scene, rect_of);
                assert_eq!((plan.present.zoom, plan.canvas()), ((1, 1), (3258, 1904)), "TS Neo at the default ×1");
                let mut layout = tile_viewer::layout::Layout::default();
                layout.driver_screen = Some(plan.screen);
                layout.menu_scale = plan.menu.zoom;
                let mut frame = layout.frame(&scene, plan.canvas(), (0, 0, 0, 0), vec![], None);
                tile_viewer::scaling::arrange_panned(&plan, &scene, &mut frame, &mut layout.menu_pan, rect_of);
                let drawn = tile_viewer::scaling::drawn(&frame.cameras, &frame.copies, plan.present, |zoom| layout.zoomed_art(zoom));
                assert_eq!((drawn.pixel_box, drawn.present.canvas, drawn.present.zoom), (1, (3258, 1904), (1, 1)), "{name}: composed at the window's resolution");
                let menu = frame.view_cameras[0].unwrap() as usize;
                let camera = drawn.cameras[menu];
                assert!(camera.zoomed_art, "{name}: text from outlines");
                let zoom = camera.zoom.0 as f64 / camera.zoom.1 as f64;
                let what = format!("{name}, menus at {menu_scale_percent} %, {zoom}x on the window");
                assert!(zoom > 1.0 && (zoom - frame.cameras[menu].zoom.0 as f64 / frame.cameras[menu].zoom.1 as f64).abs() < 1e-9, "{what}");
                let world = tile_viewer::scene_world_for(&compiled, &scene, &frame.view_cameras);
                let picture = tile_render::reference::render_views(&world, &compiled.atlas, &drawn.cameras, drawn.present.target, 0, [0, 0, 0, 255]);
                // Without the outline glyphs nothing of the text is drawn.
                let mut sheets = world.clone();
                sheets.free.retain(|s| s.text.is_none());
                let rest = tile_render::reference::render_views(&sheets, &compiled.atlas, &drawn.cameras, drawn.present.target, 0, [0, 0, 0, 255]);
                assert!(rest.pixels.iter().all(|p| p[..3] == [0, 0, 0]), "{what}: no 1:1 glyph sheet is drawn");
                // (For looking at: the window's pixels as drawn.)
                if let Some(dir) = std::env::var_os("CIV3R_TEXT_DUMP") {
                    let save = |file: String, f: &dyn Fn(u32, u32) -> [u8; 4]| image::RgbaImage::from_fn(drawn.present.target.0, drawn.present.target.1, |x, y| image::Rgba(f(x, y))).save(std::path::Path::new(&dir).join(file)).unwrap();
                    save(format!("{name}-{menu_scale_percent}-window.png"), &|x, y| picture.get(x, y));
                }
                // Driver pixel (x, y) on the window (its top-left).
                let base = (camera.origin.0 - camera.screen_shift.0, camera.origin.1 - camera.screen_shift.1);
                let at = |x: i32, y: i32| (camera.origin.0 as f64 + (x - base.0) as f64 * zoom, camera.origin.1 as f64 + (y - base.1) as f64 * zoom);
                for r in runs.iter() {
                    let (width, _, _) = fonts.metrics(r).unwrap();
                    let em = r.height.abs() as f64 * zoom;
                    let ((x0, baseline), (x1, _)) = (at(r.x, r.y), at(r.x + width, r.y));
                    let around = [x0 as i32 - 8, (baseline - 1.5 * em) as i32, x1 as i32 + 8, (baseline + em) as i32];
                    let [l, t, rr, b] = ink(&picture, around).unwrap_or_else(|| panic!("{what}: {:?} drawn", r.text));
                    let what = format!("{what}: {:?} at {em:.1} px per em", r.text);
                    assert!(l as f64 >= x0 - 1.0 && rr as f64 <= x1 + 1.0, "{what}: ink {l}..{rr} in its scaled advances {x0:.1}..{x1:.1}");
                    assert!(t as f64 >= baseline - em - 1.0 && b as f64 <= baseline + em / 3.0 + 1.0, "{what}: ink {t}..{b} about its baseline {baseline:.1}");
                    // Capitals and ascenders reach about 0.72 em up.
                    assert!((baseline - t as f64) > 0.6 * em && (baseline - t as f64) < 0.85 * em, "{what}: {} pixels above the baseline", baseline - t as f64);
                    let boxed = (t..b).all(|y| (l..rr).all(|x| picture.get(x as u32, y as u32) == picture.get((x & !1) as u32, (y & !1) as u32)));
                    assert!(!boxed, "{what}: drawn at the window's resolution, not in 2x2 boxes");
                }
            }
        }
    }
}

#[cfg(test)]
mod fit_tests {
    use super::*;

    /// Leftmost and rightmost columns with ink in `image`.
    fn columns(image: &tile_render::RgbaImage) -> Option<(u32, u32)> {
        let lit: Vec<u32> = (0..image.width).filter(|&x| (0..image.height).any(|y| image.get(x, y)[..3] != [0, 0, 0])).collect();
        Some((*lit.first()?, *lit.last()? + 1))
    }

    /// Tooltips and popups whose background the game sized by its own
    /// measure (here the Windows one, and one 3 % narrower as Wine's can
    /// be): drawn from outlines at the scales a window shows them at, no
    /// glyph's ink reaches past the game's text box.
    #[test]
    fn tooltip_and_popup_text_stays_within_the_games_measure_at_every_scale() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        let files = GameFiles::complete(&install);
        if files.read("LSANS.TTF").is_err() {
            return eprintln!("no Civ III installation, skipped");
        }
        let run = |height: i32, weight, text: &str| TextRun { x: 6, y: 30, color: [255, 255, 255], face: "Lucida Sans".into(), height: -height, weight, italic: false, underline: false, text: text.into(), backdrop: None };
        let runs = [
            run(12, 0, "Fortify (F)"),
            run(12, 0, "Build Colony (B)"),
            run(12, 0, "Settler: Movement 1, Cost 30, Pop 1"),
            run(12, 700, "Pillage (Shift-P)"),
            run(14, 0, "Do you want to declare war on the Babylonians?"),
            run(14, 700, "Our scientists have discovered Bronze Working!"),
            run(16, 0, "Wonderful! We have built the Pyramids."),
        ];
        let mut fonts = Fonts::default();
        for r in &runs {
            let key = key(r);
            if fonts.fonts.contains_key(&key) { continue; }
            let Ok(metrics) = civ3_assets::fonts::load(&files, &key.0, r.color) else { return eprintln!("no Lucida Sans glyph sheets of {:?}, skipped", key.0) };
            fonts.fonts.insert(key, Some(metrics));
        }
        let description = render_desc::Description::parse(&format!("grid = {{ layout = \"orthogonal\", step = [1, 1] }}\n{}", fonts.description())).unwrap();
        let compiled = description.compile(&civ3_assets::loader::Civ3Loader::new(files), 1024).unwrap();
        for r in &runs {
            let (sheets, _, _) = fonts.metrics(r).unwrap();
            for width in [sheets, sheets * 97 / 100] {
                fonts.set_game_width(r, width);
                let mut sprites = Vec::new();
                assert!(fonts.run_glyphs(r, None, &compiled, u32::MAX, &mut sprites));
                // (The pens: on the game's measure, the last glyph ending it.)
                let pens: Vec<i32> = sprites.iter().filter(|s| s.glyph.is_some_and(|g| g.bar.is_none())).map(|s| s.x + s.glyph.unwrap().advance).collect();
                assert_eq!(pens.last().copied(), Some(r.x + width), "{:?}", r.text);
                let mut world = compiled.world(1, 1, false, false);
                for s in &sprites {
                    world.free.push(tile_viewer::placed_sprite(&compiled, &scene::FreeSprite { zoom_art: 0, ..s.clone() }, None, None));
                }
                for (n, d) in [(1u32, 1u32), (3, 2), (2, 1), (3, 1)] {
                    let camera = tile_render::Camera { zoomed_art: true, zoom: (n, d), ..tile_render::Camera::new((1400 * n / d, 60 * n / d)) };
                    let picture = tile_render::reference::render(&world, &compiled.atlas, &camera, 0, [0, 0, 0, 255]);
                    // (Ink on black: drawn in white.)
                    let Some((l, right)) = columns(&picture) else { panic!("{:?} at {n}/{d}: nothing drawn", r.text) };
                    let (x0, x1) = ((r.x * n as i32) as f64 / d as f64, ((r.x + width) * n as i32) as f64 / d as f64);
                    let what = format!("{:?} ({} px, weight {}) measured {width} at {n}/{d}: ink {l}..{right}, box {x0}..{x1}", r.text, -r.height, r.weight);
                    assert!(l as f64 >= x0 - 1.0 && right as f64 <= x1 + 1.0, "{what}");
                }
            }
        }
    }

    #[test]
    fn measured_character_pens_do_not_change_when_glyph_metrics_arrive() {
        use super::readiness_tests::{fixture_art, font};
        let run = TextRun { x: 10, y: 20, color: [0; 3], face: "Readiness fixture".into(), height: -12, weight: 400, italic: false, underline: true, text: "ABA".into(), backdrop: None };
        let compiled = fixture_art(&[], "AB");
        let mut fonts = Fonts::default();
        fonts.set_game_layout(&run, gdi_text::TextLayout { advances: vec![3, 8, 4], ascent: 9, descent: 3, underline: (1, 1) });
        let geometry = fonts.metrics(&run);
        assert_eq!(geometry, Some((15, 9, 3)), "panel layout is final before sheet metrics exist");
        let mut before = Vec::new();
        fonts.run_sprites(&run, None, &compiled, u32::MAX, &mut before);
        assert_eq!(before.iter().filter(|s| s.glyph.unwrap().bar.is_none()).map(|s| s.x).collect::<Vec<_>>(), [10, 13, 21]);
        fonts.fonts.insert(key(&run), Some(font("AB")));
        let names: Vec<String> = run.text.chars().collect::<BTreeSet<_>>().into_iter().map(|c| format!("Text@{}/{}", Fonts::tag(&key(&run)), c as u32)).collect();
        let ready = fixture_art(&names, "AB");
        assert!(fonts.run_ready(&run, &ready), "exact metrics and atlas art have arrived");
        let mut after = Vec::new();
        fonts.run_sprites(&run, None, &ready, u32::MAX, &mut after);
        assert_eq!(before, after, "exact sheet metrics cannot move the first emitted layout");
        assert_eq!(fonts.metrics(&run), geometry, "panel measurements remain final too");
        let mut zoomed = Vec::new();
        assert!(fonts.run_glyphs(&run, None, &ready, u32::MAX, &mut zoomed));
        for s in &mut zoomed { s.zoom_art = 0; }
        assert_eq!(before, zoomed, "every zoom uses the same final pens and underline");
    }

    #[test]
    fn missing_outline_glyphs_keep_later_measured_pens() {
        let run = TextRun { x: 10, y: 20, color: [0; 3], face: "Readiness fixture".into(), height: -12, weight: 400, italic: false, underline: false, text: "A\tCB".into(), backdrop: None };
        let compiled = super::readiness_tests::fixture_art(&[], "AB");
        let fonts = Fonts::default();
        fonts.set_game_layout(&run, gdi_text::TextLayout { advances: vec![3, 2, 7, 4], ascent: 9, descent: 3, underline: (1, 1) });
        let mut commands = Vec::new();
        fonts.run_sprites(&run, None, &compiled, u32::MAX, &mut commands);
        assert_eq!(commands.iter().map(|s| (s.x, s.glyph.unwrap().ch)).collect::<Vec<_>>(), [(10, 'A'), (22, 'B')]);
        assert!(!fonts.run_ready(&run, &compiled), "diagnostics may report missing C without hiding A or B");
    }

    #[cfg(windows)]
    #[test]
    fn game_width_preserves_the_games_lfheight_sign() {
        let fonts = Fonts::in_background().unwrap();
        for height in [-18, 18] {
            let run = TextRun { x: 0, y: 0, color: [0; 3], face: "Arial".into(), height, weight: 400, italic: false, underline: false, text: "Fortify and build a colony".into(), backdrop: None };
            let spec = gdi_text::FontSpec { face: run.face.clone(), height: -height, weight: run.weight, italic: run.italic, quality: 0 };
            let expected = gdi_text::text_extent(&spec, run.text.as_bytes()).unwrap();
            let layout = gdi_text::text_layout(&spec, run.text.as_bytes()).unwrap();
            assert_eq!(layout.advances.len(), run.text.len());
            assert_eq!(layout.advances.iter().sum::<i32>(), expected);
            assert!(expected > 0);
            assert_eq!(fonts.game_width(&run), Some(expected), "actual lfHeight {height}");
            fonts.set_game_width(&run, expected + 7);
            assert_eq!(fonts.game_width(&run), Some(expected + 7), "injected widths use the same cache key");
        }
    }

    #[cfg(windows)]
    #[test]
    fn measured_layout_keeps_unicode_and_raw_ansi_punctuation_pens_equal() {
        let fonts = Fonts::in_background().unwrap();
        let unicode = TextRun { x: 0, y: 0, color: [0; 3], face: "Arial".into(), height: -18, weight: 400, italic: false, underline: false, text: "A’…€B".into(), backdrop: None };
        let raw = TextRun { text: "A\u{92}\u{85}\u{80}B".into(), ..unicode.clone() };
        let layout = fonts.game_layout(&unicode).unwrap();
        assert_eq!(layout, fonts.game_layout(&raw).unwrap());
        let spec = gdi_text::FontSpec { face: "Arial".into(), height: 18, weight: 400, italic: false, quality: 0 };
        assert_eq!(layout.advances.iter().sum::<i32>(), gdi_text::text_extent(&spec, &[b'A', 0x92, 0x85, 0x80, b'B']).unwrap());
        assert_eq!(fonts.metrics(&unicode), fonts.metrics(&raw));
    }

    #[test]
    fn advances_are_spread_over_the_games_measure() {
        let fonts = Fonts::default();
        let run = TextRun { x: 0, y: 0, color: [0; 3], face: "f".into(), height: -12, weight: 0, italic: false, underline: false, text: "abcd".into(), backdrop: None };
        let mut advances = [7, 7, 7, 7];
        fonts.fit(&run, &mut advances);
        assert_eq!(advances, [7, 7, 7, 7], "no measure (not in the game's process): as they are");
        fonts.set_game_width(&run, 26);
        fonts.fit(&run, &mut advances);
        assert_eq!(advances.iter().sum::<i32>(), 26);
        assert!(advances.iter().all(|&a| a == 6 || a == 7), "{advances:?}");
        fonts.set_game_width(&run, 30);
        let mut wider = [7, 7, 7, 7];
        fonts.fit(&run, &mut wider);
        assert_eq!(wider.iter().sum::<i32>(), 30);
    }
}

