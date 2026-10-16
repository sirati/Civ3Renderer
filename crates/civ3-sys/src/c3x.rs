//! C3X's own state: the districts it adds to the map (C3X R26 and later).
//!
//! C3X keeps its state (`struct injected_state`) at the start of the PE
//! section `.c3xdat` of the exe it modded; its layout comes from C3X's
//! header (`c3x_layout`, generated) and is valid for that release only, so
//! the state is used only if its version and size match. Districts are
//! recorded only in C3X's hash table `district_tile_map` (tile pointer to
//! `district_instance`).

use inproc::Ptr32;

use crate::Civ3Error;
use crate::c3x_layout as X;

/// A district on a tile.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct District {
    pub x: i32,
    pub y: i32,
    /// Index into [`DistrictConfigs::districts`].
    pub kind: i32,
    /// Completed (`DS_COMPLETED`), else under construction.
    pub completed: bool,
    pub built_by: i32,
    /// Wonder districts: the wonder's state (0 none, 1 being built, 2
    /// completed, 3 ruined), the wonder (index into
    /// [`DistrictConfigs::wonders`]) and the city building it.
    pub wonder_state: i32,
    pub wonder: i32,
    pub wonder_city: i32,
    /// Natural wonders: index into [`DistrictConfigs::natural_wonders`].
    pub natural_wonder: i32,
}

/// How a kind of district is drawn (`struct district_config` and the ids
/// C3X resolved for it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DistrictKind {
    pub name: String,
    /// Sheets under `Art/Districts/<season>/<time>/`: one, or one per
    /// culture group.
    pub images: Vec<String>,
    pub columns: i32,
    pub by_era: bool,
    pub by_culture: bool,
    /// `DRS_BY_COUNT` or `DRS_BY_BUILDING`.
    pub render_strategy: i32,
    /// 0 district, 1 tile improvement.
    pub kind: i32,
    pub align_to_coast: bool,
    pub draw_over_resources: bool,
    pub subsumes_tile_resource: bool,
    /// The district can be an irrigation source: it hides its tile's own
    /// irrigation.
    pub allow_irrigation_from: bool,
    /// Cell size (0: 128×64) and drawing offset.
    pub size: (i32, i32),
    pub offset: (i32, i32),
    /// The resource the district generates (-1: none) and its `MF_*`
    /// flags (`MF_NO_TECH_REQ` ignores the resource's tech).
    pub generated_resource: i32,
    pub generated_resource_flags: i32,
    /// The resource the district's own tile must hold (-1: none), and how
    /// many resources it needs a connected city to have.
    pub resource_prereq_on_tile: i32,
    pub resource_prereqs: i32,
    /// Improvements whose presence picks the sheet's column.
    pub dependent_buildings: Vec<i32>,
}

impl DistrictKind {
    /// A tile improvement (`type = tile-improvement`) counts the buildings
    /// of the one city working the tile, not those of the cities around it.
    pub fn is_tile_improvement(&self) -> bool {
        i64::from(self.kind) == X::DT_TILE_IMPROVEMENT
    }

    /// Cell size of the district's sheet: the custom size, else 128×64.
    pub fn cell_size(&self) -> (i32, i32) {
        (if self.size.0 > 0 { self.size.0 } else { 128 }, if self.size.1 > 0 { self.size.1 } else { 64 })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WonderKind {
    pub name: String,
    pub image: String,
    pub cell: (i32, i32),
    pub construct_cell: (i32, i32),
    /// The alternative direction's cells (`enable_img_alt_dir`).
    pub alt: Option<((i32, i32), (i32, i32))>,
    pub size: (i32, i32),
    /// `buildable_square_types_mask` (bit `SQ_*`; `SQ_RIVER` is bit 14) and
    /// `buildable_on_rivers`.
    pub buildable_square_types: u32,
    pub buildable_on_rivers: bool,
    /// The wonder's improvement (the first improvement of that name), once
    /// [`DistrictConfigs::resolve_wonders`] has seen the improvements.
    pub improvement: Option<i32>,
}

impl WonderKind {
    /// Cell size of the wonder's art: the custom size, else 128×64.
    pub fn cell_size(&self) -> (i32, i32) {
        (if self.size.0 > 0 { self.size.0 } else { 128 }, if self.size.1 > 0 { self.size.1 } else { 64 })
    }

    /// `wonder_requires_river`: the wonder may go on rivers, and then faces
    /// along them.
    pub fn requires_river(&self) -> bool {
        self.buildable_square_types & (1 << X::SQ_RIVER) != 0 || self.buildable_on_rivers
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NaturalWonderKind {
    pub name: String,
    pub image: String,
    pub cell: (i32, i32),
}

/// Which kinds of district C3X draws (`current_config.enable_*`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct DistrictFlags {
    pub districts: bool,
    pub natural_wonders: bool,
    pub neighborhood: bool,
    pub wonder: bool,
    pub distribution_hub: bool,
    pub energy_grid: bool,
    pub bridge: bool,
    pub canal: bool,
    pub great_wall: bool,
    /// `draw_forests_over_roads_and_railroads`: forests drawn again after a
    /// tile's road and railroad.
    pub forests_over_roads: bool,
}

impl DistrictFlags {
    /// Whether C3X changes the game's tile drawing at all.
    pub fn any(&self) -> bool {
        self.districts || self.natural_wonders || self.forests_over_roads
    }
}

/// The district kinds C3X loaded (from its config files).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DistrictConfigs {
    pub flags: DistrictFlags,
    pub districts: Vec<DistrictKind>,
    pub wonders: Vec<WonderKind>,
    pub natural_wonders: Vec<NaturalWonderKind>,
    /// Tiles in a city's work area, centre included (`workable_tile_count`;
    /// 0: not known, the game's default radius of 2 is 21).
    pub workable_tiles: i32,
    /// The mod's folder relative to `Conquests` (`mod_rel_dir`): the
    /// districts' art is looked up there after the scenario's.
    pub mod_dir: String,
}

impl DistrictConfigs {
    /// The offsets of a city's work area, nearest first (C3X's
    /// `patch_ni_to_diff_for_work_area`).
    pub fn work_area(&self) -> &'static [(i8, i8)] {
        let n = if self.workable_tiles > 0 { self.workable_tiles as usize } else { 21 };
        &X::WORK_AREA_OFFSETS[..n.min(X::WORK_AREA_OFFSETS.len())]
    }

    /// Finds each wonder's improvement among the improvements' names
    /// (`find_game_object_id_by_name`: exact match, the first one).
    pub fn resolve_wonders(&mut self, improvement_names: &[String]) {
        for w in &mut self.wonders {
            w.improvement = improvement_names.iter().position(|n| *n == w.name).map(|i| i as i32);
        }
    }
}

/// C3X's state in the running game.
#[derive(Copy, Clone, Debug)]
pub struct C3xState {
    base: Ptr32<u8>,
}

/// Section `name` of the module at `base`: absolute address and virtual size.
pub fn section(base: u32, name: &[u8; 8]) -> Result<Option<(u32, u32)>, Civ3Error> {
    let image = Ptr32::<u8>::new(base);
    let e_lfanew: u32 = image.offset::<u32>(0x3C)?.read()?;
    let nt = image.offset::<u8>(e_lfanew)?;
    let sections: u16 = nt.offset::<u16>(6)?.read()?;
    let optional_size: u16 = nt.offset::<u16>(20)?.read()?;
    let table = nt.offset::<[u8; 40]>(24 + optional_size as u32)?;
    for i in 0..sections as u32 {
        let header = table.index(i)?.read()?;
        if &header[..8] == name {
            let word = |at: usize| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
            return Ok(Some((base + word(12), word(8))));
        }
    }
    Ok(None)
}

/// A NUL-terminated string at `p` (at most 260 bytes), lossy.
fn c_string(p: u32) -> String {
    if p == 0 {
        return String::new();
    }
    let mut bytes = Vec::new();
    for i in 0..260 {
        match Ptr32::<u8>::new(p + i).read() {
            Ok(0) | Err(_) => break,
            Ok(b) => bytes.push(b),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

impl C3xState {
    /// C3X's state in the exe at `image_base`, if it is the C3X release
    /// `c3x_layout` was generated from.
    pub fn find(image_base: u32) -> Result<Option<C3xState>, Civ3Error> {
        let Some((va, size)) = section(image_base, b".c3xdat\0")? else { return Ok(None) };
        let base = Ptr32::<u8>::new(va);
        let version: i32 = base.offset::<i32>(X::STATE_MOD_VERSION)?.read()?;
        let fits = size >= X::STATE_SIZE && size < X::STATE_SIZE + 0x1000;
        if version as i64 != X::C3X_MOD_VERSION || !fits {
            log::info!("C3X state: version {version}, {size:#x} bytes; districts need version {} ({:#x} bytes)", X::C3X_MOD_VERSION, X::STATE_SIZE);
            return Ok(None);
        }
        Ok(Some(C3xState { base }))
    }

    fn field<T: bytemuck::Pod>(&self, offset: u32) -> Result<T, Civ3Error> {
        Ok(self.base.offset::<T>(offset)?.read()?)
    }

    /// Which kinds of district are switched on (one read: the flags sit
    /// together in `current_config`).
    pub fn flags(&self) -> Result<DistrictFlags, Civ3Error> {
        let at = [
            X::STATE_CURRENT_CONFIG_ENABLE_DISTRICTS,
            X::STATE_CURRENT_CONFIG_ENABLE_NATURAL_WONDERS,
            X::STATE_CURRENT_CONFIG_ENABLE_NEIGHBORHOOD_DISTRICTS,
            X::STATE_CURRENT_CONFIG_ENABLE_WONDER_DISTRICTS,
            X::STATE_CURRENT_CONFIG_ENABLE_DISTRIBUTION_HUB_DISTRICTS,
            X::STATE_CURRENT_CONFIG_ENABLE_ENERGY_GRID_DISTRICTS,
            X::STATE_CURRENT_CONFIG_ENABLE_BRIDGE_DISTRICTS,
            X::STATE_CURRENT_CONFIG_ENABLE_CANAL_DISTRICTS,
            X::STATE_CURRENT_CONFIG_ENABLE_GREAT_WALL_DISTRICTS,
            X::STATE_CURRENT_CONFIG_DRAW_FORESTS_OVER_ROADS_AND_RAILROADS,
        ];
        let lo = at.into_iter().min().unwrap_or(0);
        let hi = at.into_iter().max().unwrap_or(0);
        let bytes = self.base.offset::<u8>(lo)?.read_array((hi - lo + 1) as usize)?;
        let on = |offset: u32| bytes[(offset - lo) as usize] != 0;
        Ok(DistrictFlags {
            districts: on(at[0]),
            natural_wonders: on(at[1]),
            neighborhood: on(at[2]),
            wonder: on(at[3]),
            distribution_hub: on(at[4]),
            energy_grid: on(at[5]),
            bridge: on(at[6]),
            canal: on(at[7]),
            great_wall: on(at[8]),
            forests_over_roads: on(at[9]),
        })
    }

    /// Whether districts (and natural wonders) are switched on.
    pub fn enabled(&self) -> Result<(bool, bool), Civ3Error> {
        let flags = self.flags()?;
        Ok((flags.districts, flags.natural_wonders))
    }

    /// A hash of the memory [`C3xState::configs`] reads from, a few hundred
    /// kilobytes in four reads: while it stays the same, so do the configs
    /// (they change when a game or scenario is loaded).
    pub fn fingerprint(&self) -> Result<u64, Civ3Error> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let regions = [
            (X::STATE_DISTRICT_CONFIGS, X::MAX_DISTRICT_TYPES as u32 * X::DISTRICTCONFIG_SIZE),
            (X::STATE_DISTRICT_INFOS, X::MAX_DISTRICT_TYPES as u32 * X::DISTRICTINFOS_SIZE),
            (X::STATE_WONDER_DISTRICT_CONFIGS, X::MAX_WONDER_DISTRICTS as u32 * X::WONDERCONFIG_SIZE),
            (X::STATE_NATURAL_WONDER_CONFIGS, X::MAX_NATURAL_WONDERS as u32 * X::NATURALWONDERCONFIG_SIZE),
        ];
        for (offset, len) in regions {
            self.base.offset::<u8>(offset)?.read_array(len as usize)?.hash(&mut hasher);
        }
        self.flags()?.hash(&mut hasher);
        self.field::<i32>(X::STATE_WORKABLE_TILE_COUNT)?.hash(&mut hasher);
        Ok(hasher.finish())
    }

    /// The district kinds, with the settings drawing them depends on. Many
    /// small reads: keep the result.
    pub fn configs(&self) -> Result<DistrictConfigs, Civ3Error> {
        let count = |offset: u32, max: i64| -> Result<u32, Civ3Error> { Ok(self.field::<i32>(offset)?.clamp(0, max as i32) as u32) };
        let at = |offset: u32| self.base.raw() + offset;
        let read_i32 = |p: u32| -> Result<i32, Civ3Error> { Ok(Ptr32::<i32>::new(p).read()?) };
        let read_u32 = |p: u32| -> Result<u32, Civ3Error> { Ok(Ptr32::<u32>::new(p).read()?) };
        let read_i16 = |p: u32| -> Result<i32, Civ3Error> { Ok(Ptr32::<i16>::new(p).read()?.into()) };
        let read_u8 = |p: u32| -> Result<bool, Civ3Error> { Ok(Ptr32::<u8>::new(p).read()? != 0) };
        let read_str = |p: u32| -> Result<String, Civ3Error> { Ok(c_string(Ptr32::<u32>::new(p).read()?)) };
        let mut out = DistrictConfigs {
            flags: self.flags()?,
            workable_tiles: self.field(X::STATE_WORKABLE_TILE_COUNT)?,
            mod_dir: c_string(at(X::STATE_MOD_REL_DIR)),
            ..Default::default()
        };
        for i in 0..count(X::STATE_DISTRICT_COUNT, X::MAX_DISTRICT_TYPES)? {
            let c = at(X::STATE_DISTRICT_CONFIGS) + i * X::DISTRICTCONFIG_SIZE;
            let infos = at(X::STATE_DISTRICT_INFOS) + i * X::DISTRICTINFOS_SIZE;
            let paths = read_i32(c + X::DISTRICTCONFIG_IMG_PATH_COUNT)?.clamp(0, 10) as u32;
            let dependents = read_i32(infos + X::DISTRICTINFOS_DEPENDENT_BUILDING_COUNT)?.clamp(0, 64) as u32;
            out.districts.push(DistrictKind {
                name: read_str(c + X::DISTRICTCONFIG_NAME)?,
                images: (0..paths).map(|k| read_str(c + X::DISTRICTCONFIG_IMG_PATHS + 4 * k)).collect::<Result<_, _>>()?,
                columns: read_i32(c + X::DISTRICTCONFIG_IMG_COLUMN_COUNT)?,
                by_era: read_u8(c + X::DISTRICTCONFIG_VARY_IMG_BY_ERA)?,
                by_culture: read_u8(c + X::DISTRICTCONFIG_VARY_IMG_BY_CULTURE)?,
                render_strategy: read_i32(c + X::DISTRICTCONFIG_RENDER_STRATEGY)?,
                kind: read_i32(c + X::DISTRICTCONFIG_TYPE)?,
                align_to_coast: read_u8(c + X::DISTRICTCONFIG_ALIGN_TO_COAST)?,
                draw_over_resources: read_u8(c + X::DISTRICTCONFIG_DRAW_OVER_RESOURCES)?,
                subsumes_tile_resource: read_u8(c + X::DISTRICTCONFIG_SUBSUMES_TILE_RESOURCE)?,
                allow_irrigation_from: read_u8(c + X::DISTRICTCONFIG_ALLOW_IRRIGATION_FROM)?,
                size: (read_i32(c + X::DISTRICTCONFIG_CUSTOM_WIDTH)?, read_i32(c + X::DISTRICTCONFIG_CUSTOM_HEIGHT)?),
                offset: (read_i32(c + X::DISTRICTCONFIG_X_OFFSET)?, read_i32(c + X::DISTRICTCONFIG_Y_OFFSET)?),
                generated_resource: read_i32(c + X::DISTRICTCONFIG_GENERATED_RESOURCE_ID)?,
                generated_resource_flags: read_i16(c + X::DISTRICTCONFIG_GENERATED_RESOURCE_FLAGS)?,
                resource_prereq_on_tile: read_i32(infos + X::DISTRICTINFOS_RESOURCE_PREREQ_ON_TILE_ID)?,
                resource_prereqs: read_i32(infos + X::DISTRICTINFOS_RESOURCE_PREREQ_COUNT)?,
                dependent_buildings: (0..dependents)
                    .map(|k| read_i32(infos + X::DISTRICTINFOS_DEPENDENT_BUILDING_IDS + 4 * k))
                    .collect::<Result<_, _>>()?,
            });
        }
        for i in 0..count(X::STATE_WONDER_DISTRICT_COUNT, X::MAX_WONDER_DISTRICTS)? {
            let c = at(X::STATE_WONDER_DISTRICT_CONFIGS) + i * X::WONDERCONFIG_SIZE;
            let pair = |a: u32, b: u32| -> Result<(i32, i32), Civ3Error> { Ok((read_i32(c + a)?, read_i32(c + b)?)) };
            out.wonders.push(WonderKind {
                name: read_str(c + X::WONDERCONFIG_WONDER_NAME)?,
                image: read_str(c + X::WONDERCONFIG_IMG_PATH)?,
                cell: pair(X::WONDERCONFIG_IMG_COLUMN, X::WONDERCONFIG_IMG_ROW)?,
                construct_cell: pair(X::WONDERCONFIG_IMG_CONSTRUCT_COLUMN, X::WONDERCONFIG_IMG_CONSTRUCT_ROW)?,
                alt: if read_u8(c + X::WONDERCONFIG_ENABLE_IMG_ALT_DIR)? {
                    Some((
                        pair(X::WONDERCONFIG_IMG_ALT_DIR_COLUMN, X::WONDERCONFIG_IMG_ALT_DIR_ROW)?,
                        pair(X::WONDERCONFIG_IMG_ALT_DIR_CONSTRUCT_COLUMN, X::WONDERCONFIG_IMG_ALT_DIR_CONSTRUCT_ROW)?,
                    ))
                } else {
                    None
                },
                size: pair(X::WONDERCONFIG_CUSTOM_WIDTH, X::WONDERCONFIG_CUSTOM_HEIGHT)?,
                buildable_square_types: read_u32(c + X::WONDERCONFIG_BUILDABLE_SQUARE_TYPES_MASK)?,
                buildable_on_rivers: read_u8(c + X::WONDERCONFIG_BUILDABLE_ON_RIVERS)?,
                improvement: None,
            });
        }
        for i in 0..count(X::STATE_NATURAL_WONDER_COUNT, X::MAX_NATURAL_WONDERS)? {
            let c = at(X::STATE_NATURAL_WONDER_CONFIGS) + i * X::NATURALWONDERCONFIG_SIZE;
            out.natural_wonders.push(NaturalWonderKind {
                name: read_str(c + X::NATURALWONDERCONFIG_NAME)?,
                image: read_str(c + X::NATURALWONDERCONFIG_IMG_PATH)?,
                cell: (read_i32(c + X::NATURALWONDERCONFIG_IMG_COLUMN)?, read_i32(c + X::NATURALWONDERCONFIG_IMG_ROW)?),
            });
        }
        Ok(out)
    }

    /// Every district instance. `tile_coords` is given a `Tile` pointer and
    /// the coordinates the instance records (-1 if it records none) and
    /// returns the tile's real coordinates (`None` drops the instance).
    ///
    /// C3X changes the table on the game's thread: a torn read is possible,
    /// so the header is read again afterwards and the whole read retried
    /// once if it moved; entries that make no sense are dropped.
    pub fn districts(&self, mut tile_coords: impl FnMut(u32, i32, i32) -> Option<(i32, i32)>, kinds: usize) -> Result<Vec<District>, Civ3Error> {
        for _ in 0..2 {
            let header = |s: &Self| -> Result<(u32, u32, u32), Civ3Error> {
                let t = X::STATE_DISTRICT_TILE_MAP;
                Ok((s.field(t + X::TABLE_BLOCK)?, s.field(t + X::TABLE_CAPACITY_EXPONENT)?, s.field(t + X::TABLE_LEN)?))
            };
            let (block, exponent, len) = header(self)?;
            if block == 0 || len == 0 {
                return Ok(Vec::new());
            }
            if exponent > 20 {
                continue;
            }
            // A bitmap of used slots, then (key, value) pairs.
            let capacity = 1u32 << exponent;
            let bitmap = Ptr32::<u8>::new(block).read_array(capacity.div_ceil(8) as usize)?;
            let pairs = Ptr32::<[u32; 2]>::new(block + capacity.div_ceil(8)).read_array(capacity as usize)?;
            let mut out = Vec::new();
            for (n, [key, value]) in pairs.into_iter().enumerate() {
                if bitmap[n >> 3] & (1 << (n & 7)) == 0 || value == 0 {
                    continue;
                }
                let d = Ptr32::<u8>::new(value);
                let int = |o: u32| -> Result<i32, Civ3Error> { Ok(d.offset::<i32>(o)?.read()?) };
                let Some((x, y)) = tile_coords(key, int(X::DISTRICT_TILE_X)?, int(X::DISTRICT_TILE_Y)?) else { continue };
                let kind = int(X::DISTRICT_DISTRICT_ID)?;
                if !(0..kinds as i32).contains(&kind) {
                    continue;
                }
                out.push(District {
                    x,
                    y,
                    kind,
                    completed: int(X::DISTRICT_STATE)? as i64 == X::DS_COMPLETED,
                    built_by: int(X::DISTRICT_BUILT_BY_CIV_ID)?,
                    wonder_state: int(X::DISTRICT_WONDER_INFO_STATE)?,
                    wonder: int(X::DISTRICT_WONDER_INFO_WONDER_INDEX)?,
                    wonder_city: int(X::DISTRICT_WONDER_INFO_CITY_ID)?,
                    natural_wonder: int(X::DISTRICT_NATURAL_WONDER_INFO_NATURAL_WONDER_ID)?,
                });
            }
            if header(self)? == (block, exponent, len) {
                return Ok(out);
            }
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 21 tiles of the 5×5 block of tile diamonds without its corners
    /// (the game's own work area), as offsets.
    fn in_radius_two(dx: i32, dy: i32) -> bool {
        let (u, v) = ((dx + dy) / 2, (dy - dx) / 2);
        (dx + dy) % 2 == 0 && u.abs() <= 2 && v.abs() <= 2 && !(u.abs() == 2 && v.abs() == 2)
    }

    #[test]
    fn the_work_area_starts_with_the_games_own() {
        let configs = DistrictConfigs { workable_tiles: 21, ..Default::default() };
        let area = configs.work_area();
        assert_eq!(area.len(), 21);
        assert_eq!(area[0], (0, 0));
        assert!(area.iter().all(|&(dx, dy)| in_radius_two(dx.into(), dy.into())));
        let mut sorted = area.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 21);
        // Radius one is the first ring: the centre and its eight neighbours.
        assert_eq!(DistrictConfigs { workable_tiles: 9, ..Default::default() }.work_area().len(), 9);
        // Not known: the game's default radius.
        assert_eq!(DistrictConfigs::default().work_area(), area);
        assert_eq!(DistrictConfigs { workable_tiles: 193, ..Default::default() }.work_area().len(), 193);
    }

    #[test]
    fn wonders_find_their_improvement_by_exact_name() {
        let mut configs = DistrictConfigs {
            wonders: ["The Pyramids", "The Oracle", "Lost"].map(|name| WonderKind { name: name.to_owned(), ..Default::default() }).to_vec(),
            ..Default::default()
        };
        let names: Vec<String> = ["Palace", "The Oracle", "The Pyramids", "The Pyramids"].map(str::to_owned).to_vec();
        configs.resolve_wonders(&names);
        assert_eq!(configs.wonders.iter().map(|w| w.improvement).collect::<Vec<_>>(), [Some(2), Some(1), None]);
    }

    #[test]
    fn wonders_on_rivers_are_those_with_the_river_bit_or_flag() {
        let wonder = |mask, on_rivers| WonderKind { buildable_square_types: mask, buildable_on_rivers: on_rivers, ..Default::default() };
        assert!(!wonder(0, false).requires_river());
        assert!(!wonder(1 << 1, false).requires_river());
        assert!(wonder(1 << X::SQ_RIVER, false).requires_river());
        assert!(wonder(0, true).requires_river());
    }
}
