//! Operations on caller-written data buffers. These do not rasterise drawing
//! records. The host must identify data images before the shim calls them.

use crate::{Rect, object::{error, intersect}};

#[derive(Copy, Clone, Debug)]
pub struct Plane {
    pub width: i32,
    pub height: i32,
    /// Pixels per row, including padding.
    pub row: i32,
    pub bits: i32,
}

impl Plane {
    fn bytes_per_pixel(self) -> Result<usize, u32> {
        match self.bits {
            8 | 16 => Ok(self.bits as usize / 8),
            _ => Err(error::DEPTH),
        }
    }

    fn stride(self) -> Result<usize, u32> {
        if self.width < 0 || self.height < 0 || self.row < self.width {
            return Err(error::ARGUMENT);
        }
        (self.row as usize).checked_mul(self.bytes_per_pixel()?).ok_or(error::ARGUMENT)
    }

    fn fits(self, len: usize) -> Result<(), u32> {
        if self.stride()?.checked_mul(self.height as usize).is_none_or(|size| size > len) {
            Err(error::ARGUMENT)
        } else { Ok(()) }
    }

    fn bounds(self) -> Rect { Rect::new(0, 0, self.width, self.height) }
}

/// An equal-size, same-depth copy. Navigator buffers use this subset of
/// image +0x40; arbitrary stretching and depth conversion remain drawing.
#[derive(Copy, Clone, Debug)]
pub struct RowCopy {
    source: usize,
    target: usize,
    source_stride: usize,
    target_stride: usize,
    bytes: usize,
    rows: usize,
}

impl RowCopy {
    pub fn new(source: Plane, target: Plane, src: Rect, dst: Rect, clip: Rect) -> Result<Self, u32> {
        if source.bits != target.bits { return Err(error::DEPTH); }
        source.fits(usize::MAX)?;
        target.fits(usize::MAX)?;
        let bytes = source.bytes_per_pixel()?;
        let (source_stride, target_stride) = (source.stride()?, target.stride()?);
        let size = |r: Rect| (i64::from(r.right) - i64::from(r.left), i64::from(r.bottom) - i64::from(r.top));
        let (w, h) = size(src);
        if size(dst) != (w, h) || w < 0 || h < 0 { return Err(error::ARGUMENT); }
        let empty = Self { source: 0, target: 0, source_stride, target_stride, bytes: 0, rows: 0 };
        if w == 0 || h == 0 { return Ok(empty); }
        if intersect(src, source.bounds()) != Some(src) { return Err(error::ARGUMENT); }
        let Some(area) = intersect(dst, clip).and_then(|r| intersect(r, target.bounds())) else { return Ok(empty) };
        let sx = i64::from(src.left) + i64::from(area.left) - i64::from(dst.left);
        let sy = i64::from(src.top) + i64::from(area.top) - i64::from(dst.top);
        Ok(Self {
            source: sy as usize * source_stride + sx as usize * bytes,
            target: area.top as usize * target_stride + area.left as usize * bytes,
            source_stride, target_stride,
            bytes: (area.right - area.left) as usize * bytes,
            rows: (area.bottom - area.top) as usize,
        })
    }

    fn ranges(self, row: usize) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        let (s, t) = (self.source + row * self.source_stride, self.target + row * self.target_stride);
        (s..s + self.bytes, t..t + self.bytes)
    }

    fn fits(self, source: usize, target: usize) -> Result<(), u32> {
        if self.rows == 0 { return Ok(()); }
        let (s, t) = self.ranges(self.rows - 1);
        if s.end > source || t.end > target { Err(error::ARGUMENT) } else { Ok(()) }
    }

    pub fn copy(self, source: &[u8], target: &mut [u8]) -> Result<(), u32> {
        self.fits(source.len(), target.len())?;
        for row in 0..self.rows {
            let (s, t) = self.ranges(row);
            target[t].copy_from_slice(&source[s]);
        }
        Ok(())
    }

    pub fn copy_within(self, pixels: &mut [u8]) -> Result<(), u32> {
        self.fits(pixels.len(), pixels.len())?;
        for i in 0..self.rows {
            let row = if self.target > self.source { self.rows - 1 - i } else { i };
            let (s, t) = self.ranges(row);
            pixels.copy_within(s, t.start);
        }
        Ok(())
    }
}

/// Clear caller-written data, including a 16-bit navigator's black reset.
pub fn fill(pixels: &mut [u8], plane: Plane, area: Rect, color: u32) -> Result<(), u32> {
    plane.fits(pixels.len())?;
    let Some(area) = intersect(area, plane.bounds()) else { return Ok(()) };
    let (stride, bytes) = (plane.stride()?, plane.bytes_per_pixel()?);
    let color = color.to_le_bytes();
    for y in area.top..area.bottom {
        let at = y as usize * stride;
        let row = &mut pixels[at + area.left as usize * bytes..at + area.right as usize * bytes];
        for pixel in row.chunks_exact_mut(bytes) { pixel.copy_from_slice(&color[..bytes]); }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_copy_preserves_padding_and_clips_the_target_without_scaling() {
        let source = Plane { width: 3, height: 3, row: 4, bits: 16 };
        let target = Plane { width: 5, height: 4, row: 6, bits: 16 };
        let bytes: Vec<u8> = (0..24).collect();
        let mut output = [99; 48];
        RowCopy::new(source, target, source.bounds(), Rect::new(1, 1, 4, 4), Rect::new(2, 0, 5, 3)).unwrap().copy(&bytes, &mut output).unwrap();
        assert_eq!(&output[16..20], &bytes[2..6]);
        assert_eq!(&output[28..32], &bytes[10..14]);
        assert!(output[..16].iter().chain(&output[20..28]).chain(&output[32..]).all(|&b| b == 99));
        let before = output;
        assert!(RowCopy::new(source, target, source.bounds(), Rect::new(0, 0, 5, 4), target.bounds()).is_err());
        assert_eq!(output, before);
    }

    #[test]
    fn overlapping_data_moves_keep_the_source_and_fill_leaves_row_padding_alone() {
        let plane = Plane { width: 3, height: 4, row: 4, bits: 8 };
        let mut bytes: Vec<u8> = (0..16).collect();
        RowCopy::new(plane, plane, Rect::new(0, 0, 2, 3), Rect::new(1, 1, 3, 4), plane.bounds()).unwrap().copy_within(&mut bytes).unwrap();
        assert_eq!(bytes, [0,1,2,3, 4,0,1,7, 8,4,5,11, 12,8,9,15]);
        let plane = Plane { width: 3, height: 2, row: 4, bits: 16 };
        let mut bytes = [0xCD; 16];
        fill(&mut bytes, plane, Rect::new(1, 0, 3, 2), 0x8000_1234).unwrap();
        assert_eq!(bytes, [0xCD,0xCD,0x34,0x12,0x34,0x12,0xCD,0xCD, 0xCD,0xCD,0x34,0x12,0x34,0x12,0xCD,0xCD]);
        let before = bytes;
        assert!(fill(&mut bytes[..6], plane, plane.bounds(), 0).is_err());
        assert_eq!(bytes, before);
    }
}
