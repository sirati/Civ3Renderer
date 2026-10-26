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

/// The indexed image +0x38 copy. It replaces destination key pixels with
/// source pixels, preserving the source palette for an 8→16 target. The
/// object shim supplies the planes and byte slices; this module owns the
/// bounds and byte semantics.
#[derive(Copy, Clone, Debug)]
pub struct IndexedCopy {
    source: usize,
    target: usize,
    source_stride: usize,
    target_stride: usize,
    width: usize,
    height: usize,
    target_bits: i32,
    key8: u8,
    palette: [u16; 256],
}

impl IndexedCopy {
    #[allow(clippy::too_many_arguments)]
    pub fn new(source: Plane, target: Plane, src_x: i32, src_y: i32, dst_x: i32, dst_y: i32, width: i32, height: i32, clip: Rect, key: u32, palette: Option<&[u16; 256]>) -> Result<Self, u32> {
        if source.bits != 8 || !matches!(target.bits, 8 | 16) {
            return Err(error::DEPTH);
        }
        let source_stride = source.stride()?;
        let target_stride = target.stride()?;
        if width <= 0 || height <= 0 {
            return Ok(Self { source: 0, target: 0, source_stride, target_stride, width: 0, height: 0, target_bits: target.bits, key8: key as u8, palette: [0; 256] });
        }
        if target.bits == 16 && palette.is_none() {
            return Err(error::NULL);
        }
        let mut sx = i64::from(src_x);
        let mut sy = i64::from(src_y);
        let mut dx = i64::from(dst_x);
        let mut dy = i64::from(dst_y);
        let mut w = i64::from(width);
        let mut h = i64::from(height);
        let trim_left = (-sx).max(0);
        let trim_top = (-sy).max(0);
        sx += trim_left;
        dx += trim_left;
        w -= trim_left;
        sy += trim_top;
        dy += trim_top;
        h -= trim_top;
        if sx < i64::from(source.width) {
            w = w.min(i64::from(source.width) - sx);
        } else {
            w = 0;
        }
        if sy < i64::from(source.height) {
            h = h.min(i64::from(source.height) - sy);
        } else {
            h = 0;
        }
        let target_bounds = target.bounds();
        let clip = intersect(clip, target_bounds).unwrap_or_default();
        let clip_right = i64::from(clip.right);
        let clip_bottom = i64::from(clip.bottom);
        let clip_left = i64::from(clip.left);
        let clip_top = i64::from(clip.top);
        if dx < clip_left {
            let trim = clip_left - dx;
            dx = clip_left;
            sx += trim;
            w -= trim;
        }
        if dy < clip_top {
            let trim = clip_top - dy;
            dy = clip_top;
            sy += trim;
            h -= trim;
        }
        w = w.min(clip_right - dx);
        h = h.min(clip_bottom - dy);
        if w < 0 { w = 0; }
        if h < 0 { h = 0; }
        if sx < 0 {
            let trim = -sx;
            sx = 0;
            dx += trim;
            w -= trim;
        }
        if sy < 0 {
            let trim = -sy;
            sy = 0;
            dy += trim;
            h -= trim;
        }
        let width = w.max(0) as usize;
        let height = h.max(0) as usize;
        let source = sy.max(0) as usize * source_stride + sx.max(0) as usize;
        let target_at = dy.max(0) as usize * target_stride + dx.max(0) as usize * (target.bits as usize / 8);
        let mut table = [0u16; 256];
        if let Some(p) = palette { table = *p; }
        Ok(Self { source, target: target_at, source_stride, target_stride, width, height, target_bits: target.bits, key8: key as u8, palette: table })
    }

    fn fits(self, source_len: usize, target_len: usize) -> Result<(), u32> {
        if self.width == 0 || self.height == 0 { return Ok(()); }
        let source_end = self.source + (self.height - 1) * self.source_stride + self.width;
        let target_bytes = self.width * (self.target_bits as usize / 8);
        let target_end = self.target + (self.height - 1) * self.target_stride + target_bytes;
        if source_end > source_len || target_end > target_len { Err(error::ARGUMENT) } else { Ok(()) }
    }

    pub fn copy(self, source: &[u8], target: &mut [u8]) -> Result<(), u32> {
        self.fits(source.len(), target.len())?;
        for row in 0..self.height {
            let source_at = self.source + row * self.source_stride;
            let target_at = self.target + row * self.target_stride;
            let pixels = source[source_at..source_at + self.width].to_vec();
            if self.target_bits == 8 {
                for (i, &pixel) in pixels.iter().enumerate() {
                    let at = target_at + i;
                    if target[at] == self.key8 { target[at] = pixel; }
                }
            } else {
                for (i, &pixel) in pixels.iter().enumerate() {
                    let at = target_at + 2 * i;
                    let old = u16::from_le_bytes([target[at], target[at + 1]]);
                    if old == self.palette[self.key8 as usize] {
                        target[at..at + 2].copy_from_slice(&self.palette[pixel as usize].to_le_bytes());
                    }
                }
            }
        }
        Ok(())
    }
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

/// Image +0x58 `replace_color` on a caller-written buffer. The original
/// compares complete bytes for 8-bit images and complete little-endian words
/// for 16-bit images; row padding stays untouched.
pub fn replace(pixels: &mut [u8], plane: Plane, area: Rect, from: u32, to: u32) -> Result<(), u32> {
    plane.fits(pixels.len())?;
    let Some(area) = intersect(area, plane.bounds()) else { return Ok(()) };
    let (stride, bytes) = (plane.stride()?, plane.bytes_per_pixel()?);
    if !matches!(plane.bits, 8 | 16) { return Err(error::DEPTH); }
    for y in area.top..area.bottom {
        let at = y as usize * stride + area.left as usize * bytes;
        let row = &mut pixels[at..y as usize * stride + area.right as usize * bytes];
        if bytes == 1 {
            for pixel in row { if *pixel == from as u8 { *pixel = to as u8; } }
        } else {
            for pixel in row.chunks_exact_mut(2) {
                if u16::from_le_bytes([pixel[0], pixel[1]]) == from as u16 {
                    pixel.copy_from_slice(&(to as u16).to_le_bytes());
                }
            }
        }
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

    #[test]
    fn indexed_copy_replaces_only_the_key_and_maps_an_8_to_16_palette() {
        let source = Plane { width: 3, height: 2, row: 4, bits: 8 };
        let target = Plane { width: 5, height: 3, row: 6, bits: 8 };
        let source_bytes = [1, 2, 3, 0, 4, 5, 6, 0];
        let mut target_bytes = [9, 9, 9, 9, 9, 0, 9, 9, 255, 9, 9, 0, 9, 9, 9, 9, 9, 0];
        let copy = IndexedCopy::new(source, target, 0, 0, 1, 1, 3, 2, Rect::new(0, 0, 5, 3), 9, None).unwrap();
        copy.copy(&source_bytes, &mut target_bytes).unwrap();
        assert_eq!(target_bytes, [9, 9, 9, 9, 9, 0, 9, 1, 255, 3, 9, 0, 9, 4, 5, 6, 9, 0]);

        let target16 = Plane { width: 3, height: 1, row: 4, bits: 16 };
        let mut sixteen = [20, 0, 77, 0, 20, 0, 88, 0];
        let palette = std::array::from_fn(|i| (i as u16) * 10);
        let copy = IndexedCopy::new(source, target16, 0, 0, 0, 0, 3, 1, target16.bounds(), 2, Some(&palette)).unwrap();
        copy.copy(&source_bytes, &mut sixteen).unwrap();
        assert_eq!(sixteen, [10, 0, 77, 0, 30, 0, 88, 0]);
    }

    #[test]
    fn indexed_copy_can_snapshot_an_in_place_source_without_aliasing_references() {
        let plane = Plane { width: 4, height: 3, row: 4, bits: 8 };
        let mut pixels = [9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9];
        pixels[0..4].copy_from_slice(&[1, 2, 3, 4]);
        pixels[4..8].copy_from_slice(&[5, 6, 7, 8]);
        pixels[8..12].copy_from_slice(&[9, 10, 11, 12]);
        let copy = IndexedCopy::new(plane, plane, 0, 0, 0, 1, 4, 2, plane.bounds(), 9, None).unwrap();
        let source = pixels.to_vec();
        copy.copy(&source, &mut pixels).unwrap();
        assert_eq!(pixels, [1, 2, 3, 4, 5, 6, 7, 8, 5, 10, 11, 12]);
    }

    #[test]
    fn replace_changes_pixels_but_not_padding_at_8_and_16_bits() {
        let p8 = Plane { width: 3, height: 2, row: 4, bits: 8 };
        let mut bytes8 = [7, 1, 7, 99, 7, 2, 3, 99];
        replace(&mut bytes8, p8, Rect::new(0, 0, 3, 2), 7, 8).unwrap();
        assert_eq!(bytes8, [8, 1, 8, 99, 8, 2, 3, 99]);
        let p16 = Plane { width: 2, height: 1, row: 2, bits: 16 };
        let mut bytes16 = [0x34, 0x12, 0x34, 0x12];
        replace(&mut bytes16, p16, Rect::new(1, 0, 2, 1), 0x1234, 0x5678).unwrap();
        assert_eq!(bytes16, [0x34, 0x12, 0x78, 0x56]);
    }
}
