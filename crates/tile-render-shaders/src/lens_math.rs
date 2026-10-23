//! Analytic rounded glass slab, shared by the GPU and CPU oracle.
//! Rounded-box distance: https://iquilezles.org/articles/distfunctions2d/

#![allow(unexpected_cfgs)]
#[cfg(target_arch = "spirv")]
use spirv_std::num_traits::Float;
use tile_render_shared::LensParams;

pub struct Surface {
    pub distance: f32,
    pub nx: f32,
    pub ny: f32,
    pub depth: f32,
    pub normal: [f32; 3],
    /// Inward ray displacement and per-channel separation, in window pixels
    /// in the presentation pass, composition pixels in the unscaled pass.
    pub bend: f32,
    pub dispersion: f32,
    pub scratch: f32,
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn surface(p: &LensParams, x: f32, y: f32) -> Surface {
    let (lx, ly) = (x - p.x, y - p.y);
    let (px, py) = (lx - p.width * 0.5, ly - p.height * 0.5);
    let r = p.radius.max(0.0).min(p.width.min(p.height) * 0.5);
    let (qx, qy) = (px.abs() - p.width * 0.5 + r, py.abs() - p.height * 0.5 + r);
    let (ax, ay) = (qx.max(0.0), qy.max(0.0));
    let length = (ax * ax + ay * ay).sqrt();
    let distance = length + qx.max(qy).min(0.0) - r;
    let (gx, gy) = if length > 0.0001 { (ax / length, ay / length) }
        else if qx > qy { (1.0, 0.0) } else { (0.0, 1.0) };
    let (nx, ny) = (if px < 0.0 { -gx } else { gx }, if py < 0.0 { -gy } else { gy });
    let rim = p.rim.max(0.5);
    let depth = (-distance / rim).clamp(0.0, 1.0);
    // One short, asymmetric mould step on the right. Fixed local coordinates
    // avoid the repeated edge ticks that noise or a periodic seed produces.
    let seam = smooth(0.342, 0.351, ly / p.height)
        * (1.0 - smooth(0.369, 0.397, ly / p.height))
        * smooth(0.94, 0.985, lx / p.width);
    let t = ((1.0 - depth) * (1.0 + 0.085 * seam)).min(1.0);
    // A convex quarter-ellipse descends from the flat face to the outer lip.
    // Its analytic slope grows continuously from zero to a near-vertical
    // tangent. The finite lip rounds off the singularity.
    let slope = 1.1 * t / (1.0 - t*t + 0.025).sqrt();
    let nz = 1.0 / (1.0 + slope*slope).sqrt();
    let normal = [nx * slope * nz, ny * slope * nz, nz];
    // A finite optical path bounds the grazing ray. Unlike refraction of
    // the pre-zoom backdrop, sampling the face joins its lines continuously
    // at the inner edge. A 24px bevel pulls content inward by up to 85px.
    let bend = rim * 1.65 * slope * t / (1.0 + 0.32 * slope);
    let dispersion = rim * 0.07 * slope * t / (1.0 + slope);
    let line = (lx - p.width * 0.683 - (ly - rim * 0.43) * 0.38).abs();
    let scratch = (1.0 - line / (rim * 0.028).max(0.35)).max(0.0)
        * (1.0 - ((ly - rim * 0.43) / (rim * 0.29)).abs()).max(0.0);
    Surface { distance, nx, ny, depth, normal, bend, dispersion, scratch }
}

/// Broad reflected sky and softbox bands illuminate even a black backdrop.
/// No colour or light is added beyond the inner edge of the bevel.
pub fn shade(s: &Surface, rgb: [f32; 3]) -> [f32; 3] {
    let [normal_x, normal_y, nz] = s.normal;
    let t = 1.0 - s.depth;
    let fresnel = 0.04 + 0.96 * (1.0 - nz).powi(5);
    let cool = (-s.nx * 0.60 - s.ny * 0.80).max(0.0);
    let warm = (s.nx * 0.80 + s.ny * 0.60).max(0.0);
    // Reflect the view direction about the bevel normal. Two soft angular
    // bands represent windows beyond the visible image, not painted stripes.
    let reflected_z = 2.0 * nz * nz - 1.0;
    let sky_band = (1.0 - ((reflected_z - 0.32) / 0.48).abs()).max(0.0);
    let room_band = (1.0 - ((reflected_z + 0.65) / 0.25).abs()).max(0.0);
    let streak = (-normal_x * 0.30 - normal_y * 0.40 + nz * 0.866).max(0.0).powi(20);
    let inner_line = (1.0 - ((s.depth - 0.93) / 0.07).abs()).max(0.0);
    let fade = smooth(0.0, 0.18, t);
    let reflection = (0.035 + 0.40 * fresnel + sky_band * 0.15 + room_band * 0.10) * fade;
    let mut out = [0.0; 3];
    for c in 0..3 {
        let sky = [0.74, 0.88, 1.0][c];
        let room = [1.0, 0.87, 0.68][c];
        out[c] = (rgb[c] * (1.0 - fresnel * 0.45 * fade) * (1.0 - inner_line * 0.22)
            + reflection * (0.45 + cool * 1.25) * sky
            + reflection * warm * 0.65 * room
            + streak * 0.65 * fade + s.scratch * (0.14 + cool * 0.22)).clamp(0.0, 1.0);
    }
    out
}

pub fn halo(s: &Surface, rgb: [f32; 3], rim: f32) -> [f32; 3] {
    let fade = (1.0 - s.distance.max(0.0) / (rim * 0.16).max(0.5)).max(0.0);
    let light = (-s.nx * 0.6 - s.ny * 0.8).abs();
    let amount = fade * fade * (0.004 + light * 0.018);
    [(rgb[0] + amount * 0.85).min(1.0), (rgb[1] + amount * 0.93).min(1.0), (rgb[2] + amount).min(1.0)]
}

/// One physical pixel of silhouette coverage, including rounded corners.
pub fn coverage(s: &Surface) -> f32 { (0.5 - s.distance).clamp(0.0, 1.0) }

pub fn composite(s: &Surface, glass: [f32; 3], backdrop: [f32; 3]) -> [f32; 3] {
    let a = coverage(s);
    [backdrop[0]*(1.0-a)+glass[0]*a, backdrop[1]*(1.0-a)+glass[1]*a, backdrop[2]*(1.0-a)+glass[2]*a]
}
