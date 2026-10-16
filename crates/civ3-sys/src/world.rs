//! Reading the game's world state: map, tiles, view.

use bytemuck::{Pod, Zeroable};
use inproc::Ptr32;

use crate::build::{Addresses, Build, IMAGE_BASE};
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

/// A city, as far as the map shows it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CityInfo {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub civ: i32,
    /// Population.
    pub size: i32,
    /// Bit i: has improvement i (the first 256).
    pub improvements: [u8; 32],
}

/// A unit as the animator draws it (docs §10).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DrawnUnit {
    pub id: i32,
    /// Tile.
    pub x: i32,
    pub y: i32,
    pub civ: i32,
    pub unit_type: i32,
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

    /// Whether `civ` knows `tech` (`Leader_has_tech`); `None` if the tech
    /// table of this build is not known.
    pub fn has_tech(&self, civ: i32, tech: i32) -> Option<bool> {
        if tech < 0 {
            return Some(true);
        }
        let (table, never) = crate::hooks::tech_table(self.build)?;
        if never.read().ok()? == tech {
            return Some(false);
        }
        let bits = table.read().ok()?.index(tech as u32).ok()?.read().ok()?;
        Some(bits & (1u32 << (civ & 31)) != 0)
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
        })
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

    fn read_unit(body: Ptr32<u8>) -> Result<DrawnUnit, Civ3Error> {
        let anim = body.offset::<u8>(L::UNIT_BODY_ANIMATION)?;
        let i32_at = |p: Ptr32<u8>, at: u32| -> Result<i32, Civ3Error> { Ok(p.offset::<i32>(at)?.read()?) };
        Ok(DrawnUnit {
            id: i32_at(body, L::UNIT_BODY_ID)?,
            x: i32_at(body, L::UNIT_BODY_X)?,
            y: i32_at(body, L::UNIT_BODY_Y)?,
            civ: i32_at(body, L::UNIT_BODY_CIVID)?,
            unit_type: i32_at(body, L::UNIT_BODY_UNITTYPEID)?,
            experience: i32_at(body, L::UNIT_BODY_COMBAT_EXPERIENCE)?,
            damage: i32_at(body, L::UNIT_BODY_DAMAGE)?,
            moves: i32_at(body, L::UNIT_BODY_MOVES)?,
            container: i32_at(body, L::UNIT_BODY_CONTAINER_UNIT)?,
            state: i32_at(body, L::UNIT_BODY_UNITSTATE)?,
            always_on_top: body.offset::<u8>(L::UNIT_BODY_ALWAYS_ON_TOP)?.read()? != 0,
            pixel: (i32_at(anim, L::FLC_ANIMATION_SUMMARY_PIXEL_LOC_X)?, i32_at(anim, L::FLC_ANIMATION_SUMMARY_PIXEL_LOC_Y)?),
            direction: i32_at(anim, L::FLC_ANIMATION_SUMMARY_DIRECTION_2)?,
            animation: i32_at(anim, L::FLC_ANIMATION_SUMMARY_CURRENT_ANIM_TYPE)?,
            frame: i32_at(anim, L::FLC_ANIMATION_FIELD_FC)?,
            // The header's `Last` member of `FLC_Animation`.
            visible: i32_at(anim, L::FLC_ANIMATION_LAST)? & 0xFF != 0,
            // `field_1B0[1]` / `[2]`.
            army_member: i32_at(body, L::UNIT_BODY_FIELD_1B0 + 4)?,
            army_size: i32_at(body, L::UNIT_BODY_FIELD_1B0 + 8)?,
            // The high byte of the header's `Active`.
            moving: body.offset::<u8>(L::UNIT_BODY_ACTIVE + 1)?.read()? != 0,
        })
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
    /// (whole tiles), hit point bonus, class, worker actions.
    pub fn unit_type_stats(&self) -> Result<Vec<[i32; 6]>, Civ3Error> {
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
        let items: Ptr32<u8> = list.offset::<Ptr32<u8>>(L::CITIES_CITIES)?.read()?;
        let last: i32 = list.offset::<i32>(L::CITIES_LASTINDEX)?.read()?;
        let mut out = Vec::new();
        if items.is_null() {
            return Ok(out);
        }
        // `Units` has the same shape as `Cities`; items point at the body.
        // LastIndex is -1 for an empty list.
        for i in 0..last.clamp(-1, 0xFFFF).saturating_add(1) as u32 {
            let body: Ptr32<u8> = items.offset::<Ptr32<u8>>(i * L::CITYITEM_SIZE + L::CITYITEM_CITY)?.read()?;
            if body.is_null() {
                continue;
            }
            let f = |at| -> Result<i32, Civ3Error> { Ok(body.offset::<i32>(at)?.read()?) };
            out.push((f(L::UNIT_BODY_X)?, f(L::UNIT_BODY_Y)?, f(L::UNIT_BODY_CONTAINER_UNIT)?));
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

    /// The `P_*` preference flags.
    pub fn preferences(&self) -> Result<u32, Civ3Error> {
        Ok(Ptr32::<u32>::new(self.addrs.preferences).read()?)
    }

    /// Every unit type's Civilopedia key (`PRTO_...`), by type id.
    pub fn unit_type_keys(&self) -> Result<Vec<String>, Civ3Error> {
        self.strings(L::BIC_UNITTYPES, L::BIC_UNITTYPECOUNT, L::UNITTYPE_SIZE, L::UNITTYPE_CIVILIPEDIA_ENTRY, 32)
    }

    /// Every era's Civilopedia key (`ERAS_...`).
    pub fn era_keys(&self) -> Result<Vec<String>, Civ3Error> {
        self.strings(L::BIC_ERAS, L::BIC_ERASCOUNT, L::ERA_TYPE_SIZE, L::ERA_TYPE_CIV_ENTRY, 32)
    }

    /// All cities.
    pub fn cities(&self) -> Result<Vec<CityInfo>, Civ3Error> {
        let list = Ptr32::<u8>::new(self.addrs.cities);
        let items: Ptr32<u8> = list.offset::<Ptr32<u8>>(L::CITIES_CITIES)?.read()?;
        let last: i32 = list.offset::<i32>(L::CITIES_LASTINDEX)?.read()?;
        let mut out = Vec::new();
        if items.is_null() {
            return Ok(out);
        }
        // LastIndex is -1 for an empty list.
        for i in 0..last.clamp(-1, 0xFFFF).saturating_add(1) as u32 {
            // `CityItem.City` points at the city's body (as the header types
            // it), not at the `City` around it.
            let body: Ptr32<u8> = items.offset::<Ptr32<u8>>(i * L::CITYITEM_SIZE + L::CITYITEM_CITY)?.read()?;
            if body.is_null() {
                continue;
            }
            out.push(CityInfo {
                id: body.offset::<i32>(L::CITY_BODY_ID)?.read()?,
                x: body.offset::<i16>(L::CITY_BODY_X)?.read()?.into(),
                y: body.offset::<i16>(L::CITY_BODY_Y)?.read()?.into(),
                civ: body.offset::<i8>(L::CITY_BODY_CIVID)?.read()?.into(),
                size: body.offset::<i32>(L::CITY_BODY_POPULATION + L::POPULATION_SIZE)?.read()?,
                improvements: body.offset::<[u8; 32]>(L::CITY_BODY_IMPROVEMENTS_1 + L::CITY_IMPROVEMENTS_IMPROVEMENTS)?.read()?,
            });
        }
        Ok(out)
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
