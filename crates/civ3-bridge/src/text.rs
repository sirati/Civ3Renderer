//! The game's text (labels, status lines), as it draws it through
//! `jgl.dll`'s `TextOutA`: runs of characters at a baseline position in a
//! font and colour. Drawn with the same GDI-rasterised glyphs
//! (`civ3_assets::fonts`), as screen-space sprites on top of everything.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

use civ3_assets::GameFiles;
use civ3_assets::fonts::{GlyphFont, charset};
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
    pub text: String,
}

impl TextRun {
    /// A trace line `text dc D at X Y color R G B font "Face"/H/W/Q str TEXT`.
    pub fn parse(line: &str) -> Option<TextRun> {
        let rest = line.strip_prefix("text dc ")?;
        let (_, rest) = rest.split_once(" at ")?;
        let (head, text) = rest.split_once(" str ")?;
        let f: Vec<&str> = head.split_whitespace().collect();
        let font = head.split_once(" font ")?.1;
        let (face, metrics) = font.strip_prefix('"')?.split_once("\"/")?;
        let height = metrics.split('/').next()?.parse().ok()?;
        Some(TextRun {
            x: f.first()?.parse().ok()?,
            y: f.get(1)?.parse().ok()?,
            color: [f.get(3)?.parse().ok()?, f.get(4)?.parse().ok()?, f.get(5)?.parse().ok()?],
            face: face.to_owned(),
            height,
            text: text.to_owned(),
        })
    }
}

type FontKey = (String, i32, [u8; 3]);

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
            let key = (r.face.clone(), r.height.abs(), r.color);
            if !self.fonts.contains_key(&key) {
                let font = civ3_assets::fonts::load(files, &key.0, key.1, key.2)
                    .inspect_err(|e| log::warn!("font {key:?}: {e}"))
                    .ok();
                self.fonts.insert(key, font);
                added = true;
            }
        }
        added
    }

    fn index(&self, key: &FontKey) -> Option<usize> {
        self.fonts.keys().position(|k| k == key)
    }

    /// Render description entries: glyph sprites `Text@<font>/<code>`
    /// anchored at the pen on the baseline.
    pub fn description(&self) -> String {
        let mut out = String::from("\n# Text glyphs (generated).\n");
        for (n, ((face, height, [r, g, b]), font)) in self.fonts.iter().enumerate() {
            let Some(font) = font else { continue };
            let _ = writeln!(out, "[images.text{n}]\nsource = \"font:{face}/{height}/{r},{g},{b}\"\n");
            for (k, c) in charset().enumerate() {
                let Some(glyph) = font.glyphs.get(&c) else { continue };
                let _ = writeln!(
                    out,
                    "[sheets.text{n}_{k}]\nimage = \"text{n}\"\nframe = {k}\ncell = [{}, {}]\nanchor = [{}, {}]\ngroup = \"Text@{n}\"\nfirst_index = {}\nscaling = \"keep\"\n",
                    glyph.image.width,
                    glyph.image.height,
                    glyph.left,
                    -font.ascent,
                    c as u32
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
                self.run_sprites(r, None, compiled, &mut out);
            }
        }
        out
    }

    /// Glyph sprites of one run, clipped to `clip` (screen rectangle).
    pub fn run_sprites(&self, r: &TextRun, clip: Option<[i32; 4]>, compiled: &Compiled, out: &mut Vec<scene::FreeSprite>) {
        let key = (r.face.clone(), r.height.abs(), r.color);
        let (Some(n), Some(Some(font))) = (self.index(&key), self.fonts.get(&key)) else { return };
        let mut pen = r.x;
        for c in r.text.chars() {
            let Some(glyph) = font.glyphs.get(&c) else { continue };
            if let Some((set, _)) = compiled.set(&format!("Text@{n}/{}", c as u32)) {
                out.push(scene::FreeSprite {
                    set: set.raw(),
                    x: pen,
                    y: r.y,
                    screen: true,
                    after_pass: u32::MAX,
                    phase: 0,
                    ticks_per_frame: 1,
                    scale_den: 0,
                    clip,
                });
            }
            pen += glyph.advance;
        }
    }
}
