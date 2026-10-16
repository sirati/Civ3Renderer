//! `render_desc::ImageLoader` for Civilization III art.
//!
//! Image options understood (all optional):
//!
//! * `transparent = [254, 255]` palette indices drawn transparent
//!   (default: both, Civ III's green and magenta keys).
//! * `alpha = { 1 = 128, 2 = 64 }` palette index → alpha, for translucent
//!   colours such as shadows.
//! * `palette = "Art/Units/Palettes/ntp00.pcx"` substitute the palette of
//!   this PCX (whole, or `palette_range = [first, count]` of it), e.g. to
//!   recolour units to a civilisation's colours.
//!
//! PCX sources yield one image, FLC sources one image per frame.

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
    RgbaImage::from_fn(width, height, |x, y| {
        let i = pixels[(y * width + x) as usize];
        let a = alpha[i as usize];
        if a == 0 {
            return [0, 0, 0, 0];
        }
        let c = palette.get(i);
        [c.0, c.1, c.2, a]
    })
}

impl ImageLoader for Civ3Loader {
    fn load(&self, source: &str, options: &toml::Table) -> Result<Vec<RgbaImage>, String> {
        let bytes = self.files.read(source).map_err(|e| e.to_string())?;
        let alpha = alpha_table(options)?;
        let lower = source.to_lowercase();
        if lower.ends_with(".pcx") {
            let img = pcx::decode(&bytes).map_err(|e| format!("{source}: {e}"))?;
            let palette = self.options_palette(options, &img.palette)?;
            Ok(vec![to_rgba(img.width, img.height, &img.pixels, &palette, &alpha)])
        } else if lower.ends_with(".flc") || lower.ends_with(".fli") {
            let anim = flc::decode(&bytes).map_err(|e| format!("{source}: {e}"))?;
            anim.frames
                .iter()
                .map(|f| Ok(to_rgba(anim.width, anim.height, &f.pixels, &self.options_palette(options, &f.palette)?, &alpha)))
                .collect()
        } else {
            Err(format!("{source}: unknown image format"))
        }
    }
}
