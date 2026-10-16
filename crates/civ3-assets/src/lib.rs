//! Rust-native readers for Civilization III's asset formats.
//!
//! Pure file formats and file lookup: no game memory, no rendering. A
//! renderer turns what this crate decodes into textures; a render
//! description (TOML) says which file holds what.

pub mod files;
pub mod flc;
pub mod ini;
#[cfg(feature = "render-desc")]
pub mod fonts;
#[cfg(feature = "render-desc")]
pub mod loader;
pub mod pcx;

pub use files::GameFiles;
pub use pcx::Indexed;

/// An RGB colour.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// A 256-entry palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette(pub [Rgb; 256]);

impl Default for Palette {
    fn default() -> Self {
        Palette([Rgb::default(); 256])
    }
}

impl Palette {
    pub fn get(&self, index: u8) -> Rgb {
        self.0[index as usize]
    }
}
