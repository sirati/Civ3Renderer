//! `render_desc::ImageLoader` for Civilization III art.
//!
//! Image options understood (all optional):
//!
//! * `transparent = [254, 255]` palette indices drawn transparent
//!   (default: both, Civ III's green and magenta keys).
//! * `alpha = { 1 = 128, 2 = 64 }` palette index → alpha, for translucent
//!   colours such as shadows.
//! * `rgba = { 248 = [0, 0, 0, 64] }` palette index → exact RGBA, bypassing
//!   the palette (translucent shadow ramps).
//! * `quantize = "rgb555"` reduce colours to 15 bits the way Civ III's
//!   renderer does (5 bits per channel, expanded as `(v << 3) | (v >> 2)`).
//! * `palette = "Art/Units/Palettes/ntp00.pcx"` substitute the palette of
//!   this PCX (whole, or `palette_range = [first, count]` of it), e.g. to
//!   recolour units to a civilisation's colours; `palette_map = { 247 = 66 }`
//!   also copies its entry 66 to entry 247.
//!
//! PCX sources yield one image, FLC sources one image per frame.
//!
//! `font:<face>/<height>/<r>,<g>,<b>` sources are the game's text glyphs
//! (see `fonts`), one image per character of `fonts::charset()`, in order.

use render_desc::ImageLoader;
use tile_render::RgbaImage;

use crate::{GameFiles, Palette, flc, pcx};

pub struct Civ3Loader {
    pub files: GameFiles,
}

impl Civ3Loader {
    pub fn new(files: GameFiles) -> Self {
        Self { files }
    }

    fn options_palette(&self, options: &toml::Table, base: &Palette) -> Result<Palette, String> {
        let Some(path) = options.get("palette").and_then(|v| v.as_str()) else { return Ok(base.clone()) };
        let bytes = self.files.read(path).map_err(|e| e.to_string())?;
        let other = pcx::decode(&bytes).map_err(|e| format!("{path}: {e}"))?.palette;
        let (first, count) = match options.get("palette_range").and_then(|v| v.as_array()) {
            Some(r) if r.len() == 2 => (int(&r[0])?, int(&r[1])?),
            _ => (0, 256),
        };
        let mut out = base.clone();
        for i in first..(first + count).min(256) {
            out.0[i] = other.0[i];
        }
        if let Some(map) = options.get("palette_map").and_then(|v| v.as_table()) {
            for (dst, src) in map {
                let dst: usize = dst.parse().map_err(|_| format!("palette_map key {dst:?} is not an index"))?;
                out.0[dst.min(255)] = other.0[int(src)?.min(255)];
            }
        }
        Ok(out)
    }
}

fn int(v: &toml::Value) -> Result<usize, String> {
    v.as_integer().and_then(|i| usize::try_from(i).ok()).ok_or_else(|| format!("expected an index, got {v}"))
}

/// Alpha per palette index from the options.
fn alpha_table(options: &toml::Table) -> Result<[u8; 256], String> {
    let mut alpha = [255u8; 256];
    let transparent = match options.get("transparent").and_then(|v| v.as_array()) {
        Some(list) => list.iter().map(int).collect::<Result<Vec<_>, _>>()?,
        None => vec![254, 255],
    };
    for i in transparent {
        alpha[i.min(255)] = 0;
    }
    if let Some(table) = options.get("alpha").and_then(|v| v.as_table()) {
        for (k, v) in table {
            let i: usize = k.parse().map_err(|_| format!("alpha key {k:?} is not an index"))?;
            alpha[i.min(255)] = int(v)?.min(255) as u8;
        }
    }
    Ok(alpha)
}

pub fn to_rgba(width: u32, height: u32, pixels: &[u8], palette: &Palette, alpha: &[u8; 256]) -> RgbaImage {
    to_rgba_with(width, height, pixels, palette, alpha, &[None; 256], |v| v)
}

/// 8-bit channel through 5 bits and back, as a 15-bit framebuffer stores it.
pub fn rgb555(v: u8) -> u8 {
    let v5 = v >> 3;
    (v5 << 3) | (v5 >> 2)
}

/// Explicit RGBA per palette index from the options.
fn rgba_table(options: &toml::Table) -> Result<[Option<[u8; 4]>; 256], String> {
    let mut out = [None; 256];
    if let Some(table) = options.get("rgba").and_then(|v| v.as_table()) {
        for (k, v) in table {
            let i: usize = k.parse().map_err(|_| format!("rgba key {k:?} is not an index"))?;
            let c = v.as_array().filter(|a| a.len() == 4).ok_or_else(|| format!("rgba.{k} must be [r, g, b, a]"))?;
            let mut px = [0u8; 4];
            for (slot, val) in px.iter_mut().zip(c) {
                *slot = int(val)?.min(255) as u8;
            }
            out[i.min(255)] = Some(px);
        }
    }
    Ok(out)
}

fn to_rgba_with(
    width: u32,
    height: u32,
    pixels: &[u8],
    palette: &Palette,
    alpha: &[u8; 256],
    explicit: &[Option<[u8; 4]>; 256],
    q: impl Fn(u8) -> u8,
) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        let i = pixels[(y * width + x) as usize];
        if let Some(px) = explicit[i as usize] {
            return px;
        }
        let a = alpha[i as usize];
        if a == 0 {
            return [0, 0, 0, 0];
        }
        let c = palette.get(i);
        [q(c.0), q(c.1), q(c.2), a]
    })
}

/// The font of a `font:` source: `<face>/<height>/<r>,<g>,<b>`.
pub fn font_of(files: &GameFiles, spec: &str) -> Result<crate::fonts::GlyphFont, String> {
    let err = || format!("font source {spec:?}: expected <face>/<height>/<r>,<g>,<b>");
    let mut parts = spec.split('/');
    let (face, height, color) = (parts.next().ok_or_else(err)?, parts.next().ok_or_else(err)?, parts.next().ok_or_else(err)?);
    let height: i32 = height.parse().map_err(|_| err())?;
    let rgb: Vec<u8> = color.split(',').map(|v| v.trim().parse().map_err(|_| err())).collect::<Result<_, _>>()?;
    let rgb = <[u8; 3]>::try_from(rgb).map_err(|_| err())?;
    crate::fonts::load(files, face, height, rgb)
}

impl ImageLoader for Civ3Loader {
    fn load(&self, source: &str, options: &toml::Table) -> Result<Vec<RgbaImage>, String> {
        if let Some(spec) = source.strip_prefix("font:") {
            let font = font_of(&self.files, spec)?;
            return Ok(crate::fonts::charset().map(|c| font.glyphs.get(&c).map_or_else(|| RgbaImage::from_fn(1, 1, |_, _| [0; 4]), |g| g.image.clone())).collect());
        }
        let bytes = self.files.read(source).map_err(|e| e.to_string())?;
        let alpha = alpha_table(options)?;
        let explicit = rgba_table(options)?;
        let q: fn(u8) -> u8 = match options.get("quantize").and_then(|v| v.as_str()) {
            None => |v| v,
            Some("rgb555") => rgb555,
            Some(other) => return Err(format!("unknown quantize mode {other:?}")),
        };
        let lower = source.to_lowercase();
        if lower.ends_with(".pcx") {
            let img = pcx::decode(&bytes).map_err(|e| format!("{source}: {e}"))?;
            let palette = self.options_palette(options, &img.palette)?;
            Ok(vec![to_rgba_with(img.width, img.height, &img.pixels, &palette, &alpha, &explicit, q)])
        } else if lower.ends_with(".flc") || lower.ends_with(".fli") {
            let anim = flc::decode(&bytes).map_err(|e| format!("{source}: {e}"))?;
            anim.frames
                .iter()
                .map(|f| Ok(to_rgba_with(anim.width, anim.height, &f.pixels, &self.options_palette(options, &f.palette)?, &alpha, &explicit, q)))
                .collect()
        } else {
            Err(format!("{source}: unknown image format"))
        }
    }
}
