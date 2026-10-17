//! Where the game's art comes from, and compiling the render description
//! against it.

use std::path::{Path, PathBuf};

use civ3_assets::GameFiles;
use civ3_assets::loader::Civ3Loader;
use render_desc::{Compiled, DescError, Description};

/// The render description built into the bridge.
pub const BUILTIN_DESC: &str = include_str!("../../../assets/civ3/civ3.toml");

/// The art search roots of `install` ("Civilization III Complete") while
/// a scenario is loaded: its art folder and search folders (as the game
/// names them, relative to `Conquests` or `Conquests/Conquests`, with
/// backslashes) first, then Conquests, Play the World and the base game.
pub fn game_files(install: &Path, scenario_folder: Option<&str>, search_roots: &[String]) -> GameFiles {
    let mut files = GameFiles::complete(install);
    let base = GameFiles::with_roots(vec![install.to_path_buf()]);
    let scenario: Vec<PathBuf> = scenario_folder
        .into_iter()
        .chain(search_roots.iter().map(String::as_str))
        .filter_map(|folder| {
            let rel = folder.replace('\\', "/");
            let rel = rel.trim_matches('/');
            ["Conquests/Conquests", "Conquests", ""].iter().find_map(|b| base.resolve(&format!("{b}/{rel}")).ok())
        })
        .collect();
    for dir in scenario.into_iter().rev() {
        files.push_front(dir);
    }
    files
}

/// Compiles a description (TOML text) against the game's art.
pub fn compile(text: &str, files: GameFiles) -> Result<Compiled, DescError> {
    // 2048² pages: the atlas texture is sized to the pages in use, and stays
    // small enough for 32-bit processes and software renderers.
    Description::parse(text)?.compile(&Civ3Loader::new(files), 2048)
}

/// Like [`compile`], for the zoomed-out view when `scale_down` is 2: the
/// art at half size (§3).
pub fn compile_scaled(text: &str, files: GameFiles, scale_down: u32) -> Result<Compiled, DescError> {
    compile_as(text, files, scale_down, Pixels::Made(None))
}

/// What a compile makes of the art (see [`compile_as`]).
pub enum Pixels<'a> {
    /// The atlas, reusing the sheets of the cache's last compile, if any
    /// (`render_desc::SheetCache`).
    Made(Option<&'a mut render_desc::SheetCache>),
    /// Only the tables (sprite names, sizes, sets, layers:
    /// `Compiled::without_pixels`), from the art files' headers: what a
    /// driver needs whose renderer makes its own atlas.
    TablesOnly,
}

/// [`compile_scaled`], with or without the pixels.
pub fn compile_as(text: &str, files: GameFiles, scale_down: u32, pixels: Pixels) -> Result<Compiled, DescError> {
    let desc = Description::parse(text)?;
    let desc = if scale_down > 1 { desc.scaled_down(scale_down) } else { desc };
    let loader = Civ3Loader::new(files);
    match pixels {
        Pixels::Made(Some(cache)) => desc.compile_cached(&loader, 2048, cache),
        Pixels::Made(None) => desc.compile(&loader, 2048),
        Pixels::TablesOnly => desc.compile_layout(&loader, 2048),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The built-in description compiled for the zoomed-out view against the
    /// game's art (`CIV3_INSTALL`; skipped without an installation): terrain
    /// sprites are cropped to the box of their non-255 pixels (columns 1..126
    /// of the cell), and sampled from there (§3.1).
    #[test]
    fn zoomed_out_terrain_is_sampled_from_its_box() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        if !Path::new(&install).is_dir() {
            return eprintln!("no Civ III install, skipped");
        }
        let compiled = compile_scaled(BUILTIN_DESC, GameFiles::complete(&install), 2).unwrap();
        let sprite = |name: &str| compiled.atlas.sprites[compiled.sprite_names.iter().position(|n| n == name).unwrap()];
        let pixel = |name: &str, x: u32, y: u32| {
            let s = sprite(name);
            compiled.atlas.pages[s.page as usize].get(s.atlas_x + x, s.atlas_y + y)
        };
        let s = sprite("Std_Terrain_Images/0");
        assert_eq!((s.width, s.height), (64, 32));
        // The tables alone agree with the full compile.
        let layout = compile_as(BUILTIN_DESC, GameFiles::complete(&install), 2, Pixels::TablesOnly).unwrap();
        assert_eq!(layout.sprite_names, compiled.sprite_names);
        let sizes = |c: &Compiled| c.atlas.sprites.iter().map(|s| (s.width, s.height, s.anchor_x, s.anchor_y)).collect::<Vec<_>>();
        assert_eq!(sizes(&layout), sizes(&compiled));
        // Cell column 1 (the left tip of the diamond) is sample 0; the 126
        // columns of the box make 63 samples, so the last column stays clear
        // (taking every second pixel from the cell's corner it would not).
        assert_eq!(pixel("Std_Terrain_Images/0", 0, 16)[3], 255);
        assert_eq!(pixel("Std_Terrain_Images/0", 63, 16)[3], 0);
        // The top row has two opaque pixels, cell columns 63 and 64: the odd
        // one is sampled (sample 31), the even one is not (sample 32).
        assert_eq!(pixel("Std_Terrain_Images/0", 31, 0)[3], 255);
        assert_eq!(pixel("Std_Terrain_Images/0", 32, 0)[3], 0);
    }
}
