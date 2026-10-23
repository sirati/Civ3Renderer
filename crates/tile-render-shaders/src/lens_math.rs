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
    pub flaw_shadow: f32,
    pub bubble_light: f32,
    pub position: [f32; 2],
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
    let t = ((1.0 - depth) * (1.0 + 0.10 * seam)).min(1.0);
    // A convex quarter-ellipse descends from the flat face to the outer lip.
    // Its analytic slope grows continuously from zero to a near-vertical
    // tangent. The finite lip rounds off the singularity.
    let slope = 1.1 * t / (1.0 - t*t + 0.025).sqrt();
    let nz = 1.0 / (1.0 + slope*slope).sqrt();
    let normal = [nx * slope * nz, ny * slope * nz, nz];
    // A finite optical path bounds the grazing ray. Unlike refraction of
    // the pre-zoom backdrop, sampling the face joins its lines continuously
    // at the inner edge. A 24px bevel pulls content inward by up to 85px.
    let ray = rim * 1.65 * slope * t / (1.0 + 0.32 * slope);
    // Compress the whole curved corner into an inner annulus. Its sampled
    // radius decreases monotonically, never folding back across a source line
    // or crossing the centre of curvature. Blend into the longer planar rays
    // along each adjoining straight, with no discontinuity at the tangent.
    let corner_distance = (-qx).max(0.0) + (-qy).max(0.0);
    let (bend, ray_scale) = if r > 0.0 && corner_distance < rim * 3.0 {
        let corner_rim = rim.min(r * 0.85);
        let corner_t = (1.0 + distance / corner_rim).clamp(0.0, 1.0);
        let corner_ray = corner_rim * corner_t + (r - corner_rim) * 0.65 * smooth(0.0, 1.0, corner_t);
        let weight = 1.0 - smooth(0.0, rim * 3.0, corner_distance);
        let bend = ray * (1.0 - weight) + corner_ray * weight;
        (bend, if ray > 0.001 { bend / ray } else { 1.0 })
    } else { (ray, 1.0) };
    let dispersion = rim * 0.07 * slope * t / (1.0 + slope) * ray_scale;
    let line = lx - p.width * 0.683 - (ly - rim * 0.43) * 0.38;
    let scratch_end = (1.0 - ((ly - rim * 0.43) / (rim * 0.34)).abs()).max(0.0);
    let scratch = (1.0 - line.abs() / (rim * 0.035).max(0.4)).max(0.0) * scratch_end;
    let scratch_shadow = (1.0 - (line - rim * 0.05).abs() / (rim * 0.026).max(0.3)).max(0.0) * scratch_end;
    // One tiny trapped bubble, with a dark core and a directional crescent.
    let (bx, by) = ((lx - p.width * 0.291) / rim, (ly - rim * 0.62) / rim);
    let (bubble, bubble_light) = if bx.abs() < 0.105 && by.abs() < 0.105 {
        let br = (bx * bx + by * by).sqrt();
        let core = 1.0 - smooth(0.035, 0.09, br);
        let edge = (1.0 - ((br - 0.072) / 0.032).abs()).max(0.0);
        (core, edge * ((-bx * 0.6 - by * 0.8) / br.max(0.001)).max(0.0))
    } else { (0.0, 0.0) };
    Surface { distance, nx, ny, depth, normal, bend, dispersion, scratch,
        flaw_shadow: bubble * 0.42 + scratch_shadow * 0.2, bubble_light,
        position: [lx / p.width - 0.5, ly / p.height - 0.5] }
}

fn band(x: f32, centre: f32, width: f32) -> f32 {
    let t = (1.0 - ((x - centre) / width).abs()).max(0.0);
    t * t * (3.0 - 2.0 * t)
}

/// Finite softboxes and reflected room light illuminate even a black backdrop.
/// No colour or light is added beyond the inner edge of the bevel.
pub fn shade(s: &Surface, rgb: [f32; 3]) -> [f32; 3] {
    let [normal_x, normal_y, nz] = s.normal;
    let t = 1.0 - s.depth;
    let fresnel = 0.04 + 0.96 * (1.0 - nz).powi(5);
    let cool = (-s.nx * 0.60 - s.ny * 0.80).max(0.0);
    let warm = (s.nx * 0.80 + s.ny * 0.60).max(0.0);
    // Reflect the view direction about the bevel normal. Tilted bands with
    // finite, nonrepeating extents stand for lights outside the visible scene.
    let (rx, ry) = (2.0 * nz * normal_x, 2.0 * nz * normal_y);
    let reflected_z = 2.0 * nz * nz - 1.0;
    let [u, v] = s.position;
    let sky_band = band(reflected_z + rx * 0.16 + ry * 0.09, 0.32, 0.44)
        * (0.65 + 0.35 * band(u + v * 0.27, -0.14, 0.68));
    let room_band = band(reflected_z - rx * 0.28 + ry * 0.13, -0.35, 0.24)
        * (0.32 + 0.68 * band(u - v * 0.43, 0.19, 0.52));
    let back_band = band(reflected_z + rx * 0.12 - ry * 0.21, -0.73, 0.18)
        * (0.45 + 0.55 * band(u + v * 0.61, 0.29, 0.63));
    let streak = (-normal_x * 0.30 - normal_y * 0.40 + nz * 0.866).max(0.0).powi(20);
    let softbox = 0.18 + 0.82 * band(u + v * 0.15, -0.34, 0.78);
    let crest_band = band(reflected_z + rx * 0.12 - ry * 0.08, 0.68, 0.12) * softbox * cool;
    let back_streak = (normal_x * 0.38 + normal_y * 0.48 + nz * 0.79).max(0.0).powi(24)
        * (0.35 + 0.65 * band(u - v * 0.37, 0.12, 0.62));
    let inner_line = (1.0 - ((s.depth - 0.93) / 0.07).abs()).max(0.0);
    let fade = smooth(0.0, 0.18, t);
    let reflection = (0.035 + 0.32 * fresnel) * fade;
    let mut out = [0.0; 3];
    for c in 0..3 {
        let sky = [0.58, 0.79, 1.0][c];
        let room = [1.0, 0.80, 0.56][c];
        let light = reflection * (0.45 + cool * 1.25) * sky
            + fade * (sky_band * (0.38 + cool * 0.22) * sky
                + room_band * (0.22 + warm * 0.65) * room
                + back_band * (0.22 + warm * 0.50) * sky
                + streak * softbox * 2.5 + crest_band * 0.65 * sky
                + back_streak * 2.0 * room);
        let light = light * (1.0 - s.flaw_shadow * fade)
            + fade * (s.scratch * (0.45 + cool * 0.65) + s.bubble_light * 1.15);
        let transmitted = rgb[c] * (1.0 - fresnel * 0.55 * fade)
            * (1.0 - inner_line * 0.22) * (1.0 - s.flaw_shadow * fade);
        // Compress reflected radiance into the remaining headroom. This
        // smooth shoulder keeps detail when a refracted white line crosses
        // a bright crest, rather than clipping their sum to a white blob.
        out[c] = transmitted + (1.0 - transmitted) * light / (1.0 + light);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_rays_preserve_line_order_and_cannot_cross_the_centre() {
        let p = LensParams { width: 600.0, height: 200.0, radius: 56.0, rim: 24.0, ..Default::default() };
        for angle in 0..=90 {
            let a = (angle as f32).to_radians();
            let mut previous = 0.0;
            for step in 0..=100 {
                let radius = p.radius - p.rim * step as f32 / 100.0;
                let s = surface(&p, p.radius - radius * a.cos(), p.radius - radius * a.sin());
                assert!(s.bend + s.dispersion < radius,
                    "a dispersed ray folds a line back across the corner at {angle} degrees, depth {step}");
                let sampled_radius = radius - s.bend;
                assert!(sampled_radius >= previous - 0.0001,
                    "a corner ray reverses a source line at {angle} degrees, depth {step}");
                previous = sampled_radius;
            }
        }
    }
}
