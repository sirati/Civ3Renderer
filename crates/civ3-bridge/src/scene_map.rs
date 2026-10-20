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

/// How far the map's sprites reach beyond their cells (left, top, right,
/// bottom; see `tile_render::Camera::overhang`): tall art (mountains,
/// cities) up, units' 240×240 frames around their tiles' centres up, down
/// and to the sides, an army's banner 40 px further right (§10.11). Views
/// without the game's cull rectangle draw the cells that far outside.
pub const OVERHANG: (u32, u32, u32, u32) = (128, 128, 128, 128);

/// The view showing what the game shows.
pub const GAME_VIEW: u32 = 0;
/// The city screen's view of the city's surroundings (while it is open).
pub const CITY_VIEW: u32 = 1;
/// The binding of [`CITY_VIEW`] (`scene::View::binding`): the city screen's
/// fog of war, in slots of the fog layers ([`CITY_FOG_SLOTS`]), while every
/// other view keeps the player's (docs/per-view-layers.md).
pub const CITY_BINDING: u32 = 1;
/// The slots of the city screen's fog: (layer, slot id).
pub const CITY_FOG_SLOTS: [(&str, u32); 2] = [("fog", 1), ("fog_edge", 2)];
/// The game's frame (its main screen's panels and buttons), over every
/// view (no world of its own).
pub const INTERFACE_VIEW: u32 = 254;
/// The menus over the frame: the game's other screens (the popup has a
/// view of its own, `interface_parts::POPUP_VIEW`, over this one).
pub const MENU_VIEW: u32 = 255;

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

/// Puts the menus' screen sprites into the menu view (over the frame;
/// those drawn under the fog of war, if any, into the game view's overlay);
/// no menu view without menus.
pub fn overlay_menus(scene: &mut Scene, sprites: Vec<scene::FreeSprite>) {
    let (top, under): (Vec<_>, Vec<_>) = sprites.into_iter().partition(|s| s.after_pass == u32::MAX);
    scene.view_mut(GAME_VIEW).overlay.extend(under);
    if top.is_empty() {
        return;
    }
    let menus = scene.view_mut(MENU_VIEW);
    menus.view.interface_only = true;
    menus.view.menu = true;
    menus.overlay.extend(top);
}

/// Resolves (layer, sprite) names to cell values, caching the lookups.
pub struct Mapper<'a> {
    compiled: &'a Compiled,
    cache: RefCell<HashMap<(usize, String, u32), u32>>,
}

impl<'a> Mapper<'a> {
    pub fn new(compiled: &'a Compiled) -> Self {
        Self { compiled, cache: RefCell::new(HashMap::new()) }
    }

    fn put(&self, scene: &mut Scene, layer: &str, col: u32, row: u32, sprite: Option<String>) {
        self.put_in(scene, layer, col, row, sprite, 0);
    }

    /// [`Mapper::put`], the layer's indexed sprites through named palette `palette`.
    fn put_in(&self, scene: &mut Scene, layer: &str, col: u32, row: u32, sprite: Option<String>, palette: u32) {
        if let Some((index, value)) = self.value(layer, sprite, palette) {
            scene.set(index, col, row, value);
        }
    }

    /// The layer's index and the value drawing `sprite` on it (through
    /// named palette `palette`), unless it draws nothing.
    fn value(&self, layer: &str, sprite: Option<String>, palette: u32) -> Option<(usize, u32)> {
        let (Some(index), Some(sprite)) = (self.compiled.layer_index(layer), sprite) else { return None };
        let mut cache = self.cache.borrow_mut();
        let value = *cache.entry((index, sprite, palette)).or_insert_with_key(|(_, s, p)| {
            self.compiled.value_in(layer, s, *p).unwrap_or_else(|| {
                log::warn!("layer {layer} has no sprite {s}");
                0
            })
        });
        (value != 0).then_some((index, value))
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
        let mut scene = Scene::empty_with(shape, self.compiled.layers.iter().map(|l| (l.per_cell_phase, l.per_cell_offset)));
        scene.tick = tick;
        // The game draws lattice cells TileX_Min..=TileX_Max ×
        // TileY_Min..=TileY_Max only (§3), even where a tall sprite from
        // outside that range would reach into the screen.
        let (sx, sy) = if view.zoomed_out { (32, 16) } else { (64, 32) };
        let cull = (view.tile_x.0 * sx, view.tile_y.0 * sy, (view.tile_x.1 + 1) * sx, (view.tile_y.1 + 1) * sy);
        // The game's view (id 0), at the window's top-left; zoomed out the
        // same art at half size (§3), drawn so by the renderer.
        let downscale = if view.zoomed_out { 2 } else { 1 };
        scene.view_mut(GAME_VIEW).view =
            View { camera_x: view.camera_x, camera_y: view.camera_y, zoom: (1, 1), viewport, cull: Some(cull), downscale, hidden_categories: world.clean_map, ..Default::default() };

        for row in 0..shape.rows {
            for col in 0..shape.cols {
                let (x, y) = (2 * col as i32 + (row as i32 & 1), row as i32);
                let tile = world.tile(x, y);
                // Vertex layers: the vertex above this tile, (x, y - 1). Above
                // the top row of a map that does not wrap vertically there is
                // none: the game skips cells outside the map (§5.2; polar
                // caps fill the gap).
                let off_map = y == 0 && !world.wrap_y;
                let terrain = if off_map { None } else { rules::terrain_vertex(world, x, y - 1) };
                let array = if rules::landmark_vertex(world, x, y - 1) { "LM_Terrain_Images" } else { "Std_Terrain_Images" };
                self.put(&mut scene, "terrain", col, row, terrain.map(|i| format!("{array}/{i}")));
                self.put(&mut scene, "polar_cap", col, row, rules::polar_cap(world, x, y - 1));
                let river = if off_map { rules::VertexRiver::default() } else { rules::vertex_river(world, x, y - 1) };
                self.put(&mut scene, "flood_plain", col, row, river.flood_plain);
                let layer = if river.under_objects { "rivers_under" } else { "rivers_over" };
                self.put(&mut scene, layer, col, row, river.river);
                // (The fog of war is drawn there, measured: the player's;
                // a city screen's in slots of its own, `city_fog`.)
                self.put(&mut scene, "fog", col, row, rules::fog_with(world, None, x, y - 1));
                // Tile layers.
                if tile.is_none() {
                    continue;
                }
                // C3X's districts (drawn instead of the tile's buildings, and in
                // its resources' place), as slices of their art.
                if !world.districts.is_empty() {
                    let stages = crate::districts::stages(world, x, y, view.zoomed_out);
                    let slice = |d: Option<&crate::districts::DistrictDraw>| d.map(|d| crate::district_art::Slice::of(d).sprite());
                    // A district drawn over a resource must not merge its
                    // resource/shadow into the building category. Preserve
                    // their original order in separate resource layers.
                    let separate = self.compiled.layer_index(crate::district_art::UNDER_RESOURCE_LAYERS[0]).is_some();
                    let before = if separate { stages.building_resources } else { 0 };
                    for (i, layer) in crate::district_art::UNDER_RESOURCE_LAYERS.iter().enumerate().filter(|(i, _)| *i < before) {
                        self.put(&mut scene, layer, col, row, slice(stages.buildings.get(i)));
                    }
                    for (i, layer) in crate::district_art::LAYERS.iter().enumerate() {
                        self.put(&mut scene, layer, col, row, slice(stages.buildings.get(before + i)));
                    }
                    for (i, layer) in crate::district_art::RESOURCE_LAYERS.iter().enumerate() {
                        self.put(&mut scene, layer, col, row, slice(stages.resources.get(i)));
                    }
                }
                let mountain_layer = if tile.is_some_and(|t| t.terrain() == rules::terrain::HILLS) && self.compiled.layer_index("hills").is_some() { "hills" } else { "mountain" };
                self.put(&mut scene, mountain_layer, col, row, rules::mountain(world, x, y));
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
                    let palette = self.compiled.palette(&format!("ntp@{colors}")).unwrap_or(0);
                    self.put_in(&mut scene, &format!("border_{dir}"), col, row, Some(format!("Territory_Images/{index}")), palette);
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
        self.edge_fog(&mut scene, world, None);
        self.city_fog(&mut scene, world);
        scene
    }

    /// The fog sweep visits odd vertices outside the map too, unlike the
    /// terrain sweep. One alternating half-diamond along each non-wrapping
    /// side belongs to a vertex the tile grid cannot represent. Store it in
    /// the adjacent edge cell with an offset, so it also repeats correctly
    /// on maps that wrap vertically without changing the map's dimensions.
    /// As the city screen of city `focus` shows it (`None`: the player's).
    fn edge_fog(&self, scene: &mut Scene, world: &Civ3World, focus: Option<i32>) {
        if scene.shape.cols == 0 { return; }
        let Some(layer) = self.compiled.layer_index("fog_edge").filter(|_| !world.wrap_x) else { return };
        for row in 0..scene.shape.rows {
            let (col, vx, dx) = if row & 1 != 0 {
                (0, -1, -128)
            } else {
                (scene.shape.cols - 1, world.width, 128)
            };
            self.put(scene, "fog_edge", col, row, rules::fog_with(world, focus, vx, row as i32 - 1));
            scene.set_offset(layer, col, row, (dx, 0));
        }
    }

    /// The open city screen's fog of war (its city's own tiles lit, the
    /// rest of its radius dimmed; `rules::fog`) for [`CITY_VIEW`] only:
    /// the fog layers' cells as it shows them, in slots bound to
    /// [`CITY_BINDING`], where they differ from the player's fog, which
    /// every other view keeps. None without a city in focus. (Only the fog
    /// differs: the city screen shows the player's world otherwise.)
    fn city_fog(&self, scene: &mut Scene, world: &Civ3World) {
        let Some(focus) = world.focus_city.filter(|&id| world.city(id).is_some()) else { return };
        let layers: Vec<(usize, u32)> = CITY_FOG_SLOTS.iter().filter_map(|&(name, slot)| self.compiled.layer_index(name).map(|l| (l, slot))).collect();
        // The fog layers alone, as the city screen shows them.
        let mut lit = Scene { shape: scene.shape, layers: vec![scene::LayerCells::default(); scene.layers.len()], ..Default::default() };
        let n = (scene.shape.cols * scene.shape.rows) as usize;
        for &(l, _) in &layers {
            let own = &scene.layers[l];
            lit.layers[l] = scene::LayerCells { values: vec![0; n], phases: own.phases.as_ref().map(|_| vec![0; n]), offsets: own.offsets.as_ref().map(|_| vec![(0, 0); n]) };
        }
        for row in 0..scene.shape.rows {
            for col in 0..scene.shape.cols {
                let (x, y) = (2 * col as i32 + (row as i32 & 1), row as i32);
                self.put(&mut lit, "fog", col, row, rules::fog_with(world, Some(focus), x, y - 1));
            }
        }
        self.edge_fog(&mut lit, world, Some(focus));
        let mut binding = scene::Binding { id: CITY_BINDING, slots: Vec::new() };
        for (l, id) in layers {
            let cells = std::mem::take(&mut lit.layers[l]);
            if cells != scene.layers[l] {
                scene.slots.push(scene::LayerSlot { id, layer: l as u32, cells });
                binding.slots.push((l as u32, id));
            }
        }
        if !binding.slots.is_empty() {
            scene.bindings.push(binding);
        }
    }
}

/// World-space free sprites placed in the pixels of the zoomed-out view
/// (the city labels, laid out there, §10.8) at full size: the renderer
/// halves world positions again for the view showing the world at half
/// size (`View::downscale`), as the game draws everything zoomed out (§3).
pub fn from_zoomed_out(sprites: &mut [scene::FreeSprite]) {
    for s in sprites.iter_mut().filter(|s| !s.screen) {
        s.x *= 2;
        s.y *= 2;
    }
}

/// The city screen is open: the game view is hidden (the screen covers it)
/// and the city's surroundings show in `rect` (window pixels: x, y, w, h)
/// through a camera at `camera` (the window's top-left, full-size world
/// pixels: `city_screen::camera`), with the map labels. The city screen
/// always shows the world at its normal size (`City_Form_open` forces the
/// normal zoom), whatever the game view was.
pub fn city_view(scene: &mut Scene, camera: (i32, i32), [x, y, w, h]: [i32; 4]) {
    let game = scene.view_mut(GAME_VIEW);
    game.view.interface_only = true;
    let labels = std::mem::take(&mut game.overlay);
    let downscale = 1;
    let city = scene.view_mut(CITY_VIEW);
    city.view = View {
        camera_x: camera.0 + x,
        camera_y: camera.1 + y,
        zoom: (1, 1),
        viewport: (w as u32, h as u32),
        origin: (x, y),
        downscale,
        // Its own fog of war (`Mapper::city_fog`).
        binding: CITY_BINDING,
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

#[cfg(test)]
mod clean_map_tests {
    use super::*;
    use crate::world::Civ3Tile;
    use viewer_settings::clean_map;

    #[test]
    fn every_game_map_category_is_tagged_and_fog_cannot_be_hidden() {
        let desc = render_desc::Description::parse(crate::assets::BUILTIN_DESC).unwrap();
        let category = |name: &str| desc.layers.iter().find(|l| l.name == name).unwrap().category;
        for (name, expected) in [
            ("flood_plain", clean_map::RIVERS), ("rivers_under", clean_map::RIVERS), ("rivers_over", clean_map::RIVERS),
            ("mountain", clean_map::MOUNTAINS), ("hills", clean_map::HILLS),
            ("forest", clean_map::FORESTS), ("forest_over_road", clean_map::FORESTS), ("forest_over_railroad", clean_map::FORESTS),
            ("mine", clean_map::BUILDINGS), ("district_0", clean_map::BUILDINGS), ("colony", clean_map::BUILDINGS),
            ("road", clean_map::ROADS), ("railroad", clean_map::ROADS), ("irrigation", clean_map::IRRIGATION),
            ("resource", clean_map::RESOURCES), ("resource_shadow", clean_map::RESOURCES), ("district_resource_0", clean_map::RESOURCES), ("district_under_resource_0", clean_map::RESOURCES), ("tnt", clean_map::YIELDS),
            ("pollution", clean_map::POLLUTION), ("craters", clean_map::POLLUTION), ("border_nw", clean_map::BOUNDARIES),
            ("border_nw_shade", clean_map::BOUNDARIES), ("city", clean_map::CITIES), ("victory_location", clean_map::CITIES), ("unit_disc", clean_map::UNITS),
        ] { assert_eq!(category(name), expected, "{name}"); }
        for name in ["terrain", "polar_cap", "fog", "fog_edge"] { assert_eq!(category(name), 0, "{name}"); }
        for layer in desc.layers.iter().filter(|l| l.name.starts_with("unit_")) {
            assert_eq!(layer.category, clean_map::UNITS, "{}", layer.name);
        }
        let mountains = desc.layers.iter().find(|l| l.name == "mountain").unwrap();
        assert!(mountains.sets.iter().all(|s| !s.contains("Hills")));
    }

    #[test]
    fn clean_map_keeps_all_cells_and_hills_are_independent_of_mountains() {
        struct NoFiles;
        impl render_desc::ImageLoader for NoFiles {
            fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> { panic!("fixture is generated"); }
        }
        let compiled = render_desc::Description::parse(r#"
            [grid]
            layout = "staggered"
            step = [64, 32]
            [images.hills]
            source = "solid:100,180,40,255:16x1"
            [images.mountains]
            source = "solid:180,180,180,255:16x1"
            [sheets.hills]
            image = "hills"
            cell = [1, 1]
            grid = [16, 1]
            group = "Hills_Images"
            [sheets.mountains]
            image = "mountains"
            cell = [1, 1]
            grid = [16, 1]
            group = "Mountains_Images"
            [[layers]]
            name = "mountain"
            category = 2
            sets = ["Mountains_Images/*"]
            [[layers]]
            name = "hills"
            category = 4
            sets = ["Hills_Images/*"]
        "#).unwrap().compile(&NoFiles, 256).unwrap();
        let tile = |terrain| Civ3Tile { square_type: (terrain << 12) | (rules::terrain::GRASSLAND << 8), ..Default::default() };
        let mut world = Civ3World { width: 4, height: 2, tiles: vec![tile(rules::terrain::HILLS), tile(rules::terrain::MOUNTAINS), tile(rules::terrain::GRASSLAND), tile(rules::terrain::GRASSLAND)], ..Default::default() };
        let mapper = Mapper::new(&compiled);
        let normal = mapper.scene(&world, &Civ3View::default(), (100, 100), 0);
        world.clean_map = clean_map::ALL;
        let clean = mapper.scene(&world, &Civ3View::default(), (100, 100), 0);
        assert_eq!(normal.layers, clean.layers, "each camera filters its own immutable world");
        assert_eq!(clean.view(GAME_VIEW).unwrap().view.hidden_categories, clean_map::ALL);
        let hills = &clean.layers[compiled.layer_index("hills").unwrap()].values;
        let mountain = &clean.layers[compiled.layer_index("mountain").unwrap()].values;
        assert!(hills[0] > 0 && hills[1] == 0);
        assert!(mountain[0] == 0 && mountain[1] > 0);
    }
}


#[cfg(test)]
mod edge_fog_tests {
    use super::*;
    use crate::world::Civ3Tile;
    use tile_render::{Blend, Camera, PlacedSprite, RgbaImage};

    fn fog_description() -> render_desc::Description {
        let mut desc = render_desc::Description::parse(crate::assets::BUILTIN_DESC).unwrap();
        desc.images.retain(|name, _| name == "fog");
        desc.sheets.retain(|name, _| name == "fog");
        desc.layers.retain(|layer| matches!(layer.name.as_str(), "fog" | "fog_edge"));
        desc.fonts.clear();
        desc.palettes.clear();
        desc.sets.clear();
        desc
    }

    struct FakeFog;
    impl render_desc::ImageLoader for FakeFog {
        fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<RgbaImage>, String> {
            Ok(vec![RgbaImage::from_fn(1152, 576, |_, _| [9, 0, 0, 255])])
        }
    }

    #[test]
    fn a_city_screen_in_one_view_leaves_the_players_fog_in_every_other() {
        use crate::world::Civ3City;
        let compiled = fog_description().compile(&FakeFog, 1024).unwrap();
        let mapper = Mapper::new(&compiled);
        // Everything seen by player 1; the city at (8, 8) owns the tiles
        // west of x = 8.
        let mut world = Civ3World { player: 1, width: 20, height: 20, wrap_x: true, ..Default::default() };
        world.tiles = (0..10 * 20).map(|i| Civ3Tile { explored: 2, visibility: 2, owner: if (i % 10) * 2 < 8 { 1 } else { 0 }, ..Default::default() }).collect();
        world.cities.push(Civ3City { id: 0, x: 8, y: 8, civ: 1, ..Default::default() });
        let plain = mapper.scene(&world, &Civ3View::default(), (640, 320), 0);
        assert!(plain.slots.is_empty() && plain.bindings.is_empty(), "no screen of its own: one fog, shared");
        world.focus_city = Some(0);
        let mut open = mapper.scene(&world, &Civ3View::default(), (640, 320), 0);
        let camera = crate::city_screen::camera((8, 8), (1280, 640));
        city_view(&mut open, camera, [100, 50, 640, 320]);
        // Every view but the city screen's: the player's fog, unchanged.
        assert_eq!(open.layers, plain.layers);
        assert_eq!(open.view(GAME_VIEW).unwrap().view.binding, 0);
        assert_eq!(open.view(CITY_VIEW).unwrap().view.binding, CITY_BINDING);
        // The city screen's: the game's city-screen fog, in a slot.
        let fog = compiled.layer_index("fog").unwrap();
        assert_eq!(open.bindings, [scene::Binding { id: CITY_BINDING, slots: vec![(fog as u32, CITY_FOG_SLOTS[0].1)] }], "the edge fog does not differ on a map that wraps");
        let slot = &open.slots[0];
        for row in 0..open.shape.rows {
            for col in 0..open.shape.cols {
                let (x, y) = (2 * col as i32 + (row as i32 & 1), row as i32);
                let want = rules::fog(&world, x, y - 1).map_or(0, |s| compiled.value("fog", &s).unwrap());
                assert_eq!(slot.cells.values[(row * open.shape.cols + col) as usize], want, "({col}, {row})");
            }
        }
        // Drawn: a view of the city binding shows the city's fog, one of
        // the shared binding (the second pane, B) the player's.
        let map = tile_viewer::scene_world(&compiled, &open);
        let cameras = tile_viewer::scene_cameras(&open, (1280, 640), OVERHANG);
        let city = cameras[open.views.iter().position(|v| v.id == CITY_VIEW).unwrap()];
        let b = Camera { binding: tile_render::SHARED_BINDING, ..city };
        let render = |map: &tile_render::TileWorld, camera: &Camera| tile_render::reference::render(map, &compiled.atlas, camera, 0, [192, 160, 128, 255]);
        let player = render(&tile_viewer::scene_world(&compiled, &plain), &b);
        assert_eq!(render(&map, &b).as_bytes(), player.as_bytes(), "B's fog equals the normal fog");
        let mut old = plain.clone();
        old.layers[fog] = slot.cells.clone();
        let city_fog = render(&tile_viewer::scene_world(&compiled, &old), &b);
        assert_ne!(city_fog.as_bytes(), player.as_bytes());
        assert_eq!(render(&map, &city).as_bytes(), city_fog.as_bytes(), "A shows the city screen's dimming");
        // A map that does not wrap: the edge fog's slot too.
        world.wrap_x = false;
        let open = mapper.scene(&world, &Civ3View::default(), (640, 320), 0);
        assert_eq!(open.slots.iter().map(|s| s.id).collect::<Vec<_>>(), [1, 2]);
        world.focus_city = None;
        assert_eq!(open.layers, mapper.scene(&world, &Civ3View::default(), (640, 320), 0).layers);
    }

    #[test]
    fn nonwrapping_sides_keep_the_missing_fog_vertices_without_resizing_the_map() {
        let compiled = fog_description().compile(&FakeFog, 1024).unwrap();
        let edge = compiled.layer_index("fog_edge").unwrap();
        let mapper = Mapper::new(&compiled);
        let mut world = Civ3World { player: 1, width: 8, height: 8, tiles: vec![Civ3Tile { explored: 2, ..Default::default() }; 32], ..Default::default() };
        for wrap_y in [false, true] {
            world.wrap_y = wrap_y;
            let scene = mapper.scene(&world, &Civ3View::default(), (512, 256), 0);
            assert_eq!((scene.shape.cols, scene.shape.rows), (4, 8 + 2 * u32::from(!wrap_y)));
            for row in 0..scene.shape.rows {
                let col = if row & 1 == 0 { 3 } else { 0 };
                let vy = row as i32 - 1;
                let digit = if wrap_y || (0..8).contains(&vy) { 1 } else { 0 };
                let index = digit * 40;
                let value = compiled.value("fog_edge", &format!("Fog_Of_War_Images/{index}")).unwrap();
                for c in 0..4 {
                    assert_eq!(scene.layers[edge].values[(row * 4 + c) as usize], if c == col { value } else { 0 }, "row {row}, col {c}, wrap y {wrap_y}");
                }
                assert_eq!(scene.layers[edge].offsets.as_ref().unwrap()[(row * 4 + col) as usize], (if row & 1 == 0 { 128 } else { -128 }, 0));
            }
        }
        world.wrap_x = true;
        let wrapped = mapper.scene(&world, &Civ3View::default(), (512, 256), 0);
        assert!(wrapped.layers[edge].values.iter().all(|&value| value == 0), "wrapping grid cells already provide these vertices");
    }

    /// Compare the mapped grid with a complete fog-vertex sweep using
    /// logical edge visibility and the installed FogOfWar.pcx. Expected
    /// colours come from actual tile states, never native border pixels.
    #[test]
    fn edge_brightness_matches_a_complete_logical_fog_sweep_with_real_art() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        if !std::path::Path::new(&install).is_dir() {
            return eprintln!("no Civ III install, skipped");
        }
        let loader = civ3_assets::loader::Civ3Loader::new(civ3_assets::GameFiles::complete(&install));
        let compiled = fog_description().compile(&loader, 1024).unwrap();
        let edge = compiled.layer_index("fog_edge").unwrap();
        let mapper = Mapper::new(&compiled);
        let mut world = Civ3World { player: 1, width: 8, height: 10, ..Default::default() };
        world.tiles = (0..40).map(|i| {
            let state = (i / 4 + i % 4) % 3;
            Civ3Tile { explored: if state == 0 { 0 } else { 2 }, visibility: if state == 2 { 2 } else { 0 }, ..Default::default() }
        }).collect();
        for wrap_x in [false, true] {
            for wrap_y in [false, true] {
                world.wrap_x = wrap_x;
                world.wrap_y = wrap_y;
                let scene = mapper.scene(&world, &Civ3View::default(), (576, 352), 0);
                let actual = tile_viewer::scene_world(&compiled, &scene);
                let mut expected = compiled.world(scene.shape.cols, scene.shape.rows, wrap_x, wrap_y);
                // The native pass iterates every odd vertex in its draw
                // rectangle. Bounds checks apply to its four neighbours,
                // not to the vertex itself (0x4C5148..0x4C551A).
                for vy in -1..=world.height {
                    for vx in -1..=world.width {
                        if (vx + vy) & 1 == 0 { continue; }
                        let corners = [(vx, vy - 1), (vx - 1, vy), (vx + 1, vy), (vx, vy + 1)].map(|(x, y)| {
                            world.tile(x, y).map(|tile| if tile.visibility & 2 != 0 { 2 } else { i32::from(tile.explored & 2 != 0) })
                        });
                        let fallback = corners.into_iter().flatten().next().unwrap_or(0);
                        let opposite = [3, 2, 1, 0];
                        let index: i32 = (0..4).zip([1, 3, 9, 27]).map(|(k, weight)| corners[k].or(corners[opposite[k]]).unwrap_or(fallback) * weight).sum();
                        if index == 80 { continue; }
                        let mut sprite = PlacedSprite::new(compiled.set(&format!("Fog_Of_War_Images/{index}")).unwrap().0, vx * 64, (vy + 1) * 32);
                        sprite.blend = Blend::Scale { denominator: 16 };
                        expected.free.push(sprite);
                    }
                }
                let mut unfixed = actual.clone();
                for row in 0..scene.shape.rows {
                    for col in 0..scene.shape.cols {
                        unfixed.set(tile_render::LayerId(edge), col, row, 0);
                    }
                }
                for downscale in [1, 2, 4] {
                    for zoom in [(1, 1), (2, 1)] {
                        let camera = Camera { downscale, zoom, overhang: OVERHANG, ..Camera::new((576 / downscale, 352 / downscale)) };
                        let render = |map: &tile_render::TileWorld| tile_render::reference::render(map, &compiled.atlas, &camera, 0, [192, 160, 128, 255]);
                        let oracle = render(&expected);
                        let fixed = render(&actual);
                        assert_eq!(fixed.as_bytes(), oracle.as_bytes(), "wrap ({wrap_x}, {wrap_y}), downscale {downscale}, zoom {zoom:?}");
                        if !wrap_x {
                            assert_ne!(render(&unfixed).as_bytes(), oracle.as_bytes(), "the previous missing edge fog must reproduce the brightness error");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn real_fog_art_keeps_uniform_brightness_on_all_edge_half_tiles() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        if !std::path::Path::new(&install).is_dir() { return eprintln!("no Civ III install, skipped"); }
        let loader = civ3_assets::loader::Civ3Loader::new(civ3_assets::GameFiles::complete(&install));
        let compiled = fog_description().compile(&loader, 1024).unwrap();
        let mapper = Mapper::new(&compiled);
        let mut world = Civ3World { player: 1, width: 8, height: 10, tiles: vec![Civ3Tile::default(); 40], ..Default::default() };
        for (visibility, expected) in [(0, [107, 90, 74, 255]), (2, [198, 165, 132, 255])] {
            world.tiles.fill(Civ3Tile { explored: 2, visibility, ..Default::default() });
            for wrap_x in [false, true] {
                for wrap_y in [false, true] {
                    world.wrap_x = wrap_x;
                    world.wrap_y = wrap_y;
                    let scene = mapper.scene(&world, &Civ3View::default(), (576, 352), 0);
                    let map = tile_viewer::scene_world(&compiled, &scene);
                    for downscale in [1, 2, 4] {
                        let camera = Camera { downscale, overhang: OVERHANG, hidden_categories: viewer_settings::clean_map::ALL, ..Camera::new((576 / downscale, 352 / downscale)) };
                        let image = tile_render::reference::render(&map, &compiled.atlas, &camera, 0, [192, 160, 128, 255]);
                        for y in 0..10 {
                            for col in 0..4 {
                                let x = 2 * col + (y & 1);
                                for (dx, dy) in [(-48, 0), (48, 0), (0, -16), (0, 16)] {
                                    let (px, py) = ((64 * x + 64 + dx) as u32 / downscale, (32 * y + 32 + dy) as u32 / downscale);
                                    assert_eq!(image.get(px, py), expected, "tile ({x}, {y}), sample ({dx}, {dy}), wrap ({wrap_x}, {wrap_y}), downscale {downscale}, visible {visibility}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn real_inland_edge_art_has_no_invented_coast_but_actual_water_stays_blue() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        if !std::path::Path::new(&install).is_dir() { return eprintln!("no Civ III install, skipped"); }
        let loader = civ3_assets::loader::Civ3Loader::new(civ3_assets::GameFiles::complete(&install));
        let mut desc = render_desc::Description::parse(crate::assets::BUILTIN_DESC).unwrap();
        desc.images.retain(|name, _| name == "xdgc");
        desc.sheets.retain(|name, _| name == "terrain_xdgc");
        desc.layers.retain(|layer| layer.name == "terrain");
        desc.fonts.clear(); desc.palettes.clear(); desc.sets.clear();
        let compiled = desc.compile(&loader, 1024).unwrap();
        let mapper = Mapper::new(&compiled);
        let mut world = Civ3World { width: 8, height: 8, tiles: vec![Civ3Tile { square_type: rules::terrain::DESERT << 8, square_parts: 2 << 8, ..Default::default() }; 32], ..Default::default() };
        let at = world.index(0, 2);
        world.tiles[at].square_parts |= 6; // Only the nonexistent W corner is coast.
        let scene = mapper.scene(&world, &Civ3View::default(), (576, 288), 0);
        let mut map = tile_viewer::scene_world(&compiled, &scene);
        let terrain = compiled.layer_index("terrain").unwrap();
        let desert = compiled.value("terrain", "Std_Terrain_Images/162").unwrap();
        let false_coast = compiled.value("terrain", "Std_Terrain_Images/168").unwrap();
        assert_eq!(map.get(tile_render::LayerId(terrain), 0, 2), desert);
        let camera = Camera { overhang: OVERHANG, ..Camera::new((576, 288)) };
        let render = |map: &tile_render::TileWorld| tile_render::reference::render(map, &compiled.atlas, &camera, 0, [0, 0, 0, 255]);
        let blue_pixels = |image: &RgbaImage| image.as_bytes().chunks_exact(4).filter(|p| u16::from(p[2]) > u16::from(p[0]) + 8).count();
        assert_eq!(blue_pixels(&render(&map)), 0, "inland desert must stay land-coloured to its edge");
        map.set(tile_render::LayerId(terrain), 0, 2, false_coast);
        assert!(blue_pixels(&render(&map)) > 0, "the old stored outside-water corner reproduces the blue shoreline");
        world.wrap_x = true;
        let wrapped = mapper.scene(&world, &Civ3View::default(), (576, 288), 0);
        assert_eq!(wrapped.layers[terrain].values[at], false_coast, "a real neighbour across a seam keeps its coast");
        world.wrap_x = false;
        world.tiles.fill(Civ3Tile { square_type: rules::terrain::COAST << 8, square_parts: (2 << 8) | 80, ..Default::default() });
        let water = mapper.scene(&world, &Civ3View::default(), (576, 288), 0);
        assert!(blue_pixels(&render(&tile_viewer::scene_world(&compiled, &water))) > 0, "genuine water at the boundary stays water");
    }
}
