//! What a scenario has revealed when it starts (docs §19.4): the tiles its
//! players have explored at turn 1 and those they see then, and the part of
//! the map they make up.
//!
//! Measured in the game after starting scenarios (every civ's
//! `Tile.Explored` at turn 1): a player has explored the tiles within a
//! circle around each of its cities, one tile wider than the city's borders
//! (`CITY` border level), and around each of its units the eight tiles next
//! to it; everything if the scenario reveals the entire map (`GAME`). The
//! tiles the editor's fog-of-war brush marks (`TILE` fog of war) are not
//! revealed by that: in *Rise of Rome* 3679 tiles are marked, Rome has
//! explored 327 at turn 1, all of them its own cities' and units' (Egypt
//! 204, none of them marked).
//!
//! The preview shows what a nation that does not exist would know if it
//! had the start of every civ a human may play: the tiles any of them has
//! explored, those their cities or units see now not fogged. The others'
//! (the computer's only, the barbarians') stay unknown. A scenario whose
//! players have nothing yet (their places are the game's choice when it
//! starts) reveals nothing that way. Its unexplored tiles stay black until
//! the game generates the starting positions (and the preview frames the
//! whole map).
//!
//! Which civs a human may play, as the game's civ selection decides
//! (0x585480..0x585527; `LEAD` in memory: human flag +8, race +0x58): one
//! the scenario's playable list (`GAME`) holds, or any if the list is
//! empty (0x5E22B0), and that no `LEAD` player of it is the computer's
//! (*Human player* unset). `TILE` holds no reveal bits per civ or player:
//! the start is the cities' and units' (above).

use civ3_assets::biq::{Map, Owner, Player};

/// The tiles revealed, indexed as `Map::tiles`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revealed {
    /// Explored by some player at the start.
    pub explored: Vec<bool>,
    /// Seen by some player then (not under the fog of war).
    pub in_sight: Vec<bool>,
}

/// How far a city sees beyond its borders, in tiles.
const CITY_BEYOND_BORDERS: i32 = 1;
/// How far a unit sees, in tiles.
const UNIT_SIGHT: i32 = 1;

/// Whether the tile `(du, dv)` tiles away along the map's two diagonals
/// lies in the circle of radius `r` the game reveals around a city or unit
/// (measured: radius 1 the eight tiles around, 2 the 21 of a city's work
/// area, 3 and more the rounder circles of culture borders).
fn within(du: i32, dv: i32, r: i32) -> bool {
    du * du + dv * dv <= r * r + r + 1
}

/// The tiles of `map` within radius `r` of tile `(x, y)` (wrapping where the
/// map wraps), as `Map::tiles` indices.
fn around(map: &Map, (x, y): (i32, i32), r: i32, mut f: impl FnMut(usize)) {
    let wrap_x = map.flags & 1 != 0;
    let wrap_y = map.flags & 2 != 0;
    for du in -r..=r {
        for dv in -r..=r {
            if !within(du, dv, r) {
                continue;
            }
            // (Along the diagonals: one step right and one down is two
            // columns of the map's x.)
            let (mut tx, mut ty) = (x + du + dv, y + du - dv);
            if wrap_x {
                tx = tx.rem_euclid(map.width.max(1));
            }
            if wrap_y {
                ty = ty.rem_euclid(map.height.max(1));
            }
            if map.tile(tx, ty).is_some() {
                f(((ty * map.width + tx) / 2) as usize);
            }
        }
    }
}

/// Whether a human may play civilization `race` (see the module docs):
/// `playable` the scenario's list (`Biq::playable_races`, empty: all).
/// Never the barbarians (0).
pub fn race_playable(race: i32, players: &[Player], playable: &[i32]) -> bool {
    race > 0 && (playable.is_empty() || playable.contains(&race)) && !players.iter().any(|p| p.race == race && !p.human)
}

/// Whether what `owner` owns is a human-playable civ's: a player's own, or
/// its civilization's (a civilization no player plays only in a scenario
/// without players).
pub fn playable_owner(owner: Owner, players: &[Player], playable: &[i32]) -> bool {
    match owner {
        // (A player whose civ is random or the human's choice, race < 0,
        // may be the human's if its flag says so.)
        Owner::Player(p) => usize::try_from(p)
            .ok()
            .and_then(|p| players.get(p))
            .is_some_and(|p| p.human && (p.race < 0 || race_playable(p.race, players, playable))),
        Owner::Race(r) => (players.is_empty() || players.iter().any(|p| p.race == r)) && race_playable(r, players, playable),
        Owner::None | Owner::Barbarians => false,
    }
}

/// What the scenario's human-playable civs (`players`, `playable`: see
/// [`playable_owner`]) have revealed at its start (`map_visible`: the
/// scenario reveals the entire map).
pub fn revealed(map: &Map, players: &[Player], playable: &[i32], map_visible: bool) -> Revealed {
    let n = map.tiles.len();
    if map_visible {
        return Revealed { explored: vec![true; n], in_sight: vec![true; n] };
    }
    let a_player = |o: Owner| playable_owner(o, players, playable);
    let mut explored = vec![false; n];
    let mut in_sight = vec![false; n];
    for c in map.cities.iter().filter(|c| a_player(c.owner)) {
        around(map, (c.x, c.y), c.border_level.max(1) + CITY_BEYOND_BORDERS, |i| {
            explored[i] = true;
            in_sight[i] = true;
        });
    }
    for u in map.units.iter().filter(|u| a_player(u.owner)) {
        around(map, (u.x, u.y), UNIT_SIGHT, |i| {
            explored[i] = true;
            in_sight[i] = true;
        });
    }
    Revealed { explored, in_sight }
}

/// A rectangle of tiles: columns `x .. x + w` (the map's x, two per tile
/// column; past the right edge of a map that wraps it goes on at its left)
/// and rows `y .. y + h`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TileBox {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The shortest stretch of `n` places (wrapping round if `wraps`) that holds
/// every place `used` says: its start and length (none if none is).
fn stretch(n: i32, wraps: bool, used: impl Fn(i32) -> bool) -> Option<(i32, i32)> {
    let places: Vec<i32> = (0..n).filter(|&i| used(i)).collect();
    let (&first, &last) = (places.first()?, places.last()?);
    if !wraps {
        return Some((first, last - first + 1));
    }
    // Round a map that wraps: everything but its longest gap (the one past
    // the last place, round to the first, too).
    let mut start = first;
    let mut gap = first + n - last - 1;
    for w in places.windows(2) {
        if w[1] - w[0] - 1 > gap {
            (gap, start) = (w[1] - w[0] - 1, w[1]);
        }
    }
    Some((start, n - gap))
}

/// The smallest rectangle of tiles that holds every tile `revealed` says,
/// across the edges of a map that wraps (none: nothing revealed).
pub fn revealed_box(map: &Map, revealed: &[bool]) -> Option<TileBox> {
    let (w, h) = (map.width, map.height);
    let index = |x: i32, y: i32| ((y * w + x) / 2) as usize;
    let tile_at = |x: i32, y: i32| (x + y) % 2 == 0 && revealed.get(index(x, y)).copied().unwrap_or(false);
    let (x, bw) = stretch(w, map.flags & 1 != 0, |x| (0..h).any(|y| tile_at(x, y)))?;
    let (y, bh) = stretch(h, map.flags & 2 != 0, |y| (0..w).any(|x| tile_at(x, y)))?;
    Some(TileBox { x, y, w: bw, h: bh })
}

/// The world pixels (x, y, w, h, at full size) of the tiles of `b`, with
/// `margin` pixels around (what reaches above and below the tiles: units,
/// mountains, city names; past the map's edge the black beyond).
pub fn box_pixels(b: TileBox, margin: (i32, i32)) -> [i32; 4] {
    // (A tile (x, y) covers 128 × 64 pixels from (64 x, 32 y).)
    let (x0, y0) = (64 * b.x - margin.0, 32 * b.y - margin.1);
    let (x1, y1) = (64 * (b.x + b.w - 1) + 128 + margin.0, 32 * (b.y + b.h - 1) + 64 + margin.1);
    [x0, y0, x1 - x0, y1 - y0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use civ3_assets::biq::{Biq, City, Tile, Unit};

    fn map(width: i32, height: i32, flags: u32) -> Map {
        Map { width, height, flags, tiles: vec![Tile::default(); (width * height / 2) as usize], ..Default::default() }
    }

    fn player(race: i32) -> Player {
        Player { custom: false, human: true, leader_name: String::new(), initial_era: 0, government: 0, race, color: 0 }
    }

    fn unit(owner: Owner, x: i32, y: i32) -> Unit {
        Unit { name: String::new(), owner, experience: 0, unit_type: 0, x, y }
    }

    fn city(owner: Owner, x: i32, y: i32, border_level: i32) -> City {
        City { name: String::new(), owner, size: 1, x, y, buildings: vec![], has_walls: false, has_palace: false, culture: 0, border_level }
    }

    fn count(r: &[bool]) -> usize {
        r.iter().filter(|&&b| b).count()
    }

    #[test]
    fn units_see_the_eight_tiles_around_and_cities_beyond_their_borders() {
        let mut m = map(40, 40, 0);
        m.units.push(unit(Owner::Race(3), 10, 10));
        let r = revealed(&m, &[player(3)], &[], false);
        assert_eq!(count(&r.explored), 9);
        assert!(r.explored[((9 * 40 + 11) / 2) as usize] && r.explored[((10 * 40 + 12) / 2) as usize]);
        // A city of border level 1 (the tiles around it): 21 tiles, its
        // work area; level 2: radius 3 (45: no corners), as the game reveals.
        let mut m = map(40, 40, 0);
        m.cities.push(city(Owner::Player(0), 20, 20, 1));
        assert_eq!(count(&revealed(&m, &[player(3)], &[], false).explored), 21);
        m.cities[0].border_level = 2;
        assert_eq!(count(&revealed(&m, &[player(3)], &[], false).explored), 45);
        // The barbarians' and nobody's reveal nothing; a map without
        // anyone's reveals nothing; one the scenario reveals, all seen.
        let mut m = map(40, 40, 0);
        m.units.push(unit(Owner::Barbarians, 10, 10));
        m.units.push(unit(Owner::Race(7), 20, 10));
        let r = revealed(&m, &[player(3)], &[], false);
        assert!(!r.explored.contains(&true) && !r.in_sight.contains(&true));
        let r = revealed(&m, &[player(3)], &[], true);
        assert!(r.explored.iter().chain(&r.in_sight).all(|&e| e));
    }

    #[test]
    fn what_is_revealed_wraps_round_the_map() {
        let mut m = map(40, 20, 1);
        m.units.push(unit(Owner::Player(0), 0, 10));
        let r = revealed(&m, &[player(1)], &[], false);
        // (39, 9) and (38, 10) are next to (0, 10) across the edge.
        assert!(r.explored[((9 * 40 + 39) / 2) as usize] && r.explored[((10 * 40 + 38) / 2) as usize]);
        assert_eq!(count(&r.explored), 9);
    }

    #[test]
    fn the_box_is_the_smallest_across_the_wrap() {
        let mut m = map(40, 20, 0);
        let mut at = vec![false; m.tiles.len()];
        let mut set = |x: i32, y: i32| at[((y * 40 + x) / 2) as usize] = true;
        set(2, 4);
        set(37, 5);
        set(20, 10);
        // Not wrapping: from column 2 to 37, rows 4 to 10.
        assert_eq!(revealed_box(&m, &at), Some(TileBox { x: 2, y: 4, w: 36, h: 7 }));
        // Wrapping round x: from 20 on, over the edge, to 2 (the gap 3..19
        // is longer than 21..36).
        m.flags = 1;
        assert_eq!(revealed_box(&m, &at), Some(TileBox { x: 20, y: 4, w: 23, h: 7 }));
        // Two tiles either side of the edge only: across it.
        let mut at = vec![false; m.tiles.len()];
        at[((6 * 40) / 2) as usize] = true;
        at[((6 * 40 + 38) / 2) as usize] = true;
        assert_eq!(revealed_box(&m, &at), Some(TileBox { x: 38, y: 6, w: 3, h: 1 }));
        // Round y too.
        m.flags = 3;
        let mut at = vec![false; m.tiles.len()];
        at[((19 * 40 + 1) / 2) as usize] = true;
        at[((0 * 40 + 2) / 2) as usize] = true;
        assert_eq!(revealed_box(&m, &at), Some(TileBox { x: 1, y: 19, w: 2, h: 2 }));
        assert_eq!(revealed_box(&m, &vec![false; m.tiles.len()]), None);
        // In pixels: past the edge of a map that wraps, the margin round it.
        let b = TileBox { x: 38, y: 6, w: 3, h: 1 };
        assert_eq!(box_pixels(b, (64, 32)), [64 * 38 - 64, 32 * 6 - 32, 64 * 2 + 128 + 128, 64 + 64]);
        assert_eq!(box_pixels(TileBox { x: 0, y: 0, w: 2, h: 1 }, (64, 32)), [-64, -32, 64 + 128 + 128, 64 + 64]);
    }

    /// The game's own scenarios (an installation at `CIV3_INSTALL`).
    fn scenario(name: &str) -> Option<Biq> {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        let path = std::path::Path::new(&install).join("Conquests/Conquests").join(name);
        path.is_file().then(|| Biq::open(&path).unwrap())
    }

    /// What each owner of `map` reveals alone (any owner counted), keyed by
    /// owner: the per-civ start the preview unites.
    fn by_owner(map: &Map, players: &[Player]) -> Vec<(Owner, Vec<bool>)> {
        let mut owners: Vec<Owner> = Vec::new();
        for o in map.cities.iter().map(|c| c.owner).chain(map.units.iter().map(|u| u.owner)) {
            if !owners.contains(&o) {
                owners.push(o);
            }
        }
        let everyone: Vec<Player> = players.iter().map(|p| Player { human: true, ..p.clone() }).collect();
        owners
            .into_iter()
            .map(|o| {
                let alone = Map {
                    cities: map.cities.iter().filter(|c| c.owner == o).cloned().collect(),
                    units: map.units.iter().filter(|u| u.owner == o).cloned().collect(),
                    ..map.clone()
                };
                (o, revealed(&alone, &everyone, &[], false).explored)
            })
            .collect()
    }

    #[test]
    fn only_the_civs_a_human_may_play_reveal() {
        // Race 3 the computer's only (LEAD human flag unset), race 5 not in
        // the playable list, race 4 a human's: only race 4's units reveal.
        let mut m = map(40, 40, 0);
        m.units.push(unit(Owner::Race(3), 10, 10));
        m.units.push(unit(Owner::Race(5), 20, 10));
        m.units.push(unit(Owner::Race(4), 30, 30));
        m.units.push(unit(Owner::Player(2), 10, 30));
        m.units.push(unit(Owner::Barbarians, 20, 20));
        let ai = Player { human: false, ..player(3) };
        let players = [ai.clone(), player(4), player(5)];
        let r = revealed(&m, &players, &[3, 4, 6], false);
        assert_eq!(count(&r.explored), 9);
        assert!(r.explored[((30 * 40 + 30) / 2) as usize] && !r.explored[((10 * 40 + 10) / 2) as usize]);
        assert!(!r.explored[((10 * 40 + 20) / 2) as usize] && !r.explored[((30 * 40 + 10) / 2) as usize]);
        // No playable list: every civ but the computer's (and the
        // barbarians) may be played; a player whose civ is chosen at the
        // start (race < 0) is the human's if flagged so.
        let r = revealed(&m, &players, &[], false);
        assert_eq!(count(&r.explored), 27);
        let chosen = [ai, player(4), player(-3)];
        assert!(playable_owner(Owner::Player(2), &chosen, &[4]));
        assert!(!playable_owner(Owner::Player(0), &chosen, &[3, 4]));
        assert!(!playable_owner(Owner::Race(0), &[], &[]) && playable_owner(Owner::Race(9), &[], &[]));
        // A playable civ with nothing revealed: none; the preview frames
        // the whole map then (`preview::area`).
        let mut m = map(40, 40, 0);
        m.units.push(unit(Owner::Race(3), 10, 10));
        let r = revealed(&m, &players, &[], false);
        assert!(!r.explored.contains(&true) && revealed_box(&m, &r.explored).is_none());
    }

    #[test]
    fn the_real_scenarios_reveal_what_their_playable_civs_start_with() {
        let Some(rome) = scenario("2 Rise of Rome.biq") else { return eprintln!("no Civ III install, skipped") };
        // (Scenario, revealed by all players, by the playable civs only.)
        let cases = [
            ("1 Mesopotamia.biq", 231, 231),
            ("2 Rise of Rome.biq", 3178, 2274),
            ("3 Fall of Rome.biq", 2825, 729),
            ("4 Middle Ages.biq", 1964, 1505),
            ("5 Mesoamerica.biq", 135, 135),
            ("6 Age of Discovery.biq", 565, 544),
            ("7 Sengoku - Sword of the Shogun.biq", 0, 0),
            ("8 Napoleonic Europe.biq", 3640, 3640),
            ("9 WWII European Front 6 .biq", 32768, 32768),
            ("Intro2 The Three Sisters.biq", 84, 84),
            ("MP1vAll3.biq", 656, 160),
        ];
        for (name, before, after) in cases {
            let Some(biq) = scenario(name) else { continue };
            let (map, players, playable) = (biq.map().unwrap(), biq.players(), biq.playable_races());
            if biq.map_visible() {
                // (Napoleonic Europe: everyone knows all of it.)
                let r = revealed(&map, &players, &playable, true);
                assert_eq!((count(&r.explored), count(&r.explored)), (before, after), "{name}");
                continue;
            }
            let r = revealed(&map, &players, &playable, false);
            assert_eq!(r.explored, r.in_sight);
            // The union of the playable civs' starts, none of the others'.
            let mut union = vec![false; map.tiles.len()];
            for (owner, seen) in by_owner(&map, &players) {
                if playable_owner(owner, &players, &playable) {
                    union.iter_mut().zip(&seen).for_each(|(u, s)| *u |= s);
                }
            }
            assert_eq!(r.explored, union, "{name}");
            let everyone: Vec<Player> = players.iter().map(|p| Player { human: true, ..p.clone() }).collect();
            let all = count(&revealed(&map, &everyone, &[], false).explored);
            assert_eq!((all, count(&r.explored)), (before, after), "{name}");
        }
        let map = rome.map().unwrap();
        let r = revealed(&map, &rome.players(), &rome.playable_races(), false);
        // Rome (47, 67), border level 3: radius 4 explored.
        let i = |x: i32, y: i32| ((y * map.width + x) / 2) as usize;
        assert!(r.explored[i(47, 67)] && r.explored[i(47 + 4, 67 + 4)] && r.explored[i(47 + 2, 67 - 4)]);
        // The Atlantic and Arabia unknown; the box about the Mediterranean.
        assert!(!r.explored[i(2, 70)] && !r.explored[i(136, 70)]);
        let b = revealed_box(&map, &r.explored).unwrap();
        assert!(b.w < map.width && b.h < map.height, "{b:?}");
        // WWII in the Pacific reveals the entire map.
        let Some(pacific) = scenario("9 WWII in the Pacific.biq") else { return };
        assert!(pacific.map_visible());
        let map = pacific.map().unwrap();
        let r = revealed(&map, &pacific.players(), &pacific.playable_races(), pacific.map_visible());
        assert!(r.explored.iter().all(|&e| e));
        assert_eq!(revealed_box(&map, &r.explored), Some(TileBox { x: 0, y: 0, w: map.width, h: map.height }));
    }
}
