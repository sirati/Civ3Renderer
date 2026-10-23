//! Pictures of the system cursors the viewer makes from the game's art
//! (`tile_viewer::cursor::Art::native_image`): the arrow, the arrow with
//! its busy badge turning, a special game cursor with the badge and the
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
    let text = format!("{DESCRIPTION}\n{}", include_str!("../../../assets/civ3/loading-cursor.toml"));
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
