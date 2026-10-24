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

/// Every active replacement viewer suppresses native pixels by default.
/// Showing its diagnostic window does not opt into duplicate rendering.
/// `on`/`compare` explicitly enables the original renderer; `compat` keeps
/// replacement rendering active and allows native recovery on viewer loss.
pub fn game_draws(setting: Option<&str>, viewer: Option<&str>, _window: Option<&str>) -> bool {
    match setting {
        Some("off") => false,
        Some("on" | "compare") => true,
        _ => viewer.is_none(),
    }
}

pub fn compatibility_fallback(setting: Option<&str>) -> bool {
    matches!(setting, Some("compat" | "compatibility"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupPolicy {
    pub capture: bool,
    pub game_draws: bool,
    pub fallback: bool,
    pub initial_map_off: bool,
}

pub fn startup_policy(setting: Option<&str>, viewer: Option<&str>, window: Option<&str>, ui: Option<&str>, map: Option<&str>) -> StartupPolicy {
    let game_draws = game_draws(setting, viewer, window);
    let capture = viewer.is_some() && ui != Some("off");
    StartupPolicy { capture, game_draws, fallback: compatibility_fallback(setting), initial_map_off: map_off(!game_draws, map) }
}

/// The explicit map-only diagnostic can enable map pixels while UI and
/// final-window pixel output remain suppressed.
pub fn map_off(skipping: bool, setting: Option<&str>) -> bool {
    match setting { Some("on") => false, Some("off") => true, _ => skipping }
}

/// The optional compatibility renderer retains the one-time title canvas
/// because the native m22 redraw cannot reconstruct it after viewer loss.
/// Normal replacement mode never retains this legacy pixel cache.
pub fn retained_title_canvas(compatibility: bool, canvas: u32, map_canvas: u32, main_gui_visible: Option<bool>) -> bool {
    compatibility && canvas != 0 && canvas == map_canvas && main_gui_visible == Some(false)
}

/// Invoke a native pixel callback only when policy permits it. Capture,
/// measurements and game-state callbacks run independently of this gate.
pub fn pixel_callback(skip: bool, original: impl FnOnce() -> i32) -> i32 {
    if skip { 0 } else { original() }
}

/// Native scaled wrappers multiply their crop offset by each numerator and
/// divide by the common positive denominator, truncating toward zero.
/// Preserve original crop size in the capture and resize its cached art.
pub fn scaled_geometry(origin: (i32, i32), crop: (i32, i32, i32, i32), scale: [u32; 3], compiled: bool) -> Option<((i32, i32), (i32, i32), [u32; 2])> {
    let [nx, ny, den] = scale.map(|n| n as i32);
    if nx <= 0 || ny <= 0 || den <= 0 { return None; }
    let crop_mul = |value: i32, numerator: i32| i32::try_from(i64::from(value) * i64::from(numerator) / i64::from(den)).ok();
    let placed = (origin.0.checked_add(crop_mul(crop.0, nx)?)?, origin.1.checked_add(crop_mul(crop.1, ny)?)?);
    scaled_placed(placed, (crop.2, crop.3), scale, compiled)
}

/// [`scaled_geometry`] of a sprite already placed: `placed` is the origin
/// plus the crop offset scaled (what the scaled wrappers pass jgl's blits),
/// `stored` the size of the sprite's stored pixels.
pub fn scaled_placed(placed: (i32, i32), stored: (i32, i32), scale: [u32; 3], compiled: bool) -> Option<((i32, i32), (i32, i32), [u32; 2])> {
    let [nx, ny, den] = scale.map(|n| n as i32);
    if nx <= 0 || ny <= 0 || den <= 0 || stored.0 <= 0 || stored.1 <= 0 { return None; }
    // JGL stores each ratio as f32, then x87 multiplies the integer extent
    // at extended precision before truncation. f32 multiplication is wrong
    // for examples such as49*(32/49), whose native extent is31, not32.
    let ratio = |n: i32| (f64::from(n) / f64::from(den)) as f32 as f64;
    let (sx, sy) = (ratio(nx), ratio(ny));
    let extent = |value: i32, ratio: f64| {
        let result = f64::from(value) * ratio;
        (result > 0.0 && result <= f64::from(i32::MAX)).then_some(result as i32)
    };
    let offset = !compiled && (nx != den || ny != den);
    let at = (placed.0.checked_add(if offset { sx as i32 } else { 0 })?, placed.1.checked_add(if offset { sy as i32 } else { 0 })?);
    let size = (extent(stored.0, sx)?, extent(stored.1, sy)?);
    let step = |n: i32| u32::try_from((u64::from(den as u32) << 16) / u64::from(n as u32)).ok();
    (size.0 > 0 && size.1 > 0).then_some((at, size, [step(nx)?, step(ny)?]))
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
    fn every_active_viewer_replaces_native_pixels_even_with_a_shown_diagnostic_window() {
        for viewer in ["1", "fullscreen", "native", "native-fullscreen", "external", "external-fullscreen"] {
            for window in [None, Some("hide"), Some("show")] {
                let policy = startup_policy(None, Some(viewer), window, None, None);
                assert_eq!(policy, StartupPolicy { capture: true, game_draws: false, fallback: false, initial_map_off: true });
                assert!(!game_draws(Some("off"), Some(viewer), window));
                assert!(game_draws(Some("on"), Some(viewer), window));
                assert!(game_draws(Some("compare"), Some(viewer), window));
            }
        }
        assert!(game_draws(None, None, None));
        assert!(!game_draws(Some("off"), None, None));
    }

    #[test]
    fn comparison_compatibility_and_map_only_diagnostics_are_explicit() {
        for setting in [None, Some("off"), Some("on"), Some("compare"), Some("yes")] {
            assert!(!startup_policy(setting, Some("native"), None, None, None).fallback);
        }
        for setting in ["compat", "compatibility"] {
            assert_eq!(startup_policy(Some(setting), Some("native"), None, None, None),
                StartupPolicy { capture: true, game_draws: false, fallback: true, initial_map_off: true });
        }
        assert!(!startup_policy(None, Some("native"), None, Some("off"), None).capture);
        assert!(!startup_policy(None, None, None, None, None).initial_map_off);
        assert!(!startup_policy(Some("off"), Some("native"), None, None, Some("on")).initial_map_off);
        assert!(startup_policy(Some("on"), Some("native"), None, None, Some("off")).initial_map_off);
    }

    #[test]
    fn title_pixel_callback_count_is_zero_by_default_and_enabled_only_explicitly() {
        use std::cell::Cell;
        let (map, screen, other) = (0x1000, 0x2000, 0x3000);
        for (setting, expected) in [(None, 0), (Some("off"), 0), (Some("on"), 1), (Some("compare"), 1), (Some("compat"), 1)] {
            let policy = startup_policy(setting, Some("native"), Some("show"), None, None);
            let mut skipper = Skipper::new(vec![]);
            let keep_title = retained_title_canvas(policy.fallback, map, map, Some(false));
            let skip = !policy.game_draws && !keep_title && skipper.skip(map);
            let calls = Cell::new(0);
            pixel_callback(skip, || { calls.set(calls.get() + 1); 7 });
            assert_eq!(calls.get(), expected, "native title callback count for {setting:?}");
        }
        assert!(!retained_title_canvas(false, map, map, Some(false)));
        assert!(!retained_title_canvas(true, map, map, Some(true)));
        assert!(!retained_title_canvas(true, map, map, None));
        assert!(!retained_title_canvas(true, screen, map, Some(false)));
        assert!(!retained_title_canvas(true, other, map, Some(false)));
        assert!(!retained_title_canvas(true, 0, 0, Some(false)));
    }

    #[test]
    fn scaled_resource_icons_and_half_size_form_art_keep_native_crop_offsets() {
        assert_eq!(scaled_geometry((10, 20), (2, 3, 120, 80), [128, 128, 160], true), Some(((11, 22), (96, 64), [81920, 81920])));
        assert_eq!(scaled_geometry((10, 20), (2, 3, 120, 80), [32, 32, 160], true), Some(((10, 20), (24, 16), [327680, 327680])));
        assert_eq!(scaled_geometry((10, 20), (-3, 3, 120, 80), [1, 1, 2], true), Some(((9, 21), (60, 40), [131072, 131072])));
        assert_eq!(scaled_geometry((10, 20), (2, 3, 120, 80), [1, 1, 1], false), Some(((12, 23), (120, 80), [65536, 65536])));
        let compiled = scaled_geometry((10, 20), (0, 0, 49, 49), [32, 32, 49], true).unwrap();
        assert_eq!(compiled.1, (31, 31));
        let compiled = scaled_geometry((10, 20), (0, 0, 49, 49), [128, 128, 49], true).unwrap();
        let plain = scaled_geometry((10, 20), (0, 0, 49, 49), [128, 128, 49], false).unwrap();
        assert_eq!(compiled.0, (10, 20));
        assert_eq!(plain.0, (12, 22));
        assert_eq!(compiled.1, (127, 127));
        for scale in [[1, 1, 0], [0, 1, 1], [1, u32::MAX, 1]] {
            assert_eq!(scaled_geometry((0, 0), (2, 3, 120, 80), scale, true), None);
        }
        assert_eq!(scaled_geometry((i32::MAX, 0), (2, 3, 120, 80), [1, 1, 1], true), None);
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
