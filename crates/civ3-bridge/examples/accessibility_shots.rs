//! Read-only visual fixtures using the real game art and production scene
//! paths. CIV3_INSTALL selects the installation; output defaults to /tmp.
//! nix develop -c cargo run -p civ3-bridge --example accessibility_shots
use std::{path::Path, time::Duration};
use civ3_assets::GameFiles;
use ifc23::{assets, city_labels, prefs, text::{Fonts, TextRun}, ui::{Frame, ScreenDraw}, ui_screen, units,
    world::{Civ3Tile, Civ3Unit, Civ3World}, world_items::WorldItems, rings::Rings};
use menu::{panel::{self, Panels, PanelState}, screen::ScreenArt};
use render_desc::Compiled;
use tile_render::{Camera, RgbaImage, reference};
use viewer_settings::{Settings, accessibility::Accessibility};

fn save(dir: &Path, name: &str, img: &RgbaImage) {
    let path = dir.join(format!("{name}.png"));
    image::save_buffer(&path, img.as_bytes(), img.width, img.height, image::ExtendedColorType::Rgba8).unwrap();
    println!("{}", path.display());
}
fn run(style: &panel::TextStyle, text: &str) -> TextRun {
    TextRun { x: 0, y: 0, color: style.color, face: style.face.clone(), height: -(style.height as i32),
        weight: style.weight, italic: style.italic, underline: false, text: text.into(), backdrop: None }
}
fn draw(compiled: &Compiled, sprites: Vec<scene::FreeSprite>, size: (u32, u32), camera: Camera, arrow: Option<Duration>) -> RgbaImage {
    let mut scene = scene::Scene::empty(scene::WorldShape { cols: 1, rows: 1, wrap_x: false, wrap_y: false }, compiled.layers.len());
    scene.sprites = sprites;
    let mut world = tile_viewer::scene_world(compiled, &scene);
    if let Some(time) = arrow {
        world.free.extend(tile_viewer::marker_sprites(compiled, &[tile_viewer::layout::Marker::Focus((128, 176))], 1, time, true));
    }
    reference::render_views(&world, &compiled.atlas, &[camera], size, 0, [105, 119, 73, 255])
}
fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/c3r-access-shots".into());
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    let files = GameFiles::complete(&install);
    let panels = Panels::parse(prefs::PANELS).unwrap();
    let p = panels.panel("preferences").unwrap();
    let mut settings = Settings::default();
    settings.accessibility.active_unit_arrow = true;
    settings.accessibility.unit_rings = true;
    let state = PanelState { values: prefs::values(&settings, prefs::TAB_ACCESSIBILITY, (1024, 768)), ..Default::default() };
    let mut fonts = Fonts::default();
    for style in panels.text_styles() {
        fonts.add(&files, &[run(&style, "")]);
    }
    let measure = |style: &panel::TextStyle, text: &str| fonts.metrics(&run(style, text)).map_or_else(|| panel::estimate(style, text), |(width, ascent, descent)| panel::TextMetrics { width, ascent, descent });
    let panel_draws = panels.draw(p, &state, [0, 0, 1024, 768], &measure);
    let bg = Frame { draws: vec![ScreenDraw::Image { path: "Art/interface/x_pref background.pcx".into(), rect: [0,0,1024,768],
        x: 0, y: 0, keyed: false, clip: [0,0,1024,768], mask: None, palette: None, opacity: 255 }], ..Default::default() };
    let mut panel_items = ui_screen::items(&bg, &[], &[], 0);
    panel_items.extend(prefs::items(&panel_draws));
    let mut screen = ScreenArt::default();
    ui_screen::add_art(&panel_items, &mut screen, &mut fonts, &files);
    let mut world = Civ3World { width: 8, height: 8, player: 1, color_tables: vec![0, 3, 1], preferences: 0x800,
        unit_type_keys: vec!["PRTO_Warrior".into()], unit_type_stats: vec![[1,1,1,0,0,0,0]], experience_hit_points: vec![3],
        tiles: vec![Civ3Tile { city: -1, ..Default::default() }; 32],
        relations: vec![civ3_sys::world::Relation::None, civ3_sys::world::Relation::Peace, civ3_sys::world::Relation::War],
        ..Default::default() };
    world.units = vec![Civ3Unit { id: 1, x: 0, y: 0, civ: 2, pixel: (128,176), animation: 1, direction: 4, visible: true, state: 1, ..Default::default() }];
    // Include own-unit art as well, for the native movement LED fixtures.
    world.units.push(Civ3Unit { id: 2, civ: 1, ..world.units[0].clone() });
    let mut art = units::UnitArt::new(files.clone());
    let needed = units::Needed::of(&world, &mut art);
    world.units.pop();
    world.unit_tiles = vec![(0,0,-1); 3];
    let mut env = city_labels::FileEnv::new(Box::new(files.clone()));
    let mut labels = Vec::new();
    for size in 0..4 {
        for count in [2,3] {
            world.unit_tiles = vec![(0,0,-1); count];
            labels.extend(units::stack_items(&world, false, &Accessibility { hud_size: size, stack_count: true, ..Default::default() }, &mut env));
        }
    }
    ui_screen::add_art(&labels, &mut screen, &mut fonts, &files);
    let desc = format!("{}{}{}{}{}{}", assets::BUILTIN_DESC, needed.description(), Rings::all_description(), panels.description(),
        ui_screen::description(&screen), fonts.description());
    let compiled = assets::compile(&desc, files).unwrap();
    let panel_sprites = ui_screen::sprites(&panel_items, &fonts, &compiled, u32::MAX, false);
    save(dir, "accessibility-tab", &draw(&compiled, panel_sprites, (1024,768), Camera::new((1024,768)), None));
    let size = (256,224);
    let camera = Camera::new(size);
    let ordinary = Accessibility::default();
    for (name, phase) in [("arrow-low",325), ("arrow-high",975)] {
        let sprites = units::free_sprites_accessible(&world, &mut art, &compiled, None, &ordinary);
        save(dir, name, &draw(&compiled, sprites, size, camera, Some(Duration::from_millis(phase))));
    }
    for (name, relation, invert) in [("ring-war",civ3_sys::world::Relation::War,false), ("ring-allied",civ3_sys::world::Relation::Defense,false),
        ("ring-peace",civ3_sys::world::Relation::Peace,false), ("ring-invert",civ3_sys::world::Relation::War,true)] {
        world.relations[2] = relation;
        let a = Accessibility { unit_rings: true, war: viewer_settings::accessibility::RingColor { invert, ..Default::default() }, ..Default::default() };
        let rings = Rings::configured(&a, None);
        let sprites = units::free_sprites_accessible(&world, &mut art, &compiled, rings.as_ref(), &a);
        save(dir, name, &draw(&compiled, sprites, size, camera, None));
    }
    world.units[0].civ = 1;
    for (index, name) in [(0,"hud-1x"),(1,"hud-1_5x"),(2,"hud-2x"),(3,"hud-3x")] {
        let a = Accessibility { hud_size: index, ..Default::default() };
        let sprites = units::free_sprites_accessible(&world, &mut art, &compiled, None, &a);
        save(dir, name, &draw(&compiled, sprites, size, camera, None));
        let sprites = units::free_sprites_accessible(&world, &mut art, &compiled, None, &a);
        let reduced = Camera { downscale: 2, zoom: (2,1), ..camera };
        save(dir, &format!("{name}-native-reduced"), &draw(&compiled, sprites, size, reduced, None));
    }
    for count in [2,3] {
        world.unit_tiles = vec![(0,0,-1); count];
        for enabled in [false,true] {
            let a = Accessibility { stack_count: enabled, ..Default::default() };
            let items = WorldItems::new(false, |reduced| units::stack_items(&world, reduced, &a, &mut env));
            let mut sprites = units::free_sprites_accessible(&world, &mut art, &compiled, None, &a);
            sprites.extend(items.sprites(&fonts, &compiled, units::unit_passes(&compiled).0, false));
            save(dir, &format!("stack-{count}-{}", if enabled { "label" } else { "lines" }), &draw(&compiled, sprites, size, camera, None));
        }
    }
    // Small comparisons retain native pixels, with a little space around
    // the artwork, so all states can be reviewed side by side.
    for (name, shots, crop) in [
        ("arrow-comparison", vec!["arrow-low", "arrow-high"], [64,32,128,176]),
        ("rings-comparison", vec!["ring-war", "ring-allied", "ring-peace", "ring-invert"], [80,112,96,96]),
        ("hud-comparison", vec!["hud-1x", "hud-2x", "hud-3x"], [80,96,96,112]),
        ("stack-comparison", vec!["stack-2-lines", "stack-2-label", "stack-3-lines", "stack-3-label"], [80,112,112,96]),
    ] {
        let images: Vec<_> = shots.iter().map(|n| image::open(dir.join(format!("{n}.png"))).unwrap().into_rgba8()).collect();
        let [x,y,w,h] = crop;
        let result = RgbaImage::from_fn(w*images.len() as u32, h, |px,py| images[(px/w) as usize].get_pixel(x+px%w,y+py).0);
        save(dir, name, &result);
    }
}
