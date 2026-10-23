//! Original implementation of an SDF bevel and backdrop refraction.
//! Technique references, not copied shader code:
//! https://iquilezles.org/articles/distfunctions2d/ (rounded-box distance)
//! https://github.com/rukkiecodes/liquid-glass (SDF gradient, convex bevel,
//! refraction and specular lighting). This uses a cheap thin-lens offset
//! rather than ray marching or a blur pyramid. Only the rim is shaded.

#![allow(unexpected_cfgs)]
#[cfg(target_arch = "spirv")]
use spirv_std::num_traits::Float;
use tile_render_shared::LensParams;

pub struct Surface {
    pub distance: f32,
    pub nx: f32,
    pub ny: f32,
    pub depth: f32,
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
    // Local lens coordinates: the mould seam and scratch move with the lens,
    // with no frame/time seed or screen-space noise to cause flicker.
    let seed = ((p.width * 0.017 + p.height * 0.013) / rim).fract();
    let seam = (1.0 - ((ly / p.height - 0.34 - seed * 0.08) / 0.018).abs()).max(0.0)
        * (lx / p.width - 0.9).max(0.0) * 10.0;
    let depth = (-distance / rim + seam * 0.045).clamp(0.0, 1.0);
    let slope = (1.0 - depth).clamp(0.0, 1.0);
    // Convex quarter-circle bevel, bounded near the silhouette.
    let bend = rim * 0.85 * slope / (1.0 - slope * slope).max(0.16).sqrt();
    let line = (lx / p.width - 0.69 - (ly / p.height - 0.02) * 0.14).abs() * p.width;
    let scratch = (1.0 - line / (rim * 0.10).max(0.35)).max(0.0)
        * (1.0 - ((ly / p.height - 0.023) / 0.018).abs()).max(0.0);
    Surface { distance, nx, ny, depth, bend, scratch }
}

/// A fixed unseen environment keeps polished glass visible over black.
/// Refraction/dispersion samples are supplied separately by the caller.
pub fn shade(s: &Surface, rgb: [f32; 3]) -> [f32; 3] {
    let slope = 1.0 - s.depth;
    let nz = (1.0 - slope * slope).max(0.0).sqrt();
    let fresnel = 0.04 + 0.96 * (1.0 - nz).powi(5);
    let light = (-s.nx * 0.48 - s.ny * 0.64).max(0.0);
    let half_dot = ((-s.nx * 0.24 - s.ny * 0.32) * slope + nz * 0.9165).max(0.0);
    let streak = half_dot.powi(64) * 0.75;
    let counter = ((s.nx * 0.32 + s.ny * 0.24) * slope + nz * 0.9165).max(0.0).powi(48) * 0.12;
    let darkness = (1.0 - rgb[0].max(rgb[1]).max(rgb[2]) * 18.0).clamp(0.0, 1.0);
    let environment = 0.012 + darkness * 0.025 + 0.065 * light + 0.09 * fresnel;
    let inner = 1.0 - 0.30 * (1.0 - ((s.depth - 0.88) / 0.12).abs()).max(0.0);
    let mut out = [0.0; 3];
    let tint = [0.96, 0.985, 1.0];
    let sky = [0.80, 0.91, 1.0];
    for c in 0..3 {
        out[c] = (rgb[c] * tint[c] * (1.0 - fresnel * 0.32) * inner
            + sky[c] * environment + streak + counter + s.scratch * 0.12).clamp(0.0, 1.0);
    }
    out
}
