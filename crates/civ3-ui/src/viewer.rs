//! What the Civilization III viewer plugs into the generic viewer
//! (`tile_viewer::extension`): every host of the viewer (in the game's
//! process, natively under Wine, standalone) runs with these.

/// The extensions the viewer runs with.
pub fn extensions() -> tile_viewer::extension::Extensions {
    tile_viewer::extension::Extensions::default().with(crate::production_queue::viewer_extension)
}

/// Where the renderer's settings are kept: `$CIV3R_SETTINGS`, else
/// `civ3r/settings.toml` in the user's configuration folder
/// (`viewer_settings::Settings::default_path_for`).
pub fn settings_path() -> std::path::PathBuf {
    viewer_settings::Settings::default_path_for("civ3r", "CIV3R_SETTINGS")
}

/// Where a host records the Alt mode: `CIV3R_ALTREC_DIR` if set, else
/// `default` (the folder of the game's log).
pub fn recording_dir(default: Option<std::path::PathBuf>) -> Option<std::path::PathBuf> {
    std::env::var_os("CIV3R_ALTREC_DIR").filter(|d| !d.is_empty()).map(Into::into).or(default)
}

/// The file the launcher waits for (`CIV3R_STARTUP_READY_FILE`), if any.
pub fn startup_ready_file() -> Option<std::path::PathBuf> {
    std::env::var_os("CIV3R_STARTUP_READY_FILE").map(Into::into)
}

/// The desktop scale a native viewer's launcher gave the game
/// (`CIV3R_SCALE`), if any.
pub fn launcher_scale() -> Option<f64> {
    std::env::var("CIV3R_SCALE").ok().and_then(|s| s.trim().parse::<f64>().ok())
}
