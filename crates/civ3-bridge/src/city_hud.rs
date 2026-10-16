//! What the game draws over cities after the units (docs §10.8): the
//! harbour, airport and barracks icons beside a city. (The name and
//! production labels are text and boxes, not yet drawn here.)

use render_desc::Compiled;

use crate::world::{Civ3City, Civ3World};

/// `ITF_Veteran_*` flags of the icon buildings.
const GROUND: u32 = 0x2;
const SEA: u32 = 0x2_0000;
const AIR: u32 = 0x4_0000;

/// Whether `city` has a working, not obsolete improvement with `flag`
/// (`City_count_improvements_with_flag`).
fn has_building_with(world: &Civ3World, city: &Civ3City, flag: u32) -> bool {
    world
        .icon_buildings
        .iter()
        .any(|b| b.flags & flag != 0 && city.has_improvement(b.id) && b.obsolete_for & (1 << (city.civ & 31)) == 0)
}

/// The icons, as world-space sprites (name, top-left).
fn icons(world: &Civ3World) -> Vec<(&'static str, (i32, i32))> {
    let mut out = Vec::new();
    let bit = 1u32 << (world.player & 31);
    for city in &world.cities {
        // Only cities on tiles the viewer has explored.
        if !world.tile(city.x, city.y).is_some_and(|t| t.explored & bit != 0) {
            continue;
        }
        // The city tile's box.
        let (sx, sy) = (64 * city.x, 32 * city.y);
        // C3X (city_icons_show_unit_effects_not_trade, its default): the
        // harbour and airport icons mark veteran sea/air unit buildings.
        if has_building_with(world, city, SEA) {
            out.push(("CityIcon_Harbor/0", (sx, sy + 47)));
        }
        if has_building_with(world, city, AIR) {
            out.push(("CityIcon_Airport/0", (sx + 103, sy + 47)));
        }
        if city.civ == world.player && has_building_with(world, city, GROUND) {
            out.push(("CityIcon_Barracks/0", (sx + 103, sy + 29)));
        }
    }
    out
}

/// The free sprites of the city icons, drawn after the units.
pub fn sprites(world: &Civ3World, compiled: &Compiled) -> Vec<scene::FreeSprite> {
    icons(world)
        .into_iter()
        .filter_map(|(name, (x, y))| {
            let (set, _) = compiled.set(name)?;
            Some(scene::FreeSprite {
                set: set.raw(),
                x,
                y,
                screen: false,
                after_pass: u32::MAX,
                phase: 0,
                ticks_per_frame: 1,
                scale_den: 0,
                tint: 0,
                zoom_art: 0,
                clip: None,
            })
        })
        .collect()
}
