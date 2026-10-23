//! Civilization III world state, independent of where it came from: read
//! live from the game's memory, or parsed from a tracer dump.

use std::collections::HashMap;
use std::sync::Arc;

use civ3_sys::c3x::{District, DistrictConfigs, DistrictFlags, DistrictKind, NaturalWonderKind, WonderKind};

/// The fields of a `Tile` the renderer needs.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Civ3Tile {
    pub square_type: u32,
    /// Base terrain image: `(file << 8) | location` (docs §5).
    pub square_parts: u16,
    pub overlays: u32,
    pub terrain_overlays: u32,
    pub river: u8,
    pub resource: i32,
    pub owner: i8,
    pub city: i16,
    pub fow_status: u32,
    pub visibility: u32,
    /// Bit c: explored by civ c.
    pub explored: u32,
    pub v3: u32,
    pub visibility_d0: u32,
    /// Per civ: the improvements (overlay bits 0..7) that civ remembers.
    pub visible_overlays: [u8; 32],
    /// -1, or the victory location on this tile.
    pub victory_location: i16,
    /// The tile building (colony, airfield, radar, outpost) here.
    pub building: Option<i16>,
    pub ruins: bool,
    /// The city whose area the tile belongs to (-1: none).
    pub city_area: i16,
    pub continent: i16,
    /// The district on this tile: its index in [`Civ3World::districts`].
    pub district: Option<u32>,
}

impl Civ3Tile {
    /// Real (underlying) terrain id, `SquareType` bits 8..12.
    pub fn real_terrain(&self) -> u32 {
        (self.square_type >> 8) & 0xF
    }

    /// Effective terrain id (hills, forest, ... when present), bits 12..16.
    pub fn terrain(&self) -> u32 {
        (self.square_type >> 12) & 0xF
    }
}

/// A city.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Civ3City {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub civ: i32,
    /// Population.
    pub size: i32,
    /// Bit i: has improvement i.
    pub improvements: [u8; 32],
    /// The wonder or small wonder the city is building (an improvement id).
    pub wonder_in_production: Option<i32>,
    /// What its map label shows (docs §10.8, [`crate::city_labels`]): the name,
    /// what it builds (`Order_Type`, `Order_ID`; by name: an improvement or a
    /// unit type), the `Common_Policy` bits that outline the label (1 its size
    /// box, 0x10 its production plate), the food surplus, the turns until it
    /// grows and until it completes its order (`city_label::NEVER`: never; `None`
    /// where a dump has none) and the icons of its HUD box (`None`: only the
    /// capital's star).
    pub name: String,
    pub order: (i32, i32),
    pub order_name: Option<String>,
    pub policy: i32,
    pub food_income: i32,
    pub grow_turns: Option<i32>,
    pub build_turns: Option<i32>,
    pub hud_icons: Option<Vec<u8>>,
    /// The harbour, airport and barracks icons (`civ3_sys::city_label::VETERAN_*`);
    /// `None` where a dump has none: by the city's buildings (`icon_buildings`).
    pub veteran_icons: Option<u8>,
}

impl Civ3City {
    pub fn has_improvement(&self, id: i32) -> bool {
        (0..256).contains(&id) && self.improvements[id as usize >> 3] & (1 << (id & 7)) != 0
    }
}

/// An improvement that gives its city an icon on the map (§10.8): id,
/// `ITF_Veteran_*` flags, bit per player that knows the tech obsoleting it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct IconBuilding {
    pub id: i32,
    pub flags: u32,
    pub obsolete_for: u32,
}

/// An improvement that makes towns look walled: id, government it needs
/// (-1 any), bit per player that knows the tech obsoleting it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct WallBuilding {
    pub id: i32,
    pub government: i32,
    pub obsolete_for: u32,
}

/// What an improvement's rules say about a city counting as having it
/// (C3X's `has_active_building`, docs §18).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Civ3Improvement {
    /// The government it needs (`None`: any).
    pub government: Option<i32>,
    /// Bit per player that knows the tech obsoleting it.
    pub obsolete_for: u32,
}

/// A unit the game draws, in the animator's drawing order (docs §10).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Civ3Unit {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub civ: i32,
    pub unit_type: i32,
    /// Native LeaderKind (2 selects scientific leader art).
    pub leader_kind: i32,
    pub experience: i32,
    pub damage: i32,
    pub moves: i32,
    /// Game-thread Unit::get_max_move_points, including army/civ/mod bonuses.
    /// None for cold previews and old traces without this calculated stat.
    pub max_move_points: Option<i32>,
    pub container: i32,
    pub state: i32,
    pub always_on_top: bool,
    /// World pixel of the anchor (tile centre when idle).
    pub pixel: (i32, i32),
    pub direction: i32,
    pub animation: i32,
    pub frame: i32,
    pub visible: bool,
    /// Armies (§10.11): the member shown, and how many members there are.
    pub army_member: Option<i32>,
    pub army_size: i32,
    /// On its way between tiles.
    pub moving: bool,
}

/// An animation the animator draws besides units (docs §10.14): an
/// explosion, smoke, a bomber's bombs, a city's disorder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Civ3Effect {
    pub source: civ3_sys::EffectSource,
    /// Its tile, and the world pixel of its anchor (full size).
    pub tile: (i32, i32),
    pub pixel: (i32, i32),
    /// Facing (`enum direction`), `AnimationType`, frame in the block.
    pub direction: i32,
    pub animation: i32,
    pub frame: i32,
    /// Loaded and not hidden.
    pub shown: bool,
    /// Its art's INI file, game-relative (`Art/Animations/...`), as the
    /// game's search folders find it; empty if not known.
    pub ini: String,
}

/// The map: shape plus every tile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Civ3World {
    /// The civ whose view is drawn (fog of war, remembered improvements).
    pub player: i32,
    /// `BIC.ResourceTypes[i].IconID`.
    pub resource_icons: Vec<i32>,
    /// Per resource type: shown to `player` (prerequisite tech known).
    pub resource_visible: Vec<bool>,
    /// Every civ's era (0..3).
    pub eras: Vec<i32>,
    /// Every civ's colour table (`ntpNN.pcx`).
    pub color_tables: Vec<i32>,
    /// Every civ's capital (city id) and race.
    pub capitals: Vec<i32>,
    pub races: Vec<i32>,
    /// Per race: its culture group (city art).
    pub culture_groups: Vec<i32>,
    /// Population above which a town is a city, and a city a metropolis.
    pub max_town: i32,
    pub max_city: i32,
    pub cities: Vec<Civ3City>,
    pub governments: Vec<i32>,
    pub wall_buildings: Vec<WallBuilding>,
    pub icon_buildings: Vec<IconBuilding>,
    /// Outposts: (tile building id, owner).
    pub outposts: Vec<(i32, i32)>,
    /// Drawn units, in drawing order.
    pub units: Vec<Civ3Unit>,
    /// The members of the armies drawn (not drawn themselves; each with
    /// its army as `container`), among them the one each army shows.
    pub army_members: Vec<Civ3Unit>,
    /// Civilopedia keys of unit types (`PRTO_...`) and eras (`ERAS_...`).
    pub unit_type_keys: Vec<String>,
    pub era_keys: Vec<String>,
    /// Per unit type: attack, defence, movement (tiles), hit point bonus,
    /// class (0 land, 1 sea, 2 air), worker actions (bit 1: found cities),
    /// abilities (bit `n`: ability `n`; traces from before have none).
    pub unit_type_stats: Vec<[i32; 7]>,
    /// The tile under the mouse, if it is over the map.
    pub mouse_tile: Option<(i32, i32)>,
    /// The selected unit (also while its cursor blinks off).
    pub current_unit: Option<i32>,
    /// The city the city screen shows, while it is open (§16.8).
    pub focus_city: Option<i32>,
    /// Base hit points per experience level.
    pub experience_hit_points: Vec<i32>,
    /// Every unit's tile and container (-1: not loaded).
    pub unit_tiles: Vec<(i32, i32, i32)>,
    /// The animations drawn besides units, in the animator's order (§10.14).
    pub effects: Vec<Civ3Effect>,
    /// The engine's Geography/Territory selection for the HUD and advisor.
    pub navigator_modes: civ3_sys::ui_hooks::NavigatorModes,
    /// The game's `P_*` preference flags.
    pub preferences: u32,
    /// Effective normalized Clean Map flags; zero while the game toggles it off.
    pub clean_map: u32,
    /// `p_debug_mode_bits`: bit 3 shows every city's full label (docs §10.8).
    pub debug_mode_bits: u32,
    /// Per civ: its adjective (`Leader_get_civ_adjective`), which a label may
    /// show beside the city's name.
    pub civ_adjectives: Vec<String>,
    /// The selected unit (id) and the selection cursor's frame, while shown.
    pub selection: Option<(i32, i32)>,
    /// The command waiting for a target tile, and what the game made of the
    /// tile under the mouse (docs §10.15).
    pub targeting: civ3_sys::world::Targeting,
    /// The go-to path to the tile under the mouse, as the game's path
    /// finder found it (docs §10.15).
    pub goto: Option<crate::targeting::GotoPath>,
    /// How `player` stands with each civ (by civ; empty if unknown).
    pub relations: Vec<civ3_sys::world::Relation>,
    /// C3X's districts (docs §18): empty without C3X districts or while
    /// they are off. Link them to their tiles with [`Civ3World::set_districts`].
    pub districts: Vec<District>,
    /// What C3X loaded to draw them with (shared: it changes with the game).
    pub district_configs: Arc<DistrictConfigs>,
    /// Every improvement's rules for counting as present, by id (read only
    /// while districts are on).
    pub improvements: Vec<Civ3Improvement>,
    /// Per civ, the keys of the buildings wonders give it for free (see
    /// `Civ3::free_improvements`).
    pub free_improvements: Vec<Vec<i32>>,
    /// In Civ III x units: tiles exist where x + y is even.
    pub width: i32,
    pub height: i32,
    pub wrap_x: bool,
    pub wrap_y: bool,
    pub seed: u32,
    /// `Map.Flags`: bit 0 wrap x, bit 1 wrap y, bit 2 polar ice caps.
    pub flags: u32,
    /// Indexed `(y·width + x) / 2`.
    pub tiles: Vec<Civ3Tile>,
}

impl Civ3World {
    /// Whether the viewing player sees the tile now (not fogged).
    pub fn visible_now(&self, tile: &Civ3Tile) -> bool {
        let bit = 1u32 << (self.player & 31);
        (tile.fow_status | tile.v3 | tile.visibility | tile.visibility_d0) & bit != 0
    }

    /// The tile's overlays as the viewing player knows them
    /// (`Tile::m42_Get_Overlays`, §6.5): as they are where the player sees
    /// the tile now, else only the improvements (bits 0..7) last seen.
    pub fn seen_overlays(&self, tile: &Civ3Tile) -> u32 {
        match usize::try_from(self.player) {
            Ok(civ) if civ > 0 && civ < 32 && !self.visible_now(tile) => u32::from(tile.visible_overlays[civ]),
            _ => tile.overlays,
        }
    }

    pub fn city(&self, id: i32) -> Option<&Civ3City> {
        self.cities.iter().find(|c| c.id == id)
    }

    pub fn index(&self, x: i32, y: i32) -> usize {
        ((y * self.width + x) / 2) as usize
    }

    /// Tile (x, y) after wrapping; `None` outside a non-wrapping edge or for
    /// coordinates that are not a tile (x + y odd).
    pub fn tile(&self, x: i32, y: i32) -> Option<&Civ3Tile> {
        let x = if self.wrap_x { x.rem_euclid(self.width) } else { x };
        let y = if self.wrap_y { y.rem_euclid(self.height) } else { y };
        if x < 0 || y < 0 || x >= self.width || y >= self.height || (x + y) % 2 != 0 {
            return None;
        }
        self.tiles.get(self.index(x, y))
    }

    /// Takes the districts and links each one to its tile
    /// ([`Civ3Tile::district`]), so a tile finds its district at once.
    pub fn set_districts(&mut self, districts: Vec<District>) {
        for old in std::mem::take(&mut self.districts) {
            if let Some(tile) = self.district_tile(&old) {
                tile.district = None;
            }
        }
        for (i, d) in districts.iter().enumerate() {
            if let Some(tile) = self.district_tile(d) {
                tile.district = Some(i as u32);
            }
        }
        self.districts = districts;
    }

    fn district_tile(&mut self, d: &District) -> Option<&mut Civ3Tile> {
        let on_map = (0..self.width).contains(&d.x) && (0..self.height).contains(&d.y) && (d.x + d.y) % 2 == 0;
        let at = self.index(d.x, d.y);
        self.tiles.get_mut(at).filter(|_| on_map)
    }
}

/// The districts' part of a tracer dump: what draws them and where they are
/// (read back by [`Trace::parse`]). Nothing when C3X has none.
pub fn district_lines(configs: &DistrictConfigs, improvements: &[Civ3Improvement], free: &[Vec<i32>], districts: &[District]) -> Vec<String> {
    if !configs.flags.any() && districts.is_empty() {
        return Vec::new();
    }
    // Lists are comma separated, `-` for none; a name, which can have spaces,
    // goes last.
    let list = |v: Vec<String>| if v.is_empty() { "-".to_owned() } else { v.join(",") };
    let on: Vec<&str> = FLAG_NAMES.iter().filter(|(_, get)| get(&configs.flags)).map(|(name, _)| *name).collect();
    let mut out = vec![
        format!("district_flags {}", on.join(" ")),
        format!("district_work_area {} {}", configs.workable_tiles, configs.mod_dir),
    ];
    out.push("# district_kind index render_strategy type columns width height x_offset y_offset bits generated flags on_tile resources images buildings name".to_owned());
    for (i, k) in configs.districts.iter().enumerate() {
        out.push(format!(
            "district_kind {i} {} {} {} {} {} {} {} {:#x} {} {} {} {} {} {} {}",
            k.render_strategy,
            k.kind,
            k.columns,
            k.size.0,
            k.size.1,
            k.offset.0,
            k.offset.1,
            kind_bits(k),
            k.generated_resource,
            k.generated_resource_flags,
            k.resource_prereq_on_tile,
            k.resource_prereqs,
            list(k.images.clone()),
            list(k.dependent_buildings.iter().map(|b| b.to_string()).collect()),
            k.name
        ));
    }
    out.push("# wonder_kind index improvement column row construct_column construct_row alt width height square_types on_rivers image name".to_owned());
    for (i, w) in configs.wonders.iter().enumerate() {
        let alt = w.alt.map_or("-".to_owned(), |(a, b)| format!("{},{},{},{}", a.0, a.1, b.0, b.1));
        out.push(format!(
            "wonder_kind {i} {} {} {} {} {} {alt} {} {} {:#x} {} {} {}",
            w.improvement.unwrap_or(-1),
            w.cell.0,
            w.cell.1,
            w.construct_cell.0,
            w.construct_cell.1,
            w.size.0,
            w.size.1,
            w.buildable_square_types,
            w.buildable_on_rivers as u8,
            list(Some(w.image.clone()).filter(|s| !s.is_empty()).into_iter().collect()),
            w.name
        ));
    }
    for (i, n) in configs.natural_wonders.iter().enumerate() {
        let image = list(Some(n.image.clone()).filter(|s| !s.is_empty()).into_iter().collect());
        out.push(format!("natural_wonder_kind {i} {} {} {image} {}", n.cell.0, n.cell.1, n.name));
    }
    out.push("# improvement id government obsolete_for".to_owned());
    for (i, b) in improvements.iter().enumerate() {
        out.push(format!("improvement {i} {} {:#x}", b.government.unwrap_or(-1), b.obsolete_for));
    }
    out.push("# free_improvements civ keys".to_owned());
    for (civ, keys) in free.iter().enumerate().filter(|(_, keys)| !keys.is_empty()) {
        out.push(format!("free_improvements {civ} {}", keys.iter().map(i32::to_string).collect::<Vec<_>>().join(",")));
    }
    out.push("# district x y kind completed built_by wonder_state wonder wonder_city natural_wonder".to_owned());
    for d in districts {
        out.push(format!(
            "district {} {} {} {} {} {} {} {} {}",
            d.x, d.y, d.kind, d.completed as u8, d.built_by, d.wonder_state, d.wonder, d.wonder_city, d.natural_wonder
        ));
    }
    out
}

/// The names of the `DistrictFlags` in the dump's `district_flags` line.
const FLAG_NAMES: [(&str, fn(&DistrictFlags) -> bool); 10] = [
    ("districts", |f| f.districts),
    ("natural_wonders", |f| f.natural_wonders),
    ("neighborhood", |f| f.neighborhood),
    ("wonder", |f| f.wonder),
    ("distribution_hub", |f| f.distribution_hub),
    ("energy_grid", |f| f.energy_grid),
    ("bridge", |f| f.bridge),
    ("canal", |f| f.canal),
    ("great_wall", |f| f.great_wall),
    ("forests_over_roads", |f| f.forests_over_roads),
];

/// `DistrictKind`'s yes/no settings as the dump's bit set.
fn kind_bits(k: &DistrictKind) -> u32 {
    [k.by_era, k.by_culture, k.align_to_coast, k.draw_over_resources, k.subsumes_tile_resource, k.allow_irrigation_from]
        .iter()
        .enumerate()
        .fold(0, |bits, (i, &on)| bits | (on as u32) << i)
}

/// A list field of a dump line (`-`: empty).
fn list_field(s: &str) -> impl Iterator<Item = &str> {
    s.split(',').filter(move |_| s != "-")
}

/// Reads one of the district lines of a dump into `world`; the `district`
/// lines are collected in `districts` until the tiles are known.
fn parse_district_line(world: &mut Civ3World, districts: &mut Vec<District>, f: &[&str]) -> Result<(), &'static str> {
    let at = |k: usize| f.get(k).and_then(|s| num(s)).map(|v| v as i32).ok_or("bad number");
    let bits = |k: usize| f.get(k).and_then(|s| num(s)).map(|v| v as u32).ok_or("bad number");
    let text = |k: usize| f.get(k).copied().ok_or("missing field");
    let image = |k: usize| text(k).map(|s| if s == "-" { String::new() } else { s.to_owned() });
    let rest = |k: usize| f.get(k..).unwrap_or_default().join(" ");
    let configs = Arc::make_mut(&mut world.district_configs);
    match f[0] {
        "district_flags" => {
            let on = |name: &str| f[1..].contains(&name);
            configs.flags = DistrictFlags {
                districts: on("districts"),
                natural_wonders: on("natural_wonders"),
                neighborhood: on("neighborhood"),
                wonder: on("wonder"),
                distribution_hub: on("distribution_hub"),
                energy_grid: on("energy_grid"),
                bridge: on("bridge"),
                canal: on("canal"),
                great_wall: on("great_wall"),
                forests_over_roads: on("forests_over_roads"),
            };
        }
        "district_work_area" => {
            configs.workable_tiles = at(1)?;
            configs.mod_dir = rest(2);
        }
        "district_kind" => {
            let flag = |i: u32| bits(9).map(|b| b >> i & 1 != 0);
            configs.districts.push(DistrictKind {
                render_strategy: at(2)?,
                kind: at(3)?,
                columns: at(4)?,
                size: (at(5)?, at(6)?),
                offset: (at(7)?, at(8)?),
                by_era: flag(0)?,
                by_culture: flag(1)?,
                align_to_coast: flag(2)?,
                draw_over_resources: flag(3)?,
                subsumes_tile_resource: flag(4)?,
                allow_irrigation_from: flag(5)?,
                generated_resource: at(10)?,
                generated_resource_flags: at(11)?,
                resource_prereq_on_tile: at(12)?,
                resource_prereqs: at(13)?,
                images: list_field(text(14)?).map(str::to_owned).collect(),
                dependent_buildings: list_field(text(15)?).filter_map(|s| s.parse().ok()).collect(),
                name: rest(16),
            });
        }
        "wonder_kind" => {
            let alt = match text(7)? {
                "-" => None,
                s => {
                    let v: Vec<i32> = s.split(',').filter_map(|v| v.parse().ok()).collect();
                    (v.len() == 4).then(|| ((v[0], v[1]), (v[2], v[3])))
                }
            };
            configs.wonders.push(WonderKind {
                improvement: Some(at(2)?).filter(|&i| i >= 0),
                cell: (at(3)?, at(4)?),
                construct_cell: (at(5)?, at(6)?),
                alt,
                size: (at(8)?, at(9)?),
                buildable_square_types: bits(10)?,
                buildable_on_rivers: at(11)? != 0,
                image: image(12)?,
                name: rest(13),
            });
        }
        "natural_wonder_kind" => {
            configs.natural_wonders.push(NaturalWonderKind { cell: (at(2)?, at(3)?), image: image(4)?, name: rest(5) });
        }
        "improvement" => {
            let id = usize::try_from(at(1)?).map_err(|_| "bad improvement")?;
            if world.improvements.len() <= id {
                world.improvements.resize(id + 1, Civ3Improvement::default());
            }
            world.improvements[id] = Civ3Improvement { government: Some(at(2)?).filter(|&g| g >= 0), obsolete_for: bits(3)? };
        }
        "free_improvements" => {
            let civ = usize::try_from(at(1)?).map_err(|_| "bad civ")?;
            if world.free_improvements.len() <= civ {
                world.free_improvements.resize(civ + 1, Vec::new());
            }
            world.free_improvements[civ] = list_field(text(2)?).filter_map(|s| s.parse().ok()).collect();
        }
        "district" => districts.push(District {
            x: at(1)?,
            y: at(2)?,
            kind: at(3)?,
            completed: at(4)? != 0,
            built_by: at(5)?,
            wonder_state: at(6)?,
            wonder: at(7)?,
            wonder_city: at(8)?,
            natural_wonder: at(9)?,
        }),
        _ => {}
    }
    Ok(())
}

/// What the main map view shows.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Civ3View {
    pub camera_x: i32,
    pub camera_y: i32,
    pub tile_x: (i32, i32),
    pub tile_y: (i32, i32),
    /// The zoomed-out view (half size; camera in its pixels, §3).
    pub zoomed_out: bool,
}

/// A tracer dump (`civ3-trace.txt`): world, view and the game's draw list.
#[derive(Clone, Debug, Default)]
pub struct Trace {
    /// The scenario's art folder and its search folders, as the game
    /// names them (relative to the install's `Conquests` folders).
    pub scenario_folder: Option<String>,
    pub search_roots: Vec<String>,
    /// Address of the `Map_Renderer` the redraw used (also the canvas of
    /// map blits).
    pub map_renderer: u32,
    pub world: Civ3World,
    pub view: Civ3View,
    /// Sheet file names by (Map_Renderer member, element).
    pub files: HashMap<(String, u32), String>,
    pub draws: Vec<TraceDraw>,
    /// The game's text output during the redraw.
    pub texts: Vec<crate::text::TextRun>,
    /// The mirrored interface, if recorded.
    pub ui: Option<crate::ui::Frame>,
}

/// One `Sprite_draw_on_map` call the game made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceDraw {
    /// Map_Renderer array and index, `None` if unresolved.
    pub sprite: Option<(String, u32)>,
    pub x: i32,
    pub y: i32,
    /// Which blit: "map", "blend" or "colored".
    pub kind: String,
    /// Sprite size, if the trace has it.
    pub size: Option<(u32, u32)>,
    /// The blit's other arguments (for "colored": canvas, second canvas,
    /// colour table, p6).
    pub params: Vec<u32>,
}

#[derive(Debug, thiserror::Error)]
#[error("trace line {line}: {message}")]
pub struct TraceError {
    pub line: usize,
    pub message: String,
}

fn num(s: &str) -> Option<i64> {
    match s.strip_prefix("0x") {
        Some(h) => i64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

impl Trace {
    pub fn parse(text: &str) -> Result<Trace, TraceError> {
        let mut t = Trace::default();
        let mut tiles: Vec<(i32, i32, Civ3Tile)> = Vec::new();
        let mut districts: Vec<District> = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let err = |message: &str| TraceError { line: i + 1, message: message.to_owned() };
            let f: Vec<&str> = line.split_whitespace().collect();
            let n = |k: usize| f.get(k).and_then(|s| num(s)).ok_or_else(|| err("bad number"));
            match f.first().copied() {
                Some("#") if f.get(1) == Some(&"civ3-bridge") && f.get(3) == Some(&"map_renderer") => {
                    t.map_renderer = n(4)? as u32;
                }
                Some("text") => t.texts.extend(crate::text::TextRun::parse(line)),
                Some("ui_image") if f.len() >= 12 => {
                    let v = (1..12).map(|k| n(k).map(|v| v as i32)).collect::<Result<Vec<_>, _>>()?;
                    let path = f[12..].join(" ");
                    t.ui.get_or_insert_with(Default::default).draws.push(crate::ui::ScreenDraw::Image {
                        path,
                        rect: [v[7], v[8], v[9], v[10]],
                        x: v[0],
                        y: v[1],
                        keyed: v[2] != 0,
                        clip: [v[3], v[4], v[5], v[6]],
                        mask: None,
                        palette: None,
                        opacity: 255,
                    });
                }
                Some("ui_resized_image") if f.len() >= 17 => {
                    let v=(1..16).map(|k|n(k).map(|v|v as i32)).collect::<Result<Vec<_>,_>>()?;
                    t.ui.get_or_insert_with(Default::default).draws.push(crate::ui::ScreenDraw::ResizedImage {
                        path:f[16..].join(" "),rect:[v[7],v[8],v[9],v[10]],x:v[0],y:v[1],size:(v[11],v[12]),
                        sample_step:[v[13] as u32,v[14] as u32],keyed:v[2]!=0,clip:[v[3],v[4],v[5],v[6]],mask:None,palette:None,opacity:255,
                    });
                }
                Some("ui_surface") if f.len() >= 12 => {
                    let v = (1..12).map(|k| n(k)).collect::<Result<Vec<_>, _>>()?;
                    t.ui.get_or_insert_with(Default::default).draws.push(crate::ui::ScreenDraw::Surface {
                        image: v[0] as u32,
                        x: v[1] as i32,
                        y: v[2] as i32,
                        clip: [v[3] as i32, v[4] as i32, v[5] as i32, v[6] as i32],
                        rect: [v[7] as i32, v[8] as i32, v[9] as i32, v[10] as i32],
                        size: (
                            f.get(12).and_then(|s| s.parse().ok()).unwrap_or((v[7] + v[9]) as i32),
                            f.get(13).and_then(|s| s.parse().ok()).unwrap_or((v[8] + v[10]) as i32),
                        ),
                    });
                }
                Some(kind @ ("ui_flc" | "ui_flc_plain")) if f.len() >= 9 => {
                    let v = (1..8).map(|k| n(k).map(|v| v as i32)).collect::<Result<Vec<_>, _>>()?;
                    t.ui.get_or_insert_with(Default::default).draws.push(crate::ui::ScreenDraw::Flc {
                        path: f[8..].join(" "),
                        chunk: v[6] as u32,
                        x: v[0],
                        y: v[1],
                        clip: [v[2], v[3], v[4], v[5]],
                        plain: kind == "ui_flc_plain",
                    });
                }
                Some("ui_underlay") => {
                    let range = n(1)? as usize..n(2)? as usize;
                    t.ui.get_or_insert_with(Default::default).underlay_draws.push(range);
                }
                Some("ui_image_indices") if f.len() >= 3 => {
                    let transparent = if f[1] == "-" { Some(Vec::new()) } else { f[1].split(',').map(|s| s.parse::<u8>().ok()).collect::<Option<Vec<_>>>() };
                    let shade = if f[2] == "-" { Some(Vec::new()) } else {
                        f[2].split(',').map(|pair| {
                            let (index, level) = pair.split_once(':')?;
                            let (index, level) = (index.parse::<u8>().ok()?, level.parse::<u8>().ok()?);
                            (level <= 16).then_some((index, level))
                        }).collect::<Option<Vec<_>>>()
                    };
                    if let (Some(transparent), Some(shade), Some(draw)) = (transparent, shade, t.ui.as_mut().and_then(|u| u.draws.last_mut())) {
                        let indices = menu::screen::Indices { transparent, shade, ..Default::default() };
                        match draw {
                            crate::ui::ScreenDraw::Image { path, rect, x, y, clip, palette, .. } => {
                                *draw = crate::ui::ScreenDraw::Indexed { path: path.clone(), rect: *rect, x: *x, y: *y, clip: *clip, palette: palette.clone(), indices };
                            }
                            crate::ui::ScreenDraw::ResizedImage { path, rect, x, y, clip, palette, size, sample_step, .. } => {
                                let indices = menu::screen::Indices { resize: Some(menu::screen::IndexedResize { size: (size.0 as u32, size.1 as u32), sample_step: *sample_step }), ..indices };
                                *draw = crate::ui::ScreenDraw::Indexed { path: path.clone(), rect: *rect, x: *x, y: *y, clip: *clip, palette: palette.clone(), indices };
                            }
                            _ => {}
                        }
                    }
                }
                Some("ui_image_frame") if f.len() == 2 => {
                    if let (Ok(frame), Some(crate::ui::ScreenDraw::Indexed { indices, .. })) = (f[1].parse::<u32>(), t.ui.as_mut().and_then(|u| u.draws.last_mut())) { indices.frame = Some(frame); }
                }
                Some("ui_image_palette_alpha") if f.len() >= 7 => {
                    let values = f[1..6].iter().map(|v| v.parse::<u8>().ok()).collect::<Option<Vec<_>>>();
                    if let (Some(v), Some(crate::ui::ScreenDraw::Indexed { indices, .. })) = (values, t.ui.as_mut().and_then(|u| u.draws.last_mut())) {
                        if v[1] > 0 && u16::from(v[0]) + u16::from(v[1]) <= 256 {
                            indices.palette_alpha = Some(menu::screen::PaletteAlpha { source: f[6..].join(" "), first: v[0], count: v[1], skip: [v[2], v[3], v[4]] });
                        }
                    }
                }
                Some("ui_image_opacity") if f.len() >= 2 => {
                    let value = n(1)?.clamp(0, 255) as u8;
                    if let Some(crate::ui::ScreenDraw::Image { opacity, .. } | crate::ui::ScreenDraw::ResizedImage { opacity, .. }) = t.ui.as_mut().and_then(|u| u.draws.last_mut()) {
                        *opacity = value;
                    }
                }
                Some("ui_image_palette") if f.len() >= 4 => {
                    let (first, count) = (n(1)? as u32, n(2)? as u32);
                    if let Some(crate::ui::ScreenDraw::Image { palette, .. } | crate::ui::ScreenDraw::ResizedImage { palette, .. }) = t.ui.as_mut().and_then(|u| u.draws.last_mut()) {
                        *palette = Some(menu::screen::PaletteFrom { image: f[3..].join(" "), first, count });
                    }
                }
                Some("ui_image_mask") if f.len() >= 4 => {
                    let at = (n(1)? as i32, n(2)? as i32);
                    if let Some(crate::ui::ScreenDraw::Image { mask, .. } | crate::ui::ScreenDraw::ResizedImage { mask, .. }) = t.ui.as_mut().and_then(|u| u.draws.last_mut()) {
                        *mask = Some(crate::ui::Mask { path: f[3..].join(" "), at });
                    }
                }
                Some("ui_fill") if f.len() >= 12 => {
                    let v = (1..12).map(|k| n(k).map(|v| v as i32)).collect::<Result<Vec<_>, _>>()?;
                    t.ui.get_or_insert_with(Default::default).draws.push(crate::ui::ScreenDraw::Fill {
                        rect: [v[0], v[1], v[2], v[3]],
                        rgb: [v[4] as u8, v[5] as u8, v[6] as u8],
                        alpha: f.get(12).and_then(|a| a.parse().ok()).unwrap_or(255),
                        clip: [v[7], v[8], v[9], v[10]],
                    });
                }
                Some("ui_text") if f.len() >= 6 => {
                    let clip = [n(1)? as i32, n(2)? as i32, n(3)? as i32, n(4)? as i32];
                    let rest = line.splitn(6, ' ').nth(5).unwrap_or("");
                    if let Some(run) = crate::text::TextRun::parse(rest) {
                        t.ui.get_or_insert_with(Default::default).draws.push(crate::ui::ScreenDraw::Text { run, clip });
                    }
                }
                // The forms composed and which draws each made (what tells
                // the screens apart, docs §16.12).
                Some("ui_form") if f.len() >= 7 => {
                    let v = (1..7).map(|k| n(k)).collect::<Result<Vec<_>, _>>()?;
                    t.ui.get_or_insert_with(Default::default).forms.push((v[0] as u32, v[1] as u32, (v[2] as i32, v[3] as i32), (v[4] as i32, v[5] as i32)));
                }
                Some("#") if f.get(1) == Some(&"ui_canvas_draws") && f.len() >= 5 => {
                    let (canvas, start, end) = (n(2)? as u32, n(3)? as usize, n(4)? as usize);
                    t.ui.get_or_insert_with(Default::default).form_draws.push((canvas, start..end));
                }
                Some("zoomed_out") => t.view.zoomed_out = n(1)? != 0,
                Some("scenario_folder") => t.scenario_folder = Some(f[1..].join(" ")),
                Some("search_root") => t.search_roots.push(f[1..].join(" ")),
                Some("view") => {
                    t.view = Civ3View {
                        camera_x: n(2)? as i32,
                        camera_y: n(3)? as i32,
                        tile_x: (n(5)? as i32, n(6)? as i32),
                        tile_y: (n(8)? as i32, n(9)? as i32),
                        zoomed_out: f.get(11).is_some_and(|z| *z == "1"),
                    }
                }
                Some("map") => {
                    t.world.width = n(1)? as i32;
                    t.world.height = n(2)? as i32;
                    t.world.wrap_x = n(4)? != 0;
                    t.world.wrap_y = n(5)? != 0;
                    if f.get(6) == Some(&"seed") {
                        t.world.seed = n(7)? as u32;
                    }
                    if f.get(8) == Some(&"flags") {
                        t.world.flags = n(9)? as u32;
                    }
                }
                Some("player") => t.world.player = n(1)? as i32,
                Some("resource_icons") => {
                    t.world.resource_icons = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?;
                }
                Some("color_tables") => {
                    t.world.color_tables = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?;
                }
                Some("capitals") => t.world.capitals = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?,
                Some("races") => t.world.races = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?,
                Some("culture_groups") => {
                    t.world.culture_groups = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?;
                }
                Some("city_sizes") => {
                    t.world.max_town = n(1)? as i32;
                    t.world.max_city = n(2)? as i32;
                }
                Some("city") => {
                    // `city ... bits wonder [order_type order_id policy food_income
                    // grow build icons]<TAB>order name<TAB>city name` (the later
                    // columns are newer than the first dumps).
                    let named: Vec<&str> = line.split('\t').collect();
                    let opt = |k: usize| f.get(k).filter(|_| named.len() > 1).map(|_| n(k)).transpose();
                    let icons = match f.get(14).filter(|_| named.len() > 1) {
                        Some(&"-") => Some(Vec::new()),
                        Some(list) => Some(list.split(',').filter_map(|c| c.parse().ok()).collect()),
                        None => None,
                    };
                    t.world.cities.push(Civ3City {
                        id: n(1)? as i32,
                        x: n(2)? as i32,
                        y: n(3)? as i32,
                        civ: n(4)? as i32,
                        size: n(5)? as i32,
                        improvements: {
                            let mut v = [0u8; 32];
                            if let Some(hex) = f.get(6) {
                                for (k, b) in v.iter_mut().enumerate() {
                                    *b = hex.get(2 * k..2 * k + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(0);
                                }
                            }
                            v
                        },
                        wonder_in_production: f.get(7).map(|_| n(7)).transpose()?.map(|w| w as i32).filter(|&w| w >= 0),
                        order: (opt(8)?.unwrap_or(0) as i32, opt(9)?.unwrap_or(0) as i32),
                        policy: opt(10)?.unwrap_or(0) as i32,
                        food_income: opt(11)?.unwrap_or(0) as i32,
                        grow_turns: opt(12)?.map(|v| v as i32),
                        build_turns: opt(13)?.map(|v| v as i32),
                        hud_icons: icons,
                        veteran_icons: opt(15)?.map(|v| v as u8),
                        order_name: named.get(1).filter(|s| !s.is_empty()).map(|s| (*s).to_owned()),
                        name: named.get(2).map(|s| (*s).to_owned()).unwrap_or_default(),
                    });
                }
                Some("civ_adjective") => {
                    let civ = n(1)? as usize;
                    if t.world.civ_adjectives.len() <= civ {
                        t.world.civ_adjectives.resize(civ + 1, String::new());
                    }
                    t.world.civ_adjectives[civ] = f[2..].join(" ");
                }
                Some("debug_mode_bits") => t.world.debug_mode_bits = n(1)? as u32,
                Some(kind @ ("unit" | "army_unit")) => {
                    let v = (1..f.len()).map(n).collect::<Result<Vec<_>, _>>()?;
                    let at = |i: usize| v.get(i).copied().unwrap_or(0) as i32;
                    let unit = Civ3Unit {
                        id: at(0),
                        x: at(1),
                        y: at(2),
                        civ: at(3),
                        unit_type: at(4),
                        damage: at(5),
                        moves: at(6),
                        container: at(7),
                        state: at(8),
                        always_on_top: at(9) != 0,
                        pixel: (at(10), at(11)),
                        direction: at(12),
                        animation: at(13),
                        frame: at(14),
                        visible: at(15) != 0,
                        experience: at(16),
                        army_member: Some(at(17)).filter(|&m| m >= 0 && v.len() > 17),
                        army_size: at(18),
                        moving: at(19) != 0,
                        leader_kind: at(20),
                        max_move_points: v.get(21).copied().filter(|&n| n >= 0).map(|n| n as i32),
                    };
                    if kind == "unit" {
                        t.world.units.push(unit);
                    } else {
                        t.world.army_members.push(unit);
                    }
                }
                Some("unit_type_stats") => {
                    let opt = |k: usize| -> Result<i32, TraceError> { Ok(f.get(k).map(|_| n(k)).transpose()?.unwrap_or(0) as i32) };
                    t.world.unit_type_stats.push([n(2)? as i32, n(3)? as i32, n(4)? as i32, n(5)? as i32, opt(6)?, opt(7)?, opt(8)?]);
                }
                Some("experience_hit_points") => {
                    t.world.experience_hit_points = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?;
                }
                Some("unit_tile") => t.world.unit_tiles.push((n(1)? as i32, n(2)? as i32, n(3)? as i32)),
                // `effect city|tile|extra id x y px py direction animation frame shown<TAB>ini`
                Some("effect") if f.len() >= 11 => {
                    let id = n(2)? as i32;
                    let source = match f[1] {
                        "city" => civ3_sys::EffectSource::City { effect: id },
                        "tile" => civ3_sys::EffectSource::Tile { effect: id },
                        _ => civ3_sys::EffectSource::Extra,
                    };
                    let v = (3..11).map(|k| n(k).map(|v| v as i32)).collect::<Result<Vec<_>, _>>()?;
                    t.world.effects.push(Civ3Effect {
                        source,
                        tile: (v[0], v[1]),
                        pixel: (v[2], v[3]),
                        direction: v[4],
                        animation: v[5],
                        frame: v[6],
                        shown: v[7] != 0,
                        ini: line.split_once('\t').map(|(_, p)| p.to_owned()).unwrap_or_default(),
                    });
                }
                Some("outpost") => t.world.outposts.push((n(1)? as i32, n(2)? as i32)),
                Some("mouse_tile") => t.world.mouse_tile = Some((n(1)? as i32, n(2)? as i32)),
                Some("focus_city") => t.world.focus_city = Some(n(1)? as i32),
                Some("current_unit") => t.world.current_unit = Some(n(1)? as i32),
                Some("navigator_modes") => {
                    use civ3_sys::ui_hooks::{NavigatorMode, NavigatorModes};
                    let mode = |geography| if geography { NavigatorMode::Geography } else { NavigatorMode::Territory };
                    t.world.navigator_modes = NavigatorModes { main: mode(n(1)? != 0), advisor: mode(n(2)? != 0) };
                }
                Some("preferences") => t.world.preferences = n(1)? as u32,
                Some("selection") => t.world.selection = Some((n(1)? as i32, n(2)? as i32)),
                Some("targeting") => {
                    t.world.targeting = civ3_sys::world::Targeting {
                        mode: n(1)? as i32,
                        refused: n(2)? != 0,
                        range: n(3)? as i32,
                        target_ok: n(4)? != 0,
                        armed: n(5)? != 0,
                        start: (n(6)? as i32, n(7)? as i32),
                        pointer_tile: Some((n(8)? as i32, n(9)? as i32)).filter(|&(x, y)| x >= 0 && y >= 0),
                        range_grid: Some(civ3_sys::world::RangeGrid { centre: (n(10)? as i32, n(11)? as i32), range: n(12)? as i32 }).filter(|g| g.range >= 0),
                        // (Not in older traces.)
                        grid: n(13).is_ok_and(|g| g != 0),
                    }
                }
                Some("goto_path") => {
                    let v = (1..f.len()).map(n).collect::<Result<Vec<_>, _>>()?;
                    t.world.goto = crate::targeting::GotoPath::parse(&v);
                }
                Some("unit_type_key") => t.world.unit_type_keys.push(f.get(2).copied().unwrap_or("").to_owned()),
                Some("era_key") => t.world.era_keys.push(f.get(2).copied().unwrap_or("").to_owned()),
                Some("governments") => t.world.governments = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?,
                Some("wall_building") => t.world.wall_buildings.push(WallBuilding {
                    id: n(1)? as i32,
                    government: n(2)? as i32,
                    obsolete_for: n(3)? as u32,
                }),
                Some("icon_building") => t.world.icon_buildings.push(IconBuilding {
                    id: n(1)? as i32,
                    flags: n(2)? as u32,
                    obsolete_for: n(3)? as u32,
                }),
                Some("eras") => {
                    t.world.eras = (1..f.len()).map(|k| n(k).map(|v| v as i32)).collect::<Result<_, _>>()?;
                }
                Some("relations") => {
                    use civ3_sys::world::Relation;
                    t.world.relations = f[1..]
                        .iter()
                        .map(|v| match *v {
                            "w" => Relation::War,
                            "d" => Relation::Defense,
                            "p" => Relation::Peace,
                            _ => Relation::None,
                        })
                        .collect();
                }
                Some("resource_visible") => {
                    t.world.resource_visible = f[1..].iter().map(|v| *v == "1").collect();
                }
                Some(
                    "district_flags" | "district_work_area" | "district_kind" | "wonder_kind" | "natural_wonder_kind" | "improvement"
                    | "free_improvements" | "district",
                ) => parse_district_line(&mut t.world, &mut districts, &f).map_err(|m| err(m))?,
                Some("file") if f.len() >= 3 => {
                    let path = f[3..].join(" ");
                    t.files.insert((f[1].to_owned(), n(2)? as u32), path);
                }
                Some("tile") => {
                    let tile = Civ3Tile {
                        square_type: n(3)? as u32,
                        square_parts: n(4)? as u16,
                        overlays: n(5)? as u32,
                        terrain_overlays: n(6)? as u32,
                        river: n(7)? as u8,
                        resource: n(8)? as i32,
                        owner: n(9)? as i8,
                        city: n(10)? as i16,
                        fow_status: n(11)? as u32,
                        visibility: n(12)? as u32,
                        explored: f.get(13).map(|_| n(13)).transpose()?.unwrap_or(0) as u32,
                        v3: f.get(14).map(|_| n(14)).transpose()?.unwrap_or(0) as u32,
                        visibility_d0: f.get(15).map(|_| n(15)).transpose()?.unwrap_or(0) as u32,
                        visible_overlays: {
                            let mut v = [0u8; 32];
                            if let Some(hex) = f.get(16) {
                                for (k, b) in v.iter_mut().enumerate() {
                                    *b = hex.get(2 * k..2 * k + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(0);
                                }
                            }
                            v
                        },
                        victory_location: f.get(17).map(|_| n(17)).transpose()?.unwrap_or(-1) as i16,
                        building: f.get(18).map(|_| n(18)).transpose()?.map(|b| b as i16).filter(|&b| b >= 0),
                        ruins: f.get(19).map(|_| n(19)).transpose()?.unwrap_or(0) != 0,
                        city_area: f.get(20).map(|_| n(20)).transpose()?.unwrap_or(-1) as i16,
                        continent: f.get(21).map(|_| n(21)).transpose()?.unwrap_or(-1) as i16,
                        district: None,
                    };
                    tiles.push((n(1)? as i32, n(2)? as i32, tile));
                }
                Some("draw") => {
                    let sprite = (f[1] != "?").then(|| Ok::<_, TraceError>((f[1].to_owned(), n(2)? as u32))).transpose()?;
                    let kind = f.get(5).filter(|k| !k.starts_with("0x")).map_or("map", |k| k).to_owned();
                    // Newer traces: kind w h params (params are hex).
                    let size = match (f.get(6), f.get(7)) {
                        (Some(w), Some(h)) if !w.starts_with("0x") && !h.starts_with("0x") => Some((n(6)? as u32, n(7)? as u32)),
                        _ => None,
                    };
                    let first_param = if size.is_some() { 8 } else { 6 };
                    let params = (first_param..f.len()).map(|k| n(k).map(|v| v as u32)).collect::<Result<_, _>>()?;
                    t.draws.push(TraceDraw { sprite, x: n(3)? as i32, y: n(4)? as i32, kind, size, params });
                }
                _ => {}
            }
        }
        let w = &mut t.world;
        w.tiles = vec![Civ3Tile::default(); (w.width * w.height / 2).max(0) as usize];
        for (x, y, tile) in tiles {
            let i = ((y * w.width + x) / 2) as usize;
            if let Some(slot) = w.tiles.get_mut(i) {
                *slot = tile;
            }
        }
        w.set_districts(districts);
        Ok(t)
    }
}

#[cfg(test)]
mod navigator_trace_tests {
    use super::*;
    use civ3_sys::ui_hooks::{NavigatorMode, NavigatorModes};

    #[test]
    fn traces_preserve_independent_navigator_modes_and_old_defaults() {
        assert_eq!(Trace::parse("").unwrap().world.navigator_modes, NavigatorModes::default());
        for (text, main, advisor) in [
            ("navigator_modes 1 0", NavigatorMode::Geography, NavigatorMode::Territory),
            ("navigator_modes 0 1", NavigatorMode::Territory, NavigatorMode::Geography),
            ("navigator_modes 0 0", NavigatorMode::Territory, NavigatorMode::Territory),
        ] {
            assert_eq!(Trace::parse(text).unwrap().world.navigator_modes, NavigatorModes { main, advisor });
        }
        assert!(Trace::parse("navigator_modes 1").is_err());
    }
    #[test]
    fn a_ui_trace_keeps_its_forms_and_what_each_drew() {
        // As `crate::trace` writes them: the forms composed, bottom to top,
        // and per canvas the range of draws it made.
        let trace = Trace::parse("ui_form 0x66b570 0x9fdd84 0 0 1920 1080\nui_form 0x66e838 0xc887fc 448 156 1024 768\n# ui_canvas_draws 0x9fdd84 0 1\n# ui_canvas_draws 0xc887fc 1 2\nui_fill 0 0 1920 1080 0 0 0 0 0 1920 1080\nui_fill 448 156 1024 768 1 2 3 0 0 1920 1080\n").unwrap();
        let ui = trace.ui.unwrap();
        assert_eq!(ui.forms, [(0x66_B570, 0x9F_DD84, (0, 0), (1920, 1080)), (0x66_E838, 0xC8_87FC, (448, 156), (1024, 768))]);
        assert_eq!(ui.form_draws, [(0x9F_DD84, 0..1), (0xC8_87FC, 1..2)]);
        assert_eq!(ui.draws.len(), 2);
    }
    #[test]
    fn indexed_ui_trace_retains_palette_transparency_and_blend_levels() {
        let trace = Trace::parse("ui_image 20 30 1 0 0 1024 768 8 4 48 49 Art/shadow with spaces.pcx\nui_image_palette 64 2 Art/ntp03.pcx\nui_image_indices 0,1,255 248:8,249:10\n").unwrap();
        assert!(matches!(&trace.ui.unwrap().draws[..], [crate::ui::ScreenDraw::Indexed { path, rect: [8, 4, 48, 49], x: 20, y: 30, palette: Some(palette), indices, .. }]
            if path == "Art/shadow with spaces.pcx" && palette.first == 64 && palette.count == 2 && palette.image == "Art/ntp03.pcx"
                && indices.transparent == [0, 1, 255] && indices.shade == [(248, 8), (249, 10)]));
        let trace = Trace::parse("ui_image 20 30 1 0 0 1024 768 8 4 48 49 shadow.pcx\nui_image_indices - 248:8\n").unwrap();
        assert!(matches!(&trace.ui.unwrap().draws[..], [crate::ui::ScreenDraw::Indexed { indices, .. }] if indices.transparent.is_empty() && indices.shade == [(248, 8)]));
    }

    #[test]
    fn resized_indexed_and_smoke_trace_metadata_preserve_native_rules_and_old_defaults() {
        let text = "ui_resized_image 20 30 1 21 31 25 33 8 4 9 5 5 3 117964 109226 Art/smoke with spaces.pcx\nui_image_palette 64 2 Art/ntp03.pcx\nui_image_indices 0,1,255 -\nui_image_frame 17\nui_image_palette_alpha 224 16 0 0 255 Art/Explosion Palette.pcx\n";
        let trace = Trace::parse(text).unwrap();
        let crate::ui::ScreenDraw::Indexed { indices, palette, rect, .. } = &trace.ui.unwrap().draws[0] else { panic!("indexed resized trace") };
        assert_eq!(*rect, [8, 4, 9, 5]);
        assert_eq!(indices.resize, Some(menu::screen::IndexedResize { size: (5, 3), sample_step: [117964, 109226] }));
        assert_eq!(indices.frame, Some(17));
        assert_eq!(indices.palette_alpha.as_ref().unwrap(), &menu::screen::PaletteAlpha { source: "Art/Explosion Palette.pcx".into(), first: 224, count: 16, skip: [0, 0, 255] });
        assert_eq!(palette.as_ref().unwrap().first, 64);
        let old = Trace::parse("ui_image 0 0 1 0 0 8 8 0 0 8 8 old.pcx\nui_image_indices - 248:8\n").unwrap();
        assert!(matches!(&old.ui.unwrap().draws[0], crate::ui::ScreenDraw::Indexed { indices, .. } if indices.frame.is_none() && indices.resize.is_none() && indices.palette_alpha.is_none() && indices.shade == [(248, 8)]));
    }

    #[test]
    fn resized_ui_trace_retains_sampler_palette_mask_and_paths_with_spaces() {
        let trace=Trace::parse("ui_resized_image 20 30 1 0 0 1024 768 8 4 49 49 31 31 100352 100352 Art/icon with spaces.pcx\nui_image_opacity 128\nui_image_palette 0 64 Art/palette file.pcx\nui_image_mask 2 3 Art/mask file.pcx").unwrap();
        let draw=&trace.ui.unwrap().draws[0];
        assert!(matches!(draw,crate::ui::ScreenDraw::ResizedImage { path,rect:[8,4,49,49],size:(31,31),sample_step:[100352,100352],opacity:128,palette:Some(p),mask:Some(m),.. }
            if path=="Art/icon with spaces.pcx" && p.image=="Art/palette file.pcx" && p.count==64 && m.path=="Art/mask file.pcx" && m.at==(2,3)));
    }

}
