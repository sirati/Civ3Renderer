//! The summary replay's navigator, rendered from terrain and the drawing
//! operations chosen by the engine. No image pixels are read from the game.

use std::collections::HashMap;

use crate::minimap::{self, Geometry, Terrain};
use crate::world::Civ3World;

/// The replay engine's changes to its political map, in drawing order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Tint { x: i32, y: i32, color: u16 },
    Restore { x: i32, y: i32 },
    AddCity { x: i32, y: i32 },
    EraseCity { x: i32, y: i32 },
}

#[derive(Debug)]
struct Cell {
    water: bool,
    pixels: Vec<usize>,
}

/// A retained RGB555 image. Cell membership is computed once, so a territory
/// event visits only that cell's pixels. Memory does not grow with playback.
#[derive(Debug)]
pub struct ReplayMap {
    geometry: Geometry,
    bright: Vec<u16>,
    pixels: Vec<u16>,
    cells: HashMap<(i32, i32), Cell>,
}

impl ReplayMap {
    /// The whole world's darkened terrain, without live territory, cities,
    /// or exploration. Replay events add those marks independently.
    pub fn new(world: &Civ3World, size: (u32, u32)) -> Self {
        let Terrain { geometry, bright, tiles } = minimap::terrain(world, size);
        let pixels = bright.iter().copied().map(minimap::darken).collect();
        let mut cells = HashMap::<_, Cell>::new();
        for (i, tile) in tiles.into_iter().enumerate() {
            let Some((x, y)) = tile else { continue };
            let (x, y) = (x as i32, y as i32);
            cells.entry((x, y)).or_insert_with(|| Cell {
                water: world.tile(x, y).is_none_or(|tile| minimap::WATER.contains(&tile.terrain())),
                pixels: Vec::new(),
            }).pixels.push(i);
        }
        Self { geometry, bright, pixels, cells }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.geometry.w as u32, self.geometry.h as u32)
    }

    /// Restores the initial replay image. Live ownership is not applied.
    pub fn reset(&mut self) {
        for (pixel, &bright) in self.pixels.iter_mut().zip(&self.bright) {
            *pixel = minimap::darken(bright);
        }
    }

    /// Applies the engine's operation, preserving overlap and redraw order.
    pub fn apply(&mut self, op: &Operation) {
        match *op {
            Operation::Tint { x, y, color } => {
                let Some(cell) = self.cells.get(&(x, y)).filter(|cell| !cell.water) else { return };
                for &i in &cell.pixels {
                    self.pixels[i] = minimap::tint(self.bright[i], color);
                }
            }
            Operation::Restore { x, y } => {
                let Some(cell) = self.cells.get(&(x, y)) else { return };
                for &i in &cell.pixels {
                    self.pixels[i] = minimap::darken(self.bright[i]);
                }
            }
            Operation::AddCity { x, y } => self.city(x, y, true),
            Operation::EraseCity { x, y } => self.city(x, y, false),
        }
    }

    /// The image in row-major RGBA, ready for a scene patch.
    pub fn rgba(&self) -> Vec<u8> {
        if self.geometry.width <= 0 || self.geometry.height <= 1 {
            return vec![0; self.pixels.len() * 4];
        }
        minimap::rgba(&self.pixels)
    }

    /// 0x5797B0 / 0x5799B0: city squares wrap their tile coordinates before
    /// computing their centre; row and column zero are never written.
    fn city(&mut self, x: i32, y: i32, add: bool) {
        let g = &self.geometry;
        if g.width <= 0 || g.height <= 1 || g.w <= 0 || g.h <= 0 {
            return;
        }
        let (x, y) = (g.wrap_x(i64::from(x)), g.wrap_y(i64::from(y)));
        if !g.valid(x, y) {
            return;
        }
        let per_col = g.w / g.width;
        let s = (per_col >> 2).clamp(1, 4);
        let cx = x * g.w / g.width + (per_col >> 2);
        let cy = y * g.h / (g.height - 1) + (per_col >> 4);
        if !add {
            // 0x5799B0 increments the tile arguments as loop counters but
            // repeatedly addresses the square's saved top-left pixel. Keep
            // that erase quirk so rewind matches the engine's drawing.
            let (px, py) = (cx - s, cy - s);
            if x <= cx + s && y <= cy + s && px > 0 && py > 0 && px < g.w && py < g.h {
                let i = (py * g.w + px) as usize;
                self.pixels[i] = minimap::darken(self.bright[i]);
            }
            return;
        }
        for py in (cy - s).max(1)..=(cy + s).min(g.h - 1) {
            for px in (cx - s).max(1)..=(cx + s).min(g.w - 1) {
                let i = (py * g.w + px) as usize;
                self.pixels[i] = minimap::WHITE;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Civ3City, Civ3Tile};

    fn world() -> Civ3World {
        Civ3World {
            width: 8,
            height: 8,
            flags: 3,
            wrap_x: true,
            wrap_y: true,
            tiles: vec![Civ3Tile { square_type: 2 << 12, owner: 1, explored: u32::MAX, ..Default::default() }; 32],
            cities: vec![Civ3City { x: 2, y: 2, ..Default::default() }],
            ..Default::default()
        }
    }

    fn red() -> u16 {
        minimap::from_rgb([248, 0, 0])
    }

    fn white_pixels(map: &ReplayMap) -> usize {
        map.pixels.iter().filter(|&&pixel| pixel == minimap::WHITE).count()
    }

    #[test]
    fn initial_image_uses_whole_terrain_without_live_ownership_cities_or_exploration() {
        let mut world = world();
        let marked = ReplayMap::new(&world, (64, 56));
        world.cities.clear();
        world.player = 7;
        for tile in &mut world.tiles {
            tile.owner = 0;
            tile.explored = 0;
        }
        let unseen = ReplayMap::new(&world, (64, 56));
        assert_eq!(marked.rgba(), unseen.rgba());
        assert_eq!(marked.size(), (64, 56));
        assert_eq!(white_pixels(&marked), 0);
        assert!(marked.rgba().chunks_exact(4).all(|pixel| pixel[3] == 255));
        // Terrain ID 2 is RGB555 0x4646. Per-term interpolation truncation
        // at this sample gives channels 128,136,40. Averaging with grey
        // then truncating to RGB555 gives 88,96,48.
        let expected = minimap::from_rgb([88, 96, 48]);
        assert_eq!(marked.pixels[16 * 64 + 18], expected);
    }

    #[test]
    fn tint_uses_bright_terrain_and_changes_only_the_requested_land_cell() {
        let mut map = ReplayMap::new(&world(), (64, 56));
        let before = map.pixels.clone();
        let indices = map.cells[&(2, 2)].pixels.clone();
        map.apply(&Operation::Tint { x: 2, y: 2, color: red() });
        // (3 * (248,0,0) + (136,144,48)) / 4 = (220,36,12).
        let expected = minimap::from_rgb([220, 36, 12]);
        assert!(indices.iter().all(|&i| map.pixels[i] == expected));
        assert!(map.pixels.iter().zip(&before).enumerate().all(|(i, (after, before))| indices.contains(&i) || after == before));
        map.apply(&Operation::Restore { x: 2, y: 2 });
        assert_eq!(map.pixels, before);
    }

    #[test]
    fn water_is_not_tinted_and_invalid_cells_are_ignored() {
        let mut world = world();
        for terrain in 11..=13 {
            for tile in &mut world.tiles {
                tile.square_type = terrain << 12;
            }
            let mut map = ReplayMap::new(&world, (64, 56));
            let before = map.rgba();
            map.apply(&Operation::Tint { x: 2, y: 2, color: red() });
            map.apply(&Operation::Tint { x: -2, y: 2, color: red() });
            map.apply(&Operation::Restore { x: 3, y: 2 });
            assert_eq!(map.rgba(), before);
        }
    }

    #[test]
    fn later_cell_drawing_overwrites_city_pixels_and_redrawing_the_city_restores_them() {
        let mut map = ReplayMap::new(&world(), (64, 56));
        map.apply(&Operation::AddCity { x: 2, y: 2 });
        let city = map.pixels.clone();
        let white = white_pixels(&map);
        assert_eq!(white, 25);
        map.apply(&Operation::Tint { x: 2, y: 2, color: red() });
        assert!(white_pixels(&map) < white);
        assert_ne!(map.pixels, city);
        map.apply(&Operation::AddCity { x: 2, y: 2 });
        assert_eq!(white_pixels(&map), white);
    }

    #[test]
    fn erasing_a_city_preserves_the_engines_top_left_pixel_quirk() {
        let mut map = ReplayMap::new(&world(), (64, 56));
        map.apply(&Operation::Tint { x: 2, y: 2, color: red() });
        let tinted = map.pixels.clone();
        map.apply(&Operation::AddCity { x: 2, y: 2 });
        map.apply(&Operation::EraseCity { x: 2, y: 2 });
        assert_ne!(map.pixels, tinted);
        assert_eq!(white_pixels(&map), 24);
        assert_eq!(map.pixels[14 * 64 + 16], minimap::darken(map.bright[14 * 64 + 16]));
        assert_eq!(map.pixels[16 * 64 + 18], minimap::WHITE);
        // A subsequent ownership undo independently overwrites the city
        // pixels belonging to that cell, leaving any outside it retained.
        map.apply(&Operation::Restore { x: 2, y: 2 });
        assert!(white_pixels(&map) < 24);
        for y in 0..8 {
            for x in 0..8 {
                map.apply(&Operation::Restore { x, y });
            }
        }
        assert_eq!(map.rgba(), ReplayMap::new(&world(), (64, 56)).rgba());
    }

    #[test]
    fn city_coordinates_wrap_and_squares_do_not_write_row_or_column_zero() {
        let world = world();
        let mut edge = ReplayMap::new(&world, (64, 56));
        let before = edge.pixels.clone();
        edge.apply(&Operation::AddCity { x: 0, y: 0 });
        for x in 0..64 {
            assert_eq!(edge.pixels[x], before[x]);
        }
        for y in 0..56 {
            assert_eq!(edge.pixels[y * 64], before[y * 64]);
        }
        let mut wrapped = ReplayMap::new(&world, (64, 56));
        let mut canonical = ReplayMap::new(&world, (64, 56));
        wrapped.apply(&Operation::AddCity { x: -2, y: 10 });
        canonical.apply(&Operation::AddCity { x: 6, y: 2 });
        assert_eq!(wrapped.rgba(), canonical.rgba());
        let mut unwrapped_world = world;
        unwrapped_world.flags = 0;
        unwrapped_world.wrap_x = false;
        unwrapped_world.wrap_y = false;
        let mut unwrapped = ReplayMap::new(&unwrapped_world, (64, 56));
        let before = unwrapped.rgba();
        unwrapped.apply(&Operation::AddCity { x: -2, y: 2 });
        assert_eq!(unwrapped.rgba(), before);
    }

    #[test]
    fn city_centres_use_integer_division_and_marker_radius_is_clamped() {
        let mut small = ReplayMap::new(&world(), (63, 55));
        small.apply(&Operation::AddCity { x: 2, y: 2 });
        let marked: Vec<_> = small.pixels.iter().enumerate()
            .filter_map(|(i, &pixel)| (pixel == minimap::WHITE).then_some((i % 63, i / 63)))
            .collect();
        assert_eq!(marked, vec![(15, 14), (16, 14), (17, 14), (15, 15), (16, 15), (17, 15), (15, 16), (16, 16), (17, 16)]);
        let mut large = ReplayMap::new(&world(), (256, 224));
        large.apply(&Operation::AddCity { x: 2, y: 2 });
        assert_eq!(white_pixels(&large), 81);
        assert_eq!(large.pixels[62 * 256 + 68], minimap::WHITE);
        assert_ne!(large.pixels[61 * 256 + 68], minimap::WHITE);
    }

    #[test]
    fn city_erase_preserves_the_engines_tile_counter_and_edge_guards() {
        let mut small = ReplayMap::new(&world(), (4, 4));
        small.apply(&Operation::AddCity { x: 6, y: 6 });
        assert!(white_pixels(&small) > 0);
        let before = small.rgba();
        small.apply(&Operation::EraseCity { x: 6, y: 6 });
        assert_eq!(small.rgba(), before);
        let mut edge = ReplayMap::new(&world(), (64, 56));
        edge.apply(&Operation::AddCity { x: 0, y: 0 });
        assert!(white_pixels(&edge) > 0);
        let before = edge.rgba();
        edge.apply(&Operation::EraseCity { x: 0, y: 0 });
        assert_eq!(edge.rgba(), before);
    }

    #[test]
    fn reset_clears_replay_marks_and_playback_does_not_accumulate_storage() {
        let mut map = ReplayMap::new(&world(), (64, 56));
        let initial = map.rgba();
        let cells = map.cells.len();
        let pixel_memberships: usize = map.cells.values().map(|cell| cell.pixels.len()).sum();
        for _ in 0..200 {
            map.apply(&Operation::Tint { x: 2, y: 2, color: red() });
            map.apply(&Operation::AddCity { x: 2, y: 2 });
            map.apply(&Operation::EraseCity { x: 2, y: 2 });
        }
        map.reset();
        assert_eq!(map.rgba(), initial);
        assert_eq!(map.cells.len(), cells);
        assert_eq!(map.cells.values().map(|cell| cell.pixels.len()).sum::<usize>(), pixel_memberships);
    }

    #[test]
    fn empty_and_invalid_maps_have_no_drawable_pixels() {
        for size in [(0, 0), (0, 56), (64, 0), (64, 56)] {
            let mut map = ReplayMap::new(&Civ3World::default(), size);
            map.apply(&Operation::AddCity { x: 0, y: 0 });
            map.apply(&Operation::Tint { x: 0, y: 0, color: red() });
            assert_eq!(map.rgba(), vec![0; size.0 as usize * size.1 as usize * 4]);
        }
    }
}
