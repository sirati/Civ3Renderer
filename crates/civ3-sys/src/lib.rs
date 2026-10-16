//! The interface to a running Civilization III: Conquests (v1.22), seen from
//! code injected into its process.
//!
//! Everything here is a *fact about the game binary*: which build is running,
//! where its global objects live, the byte layout of its structs (generated
//! from C3X's reverse-engineered `Civ3Conquests.h`, see `layout.rs`), and the
//! calling conventions of the functions we hook. Reads go through
//! [`inproc::Ptr32`], so they are safe; interpretation of the data (what a
//! tile *means* for rendering) is the business of the crates above.
// No unsafe code of our own; the hook declarations in `hooks` expand
// `inproc::fastcall_hook!`, whose generated trampolines are the exception.
#![deny(unsafe_code)]

pub mod build;
pub mod c3x;
pub mod c3x_layout;
pub mod city_label;
pub mod hooks;
pub mod layout;
pub mod sprites;
pub mod ui_hooks;
pub mod world;

pub use build::{Addresses, Build};
pub use world::{Civ3, CityInfo, DrawnUnit, IconBuilding, ImprovementInfo, MapInfo, Players, TileRecord, ViewState, WallBuilding};

#[derive(Debug, thiserror::Error)]
pub enum Civ3Error {
    #[error(transparent)]
    Mem(#[from] inproc::MemError),
    #[error("unrecognised Civ3Conquests.exe build (.rdata at {rdata_va:#x}, {rdata_size:#x} bytes)")]
    UnknownBuild { rdata_va: u32, rdata_size: u32 },
    #[error("no game in progress")]
    NoGame,
}
