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
    /// -1, or the victory location on this tile.
    pub victory_location: i16,
    /// The tile building (colony, airfield, radar, outpost) here.
    pub building: Option<i16>,
    pub ruins: bool,
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

/// A city.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Civ3City {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub civ: i32,
    /// Population.
    pub size: i32,
    /// Bit i: has improvement i.
    pub improvements: [u8; 32],
}

impl Civ3City {
    pub fn has_improvement(&self, id: i32) -> bool {
        (0..256).contains(&id) && self.improvements[id as usize >> 3] & (1 << (id & 7)) != 0
    }
}

/// An improvement that gives its city an icon on the map (§10.8): id,
/// `ITF_Veteran_*` flags, bit per player that knows the tech obsoleting it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct IconBuilding {
    pub id: i32,
    pub flags: u32,
    pub obsolete_for: u32,
}

/// An improvement that makes towns look walled: id, government it needs
/// (-1 any), bit per player that knows the tech obsoleting it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct WallBuilding {
    pub id: i32,
    pub government: i32,
    pub obsolete_for: u32,
}

/// A unit the game draws, in the animator's drawing order (docs §10).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Civ3Unit {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub civ: i32,
    pub unit_type: i32,
    pub experience: i32,
    pub damage: i32,
    pub moves: i32,
    pub container: i32,
    pub state: i32,
    pub always_on_top: bool,
    /// World pixel of the anchor (tile centre when idle).
    pub pixel: (i32, i32),
    pub direction: i32,
    pub animation: i32,
    pub frame: i32,
    pub visible: bool,
    /// Armies (§10.11): the member shown, and how many members there are.
    pub army_member: Option<i32>,
    pub army_size: i32,
    /// On its way between tiles.
    pub moving: bool,
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
    /// Every civ's capital (city id) and race.
    pub capitals: Vec<i32>,
    pub races: Vec<i32>,
    /// Per race: its culture group (city art).
    pub culture_groups: Vec<i32>,
    /// Population above which a town is a city, and a city a metropolis.
    pub max_town: i32,
    pub max_city: i32,
    pub cities: Vec<Civ3City>,
    pub governments: Vec<i32>,
    pub wall_buildings: Vec<WallBuilding>,
    pub icon_buildings: Vec<IconBuilding>,
    /// Outposts: (tile building id, owner).
    pub outposts: Vec<(i32, i32)>,
    /// Drawn units, in drawing order.
    pub units: Vec<Civ3Unit>,
    /// The members armies show (not drawn themselves).
    pub army_members: Vec<Civ3Unit>,
    /// Civilopedia keys of unit types (`PRTO_...`) and eras (`ERAS_...`).
    pub unit_type_keys: Vec<String>,
    pub era_keys: Vec<String>,
    /// Per unit type: attack, defence, movement (tiles), hit point bonus,
    /// class (0 land, 1 sea, 2 air), worker actions (bit 1: found cities).
    pub unit_type_stats: Vec<[i32; 6]>,
    /// The tile under the mouse, if it is over the map.
    pub mouse_tile: Option<(i32, i32)>,
    /// The selected unit (also while its cursor blinks off).
    pub current_unit: Option<i32>,
    /// Base hit points per experience level.
    pub experience_hit_points: Vec<i32>,
    /// Every unit's tile and container (-1: not loaded).
    pub unit_tiles: Vec<(i32, i32, i32)>,
    /// The game's `P_*` preference flags.
    pub preferences: u32,
    /// The selected unit (id) and the selection cursor's frame, while shown.
    pub selection: Option<(i32, i32)>,
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
    /// Whether the viewing player sees the tile now (not fogged).
    pub fn visible_now(&self, tile: &Civ3Tile) -> bool {
        let bit = 1u32 << (self.player & 31);
        (tile.fow_status | tile.v3 | tile.visibility | tile.visibility_d0) & bit != 0
    }

    /// The tile's overlays as the viewing player knows them
    /// (`Tile::m42_Get_Overlays`, §6.5): as they are where the player sees
    /// the tile now, else only the improvements (bits 0..7) last seen.
    pub fn seen_overlays(&self, tile: &Civ3Tile) -> u32 {
        match usize::try_from(self.player) {
            Ok(civ) if civ > 0 && civ < 32 && !self.visible_now(tile) => u32::from(tile.visible_overlays[civ]),
            _ => tile.overlays,
        }
    }

    pub fn city(&self, id: i32) -> Option<&Civ3City> {
        self.cities.iter().find(|c| c.id == id)
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
    /// The zoomed-out view (half size; camera in its pixels, §3).
    pub zoomed_out: bool,
}

/// A tracer dump (`civ3-trace.txt`): world, view and the game's draw list.
#[derive(Clone, Debug, Default)]
pub struct Trace {
    /// The scenario's art folder and its search folders, as the game
    /// names them (relative to the install's `Conquests` folders).
    pub scenario_folder: Option<String>,
    pub search_roots: Vec<String>,
    /// Address of the `Map_Renderer` the redraw used (also the canvas of
    /// map blits).
    pub map_renderer: u32,
    pub world: Civ3World,
    pub view: Civ3View,
    /// Sheet file names by (Map_Renderer member, element).
    pub files: HashMap<(String, u32), String>,
    pub draws: Vec<TraceDraw>,
    /// The game's text output during the redraw.
    pub texts: Vec<crate::text::TextRun>,
    /// The mirrored interface, if recorded.
    pub ui: Option<crate::ui::Frame>,
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
                Some("text") => t.texts.extend(crate::text::TextRun::parse(line)),
                Some("ui_image") if f.len() >= 12 => {
                    let v = (1..12).map(|k| n(k).map(|v| v as i32)).collect::<Result<Vec<_>, _>>()?;
                    let path = f[12..].join(" ");
                    t.ui.get_or_insert_with(Default::default).draws.push(crate::ui::ScreenDraw::Image {
                        path,
                        rect: [v[7], v[8], v[9], v[10]],
                        x: v[0],
                        y: v[1],
                        keyed: v[2] != 0,
                        clip: [v[3], v[4], v[5], v[6]],
                    });
                }
                Some("ui_surface") if f.len() >= 12 => {
                    let v = (1..12).map(|k| n(k)).collect::<Result<Vec<_>, _>>()?;
                    t.ui.get_or_insert_with(Default::default).draws.push(crate::ui::ScreenDraw::Surface {
                        image: v[0] as u32,
                        x: v[1] as i32,
                        y: v[2] as i32,
                        clip: [v[3] as i32, v[4] as i32, v[5] as i32, v[6] as i32],
                        rect: [v[7] as i32, v[8] as i32, v[9] as i32, v[10] as i32],
                    });
                }
                Some("ui_fill") if f.len() >= 12 => {
                    let v = (1..12).map(|k| n(k).map(|v| v as i32)).collect::<Result<Vec<_>, _>>()?;
                    t.ui.get_or_insert_with(Default::default).draws.push(crate::ui::ScreenDraw::Fill {
                        rect: [v[0], v[1], v[2], v[3]],
                        rgb: [v[4] as u8, v[5] as u8, v[6] as u8],
                        clip: [v[7], v[8], v[9], v[10]],
                    });
                }
                Some("ui_text") if f.len() >= 6 => {
                    let clip = [n(1)? as i32, n(2)? as i32, n(3)? as i32, n(4)? as i32];
                    let rest = line.splitn(6, ' ').nth(5).unwrap_or("");
                    if let Some(run) = crate::text::TextRun::parse(rest) {
                        t.ui.get_or_insert_with(Default::default).draws.push(crate::ui::ScreenDraw::Text { run, clip });
                    }
                }
                Some("zoomed_out") => t.view.zoomed_out = n(1)? != 0,
                Some("scenario_folder") => t.scenario_folder = Some(f[1..].join(" ")),
                Some("search_root") => t.search_roots.push(f[1..].join(" ")),
                Some("view") => {
                    t.view = Civ3View {
                        camera_x: n(2)? as i32,
                        camera_y: n(3)? as i32,
                        tile_x: (n(5)? as i32, n(6)? as i32),
                        tile_y: (n(8)? as i32, n(9)? as i32),
                        zoomed_out: f.get(11).is_some_and(|z| *z == "1"),
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
                Some("capitals") => t.world.capitals = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?,
                Some("races") => t.world.races = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?,
                Some("culture_groups") => {
                    t.world.culture_groups = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?;
                }
                Some("city_sizes") => {
                    t.world.max_town = n(1)? as i32;
                    t.world.max_city = n(2)? as i32;
                }
                Some("city") => t.world.cities.push(Civ3City {
                    id: n(1)? as i32,
                    x: n(2)? as i32,
                    y: n(3)? as i32,
                    civ: n(4)? as i32,
                    size: n(5)? as i32,
                    improvements: {
                        let mut v = [0u8; 32];
                        if let Some(hex) = f.get(6) {
                            for (k, b) in v.iter_mut().enumerate() {
                                *b = hex.get(2 * k..2 * k + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(0);
                            }
                        }
                        v
                    },
                }),
                Some(kind @ ("unit" | "army_unit")) => {
                    let v = (1..f.len()).map(n).collect::<Result<Vec<_>, _>>()?;
                    let at = |i: usize| v.get(i).copied().unwrap_or(0) as i32;
                    let unit = Civ3Unit {
                        id: at(0),
                        x: at(1),
                        y: at(2),
                        civ: at(3),
                        unit_type: at(4),
                        damage: at(5),
                        moves: at(6),
                        container: at(7),
                        state: at(8),
                        always_on_top: at(9) != 0,
                        pixel: (at(10), at(11)),
                        direction: at(12),
                        animation: at(13),
                        frame: at(14),
                        visible: at(15) != 0,
                        experience: at(16),
                        army_member: Some(at(17)).filter(|&m| m >= 0 && v.len() > 17),
                        army_size: at(18),
                        moving: at(19) != 0,
                    };
                    if kind == "unit" {
                        t.world.units.push(unit);
                    } else {
                        t.world.army_members.push(unit);
                    }
                }
                Some("unit_type_stats") => {
                    let opt = |k: usize| -> Result<i32, TraceError> { Ok(f.get(k).map(|_| n(k)).transpose()?.unwrap_or(0) as i32) };
                    t.world.unit_type_stats.push([n(2)? as i32, n(3)? as i32, n(4)? as i32, n(5)? as i32, opt(6)?, opt(7)?]);
                }
                Some("experience_hit_points") => {
                    t.world.experience_hit_points = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?;
                }
                Some("unit_tile") => t.world.unit_tiles.push((n(1)? as i32, n(2)? as i32, n(3)? as i32)),
                Some("outpost") => t.world.outposts.push((n(1)? as i32, n(2)? as i32)),
                Some("mouse_tile") => t.world.mouse_tile = Some((n(1)? as i32, n(2)? as i32)),
                Some("current_unit") => t.world.current_unit = Some(n(1)? as i32),
                Some("preferences") => t.world.preferences = n(1)? as u32,
                Some("selection") => t.world.selection = Some((n(1)? as i32, n(2)? as i32)),
                Some("unit_type_key") => t.world.unit_type_keys.push(f.get(2).copied().unwrap_or("").to_owned()),
                Some("era_key") => t.world.era_keys.push(f.get(2).copied().unwrap_or("").to_owned()),
                Some("governments") => t.world.governments = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?,
                Some("wall_building") => t.world.wall_buildings.push(WallBuilding {
                    id: n(1)? as i32,
                    government: n(2)? as i32,
                    obsolete_for: n(3)? as u32,
                }),
                Some("icon_building") => t.world.icon_buildings.push(IconBuilding {
                    id: n(1)? as i32,
                    flags: n(2)? as u32,
                    obsolete_for: n(3)? as u32,
                }),
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
                        victory_location: f.get(17).map(|_| n(17)).transpose()?.unwrap_or(-1) as i16,
                        building: f.get(18).map(|_| n(18)).transpose()?.map(|b| b as i16).filter(|&b| b >= 0),
                        ruins: f.get(19).map(|_| n(19)).transpose()?.unwrap_or(0) != 0,
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
