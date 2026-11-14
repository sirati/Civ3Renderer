//! What the Civilization III viewer plugs into the generic viewer
//! (`tile_viewer::extension`): every host of the viewer (in the game's
//! process, natively under Wine, standalone) runs with these.

/// Application module: generic viewer shaders plus the selected glass effect.
pub const SHADERS: tile_render::Shaders = tile_render::Shaders::new(civ3_spirv::SPIRV, civ3_spirv::SPIRV_FALLBACK)
    .with_effect(glass::effect)
    .with_painter(civ3_spirv::PAINTER);

/// CPU oracle for the same application-selected renderer parts.
pub const ORACLE: tile_render::reference::Oracle<'static> = tile_render::reference::Oracle {
    effect: Some(&glass::ORACLE), painter: Some(&marker::PAINTER),
};

/// Defaults for a viewer hosted by this application, including its effect.
pub fn config() -> tile_viewer::ViewerConfig {
    tile_viewer::ViewerConfig { shaders: SHADERS, ..Default::default() }
}

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

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires an AMD GPU; validates application-selected glass through the generic headless viewer"]
    fn application_effect_is_used_by_headless_viewer_and_changes_pixels() {
        struct NoFiles;
        impl render_desc::ImageLoader for NoFiles {
            fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
                Err("the fixture has no external art".into())
            }
        }
        let compiled = render_desc::Description::parse("grid = { layout = 'orthogonal', step = [8, 8] }")
            .unwrap().compile(&NoFiles, 16).unwrap();
        let gpu = tile_render::Headless::new().unwrap();
        let scene = scene::Scene {
            screen_size: (128, 96),
            shape: scene::WorldShape { cols: 16, rows: 12, wrap_x: false, wrap_y: false },
            views: vec![scene::ViewState {
                id: 0, view: scene::View { viewport: (128, 96), ..Default::default() }, overlay: vec![],
            }],
            ..Default::default()
        };
        let settings = viewer_settings::Layout {
            zoom: viewer_settings::Zoom::Floating131,
            zoom_content: viewer_settings::ZoomContent::World,
            ..Default::default()
        };
        let plain = tile_viewer::ViewerConfig { clear: [48, 80, 112, 255], ..Default::default() };
        let application = tile_viewer::ViewerConfig { shaders: super::SHADERS, ..plain.clone() };
        let draw = |config| tile_viewer::render_planned(&gpu, &compiled, &scene, (128, 96), &Default::default(), &settings, config);
        let baseline = draw(&plain);
        let decorated = draw(&application);
        assert_ne!(baseline, decorated, "the application-supplied effect must reach the renderer");
        assert!(plain.shaders.effect.is_none());
        assert!(application.shaders.effect.is_some());
    }
}
