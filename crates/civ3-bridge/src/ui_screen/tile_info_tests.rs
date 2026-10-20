//! The picture of the map in the tile information box (`open_tile_info`,
//! PCGames 0x5AA530): the game draws a tile with its neighbours into the
//! box's own canvas through the map renderer (`Map_Renderer` slot +0x118 ->
//! m20). Mirrored, it must show the pixels the game's blits make: plain
//! map sprites colour-keyed (254 and 255 not drawn), the resource shadows
//! through the shadow blend tables, the territory borders through a colour
//! table and the shade tables. The reference here is a CPU copy of those
//! blits on an RGB555 canvas, from the real art.

use super::*;
use crate::ui::{indices, Event, UiModel};

const CANVAS: u32 = 300;
const SIZE: (i32, i32) = (256, 173);
/// The owner's colour table of the border (`ntpNN.pcx`).
const OWNER_TABLE: u32 = 3;

/// One image of the install, as the game loads it.
struct Art {
    image: u32,
    path: &'static str,
    pcx: civ3_assets::pcx::Indexed,
}

/// How the game's blit draws a sprite.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// `Sprite_draw_on_map` / `Sprite_draw` (jgl 0x1000ADA0): indices of
    /// 254 and up not drawn, the rest in the image's colours.
    Plain,
    /// The blend blit through the shadow tables (0x5F8560 -> jgl
    /// 0x100141E0 with `[0x73D9F0]`).
    Shadow,
    /// `Sprite_draw_with_color_table` through the owner's colours and the
    /// shade tables (unscaled: jgl 0x100159E0, its pixels from 0x10015BF1).
    Colored,
}

/// A sprite the game cut from `art`'s `cell` (x, y, w, h), drawn at `at`
/// (the cell's top-left: the blit adds the crop offset).
struct Draw {
    art: usize,
    cell: [i32; 4],
    at: (i32, i32),
    kind: Kind,
}

/// The cell cropped to the box of its pixels that are not 255, as
/// `Sprite_slice_pcx` (0x5F7EC0 -> 0x62B170) keeps it, after the indices the
/// game blanks in that file (`indices::blank`): the stored rectangle.
fn sliced(art: &Art, [x, y, w, h]: [i32; 4]) -> Option<[i32; 4]> {
    let blank = indices::blank(art.path);
    let index = |px: i32, py: i32| {
        let i = art.pcx.get(px as u32, py as u32);
        if blank.contains(&i) { 255 } else { i }
    };
    let (mut l, mut t, mut r, mut b) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for py in y..y + h {
        for px in x..x + w {
            if index(px, py) != 255 {
                (l, t, r, b) = (l.min(px), t.min(py), r.max(px + 1), b.max(py + 1));
            }
        }
    }
    (r > l).then_some([l, t, r - l, b - t])
}

fn rgb5(c: civ3_assets::Rgb) -> [u8; 3] {
    [c.0 >> 3, c.1 >> 3, c.2 >> 3]
}

/// The game's canvas after `draws` (RGB555 channels), starting black.
fn game_picture(arts: &[Art], draws: &[Draw], owner: &civ3_assets::Palette) -> Vec<[u8; 3]> {
    let mut canvas = vec![[0u8; 3]; (SIZE.0 * SIZE.1) as usize];
    for d in draws {
        let art = &arts[d.art];
        let Some([sx, sy, w, h]) = sliced(art, d.cell) else { continue };
        let blank = indices::blank(art.path);
        let (x0, y0) = (d.at.0 + sx - d.cell[0], d.at.1 + sy - d.cell[1]);
        for j in 0..h {
            for i in 0..w {
                let (x, y) = (x0 + i, y0 + j);
                let (u, v) = (sx + i, sy + j);
                if x < 0 || y < 0 || x >= SIZE.0 || y >= SIZE.1 {
                    continue;
                }
                let mut index = art.pcx.get(u as u32, v as u32);
                if blank.contains(&index) {
                    index = 255;
                }
                let px = &mut canvas[(y * SIZE.0 + x) as usize];
                let shade = |px: &mut [u8; 3], k: u32| *px = px.map(|c| (u32::from(c) * k / 16) as u8);
                match d.kind {
                    Kind::Plain if index < 254 => *px = rgb5(art.pcx.palette.0[index as usize]),
                    Kind::Shadow if (248..252).contains(&index) => shade(px, 8 + 2 * u32::from(index - 248)),
                    Kind::Colored if index < 224 => {
                        let c = if (64..66).contains(&index) { owner.0[index as usize] } else { art.pcx.palette.0[index as usize] };
                        *px = rgb5(c);
                    }
                    Kind::Colored if (240..255).contains(&index) => shade(px, u32::from(index) - 239),
                    _ => {}
                }
            }
        }
    }
    canvas
}

/// The tile information box's canvas as the mirror records it (the events
/// the tracer emits for these draws; `fixed`: as it does now, else as it
/// did before: every sprite a colour-keyed blit in its file's colours).
fn mirrored(arts: &[Art], draws: &[Draw], fixed: bool) -> Frame {
    let mut m = UiModel::new(vec![], vec![]);
    m.apply(Event::Size { image: CANVAS, size: SIZE });
    // A new form canvas: zeroed pixels (`initialized_canvas`).
    m.apply(Event::Fill { canvas: CANVAS, rect: [0, 0, SIZE.0, SIZE.1], rgb: [0, 0, 0], alpha: 255, clip: None });
    for art in arts {
        m.apply(Event::Loaded { image: art.image, path: art.path.into(), size: (art.pcx.width as i32, art.pcx.height as i32) });
    }
    let owner = menu::screen::PaletteFrom { image: format!("Art/Units/Palettes/ntp{OWNER_TABLE:02}.pcx"), first: 64, count: 2 };
    for (n, d) in draws.iter().enumerate() {
        let art = &arts[d.art];
        let Some(rect) = sliced(art, d.cell) else { continue };
        let sprite = 1000 + n as u32;
        m.apply(Event::Sliced { sprite, image: art.image, rect });
        let (x, y) = (d.at.0 + rect[0] - d.cell[0], d.at.1 + rect[1] - d.cell[1]);
        let indexed = |indices, palette| Event::IndexedBlit { sprite, canvas: CANVAS, x, y, clip: None, palette, indices };
        match (d.kind, fixed) {
            (Kind::Plain, _) | (_, false) => m.apply(Event::Blit { sprite, canvas: CANVAS, x, y, clip: None, palette: None }),
            (Kind::Shadow, true) => m.apply(indexed(indices::blend(&indices::SHADOW_LEVELS), None)),
            (Kind::Colored, true) => {
                m.apply(indexed(indices::colored(indices::blank(art.path)), Some(owner.clone())));
                m.apply(indexed(indices::colored_shades(), None));
            }
        }
    }
    m.apply(Event::PaintBegin);
    m.apply(Event::Composite { canvas: CANVAS, vtable: 0x66_F5C8, origin: (0, 0), size: SIZE, clip: [0, 0, SIZE.0, SIZE.1], keyed: false });
    m.apply(Event::PaintEnd);
    m.frame()
}

/// What our renderer draws of `frame` (RGB555 channels), on the CPU.
fn rendered(frame: &Frame, files: &civ3_assets::GameFiles) -> Vec<[u8; 3]> {
    let items = items(frame, files.roots(), &[], 0);
    let mut art = ScreenArt::default();
    let mut fonts = Fonts::default();
    add_art(&items, &mut art, &mut fonts, files);
    // The map description's colour model and loader (assets/civ3/civ3.toml).
    let header = "grid = { layout = 'orthogonal', step = [1, 1], color_bits = 5 }\n[loader]\nquantize = \"rgb555\"\ntransparent = [254, 255]\n";
    let compiled = crate::assets::compile(&format!("{header}{}", description(&art)), civ3_assets::GameFiles::with_roots(files.roots().to_vec())).unwrap();
    let mut world = compiled.world(1, 1, false, false);
    world.free = sprites(&items, &fonts, &compiled, 0, false).iter().map(|s| tile_viewer::placed_sprite(&compiled, s, Some(0), None)).collect();
    let camera = tile_render::Camera { cells: false, ..tile_render::Camera::new((SIZE.0 as u32, SIZE.1 as u32)) };
    let image = tile_render::reference::render(&world, &compiled.atlas, &camera, 0, [0, 0, 0, 255]);
    image.pixels.iter().map(|p| [p[0] >> 3, p[1] >> 3, p[2] >> 3]).collect()
}

fn save(dir: &std::path::Path, name: &str, pixels: &[[u8; 3]]) {
    let bytes: Vec<u8> = pixels.iter().flat_map(|c| c.map(|v| (v << 3) | (v >> 2))).collect();
    image::save_buffer(dir.join(name), &bytes, SIZE.0 as u32, SIZE.1 as u32, image::ColorType::Rgb8).unwrap();
}

/// A grassland tile with cattle, a road and a river, its neighbours and
/// a border, in the box: the mirrored picture is the game's, the cattle's
/// shadow darkening the terrain below it (not the shadow file's green
/// palette colours), the border in its owner's colours with its shades and
/// none of the diamond's filler.
#[test]
fn tile_info_picture_with_cattle_road_and_river_matches_the_games_blits() {
    let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    if !std::path::Path::new(&install).is_dir() { return eprintln!("no Civ III assets for the tile information picture"); }
    let files = civ3_assets::GameFiles::complete(&install);
    let load = |image: u32, path: &'static str| Art { image, path, pcx: civ3_assets::pcx::decode(&files.read(path).unwrap()).unwrap() };
    let arts = vec![
        load(1, "Art/Terrain/xtgc.pcx"),
        load(2, "Art/Terrain/mtnRivers.pcx"),
        load(3, "Art/Terrain/roads.pcx"),
        load(4, "Art/resources_shadows.pcx"),
        load(5, "Art/resources.pcx"),
        load(6, "Art/Terrain/Territory.pcx"),
        load(7, "Art/TileInfo.pcx"),
    ];
    let owner = civ3_assets::pcx::decode(&files.read(&format!("Art/Units/Palettes/ntp{OWNER_TABLE:02}.pcx")).unwrap()).unwrap().palette;
    // Cattle: resource icon 19 (column 1, row 3), its 48x49 cells at
    // (1 + 50c, 1 + 50r); the tile at (64, 23) of the picture, the icon at
    // (+40, +8) (docs §6.7). Positions as traced in the box (M1-popup).
    let cattle = [1 + 50, 1 + 150, 48, 49];
    let terrain = |cell: i32| [128 * (cell % 9), 64 * (cell / 9), 128, 64];
    let draws = [
        Draw { art: 0, cell: terrain(40), at: (64, -9), kind: Kind::Plain },
        Draw { art: 0, cell: terrain(41), at: (0, 23), kind: Kind::Plain },
        Draw { art: 0, cell: terrain(49), at: (128, 23), kind: Kind::Plain },
        Draw { art: 0, cell: terrain(50), at: (64, 55), kind: Kind::Plain },
        Draw { art: 1, cell: [128 * 2, 64, 128, 64], at: (64, 39), kind: Kind::Plain },
        Draw { art: 2, cell: [128 * 5, 0, 128, 64], at: (64, 23), kind: Kind::Plain },
        // Borders: the tile's own to its NE and SE neighbours (sprites 2
        // and 6 at (px, py - 8), docs §6.9), and a neighbour's cut by the
        // canvas's top edge.
        Draw { art: 5, cell: [0, 72, 128, 72], at: (64, 15), kind: Kind::Colored },
        Draw { art: 5, cell: [0, 72 * 3, 128, 72], at: (64, 15), kind: Kind::Colored },
        Draw { art: 5, cell: [0, 72, 128, 72], at: (128, -17), kind: Kind::Colored },
        Draw { art: 3, cell: cattle, at: (104, 31), kind: Kind::Shadow },
        Draw { art: 4, cell: cattle, at: (104, 31), kind: Kind::Plain },
        Draw { art: 6, cell: [0, 0, SIZE.0, SIZE.1], at: (0, 0), kind: Kind::Plain },
    ];
    let game = game_picture(&arts, &draws, &owner);
    let before = rendered(&mirrored(&arts, &draws, false), &files);
    let after = rendered(&mirrored(&arts, &draws, true), &files);
    if let Some(dir) = std::env::var_os("C3R_TILEINFO_EVIDENCE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        save(&dir, "tileinfo-game.png", &game);
        save(&dir, "tileinfo-before.png", &before);
        save(&dir, "tileinfo-after.png", &after);
    }
    // The pixels the cattle's shadow darkens (under the frame and the icon
    // they stay as the game left them).
    let shadow_art = &arts[3];
    let [sx, sy, sw, sh] = sliced(shadow_art, cattle).unwrap();
    let shadow: Vec<usize> = (0..sh).flat_map(|j| (0..sw).map(move |i| (i, j)))
        .filter(|&(i, j)| (248..252).contains(&shadow_art.pcx.get((sx + i) as u32, (sy + j) as u32)))
        .map(|(i, j)| ((31 + sy - cattle[1] + j) * SIZE.0 + 104 + sx - cattle[0] + i) as usize)
        .collect();
    let differ = |picture: &[[u8; 3]], at: &mut dyn Iterator<Item = usize>| at.filter(|&i| picture[i] != game[i]).count();
    let all = |picture: &[[u8; 3]]| differ(picture, &mut (0..game.len()));
    let shadow_before = differ(&before, &mut shadow.iter().copied());
    eprintln!("tile information picture: {} pixels, {} of the shadow; differ before {} ({shadow_before} of the shadow), after {}", game.len(), shadow.len(), all(&before), all(&after));
    assert!(shadow.len() > 50, "the cattle cell has its shadow");
    assert!(shadow_before > shadow.len() / 2, "the old mirror drew the shadow in its file's colours");
    assert_eq!(differ(&after, &mut shadow.iter().copied()), 0, "shadow pixels darken what is below them as the game's shadow tables do");
    if let Some(i) = (0..game.len()).find(|&i| after[i] != game[i]) {
        panic!("mirrored picture differs from the game's blits at ({}, {}): {:?} vs {:?}; {} pixels", i as i32 % SIZE.0, i as i32 / SIZE.0, after[i], game[i], all(&after));
    }
    // Without the frame, all of the canvas (the border cut at its top too).
    let unframed = &draws[..draws.len() - 1];
    let (game, after) = (game_picture(&arts, unframed, &owner), rendered(&mirrored(&arts, unframed, true), &files));
    let before = rendered(&mirrored(&arts, unframed, false), &files);
    if let Some(dir) = std::env::var_os("C3R_TILEINFO_EVIDENCE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        save(&dir, "tileinfo-unframed-game.png", &game);
        save(&dir, "tileinfo-unframed-before.png", &before);
        save(&dir, "tileinfo-unframed-after.png", &after);
    }
    let count = |picture: &[[u8; 3]]| (0..game.len()).filter(|&i| picture[i] != game[i]).count();
    eprintln!("unframed: differ before {}, after {}", count(&before), count(&after));
    if let Some(i) = (0..game.len()).find(|&i| after[i] != game[i]) {
        panic!("unframed picture differs from the game's blits at ({}, {}): {:?} vs {:?}; {} pixels", i as i32 % SIZE.0, i as i32 / SIZE.0, after[i], game[i], count(&after));
    }
}
