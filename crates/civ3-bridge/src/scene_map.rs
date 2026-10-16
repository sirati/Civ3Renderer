//! Civilization III world -> game-agnostic scene.
//!
//! This is where the game's rendering *logic* lives (which sprite a tile
//! shows); *what* the sprites are and how they are drawn is data in
//! `assets/civ3/civ3.toml`. Layers are addressed by name, sprites by the
//! game's own array names, so the two stay in step.

use render_desc::Compiled;
use scene::{Scene, View, WorldShape};

use crate::world::{Civ3View, Civ3World};

/// Resolves (layer, sprite) names to cell values once.
pub struct Mapper<'a> {
    compiled: &'a Compiled,
    terrain: Option<(usize, Vec<u32>)>,
}

impl<'a> Mapper<'a> {
    pub fn new(compiled: &'a Compiled) -> Self {
        let terrain = compiled.layer_index("terrain").map(|layer| {
            let values = (0..9 * 81).map(|i| compiled.value("terrain", &format!("Std_Terrain_Images/{i}")).unwrap_or(0)).collect();
            (layer, values)
        });
        Self { compiled, terrain }
    }

    /// Builds the scene for `world` seen through `view` in a viewport of
    /// `viewport` pixels.
    pub fn scene(&self, world: &Civ3World, view: &Civ3View, viewport: (u32, u32), tick: u32) -> Scene {
        let shape = WorldShape {
            cols: (world.width / 2).max(1) as u32,
            rows: world.height.max(1) as u32,
            wrap_x: world.wrap_x,
            wrap_y: world.wrap_y,
        };
        let mut scene = Scene::empty(shape, self.compiled.layers.len());
        scene.tick = tick;
        scene.view = View { camera_x: view.camera_x, camera_y: view.camera_y, zoom: (1, 1), viewport };

        for row in 0..shape.rows {
            for col in 0..shape.cols {
                let (x, y) = (2 * col as i32 + (row as i32 & 1), row as i32);
                let Some(tile) = world.tile(x, y) else { continue };
                if let Some((layer, values)) = &self.terrain {
                    // §5.2: the image of the vertex above the tile.
                    let sp = tile.square_parts;
                    let index = (sp >> 8) as usize * 81 + (sp & 0xFF) as usize;
                    if let Some(&v) = values.get(index) {
                        scene.set(*layer, col, row, v);
                    }
                }
            }
        }
        scene
    }
}
