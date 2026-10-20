//! Reading the game's world state: map, tiles, view.

use std::collections::HashSet;

use bytemuck::{Pod, Zeroable};
use inproc::Ptr32;

use crate::build::{Addresses, Build, IMAGE_BASE};
use crate::city_label::{self, CityFacts, ImprovementRules, LeaderRules, Rules};
use crate::{Civ3Error, layout as L};

/// Handle to the running game.
#[derive(Copy, Clone, Debug)]
pub struct Civ3 {
    pub build: Build,
    pub addrs: Addresses,
}

/// The main map view.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ViewState {
    /// `Main_Screen_Form.camera_x/y`.
    pub camera_x: i32,
    pub camera_y: i32,
    pub mouse_x: i32,
    pub mouse_y: i32,
    /// Visible tile range as the game computed it.
    pub tile_x: (i32, i32),
    pub tile_y: (i32, i32),
    /// Address of the selected unit (0 if none).
    pub current_unit: u32,
    pub player_civ: i32,
}

/// The main screen's targeting state (docs §10.15): `Mode_Action` (a
/// `Unit_Mode_Actions` value, or 0 / `IDC_ARROW` 0x7F00 for none) is the
/// command waiting for a target tile and the index of its pointer in
/// `Art/Cursor.pcx`; `Mode_Action_Range` its range in tiles (-1: any).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Targeting {
    pub mode: i32,
    /// The pointer shows "not here" (`field_4ED0`).
    pub refused: bool,
    pub range: i32,
    /// The tile under the mouse is a target the command takes, within
    /// range (`field_2E1AC`, set by 0x4DC360 as the mouse moves).
    pub target_ok: bool,
    /// The command was given (by key or button: `field_4E80[0]`, set by
    /// `Main_Screen_Form::issue_command`; dragging a unit with the right
    /// button leaves it off) and waits for its tile: the map shows the
    /// overlays only then (0x4EF187).
    pub armed: bool,
    /// Where a go-to path starts (`field_4E80[1..2]`, the unit's tile when
    /// the command was given).
    pub start: (i32, i32),
    /// The tile under the pointer (0x4E3D20), which the tile frame marks
    /// (the go-to modes keep theirs in `field_4E80[3..4]`).
    pub pointer_tile: Option<(i32, i32)>,
    /// The tiles the command reaches, which the map outlines (see
    /// [`RangeGrid`]), while it shows them.
    pub range_grid: Option<RangeGrid>,
}

/// The range of a targeting command the map's grid pass outlines tile by
/// tile (`Map_Renderer::m36`, 0x4C57C0): every tile at most `range` from
/// `centre` (`Map::get_distance`, 0x44A950) gets its diamond's four edges
/// in red (`Map_Renderer.field_2C8`, set to pure red at 0x5F4437). Set
/// with the command (0x4D8822: `b_Action_Mode`, `Action_Mode_*`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct RangeGrid {
    pub centre: (i32, i32),
    pub range: i32,
}

/// 0x4E3D20: the tile at pixel `p` of the main view (its camera plus the
/// pointer), tiles `w × h` = 128 × 64 (zoomed out 64 × 32), the map `size`
/// tiles big (in tile units) wrapping as `wrap` says; `None` off the map.
pub fn tile_under((px, py): (i32, i32), zoomed_out: bool, size: (i32, i32), wrap: (bool, bool)) -> Option<(i32, i32)> {
    let (w, h) = if zoomed_out { (64, 32) } else { (128, 64) };
    let cell = w * h / 2;
    let (a, b) = (px * (h / 2), py * (w / 2));
    let half = cell / 2;
    // (C division, rounding towards zero after the game's shift down.)
    let down = |v: i32| if v < 0 { v - cell } else { v } / cell;
    let p = down(half - b + a);
    let q = down(half + a + b);
    let axis = |v: i32, n: i32, wraps: bool| if wraps && v < 0 { v + n } else if wraps && v >= n { v - n } else { v };
    let (x, y) = (axis(q + p - 1, size.0, wrap.0), axis(q - p - 1, size.1, wrap.1));
    ((0..size.0).contains(&x) && (0..size.1).contains(&y)).then_some((x, y))
}

/// A city, as far as the map shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CityInfo {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub civ: i32,
    /// Population.
    pub size: i32,
    /// Bit i: has improvement i (the first 256).
    pub improvements: [u8; 32],
    /// What the city is building: `Order_Type` (`COT_IMPROVEMENT`: 1) and
    /// `Order_ID`.
    pub order: (i32, i32),
    /// What its map label shows (docs §10.8): the name, what it builds by
    /// name (an improvement or a unit type), the `Common_Policy` bits that
    /// outline its label, the food surplus, the turns until it grows and
    /// until it completes its order ([`crate::city_label::NEVER`]: never),
    /// and the icons of its HUD box.
    pub name: String,
    pub order_name: Option<String>,
    pub policy: i32,
    pub food_income: i32,
    pub grow_turns: i32,
    pub build_turns: i32,
    pub hud_icons: Vec<u8>,
    /// The harbour, airport and barracks icons it shows (`city_label::VETERAN_*`).
    pub veteran_icons: u8,
}

impl CityInfo {
    /// The improvement the city is building, if it is building one.
    pub fn improvement_in_production(&self) -> Option<i32> {
        (i64::from(self.order.0) == L::COT_IMPROVEMENT).then_some(self.order.1)
    }
}

/// What an improvement's rules say about whether a city counts as having
/// it (C3X's `has_active_building`, docs §18).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImprovementInfo {
    pub name: String,
    /// The government it needs (-1: any).
    pub government: i32,
    /// Bit per player that knows the tech obsoleting it.
    pub obsolete_for: u32,
    /// A wonder or a small wonder.
    pub wonder: bool,
}

/// A unit as the animator draws it (docs §10).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DrawnUnit {
    /// Opaque native object identity, compared only across copied snapshots.
    /// Raw memory reads never invoke the game to populate calculated stats.
    pub native_address: u32,
    /// Calculated only in the bridge's game-thread capture. Raw reads are None.
    pub max_move_points: Option<i32>,
    pub id: i32,
    /// Tile.
    pub x: i32,
    pub y: i32,
    pub civ: i32,
    pub unit_type: i32,
    pub leader_kind: i32,
    pub experience: i32,
    pub damage: i32,
    /// Move points used.
    pub moves: i32,
    pub container: i32,
    pub state: i32,
    pub always_on_top: bool,
    /// World pixel of the unit's anchor (the tile centre when idle).
    pub pixel: (i32, i32),
    /// Displayed facing (`enum direction`, 1 NE .. 8 N).
    pub direction: i32,
    /// `AnimationType` (1 DEFAULT ...).
    pub animation: i32,
    /// Frame within the direction's block.
    pub frame: i32,
    pub visible: bool,
    /// Armies (docs §10.11): the member shown (-1: none) and how many
    /// members there are.
    pub army_member: i32,
    pub army_size: i32,
    /// On its way between tiles.
    pub moving: bool,
}

const UNIT_RECORD_SIZE: usize = (L::UNIT_BODY_ANIMATION + L::FLC_ANIMATION_LAST + 4) as usize;

fn unit_record(bytes: &[u8; UNIT_RECORD_SIZE]) -> DrawnUnit {
    let field = |at: u32| i32::from_le_bytes(bytes[at as usize..at as usize + 4].try_into().unwrap());
    let anim = |at| field(L::UNIT_BODY_ANIMATION + at);
    DrawnUnit {
        native_address: 0,
        max_move_points: None,
        id: field(L::UNIT_BODY_ID),
        x: field(L::UNIT_BODY_X),
        y: field(L::UNIT_BODY_Y),
        civ: field(L::UNIT_BODY_CIVID),
        unit_type: field(L::UNIT_BODY_UNITTYPEID),
        leader_kind: field(L::UNIT_BODY_LEADER_KIND),
        experience: field(L::UNIT_BODY_COMBAT_EXPERIENCE),
        damage: field(L::UNIT_BODY_DAMAGE),
        moves: field(L::UNIT_BODY_MOVES),
        container: field(L::UNIT_BODY_CONTAINER_UNIT),
        state: field(L::UNIT_BODY_UNITSTATE),
        always_on_top: bytes[L::UNIT_BODY_ALWAYS_ON_TOP as usize] != 0,
        pixel: (anim(L::FLC_ANIMATION_SUMMARY_PIXEL_LOC_X), anim(L::FLC_ANIMATION_SUMMARY_PIXEL_LOC_Y)),
        direction: anim(L::FLC_ANIMATION_SUMMARY_DIRECTION_2),
        animation: anim(L::FLC_ANIMATION_SUMMARY_CURRENT_ANIM_TYPE),
        frame: anim(L::FLC_ANIMATION_FIELD_FC),
        visible: anim(L::FLC_ANIMATION_LAST) & 0xFF != 0,
        army_member: field(L::UNIT_BODY_FIELD_1B0 + 4),
        army_size: field(L::UNIT_BODY_FIELD_1B0 + 8),
        moving: bytes[(L::UNIT_BODY_ACTIVE + 1) as usize] != 0,
    }
}

/// Where an animation the animator draws besides units comes from (docs
/// §10.14).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EffectSource {
    /// A city's own (disorder, celebration): its `AnimatedEffect`.
    City { effect: i32 },
    /// The game's list of tile effects (hits and misses of bombardment and
    /// combat, smoke over a volcano, ...): its `AnimatedEffect`.
    Tile { effect: i32 },
    /// The animator's extra animations (a bomber's bombs, SAM and SDI shots).
    Extra,
}

/// An animation the animator draws besides units (docs §10.14): an
/// `FLC_Animation` of the art `ini` names, as it stands now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawnEffect {
    pub source: EffectSource,
    /// The tile it belongs to.
    pub tile: (i32, i32),
    /// World pixel of its anchor (full size).
    pub pixel: (i32, i32),
    /// Displayed facing, `AnimationType` and frame within the block, as a
    /// unit's (docs §10.4).
    pub direction: i32,
    pub animation: i32,
    pub frame: i32,
    /// Its art is loaded and it is not hidden (`field_111`, `Last`).
    pub shown: bool,
    /// The art's INI file (`Animation_Info.Data`), as the game resolved it
    /// (an absolute path); empty without art.
    pub ini: String,
}

/// An improvement that makes a town look walled (`Combat_Bombard > 0`,
/// docs §8): its id, the government it needs (-1: any), and which players
/// know the tech that obsoletes it (bit per player).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct WallBuilding {
    pub id: i32,
    pub government: i32,
    pub obsolete_for: u32,
}

/// An improvement that gives its city an icon on the map (docs §10.8):
/// its id, its `ITF_Veteran_*` flags, and which players know the tech that
/// obsoletes it (bit per player).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct IconBuilding {
    pub id: i32,
    pub flags: u32,
    pub obsolete_for: u32,
}

impl IconBuilding {
    /// Barracks (`ITF_Veteran_Ground_Units`).
    pub const GROUND: u32 = 0x2;
    /// Harbour (`ITF_Veteran_Sea_Units`).
    pub const SEA: u32 = 0x2_0000;
    /// Airport (`ITF_Veteran_Air_Units`).
    pub const AIR: u32 = 0x4_0000;
    pub const FLAGS: u32 = Self::GROUND | Self::SEA | Self::AIR;
}

/// Per player, the data city and unit art depends on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Players {
    pub eras: Vec<i32>,
    pub color_tables: Vec<i32>,
    pub capitals: Vec<i32>,
    pub races: Vec<i32>,
    /// Per race (not player): `Race.CultureGroupID`.
    pub culture_groups: Vec<i32>,
    /// `General.MaximumSize_Town` / `MaximumSize_City`.
    pub max_town: i32,
    pub max_city: i32,
    pub governments: Vec<i32>,
    pub wall_buildings: Vec<WallBuilding>,
    pub icon_buildings: Vec<IconBuilding>,
    /// Every player's adjective (`Leader_get_civ_adjective`).
    pub adjectives: Vec<String>,
}

/// Shape of the map.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MapInfo {
    /// In Civ III's x units: tiles exist where x + y is even, so a row holds
    /// `width / 2` tiles.
    pub width: i32,
    pub height: i32,
    pub tile_count: i32,
    pub wrap_x: bool,
    pub wrap_y: bool,
    /// `Map.Flags` (bit 0 wrap x, bit 1 wrap y, bit 2 polar ice caps).
    pub flags: u32,
    /// Seed of the per-tile art randomness (docs §9).
    pub seed: u32,
    tiles: Ptr32<Ptr32<TileRecord>>,
}

impl MapInfo {
    /// The map's tile-pointer array, used to distinguish loaded worlds
    /// with the same dimensions and art seed.
    pub fn tiles_address(&self) -> u32 { self.tiles.raw() }
}

/// A copy of one `Tile` struct, with typed accessors for the members listed
/// in `layout.fields`.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct TileRecord(pub [u8; L::TILE_SIZE as usize]);

impl std::fmt::Debug for TileRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TileRecord")
            .field("square_type", &self.square_type())
            .field("square_parts", &self.square_parts())
            .field("overlays", &format_args!("{:#x}", self.overlays()))
            .field("river_code", &self.river_code())
            .finish_non_exhaustive()
    }
}

impl TileRecord {
    fn u8(&self, at: u32) -> u8 {
        self.0[at as usize]
    }
    fn i16(&self, at: u32) -> i16 {
        i16::from_le_bytes(self.0[at as usize..at as usize + 2].try_into().unwrap())
    }
    fn u32(&self, at: u32) -> u32 {
        u32::from_le_bytes(self.0[at as usize..at as usize + 4].try_into().unwrap())
    }

    pub fn river_code(&self) -> u8 {
        self.u8(L::TILE_RIVER_CODE)
    }
    pub fn territory_owner(&self) -> i8 {
        self.u8(L::TILE_TERRITORY_OWNERID) as i8
    }
    pub fn resource_type(&self) -> i32 {
        self.u32(L::TILE_RESOURCETYPE) as i32
    }
    pub fn tile_unit_id(&self) -> i32 {
        self.u32(L::TILE_TILEUNITID) as i32
    }
    pub fn square_parts(&self) -> i16 {
        self.i16(L::TILE_SQUAREPARTS)
    }
    /// The header's `VictoryPoint`; stays 0 in scenarios with victory
    /// locations (measured), see `victory_location`.
    pub fn victory_point(&self) -> i16 {
        self.i16(L::TILE_VICTORYPOINT)
    }
    /// The upper half of the header's `field_20`: -1, or the victory
    /// location on this tile (measured: set exactly on the tiles the game
    /// draws `Victory_Image` on).
    pub fn victory_location(&self) -> i16 {
        self.i16(L::TILE_FIELD_20 + 2)
    }
    pub fn city_id(&self) -> i16 {
        self.i16(L::TILE_CITYID)
    }
    /// The city whose area the tile belongs to (-1: none; C3X's tile
    /// improvements count only that city's buildings).
    pub fn city_area(&self) -> i16 {
        self.i16(L::TILE_BODY + L::TILE_BODY_CITYAREAID)
    }
    pub fn continent(&self) -> i16 {
        self.i16(L::TILE_CONTINENTID)
    }
    /// -1, or the tile building (colony, airfield, radar tower, outpost)
    /// on this tile.
    pub fn building_id(&self) -> i16 {
        self.i16(L::TILE_TILE_BUILDINGID)
    }
    pub fn ruins(&self) -> bool {
        self.u32(L::TILE_RUINS) != 0
    }
    pub fn overlays(&self) -> u32 {
        self.u32(L::TILE_OVERLAYS)
    }
    pub fn square_type(&self) -> u32 {
        self.u32(L::TILE_SQUARETYPE)
    }
    pub fn terrain_overlays(&self) -> u32 {
        self.u32(L::TILE_TERRAIN_OVERLAYS)
    }
    pub fn fog_of_war(&self) -> u32 {
        self.u32(L::TILE_BODY + L::TILE_BODY_FOG_OF_WAR)
    }
    pub fn fow_status(&self) -> u32 {
        self.u32(L::TILE_BODY + L::TILE_BODY_FOWSTATUS)
    }
    pub fn visibility(&self) -> u32 {
        self.u32(L::TILE_BODY + L::TILE_BODY_VISIBILITY)
    }
    /// Bit c: explored by civ c.
    pub fn explored(&self) -> u32 {
        self.u32(L::TILE_BODY + L::TILE_BODY_FOG_OF_WAR)
    }
    pub fn v3(&self) -> u32 {
        self.u32(L::TILE_BODY + L::TILE_BODY_V3)
    }
    pub fn visibility_d0(&self) -> u32 {
        self.u32(L::TILE_BODY + L::TILE_BODY_FIELD_D0_VISIBILITY)
    }
    /// Per civ: the overlays (improvements) that civ remembers here.
    pub fn visible_overlays(&self) -> [u8; 32] {
        let at = (L::TILE_BODY + L::TILE_BODY_VISIBILE_OVERLAYS) as usize;
        self.0[at..at + 32].try_into().unwrap()
    }
    /// Raw bytes at `offset`, for fields not (yet) given a name.
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Civ3 {
    /// Identifies the running build. Call from inside the game process.
    pub fn attach() -> Result<Civ3, Civ3Error> {
        let build = Build::detect(IMAGE_BASE)?;
        Ok(Civ3 { build, addrs: build.addresses() })
    }

    fn screen<T: Pod>(&self, offset: u32) -> Result<T, Civ3Error> {
        Ok(Ptr32::<u8>::new(self.addrs.main_screen_form).offset::<T>(offset)?.read()?)
    }

    pub fn view(&self) -> Result<ViewState, Civ3Error> {
        Ok(ViewState {
            camera_x: self.screen(L::MAIN_SCREEN_FORM_CAMERA_X)?,
            camera_y: self.screen(L::MAIN_SCREEN_FORM_CAMERA_Y)?,
            mouse_x: self.screen(L::MAIN_SCREEN_FORM_MOUSE_X)?,
            mouse_y: self.screen(L::MAIN_SCREEN_FORM_MOUSE_Y)?,
            tile_x: (self.screen(L::MAIN_SCREEN_FORM_TILEX_MIN)?, self.screen(L::MAIN_SCREEN_FORM_TILEX_MAX)?),
            tile_y: (self.screen(L::MAIN_SCREEN_FORM_TILEY_MIN)?, self.screen(L::MAIN_SCREEN_FORM_TILEY_MAX)?),
            current_unit: self.screen(L::MAIN_SCREEN_FORM_CURRENT_UNIT)?,
            player_civ: self.screen(L::MAIN_SCREEN_FORM_PLAYER_CIVID)?,
        })
    }

    fn map_field<T: Pod>(&self, offset: u32) -> Result<T, Civ3Error> {
        Ok(Ptr32::<u8>::new(self.addrs.bic_data).offset::<T>(L::BIC_MAP + offset)?.read()?)
    }

    pub fn map(&self) -> Result<MapInfo, Civ3Error> {
        let flags: u32 = self.map_field(L::MAP_FLAGS)?;
        let info = MapInfo {
            width: self.map_field(L::MAP_WIDTH)?,
            height: self.map_field(L::MAP_HEIGHT)?,
            tile_count: self.map_field(L::MAP_TILECOUNT)?,
            wrap_x: flags & 1 != 0,
            wrap_y: flags & 2 != 0,
            flags,
            seed: self.map_field(L::MAP_SEED)?,
            tiles: self.map_field(L::MAP_TILES)?,
        };
        if info.tiles.is_null() || info.width <= 0 || info.height <= 0 {
            return Err(Civ3Error::NoGame);
        }
        Ok(info)
    }

    /// The `Tile` pointers, indexed like [`Civ3::tiles`] (to tell which tile
    /// a pointer kept elsewhere means).
    pub fn tile_pointers(&self, map: &MapInfo) -> Result<Vec<u32>, Civ3Error> {
        Ok(map.tiles.read_array(map.tile_count.max(0) as usize)?.into_iter().map(|p| p.raw()).collect())
    }

    /// All tiles, indexed like the game: tile (x, y) is at `(y·width + x) / 2`.
    pub fn tiles(&self, map: &MapInfo) -> Result<Vec<TileRecord>, Civ3Error> {
        let pointers = map.tiles.read_array(map.tile_count.max(0) as usize)?;
        pointers.into_iter().map(|p| Ok(p.read()?)).collect()
    }
}

impl Civ3 {
    fn bic<T: Pod>(&self, offset: u32) -> Result<T, Civ3Error> {
        Ok(Ptr32::<u8>::new(self.addrs.bic_data).offset::<T>(offset)?.read()?)
    }

    /// The folder the running scenario's art overrides live in (searched
    /// before the install's), if any: a C string at BIC+0x128 (docs §11,
    /// `BIC_get_asset_path`; inside the header's unnamed `field_1C`).
    pub fn scenario_folder(&self) -> Result<Option<String>, Civ3Error> {
        let bytes: [u8; 260] = self.bic(0x128)?;
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let path = String::from_utf8_lossy(&bytes[..len]).trim().to_owned();
        Ok((!path.is_empty()).then_some(path))
    }

    /// The `;`-separated art search roots tried after the scenario folder
    /// (docs §11: a string at BIC+0xC7C, filled at runtime; holds the
    /// scenario's search folders, e.g. `Conquests\WWII in the Pacific`).
    pub fn search_roots(&self) -> Result<Vec<String>, Civ3Error> {
        let bytes: [u8; 1024] = self.bic(0xC7C)?;
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Ok(String::from_utf8_lossy(&bytes[..len]).split(';').map(|r| r.trim().to_owned()).filter(|r| !r.is_empty()).collect())
    }

    /// Icon index (into resources.pcx) of every resource type.
    pub fn resource_icons(&self) -> Result<Vec<i32>, Civ3Error> {
        let count: i32 = self.bic(L::BIC_RESOURCETYPECOUNT)?;
        let base: Ptr32<u8> = self.bic(L::BIC_RESOURCETYPES)?;
        (0..count.clamp(0, 1024) as u32)
            .map(|i| Ok(base.offset::<i32>(i * L::RESOURCE_TYPE_SIZE + L::RESOURCE_TYPE_ICONID)?.read()?))
            .collect()
    }

    /// The players that know `tech`, a bit each (`Leader_has_tech` reads
    /// one of these); `None` if the tech table of this build is not known.
    pub fn tech_known_by(&self, tech: i32) -> Option<u32> {
        if tech < 0 {
            return Some(u32::MAX);
        }
        let (table, never) = crate::hooks::tech_table(self.build)?;
        if never.read().ok()? == tech {
            return Some(0);
        }
        table.read().ok()?.index(tech as u32).ok()?.read().ok()
    }

    /// Whether `civ` knows `tech` (`Leader_has_tech`); `None` if the tech
    /// table of this build is not known.
    pub fn has_tech(&self, civ: i32, tech: i32) -> Option<bool> {
        Some(self.tech_known_by(tech)? & (1u32 << (civ & 31)) != 0)
    }

    /// Per resource type: is its icon shown to `civ` (the prerequisite tech
    /// is known), as `Tile_get_resource_visible_to` decides.
    pub fn resources_visible_to(&self, civ: i32) -> Result<Vec<bool>, Civ3Error> {
        let count: i32 = self.bic(L::BIC_RESOURCETYPECOUNT)?;
        let base: Ptr32<u8> = self.bic(L::BIC_RESOURCETYPES)?;
        (0..count.clamp(0, 1024) as u32)
            .map(|i| {
                let req: i32 = base.offset::<i32>(i * L::RESOURCE_TYPE_SIZE + L::RESOURCE_TYPE_REQUIREID)?.read()?;
                Ok(self.has_tech(civ, req).unwrap_or(true))
            })
            .collect()
    }

    fn leader_field(&self, offset: u32) -> Result<Vec<i32>, Civ3Error> {
        (0..32u32).map(|civ| Ok(Ptr32::<i32>::new(self.addrs.leaders + civ * L::LEADER_SIZE + offset).read()?)).collect()
    }

    /// How `civ` stands with every other civ (indexed by civ).
    pub fn diplomacy(&self, civ: i32) -> Result<Diplomacy, Civ3Error> {
        let base = self.addrs.leaders + (civ.clamp(0, 31) as u32) * L::LEADER_SIZE;
        let words = |offset: u32| -> Result<[i32; 32], Civ3Error> {
            let v = Ptr32::<i32>::new(base + offset).read_array(32)?;
            Ok(<[i32; 32]>::try_from(v).unwrap_or([0; 32]))
        };
        let at_war = Ptr32::<u8>::new(base + L::LEADER_AT_WAR).read_array(32)?;
        Ok(Diplomacy {
            at_war: std::array::from_fn(|i| at_war.get(i).is_some_and(|&b| b != 0)),
            contacts: words(L::LEADER_CONTACTS)?,
            treaties: words(L::LEADER_RELATION_TREATIES)?,
            military_allies: words(L::LEADER_MILITARY_ALLIES)?,
        })
    }

    /// Every player's era (0 ancient .. 3 modern).
    pub fn eras(&self) -> Result<Vec<i32>, Civ3Error> {
        self.leader_field(L::LEADER_ERA)
    }

    /// Every player's colour table (`Art/Units/Palettes/ntpNN.pcx`).
    pub fn color_tables(&self) -> Result<Vec<i32>, Civ3Error> {
        self.leader_field(L::LEADER_COLOR_TABLE_ID)
    }

    pub fn players(&self) -> Result<Players, Civ3Error> {
        let races: Ptr32<u8> = self.bic(L::BIC_RACES)?;
        let race_count: i32 = self.bic(L::BIC_RACESCOUNT)?;
        let culture_groups = (0..race_count.clamp(0, 256) as u32)
            .map(|r| Ok(races.offset::<i32>(r * L::RACE_SIZE + L::RACE_CULTUREGROUPID)?.read()?))
            .collect::<Result<_, Civ3Error>>()?;
        let (wall_buildings, icon_buildings) = self.marked_buildings()?;
        Ok(Players {
            eras: self.eras()?,
            color_tables: self.color_tables()?,
            capitals: self.leader_field(L::LEADER_CAPITALID)?,
            races: self.leader_field(L::LEADER_RACEID)?,
            culture_groups,
            max_town: self.bic(L::BIC_GENERAL + L::GENERAL_MAXIMUMSIZE_TOWN)?,
            max_city: self.bic(L::BIC_GENERAL + L::GENERAL_MAXIMUMSIZE_CITY)?,
            governments: self.leader_field(L::LEADER_GOVERNMENTTYPE)?,
            wall_buildings,
            icon_buildings,
            adjectives: self.civ_adjectives().unwrap_or_default(),
        })
    }

    /// Every player's adjective (`Leader_get_civ_adjective`, 0x55A1C0): the
    /// one the player gave their tribe, else their race's.
    pub fn civ_adjectives(&self) -> Result<Vec<String>, Civ3Error> {
        let races: Ptr32<u8> = self.bic(L::BIC_RACES)?;
        let race_count: i32 = self.bic(L::BIC_RACESCOUNT)?;
        // Windows-1252, read as Latin-1.
        let text = |bytes: &[u8]| -> String { bytes.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect() };
        (0..32u32)
            .map(|civ| {
                let leader = Ptr32::<u8>::new(self.addrs.leaders + civ * L::LEADER_SIZE);
                let custom = leader.offset::<u8>(L::LEADER_TRIBE_CUSTOMIZATION_CIV_ADJECTIVE)?.read_array(16)?;
                if custom.first().is_some_and(|&b| b != 0) {
                    return Ok(text(&custom));
                }
                let race: i32 = leader.offset::<i32>(L::LEADER_RACEID)?.read()?;
                if !(0..race_count).contains(&race) {
                    return Ok(String::new());
                }
                Ok(text(&races.offset::<u8>(race as u32 * L::RACE_SIZE + L::RACE_ADJECTIVENAME)?.read_array(40)?))
            })
            .collect()
    }

    /// The improvements that change how a city looks: walls (docs §8) and
    /// those with a city icon (§10.8).
    fn marked_buildings(&self) -> Result<(Vec<WallBuilding>, Vec<IconBuilding>), Civ3Error> {
        let base: Ptr32<u8> = self.bic(L::BIC_IMPROVEMENTS)?;
        let count: i32 = self.bic(L::BIC_IMPROVEMENTSCOUNT)?;
        let (mut walls, mut icons) = (Vec::new(), Vec::new());
        for id in 0..count.clamp(0, 256) as u32 {
            let field = |offset| -> Result<i32, Civ3Error> { Ok(base.offset::<i32>(id * L::IMPROVEMENT_SIZE + offset)?.read()?) };
            let wall = field(L::IMPROVEMENT_COMBAT_BOMBARD)? > 0;
            let flags = field(L::IMPROVEMENT_IMPROVEMENTFLAGS)? as u32 & IconBuilding::FLAGS;
            if !wall && flags == 0 {
                continue;
            }
            let obsolete = field(L::IMPROVEMENT_OBSOLETEID)?;
            let obsolete_for = (0..32).filter(|&civ| obsolete >= 0 && self.has_tech(civ, obsolete) == Some(true)).fold(0, |m, civ| m | 1 << civ);
            if wall {
                walls.push(WallBuilding { id: id as i32, government: field(L::IMPROVEMENT_GOVERNMENTID)?, obsolete_for });
            }
            if flags != 0 {
                icons.push(IconBuilding { id: id as i32, flags, obsolete_for });
            }
        }
        Ok((walls, icons))
    }

    /// The keys of the free buildings wonders give `civ`
    /// (`Leader.Auto_Improvements`): an improvement id for a building in all
    /// its cities, `(continent + 1) · ImprovementsCount + id` for one in its
    /// cities on that continent (docs §8, §18). The table is open addressed
    /// items of (key, value); an unused one has the key -1.
    pub fn free_improvements(&self, civ: i32) -> Result<Vec<i32>, Civ3Error> {
        let table = Ptr32::<u8>::new(self.addrs.leaders + civ.clamp(0, 31) as u32 * L::LEADER_SIZE + L::LEADER_AUTO_IMPROVEMENTS);
        let items: Ptr32<[i32; 2]> = table.offset::<Ptr32<[i32; 2]>>(L::HASH_TABLE_ITEMS)?.read()?;
        let capacity: i32 = table.offset::<i32>(L::HASH_TABLE_CAPACITY)?.read()?;
        if items.is_null() || capacity <= 0 {
            return Ok(Vec::new());
        }
        let items = items.read_array(capacity.min(1 << 16) as usize)?;
        Ok(items.into_iter().map(|[key, _]| key).filter(|&key| key >= 0).collect())
    }

    /// Every improvement's rules for being counted as present (see
    /// [`ImprovementInfo`]), by improvement id. One read for all of them.
    pub fn improvements(&self) -> Result<Vec<ImprovementInfo>, Civ3Error> {
        let base: Ptr32<u8> = self.bic(L::BIC_IMPROVEMENTS)?;
        let count = self.bic::<i32>(L::BIC_IMPROVEMENTSCOUNT)?.clamp(0, 1024) as usize;
        let size = L::IMPROVEMENT_SIZE as usize;
        let bytes = base.read_array(count * size)?;
        Ok(bytes
            .chunks_exact(size)
            .map(|record| {
                let field = |offset: u32| i32::from_le_bytes(record[offset as usize..offset as usize + 4].try_into().unwrap());
                let name = &record[L::IMPROVEMENT_NAME as usize..][..32];
                let obsolete = field(L::IMPROVEMENT_OBSOLETEID);
                ImprovementInfo {
                    name: String::from_utf8_lossy(&name[..name.iter().position(|&b| b == 0).unwrap_or(32)]).into_owned(),
                    government: field(L::IMPROVEMENT_GOVERNMENTID),
                    obsolete_for: if obsolete >= 0 { self.tech_known_by(obsolete).unwrap_or(0) } else { 0 },
                    wonder: i64::from(field(L::IMPROVEMENT_CHARACTERISTICS)) & (L::ITC_WONDER | L::ITC_SMALL_WONDER) != 0,
                }
            })
            .collect())
    }

    /// The units the animator draws this frame, in drawing order.
    pub fn drawn_units(&self) -> Result<Vec<DrawnUnit>, Civ3Error> {
        let animator = Ptr32::<u8>::new(self.addrs.main_screen_form + L::MAIN_SCREEN_FORM_ANIMATOR);
        let count: i32 = animator.offset::<i32>(L::ANIMATOR_UNITS_COUNT)?.read()?;
        let mut out = Vec::new();
        for i in 0..count.clamp(0, 1024) as u32 {
            let unit: Ptr32<u8> = animator.offset::<Ptr32<u8>>(L::ANIMATOR_UNITS + 4 * i)?.read()?;
            if unit.is_null() {
                continue;
            }
            out.push(Self::read_unit(unit.offset::<u8>(L::UNIT_BODY)?)?);
        }
        Ok(out)
    }

    /// `Unit *` for an id, for engine functions that take an excluded unit.
    /// Unit list items contain `Unit_Body *`, so undo its header offset.
    pub fn unit_ptr(&self, id: i32) -> Result<Option<u32>, Civ3Error> {
        let list = Ptr32::<u8>::new(self.addrs.units);
        let items: Ptr32<u8> = list.offset::<Ptr32<u8>>(L::CITIES_CITIES)?.read()?;
        let last: i32 = list.offset::<i32>(L::CITIES_LASTINDEX)?.read()?;
        if items.is_null() || id < 0 || id > last { return Ok(None); }
        let body: Ptr32<u8> = items.offset::<Ptr32<u8>>(id as u32 * L::CITYITEM_SIZE + L::CITYITEM_CITY)?.read()?;
        Ok(body.raw().checked_sub(L::UNIT_BODY).filter(|_| !body.is_null()))
    }

    /// The unit with `id` (e.g. the member an army shows, docs §10.11).
    pub fn unit(&self, id: i32) -> Result<Option<DrawnUnit>, Civ3Error> {
        let list = Ptr32::<u8>::new(self.addrs.units);
        let items: Ptr32<u8> = list.offset::<Ptr32<u8>>(L::CITIES_CITIES)?.read()?;
        let last: i32 = list.offset::<i32>(L::CITIES_LASTINDEX)?.read()?;
        if items.is_null() || id < 0 || id > last {
            return Ok(None);
        }
        let body: Ptr32<u8> = items.offset::<Ptr32<u8>>(id as u32 * L::CITYITEM_SIZE + L::CITYITEM_CITY)?.read()?;
        if body.is_null() {
            return Ok(None);
        }
        Ok(Some(Self::read_unit(body)?))
    }

    /// A unit's current animation and map data from `Unit *`, as returned
    /// by `Main_Screen_Form::find_visible_unit` on the game's thread.
    pub fn drawn_unit_at(&self, unit: u32) -> Result<DrawnUnit, Civ3Error> {
        Self::read_unit(Ptr32::<u8>::new(unit).offset::<u8>(L::UNIT_BODY)?)
    }

    fn read_unit(body: Ptr32<u8>) -> Result<DrawnUnit, Civ3Error> {
        // One checked copy keeps the animation fields together and avoids
        // a process-memory call for every scalar of every off-camera unit.
        let bytes: [u8; UNIT_RECORD_SIZE] = body.cast().read()?;
        let mut record = unit_record(&bytes);
        record.native_address = body.raw().checked_sub(L::UNIT_BODY).unwrap_or(0);
        Ok(record)
    }

    fn strings(&self, list: u32, count: u32, stride: u32, member: u32, len: usize) -> Result<Vec<String>, Civ3Error> {
        let base: Ptr32<u8> = self.bic(list)?;
        let n: i32 = self.bic(count)?;
        (0..n.clamp(0, 4096) as u32)
            .map(|i| {
                let bytes = base.offset::<u8>(i * stride + member)?.read_array(len)?;
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(len);
                Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
            })
            .collect()
    }

    /// Every unit type's numbers the map shows: attack, defence, movement
    /// (whole tiles), hit point bonus, class, worker actions, abilities (bit
    /// `n`: ability `n`, `UnitTypeAbilities`).
    pub fn unit_type_stats(&self) -> Result<Vec<[i32; 7]>, Civ3Error> {
        let base: Ptr32<u8> = self.bic(L::BIC_UNITTYPES)?;
        let n: i32 = self.bic(L::BIC_UNITTYPECOUNT)?;
        (0..n.clamp(0, 4096) as u32)
            .map(|i| {
                let f = |at| -> Result<i32, Civ3Error> { Ok(base.offset::<i32>(i * L::UNITTYPE_SIZE + at)?.read()?) };
                Ok([
                    f(L::UNITTYPE_ATTACK)?,
                    f(L::UNITTYPE_DEFENCE)?,
                    f(L::UNITTYPE_MOVEMENT)?,
                    f(L::UNITTYPE_HIT_POINT_BONUS)?,
                    f(L::UNITTYPE_UNIT_CLASS)?,
                    f(L::UNITTYPE_WORKER_ACTIONS)?,
                    f(L::UNITTYPE_UNITABILITIES)?,
                ])
            })
            .collect()
    }

    /// Base hit points of each experience level.
    pub fn experience_hit_points(&self) -> Result<Vec<i32>, Civ3Error> {
        let base: Ptr32<u8> = self.bic(L::BIC_COMBATEXPERIENCE)?;
        let n: i32 = self.bic(L::BIC_COMBATEXPERIENCECOUNT)?;
        (0..n.clamp(0, 64) as u32)
            .map(|i| Ok(base.offset::<i32>(i * L::COMBAT_EXPERIENCE_SIZE + L::COMBAT_EXPERIENCE_BASE_HIT_POINTS)?.read()?))
            .collect()
    }

    /// Tile and container of every unit (for counting stacks).
    pub fn unit_tiles(&self) -> Result<Vec<(i32, i32, i32)>, Civ3Error> {
        let list = Ptr32::<u8>::new(self.addrs.units);
        let items: Ptr32<[u32; 2]> = list.offset::<Ptr32<[u32; 2]>>(L::CITIES_CITIES)?.read()?;
        let last: i32 = list.offset::<i32>(L::CITIES_LASTINDEX)?.read()?;
        let mut out = Vec::new();
        if items.is_null() { return Ok(out); }
        // UnitItem = { index, Unit_Body * }. Copy its array once, then the
        // short prefix containing position and container once per unit.
        for [_, body] in items.read_array(last.clamp(-1, 0xFFFF).saturating_add(1) as usize)? {
            if body == 0 { continue; }
            let bytes: [u8; (L::UNIT_BODY_CONTAINER_UNIT + 4) as usize] = Ptr32::new(body).read()?;
            let f = |at: u32| i32::from_le_bytes(bytes[at as usize..at as usize + 4].try_into().unwrap());
            out.push((f(L::UNIT_BODY_X), f(L::UNIT_BODY_Y), f(L::UNIT_BODY_CONTAINER_UNIT)));
        }
        Ok(out)
    }

    /// The units inside the units `containers` (an army's members, docs
    /// §10.11: their `Container_Unit` is the army).
    pub fn units_in(&self, containers: &[i32]) -> Result<Vec<DrawnUnit>, Civ3Error> {
        let list = Ptr32::<u8>::new(self.addrs.units);
        let items: Ptr32<[u32; 2]> = list.offset::<Ptr32<[u32; 2]>>(L::CITIES_CITIES)?.read()?;
        let last: i32 = list.offset::<i32>(L::CITIES_LASTINDEX)?.read()?;
        let mut out = Vec::new();
        if items.is_null() || containers.is_empty() { return Ok(out); }
        for [_, body] in items.read_array(last.clamp(-1, 0xFFFF).saturating_add(1) as usize)? {
            if body == 0 { continue; }
            let container: i32 = Ptr32::<u8>::new(body).offset::<i32>(L::UNIT_BODY_CONTAINER_UNIT)?.read()?;
            if container >= 0 && containers.contains(&container) {
                out.push(Self::read_unit(Ptr32::new(body))?);
            }
        }
        Ok(out)
    }

    /// The selected unit's id and the selection cursor's frame, unless the
    /// cursor is in the hidden half of its blink (docs §10.6).
    /// The selected unit (`Current_Unit`), blinking or not.
    pub fn current_unit(&self) -> Result<Option<i32>, Civ3Error> {
        let current: Ptr32<u8> = self.screen(L::MAIN_SCREEN_FORM_CURRENT_UNIT)?;
        if current.is_null() {
            return Ok(None);
        }
        Ok(Some(current.offset::<i32>(L::UNIT_BODY + L::UNIT_BODY_ID)?.read()?))
    }

    pub fn selection(&self) -> Result<Option<(i32, i32)>, Civ3Error> {
        let current: Ptr32<u8> = self.screen(L::MAIN_SCREEN_FORM_CURRENT_UNIT)?;
        if current.is_null() {
            return Ok(None);
        }
        let animator = Ptr32::<u8>::new(self.addrs.main_screen_form + L::MAIN_SCREEN_FORM_ANIMATOR);
        // The header's `field_18E4[12]`: bit 0 = cursor blinked off.
        let blink: u32 = animator.offset::<u32>(L::ANIMATOR_FIELD_18E4_12)?.read()?;
        if blink & 1 != 0 {
            return Ok(None);
        }
        let id: i32 = current.offset::<i32>(L::UNIT_BODY + L::UNIT_BODY_ID)?.read()?;
        let frame: i32 = animator.offset::<i32>(L::ANIMATOR_UNIT_CURSOR_ANIMATION + L::FLC_ANIMATION_FIELD_FC)?.read()?;
        Ok(Some((id, frame)))
    }

    /// The animations the animator draws besides units (docs §10.14), in
    /// the order its frame collects them (`0x4EFC30`): the cities' own, the
    /// tile effects in list order, the extra animations. Which of them the
    /// game shows (where the player sees their tile, not while the
    /// animation is DEFAULT) is for the caller to decide.
    pub fn effects(&self) -> Result<Vec<DrawnEffect>, Civ3Error> {
        let mut out = Vec::new();
        // The cities' own.
        let list = Ptr32::<u8>::new(self.addrs.cities);
        let items: Ptr32<u8> = list.offset::<Ptr32<u8>>(L::CITIES_CITIES)?.read()?;
        let last: i32 = list.offset::<i32>(L::CITIES_LASTINDEX)?.read()?;
        if !items.is_null() {
            for i in 0..last.clamp(-1, 0xFFFF).saturating_add(1) as u32 {
                let body: Ptr32<u8> = items.offset::<Ptr32<u8>>(i * L::CITYITEM_SIZE + L::CITYITEM_CITY)?.read()?;
                if body.is_null() {
                    continue;
                }
                let effect: i32 = body.offset::<i32>(L::CITY_BODY_FIELD_A4)?.read()?;
                if effect > 0 {
                    out.push(Self::read_effect(body.offset::<u8>(L::CITY_BODY_STRUCT_198)?, EffectSource::City { effect })?);
                }
            }
        }
        // The tile effects: a linked list (`next` is `field_194[1]`).
        let mut fx: Ptr32<u8> = Ptr32::<Ptr32<u8>>::new(self.addrs.tile_effects).read()?;
        for _ in 0..4096 {
            if fx.is_null() {
                break;
            }
            let effect: i32 = fx.offset::<i32>(L::TILE_ANIMATED_EFFECT_V + 8)?.read()?;
            out.push(Self::read_effect(fx.offset::<u8>(L::TILE_ANIMATED_EFFECT_FLC_ANIMATION)?, EffectSource::Tile { effect })?);
            fx = fx.offset::<Ptr32<u8>>(L::TILE_ANIMATED_EFFECT_FIELD_194 + 4)?.read()?;
        }
        // The extra animations: a vector of `FLC_Animation *` (begin, end).
        let animator = Ptr32::<u8>::new(self.addrs.main_screen_form + L::MAIN_SCREEN_FORM_ANIMATOR);
        let begin: u32 = animator.offset::<u32>(L::ANIMATOR_FIELD_18E4_3)?.read()?;
        let end: u32 = animator.offset::<u32>(L::ANIMATOR_FIELD_18E4_4)?.read()?;
        if begin != 0 && end > begin {
            for i in 0..((end - begin) / 4).min(256) {
                let anim: Ptr32<u8> = Ptr32::<Ptr32<u8>>::new(begin + 4 * i).read()?;
                if !anim.is_null() {
                    out.push(Self::read_effect(anim, EffectSource::Extra)?);
                }
            }
        }
        Ok(out)
    }

    fn read_effect(anim: Ptr32<u8>, source: EffectSource) -> Result<DrawnEffect, Civ3Error> {
        let i32_at = |at: u32| -> Result<i32, Civ3Error> { Ok(anim.offset::<i32>(at)?.read()?) };
        let info: u32 = anim.offset::<u32>(L::FLC_ANIMATION_ANIMATION_INFO)?.read()?;
        let ini = if info == 0 { String::new() } else { crate::c3x::c_string(Ptr32::<u32>::new(info + L::ANIMATION_INFO_DATA).read()?) };
        let loaded = anim.offset::<u8>(L::FLC_ANIMATION_FIELD_111)?.read()? != 0;
        Ok(DrawnEffect {
            source,
            tile: (i32_at(L::FLC_ANIMATION_SUMMARY_TILE_X)?, i32_at(L::FLC_ANIMATION_SUMMARY_TILE_Y)?),
            pixel: (i32_at(L::FLC_ANIMATION_SUMMARY_PIXEL_LOC_X)?, i32_at(L::FLC_ANIMATION_SUMMARY_PIXEL_LOC_Y)?),
            direction: i32_at(L::FLC_ANIMATION_SUMMARY_DIRECTION_2)?,
            animation: i32_at(L::FLC_ANIMATION_SUMMARY_CURRENT_ANIM_TYPE)?,
            frame: i32_at(L::FLC_ANIMATION_FIELD_FC)?,
            // The header's `Last` member of `FLC_Animation` (as a unit's).
            shown: loaded && i32_at(L::FLC_ANIMATION_LAST)? & 0xFF != 0,
            ini,
        })
    }

    /// The `P_*` preference flags.
    pub fn preferences(&self) -> Result<u32, Civ3Error> {
        Ok(Ptr32::<u32>::new(self.addrs.preferences).read()?)
    }

    /// `p_debug_mode_bits`.
    pub fn debug_mode_bits(&self) -> Result<u32, Civ3Error> {
        Ok(Ptr32::<u32>::new(self.addrs.debug_mode_bits).read()?)
    }

    /// Every unit type's Civilopedia key (`PRTO_...`), by type id.
    pub fn unit_type_keys(&self) -> Result<Vec<String>, Civ3Error> {
        self.strings(L::BIC_UNITTYPES, L::BIC_UNITTYPECOUNT, L::UNITTYPE_SIZE, L::UNITTYPE_CIVILIPEDIA_ENTRY, 32)
    }

    /// Every era's Civilopedia key (`ERAS_...`).
    pub fn era_keys(&self) -> Result<Vec<String>, Civ3Error> {
        self.strings(L::BIC_ERAS, L::BIC_ERASCOUNT, L::ERA_TYPE_SIZE, L::ERA_TYPE_CIV_ENTRY, 32)
    }

    /// All cities, with what their labels show (docs §10.8). The numbers
    /// come from the game's own rules ([`crate::city_label`]), read from its
    /// tables; if those cannot be read (no game yet) the labels' numbers are
    /// the game's "never".
    pub fn cities(&self) -> Result<Vec<CityInfo>, Civ3Error> {
        let list = Ptr32::<u8>::new(self.addrs.cities);
        let items: Ptr32<u8> = list.offset::<Ptr32<u8>>(L::CITIES_CITIES)?.read()?;
        let last: i32 = list.offset::<i32>(L::CITIES_LASTINDEX)?.read()?;
        let mut out = Vec::new();
        let mut facts = Vec::new();
        if items.is_null() {
            return Ok(out);
        }
        let rules = self.label_rules().inspect_err(|e| log::debug!("label rules: {e}")).ok();
        // LastIndex is -1 for an empty list.
        for i in 0..last.clamp(-1, 0xFFFF).saturating_add(1) as u32 {
            // `CityItem.City` points at the city's body (as the header types
            // it), not at the `City` around it.
            let body: Ptr32<u8> = items.offset::<Ptr32<u8>>(i * L::CITYITEM_SIZE + L::CITYITEM_CITY)?.read()?;
            if body.is_null() {
                continue;
            }
            let i32_at = |at: u32| -> Result<i32, Civ3Error> { Ok(body.offset::<i32>(at)?.read()?) };
            let (x, y): (i32, i32) = (body.offset::<i16>(L::CITY_BODY_X)?.read()?.into(), body.offset::<i16>(L::CITY_BODY_Y)?.read()?.into());
            let name = body.offset::<u8>(L::CITY_BODY_CITYNAME)?.read_array(20)?;
            let fact = CityFacts {
                id: i32_at(L::CITY_BODY_ID)?,
                civ: body.offset::<i8>(L::CITY_BODY_CIVID)?.read()?.into(),
                size: i32_at(L::CITY_BODY_POPULATION + L::POPULATION_SIZE)?,
                improvements: body.offset::<[u8; 32]>(L::CITY_BODY_IMPROVEMENTS_1 + L::CITY_IMPROVEMENTS_IMPROVEMENTS)?.read()?,
                order: (i32_at(L::CITY_BODY_ORDER_TYPE)?, i32_at(L::CITY_BODY_ORDER_ID)?),
                stored_food: i32_at(L::CITY_BODY_STOREDFOOD)?,
                food_income: i32_at(L::CITY_BODY_FOODINCOME)?,
                stored_production: i32_at(L::CITY_BODY_STOREDPRODUCTION)?,
                production_income: i32_at(L::CITY_BODY_PRODUCTIONINCOME)?,
                specialist_stock: i32_at(L::CITY_BODY_STORED_PRODUCTION_FROM_SPECIALISTS)?,
                specialist_shields: rules.as_ref().map_or(0, |r| self.specialist_shields(body, r)),
                on_river: self.tile_record(x, y).is_ok_and(|t| t.river_code() != 0),
            };
            out.push(CityInfo {
                id: fact.id,
                x,
                y,
                civ: fact.civ,
                size: fact.size,
                improvements: fact.improvements,
                order: fact.order,
                // Windows-1252, read as Latin-1 (the glyph set covers the same codes).
                name: name.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect(),
                order_name: None,
                policy: i32_at(L::CITY_BODY_COMMON_POLICY)?,
                food_income: fact.food_income,
                grow_turns: city_label::NEVER,
                build_turns: city_label::NEVER,
                hud_icons: Vec::new(),
                veteran_icons: 0,
            });
            facts.push(fact);
        }
        if let Some(rules) = &rules {
            let owner: std::collections::HashMap<i32, i32> = facts.iter().map(|f| (f.id, f.civ)).collect();
            let owner = |city: i32| owner.get(&city).copied();
            // The free buildings of each player, looked up when the game asks for them.
            let free_keys = std::cell::RefCell::new(std::collections::HashMap::<i32, std::rc::Rc<HashSet<i32>>>::new());
            let free_of = |civ: i32| {
                free_keys
                    .borrow_mut()
                    .entry(civ)
                    .or_insert_with(|| std::rc::Rc::new(self.free_improvements(civ).unwrap_or_default().into_iter().collect()))
                    .clone()
            };
            for (city, fact) in out.iter_mut().zip(&facts) {
                let (x, y) = (city.x, city.y);
                let continent = std::cell::OnceCell::new();
                let has = |id: i32| {
                    let continent = *continent.get_or_init(|| self.tile_record(x, y).map_or(-1, |t| t.continent().into()));
                    rules.has_improvement(fact, &free_of(fact.civ), continent, id)
                };
                city.grow_turns = rules.turns_to_grow(fact, &has);
                city.build_turns = rules.turns_to_build(fact);
                city.order_name = rules.order_name(fact.order);
                city.hud_icons = rules.hud_icons(fact, &owner);
                city.veteran_icons = rules.veteran_icons(fact, &has);
            }
        }
        Ok(out)
    }

    /// One tile's record, by its coordinates.
    pub fn tile_record(&self, x: i32, y: i32) -> Result<TileRecord, Civ3Error> {
        let map = self.map()?;
        if x < 0 || y < 0 || x >= map.width || y >= map.height || (x + y) % 2 != 0 {
            return Err(Civ3Error::NoGame);
        }
        let tile: Ptr32<TileRecord> = map.tiles.index(map.tile_index(x, y) as u32)?.read()?;
        Ok(tile.read()?)
    }

    /// The shields a city's specialists add: the `Construction` of each citizen
    /// of status 0 (`City_get_turns_to_build`, 0x4BFDD0, and C3X's guard).
    fn specialist_shields(&self, body: Ptr32<u8>, rules: &Rules) -> i32 {
        let read = || -> Result<i32, Civ3Error> {
            let citizens = body.offset::<u8>(L::CITY_BODY_CITIZENS)?;
            let items: Ptr32<u8> = citizens.offset::<Ptr32<u8>>(L::CITIZENS_ITEMS)?.read()?;
            let last: i32 = citizens.offset::<i32>(L::CITIZENS_LASTINDEX)?.read()?;
            let mut sum = 0;
            if items.is_null() {
                return Ok(0);
            }
            for i in 0..last.clamp(-1, 0xFFFF).saturating_add(1) as u32 {
                let citizen: Ptr32<u8> = items.offset::<Ptr32<u8>>(i * L::CITIZEN_INFO_SIZE + L::CITIZEN_INFO_BODY)?.read()?;
                if citizen.is_null() || citizen.offset::<u8>(L::CITIZEN_BODY_FIELD_20)?.read()? != 0 {
                    continue;
                }
                let kind: i32 = citizen.offset::<i32>(L::CITIZEN_BODY_WORKERTYPE)?.read()?;
                sum += usize::try_from(kind).ok().and_then(|k| rules.citizen_construction.get(k)).copied().unwrap_or(0);
            }
            Ok(sum)
        };
        read().unwrap_or(0)
    }

    /// What the labels' numbers need from the game's tables and players,
    /// read once for all cities (docs §10.8).
    fn label_rules(&self) -> Result<Rules, Civ3Error> {
        let a = &self.addrs;
        let viewer: i32 = self.screen(L::MAIN_SCREEN_FORM_PLAYER_CIVID)?;
        let table = |count: u32, list: u32| -> Result<(Ptr32<u8>, usize), Civ3Error> {
            Ok((self.bic(list)?, self.bic::<i32>(count)?.clamp(0, 4096) as usize))
        };
        let improvements = {
            let (base, count) = table(L::BIC_IMPROVEMENTSCOUNT, L::BIC_IMPROVEMENTS)?;
            let size = L::IMPROVEMENT_SIZE as usize;
            let bytes = base.read_array(count * size)?;
            bytes
                .chunks_exact(size)
                .map(|r| {
                    let word = |at: u32| i32::from_le_bytes(r[at as usize..at as usize + 4].try_into().unwrap());
                    let name = &r[L::IMPROVEMENT_NAME as usize..][..32];
                    ImprovementRules {
                        name: name.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect(),
                        cost: word(L::IMPROVEMENT_COST),
                        flags: word(L::IMPROVEMENT_IMPROVEMENTFLAGS) as u32,
                        characteristics: word(L::IMPROVEMENT_CHARACTERISTICS) as u32,
                        small_wonder_flags: word(L::IMPROVEMENT_SMALLWONDERFLAGS) as u32,
                        government: word(L::IMPROVEMENT_GOVERNMENTID),
                        obsolete: word(L::IMPROVEMENT_OBSOLETEID),
                    }
                })
                .collect::<Vec<_>>()
        };
        let (unit_names, unit_costs) = {
            let (base, count) = table(L::BIC_UNITTYPECOUNT, L::BIC_UNITTYPES)?;
            let size = L::UNITTYPE_SIZE as usize;
            let bytes = base.read_array(count * size)?;
            bytes
                .chunks_exact(size)
                .map(|r| {
                    let name = &r[L::UNITTYPE_NAME as usize..][..32];
                    let cost = i32::from_le_bytes(r[L::UNITTYPE_COST as usize..][..4].try_into().unwrap());
                    (name.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect::<String>(), cost)
                })
                .unzip::<String, i32, Vec<_>, Vec<_>>()
        };
        let citizen_construction = {
            let (base, count) = table(L::BIC_CITIZENTYPECOUNT, L::BIC_CITIZENTYPES)?;
            (0..count as u32)
                .map(|i| Ok(base.offset::<i32>(i * L::CITIZEN_TYPE_SIZE + L::CITIZEN_TYPE_CONSTRUCTION)?.read()?))
                .collect::<Result<Vec<i32>, Civ3Error>>()?
        };
        let (tech_flags, tech_known_by) = {
            let (base, count) = table(L::BIC_ADVANCECOUNT, L::BIC_ADVANCES)?;
            let flags = (0..count as u32)
                .map(|i| Ok(base.offset::<u32>(i * L::ADVANCE_SIZE + L::ADVANCE_FLAGS)?.read()?))
                .collect::<Result<Vec<u32>, Civ3Error>>()?;
            let known = match crate::hooks::tech_table(self.build) {
                Some((known, _)) => known.read()?.read_array(count)?,
                None => vec![0; count],
            };
            (flags, known)
        };
        let difficulty_cost_factor = {
            let (base, count) = table(L::BIC_DIFFICULTYLEVELCOUNT, L::BIC_DIFFICULTYLEVELS)?;
            let level = Ptr32::<i32>::new(a.game_difficulty).read()?;
            if (0..count as i32).contains(&level) {
                base.offset::<i32>(level as u32 * L::DIFFICULTY_LEVEL_SIZE + L::DIFFICULTY_LEVEL_COST_FACTOR)?.read()?
            } else {
                10
            }
        };
        let optimal_cities = {
            let size: i32 = self.map_field(L::MAP_WORLD_WORLD_SIZE)?;
            let sizes: Ptr32<u8> = self.bic(L::BIC_WORLDSIZES)?;
            sizes.offset::<i32>(size.clamp(0, 16) as u32 * L::WORLD_SIZE_SIZE + L::WORLD_SIZE_OPTIMALCITYCOUNT)?.read()?
        };
        let races: Ptr32<u8> = self.bic(L::BIC_RACES)?;
        let leaders = (0..32u32)
            .map(|civ| {
                let base = Ptr32::<u8>::new(a.leaders + civ * L::LEADER_SIZE);
                let word = |at: u32| -> Result<i32, Civ3Error> { Ok(base.offset::<i32>(at)?.read()?) };
                let race = word(L::LEADER_RACEID)?;
                let race_bonuses = if (0..self.bic::<i32>(L::BIC_RACESCOUNT)?).contains(&race) {
                    races.offset::<u32>(race as u32 * L::RACE_SIZE + L::RACE_BONUSES)?.read()?
                } else {
                    0
                };
                let wonders: Ptr32<i32> = base.offset::<Ptr32<i32>>(L::LEADER_SMALL_WONDERS)?.read()?;
                Ok(LeaderRules {
                    government: word(L::LEADER_GOVERNMENTTYPE)?,
                    capital: word(L::LEADER_CAPITALID)?,
                    cities: word(L::LEADER_CITIES_COUNT)?,
                    race_bonuses,
                    small_wonders: if wonders.is_null() { Vec::new() } else { wonders.read_array(improvements.len())? },
                })
            })
            .collect::<Result<Vec<_>, Civ3Error>>()?;
        let wonder_cities: Ptr32<i32> = Ptr32::<Ptr32<i32>>::new(a.game + 0x4F8).read()?;
        let viewer_flags = |at: u32| -> Result<[bool; 32], Civ3Error> {
            let bytes = Ptr32::<u8>::new(a.leaders + viewer.clamp(0, 31) as u32 * L::LEADER_SIZE + at).read_array(32)?;
            Ok(std::array::from_fn(|i| bytes[i] != 0))
        };
        Ok(Rules {
            viewer,
            human_players: Ptr32::<u32>::new(a.human_player_bits).read()?,
            difficulty_cost_factor,
            accelerated_production: Ptr32::<u32>::new(a.toggleable_rules).read()? & L::TR_ACCELERATED_PRODUCTION as u32 != 0,
            max_town: self.bic(L::BIC_GENERAL + L::GENERAL_MAXIMUMSIZE_TOWN)?,
            max_city: self.bic(L::BIC_GENERAL + L::GENERAL_MAXIMUMSIZE_CITY)?,
            wonder_cities: if wonder_cities.is_null() { Vec::new() } else { wonder_cities.read_array(improvements.len())? },
            improvements,
            unit_names,
            unit_costs,
            citizen_construction,
            tech_flags,
            tech_known_by,
            optimal_cities,
            anarchy: self.bic(L::BIC_TRANSITION_GOVERNMENT)?,
            leaders,
            viewer_war: viewer_flags(L::LEADER_AT_WAR)?,
            viewer_d50: viewer_flags(L::LEADER_FIELD_D50)?,
            viewer_d70: viewer_flags(L::LEADER_FIELD_D70)?,
        })
    }

    /// Every outpost: (tile building id = list index, owner).
    pub fn outposts(&self) -> Result<Vec<(i32, i32)>, Civ3Error> {
        let list = Ptr32::<u8>::new(self.addrs.outposts);
        let items: Ptr32<u8> = list.offset::<Ptr32<u8>>(L::CITIES_CITIES)?.read()?;
        let last: i32 = list.offset::<i32>(L::CITIES_LASTINDEX)?.read()?;
        let mut out = Vec::new();
        if items.is_null() {
            return Ok(out);
        }
        for i in 0..last.clamp(-1, 0xFFFF).saturating_add(1) as u32 {
            let body: Ptr32<u8> = items.offset::<Ptr32<u8>>(i * L::CITYITEM_SIZE + L::CITYITEM_CITY)?.read()?;
            if body.is_null() {
                continue;
            }
            // The game reads the owner as a byte.
            out.push((i as i32, body.offset::<i8>(L::TILE_BUILDING_BODY_OWNERID)?.read()?.into()));
        }
        Ok(out)
    }

    /// The city the city screen shows (its id), while it is open: the map
    /// is then drawn around it with its own fog rule (docs §16.8).
    pub fn focus_city(&self) -> Result<Option<i32>, Civ3Error> {
        let Some(slot) = crate::ui_hooks::focus_city_slot(self.build) else { return Ok(None) };
        let city: Ptr32<u8> = Ptr32::<Ptr32<u8>>::new(slot).read()?;
        if city.is_null() {
            return Ok(None);
        }
        Ok(Some(city.offset::<i32>(L::CITY_BODY + L::CITY_BODY_ID)?.read()?))
    }

    /// The tile under the mouse (`field_4E80[3..4]`, docs §10.9), unless the
    /// mouse is off the map.
    pub fn mouse_tile(&self) -> Result<Option<(i32, i32)>, Civ3Error> {
        let x: i32 = self.screen(L::MAIN_SCREEN_FORM_FIELD_4E80 + 12)?;
        let y: i32 = self.screen(L::MAIN_SCREEN_FORM_FIELD_4E80 + 16)?;
        Ok((x >= 0 && y >= 0).then_some((x, y)))
    }

    /// The unit command waiting for a target tile, and what the game made
    /// of the tile under the mouse (docs §10.15).
    pub fn targeting(&self) -> Result<Targeting, Civ3Error> {
        let f = L::MAIN_SCREEN_FORM_FIELD_4E80;
        Ok(Targeting {
            mode: self.screen(L::MAIN_SCREEN_FORM_MODE_ACTION)?,
            refused: self.screen::<u8>(L::MAIN_SCREEN_FORM_FIELD_4ED0)? != 0,
            range: self.screen(L::MAIN_SCREEN_FORM_MODE_ACTION_RANGE)?,
            target_ok: self.screen::<u8>(L::MAIN_SCREEN_FORM_FIELD_2E1AC)? != 0,
            armed: self.screen::<u8>(f)? != 0,
            start: (self.screen(f + 4)?, self.screen(f + 8)?),
            pointer_tile: self.tile_under_mouse()?,
            range_grid: self.range_grid()?,
        })
    }

    /// The range the map outlines now, if any (see [`RangeGrid`]).
    pub fn range_grid(&self) -> Result<Option<RangeGrid>, Civ3Error> {
        let r = |f: u32| self.map_field::<i32>(L::MAP_RENDERER + f);
        if r(L::MAP_RENDERER_B_ACTION_MODE)? == 0 {
            return Ok(None);
        }
        Ok(Some(RangeGrid {
            centre: (r(L::MAP_RENDERER_ACTION_MODE_CENTERX)?, r(L::MAP_RENDERER_ACTION_MODE_CENTERY)?),
            range: r(L::MAP_RENDERER_ACTION_MODE_RANGE)?,
        }))
    }

    /// The tile under the game's pointer (`mouse_x/y`), as
    /// `Main_Screen_Form::get_tile_coords_under_mouse` (0x4E3D20) finds it;
    /// `None` off the map.
    pub fn tile_under_mouse(&self) -> Result<Option<(i32, i32)>, Civ3Error> {
        if let Some(tile) = crate::world_input::virtual_pointer_for(self.addrs.main_screen_form) {
            return Ok(tile);
        }
        let view = self.view()?;
        let map = self.map()?;
        Ok(tile_under(
            (view.camera_x + view.mouse_x, view.camera_y + view.mouse_y),
            self.zoomed_out()?,
            (map.width, map.height),
            (map.wrap_x, map.wrap_y),
        ))
    }

    pub fn zoomed_out(&self) -> Result<bool, Civ3Error> {
        Ok(self.bic::<u8>(L::BIC_IS_ZOOMED_OUT)? != 0)
    }
}

impl MapInfo {
    pub fn tile_index(&self, x: i32, y: i32) -> usize {
        ((y * self.width + x) / 2) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pointer_finds_the_diamond_it_is_in() {
        // Tile (x, y)'s centre is at (64x + 64, 32y + 32); zoomed out half.
        for (x, y) in [(81, 51), (79, 51), (80, 50), (0, 0), (1, 1)] {
            let c = (64 * x + 64, 32 * y + 32);
            assert_eq!(tile_under(c, false, (200, 200), (false, false)), Some((x, y)));
            assert_eq!(tile_under((c.0 / 2, c.1 / 2), true, (200, 200), (false, false)), Some((x, y)));
            // Just inside the diamond's corners.
            for (dx, dy) in [(-62, 0), (62, 0), (0, -30), (0, 30)] {
                assert_eq!(tile_under((c.0 + dx, c.1 + dy), false, (200, 200), (false, false)), Some((x, y)), "{:?}", (x, y, dx, dy));
            }
        }
        // Off the map's edge, unless it wraps.
        assert_eq!(tile_under((2, 60), false, (200, 200), (false, false)), None);
        assert_eq!(tile_under((2, 60), false, (200, 200), (true, false)), Some((199, 1)));
        assert_eq!(tile_under((2, 2), false, (200, 200), (true, false)), None);
    }

    #[test]
    fn unit_record_keeps_animation_flags_positions_and_army_members() {
        let mut bytes = [0u8; UNIT_RECORD_SIZE];
        let mut set = |at: u32, value: i32| bytes[at as usize..at as usize + 4].copy_from_slice(&value.to_le_bytes());
        for (at, value) in [
            (L::UNIT_BODY_ID, 71), (L::UNIT_BODY_X, 80), (L::UNIT_BODY_Y, 40),
            (L::UNIT_BODY_LEADER_KIND, 2), (L::UNIT_BODY_CIVID, 2), (L::UNIT_BODY_UNITTYPEID, 9), (L::UNIT_BODY_COMBAT_EXPERIENCE, 3),
            (L::UNIT_BODY_DAMAGE, 1), (L::UNIT_BODY_MOVES, 6), (L::UNIT_BODY_CONTAINER_UNIT, -1),
            (L::UNIT_BODY_UNITSTATE, 1), (L::UNIT_BODY_FIELD_1B0 + 4, 72), (L::UNIT_BODY_FIELD_1B0 + 8, 3),
            (L::UNIT_BODY_ANIMATION + L::FLC_ANIMATION_SUMMARY_PIXEL_LOC_X, 5177),
            (L::UNIT_BODY_ANIMATION + L::FLC_ANIMATION_SUMMARY_PIXEL_LOC_Y, -24),
            (L::UNIT_BODY_ANIMATION + L::FLC_ANIMATION_SUMMARY_DIRECTION_2, 8),
            (L::UNIT_BODY_ANIMATION + L::FLC_ANIMATION_SUMMARY_CURRENT_ANIM_TYPE, 2),
            (L::UNIT_BODY_ANIMATION + L::FLC_ANIMATION_FIELD_FC, 17),
            (L::UNIT_BODY_ANIMATION + L::FLC_ANIMATION_LAST, 0x10001),
        ] { set(at, value); }
        bytes[L::UNIT_BODY_ALWAYS_ON_TOP as usize] = 1;
        bytes[(L::UNIT_BODY_ACTIVE + 1) as usize] = 1;
        assert_eq!(unit_record(&bytes), DrawnUnit {
            native_address: 0, max_move_points: None, id: 71, x: 80, y: 40, civ: 2, unit_type: 9, leader_kind: 2, experience: 3, damage: 1, moves: 6,
            container: -1, state: 1, always_on_top: true, pixel: (5177,-24), direction: 8,
            animation: 2, frame: 17, visible: true, army_member: 72, army_size: 3, moving: true,
        });
        bytes[(L::UNIT_BODY_ANIMATION + L::FLC_ANIMATION_LAST) as usize] = 0;
        bytes[(L::UNIT_BODY_ACTIVE + 1) as usize] = 0;
        bytes[L::UNIT_BODY_ACTIVE as usize] = 1;
        let hidden = unit_record(&bytes);
        assert!(!hidden.visible, "Last's low byte alone controls visibility");
        assert!(!hidden.moving, "Active's high byte alone controls movement");
        assert_eq!(size_of::<[u32; 2]>(), L::CITYITEM_SIZE as usize);
    }

    #[test]
    fn tile_record_mirrors_the_game_struct_size() {
        assert_eq!(size_of::<TileRecord>(), 220);
    }
}

/// One civ's relations to every other civ (`Leader` fields, indexed by civ).
/// Of `treaties` only bit 1 (right of passage) is known from C3X; the other
/// bits are read as [`Diplomacy::relation`] says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diplomacy {
    pub at_war: [bool; 32],
    pub contacts: [i32; 32],
    pub treaties: [i32; 32],
    pub military_allies: [i32; 32],
}

/// How two civs stand, for drawing.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Relation {
    War,
    /// A mutual protection pact (or a military alliance).
    Defense,
    Peace,
    /// Not met.
    None,
}

/// `treaties` bits, measured against the foreign advisor: 2 right of
/// passage (also tested by C3X), 4 mutual protection pact (the allies of a
/// scenario have it). `contacts` bit 1: in contact.
pub const CONTACT: i32 = 1;
pub const TREATY_RIGHT_OF_PASSAGE: i32 = 2;
pub const TREATY_MUTUAL_PROTECTION: i32 = 4;

impl Diplomacy {
    pub fn relation(&self, other: i32) -> Relation {
        let i = other.clamp(0, 31) as usize;
        if self.at_war[i] {
            Relation::War
        } else if self.treaties[i] & TREATY_MUTUAL_PROTECTION != 0 || self.military_allies[i] != 0 {
            Relation::Defense
        } else if self.contacts[i] & CONTACT != 0 {
            // The game calls every civ it is not at war with and knows "at
            // peace" (foreign and military advisors).
            Relation::Peace
        } else {
            Relation::None
        }
    }
}
