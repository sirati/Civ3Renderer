use std::path::PathBuf;

#[derive(Clone, Default)]
pub struct PathMapper {
    pub prefix: Option<PathBuf>,
    pub mounts: Vec<(String, PathBuf)>,
}

impl PathMapper {
    pub fn add(&mut self, mapping: &str) -> Result<(), String> {
        let (windows, local) = mapping
            .split_once('=')
            .ok_or("--map-root expects WINDOWS=LOCAL")?;
        if windows.is_empty() || local.is_empty() {
            return Err("--map-root expects nonempty paths".into());
        }
        self.mounts.push((
            windows.replace('\\', "/").trim_end_matches('/').to_owned(),
            PathBuf::from(local),
        ));
        self.mounts
            .sort_by_key(|(root, _)| std::cmp::Reverse(root.len()));
        Ok(())
    }

    pub fn resolve(&self, windows: &str) -> Result<PathBuf, String> {
        let windows = windows.replace('\\', "/");
        let lower = windows.to_ascii_lowercase();
        for (root, local) in &self.mounts {
            let root_lower = root.to_ascii_lowercase();
            if lower == root_lower
                || lower
                    .strip_prefix(&root_lower)
                    .is_some_and(|tail| tail.starts_with('/'))
            {
                return Ok(local.join(windows[root.len()..].trim_start_matches('/')));
            }
        }
        if let Some((drive, tail)) = windows.split_once(':') {
            if drive.len() == 1 && drive.bytes().all(|b| b.is_ascii_alphabetic()) {
                let prefix = self
                    .prefix
                    .as_ref()
                    .ok_or_else(|| format!("no --wine-prefix or --map-root for {windows}"))?;
                return Ok(prefix
                    .join("dosdevices")
                    .join(format!("{}:", drive.to_ascii_lowercase()))
                    .join(tail.trim_start_matches('/')));
            }
        }
        Ok(PathBuf::from(windows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_nested_windows_roots_by_longest_prefix_without_false_matches() {
        let mut paths = PathMapper::default();
        paths.add("D:\\Games=/Volumes/Game").unwrap();
        paths.add("d:\\Games\\Civ3\\Mods=/Volumes/Mods").unwrap();
        assert_eq!(
            paths.resolve("D:\\GAMES\\Civ3\\Mods\\Scenario").unwrap(),
            PathBuf::from("/Volumes/Mods/Scenario")
        );
        assert_eq!(
            paths.resolve("d:/Games/Civ3/Conquests").unwrap(),
            PathBuf::from("/Volumes/Game/Civ3/Conquests")
        );
        assert!(paths.resolve("D:/GamesExtra").is_err());
        assert_eq!(
            paths.resolve("/Volumes/Game/Conquests").unwrap(),
            PathBuf::from("/Volumes/Game/Conquests")
        );
    }
    #[test]
    fn maps_wine_drives_without_linux_install_paths() {
        let paths = PathMapper {
            prefix: Some("/Users/player/Wine".into()),
            ..Default::default()
        };
        assert_eq!(
            paths.resolve("C:\\Games\\Civ3").unwrap(),
            PathBuf::from("/Users/player/Wine/dosdevices/c:/Games/Civ3")
        );
    }
}
