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
    pub fn city_id(&self) -> i16 {
        self.i16(L::TILE_CITYID)
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
