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
pub mod effect;
pub mod gpu;
pub mod reference;
pub mod renderer;
pub mod timing;
pub mod world;

pub use atlas::{Atlas, AtlasBuilder, AtlasGrowth, Blit, BlitPixels, IndexImage, RgbaImage, Shelves, SpriteId};
pub use camera::{BackgroundSampling, Camera};
pub use effect::Effect;
pub use gpu::Headless;
pub use renderer::{GlyphCacheStats, Shaders, AtlasTransferError, AtlasTransferProgress, AtlasTransferSource, MAX_VIEWS, Present, ScreenCopy, StagedAtlas, TileRenderer};
pub use world::{ALL_CELLS, Blend, ColorModel, Dirty, GlyphShape, Grid, LayerId, LayerSpec, Layout, OutlineGlyph, OutputRect, PaintedShape, Pass, PlacedSprite, Shape, SolidRect, SHARED_BINDING, SlotId, SpriteSetId, SpriteSets, TileWorld, ZoomArt};
/// How sprites are drawn in views showing the world smaller
/// (`Camera::downscale`).
pub use tile_render_shared::{BLANK_EMPTY, HOST_CLOCK, KEEP_SIZE, Placed, SHRINK, SHRINK_KEEP_ANCHOR, layer_frame_of, placed};
/// Per-camera category bits and indexed-sprite palette helpers.
pub use tile_render_shared::{
    CATEGORY_FULL_RESOLUTION, CATEGORY_REDUCED_RESOLUTION, CATEGORY_HALF_RESOLUTION, CATEGORY_NOT_HALF_RESOLUTION, CATEGORY_RESERVED,
    PALETTE_SIZE, remap_range,
};
pub use gpu_text;
