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
