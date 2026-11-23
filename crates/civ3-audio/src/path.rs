//! Windows-to-host path resolution with case-insensitive component matching on
//! Unix (Linux/macOS) and pass-through on Windows.

use std::path::{Path, PathBuf};

/// Resolves a game-supplied path (possibly Windows-formatted with `\`, drive
/// letter `C:\...`, or mixed case) to a real host [`PathBuf`].
pub fn resolve_game_path(raw: &str, base_dir: Option<&Path>) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    #[cfg(windows)]
    {
        let p = PathBuf::from(trimmed);
        if p.is_absolute() {
            return Some(p);
        }
        return Some(match base_dir {
            Some(dir) => dir.join(p),
            None => p,
        });
    }

    #[cfg(unix)]
    {
        let normalized = trimmed.replace('\\', "/");

        // Handle Windows drive letter paths (`C:/Games/...`).
        let (root, rest) = if let Some((drive, tail)) = normalized.split_once(':') {
            if drive.len() == 1 && drive.as_bytes()[0].is_ascii_alphabetic() {
                let prefix = std::env::var_os("WINEPREFIX")
                    .map(PathBuf::from)
                    .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".wine")))
                    .unwrap_or_default();
                let drive_root = prefix
                    .join("dosdevices")
                    .join(format!("{}:", drive.to_ascii_lowercase()));
                (drive_root, tail.trim_start_matches('/'))
            } else {
                let start = base_dir
                    .map(Path::to_path_buf)
                    .or_else(|| std::env::current_dir().ok())
                    .unwrap_or_else(|| PathBuf::from("."));
                (start, normalized.as_str())
            }
        } else if normalized.starts_with('/') {
            (PathBuf::from("/"), normalized.trim_start_matches('/'))
        } else {
            let start = base_dir
                .map(Path::to_path_buf)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            (start, normalized.as_str())
        };

        // Fast path: exact match exists as-is.
        let direct = root.join(rest);
        if direct.is_file() {
            return Some(direct);
        }

        // Case-insensitive walk component by component.
        let mut cur = root;
        for part in rest.split('/').filter(|s| !s.is_empty()) {
            if part == "." {
                continue;
            }
            if part == ".." {
                cur = cur.join("..");
                continue;
            }
            let exact = cur.join(part);
            if exact.exists() {
                cur = exact;
                continue;
            }
            let lower = part.to_ascii_lowercase();
            let mut found = None;
            if let Ok(rd) = std::fs::read_dir(&cur) {
                for entry in rd.flatten() {
                    if entry.file_name().to_string_lossy().to_ascii_lowercase() == lower {
                        found = Some(entry.path());
                        break;
                    }
                }
            }
            cur = found?;
        }
        cur.is_file().then_some(cur)
    }
}

/// Resolves a child `.wav` referenced inside an `.amb` file. When
/// `relative_to_amb` is true (matching `AmbSound+0x58 & 1`), searches the
/// `.amb`'s parent directory first, then falls back to the game CWD.
pub fn resolve_amb_wav(amb_path: &Path, wav_name: &str, relative_to_amb: bool) -> Option<PathBuf> {
    if relative_to_amb {
        if let Some(parent) = amb_path.parent() {
            if let Some(found) = resolve_game_path(wav_name, Some(parent)) {
                return Some(found);
            }
        }
    }
    resolve_game_path(wav_name, None).or_else(|| {
        amb_path
            .parent()
            .and_then(|parent| resolve_game_path(wav_name, Some(parent)))
    })
}
