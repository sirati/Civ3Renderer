//! Locating game files.
//!
//! Civilization III resolves an art path such as `Art\Terrain\xtgc.pcx` by
//! searching the scenario folder, then `Conquests`, then `civ3PTW`, then the
//! base install. Paths in its data use any letter case and backslashes, so on
//! a case-sensitive file system every component is matched case-insensitively.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("{0} not found in any search root")]
    NotFound(String),
    #[error("reading {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
}

/// The search roots of an installation, highest priority first.
pub struct GameFiles {
    roots: Vec<PathBuf>,
    dir_cache: Mutex<HashMap<PathBuf, Vec<(String, PathBuf)>>>,
}

impl GameFiles {
    /// Search roots for a "Civilization III Complete" folder: Conquests,
    /// then Play the World, then the original game.
    pub fn complete(install: impl AsRef<Path>) -> Self {
        let install = install.as_ref();
        let mut roots = Vec::new();
        for sub in ["Conquests", "civ3PTW", ""] {
            if let Some(dir) = find_ci(install, sub) {
                roots.push(dir);
            }
        }
        Self::with_roots(roots)
    }

    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self { roots, dir_cache: Mutex::new(HashMap::new()) }
    }

    /// Puts `dir` (e.g. a scenario's search folder) before all other roots.
    pub fn push_front(&mut self, dir: PathBuf) {
        self.roots.insert(0, dir);
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Resolves a game-relative path (`/` or `\` separated, any case).
    pub fn resolve(&self, rel: &str) -> Result<PathBuf, FileError> {
        let parts: Vec<&str> = rel.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
        'roots: for root in &self.roots {
            let mut dir = root.clone();
            for part in &parts {
                match self.child(&dir, part) {
                    Some(next) => dir = next,
                    None => continue 'roots,
                }
            }
            return Ok(dir);
        }
        Err(FileError::NotFound(rel.to_owned()))
    }

    pub fn read(&self, rel: &str) -> Result<Vec<u8>, FileError> {
        let path = self.resolve(rel)?;
        std::fs::read(&path).map_err(|source| FileError::Io { path, source })
    }

    fn child(&self, dir: &Path, name: &str) -> Option<PathBuf> {
        let exact = dir.join(name);
        if exact.exists() {
            return Some(exact);
        }
        let mut cache = self.dir_cache.lock().unwrap();
        let entries = cache.entry(dir.to_owned()).or_insert_with(|| {
            std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| (e.file_name().to_string_lossy().to_lowercase(), e.path()))
                .collect()
        });
        let lower = name.to_lowercase();
        entries.iter().find(|(n, _)| *n == lower).map(|(_, p)| p.clone())
    }
}

fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return dir.is_dir().then(|| dir.to_owned());
    }
    let lower = name.to_lowercase();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().to_lowercase() == lower)
        .map(|e| e.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_case_insensitively_with_priority() {
        let tmp = std::env::temp_dir().join(format!("civ3-assets-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("Conquests/Art/Terrain")).unwrap();
        std::fs::create_dir_all(tmp.join("Art/Terrain")).unwrap();
        std::fs::write(tmp.join("Conquests/Art/Terrain/XTGC.pcx"), b"conquests").unwrap();
        std::fs::write(tmp.join("Art/Terrain/xtgc.pcx"), b"base").unwrap();
        std::fs::write(tmp.join("Art/Terrain/only_base.pcx"), b"only").unwrap();
        let files = GameFiles::complete(&tmp);
        assert_eq!(files.read(r"art\terrain\xtgc.PCX").unwrap(), b"conquests");
        assert_eq!(files.read("Art/Terrain/Only_Base.pcx").unwrap(), b"only");
        assert!(files.read("Art/nope.pcx").is_err());
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
