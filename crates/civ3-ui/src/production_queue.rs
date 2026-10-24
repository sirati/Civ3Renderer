//! The city screen's build queue is a site's production queue
//! (`tile_game::production_queue`): reordered by dragging in the viewer,
//! its handles and hints in the city screen's colours, brown on its
//! parchment. The bridge sends the queue and does the edits
//! (`civ3_bridge::build_queue`).

use tile_game::production_queue::{QueueDrag, QueueStyle};

/// The city screen's colours.
pub const STYLE: QueueStyle = QueueStyle {
    // The handle's bars: dark groove, light edge below (as the screen's
    // engraved lines).
    groove: [92, 64, 34, 255],
    edge: [250, 240, 214, 255],
    line: [176, 18, 8, 255],
    line_edge: [255, 248, 226, 255],
    hint_fill: [255, 214, 90, 80],
    hint_frame: [214, 150, 20, 255],
    arrow: [120, 60, 10, 255],
    dim: [240, 230, 205, 170],
    ghost_back: [246, 236, 210, 110],
    ghost_frame: [120, 84, 44, 150],
    ghost_opacity: 130,
};

/// The viewer's drag of the build queue, for `tile_viewer::ViewerConfig::extensions`.
pub fn viewer_extension() -> Box<dyn tile_viewer::extension::InterfaceExtension> {
    Box::new(QueueDrag::new(STYLE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copy_lets_the_hints_show_through() {
        assert!(STYLE.ghost_back[3] <= 128 && STYLE.ghost_opacity <= 140, "translucent enough for the line and arrows");
    }
}
