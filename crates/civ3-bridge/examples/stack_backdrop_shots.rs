//! The Accessibility stack count ("xN") with and without its dark
//! backdrop, over light (desert) and dark (forest) terrain, in a view at
//! world scale 1× and 2× (text from outlines at the view's resolution),
//! with the HUD at 1× and 2×. Real game art (CIV3_INSTALL).
//! nix develop -c cargo run -p civ3-bridge --example stack_backdrop_shots [dir]
use std::path::Path;
use civ3_assets::GameFiles;
use ifc23::{assets, city_labels, rules::terrain, scene_map::Mapper, text::Fonts, ui_screen, units,
    world::{Civ3Tile, Civ3Unit, Civ3View, Civ3World}, world_items::WorldItems};
use menu::screen::ScreenArt;
use tile_render::{Camera, RgbaImage, reference};
use viewer_settings::accessibility::Accessibility;

/// The world rectangle shown: both stacks.
const AREA: [i32; 4] = [176, 120, 256, 96];

fn save(dir: &Path, name: &str, img: &RgbaImage) {
    let path = dir.join(format!("{name}.png"));
    image::save_buffer(&path, img.as_bytes(), img.width, img.height, image::ExtendedColorType::Rgba8).unwrap();
    println!("{}", path.display());
}

/// `images` in a grid of `columns`, white lines between them.
fn grid(images: &[RgbaImage], columns: usize) -> RgbaImage {
    let (w, h) = (images[0].width, images[0].height);
    let rows = images.len().div_ceil(columns) as u32;
    let columns = columns as u32;
    RgbaImage::from_fn(columns * (w + 4) - 4, rows * (h + 4) - 4, |x, y| {
        let (c, r, ix, iy) = (x / (w + 4), y / (h + 4), x % (w + 4), y % (h + 4));
        match images.get((r * columns + c) as usize) {
            Some(i) if ix < w && iy < h => i.get(ix, iy),
            _ => [255; 4],
        }
    })
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/c3r-stackbg-shots".into());
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    let files = GameFiles::complete(&install);
    let (width, height) = (16, 24);
    let mut world = Civ3World { width, height, player: 1, color_tables: vec![0, 3], preferences: 0x800,
        unit_type_keys: vec!["PRTO_Warrior".into()], unit_type_stats: vec![[1, 1, 1, 0, 0, 0, 0]], experience_hit_points: vec![3],
        ..Default::default() };
    // Two of player 1's stacks: three warriors and twelve.
    let unit = |id, x: i32, y: i32| Civ3Unit { id, x, y, civ: 1, pixel: (x * 32 + 32, y * 16 + 16), animation: 1, direction: 4, visible: true, ..Default::default() };
    world.units = vec![unit(1, 7, 10), unit(2, 10, 12)];
    world.unit_tiles = [(7, 10, 3), (10, 12, 12)].into_iter().flat_map(|(x, y, n)| std::iter::repeat_n((x, y, -1), n)).collect();
    let tiles = |kind: u32| vec![Civ3Tile { square_type: kind << 12 | kind << 8, city: -1, explored: u32::MAX, visibility: u32::MAX, victory_location: -1, ..Default::default() }; (width * height / 2) as usize];
    world.tiles = tiles(terrain::DESERT);
    let mut art = units::UnitArt::new(files.clone());
    let needed = units::Needed::of(&world, &mut art);
    let mut env = city_labels::FileEnv::new(Box::new(files.clone()));
    let settings = |hud_size, backdrop| Accessibility { stack_count: true, stack_backdrop: backdrop, hud_size, ..Default::default() };
    let mut fonts = Fonts::default();
    let mut screen = ScreenArt::default();
    for hud in [0, 2] {
        for backdrop in [true, false] {
            ui_screen::add_art(&units::stack_items(&world, false, &settings(hud, backdrop), &mut env), &mut screen, &mut fonts, &files);
        }
    }
    let desc = format!("{}{}{}{}", assets::BUILTIN_DESC, needed.description(), ui_screen::description(&screen), fonts.description());
    let compiled = assets::compile(&desc, files).unwrap();
    let view = Civ3View { tile_x: (0, width - 1), tile_y: (0, height - 1), ..Default::default() };
    for (hud, hud_name) in [(0, "hud1x"), (2, "hud2x")] {
        for zoom in [1u32, 2] {
            let mut shots = Vec::new();
            for (kind, terrain_name) in [(terrain::DESERT, "light"), (terrain::FOREST, "dark")] {
                world.tiles = tiles(kind);
                for backdrop in [true, false] {
                    let a = settings(hud, backdrop);
                    let mut scene = Mapper::new(&compiled).scene(&world, &view, (1024, 512), 0);
                    scene.sprites.extend(units::free_sprites_accessible(&world, &mut art, &compiled, None, &a));
                    let items = WorldItems::new(false, |reduced| units::stack_items(&world, reduced, &a, &mut env));
                    scene.sprites.extend(items.sprites(&fonts, &compiled, units::unit_passes(&compiled).0, true));
                    let scene_world = tile_viewer::scene_world(&compiled, &scene);
                    let size = (AREA[2] as u32 * zoom, AREA[3] as u32 * zoom);
                    let camera = Camera { x: AREA[0], y: AREA[1], zoom: (zoom, 1), zoomed_art: true, overhang: ifc23::scene_map::OVERHANG, ..Camera::new(size) };
                    let picture = reference::render_views(&scene_world, &compiled.atlas, &[camera], size, scene.tick, [0, 0, 0, 255]);
                    let name = format!("stack-{terrain_name}-{}-{hud_name}-view{zoom}x", if backdrop { "backdrop" } else { "bare" });
                    save(dir, &name, &picture);
                    shots.push(picture);
                }
            }
            // Rows: light, dark terrain; columns: with, without the backdrop.
            save(dir, &format!("stack-compare-{hud_name}-view{zoom}x"), &grid(&shots, 2));
        }
    }
}
