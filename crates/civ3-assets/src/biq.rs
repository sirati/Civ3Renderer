//! Civilization III scenario files (`.biq`, and the `.bix`/`.bic` of the
//! earlier games): the rules, the map, cities, units and players a
//! scenario defines.
//!
//! A file is usually compressed ([`crate::dcl`]); unpacked it starts with
//! `BIC` and is a list of sections: a four-letter name, a record count, then
//! the records, each a length and that many bytes. `FLAV` is the exception
//! (groups of records without lengths). Only the fields a map needs are
//! decoded; the layouts follow the community's format notes (Gramphos'
//! `bicformat`, OpenCiv3's QueryCiv3) and were checked against the game's
//! memory after loading a scenario (docs/civ3-map-rendering.md §19).
//!
//! A scenario without rules of its own (no `BLDG` section) uses the game's
//! (`conquests.biq`); one without a map of its own (no `WMAP`) has the game
//! make one.

use std::ops::Range;

/// A section: its name and its records' byte ranges in [`Biq::data`].
#[derive(Clone, Debug)]
struct Section {
    name: [u8; 4],
    records: Vec<Range<usize>>,
}

/// An unpacked scenario file, its sections found.
#[derive(Clone, Debug)]
pub struct Biq {
    data: Vec<u8>,
    sections: Vec<Section>,
}

#[derive(Debug, thiserror::Error)]
pub enum BiqError {
    #[error(transparent)]
    Dcl(#[from] crate::dcl::DclError),
    #[error("not a scenario file (starts with {0:02x?})")]
    NotBiq(Vec<u8>),
    #[error("section {section} at {at:#x}: {what}")]
    Bad { section: String, at: usize, what: &'static str },
}

/// Little-endian reads from a record, out of range reading as 0 (a short
/// record of an older version).
#[derive(Copy, Clone)]
struct Rec<'a>(&'a [u8]);

impl Rec<'_> {
    fn u8(self, at: usize) -> u8 {
        self.0.get(at).copied().unwrap_or(0)
    }
    fn i16(self, at: usize) -> i16 {
        self.0.get(at..at + 2).map_or(0, |b| i16::from_le_bytes([b[0], b[1]]))
    }
    fn i32(self, at: usize) -> i32 {
        self.0.get(at..at + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u32(self, at: usize) -> u32 {
        self.i32(at) as u32
    }
    /// A zero-terminated string in a field of `len` bytes (Windows-1252,
    /// read as Latin-1).
    fn text(self, at: usize, len: usize) -> String {
        let bytes = self.0.get(at..(at + len).min(self.0.len())).unwrap_or_default();
        bytes.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect()
    }
}

/// Who owns a city, unit or colony in a scenario.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    None,
    Barbarians,
    /// A civilization (a `RACE` index), whichever player plays it.
    Race(i32),
    /// A scenario player (a `LEAD` index).
    Player(i32),
}

impl Owner {
    fn of(kind: i32, owner: i32) -> Owner {
        match kind {
            1 => Owner::Barbarians,
            2 => Owner::Race(owner),
            3 => Owner::Player(owner),
            _ => Owner::None,
        }
    }
}

/// A `TILE` record (45 bytes in Conquests files). The Conquests fields
/// are what the game copies into its `Tile` (docs §2): `overlays` is
/// `Tile.Overlays`, `terrain` byte 1 of `Tile.SquareType` (low nibble
/// real terrain, high nibble effective), `bonuses` `Tile.Terrain_Overlays`
/// shifted down 16 bits.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Tile {
    /// `River_Code`: bit per direction (0 N, 1 NE, … 7 NW).
    pub river: u8,
    /// The territory's owner: 0 none, else 33 + its race (the game
    /// works the borders out again).
    pub border: u8,
    pub resource: i32,
    /// The base terrain image (`SquareParts`, docs §5).
    pub texture_location: u8,
    pub texture_file: u8,
    pub barbarian_tribe: i16,
    /// The city here (a `CITY` index), -1 none.
    pub city: i16,
    /// The colony here (a `CLNY` index), -1 none.
    pub colony: i16,
    pub continent: i16,
    /// 0: a victory point location, -1: none.
    pub victory_location: i16,
    pub ruin: bool,
    pub overlays: u32,
    pub terrain: u8,
    pub bonuses: u32,
    /// Bit per player who has seen the tile at the start (0: none).
    pub fog_of_war: u16,
}

impl Tile {
    fn read(r: Rec) -> Tile {
        Tile {
            river: r.u8(0),
            border: r.u8(1),
            resource: r.i32(2),
            texture_location: r.u8(6),
            texture_file: r.u8(7),
            barbarian_tribe: r.i16(14),
            city: r.i16(16),
            colony: r.i16(18),
            continent: r.i16(20),
            victory_location: r.i16(23),
            ruin: r.i32(25) != 0,
            overlays: r.u32(29),
            terrain: r.u8(34),
            fog_of_war: r.i16(37) as u16,
            bonuses: r.u32(39),
        }
    }

    /// The owner of its territory as a race index.
    pub fn border_race(&self) -> Option<i32> {
        (self.border > 32).then(|| i32::from(self.border) - 33)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct City {
    pub name: String,
    pub owner: Owner,
    pub size: i32,
    pub x: i32,
    pub y: i32,
    /// Its buildings (`BLDG` indices).
    pub buildings: Vec<i32>,
    pub has_walls: bool,
    pub has_palace: bool,
    pub culture: i32,
    /// How far its culture reaches (1: the tiles around it; the game's
    /// borders and what it has explored at the start follow it, docs §19.4).
    pub border_level: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub name: String,
    pub owner: Owner,
    pub experience: i32,
    /// A `PRTO` index.
    pub unit_type: i32,
    pub x: i32,
    pub y: i32,
}

/// A colony, airfield, radar tower or outpost.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Colony {
    pub owner: Owner,
    pub x: i32,
    pub y: i32,
    /// 0 colony, 1 airfield, 2 radar tower, 3 outpost.
    pub kind: i32,
}

/// The map a scenario brings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Map {
    /// In the game's x units (tiles where x + y is even).
    pub width: i32,
    pub height: i32,
    pub seed: i32,
    /// Bit 0 wraps x, bit 1 wraps y, bit 2 polar ice caps.
    pub flags: u32,
    /// Indexed `(y · width + x) / 2`.
    pub tiles: Vec<Tile>,
    pub cities: Vec<City>,
    pub units: Vec<Unit>,
    pub colonies: Vec<Colony>,
}

impl Map {
    pub fn tile(&self, x: i32, y: i32) -> Option<&Tile> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height || (x + y) % 2 != 0 {
            return None;
        }
        self.tiles.get(((y * self.width + x) / 2) as usize)
    }
}

/// A scenario player (`LEAD`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    /// Whether the scenario sets the player's civilization data (era,
    /// government, colour) itself.
    pub custom: bool,
    pub human: bool,
    pub leader_name: String,
    pub initial_era: i32,
    pub government: i32,
    /// A `RACE` index; -2 random, -3 any.
    pub race: i32,
    /// The colour table (`ntpNN.pcx`).
    pub color: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub name: String,
    /// The cell in `resources.pcx`.
    pub icon: i32,
    pub prerequisite: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitType {
    pub name: String,
    /// Its Civilopedia key (`PRTO_...`), which names its art folder.
    pub key: String,
    pub attack: i32,
    pub defence: i32,
    pub movement: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Race {
    pub name: String,
    pub adjective: String,
    pub culture_group: i32,
    pub default_color: i32,
    pub unique_color: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Era {
    pub name: String,
    pub key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Building {
    pub name: String,
    /// Defence against bombardment (walls have some).
    pub bombard_defence: i32,
    /// The government it needs (-1: any).
    pub government: i32,
    /// `ImprovementFlags` (the veteran-unit flags among them).
    pub flags: u32,
    pub obsolete_by: i32,
}

/// The rules a scenario (or the game's `conquests.biq`) defines, as far as
/// drawing its map needs them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    pub resources: Vec<Resource>,
    pub unit_types: Vec<UnitType>,
    pub races: Vec<Race>,
    pub eras: Vec<Era>,
    pub buildings: Vec<Building>,
    /// Largest town and largest city (larger: a city, a metropolis).
    pub max_town: i32,
    pub max_city: i32,
}

impl Biq {
    /// Reads a scenario file's bytes (compressed or not).
    pub fn read(bytes: &[u8]) -> Result<Biq, BiqError> {
        let data = if crate::dcl::is_compressed(bytes) { crate::dcl::explode(bytes)? } else { bytes.to_vec() };
        Biq::from_unpacked(data)
    }

    pub fn open(path: &std::path::Path) -> std::io::Result<Biq> {
        let bytes = std::fs::read(path)?;
        Biq::read(&bytes).map_err(std::io::Error::other)
    }

    fn from_unpacked(data: Vec<u8>) -> Result<Biq, BiqError> {
        if !data.starts_with(b"BIC") {
            return Err(BiqError::NotBiq(data.iter().take(8).copied().collect()));
        }
        let mut sections = Vec::new();
        let mut at = 4;
        let int = |at: usize| data.get(at..at + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        while at + 8 <= data.len() {
            let name: [u8; 4] = data[at..at + 4].try_into().expect("4 bytes");
            let section = String::from_utf8_lossy(&name).into_owned();
            let bad = |at: usize, what| BiqError::Bad { section: section.clone(), at, what };
            let count = int(at + 4).ok_or_else(|| bad(at, "no count"))?;
            if !(0..=1 << 20).contains(&count) || !name.iter().all(|b| b.is_ascii_uppercase() || *b == b'#') {
                return Err(bad(at, "not a section"));
            }
            at += 8;
            let mut records = Vec::new();
            if &name == b"FLAV" {
                // Groups of flavours: a count, then per flavour 4 bytes, a
                // 256-byte name and its relations to the others.
                for _ in 0..count {
                    let n = int(at).ok_or_else(|| bad(at, "truncated"))?;
                    at += 4;
                    for _ in 0..n.clamp(0, 1024) {
                        let relations = int(at + 260).ok_or_else(|| bad(at, "truncated"))?;
                        let len = 264 + 4 * relations.clamp(0, 1024) as usize;
                        records.push(at..at + len);
                        at += len;
                    }
                }
            } else {
                for _ in 0..count {
                    let len = int(at).filter(|&l| l >= 0).ok_or_else(|| bad(at, "bad record length"))? as usize;
                    if at + 4 + len > data.len() {
                        return Err(bad(at, "record past the end"));
                    }
                    records.push(at + 4..at + 4 + len);
                    at += 4 + len;
                }
            }
            sections.push(Section { name, records });
        }
        Ok(Biq { data, sections })
    }

    /// The unpacked file.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The records of section `name`, if the file has it.
    pub fn records(&self, name: &str) -> Option<Vec<&[u8]>> {
        let s = self.sections.iter().find(|s| s.name == name.as_bytes())?;
        Some(s.records.iter().map(|r| &self.data[r.clone()]).collect())
    }

    fn recs(&self, name: &str) -> Vec<Rec<'_>> {
        self.records(name).unwrap_or_default().into_iter().map(Rec).collect()
    }

    /// Whether the scenario has rules of its own.
    pub fn has_rules(&self) -> bool {
        self.records("BLDG").is_some()
    }

    /// The `;`-separated folders the scenario's art is searched in.
    pub fn search_folders(&self) -> Vec<String> {
        let Some(game) = self.recs("GAME").into_iter().next() else { return Vec::new() };
        let playable = game.i32(8).clamp(0, 256) as usize;
        let text = game.text(116 + 4 * playable, 5200);
        text.split(';').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect()
    }

    /// Whether every player knows the whole map from the start (`GAME`:
    /// *Reveal entire map*, a byte after the plague and flag settings that
    /// follow the search folders).
    pub fn map_visible(&self) -> bool {
        let Some(game) = self.recs("GAME").into_iter().next() else { return false };
        let playable = game.i32(8).clamp(0, 256) as usize;
        // Search folders (5200), the alliance of every playable civ, 13
        // victory settings, a byte, 5 alliance names (256), the wars between
        // them (5 × 5), the alliance victory type, the plague (a name of 260,
        // a byte, 7 numbers), 260 bytes of text, flag units, a byte, the gold
        // for capture.
        let at = 116 + 4 * playable + 5200 + 4 * playable + 52 + 1 + 1280 + 100 + 4 + 260 + 1 + 28 + 260 + 4 + 1 + 4;
        game.u8(at) != 0
    }

    /// The scenario's map, if it brings one.
    pub fn map(&self) -> Option<Map> {
        let wmap = self.recs("WMAP").into_iter().next()?;
        let resources = wmap.i32(0).clamp(0, 1 << 16) as usize;
        let at = 4 + 4 * resources;
        let (height, width) = (wmap.i32(at + 4), wmap.i32(at + 24));
        let (seed, flags) = (wmap.i32(at + 156), wmap.u32(at + 160));
        let tiles: Vec<Tile> = self.recs("TILE").into_iter().map(Tile::read).collect();
        if width <= 0 || height <= 0 || tiles.len() != (width * height / 2) as usize {
            return None;
        }
        let cities = self
            .recs("CITY")
            .into_iter()
            .map(|c| {
                let n = c.i32(30).clamp(0, 1024) as usize;
                let at = 34 + 4 * n;
                City {
                    name: c.text(2, 24),
                    owner: Owner::of(c.i32(26), c.i32(at + 4)),
                    size: c.i32(at + 8),
                    x: c.i32(at + 12),
                    y: c.i32(at + 16),
                    buildings: (0..n).map(|k| c.i32(34 + 4 * k)).collect(),
                    has_walls: c.u8(0) != 0,
                    has_palace: c.u8(1) != 0,
                    culture: c.i32(at),
                    border_level: c.i32(at + 24),
                }
            })
            .collect();
        let units = self
            .recs("UNIT")
            .into_iter()
            .map(|u| Unit {
                name: u.text(60, 57),
                owner: Owner::of(u.i32(32), u.i32(40)),
                experience: u.i32(36),
                unit_type: u.i32(44),
                x: u.i32(52),
                y: u.i32(56),
            })
            .collect();
        let colonies =
            self.recs("CLNY").into_iter().map(|c| Colony { owner: Owner::of(c.i32(0), c.i32(4)), x: c.i32(8), y: c.i32(12), kind: c.i32(16) }).collect();
        Some(Map { width, height, seed, flags, tiles, cities, units, colonies })
    }

    /// The scenario's players.
    pub fn players(&self) -> Vec<Player> {
        self.recs("LEAD")
            .into_iter()
            .map(|l| {
                let units = l.i32(48).clamp(0, 1024) as usize;
                let at = 52 + 8 * units;
                let techs = l.i32(at + 4).clamp(0, 1024) as usize;
                let at = at + 8 + 4 * techs;
                Player {
                    custom: l.i32(0) != 0,
                    human: l.i32(4) != 0,
                    leader_name: l.text(8, 32),
                    initial_era: l.i32(at + 4),
                    government: l.i32(at + 12),
                    race: l.i32(at + 16),
                    color: l.i32(at + 20),
                }
            })
            .collect()
    }

    /// The scenario's rules, if it has its own.
    pub fn rules(&self) -> Option<Rules> {
        if !self.has_rules() {
            return None;
        }
        let eras: Vec<Era> = self.recs("ERAS").into_iter().map(|e| Era { name: e.text(0, 64), key: e.text(64, 32) }).collect();
        let races = self
            .recs("RACE")
            .into_iter()
            .map(|r| {
                let cities = r.i32(0).clamp(0, 1 << 16) as usize;
                let at = 4 + 24 * cities;
                let leaders = r.i32(at).clamp(0, 1 << 16) as usize;
                // Leader name 32, title 24, Civilopedia key 32, adjective
                // 40, name 40, noun 40; then two file names per era.
                let text = at + 4 + 32 * leaders;
                let at = text + 208 + 520 * eras.len();
                Race {
                    name: r.text(text + 128, 40),
                    adjective: r.text(text + 88, 40),
                    culture_group: r.i32(at),
                    default_color: r.i32(at + 28),
                    unique_color: r.i32(at + 32),
                }
            })
            .collect();
        let rule = self.recs("RULE").into_iter().next();
        let (max_town, max_city) = rule.map_or((6, 12), |r| {
            let parts = r.i32(96).clamp(0, 1024) as usize;
            let at = 100 + 4 * parts;
            (r.i32(at + 144), r.i32(at + 148))
        });
        Some(Rules {
            resources: self.recs("GOOD").into_iter().map(|g| Resource { name: g.text(0, 24), icon: g.i32(68), prerequisite: g.i32(72) }).collect(),
            unit_types: self
                .recs("PRTO")
                .into_iter()
                .map(|p| UnitType { name: p.text(4, 32), key: p.text(36, 32), defence: p.i32(84), attack: p.i32(92), movement: p.i32(108) })
                .collect(),
            races,
            eras,
            buildings: self
                .recs("BLDG")
                .into_iter()
                .map(|b| Building { name: b.text(64, 32), bombard_defence: b.i32(152), government: b.i32(208), flags: b.u32(232), obsolete_by: b.i32(220) })
                .collect(),
            max_town,
            max_city,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of `sections` (name, records), uncompressed.
    fn file(sections: &[(&str, Vec<Vec<u8>>)]) -> Vec<u8> {
        let mut out = b"BICX".to_vec();
        for (name, records) in sections {
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&(records.len() as i32).to_le_bytes());
            for r in records {
                out.extend_from_slice(&(r.len() as i32).to_le_bytes());
                out.extend_from_slice(r);
            }
        }
        out
    }

    #[test]
    fn sections_and_a_map() {
        let mut wmap = vec![0u8; 172];
        wmap[0..4].copy_from_slice(&1i32.to_le_bytes()); // one resource
        wmap[12..16].copy_from_slice(&2i32.to_le_bytes()); // height
        wmap[32..36].copy_from_slice(&4i32.to_le_bytes()); // width
        wmap[164..168].copy_from_slice(&5i32.to_le_bytes()); // seed
        let mut tile = vec![0u8; 45];
        tile[1] = 0x22;
        tile[16..18].copy_from_slice(&0i16.to_le_bytes());
        tile[34] = 0x62;
        let data = file(&[("VER#", vec![vec![0; 720]]), ("WMAP", vec![wmap]), ("TILE", vec![tile; 4])]);
        let biq = Biq::read(&data).unwrap();
        assert!(!biq.has_rules());
        assert!(biq.rules().is_none());
        let map = biq.map().unwrap();
        assert_eq!((map.width, map.height, map.seed, map.tiles.len()), (4, 2, 5, 4));
        let t = map.tile(0, 0).unwrap();
        assert_eq!((t.terrain, t.city, t.border_race()), (0x62, 0, Some(1)));
        assert!(map.tile(1, 0).is_none());
    }

    #[test]
    fn rejects_other_files() {
        assert!(matches!(Biq::read(b"PK\x03\x04 not a scenario"), Err(BiqError::NotBiq(_))));
        let mut cut = file(&[("TILE", vec![vec![0; 45]])]);
        cut.truncate(cut.len() - 1);
        assert!(matches!(Biq::read(&cut), Err(BiqError::Bad { .. })));
    }
}
