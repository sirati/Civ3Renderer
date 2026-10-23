//! SDF bevel and backdrop refraction, shared by the GPU and CPU oracle.
//! Rounded-box distance: https://iquilezles.org/articles/distfunctions2d/
//! The curved bevel uses its analytic gradient, without ray marching.

#![allow(unexpected_cfgs)]
#[cfg(target_arch = "spirv")]
use spirv_std::num_traits::Float;
use tile_render_shared::LensParams;

pub struct Surface {
    pub distance: f32,
    pub nx: f32,
    pub ny: f32,
    pub depth: f32,
    /// Unit 3D normal of the rising glass height field.
    pub normal: [f32; 3],
    pub bend: f32,
    pub scratch: f32,
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
    // Exactly two local, fixed flaws. No noise, periodic seed or frame input.
    // A small mould step on the right and a short diagonal scratch at the top.
    let seam = (1.0 - ((ly / p.height - 0.37) / 0.014).abs()).max(0.0)
        * (lx / p.width - 0.94).max(0.0) / 0.06;
    let depth = (-distance / rim + seam * 0.035).clamp(0.0, 1.0);
    let t = 1.0 - depth;
    // Rising height h(t) = 0.55 * rim * (1 - sqrt(1 - t²)), where t
    // runs from the clear face to the outer edge. dh/ddistance grows
    // sharply at the crest; the finite lip limits the slope to 2.75.
    let slope = 0.55 * t / (1.0 - t*t).max(0.04).sqrt();
    let nz = 1.0 / (1.0 + slope*slope).sqrt();
    let normal = [-nx * slope * nz, -ny * slope * nz, nz];
    // Thin-glass Snell approximation for index 1.5: displacement is
    // projected normal / normal.z * path * (1 - 1/1.5). The effective
    // optical path is 2.4 rim widths. Only the bevel refracts the backdrop.
    let bend = rim * 0.8 / nz;
    let line = (lx - p.width * 0.69 - (ly - rim * 0.38) * 0.32).abs();
    let scratch = (1.0 - line / (rim * 0.035).max(0.25)).max(0.0)
        * (1.0 - ((ly - rim * 0.38) / (rim * 0.25)).abs()).max(0.0);
    Surface { distance, nx, ny, depth, normal, bend, scratch }
}

/// Reflection from broad, unseen cool and warm lights. Only the bevel is
/// shaded, so the magnified face keeps its original colour and brightness.
pub fn shade(s: &Surface, rgb: [f32; 3]) -> [f32; 3] {
    let [normal_x, normal_y, nz] = s.normal;
    let fresnel = 0.04 + 0.96 * (1.0 - nz).powi(5);
    let cool = (-s.nx * 0.60 - s.ny * 0.80).max(0.0);
    let warm = (s.nx * 0.80 + s.ny * 0.60).max(0.0);
    let streak = (normal_x * 0.30 + normal_y * 0.40 + nz * 0.866).max(0.0).powi(32);
    let inner_light = (1.0 - ((s.depth - 0.72) / 0.18).abs()).max(0.0);
    let inner_line = (1.0 - ((s.depth - 0.94) / 0.06).abs()).max(0.0);
    let reflection = 0.030 + 0.18 * fresnel + 0.11 * inner_light;
    let mut out = [0.0; 3];
    for c in 0..3 {
        let sky = [0.76, 0.88, 1.0][c];
        let room = [1.0, 0.88, 0.73][c];
        out[c] = (rgb[c] * (1.0 - fresnel * 0.20) * (1.0 - inner_line * 0.28)
            + reflection * (0.30 + cool * 1.4) * sky + reflection * warm * 0.85 * room
            + streak * 0.30 + s.scratch * 0.055).clamp(0.0, 1.0);
    }
    out
}

/// A faint scattered glow outside the silhouette, strongest near the lights.
pub fn halo(s: &Surface, rgb: [f32; 3], rim: f32) -> [f32; 3] {
    let fade = (1.0 - s.distance.max(0.0) / (rim * 0.28).max(0.5)).max(0.0);
    let light = (-s.nx * 0.6 - s.ny * 0.8).abs();
    let amount = fade * fade * (0.004 + light * 0.020);
    [(rgb[0] + amount * 0.85).min(1.0), (rgb[1] + amount * 0.93).min(1.0), (rgb[2] + amount).min(1.0)]
}
