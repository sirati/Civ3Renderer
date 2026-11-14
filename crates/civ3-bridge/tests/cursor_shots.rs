//! Pictures of the system cursors the viewer makes from the game's art
//! (`tile_viewer::cursor::Art::native_image`): the arrow, the arrow with
//! its busy badge running, a special game cursor with the badge and the
//! stuck hourglass, at cursor sizes 32 and 64, beside a drawing of
//! Windows XP's "Working in Background" pointer for comparison.
//!
//! Needs the game's files (`WINEPREFIX`, else the dataset prefix):
//! `CIV3R_CURSOR_SHOTS=/tmp/c3r-cursor-shots cargo test -p civ3-bridge
//! --test cursor_shots -- --ignored`.

use image::{Rgba, RgbaImage};
use tile_viewer::cursor::{Art, Image, Scale, Shown};

const DESCRIPTION: &str = r#"
grid = { layout = "orthogonal", step = [1, 1] }
[images.cursors]
source = "Art/cursor.pcx"
[sheets.cursors]
image = "cursors"
cell = [32, 32]
grid = [23, 1]
origin = [1, 1]
spacing = [1, 0]
anchor_marks = [228, 152, 24]
group = "Viewer@cursors"
[sheets.capture_cursor]
image = "cursors"
cell = [32, 32]
grid = [23, 1]
origin = [1, 1]
spacing = [1, 0]
only = [9]
anchor_marks = [228, 152, 24]
group = "Viewer@capture"
"#;

fn art() -> (Art, render_desc::Compiled) {
    let prefix = std::env::var("WINEPREFIX").unwrap_or_else(|_| "/mnt/datasets/winvm/wine-civ3".into());
    let root = std::path::Path::new(&prefix).join("drive_c/Games");
    let files = civ3_assets::GameFiles::with_roots(vec![root.join("Civilization III Complete"), root]);
    let text = format!("{DESCRIPTION}\n{}", civ3_ui::loading_cursor::description());
    let desc = render_desc::Description::parse(&text).unwrap();
    let compiled = desc.compile(&civ3_assets::loader::Civ3Loader::new(files), 1024).unwrap();
    (Art::of(&compiled), compiled)
}

fn to_png(image: &Image) -> RgbaImage {
    RgbaImage::from_raw(image.size.0.into(), image.size.1.into(), image.rgba.clone()).unwrap()
}

/// Windows XP's "Working in Background" pointer (IDC_APPSTARTING, the
/// standard scheme), drawn from memory: the 12×19 arrow and the hourglass
/// at its lower right, 32×32, hot spot (0, 0).
fn xp_reference() -> Image {
    const ROWS: [&str; 22] = [
        "B                       ",
        "BB                      ",
        "BWB                     ",
        "BWWB                    ",
        "BWWWB                   ",
        "BWWWWB                  ",
        "BWWWWWB                 ",
        "BWWWWWWB                ",
        "BWWWWWWWB               ",
        "BWWWWWWWWB BBBBBBBBB    ",
        "BWWWWWBBBBBBWWWWWWWBB   ",
        "BWWBWWB    BBBBBBBBB    ",
        "BWB BWWB    BWWWWWB     ",
        "BB  BWWB    BWYYYWB     ",
        "B    BWWB    BWYWB      ",
        "     BWWB     BYB       ",
        "      BWWB   BWWYWB     ",
        "      BWWB  BWWWYWWB    ",
        "       BB   BWWYYYWB    ",
        "            BWYYYYYB    ",
        "           BBBBBBBBBB   ",
        "           BWWWWWWWWB   ",
    ];
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for (y, row) in ROWS.iter().enumerate() {
        for (x, c) in row.chars().enumerate() {
            let p = match c { 'B' => [0, 0, 0, 255], 'W' => [255, 255, 255, 255], 'Y' => [214, 182, 82, 255], _ => continue };
            rgba[(y * 32 + x) * 4..][..4].copy_from_slice(&p);
        }
    }
    // Close the hourglass's bottom bar.
    for x in 11..21 { rgba[(22 * 32 + x) * 4..][..4].copy_from_slice(&[0, 0, 0, 255]); }
    Image { rgba, size: (32, 32), hotspot: (0, 0) }
}

/// Each image `zoom`× (nearest pixel) on a game-like backdrop, in a row,
/// its hot spot marked by a magenta outline; with a caption bar width.
fn sheet(images: &[Image], zoom: u32) -> RgbaImage {
    let pad = 8 * zoom;
    let cell = images.iter().map(|i| i.size.0.max(i.size.1) as u32).max().unwrap_or(1) * zoom + 2 * pad;
    let mut out = RgbaImage::from_fn(cell * images.len() as u32, cell, |x, y| {
        // A dark-green and grass backdrop with a light half, as the map
        // and the parchment panels behind the pointer.
        if (x % cell) < cell / 2 { if ((x / 8) + (y / 8)) % 2 == 0 { Rgba([46, 92, 38, 255]) } else { Rgba([58, 108, 46, 255]) } }
        else { Rgba([214, 196, 150, 255]) }
    });
    for (k, image) in images.iter().enumerate() {
        let (ox, oy) = (k as u32 * cell + pad, pad);
        for y in 0..image.size.1 as u32 * zoom {
            for x in 0..image.size.0 as u32 * zoom {
                let i = (((y / zoom) * image.size.0 as u32 + x / zoom) * 4) as usize;
                let p = &image.rgba[i..i + 4];
                let a = p[3] as u32;
                let under = out.get_pixel(ox + x, oy + y).0;
                let mix = |c: usize| ((p[c] as u32 * a + under[c] as u32 * (255 - a)) / 255) as u8;
                out.put_pixel(ox + x, oy + y, Rgba([mix(0), mix(1), mix(2), 255]));
            }
        }
        // The hot spot: a magenta box around its pixel.
        let (hx, hy) = (ox + image.hotspot.0 as u32 * zoom, oy + image.hotspot.1 as u32 * zoom);
        for d in 0..zoom + 2 {
            for (x, y) in [(hx + d, hy), (hx + d, hy + zoom + 1), (hx, hy + d), (hx + zoom + 1, hy + d)] {
                if x >= 1 && y >= 1 { out.put_pixel(x - 1, y - 1, Rgba([255, 0, 255, 255])); }
            }
        }
        // The image's box, faint.
        for x in 0..image.size.0 as u32 * zoom {
            for y in [0, image.size.1 as u32 * zoom - 1] { blend(&mut out, ox + x, oy + y); }
        }
        for y in 0..image.size.1 as u32 * zoom {
            for x in [0, image.size.0 as u32 * zoom - 1] { blend(&mut out, ox + x, oy + y); }
        }
    }
    out
}

fn blend(out: &mut RgbaImage, x: u32, y: u32) {
    let p = out.get_pixel(x, y).0;
    out.put_pixel(x, y, Rgba([((p[0] as u32 + 255) / 2) as u8, ((p[1] as u32 + 128) / 2) as u8, ((p[2] as u32 + 255) / 2) as u8, 255]));
}

#[test]
#[ignore = "needs the game's files; writes pictures"]
fn cursor_shots() {
    let dir = std::path::PathBuf::from(std::env::var("CIV3R_CURSOR_SHOTS").unwrap_or_else(|_| "/tmp/c3r-cursor-shots".into()));
    std::fs::create_dir_all(&dir).unwrap();
    let (art, compiled) = art();
    let set = |name: &str| compiled.set(name).unwrap().0.raw();
    let arrow = set("Viewer@cursors/0");
    // Cell 5, the "not here" circle, cell 9 (the capture crosshair) and cell 4.
    let special = [set("Viewer@cursors/5"), set("Viewer@cursors/9"), set("Viewer@cursors/4")];
    let at = |frame: u64| std::time::Duration::from_millis(frame * scene::loading_cursor::FRAME_MS);
    for size in [32, 64] {
        let scale = Scale::new(size, 1);
        let image = |shown: Shown| art.native_image(shown, scale).unwrap_or_else(|| panic!("{shown:?}"));
        let normal = image(Shown::Custom(arrow));
        let busy: Vec<Image> = [0, 1, 3, 6, 9].iter().map(|&k| image(art.badged(arrow, at(k)).unwrap())).collect();
        let specials: Vec<Image> = special.iter().map(|&s| image(art.badged(s, at(2)).unwrap())).collect();
        let stuck: Vec<Image> = [0, 3].iter().map(|&k| image(Shown::Custom(art.busy_cursor(at(k)).unwrap()))).collect();
        let xp = Scale::new(size, 1).native(&xp_reference());
        to_png(&normal).save(dir.join(format!("arrow-{size}.png"))).unwrap();
        for (k, b) in [0, 1, 3, 6, 9].iter().zip(&busy) {
            to_png(b).save(dir.join(format!("arrow-busy-frame{k}-{size}.png"))).unwrap();
        }
        for (s, b) in special.iter().zip(&specials) {
            to_png(b).save(dir.join(format!("special-{s}-busy-{size}.png"))).unwrap();
        }
        to_png(&stuck[0]).save(dir.join(format!("stuck-{size}.png"))).unwrap();
        let zoom = if size == 32 { 6 } else { 3 };
        let mut all = vec![normal.clone()];
        all.extend(busy.iter().cloned());
        sheet(&all, zoom).save(dir.join(format!("sheet-busy-{size}.png"))).unwrap();
        // The whole cycle: the sand drains in eleven steps, then the flip.
        let cycle: Vec<Image> = (0..12).map(|k| image(art.badged(arrow, at(k)).unwrap())).collect();
        sheet(&cycle, zoom).save(dir.join(format!("cycle-{size}.png"))).unwrap();
        let mut rest = specials.clone();
        rest.extend(stuck.iter().cloned());
        sheet(&rest, zoom).save(dir.join(format!("sheet-special-stuck-{size}.png"))).unwrap();
        sheet(&[xp, busy[0].clone(), busy[3].clone()], zoom).save(dir.join(format!("xp-side-by-side-{size}.png"))).unwrap();
        // The viewer's own drawing of the same pointers (captured mouse,
        // button snapping: `compose_with_busy`, rendered by the CPU
        // reference renderer), beside their system cursor images.
        let side = size * 2;
        let mut drawn = RgbaImage::new(side * 4, side);
        for (k, base) in [arrow, special[0], special[1], special[2]].into_iter().enumerate() {
            let set = tile_render::SpriteSetId::from_raw(base);
            let set_art = tile_viewer::cursor::SetArt::of(&compiled, set).unwrap();
            let hot = Scale::new(size, 1).native_hotspot(set_art.hotspot, (32, 32));
            let at = (size as i32 / 4 + hot.0 as i32, size as i32 / 4 + hot.1 as i32);
            let marks = tile_viewer::cursor::Marks { window: vec![at], ..Default::default() };
            let out = tile_viewer::cursor::compose_with_busy(&compiled, &art, set_art, &marks, &[], &[], scale, (side, side), 0, (0, 0), tile_viewer::host_status::Responsiveness::Busy);
            let mut world = tile_render::TileWorld::new(tile_render::Grid { cols: 1, rows: 1, layout: tile_render::Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (8, 8) }, compiled.sets.clone());
            world.free = out.sprites;
            let mut cameras = vec![tile_viewer::cursor::overlay_camera((side, side))];
            cameras.extend(out.top);
            let picture = tile_render::reference::render_views(&world, &compiled.atlas, &cameras, (side, side), 0, [46, 92, 38, 255]);
            for y in 0..side { for x in 0..side { drawn.put_pixel(k as u32 * side + x, y, Rgba(picture.get(x, y))); } }
        }
        let drawn = image::imageops::resize(&drawn, side * 4 * zoom / 2, side * zoom / 2, image::imageops::FilterType::Nearest);
        drawn.save(dir.join(format!("drawn-busy-{size}.png"))).unwrap();
        // At its real size, as it shows on screen.
        sheet(&[normal, busy[0].clone(), busy[2].clone(), specials[0].clone(), stuck[0].clone()], 1).save(dir.join(format!("actual-size-{size}.png"))).unwrap();
    }
}

/// The game's Civilopedia book (cell 13) as the system cursor and as the
/// viewer's drawing: the arrow's tip its hot spot, at 1× and 2×, the busy
/// badge beside the book without covering the tip. Skipped without the
/// game's files.
#[test]
fn the_civilopedia_pointer_from_the_games_art() {
    let prefix = std::env::var("WINEPREFIX").unwrap_or_else(|_| "/mnt/datasets/winvm/wine-civ3".into());
    if !std::path::Path::new(&prefix).join("drive_c/Games/Civilization III Complete/Art/Cursor.pcx").exists() {
        return eprintln!("no game files, skipped");
    }
    let (art, compiled) = art();
    let book = compiled.set(&ifc23::cursor::set_name(ifc23::cursor::CIVILOPEDIA)).unwrap().0.raw();
    let arrow = compiled.set("Viewer@cursors/0").unwrap().0.raw();
    // Its own pointer, not the arrow; a special one (busy: badge, never the
    // hourglass in its place).
    assert!(art.special(scene::Cursor::Sprite(book)));
    assert_eq!(art.system_shown(scene::Cursor::Sprite(book)), scene::Cursor::Sprite(book));
    let plain = art.get(book).unwrap();
    assert_eq!((plain.size, plain.hotspot), ((32, 32), (1, 1)));
    for (size, n) in [(32u32, 1u16), (64, 2)] {
        let scale = Scale::new(size, 1);
        let image = art.native_image(Shown::Custom(book), scale).unwrap();
        assert_eq!((image.size, image.hotspot), ((32 * n, 32 * n), (n, n)), "{size}");
        // The tip shows: its pixel is the art's.
        assert_ne!(image.rgba[((n as usize * image.size.0 as usize) + n as usize) * 4 + 3], 0);
        let badged = art.native_image(art.badged(book, std::time::Duration::ZERO).unwrap(), scale).unwrap();
        assert_eq!(badged.hotspot, image.hotspot, "{size}: the badge keeps the hot spot");
        // The book's own pixels are all still there (the badge sits beside it).
        let (w, ow) = (badged.size.0 as usize, image.size.0 as usize);
        for y in 0..image.size.1 as usize {
            for x in 0..ow {
                let p = &image.rgba[(y * ow + x) * 4..][..4];
                if p[3] == 0 { continue; }
                assert_eq!(&badged.rgba[(y * w + x) * 4..][..4], p, "{size}: covered at {x},{y}");
            }
        }
    }
    let place = art.badge_for(book, (32, 32), (1, 1)).unwrap();
    assert_ne!(Some(place), art.badge_for(arrow, (32, 32), (1, 1)), "placed on the book's shape, not the arrow's");
}

/// The Civilopedia pointer over the F7 screen's Pyramids picture, as the
/// system cursor (1× and 2×, plain and busy) and as the viewer's drawing in
/// the Alt mode, on a viewer screenshot of the screen without a pointer
/// (`CIV3R_PEDIA_BACKDROP`, a PNG; the tip at `CIV3R_PEDIA_AT`, "x,y"; the
/// cell `CIV3R_PEDIA_CELL`, default the book, and the pictures' names
/// `CIV3R_PEDIA_NAME`, default `f7-pyramids`).
/// `CIV3R_CURSOR_SHOTS=/tmp/c3r-pedia-cursor-shots cargo test -p
/// civ3-bridge --test cursor_shots -- --ignored pedia`.
#[test]
#[ignore = "needs the game's files and a screenshot; writes pictures"]
fn pedia_cursor_shots() {
    let dir = std::path::PathBuf::from(std::env::var("CIV3R_CURSOR_SHOTS").unwrap_or_else(|_| "/tmp/c3r-pedia-cursor-shots".into()));
    std::fs::create_dir_all(&dir).unwrap();
    let backdrop = image::open(std::env::var("CIV3R_PEDIA_BACKDROP").expect("CIV3R_PEDIA_BACKDROP")).unwrap().to_rgba8();
    let at: Vec<i32> = std::env::var("CIV3R_PEDIA_AT").unwrap_or_else(|_| "645,385".into()).split(',').map(|v| v.trim().parse().unwrap()).collect();
    let (art, compiled) = art();
    let cell = std::env::var("CIV3R_PEDIA_CELL").ok().and_then(|v| v.parse().ok()).unwrap_or(ifc23::cursor::CIVILOPEDIA);
    let name_of = std::env::var("CIV3R_PEDIA_NAME").unwrap_or_else(|_| "f7-pyramids".into());
    let book = compiled.set(&ifc23::cursor::set_name(cell)).unwrap().0.raw();
    let around = |picture: &RgbaImage, zoom: u32| {
        let (x0, y0) = ((at[0] - 90).max(0) as u32, (at[1] - 70).max(0) as u32);
        let crop = image::imageops::crop_imm(picture, x0, y0, 260, 180).to_image();
        image::imageops::resize(&crop, 260 * zoom, 180 * zoom, image::imageops::FilterType::Nearest)
    };
    let put = |picture: &mut RgbaImage, image: &Image| {
        let origin = (at[0] - image.hotspot.0 as i32, at[1] - image.hotspot.1 as i32);
        for y in 0..image.size.1 as i32 {
            for x in 0..image.size.0 as i32 {
                let p = &image.rgba[((y * image.size.0 as i32 + x) * 4) as usize..][..4];
                let (px, py) = (origin.0 + x, origin.1 + y);
                if p[3] == 0 || px < 0 || py < 0 || px as u32 >= picture.width() || py as u32 >= picture.height() { continue; }
                let under = picture.get_pixel(px as u32, py as u32).0;
                let a = p[3] as u32;
                let mix = |c: usize| ((p[c] as u32 * a + under[c] as u32 * (255 - a)) / 255) as u8;
                picture.put_pixel(px as u32, py as u32, Rgba([mix(0), mix(1), mix(2), 255]));
            }
        }
    };
    for (size, name) in [(32, "1x"), (64, "2x")] {
        let scale = Scale::new(size, 1);
        for (busy, label) in [(false, "plain"), (true, "busy")] {
            let shown = if busy { art.badged(book, std::time::Duration::from_millis(2 * scene::loading_cursor::FRAME_MS)).unwrap() } else { Shown::Custom(book) };
            let image = art.native_image(shown, scale).unwrap();
            let mut picture = backdrop.clone();
            put(&mut picture, &image);
            picture.save(dir.join(format!("{name_of}-system-{name}-{label}.png"))).unwrap();
            around(&picture, 3).save(dir.join(format!("{name_of}-system-{name}-{label}-zoom3.png"))).unwrap();
        }
        // The viewer's own drawing (Alt mode, busy): `compose_with_busy`
        // through the CPU reference renderer, over the screenshot.
        let set = tile_render::SpriteSetId::from_raw(book);
        let set_art = tile_viewer::cursor::SetArt::of(&compiled, set).unwrap();
        let canvas = (backdrop.width(), backdrop.height());
        let marks = tile_viewer::cursor::Marks { window: vec![(at[0], at[1])], ..Default::default() };
        let out = tile_viewer::cursor::compose_with_busy(&compiled, &art, set_art, &marks, &[], &[], scale, canvas, 0, (0, 0), tile_viewer::host_status::Responsiveness::Busy);
        let mut world = tile_render::TileWorld::new(tile_render::Grid { cols: 1, rows: 1, layout: tile_render::Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (8, 8) }, compiled.sets.clone());
        world.free = out.sprites;
        let mut cameras = vec![tile_viewer::cursor::overlay_camera(canvas)];
        cameras.extend(out.top);
        let drawn = tile_render::reference::render_views(&world, &compiled.atlas, &cameras, canvas, 0, [0, 0, 0, 0]);
        let mut picture = backdrop.clone();
        for y in 0..canvas.1 {
            for x in 0..canvas.0 {
                let p = drawn.get(x, y);
                if p[3] == 0 { continue; }
                let under = picture.get_pixel(x, y).0;
                let a = p[3] as u32;
                let mix = |c: usize| ((p[c] as u32 * a + under[c] as u32 * (255 - a)) / 255) as u8;
                picture.put_pixel(x, y, Rgba([mix(0), mix(1), mix(2), 255]));
            }
        }
        around(&picture, 3).save(dir.join(format!("{name_of}-drawn-{name}-busy-zoom3.png"))).unwrap();
    }
}
