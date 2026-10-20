//! The city-site preview (docs §10.9): while a unit that can found cities
//! is selected and standing, the game outlines the tiles the new city would
//! work, with 2-pixel lines at 50 % in the owner's colour (its colour
//! table's entry 66) along tile-diamond edges. Drawn into the map after the
//! fog of war and before the units.

use std::collections::BTreeSet;

use render_desc::Compiled;

use crate::rules::terrain::COAST;
use crate::world::{Civ3Unit, Civ3World};

/// `UnitType.Worker_Actions` bit of `UCV_Build_City`.
const FOUND_CITY: i32 = 0x2;
/// `AnimationType` DEFAULT (standing).
const ANIMATION_DEFAULT: i32 = 1;
/// Diagonal neighbours in the game's order (NE, SE, SW, NW).
const DIAGONALS: [(i32, i32); 4] = [(1, -1), (1, 1), (-1, 1), (-1, -1)];

/// The line from a tile's centre-relative vertex `(64·dx, 0)` to
/// `(0, 32·dy)`: its extent from the start.
fn edge(dx: i32, dy: i32) -> (i32, i32) {
    (-64 * dx, 32 * dy)
}

/// Sprite name of the edge line towards diagonal `d` in colour table `c`.
fn sprite(c: i32, d: usize) -> String {
    format!("CitySite@{c}/{d}")
}

/// Render description entries for the lines in the colour tables
/// `colors` (table → its entry 66 as RGB).
pub fn description(colors: &[(i32, [u8; 3])]) -> String {
    let mut out = String::from("\n# City-site preview lines (generated, docs §10.9).\n");
    for &(c, [r, g, b]) in colors {
        for (d, &(dx, dy)) in DIAGONALS.iter().enumerate() {
            let (ex, ey) = edge(dx, dy);
            let (ax, ay) = render_desc::line_origin(ex, ey, 2);
            out.push_str(&format!(
                "[images.city_site_{c}_{d}]\nsource = \"line:{r},{g},{b},128:{ex},{ey}:2\"\n\n\
                 [sheets.city_site_{c}_{d}]\nimage = \"city_site_{c}_{d}\"\ncell = [{}, {}]\nanchor = [{ax}, {ay}]\ngroup = \"CitySite@{c}\"\nfirst_index = {d}\n\n",
                ex.abs(),
                ey.abs() + 1,
            ));
        }
    }
    out
}

/// The unit whose preview is shown, if any.
fn previewing(world: &Civ3World) -> Option<&Civ3Unit> {
    // (Traces from before `current_unit` was recorded: the cursor's unit.)
    let id = world.current_unit.or(world.selection.map(|s| s.0))?;
    let u = world.units.iter().find(|u| u.id == id && u.visible)?;
    let actions = world.unit_type_stats.get(u.unit_type.max(0) as usize)?[5];
    (actions & FOUND_CITY != 0 && u.animation == ANIMATION_DEFAULT).then_some(u)
}

/// The colour tables the preview needs now.
pub fn needed(world: &Civ3World) -> BTreeSet<i32> {
    previewing(world).map(|u| world.color_tables.get(u.civ.max(0) as usize).copied().unwrap_or(0)).into_iter().collect()
}

/// The 21 tiles of a city's radius around (0, 0): the 5×5 block of tile
/// diamonds without its corners.
fn in_radius(dx: i32, dy: i32) -> bool {
    crate::rules::in_city_radius(dx, dy)
}

/// Whether a city could be founded at (x, y) [I: the game asks
/// `Map_check_city_location`; modelled as land without a city on or next
/// to it].
fn city_site(world: &Civ3World, x: i32, y: i32) -> bool {
    let Some(t) = world.tile(x, y) else { return false };
    if t.terrain() >= COAST || t.city >= 0 {
        return false;
    }
    crate::rules::NEIGHBOURS.iter().all(|(dx, dy)| world.tile(x + dx, y + dy).is_none_or(|n| n.city < 0))
}

/// The lines: (sprite, start point in world pixels).
fn lines(world: &Civ3World) -> Vec<(String, (i32, i32))> {
    let Some(u) = previewing(world) else { return Vec::new() };
    let bit = 1u32 << (world.player & 31);
    // On the explored tile under the mouse, else (mouse off the map) on
    // the unit.
    let (x, y) = match world.mouse_tile {
        Some((mx, my)) if world.tile(mx, my).is_some() => {
            if world.tile(mx, my).is_some_and(|t| t.explored & bit == 0) {
                return Vec::new();
            }
            (mx, my)
        }
        _ => (u.x, u.y),
    };
    if !city_site(world, x, y) {
        return Vec::new();
    }
    // A radius tile is in the area if nobody else owns it (inner ring) or
    // the unit's civ owns it (outer ring) [I: the game's ownership tests
    // are approximated].
    let inside = |dx: i32, dy: i32| {
        in_radius(dx, dy)
            && world.tile(x + dx, y + dy).is_some_and(|t| {
                let owner = i32::from(t.owner);
                if dx.abs() + dy.abs() <= 2 { owner <= 0 || owner == u.civ } else { owner == u.civ }
            })
    };
    let colors = world.color_tables.get(u.civ.max(0) as usize).copied().unwrap_or(0);
    let mut out = Vec::new();
    for ty in -4..=4 {
        for tx in -4..=4 {
            if !inside(tx, ty) {
                continue;
            }
            // The tile's centre in world pixels.
            let (cx, cy) = (64 * (x + tx) + 64, 32 * (y + ty) + 32);
            for (d, &(dx, dy)) in DIAGONALS.iter().enumerate() {
                if !inside(tx + dx, ty + dy) {
                    out.push((sprite(colors, d), (cx + 64 * dx, cy)));
                }
            }
        }
    }
    out
}

/// The free sprites of the preview: over the fog of war, under the units
/// (after the pass before theirs).
pub fn sprites(world: &Civ3World, compiled: &Compiled) -> Vec<scene::FreeSprite> {
    let after_pass = crate::units::unit_passes(compiled).1;
    lines(world)
        .into_iter()
        .filter_map(|(name, (x, y))| {
            let (set, _) = compiled.set(&name)?;
            Some(scene::FreeSprite {
                set: set.raw(),
                x,
                y,
                screen: false,
                after_pass,
                phase: 0,
                ticks_per_frame: 1,
                scale_den: 0,
                blend: scene::SpriteBlend::Alpha,
                tint: 0,
                zoom_art: 0,
                zoom: (0, 0),
                zoom_offset: (0, 0),
                clip: None,
                not_in_view: None,
                glyph: None, solid: None,
                palette: 0,
                category: viewer_settings::clean_map::CITIES,
            })
        })
        .collect()
}
