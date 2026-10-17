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
pub mod timing;
pub mod world;

pub use atlas::{Atlas, AtlasBuilder, AtlasGrowth, Blit, BlitPixels, IndexImage, RgbaImage, Shelves, SpriteId};
pub use camera::Camera;
pub use gpu::Headless;
pub use renderer::{MAX_VIEWS, Present, ScreenCopy, StagedAtlas, TileRenderer};
pub use world::{Blend, ColorModel, Dirty, GlyphShape, Grid, LayerId, LayerSpec, Layout, OutlineGlyph, Pass, PlacedSprite, SpriteSetId, SpriteSets, TileWorld, ZoomArt};
/// How sprites are drawn in views showing the world smaller
/// (`Camera::downscale`).
pub use tile_render_shared::{KEEP_SIZE, Placed, SHRINK, SHRINK_KEEP_ANCHOR, placed};
/// Indexed sprites (`SpriteDesc::palette`).
pub use tile_render_shared::{PALETTE_SIZE, remap_range};
pub use gpu_text;
