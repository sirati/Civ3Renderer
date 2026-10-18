//! C3X's districts (docs §18): the sprites its replacement of
//! `Map_Renderer::m12_Draw_Tile_Buildings` draws for a district on a tile,
//! and what a district changes in the game's other map layers.
//!
//! A port of C3X's `draw_district_on_tile` and what it calls; line numbers in
//! the comments are those of C3X's `injected_code.c` (release 28 preview,
//! d4e71c4). C3X draws what the game holds now: the buildings in a city, the
//! neighbouring districts. It does not ask what the viewer has seen (its
//! `visible_to_civ_id` only decides which resources show, see `resources.rs`).

mod access;
mod port;
mod resources;
mod shapes;
mod source;
mod wonders;
#[cfg(test)]
mod tests;

use civ3_sys::c3x::{District, DistrictKind};
use civ3_sys::c3x_layout as X;

use crate::world::{Civ3Tile, Civ3World};
pub use source::{DistrictState, read};

/// Where the district sheets are: the folder of the season C3X starts in
/// (`Summer` without a seasonal cycle) and the hour its art is loaded for
/// (`1200`), from `init_district_images` (39134). The sheets are in C3X's own
/// folder (`DistrictConfigs::mod_dir`, relative to `Conquests`) unless the
/// scenario brings its own.
pub const ART_DIR: &str = "Art/Districts/Summer/1200";

/// One sprite of a district: the `size` pixels at `origin` of an image file,
/// its top-left `offset` pixels from the tile's 128×64 cell origin (the point
/// C3X calls `pixel_x`, `pixel_y`). `origin` and `size` are what C3X passes to
/// its slicing call (`Sprite_slice_pcx`).
///
/// `image` is the path C3X loads, relative to the art root, with the file name
/// as its config spells it (`Neighborhood_AMER.pcx`; the files on disk have
/// other letter cases). Besides the sheets under [`ART_DIR`] there are the
/// game's resource sheets, `Art/resources.pcx` (49×49 cells) and
/// `Art/resources_shadows.pcx`, whose sprites are to be drawn as the game
/// draws its resource shadows.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DistrictDraw {
    pub image: String,
    pub origin: (u32, u32),
    pub size: (u32, u32),
    pub offset: (i32, i32),
}

/// A district's sprites by the place C3X draws them in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stages {
    /// Where the game draws a tile's buildings (the replaced m12): the
    /// district, after the resource sprites if it is drawn over resources.
    pub buildings: Vec<DistrictDraw>,
    /// Leading resource sprites in `buildings`, kept distinguishable so a
    /// renderer can hide resources without hiding the district over them.
    pub building_resources: usize,
    /// Where the game draws a tile's resources (m09, after pollution and
    /// craters): C3X's resource sprites, but only where they differ from the
    /// game's own (see [`takes_over_resource`]).
    pub resources: Vec<DistrictDraw>,
}

/// The sprites C3X draws for the district on tile (x, y), in drawing order,
/// zoomed in; the caller halves them for the zoomed-out view. See [`stages`]
/// for where each is drawn, and [`draws_at`] for the zoomed-out numbers C3X
/// really uses.
pub fn draws(world: &Civ3World, x: i32, y: i32) -> Vec<DistrictDraw> {
    draws_at(world, x, y, false)
}

/// Like [`draws`], for either view. C3X does not scale what it adds to the
/// drawing position: zoomed out the `offset`s are the same as zoomed in (in
/// pixels of the zoomed-out cell, 64×32) and only the art is half size, so
/// tall sprites sit higher than half of their zoomed-in position would put
/// them. The generated resources are centred in the smaller cell (see
/// `resources::draws`).
pub fn draws_at(world: &Civ3World, x: i32, y: i32, zoomed_out: bool) -> Vec<DistrictDraw> {
    let stages = stages(world, x, y, zoomed_out);
    stages.buildings.into_iter().chain(stages.resources).collect()
}

/// The sprites of the district on tile (x, y) by where C3X draws them: its
/// m12 hook draws the district, after the tile's resources if the district
/// is drawn over resources; its m09 hook draws the resources otherwise.
pub fn stages(world: &Civ3World, x: i32, y: i32, zoomed_out: bool) -> Stages {
    let mut out = Stages::default();
    let Some(tile) = world.tile(x, y) else { return out };
    let Some(d) = present(world, tile) else { return out };
    let Some(kind) = access::kind_of(world, d) else { return out };
    let resources = resources::draws(world, tile, d, kind, zoomed_out);
    if kind.draw_over_resources {
        out.building_resources = resources.len();
        out.buildings = resources;
    } else {
        out.resources = resources;
    }
    out.buildings.extend(district(world, tile, kind, d, x, y));
    out
}

/// Whether C3X draws districts at all (its m12 hook is a plain call to the
/// game's otherwise).
fn drawing(world: &Civ3World) -> bool {
    world.district_configs.flags.any()
}

/// The district on `tile`, while C3X draws districts.
fn present<'a>(world: &'a Civ3World, tile: &Civ3Tile) -> Option<&'a District> {
    access::instance(world, tile).filter(|_| drawing(world))
}

// ---------------------------------------------------------------------------
// Which sprite.

/// The path of a sheet, as C3X names it (39134).
fn art(file: &str) -> String {
    format!("{ART_DIR}/{file}")
}

/// The sprite at (`column`, `row`) of a sheet of `cell`-sized sprites, drawn
/// `at` from the cell origin.
fn cell_draw(file: &str, column: i32, row: i32, cell: (i32, i32), at: (i32, i32)) -> DistrictDraw {
    DistrictDraw {
        image: art(file),
        origin: ((cell.0 * column).max(0) as u32, (cell.1 * row).max(0) as u32),
        size: (cell.0.max(0) as u32, cell.1.max(0) as u32),
        offset: at,
    }
}

/// The sprite of a district for sheet `variant` (culture, or the coast's),
/// `era` and `column`, as `init_district_images` slices them (39129-39169):
/// one sheet per variant, a row per era if the district varies by era, a
/// column per building or direction. `None` where C3X sliced none.
fn sprite(kind: &DistrictKind, variant: i32, era: i32, column: i32, at: (i32, i32)) -> Option<DistrictDraw> {
    let file = kind.images.get(usize::try_from(variant).ok()?).filter(|f| !f.is_empty())?;
    let eras = if kind.by_era { 4 } else { 1 };
    ((0..kind.columns).contains(&column) && (0..eras).contains(&era)).then(|| cell_draw(file, column, era, kind.cell_size(), at))
}

/// The civ's era, 0..3.
fn era_of(world: &Civ3World, civ: i32) -> i32 {
    usize::try_from(civ).ok().and_then(|c| world.eras.get(c)).copied().unwrap_or(0).clamp(0, 3)
}

/// The civ's culture group, `Races[leader.RaceID].CultureGroupID` (40323).
fn culture_of(world: &Civ3World, civ: i32) -> i32 {
    let race = usize::try_from(civ).ok().and_then(|c| world.races.get(c)).copied().unwrap_or(0);
    usize::try_from(race).ok().and_then(|r| world.culture_groups.get(r)).copied().unwrap_or(0)
}

/// The Neighborhood's column, 0..3: a hash of its tile (40420-40426).
pub fn neighborhood_column(x: i32, y: i32) -> i32 {
    let mut v = (x as u32).wrapping_mul(0x9E37_79B1).wrapping_add((y as u32).wrapping_mul(0x85EB_CA6B));
    v ^= v >> 16;
    v = v.wrapping_mul(0x7FEB_352D);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846C_A68B);
    v ^= v >> 16;
    (v & 3) as i32
}

/// `draw_district_on_tile` (40276) without the resources: the sprites of
/// district `d` on the tile at (x, y).
fn district(world: &Civ3World, tile: &Civ3Tile, kind: &DistrictKind, d: &District, x: i32, y: i32) -> Vec<DistrictDraw> {
    let flags = &world.district_configs.flags;
    if d.kind == X::NATURAL_WONDER_DISTRICT_ID {
        return wonders::natural_wonder(world, d);
    }
    if !flags.districts || !access::is_complete(world, tile, d) {
        return Vec::new();
    }

    let owner = i32::from(tile.owner);
    let (mut variant, mut era, mut culture) = (0, 0, 0);
    let mut moved = (0, 0);
    if owner > 0 {
        // In a territory: its owner's culture and era.
        culture = culture_of(world, owner);
        if kind.by_culture {
            variant = culture;
        }
        if kind.by_era {
            era = era_of(world, owner);
        }
        if kind.align_to_coast {
            let (sheet, shift) = port::align(world, tile, x, y);
            variant = sheet.unwrap_or(variant);
            moved = shift;
        }
    } else if ![X::WONDER_DISTRICT_ID, X::NATURAL_WONDER_DISTRICT_ID, X::BRIDGE_DISTRICT_ID, X::CANAL_DISTRICT_ID].contains(&d.kind) {
        // Nobody's: abandoned (40332), a land or a maritime sprite, moved by
        // the district's offset but not by its size.
        let maritime = access::is_water(tile);
        return vec![cell_draw("Abandoned.pcx", maritime as i32, 0, (128, 64), kind.offset)];
    }
    if owner < 0 && d.built_by >= 0 {
        // Outside any territory: its builder's.
        culture = culture_of(world, d.built_by);
        if kind.by_culture {
            variant = culture;
        }
        if kind.by_era {
            era = era_of(world, d.built_by);
        }
    }

    // The sprite is centred on the tile and bottom-aligned to it (40352).
    let (w, h) = kind.cell_size();
    let at = (moved.0 + kind.offset.0 - (w - 128) / 2, moved.1 + kind.offset.1 - (h - 64));
    let one = |variant: i32, column: i32| sprite(kind, variant, era, column, at).into_iter().collect::<Vec<_>>();
    match d.kind {
        X::WONDER_DISTRICT_ID if flags.wonder => match wonders::wonder_art(world, tile, kind, d, owner, moved, x, y) {
            wonders::Art::Plain => one(variant, 0),
            wonders::Art::Draw(draw) => vec![draw],
            wonders::Art::Nothing => Vec::new(),
        },
        // The Neighborhood picks its sheet by culture whatever its config says.
        X::NEIGHBORHOOD_DISTRICT_ID if flags.neighborhood => one(culture, neighborhood_column(x, y)),
        X::DISTRIBUTION_HUB_DISTRICT_ID if flags.distribution_hub => one(variant, 0),
        X::ENERGY_GRID_DISTRICT_ID if flags.energy_grid => one(variant, access::energy_grid_column(world, x, y, kind)),
        X::BRIDGE_DISTRICT_ID if flags.bridge => one(variant, shapes::bridge_column(world, tile, x, y)),
        X::CANAL_DISTRICT_ID if flags.canal => shapes::canal_draws(world, kind, era, x, y),
        X::GREAT_WALL_DISTRICT_ID if flags.great_wall => shapes::great_wall_draws(world, kind, tile, x, y),
        // (Switched off.)
        X::WONDER_DISTRICT_ID
        | X::NEIGHBORHOOD_DISTRICT_ID
        | X::DISTRIBUTION_HUB_DISTRICT_ID
        | X::ENERGY_GRID_DISTRICT_ID
        | X::BRIDGE_DISTRICT_ID
        | X::CANAL_DISTRICT_ID
        | X::GREAT_WALL_DISTRICT_ID => Vec::new(),
        // Every other district: a column for how many of its buildings the
        // cities around have, or a layer for each of them.
        _ if i64::from(kind.render_strategy) == X::DRS_BY_COUNT => one(variant, access::count_buildings(world, x, y, kind)),
        _ if i64::from(kind.render_strategy) == X::DRS_BY_BUILDING => {
            let layers = kind.dependent_buildings.iter().enumerate().filter(|&(_, &b)| b >= 0 && access::building_serves(world, x, y, kind, b));
            one(variant, 0).into_iter().chain(layers.filter_map(|(i, _)| sprite(kind, variant, era, i as i32 + 1, at))).collect()
        }
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// What a district changes in the game's other layers.

/// Whether the district on tile (x, y) replaces the tile's buildings (the
/// game's `m12_Draw_Tile_Buildings`: ruins, airfields, mines, fortresses,
/// colonies, victory markers, ...). C3X's replacement draws them only on
/// tiles without a district; with districts off (natural wonders on) it
/// draws them under every district but the natural wonders.
pub fn replaces_tile_buildings(world: &Civ3World, x: i32, y: i32) -> bool {
    let Some(tile) = world.tile(x, y) else { return false };
    present(world, tile).is_some_and(|d| d.kind == X::NATURAL_WONDER_DISTRICT_ID || world.district_configs.flags.districts)
}

/// Whether C3X draws the resource of `tile` itself ([`Stages::resources`],
/// or before the district) instead of the game's resource layer: the
/// district is drawn over resources, subsumes the tile's resource or
/// generates one.
pub fn takes_over_resource(world: &Civ3World, tile: &Civ3Tile) -> bool {
    let Some(d) = present(world, tile) else { return false };
    access::kind_of(world, d).is_some_and(|kind| resources::takes_over(world, tile, d, kind))
}

/// A bridge has no road or railroad drawn on its tile, finished or not
/// (`patch_Map_Renderer_m52_Draw_Roads` and `_Railroads`, 27404, 27426): its
/// own art has them.
pub fn hides_roads(world: &Civ3World, tile: &Civ3Tile) -> bool {
    let flags = &world.district_configs.flags;
    flags.districts && flags.bridge && access::instance(world, tile).is_some_and(|d| d.kind == X::BRIDGE_DISTRICT_ID)
}

/// A finished district that can be an irrigation source has no irrigation
/// drawn on its tile (`patch_Map_Renderer_m11_Draw_Tile_Irrigation`, 40588).
pub fn hides_irrigation(world: &Civ3World, tile: &Civ3Tile) -> bool {
    let districts_on = world.district_configs.flags.districts;
    access::instance(world, tile)
        .is_some_and(|d| districts_on && access::kind_of(world, d).is_some_and(|k| k.allow_irrigation_from) && access::is_complete(world, tile, d))
}

/// A district next to a river vertex makes the river "special", drawn under
/// the objects like it is next to a city: C3X has the four `Tile_has_city`
/// calls of the river drawing ask for districts too (`patch_Tile_has_city_or_district`,
/// 40609; the call sites are in civ_prog_objects.csv:872-875). Any district,
/// finished or not.
pub fn makes_river_special(world: &Civ3World, tile: &Civ3Tile) -> bool {
    present(world, tile).is_some()
}
