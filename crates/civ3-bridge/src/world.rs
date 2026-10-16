//! Civilization III world state, independent of where it came from: read
//! live from the game's memory, or parsed from a tracer dump.

use std::collections::HashMap;

/// The fields of a `Tile` the renderer needs.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Civ3Tile {
    pub square_type: u32,
    /// Base terrain image: `(file << 8) | location` (docs §5).
    pub square_parts: u16,
    pub overlays: u32,
    pub terrain_overlays: u32,
    pub river: u8,
    pub resource: i32,
    pub owner: i8,
    pub city: i16,
    pub fow_status: u32,
    pub visibility: u32,
    /// Bit c: explored by civ c.
    pub explored: u32,
    pub v3: u32,
    pub visibility_d0: u32,
    /// Per civ: the improvements (overlay bits 0..7) that civ remembers.
    pub visible_overlays: [u8; 32],
    pub victory_point: i16,
}

impl Civ3Tile {
    /// Real (underlying) terrain id, `SquareType` bits 8..12.
    pub fn real_terrain(&self) -> u32 {
        (self.square_type >> 8) & 0xF
    }

    /// Effective terrain id (hills, forest, ... when present), bits 12..16.
    pub fn terrain(&self) -> u32 {
        (self.square_type >> 12) & 0xF
    }
}

/// The map: shape plus every tile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Civ3World {
    /// The civ whose view is drawn (fog of war, remembered improvements).
    pub player: i32,
    /// `BIC.ResourceTypes[i].IconID`.
    pub resource_icons: Vec<i32>,
    /// Per resource type: shown to `player` (prerequisite tech known).
    pub resource_visible: Vec<bool>,
    /// Every civ's era (0..3).
    pub eras: Vec<i32>,
    /// Every civ's colour table (`ntpNN.pcx`).
    pub color_tables: Vec<i32>,
    /// In Civ III x units: tiles exist where x + y is even.
    pub width: i32,
    pub height: i32,
    pub wrap_x: bool,
    pub wrap_y: bool,
    pub seed: u32,
    /// `Map.Flags`: bit 0 wrap x, bit 1 wrap y, bit 2 polar ice caps.
    pub flags: u32,
    /// Indexed `(y·width + x) / 2`.
    pub tiles: Vec<Civ3Tile>,
}

impl Civ3World {
    /// The tile's overlays as the viewing player knows them: improvements
    /// (bits 0..7) as last seen, the rest as they are.
    pub fn seen_overlays(&self, tile: &Civ3Tile) -> u32 {
        match usize::try_from(self.player) {
            Ok(civ) if civ > 0 && civ < 32 => (tile.overlays & !0xFF) | u32::from(tile.visible_overlays[civ]),
            _ => tile.overlays,
        }
    }

    pub fn index(&self, x: i32, y: i32) -> usize {
        ((y * self.width + x) / 2) as usize
    }

    /// Tile (x, y) after wrapping; `None` outside a non-wrapping edge or for
    /// coordinates that are not a tile (x + y odd).
    pub fn tile(&self, x: i32, y: i32) -> Option<&Civ3Tile> {
        let x = if self.wrap_x { x.rem_euclid(self.width) } else { x };
        let y = if self.wrap_y { y.rem_euclid(self.height) } else { y };
        if x < 0 || y < 0 || x >= self.width || y >= self.height || (x + y) % 2 != 0 {
            return None;
        }
        self.tiles.get(self.index(x, y))
    }
}

/// What the main map view shows.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Civ3View {
    pub camera_x: i32,
    pub camera_y: i32,
    pub tile_x: (i32, i32),
    pub tile_y: (i32, i32),
}

/// A tracer dump (`civ3-trace.txt`): world, view and the game's draw list.
#[derive(Clone, Debug, Default)]
pub struct Trace {
    /// Address of the `Map_Renderer` the redraw used (also the canvas of
    /// map blits).
    pub map_renderer: u32,
    pub world: Civ3World,
    pub view: Civ3View,
    /// Sheet file names by (Map_Renderer member, element).
    pub files: HashMap<(String, u32), String>,
    pub draws: Vec<TraceDraw>,
}

/// One `Sprite_draw_on_map` call the game made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceDraw {
    /// Map_Renderer array and index, `None` if unresolved.
    pub sprite: Option<(String, u32)>,
    pub x: i32,
    pub y: i32,
    /// Which blit: "map", "blend" or "colored".
    pub kind: String,
    /// Sprite size, if the trace has it.
    pub size: Option<(u32, u32)>,
    /// The blit's other arguments (for "colored": canvas, second canvas,
    /// colour table, p6).
    pub params: Vec<u32>,
}

#[derive(Debug, thiserror::Error)]
#[error("trace line {line}: {message}")]
pub struct TraceError {
    pub line: usize,
    pub message: String,
}

fn num(s: &str) -> Option<i64> {
    match s.strip_prefix("0x") {
        Some(h) => i64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

impl Trace {
    pub fn parse(text: &str) -> Result<Trace, TraceError> {
        let mut t = Trace::default();
        let mut tiles: Vec<(i32, i32, Civ3Tile)> = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let err = |message: &str| TraceError { line: i + 1, message: message.to_owned() };
            let f: Vec<&str> = line.split_whitespace().collect();
            let n = |k: usize| f.get(k).and_then(|s| num(s)).ok_or_else(|| err("bad number"));
            match f.first().copied() {
                Some("#") if f.get(1) == Some(&"civ3-bridge") && f.get(3) == Some(&"map_renderer") => {
                    t.map_renderer = n(4)? as u32;
                }
                Some("view") => {
                    t.view = Civ3View {
                        camera_x: n(2)? as i32,
                        camera_y: n(3)? as i32,
                        tile_x: (n(5)? as i32, n(6)? as i32),
                        tile_y: (n(8)? as i32, n(9)? as i32),
                    }
                }
                Some("map") => {
                    t.world.width = n(1)? as i32;
                    t.world.height = n(2)? as i32;
                    t.world.wrap_x = n(4)? != 0;
                    t.world.wrap_y = n(5)? != 0;
                    if f.get(6) == Some(&"seed") {
                        t.world.seed = n(7)? as u32;
                    }
                    if f.get(8) == Some(&"flags") {
                        t.world.flags = n(9)? as u32;
                    }
                }
                Some("player") => t.world.player = n(1)? as i32,
                Some("resource_icons") => {
                    t.world.resource_icons = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?;
                }
                Some("color_tables") => {
                    t.world.color_tables = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?;
                }
                Some("eras") => {
                    t.world.eras = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?;
                }
                Some("resource_visible") => {
                    t.world.resource_visible = f[1..].iter().map(|v| *v == "1").collect();
                }
                Some("file") if f.len() >= 3 => {
                    let path = f[3..].join(" ");
                    t.files.insert((f[1].to_owned(), n(2)? as u32), path);
                }
                Some("tile") => {
                    let tile = Civ3Tile {
                        square_type: n(3)? as u32,
                        square_parts: n(4)? as u16,
                        overlays: n(5)? as u32,
                        terrain_overlays: n(6)? as u32,
                        river: n(7)? as u8,
                        resource: n(8)? as i32,
                        owner: n(9)? as i8,
                        city: n(10)? as i16,
                        fow_status: n(11)? as u32,
                        visibility: n(12)? as u32,
                        explored: f.get(13).map(|_| n(13)).transpose()?.unwrap_or(0) as u32,
                        v3: f.get(14).map(|_| n(14)).transpose()?.unwrap_or(0) as u32,
                        visibility_d0: f.get(15).map(|_| n(15)).transpose()?.unwrap_or(0) as u32,
                        visible_overlays: {
                            let mut v = [0u8; 32];
                            if let Some(hex) = f.get(16) {
                                for (k, b) in v.iter_mut().enumerate() {
                                    *b = hex.get(2 * k..2 * k + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(0);
                                }
                            }
                            v
                        },
                        victory_point: f.get(17).map(|_| n(17)).transpose()?.unwrap_or(-1) as i16,
                    };
                    tiles.push((n(1)? as i32, n(2)? as i32, tile));
                }
                Some("draw") => {
                    let sprite = (f[1] != "?").then(|| Ok::<_, TraceError>((f[1].to_owned(), n(2)? as u32))).transpose()?;
                    let kind = f.get(5).filter(|k| !k.starts_with("0x")).map_or("map", |k| k).to_owned();
                    // Newer traces: kind w h params (params are hex).
                    let size = match (f.get(6), f.get(7)) {
                        (Some(w), Some(h)) if !w.starts_with("0x") && !h.starts_with("0x") => Some((n(6)? as u32, n(7)? as u32)),
                        _ => None,
                    };
                    let first_param = if size.is_some() { 8 } else { 6 };
                    let params = (first_param..f.len()).map(|k| n(k).map(|v| v as u32)).collect::<Result<_, _>>()?;
                    t.draws.push(TraceDraw { sprite, x: n(3)? as i32, y: n(4)? as i32, kind, size, params });
                }
                _ => {}
            }
        }
        let w = &mut t.world;
        w.tiles = vec![Civ3Tile::default(); (w.width * w.height / 2).max(0) as usize];
        for (x, y, tile) in tiles {
            let i = ((y * w.width + x) / 2) as usize;
            if let Some(slot) = w.tiles.get_mut(i) {
                *slot = tile;
            }
        }
        Ok(t)
    }
}
