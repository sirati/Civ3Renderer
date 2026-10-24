//! The render error notice in the game's message area, below the chat
//! lines, with the game's corner buttons and fonts, at 1× and 2×, drawn by
//! the viewer's planned frame (`tile_viewer::render_planned_with_notice`).
//! CIV3_INSTALL selects the installation.
//! nix develop -c cargo run -p civ3-bridge --example render_error_shots [OUT_DIR]
use std::path::Path;

use civ3_assets::GameFiles;
use ifc23::{assets, interface_parts, scene_map, text::{Fonts, TextRun}, ui::{Frame, ScreenDraw}, ui_screen};
use menu::screen::ScreenArt;

const BUTTONS: &str = "Art/interface/menuButtons.pcx";

fn run(x: i32, y: i32, color: [u8; 3], text: &str) -> TextRun {
    TextRun { x, y, color, face: "Lucida Sans".into(), height: -14, weight: 400, italic: false, underline: false, text: text.into(), backdrop: None }
}

/// The three corner buttons (menu, Civilopedia, advisors), where the game
/// draws them.
fn buttons() -> Frame {
    let image = |src: [i32; 4], x, y| ScreenDraw::Image { path: BUTTONS.into(), rect: src, x, y, keyed: false, clip: [0, 0, 1024, 768], mask: None, palette: None, opacity: 255 };
    Frame { draws: vec![image([1, 1, 36, 30], 21, 12), image([37, 1, 36, 30], 57, 12), image([74, 1, 33, 30], 94, 12)], ..Default::default() }
}

/// `lines` chat and log lines in the column below the buttons, white with a
/// dark shadow.
fn chat(lines: &[&str]) -> Frame {
    let mut draws = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let y = 54 + 18 + 18 * i as i32;
        for (dx, color) in [(1, [0, 0, 0]), (0, [255, 255, 255])] {
            draws.push(ScreenDraw::Text { run: run(12 + dx, y + dx, color, line), clip: [0, 0, 1024, 768] });
        }
    }
    Frame { draws, ..Default::default() }
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/c3r-rendererr-shots".into());
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    let files = GameFiles::complete(&install);
    let few = ["Caesar: Shall we trade Bronze Working for your Pottery?", "Babylon has completed the Pyramids.", "Hammurabi: Agreed."];
    let many: Vec<String> = (0..24).map(|i| format!("Message {}: the chat goes on, line after line, down the map.", i + 1)).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    let cases: [(&str, &[&str]); 3] = [("no-chat", &[]), ("chat", &few), ("many-chat", &many)];
    let items = |frame: &Frame| ui_screen::items(frame, files.roots(), &[], 0);
    let mut fonts = Fonts::default();
    fonts.add(&files, &[run(0, 0, [255, 255, 255], "")]);
    let mut art = ScreenArt::default();
    ui_screen::add_art(&items(&buttons()), &mut art, &mut fonts, &files);
    for (_, lines) in &cases {
        ui_screen::add_art(&items(&chat(lines)), &mut art, &mut fonts, &files);
    }
    let desc = format!("{}{}{}", assets::BUILTIN_DESC, ui_screen::description(&art), fonts.description());
    let compiled = assets::compile(&desc, files.clone()).unwrap();
    assert!(compiled.font(tile_viewer::host_status::FONT).is_some(), "the game's font as an outline face");
    let gpu = tile_render::Headless::new().unwrap();
    let config = tile_viewer::ViewerConfig { clear: [58, 86, 44, 255], ..Default::default() };
    for (name, lines) in cases {
        let mut scene = scene::Scene::default();
        scene.screen_size = (1024, 768);
        let corner = ui_screen::sprites(&items(&buttons()), &fonts, &compiled, u32::MAX, true);
        interface_parts::part_view(&mut scene, interface_parts::CORNER_VIEW, interface_parts::corner_region([21, 12, 106, 30]), corner);
        scene.interface.roaming = Some(interface_parts::CORNER_VIEW);
        scene.sprites = ui_screen::sprites(&items(&chat(lines)), &fonts, &compiled, u32::MAX, true).into_iter().map(|s| scene::FreeSprite { screen: true, after_pass: u32::MAX, ..s }).collect();
        scene_map::overlay_screen_sprites(&mut scene);
        for scale in [1u32, 2] {
            let mut resolution = viewer_settings::Resolution::default();
            resolution.base_ui_scale = scale as f32;
            let window = (1024 * scale, 768 * scale);
            let errors = if name == "many-chat" { 1234 } else { 3 };
            let img = tile_viewer::render_planned_with_notice(&gpu, &compiled, &scene, window, &resolution, &viewer_settings::Layout::default(), &config, Some(errors));
            let path = dir.join(format!("{name}-{scale}x.png"));
            image::save_buffer(&path, img.as_bytes(), img.width, img.height, image::ExtendedColorType::Rgba8).unwrap();
            println!("{}", path.display());
        }
    }
}
