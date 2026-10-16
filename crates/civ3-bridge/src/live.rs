//! The reimplemented renderer, running inside the game: a publisher thread
//! turns the game's world and view into scenes, a viewer window draws them.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use civ3_sys::{Civ3, Civ3Error};
use render_desc::Compiled;

use crate::presenter::{self, Presenter};
use crate::{assets, city_hud, city_site, districts, text, trace, ui_screen, ui_trace, units};
use crate::scene_map::Mapper;
use crate::world::{Civ3City, Civ3Tile, Civ3Unit, Civ3View, Civ3World, IconBuilding, WallBuilding};

/// The viewer window's title (also how we tell it from the game's window).
const VIEWER_TITLE: &str = "Civ III (reimplemented renderer)";

/// Shortest time between two art rebuilds.
const RECOMPILE_EVERY: Duration = Duration::from_secs(2);

/// How often the publisher looks at the game.
const POLL: Duration = Duration::from_millis(33);

fn unit(u: &civ3_sys::DrawnUnit) -> Civ3Unit {
    Civ3Unit {
        id: u.id,
        x: u.x,
        y: u.y,
        civ: u.civ,
        unit_type: u.unit_type,
        experience: u.experience,
        damage: u.damage,
        moves: u.moves,
        container: u.container,
        state: u.state,
        always_on_top: u.always_on_top,
        pixel: u.pixel,
        direction: u.direction,
        animation: u.animation,
        frame: u.frame,
        visible: u.visible,
        army_member: Some(u.army_member).filter(|&m| m >= 0),
        army_size: u.army_size,
        moving: u.moving,
    }
}

/// A copy of the parts of the game state the map is drawn from.
/// How `player` stands with every civ (the barbarians, civ 0, are always
/// at war). The raw fields are logged when they change: not all treaty
/// bits are known (`civ3_sys::world::Diplomacy`).
fn relations(game: &Civ3, player: i32) -> Vec<civ3_sys::world::Relation> {
    static LAST: std::sync::Mutex<Option<civ3_sys::world::Diplomacy>> = std::sync::Mutex::new(None);
    let Ok(d) = game.diplomacy(player) else { return Vec::new() };
    if let Ok(mut last) = LAST.lock()
        && last.as_ref() != Some(&d)
    {
        let known: Vec<String> = (0..32)
            .filter(|&c| d.at_war[c] || d.contacts[c] != 0 || d.treaties[c] != 0 || d.military_allies[c] != 0)
            .map(|c| format!("{c}: war {} contacts {:#x} treaties {:#x} allies {:#x}", d.at_war[c] as u8, d.contacts[c], d.treaties[c], d.military_allies[c]))
            .collect();
        log::info!("diplomacy of civ {player}: {}", known.join("; "));
        *last = Some(d.clone());
    }
    (0..32).map(|c| if c == 0 { civ3_sys::world::Relation::War } else { d.relation(c) }).collect()
}

pub fn snapshot(game: &Civ3) -> Result<(Civ3World, Civ3View), Civ3Error> {
    let view = game.view()?;
    let map = game.map()?;
    let tiles = game
        .tiles(&map)?
        .iter()
        .map(|t| Civ3Tile {
            square_type: t.square_type(),
            square_parts: t.square_parts() as u16,
            overlays: t.overlays(),
            terrain_overlays: t.terrain_overlays(),
            river: t.river_code(),
            resource: t.resource_type(),
            owner: t.territory_owner(),
            city: t.city_id(),
            fow_status: t.fow_status(),
            visibility: t.visibility(),
            explored: t.explored(),
            v3: t.v3(),
            visibility_d0: t.visibility_d0(),
            visible_overlays: t.visible_overlays(),
            victory_location: t.victory_location(),
            building: Some(t.building_id()).filter(|&b| b >= 0),
            ruins: t.ruins(),
            city_area: t.city_area(),
            continent: t.continent(),
            district: None,
        })
        .collect();
    let district_state = districts::read(game, &map);
    let players = optional("players", game.players());
    let drawn = optional("units", game.drawn_units());
    // The members armies show (§10.11).
    let army_members = drawn
        .iter()
        .filter(|u| u.army_member >= 0)
        .filter_map(|u| game.unit(u.army_member).ok().flatten())
        .map(|m| unit(&m))
        .collect();
    let cities = optional("cities", game.cities())
        .into_iter()
        .map(|c| Civ3City {
            id: c.id,
            x: c.x,
            y: c.y,
            civ: c.civ,
            size: c.size,
            improvements: c.improvements,
            wonder_in_production: c.improvement_in_production().filter(|&i| district_state.is_wonder(i)),
        })
        .collect();
    let mut world = Civ3World {
        player: view.player_civ,
        resource_icons: game.resource_icons()?,
        resource_visible: game.resources_visible_to(view.player_civ)?,
        eras: players.eras,
        color_tables: players.color_tables,
        capitals: players.capitals,
        races: players.races,
        culture_groups: players.culture_groups,
        max_town: players.max_town,
        max_city: players.max_city,
        cities,
        governments: players.governments,
        units: drawn.iter().map(unit).collect(),
        army_members,
        unit_type_keys: optional("unit_type_keys", game.unit_type_keys()),
        era_keys: optional("era_keys", game.era_keys()),
        unit_type_stats: optional("unit_type_stats", game.unit_type_stats()),
        experience_hit_points: optional("experience_hit_points", game.experience_hit_points()),
        unit_tiles: optional("unit_tiles", game.unit_tiles()),
        preferences: optional("preferences", game.preferences()),
        mouse_tile: optional("mouse_tile", game.mouse_tile()),
        current_unit: optional("current_unit", game.current_unit()),
        focus_city: optional("focus_city", game.focus_city()),
        outposts: optional("outposts", game.outposts()),
        selection: optional("selection", game.selection()),
        relations: relations(game, view.player_civ),
        wall_buildings: players
            .wall_buildings
            .iter()
            .map(|b| WallBuilding { id: b.id, government: b.government, obsolete_for: b.obsolete_for })
            .collect(),
        icon_buildings: players
            .icon_buildings
            .iter()
            .map(|b| IconBuilding { id: b.id, flags: b.flags, obsolete_for: b.obsolete_for })
            .collect(),
        width: map.width,
        height: map.height,
        wrap_x: map.wrap_x,
        wrap_y: map.wrap_y,
        seed: map.seed,
        flags: map.flags,
        tiles,
        districts: Vec::new(),
        district_configs: district_state.configs,
        improvements: district_state.improvements,
        free_improvements: district_state.free_improvements,
    };
    world.set_districts(district_state.districts);
    let view = Civ3View {
        camera_x: view.camera_x,
        camera_y: view.camera_y,
        tile_x: view.tile_x,
        tile_y: view.tile_y,
        zoomed_out: game.zoomed_out().unwrap_or(false),
    };
    Ok((world, view))
}

/// Parts of the game state the map can do without when they cannot be
/// read (e.g. lists the game is rebuilding): their default, and a note.
fn optional<T: Default>(what: &str, read: Result<T, Civ3Error>) -> T {
    read.unwrap_or_else(|e| {
        log::debug!("snapshot: no {what}: {e}");
        T::default()
    })
}

#[derive(Debug, thiserror::Error)]
pub enum LiveError {
    #[error("render description {0}: {1}")]
    Desc(String, String),
    #[error("game folder: {0}")]
    Install(std::io::Error),
}

/// The game install root ("Civilization III Complete"): the parent of the
/// folder the running executable is in.
fn install_root() -> Result<PathBuf, LiveError> {
    let exe = std::env::current_exe().map_err(LiveError::Install)?;
    exe.parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .ok_or_else(|| LiveError::Install(std::io::Error::other("executable has no grandparent folder")))
}

/// The description text and a name for messages.
fn description() -> Result<(String, String), LiveError> {
    match std::env::var("CIV3R_DESC") {
        Ok(path) => {
            let text = std::fs::read_to_string(&path).map_err(|e| LiveError::Desc(path.clone(), e.to_string()))?;
            Ok((path, text))
        }
        Err(_) => Ok(("built-in civ3.toml".to_owned(), assets::BUILTIN_DESC.to_owned())),
    }
}

/// Where the game currently loads art from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ArtFolders {
    scenario: Option<String>,
    search: Vec<String>,
}

impl ArtFolders {
    fn of(game: &Civ3) -> Self {
        Self { scenario: game.scenario_folder().ok().flatten(), search: game.search_roots().unwrap_or_default() }
    }
}

struct Compiler {
    install: PathBuf,
    name: String,
    text: String,
    /// Rings around foreign units (their art is in `text`).
    rings: Option<crate::rings::Rings>,
}

impl Compiler {
    fn files(&self, folders: &ArtFolders) -> civ3_assets::GameFiles {
        assets::game_files(&self.install, folders.scenario.as_deref(), &folders.search)
    }

    /// The compiled description, and what a renderer compiling its own copy
    /// needs (the full text, the art roots).
    fn compile(
        &self,
        folders: &ArtFolders,
        units: &units::Needed,
        fonts: &text::Fonts,
        screen: &menu::screen::ScreenArt,
        districts: &crate::district_art::DistrictArt,
        zoomed_out: bool,
    ) -> Result<(Arc<Compiled>, scene::wire::Assets), LiveError> {
        let text = format!(
            "{}{}{}{}{}",
            self.text,
            units.description(),
            fonts.description(),
            ui_screen::description(screen),
            districts.description()
        );
        let files = self.files(folders);
        let wire = scene::wire::Assets {
            description: text.clone(),
            roots: files.roots().iter().map(|r| r.to_string_lossy().into_owned()).collect(),
            scale_down: if zoomed_out { 2 } else { 1 },
        };
        let compiled = assets::compile_scaled(&text, files, wire.scale_down).map_err(|e| LiveError::Desc(self.name.clone(), e.to_string()))?;
        let pages = &compiled.atlas.pages;
        let mb = pages.iter().map(|p| p.width as u64 * p.height as u64 * 4).sum::<u64>() >> 20;
        log::info!(
            "live renderer: {} layers, {} sprites, {} atlas pages ({mb} MiB) ({folders:?})",
            compiled.layers.len(),
            compiled.sprite_names.len(),
            pages.len()
        );
        Ok((Arc::new(compiled), wire))
    }
}

/// Starts the publisher and the viewer; returns at once. `CIV3R_VIEWER`:
/// `1` / `fullscreen` a window of this process, `native` /
/// `native-fullscreen` the native Linux viewer (Wine with
/// `civ3r_native.dll`, see `tools/install-native`).
pub fn start(game: Civ3) -> Result<(), LiveError> {
    let (name, mut text) = description()?;
    // Rings around foreign units, if asked for (`crate::rings`).
    let rings = crate::rings::Rings::from_env();
    text += &rings.as_ref().map(|r| r.description()).unwrap_or_default();
    let compiler = Compiler { install: install_root()?, name, text, rings };
    let folders = ArtFolders::of(&game);
    // Mirror the game's interface (`CIV3R_UI=off`: only its text).
    if std::env::var("CIV3R_UI").as_deref() != Ok("off") {
        // `Base_Form` is its vtable, then `Base_Form_Data`.
        let map_canvas = game.addrs.main_screen_form + 4 + civ3_sys::layout::BASE_FORM_DATA_CANVAS;
        // `CIV3R_UI_SKIP=off`: mirror the map canvas too (for finding out what
        // is drawn there).
        let inspect = std::env::var("CIV3R_UI_SKIP").as_deref() == Ok("off");
        // The map canvas carries the city labels (kept) over the fog tiles
        // (ours); the unit overlay carries discs, LEDs, bars (all ours).
        let overlay = civ3_sys::ui_hooks::unit_overlay_canvas(game.build);
        let (skip, underlays) = if inspect { (Vec::new(), Vec::new()) } else { (overlay.into_iter().collect(), vec![map_canvas]) };
        match ui_trace::install(&game, skip, underlays, vec!["fogofwar.pcx".into()]) {
            Ok(tracer) => std::mem::forget(tracer),
            Err(e) => log::warn!("not mirroring the interface: {e}"),
        }
    }
    let (compiled, wire) = compiler.compile(&folders, &units::Needed::default(), &text::Fonts::default(), &Default::default(), &Default::default(), false)?;
    let mode = std::env::var("CIV3R_VIEWER").unwrap_or_default();
    let mut presenter = if mode.starts_with("native") {
        let native = presenter::Native::load(mode == "native-fullscreen").map_err(|e| LiveError::Install(std::io::Error::other(e)))?;
        Presenter::native(native)
    } else {
        let config = tile_viewer::ViewerConfig {
            title: VIEWER_TITLE.into(),
            size: (1024, 768),
            fullscreen: mode == "fullscreen",
            overhang: (0, 128, 0, 128),
            ..Default::default()
        };
        Presenter::in_process(config, compiled.clone()).map_err(LiveError::Install)?
    };
    presenter.set_assets(&compiled, &wire);
    let compiled = Arc::new(compiled.without_pixels());
    std::thread::Builder::new()
        .name("civ3r-publish".into())
        .spawn(move || publish(game, &compiler, folders, compiled, presenter))
        .map_err(LiveError::Install)?;
    Ok(())
}

/// The game's menus in our renderer (`assets/civ3/menus.toml`).
const MENUS: &str = include_str!("../../../assets/civ3/menus.toml");

/// What the viewer shows: the map, or the title menu while no game runs.
#[derive(PartialEq)]
enum Shown {
    Map(Civ3World, Civ3View),
    Title(menu::MenuState),
}

/// Publishes a new scene whenever what the game shows changes.
fn publish(game: Civ3, compiler: &Compiler, mut folders: ArtFolders, mut compiled: Arc<Compiled>, mut presenter: Presenter) {
    let mut mapper = Mapper::new(&compiled);
    let menus_of = |c: &Compiled| menu::Menus::parse(MENUS, c).inspect_err(|e| log::error!("menus: {e}")).ok();
    let mut menus = menus_of(&compiled);
    let mut last: Option<(Shown, Vec<text::TextRun>, Option<crate::ui::Frame>)> = None;
    let mut seq = 0;
    let mut input = InputForwarder::default();
    let mut title = menu::MenuState::default();
    let mut art = units::UnitArt::new(compiler.files(&folders));
    let mut have = units::Needed::default();
    let mut fonts = text::Fonts::default();
    let mut screen_art = menu::screen::ScreenArt::default();
    let mut district_art = crate::district_art::DistrictArt::default();
    let mut zoomed_out = false;
    let mut table_colors = std::collections::HashMap::new();
    let mut art_pending = false;
    let mut last_compile = std::time::Instant::now() - RECOMPILE_EVERY;
    let mut city_bands = std::collections::HashMap::new();
    loop {
        std::thread::sleep(POLL);
        // A scenario brings its own art.
        let now = ArtFolders::of(&game);
        let mut recompile = false;
        if now != folders {
            folders = now;
            art = units::UnitArt::new(compiler.files(&folders));
            have = units::Needed::default();
            recompile = true;
        }
        let inputs = presenter.take_input();
        let shown = match snapshot(&game) {
            Ok((world, view)) => Shown::Map(world, view),
            // Between games: the title menu, which follows the pointer.
            Err(Civ3Error::NoGame) => {
                if let Some(m) = menus.as_ref().and_then(|ms| ms.menu("title").map(|m| (ms, m))) {
                    for i in &inputs {
                        m.0.input(m.1, &mut title, input.viewport, i);
                    }
                }
                Shown::Title(title.clone())
            }
            Err(e) => {
                log::debug!("snapshot: {e}");
                forward_input(&mut input, inputs);
                continue;
            }
        };
        ui_trace::set_skipping(matches!(shown, Shown::Map(..)));
        forward_input(&mut input, inputs);
        if input.viewport == (0, 0) {
            input.game_window();
        }
        // The game's interface, mirrored (else its text only).
        let ui = ui_trace::frame();
        let texts = if ui.is_some() { Vec::new() } else { trace::recent_texts() };
        let state = (shown, texts, ui);
        if last.as_ref() == Some(&state) && !recompile && !art_pending && input.look.is_none() {
            continue;
        }
        let (shown, texts, ui) = &state;
        let files = compiler.files(&folders);
        let generated = civ3_sys::ui_hooks::generated_surfaces(game.build);
        // Unit pictures in panels are drawn in the viewing player's colours.
        let unit_colors = match shown {
            Shown::Map(world, _) => world.color_tables.get(world.player.max(0) as usize).copied().unwrap_or(0),
            Shown::Title(_) => 0,
        };
        // An open city screen covers everything under it; the map shows in
        // its gap, as a view of its own (§16.8).
        let city = match (shown, ui) {
            (Shown::Map(world, _), Some(frame)) if world.focus_city.is_some() => {
                ui_screen::city_screen(frame, &files, files.roots(), &mut city_bands)
            }
            _ => None,
        };
        let visible_ui = match city {
            Some((_, first)) => ui.as_ref().map(|f| ui_screen::from_draw(f, first)),
            None => ui.clone(),
        };
        let ui_items = visible_ui.as_ref().map(|f| ui_screen::items(f, files.roots(), generated, unit_colors)).unwrap_or_default();
        // The viewer asked to see a world pixel: click it on the minimap (the
        // game centres its view there).
        if let (Some(look), Shown::Map(world, view)) = (input.look.take(), shown) {
            match crate::scene_map::tile_at(world, look, view.zoomed_out).and_then(|t| ui_screen::minimap_point(&ui_items, world, t)) {
                Some(p) => input.click(p),
                None => log::debug!("look at {look:?}: not on the minimap"),
            }
        }
        if let Some(f) = ui {
            let mut more = have.clone();
            for path in ui_screen::unit_files(f, files.roots()) {
                more.add_file(&mut art, &path, unit_colors);
            }
            if more != have {
                have = more;
                recompile = true;
            }
        }
        recompile |= ui_screen::add_art(&ui_items, &mut screen_art, &mut fonts, &files);
        // The zoomed-out view has its own (half-size) art.
        if let Shown::Map(_, view) = shown {
            if view.zoomed_out != zoomed_out {
                zoomed_out = view.zoomed_out;
                recompile = true;
            }
        }
        // Art the description does not have yet: units, text fonts.
        if let Shown::Map(world, _) = shown {
            let needed = units::Needed::of(world, &mut art);
            if !have.covers(&needed) {
                have.merge(&needed);
                recompile = true;
            }
        }
        recompile |= fonts.add(&compiler.files(&folders), texts);
        // The art of the districts on the map (C3X).
        if let Shown::Map(world, view) = shown {
            for d in &world.districts {
                let stages = crate::districts::stages(world, d.x, d.y, view.zoomed_out);
                let slices: Vec<_> = stages.buildings.iter().chain(&stages.resources).map(|s| crate::district_art::Slice::of(s, view.zoomed_out)).collect();
                recompile |= district_art.add(&slices);
            }
        }
        // Rebuild the art at most every `RECOMPILE_EVERY`: a new screen asks
        // for new art several times in a row, and each atlas is big (a 32-bit
        // process with a software renderer runs out of memory otherwise).
        art_pending |= recompile;
        if art_pending && last_compile.elapsed() >= RECOMPILE_EVERY {
            art_pending = false;
            match compiler.compile(&folders, &have, &fonts, &screen_art, &district_art, zoomed_out) {
                Ok((c, wire)) => {
                    presenter.set_assets(&c, &wire);
                    // The viewer has the pixels; this side needs the rest only.
                    compiled = Arc::new(c.without_pixels());
                    mapper = Mapper::new(&compiled);
                    menus = menus_of(&compiled);
                }
                Err(e) => log::error!("recompiling for {folders:?}: {e}"),
            }
            // From the end: on a slow machine a compile can take longer than
            // the interval itself.
            last_compile = std::time::Instant::now();
        }
        seq += 1;
        let mut scene = match shown {
            Shown::Map(world, view) => {
                let mut scene = mapper.scene(world, view, (0, 0), 0);
                // The mirrored interface has the game's own city-site lines.
                if ui.is_none() {
                    scene.sprites = city_site::sprites(world, &compiled);
                }
                scene.sprites.extend(units::sprites(world, &mut art, &compiled, compiler.rings.as_ref()));
                // The mirrored interface has the city icons already.
                if ui.is_none() {
                    scene.sprites.extend(city_hud::sprites(world, &compiled));
                }
                // The point of interest: the active unit.
                scene.focus = units::focus(world);
                if view.zoomed_out {
                    crate::scene_map::zoom_out(&mut scene.sprites);
                    scene.focus = scene.focus.map(|(x, y)| (x / 2, y / 2));
                }
                scene
            }
            Shown::Title(state) => {
                let mut scene = scene::Scene::empty(scene::WorldShape::default(), compiled.layers.len());
                // Our own title menu, unless the game's is mirrored.
                if let Some(m) = menus.as_ref().and_then(|ms| ms.menu("title").map(|m| (ms, m))).filter(|_| ui.is_none()) {
                    scene.sprites = m.0.sprites(m.1, state, input.viewport, u32::MAX);
                }
                scene
            }
        };
        scene.sprites.extend(fonts.sprites(texts, &compiled));
        scene.sprites.extend(ui_screen::sprites(&ui_items, &fonts, &compiled, ui_screen::under_pass(&compiled)));
        // Pictures the game makes itself: the minimap (§17).
        if let Shown::Map(world, _) = shown {
            scene.patches = ui_screen::minimap_patches(&ui_items, world, &compiled, |table| {
                *table_colors.entry(table).or_insert_with(|| art.table_color(table, 66).unwrap_or_default())
            });
        }
        crate::scene_map::overlay_screen_sprites(&mut scene);
        if let (Shown::Map(_, view), Some(([x, y, w, h], _))) = (shown, city) {
            crate::scene_map::city_view(&mut scene, (view.camera_x, view.camera_y), [x, y, w, h]);
        }
        scene.seq = seq;
        presenter.publish(scene);
        last = Some(state);
    }
}

/// Delivers the viewer's input to the game's window. The viewer shows the
/// game's map with the game's camera at its own top-left corner, 1:1, so a
/// viewer pixel is the same game client pixel.
#[derive(Default)]
struct InputForwarder {
    window: Option<inproc::window::Window>,
    held: inproc::window::Buttons,
    clicks: inproc::window::DoubleClicks,
    /// A world pixel the viewer asked to see (`Input::Look`), not yet acted on.
    look: Option<(i32, i32)>,
    /// The viewer's size, as it last reported.
    viewport: (u32, u32),
}

impl InputForwarder {
    /// A left click at client position `p`.
    fn click(&mut self, (x, y): (i32, i32)) {
        use inproc::window::{Buttons, MouseButton};
        let Some(window) = self.game_window() else { return };
        window.mouse_move(x, y, self.held);
        window.mouse_button(MouseButton::Left, true, false, x, y, Buttons { left: true, ..self.held });
        window.mouse_button(MouseButton::Left, false, false, x, y, self.held);
    }

    fn game_window(&mut self) -> Option<inproc::window::Window> {
        if self.window.is_none() {
            self.window = inproc::window::Window::of_this_process()
                .into_iter()
                // Not ours, and a real window (titled, with an area).
                .filter(|w| {
                    let t = w.title();
                    !t.is_empty() && t != VIEWER_TITLE && w.client_size().is_some_and(|(w, h)| w > 0 && h > 0)
                })
                .max_by_key(|w| w.client_size().map_or(0, |(w, h)| w as u64 * h as u64));
            if let Some(w) = self.window {
                log::info!("forwarding input to {:?} ({:?})", w.title(), w.client_size());
                if self.viewport == (0, 0) {
                    self.viewport = w.client_size().unwrap_or((1024, 768));
                }
            }
        }
        self.window
    }
}

fn forward_input(forwarder: &mut InputForwarder, inputs: Vec<scene::Input>) {
    use inproc::window::MouseButton;
    use scene::{Button, Input};
    if inputs.is_empty() {
        return;
    }
    let Some(window) = forwarder.game_window() else { return };
    for input in inputs {
        log::trace!("input {input:?}");
        match input {
            Input::MouseMove { x, y } => window.mouse_move(x, y, forwarder.held),
            Input::MouseButton { button, down, x, y } => {
                let button = match button {
                    Button::Left => {
                        forwarder.held.left = down;
                        MouseButton::Left
                    }
                    Button::Right => {
                        forwarder.held.right = down;
                        MouseButton::Right
                    }
                    Button::Middle => {
                        forwarder.held.middle = down;
                        MouseButton::Middle
                    }
                };
                let double = down && forwarder.clicks.press(button, x, y) && window.wants_double_clicks();
                window.mouse_button(button, down, double, x, y, forwarder.held);
            }
            Input::Wheel { delta, x, y } => window.wheel(delta, x, y, forwarder.held),
            Input::Key { vk, down, text } => {
                window.key(vk, down);
                // The game's message loop makes the character from the key
                // (TranslateMessage); only text without a key is sent as such.
                if let (true, Some(c), 0) = (down, text, vk) {
                    window.char(c);
                }
            }
            Input::Resized { width, height } => forwarder.viewport = (width, height),
            Input::Quit => {}
            Input::Look { x, y } => forwarder.look = Some((x, y)),
        }
    }
}
