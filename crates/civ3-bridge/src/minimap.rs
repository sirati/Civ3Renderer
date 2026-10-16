//! The minimap (docs §17): made from the world state, pixel for pixel as the
//! game makes it, in its RGB555 arithmetic. The frame around it and the
//! viewport rectangle are interface drawing (mirrored, `ui`).

use crate::world::Civ3World;

/// Terrain colours in RGB555, by effective terrain id (§17.4).
const TERRAIN: [u16; 14] =
    [0x7BD7, 0x6AAA, 0x4646, 0x6FBD, 0x30C2, 0x1986, 0x6739, 0x25E6, 0x0D60, 0x31EB, 0x3401, 0x1295, 0x0E11, 0x012B];
const WHITE: u16 = 0x7FFF;
/// Coast, sea, ocean.
const WATER: std::ops::RangeInclusive<u32> = 11..=13;

fn red(c: u16) -> u32 {
    (u32::from(c) >> 7) & 0xF8
}
fn green(c: u16) -> u32 {
    (u32::from(c) >> 2) & 0xF8
}
fn blue(c: u16) -> u32 {
    (u32::from(c) << 3) & 0xFF
}
fn rgb(r: u32, g: u32, b: u32) -> u16 {
    (((r & 0xF8) << 7) | ((g & 0xF8) << 2) | ((b & 0xFF) >> 3)) as u16
}

/// An 8-bit colour as the game converts it to RGB555.
pub fn from_rgb([r, g, b]: [u8; 3]) -> u16 {
    rgb(r.into(), g.into(), b.into())
}

fn expand(c: u16) -> [u8; 3] {
    let e = |v: u16| ((v << 3) | (v >> 2)) as u8;
    [e((c >> 10) & 31), e((c >> 5) & 31), e(c & 31)]
}

/// The game's mapping between minimap pixels and tiles (§17.3).
struct Geometry {
    w: i64,
    h: i64,
    width: i64,
    height: i64,
    wrap_x: bool,
    wrap_y: bool,
}

/// C integer division and remainder (truncating).
fn cdiv(a: i64, b: i64) -> i64 {
    a / b
}
fn cmod(a: i64, b: i64) -> i64 {
    a % b
}

impl Geometry {
    /// `Map_wrap_horiz` / `Map_wrap_vert`: one step, if the map wraps.
    fn wrap_x(&self, x: i64) -> i64 {
        match self.wrap_x {
            true if x < 0 => x + self.width,
            true if x >= self.width => x - self.width,
            _ => x,
        }
    }
    fn wrap_y(&self, y: i64) -> i64 {
        match self.wrap_y {
            true if y < 0 => y + self.height,
            true if y >= self.height => y - self.height,
            _ => y,
        }
    }
    fn valid(&self, x: i64, y: i64) -> bool {
        (0..self.width).contains(&x) && (0..self.height).contains(&y)
    }

    /// 0x57A5D0: the nearest tile of pixel (px, py) and the position inside
    /// its rotated cell; `None` off the map.
    fn tile_of(&self, px: i64, py: i64) -> Option<(i64, i64, i64, i64)> {
        let (w, h, wd, ht) = (self.w, self.h, self.width, self.height);
        let d = 2 * w * h;
        let half = d / 2;
        let mut s = wd * px * h - (ht - 1) * py * w + half;
        let mut fx = cmod(s, d);
        if s < 0 {
            s -= d;
        }
        let a = cdiv(s, d);
        let mut s2 = (ht - 1) * py * w + wd * px * h + half;
        let mut fy = cmod(s2, d);
        if s2 < 0 {
            s2 -= d;
        }
        let b = cdiv(s2, d);
        let (mut x, y) = (a + b, b - a);
        if x >= wd {
            x -= wd;
        }
        if fx < 0 {
            fx += d;
        }
        if fy < 0 {
            fy += d;
        }
        self.valid(x, y).then_some((x, y, fx, fy))
    }
}

/// The minimap the viewing player sees, `size` pixels, as row-major RGBA.
/// `owner_color`: a civ's colour (its colour table's entry 66) in RGB555.
pub fn image(world: &Civ3World, (w, h): (u32, u32), owner_color: impl Fn(i32) -> u16) -> Vec<u8> {
    let g = Geometry {
        w: w as i64,
        h: h as i64,
        width: world.width as i64,
        height: world.height as i64,
        wrap_x: world.flags & 1 != 0,
        wrap_y: world.flags & 2 != 0,
    };
    let mut out = vec![0u8; (w * h * 4) as usize];
    if w == 0 || h == 0 || g.width <= 0 || g.height <= 1 {
        return out;
    }
    let (w, h) = (w as i64, h as i64);
    let d = 2 * w * h;
    let half = d / 2;
    let terrain_at = |x: i64, y: i64| world.tile(x as i32, y as i32).map(|t| t.terrain());
    let off_map = if world.flags & 3 != 0 { WHITE } else { 0 };

    // The bright (terrain) image: the four surrounding tiles' colours,
    // blended bilinearly; its pixels are written in order, skipping pixels
    // off the map (the game's output pointer only moves on valid pixels).
    let mut bright = vec![0u16; (w * h) as usize];
    let mut at = 0usize;
    let mut tiles = vec![None; (w * h) as usize];
    for py in 0..h {
        for px in 0..w {
            let Some((x0, y0, fx0, fy0)) = g.tile_of(px, py) else { continue };
            tiles[(py * w + px) as usize] = Some((x0, y0));
            let (mut x, mut y, mut fx, mut fy) = (x0, y0, fx0, fy0);
            if fx < half {
                fx += half;
                x -= 1;
                y += 1;
            } else {
                fx -= half;
            }
            if fy < half {
                fy += half;
                x -= 1;
                y -= 1;
            } else {
                fy -= half;
            }
            let (mut r, mut gr, mut b) = (0u64, 0u64, 0u64);
            for i in 0..4i64 {
                let xx = g.wrap_x(x + ((i + 1) >> 1));
                let yy = g.wrap_y(y + (i >> 1) - (i & 1));
                let c = if g.valid(xx, yy) {
                    terrain_at(xx, yy).and_then(|t| TERRAIN.get(t as usize).copied()).unwrap_or(0)
                } else {
                    off_map
                };
                let wx = (if i & 1 != 0 { fx } else { d - fx }) as u64;
                let wy = (if i >> 1 != 0 { fy } else { d - fy }) as u64;
                let d = d as u64;
                r += u64::from(red(c)) * wx / d * wy / d;
                gr += u64::from(green(c)) * wx / d * wy / d;
                b += u64::from(blue(c)) * wx / d * wy / d;
            }
            bright[at] = rgb(r.min(255) as u32, gr.min(255) as u32, b.min(255) as u32);
            at += 1;
        }
    }
    // Darkened: averaged with grey (56, 56, 56); owned land tinted 3/4 with
    // the owner's colour over the bright pixel.
    let mut political: Vec<u16> =
        bright.iter().map(|&c| rgb((red(c) + 56) >> 1, (green(c) + 56) >> 1, (blue(c) + 56) >> 1)).collect();
    for (i, tile) in tiles.iter().enumerate() {
        let Some((x, y)) = *tile else { continue };
        let Some(t) = world.tile(x as i32, y as i32) else { continue };
        let owner = i32::from(t.owner);
        if owner > 0 && owner < 32 && !WATER.contains(&t.terrain()) {
            let (k, p) = (owner_color(owner), bright[i]);
            political[i] = rgb((3 * red(k) + red(p)) >> 2, (3 * green(k) + green(p)) >> 2, (3 * blue(k) + blue(p)) >> 2);
        }
    }
    // Shown: explored tiles' pixels; cities as white squares.
    let bit = 1u32 << (world.player & 31);
    let explored = |x: i64, y: i64| world.tile(x as i32, y as i32).is_some_and(|t| t.explored & bit != 0);
    let mut shown = vec![0u16; (w * h) as usize];
    for (i, tile) in tiles.iter().enumerate() {
        if let Some((x, y)) = *tile {
            if explored(x, y) {
                shown[i] = political[i];
            }
        }
    }
    let per_col = w / g.width;
    let s = (per_col >> 2).clamp(1, 4);
    for city in &world.cities {
        let (x, y) = (g.wrap_x(city.x.into()), g.wrap_y(city.y.into()));
        if !g.valid(x, y) || !explored(x, y) {
            continue;
        }
        let (cx, cy) = (x * w / g.width + (per_col >> 2), y * h / (g.height - 1) + (per_col >> 4));
        // The square is whitened in the political image except in row and
        // column 0, then copied over.
        for py in (cy - s).max(0)..=(cy + s).min(h - 1) {
            for px in (cx - s).max(0)..=(cx + s).min(w - 1) {
                let i = (py * w + px) as usize;
                if px > 0 && py > 0 {
                    political[i] = WHITE;
                }
                shown[i] = political[i];
            }
        }
    }
    for (i, &c) in shown.iter().enumerate() {
        let [r, gr, b] = expand(c);
        out[4 * i..4 * i + 4].copy_from_slice(&[r, gr, b, 255]);
    }
    out
}

/// The pixel of a `size` minimap nearest the middle of tile `(x, y)`'s
/// area on it, if the minimap shows the tile.
pub fn pixel_of(world: &Civ3World, (w, h): (u32, u32), (x, y): (i32, i32)) -> Option<(u32, u32)> {
    let g = Geometry {
        w: w as i64,
        h: h as i64,
        width: world.width as i64,
        height: world.height as i64,
        wrap_x: world.flags & 1 != 0,
        wrap_y: world.flags & 2 != 0,
    };
    if w == 0 || h == 0 || g.width <= 0 || g.height <= 1 {
        return None;
    }
    let half = w as i64 * h as i64;
    let mut best: Option<(i64, (u32, u32))> = None;
    for py in 0..h {
        for px in 0..w {
            let Some((tx, ty, fx, fy)) = g.tile_of(px as i64, py as i64) else { continue };
            if (tx, ty) != (x as i64, y as i64) {
                continue;
            }
            let d = (fx - half).abs() + (fy - half).abs();
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, (px, py)));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// The viewport rectangle the game draws on a `size` minimap for tile range
/// `xa..=xb`, `ya..=yb` (0x579F20, docs §17.7), as one-pixel lines (x, y,
/// width, height) in the minimap's pixels; split where it wraps; none if a
/// corner is off the map.
pub fn view_rect(world: &Civ3World, (w, h): (u32, u32), (xa, ya, xb, yb): (i32, i32, i32, i32)) -> Vec<[i32; 4]> {
    let (width, height) = (world.width, world.height);
    if width <= 0 || height <= 1 {
        return Vec::new();
    }
    let fit = |v: i32, size: i32, wraps: bool| if wraps { v.rem_euclid(size) } else { v.clamp(0, size - 1) };
    let (xa, xb) = (fit(xa, width, world.flags & 1 != 0), fit(xb, width, world.flags & 1 != 0));
    let (ya, yb) = (fit(ya, height, world.flags & 2 != 0), fit(yb, height, world.flags & 2 != 0));
    if world.tile(xa, ya).is_none() && world.tile(xa ^ 1, ya).is_none() || world.tile(xb, yb).is_none() && world.tile(xb ^ 1, yb).is_none() {
        return Vec::new();
    }
    let (w, h) = (w as i32, h as i32);
    let (x0, y0) = (xa * w / width, ya * h / (height - 1));
    let (x1, y1) = (xb * w / width, yb * h / (height - 1));
    let hline = |xs: i32, xe: i32, y: i32| [xs, y, xe - xs + 1, 1];
    let vline = |x: i32, ys: i32, ye: i32| [x, ys, 1, ye - ys + 1];
    let mut out = Vec::new();
    match (x0 <= x1, y0 <= y1) {
        (true, true) => out.extend([hline(x0, x1 - 1, y0), hline(x0, x1 - 1, y1 - 1), vline(x0, y0, y1 - 1), vline(x1 - 1, y0, y1 - 1)]),
        (false, true) => out.extend([
            vline(x1 - 1, y0, y1 - 1),
            hline(0, x1 - 1, y0),
            hline(0, x1 - 1, y1 - 1),
            vline(x0, y0, y1 - 1),
            hline(x0, w - 1, y0),
            hline(x0, w - 1, y1 - 1),
        ]),
        (true, false) => out.extend([
            hline(x0, x1 - 1, y1 - 1),
            vline(x0, 0, y1 - 1),
            vline(x1 - 1, 0, y1 - 1),
            hline(x0, x1 - 1, y0),
            vline(x0, y0, h - 1),
            vline(x1 - 1, y0, h - 1),
        ]),
        (false, false) => out.extend([
            hline(0, x1 - 1, y0),
            hline(x0, w - 1, y0),
            hline(0, x1 - 1, y1 - 1),
            hline(x0, w - 1, y1 - 1),
            vline(x0, 0, y1 - 1),
            vline(x0, y0, h - 1),
            vline(x1 - 1, 0, y1 - 1),
            vline(x1 - 1, y0, h - 1),
        ]),
    }
    out.retain(|r| r[2] > 0 && r[3] > 0);
    out
}
