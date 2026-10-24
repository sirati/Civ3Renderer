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
//!   also copies its entry 66 to entry 247; `palette2 = "..."` with
//!   `palette2_map = { 64 = 0 }` copies entries of a second palette.
//! Indexed frames (`render_desc::ImageLoader::indexed`): PCX and FLC sources
//! as their palette indices, each index's colour as `load` draws it (all of
//! the above options but `alpha_image`, whose alpha is per pixel: such
//! images are loaded in colour).
//!
//! Palettes (`render_desc::ImageLoader::palette`): a PCX or FLC file's
//! palette, its colours reduced as `quantize` says.
//!
//! * `alpha_image = "Art/mask.pcx"` alpha per pixel from another PCX (repeated
//!   if smaller), whose palette indices are inverted weights (alpha = 255 −
//!   index), as the game's button masks are; `alpha_offset = [dx, dy]`: image
//!   pixel (x, y) takes the mask's (x + dx, y + dy).
//!
//! * `frames = [3, 25]` (FLC) only these frames are made; the others are
//!   1×1 and transparent, so that the frames keep their numbers (an
//!   animation of which only some frames are drawn).
//!
//! PCX sources yield one image, FLC sources one image per frame.
//!
//! Font files (`render_desc::Description::fonts`, e.g. `LSANS.TTF`) are found in the game's
//! folders or the folder above them.
//!
//! `font:<face>/<height>/<r>,<g>,<b>[/<weight>/<italic>]` sources are the game's text glyphs
//! (see `fonts`), one image per character of `fonts::charset()`, in order.
//!
//! `system-cursor:<IDC>/<size>` sources are a system pointer the game asks
//! the system for, e.g. `system-cursor:32514/32` its wait cursor (`IDC_WAIT`)
//! at 32 pixels, from the system's `user32.dll` (see `syscursor`): one
//! image, `size`×`size` (drawn from its top-left); clear where the system
//! has no such cursor (real Windows keeps its cursors in files instead).
//! With `rotations = <n>`: `n` images instead, the cursor turned clockwise
//! in `n` equal steps (see `rotate`), each a square of its diagonal.

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

    fn options_rgba(&self, options: &toml::Table) -> Result<[Option<[u8; 4]>; 256], String> {
        let mut explicit = rgba_table(options)?;
        let Some(value) = options.get("palette_alpha") else { return Ok(explicit) };
        let table = value.as_table().ok_or("palette_alpha must be a table")?;
        let source = table.get("source").and_then(|v| v.as_str()).ok_or("palette_alpha.source is required")?;
        let first = table.get("first").ok_or("palette_alpha.first is required").and_then(|v| int(v).map_err(|_| "invalid palette_alpha.first"))?;
        let count = table.get("count").ok_or("palette_alpha.count is required").and_then(|v| int(v).map_err(|_| "invalid palette_alpha.count"))?;
        if first >= 256 || count == 0 || count > 256 - first { return Err("palette_alpha range is outside palette".into()); }
        let skip = table.get("skip").and_then(|v| v.as_array()).filter(|v| v.len() == 3).ok_or("palette_alpha.skip must be RGB")?;
        let byte = |v| u8::try_from(int(v)?).map_err(|_| "palette_alpha.skip channel is outside byte range".to_string());
        let skip = [byte(&skip[0])?, byte(&skip[1])?, byte(&skip[2])?];
        let bytes = self.files.read(source).map_err(|e| e.to_string())?;
        let palette = pcx::decode(&bytes).map_err(|e| format!("{source}: {e}"))?.palette;
        for (dst, value) in palette_alpha_colors(&palette, first, count, skip).into_iter().enumerate() {
            if let Some(value) = value {
                if explicit[dst].is_some() { return Err("palette_alpha overlaps an explicit rgba override".into()); }
                explicit[dst] = Some(value);
            }
        }
        Ok(explicit)
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
        map_entries(&mut out, &other, options.get("palette_map"))?;
        if let Some(path) = options.get("palette2").and_then(|v| v.as_str()) {
            let bytes = self.files.read(path).map_err(|e| e.to_string())?;
            let second = pcx::decode(&bytes).map_err(|e| format!("{path}: {e}"))?.palette;
            map_entries(&mut out, &second, options.get("palette2_map"))?;
        }
        Ok(out)
    }
}

/// Raw palette channels and the original entry number. No RGB555 or alpha
/// quantization is allowed here; the destination-dependent GPU mode does it.
fn palette_alpha_colors(palette: &Palette, first: usize, count: usize, skip: [u8; 3]) -> [Option<[u8; 4]>; 256] {
    let mut out = [None; 256];
    for (j, (index, c)) in palette.0.iter().enumerate().filter(|(_, c)| [c.0, c.1, c.2] != skip).take(count).enumerate() {
        out[first + j] = Some([c.0, c.1, c.2, index as u8]);
    }
    out
}

/// Copies entries of `from` into `out` per a `{ dst = src }` table.
fn map_entries(out: &mut Palette, from: &Palette, map: Option<&toml::Value>) -> Result<(), String> {
    let Some(map) = map.and_then(|v| v.as_table()) else { return Ok(()) };
    for (dst, src) in map {
        let dst: usize = dst.parse().map_err(|_| format!("palette map key {dst:?} is not an index"))?;
        out.0[dst.min(255)] = from.0[int(src)?.min(255)];
    }
    Ok(())
}

fn signed(v: &toml::Value) -> Result<i64, String> {
    v.as_integer().ok_or_else(|| format!("expected an integer, got {v}"))
}

fn int(v: &toml::Value) -> Result<usize, String> {
    v.as_integer().and_then(|i| usize::try_from(i).ok()).ok_or_else(|| format!("expected an index, got {v}"))
}

/// The frames an FLC source makes (`frames`), if not all.
fn only_frames(options: &toml::Table) -> Result<Option<Vec<usize>>, String> {
    let Some(list) = options.get("frames") else { return Ok(None) };
    let list = list.as_array().ok_or("frames: expected a list")?;
    Ok(Some(list.iter().map(int).collect::<Result<_, _>>()?))
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
    // `opacity = 0..255`: every index that much more transparent (e.g. a
    // ghost of a sprite).
    if let Some(v) = options.get("opacity") {
        let o = int(v)?.min(255) as u32;
        for a in &mut alpha {
            *a = (*a as u32 * o / 255) as u8;
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

/// The colour reduction `quantize` asks for.
fn quantizer(options: &toml::Table) -> Result<fn(u8) -> u8, String> {
    match options.get("quantize").and_then(|v| v.as_str()) {
        None => Ok(|v| v),
        Some("rgb555") => Ok(rgb555),
        Some(other) => Err(format!("unknown quantize mode {other:?}")),
    }
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

/// The colour `to_rgba_with` draws each palette index in.
fn index_colors(palette: &Palette, alpha: &[u8; 256], explicit: &[Option<[u8; 4]>; 256], q: impl Fn(u8) -> u8) -> Vec<[u8; 4]> {
    (0..256)
        .map(|i| {
            if let Some(px) = explicit[i] {
                return px;
            }
            if alpha[i] == 0 {
                return [0, 0, 0, 0];
            }
            let c = palette.get(i as u8);
            [q(c.0), q(c.1), q(c.2), alpha[i]]
        })
        .collect()
}

/// The font of a `font:` source: `<face>/<height>/<r>,<g>,<b>[/<weight>/<italic 0|1>[/on=<r>,<g>,<b>]]`.
/// With `on=`, the glyphs drawn onto that colour, opaque where the result
/// (in 15 bits) differs from it, transparent where not (see
/// [`on_backdrop`]).
pub fn font_of(files: &GameFiles, spec: &str) -> Result<crate::fonts::GlyphFont, String> {
    let (style, rgb, backdrop) = font_spec(spec)?;
    let mut font = crate::fonts::load(files, &style, rgb)?;
    if let Some(key) = backdrop {
        for g in font.glyphs.values_mut() {
            g.image = on_backdrop(&g.image, rgb, key);
        }
    }
    Ok(font)
}

/// Glyph coverage `image` (per colour channel, see `gdi_text::paint`) of
/// text in `color` blended onto `key` as the game's 15-bit canvases keep it,
/// opaque where that differs from `key`: what a canvas cleared to its colour
/// key shows of text drawn on it, put on the screen keyed.
pub fn on_backdrop(image: &RgbaImage, color: [u8; 3], key: [u8; 3]) -> RgbaImage {
    let five = |v: u32| (v >> 3) as u8;
    RgbaImage::from_fn(image.width, image.height, |x, y| {
        let p = image.get(x, y);
        let mixed: [u8; 3] = std::array::from_fn(|i| five((key[i] as u32 * (255 - p[i] as u32) + color[i] as u32 * p[i] as u32) / 255));
        if mixed == key.map(|k| five(k as u32)) {
            [0; 4]
        } else {
            let [r, g, b] = mixed.map(|v| (v << 3) | (v >> 2));
            [r, g, b, 255]
        }
    })
}

/// The style, colour and backdrop a `font:` source names.
/// `<IDC>/<size>` of a `system-cursor:` source.
fn system_cursor_spec(spec: &str) -> Result<(u32, u32), String> {
    let err = || format!("system-cursor:{spec}: want <IDC>/<size>");
    let (id, size) = spec.split_once('/').ok_or_else(err)?;
    let size: u32 = size.parse().map_err(|_| err())?;
    if size == 0 || size > 256 {
        return Err(err());
    }
    Ok((id.parse().map_err(|_| err())?, size))
}

/// An image's `rotations = <n>` (1 to 360), if it has one.
fn rotations(options: &toml::Table) -> Result<Option<u32>, String> {
    let Some(value) = options.get("rotations") else { return Ok(None) };
    match value.as_integer() {
        Some(n @ 1..=360) => Ok(Some(n as u32)),
        _ => Err(format!("rotations: expected 1 to 360, not {value}")),
    }
}

/// A `system-cursor:` source's image: the cursor at the top-left of a
/// `size`×`size` image (cut if larger); clear (logged) where the system has
/// none.
fn system_cursor(files: &GameFiles, spec: &str) -> Result<RgbaImage, String> {
    let (id, size) = system_cursor_spec(spec)?;
    let found = crate::syscursor::find(&crate::syscursor::user32_candidates(files.roots()), id, size);
    let c = found.unwrap_or_else(|e| {
        log::warn!("{e}: left clear");
        render_errors::report(render_errors::Kind::MissingSprite, "a system cursor image, left clear", format_args!("{e}"));
        crate::syscursor::CursorImage { width: 0, height: 0, rgba: Vec::new(), hotspot: (0, 0) }
    });
    Ok(RgbaImage::from_fn(size, size, |x, y| {
        if x < c.width && y < c.height {
            let i = ((y * c.width + x) * 4) as usize;
            [c.rgba[i], c.rgba[i + 1], c.rgba[i + 2], c.rgba[i + 3]]
        } else {
            [0; 4]
        }
    }))
}

fn font_spec(spec: &str) -> Result<(crate::fonts::FontStyle, [u8; 3], Option<[u8; 3]>), String> {
    let err = || format!("font source {spec:?}: expected <face>/<height>/<r>,<g>,<b>[/<weight>/<italic>[/on=<r>,<g>,<b>]]");
    let rgb_of = |s: &str| -> Result<[u8; 3], String> {
        let rgb: Vec<u8> = s.split(',').map(|v| v.trim().parse().map_err(|_| err())).collect::<Result<_, _>>()?;
        <[u8; 3]>::try_from(rgb).map_err(|_| err())
    };
    let mut parts = spec.split('/');
    let (face, height, color) = (parts.next().ok_or_else(err)?, parts.next().ok_or_else(err)?, parts.next().ok_or_else(err)?);
    let height: i32 = height.parse().map_err(|_| err())?;
    let weight: i32 = parts.next().map_or(Ok(0), |w| w.parse().map_err(|_| err()))?;
    let italic = parts.next().is_some_and(|i| i == "1");
    let backdrop = parts.next().map(|p| p.strip_prefix("on=").ok_or_else(err).and_then(rgb_of)).transpose()?;
    Ok((crate::fonts::FontStyle { face: face.to_owned(), height, weight, italic }, rgb_of(color)?, backdrop))
}

impl ImageLoader for Civ3Loader {
    /// From the files' headers (FLC: and their chunks'), and font sheets'
    /// metrics: no pixels are made.
    fn sizes(&self, source: &str, options: &toml::Table) -> Result<Vec<(u32, u32)>, String> {
        if let Some(spec) = source.strip_prefix("system-cursor:") {
            let (_, size) = system_cursor_spec(spec)?;
            return Ok(match rotations(options)? {
                Some(n) => vec![(crate::rotate::frame_side(size), crate::rotate::frame_side(size)); n as usize],
                None => vec![(size, size)],
            });
        }
        if let Some(spec) = source.strip_prefix("font:") {
            let sizes = crate::fonts::glyph_sizes(&self.files, &font_spec(spec)?.0)?;
            return Ok(crate::fonts::charset().map(|c| sizes.get(&c).copied().unwrap_or((1, 1))).collect());
        }
        let path = self.files.resolve(source).map_err(|e| e.to_string())?;
        let mut file = std::fs::File::open(&path).map_err(|e| format!("{source}: {e}"))?;
        let lower = source.to_lowercase();
        if lower.ends_with(".pcx") {
            let mut header = [0u8; 128];
            std::io::Read::read_exact(&mut file, &mut header).map_err(|e| format!("{source}: {e}"))?;
            Ok(vec![pcx::dimensions(&header).map_err(|e| format!("{source}: {e}"))?])
        } else if lower.ends_with(".flc") || lower.ends_with(".fli") {
            let (w, h, n) = flc::dimensions(&mut file).map_err(|e| format!("{source}: {e}"))?;
            let only = only_frames(options)?;
            Ok((0..n).map(|i| if only.as_ref().is_some_and(|o| !o.contains(&i)) { (1, 1) } else { (w, h) }).collect())
        } else {
            Err(format!("{source}: unknown image format"))
        }
    }

    /// A font file of the game's folders, or of the folder above them (the
    /// game installs its LSANS.TTF from its own folder, next to the
    /// expansions').
    fn font(&self, source: &str) -> Result<Vec<u8>, String> {
        let path = self.files.resolve(source).or_else(|_| self.files.resolve(&format!("../{source}"))).map_err(|e| e.to_string())?;
        crate::files::read_file(&path).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The files `source` and the files its options name (palettes, masks)
    /// resolve to among the game's folders now (a scenario's own art
    /// instead of the game's).
    fn identity(&self, source: &str, options: &toml::Table) -> String {
        let resolved = |rel: &str| self.files.resolve(rel).map_or_else(|_| rel.to_owned(), |p| p.to_string_lossy().into_owned());
        let mut out = if source.starts_with("font:") { source.to_owned() } else { resolved(source) };
        for key in ["palette", "palette2", "alpha_image"] {
            if let Some(path) = options.get(key).and_then(|v| v.as_str()) {
                out.push('|');
                out.push_str(&resolved(path));
            }
        }
        if let Some(source) = options.get("palette_alpha").and_then(|v| v.as_table()).and_then(|v| v.get("source")).and_then(|v| v.as_str()) {
            out.push('|');
            out.push_str(&resolved(source));
        }
        // (Other frames made, other sheets.)
        if let Some(frames) = options.get("frames") {
            out.push_str(&format!("|frames={frames}"));
        }
        out
    }

    fn indexed(&self, source: &str, options: &toml::Table) -> Result<Option<Vec<render_desc::IndexedFrame>>, String> {
        let lower = source.to_lowercase();
        let flc = lower.ends_with(".flc") || lower.ends_with(".fli");
        // (Alpha per pixel from a mask is no palette's.)
        if source.starts_with("font:") || options.contains_key("alpha_image") || !(lower.ends_with(".pcx") || flc) {
            return Ok(None);
        }
        let bytes = self.files.read(source).map_err(|e| e.to_string())?;
        let alpha = alpha_table(options)?;
        let explicit = self.options_rgba(options)?;
        let q = quantizer(options)?;
        let frame = |w: u32, h: u32, pixels: &[u8], palette: &Palette| -> Result<render_desc::IndexedFrame, String> {
            Ok(render_desc::IndexedFrame {
                pixels: tile_render::IndexImage { width: w, height: h, pixels: pixels.to_vec() },
                palette: index_colors(&self.options_palette(options, palette)?, &alpha, &explicit, q),
            })
        };
        if flc {
            let anim = flc::decode(&bytes).map_err(|e| format!("{source}: {e}"))?;
            let only = only_frames(options)?;
            // (A frame not made: one transparent pixel.)
            let none = || render_desc::IndexedFrame { pixels: tile_render::IndexImage { width: 1, height: 1, pixels: vec![0] }, palette: vec![[0; 4]; 256] };
            anim.frames
                .iter()
                .enumerate()
                .map(|(i, f)| if only.as_ref().is_some_and(|o| !o.contains(&i)) { Ok(none()) } else { frame(anim.width, anim.height, &f.pixels, &f.palette) })
                .collect::<Result<_, _>>()
                .map(Some)
        } else {
            let img = pcx::decode(&bytes).map_err(|e| format!("{source}: {e}"))?;
            Ok(Some(vec![frame(img.width, img.height, &img.pixels, &img.palette)?]))
        }
    }

    fn palette(&self, source: &str, options: &toml::Table) -> Result<Vec<[u8; 4]>, String> {
        let bytes = self.files.read(source).map_err(|e| e.to_string())?;
        let q = quantizer(options)?;
        let lower = source.to_lowercase();
        let palette = if lower.ends_with(".pcx") {
            pcx::decode(&bytes).map_err(|e| format!("{source}: {e}"))?.palette
        } else if lower.ends_with(".flc") || lower.ends_with(".fli") {
            flc::decode(&bytes).map_err(|e| format!("{source}: {e}"))?.frames.first().ok_or_else(|| format!("{source}: no frames"))?.palette.clone()
        } else {
            return Err(format!("{source}: unknown palette format"));
        };
        Ok(palette.0.iter().map(|c| [q(c.0), q(c.1), q(c.2), 255]).collect())
    }

    fn load(&self, source: &str, options: &toml::Table) -> Result<Vec<RgbaImage>, String> {
        if let Some(spec) = source.strip_prefix("system-cursor:") {
            let cursor = system_cursor(&self.files, spec)?;
            return Ok(match rotations(options)? {
                Some(n) => crate::rotate::turned(&cursor, n),
                None => vec![cursor],
            });
        }
        if let Some(spec) = source.strip_prefix("font:") {
            let font = font_of(&self.files, spec)?;
            return Ok(crate::fonts::charset().map(|c| font.glyphs.get(&c).map_or_else(|| RgbaImage::from_fn(1, 1, |_, _| [0; 4]), |g| g.image.clone())).collect());
        }
        let bytes = self.files.read(source).map_err(|e| e.to_string())?;
        let alpha = alpha_table(options)?;
        let explicit = self.options_rgba(options)?;
        let q = quantizer(options)?;
        let lower = source.to_lowercase();
        if lower.ends_with(".pcx") {
            let img = pcx::decode(&bytes).map_err(|e| format!("{source}: {e}"))?;
            let palette = self.options_palette(options, &img.palette)?;
            let mut out = to_rgba_with(img.width, img.height, &img.pixels, &palette, &alpha, &explicit, q);
            if let Some(mask) = options.get("alpha_image").and_then(|v| v.as_str()) {
                let m = pcx::decode(&self.files.read(mask).map_err(|e| e.to_string())?).map_err(|e| format!("{mask}: {e}"))?;
                let (dx, dy) = match options.get("alpha_offset").and_then(|v| v.as_array()) {
                    Some(o) if o.len() == 2 => (signed(&o[0])?, signed(&o[1])?),
                    _ => (0, 0),
                };
                for y in 0..out.height {
                    for x in 0..out.width {
                        let mut p = out.get(x, y);
                        let mx = (x as i64 + dx).rem_euclid(m.width as i64) as u32;
                        let my = (y as i64 + dy).rem_euclid(m.height as i64) as u32;
                        // Transparent stays transparent.
                        p[3] = p[3].min(255 - m.pixels[(my * m.width + mx) as usize]);
                        out.put(x, y, p);
                    }
                }
            }
            Ok(vec![out])
        } else if lower.ends_with(".flc") || lower.ends_with(".fli") {
            let anim = flc::decode(&bytes).map_err(|e| format!("{source}: {e}"))?;
            let only = only_frames(options)?;
            anim.frames
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    if only.as_ref().is_some_and(|o| !o.contains(&i)) {
                        return Ok(RgbaImage::from_fn(1, 1, |_, _| [0; 4]));
                    }
                    Ok(to_rgba_with(anim.width, anim.height, &f.pixels, &self.options_palette(options, &f.palette)?, &alpha, &explicit, q))
                })
                .collect()
        } else {
            Err(format!("{source}: unknown image format"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_alpha_preserves_non_filler_order_rgb_bytes_and_original_index_weights() {
        let mut palette = Palette([crate::Rgb(0, 0, 255); 256]);
        palette.0[0] = crate::Rgb(3, 7, 11);
        palette.0[51] = crate::Rgb(150, 149, 151);
        palette.0[231] = crate::Rgb(50, 51, 52);
        let colors = palette_alpha_colors(&palette, 224, 16, [0, 0, 255]);
        assert_eq!(colors[224], Some([3, 7, 11, 0]));
        assert_eq!(colors[225], Some([150, 149, 151, 51]));
        assert_eq!(colors[226], Some([50, 51, 52, 231]));
        assert!(colors[227..].iter().all(Option::is_none));
        assert!(colors[..224].iter().all(Option::is_none));
        // Explicit palette channels bypass the generic RGB555 asset option.
        let img = to_rgba_with(1, 1, &[225], &palette, &[0; 256], &colors, |v| (v >> 3) << 3);
        assert_eq!(img.pixels, [[150, 149, 151, 51]]);
    }

    #[test]
    fn missing_palette_alpha_source_is_an_error_instead_of_transparent_fallback() {
        let loader = Civ3Loader::new(GameFiles::with_roots(Vec::new()));
        let options: toml::Table = toml::from_str("palette_alpha = { source = 'missing.pcx', first = 224, count = 16, skip = [0, 0, 255] }").unwrap();
        assert!(loader.options_rgba(&options).is_err());
    }

    #[test]
    fn opacity_scales_every_index_s_alpha() {
        let options: toml::Table = toml::from_str("opacity = 128\nalpha = { 7 = 100 }").unwrap();
        let alpha = alpha_table(&options).unwrap();
        assert_eq!((alpha[0], alpha[7], alpha[254], alpha[255]), (128, 50, 0, 0));
        assert_eq!(alpha_table(&toml::Table::new()).unwrap()[0], 255);
    }

    /// The sizes told without pixels are those of the loaded frames, for the
    /// game's own art (`CIV3_INSTALL`; skipped without an installation) and
    /// the glyph sheets (skipped without them).
    #[test]
    fn sizes_without_pixels_are_the_loaded_ones() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        if !std::path::Path::new(&install).is_dir() {
            return eprintln!("no Civ III install, skipped");
        }
        let loader = Civ3Loader::new(GameFiles::complete(&install));
        let none = toml::Table::new();
        let mut sources = vec!["Art/city screen/background.pcx", "Art/Animations/Disorder/CivilDisorder.flc", "Art/Units/Aegis Cruiser/AegisCruiserDefault.flc"];
        let font = "font:Lucida Sans/12/0,0,0/0/0";
        if crate::fonts::glyph_sizes(&loader.files, &crate::fonts::FontStyle { face: "Lucida Sans".into(), height: 12, weight: 0, italic: false }).is_ok() {
            sources.push(font);
        }
        for source in sources {
            let Ok(frames) = loader.load(source, &none) else { continue };
            let loaded: Vec<(u32, u32)> = frames.iter().map(|f| (f.width, f.height)).collect();
            assert_eq!(loader.sizes(source, &none).unwrap(), loaded, "{source}");
        }
        assert!(loader.sizes("Art/none.pcx", &none).is_err());
        // Only some frames of an animation: the others 1×1, in both.
        let source = "Art/Units/Aegis Cruiser/AegisCruiserDefault.flc";
        let only: toml::Table = toml::from_str("frames = [2]").unwrap();
        let frames = loader.load(source, &only).unwrap();
        let sizes: Vec<(u32, u32)> = frames.iter().map(|f| (f.width, f.height)).collect();
        assert_eq!(loader.sizes(source, &only).unwrap(), sizes);
        assert!(sizes[2].0 > 1 && sizes.iter().enumerate().all(|(i, &s)| i == 2 || s == (1, 1)));
    }

    #[test]
    fn glyphs_on_the_key_keep_their_blended_edges_and_lose_the_rest() {
        // Black text on magenta (the colour key): full coverage black, a
        // half-covered edge pixel half magenta (green stays 0), a barely
        // covered one still the key in 15 bits, so transparent.
        let coverage = RgbaImage::from_fn(4, 1, |x, _| match x {
            0 => [255, 255, 255, 255],
            1 => [128, 128, 128, 128],
            2 => [3, 3, 3, 3],
            _ => [0; 4],
        });
        let out = on_backdrop(&coverage, [0, 0, 0], [255, 0, 255]);
        assert_eq!(out.get(0, 0), [0, 0, 0, 255]);
        assert_eq!(out.get(1, 0), [123, 0, 123, 255]);
        assert_eq!(out.get(2, 0), [0; 4]);
        assert_eq!(out.get(3, 0), [0; 4]);
        assert_eq!(font_spec("Lucida Sans/12/0,0,0/0/0/on=255,0,255").unwrap().2, Some([255, 0, 255]));
        assert_eq!(font_spec("Lucida Sans/12/0,0,0/0/0").unwrap().2, None);
    }
}
