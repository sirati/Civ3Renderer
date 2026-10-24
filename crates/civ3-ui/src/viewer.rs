//! What the Civilization III viewer plugs into the generic viewer
//! (`tile_viewer::extension`): every host of the viewer (in the game's
//! process, natively under Wine, standalone) runs with these.

/// The extensions the viewer runs with.
pub fn extensions() -> tile_viewer::extension::Extensions {
    tile_viewer::extension::Extensions::default().with(crate::production_queue::viewer_extension)
}
