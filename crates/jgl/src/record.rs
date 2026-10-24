//! The record encoding: one record per jgl call that matters, appended by
//! the slot bodies and read back by the consumer.
//!
//! Layout, little endian, every record a multiple of 4 bytes:
//!
//! ```text
//! u16 op | u16 words      length in 4-byte words, header included
//! u32 ...                 the op's fixed fields
//! u8  ...                 tail bytes (text, face), length in the last
//!                         fixed field, zero padded to 4
//! ```
//!
//! The format is the same in memory and in a dump file (`jgl::record`
//! streams can be written out and replayed offline).

use crate::draw::{FillCall, MAX_ARGS, Surface};
use crate::{FontId, ImageId, Rect};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Op {
    /// Font slot +0x08 `create(face, height, flags)`: jgl's arguments.
    FontCreated = 1,
    /// A font's `LOGFONT` read from GDI: the stock font, and fonts created
    /// before the takeover.
    FontDefined = 2,
    /// Font slot +0x0C `release` (its HFONT is deleted).
    FontReleased = 3,
    /// Image slot +0x08 `clear`: the image's DC is gone; the next one
    /// starts with GDI's defaults.
    ImageReset = 4,
    /// Image slots +0xA8 `select_font` (font) and +0xAC `deselect_font`
    /// (font 0, the stock font).
    SelectFont = 5,
    /// Image slots +0xB0/+0xB4: the `COLORREF` passed to `SetTextColor`.
    TextColor = 6,
    /// An image's font and colour read from its DC (an image whose DC
    /// predates the takeover).
    DcState = 7,
    /// Image slot +0xB8 `text_out(x, y, text, len)`.
    Text = 8,
    /// Image slot +0x44 (`FillCall::Fill`).
    Fill = 9,
    /// Image slot +0x48 (`FillCall::Shade`).
    Shade = 10,
    /// Image slot +0x58 (`FillCall::ReplaceColor`).
    ReplaceColor = 11,
    /// Image slot +0x64 (`FillCall::Line`).
    Line = 12,
    /// The fill group's other slots (`FillCall::Other`).
    FillOther = 13,
}

impl Op {
    fn from_u16(v: u16) -> Option<Op> {
        Some(match v {
            1 => Op::FontCreated,
            2 => Op::FontDefined,
            3 => Op::FontReleased,
            4 => Op::ImageReset,
            5 => Op::SelectFont,
            6 => Op::TextColor,
            7 => Op::DcState,
            8 => Op::Text,
            9 => Op::Fill,
            10 => Op::Shade,
            11 => Op::ReplaceColor,
            12 => Op::Line,
            13 => Op::FillOther,
            _ => return None,
        })
    }
}

/// One decoded record, borrowing its tail from the buffer.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Record<'a> {
    FontCreated { font: FontId, height: i32, flags: u32, face: &'a [u8] },
    /// `style`: bit 0 italic, bit 1 underline.
    FontDefined { font: FontId, height: i32, weight: i32, style: u32, face: &'a [u8] },
    FontReleased { font: FontId },
    ImageReset { image: ImageId },
    SelectFont { image: ImageId, font: FontId },
    TextColor { image: ImageId, colorref: u32 },
    DcState { image: ImageId, colorref: u32, height: i32, weight: i32, style: u32, face: &'a [u8] },
    /// `owner`: the image's back pointer (+0x80), `clip` its clip
    /// rectangle (+0x44), both as they were at the call: until the surface
    /// group is ours, the consumer cannot know them otherwise.
    Text { image: ImageId, owner: u32, x: i32, y: i32, clip: Rect, bytes: &'a [u8] },
    Fill(FillCall),
}

/// The longest tail a record keeps (`text_out` lengths are clamped to it,
/// as the import hook it replaces did).
pub const MAX_TAIL: usize = 1024;

/// Appends records.
#[derive(Default, Debug, Clone)]
pub struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub fn new() -> Writer {
        Writer { bytes: Vec::with_capacity(4096) }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn clear(&mut self) {
        self.bytes.clear();
    }

    fn put(&mut self, op: Op, fields: &[u32], tail: &[u8]) {
        let tail = &tail[..tail.len().min(MAX_TAIL)];
        let words = 1 + fields.len() + tail.len().div_ceil(4);
        self.bytes.reserve(words * 4);
        self.bytes.extend_from_slice(&(op as u16).to_le_bytes());
        self.bytes.extend_from_slice(&(words as u16).to_le_bytes());
        for f in fields {
            self.bytes.extend_from_slice(&f.to_le_bytes());
        }
        self.bytes.extend_from_slice(tail);
        self.bytes.resize(self.bytes.len() + (4 - tail.len() % 4) % 4, 0);
    }

    pub fn push(&mut self, r: &Record<'_>) {
        match *r {
            Record::FontCreated { font, height, flags, face } => {
                self.put(Op::FontCreated, &[font.0, height as u32, flags, tail_len(face)], face)
            }
            Record::FontDefined { font, height, weight, style, face } => {
                self.put(Op::FontDefined, &[font.0, height as u32, weight as u32, style, tail_len(face)], face)
            }
            Record::FontReleased { font } => self.put(Op::FontReleased, &[font.0], &[]),
            Record::ImageReset { image } => self.put(Op::ImageReset, &[image.0], &[]),
            Record::SelectFont { image, font } => self.put(Op::SelectFont, &[image.0, font.0], &[]),
            Record::TextColor { image, colorref } => self.put(Op::TextColor, &[image.0, colorref], &[]),
            Record::DcState { image, colorref, height, weight, style, face } => self.put(
                Op::DcState,
                &[image.0, colorref, height as u32, weight as u32, style, tail_len(face)],
                face,
            ),
            Record::Text { image, owner, x, y, clip, bytes } => self.put(
                Op::Text,
                &[image.0, owner, x as u32, y as u32, clip.left as u32, clip.top as u32, clip.right as u32, clip.bottom as u32, tail_len(bytes)],
                bytes,
            ),
            Record::Fill(call) => self.fill(&call),
        }
    }

    fn fill(&mut self, call: &FillCall) {
        let mut f = Fields::new();
        let op = match *call {
            FillCall::Fill { target, rect, color } => {
                f.surface(target).opt_rect(rect).push(color);
                Op::Fill
            }
            FillCall::Shade { target, rect, color, percent } => {
                f.surface(target).opt_rect(rect).push(color).push(percent);
                Op::Shade
            }
            FillCall::ReplaceColor { target, rect, from, to } => {
                f.surface(target).rect(rect).push(from).push(to);
                Op::ReplaceColor
            }
            FillCall::Line { target, from, to, color, flag } => {
                f.surface(target).point(from).point(to).push(color).push(flag);
                Op::Line
            }
            FillCall::Other { slot, target, args } => {
                f.push(slot as u32).surface(target).words(&args);
                Op::FillOther
            }
        };
        self.put(op, f.get(), &[]);
    }

}

/// The fixed fields of a drawing record, without allocating.
struct Fields {
    words: [u32; 32],
    len: usize,
}

impl Fields {
    fn new() -> Fields {
        Fields { words: [0; 32], len: 0 }
    }

    fn get(&self) -> &[u32] {
        &self.words[..self.len]
    }

    fn push(&mut self, w: u32) -> &mut Fields {
        self.words[self.len] = w;
        self.len += 1;
        self
    }

    fn words(&mut self, ws: &[u32]) -> &mut Fields {
        ws.iter().for_each(|&w| {
            self.push(w);
        });
        self
    }

    fn point(&mut self, (x, y): (i32, i32)) -> &mut Fields {
        self.push(x as u32).push(y as u32)
    }

    fn rect(&mut self, r: Rect) -> &mut Fields {
        self.words(&r.to_array().map(|v| v as u32))
    }

    fn opt_rect(&mut self, r: Option<Rect>) -> &mut Fields {
        self.push(u32::from(r.is_some())).rect(r.unwrap_or_default())
    }

    fn surface(&mut self, s: Surface) -> &mut Fields {
        self.push(s.image.0).push(s.owner).rect(s.clip).rect(s.bounds)
    }
}

fn tail_len(tail: &[u8]) -> u32 {
    tail.len().min(MAX_TAIL) as u32
}

/// A malformed buffer (truncated, or a length that does not fit its op).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    /// Byte offset of the bad record.
    pub at: usize,
}

/// Reads the records of a buffer in order. Unknown ops are skipped by their
/// length; a malformed record ends the iteration with an error.
pub struct Records<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Records<'a> {
    pub fn new(bytes: &'a [u8]) -> Records<'a> {
        Records { bytes, at: 0 }
    }
}

impl<'a> Iterator for Records<'a> {
    type Item = Result<Record<'a>, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.at >= self.bytes.len() {
                return None;
            }
            let at = self.at;
            let bad = || Some(Err(DecodeError { at }));
            let Some(head) = self.bytes.get(at..at + 4) else {
                self.at = self.bytes.len();
                return bad();
            };
            let op = u16::from_le_bytes([head[0], head[1]]);
            let words = u16::from_le_bytes([head[2], head[3]]) as usize;
            let Some(body) = (words >= 1).then(|| self.bytes.get(at + 4..at + words * 4)).flatten() else {
                self.at = self.bytes.len();
                return bad();
            };
            self.at = at + words * 4;
            let Some(op) = Op::from_u16(op) else { continue };
            return Some(decode(op, body).ok_or(DecodeError { at }));
        }
    }
}

fn decode(op: Op, body: &[u8]) -> Option<Record<'_>> {
    let n = |k: usize| -> Option<u32> { body.get(k * 4..k * 4 + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])) };
    let tail = |fields: usize| -> Option<&[u8]> {
        let len = n(fields - 1)? as usize;
        let start = fields * 4;
        (len <= MAX_TAIL).then(|| body.get(start..start + len)).flatten()
    };
    Some(match op {
        Op::FontCreated => Record::FontCreated { font: FontId(n(0)?), height: n(1)? as i32, flags: n(2)?, face: tail(4)? },
        Op::FontDefined => Record::FontDefined { font: FontId(n(0)?), height: n(1)? as i32, weight: n(2)? as i32, style: n(3)?, face: tail(5)? },
        Op::FontReleased => Record::FontReleased { font: FontId(n(0)?) },
        Op::ImageReset => Record::ImageReset { image: ImageId(n(0)?) },
        Op::SelectFont => Record::SelectFont { image: ImageId(n(0)?), font: FontId(n(1)?) },
        Op::TextColor => Record::TextColor { image: ImageId(n(0)?), colorref: n(1)? },
        Op::DcState => Record::DcState {
            image: ImageId(n(0)?),
            colorref: n(1)?,
            height: n(2)? as i32,
            weight: n(3)? as i32,
            style: n(4)?,
            face: tail(6)?,
        },
        Op::Text => Record::Text {
            image: ImageId(n(0)?),
            owner: n(1)?,
            x: n(2)? as i32,
            y: n(3)? as i32,
            clip: Rect::new(n(4)? as i32, n(5)? as i32, n(6)? as i32, n(7)? as i32),
            bytes: tail(9)?,
        },
        Op::Fill | Op::Shade | Op::ReplaceColor | Op::Line | Op::FillOther => Record::Fill(fill(op, &mut Cursor { body, at: 0 })?),
    })
}

/// Reads a drawing record's fields in order.
struct Cursor<'a> {
    body: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn word(&mut self) -> Option<u32> {
        let b = self.body.get(self.at..self.at + 4)?;
        self.at += 4;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn int(&mut self) -> Option<i32> {
        self.word().map(|w| w as i32)
    }

    fn point(&mut self) -> Option<(i32, i32)> {
        Some((self.int()?, self.int()?))
    }

    fn rect(&mut self) -> Option<Rect> {
        Some(Rect::new(self.int()?, self.int()?, self.int()?, self.int()?))
    }

    fn opt_rect(&mut self) -> Option<Option<Rect>> {
        let some = self.word()? != 0;
        let r = self.rect()?;
        Some(some.then_some(r))
    }

    fn surface(&mut self) -> Option<Surface> {
        Some(Surface { image: ImageId(self.word()?), owner: self.word()?, clip: self.rect()?, bounds: self.rect()? })
    }

    fn words<const N: usize>(&mut self) -> Option<[u32; N]> {
        let mut out = [0; N];
        for w in &mut out {
            *w = self.word()?;
        }
        Some(out)
    }
}

fn fill(op: Op, c: &mut Cursor<'_>) -> Option<FillCall> {
    Some(match op {
        Op::Fill => FillCall::Fill { target: c.surface()?, rect: c.opt_rect()?, color: c.word()? },
        Op::Shade => FillCall::Shade { target: c.surface()?, rect: c.opt_rect()?, color: c.word()?, percent: c.word()? },
        Op::ReplaceColor => FillCall::ReplaceColor { target: c.surface()?, rect: c.rect()?, from: c.word()?, to: c.word()? },
        Op::Line => FillCall::Line { target: c.surface()?, from: c.point()?, to: c.point()?, color: c.word()?, flag: c.word()? },
        Op::FillOther => FillCall::Other { slot: c.word()? as u16, target: c.surface()?, args: c.words::<MAX_ARGS>()? },
        _ => return None,
    })
}


#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Record<'static>> {
        vec![
            Record::FontCreated { font: FontId(0x1000), height: 14, flags: 1, face: b"Times New Roman" },
            Record::FontDefined { font: FontId::STOCK, height: -16, weight: 700, style: 0, face: b"System" },
            Record::FontReleased { font: FontId(0x1000) },
            Record::ImageReset { image: ImageId(0x2000) },
            Record::SelectFont { image: ImageId(0x2000), font: FontId(0x1000) },
            Record::TextColor { image: ImageId(0x2000), colorref: 0x10FF_0000 },
            Record::DcState { image: ImageId(0x2004), colorref: 0x00FF_FFFF, height: -12, weight: 0, style: 3, face: b"Arial" },
            Record::Text { image: ImageId(0x2000), owner: 0x3000, x: -5, y: 40, clip: Rect::new(0, 0, 640, 480), bytes: b"Rome (3)" },
            Record::Text { image: ImageId(0x2000), owner: 0x3000, x: 0, y: 0, clip: Rect::default(), bytes: b"" },
            Record::Fill(FillCall::Fill { target: surface(0x2000), rect: None, color: 0x8000_7C1F }),
            Record::Fill(FillCall::Fill { target: surface(0x2000), rect: Some(Rect::new(1, 2, 30, 40)), color: 5 }),
            Record::Fill(FillCall::Shade { target: surface(0x2000), rect: Some(Rect::new(-1, 2, 3, 4)), color: 0x8000_0000, percent: 60 }),
            Record::Fill(FillCall::ReplaceColor { target: surface(0x2000), rect: Rect::new(0, 0, 9, 9), from: 7, to: 8 }),
            Record::Fill(FillCall::Line { target: surface(0x2000), from: (-3, 4), to: (50, 60), color: 0x1F, flag: 1 }),
            Record::Fill(FillCall::Other { slot: 0x60, target: surface(0x2000), args: [1, 2, 3, 4, 5, 6, 7, 8, 9] }),
        ]
    }

    fn surface(image: u32) -> Surface {
        Surface { image: ImageId(image), owner: image + 0x10, clip: Rect::new(0, 0, 640, 480), bounds: Rect::new(0, 0, 800, 600) }
    }

    #[test]
    fn records_round_trip() {
        let mut w = Writer::new();
        all().iter().for_each(|r| w.push(r));
        assert_eq!(w.bytes().len() % 4, 0);
        let back: Vec<_> = Records::new(w.bytes()).collect::<Result<_, _>>().unwrap();
        assert_eq!(back, all());
    }

    #[test]
    fn sizes_are_as_documented() {
        let size = |r: Record| {
            let mut w = Writer::new();
            w.push(&r);
            w.bytes().len()
        };
        assert_eq!(size(Record::SelectFont { image: ImageId(1), font: FontId(2) }), 12);
        assert_eq!(size(Record::Text { image: ImageId(1), owner: 0, x: 0, y: 0, clip: Rect::default(), bytes: b"abcde" }), 40 + 8);
    }

    #[test]
    fn long_text_is_clamped() {
        let long = vec![b'a'; 3000];
        let mut w = Writer::new();
        w.push(&Record::Text { image: ImageId(1), owner: 0, x: 0, y: 0, clip: Rect::default(), bytes: &long });
        let Some(Ok(Record::Text { bytes, .. })) = Records::new(w.bytes()).next() else { panic!() };
        assert_eq!(bytes.len(), MAX_TAIL);
    }

    #[test]
    fn unknown_ops_are_skipped_and_truncation_is_an_error() {
        let mut w = Writer::new();
        w.push(&Record::ImageReset { image: ImageId(7) });
        let mut bytes = vec![0x99, 0, 2, 0, 1, 2, 3, 4];
        bytes.extend_from_slice(w.bytes());
        let r: Vec<_> = Records::new(&bytes).collect();
        assert_eq!(r, vec![Ok(Record::ImageReset { image: ImageId(7) })]);
        let cut = &w.bytes()[..6];
        assert_eq!(Records::new(cut).collect::<Vec<_>>(), vec![Err(DecodeError { at: 0 })]);
    }
}
