//! Which Civ3Conquests.exe is running, and where its globals are.
//!
//! The three v1.22 builds C3X supports (GOG, Steam, PCGames.de) are separate
//! compilations: globals and functions sit at different addresses. They are
//! told apart like C3X does it, by their `.rdata` section, read here from the
//! loaded image (so it works on the C3X-modded exe too, whose file size no
//! longer identifies it).

use inproc::Ptr32;

use crate::Civ3Error;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Build {
    Gog,
    Steam,
    PcGames,
}

/// Addresses of the game's global objects (from C3X's civ_prog_objects.csv,
/// "define" rows: these are the objects themselves, not pointers to them).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Addresses {
    /// `BIC` — scenario/rules data; contains the `Map`.
    pub bic_data: u32,
    /// `Main_Screen_Form` — the main map screen (camera, selection, mouse).
    pub main_screen_form: u32,
    pub units: u32,
    pub tile_units: u32,
    pub cities: u32,
    /// `Leader[32]` — the players.
    pub leaders: u32,
    pub current_turn_no: u32,
    /// `p_preferences` (C3X's civ_prog_objects.csv): `P_*` flag word.
    pub preferences: u32,
}

impl Build {
    pub const fn addresses(self) -> Addresses {
        match self {
            Build::Gog => Addresses {
                bic_data: 0x9C_3508,
                main_screen_form: 0x9F_8700,
                units: 0xA5_2E80,
                tile_units: 0xA5_2DD4,
                cities: 0xA5_2E68,
                leaders: 0xA5_2E98,
                current_turn_no: 0xA5_26AC,
                preferences: 0xA5_2678,
            },
            Build::Steam => Addresses {
                bic_data: 0x9E_5D08,
                main_screen_form: 0xA1_AF00,
                units: 0xA7_5680,
                tile_units: 0xA7_55D0,
                cities: 0xA7_5668,
                leaders: 0xA7_5698,
                current_turn_no: 0xA7_4EA4,
                preferences: 0xA7_4E70,
            },
            Build::PcGames => Addresses {
                bic_data: 0x9C_34C8,
                main_screen_form: 0x9F_86C0,
                units: 0xA5_2E40,
                tile_units: 0xA5_2D94,
                cities: 0xA5_2E28,
                leaders: 0xA5_2E58,
                current_turn_no: 0xA5_266C,
                preferences: 0xA5_2638,
            },
        }
    }

    /// `.rdata` (virtual address, raw size) of each build, from C3X's ep.c.
    const fn rdata(self) -> (u32, u32) {
        match self {
            Build::Gog => (0x66_5000, 0x1_B000),
            Build::Steam => (0x68_2000, 0x1_B000),
            Build::PcGames => (0x66_5000, 0x1_A400),
        }
    }

    /// Identifies the build from the PE image loaded at `image_base`.
    pub fn detect(image_base: u32) -> Result<Build, Civ3Error> {
        let (rdata_va, rdata_size) = rdata_section(image_base)?;
        [Build::Gog, Build::Steam, Build::PcGames]
            .into_iter()
            .find(|b| b.rdata() == (rdata_va, rdata_size))
            .ok_or(Civ3Error::UnknownBuild { rdata_va, rdata_size })
    }
}

/// Image base of Civ3Conquests.exe (it has no relocations).
pub const IMAGE_BASE: u32 = 0x40_0000;

/// Finds the `.rdata` section header and returns (absolute VA, raw size).
fn rdata_section(base: u32) -> Result<(u32, u32), Civ3Error> {
    let image = Ptr32::<u8>::new(base);
    let e_lfanew: u32 = image.offset::<u32>(0x3C)?.read()?;
    let nt = image.offset::<u8>(e_lfanew)?;
    let sections: u16 = nt.offset::<u16>(6)?.read()?;
    let optional_size: u16 = nt.offset::<u16>(20)?.read()?;
    let table = nt.offset::<[u8; 40]>(24 + optional_size as u32)?;
    for i in 0..sections as u32 {
        let header = table.index(i)?.read()?;
        if &header[..8] == b".rdata\0\0" {
            let word = |at: usize| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
            return Ok((base + word(12), word(16)));
        }
    }
    Err(Civ3Error::UnknownBuild { rdata_va: 0, rdata_size: 0 })
}
