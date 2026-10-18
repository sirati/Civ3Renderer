//! Which sprite the game draws for what: Civilization III's map rendering
//! rules (docs/civ3-map-rendering.md, section numbers below). Every function
//! returns sprite names in the game's own array numbering
//! (`"Mountains_Images/18"`), which the render description maps to art.

use crate::districts;
use crate::world::{Civ3Tile, Civ3World};

/// Terrain ids (enum `SquareTypes`).
pub mod terrain {
    pub const DESERT: u32 = 0;
    pub const PLAINS: u32 = 1;
    pub const GRASSLAND: u32 = 2;
    pub const TUNDRA: u32 = 3;
    pub const FLOOD_PLAIN: u32 = 4;
    pub const HILLS: u32 = 5;
    pub const MOUNTAINS: u32 = 6;
    pub const FOREST: u32 = 7;
    pub const JUNGLE: u32 = 8;
    pub const SWAMP: u32 = 9;
    pub const VOLCANO: u32 = 10;
    pub const COAST: u32 = 11;
    pub const SEA: u32 = 12;
    pub const OCEAN: u32 = 13;
}
use terrain::*;

/// `Tile.Overlays` bits (§6.5).
pub mod overlay {
    pub const ROAD: u32 = 1 << 0;
    pub const RAILROAD: u32 = 1 << 1;
    pub const MINE: u32 = 1 << 2;
    pub const IRRIGATION: u32 = 1 << 3;
    pub const FORTRESS: u32 = 1 << 4;
    pub const GOODY_HUT: u32 = 1 << 5;
    pub const POLLUTION: u32 = 1 << 6;
    pub const BARBARIAN_CAMP: u32 = 1 << 7;
    pub const CRATERS: u32 = 1 << 8;
    pub const BARRICADE: u32 = 1 << 28;
    pub const AIRFIELD: u32 = 1 << 29;
    pub const RADAR: u32 = 1 << 30;
    pub const OUTPOST: u32 = 1 << 31;
}

/// `Tile.Terrain_Overlays` bits (§2.2).
pub mod terrain_overlay {
    pub const BONUS_SHIELD: u32 = 1 << 16;
    pub const SNOW_CAP: u32 = 1 << 20;
    pub const PINES: u32 = 1 << 21;
    pub const LANDMARK: u32 = 1 << 29;
}

/// Fill absent corners of a map vertex by extending the opposite valid
/// boundary tile. At an outside/corner vertex only one tile can survive;
/// its state fills the rest. Real neighbours, including wrapped ones, stay
/// unchanged. A non-wrapping boundary has no hidden sea or unexplored tile.
fn extend_vertex_corners<T: Copy>(corners: [Option<T>; 4], empty: T) -> [T; 4] {
    let only = corners.into_iter().flatten().next().unwrap_or(empty);
    let opposite = [3, 2, 1, 0];
    std::array::from_fn(|k| corners[k].or(corners[opposite[k]]).unwrap_or(only))
}

/// The terrain image at a vertex, keeping the game's stored sheet and
/// every in-map corner's terrain/variant. At a non-wrapping edge replace
/// only the missing corners, whose stored coast/water would otherwise
/// invent a shoreline through inland terrain.
pub fn terrain_vertex(world: &Civ3World, vx: i32, vy: i32) -> Option<u32> {
    let tile = world.tile(vx, vy + 1)?;
    let (file, location) = ((tile.square_parts >> 8) as u32, (tile.square_parts & 0xFF) as u32);
    // The all-sea/all-ocean sheets contain arbitrary visual variants, not
    // ternary terrain combinations. They already have no false shoreline.
    if file >= 7 || location > 80 || ((world.wrap_x || (1..world.width - 1).contains(&vx)) && (world.wrap_y || (1..world.height - 1).contains(&vy))) {
        return Some(file * 81 + location);
    }
    let neighbours = [(vx, vy - 1), (vx - 1, vy), (vx + 1, vy), (vx, vy + 1)];
    let weights = [1, 3, 9, 27];
    let corners = std::array::from_fn(|k| world.tile(neighbours[k].0, neighbours[k].1).map(|_| location / weights[k] % 3));
    let corners = extend_vertex_corners(corners, 0);
    Some(file * 81 + corners.into_iter().zip(weights).map(|(digit, weight)| digit * weight).sum::<u32>())
}

/// The game's per-tile art randomness (§9): ANSI C `rand()` seeded from the
/// map seed and the tile index.
pub struct Rng(u32);

impl Rng {
    /// Seeded for one consumer (`k`) at tile index `a`, with the four
    /// discarded advances every consumer performs first.
    pub fn new(seed: u32, k: u32, a: u32) -> Self {
        Self::with_advances(seed, k, a, 4)
    }

    /// Like [`Rng::new`] for the consumers that discard `n` values first.
    pub fn with_advances(seed: u32, k: u32, a: u32, n: u32) -> Self {
        let mut r = Rng(seed.wrapping_add(a.wrapping_mul(k)));
        for _ in 0..n {
            r.advance();
        }
        r
    }

    fn advance(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(0x41C6_4E6D).wrapping_add(0x3039);
        (self.0 >> 16) & 0x7FFF
    }

    /// `floor(next_float() * n)`.
    pub fn rand_int(&mut self, n: u32) -> u32 {
        (self.advance() * n) >> 15
    }
}

/// Seed constants of the consumers (§9).
pub mod seed_k {
    pub const FOREST: u32 = 1313;
    pub const HILLS: u32 = 121_472;
    pub const RIVER: u32 = 0x84_5FED;
    pub const TNT: u32 = 64_536;
    pub const POLAR: u32 = 21_031;
    pub const POLLUTION: u32 = 21_030;
    pub const BUILDINGS: u32 = 0x2_0C7D;
    /// Three discarded values, not four.
    pub const RAILROAD: u32 = 15_221;
}

/// The tile index the RNG is seeded with (16 bits).
pub fn tile_a(world: &Civ3World, x: i32, y: i32) -> u32 {
    let x = if world.wrap_x { x.rem_euclid(world.width) } else { x };
    ((((world.width / 2) * y) + (x >> 1)) & 0xFFFF) as u32
}

/// Diagonal neighbours in the order the game builds masks: bit 0 NW,
/// bit 1 NE, bit 2 SW, bit 3 SE.
pub const DIAGONALS: [(i32, i32); 4] = [(-1, -1), (1, -1), (-1, 1), (1, 1)];

/// The edge hash that prunes some links between neighbouring hills,
/// irrigation and border segments (§6.1), for the edge from (x, y) to
/// diagonal neighbour `n` (index into [`DIAGONALS`]). The neighbour is
/// wrapped first, so across a wrap seam the midpoint is far off (§6.1).
pub fn edge_keep(world: &Civ3World, x: i32, y: i32, n: usize) -> bool {
    let (dx, dy) = DIAGONALS[n];
    let nx = if world.wrap_x { (x + dx).rem_euclid(world.width) } else { x + dx };
    let ny = if world.wrap_y { (y + dy).rem_euclid(world.height) } else { y + dy };
    // C integer division (truncating) of the shared edge's midpoint.
    let mx = (x + nx) / 2;
    let my = (y + ny) / 2;
    match n {
        // NW, SE
        0 | 3 => (3 * mx - my) % 5 != 0 && (mx - 2 * my) % 5 != 0,
        // NE, SW
        _ => (3 * mx - my) % 7 != 0 && (mx + 2 * my) % 5 != 0 && (7 * mx + my) % 11 != 0,
    }
}

fn is_hilly(t: &Civ3Tile) -> bool {
    matches!(t.terrain(), HILLS | MOUNTAINS | VOLCANO)
}

/// §6.1: which diagonal neighbours of (x, y) a hill, mountain or volcano
/// links to (bit `n`: [`DIAGONALS`]`[n]`). The volcano drawer keeps it in
/// `Tile.field_38`, where the smoke over a volcano is shifted by (§10.14).
pub fn hill_mask(world: &Civ3World, x: i32, y: i32) -> u32 {
    let mut mask = 0;
    for (n, (dx, dy)) in DIAGONALS.iter().enumerate() {
        if world.tile(x + dx, y + dy).is_some_and(is_hilly) && edge_keep(world, x, y, n) {
            mask |= 1 << n;
        }
    }
    mask
}

/// §6.1: mountains, volcanoes and hills.
pub fn mountain(world: &Civ3World, x: i32, y: i32) -> Option<String> {
    let tile = world.tile(x, y)?;
    let (array, snow_ok) = match tile.terrain() {
        MOUNTAINS => ("Mountains_Images", true),
        VOLCANO => ("Volcanos_Images", false),
        HILLS => ("Hills_Images", false),
        _ => return None,
    };
    let mask = hill_mask(world, x, y);
    // Landmark mountains and hills: one sheet, by the mask only (§7).
    if tile.terrain_overlays & terrain_overlay::LANDMARK != 0 {
        match tile.terrain() {
            MOUNTAINS => return Some(format!("LM_Mountains_Images/{mask}")),
            HILLS => return Some(format!("LM_Hills_Images/{mask}")),
            _ => {}
        }
    }
    let kind = if snow_ok && tile.terrain_overlays & terrain_overlay::SNOW_CAP != 0 {
        3
    } else {
        // NE, SE, SW, NW.
        let neighbours: Vec<&Civ3Tile> =
            [(1, -1), (1, 1), (-1, 1), (-1, -1)].iter().filter_map(|(dx, dy)| world.tile(x + dx, y + dy)).collect();
        let only_rough = neighbours.iter().all(|t| matches!(t.terrain(), HILLS | MOUNTAINS | VOLCANO | FOREST | JUNGLE));
        let forests = neighbours.iter().filter(|t| t.terrain() == FOREST).count();
        let jungles = neighbours.iter().filter(|t| t.terrain() == JUNGLE).count();
        if !only_rough || forests + jungles == 0 {
            0
        } else if forests > jungles {
            1
        } else if jungles > forests {
            2
        } else if Rng::new(world.seed, seed_k::HILLS, tile_a(world, x, y)).rand_int(2) == 0 {
            1
        } else {
            2
        }
    };
    Some(format!("{array}/{}", kind * 16 + mask))
}

/// §6.2: forests, jungles and swamps.
pub fn forest(world: &Civ3World, x: i32, y: i32) -> Option<String> {
    let tile = world.tile(x, y)?;
    let kind = tile.terrain();
    if !matches!(kind, FOREST | JUNGLE | SWAMP) {
        return None;
    }
    let diag = |dx: i32, dy: i32| world.tile(x + dx, y + dy);
    let small = [(1, -1), (1, 1), (-1, 1), (-1, -1)].iter().any(|&(dx, dy)| diag(dx, dy).is_some_and(|t| t.terrain() == COAST))
        || [(1, -1), (-1, -1)].iter().any(|&(dx, dy)| diag(dx, dy).is_some_and(is_hilly));
    let mut rng = Rng::new(world.seed, seed_k::FOREST, tile_a(world, x, y));
    let name = match kind {
        FOREST => {
            // Landmark forests: their own sheets, chosen alike (§7).
            let (file, suffix) = match tile.real_terrain() {
                _ if tile.terrain_overlays & terrain_overlay::LANDMARK != 0 => ("LM", "_Images"),
                PLAINS => ("Plains", ""),
                TUNDRA => ("Tundra", ""),
                _ => ("Grassland", ""),
            };
            if tile.terrain_overlays & terrain_overlay::PINES != 0 {
                format!("{file}_Forests_Pines{suffix}/{}", rng.rand_int(12))
            } else if small {
                format!("{file}_Forests_Small{suffix}/{}", rng.rand_int(10))
            } else {
                format!("{file}_Forests_Large{suffix}/{}", rng.rand_int(8))
            }
        }
        JUNGLE if small => format!("Grassland_Jungles_Small/{}", rng.rand_int(12)),
        JUNGLE => format!("Grassland_Jungles_Large/{}", rng.rand_int(8)),
        _ if small => format!("Marsh_Small/{}", rng.rand_int(10)),
        _ => format!("Marsh_Large/{}", rng.rand_int(8)),
    };
    Some(name)
}

/// §6.3: irrigation. A district that can be an irrigation source hides the
/// irrigation of its own tile (§18).
pub fn irrigation(world: &Civ3World, x: i32, y: i32) -> Option<String> {
    let tile = world.tile(x, y)?;
    if world.seen_overlays(tile) & overlay::IRRIGATION == 0 || districts::hides_irrigation(world, tile) {
        return None;
    }
    let mut mask = 0;
    for (n, (dx, dy)) in DIAGONALS.iter().enumerate() {
        if world.tile(x + dx, y + dy).is_some_and(|t| world.seen_overlays(t) & overlay::IRRIGATION != 0) && edge_keep(world, x, y, n) {
            mask |= 1 << n;
        }
    }
    let kind = match tile.terrain() {
        DESERT => 0,
        PLAINS => 1,
        TUNDRA => 3,
        _ => 2,
    };
    Some(format!("Irrigation_Desert_Images/{}", kind * 16 + mask))
}

/// §6.7: small ground decorations on bonus-shield grassland.
pub fn tnt(world: &Civ3World, x: i32, y: i32) -> Option<String> {
    let tile = world.tile(x, y)?;
    if tile.terrain_overlays & terrain_overlay::BONUS_SHIELD == 0 {
        return None;
    }
    // Rows of the game's array (not of tnt.pcx, see civ3.toml).
    let row = match tile.terrain() {
        GRASSLAND => 0,
        PLAINS => 1,
        DESERT => 2,
        TUNDRA => 3,
        FLOOD_PLAIN => 4,
        _ => return None,
    };
    let variant = Rng::new(world.seed, seed_k::TNT, tile_a(world, x, y)).rand_int(3);
    Some(format!("Tnt_Images/{}", row * 3 + variant))
}

/// The resource the viewing player sees on `tile` (`Tile_get_resource_visible_to`:
/// hidden until the prerequisite is known): its type and its icon.
pub fn visible_resource(world: &Civ3World, tile: &Civ3Tile) -> Option<(usize, i32)> {
    let kind = usize::try_from(tile.resource).ok()?;
    if !world.resource_visible.get(kind).copied().unwrap_or(true) {
        return None;
    }
    Some((kind, *world.resource_icons.get(kind)?))
}

/// §6.7: a resource's shadow and icon. Where a district has C3X draw the
/// resource itself (§18) there is none here.
pub fn resource(world: &Civ3World, x: i32, y: i32) -> Option<(String, String)> {
    let tile = world.tile(x, y)?;
    let (_, icon) = visible_resource(world, tile)?;
    if districts::takes_over_resource(world, tile) {
        return None;
    }
    Some((format!("ResourcesShadows/{icon}"), format!("Resources/{icon}")))
}

/// What the game draws at one vertex for rivers (§6.4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VertexRiver {
    /// Flood plain sprite (pass 1).
    pub flood_plain: Option<String>,
    pub river: Option<String>,
    /// The river is drawn before the objects (pass 2) rather than after
    /// them (pass 4): it touches a mountain, volcano or building.
    pub under_objects: bool,
}

/// §6.4: rivers and flood plains at vertex (vx, vy) (vx + vy odd).
pub fn vertex_river(world: &Civ3World, vx: i32, vy: i32) -> VertexRiver {
    let n = world.tile(vx, vy - 1);
    let e = world.tile(vx + 1, vy);
    let s = world.tile(vx, vy + 1);
    let w = world.tile(vx - 1, vy);
    let has = |t: Option<&Civ3Tile>, bit: u8| t.is_some_and(|t| t.river & bit != 0);
    let fp = |t: Option<&Civ3Tile>, bit: u8| t.is_some_and(|t| t.river & bit != 0 && t.terrain() == FLOOD_PLAIN);

    let mut mask = 0u32;
    let mut fp_mask = 0u32;
    let edges: [(u32, [(Option<&Civ3Tile>, u8); 2]); 4] = [
        (1, [(n, 0x20), (w, 0x02)]),
        (2, [(n, 0x08), (e, 0x80)]),
        (4, [(w, 0x08), (s, 0x80)]),
        (8, [(e, 0x20), (s, 0x02)]),
    ];
    for (bit, sources) in edges {
        for (k, (t, b)) in sources.into_iter().enumerate() {
            if has(t, b) {
                mask |= bit;
            }
            // W never makes the NW edge a flood-plain edge (measured, 17
            // flood-plain vertices in two games).
            if fp(t, b) && !(bit == 1 && k == 1) {
                fp_mask |= bit;
            }
        }
    }

    let flood_plain = (fp_mask != 0).then(|| format!("Flood_Plains_Images/{fp_mask}"));
    let around = [n, e, s, w];
    let river = (mask != 0).then(|| {
        // A river end (one edge) takes precedence over the flood-plain
        // override (measured: a flood-plain river end uses variant 1).
        let variant = if mask.count_ones() == 1 {
            // (The tile above is not asked, §6.4.)
            if [w, e, s].iter().flatten().any(|t| t.real_terrain() == COAST) { 0 } else { 1 }
        } else if fp_mask != 0 {
            0
        } else {
            // Seeded with the wrapped vertex row.
            let vy = if world.wrap_y { vy.rem_euclid(world.height) } else { vy };
            Rng::new(world.seed, seed_k::RIVER, tile_a(world, vx, vy - 1)).rand_int(2)
        };
        format!("Delta_Rivers_Images/{}", mask * 2 + variant)
    });
    // Mountains and volcanos only count below the vertex, or beside it when
    // the river runs along their edge with the tile above (measured on 111
    // rivers in three games; the tile above never counts).
    let tall = |t: Option<&Civ3Tile>| t.is_some_and(|t| matches!(t.terrain(), MOUNTAINS | VOLCANO));
    let tall_special = tall(s) || (tall(w) && has(w, 0x02)) || (tall(e) && has(e, 0x80));
    let special = tall_special
        // The actual overlays, not the player's view (measured: a goody hut
        // in unexplored land still counts).
        || around.iter().flatten().any(|t| {
            t.city >= 0
            // C3X's districts count like cities (§18).
            || districts::makes_river_special(world, t)
            // Tile buildings (colonies, airfields, radar, outposts) and ruins.
            || t.building.is_some()
            || t.ruins
            || t.overlays
                & (overlay::FORTRESS
                    | overlay::GOODY_HUT
                    | overlay::BARBARIAN_CAMP
                    | overlay::BARRICADE
                    | overlay::AIRFIELD
                    | overlay::RADAR
                    | overlay::OUTPOST)
                    != 0
        });
    VertexRiver { flood_plain, river, under_objects: special }
}

/// §7: whether vertex (vx, vy) is drawn from the landmark terrain sheets.
/// The game asks the tile beside the vertex in its row (W when the row is
/// even, E when odd; no wrapping in y), not the tile below it whose image
/// is drawn.
pub fn landmark_vertex(world: &Civ3World, vx: i32, vy: i32) -> bool {
    if vy < 0 {
        return false;
    }
    let x = if vy & 1 == 0 { vx - 1 } else { vx + 1 };
    world.tile(x, vy).is_some_and(|t| t.terrain_overlays & terrain_overlay::LANDMARK != 0)
}

/// Whether (x, y) is one of the 21 tiles of a city's radius: the 5×5 block
/// of tile diamonds without its corners, relative (dx, dy) in tile
/// coordinates.
pub fn in_city_radius(dx: i32, dy: i32) -> bool {
    let (u, v) = ((dx + dy) / 2, (dy - dx) / 2);
    (dx + dy) % 2 == 0 && u.abs() <= 2 && v.abs() <= 2 && !(u.abs() == 2 && v.abs() == 2)
}

/// `City_tile_in_view` (§16.8): a tile of the city's radius in its civ's
/// territory (the city tile always).
fn in_city_view(world: &Civ3World, city: &crate::world::Civ3City, x: i32, y: i32, tile: &crate::world::Civ3Tile) -> bool {
    let mut dx = x - city.x;
    if world.wrap_x {
        dx = (dx + world.width / 2).rem_euclid(world.width) - world.width / 2;
    }
    let dy = y - city.y;
    (dx == 0 && dy == 0) || (in_city_radius(dx, dy) && i32::from(tile.owner) == city.civ)
}

/// §6.8: the fog-of-war sprite at vertex (vx, vy): per surrounding tile
/// N, W, E, S a digit 2 (visible to the player), 1 (explored) or 0
/// (unexplored), weighted 1, 3, 9, 27. All visible (80) draws nothing.
/// At a non-wrapping edge, absent corners take the opposite in-map tile's
/// state (the sole surviving corner at an outer vertex). The world ends
/// there; it is not an unexplored neighbour that should darken valid art.
///
/// While the city screen is open the game's map is in "focus city" mode: only
/// the city's own tiles (in its radius and its civ's territory, the city tile
/// always) count as visible, and as explored; every other tile at most as
/// explored (§16.8). The city screen shows the map so, dimmed around the
/// city's tiles.
pub fn fog(world: &Civ3World, vx: i32, vy: i32) -> Option<String> {
    let bit = 1u32 << (world.player & 31);
    let focus = world.focus_city.and_then(|id| world.city(id));
    let digit = |x: i32, y: i32| world.tile(x, y).map(|t| match focus {
        Some(c) if in_city_view(world, c, x, y, t) => 2,
        None if world.visible_now(t) => 2,
        _ if t.explored & bit != 0 => 1,
        _ => 0,
    });
    let corners = [(vx, vy - 1), (vx - 1, vy), (vx + 1, vy), (vx, vy + 1)].map(|(x, y)| digit(x, y));
    let index = extend_vertex_corners(corners, 0).into_iter().rev().fold(0, |acc, digit| acc * 3 + digit);
    (index != 80).then(|| format!("Fog_Of_War_Images/{index}"))
}

/// The eight neighbours in the game's spiral order (`neighbor_index_to_diff`):
/// bit i of a road/railroad mask is a link to neighbour i.
pub const NEIGHBOURS: [(i32, i32); 8] = [(1, -1), (2, 0), (1, 1), (0, 2), (-1, 1), (-2, 0), (-1, -1), (0, -2)];

/// §6.6: the road and railroad sprites of tile (x, y). A link between two
/// railroad tiles is drawn as railroad only. A bridge district has the
/// tile's roads and railroads drawn as part of itself (§18).
pub fn roads(world: &Civ3World, x: i32, y: i32) -> (Option<String>, Option<String>) {
    let Some(tile) = world.tile(x, y) else { return (None, None) };
    if districts::hides_roads(world, tile) {
        return (None, None);
    }
    let road = world.seen_overlays(tile) & overlay::ROAD != 0;
    let rail = world.seen_overlays(tile) & overlay::RAILROAD != 0;
    let (mut road_mask, mut rail_mask) = (0u32, 0u32);
    for (i, (dx, dy)) in NEIGHBOURS.iter().enumerate() {
        let Some(n) = world.tile(x + dx, y + dy) else { continue };
        let n_rail = world.seen_overlays(n) & overlay::RAILROAD != 0;
        if rail && n_rail {
            rail_mask |= 1 << i;
        } else if road && world.seen_overlays(n) & overlay::ROAD != 0 {
            road_mask |= 1 << i;
        }
    }
    let city = tile.city >= 0;
    let road = match (road, road_mask) {
        (false, _) => None,
        // The lone stub: left out on cities and on tiles that (really, not
        // as remembered) have a railroad (§6.6).
        (true, 0) if city || tile.overlays & overlay::RAILROAD != 0 => None,
        (true, mask) => Some(format!("Roads_Images/{mask}")),
    };
    let rail = match (rail, rail_mask) {
        (false, _) => None,
        (true, 0) if city => None,
        // Linked all around: one of 17 junctions.
        (true, 0xFF) => {
            let r = Rng::with_advances(world.seed, seed_k::RAILROAD, tile_a(world, x, y), 3).rand_int(17);
            Some(format!("Railroads_Images/{}", 255 + r))
        }
        (true, mask) => Some(format!("Railroads_Images/{mask}")),
    };
    (road, rail)
}

/// §6.9: territory borders of tile (x, y): per diagonal neighbour (NW, NE,
/// SW, SE) owned by someone else, sprite `2n + hilly` (`hilly`: both tiles
/// hilly and the edge hash keeps the edge), in the owner's colour table.
pub fn borders(world: &Civ3World, x: i32, y: i32) -> [Option<(i32, usize)>; 4] {
    let mut out = [None, None, None, None];
    let Some(tile) = world.tile(x, y) else { return out };
    if tile.owner <= 0 {
        return out;
    }
    let colors = world.color_tables.get(tile.owner as usize).copied().unwrap_or(0);
    for (n, (dx, dy)) in DIAGONALS.iter().enumerate() {
        let Some(other) = world.tile(x + dx, y + dy) else { continue };
        if other.owner == tile.owner {
            continue;
        }
        let hilly = is_hilly(tile) && is_hilly(other) && edge_keep(world, x, y, n);
        out[n] = Some((colors, 2 * n + hilly as usize));
    }
    out
}

/// The viewing player's era (0..3): the era row of fortresses,
/// barricades, colonies and airfields (§6.7b).
fn viewer_era(world: &Civ3World) -> i32 {
    world.eras.get(world.player.max(0) as usize).copied().unwrap_or(0).clamp(0, 3)
}

/// §6.7b: what the game's `Draw_Tile_Buildings` draws on tile (x, y), in
/// drawing order. One RNG serves every random choice here. Nothing on a tile
/// with a district: C3X draws that instead (§18).
pub fn terrain_buildings(world: &Civ3World, x: i32, y: i32) -> Vec<(&'static str, String)> {
    let Some(tile) = world.tile(x, y) else { return Vec::new() };
    if districts::replaces_tile_buildings(world, x, y) {
        return Vec::new();
    }
    let era = viewer_era(world);
    let seen = world.seen_overlays(tile);
    // Airfields, radar towers and outposts are drawn as they are, through
    // fog too.
    let actual = tile.overlays;
    let mut rng = Rng::new(world.seed, seed_k::BUILDINGS, tile_a(world, x, y));
    let mut out = Vec::new();
    if tile.ruins {
        out.push(("ruins", format!("Ruins/{}", rng.rand_int(3))));
    }
    if actual & overlay::AIRFIELD != 0 {
        // C division: eras 0..2 -> 0, era 3 -> 1.
        let i = (era - 1) / 2;
        out.push(("airfield_shadow", format!("Terrain_Buldings_Airfields_Shadow/{i}")));
        out.push(("airfield", format!("Terrain_Buldings_Airfields/{i}")));
    }
    if seen & overlay::MINE != 0 {
        out.push(("mine", "Terrain_Buldings_Mines/0".to_owned()));
    }
    // A barricade is an upgraded fortress: both bits are set.
    if seen & overlay::BARRICADE != 0 {
        out.push(("barricade", format!("Terrain_Buldings_Barricade/{era}")));
    } else if seen & overlay::FORTRESS != 0 {
        out.push(("fortress", format!("Terrain_Buldings_Fortress/{era}")));
    }
    if seen & overlay::GOODY_HUT != 0 {
        out.push(("goody_hut", format!("Goody_Huts_Images/{}", rng.rand_int(8))));
    }
    if actual & seen & overlay::BARBARIAN_CAMP != 0 {
        out.push(("barbarian_camp", "Terrain_Buldings_Barbarian_Camp/0".to_owned()));
    }
    // Airfields, radar towers and outposts are tile buildings too.
    if tile.building.is_some() && actual & (overlay::AIRFIELD | overlay::RADAR | overlay::OUTPOST) == 0 {
        out.push(("colony", format!("Terrain_Buldings_Camp/{era}")));
    }
    if actual & overlay::RADAR != 0 {
        out.push(("radar_shadow", "Terrain_Buldings_Radar_Shadow/0".to_owned()));
        out.push(("radar", "Terrain_Buldings_Radar/0".to_owned()));
    }
    if actual & overlay::OUTPOST != 0 {
        let owner = tile
            .building
            .and_then(|b| world.outposts.iter().find(|&&(id, _)| id == i32::from(b)))
            .map_or(world.player, |&(_, owner)| owner);
        let i = world.eras.get(owner.max(0) as usize).copied().unwrap_or(0).clamp(0, 2);
        out.push(("outpost_shadow", format!("Terrain_Buldings_Outposts_Shadow/{i}")));
        out.push(("outpost", format!("Terrain_Buldings_Outposts/{i}")));
    }
    // On city tiles the marker is drawn after the city (`victory_location`).
    if tile.victory_location >= 0 && tile.city < 0 {
        out.push(("victory_point", "Victory_Image/0".to_owned()));
    }
    out
}

/// §6.7b: craters, then pollution: linked to diagonal neighbours (NW, NE,
/// SE, SW) that have them too (cells 10..24), else one of ten random cells
/// from one RNG shared by both.
pub fn pollution(world: &Civ3World, x: i32, y: i32) -> Vec<(&'static str, String)> {
    let Some(tile) = world.tile(x, y) else { return Vec::new() };
    let mut rng = Rng::new(world.seed, seed_k::POLLUTION, tile_a(world, x, y));
    let mut out = Vec::new();
    for (bit, layer, array) in [(overlay::CRATERS, "craters", "Craters"), (overlay::POLLUTION, "pollution", "Pollution")] {
        if world.seen_overlays(tile) & bit == 0 {
            continue;
        }
        let mut mask = 0;
        for (n, (dx, dy)) in [(-1, -1), (1, -1), (1, 1), (-1, 1)].iter().enumerate() {
            if world.tile(x + dx, y + dy).is_some_and(|t| world.seen_overlays(t) & bit != 0) {
                mask |= 1 << n;
            }
        }
        let index = if mask != 0 { mask + 9 } else { rng.rand_int(10) };
        out.push((layer, format!("{array}/{index}")));
    }
    out
}

/// §8: the city sprite of tile (x, y): by size class (town, city,
/// metropolis; the capital one class larger), the owner's era and culture
/// group.
pub fn city(world: &Civ3World, x: i32, y: i32) -> Option<String> {
    let tile = world.tile(x, y)?;
    // Not on tiles the viewer has never seen (measured).
    if tile.city < 0 || tile.explored & (1 << (world.player & 31)) == 0 {
        return None;
    }
    let city = world.city(tile.city as i32)?;
    let civ = city.civ.clamp(0, 31) as usize;
    let size_class = if city.size > world.max_city {
        2
    } else if city.size > world.max_town {
        1
    } else {
        0
    };
    let capital = world.capitals.get(civ) == Some(&city.id);
    let size_class = (size_class + capital as i32).min(2);
    // A walled town: one of its working buildings defends against bombard
    // (Walls). Free buildings from wonders (the Great Wall) are not
    // modelled yet.
    let government = world.governments.get(civ).copied().unwrap_or(-1);
    let walled = world.wall_buildings.iter().any(|b| {
        city.has_improvement(b.id) && (b.government < 0 || b.government == government) && b.obsolete_for & (1 << civ) == 0
    });
    let size_class = if size_class == 0 && walled { 3 } else { size_class };
    let era = world.eras.get(civ).copied().unwrap_or(0).clamp(0, 3);
    let race = world.races.get(civ).copied().unwrap_or(0);
    let culture = world.culture_groups.get(race.max(0) as usize).copied().unwrap_or(0).clamp(0, 4);
    Some(format!("City_Images/{}", size_class * 20 + era * 5 + culture))
}

/// The victory location marker of city tile (x, y), drawn after the city
/// (elsewhere it is a tile building, see [`terrain_buildings`]).
pub fn victory_location(world: &Civ3World, x: i32, y: i32) -> Option<String> {
    let tile = world.tile(x, y)?;
    (tile.victory_location >= 0 && tile.city >= 0).then(|| "Victory_Image/0".to_owned())
}

/// §5.2: the polar ice cap at vertex (vx, vy), on maps with polar caps
/// (and no vertical wrap): the two rows at each pole, sheet rows 0/1 at the
/// top, 3/2 at the bottom. Unlike the other consumers, one LCG step only.
pub fn polar_cap(world: &Civ3World, vx: i32, vy: i32) -> Option<String> {
    if world.flags & 2 != 0 || world.flags & 4 == 0 {
        return None;
    }
    let base = match vy {
        0 => 0,
        1 => 8,
        y if y == world.height - 1 => 16,
        y if y == world.height - 2 => 24,
        _ => return None,
    };
    let a = (((world.width >> 1) * vy + ((vx & 0xFFFF) >> 1)) & 0xFFFF) as u32;
    let r = Rng(world.seed.wrapping_add(a.wrapping_mul(seed_k::POLAR))).rand_int(8);
    Some(format!("Polar_Icecaps_Images/{}", base + r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_city_screen_lights_only_the_citys_own_tiles() {
        use crate::world::{Civ3City, Civ3Tile};
        // Everything explored and seen by player 1; the city at (10, 10) owns
        // the tiles west of x = 10.
        let mut world = Civ3World { player: 1, width: 40, height: 40, ..Default::default() };
        world.tiles = (0..20 * 40).map(|i| Civ3Tile { explored: 2, visibility: 2, owner: if (i % 20) * 2 < 10 { 1 } else { 0 }, ..Default::default() }).collect();
        world.cities.push(Civ3City { id: 0, x: 10, y: 10, civ: 1, ..Default::default() });
        // Normally all visible: no fog.
        assert_eq!(fog(&world, 10, 9), None);
        assert_eq!(fog(&world, 20, 21), None);
        world.focus_city = Some(0);
        // Above the city: west (9, 9) and the city tile lit, north (10, 8)
        // and east (11, 9) in the radius but not the city's (N + 3 W + 9 E +
        // 27 S).
        assert_eq!(fog(&world, 10, 9), Some(format!("Fog_Of_War_Images/{}", 1 + 3 * 2 + 9 + 27 * 2)));
        // Far away: everything only explored (index 40, all four digits 1).
        assert_eq!(fog(&world, 20, 21), Some("Fog_Of_War_Images/40".into()));
    }

    #[test]
    fn uniform_visibility_does_not_fade_to_unknown_past_the_map_boundary() {
        let mut world = Civ3World { player: 1, width: 8, height: 8, tiles: vec![Civ3Tile::default(); 32], ..Default::default() };
        for (explored, visible, expected) in [(0, 0, Some("Fog_Of_War_Images/0")), (2, 0, Some("Fog_Of_War_Images/40")), (2, 2, None)] {
            world.tiles.fill(Civ3Tile { explored, visibility: visible, ..Default::default() });
            for vx in -1..=8 {
                for vy in -1..=8 {
                    if (vx + vy) & 1 == 0 { continue; }
                    let touches_map = [(vx, vy - 1), (vx - 1, vy), (vx + 1, vy), (vx, vy + 1)].into_iter().any(|(x, y)| world.tile(x, y).is_some());
                    let want = if touches_map { expected } else { Some("Fog_Of_War_Images/0") };
                    assert_eq!(fog(&world, vx, vy).as_deref(), want, "vertex ({vx}, {vy})");
                }
            }
        }
        // A real unexplored neighbour remains unknown. Only missing map
        // coordinates receive an extended boundary state.
        let unknown = world.index(1, 1);
        world.tiles[unknown].visibility = 0;
        world.tiles[unknown].explored = 0;
        assert_eq!(fog(&world, 0, 1).as_deref(), Some("Fog_Of_War_Images/56"));
    }

    #[test]
    fn edge_terrain_keeps_every_real_corner_and_interior_texture_variant() {
        let mut world = Civ3World { width: 8, height: 8, tiles: vec![Civ3Tile::default(); 32], ..Default::default() };
        for file in 0..=6 {
            for location in 0..=80 {
                for ((vx, vy), missing, opposite) in [((0, 1), 1, 2), ((7, 2), 2, 1), ((1, 0), 0, 3)] {
                    let at = world.index(vx, vy + 1);
                    world.tiles[at].square_parts = ((file << 8) | location) as u16;
                    let digits = [location % 3, location / 3 % 3, location / 9 % 3, location / 27 % 3];
                    let weights = [1, 3, 9, 27];
                    let expected = location - digits[missing] * weights[missing] + digits[opposite] * weights[missing];
                    assert_eq!(terrain_vertex(&world, vx, vy), Some(file * 81 + expected));
                }
                let inside = world.index(2, 4);
                world.tiles[inside].square_parts = ((file << 8) | location) as u16;
                assert_eq!(terrain_vertex(&world, 2, 3), Some(file * 81 + location));
            }
        }
    }

    #[test]
    fn terrain_edges_repeat_land_without_erasing_real_or_wrapped_coasts() {
        let mut world = Civ3World { width: 8, height: 8, tiles: vec![Civ3Tile::default(); 32], ..Default::default() };
        let at = world.index(0, 2);
        // xdgc: desert=0, grass=1, coast=2. The W coast corner is outside
        // this inland map; N/E/S all desert. Keep the chosen sheet.
        world.tiles[at].square_parts = (2 << 8) | 6;
        assert_eq!(terrain_vertex(&world, 0, 1), Some(2 * 81));
        // xggc grass variants 0/1 stay as stored; only the outside coast
        // digit is replaced by the valid E grass variant.
        world.tiles[at].square_parts = (5 << 8) | 15;
        assert_eq!(terrain_vertex(&world, 0, 1), Some(5 * 81 + 12));
        // Real coastal water at the boundary stays water.
        world.tiles[at].square_parts = (2 << 8) | 80;
        assert_eq!(terrain_vertex(&world, 0, 1), Some(2 * 81 + 80));
        // Wrap seams have actual neighbours. Their complete stored terrain
        // combination, including a genuine coast there, remains unchanged.
        world.tiles[at].square_parts = (2 << 8) | 6;
        world.wrap_x = true;
        assert_eq!(terrain_vertex(&world, 0, 1), Some(2 * 81 + 6));
        world.wrap_x = false;
        // Top edge: repeat S into missing N, keeping actual W/E/S colours.
        let top = world.index(1, 1);
        world.tiles[top].square_parts = (2 << 8) | 38;
        assert_eq!(terrain_vertex(&world, 1, 0), Some(2 * 81 + 37));
        world.wrap_y = true;
        assert_eq!(terrain_vertex(&world, 1, 0), Some(2 * 81 + 38));
        // All-sea and all-ocean locations are texture variants.
        for file in [7, 8] {
            world.tiles[at].square_parts = (file << 8) | 53;
            assert_eq!(terrain_vertex(&world, 0, 1), Some(u32::from(file) * 81 + 53));
        }
    }

    #[test]
    fn rng_is_ansi_c_rand() {
        // ANSI C rand() with seed 1: 16838, 5758, 10113, ...
        let mut r = Rng(1);
        assert_eq!([r.advance(), r.advance(), r.advance()], [16838, 5758, 10113]);
    }
}
