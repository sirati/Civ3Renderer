//! Wonder districts and natural wonders.

use civ3_sys::c3x::{District, DistrictKind, WonderKind};
use civ3_sys::c3x_layout as X;

use super::access::*;
use super::{DistrictDraw, cell_draw};
use crate::world::{Civ3Tile, Civ3World};

/// What a wonder district shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Art {
    /// The plain wonder district: no wonder, or none being built.
    Plain,
    Draw(DistrictDraw),
    /// C3X draws nothing (a finished wonder it has no art for).
    Nothing,
}

/// `direction_to_neighbor_bit` (4443): the bit of the river code (`Tile`
/// `River_Code`: 0 N, 1 NE, 2 E, 3 SE, 4 S, 5 SW, 6 W, 7 NW) for a direction.
fn river_bit(dir: usize) -> u32 {
    if dir == N { 0 } else { dir as u32 }
}

/// `get_primary_river_direction` (4460): the direction of the tile's river
/// that C3X prefers (E, W, then the diagonals, then S, N).
pub fn primary_river_direction(tile: &Civ3Tile) -> Option<usize> {
    [E, W, SE, NE, SW, NW, S, N].into_iter().find(|&dir| tile.river & (1 << river_bit(dir)) != 0)
}

/// `align_district_with_river` (39388): a district on a river moves towards
/// it. In x by the tile step of the direction (0, 1 or 2 pixels, not a pixel
/// distance), in y by 36 pixels scaled by the direction.
pub fn river_shift(tile: &Civ3Tile) -> (i32, i32) {
    let Some(dir) = primary_river_direction(tile) else { return (0, 0) };
    let dy = match dir {
        N => -36,
        NE | NW => -18,
        SE | SW => 18,
        S => 36,
        _ => 0,
    };
    (step(dir).0, dy)
}

/// `wonder_district_tile_under_construction` (17195): the wonder (index in
/// the wonder configs) of the city that reserved this district, if that
/// city still builds a wonder (the wonder's config is found by its name) and
/// the tile is in its work area.
fn wonder_under_construction(world: &Civ3World, d: &District, x: i32, y: i32) -> Option<usize> {
    if i64::from(d.wonder_state) != X::WDS_UNDER_CONSTRUCTION {
        return None;
    }
    let city = world.city(d.wonder_city)?;
    let improvement = city.wonder_in_production?;
    if !city_radius_contains(world, city, x, y) {
        return None;
    }
    world.district_configs.wonders.iter().position(|w| w.improvement == Some(improvement))
}

/// `wonder_should_use_alternative_direction_image` (39680): whether the
/// wonder faces east (the alternative art; the plain art faces west). Along
/// its river if it may stand on one, else away from the nearest city of its
/// owner.
fn faces_east(world: &Civ3World, tile: &Civ3Tile, wonder: &WonderKind, owner: i32, x: i32, y: i32) -> bool {
    if owner <= 0 {
        return false;
    }
    if wonder.requires_river()
        && let Some(dir) = primary_river_direction(tile)
    {
        // "Seems to match the vanilla behaviour" (C3X): due east is not east.
        let dx = step(dir).0;
        return dx != 2 && dx > 0;
    }
    let (mut best_dist, mut best_dx) = (i32::MAX, 0i32);
    // The city's own coordinates (C3X compares those, not the wrapped
    // distance) and none: (0, 0).
    let mut city_at = (0, 0);
    for city in cities_around(world, x, y) {
        if city.civ != owner {
            continue;
        }
        let (mut dx, mut dy) = (city.x - x, city.y - y);
        if world.wrap_x {
            let half = world.width >> 1;
            if dx > half {
                dx -= world.width;
            } else if dx < -half {
                dx += world.width;
            }
        }
        if world.wrap_y {
            let half = world.height >> 1;
            if dy > half {
                dy -= world.height;
            } else if dy < -half {
                dy += world.height;
            }
        }
        let dist = dx.abs() + dy.abs();
        // The nearest; of equally near ones, the one more clearly east or west.
        if dist < best_dist || (dist == best_dist && (dx.abs() < best_dx.abs() || best_dx == 0)) {
            (best_dist, best_dx, city_at) = (dist, dx, (city.x, city.y));
        }
    }
    // A city straight above or below the wonder: east if there is land to
    // the east.
    if city_at.0 == x && (city_at.1 == y - 2 || city_at.1 == y + 2) {
        let land = |dx: i32, dy: i32| tile_is_land(world, owner, x + dx, y + dy, true);
        if land(1, -1) || land(2, 0) || land(1, 1) {
            return true;
        }
    }
    best_dist != i32::MAX && best_dx > 0
}

/// A wonder district (`draw_district_on_tile`, 40362): the finished wonder,
/// the building site of the wonder its city is building, or the plain
/// wonder district. `moved` is where a coast would have moved the district
/// (never, for this kind), `owner` the tile's owner.
pub fn wonder_art(world: &Civ3World, tile: &Civ3Tile, kind: &DistrictKind, d: &District, owner: i32, moved: (i32, i32), x: i32, y: i32) -> Art {
    let configs = &world.district_configs;
    let (index, finished) = if i64::from(d.wonder_state) == X::WDS_COMPLETED {
        match usize::try_from(d.wonder).ok().filter(|&i| i < configs.wonders.len()) {
            Some(i) => (i, true),
            None => return Art::Nothing,
        }
    } else if let Some(i) = wonder_under_construction(world, d, x, y) {
        (i, false)
    } else {
        return Art::Plain;
    };
    let wonder = &configs.wonders[index];
    let moved = if wonder.requires_river() {
        let (dx, dy) = river_shift(tile);
        (moved.0 + dx, moved.1 + dy)
    } else {
        moved
    };
    // The wonder's own size, the district's offset (40399).
    let (w, h) = wonder.cell_size();
    let at = (moved.0 + kind.offset.0 - (w - 128) / 2, moved.1 + kind.offset.1 - (h - 64));
    let east = wonder.alt.is_some() && faces_east(world, tile, wonder, owner, x, y);
    let (cell, site) = match wonder.alt {
        Some((alt, alt_site)) if east => (alt, alt_site),
        _ => (wonder.cell, wonder.construct_cell),
    };
    let (column, row) = if finished { cell } else { site };
    // (A wonder without an image path uses C3X's `Wonders.pcx`, 40212.)
    let file = if wonder.image.is_empty() { "Wonders.pcx" } else { &wonder.image };
    Art::Draw(cell_draw(file, column, row, (w, h), at))
}

/// A natural wonder (`draw_district_on_tile`, 40286): its 128×88 cell,
/// bottom-aligned to the tile. Drawn whether or not it counts as complete.
pub fn natural_wonder(world: &Civ3World, d: &District) -> Vec<DistrictDraw> {
    let configs = &world.district_configs;
    let wonder = usize::try_from(d.natural_wonder).ok().and_then(|i| configs.natural_wonders.get(i));
    match wonder {
        Some(n) if configs.flags.natural_wonders && !n.name.is_empty() => vec![cell_draw(&n.image, n.cell.0, n.cell.1, (128, 88), (0, -(88 - 64)))],
        _ => Vec::new(),
    }
}
