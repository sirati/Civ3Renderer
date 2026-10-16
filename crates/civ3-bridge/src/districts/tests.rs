//! Tests of the districts: the choices that depend on the neighbours are
//! compared with C3X's own code over every combination of their inputs
//! (checksums from `tools/district-oracle`, C3X d4e71c4), the rest is checked
//! on small hand-built worlds.

use std::sync::Arc;

use civ3_sys::c3x::{District, DistrictConfigs, DistrictFlags, DistrictKind, NaturalWonderKind, WonderKind};

use super::port::{Anchor, coast_choice};
use super::shapes::{bridge_choice, canal_choice};
use super::*;
use crate::rules::{self, overlay, terrain};
use crate::world::{Civ3City, Civ3Improvement, Civ3Tile, Civ3World};

// ---------------------------------------------------------------------------
// Against C3X's code.

const FNV_START: u64 = 0xcbf2_9ce4_8422_2325;

fn fnv(h: u64, v: i32) -> u64 {
    (h ^ u64::from(v as u32)).wrapping_mul(0x100_0000_01b3)
}

/// Bit `base + d - 1` of `mask` for the directions d = 1..8 (index 0 unused).
fn bits(mask: u32, base: u32) -> [bool; 9] {
    std::array::from_fn(|d| d > 0 && mask >> (base + d as u32 - 1) & 1 != 0)
}

#[test]
fn neighborhood_column_is_c3x_hash() {
    // From C3X's formula (40420), compiled and run.
    for ((x, y), column) in [((0, 0), 0), ((1, 1), 0), ((2, 0), 1), ((10, 10), 2), ((11, 9), 1), ((20, 18), 3), ((33, 7), 1), ((64, 64), 2), ((99, 51), 1), ((127, 127), 1), ((-3, 5), 1)] {
        assert_eq!(neighborhood_column(x, y), column, "tile ({x}, {y})");
    }
    let mut h = FNV_START;
    for x in 0..128 {
        for y in 0..128 {
            h = fnv(h, neighborhood_column(x, y));
        }
    }
    assert_eq!(h, 0x1b57_8dc5_e64b_2181);
}

#[test]
fn bridge_choice_is_c3x_for_every_neighbourhood() {
    let mut h = FNV_START;
    for mask in 0..1u32 << 17 {
        h = fnv(h, bridge_choice(&bits(mask, 0), &bits(mask, 8), mask >> 16 & 1 != 0));
    }
    assert_eq!(h, 0x6034_3ba5_7ed8_9bf5);
}

#[test]
fn canal_choice_is_c3x_for_every_neighbourhood() {
    let mut h = FNV_START;
    for mask in 0..1u32 << 16 {
        let (one, two) = canal_choice(&bits(mask, 0), &bits(mask, 8));
        h = fnv(fnv(h, one), two);
    }
    assert_eq!(h, 0x0db1_c45a_0cea_1997);
}

#[test]
fn coast_choice_is_c3x_for_random_neighbourhoods() {
    let mut lcg = 12345u64;
    let mut next = || {
        lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (lcg >> 33) as u32
    };
    let mut h = FNV_START;
    for _ in 0..300_000 {
        let cx = (next() % 23) as i32 - 11;
        let mut cy = (next() % 23) as i32 - 11;
        if (cx + cy) & 1 != 0 {
            cy += 1;
        }
        let (mut same, mut any, mut anchors) = ([false; 9], [false; 9], [Anchor::NONE; 9]);
        for d in 1..=8 {
            same[d] = next() & 1 != 0;
            any[d] = next() & 1 != 0;
            anchors[d].rough = next() % 4 == 0;
            anchors[d].sheet = (next() % 7) as i32 - 1;
            anchors[d].sprite = (next() % 80) as i32;
        }
        let coast = coast_choice((cx, cy), &same, &any, &anchors);
        h = fnv(fnv(fnv(h, coast.variant), coast.shift.0), coast.shift.1);
    }
    assert_eq!(h, 0x5db9_8853_c277_7aac);
}

// ---------------------------------------------------------------------------
// Small worlds.

const WIDTH: i32 = 40;
const HEIGHT: i32 = 40;

/// Both the underlying and the effective terrain.
fn ground(terrain: u32) -> u32 {
    terrain << 12 | terrain << 8
}

fn kind(name: &str, images: &[&str], columns: i32) -> DistrictKind {
    DistrictKind {
        name: name.to_owned(),
        images: images.iter().map(|s| s.to_string()).collect(),
        columns,
        by_era: true,
        generated_resource: -1,
        resource_prereq_on_tile: -1,
        ..Default::default()
    }
}

const CULTURES: [&str; 5] = ["AMER", "EURO", "ROMAN", "MIDEAST", "ASIAN"];

fn by_culture(name: &str, columns: i32) -> DistrictKind {
    let files: Vec<String> = CULTURES.iter().map(|c| format!("{name}_{c}.pcx")).collect();
    DistrictKind { by_culture: true, ..kind(name, &files.iter().map(String::as_str).collect::<Vec<_>>(), columns) }
}

// Improvement ids of the tests.
const TEMPLE: i32 = 3;
const CATHEDRAL: i32 = 4;
const COURTHOUSE: i32 = 5;
const POLICE: i32 = 6;
const HOSPITAL: i32 = 7;
const GRANARY: i32 = 8;
const HARBOR: i32 = 10;
const COAL_PLANT: i32 = 40;
const HYDRO_PLANT: i32 = 41;
const ARTEMIS: i32 = 60;

// District kinds beyond the special ones.
const HOLY_SITE: i32 = 11;
const MUNICIPAL: i32 = 12;
const FARM: i32 = 13;
const OIL_RIG: i32 = 14;

/// C3X's special districts as its defaults make them (C3X.h:1205), then a
/// few like the ones in default.districts_config.txt.
fn configs() -> DistrictConfigs {
    let mut districts = vec![
        by_culture("Neighborhood", 4),
        kind("Wonder District", &["WonderDistrict.pcx"], 1),
        kind("Distribution Hub", &["DistributionHub.pcx"], 1),
        DistrictKind { dependent_buildings: vec![20], ..kind("Aerodrome", &["Aerodrome.pcx"], 2) },
        DistrictKind { by_era: false, columns: 0, ..kind("Natural Wonder", &[], 0) },
        DistrictKind {
            align_to_coast: true,
            dependent_buildings: vec![HARBOR, 11],
            ..kind("Port", &["Port_NW.pcx", "Port_NE.pcx", "Port_SE.pcx", "Port_SW.pcx"], 4)
        },
        by_culture("CentralRailHub", 2),
        DistrictKind {
            size: (0, 88),
            dependent_buildings: vec![COAL_PLANT, HYDRO_PLANT],
            ..kind("Energy Grid", &["EnergyGrid.pcx"], 3)
        },
        DistrictKind { size: (176, 112), offset: (0, 24), ..kind("Bridge", &["Bridge.pcx"], 9) },
        DistrictKind { size: (176, 112), offset: (0, 24), ..kind("Canal", &["Canal.pcx"], 9) },
        DistrictKind { by_era: false, size: (0, 88), draw_over_resources: true, ..kind("Great Wall", &["GreatWall.pcx"], 9) },
    ];
    districts.push(DistrictKind { by_era: false, dependent_buildings: vec![TEMPLE, CATHEDRAL], ..by_culture("HolySite", 3) });
    districts.push(DistrictKind {
        render_strategy: 1,
        size: (0, 88),
        dependent_buildings: vec![COURTHOUSE, POLICE, HOSPITAL],
        ..by_culture("Municipal", 4)
    });
    districts.push(DistrictKind {
        kind: 1,
        by_era: false,
        allow_irrigation_from: true,
        dependent_buildings: vec![GRANARY],
        ..kind("Farm", &["Farm.pcx"], 2)
    });
    districts.push(DistrictKind {
        by_era: false,
        generated_resource: 2,
        ..kind("Oil Rig", &["OilRig.pcx"], 1)
    });
    DistrictConfigs {
        flags: DistrictFlags {
            districts: true,
            natural_wonders: true,
            neighborhood: true,
            wonder: true,
            distribution_hub: true,
            energy_grid: true,
            bridge: true,
            canal: true,
            great_wall: true,
        },
        districts,
        wonders: vec![WonderKind {
            name: "The Temple of Artemis".to_owned(),
            image: "Wonders.pcx".to_owned(),
            cell: (3, 1),
            construct_cell: (2, 1),
            alt: Some(((5, 1), (4, 1))),
            improvement: Some(ARTEMIS),
            ..Default::default()
        }],
        natural_wonders: vec![NaturalWonderKind { name: "Angel Falls".to_owned(), image: "NaturalWonders.pcx".to_owned(), cell: (1, 2) }],
        workable_tiles: 21,
        mod_dir: "C3X_R28".to_owned(),
    }
}

/// A plains map; civ 1 owns everything. Eras by civ: 0 ancient, civ 1 is in
/// the second era, civ 2 in the third. Civ 1's culture group is EURO, civ
/// 2's ROMAN.
fn world() -> Civ3World {
    // Seen by civ 1 now, so what is on the tiles is what it sees.
    let tile = Civ3Tile { square_type: ground(terrain::PLAINS), resource: -1, city: -1, city_area: -1, owner: 1, fow_status: 1 << 1, ..Default::default() };
    Civ3World {
        player: 1,
        width: WIDTH,
        height: HEIGHT,
        tiles: vec![tile; (WIDTH * HEIGHT / 2) as usize],
        eras: vec![0, 1, 2],
        races: vec![0, 1, 2],
        culture_groups: vec![0, 1, 2],
        governments: vec![0, 3, 3],
        improvements: vec![Civ3Improvement::default(); 64],
        district_configs: Arc::new(configs()),
        ..Default::default()
    }
}

fn tile_mut(w: &mut Civ3World, x: i32, y: i32) -> &mut Civ3Tile {
    let i = w.index(x, y);
    &mut w.tiles[i]
}

fn configs_mut(w: &mut Civ3World) -> &mut DistrictConfigs {
    Arc::make_mut(&mut w.district_configs)
}

fn instance(kind: i32, x: i32, y: i32) -> District {
    District { x, y, kind, completed: true, built_by: 1, wonder_state: 0, wonder: -1, wonder_city: -1, natural_wonder: -1 }
}

fn add_district(w: &mut Civ3World, kind: i32, x: i32, y: i32) {
    let mut all = w.districts.clone();
    all.push(instance(kind, x, y));
    w.set_districts(all);
}

fn edit_district(w: &mut Civ3World, x: i32, y: i32, f: impl FnOnce(&mut District)) {
    let mut all = w.districts.clone();
    f(all.iter_mut().find(|d| (d.x, d.y) == (x, y)).unwrap());
    w.set_districts(all);
}

fn add_city(w: &mut Civ3World, id: i32, x: i32, y: i32, civ: i32, buildings: &[i32]) {
    let mut city = Civ3City { id, x, y, civ, size: 5, ..Default::default() };
    for &b in buildings {
        city.improvements[b as usize >> 3] |= 1 << (b & 7);
    }
    w.cities.push(city);
    let tile = tile_mut(w, x, y);
    tile.city = id as i16;
    tile.owner = civ as i8;
}

fn water(w: &mut Civ3World, x: i32, y: i32) {
    tile_mut(w, x, y).square_type = ground(terrain::COAST);
}

/// A sprite of a district sheet.
fn sheet(file: &str, origin: (u32, u32), size: (u32, u32), offset: (i32, i32)) -> DistrictDraw {
    DistrictDraw { image: format!("Art/Districts/Summer/1200/{file}"), origin, size, offset }
}

#[test]
fn nothing_without_c3x_districts() {
    let mut w = world();
    add_district(&mut w, HOLY_SITE, 10, 10);
    w.district_configs = Arc::default();
    assert!(draws(&w, 10, 10).is_empty());
    assert!(!replaces_tile_buildings(&w, 10, 10));
    let tile = *w.tile(10, 10).unwrap();
    assert!(!takes_over_resource(&w, &tile) && !hides_roads(&w, &tile) && !hides_irrigation(&w, &tile) && !makes_river_special(&w, &tile));
    // And a world that never heard of districts has none to find.
    assert!(draws(&Civ3World::default(), 0, 0).is_empty());
}

#[test]
fn by_count_counts_buildings_that_are_not_obsolete_and_allowed() {
    let mut w = world();
    add_district(&mut w, HOLY_SITE, 10, 10);
    // Civ 1's city two tiles east has both buildings; civ 2's city nearby has
    // one more of each, to no avail.
    add_city(&mut w, 1, 12, 10, 1, &[TEMPLE, CATHEDRAL]);
    add_city(&mut w, 2, 9, 9, 2, &[TEMPLE, CATHEDRAL]);
    let column = |column: u32| vec![sheet("HolySite_EURO.pcx", (128 * column, 0), (128, 64), (0, 0))];
    assert_eq!(draws(&w, 10, 10), column(2));

    // The cathedral is obsolete for civ 1 (it knows the tech), the temple is
    // for another government than civ 1's.
    w.improvements[CATHEDRAL as usize].obsolete_for = 1 << 1;
    assert_eq!(draws(&w, 10, 10), column(1));
    w.improvements[TEMPLE as usize].government = Some(2);
    assert_eq!(draws(&w, 10, 10), column(0));
    w.improvements[TEMPLE as usize].government = Some(3);
    assert_eq!(draws(&w, 10, 10), column(1));
    // Another civ knowing the tech changes nothing.
    w.improvements[CATHEDRAL as usize].obsolete_for = 1 << 2;
    assert_eq!(draws(&w, 10, 10), column(2));
    // A city out of reach (beyond the 21 tiles of the work area) does not count.
    w.cities[0].x = 16;
    w.cities[0].y = 10;
    tile_mut(&mut w, 12, 10).city = -1;
    tile_mut(&mut w, 16, 10).city = 1;
    assert_eq!(draws(&w, 10, 10), column(0));
}

#[test]
fn by_building_layers_the_buildings_over_the_base() {
    let mut w = world();
    add_district(&mut w, MUNICIPAL, 10, 10);
    add_city(&mut w, 1, 12, 10, 1, &[COURTHOUSE, HOSPITAL]);
    // 128×88 cells, a row per era (civ 1: the second); bottom-aligned, 24 up.
    let layer = |column: u32| sheet("Municipal_EURO.pcx", (128 * column, 88), (128, 88), (0, -24));
    assert_eq!(draws(&w, 10, 10), vec![layer(0), layer(1), layer(3)]);
}

#[test]
fn tile_improvements_count_only_the_city_that_works_them() {
    let mut w = world();
    add_district(&mut w, FARM, 10, 10);
    add_city(&mut w, 1, 12, 10, 1, &[]);
    add_city(&mut w, 2, 8, 10, 1, &[GRANARY]);
    let column = |c: u32| vec![sheet("Farm.pcx", (128 * c, 0), (128, 64), (0, 0))];
    // Not assigned to a city: nobody's buildings count.
    assert_eq!(draws(&w, 10, 10), column(0));
    tile_mut(&mut w, 10, 10).city_area = 1;
    assert_eq!(draws(&w, 10, 10), column(0));
    tile_mut(&mut w, 10, 10).city_area = 2;
    assert_eq!(draws(&w, 10, 10), column(1));
}

#[test]
fn abandoned_districts_are_land_or_maritime() {
    let mut w = world();
    add_district(&mut w, HOLY_SITE, 10, 10);
    add_district(&mut w, X::PORT_DISTRICT_ID, 12, 10);
    water(&mut w, 12, 10);
    tile_mut(&mut w, 10, 10).owner = 0;
    tile_mut(&mut w, 12, 10).owner = 0;
    assert_eq!(draws(&w, 10, 10), vec![sheet("Abandoned.pcx", (0, 0), (128, 64), (0, 0))]);
    assert_eq!(draws(&w, 12, 10), vec![sheet("Abandoned.pcx", (128, 0), (128, 64), (0, 0))]);
    // Moved by the district's offset, not by its size.
    configs_mut(&mut w).districts[HOLY_SITE as usize].offset = (3, -5);
    configs_mut(&mut w).districts[HOLY_SITE as usize].size = (128, 88);
    assert_eq!(draws(&w, 10, 10), vec![sheet("Abandoned.pcx", (0, 0), (128, 64), (3, -5))]);
}

#[test]
fn bridges_and_canals_are_not_abandoned() {
    let mut w = world();
    add_district(&mut w, X::BRIDGE_DISTRICT_ID, 10, 10);
    water(&mut w, 10, 10);
    tile_mut(&mut w, 10, 10).owner = 0;
    // Era 0, the first culture; column 0 (SW-NE, the default).
    assert_eq!(draws(&w, 10, 10), vec![sheet("Bridge.pcx", (0, 0), (176, 112), (-24, -24))]);
    // Outside any territory, its builder's era shows.
    tile_mut(&mut w, 10, 10).owner = -1;
    assert_eq!(draws(&w, 10, 10), vec![sheet("Bridge.pcx", (0, 112), (176, 112), (-24, -24))]);
    edit_district(&mut w, 10, 10, |d| d.built_by = 2);
    assert_eq!(draws(&w, 10, 10), vec![sheet("Bridge.pcx", (0, 224), (176, 112), (-24, -24))]);
}

#[test]
fn era_and_culture_are_the_owners() {
    let mut w = world();
    add_district(&mut w, X::CENTRAL_RAIL_HUB_DISTRICT_ID, 10, 10);
    // Civ 1: EURO, second era row.
    assert_eq!(draws(&w, 10, 10), vec![sheet("CentralRailHub_EURO.pcx", (0, 64), (128, 64), (0, 0))]);
    tile_mut(&mut w, 10, 10).owner = 2;
    assert_eq!(draws(&w, 10, 10), vec![sheet("CentralRailHub_ROMAN.pcx", (0, 128), (128, 64), (0, 0))]);
}

#[test]
fn neighborhoods_pick_their_culture_and_hashed_column() {
    let mut w = world();
    add_district(&mut w, X::NEIGHBORHOOD_DISTRICT_ID, 10, 10);
    add_district(&mut w, X::NEIGHBORHOOD_DISTRICT_ID, 11, 9);
    // (10, 10) hashes to column 2, (11, 9) to 1.
    assert_eq!(draws(&w, 10, 10), vec![sheet("Neighborhood_EURO.pcx", (256, 64), (128, 64), (0, 0))]);
    assert_eq!(draws(&w, 11, 9), vec![sheet("Neighborhood_EURO.pcx", (128, 64), (128, 64), (0, 0))]);
    configs_mut(&mut w).flags.neighborhood = false;
    assert!(draws(&w, 10, 10).is_empty());
}

#[test]
fn the_energy_grid_shows_the_first_building_a_city_has() {
    let mut w = world();
    add_district(&mut w, X::ENERGY_GRID_DISTRICT_ID, 10, 10);
    let column = |c: u32| vec![sheet("EnergyGrid.pcx", (128 * c, 88), (128, 88), (0, -24))];
    assert_eq!(draws(&w, 10, 10), column(0));
    add_city(&mut w, 1, 12, 10, 1, &[HYDRO_PLANT]);
    assert_eq!(draws(&w, 10, 10), column(2));
    w.cities[0].improvements[COAL_PLANT as usize >> 3] |= 1 << (COAL_PLANT & 7);
    assert_eq!(draws(&w, 10, 10), column(1));
}

#[test]
fn bridges_run_along_their_neighbours_or_towards_land() {
    let mut w = world();
    add_district(&mut w, X::BRIDGE_DISTRICT_ID, 10, 10);
    for (x, y) in [(10, 10), (11, 9), (9, 11), (10, 8), (10, 12), (12, 10), (8, 10), (9, 9), (11, 11)] {
        water(&mut w, x, y);
    }
    let column = |w: &Civ3World, c: u32| draws(w, 10, 10) == vec![sheet("Bridge.pcx", (176 * c, 112), (176, 112), (-24, -24))];
    // Nothing around: SW-NE.
    assert!(column(&w, 0));
    // Land to the N: N-S.
    tile_mut(&mut w, 10, 8).square_type = ground(terrain::PLAINS);
    assert!(column(&w, 2));
    // Land at the NW and SE as well: NW-SE comes first.
    tile_mut(&mut w, 9, 9).square_type = ground(terrain::PLAINS);
    tile_mut(&mut w, 11, 11).square_type = ground(terrain::PLAINS);
    assert!(column(&w, 1));
    // Bridges at W and E win over land: W-E.
    add_district(&mut w, X::BRIDGE_DISTRICT_ID, 8, 10);
    add_district(&mut w, X::BRIDGE_DISTRICT_ID, 12, 10);
    assert!(column(&w, 3));
    // A railroad on the bridge moves to the second four.
    tile_mut(&mut w, 10, 10).overlays |= overlay::RAILROAD;
    assert!(column(&w, 7));
    // A bridge still being built is no neighbour.
    edit_district(&mut w, 12, 10, |d| d.completed = false);
    edit_district(&mut w, 8, 10, |d| d.completed = false);
    assert!(column(&w, 5));
}

#[test]
fn canals_reach_water_with_a_second_sprite() {
    let mut w = world();
    add_district(&mut w, X::CANAL_DISTRICT_ID, 10, 10);
    water(&mut w, 11, 9);
    // 176×112 cells, offset (0, 24): the canal runs NE, towards the water
    // that lies there, and is drawn again 18 pixels right and 9 up.
    let at = |dx: i32, dy: i32| sheet("Canal.pcx", (176, 112), (176, 112), (-24 + dx, -24 + dy));
    assert_eq!(draws(&w, 10, 10), vec![at(0, 0), at(18, -9)]);
    // Water at the SW as well: both ends, NE first (the fixed pair).
    water(&mut w, 9, 11);
    let sw = |dx: i32, dy: i32| sheet("Canal.pcx", (5 * 176, 112), (176, 112), (-24 + dx, -24 + dy));
    assert_eq!(draws(&w, 10, 10), vec![at(0, 0), at(18, -9), sw(0, 0), sw(-18, 9)]);
    // A canal next to this one decides the direction.
    add_district(&mut w, X::CANAL_DISTRICT_ID, 12, 10);
    let e = sheet("Canal.pcx", (2 * 176, 112), (176, 112), (-24, -24));
    assert_eq!(draws(&w, 10, 10)[0], e);
}

#[test]
fn the_great_wall_is_drawn_between_its_pieces_and_over_resources() {
    let mut w = world();
    add_district(&mut w, X::GREAT_WALL_DISTRICT_ID, 10, 10);
    add_district(&mut w, X::GREAT_WALL_DISTRICT_ID, 11, 9);
    add_district(&mut w, X::GREAT_WALL_DISTRICT_ID, 9, 11);
    add_district(&mut w, X::GREAT_WALL_DISTRICT_ID, 9, 9);
    // The wall at the NW is another owner's: no piece reaches it.
    tile_mut(&mut w, 9, 9).owner = 2;
    // 128×88 cells on the cell origin. Pieces: NE (1), the pillar (0), SW (5).
    let piece = |c: u32| sheet("GreatWall.pcx", (128 * c, 0), (128, 88), (0, 0));
    assert_eq!(draws(&w, 10, 10), vec![piece(1), piece(0), piece(5)]);

    // A resource on the tile: drawn before the wall, and not by the game's layer.
    w.resource_icons = vec![4];
    w.resource_visible = vec![true];
    tile_mut(&mut w, 10, 10).resource = 0;
    tile_mut(&mut w, 14, 10).resource = 0;
    let icon = |image: &str| DistrictDraw { image: image.to_owned(), origin: (201, 1), size: (49, 49), offset: (40, 8) };
    let out = draws(&w, 10, 10);
    assert_eq!(out[..2], [icon("Art/resources_shadows.pcx"), icon("Art/resources.pcx")]);
    assert_eq!(out[2..], [piece(1), piece(0), piece(5)]);
    assert_eq!(stages(&w, 10, 10, false).resources, vec![]);
    let tile = *w.tile(10, 10).unwrap();
    assert!(takes_over_resource(&w, &tile));
    assert!(rules::resource(&w, 10, 10).is_none());
    // Where there is no wall the game draws it.
    assert!(rules::resource(&w, 14, 10).is_some());
}

#[test]
fn resources_stay_the_games_unless_the_district_takes_them_over() {
    let mut w = world();
    w.resource_icons = vec![4, 6, 7];
    w.resource_visible = vec![true, true, true];
    add_district(&mut w, HOLY_SITE, 10, 10);
    tile_mut(&mut w, 10, 10).resource = 0;
    let tile = *w.tile(10, 10).unwrap();
    assert!(!takes_over_resource(&w, &tile));
    assert!(rules::resource(&w, 10, 10).is_some());
    assert_eq!(stages(&w, 10, 10, false).resources, vec![]);

    // Taken over once the district is finished (not before): not drawn.
    configs_mut(&mut w).districts[HOLY_SITE as usize].subsumes_tile_resource = true;
    let tile = *w.tile(10, 10).unwrap();
    assert!(takes_over_resource(&w, &tile) && rules::resource(&w, 10, 10).is_none());
    edit_district(&mut w, 10, 10, |d| d.completed = false);
    let tile = *w.tile(10, 10).unwrap();
    assert!(!takes_over_resource(&w, &tile) && rules::resource(&w, 10, 10).is_some());
}

#[test]
fn generated_resources_go_beside_the_tiles_own() {
    let mut w = world();
    w.resource_icons = vec![4, 6, 7];
    w.resource_visible = vec![true, true, true];
    add_district(&mut w, OIL_RIG, 10, 10);
    // Resource 2 has icon 7, cell (1, 1) of resources.pcx: centred in the
    // tile ((128 - 49) / 2, (64 - 49) / 2), the icon only.
    let generated = |offset: (i32, i32)| DistrictDraw { image: "Art/resources.pcx".to_owned(), origin: (51, 51), size: (49, 49), offset };
    // The tile's own resource, icon 4, cell (4, 0), after the game's usual offset.
    let own = |image: &str, dx: i32| DistrictDraw { image: image.to_owned(), origin: (201, 1), size: (49, 49), offset: (40 + dx, 8) };
    let rig = sheet("OilRig.pcx", (0, 0), (128, 64), (0, 0));
    assert_eq!(draws(&w, 10, 10), vec![rig.clone(), generated((39, 7))]);
    assert_eq!(stages(&w, 10, 10, false), Stages { buildings: vec![rig.clone()], resources: vec![generated((39, 7))] });

    // With the tile's own resource: that one 16 pixels left, this one 16 right of the centre.
    tile_mut(&mut w, 10, 10).resource = 0;
    assert_eq!(
        draws(&w, 10, 10),
        vec![rig.clone(), own("Art/resources_shadows.pcx", -16), own("Art/resources.pcx", -16), generated((16 + 39, 7))]
    );
    let tile = *w.tile(10, 10).unwrap();
    assert!(takes_over_resource(&w, &tile) && rules::resource(&w, 10, 10).is_none());

    // Zoomed out the cell is 64×32 but the sprite counts as 49 wide: it ends
    // up above and left of the centre, half size.
    let out = draws_at(&w, 10, 10, true);
    assert_eq!(out[1].offset, (-8 + 20, 4));
    assert_eq!(out[3].offset, (8 + (64 - 49) / 2, (32 - 49 >> 1)));

    // Only for the viewer who owns the tile; the district alone otherwise.
    w.player = 2;
    assert_eq!(draws(&w, 10, 10), vec![rig.clone()]);
    w.player = 1;
    // Only if the owner knows the resource, unless the district ignores that.
    w.resource_visible[2] = false;
    assert_eq!(draws(&w, 10, 10), vec![rig.clone()]);
    configs_mut(&mut w).districts[OIL_RIG as usize].generated_resource_flags = 2;
    assert_eq!(draws(&w, 10, 10).len(), 4);
    // Only if the tile has the resource the district wants on it.
    configs_mut(&mut w).districts[OIL_RIG as usize].resource_prereq_on_tile = 1;
    assert_eq!(draws(&w, 10, 10), vec![rig.clone()]);
    configs_mut(&mut w).districts[OIL_RIG as usize].resource_prereq_on_tile = 0;
    assert_eq!(draws(&w, 10, 10).len(), 4);
    // Not while it is being built: nothing, and the game's resource stays.
    edit_district(&mut w, 10, 10, |d| d.completed = false);
    assert!(rules::resource(&w, 10, 10).is_some());
    assert!(draws(&w, 10, 10).is_empty());
}

#[test]
fn ports_face_the_city_and_follow_the_land() {
    // The Port at (10, 10) is on water; everything else is the owner's land,
    // its image (sheet 0, image 0). Era 1, one harbour: column 1, row 1.
    let port = |city: (i32, i32)| {
        let mut w = world();
        add_district(&mut w, X::PORT_DISTRICT_ID, 10, 10);
        water(&mut w, 10, 10);
        add_city(&mut w, 1, city.0, city.1, 1, &[HARBOR]);
        draws(&w, 10, 10)
    };
    // A city to the east, land at the NE: the SW sheet, anchored at the NE
    // tile. Moved 6 down for that, 32 right for sheet 0 and 2 right, 8 up for
    // image 0.
    assert_eq!(port((12, 10)), vec![sheet("Port_SW.pcx", (128, 64), (128, 64), (34, -2))]);
    // A city straight above, land on both sides below: the SW sheet again
    // but moved 4 up and left first.
    assert_eq!(port((10, 8)), vec![sheet("Port_SW.pcx", (128, 64), (128, 64), (30, -6))]);
    // A city on the NW diagonal: the SE sheet, anchored at the NW tile, moved
    // 2 left and 6 down, and 8 left and up for being a diagonal.
    assert_eq!(port((9, 9)), vec![sheet("Port_SE.pcx", (128, 64), (128, 64), (-10, -2))]);
    // A city to the south-east: the NW sheet, 2 up, then 6 right and down.
    assert_eq!(port((11, 11)), vec![sheet("Port_NW.pcx", (128, 64), (128, 64), (6, 4))]);
    // And one on the SW diagonal: the NE sheet, 6 right and down.
    assert_eq!(port((9, 11)), vec![sheet("Port_NE.pcx", (128, 64), (128, 64), (6, 6))]);
}

#[test]
fn wonder_districts_show_their_wonder_or_its_building_site() {
    let mut w = world();
    add_district(&mut w, X::WONDER_DISTRICT_ID, 10, 10);
    // A bare wonder district: era 1.
    let plain = sheet("WonderDistrict.pcx", (0, 64), (128, 64), (0, 0));
    assert_eq!(draws(&w, 10, 10), vec![plain.clone()]);

    // Reserved by city 7, which builds the wonder and has the tile in reach.
    // A city to the east has the wonder face east: the alternative art, here
    // the building site at (4, 1).
    add_city(&mut w, 7, 12, 10, 1, &[]);
    w.cities[0].wonder_in_production = Some(ARTEMIS);
    edit_district(&mut w, 10, 10, |d| (d.wonder_state, d.wonder_city) = (1, 7));
    assert_eq!(draws(&w, 10, 10), vec![sheet("Wonders.pcx", (512, 64), (128, 64), (0, 0))]);
    // Without alternative art the plain site, at (2, 1).
    configs_mut(&mut w).wonders[0].alt = None;
    assert_eq!(draws(&w, 10, 10), vec![sheet("Wonders.pcx", (256, 64), (128, 64), (0, 0))]);
    configs_mut(&mut w).wonders[0].alt = Some(((5, 1), (4, 1)));
    // A city that builds something else, or out of reach: the district alone.
    w.cities[0].wonder_in_production = None;
    assert_eq!(draws(&w, 10, 10), vec![plain.clone()]);
    w.cities[0].wonder_in_production = Some(ARTEMIS);
    w.cities[0].x = 18;
    assert_eq!(draws(&w, 10, 10), vec![plain]);
    w.cities[0].x = 12;

    // Finished: (3, 1), the alternative (5, 1) when the nearest city is east.
    edit_district(&mut w, 10, 10, |d| (d.wonder_state, d.wonder) = (2, 0));
    assert_eq!(draws(&w, 10, 10), vec![sheet("Wonders.pcx", (640, 64), (128, 64), (0, 0))]);
    w.cities[0].x = 8;
    tile_mut(&mut w, 12, 10).city = -1;
    tile_mut(&mut w, 8, 10).city = 7;
    assert_eq!(draws(&w, 10, 10), vec![sheet("Wonders.pcx", (384, 64), (128, 64), (0, 0))]);

    // A wonder that may stand on rivers faces along the river and moves
    // towards it. A river to the east is not facing east (C3X's rule); moved
    // by the step of the direction, 2.
    configs_mut(&mut w).wonders[0].buildable_square_types = 1 << 14;
    tile_mut(&mut w, 10, 10).river = 1 << 2;
    assert_eq!(draws(&w, 10, 10), vec![sheet("Wonders.pcx", (384, 64), (128, 64), (2, 0))]);
    // To the south-east it is: step 1, and 18 down.
    tile_mut(&mut w, 10, 10).river = 1 << 3;
    assert_eq!(draws(&w, 10, 10), vec![sheet("Wonders.pcx", (640, 64), (128, 64), (1, 18))]);

    // Wonder districts switched off: nothing.
    configs_mut(&mut w).flags.wonder = false;
    assert!(draws(&w, 10, 10).is_empty());
}

#[test]
fn natural_wonders_need_no_finished_district() {
    let mut w = world();
    add_district(&mut w, X::NATURAL_WONDER_DISTRICT_ID, 10, 10);
    edit_district(&mut w, 10, 10, |d| (d.completed, d.natural_wonder) = (false, 0));
    // 128×88 cells, bottom-aligned to the tile.
    let falls = sheet("NaturalWonders.pcx", (128, 176), (128, 88), (0, -24));
    assert_eq!(draws(&w, 10, 10), vec![falls.clone()]);
    // With districts off but natural wonders on it still shows, and keeps the
    // tile's buildings away.
    configs_mut(&mut w).flags.districts = false;
    assert_eq!(draws(&w, 10, 10), vec![falls]);
    assert!(replaces_tile_buildings(&w, 10, 10));
    configs_mut(&mut w).flags.natural_wonders = false;
    assert!(draws(&w, 10, 10).is_empty());
    assert!(!replaces_tile_buildings(&w, 10, 10));
}

// ---------------------------------------------------------------------------
// The other layers.

#[test]
fn districts_replace_the_tiles_buildings() {
    let mut w = world();
    tile_mut(&mut w, 10, 10).ruins = true;
    tile_mut(&mut w, 10, 10).overlays |= overlay::MINE;
    tile_mut(&mut w, 12, 10).ruins = true;
    assert!(!rules::terrain_buildings(&w, 10, 10).is_empty());
    add_district(&mut w, HOLY_SITE, 10, 10);
    assert!(replaces_tile_buildings(&w, 10, 10) && !replaces_tile_buildings(&w, 12, 10));
    assert!(rules::terrain_buildings(&w, 10, 10).is_empty());
    assert!(!rules::terrain_buildings(&w, 12, 10).is_empty());
    // Even one under construction.
    edit_district(&mut w, 10, 10, |d| d.completed = false);
    assert!(rules::terrain_buildings(&w, 10, 10).is_empty());
    // With districts off the game draws them (only natural wonders are on).
    configs_mut(&mut w).flags.districts = false;
    assert!(!replaces_tile_buildings(&w, 10, 10));
    assert!(!rules::terrain_buildings(&w, 10, 10).is_empty());
}

#[test]
fn bridges_hide_roads_and_railroads() {
    let mut w = world();
    for (x, y) in [(10, 10), (12, 10)] {
        tile_mut(&mut w, x, y).overlays |= overlay::ROAD | overlay::RAILROAD;
    }
    assert!(rules::roads(&w, 10, 10).0.is_some() || rules::roads(&w, 10, 10).1.is_some());
    add_district(&mut w, X::BRIDGE_DISTRICT_ID, 10, 10);
    assert_eq!(rules::roads(&w, 10, 10), (None, None));
    assert!(rules::roads(&w, 12, 10) != (None, None));
    configs_mut(&mut w).flags.bridge = false;
    assert!(rules::roads(&w, 10, 10) != (None, None));
}

#[test]
fn irrigation_sources_hide_their_tiles_irrigation() {
    let mut w = world();
    tile_mut(&mut w, 10, 10).overlays |= overlay::IRRIGATION;
    add_district(&mut w, FARM, 10, 10);
    assert!(rules::irrigation(&w, 10, 10).is_none());
    // Under construction it still shows, until the worker's mine is there.
    edit_district(&mut w, 10, 10, |d| d.completed = false);
    assert!(rules::irrigation(&w, 10, 10).is_some());
    tile_mut(&mut w, 10, 10).overlays |= overlay::MINE;
    assert!(rules::irrigation(&w, 10, 10).is_none());
    // A district that is no irrigation source leaves it.
    let mut v = world();
    tile_mut(&mut v, 10, 10).overlays |= overlay::IRRIGATION;
    add_district(&mut v, HOLY_SITE, 10, 10);
    assert!(rules::irrigation(&v, 10, 10).is_some());
}

#[test]
fn districts_make_rivers_special_like_cities() {
    let mut w = world();
    // The vertex (11, 10) between (11, 9), (10, 10), (12, 10), (11, 11): a river
    // edge from the tile above, so it is drawn.
    tile_mut(&mut w, 11, 9).river = 0x08;
    let before = rules::vertex_river(&w, 11, 10);
    assert!(before.river.is_some() && !before.under_objects);
    add_district(&mut w, HOLY_SITE, 12, 10);
    assert!(rules::vertex_river(&w, 11, 10).under_objects);
    // Not one that is not there.
    let mut v = world();
    tile_mut(&mut v, 11, 9).river = 0x08;
    add_district(&mut v, HOLY_SITE, 14, 10);
    assert!(!rules::vertex_river(&v, 11, 10).under_objects);
}

// ---------------------------------------------------------------------------
// The dump.

#[test]
fn a_dump_gives_the_world_back() {
    let mut w = world();
    add_district(&mut w, X::WONDER_DISTRICT_ID, 10, 10);
    edit_district(&mut w, 10, 10, |d| (d.wonder_state, d.wonder, d.wonder_city) = (2, 0, 7));
    add_district(&mut w, X::NATURAL_WONDER_DISTRICT_ID, 12, 10);
    edit_district(&mut w, 12, 10, |d| (d.completed, d.natural_wonder) = (false, 0));
    add_district(&mut w, X::PORT_DISTRICT_ID, 14, 10);
    w.improvements[TEMPLE as usize] = Civ3Improvement { government: Some(2), obsolete_for: 0x6 };
    configs_mut(&mut w).districts[HOLY_SITE as usize].subsumes_tile_resource = true;
    configs_mut(&mut w).districts[OIL_RIG as usize].resource_prereqs = 2;
    configs_mut(&mut w).wonders[0].improvement = Some(ARTEMIS);
    w.free_improvements = vec![vec![], vec![TEMPLE, 325], vec![]];

    let mut text = String::from("map 40 40 wrap 0 0 seed 5 flags 0\nplayer 1\n");
    for line in crate::world::district_lines(&w.district_configs, &w.improvements, &w.free_improvements, &w.districts) {
        text += &line;
        text.push('\n');
    }
    for y in 0..HEIGHT {
        for x in (y & 1..WIDTH).step_by(2) {
            text += &format!("tile {x} {y} 0x1100 0 0 0 0 -1 1 -1 0 0 0 0 0 00 -1 -1 0 -1\n");
        }
    }
    let back = crate::world::Trace::parse(&text).unwrap().world;
    assert_eq!(back.district_configs, w.district_configs);
    assert_eq!(back.districts, w.districts);
    assert_eq!(back.free_improvements, vec![vec![], vec![TEMPLE, 325]]);
    assert_eq!(back.improvements[TEMPLE as usize], w.improvements[TEMPLE as usize]);
    assert_eq!(back.tile(14, 10).unwrap().district, Some(2));
    assert_eq!(draws(&back, 10, 10), draws(&w, 10, 10));
    // A dump of a game without C3X districts has none of it.
    assert!(crate::world::district_lines(&DistrictConfigs::default(), &[], &[], &[]).is_empty());
    assert!(!crate::world::Trace::parse("map 40 40 wrap 0 0\n").unwrap().world.district_configs.flags.any());
}

#[test]
fn wonders_give_free_buildings_to_all_cities_or_their_continent() {
    let mut w = world();
    add_district(&mut w, HOLY_SITE, 10, 10);
    add_city(&mut w, 1, 12, 10, 1, &[]);
    let column = |c: u32| vec![sheet("HolySite_EURO.pcx", (128 * c, 0), (128, 64), (0, 0))];
    assert_eq!(draws(&w, 10, 10), column(0));
    // A temple in all of civ 1's cities, a cathedral in those on continent 4
    // (the key is (continent + 1) · number of improvements + the improvement).
    w.free_improvements = vec![vec![], vec![TEMPLE, 5 * 64 + CATHEDRAL]];
    assert_eq!(draws(&w, 10, 10), column(1));
    tile_mut(&mut w, 12, 10).continent = 4;
    assert_eq!(draws(&w, 10, 10), column(2));
    // On another continent the cathedral is not theirs; another civ's wonder is not either.
    tile_mut(&mut w, 12, 10).continent = 3;
    assert_eq!(draws(&w, 10, 10), column(1));
    w.free_improvements = vec![vec![], vec![], vec![TEMPLE]];
    assert_eq!(draws(&w, 10, 10), column(0));
    // Free or built, the government and the tech still count.
    w.free_improvements = vec![vec![], vec![TEMPLE]];
    w.improvements[TEMPLE as usize].obsolete_for = 1 << 1;
    assert_eq!(draws(&w, 10, 10), column(0));
}
