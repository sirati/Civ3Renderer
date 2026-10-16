//! What C3X's drawing code asks of the game: tiles, the districts on them,
//! the cities around them and the buildings those have. Line numbers are
//! those of C3X's `injected_code.c` (release 28 preview, d4e71c4).

use civ3_sys::c3x::{District, DistrictKind};
use civ3_sys::c3x_layout as X;

use crate::rules::{NEIGHBOURS, overlay, terrain};
use crate::world::{Civ3City, Civ3Tile, Civ3World};

/// `enum direction` (Civ3Conquests.h:363): 1 NE, 2 E, ... 8 N. Minus one it
/// indexes [`NEIGHBOURS`], the tile steps (`direction_to_offset`, 4417).
pub const NE: usize = 1;
pub const E: usize = 2;
pub const SE: usize = 3;
pub const S: usize = 4;
pub const SW: usize = 5;
pub const W: usize = 6;
pub const NW: usize = 7;
pub const N: usize = 8;
pub const DIRECTIONS: [usize; 8] = [NE, E, SE, S, SW, W, NW, N];

/// The tile step towards `dir`.
pub fn step(dir: usize) -> (i32, i32) {
    NEIGHBOURS[dir - 1]
}

/// `tile_at` (0x437AF0): the tile of index `(Width/2)·y + (x>>1)` in 16 bits,
/// whatever the parity of x + y (docs §2.1). Beyond the edge of a map that
/// does not wrap that finds another tile, or none, as it does in the game.
pub fn tile_at(world: &Civ3World, x: i32, y: i32) -> Option<&Civ3Tile> {
    let index = ((world.width / 2) * y + (x >> 1)) & 0xFFFF;
    world.tiles.get(index as usize)
}

/// `wrap_tile_coords` (4483): one wrap of each axis that wraps.
pub fn wrap(world: &Civ3World, x: i32, y: i32) -> (i32, i32) {
    let wrap = |v: i32, size: i32, on: bool| if !on { v } else if v < 0 { v + size } else if v >= size { v - size } else { v };
    (wrap(x, world.width, world.wrap_x), wrap(y, world.height, world.wrap_y))
}

/// The tile at (x, y) after `wrap_tile_coords`, as C3X's helpers find it.
pub fn tile_near(world: &Civ3World, x: i32, y: i32) -> Option<&Civ3Tile> {
    let (x, y) = wrap(world, x, y);
    tile_at(world, x, y)
}

/// `m35_Check_Is_Water`: coast, sea and ocean.
pub fn is_water(tile: &Civ3Tile) -> bool {
    matches!(tile.terrain(), terrain::COAST | terrain::SEA | terrain::OCEAN)
}

/// `tile_is_water` (5787).
pub fn tile_is_water(world: &Civ3World, x: i32, y: i32) -> bool {
    tile_near(world, x, y).is_some_and(is_water)
}

/// `tile_is_land` (5757): land, and with `same_owner` owned by `civ`.
pub fn tile_is_land(world: &Civ3World, civ: i32, x: i32, y: i32, same_owner: bool) -> bool {
    if same_owner && civ <= 0 {
        return false;
    }
    tile_near(world, x, y).is_some_and(|t| !is_water(t) && (!same_owner || i32::from(t.owner) == civ))
}

/// `get_district_instance` (3879): the district on `tile`, if its kind is one
/// C3X knows.
pub fn instance<'a>(world: &'a Civ3World, tile: &Civ3Tile) -> Option<&'a District> {
    let d = world.districts.get(tile.district? as usize)?;
    usize::try_from(d.kind).is_ok_and(|k| k < world.district_configs.districts.len()).then_some(d)
}

pub fn kind_of<'a>(world: &'a Civ3World, d: &District) -> Option<&'a DistrictKind> {
    world.district_configs.districts.get(usize::try_from(d.kind).ok()?)
}

/// `district_is_complete` (7399): done, or still under construction but its
/// tile has the mine the building worker makes (C3X then marks it done).
pub fn is_complete(world: &Civ3World, tile: &Civ3Tile, d: &District) -> bool {
    let flags = &world.district_configs.flags;
    if !flags.districts && (d.kind != X::NATURAL_WONDER_DISTRICT_ID || !flags.natural_wonders) {
        return false;
    }
    d.completed || tile.overlays & overlay::MINE != 0
}

/// `tile_has_district_at` (5733): a completed district of kind `id` at (x, y).
pub fn has_district_at(world: &Civ3World, x: i32, y: i32, id: i32) -> bool {
    tile_near(world, x, y).is_some_and(|t| instance(world, t).is_some_and(|d| d.kind == id && is_complete(world, t, d)))
}

/// `tile_has_district_at_with_owner` (5745): the same, on a tile owned by `owner`.
pub fn has_district_at_owned(world: &Civ3World, x: i32, y: i32, id: i32, owner: i32) -> bool {
    tile_near(world, x, y)
        .is_some_and(|t| i32::from(t.owner) == owner && instance(world, t).is_some_and(|d| d.kind == id && is_complete(world, t, d)))
}

/// `FOR_CITIES_AROUND` (4783-4825, 4864): the cities of the civ that owns
/// tile (x, y) that stand on tiles of that civ in the work area around it, in
/// the order C3X enumerates the area (the centre first, then ring by ring).
/// C3X also skips tiles that hold an enemy unit, which a city's does not.
pub fn cities_around(world: &Civ3World, x: i32, y: i32) -> Vec<&Civ3City> {
    let mut out = Vec::new();
    let Some(centre) = tile_at(world, x, y) else { return out };
    let civ = i32::from(centre.owner);
    if civ < 0 {
        return out;
    }
    for &(dx, dy) in world.district_configs.work_area() {
        let Some(candidate) = tile_near(world, x + i32::from(dx), y + i32::from(dy)) else { continue };
        if i32::from(candidate.owner) != civ || candidate.overlays & overlay::POLLUTION != 0 || candidate.city < 0 {
            continue;
        }
        if let Some(city) = world.city(i32::from(candidate.city)).filter(|c| c.civ == civ) {
            out.push(city);
        }
    }
    out
}

/// `city_radius_contains_tile` (7795): tile (x, y) is in the work area of `city`.
pub fn city_radius_contains(world: &Civ3World, city: &Civ3City, x: i32, y: i32) -> bool {
    world.district_configs.work_area().iter().any(|&(dx, dy)| wrap(world, city.x + i32::from(dx), city.y + i32::from(dy)) == (x, y))
}

/// `City_has_improvement` with free buildings (docs §8): the city built the
/// building, or wonders give it to its owner (in all its cities, or in the
/// ones on the continent of the city: `Leader.Auto_Improvements`).
fn has_improvement(world: &Civ3World, city: &Civ3City, building: i32) -> bool {
    if city.has_improvement(building) {
        return true;
    }
    let Some(keys) = usize::try_from(city.civ).ok().and_then(|civ| world.free_improvements.get(civ)).filter(|k| !k.is_empty()) else {
        return false;
    };
    let continent = world.tile(city.x, city.y).map_or(0, |t| i32::from(t.continent));
    keys.contains(&building) || keys.contains(&((continent + 1) * world.improvements.len() as i32 + building))
}

/// `has_active_building` (17459): the city has the building, its government
/// allows it and its owner does not know the tech that makes it obsolete.
pub fn has_active_building(world: &Civ3World, city: &Civ3City, building: i32) -> bool {
    let rule = usize::try_from(building).ok().and_then(|b| world.improvements.get(b)).copied().unwrap_or_default();
    let civ = usize::try_from(city.civ).unwrap_or(0);
    has_improvement(world, city, building)
        && rule.government.is_none_or(|g| Some(g) == world.governments.get(civ).copied())
        && rule.obsolete_for & 1u32.wrapping_shl(civ as u32) == 0
}

/// `tile_coords_has_city_with_building_in_district_radius` (39331): a city
/// served by the district at (x, y) has the building. A district serves the
/// cities around it; a tile improvement (`type = tile-improvement`) only the
/// city whose area it is in.
pub fn building_serves(world: &Civ3World, x: i32, y: i32, kind: &DistrictKind, building: i32) -> bool {
    let Some(centre) = tile_at(world, x, y) else { return false };
    if centre.owner <= 0 {
        return false;
    }
    if kind.is_tile_improvement() {
        return centre.city_area >= 0
            && world
                .city(i32::from(centre.city_area))
                .is_some_and(|c| city_radius_contains(world, c, x, y) && has_active_building(world, c, building));
    }
    cities_around(world, x, y).into_iter().any(|c| has_active_building(world, c, building))
}

/// `count_completed_buildings_in_district_radius` (40243): how many of the
/// district's buildings some city it serves has.
pub fn count_buildings(world: &Civ3World, x: i32, y: i32, kind: &DistrictKind) -> i32 {
    kind.dependent_buildings.iter().filter(|&&b| b >= 0 && building_serves(world, x, y, kind, b)).count() as i32
}

/// `get_energy_grid_image_index` (39782): the column of the first of the
/// district's buildings some city has, else 0 (the bare grid).
pub fn energy_grid_column(world: &Civ3World, x: i32, y: i32, kind: &DistrictKind) -> i32 {
    kind.dependent_buildings.iter().position(|&b| b >= 0 && building_serves(world, x, y, kind, b)).map_or(0, |i| i as i32 + 1)
}
