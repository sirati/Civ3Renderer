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
pub use camera::{BackgroundSampling, Camera, Lens};
pub use gpu::Headless;
pub use renderer::{GlyphCacheStats, AtlasTransferError, AtlasTransferProgress, AtlasTransferSource, MAX_VIEWS, Present, ScreenCopy, StagedAtlas, TileRenderer};
pub use world::{ALL_CELLS, Blend, ColorModel, Dirty, GlyphShape, Grid, LayerId, LayerSpec, Layout, OutlineGlyph, Pass, PlacedSprite, SolidRect, SHARED_BINDING, SlotId, SpriteSetId, SpriteSets, TileWorld, ZoomArt};
/// How sprites are drawn in views showing the world smaller
/// (`Camera::downscale`).
pub use tile_render_shared::{BLANK_EMPTY, KEEP_SIZE, Placed, SHRINK, SHRINK_KEEP_ANCHOR, placed};
/// Per-camera clean-map category bits and indexed-sprite palette helpers.
pub use tile_render_shared::{
    CATEGORY_FULL_RESOLUTION, CATEGORY_REDUCED_RESOLUTION, CATEGORY_HALF_RESOLUTION, CATEGORY_NOT_HALF_RESOLUTION, CATEGORY_ALL, CATEGORY_CITIES, CATEGORY_FORESTS_JUNGLES, CATEGORY_HILLS, CATEGORY_IRRIGATION, CATEGORY_MOUNTAINS,
    CATEGORY_POLLUTION, CATEGORY_RESOURCES, CATEGORY_RIVERS_FLOODPLAINS, CATEGORY_ROADS_RAILROADS, CATEGORY_TERRAIN_BUILDINGS,
    CATEGORY_TERRITORY_BOUNDARIES, CATEGORY_UNITS, CATEGORY_YIELD_ICONS, PALETTE_SIZE, remap_range,
};
pub use gpu_text;
