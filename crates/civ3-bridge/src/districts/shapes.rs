//! Districts whose art follows their surroundings: bridges, canals and the
//! Great Wall. The choices are functions of which neighbours have what (by
//! direction, 1 NE .. 8 N, index 0 unused), so they can be checked against
//! C3X's own code for every combination (`tools/district-oracle`).

use civ3_sys::c3x::DistrictKind;
use civ3_sys::c3x_layout as X;

use super::access::*;
use super::{DistrictDraw, sprite};
use crate::rules::overlay;
use crate::world::{Civ3Tile, Civ3World};

/// A bridge's columns (`get_bridge_image_index`, 39797): 0 SW-NE, 1 NW-SE,
/// 2 N-S, 3 W-E; 4..7 the same with a railroad on the bridge.
///
/// It runs along the neighbouring bridges (two opposite ones first, then
/// one), else towards land the same way.
pub fn bridge_choice(bridge: &[bool; 9], land: &[bool; 9], railroad: bool) -> i32 {
    let (sw_ne, nw_se, n_s, w_e) = if railroad { (4, 5, 6, 7) } else { (0, 1, 2, 3) };
    let toward = |a: [bool; 9]| {
        let both = |p: usize, q: usize| a[p] && a[q];
        let either = |p: usize, q: usize| a[p] || a[q];
        if both(SW, NE) {
            sw_ne
        } else if both(NW, SE) {
            nw_se
        } else if both(N, S) {
            n_s
        } else if both(W, E) {
            w_e
        } else if either(NE, SW) {
            sw_ne
        } else if either(NW, SE) {
            nw_se
        } else if either(N, S) {
            n_s
        } else if either(W, E) {
            w_e
        } else {
            sw_ne
        }
    };
    if bridge[NE..=N].iter().any(|&b| b) {
        return toward(*bridge);
    }
    toward(std::array::from_fn(|d| land[d] || bridge[d]))
}

/// The bridge's column on the tile at (x, y).
pub fn bridge_column(world: &Civ3World, tile: &Civ3Tile, x: i32, y: i32) -> i32 {
    let (mut bridge, mut land) = ([false; 9], [false; 9]);
    for dir in DIRECTIONS {
        let (dx, dy) = step(dir);
        bridge[dir] = has_district_at(world, x + dx, y + dy, X::BRIDGE_DISTRICT_ID);
        land[dir] = tile_is_land(world, i32::from(tile.owner), x + dx, y + dy, false);
    }
    bridge_choice(&bridge, &land, tile.overlays & overlay::RAILROAD != 0)
}

/// The directions a canal runs in, -1 for none (`get_canal_directions`,
/// 39889): along the neighbouring canals, else towards water (never two
/// directions side by side), the one higher on the screen first (so that
/// the lower one is drawn in front), then fixes for corners.
pub fn canal_choice(canal: &[bool; 9], water: &[bool; 9]) -> (i32, i32) {
    let available: [bool; 9] = std::array::from_fn(|d| canal[d] || water[d]);
    // Side by side looks cramped.
    let too_close = [(N, NE), (NE, E), (E, SE), (SE, S), (S, SW), (SW, W), (W, NW), (NW, N)];
    let preferred = [NE, NW, SE, SW, N, E, S, W];
    let has_canal = canal[NE..=N].iter().any(|&c| c);

    let dir1 = preferred.into_iter().find(|&d| if has_canal { canal[d] } else { available[d] });
    let dir2 = dir1.and_then(|d1| {
        [canal, &available].into_iter().find_map(|dirs| {
            preferred.into_iter().find(|&d| d != d1 && dirs[d] && !too_close.iter().any(|&p| p == (d1, d) || p == (d, d1)))
        })
    });

    let (mut one, mut two) = (dir1.map_or(-1, |d| d as i32), dir2.map_or(-1, |d| d as i32));
    if one >= 0 && two >= 0 {
        // The one higher on the screen goes first (the smaller weight).
        let weight = |d: i32| match d as usize {
            S => 2,
            SE | SW => 1,
            NE | NW | N => -1,
            _ => 0,
        };
        if weight(one) > weight(two) {
            std::mem::swap(&mut one, &mut two);
        }
    }

    // Fixes for corners, one after the other (40002).
    let (ne, nw, n, s, sw, se) = (NE as i32, NW as i32, N as i32, S as i32, SW as i32, SE as i32);
    if !has_canal && water[NE] && water[SW] {
        (one, two) = (ne, sw);
    }
    if one == ne && two == s && water[N] && water[NE] {
        (one, two) = (n, s);
    }
    if one == ne && two == se && water[SE] && water[SW] {
        two = sw;
    }
    if one == ne && two == se && water[NE] && water[N] {
        one = n;
    }
    if one == ne && two == nw && water[S] {
        two = s;
    }
    if one == ne && two == nw && canal[NE] && water[SW] {
        two = sw;
    }
    if one == ne && two == s && canal[NE] && water[S] {
        two = sw;
    }
    if one == nw && two == ne && canal[NW] && water[SE] {
        two = se;
    }
    if one == nw && two == s && canal[NW] && water[S] {
        two = se;
    }
    (one, two)
}

/// The canal's sprites (`draw_canal_district`, 40046): one per direction,
/// column = direction. Each is drawn a second time a little further towards
/// water that lies that way (`draw_canal_on_map_or_canvas`, 40019), so the
/// canal reaches it. `era` picks the row.
pub fn canal_draws(world: &Civ3World, kind: &DistrictKind, era: i32, x: i32, y: i32) -> Vec<DistrictDraw> {
    let (mut canal, mut water) = ([false; 9], [false; 9]);
    for dir in DIRECTIONS {
        let (dx, dy) = step(dir);
        canal[dir] = has_district_at(world, x + dx, y + dy, X::CANAL_DISTRICT_ID);
        water[dir] = tile_is_water(world, x + dx, y + dy);
    }
    let (one, two) = canal_choice(&canal, &water);

    // Unlike the other districts the canal ignores where a coast would move
    // it. Its size and offset are those of the district.
    let (w, h) = kind.cell_size();
    let base = (kind.offset.0 - (w - 128) / 2, kind.offset.1 - (h - 64));
    let (mut at_one, mut at_two) = (base, base);
    // Offsets for (literal) edge cases.
    let (ne, n, s, w_, se) = (NE as i32, N as i32, S as i32, W as i32, SE as i32);
    if one == ne && two == s {
        at_one = (base.0 + 7, base.1 - 2);
    } else if one == n && two == w_ {
        at_two.0 -= 12;
    } else if one == n && two == se {
        at_two.0 += 4;
    }

    let mut out = Vec::new();
    for (dir, at) in [(one, at_one), (two, at_two)] {
        if dir < 0 {
            continue;
        }
        let Some(first) = sprite(kind, 0, era, dir, at) else { continue };
        out.push(first.clone());
        let dir = dir as usize;
        if water[dir] {
            // Another sprite 18 pixels sideways and 9 down or up.
            let (sx, sy) = step(dir);
            out.push(DistrictDraw { offset: (at.0 + sx.signum() * 18, at.1 + sy.signum() * 9), ..first });
        }
    }
    out
}

/// The Great Wall's sprites (`draw_great_wall_district`, 40080): the pieces
/// towards the walls of the same owner on the four diagonals, the two at the
/// back (NW, NE) before the pillar (column 0) and the two at the front (SW,
/// SE) after it. They sit on the cell origin, whatever the sprite's height or
/// the district's offsets.
pub fn great_wall_draws(world: &Civ3World, kind: &DistrictKind, tile: &Civ3Tile, x: i32, y: i32) -> Vec<DistrictDraw> {
    let owner = i32::from(tile.owner);
    let wall = |dir: usize| {
        let (dx, dy) = step(dir);
        has_district_at_owned(world, x + dx, y + dy, X::GREAT_WALL_DISTRICT_ID, owner)
    };
    [(NW, wall(NW)), (NE, wall(NE)), (0, true), (SW, wall(SW)), (SE, wall(SE))]
        .into_iter()
        .filter(|&(_, shown)| shown)
        .filter_map(|(column, _)| sprite(kind, 0, 0, column as i32, (0, 0)))
        .collect()
}
