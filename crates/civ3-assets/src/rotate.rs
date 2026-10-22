//! Pre-rendered turns of a still image (an image's `rotations = <n>`): the
//! loading cursor's hourglass, which has no animation frames of its own,
//! turned in `n` equal clockwise steps.
//!
//! Each frame is a square of [`frame_side`] (the image's diagonal), the
//! image turned about the centre of its opaque pixels, which lands on the
//! frame's centre (to the nearest half pixel), so the hot spot of a frame
//! is its middle and the art spins in place. Every frame pixel averages
//! 4×4 nearest-texel samples (premultiplied by alpha): edges at an angle
//! are smooth while texels stay solid, and turns by whole right angles
//! (and none) are pixel-exact copies of the art.

use tile_render::RgbaImage;

/// Samples per frame pixel along each axis.
const SUPERSAMPLE: u32 = 4;

/// A turned frame's side for art `side` pixels wide: ⌈side·√2⌉ rounded up to
/// even (as `scene::loading_cursor::frame_side`).
pub fn frame_side(side: u32) -> u32 {
    let target = 2 * u64::from(side) * u64::from(side);
    let mut n = u64::from(side);
    while n * n < target {
        n += 1;
    }
    (n + (n & 1)) as u32
}

/// `image` turned clockwise by `k`·360°/`steps`, `k` in `0..steps`.
pub fn turned(image: &RgbaImage, steps: u32) -> Vec<RgbaImage> {
    let steps = steps.max(1);
    let side = frame_side(image.width.max(image.height));
    let opaque = (0..image.height).flat_map(|y| (0..image.width).map(move |x| (x, y))).filter(|&(x, y)| image.get(x, y)[3] != 0);
    let Some(bounds) = opaque.fold(None, |b: Option<[u32; 4]>, (x, y)| {
        Some(b.map_or([x, y, x, y], |[l, t, r, b]| [l.min(x), t.min(y), r.max(x), b.max(y)]))
    }) else {
        return (0..steps).map(|_| RgbaImage::new(side, side)).collect();
    };
    // The centre in pixel-corner coordinates: whole or half pixels. Equal
    // fractions on both axes keep quarter turns on the pixel grid.
    let (cx, mut cy) = (f64::from(bounds[0] + bounds[2] + 1) / 2.0, f64::from(bounds[1] + bounds[3] + 1) / 2.0);
    if cx.fract() != cy.fract() {
        cy += 0.5;
    }
    // Where that centre goes in the frame: its middle, moved by whole pixels.
    let half = f64::from(side) / 2.0;
    let (ox, oy) = (cx + (half - cx).round(), cy + (half - cy).round());
    (0..steps).map(|k| {
        let (sin, cos) = sin_cos(u64::from(k) * 360, u64::from(steps));
        RgbaImage::from_fn(side, side, |px, py| {
            let (mut sum, mut alpha) = ([0u64; 3], 0u64);
            for j in 0..SUPERSAMPLE {
                for i in 0..SUPERSAMPLE {
                    let vx = f64::from(px) + (f64::from(i) + 0.5) / f64::from(SUPERSAMPLE) - ox;
                    let vy = f64::from(py) + (f64::from(j) + 0.5) / f64::from(SUPERSAMPLE) - oy;
                    // Where the frame point was before the clockwise turn.
                    let (sx, sy) = ((cx + vx * cos + vy * sin).floor(), (cy - vx * sin + vy * cos).floor());
                    if sx < 0.0 || sy < 0.0 || sx >= f64::from(image.width) || sy >= f64::from(image.height) {
                        continue;
                    }
                    let p = image.get(sx as u32, sy as u32);
                    let a = u64::from(p[3]);
                    for c in 0..3 {
                        sum[c] += u64::from(p[c]) * a;
                    }
                    alpha += a;
                }
            }
            if alpha == 0 {
                return [0; 4];
            }
            let n = u64::from(SUPERSAMPLE * SUPERSAMPLE);
            let rgb = sum.map(|s| ((s + alpha / 2) / alpha) as u8);
            [rgb[0], rgb[1], rgb[2], ((alpha + n / 2) / n) as u8]
        })
    })
    .collect()
}

/// Sine and cosine of `degrees_num / den` degrees, exact at right angles.
fn sin_cos(degrees_num: u64, den: u64) -> (f64, f64) {
    if degrees_num % (90 * den) == 0 {
        return match (degrees_num / (90 * den)) % 4 {
            0 => (0.0, 1.0),
            1 => (1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (-1.0, 0.0),
        };
    }
    (degrees_num as f64 / den as f64).to_radians().sin_cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An asymmetric 32×32 test cursor: a 15×25 body at (9, 4) with one
    /// marked texel in its top-left corner, like Wine's hourglass box.
    fn art() -> RgbaImage {
        RgbaImage::from_fn(32, 32, |x, y| match (x, y) {
            (9, 4) => [255, 0, 0, 255],
            (9..=23, 4..=28) => [20, 40, 60, 255],
            _ => [0; 4],
        })
    }

    fn opaque_bounds(image: &RgbaImage) -> [u32; 4] {
        let mut b = [u32::MAX, u32::MAX, 0, 0];
        for y in 0..image.height {
            for x in 0..image.width {
                if image.get(x, y)[3] != 0 {
                    b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
                }
            }
        }
        b
    }

    #[test]
    fn twelve_turns_of_thirty_degrees_frame_the_whole_art_centred() {
        let frames = turned(&art(), 12);
        assert_eq!(frames.len(), 12);
        for (k, frame) in frames.iter().enumerate() {
            assert_eq!((frame.width, frame.height), (46, 46), "frame {k}");
            let [l, t, r, b] = opaque_bounds(frame);
            // Nothing is cut at the frame's edges and the art stays centred.
            assert!(l > 0 && t > 0 && r < 45 && b < 45, "frame {k}: {l},{t},{r},{b}");
            assert!(((l + r + 1) as i32 - 46).abs() <= 2 && ((t + b + 1) as i32 - 46).abs() <= 2, "frame {k} centred: {l},{t},{r},{b}");
        }
    }

    #[test]
    fn upright_and_quarter_turns_are_exact_copies_on_the_pixel_grid() {
        let src = art();
        let frames = turned(&src, 12);
        // Centre (16.5, 16.5) lands on (23.5, 23.5): the art moves by 7.
        let at = |frame: &RgbaImage, x: i32, y: i32| frame.get((x + 7) as u32, (y + 7) as u32);
        for y in 0..32 {
            for x in 0..32 {
                let p = src.get(x as u32, y as u32);
                assert_eq!(at(&frames[0], x, y), p, "0° at {x},{y}");
                // Clockwise about (16.5, 16.5): (x, y) goes to (32 - y, x).
                assert_eq!(at(&frames[3], 32 - y, x), p, "90° at {x},{y}");
                assert_eq!(at(&frames[6], 32 - x, 32 - y), p, "180° at {x},{y}");
                assert_eq!(at(&frames[9], y, 32 - x), p, "270° at {x},{y}");
            }
        }
        // The marked top-left texel turned clockwise is at the top right.
        assert_eq!(at(&frames[3], 32 - 4, 9), [255, 0, 0, 255]);
        // Only full or empty pixels at right angles: no blur.
        for k in [0, 3, 6, 9] {
            assert!(frames[k].pixels.iter().all(|p| p[3] == 0 || p[3] == 255), "frame {k}");
        }
    }

    #[test]
    fn oblique_turns_keep_solid_texels_and_smooth_only_the_edges() {
        let frames = turned(&art(), 12);
        let frame = &frames[1];
        let total = |f: &RgbaImage| f.pixels.iter().map(|p| u64::from(p[3])).sum::<u64>();
        // Area is kept to within the edge sampling.
        let (a, b) = (total(&frames[0]) as f64, total(frame) as f64);
        assert!((a - b).abs() / a < 0.02, "{a} vs {b}");
        // The body's middle is the body's own colour, untouched.
        assert_eq!(frame.get(23, 23), [20, 40, 60, 255]);
        // Partial coverage only along the edge: few pixels, and some.
        let partial = frame.pixels.iter().filter(|p| p[3] != 0 && p[3] != 255).count();
        let solid = frame.pixels.iter().filter(|p| p[3] == 255).count();
        assert!(partial > 0 && partial < solid / 2, "{partial} partial, {solid} solid");
    }

    #[test]
    fn clear_art_gives_clear_frames() {
        let frames = turned(&RgbaImage::new(32, 32), 12);
        assert_eq!(frames.len(), 12);
        assert!(frames.iter().all(|f| (f.width, f.height) == (46, 46) && f.pixels.iter().all(|p| p[3] == 0)));
    }
}
