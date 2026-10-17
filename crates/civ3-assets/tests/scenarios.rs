//! Reads the game's own scenarios when an installation is available
//! (`CIV3_INSTALL`, default: the copy on this machine). Skipped otherwise.

use std::path::PathBuf;

use civ3_assets::biq::{Biq, Owner};

fn install() -> Option<PathBuf> {
    let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    let path = PathBuf::from(install);
    path.is_dir().then_some(path)
}

fn scenario(name: &str) -> Option<Biq> {
    let path = install()?.join("Conquests/Conquests").join(name);
    Some(Biq::open(&path).unwrap_or_else(|e| panic!("{path:?}: {e}")))
}

#[test]
fn wwii_in_the_pacific() {
    let Some(biq) = scenario("9 WWII in the Pacific.biq") else { return eprintln!("no Civ III install, skipped") };
    let map = biq.map().expect("a map of its own");
    assert_eq!((map.width, map.height, map.seed, map.flags), (136, 120, 187637348, 0));
    assert_eq!(map.tiles.len(), 136 * 120 / 2);
    // What the game holds after starting it: tile (0, 0) a mountain on
    // grassland, image 53 of xdgc.
    let t = map.tile(0, 0).unwrap();
    assert_eq!((t.terrain, t.texture_file, t.texture_location, t.city), (0x62, 2, 53, -1));
    // Lanchow, the first city, at (7, 25): China's (race 1), size 9, walled.
    let lanchow = &map.cities[0];
    assert_eq!((lanchow.name.as_str(), lanchow.owner, lanchow.size, lanchow.x, lanchow.y), ("Lanchow", Owner::Race(1), 9, 7, 25));
    assert!(lanchow.has_walls);
    let t = map.tile(7, 25).unwrap();
    assert_eq!((t.city, t.river, t.overlays, t.border_race()), (0, 0x83, 3, Some(1)));
    assert_eq!((map.cities.len(), map.units.len(), map.colonies.len()), (108, 455, 9));
    // Every city sits on a tile that names it.
    for (i, c) in map.cities.iter().enumerate() {
        assert_eq!(map.tile(c.x, c.y).map(|t| t.city), Some(i as i16), "{}", c.name);
    }
    let players = biq.players();
    assert_eq!(players.iter().map(|p| (p.race, p.color)).collect::<Vec<_>>(), [(5, 22), (2, 29), (4, 27), (1, 3), (3, 1)]);
    assert!(players.iter().all(|p| p.custom && p.initial_era == 3));
    let rules = biq.rules().expect("rules of its own");
    assert_eq!(rules.races[1].name, "China");
    assert_eq!(rules.races[1].adjective, "Chinese");
    assert_eq!(rules.races[1].culture_group, 4);
    assert_eq!(rules.eras.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["ERAS_Ancient_Times", "ERAS_Middle_Ages", "ERAS_Industrial_Age", "ERAS_Modern_Era"]);
    assert_eq!(rules.unit_types[0].key, "PRTO_Worker");
    // The icons the game's tables hold for this scenario.
    let icons: Vec<i32> = rules.resources.iter().map(|r| r.icon).collect();
    assert_eq!(icons, [0, 1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 24, 22, 23]);
    assert_eq!((rules.max_town, rules.max_city), (6, 12));
    assert!(rules.buildings.iter().any(|b| b.bombard_defence > 0), "walls");
    assert_eq!(biq.search_folders(), ["WWII in the Pacific"]);
}

#[test]
fn every_conquests_scenario_reads() {
    let Some(install) = install() else { return };
    let mut n = 0;
    for entry in std::fs::read_dir(install.join("Conquests/Conquests")).unwrap().flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("biq")) {
            continue;
        }
        let biq = Biq::open(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        if let Some(map) = biq.map() {
            assert_eq!(map.tiles.len() as i32, map.width * map.height / 2, "{path:?}");
            for c in &map.cities {
                assert!(map.tile(c.x, c.y).is_some_and(|t| t.city >= 0), "{path:?}: {} off its tile", c.name);
            }
            if let Some(rules) = biq.rules() {
                assert!(map.units.iter().all(|u| (0..rules.unit_types.len() as i32).contains(&u.unit_type)), "{path:?}");
            }
        }
        n += 1;
    }
    assert!(n >= 10, "only {n} scenarios");
    // The game's own rules: the default for scenarios without theirs.
    let rules = Biq::open(&install.join("Conquests/conquests.biq")).unwrap().rules().expect("the game's rules");
    assert!(rules.unit_types.len() > 50 && rules.races.len() > 20);
}
