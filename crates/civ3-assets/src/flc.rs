//! Autodesk Animator Pro FLC (8-bit animation), as used for Civilization III
//! units and leaderheads.
//!
//! Every frame is decoded to a full image: FLC frames are deltas against the
//! previous one, so they are applied in order onto a running canvas.

use crate::{Palette, Rgb};

#[derive(Debug, thiserror::Error)]
pub enum FlcError {
    #[error("not an FLC/FLI file")]
    NotFlc,
    #[error("FLC data ends early")]
    Truncated,
    #[error("unsupported FLC colour depth {0}")]
    Depth(u16),
    #[error("frame {frame}, chunk type {chunk}: {source}")]
    InChunk { frame: usize, chunk: u16, source: Box<FlcError> },
}

/// One fully composed frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub pixels: Vec<u8>,
    /// Palette in effect for this frame.
    pub palette: Palette,
    /// Delay before the next frame in milliseconds (0 = use `Flc::speed_ms`).
    pub delay_ms: u16,
}

#[derive(Clone, Debug)]
pub struct Flc {
    pub width: u32,
    pub height: u32,
    /// Default delay between frames in milliseconds.
    pub speed_ms: u32,
    pub frames: Vec<Frame>,
    /// Frame count stated in the header (may be fewer than `frames`).
    pub header_frames: usize,
    /// The 128-byte file header, for format extensions stored in its
    /// reserved fields (Civilization III keeps animation layout there).
    pub header: [u8; 128],
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, FlcError> {
        let v = *self.bytes.get(self.at).ok_or(FlcError::Truncated)?;
        self.at += 1;
        Ok(v)
    }
    fn i8(&mut self) -> Result<i8, FlcError> {
        Ok(self.u8()? as i8)
    }
    fn u16(&mut self) -> Result<u16, FlcError> {
        Ok(u16::from_le_bytes([self.u8()?, self.u8()?]))
    }
    fn u32(&mut self) -> Result<u32, FlcError> {
        Ok(u32::from_le_bytes([self.u8()?, self.u8()?, self.u8()?, self.u8()?]))
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], FlcError> {
        let s = self.bytes.get(self.at..self.at + n).ok_or(FlcError::Truncated)?;
        self.at += n;
        Ok(s)
    }
}

const FLI_MAGIC: u16 = 0xAF11;
const FLC_MAGIC: u16 = 0xAF12;
const FRAME_CHUNK: u16 = 0xF1FA;

const COLOR_256: u16 = 4;
const DELTA_FLC: u16 = 7;
const COLOR_64: u16 = 11;
const DELTA_FLI: u16 = 12;
const BLACK: u16 = 13;
const BYTE_RUN: u16 = 15;
const FLI_COPY: u16 = 16;

/// The frame size and how many frames [`decode`] makes of the file at
/// `file`, from its header and the headers of its chunks (their bodies are
/// skipped, not read).
pub fn dimensions(file: &mut (impl std::io::Read + std::io::Seek)) -> Result<(u32, u32, usize), FlcError> {
    use std::io::SeekFrom;
    let mut header = [0u8; 128];
    file.read_exact(&mut header).map_err(|_| FlcError::NotFlc)?;
    let word = |at: usize| u16::from_le_bytes([header[at], header[at + 1]]);
    let magic = word(4);
    if magic != FLC_MAGIC && magic != FLI_MAGIC {
        return Err(FlcError::NotFlc);
    }
    let (width, height, depth) = (word(8) as u32, word(10) as u32, word(12));
    if depth != 8 && depth != 0 {
        return Err(FlcError::Depth(depth));
    }
    let len = file.seek(SeekFrom::End(0)).map_err(|_| FlcError::Truncated)?;
    let mut at = match u32::from_le_bytes(header[80..84].try_into().unwrap()) {
        off if magic == FLC_MAGIC && off != 0 => off as u64,
        _ => 128,
    };
    let mut frames = 0;
    // As `decode` walks them: every frame chunk is a frame.
    while at + 6 <= len {
        let mut chunk = [0u8; 6];
        file.seek(SeekFrom::Start(at)).and_then(|_| file.read_exact(&mut chunk)).map_err(|_| FlcError::Truncated)?;
        let size = u32::from_le_bytes(chunk[..4].try_into().unwrap()) as u64;
        if size < 6 {
            return Err(FlcError::Truncated);
        }
        if u16::from_le_bytes([chunk[4], chunk[5]]) == FRAME_CHUNK {
            frames += 1;
        }
        at += size;
    }
    Ok((width, height, frames))
}

pub fn decode(bytes: &[u8]) -> Result<Flc, FlcError> {
    if bytes.len() < 128 {
        return Err(FlcError::NotFlc);
    }
    let mut r = Reader { bytes, at: 4 };
    let magic = r.u16()?;
    if magic != FLC_MAGIC && magic != FLI_MAGIC {
        return Err(FlcError::NotFlc);
    }
    let frame_count = r.u16()? as usize;
    let width = r.u16()? as u32;
    let height = r.u16()? as u32;
    let depth = r.u16()?;
    if depth != 8 && depth != 0 {
        return Err(FlcError::Depth(depth));
    }
    let _flags = r.u16()?;
    let speed = r.u32()?;
    // FLI speed is in 1/70 s; FLC speed is in milliseconds.
    let speed_ms = if magic == FLI_MAGIC { speed * 1000 / 70 } else { speed };
    let mut header = [0u8; 128];
    header.copy_from_slice(&bytes[..128]);
    let first_frame = if magic == FLC_MAGIC {
        let off = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        if off == 0 { 128 } else { off }
    } else {
        128
    };

    let mut canvas = vec![0u8; (width * height) as usize];
    let mut palette = Palette::default();
    let mut frames = Vec::with_capacity(frame_count);
    r.at = first_frame;
    // Decode every frame chunk in the file: Civilization III files hold more
    // than the header's frame count (one extra per direction).
    while r.at + 6 <= bytes.len() {
        let start = r.at;
        let size = r.u32()? as usize;
        let kind = r.u16()?;
        if size < 6 {
            return Err(FlcError::Truncated);
        }
        if kind != FRAME_CHUNK {
            // Prefix chunks and the like: skip.
            r.at = start + size;
            continue;
        }
        let chunks = r.u16()?;
        let delay_ms = r.u16()?;
        r.at = start + 16;
        for _ in 0..chunks {
            let cstart = r.at;
            let declared = r.u32()? as usize;
            let ckind = r.u16()?;
            // Civilization III's exporter leaves the size of palette chunks
            // uninitialised (0xCDCDCDCD). A size running past the frame is
            // therefore treated as unknown: decode up to the frame end and
            // advance by what the chunk actually consumed.
            let frame_end = (start + size).min(bytes.len());
            let trusted = declared >= 6 && cstart + declared <= frame_end;
            let body_end = if trusted { cstart + declared } else { frame_end };
            let body = &bytes.get(cstart + 6..body_end).ok_or(FlcError::Truncated)?;
            let mut c = Reader { bytes: body, at: 0 };
            let decoded = match ckind {
                COLOR_256 => apply_colors(&mut c, &mut palette, |v| v),
                COLOR_64 => apply_colors(&mut c, &mut palette, |v| (v << 2) | (v >> 4)),
                BYTE_RUN => byte_run(&mut c, &mut canvas, width as usize, height as usize),
                DELTA_FLC => delta_flc(&mut c, &mut canvas, width as usize),
                DELTA_FLI => delta_fli(&mut c, &mut canvas, width as usize),
                BLACK => {
                    canvas.fill(0);
                    Ok(())
                }
                FLI_COPY => {
                    let n = canvas.len();
                    c.take(n).map(|d| canvas.copy_from_slice(d))
                }
                _ => Ok(()), // postage stamps, etc.
            };
            decoded.map_err(|e| FlcError::InChunk { frame: frames.len(), chunk: ckind, source: Box::new(e) })?;
            r.at = if trusted { cstart + declared } else { cstart + 6 + c.at };
        }
        r.at = start + size;
        frames.push(Frame { pixels: canvas.clone(), palette: palette.clone(), delay_ms });
    }
    Ok(Flc { width, height, speed_ms, header_frames: frame_count, frames, header })
}

fn apply_colors(c: &mut Reader, palette: &mut Palette, scale: impl Fn(u8) -> u8) -> Result<(), FlcError> {
    let packets = c.u16()?;
    let mut index = 0usize;
    for _ in 0..packets {
        index += c.u8()? as usize;
        let n = match c.u8()? {
            0 => 256,
            n => n as usize,
        };
        for _ in 0..n {
            let rgb = Rgb(scale(c.u8()?), scale(c.u8()?), scale(c.u8()?));
            if index < 256 {
                palette.0[index] = rgb;
            }
            index += 1;
        }
    }
    Ok(())
}

fn byte_run(c: &mut Reader, canvas: &mut [u8], width: usize, height: usize) -> Result<(), FlcError> {
    for y in 0..height {
        let _packets = c.u8()?;
        let line = &mut canvas[y * width..(y + 1) * width];
        let mut x = 0;
        while x < width {
            let n = c.i8()?;
            if n >= 0 {
                let v = c.u8()?;
                let n = (n as usize).min(width - x);
                line[x..x + n].fill(v);
                x += n;
            } else {
                let n = ((-(n as i32)) as usize).min(width - x);
                line[x..x + n].copy_from_slice(c.take(n)?);
                x += n;
            }
        }
    }
    Ok(())
}

/// Word-oriented delta (FLC chunk 7, "SS2").
fn delta_flc(c: &mut Reader, canvas: &mut [u8], width: usize) -> Result<(), FlcError> {
    let height = canvas.len() / width;
    let lines = c.u16()?;
    let mut y = 0usize;
    for _ in 0..lines {
        // Opcode words before the packet count: line skips, and the value of
        // the last pixel for odd widths.
        let packets = loop {
            let w = c.u16()?;
            match w >> 14 {
                0b11 => y += (-(w as i16) as i32) as usize,
                0b10 => {
                    if y < height {
                        canvas[y * width + width - 1] = w as u8;
                    }
                }
                _ => break w,
            }
        };
        let mut x = 0usize;
        for _ in 0..packets {
            x += c.u8()? as usize;
            let n = c.i8()?;
            if n >= 0 {
                let data = c.take(n as usize * 2)?;
                let at = y * width + x;
                canvas.get_mut(at..at + data.len()).ok_or(FlcError::Truncated)?.copy_from_slice(data);
                x += data.len();
            } else {
                let (a, b) = (c.u8()?, c.u8()?);
                for _ in 0..-(n as i32) {
                    let at = y * width + x;
                    *canvas.get_mut(at).ok_or(FlcError::Truncated)? = a;
                    *canvas.get_mut(at + 1).ok_or(FlcError::Truncated)? = b;
                    x += 2;
                }
            }
        }
        y += 1;
    }
    Ok(())
}

/// Byte-oriented delta (FLI chunk 12, "LC").
fn delta_fli(c: &mut Reader, canvas: &mut [u8], width: usize) -> Result<(), FlcError> {
    let mut y = c.u16()? as usize;
    let lines = c.u16()?;
    for _ in 0..lines {
        let packets = c.u8()?;
        let mut x = 0usize;
        for _ in 0..packets {
            x += c.u8()? as usize;
            let n = c.i8()?;
            let at = y * width + x;
            if n >= 0 {
                let data = c.take(n as usize)?;
                canvas.get_mut(at..at + data.len()).ok_or(FlcError::Truncated)?.copy_from_slice(data);
                x += data.len();
            } else {
                let v = c.u8()?;
                let n = (-(n as i32)) as usize;
                canvas.get_mut(at..at + n).ok_or(FlcError::Truncated)?.fill(v);
                x += n;
            }
        }
        y += 1;
    }
    Ok(())
}
