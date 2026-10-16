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

/// The view showing what the game shows.
pub const GAME_VIEW: u32 = 0;
/// The city screen's view of the city's surroundings (while it is open).
pub const CITY_VIEW: u32 = 1;
/// The interface, over every view (no world of its own).
pub const INTERFACE_VIEW: u32 = 255;

/// Moves the screen-space sprites among the scene's sprites into overlays:
/// those drawn between render passes (map labels, under the fog of war)
/// into the game view's, the rest into the interface view's.
pub fn overlay_screen_sprites(scene: &mut Scene) {
    let (screen, world): (Vec<_>, Vec<_>) = std::mem::take(&mut scene.sprites).into_iter().partition(|s| s.screen);
    scene.sprites = world;
    let (top, under): (Vec<_>, Vec<_>) = screen.into_iter().partition(|s| s.after_pass == u32::MAX);
    scene.view_mut(GAME_VIEW).overlay.extend(under);
    let interface = scene.view_mut(INTERFACE_VIEW);
    interface.view.interface_only = true;
    interface.overlay.extend(top);
}

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

    /// Builds the scene for `world` with the game's view (`view`, in a
    /// viewport of `viewport` pixels; (0, 0): the whole window) as view
    /// [`GAME_VIEW`].
    pub fn scene(&self, world: &Civ3World, view: &Civ3View, viewport: (u32, u32), tick: u32) -> Scene {
        let shape = WorldShape {
            cols: (world.width / 2).max(1) as u32,
            // Vertex layers live in the tile below the vertex: one more row
            // for the vertices along the bottom edge, and one for the fog of
            // war the game draws below them (off the map, measured).
            rows: (world.height + 2 * i32::from(!world.wrap_y)).max(1) as u32,
            wrap_x: world.wrap_x,
            wrap_y: world.wrap_y,
        };
        let mut scene = Scene::empty(shape, self.compiled.layers.len());
        scene.tick = tick;
        // The game draws lattice cells TileX_Min..=TileX_Max ×
        // TileY_Min..=TileY_Max only (§3), even where a tall sprite from
        // outside that range would reach into the screen.
        let (sx, sy) = if view.zoomed_out { (32, 16) } else { (64, 32) };
        let cull = (view.tile_x.0 * sx, view.tile_y.0 * sy, (view.tile_x.1 + 1) * sx, (view.tile_y.1 + 1) * sy);
        // The game's view (id 0), at the window's top-left.
        scene.view_mut(GAME_VIEW).view =
            View { camera_x: view.camera_x, camera_y: view.camera_y, zoom: (1, 1), viewport, cull: Some(cull), ..Default::default() };

        for row in 0..shape.rows {
            for col in 0..shape.cols {
                let (x, y) = (2 * col as i32 + (row as i32 & 1), row as i32);
                let tile = world.tile(x, y);
                // Vertex layers: the vertex above this tile, (x, y - 1). Above
                // the top row of a map that does not wrap vertically there is
                // none: the game skips cells outside the map (§5.2; polar
                // caps fill the gap).
                let off_map = y == 0 && !world.wrap_y;
                let terrain = tile.filter(|_| !off_map).map(|t| (t.square_parts >> 8) as u32 * 81 + (t.square_parts & 0xFF) as u32);
                let array = if rules::landmark_vertex(world, x, y - 1) { "LM_Terrain_Images" } else { "Std_Terrain_Images" };
                self.put(&mut scene, "terrain", col, row, terrain.map(|i| format!("{array}/{i}")));
                self.put(&mut scene, "polar_cap", col, row, rules::polar_cap(world, x, y - 1));
                let river = if off_map { rules::VertexRiver::default() } else { rules::vertex_river(world, x, y - 1) };
                self.put(&mut scene, "flood_plain", col, row, river.flood_plain);
                let layer = if river.under_objects { "rivers_under" } else { "rivers_over" };
                self.put(&mut scene, layer, col, row, river.river);
                // (The fog of war is drawn there, measured.)
                self.put(&mut scene, "fog", col, row, rules::fog(world, x, y - 1));
                // Tile layers.
                if tile.is_none() {
                    continue;
                }
                // C3X's districts (drawn instead of the tile's buildings, and in
                // its resources' place), as slices of their art.
                if !world.districts.is_empty() {
                    let stages = crate::districts::stages(world, x, y, view.zoomed_out);
                    let slice = |d: Option<&crate::districts::DistrictDraw>| d.map(|d| crate::district_art::Slice::of(d, view.zoomed_out).sprite());
                    for (i, layer) in crate::district_art::LAYERS.iter().enumerate() {
                        self.put(&mut scene, layer, col, row, slice(stages.buildings.get(i)));
                    }
                    for (i, layer) in crate::district_art::RESOURCE_LAYERS.iter().enumerate() {
                        self.put(&mut scene, layer, col, row, slice(stages.resources.get(i)));
                    }
                }
                self.put(&mut scene, "mountain", col, row, rules::mountain(world, x, y));
                self.put(&mut scene, "irrigation", col, row, rules::irrigation(world, x, y));
                self.put(&mut scene, "tnt", col, row, rules::tnt(world, x, y));
                // C3X's draw_forests_over_roads_and_railroads: a forest with a road
                // is drawn after the road instead, and again after the railroad.
                let (road, rail) = rules::roads(world, x, y);
                let forest = rules::forest(world, x, y);
                let over_roads = world.district_configs.flags.forests_over_roads && tile.is_some_and(|t| t.terrain() == rules::terrain::FOREST) && road.is_some();
                self.put(&mut scene, "forest", col, row, forest.clone().filter(|_| !over_roads));
                // A border's darkening and coloured texels are drawn as two
                // layers (they do not overlap, so the order between them
                // does not matter).
                for (dir, border) in ["nw", "ne", "sw", "se"].into_iter().zip(rules::borders(world, x, y)) {
                    let Some((colors, index)) = border else { continue };
                    self.put(&mut scene, &format!("border_{dir}_shade"), col, row, Some(format!("Territory_Shade/{index}")));
                    self.put(&mut scene, &format!("border_{dir}"), col, row, Some(format!("Territory_Images@{colors}/{index}")));
                }
                self.put(&mut scene, "road", col, row, road);
                self.put(&mut scene, "forest_over_road", col, row, forest.clone().filter(|_| over_roads));
                self.put(&mut scene, "railroad", col, row, rail.clone());
                self.put(&mut scene, "forest_over_railroad", col, row, forest.filter(|_| over_roads && rail.is_some()));
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

/// World-space free sprites (units and their HUD) in the zoomed-out view:
/// at half their positions, as the game draws them (§3; their art is
/// halved by the description, except what stays full size: the HUD, which
/// `units::sprites` spreads for it).
pub fn zoom_out(sprites: &mut [scene::FreeSprite]) {
    for s in sprites.iter_mut().filter(|s| !s.screen) {
        s.x /= 2;
        s.y /= 2;
    }
}

/// The city screen is open: the game view is hidden (the screen covers it)
/// and the city's surroundings show in `rect` (window pixels: x, y, w, h)
/// as the game frames them (its camera at `camera`), with the map labels.
pub fn city_view(scene: &mut Scene, camera: (i32, i32), [x, y, w, h]: [i32; 4]) {
    let game = scene.view_mut(GAME_VIEW);
    game.view.interface_only = true;
    let labels = std::mem::take(&mut game.overlay);
    let city = scene.view_mut(CITY_VIEW);
    city.view = View {
        camera_x: camera.0 + x,
        camera_y: camera.1 + y,
        zoom: (1, 1),
        viewport: (w as u32, h as u32),
        origin: (x, y),
        ..Default::default()
    };
    city.overlay = labels;
}

/// The tile whose diamond holds world pixel (wx, wy) (tile (x, y), x + y
/// even, has its 128×64 footprint at (64·x, 32·y); half that zoomed out),
/// wrapped where the map wraps; `None` off the map.
pub fn tile_at(world: &Civ3World, (wx, wy): (i32, i32), zoomed_out: bool) -> Option<(i32, i32)> {
    let (wx, wy) = if zoomed_out { (2 * wx, 2 * wy) } else { (wx, wy) };
    let (cx, cy) = ((wx - 64).div_euclid(64), (wy - 32).div_euclid(32));
    let mut best = None;
    for y in cy - 1..=cy + 2 {
        for x in cx - 1..=cx + 2 {
            if (x + y).rem_euclid(2) != 0 {
                continue;
            }
            // Diamond distance from the tile's centre (≤ 1 inside it).
            let d = (wx - (64 * x + 64)).abs() * 32 + (wy - (32 * y + 32)).abs() * 64;
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, (x, y)));
            }
        }
    }
    let (_, (x, y)) = best?;
    let x = if world.wrap_x { x.rem_euclid(world.width.max(1)) } else { x };
    let y = if world.wrap_y { y.rem_euclid(world.height.max(1)) } else { y };
    world.tile(x, y).map(|_| (x, y))
}
