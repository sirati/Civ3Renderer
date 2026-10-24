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

use crate::blit::{BlitCall, MAX_BLIT_ARGS, Scale, SpriteCall, SpriteId, SpriteRef, TableCall, TableId, TableRef};
use crate::draw::{CopyCall, FillCall, MAX_ARGS, Surface};
use crate::present::PresentCall;
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
    /// Image slot +0x40 (`CopyCall::Stretch`).
    CopyStretch = 14,
    /// Image slot +0x84 (`CopyCall::Keyed`).
    CopyKeyed = 15,
    /// Image slot +0x38 (`CopyCall::Indexed`).
    CopyIndexed = 16,
    /// The copy group's other slots (`CopyCall::Other`).
    CopyOther = 17,
    /// A sprite slot of +0x38..+0xA0 (`BlitCall`).
    Blit = 18,
    /// Sprite +0x04 (`SpriteCall::Init`).
    SpriteInit = 19,
    /// Sprite +0x0C (`SpriteCall::Duplicate`).
    SpriteDuplicate = 20,
    /// Sprite +0x10 (`SpriteCall::SetTable`).
    SpriteSetTable = 21,
    /// Sprite +0x18 (`SpriteCall::SetKey`).
    SpriteSetKey = 22,
    /// Sprite +0x08 and +0x00 (`SpriteCall::Clear`).
    SpriteClear = 25,
    /// Graphics +0x9C `set_scale`, or the scale read from jgl when it was
    /// set before the takeover.
    SetScale = 23,
    /// A colour table slot (`TableCall`): the call's slot in the first field.
    Table = 24,
    /// Graphics +0x30 (`PresentCall::Invalidate`).
    Invalidate = 26,
    /// Graphics +0xA4 (`PresentCall::Present`).
    Present = 27,
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
            14 => Op::CopyStretch,
            15 => Op::CopyKeyed,
            16 => Op::CopyIndexed,
            17 => Op::CopyOther,
            18 => Op::Blit,
            19 => Op::SpriteInit,
            20 => Op::SpriteDuplicate,
            21 => Op::SpriteSetTable,
            22 => Op::SpriteSetKey,
            23 => Op::SetScale,
            24 => Op::Table,
            25 => Op::SpriteClear,
            26 => Op::Invalidate,
            27 => Op::Present,
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
    Copy(CopyCall),
    Blit(BlitCall),
    Sprite(SpriteCall),
    SetScale(Scale),
    Table(TableCall),
    Present(PresentCall),
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
            Record::Copy(call) => self.copy(&call),
            Record::Blit(call) => self.blit(&call),
            Record::Sprite(call) => self.sprite(&call),
            Record::SetScale(s) => {
                let mut f = Fields::new();
                f.scale(s);
                self.put(Op::SetScale, f.get(), &[]);
            }
            Record::Table(call) => self.table(&call),
            Record::Present(call) => self.present(&call),
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

    fn copy(&mut self, call: &CopyCall) {
        let mut f = Fields::new();
        let op = match *call {
            CopyCall::Stretch { source, target, src, dst } => {
                f.surface(source).surface(target).rect(src).rect(dst);
                Op::CopyStretch
            }
            CopyCall::Keyed { source, target, x, y } => {
                f.surface(source).surface(target).point((x, y));
                Op::CopyKeyed
            }
            CopyCall::Indexed { source, target, src, at, size, mode } => {
                f.surface(source).surface(target).point(src).point(at).point(size).push(mode);
                Op::CopyIndexed
            }
            CopyCall::Other { slot, this, args } => {
                f.push(slot as u32).surface(this).words(&args);
                Op::CopyOther
            }
        };
        self.put(op, f.get(), &[]);
    }

    fn blit(&mut self, c: &BlitCall) {
        let mut f = Fields::new();
        f.push(c.slot as u32).push(c.caller).sprite(c.sprite).opt_surface(c.target).opt_surface(c.other);
        f.push(u32::from(c.mask.is_some())).sprite(c.mask.unwrap_or_default());
        f.point(c.at).table(c.table).scale(c.scale).words(&c.args);
        self.put(Op::Blit, f.get(), &[]);
    }

    fn sprite(&mut self, c: &SpriteCall) {
        let mut f = Fields::new();
        let op = match *c {
            SpriteCall::Init { sprite, pixels, size, depth, a5, table, compile } => {
                f.sprite(sprite).push(pixels).point(size).push(depth).push(a5).push(table.0).push(u32::from(compile));
                Op::SpriteInit
            }
            SpriteCall::Duplicate { sprite, source } => {
                f.sprite(sprite).sprite(source);
                Op::SpriteDuplicate
            }
            SpriteCall::SetTable { sprite, table } => {
                f.sprite(sprite).push(table.0);
                Op::SpriteSetTable
            }
            SpriteCall::SetKey { sprite, key } => {
                f.sprite(sprite).push(key);
                Op::SpriteSetKey
            }
            SpriteCall::Clear { sprite } => {
                f.sprite(sprite);
                Op::SpriteClear
            }
        };
        self.put(op, f.get(), &[]);
    }

    fn table(&mut self, c: &TableCall) {
        let mut f = Fields::new();
        match *c {
            TableCall::Init { table } => f.push(0x04).table(table),
            TableCall::ResetFade { table } => f.push(0x08).table(table),
            TableCall::Animate { table } => f.push(0x0C).table(table),
            TableCall::SetColors { table, first, count, colors } => f.push(0x14).table(table).push(first).push(count).push(colors),
            TableCall::Slot20 { table, arg } => f.push(0x20).table(table).push(arg),
        };
        self.put(Op::Table, f.get(), &[]);
    }

    fn present(&mut self, c: &PresentCall) {
        let mut f = Fields::new();
        let op = match *c {
            PresentCall::Invalidate { rect } => {
                f.opt_rect(rect);
                Op::Invalidate
            }
            PresentCall::Present { rect, back } => {
                f.opt_rect(rect).opt_surface(back);
                Op::Present
            }
        };
        self.put(op, f.get(), &[]);
    }
}

/// The fixed fields of a drawing record), without allocating.
struct Fields {
    words: [u32; 64],
    len: usize,
}

impl Fields {
    fn new() -> Fields {
        Fields { words: [0; 64], len: 0 }
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

    fn opt_surface(&mut self, s: Option<Surface>) -> &mut Fields {
        self.push(u32::from(s.is_some())).surface(s.unwrap_or_default())
    }

    fn sprite(&mut self, s: SpriteRef) -> &mut Fields {
        self.push(s.sprite.0).push(s.owner).push(s.table.0).push(s.bits).push(u32::from(s.compiled) | u32::from(s.placeholder) << 1)
    }

    fn table(&mut self, t: TableRef) -> &mut Fields {
        self.push(t.table.0).push(t.owner)
    }

    fn scale(&mut self, s: Scale) -> &mut Fields {
        self.push(s.num_x).push(s.num_y).push(s.den).push(s.set_at)
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
        Op::CopyStretch | Op::CopyKeyed | Op::CopyIndexed | Op::CopyOther => Record::Copy(copy(op, &mut Cursor { body, at: 0 })?),
        Op::Blit => Record::Blit(blit(&mut Cursor { body, at: 0 })?),
        Op::SpriteInit | Op::SpriteDuplicate | Op::SpriteSetTable | Op::SpriteSetKey | Op::SpriteClear => Record::Sprite(sprite(op, &mut Cursor { body, at: 0 })?),
        Op::SetScale => Record::SetScale(Cursor { body, at: 0 }.scale()?),
        Op::Table => Record::Table(table(&mut Cursor { body, at: 0 })?),
        Op::Invalidate | Op::Present => Record::Present(present(op, &mut Cursor { body, at: 0 })?),
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

    fn opt_surface(&mut self) -> Option<Option<Surface>> {
        let some = self.word()? != 0;
        let s = self.surface()?;
        Some(some.then_some(s))
    }

    fn sprite(&mut self) -> Option<SpriteRef> {
        Some(SpriteRef { sprite: SpriteId(self.word()?), owner: self.word()?, table: TableId(self.word()?), bits: self.word()?, compiled: false, placeholder: false }).and_then(|s| {
            let w = self.word()?;
            Some(SpriteRef { compiled: w & 1 != 0, placeholder: w & 2 != 0, ..s })
        })
    }

    fn table(&mut self) -> Option<TableRef> {
        Some(TableRef { table: TableId(self.word()?), owner: self.word()? })
    }

    fn scale(&mut self) -> Option<Scale> {
        Some(Scale { num_x: self.word()?, num_y: self.word()?, den: self.word()?, set_at: self.word()? })
    }

    fn words<const N: usize>(&mut self) -> Option<[u32; N]> {
        let mut out = [0; N];
        for w in &mut out {
            *w = self.word()?;
        }
        Some(out)
    }
}

fn present(op: Op, c: &mut Cursor<'_>) -> Option<PresentCall> {
    Some(match op {
        Op::Invalidate => PresentCall::Invalidate { rect: c.opt_rect()? },
        Op::Present => PresentCall::Present { rect: c.opt_rect()?, back: c.opt_surface()? },
        _ => return None,
    })
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

fn copy(op: Op, c: &mut Cursor<'_>) -> Option<CopyCall> {
    Some(match op {
        Op::CopyStretch => CopyCall::Stretch { source: c.surface()?, target: c.surface()?, src: c.rect()?, dst: c.rect()? },
        Op::CopyKeyed => {
            let (source, target, (x, y)) = (c.surface()?, c.surface()?, c.point()?);
            CopyCall::Keyed { source, target, x, y }
        }
        Op::CopyIndexed => CopyCall::Indexed {
            source: c.surface()?,
            target: c.surface()?,
            src: c.point()?,
            at: c.point()?,
            size: c.point()?,
            mode: c.word()?,
        },
        Op::CopyOther => CopyCall::Other { slot: c.word()? as u16, this: c.surface()?, args: c.words::<4>()? },
        _ => return None,
    })
}

fn blit(c: &mut Cursor<'_>) -> Option<BlitCall> {
    let (slot, caller, sprite) = (c.word()? as u16, c.word()?, c.sprite()?);
    let (target, other) = (c.opt_surface()?, c.opt_surface()?);
    let mask = (c.word()? != 0, c.sprite()?);
    Some(BlitCall {
        slot,
        caller,
        sprite,
        target,
        other,
        mask: mask.0.then_some(mask.1),
        at: c.point()?,
        table: c.table()?,
        scale: c.scale()?,
        args: c.words::<MAX_BLIT_ARGS>()?,
    })
}

fn sprite(op: Op, c: &mut Cursor<'_>) -> Option<SpriteCall> {
    let sprite = c.sprite()?;
    Some(match op {
        Op::SpriteInit => {
            let (pixels, size, depth, a5, table) = (c.word()?, c.point()?, c.word()?, c.word()?, TableId(c.word()?));
            SpriteCall::Init { sprite, pixels, size, depth, a5, table, compile: c.word()? != 0 }
        }
        Op::SpriteDuplicate => SpriteCall::Duplicate { sprite, source: c.sprite()? },
        Op::SpriteSetTable => SpriteCall::SetTable { sprite, table: TableId(c.word()?) },
        Op::SpriteSetKey => SpriteCall::SetKey { sprite, key: c.word()? },
        Op::SpriteClear => SpriteCall::Clear { sprite },
        _ => return None,
    })
}

fn table(c: &mut Cursor<'_>) -> Option<TableCall> {
    let (slot, table) = (c.word()?, c.table()?);
    Some(match slot {
        0x04 => TableCall::Init { table },
        0x08 => TableCall::ResetFade { table },
        0x0C => TableCall::Animate { table },
        0x14 => TableCall::SetColors { table, first: c.word()?, count: c.word()?, colors: c.word()? },
        0x20 => TableCall::Slot20 { table, arg: c.word()? },
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
            Record::Copy(CopyCall::Stretch { source: surface(0x2100), target: surface(0x2000), src: Rect::new(0, 0, 10, 10), dst: Rect::new(5, 5, 25, 25) }),
            Record::Copy(CopyCall::Keyed { source: surface(0x2100), target: surface(0x2000), x: -7, y: 8 }),
            Record::Copy(CopyCall::Indexed { source: surface(0x2100), target: surface(0x2000), src: (1, 2), at: (3, 4), size: (5, 6), mode: 7 }),
            Record::Copy(CopyCall::Other { slot: 0x8C, this: surface(0x2100), args: [0x2000, 1, 2, 3] }),
            Record::Blit(BlitCall {
                slot: 0x88,
                caller: 0x5F_8954,
                sprite: SpriteRef { sprite: SpriteId(0x4000), owner: 0x4104, table: TableId(0x4200), bits: 8, compiled: true, placeholder: true },
                target: Some(surface(0x2000)),
                other: Some(surface(0x2100)),
                mask: None,
                at: (-3, 70),
                table: TableRef { table: TableId(0x4300), owner: 0x4400 },
                scale: Scale { num_x: 32, num_y: 32, den: 64, set_at: 0x5F_88EB },
                args: [1, 2, 3, 4, 5, 6, 7, 8, 9],
            }),
            Record::Blit(BlitCall {
                slot: 0x58,
                caller: 0,
                sprite: SpriteRef::default(),
                target: None,
                other: None,
                mask: Some(SpriteRef { sprite: SpriteId(0x4800), owner: 0x4904, table: TableId(0), bits: 16, compiled: false, placeholder: false }),
                at: (0, 0),
                table: TableRef::default(),
                scale: Scale::default(),
                args: [0; MAX_BLIT_ARGS],
            }),
            Record::Sprite(SpriteCall::Init { sprite: SpriteRef { sprite: SpriteId(0x4000), owner: 0x4104, table: TableId(0), bits: 0, compiled: false, placeholder: false }, pixels: 0x5000, size: (64, 48), depth: 8, a5: 1, table: TableId(0x4200), compile: true }),
            Record::Sprite(SpriteCall::Clear { sprite: SpriteRef::default() }),
            Record::Sprite(SpriteCall::Duplicate { sprite: SpriteRef::default(), source: SpriteRef { sprite: SpriteId(1), owner: 2, table: TableId(3), bits: 4, compiled: true, placeholder: true } }),
            Record::Sprite(SpriteCall::SetTable { sprite: SpriteRef::default(), table: TableId(0x4200) }),
            Record::Sprite(SpriteCall::SetKey { sprite: SpriteRef::default(), key: 0xFF }),
            Record::SetScale(Scale { num_x: 1, num_y: 2, den: 3, set_at: 4 }),
            Record::Table(TableCall::Init { table: TableRef { table: TableId(0x4300), owner: 0x4400 } }),
            Record::Table(TableCall::ResetFade { table: TableRef::default() }),
            Record::Table(TableCall::Animate { table: TableRef::default() }),
            Record::Table(TableCall::SetColors { table: TableRef::default(), first: 0, count: 256, colors: 0x6000 }),
            Record::Table(TableCall::Slot20 { table: TableRef::default(), arg: 7 }),
            Record::Present(PresentCall::Invalidate { rect: None }),
            Record::Present(PresentCall::Invalidate { rect: Some(Rect::new(0, 0, 1024, 768)) }),
            Record::Present(PresentCall::Present { rect: None, back: Some(surface(0x2200)) }),
            Record::Present(PresentCall::Present { rect: Some(Rect::new(3, 4, 5, 6)), back: None }),
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
