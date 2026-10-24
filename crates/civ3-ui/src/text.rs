//! The game's text: runs of characters in a font and colour at a baseline
//! point, as jgl's `text_out` gets them, and [`TextCapture`], which makes
//! them from the jgl text hooks.

use jgl::{FontDesc, TextDraw, TextHooks};


/// One `text_out` (`TextOutA`) call: `text` with its pen at (x, baseline y).
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

/// A text run on an interface surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanvasText {
    /// The game's image the run is drawn on (`PCX_Image*`, the jgl image's
    /// owner).
    pub canvas: u32,
    /// Its clip rectangle at the call (left, top, right, bottom).
    pub clip: [i32; 4],
    pub run: TextRun,
}

/// Where [`TextCapture`] hands its runs.
pub trait TextSink {
    fn text(&mut self, text: CanvasText);
}

impl<F: FnMut(CanvasText)> TextSink for F {
    fn text(&mut self, text: CanvasText) {
        self(text)
    }
}

/// The text hooks of the interface: every `text_out` as a [`TextRun`] with
/// its canvas and clip.
pub struct TextCapture<S> {
    sink: S,
    /// Fonts seen (logged once each).
    fonts: Vec<FontDesc>,
}

impl<S: TextSink> TextCapture<S> {
    pub fn new(sink: S) -> TextCapture<S> {
        TextCapture { sink, fonts: Vec::new() }
    }
}

/// The run `draw` is. Its colour as GDI draws it on the game's 16-bit
/// canvases (a `DIBINDEX` colour is black there).
pub fn run_of(draw: &TextDraw<'_>) -> TextRun {
    TextRun {
        x: draw.x,
        y: draw.y,
        color: draw.color.on_16_bit(),
        face: draw.font.face.clone(),
        height: draw.font.height,
        weight: draw.font.weight,
        italic: draw.font.italic,
        underline: draw.font.underline,
        text: jgl::text::latin1(draw.bytes),
        backdrop: None,
    }
}

impl<S: TextSink> TextHooks for TextCapture<S> {
    fn text(&mut self, draw: &TextDraw<'_>) {
        self.sink.text(CanvasText { canvas: draw.owner, clip: draw.clip.to_array(), run: run_of(draw) });
    }

    fn font(&mut self, desc: &FontDesc) {
        if !self.fonts.contains(desc) {
            log::info!("text font {desc:?}");
            self.fonts.push(desc.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jgl::{FontId, Hooks, ImageId, Jgl, Rect};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn jgl_calls_become_runs_on_their_canvas() {
        let out = Rc::new(RefCell::new(Vec::new()));
        let o = out.clone();
        let capture = TextCapture::new(move |t: CanvasText| o.borrow_mut().push(t));
        let stock = FontDesc { face: "System".into(), height: 16, weight: 700, italic: false, underline: false };
        let mut j = Jgl::new(Hooks { text: Box::new(capture), policy: Box::new(jgl::hooks::NoHooks) }, stock);
        let (img, font) = (ImageId(0x10), FontId(0x20));
        j.with(|p| p.image_reset(img));
        j.with(|p| p.font_created(font, 12, 1, b"Times New Roman"));
        j.with(|p| p.select_font(img, font));
        j.with(|p| p.text_color(img, 0x10FF_0000));
        j.with(|p| p.text(img, 0x9C_0000, 4, 30, Rect::new(0, 0, 100, 50), b"Rome\xE9"));
        j.with(|p| p.text_color(img, 0x00C0_8040));
        j.with(|p| p.text(img, 0x9C_0000, 8, 40, Rect::new(0, 0, 100, 50), b"x"));
        let out = out.borrow();
        assert_eq!(out[0], CanvasText {
            canvas: 0x9C_0000,
            clip: [0, 0, 100, 50],
            run: TextRun { x: 4, y: 30, color: [0, 0, 0], face: "Times New Roman".into(), height: -12, weight: 700, italic: false, underline: false, text: "Rome\u{e9}".into(), backdrop: None },
        });
        assert_eq!(out[1].run.color, [0x40, 0x80, 0xC0]);
    }
}
