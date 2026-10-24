//! The Civilization III world: the map and what is on it, from the game's
//! state to the scene, drawn on `tile-render`.
//!
//! It does not read game memory. Most of the world still lives in
//! `civ3-bridge` (`world.rs`, `scene_map.rs`, `units.rs`, ...), held there
//! by its world model's game types; it moves here as they come apart
//! (`docs/jgl-replacement.md` §2).

#![forbid(unsafe_code)]

pub mod revealed;
