//! A GPU-driven, pixel-exact renderer for layered tile worlds.
//!
//! Game agnostic: it knows grids, layers, animations and sprites, nothing
//! about any particular game. A game (or a game's asset description) builds a
//! [`TileWorld`] and an [`Atlas`]; the renderer draws any [`Camera`] view of
//! it in one instanced draw call, with all per-cell work done on the GPU.
//!
//! See `tile_render_shared` for the GPU data model.

pub mod atlas;
pub mod camera;
pub mod gpu;
pub mod reference;
pub mod renderer;
pub mod world;

pub use atlas::{Atlas, AtlasBuilder, RgbaImage, SpriteId};
pub use camera::Camera;
pub use gpu::Headless;
pub use renderer::TileRenderer;
pub use world::{Blend, ColorModel, Grid, LayerId, LayerSpec, Layout, Pass, SpriteSetId, SpriteSets, TileWorld};
