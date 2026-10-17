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


/// How often the publisher looks at the game.
const POLL: Duration = Duration::from_millis(33);
/// How long a font no text has asked for stays in the art (see `text::Fonts::prune`).
const FONT_KEPT: Duration = Duration::from_secs(60);

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
            order: c.order,
            grow_turns: Some(c.grow_turns),
            build_turns: Some(c.build_turns),
            hud_icons: Some(c.hud_icons),
            veteran_icons: Some(c.veteran_icons),
            food_income: c.food_income,
            policy: c.policy,
            order_name: c.order_name,
            name: c.name,
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
        debug_mode_bits: optional("debug_mode_bits", game.debug_mode_bits()),
        civ_adjectives: players.adjectives,
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

    /// A rebuild of the art: the description with everything the game
    /// showed so far, and what a renderer compiling its own copy needs (the
    /// full text, the art roots). `pixels`: the atlas too (for a renderer
    /// given it), else only the tables.
    #[allow(clippy::too_many_arguments)]
    fn job(
        &self,
        folders: &ArtFolders,
        units: &units::Needed,
        fonts: &text::Fonts,
        screen: &menu::screen::ScreenArt,
        districts: &crate::district_art::DistrictArt,
        pixels: bool,
    ) -> ArtJob {
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
            description: text,
            roots: files.roots().iter().map(|r| r.to_string_lossy().into_owned()).collect(),
            // (The presenter numbers them.)
            generation: 0,
        };
        ArtJob { name: self.name.clone(), files, wire, pixels, folders: folders.clone(), token: 0, generation: 0 }
    }
}

/// A rebuild of the art (see [`Compiler::job`]), for the art thread.
struct ArtJob {
    /// The description's name, for messages.
    name: String,
    files: civ3_assets::GameFiles,
    wire: scene::wire::Assets,
    pixels: bool,
    folders: ArtFolders,
    /// See `ArtSchedule::start`.
    token: u64,
    /// The presenter's (`Presenter::prepare_assets`).
    generation: u64,
}

/// A rebuild done.
struct ArtDone {
    token: u64,
    generation: u64,
    compiled: Result<Compiled, LiveError>,
    took: Duration,
}

impl ArtJob {
    /// Compiles; with the pixels, reusing the sheets in `cache` (the art
    /// grows a little at a time) and keeping this compile's there.
    fn run(self, cache: &mut render_desc::SheetCache) -> ArtDone {
        let started = std::time::Instant::now();
        let pixels = if self.pixels { assets::Pixels::Made(Some(cache)) } else { assets::Pixels::TablesOnly };
        let compiled = assets::compile_as(&self.wire.description, self.files, pixels)
            .map_err(|e| LiveError::Desc(self.name.clone(), e.to_string()));
        if let Ok(c) = &compiled {
            let (rgba, index) = c.atlas.page_bytes();
            let indexed = c.atlas.sprites.iter().filter(|s| s.palette != 0).count();
            log::info!(
                "live renderer: {} layers, {} sprites ({indexed} indexed), {} ({:?})",
                c.layers.len(),
                c.sprite_names.len(),
                if self.pixels {
                    format!(
                        "atlas {} colour pages ({} MiB), {} index pages ({} MiB), {} palette rows",
                        c.atlas.pages.len(),
                        rgba >> 20,
                        c.atlas.index_pages.len(),
                        index >> 20,
                        c.atlas.palettes.len() / tile_render::PALETTE_SIZE as usize
                    )
                } else {
                    "the tables only (the renderer makes the atlas)".into()
                },
                self.folders
            );
        }
        ArtDone { token: self.token, generation: self.generation, compiled, took: started.elapsed() }
    }
}

/// Runs art rebuilds on a thread of their own, so that the publisher goes
/// on publishing with the art it has meanwhile.
struct ArtThread {
    jobs: std::sync::mpsc::Sender<ArtJob>,
    done: std::sync::mpsc::Receiver<ArtDone>,
}

impl ArtThread {
    /// `cache`: the sheets of the compiles so far.
    fn start(mut cache: render_desc::SheetCache) -> std::io::Result<ArtThread> {
        let (jobs, job_rx) = std::sync::mpsc::channel::<ArtJob>();
        let (done_tx, done) = std::sync::mpsc::channel();
        std::thread::Builder::new().name("civ3r-art".into()).spawn(move || {
            for job in job_rx {
                if done_tx.send(job.run(&mut cache)).is_err() {
                    break;
                }
            }
        })?;
        Ok(ArtThread { jobs, done })
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
    // The generated pictures of our preferences tabs (`crate::prefs`).
    text += &crate::prefs::Prefs::description();
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
    // Playing in the viewer's own views while a menu is open
    // (`crate::modal`, `CIV3R_MODAL_INPUT=off`: not).
    if std::env::var("CIV3R_MODAL_INPUT").as_deref() != Ok("off") {
        match crate::modal::install(game) {
            // The pump calls stay patched for the life of the process.
            Ok(hooks) => std::mem::forget(hooks),
            Err(e) => log::warn!("no map input while menus are open: {e}"),
        }
    }
    hook_edge_scroll();
    // Which system pointer the game sets (busy, resizing).
    match inproc::pointer::hook_set_cursor(inproc::pe::main_module()) {
        Ok(Some(hook)) => std::mem::forget(hook),
        Ok(None) => log::warn!("cursor: the game does not import SetCursor"),
        Err(e) => log::warn!("cursor: hooking SetCursor: {e}"),
    }
    let mode = std::env::var("CIV3R_VIEWER").unwrap_or_default();
    // The native viewer makes its own atlas: this side needs the tables only.
    let pixels = !mode.starts_with("native");
    let first = compiler.job(&folders, &units::Needed::default(), &text::Fonts::default(), &Default::default(), &Default::default(), pixels);
    let wire = first.wire.clone();
    let mut cache = render_desc::SheetCache::new();
    let compiled = Arc::new(first.run(&mut cache).compiled?);
    let mut presenter = if mode.starts_with("native") {
        let native = presenter::Native::load(mode == "native-fullscreen").map_err(|e| LiveError::Install(std::io::Error::other(e)))?;
        Presenter::native(native)
    } else {
        let config = tile_viewer::ViewerConfig {
            title: VIEWER_TITLE.into(),
            size: (1024, 768),
            fullscreen: mode == "fullscreen",
            overhang: crate::scene_map::OVERHANG,
            ..Default::default()
        };
        Presenter::in_process(config, compiled.clone()).map_err(LiveError::Install)?
    };
    log::info!("renderer {}", presenter.describe());
    presenter.set_assets(&compiled, &wire);
    presenter.forward_log();
    let compiled = if pixels { Arc::new(compiled.without_pixels()) } else { compiled };
    let art_thread = ArtThread::start(cache).map_err(LiveError::Install)?;
    let timing = if pixels { crate::art_schedule::Timing::SHARED_ATLAS } else { crate::art_schedule::Timing::OWN_ATLAS };
    std::thread::Builder::new()
        .name("civ3r-publish".into())
        .spawn(move || publish(game, &compiler, folders, compiled, presenter, art_thread, timing, pixels))
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
#[allow(clippy::too_many_arguments)]
fn publish(
    game: Civ3,
    compiler: &Compiler,
    mut folders: ArtFolders,
    mut compiled: Arc<Compiled>,
    mut presenter: Presenter,
    art_thread: ArtThread,
    timing: crate::art_schedule::Timing,
    pixels: bool,
) {
    let mut mapper = Mapper::new(&compiled);
    let menus_of = |c: &Compiled| menu::Menus::parse(MENUS, c).inspect_err(|e| log::error!("menus: {e}")).ok();
    let mut menus = menus_of(&compiled);
    let mut last: Option<(Shown, Vec<text::TextRun>, Option<crate::ui::Frame>, Vec<scene::Interactable>, scene::Cursor)> = None;
    let mut seq = 0;
    let mut input = InputForwarder::default();
    let mut title = menu::MenuState::default();
    let mut art = units::UnitArt::new(compiler.files(&folders));
    let mut have = units::Needed::default();
    let mut fonts = text::Fonts::default();
    let mut screen_art = menu::screen::ScreenArt::default();
    let mut district_art = crate::district_art::DistrictArt::default();
    let mut label_env = crate::city_labels::SystemTextEnv::new(crate::city_labels::FileEnv::new(Box::new(compiler.files(&folders))));
    let mut table_colors = std::collections::HashMap::new();
    let mut schedule = crate::art_schedule::ArtSchedule::new(timing);
    // When the art being built was first wanted, and whether new art came
    // (the scene goes out again, with it).
    let mut art_wanted: Option<std::time::Instant> = None;
    let mut new_art = false;
    let mut city_bands = std::collections::HashMap::new();
    let mut last_stats = std::time::Instant::now();
    // The renderer's own settings, and our tabs over the game's Preferences
    // screen (`crate::prefs`); what they drew and the settings last published.
    let mut prefs = crate::prefs::Prefs::load(viewer_settings::Settings::default_path());
    let mut published_prefs: (Vec<menu::panel::Draw>, Option<viewer_settings::Settings>) = (Vec::new(), None);
    // The last scene's size: bytes of cells, world sprites, overlay sprites.
    let mut scene_size = (0, 0, 0);
    loop {
        std::thread::sleep(POLL);
        // Now and then, how much everything holds (memory diagnostics).
        if last_stats.elapsed() >= Duration::from_secs(30) {
            last_stats = std::time::Instant::now();
            let mem = inproc::process::memory().unwrap_or_default();
            let (canvases, ops, presented) = crate::ui_trace::stats().unwrap_or_default();
            log::info!(
                "memory: working set {} MiB (peak {}), commit {} MiB (peak {}); sprites {}, fonts {}, screen art {}+{}+{}, districts {}; ui canvases {canvases}, ops {ops}, presented {presented}; scene cells {} KiB, sprites {}+{}",
                mem.working_set >> 20,
                mem.peak_working_set >> 20,
                mem.commit >> 20,
                mem.peak_commit >> 20,
                compiled.atlas.sprites.len(),
                fonts.len(),
                screen_art.regions.len(),
                screen_art.fills.len(),
                screen_art.dynamic.len(),
                district_art.len(),
                scene_size.0 >> 10,
                scene_size.1,
                scene_size.2,
            );
        }
        // A rebuild of the art done: from now on the scenes use it.
        if let Ok(done) = art_thread.done.try_recv() {
            match done.compiled {
                Ok(c) => {
                    let c = Arc::new(c);
                    let started = std::time::Instant::now();
                    presenter.adopt_assets(&c, done.generation);
                    // The viewer has the pixels; this side needs the rest only.
                    compiled = if pixels { Arc::new(c.without_pixels()) } else { c };
                    mapper = Mapper::new(&compiled);
                    menus = menus_of(&compiled);
                    log::info!(
                        "art: built in {} ms, handed over in {} ms; {} ms after it was first wanted",
                        done.took.as_millis(),
                        started.elapsed().as_millis(),
                        art_wanted.map_or(0, |t| t.elapsed().as_millis())
                    );
                }
                Err(e) => log::error!("recompiling for {folders:?}: {e}"),
            }
            schedule.done(std::time::Instant::now(), done.token);
            if !schedule.pending() {
                art_wanted = None;
            }
            new_art = true;
        }
        // A scenario brings its own art.
        let now = ArtFolders::of(&game);
        let mut recompile = false;
        // Art for what the game's screen shows (held back without it).
        let mut screen_recompile = false;
        // What the art is rebuilt for (for the log).
        let mut wanted_for: Vec<&str> = Vec::new();
        if now != folders {
            folders = now;
            art = units::UnitArt::new(compiler.files(&folders));
            label_env = crate::city_labels::SystemTextEnv::new(crate::city_labels::FileEnv::new(Box::new(compiler.files(&folders))));
            have = units::Needed::default();
            recompile = true;
            wanted_for.push("art folders");
        }
        let inputs = presenter.take_input();
        // What our preferences tabs take does not reach the game.
        let inputs = prefs.route(inputs, input.viewport);
        presenter.forward_log();
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
                forward_input(&game, &mut input, inputs);
                continue;
            }
        };
        ui_trace::set_skipping(matches!(shown, Shown::Map(..)));
        forward_input(&game, &mut input, inputs);
        if input.viewport == (0, 0) {
            input.game_window();
        }
        // The game's interface, mirrored (else its text only).
        let ui = ui_trace::frame();
        // Our tabs over the game's Preferences screen, while it is shown.
        prefs.update(ui.as_ref(), civ3_sys::ui_hooks::preferences_form(game.build), input.viewport);
        let prefs_draws = prefs.draws(&fonts);
        let prefs_changed = published_prefs.0 != prefs_draws || published_prefs.1.as_ref() != Some(prefs.settings());
        let texts = if ui.is_some() { Vec::new() } else { trace::recent_texts() };
        // What can be clicked (for the viewer's button snapping): our own
        // title menu's buttons, else the game's controls.
        let own_title = menus.as_ref().and_then(|ms| ms.menu("title").map(|m| (ms, m))).filter(|_| matches!(shown, Shown::Title(_)) && ui.is_none());
        let interactables = match own_title {
            Some((ms, m)) => ms.interactables(m, input.viewport),
            None => civ3_sys::controls::sites(game.build)
                .map(|s| ui_screen::interactables(&civ3_sys::controls::controls(&civ3_sys::controls::Live, &s)))
                .unwrap_or_default(),
        };
        // The pointer the game shows (docs §16.11).
        let game_cursor = civ3_sys::cursor::sites(game.build).and_then(|s| civ3_sys::cursor::read(&civ3_sys::controls::Live, &s));
        let standard = inproc::pointer::last_set_cursor().and_then(inproc::pointer::standard_cursor);
        let cursor = crate::cursor::scene_cursor(game_cursor, standard, |n| compiled.set(n).map(|(s, _)| s.raw()), crate::cursor::system_arrow());
        if last.as_ref().is_none_or(|l| l.4 != cursor) {
            log::debug!("cursor: the game shows {game_cursor:?} (system pointer {standard:?}): {cursor:?}");
        }
        let state = (shown, texts, ui, interactables, cursor);
        if last.as_ref() == Some(&state) && !recompile && !new_art && !schedule.pending() && input.look.is_none() && input.world_buttons.is_empty() && !prefs_changed {
            continue;
        }
        let (shown, texts, ui, interactables, cursor) = &state;
        let files = compiler.files(&folders);
        let generated = civ3_sys::ui_hooks::generated_surfaces(game.build);
        // The parts of the interface the viewer arranges (docs §16.11): the
        // screen open, the popup over it (split off: the viewer may draw it
        // elsewhere).
        let screen_open = ui.as_ref().and_then(|f| crate::interface_parts::screen(f, civ3_sys::ui_hooks::screen_forms(game.build)));
        let modal = game.modal_form().ok().flatten().filter(|&m| m != 0);
        let screen_size = (input.viewport.0 as i32, input.viewport.1 as i32);
        let popup = match (ui, modal, shown) {
            (Some(f), Some(m), Shown::Map(..)) => crate::interface_parts::popup(f, m + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4, screen_open.map(|s| s.0), screen_size),
            _ => None,
        };
        let (ui_below, ui_popup) = match (ui, popup) {
            (Some(f), Some((first, _))) => {
                let (below, above) = crate::interface_parts::split_at(f, first);
                (Some(below), Some(above))
            }
            _ => (ui.clone(), None),
        };
        // Unit pictures in panels are drawn in the viewing player's colours.
        let unit_colors = match shown {
            Shown::Map(world, _) => world.color_tables.get(world.player.max(0) as usize).copied().unwrap_or(0),
            Shown::Title(_) => 0,
        };
        // An open city screen covers everything under it; the map shows in
        // its gap, as a view of its own (§16.8).
        let city = match (shown, &ui_below) {
            (Shown::Map(world, _), Some(frame)) if world.focus_city.is_some() => {
                ui_screen::city_screen(frame, &files, files.roots(), &mut city_bands)
            }
            _ => None,
        };
        let visible_ui = match city {
            Some((_, first)) => ui_below.as_ref().map(|f| ui_screen::from_draw(f, first)),
            None => ui_below.clone(),
        };
        let mut ui_items = visible_ui.as_ref().map(|f| ui_screen::items(f, files.roots(), generated, unit_colors)).unwrap_or_default();
        // On the map's own screen: the corner's buttons (drawn in the view
        // the pointer is in) and the unit's orders (reachable from every
        // view).
        let hud = civ3_sys::ui_hooks::hud_buttons(game.build);
        let on_map = matches!(shown, Shown::Map(..)) && screen_open.is_none();
        let hud_ui = visible_ui.as_ref().filter(|_| on_map);
        let corner = hud_ui.and_then(|f| crate::interface_parts::forms_rect(f, |c| hud.corner.contains(&c))).map(crate::interface_parts::corner_region);
        let reachable = hud_ui.and_then(|f| crate::interface_parts::forms_rect(f, |c| hud.is_unit_order(c)));
        let corner_items = match (corner, hud_ui) {
            (Some(r), Some(f)) => ui_screen::items(&crate::interface_parts::region(f, r), files.roots(), generated, unit_colors),
            _ => Vec::new(),
        };
        let popup_items = ui_popup.as_ref().map(|f| ui_screen::items(f, files.roots(), generated, unit_colors)).unwrap_or_default();
        // City labels, made from the world as world sprites, so every view
        // shows them (§10.8), in place of those the game drew on its map
        // canvas (`CIV3R_MAP_LABELS=mirror`: the game's own, in its view only).
        // Every explored city has its label, also those the game's own view
        // leaves out (it skips cities past the edge of its tile range and
        // misplaces far ones).
        // A menu over the game's map (an advisor, diplomacy) leaves its own
        // labels out: what is left of its map canvas shows none. Then the
        // game's view shows none either; the viewer's other views do.
        let mut labels_not_in_game_view = false;
        let label_items = match shown {
            Shown::Map(world, view) if ui.is_some() && world_labels() => {
                // (Only under a screen or modal form: on the plain map the game
                // may have drawn none itself, e.g. with its map rendering off.)
                labels_not_in_game_view = (screen_open.is_some() || modal.is_some()) && !ui_items.iter().any(|i| matches!(i, ui_screen::Item::Element(_, true) | ui_screen::Item::Text(_, _, true)));
                ui_items.retain(|i| !matches!(i, ui_screen::Item::Element(_, true) | ui_screen::Item::Text(_, _, true)));
                let labels = crate::city_labels::labels(world, view.zoomed_out, &mut label_env);
                let everywhere = [-(1 << 24), -(1 << 24), 1 << 24, 1 << 24];
                crate::city_labels::items(&labels, (0, 0), everywhere, true)
            }
            _ => Vec::new(),
        };
        // The viewer's own views marked on the minimap.
        if let Shown::Map(world, view) = shown {
            ui_screen::mark_views(&mut ui_items, world, &input.views, view.zoomed_out, view_rect_color());
        }
        // Our tabs on top of the game's Preferences screen.
        ui_items.extend(crate::prefs::items(&prefs_draws));
        // The viewer asked to see a world pixel: click it on the minimap (the
        // game centres its view there).
        // Not while a menu is open: the click would go to the menu.
        if let (Some(look), Shown::Map(world, view)) = (input.look.take(), shown) {
            match crate::scene_map::tile_at(world, look, view.zoomed_out).and_then(|t| ui_screen::minimap_point(&ui_items, world, t)) {
                _ if crate::modal::modal_open(&game) => log::debug!("look at {look:?}: a menu is open"),
                Some(p) => input.click(p),
                None => log::debug!("look at {look:?}: not on the minimap"),
            }
        }
        // Presses on the world in the viewer's own views: acted on by the
        // game's thread while a menu is open that allows it.
        for (button, at) in std::mem::take(&mut input.world_buttons) {
            let Shown::Map(world, view) = shown else { continue };
            let action = crate::scene_map::tile_at(world, at, view.zoomed_out).and_then(|t| crate::modal::button_action(button, t));
            match action {
                Some(action) if crate::modal::world_input(&game) => {
                    crate::modal::queue(action);
                    if let Some(w) = input.game_window() {
                        w.wake();
                    }
                }
                _ => log::debug!("modal input: {button:?} at world {at:?} ({action:?}): no menu that allows it"),
            }
        }
        if let Some(f) = ui {
            let mut more = have.clone();
            for path in ui_screen::unit_files(f, files.roots(), false) {
                more.add_file(&mut art, &path, unit_colors);
            }
            // Leader heads: those shown now replace those before.
            let heads = ui_screen::unit_files(f, files.roots(), true);
            if !heads.iter().all(|p| have.plain.contains_key(p)) {
                more.set_plain(&mut art, &heads);
            }
            if more != have {
                have = more;
                screen_recompile = true;
                wanted_for.push("unit pictures");
            }
        }
        if ui_screen::add_art(&ui_items, &mut screen_art, &mut fonts, &files) | ui_screen::add_art(&popup_items, &mut screen_art, &mut fonts, &files) {
            screen_recompile = true;
            wanted_for.push("interface");
        }
        if ui_screen::add_art(&label_items, &mut screen_art, &mut fonts, &files) {
            recompile = true;
            wanted_for.push("city labels");
        }
        // Art the description does not have yet: units, text fonts.
        if let Shown::Map(world, _) = shown {
            let needed = units::Needed::of(world, &mut art);
            if !have.covers(&needed) {
                have.merge(&needed);
                recompile = true;
                wanted_for.push("units");
            }
        }
        if fonts.add(&compiler.files(&folders), texts) {
            recompile = true;
            wanted_for.push("text");
        }
        // The art of the districts on the map (C3X), for both zooms at once
        // (the zoomed-out view draws the same art at half size: zooming out
        // needs no new art).
        if let Shown::Map(world, _) = shown {
            if district_art.add(&crate::district_art::slices(world)) {
                recompile = true;
                wanted_for.push("districts");
            }
        }
        // Rebuild the art once the requests settle (a new screen asks for
        // new art several times in a row), one rebuild at a time, on the art
        // thread (`crate::art_schedule`).
        let now = std::time::Instant::now();
        if recompile || screen_recompile {
            log::debug!("art: wanted for {}", wanted_for.join(", "));
            schedule.want(now, screen_recompile);
            art_wanted.get_or_insert(now);
        }
        if let Some(token) = schedule.start(now) {
            // Fonts no screen or zoom has asked for in a while go with this
            // rebuild (each is a sheet of glyphs; zooming through sizes
            // would add up).
            let dropped = fonts.prune(FONT_KEPT);
            if dropped > 0 {
                log::debug!("art: {dropped} fonts not used for {FONT_KEPT:?} left out");
            }
            let job = compiler.job(&folders, &have, &fonts, &screen_art, &district_art, pixels);
            // A renderer that compiles its own copy starts at once.
            let generation = presenter.prepare_assets(&job.wire);
            if art_thread.jobs.send(ArtJob { token, generation, ..job }).is_err() {
                log::error!("the art thread is gone");
                schedule.done(now, token);
            }
        }
        // The screen's art is not built yet: the viewer keeps the last
        // complete scene a little longer rather than show empty panels.
        if schedule.hold(now) {
            continue;
        }
        new_art = false;
        seq += 1;
        let mut scene = match shown {
            Shown::Map(world, view) => {
                let mut scene = mapper.scene(world, view, (0, 0), 0);
                // The mirrored interface has the game's own city-site lines.
                if ui.is_none() {
                    scene.sprites = city_site::sprites(world, &compiled);
                }
                units::place(world, view, &mut art, &compiled, compiler.rings.as_ref(), &mut scene);
                // The mirrored interface has the city icons already.
                if ui.is_none() {
                    scene.sprites.extend(city_hud::sprites(world, &compiled));
                }
                // The point of interest: the active unit.
                // (In the game view's pixels: halved zoomed out; the sprites
                // stay at full size, the renderer halves them.)
                scene.focus = units::focus(world);
                if view.zoomed_out {
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
        // Labels and the interface's text also drawn from outlines at
        // every zoom (where the art has the face).
        scene.zooms = ui_screen::outline_zooms(&compiled);
        let outlines = !scene.zooms.is_empty();
        scene.sprites.extend(ui_screen::sprites(&ui_items, &fonts, &compiled, ui_screen::under_pass(&compiled), outlines));
        // The city labels go with the world (laid out in the game view's
        // pixels: zoomed out at full size, which the renderer halves).
        let mut labels: Vec<_> = ui_screen::sprites(&label_items, &fonts, &compiled, ui_screen::under_pass(&compiled), outlines)
            .into_iter()
            .map(|s| {
                let not_in_view = labels_not_in_game_view.then_some(crate::scene_map::GAME_VIEW);
                scene::FreeSprite { screen: false, clip: None, not_in_view, ..s }
            })
            .collect();
        if let Shown::Map(_, view) = shown {
            if view.zoomed_out {
                crate::scene_map::from_zoomed_out(&mut labels);
            }
        }
        scene.sprites.extend(labels);
        // Pictures the game makes itself: the minimap (§17).
        if let Shown::Map(world, _) = shown {
            scene.patches = ui_screen::minimap_patches(&ui_items, world, &compiled, |table| {
                *table_colors.entry(table).or_insert_with(|| art.table_color(table, 66).unwrap_or_default())
            });
        }
        crate::scene_map::overlay_screen_sprites(&mut scene);
        // The parts of the interface the viewer arranges, in views of their
        // own (the corner's buttons a copy, the popup not in the interface).
        let part_sprites = |items: &[ui_screen::Item]| ui_screen::sprites(items, &fonts, &compiled, ui_screen::under_pass(&compiled), outlines);
        if let Some(r) = corner {
            crate::interface_parts::part_view(&mut scene, crate::interface_parts::CORNER_VIEW, r, part_sprites(&corner_items));
        }
        if let Some((_, r)) = popup {
            crate::interface_parts::part_view(&mut scene, crate::interface_parts::POPUP_VIEW, r, part_sprites(&popup_items));
            // The popup's subject: the game brings it into view first (the
            // middle of the game's view stands for it).
            if let Some(v) = scene.view(crate::scene_map::GAME_VIEW).map(|v| v.view) {
                let (w, h) = if v.viewport.0 == 0 || v.viewport.1 == 0 { input.viewport } else { v.viewport };
                scene.focus = Some((v.camera_x + w as i32 / 2, v.camera_y + h as i32 / 2));
            }
        }
        scene.interface = scene::Interface {
            screen: screen_open.map(|s| s.1.to_owned()),
            roaming: corner.map(|_| crate::interface_parts::CORNER_VIEW),
            popup: popup.map(|_| crate::interface_parts::POPUP_VIEW),
            reachable,
        };
        if let (Shown::Map(_, view), Some(([x, y, w, h], _))) = (shown, city) {
            crate::scene_map::city_view(&mut scene, (view.camera_x, view.camera_y), [x, y, w, h]);
        }
        // The renderer's settings go with every scene (sent when they change).
        scene.settings = Some(prefs.settings().clone());
        scene.interactables = interactables.clone();
        scene.cursor = *cursor;
        scene.seq = seq;
        scene_size = (scene.cell_bytes(), scene.sprites.len(), scene.views.iter().map(|v| v.overlay.len()).sum::<usize>());
        // Held back while the renderer is busy: publish again next time.
        let published = presenter.publish(scene);
        if published {
            published_prefs = (prefs_draws, Some(prefs.settings().clone()));
        }
        last = published.then_some(state);
    }
}

/// Whether city labels are made from the world (see `publish`), unless
/// `CIV3R_MAP_LABELS=mirror`.
fn world_labels() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("CIV3R_MAP_LABELS").as_deref() != Ok("mirror"))
}

/// The colour the viewer's own views are marked with on the minimap:
/// `CIV3R_VIEW_RECT_COLOR` (`R;G;B`), yellow by default.
fn view_rect_color() -> [u8; 3] {
    static COLOR: std::sync::OnceLock<[u8; 3]> = std::sync::OnceLock::new();
    *COLOR.get_or_init(|| {
        let spec = std::env::var("CIV3R_VIEW_RECT_COLOR").unwrap_or_default();
        let c: Vec<u8> = spec.split(';').filter_map(|v| v.trim().parse().ok()).collect();
        <[u8; 3]>::try_from(c).unwrap_or([255, 255, 0])
    })
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
    /// World rectangles of the viewer's own views (`Input::Views`).
    views: Vec<(i32, i32, i32, i32)>,
    /// The viewer's size, as it last reported.
    viewport: (u32, u32),
    /// Presses on world pixels of the viewer's own views
    /// (`Input::WorldButton`), not yet acted on.
    world_buttons: Vec<(scene::Button, (i32, i32))>,
    /// World keys (`Input::WorldKey`) whose press went to the game's window
    /// as a plain key.
    plain_keys: Vec<u32>,
    /// Where the viewer's pointer rests at its window's edge
    /// (`Input::Edge`), and the game pointer that makes the game scroll
    /// (`crate::edge`).
    edge: (i32, i32),
    edge_point: Option<(i32, i32)>,
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

/// A key to the game's window.
fn key(window: inproc::window::Window, vk: u32, down: bool, text: Option<char>) {
    window.key(vk, down);
    // The game's message loop makes the character from the key
    // (TranslateMessage); only text without a key is sent as such.
    if let (true, Some(c), 0) = (down, text, vk) {
        window.char(c);
    }
}

fn forward_input(game: &Civ3, forwarder: &mut InputForwarder, inputs: Vec<scene::Input>) {
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
            Input::Key { vk, down, text } => key(window, vk, down, text),
            // A key over a view of the viewer's own: acted on (a few of
            // them) while a menu is open that allows it, else the key it is.
            // A release goes where its press went.
            Input::WorldKey { vk, down, text, .. } => {
                let plain = forwarder.plain_keys.iter().position(|&k| k == vk);
                if plain.is_some() || !crate::modal::world_input(game) {
                    match (down, plain) {
                        (true, None) => forwarder.plain_keys.push(vk),
                        (false, Some(k)) => drop(forwarder.plain_keys.swap_remove(k)),
                        _ => {}
                    }
                    key(window, vk, down, text);
                } else if down {
                    match crate::modal::key_action(vk) {
                        Some(action) => {
                            crate::modal::queue(action);
                            window.wake();
                        }
                        None => log::debug!("modal input: key {vk:#x} is not one acted on"),
                    }
                }
            }
            Input::WorldButton { button, down: true, x, y } => forwarder.world_buttons.push((button, (x, y))),
            Input::WorldButton { .. } => {}
            Input::Resized { width, height } => forwarder.viewport = (width, height),
            Input::Quit => forwarder.edge = (0, 0),
            Input::Edge { x, y } => forwarder.edge = (x, y),
            Input::Look { x, y } => forwarder.look = Some((x, y)),
            Input::Views(views) => forwarder.views = views.iter().copied().filter(|v| v.2 > 0 && v.3 > 0).collect(),
            // (Text is drawn from outlines at every zoom: no art per zoom.)
            Input::Zooms(_) => {}
        }
    }
    // The viewer's pointer at its edge: the game's at its own, to scroll.
    let point = window.client_size().and_then(|size| crate::edge::game_point(forwarder.edge, size));
    if point != forwarder.edge_point {
        match point {
            Some((x, y)) => log::debug!("edge scroll: the viewer's pointer at {:?}, the game's at ({x}, {y})", forwarder.edge),
            None => log::debug!("edge scroll: ends"),
        }
        forwarder.edge_point = point;
        inproc::pointer::set(point.map(|(x, y)| inproc::pointer::Override { window, x, y }));
    }
}

/// Lets the game scroll at the viewer's edge (`crate::edge`): its
/// executable's `GetFocus` and `jgl.dll`'s `GetCursorPos` (loaded later)
/// answer with `inproc::pointer`'s override while there is one. The hooks
/// stay for the life of the process.
fn hook_edge_scroll() {
    match inproc::pointer::hook_focus(inproc::pe::main_module()) {
        Ok(Some(hook)) => std::mem::forget(hook),
        Ok(None) => return log::warn!("edge scroll: the game does not import GetFocus"),
        Err(e) => return log::warn!("edge scroll: hooking GetFocus: {e}"),
    }
    std::thread::spawn(|| {
        for _ in 0..600 {
            if let Some(module) = inproc::pe::module_base("jgl.dll") {
                match inproc::pointer::hook_cursor_pos(module) {
                    Ok(Some(hook)) => {
                        log::info!("edge scroll: following the viewer's pointer at its edge");
                        std::mem::forget(hook);
                    }
                    Ok(None) => log::warn!("edge scroll: jgl.dll does not import GetCursorPos"),
                    Err(e) => log::warn!("edge scroll: hooking GetCursorPos: {e}"),
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
}
