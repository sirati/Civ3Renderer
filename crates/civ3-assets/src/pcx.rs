//! ZSoft PCX, as used by Civilization III: version 5, 8 bits per pixel, one
//! plane, RLE encoded, with a 256-colour palette appended to the file.

use crate::{Palette, Rgb};

#[derive(Debug, thiserror::Error)]
pub enum PcxError {
    #[error("not a PCX file")]
    NotPcx,
    #[error("unsupported PCX format: {bits} bpp × {planes} planes")]
    Unsupported { bits: u8, planes: u8 },
    #[error("PCX data ends early")]
    Truncated,
    #[error("PCX has no 256-colour palette")]
    NoPalette,
}

/// A decoded 8-bit paletted image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Indexed {
    pub width: u32,
    pub height: u32,
    /// Row-major palette indices.
    pub pixels: Vec<u8>,
    pub palette: Palette,
}

impl Indexed {
    pub fn get(&self, x: u32, y: u32) -> u8 {
        self.pixels[(y * self.width + x) as usize]
    }

    /// The `w × h` region at (`x`, `y`), keeping the palette.
    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> Indexed {
        let mut pixels = Vec::with_capacity((w * h) as usize);
        for row in y..y + h {
            let start = (row * self.width + x) as usize;
            pixels.extend_from_slice(&self.pixels[start..start + w as usize]);
        }
        Indexed { width: w, height: h, pixels, palette: self.palette.clone() }
    }
}

pub fn decode(bytes: &[u8]) -> Result<Indexed, PcxError> {
    if bytes.len() < 128 || bytes[0] != 0x0A || bytes[2] != 1 {
        return Err(PcxError::NotPcx);
    }
    let bits = bytes[3];
    let planes = bytes[65];
    if bits != 8 || planes != 1 {
        return Err(PcxError::Unsupported { bits, planes });
    }
    let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]) as u32;
    let (xmin, ymin, xmax, ymax) = (word(4), word(6), word(8), word(10));
    let width = xmax - xmin + 1;
    let height = ymax - ymin + 1;
    // Scanlines are padded to an even byte count.
    let stride = word(66).max(width) as usize;

    let mut pixels = Vec::with_capacity((width * height) as usize);
    let mut line = Vec::with_capacity(stride);
    let mut at = 128;
    for _ in 0..height {
        line.clear();
        while line.len() < stride {
            let b = *bytes.get(at).ok_or(PcxError::Truncated)?;
            at += 1;
            if b & 0xC0 == 0xC0 {
                let value = *bytes.get(at).ok_or(PcxError::Truncated)?;
                at += 1;
                // A run may spill over the end of the line; the excess is
                // dropped, as real encoders never produce it for 8 bpp.
                let n = ((b & 0x3F) as usize).min(stride - line.len());
                line.extend(std::iter::repeat_n(value, n));
            } else {
                line.push(b);
            }
        }
        pixels.extend_from_slice(&line[..width as usize]);
    }

    // The palette is the last 769 bytes: a 0x0C marker and 256 RGB triples.
    let tail = bytes.len().checked_sub(769).ok_or(PcxError::NoPalette)?;
    if tail < at || bytes[tail] != 0x0C {
        return Err(PcxError::NoPalette);
    }
    let mut colors = [Rgb::default(); 256];
    for (i, c) in colors.iter_mut().enumerate() {
        let p = tail + 1 + i * 3;
        *c = Rgb(bytes[p], bytes[p + 1], bytes[p + 2]);
    }
    Ok(Indexed { width, height, pixels, palette: Palette(colors) })
}

/// Encodes an image as a PCX (used for tests and tools).
pub fn encode(img: &Indexed) -> Vec<u8> {
    let stride = img.width.next_multiple_of(2) as usize;
    let mut out = vec![0u8; 128];
    out[0] = 0x0A;
    out[1] = 5;
    out[2] = 1;
    out[3] = 8;
    out[8..10].copy_from_slice(&((img.width - 1) as u16).to_le_bytes());
    out[10..12].copy_from_slice(&((img.height - 1) as u16).to_le_bytes());
    out[65] = 1;
    out[66..68].copy_from_slice(&(stride as u16).to_le_bytes());
    out[68] = 1;
    for y in 0..img.height {
        let mut row: Vec<u8> = (0..img.width).map(|x| img.get(x, y)).collect();
        row.resize(stride, 0);
        let mut i = 0;
        while i < row.len() {
            let v = row[i];
            let mut n = 1;
            while i + n < row.len() && row[i + n] == v && n < 63 {
                n += 1;
            }
            if n > 1 || v & 0xC0 == 0xC0 {
                out.push(0xC0 | n as u8);
            }
            out.push(v);
            i += n;
        }
    }
    out.push(0x0C);
    for c in &img.palette.0 {
        out.extend_from_slice(&[c.0, c.1, c.2]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_odd_widths_runs_and_high_values() {
        let mut palette = Palette::default();
        for (i, c) in palette.0.iter_mut().enumerate() {
            *c = Rgb(i as u8, 255 - i as u8, (i * 7) as u8);
        }
        for (w, h) in [(1, 1), (3, 2), (128, 64), (131, 5)] {
            let pixels = (0..w * h).map(|i| if i % 5 < 3 { 0xC7 } else { (i * 13) as u8 }).collect();
            let img = Indexed { width: w, height: h, pixels, palette: palette.clone() };
            assert_eq!(decode(&encode(&img)).unwrap(), img);
        }
    }
}
