//! Civilization III world -> game-agnostic scene.
//!
//! This is where the game's rendering *logic* lives (which sprite a tile
//! shows, see `rules`); *what* the sprites are and how they are drawn is data
//! in `assets/civ3/civ3.toml`. Layers are addressed by name, sprites by the
//! game's own array names, so the two stay in step.

use std::cell::RefCell;
use std::collections::HashMap;

use render_desc::Compiled;
use scene::{Scene, View, WorldShape};

use crate::rules;
use crate::world::{Civ3View, Civ3World};

/// Resolves (layer, sprite) names to cell values, caching the lookups.
pub struct Mapper<'a> {
    compiled: &'a Compiled,
    cache: RefCell<HashMap<(usize, String), u32>>,
}

impl<'a> Mapper<'a> {
    pub fn new(compiled: &'a Compiled) -> Self {
        Self { compiled, cache: RefCell::new(HashMap::new()) }
    }

    fn put(&self, scene: &mut Scene, layer: &str, col: u32, row: u32, sprite: Option<String>) {
        let (Some(index), Some(sprite)) = (self.compiled.layer_index(layer), sprite) else { return };
        let mut cache = self.cache.borrow_mut();
        let value = *cache.entry((index, sprite)).or_insert_with_key(|(_, s)| {
            self.compiled.value(layer, s).unwrap_or_else(|| {
                log::warn!("layer {layer} has no sprite {s}");
                0
            })
        });
        if value != 0 {
            scene.set(index, col, row, value);
        }
    }

    /// Builds the scene for `world` seen through `view` in a viewport of
    /// `viewport` pixels.
    pub fn scene(&self, world: &Civ3World, view: &Civ3View, viewport: (u32, u32), tick: u32) -> Scene {
        let shape = WorldShape {
            cols: (world.width / 2).max(1) as u32,
            // Vertex layers live in the tile below the vertex: one more row
            // for the vertices along the bottom edge.
            rows: (world.height + i32::from(!world.wrap_y)).max(1) as u32,
            wrap_x: world.wrap_x,
            wrap_y: world.wrap_y,
        };
        let mut scene = Scene::empty(shape, self.compiled.layers.len());
        scene.tick = tick;
        // The game draws lattice cells TileX_Min..=TileX_Max ×
        // TileY_Min..=TileY_Max only (§3), even where a tall sprite from
        // outside that range would reach into the screen.
        let cull = (view.tile_x.0 * 64, view.tile_y.0 * 32, (view.tile_x.1 + 1) * 64, (view.tile_y.1 + 1) * 32);
        scene.view = View { camera_x: view.camera_x, camera_y: view.camera_y, zoom: (1, 1), viewport, cull: Some(cull) };

        for row in 0..shape.rows {
            for col in 0..shape.cols {
                let (x, y) = (2 * col as i32 + (row as i32 & 1), row as i32);
                let tile = world.tile(x, y);
                // Vertex layers: the vertex above this tile, (x, y - 1).
                let terrain = tile.map(|t| (t.square_parts >> 8) as u32 * 81 + (t.square_parts & 0xFF) as u32);
                self.put(&mut scene, "terrain", col, row, terrain.map(|i| format!("Std_Terrain_Images/{i}")));
                self.put(&mut scene, "polar_cap", col, row, rules::polar_cap(world, x, y - 1));
                let river = rules::vertex_river(world, x, y - 1);
                self.put(&mut scene, "flood_plain", col, row, river.flood_plain);
                let layer = if river.under_objects { "rivers_under" } else { "rivers_over" };
                self.put(&mut scene, layer, col, row, river.river);
                self.put(&mut scene, "fog", col, row, rules::fog(world, x, y - 1));
                // Tile layers.
                if tile.is_none() {
                    continue;
                }
                self.put(&mut scene, "mountain", col, row, rules::mountain(world, x, y));
                self.put(&mut scene, "irrigation", col, row, rules::irrigation(world, x, y));
                self.put(&mut scene, "tnt", col, row, rules::tnt(world, x, y));
                self.put(&mut scene, "forest", col, row, rules::forest(world, x, y));
                // A border's darkening and coloured texels are drawn as two
                // layers (they do not overlap, so the order between them
                // does not matter).
                for (dir, border) in ["nw", "ne", "sw", "se"].into_iter().zip(rules::borders(world, x, y)) {
                    let Some((colors, index)) = border else { continue };
                    self.put(&mut scene, &format!("border_{dir}_shade"), col, row, Some(format!("Territory_Shade/{index}")));
                    self.put(&mut scene, &format!("border_{dir}"), col, row, Some(format!("Territory_Images@{colors}/{index}")));
                }
                let (road, rail) = rules::roads(world, x, y);
                self.put(&mut scene, "road", col, row, road);
                self.put(&mut scene, "railroad", col, row, rail);
                self.put(&mut scene, "city", col, row, rules::city(world, x, y));
                self.put(&mut scene, "victory_location", col, row, rules::victory_location(world, x, y));
                for (layer, sprite) in rules::terrain_buildings(world, x, y).into_iter().chain(rules::pollution(world, x, y)) {
                    self.put(&mut scene, layer, col, row, Some(sprite));
                }
                if let Some((shadow, icon)) = rules::resource(world, x, y) {
                    self.put(&mut scene, "resource_shadow", col, row, Some(shadow));
                    self.put(&mut scene, "resource", col, row, Some(icon));
                }
            }
        }
        scene
    }
}
