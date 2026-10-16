//! The resources of a district's tile (`draw_district_generated_resource_on_tile`,
//! C3X 40112). While districts are on, C3X draws the tile's resource itself:
//! after the district's tile buildings' place (the game's resource layer) for
//! most districts, before the district for those drawn over resources. A
//! district can take the tile's resource over (it is then not drawn) or
//! generate one of its own (drawn beside it).

use civ3_sys::c3x::{District, DistrictKind};
use civ3_sys::c3x_layout as X;

use super::DistrictDraw;
use crate::rules;
use crate::world::{Civ3Tile, Civ3World};

/// Where the game keeps its resource art (docs §6.7): 49×49 cells at
/// (1 + 50·column, 1 + 50·row), the shadows in a sheet of their own.
pub const RESOURCES: &str = "Art/resources.pcx";
pub const RESOURCE_SHADOWS: &str = "Art/resources_shadows.pcx";
const CELL: u32 = 49;
/// The icons' `Sprite.Width` C3X centres them by: one less than the cell
/// (measured: a lone generated icon sits at x 40 in the 128 px tile).
const ICON_WIDTH: i32 = 48;

/// The game's resource sprites for an icon (§6.7): its shadow, then the icon,
/// `dx` pixels to the right of where they go.
pub fn plain(icon: i32, dx: i32, zoomed_out: bool) -> [DistrictDraw; 2] {
    let at = if zoomed_out { (dx + 20, 4) } else { (dx + 40, 8) };
    let cell = |image: &str| DistrictDraw {
        image: image.to_owned(),
        origin: (1 + 50 * (icon % 6) as u32, 1 + 50 * (icon / 6) as u32),
        size: (CELL, CELL),
        offset: at,
    };
    [cell(RESOURCE_SHADOWS), cell(RESOURCES)]
}

/// `get_visible_non_subsumed_tile_resource` (17798): the resource the viewer
/// sees on the tile, unless a finished district that subsumes it is there.
pub fn base(world: &Civ3World, tile: &Civ3Tile, d: &District, kind: &DistrictKind) -> Option<(usize, i32)> {
    let resource = rules::visible_resource(world, tile)?;
    let subsumed = world.district_configs.flags.districts && d.completed && kind.subsumes_tile_resource;
    (!subsumed).then_some(resource)
}

/// The resource the district generates on its tile (the checks of 40136 and
/// `district_generates_resource_for_civ`, 15091): it is finished, its tile's
/// owner is the viewer (or there is no viewer), the owner knows the resource
/// (unless the district ignores that) and the tile holds the resource the
/// district wants on it. The connection the resources it needs (`resource_prereqs`)
/// ask of the Trade Net is taken as there.
pub fn generated(world: &Civ3World, tile: &Civ3Tile, d: &District, kind: &DistrictKind) -> Option<usize> {
    let resource = usize::try_from(kind.generated_resource).ok()?;
    let owner = i32::from(tile.owner);
    if !d.completed || owner < 0 || (world.player >= 0 && owner != world.player) {
        return None;
    }
    // The owner is the viewer: what the viewer knows, the owner knows.
    let known = |r: usize| world.resource_visible.get(r).copied().unwrap_or(true);
    if i64::from(kind.generated_resource_flags) & X::MF_NO_TECH_REQ == 0 && !known(resource) {
        return None;
    }
    if let Ok(wanted) = usize::try_from(kind.resource_prereq_on_tile) {
        let here = usize::try_from(tile.resource).ok().filter(|&r| known(r));
        if here != Some(wanted) {
            return None;
        }
    }
    Some(resource)
}

/// Whether C3X draws the tile's resource instead of the game's resource
/// layer: the district is drawn over resources, subsumes the tile's or
/// generates one.
pub fn takes_over(world: &Civ3World, tile: &Civ3Tile, d: &District, kind: &DistrictKind) -> bool {
    kind.draw_over_resources || generated(world, tile, d, kind).is_some() || base(world, tile, d, kind).is_none()
}

/// What C3X draws for the tile's resources where that differs from the game's
/// resource layer (see [`takes_over`]), in drawing order.
///
/// With a generated resource the tile's own moves an eighth of the tile's
/// width to the left, and the generated one (the icon only, no shadow) is
/// centred in the tile, an eighth to the right of the centre if the tile has
/// a resource of its own. Zoomed out C3X centres with the sprite's full width
/// in a half-width tile but draws it at half size, which leaves it up and
/// left of the centre.
pub fn draws(world: &Civ3World, tile: &Civ3Tile, d: &District, kind: &DistrictKind, zoomed_out: bool) -> Vec<DistrictDraw> {
    let base = base(world, tile, d, kind);
    let Some(generated) = generated(world, tile, d, kind) else {
        // The game's own drawing, moved before the district for those drawn over resources.
        return match base {
            Some((_, icon)) if kind.draw_over_resources => plain(icon, 0, zoomed_out).to_vec(),
            _ => Vec::new(),
        };
    };
    let tile_width = if zoomed_out { 64 } else { 128 };
    let shift = (tile_width >> 2) >> 1;
    let (left, right) = (-shift, shift);
    let icon = world.resource_icons.get(generated).copied().filter(|i| (0..36).contains(i));
    let Some(icon) = icon else {
        // No icon for it: C3X falls back to the game's drawing of the tile's own.
        return rules::visible_resource(world, tile).map(|(_, icon)| plain(icon, 0, zoomed_out).to_vec()).unwrap_or_default();
    };
    let mut out = base.map(|(_, icon)| plain(icon, left, zoomed_out).to_vec()).unwrap_or_default();
    let at = ((tile_width - ICON_WIDTH >> 1) + if base.is_some() { right } else { 0 }, (tile_width / 2 - CELL as i32) >> 1);
    out.push(DistrictDraw { offset: at, ..plain(icon, 0, zoomed_out)[1].clone() });
    out
}
