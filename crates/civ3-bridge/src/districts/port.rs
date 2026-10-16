//! Districts that align to the coast (`align_to_coast`, the Port):
//! `align_variant_and_pixel_offsets_with_coastline` (39423).
//!
//! The port is drawn from one of four sheets, named for the corner of the
//! tile the quay reaches from (NW, NE, SE, SW), a little moved. Which, comes
//! from where the first city of its owner around it lies and from where the
//! owner's land is, and the move from the look of the land tile it is
//! anchored to (hand-tuned for the art; every number below is C3X's).

use super::access::*;
use crate::rules::terrain;
use crate::world::{Civ3Tile, Civ3World};

/// The port sheets, by index in the district's image paths.
pub const PORT_NW: i32 = 0;
pub const PORT_NE: i32 = 1;
pub const PORT_SE: i32 = 2;
pub const PORT_SW: i32 = 3;

/// What the choice needs to know of the tile the port is anchored to: its
/// base terrain sheet and image (`SquareParts`), -1 if there is no such
/// tile, and whether it is hills or mountains.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub sheet: i32,
    pub sprite: i32,
    pub rough: bool,
}

impl Anchor {
    pub const NONE: Anchor = Anchor { sheet: -1, sprite: -1, rough: false };
}

/// The sheet and the move in pixels.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Coast {
    pub variant: i32,
    pub shift: (i32, i32),
}

/// The choice, from the position of the first city (`city`, relative to the
/// port), which of the eight neighbours (by direction, index 0 unused) are
/// land of the port's owner (`same`) or any land (`any`), and what the
/// neighbours look like (`anchors`).
pub fn coast_choice(city: (i32, i32), same: &[bool; 9], any: &[bool; 9], anchors: &[Anchor; 9]) -> Coast {
    let (mut px, mut py) = (0, 0);
    let mut direct_diagonal = false;
    // Sheet and anchor direction.
    let mut choice: Option<(i32, usize)> = None;
    match city {
        // A city on a diagonal next to the port: the quay faces it.
        (1, -1) => (choice, direct_diagonal) = (Some((PORT_SW, NE)), true),
        (1, 1) => (choice, direct_diagonal) = (Some((PORT_NW, SE)), true),
        (-1, 1) => (choice, direct_diagonal) = (Some((PORT_NE, SW)), true),
        (-1, -1) => (choice, direct_diagonal) = (Some((PORT_SE, NW)), true),
        (cdx, cdy) => {
            let (west, east, north, south) = (cdx < 0, cdx > 0, cdy < 0, cdy > 0);
            if (cdx, cdy) == (0, -2) {
                choice = Some(if same[NW] && !same[SE] {
                    (PORT_SE, NW)
                } else if same[NE] && !same[SW] {
                    (PORT_SW, NE)
                } else {
                    (px, py) = (px - 4, py - 4);
                    (PORT_SW, NE)
                });
            } else if (cdx, cdy) == (0, 2) {
                choice = Some(if same[SE] {
                    (PORT_NW, SE)
                } else if same[SW] {
                    (PORT_NE, SW)
                } else {
                    (px, py) = (px + 4, py + 4);
                    (PORT_NE, SW)
                });
            } else if west && cdy == 0 {
                choice = Some(if same[NW] {
                    (PORT_SE, NW)
                } else if same[SW] {
                    (PORT_NE, SW)
                } else {
                    (px, py) = (px - 30, py + 24);
                    (PORT_SE, NW)
                });
            } else if east && cdy == 0 {
                choice = Some(if same[NE] {
                    (PORT_SW, NE)
                } else if same[SE] {
                    (PORT_NW, SE)
                } else {
                    (px, py) = (px + 30, py - 24);
                    (PORT_SW, NE)
                });
            } else if north && west {
                choice = if same[NW] { Some((PORT_SE, NW)) } else if same[SW] { Some((PORT_NE, SW)) } else { None };
            } else if north && east {
                choice = if same[NE] { Some((PORT_SW, NE)) } else if same[SE] { Some((PORT_NW, SE)) } else { None };
            } else if south && east {
                choice = if same[SE] { Some((PORT_NW, SE)) } else if same[NE] { Some((PORT_SW, NE)) } else { None };
            } else if south && west {
                choice = if same[SW] { Some((PORT_NE, SW)) } else if same[NW] { Some((PORT_SE, NW)) } else { None };
            }
        }
    }

    // No ideal direction: any land of the owner next to the port, else any land.
    let any_land = |land: &[bool; 9]| {
        if land[NE] {
            Some((PORT_SW, NE))
        } else if land[SE] {
            Some((PORT_NW, SE))
        } else if land[SW] {
            Some((PORT_NE, SW))
        } else if land[NW] {
            Some((PORT_SE, NW))
        } else if land[N] {
            Some((PORT_SW, N))
        } else if land[E] {
            Some((PORT_SW, E))
        } else if land[S] {
            Some((PORT_NE, S))
        } else if land[W] {
            Some((PORT_SE, W))
        } else {
            None
        }
    };
    let choice = choice.or_else(|| any_land(same)).or_else(|| any_land(any)).unwrap_or((PORT_SW, NE));
    let (variant, anchor) = choice;

    // The general move, by sheet and anchor.
    match choice {
        (PORT_SW, NE) => py += 6,
        (PORT_SE, NW) => (px, py) = (px - 2, py + 6),
        (PORT_NW, SE) => py -= 2,
        (PORT_SW, N) => (px, py) = (px - 56, py - 26),
        (PORT_SW, E) => (px, py) = (px + 36, py + 18),
        (PORT_NE, S) => (px, py) = (px + 70, py + 30),
        (PORT_SE, W) => (px, py) = (px - 20, py + 14),
        _ => {}
    }

    // Edge cases, by the base terrain image of the anchor tile.
    let a = anchors[anchor];
    let (sw, se, nw, ne) = (variant == PORT_SW, variant == PORT_SE, variant == PORT_NW, variant == PORT_NE);
    let sp = a.sprite;
    if !a.rough && !direct_diagonal {
        if se && anchor == W {
            (px, py) = (px - 20, py + 20);
        }
        // The sideways move of each sheet: towards the quay's side.
        let sideways = |left: i32| if sw || nw { left } else if se || ne { -left } else { 0 };
        match a.sheet {
            0 => {
                px += sideways(32);
                if (ne || nw) && anchor == S && sp == 26 {
                    (px, py) = (px - 4, py + 30);
                } else if ne && anchor == SW && sp == 20 {
                    (px, py) = (px - 2, py + 4);
                }
                if sw && matches!(sp, 0 | 4 | 10 | 1) {
                    (px, py) = (px + 2, py - 8);
                } else if sw && anchor == N && sp == 6 {
                    (px, py) = (px - 6, py - 8);
                } else if se && anchor == W && sp == 18 {
                    (px, py) = (px + 6, py + 4);
                } else if ne && anchor == SW && sp == 51 {
                    (px, py) = (px + 20, py - 20);
                } else if nw && anchor == SE && sp == 44 {
                    (px, py) = (px - 8, py - 12);
                }
            }
            1 => {
                px += sideways(10);
                if sw && matches!(sp, 7 | 17 | 16) {
                    (px, py) = (px + 10, py - 8);
                }
            }
            2 => {
                if sw && sp == 6 {
                    (px, py) = (px + 2, py - 8);
                }
            }
            3 => {
                px += sideways(16);
                if sw && matches!(sp, 43 | 40 | 35 | 39) {
                    (px, py) = (px + 6, py - 12);
                } else if se && sp == 50 {
                    (px, py) = (px - 6, py - 12);
                } else if se && sp == 26 {
                    (px, py) = (px + 50, py - 2);
                }
            }
            4 => {
                if sw && sp == 71 {
                    (px, py) = (px + 2, py - 8);
                }
            }
            5 => {
                px += sideways(18);
                if (sw || se) && sp == 18 {
                    py -= 10;
                } else if sw && sp == 73 {
                    (px, py) = (px - 4, py - 10);
                } else if nw && anchor == SE && sp == 42 {
                    py -= 8;
                } else if (nw || sw) && sp == 6 {
                    (px, py) = (px + 12, py - 6);
                } else if sw && sp == 16 {
                    (px, py) = (px - 8, py - 10);
                }
            }
            _ => {}
        }
    } else if direct_diagonal && (choice == (PORT_SW, NE) || choice == (PORT_SE, NW)) {
        (px, py) = (px - 8, py - 8);
    } else if direct_diagonal && (choice == (PORT_NW, SE) || choice == (PORT_NE, SW)) {
        (px, py) = (px + 6, py + 6);
    }
    Coast { variant, shift: (px, py) }
}

/// The sheet the port at (x, y) uses and how far it moves; `None` for the
/// sheet when C3X leaves the culture's (no owner, or no city of the owner's
/// around).
pub fn align(world: &Civ3World, tile: &Civ3Tile, x: i32, y: i32) -> (Option<i32>, (i32, i32)) {
    let owner = i32::from(tile.owner);
    if owner <= 0 {
        return (None, (0, 0));
    }
    let Some(city) = cities_around(world, x, y).into_iter().next() else { return (None, (0, 0)) };
    let (mut cdx, mut cdy) = (city.x - x, city.y - y);
    if world.wrap_x {
        let half = world.width / 2;
        if cdx > half {
            cdx -= world.width;
        } else if cdx < -half {
            cdx += world.width;
        }
    }
    if world.wrap_y {
        let half = world.height / 2;
        if cdy > half {
            cdy -= world.height;
        } else if cdy < -half {
            cdy += world.height;
        }
    }

    let (mut same, mut any, mut anchors) = ([false; 9], [false; 9], [Anchor::NONE; 9]);
    for dir in DIRECTIONS {
        let (dx, dy) = step(dir);
        same[dir] = tile_is_land(world, owner, x + dx, y + dy, true);
        any[dir] = tile_is_land(world, owner, x + dx, y + dy, false);
        if let Some(t) = tile_near(world, x + dx, y + dy) {
            anchors[dir] = Anchor {
                sheet: i32::from(t.square_parts >> 8 & 0xFF),
                sprite: i32::from(t.square_parts & 0xFF),
                rough: matches!(t.terrain(), terrain::HILLS | terrain::MOUNTAINS),
            };
        }
    }
    let coast = coast_choice((cdx, cdy), &same, &any, &anchors);
    (Some(coast.variant), coast.shift)
}
