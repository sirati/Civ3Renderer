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
    Description::parse(text)?.compile(&Civ3Loader::new(files), 4096)
}
