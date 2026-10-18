//! Whether the game does its own pixel work (`CIV3R_GAME_RENDER`).
//!
//! With the renderer drawing everything, the game only has to decide what
//! to show: the interface mirror (`ui_trace`) records every drawing
//! operation it hooks and then skips the game's own drawing of it
//! (sprite blits, image copies, fills, lines, translucent shades, text
//! output), the window update (`present`'s `BitBlt`) and, through the map's
//! tracer, the map's blits. Everything that decides what is drawn still
//! runs: forms redraw, compose and present as before, text is measured,
//! animations pick their frames, the map pass visits its tiles.
//!
//! Drawing goes on where the game reads the result back: onto images of
//! palette indices (its maps of what is under the mouse), onto the canvases
//! of colour-keyed forms (whose transparent pixels let clicks through),
//! onto the minimap's images (`ui_hooks::read_back_images`), and onto any
//! other image the game made itself once it cuts a sprite from it or reads
//! a pixel of it (see [`Skipper::reads`]).

use std::collections::HashSet;

/// Whether the game draws itself, from `CIV3R_GAME_RENDER` (`on` / `off`),
/// `CIV3R_VIEWER` and `CIV3R_GAME_WINDOW`: by default not with the native
/// viewer (the game's window is not what the player looks at) unless the
/// game's window is asked to be shown (`show`), else yes (the in-process
/// viewer may show the game's window beside its own).
pub fn game_draws(setting: Option<&str>, viewer: Option<&str>, window: Option<&str>) -> bool {
    match setting {
        Some("off") => false,
        Some("on") => true,
        _ => !viewer.is_some_and(|v| v.starts_with("native")) || window == Some("show"),
    }
}

/// Startup policy before capture can enable full drawing suppression.
/// Explicit map-only rendering and UI-off/no-viewer diagnostics retain
/// their historical map policy independently of full UI capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupPolicy {
    pub capture: bool,
    pub game_draws: bool,
    pub initial_map_off: bool,
}

pub fn startup_policy(setting: Option<&str>, viewer: Option<&str>, window: Option<&str>, ui: Option<&str>, map: Option<&str>) -> StartupPolicy {
    let game_draws = game_draws(setting, viewer, window);
    let capture = viewer.is_some() && ui != Some("off");
    StartupPolicy { capture, game_draws, initial_map_off: map == Some("off") || (!capture && !game_draws) }
}

/// Which drawing is skipped.
#[derive(Debug, Default)]
pub struct Skipper {
    /// Images drawn onto as before.
    keep: HashSet<u32>,
    /// Images some drawing onto was skipped (since their content was last
    /// replaced by a file).
    skipped: HashSet<u32>,
    /// Images holding a file's pixels.
    files: HashSet<u32>,
}

impl Skipper {
    /// `keep`: images whose pixels the game reads.
    pub fn new(keep: Vec<u32>) -> Self {
        Self { keep: keep.into_iter().collect(), ..Default::default() }
    }

    /// Whether to skip a drawing onto `canvas` (noted if so).
    pub fn skip(&mut self, canvas: u32) -> bool {
        if canvas == 0 || self.keep.contains(&canvas) {
            return false;
        }
        self.skipped.insert(canvas);
        true
    }

    /// A file was loaded into `image` (all its pixels replaced).
    pub fn loaded(&mut self, image: u32) {
        self.skipped.remove(&image);
        self.files.insert(image);
    }

    /// `image` lost its pixels.
    pub fn cleared(&mut self, image: u32) {
        self.skipped.remove(&image);
        self.files.remove(&image);
    }

    /// The game uses the pixels of `image` (cuts a sprite out of it, reads a
    /// pixel): an image the game drew itself (not a file as loaded) is drawn
    /// onto as before from now on. Whether drawing onto it was skipped
    /// before (what the game got then lacks that).
    pub fn reads(&mut self, image: u32) -> bool {
        let skipped = self.skipped.remove(&image);
        if skipped || !self.files.contains(&image) {
            self.keep.insert(image);
        }
        skipped
    }

    /// Whether some drawing onto `image` was skipped.
    pub fn was_skipped(&self, image: u32) -> bool {
        self.skipped.contains(&image)
    }

    /// Whether drawing onto `image` is kept.
    pub fn keeps(&self, image: u32) -> bool {
        self.keep.contains(&image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_capture_preserves_ui_off_and_no_viewer_map_policy() {
        for (setting, viewer, window, ui, map, capture, draws, map_off) in [
            (None, Some("native"), None, None, None, true, false, false),
            (Some("off"), Some("1"), None, None, None, true, false, false),
            (Some("on"), Some("native"), None, None, None, true, true, false),
            (None, Some("native"), Some("show"), None, None, true, true, false),
            (None, Some("native"), None, Some("off"), None, false, false, true),
            (Some("off"), Some("1"), None, Some("off"), None, false, false, true),
            (Some("on"), Some("native"), None, Some("off"), None, false, true, false),
            (Some("off"), None, None, None, None, false, false, true),
            (None, None, None, None, None, false, true, false),
            (Some("on"), None, None, Some("off"), Some("off"), false, true, true),
            (None, Some("native"), None, None, Some("off"), true, false, true),
        ] {
            assert_eq!(startup_policy(setting, viewer, window, ui, map), StartupPolicy { capture, game_draws: draws, initial_map_off: map_off }, "policy for setting={setting:?} viewer={viewer:?} window={window:?} ui={ui:?} map={map:?}");
        }
    }

    #[test]
    fn the_native_viewer_skips_by_default_the_in_process_one_does_not() {
        assert!(!game_draws(None, Some("native"), None));
        assert!(!game_draws(None, Some("native-fullscreen"), None));
        assert!(game_draws(None, Some("1"), None));
        assert!(game_draws(None, Some("fullscreen"), None));
        assert!(game_draws(None, None, None));
        assert!(game_draws(Some("on"), Some("native"), None));
        assert!(!game_draws(Some("off"), Some("1"), None));
        // Anything else is the default.
        assert!(!game_draws(Some("yes"), Some("native"), None));
    }

    #[test]
    fn a_shown_game_window_draws_unless_told_not_to() {
        assert!(game_draws(None, Some("native"), Some("show")));
        assert!(!game_draws(None, Some("native"), Some("hide")));
        assert!(!game_draws(Some("off"), Some("native"), Some("show")));
    }

    #[test]
    fn kept_images_are_drawn_onto() {
        let mut s = Skipper::new(vec![7]);
        assert!(!s.skip(7));
        assert!(s.skip(8));
        assert!(!s.skip(0));
        assert!(s.was_skipped(8) && !s.was_skipped(7));
    }

    #[test]
    fn an_image_the_game_reads_is_drawn_onto_from_then_on() {
        let mut s = Skipper::new(Vec::new());
        assert!(s.skip(5));
        assert!(s.reads(5), "the earlier skip is reported");
        assert!(!s.skip(5));
        assert!(!s.reads(5));
        // One the game made itself, never drawn onto by a skipped drawing yet.
        assert!(!s.reads(6) && s.keeps(6));
    }

    #[test]
    fn files_as_loaded_are_not_kept() {
        let mut s = Skipper::new(Vec::new());
        assert!(s.skip(9));
        s.loaded(9);
        assert!(!s.was_skipped(9));
        assert!(!s.reads(9) && !s.keeps(9));
        // Drawn onto after loading: what the sprite needs was skipped.
        assert!(s.skip(9));
        assert!(s.reads(9) && s.keeps(9));
        s.cleared(4);
        assert!(!s.reads(4) && s.keeps(4));
    }
}
