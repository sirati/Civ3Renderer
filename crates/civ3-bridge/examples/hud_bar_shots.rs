//! Enlarged unit HUDs before and after they were drawn at the views'
//! resolution, at Retina-like world and pixel scales, in the main view and
//! in a zoom box. Real game art (CIV3_INSTALL).
//! nix develop -c cargo run -p civ3-bridge --example hud_bar_shots [dir]
use std::path::Path;
use civ3_assets::GameFiles;
use ifc23::{assets, units, world::{Civ3Tile, Civ3Unit, Civ3World}};
use render_desc::Compiled;
use tile_render::{Camera, RgbaImage, reference};
use viewer_settings::accessibility::{Accessibility, HUD_SCALES};

const BACKGROUND: [u8; 4] = [105, 119, 73, 255];
/// The world rectangle shown: the units' row.
const HUD_AREA: [i32; 4] = [40, 125, 360, 155];

fn save(dir: &Path, name: &str, img: &RgbaImage) {
    let path = dir.join(format!("{name}.png"));
    image::save_buffer(&path, img.as_bytes(), img.width, img.height, image::ExtendedColorType::Rgba8).unwrap();
    println!("{}", path.display());
}

/// The HUD's enlarged sheets as the viewer had them before: its 1:1 art
/// resized in world pixels (`resize`, nearest) about the bar's bottom centre.
fn old_sheets() -> String {
    let base: toml::Table = toml::from_str(&units::hud::description()).unwrap();
    let mut out = String::new();
    for (index, scale) in HUD_SCALES.into_iter().enumerate().skip(1) {
        let mut sheets = toml::Table::new();
        for (name, original) in base["sheets"].as_table().unwrap() {
            let mut sheet = original.as_table().unwrap().clone();
            let scaled = |key: &str| toml::Value::Array(sheet[key].as_array().unwrap().iter().map(|v|
                toml::Value::Integer((v.as_integer().unwrap() as f32 * scale).round() as i64)).collect());
            let mut resize = scaled("cell");
            let mut anchor = toml::Value::Array(sheet["anchor"].as_array().unwrap().iter().enumerate().map(|(axis, v)| {
                let pivot = [7.0, 24.0][axis];
                toml::Value::Integer(((v.as_integer().unwrap() as f32 - pivot) * scale).round() as i64)
            }).collect());
            if name == "unit_hud_stack" {
                resize.as_array_mut().unwrap()[1] = sheet["cell"][1].clone();
                anchor.as_array_mut().unwrap()[1] = 2.into();
            }
            sheet.insert("resize".into(), resize);
            sheet.insert("anchor".into(), anchor);
            let group = format!("{}@size{index}", sheet["group"].as_str().unwrap());
            sheet.insert("group".into(), group.into());
            sheets.insert(format!("{name}_size{index}"), sheet.into());
        }
        let mut extra = toml::Table::new();
        extra.insert("sheets".into(), sheets.into());
        out.push_str(&toml::to_string(&extra).unwrap());
    }
    out
}

/// The units as the viewer drew them before: the HUD's pieces from the
/// enlarged sheets.
fn old_sprites(world: &Civ3World, art: &mut units::UnitArt, compiled: &Compiled, hud_size: usize) -> Vec<scene::FreeSprite> {
    let (after, _) = units::unit_passes(compiled);
    let mut out = Vec::new();
    for u in world.units.iter().filter(|u| u.visible) {
        for mut p in units::pieces(world, art, u, None) {
            if p.hud.is_some() && hud_size != 0 {
                let (group, index) = p.set.rsplit_once('/').unwrap();
                p.set = format!("{group}@size{hud_size}/{index}");
                p.at = (p.at.0 + 7, p.at.1 + 24);
            }
            let Some((set, _)) = compiled.set(p.sprite.as_deref().unwrap_or(&p.set)) else { continue };
            let palette = if p.palette.is_empty() { 0 } else { compiled.palette(&p.palette).unwrap_or(0) };
            out.push(scene::FreeSprite { set: set.raw(), x: p.at.0, y: p.at.1, after_pass: after, ticks_per_frame: 1,
                scale_den: p.scale_den, tint: p.tint, palette, ..Default::default() });
        }
    }
    out
}

/// `sprites` (and `markers`) in a main view at world scale `zoom`, shown
/// `boxes` times larger by whole pixels (the pixel scale), with a zoom box
/// over its top right `magnify` times larger again (none: 0).
fn draw(compiled: &Compiled, sprites: Vec<scene::FreeSprite>, markers: &[tile_render::PlacedSprite], area: [i32; 4], zoom: u32, boxes: u32, magnify: (u32, (i32, i32))) -> RgbaImage {
    let (magnify, (cx, cy)) = magnify;
    let mut scene = scene::Scene::empty(scene::WorldShape { cols: 1, rows: 1, wrap_x: false, wrap_y: false }, compiled.layers.len());
    scene.sprites = sprites;
    let mut world = tile_viewer::scene_world(compiled, &scene);
    world.free.extend_from_slice(markers);
    let size = (area[2] as u32 * zoom, area[3] as u32 * zoom);
    let main = Camera { x: area[0], y: area[1], zoom: (zoom, 1), zoomed_art: true, ..Camera::new(size) };
    let mut cameras = vec![main];
    if magnify > 0 {
        // Around (cx, cy): a box a third of the view wide.
        let z = zoom * magnify;
        let viewport = (size.0 * 2 / 5, size.1 * 5 / 6);
        cameras.push(Camera { x: cx - (viewport.0 / z / 2) as i32, y: cy - (viewport.1 / z / 2) as i32, zoom: (z, 1), zoomed_art: true,
            origin: ((size.0 - viewport.0 - 6) as i32, 6), ..Camera::new(viewport) });
    }
    let picture = reference::render_views(&world, &compiled.atlas, &cameras, size, 0, BACKGROUND);
    if magnify > 0 {
        // The box's frame.
        let [l, t] = [cameras[1].origin.0, cameras[1].origin.1];
        let (w, h) = (cameras[1].viewport.0 as i32, cameras[1].viewport.1 as i32);
        let edge = |x: i32, y: i32| (x == l - 1 || x == l + w || y == t - 1 || y == t + h) && (l - 1..=l + w).contains(&x) && (t - 1..=t + h).contains(&y);
        let framed = RgbaImage::from_fn(picture.width, picture.height, |x, y| if edge(x as i32, y as i32) { [255, 255, 255, 255] } else { picture.get(x, y) });
        return boxed(&framed, boxes);
    }
    boxed(&picture, boxes)
}

/// `image` shown `k` times larger, each pixel a `k`×`k` box.
fn boxed(image: &RgbaImage, k: u32) -> RgbaImage {
    RgbaImage::from_fn(image.width * k, image.height * k, |x, y| image.get(x / k, y / k))
}

/// `images` side by side, a white column between them.
fn beside(images: &[RgbaImage]) -> RgbaImage {
    let height = images.iter().map(|i| i.height).max().unwrap();
    let width = images.iter().map(|i| i.width + 4).sum::<u32>() - 4;
    RgbaImage::from_fn(width, height, |x, y| {
        let mut x = x;
        for i in images {
            if x < i.width { return if y < i.height { i.get(x, y) } else { [255; 4] }; }
            if x < i.width + 4 { return [255; 4]; }
            x -= i.width + 4;
        }
        [255; 4]
    })
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/c3r-hudbar-shots".into());
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    let files = GameFiles::complete(&install);
    // Warriors (1/1/1) and spearmen (1/2/1) of player 1 at experience
    // levels of 3, 4 and 5 hit points; one of player 2.
    let mut world = Civ3World { width: 16, height: 8, player: 1, color_tables: vec![0, 3, 1], preferences: 0x800,
        unit_type_keys: vec!["PRTO_Warrior".into(), "PRTO_Spearman".into()], unit_type_stats: vec![[1, 1, 1, 0, 0, 0, 0], [1, 2, 1, 0, 0, 0, 0]],
        experience_hit_points: vec![3, 4, 5],
        tiles: vec![Civ3Tile { city: -1, ..Default::default() }; 64],
        ..Default::default() };
    let unit = |id, tile: i32, unit_type, experience, damage, moves, state, civ| Civ3Unit { id, x: 2 * tile, y: 2, civ, unit_type, experience, damage, moves,
        state, pixel: (100 + 75 * tile, 240), animation: 1, direction: 4, visible: true, ..Default::default() };
    world.units = vec![
        unit(1, 0, 0, 2, 0, 0, 0, 1),
        unit(2, 1, 1, 2, 2, 1, 1, 1),
        unit(3, 2, 0, 1, 3, 3, 0, 1),
        unit(4, 3, 1, 0, 1, 0, 0, 2),
    ];
    // A stack of three on the second unit's tile.
    world.unit_tiles = vec![(2, 2, -1), (2, 2, -1), (2, 2, -1), (0, 2, -1), (4, 2, -1), (6, 2, -1)];
    world.current_unit = Some(2);
    let mut art = units::UnitArt::new(files.clone());
    let needed = units::Needed::of(&world, &mut art);
    let desc = format!("{}{}{}", assets::BUILTIN_DESC, needed.description(), old_sheets());
    let compiled = assets::compile(&desc, files).unwrap();
    // The HUD, before and after, by world scale and pixel scale.
    for (index, scale) in HUD_SCALES.into_iter().enumerate().skip(1) {
        let a = Accessibility { hud_size: index, ..Default::default() };
        let name = format!("{}", scale).replace('.', "_");
        for (zoom, boxes, label) in [(2, 1, "world2"), (3, 1, "world3"), (1, 2, "pixel2"), (1, 3, "pixel3")] {
            let before = draw(&compiled, old_sprites(&world, &mut art, &compiled, index), &[], HUD_AREA, zoom, boxes, (0, (0, 0)));
            let after = draw(&compiled, units::free_sprites_accessible(&world, &mut art, &compiled, None, &a), &[], HUD_AREA, zoom, boxes, (0, (0, 0)));
            save(dir, &format!("hud{name}x-{label}-before"), &before);
            save(dir, &format!("hud{name}x-{label}-after"), &after);
            save(dir, &format!("hud{name}x-{label}-compare"), &beside(&[before.clone(), after.clone()]));
            // Close up on the first two units' HUDs (before above after).
            let k = (zoom * boxes) as i32;
            let close = |i: &RgbaImage| crop(i, [(60 - HUD_AREA[0]) * k, (130 - HUD_AREA[1]) * k, 140 * k, 120 * k]);
            save(dir, &format!("hud{name}x-{label}-closeup"), &above(&[close(&before), close(&after)]));
        }
        // In a zoom box three times larger again, over the world at 2×.
        let before = draw(&compiled, old_sprites(&world, &mut art, &compiled, index), &[], HUD_AREA, 2, 1, (3, (152, 196)));
        let after = draw(&compiled, units::free_sprites_accessible(&world, &mut art, &compiled, None, &a), &[], HUD_AREA, 2, 1, (3, (152, 196)));
        save(dir, &format!("hud{name}x-zoombox-compare"), &beside(&[before, after]));
    }
    // 1×: the same either way.
    let one = draw(&compiled, units::free_sprites_accessible(&world, &mut art, &compiled, None, &Accessibility::default()), &[], HUD_AREA, 2, 1, (0, (0, 0)));
    save(dir, "hud1x-world2", &one);
}

/// The `[x, y, w, h]` part of `image`.
fn crop(image: &RgbaImage, [x, y, w, h]: [i32; 4]) -> RgbaImage {
    RgbaImage::from_fn(w as u32, h as u32, |i, j| image.get((x as u32 + i).min(image.width - 1), (y as u32 + j).min(image.height - 1)))
}

/// `images` one above the other, a white row between them.
fn above(images: &[RgbaImage]) -> RgbaImage {
    let width = images.iter().map(|i| i.width).max().unwrap();
    let height = images.iter().map(|i| i.height + 4).sum::<u32>() - 4;
    RgbaImage::from_fn(width, height, |x, y| {
        let mut y = y;
        for i in images {
            if y < i.height { return if x < i.width { i.get(x, y) } else { [255; 4] }; }
            if y < i.height + 4 { return [255; 4]; }
            y -= i.height + 4;
        }
        [255; 4]
    })
}
