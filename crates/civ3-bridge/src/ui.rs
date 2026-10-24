//! The mirrored user interface (docs §16) is `civ3_ui::ui`; this is it as
//! the bridge sees it, with the one thing it asks of the platform.

pub use civ3_ui::ui::*;

/// Whether the game's line drawer draws smooth lines (see [`line_fills`]):
/// on Windows, not in Wine (where C3X, as installed, draws them with GDI+).
pub fn smooth_lines() -> bool {
    static SMOOTH: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SMOOTH.get_or_init(|| cfg!(windows) && !gdi_text::under_wine())
}
