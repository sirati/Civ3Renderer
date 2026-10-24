//! The reimplemented renderer, running inside the game: a publisher thread
//! turns the game's world and view into scenes, a viewer window draws them.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use civ3_sys::{Civ3, Civ3Error};
use render_desc::Compiled;

use crate::art::{ArtJob, ArtThread, Built, Work};

use crate::presenter::{self, Presenter};
use crate::{assets, city_hud, city_site, districts, text, trace, ui_screen, ui_trace, units};
use crate::scene_map::Mapper;
use crate::world_items::WorldItems;
use crate::world::{Civ3City, Civ3Tile, Civ3Unit, Civ3View, Civ3World, IconBuilding, WallBuilding};

/// The viewer window's title (also how we tell it from the game's window).
const VIEWER_TITLE: &str = "Civ III (reimplemented renderer)";


/// How often the publisher looks at the game at least: it goes on at once
/// when rung (`presenter::WAKE`: a present that changed the screen, input,
/// a preview stage); this is for what nobody rings for (the world).
const POLL: Duration = Duration::from_millis(33);
/// How often the publisher tells the viewer whether the game answers
/// (`crate::heartbeat`; the viewer takes 500 ms without word as busy).
const HOST_EVERY: Duration = Duration::from_millis(100);
/// The pause before trying again a scene the renderer was too busy to take.
const BUSY_RETRY: Duration = Duration::from_millis(2);

/// How often the memory diagnostics are logged (`CIV3R_MEMORY_LOG_S`,
/// seconds; 30 by default).
fn memory_every() -> Duration {
    static EVERY: std::sync::OnceLock<Duration> = std::sync::OnceLock::new();
    *EVERY.get_or_init(|| Duration::from_secs(std::env::var("CIV3R_MEMORY_LOG_S").ok().and_then(|v| v.parse().ok()).unwrap_or(30)))
}
/// How long a font no text has asked for stays in the art (see `text::Fonts::prune`).
const FONT_KEPT: Duration = Duration::from_secs(60);
/// How long a scenario stays highlighted on the Load Scenario screen before
/// its preview's own art (a whole build for its folders) is made: going
/// through the list never queues the menu's art behind such a build.
const PREVIEW_ART_REST: Duration = Duration::from_millis(250);
/// About how many bytes of pixels an addition of art made ahead has
/// (`crate::preload`): art the screen needs waits at most for one.
const PRELOAD_CHUNK: u64 = 3 << 20;
/// Atlas memory art made ahead may fill, by default: in the game's 32-bit
/// process, where software renderers keep the atlas in the process too,
/// and in a native renderer (an integrated GPU's memory is the system's).
const PRELOAD_MIB_IN_PROCESS: u64 = 640;
const PRELOAD_MIB_NATIVE: u64 = 1536;

fn unit(u: &civ3_sys::DrawnUnit) -> Civ3Unit {
    Civ3Unit {
        id: u.id,
        x: u.x,
        y: u.y,
        civ: u.civ,
        unit_type: u.unit_type,
        leader_kind: u.leader_kind,
        experience: u.experience,
        damage: u.damage,
        moves: u.moves,
        max_move_points: u.max_move_points,
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

/// An effect as the world holds it (its art's path game-relative).
pub(crate) fn effect(e: &civ3_sys::DrawnEffect) -> crate::world::Civ3Effect {
    crate::world::Civ3Effect {
        source: e.source,
        tile: e.tile,
        pixel: e.pixel,
        direction: e.direction,
        animation: e.animation,
        frame: e.frame,
        shown: e.shown,
        ini: if e.ini.is_empty() { String::new() } else { crate::effects::art_path(&e.ini) },
    }
}

/// A copy of the parts of the game state the map is drawn from.
/// How `player` stands with every civ (the barbarians, civ 0, are always
/// at war). The raw fields are logged when they change: not all treaty
/// bits are known (`civ3_sys::world::Diplomacy`).
fn relations(game: &Civ3, player: i32) -> Vec<civ3_sys::world::Relation> {
    // (Read by the publisher and the preview: either may log a change.)
    static LAST: handoff::Latest<civ3_sys::world::Diplomacy> = handoff::Latest::new();
    let Ok(d) = game.diplomacy(player) else { return Vec::new() };
    if LAST.load().as_deref() != Some(&d) {
        let known: Vec<String> = (0..32)
            .filter(|&c| d.at_war[c] || d.contacts[c] != 0 || d.treaties[c] != 0 || d.military_allies[c] != 0)
            .map(|c| format!("{c}: war {} contacts {:#x} treaties {:#x} allies {:#x}", d.at_war[c] as u8, d.contacts[c], d.treaties[c], d.military_allies[c]))
            .collect();
        log::info!("diplomacy of civ {player}: {}", known.join("; "));
        LAST.publish(d.clone());
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
    let drawn = optional("units", crate::unit_views::drawn_units(game));
    // The members of the armies drawn (§10.11): the one each shows, and
    // all of them for the army's hit points and moves (§10.6).
    let armies: Vec<i32> = drawn.iter().filter(|u| u.army_member >= 0).map(|u| u.id).collect();
    let mut army_members: Vec<Civ3Unit> = optional("army members", game.units_in(&armies)).iter().map(unit).collect();
    for u in drawn.iter().filter(|u| u.army_member >= 0) {
        if !army_members.iter().any(|m| m.id == u.army_member) {
            army_members.extend(game.unit(u.army_member).ok().flatten().map(|m| unit(&m)));
        }
    }
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
        effects: optional("effects", game.effects()).into_iter().map(|e| effect(&e)).collect(),
        navigator_modes: optional("navigator_modes", game.navigator_modes()),
        preferences: optional("preferences", game.preferences()),
        clean_map: optional("clean_map", game.clean_map()),
        debug_mode_bits: optional("debug_mode_bits", game.debug_mode_bits()),
        civ_adjectives: players.adjectives,
        mouse_tile: optional("mouse_tile", game.mouse_tile()),
        current_unit: optional("current_unit", game.current_unit()),
        focus_city: optional("focus_city", game.focus_city()),
        outposts: optional("outposts", game.outposts()),
        // Which unit's cursor shows, not its frame: the viewer runs the
        // cursor's frames on its own clock (`units::CURSOR_FRAME_MS`), so
        // the game's frame neither makes it jump nor starts a round.
        selection: optional("selection", game.selection()).map(|(id, _)| (id, 0)),
        targeting: optional("targeting", game.targeting()),
        goto: crate::targeting::latest_path(),
        preselected: crate::world_input::preselected(),
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
    #[error("UI capture: {0}")]
    Capture(String),
    #[error("Preferences audio transaction: {0}")]
    Preferences(String),
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
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
    /// Reuse resolved roots and listings until scenario search paths change.
    files: handoff::Latest<(ArtFolders, civ3_assets::GameFiles)>,
    name: String,
    text: String,
    /// An explicit environment override, parsed once. Some(None) disables
    /// rings; None leaves the decision to accepted settings.
    ring_override: Option<Option<crate::rings::Rings>>,
    /// The largest atlas page the renderer takes (`assets::compile_as`).
    max_page: u32,
    /// The largest sprite the art may grow by: a picture of the game's
    /// whole screen (the pages are made for it from the start).
    largest: u32,
}

impl Compiler {
    fn files(&self, folders: &ArtFolders) -> civ3_assets::GameFiles {
        if let Some(cached) = self.files.load().filter(|cached| cached.0 == *folders) {
            return cached.1.clone();
        }
        let files = assets::game_files(&self.install, folders.scenario.as_deref(), &folders.search);
        self.files.publish((folders.clone(), files.clone()));
        files
    }

    /// The whole description: everything the game showed so far (also
    /// what a renderer compiling its own copy needs: the art roots).
    fn whole_text(&self, units: &units::Needed, fonts: &mut text::Fonts, screen: &menu::screen::ScreenArt, districts: &crate::district_art::DistrictArt) -> String {
        format!("largest = {}\n{}{}{}{}{}", self.largest, self.text, units.description(), fonts.description(), ui_screen::description(screen), districts.description())
    }

    /// What a renderer compiling its own copy of the art needs: `text` (a
    /// whole description, or an addition: `append`) and the art roots.
    fn wire(&self, folders: &ArtFolders, text: &str, append: bool) -> scene::wire::Assets {
        let files = self.files(folders);
        scene::wire::Assets {
            description: text.to_owned(),
            roots: files.roots().iter().map(|r| r.to_string_lossy().into_owned()).collect(),
            // (The presenter numbers them.)
            generation: 0,
            append,
            reserve: 0,
            context: String::new(),
        }
    }

    /// A job for the art thread (`generation`: `Presenter::prepare_assets`).
    #[allow(clippy::too_many_arguments)]
    fn job(&self, folders: &ArtFolders, work: Work, pixels: bool, generation: u64, token: Option<u64>, reason: String, wanted: std::time::Instant) -> ArtJob {
        ArtJob { name: self.name.clone(), files: self.files(folders), work, pixels, max_page: self.max_page, generation, token, reason, wanted, context: String::new(), stop: None }
    }
}

/// What compiled art has, for the log: layers, sprites, the atlas (with
/// the pixels).
fn art_summary(c: &Compiled, pixels: bool) -> String {
    let indexed = c.atlas.sprites.iter().filter(|s| s.palette != 0).count();
    let atlas = if pixels {
        let (rgba, index) = c.atlas.page_bytes();
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
    };
    format!("{} layers, {} sprites ({indexed} indexed), {atlas}", c.layers.len(), c.sprite_names.len())
}

/// Starts the publisher and the viewer; returns at once. `CIV3R_VIEWER`:
/// `1` / `fullscreen` a window of this process, `native` /
/// `native-fullscreen` the native Linux viewer (Wine with
/// `civ3r_native.dll`, see `tools/install-native`), `external` /
/// `external-fullscreen` the launcher's separate native viewer, using
/// `CIV3R_VIEWER_ADDR` and `CIV3R_VIEWER_TOKEN`.
pub fn start(game: Civ3) -> Result<(), LiveError> {
    // Required before any presenter or UI worker starts. A byte-contract
    // failure returns through this Result path, with partial patches undone.
    let prefs_audio = if civ3_sys::ui_hooks::preferences_form(game.build).is_some() {
        Some(civ3_sys::preferences::install(game.build).map_err(|error| LiveError::Preferences(error.to_string()))?)
    } else { None };
    // Normally installed synchronously by the game's jgl load callback.
    // Late injection waits for the post-CRT graphics object before hooking.
    if let Err(e) = ui_trace::prepare_worker(&game) {
        if !ui_trace::requested_game_draws() { return Err(LiveError::Capture(e)) }
        log::warn!("not mirroring the interface: {e}; continuing with text only");
    }
    let (name, mut text) = description()?;
    let install = install_root()?;
    text += crate::loading_cursor::description();
    // Rings around foreign units, if asked for (`crate::rings`).
    let ring_override = std::env::var("CIV3R_UNIT_RINGS").ok().map(|spec|
        crate::rings::Rings::configured(&Default::default(), Some(&spec)));
    text += &crate::rings::Rings::all_description();
    // The generated pictures of our preferences tabs (`crate::prefs`).
    text += &crate::prefs::Prefs::description();
    // What the map shows while a command waits for its tile (`crate::targeting`).
    text += &crate::targeting::description();
    let mode = std::env::var("CIV3R_VIEWER").unwrap_or_default();
    // Fullscreen if the launcher asked for it (its default) and the saved
    // settings do not ask for a window (`Display::windowed`, switched at
    // run time from Preferences).
    let fullscreen = mode.ends_with("fullscreen") && !crate::prefs::saved_windowed(&crate::prefs::settings_path());
    // Remote viewers connect first so their GPU limits size both sides'
    // atlas tables. The Windows process never initializes a GPU for them.
    let remote = if mode == "external" || mode == "external-fullscreen" {
        Some(Presenter::external(fullscreen).map_err(LiveError::Install)?)
    } else if mode.starts_with("native") {
        let native = presenter::Native::load(fullscreen).map_err(|e| LiveError::Install(std::io::Error::other(e)))?;
        Some(Presenter::native(native))
    } else {
        None
    };
    let max_page = match &remote {
        Some(presenter) => presenter.max_texture(),
        None => tile_render::gpu::max_texture_dimension(),
    };
    let max_page = max_page.unwrap_or_else(|| {
        log::warn!("the renderer's texture limit is unknown: atlas pages of at most {}", render_desc::ANY_DEVICE_PAGE);
        render_errors::report(render_errors::Kind::Gpu, "texture limit unknown", format_args!("atlas pages of at most {}", render_desc::ANY_DEVICE_PAGE));
        render_desc::ANY_DEVICE_PAGE
    });
    // (Screen pictures are at most the game's screen.)
    let largest = crate::game_window::game_screen().map_or(0, |(w, h)| w.max(h));
    let compiler = Compiler { install, files: handoff::Latest::new(), name, text, ring_override, max_page, largest };
    let folders = ArtFolders::of(&game);
    // Playing in the viewer's own views while a menu is open
    // (`crate::modal`, `CIV3R_MODAL_INPUT=off`: not).
    if std::env::var("CIV3R_MODAL_INPUT").as_deref() != Ok("off") {
        match crate::modal::install(game) {
            // The pump calls stay patched for the life of the process.
            Ok(hooks) => std::mem::forget(hooks),
            Err(e) => log::warn!("no map input while menus are open: {e}"),
        }
    }
    // The game thread's heartbeat, for the viewer's busy indicator.
    match crate::heartbeat::install() {
        Ok(()) => log::info!("heartbeat: jgl.dll's PeekMessageA and WaitMessage stamp it"),
        Err(e) => log::warn!("no game heartbeat (the viewer judges by the connection alone): {e}"),
    }
    if let Err(e) = crate::native_sliders::install(game) {
        log::warn!("no transactional native sliders: {e}");
    }
    if let Err(e) = crate::build_queue::install(game) {
        log::warn!("no build queue reordering: {e}");
    }
    if let Err(e) = crate::ui_wheel::install(game) {
        log::warn!("no wheel on the interface where the game has none: {e}");
    }
    let world_input_ready = crate::world_input::install(game).map_err(|e| {
        log::warn!("no world interactions beyond the game camera: {e}");
    }).is_ok();
    // Native drawing and animation must cover the whole visible world
    // before the original map camera can become bookkeeping only.
    match civ3_sys::combat_speed::install(game) {
        Ok(hooks) => { std::mem::forget(hooks); log::info!("combat animation speed: scoped native FLC clock installed"); }
        Err(error) => log::warn!("combat animation speed unavailable: {error}"),
    }
    let world_units_ready = crate::unit_views::install(game).map_err(|e| {
        log::warn!("no units beyond the game's camera: {e}");
    }).is_ok();
    if world_input_ready && world_units_ready {
        if let Err(e) = crate::native_camera::install(game) {
            log::warn!("native camera remains active: {e}");
        }
        // Fortify's in-view test reads the parked camera. Without this it
        // skips its animation and the sound that animation starts.
        match civ3_sys::unit_views::install_action_view(game) {
            Ok(hook) => std::mem::forget(hook),
            Err(e) => log::warn!("fortify animation limited to the parked native view: {e}"),
        }
        // Sliding units move in the renderer, at its frame rate; the
        // game's slide keeps its schedule without drawing frames.
        if let Err(e) = crate::unit_slides::install(game) {
            log::warn!("unit slides stay with the game's frames: {e}");
        }
    }
    // The go-to path the map shows (`crate::targeting`, found on the
    // game's thread).
    if let Err(e) = crate::targeting::install(game) {
        log::warn!("no go-to paths: {e}");
    }
    // Which system pointer the game sets (busy, resizing).
    match inproc::pointer::hook_set_cursor(inproc::pe::main_module()) {
        Ok(Some(hook)) => std::mem::forget(hook),
        Ok(None) => log::warn!("cursor: the game does not import SetCursor"),
        Err(e) => log::warn!("cursor: hooking SetCursor: {e}"),
    }
    // (The game hiding the system pointer while it draws its own is kept
    // from the system: the viewer's window has the pointer.)
    match inproc::pointer::hook_show_cursor(inproc::pe::main_module()) {
        Ok(Some(hook)) => std::mem::forget(hook),
        Ok(None) => log::debug!("cursor: the game does not import ShowCursor"),
        Err(e) => log::warn!("cursor: hooking ShowCursor: {e}"),
    }
    // (Nor its clips of the system pointer: the clip is global, and the
    // game releasing its own released the viewer window's.)
    match inproc::pointer::hook_clip_cursor(inproc::pe::main_module()) {
        Ok(Some(hook)) => std::mem::forget(hook),
        Ok(None) => log::debug!("cursor: the game does not import ClipCursor"),
        Err(e) => log::warn!("cursor: hooking ClipCursor: {e}"),
    }
    crate::preview::install(&game);
    // Remote viewers make their own atlas: this side needs the tables only.
    let pixels = remote.is_none();
    hook_game_window(crate::game_window::hidden());
    let first = compiler.whole_text(&units::Needed::default(), &mut text::Fonts::default(), &Default::default(), &Default::default());
    let wire = compiler.wire(&folders, &first, false);
    let started = std::time::Instant::now();
    let (compiled, growing) = assets::compile_growing(&first, compiler.files(&folders), pixels, compiler.max_page, &|| false)
        .map_err(|e| LiveError::Desc(compiler.name.clone(), e.to_string()))?;
    log::info!("live renderer: {} in {} ms ({folders:?})", art_summary(&compiled, pixels), started.elapsed().as_millis());
    let compiled = Arc::new(compiled);
    let mut presenter = if let Some(presenter) = remote {
        presenter
    } else {
        let config = tile_viewer::ViewerConfig {
            extensions: civ3_ui::viewer::extensions(),
            title: VIEWER_TITLE.into(),
            size: (1024, 768),
            fullscreen,
            overhang: crate::scene_map::OVERHANG,
            // (The game's folder, where civ3-bridge.log is.)
            recording_dir: tile_viewer::altrec::recording_dir(std::env::current_dir().ok()),
            ..Default::default()
        };
        Presenter::in_process(config, compiled.clone()).map_err(LiveError::Install)?
    };
    log::info!("renderer {}", presenter.describe());
    presenter.set_assets(&compiled, &wire);
    presenter.forward_log();
    let compiled = compiled.without_pixels();
    let art_thread = ArtThread::start(Some((compiled.without_pixels(), growing))).map_err(LiveError::Install)?;
    let timing = crate::art_schedule::Timing::GROWING;
    // The game quitting closes the viewer before it frees its world.
    crate::session_end::install(game);
    std::thread::Builder::new()
        .name("civ3r-publish".into())
        .spawn(move || publish(game, &compiler, folders, compiled, presenter, art_thread, timing, pixels, prefs_audio, fullscreen))
        .map_err(LiveError::Install)?;
    Ok(())
}

/// The game's menus in our renderer (`assets/civ3/menus.toml`).
const MENUS: &str = include_str!("../../../assets/civ3/menus.toml");

/// What the viewer shows: the map, the title menu while no game runs, or
/// the Load Scenario screen (its rectangle) with the world of the scenario
/// it highlights beside it (`crate::preview`; none: no map of its own).
#[derive(PartialEq)]
enum Shown {
    Map(Civ3World, Civ3View),
    Title(menu::MenuState),
    Preview([i32; 4], Option<PreviewShown>),
}

/// A scenario's world as the Load Scenario screen previews it
/// (`crate::preview::Current`): the world, the part of it to show, its
/// labels, whether its unit art is resolved, what of it is drawn now
/// (the world as soon as it is made: `Current::drawn`), whether all its
/// own art is there, whether more of it is still coming (the loading
/// indicator over it, `preview::loading_indicator`).
#[derive(Clone)]
struct PreviewShown {
    number: u64,
    world: Arc<Civ3World>,
    area: [i32; 4],
    labels: Arc<crate::preview::LabelItems>,
    needed: Option<Arc<units::Needed>>,
    drawn: crate::preview::Drawn,
    ready: bool,
    loading: bool,
}

impl PreviewShown {
    fn of(c: &crate::preview::Current, drawn: crate::preview::Drawn, ready: bool, use_art: bool) -> Self {
        Self { number: c.number, world: c.preview.world.clone(), area: c.preview.area, labels: c.labels.clone(), needed: c.needed.clone().filter(|_| use_art), drawn, ready, loading: false }
    }
}

/// (The same preview as made: the world is not compared tile by tile.)
impl PartialEq for PreviewShown {
    fn eq(&self, o: &Self) -> bool {
        self.number == o.number && Arc::ptr_eq(&self.world, &o.world) && Arc::ptr_eq(&self.labels, &o.labels) && self.needed.is_some() == o.needed.is_some() && self.drawn == o.drawn && self.ready == o.ready && self.loading == o.loading
    }
}

/// The publisher's rounds so far (for the hang watchdog,
/// `inproc::alloc::watchdog`).
pub static PUBLISHER_ROUNDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Publishes a new scene whenever what the game shows changes.
#[allow(clippy::too_many_arguments)]
fn publish(
    game: Civ3,
    compiler: &Compiler,
    mut folders: ArtFolders,
    mut compiled: Compiled,
    mut presenter: Presenter,
    mut art_thread: ArtThread,
    timing: crate::art_schedule::Timing,
    pixels: bool,
    _prefs_audio: Option<civ3_sys::preferences::Installed>,
    fullscreen: bool,
) {
    let mut mapper = Mapper::new(&compiled);
    let menus_of = |c: &Compiled| menu::Menus::parse(MENUS, c).inspect_err(|e| {
        log::error!("menus: {e}");
        render_errors::report(render_errors::Kind::ArtDescription, "the viewer's menus", format_args!("{e}"));
    }).ok();
    let mut menus = menus_of(&compiled);
    let mut last: Option<(Shown, Vec<text::TextRun>, Option<crate::ui::Frame>, Vec<scene::Interactable>, (scene::Cursor, Option<(i32, i32)>), Option<Arc<ui_trace::NativeUi>>)> = None;
    let mut last_camera_focus = None;
    let mut last_camera_arrival = None;
    // The viewer is the only window the player sees: the game's own is
    // hidden (`CIV3R_GAME_WINDOW=show` keeps it shown, e.g. for looking at
    // it next to the viewer).
    let hide_game_window = crate::game_window::hidden();
    let mut hiding = Hiding::default();
    let mut seq = 0;
    let mut input = InputForwarder::default();
    let mut stale_inputs = crate::stale_input::BatchFilter::default();
    let mut last_input_take = std::time::Instant::now();
    let mut title = menu::MenuState::default();
    // The first run's Preferences, opened from the title menu once it is
    // up (`crate::first_run`), and where its row was last round.
    let mut first_run = crate::first_run::FirstRun::default();
    let mut first_run_round = crate::first_run::Round::default();
    let mut art = units::UnitArt::new(compiler.files(&folders));
    let mut have = units::Needed::default();
    // (Loaded on a thread of their own: text waits for no font.)
    let mut fonts = text::Fonts::in_background().unwrap_or_else(|e| {
        log::warn!("fonts loaded on the publisher's thread: {e}");
        text::Fonts::default()
    });
    let mut screen_art = menu::screen::ScreenArt::default();
    let mut district_art = crate::district_art::DistrictArt::default();
    let mut label_env = crate::city_labels::SystemTextEnv::new(crate::city_labels::FileEnv::new(Box::new(compiler.files(&folders))));
    let mut table_colors = std::collections::HashMap::new();
    let mut schedule = crate::art_schedule::ArtSchedule::new(timing);
    // When the art asked for was first wanted, and whether new art came
    // (the scene goes out again, with it).
    let mut art_wanted: Option<std::time::Instant> = None;
    let mut new_art = false;
    // What the art has (or is being built with): the rest of what is
    // wanted goes into the next addition.
    let mut described_units = units::Needed::default();
    let mut described_screen = menu::screen::ScreenArt::default();
    let mut described_districts = crate::district_art::DistrictArt::default();
    // A whole description is wanted (the art folders changed).
    let mut whole_wanted = false;
    // The adopted atlas and each whole build retain their own folder identity.
    // Optional preview metadata cannot use an atlas from another scenario.
    let mut compiled_folders = folders.clone();
    let mut whole_folders = std::collections::HashMap::new();
    // Growths a renderer compiling its own art may not have yet (scenes
    // use them once it has).
    // (With the request they cover, `ArtSchedule::done` once they are used.)
    let mut pending_growths: std::collections::VecDeque<(u64, Arc<render_desc::Growth>, Option<u64>)> = Default::default();
    // What units drew last (shown while their new animation's art is built).
    let mut unit_memory = units::UnitMemory::default();
    let mut last_notices: (Vec<crate::unit_notices::Notice>, u32, Vec<(u64, u32)>) = Default::default();
    let mut notices_shown = crate::unit_notices::Shown::default();
    // Art made ahead (`crate::preload`): the plan for the game shown (and
    // which game that is), the interface art to make, the budget, the
    // atlas memory the art takes (the art thread says, with the pixels;
    // else estimated from what was sent).
    let art_list = crate::preload::ArtList::parse(crate::preload::BUILTIN_LIST);
    let mut preload = crate::preload::Preload::none();
    let mut preload_for = None;
    let budget = crate::preload::budget(if pixels { PRELOAD_MIB_IN_PROCESS } else { PRELOAD_MIB_NATIVE });
    let mut planner = if budget == 0 { None } else {
        crate::preload::Planner::new().map_err(|e| log::error!("preload: planning: {e}")).ok()
    };
    let mut atlas_used: u64 = 0;
    let mut sent_bytes: u64 = 0;
    // `CIV3R_ART_LIST`: where to write the art the game showed (for the
    // built-in list of `crate::preload`).
    let art_list_out = std::env::var_os("CIV3R_ART_LIST").map(PathBuf::from);
    // Why art is wanted, since the last addition started (for the log).
    let mut reasons: std::collections::BTreeSet<&'static str> = Default::default();
    let mut city_bands = std::collections::HashMap::new();
    let mut last_stats = std::time::Instant::now();
    // Opt-in text diagnostics, printed only when their contents change.
    let mut text_trace = (std::env::var("CIV3R_TEXT_TRACE").as_deref() == Ok("1")).then(String::new);
    // The renderer's own settings, and our tabs over the game's Preferences
    // screen (`crate::prefs`); what they drew and the settings last published.
    let mut prefs = crate::prefs::Prefs::load(crate::prefs::settings_path());
    prefs.launched(fullscreen);
    civ3_sys::combat_speed::set_speed_quarters((prefs.settings().animation.effective_combat_speed() * 4.0) as u8);
    let mut published_prefs: (Vec<menu::panel::Draw>, Option<viewer_settings::Settings>, Option<scene::SettingsPreview>) = (Vec::new(), None, None);
    // The open popup's canvas and where it is anchored (`scene::Interface::popup_anchor`).
    let mut popup_anchor_state: Option<(u32, Option<scene::PopupAnchor>)> = None;
    // (The popup layers last logged: logged again when they change.)
    let mut popup_layers_logged: Vec<(u32, [i32; 4], bool)> = Vec::new();
    // When a popup anchored at a map click last showed (one an entry of it
    // opens, e.g. a city menu's "Terrain details", belongs to that click).
    let mut click_popup_seen: Option<std::time::Instant> = None;
    // The last scene's size: bytes of cells, world sprites, overlay sprites.
    let mut scene_size = (0, 0, 0);
    // The replay picture belongs to the renderer. The game supplies semantic
    // navigator operations; revisions avoid rebuilding unchanged pixels.
    let mut replay_surface: Vec<(&str, (u32, u32), Vec<u8>)> = Vec::new();
    let mut replay_revision = 0;
    let mut last_replay_revision = 0;
    // The scenario the Load Scenario screen previews.
    let mut previews = crate::preview::Previews::default();
    let mut complete_preview_menu = None;
    // How often the art folders changed (whole descriptions for folders
    // before stop, `crate::art::ArtStop`).
    let folders_changed = Arc::new(std::sync::atomic::AtomicU64::new(0));
    // The map screen's interface: while it is composed, a game runs.
    let main_gui = civ3_sys::ui_hooks::main_gui_canvas(game.build);
    // Whether the game has ended (the screens after it show over Main_GUI).
    let mut game_over = crate::interface_parts::GameOver::default();
    // (Measuring: when this round of the publisher began, and since when a
    // scene has been held back.)
    let mut round_began: Option<std::time::Instant> = None;
    // Since when the screen shows without some of its art (still loading).
    let mut partial_since: Option<std::time::Instant> = None;
    // Holds a scene back while its interface's art is coming.
    let mut partial_reported = false;
    // Visible art missing that `art_retry` asks for again (a static screen
    // is not skipped meanwhile).
    let mut retry_wanted = false;
    // Image files in none of the art folders, and art asked for again.
    let mut absent_art = ui_screen::AbsentArt::default();
    let mut art_retry = ui_screen::ArtRetry::default();
    // What the last publish made of the parts of the world and of the
    // interface that did not change since (`Memo`): the world's cells, the
    // city labels, the interface's and the labels' sprites, the minimap.
    let mut cells_memo = Memo::default();
    let mut labels_memo = Memo::default();
    let mut ui_sprites_memo = Memo::default();
    let mut label_sprites_memo = Memo::default();
    let mut minimap_memo = Memo::default();
    // The Load Scenario screen's map of the scenario (`minimap::scenario_map`).
    let mut scenario_map_memo = Memo::default();
    // The previewed world's scene (`crate::preview::scene_with`).
    let mut preview_memo: Memo<_, scene::Scene> = Memo::default();
    // The controls' pointer anchors (`ui_screen::content_anchors`).
    let mut anchors_memo = Memo::default();
    // The round that shows a new frame of the Load Scenario screen's menu
    // (a new highlight) shows the world beside it as it was, the next one
    // (at once) the newer selection's: the highlight does not wait for its
    // world's scene to be made and sent (tens of ms for a large map).
    let mut preview_deferred = false;
    // The selection the Load Scenario screen had last round.
    let mut preview_selection = 0u64;
    // A whole build for an older selection stopped, not replaced yet.
    let mut rebuild_stopped = false;
    // Whether art folders have a screen file (`ui_screen::retain_present_by`).
    let mut present_in_folders: std::collections::HashMap<(ArtFolders, String), bool> = std::collections::HashMap::new();
    // A press on that screen not shown yet: when, the selection then and
    // the game's frame then.
    let mut preview_input: Option<(std::time::Instant, u64, Option<crate::ui::Frame>)> = None;
    // The interface's items (the parts of the screen's frame: the frame,
    // the menu, the popup, the screen part) and the sprites of the parts drawn
    // in views of their own (corner, hint, screen, popup).
    let mut items_memos: [Memo<_, Vec<ui_screen::Item>>; 4] = Default::default();
    let mut part_memos: [Memo<_, Vec<scene::FreeSprite>>; 4] = Default::default();
    // The kept popups' sprites, per slot (`crate::detached`).
    let mut detached_memos: [Memo<_, Vec<scene::FreeSprite>>; crate::detached::MAX_KEPT] = Default::default();
    // The queued reports' items and sprites, per slot (`crate::async_popups`).
    let mut queued_item_memos: [Memo<_, Vec<ui_screen::Item>>; crate::async_popups::MAX_SHOWN] = Default::default();
    let mut queued_memos: [Memo<_, Vec<scene::FreeSprite>>; crate::async_popups::MAX_SHOWN] = Default::default();
    // The loading bar's items and sprites (`crate::loading_cursor::split_bar`).
    let mut bar_memo: Memo<_, Vec<ui_screen::Item>> = Default::default();
    let mut bar_sprites_memo: Memo<_, Vec<scene::FreeSprite>> = Default::default();
    // Bumped whenever `compiled` changes (new art, a growth): the memos of
    // what is made of the art key on it.
    let mut art_generation = 0u64;
    // Where the publisher's time goes (`CIV3R_PROFILE=1`, or the viewer's
    // `TILE_VIEWER_PROFILE=1`), logged every 5 s.
    let mut profile = (tile_viewer::profile::env_on("CIV3R_PROFILE") || tile_viewer::profile::env_on("TILE_VIEWER_PROFILE"))
        .then(|| tile_viewer::profile::Profile::new("publisher", Duration::from_secs(5)));
    // How many controls had looks last (for the log).
    let mut last_looks = (0, 0, 0, 0);
    // The longest pause before the next round (the bell, `presenter::WAKE`,
    // ends it as soon as there is something new).
    let mut wait = POLL;
    let mut host_sent = std::time::Instant::now();
    let mut stalls = crate::heartbeat::StallWatch::default();
    let mut busy = crate::heartbeat::BusyWatch::default();
    let stall_clock = std::time::Instant::now();
    loop {
        PUBLISHER_ROUNDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(t) = round_began.take() {
            let took = t.elapsed();
            if took >= Duration::from_millis(40) {
                log::info!("publish: a round took {} ms", took.as_millis());
            }
        }
        if let Some(p) = &mut profile {
            p.round_end();
            if let Some(report) = p.report() {
                log::info!("{report}");
            }
        }
        // (Sooner when a stage of the previewed scenario is ready.)
        // (Not longer than the host's status may go unsent.)
        previews.wait(std::mem::replace(&mut wait, POLL).min(HOST_EVERY));
        round_began = Some(std::time::Instant::now());
        // Once the game quits, nothing more of it is read: the viewer is
        // told to close (`crate::session_end`).
        let Some(_round) = crate::session_end::SESSION.round() else { end_session(&mut presenter) };
        // Whether the game's thread answers (`crate::heartbeat`), also
        // while nothing changes: the viewer's busy indicator.
        if host_sent.elapsed() >= HOST_EVERY {
            host_sent = std::time::Instant::now();
            presenter.send_host();
            // How much of a stall of the game's thread our hooks took.
            if let Some(line) = stalls.poll(crate::heartbeat::stale_now(), stall_clock.elapsed().as_millis() as u64, inproc::hook_time::snapshot) {
                log::info!("{line}");
            }
            // And how much of it they take while it answers (slow frames).
            if let Some(line) = busy.poll(stall_clock.elapsed().as_millis() as u64, inproc::hook_time::snapshot) {
                log::info!("{line}");
            }
        }
        if let Some(p) = &mut profile {
            p.round_start();
        }
        let mut t = std::time::Instant::now();
        // Now and then, how much everything holds (memory diagnostics).
        if last_stats.elapsed() >= memory_every() {
            last_stats = std::time::Instant::now();
            let mem = inproc::process::memory().unwrap_or_default();
            let (canvases, ops, presented) = crate::ui_trace::stats().unwrap_or_default();
            log::info!(
                "memory: working set {} MiB (peak {}), commit {} MiB (peak {}); sprites {}, atlas {} MiB{}, fonts {}, screen art {}+{}+{}, districts {}; ui canvases {canvases}, ops {ops}, presented {presented}; scene cells {} KiB, sprites {}+{}",
                mem.working_set >> 20,
                mem.peak_working_set >> 20,
                mem.commit >> 20,
                mem.peak_commit >> 20,
                compiled.atlas.sprites.len(),
                atlas_used.max(sent_bytes) >> 20,
                if pixels { "" } else { " (estimated)" },
                fonts.len(),
                screen_art.regions.len() + screen_art.resized_regions.len(),
                screen_art.fills.len(),
                screen_art.dynamic.len(),
                district_art.len(),
                scene_size.0 >> 10,
                scene_size.1,
                scene_size.2,
            );
            if let Some(detail) = crate::ui_trace::detail() {
                log::info!("memory: ui mirror {detail}");
            }
            if let Some(path) = &art_list_out {
                let lines = format!("{}{}{}", art_list_header(), screen_art.to_lines(), fonts.lines());
                if let Err(e) = std::fs::write(path, lines) {
                    log::warn!("art list {}: {e}", path.display());
                }
            }
        }
        // Art built: from now on the scenes use it.
        while let Some(done) = art_thread.try_done() {
            let now = std::time::Instant::now();
            let built_folders = whole_folders.remove(&done.generation);
            match done.built {
                Ok(Built::Whole(c)) => {
                    log::info!(
                        "art: whole description built in {} ms for {}: {}; {} ms after it was first wanted",
                        done.took.as_millis(),
                        done.reason,
                        art_summary(&c, pixels),
                        now.duration_since(done.wanted).as_millis()
                    );
                    let c = Arc::new(c);
                    // (A newer whole description replaced it in the
                    // renderer: scenes keep naming the table there was
                    // until that one is built.)
                    if !presenter.adopt_assets(&c, done.generation) {
                        if let Some(token) = done.token {
                            schedule.done(now, token);
                        }
                        new_art = true;
                        continue;
                    }
                    if let Some(folders) = built_folders {
                        compiled_folders = folders;
                    }
                    // The viewer has the pixels; this side needs the rest only.
                    compiled = c.without_pixels();
                    art_generation += 1;
                    mapper = Mapper::new(&compiled);
                    menus = menus_of(&compiled);
                    for (_, _, token) in pending_growths.drain(..) {
                        if let Some(token) = token {
                            schedule.done(now, token);
                            previews.art_done(token);
                        }
                    }
                    if let Some(token) = done.token {
                        schedule.done(now, token);
                        previews.art_done(token);
                    }
                }
                Ok(Built::Growth(g)) => {
                    log::info!(
                        "art: grown by {} sprites ({} KiB of pixels) in {} ms for {}; {} sprites, atlas {} colour and {} index pages; {} ms after it was first wanted",
                        g.atlas.sprites.len(),
                        g.atlas.bytes() >> 10,
                        done.took.as_millis(),
                        done.reason,
                        done.sprites,
                        done.pages.0,
                        done.pages.1,
                        now.duration_since(done.wanted).as_millis()
                    );
                    // Even an empty growth waits behind earlier growths.
                    // Adopting it now would retire their generation records
                    // before their sprite tables have been applied here.
                    pending_growths.push_back((done.generation, g, done.token));
                }
                Err(e) => {
                    if built_folders.is_some() {
                        presenter.abandon_assets(done.generation);
                    }
                    if e.contains(crate::assets::STOPPED) {
                        log::info!("art: a whole description for art folders no longer shown stopped after {} ms", done.took.as_millis());
                        // The renderer may still build it, but scenes
                        // cannot name it or any of its additions.
                    } else {
                        log::error!("art for {}: {e}", done.reason);
                    }
                    if let Some(token) = done.token {
                        schedule.done(now, token);
                    }
                }
            }
            if pixels {
                let page = 2048u64 * 2048;
                atlas_used = done.pages.0 as u64 * 4 * page + done.pages.1 as u64 * page;
            }
            new_art = true;
        }
        // Growths the renderer has: a renderer given the art gets them now,
        // one compiling its own has compiled them (`Presenter::art_status`).
        let ready = presenter.art_status().unwrap_or(u64::MAX);
        if pending_growths.front().is_some_and(|(generation, _, _)| *generation <= ready || presenter.assets_superseded(*generation)) {
            let now = std::time::Instant::now();
            while pending_growths.front().is_some_and(|(generation, _, _)| *generation <= ready || presenter.assets_superseded(*generation)) {
                let Some((generation, g, token)) = pending_growths.pop_front() else { break };
                // (Of a table scenes no longer name: not applied here
                // either, its sprite ids would mean other art.)
                if presenter.adopt_growth(&g, generation) {
                    compiled.apply(&g);
                    art_generation += 1;
                }
                // (Art the screen waited for is there now.)
                if let Some(token) = token {
                    schedule.done(now, token);
                    previews.art_done(token);
                }
            }
            mapper = Mapper::new(&compiled);
            menus = menus_of(&compiled);
            new_art = true;
        }
        if !schedule.pending() && !schedule.running() {
            art_wanted = None;
        }
        mark(&mut profile, "art handover", &mut t);
        // (For the preview latency: the selection before this frame.)
        let selection_before_ui = crate::preview::selected().number;
        // The game's interface, mirrored (else its text only), and the city
        // screen state it was presented with (`crate::city_screen`).
        let (mut ui, presented, native_ui, sources) = ui_trace::snapshot().map_or_else(
            || (None, None, None, ui_trace::SnapshotSources::default()),
            |snapshot| (Some(snapshot.frame), snapshot.presented, Some(snapshot.native), snapshot.sources),
        );
        // The order buttons lit as the game's targeting mode says, however
        // the command was given (`crate::lit_orders`); they have no looks.
        let lit_orders: Vec<u64> = match (ui.as_mut(), civ3_sys::controls::sites(game.build)) {
            (Some(f), Some(sites)) => {
                let live = civ3_sys::controls::Live;
                let lit = civ3_sys::lit_orders::Mode::read(&live, sites.main_gui).map(|mode| civ3_sys::lit_orders::lit(&live, sites.main_gui, mode)).unwrap_or_default();
                crate::lit_orders::apply(f, &lit, |sprites| sources.get(sprites)).into_iter().map(u64::from).collect()
            }
            _ => Vec::new(),
        };
        if let (Some(p), Some(f)) = (&mut profile, &ui) {
            p.count("ui frame draws", f.draws.len());
        }
        mark(&mut profile, "ui frame", &mut t);
        // The world shows while the map screen's interface is composed (under
        // whatever screens are open over it). Between games the game's tables
        // may still hold one (the last game's, or the scenario the Load
        // Scenario screen loaded to describe it), which is not one to show.
        // Neither once the game has ended: the screens after it (the
        // victory screen, the replay, the top cities, the ranking, the hall of
        // fame) show over Main_GUI, round a map the game leaves black.
        // (Demographics and the histograph are among them, after the
        // replay: the game they come after has ended, `GameOver`.)
        let ended = match &ui { Some(f) => game_over.update(f, game.build), None => game_over.ended() };
        let in_game = ui.as_ref().is_none_or(|f| f.forms.iter().any(|f| f.1 == main_gui) && !ended);
        // The Load Scenario screen, and the world of the scenario it shows.
        let load_screen = ui.as_ref().filter(|_| !in_game).and_then(|f| crate::preview::screen(f, game.build));
        // The first complete scenario menu goes out before its optional
        // world can replace the shared atlas. Input and menu publication
        // continue while that preview is prepared. Its whole build also
        // waits for the menu's assets to reach the viewer's GPU.
        let preview = if load_screen.is_some() {
            previews.update(&compiler.install).cloned()
        } else {
            previews.close();
            complete_preview_menu = None;
            None
        };
        let menu_published = load_screen.is_some() && complete_preview_menu == load_screen;
        // The scenario's own art (a whole build for its folders, which art
        // the menu needs for a newer selection would queue behind) only once
        // the menu showing this selection went out and the selection rested
        // a moment: going through the list never waits for it. Its world
        // shows meanwhile, in the art there is (`Current::drawn`).
        let menu_published = menu_published && previews.menu_shown() && previews.rested(PREVIEW_ART_REST);
        let use_preview_art = menu_published && preview.as_ref().is_some_and(|p| {
            let (scenario, search) = p.folders();
            p.needed.is_some() && (folders == ArtFolders { scenario, search } || presenter.current_assets_ready())
        });
        // A newer selection stops the whole build of an older one's art
        // still running (`ArtStop`): its folders are no longer wanted.
        // Only the stop, at once (an atomic): nothing else is done for it
        // in this round, the one that shows the newer selection. The art
        // thread's art is then made again whole for the folders wanted
        // next (`rebuild_stopped`), not before the newer selection rests.
        let selection = previews.asked();
        if load_screen.is_some() && selection != preview_selection && !whole_folders.is_empty() && folders != compiled_folders {
            log::info!("scenario preview: a newer selection stops the art being made for {:?}", folders);
            folders_changed.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            rebuild_stopped = true;
        }
        preview_selection = selection;
        let now = match (&preview, load_screen) {
            (Some(p), _) if use_preview_art => {
                let (scenario, search) = p.folders();
                ArtFolders { scenario, search }
            }
            (_, Some(_)) => folders.clone(),
            // A highlighted scenario also changes the game's tables before
            // its menu opens. Between games those tables do not choose the
            // title's art folders.
            _ if !in_game && !ended => ArtFolders::default(),
            _ => ArtFolders::of(&game),
        };
        mark(&mut profile, "preview update", &mut t);
        let resolved_art = if use_preview_art { previews.take_art() } else { None };
        let mut recompile = false;
        // Art for what the game's screen shows (held back without it).
        let mut screen_recompile = false;
        // What the art is rebuilt for (for the log).
        let mut wanted_for: Vec<&str> = Vec::new();
        if now != folders {
            rebuild_stopped = false;
            folders = now;
            folders_changed.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            art = resolved_art.unwrap_or_else(|| units::UnitArt::new(compiler.files(&folders)));
            label_env = crate::city_labels::SystemTextEnv::new(crate::city_labels::FileEnv::new(Box::new(compiler.files(&folders))));
            // (A scenario's own `Art/Units/Palettes` give the minimap's civ colours.)
            table_colors.clear();
            labels_memo = Memo::default();
            items_memos = Default::default();
            have = units::Needed::default();
            absent_art.clear();
            art_retry.clear();
            // (Looked up once per art folders: going back and forth between
            // scenarios does not look up every screen file again.)
            let files = compiler.files(&folders);
            ui_screen::retain_present_by(&mut screen_art, &mut |path| {
                *present_in_folders.entry((folders.clone(), path.to_owned())).or_insert_with(|| files.resolve(path).is_ok())
            });
            whole_wanted = true;
            recompile = true;
            wanted_for.push("art folders");
        } else if rebuild_stopped && (use_preview_art || load_screen.is_none()) {
            // (A whole build was stopped and nothing replaced it: the art
            // thread has nothing to add to.)
            rebuild_stopped = false;
            if let Some(a) = resolved_art {
                art = a;
            }
            whole_wanted = true;
            recompile = true;
            wanted_for.push("art folders");
        } else if let Some(a) = resolved_art {
            art = a;
        }
        mark(&mut profile, "art folders", &mut t);
        // (Until the viewer is gone, see `forward_input`.)
        if hide_game_window && inproc::hidden::hiding() {
            hide_window(&mut input, &mut hiding);
        }
        // (No unit art is read here while the preview's thread reads it for
        // the preview shown.)
        art.set_reading(!(preview.is_some() && previews.art_pending()));
        // Input that waited out a stall of this thread is dropped, not
        // posted to the game in one burst (`crate::stale_input`).
        let taken_at = std::time::Instant::now();
        let waited = taken_at.saturating_duration_since(std::mem::replace(&mut last_input_take, taken_at));
        let inputs = stale_inputs.filter(presenter.take_input(), waited.as_millis().min(u32::MAX as u128) as u32);
        // A press on the Load Scenario screen: timed until a scene shows a
        // newer selection in a newer frame of the game's menu (logged).
        if load_screen.is_some() && preview_input.is_none()
            && inputs.iter().any(|i| matches!(i, scene::Input::Key { down: true, .. } | scene::Input::MouseButton { down: true, .. }))
        {
            preview_input = Some((std::time::Instant::now(), selection_before_ui, ui.clone()));
        }
        // What our preferences tabs take does not reach the game.
        let inputs = prefs.route(inputs, input.viewport);
        presenter.forward_log();
        mark(&mut profile, "preview+input", &mut t);
        let snapshot = if in_game { snapshot(&game) } else { Err(Civ3Error::NoGame) };
        mark(&mut profile, "snapshot", &mut t);
        let shown = match (snapshot, load_screen) {
            (_, Some(rect)) => Shown::Preview(rect, preview.as_ref().map(|p| {
                let (scenario, search) = p.folders();
                let own_art = use_preview_art && compiled_folders == ArtFolders { scenario, search };
                let ready = own_art && p.art_ready(&compiled, &fonts);
                let asking = !use_preview_art || p.needed.as_ref().is_some_and(|n| !have.covers(n));
                let building = whole_wanted || schedule.pending() || schedule.running() || !pending_growths.is_empty() || !art_thread.idle();
                let loading = crate::preview::loading(ready, p.needed.is_none(), asking, building);
                PreviewShown { loading, ..PreviewShown::of(p, p.drawn(use_preview_art, &compiled, &fonts), ready, use_preview_art) }
            })),
            // The city screen is the one the game presented: its city's map
            // and fog, never another's read meanwhile (`crate::city_screen`).
            (Ok((mut world, view)), None) => {
                world.focus_city = crate::city_screen::screen_state(world.focus_city, (view.camera_x, view.camera_y), presented).focus_city;
                Shown::Map(world, view)
            }
            // Between games: the title menu, which follows the pointer.
            (Err(Civ3Error::NoGame), None) => {
                if let Some(m) = menus.as_ref().and_then(|ms| ms.menu("title").map(|m| (ms, m))) {
                    for i in &inputs {
                        m.0.input(m.1, &mut title, input.viewport, i);
                    }
                }
                Shown::Title(title.clone())
            }
            (Err(e), None) => {
                log::debug!("snapshot: {e}");
                forward_input(&game, &mut input, inputs);
                prefs.native_inputs_posted(input.window);
                continue;
            }
        };
        ui_trace::set_skipping(matches!(shown, Shown::Map(..)));
        let mut inputs = inputs;
        first_run_round.pending = prefs.first_run_pending();
        first_run_round.ready &= matches!(shown, Shown::Title(_));
        let opening = first_run.inputs(first_run_round, &inputs, std::time::Instant::now());
        inputs.extend(opening);
        forward_input(&game, &mut input, inputs);
        prefs.native_inputs_posted(input.window);
        if input.viewport == (0, 0) {
            input.game_window();
        }
        // The fence acquire must precede the native modal lookup. Captured
        // draw frames alone can still describe a popup that just closed.
        let prefs_form = civ3_sys::ui_hooks::preferences_form(game.build);
        let modal = prefs.update_with_modal(ui.as_ref(), prefs_form, input.viewport,
            || game.modal_form().ok().map(|modal| modal.filter(|&m| m != 0)));
        // Combat timing receives committed values; draft-only settings never project here.
        civ3_sys::combat_speed::set_speed_quarters((prefs.settings().animation.effective_combat_speed() * 4.0) as u8);
        let prefs_draws = prefs.draws(&fonts);
        mark(&mut profile, "controls: prefs", &mut t);
        let prefs_changed = published_prefs.0 != prefs_draws || published_prefs.1.as_ref() != Some(prefs.settings()) || published_prefs.2 != prefs.settings_preview();
        let texts = if ui.is_some() { Vec::new() } else { trace::recent_texts() };
        // What can be clicked (for the viewer's button snapping): our own
        // title menu's buttons, else the game's controls.
        let own_title = menus.as_ref().and_then(|ms| ms.menu("title").map(|m| (ms, m))).filter(|_| matches!(shown, Shown::Title(_)) && ui.is_none());
        let mut controls = match own_title {
            Some(_) => Vec::new(),
            None => native_ui.as_ref().map(|native| native.controls.clone()).unwrap_or_else(||
                civ3_sys::controls::sites(game.build).map(|sites| civ3_sys::controls::controls(&civ3_sys::controls::Live, &sites)).unwrap_or_default()),
        };
        prefs.retain_native_controls(&mut controls);
        mark(&mut profile, "controls: read", &mut t);
        // The military advisor's unit columns where it drew units.
        if let Some(frame) = ui.as_ref() {
            let strips = native_ui.as_ref().map(|native| native.military.clone()).unwrap_or_else(||
                civ3_sys::controls::screens::military_unit_strips(&civ3_sys::controls::Live, &controls));
            controls.extend(ui_screen::military_unit_controls(frame, &strips));
        }
        // The native start screen draws its text rows directly into the
        // main form, without child Button objects. Use its own layout and
        // the captured labels' system text extents for the same hit boxes.
        let native_title = native_ui.as_ref().map_or_else(
            || civ3_sys::controls::title_layout(&civ3_sys::controls::Live, game.build), |native| native.title);
        if let Some((frame, layout)) = ui.as_ref().zip(native_title) {
            use crate::city_labels::Env as _;
            controls.extend(crate::setup_controls::title(frame, layout, |run| {
                let font = civ3_assets::fonts::FontStyle { face: run.face.clone(), height: run.height.abs(), weight: run.weight, italic: run.italic };
                label_env.text_width(&font, &run.text)
            }));
        }
        // (For the first run, next round: the title's Preferences row, and
        // whether the title is up with nothing loading or over it.)
        if prefs.first_run_pending() {
            let building = whole_wanted || schedule.pending() || schedule.running() || !pending_growths.is_empty() || !art_thread.idle();
            first_run_round.target = native_title.and_then(|_| crate::setup_controls::preferences(&controls));
            first_run_round.ready = input.viewport != (0, 0) && !building && modal.is_none() && !prefs.is_open();
        }
        // Our preferences tabs' controls are over the game's.
        let prefs_looks = prefs.looks(&fonts);
        let mut interactables: Vec<scene::Interactable> =
            prefs_looks.iter().map(|l| scene::Interactable { region: l.region, alt_right_click: false, anchor: l.anchor, rect: l.rect, kind: l.kind, id: l.id }).collect();
        interactables.extend(match own_title {
            Some((ms, m)) => ms.interactables(m, input.viewport),
            None => ui_screen::interactables(&controls),
        });
        if let Some(frame) = ui.as_ref() {
            // (Measures the rows' text: made again only when the frame, the
            // controls or the fonts change, not in every round.)
            // (The production list's entries end with the unit's statistics.)
            let stat_lists: Vec<u32> = civ3_sys::build_queue::order_list_form(&civ3_sys::controls::Live).into_iter().collect();
            interactables = anchors_memo.get((frame.clone(), fonts.revision(), controls.clone(), interactables.clone(), stat_lists.clone()), || {
                let mut anchored = interactables.clone();
                ui_screen::content_anchors(frame, &fonts, &controls, &mut anchored, &stat_lists);
                anchored
            });
            mark(&mut profile, "controls: anchors", &mut t);
        }
        if let Some(f) = ui.as_ref() {
            let row = native_ui.as_ref().map_or_else(|| civ3_sys::controls::sites(game.build).and_then(|sites|
                civ3_sys::controls::status_unit_row(&civ3_sys::controls::Live, &sites, &controls)), |native| native.status_row);
            interactables.extend(ui_screen::status_unit_targets(f, row));
        }
        // What the open city screen tests in its code (citizens, units).
        if let (Shown::Map(world, _), Some(f)) = (&shown, ui.as_ref()) {
            if world.focus_city.is_some() {
                interactables.extend(ui_screen::city_drawn_targets(f, modal.map(|m| m + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4)));
            }
        }
        // The pointer the game shows (docs §16.11), and for which of the
        // viewer's pointer moves (read first: the cursor is as new).
        let cursor_at = crate::cursor::handled::at();
        let game_cursor = civ3_sys::cursor::sites(game.build).and_then(|s| civ3_sys::cursor::read(&civ3_sys::controls::Live, &s));
        // While a command waits for its tile, over the world: the command's
        // pointer, or "not here" where it cannot go (`crate::cursor::targeting`).
        let game_cursor = match &shown {
            Shown::Map(world, _) => crate::cursor::targeting(game_cursor, world.targeting.mode, world.targeting.target_ok, crate::world_input::pointer_tile().is_some()),
            _ => game_cursor,
        };
        let standard = inproc::pointer::last_set_cursor().and_then(inproc::pointer::standard_cursor);
        let cursor = crate::cursor::scene_cursor(game_cursor, standard, |n| compiled.set(n).map(|(s, _)| s.raw()), crate::cursor::system_arrow());
        if last.as_ref().is_none_or(|l| l.4.0 != cursor) {
            log::debug!("cursor: the game shows {game_cursor:?} (system pointer {standard:?}): {cursor:?}, for the move to {cursor_at:?}");
        }
        let cursor = (cursor, cursor_at);
        mark(&mut profile, "controls+prefs", &mut t);
        if crate::ui_trace::replay_revision() != replay_revision {
            let picture = crate::ui_trace::replay_picture();
            replay_revision = picture.revision;
            replay_surface = picture.pixels
                .map(|(size, pixels)| vec![("replay-map", size, pixels.to_vec())])
                .unwrap_or_default();
        }
        // A completed asynchronous font load changes a static screen too.
        if fonts.take_loaded() {
            recompile = true;
            screen_recompile = true;
            wanted_for.push("text");
        }
        let state = (shown, texts, ui, interactables, cursor, native_ui);
        let notice_state = if matches!(&state.0, Shown::Map(..)) {
            // (With the renderer's own: why a press under a screen was not
            // acted on, `crate::modal`.) One per tile, each until its own
            // time is up on this clock, not the game's frames.
            let notices = notices_shown.update(std::time::Instant::now(), crate::unit_notices::read(&game), crate::modal::notices(&game));
            (notices, game.map_message_background().unwrap_or(0x8000_0000), notices_shown.lifetimes())
        } else { Default::default() };
        let deferred = std::mem::take(&mut preview_deferred);
        // A native focus request can arrive before simulation changes the world.
        // Publish it even while combat waits for the viewer to expose its tile.
        let camera_focus = matches!(state.0, Shown::Map(..)).then(|| crate::native_camera::focus(game)).flatten();
        let camera_arrival = civ3_sys::native_camera::arrival(game).filter(|a| camera_focus.is_some_and(|f| f.serial == a.serial)).map(|a| scene::CameraArrival { serial: a.serial, bounds: a.bounds });
        // (A unit sliding to the next tile moves on in every round.)
        let unchanged = last.as_ref() == Some(&state) && last_notices == notice_state && last_camera_focus == camera_focus && last_camera_arrival == camera_arrival && !deferred
            && !crate::unit_slides::busy(std::time::Instant::now());
        // (A new frame of the game's menu: see `preview_deferred`.)
        let menu_changed = last.as_ref().is_none_or(|l| l.2 != state.2);
        last_notices = notice_state.clone();
        mark(&mut profile, "compare", &mut t);
        // GPU upload completion can make another optional preload chunk
        // eligible even on a static screen. CPU compilation alone must not
        // let background art outrun the viewer.
        let planning = planner.as_ref().is_some_and(crate::preload::Planner::ready);
        let preload_ready = if planning || !preload.finished() {
            let ready = presenter.preload_ready();
            mark(&mut profile, "preload: upload acknowledgment", &mut t);
            ready
        } else { false };
        let preload_work = planning || (!preload.finished() && preload_ready);
        if unchanged && replay_revision == last_replay_revision && !recompile && !new_art && !schedule.pending() && !retry_wanted && !preload_work && input.look.is_none() && input.world_buttons.is_empty() && !prefs_changed {
            if let Some(p) = &mut profile {
                p.event("unchanged", 1);
            }
            continue;
        }
        let (shown, texts, ui, interactables, cursor, native_ui) = &state;
        // (Once per round: the memos below key on it.)
        let still = match shown {
            Shown::Map(world, _) => Arc::new(still_world(world)),
            _ => Arc::default(),
        };
        let files = compiler.files(&folders);
        let generated = &crate::ui_trace::generated_surfaces(game.build);
        // The parts of the interface the viewer arranges (docs §16.11): the
        // screen open, the popup over it (split off: the viewer may draw it
        // elsewhere).
        let screen_open = ui.as_ref().and_then(|f| crate::interface_parts::active_screen(f, game.build));
        let screen_size = (input.viewport.0 as i32, input.viewport.1 as i32);
        // (A context menu over the map is one too, modal or not: the topmost
        // form there.)
        let context_menu = ui.as_ref().and_then(|f| f.forms.last().filter(|t| civ3_sys::controls::CONTEXT_MENU_VTABLES.contains(&t.0)).map(|t| t.1));
        let popup_and_canvas = match (ui, modal) {
            (Some(f), m) if matches!(shown, Shown::Map(..)) || screen_open.is_some() => {
                let modal_popup = m.map(|m| m + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4).filter(|&c| c != crate::detached::hidden_canvas() && c != crate::async_popups::hidden_canvas()).and_then(|c| crate::interface_parts::popup(f, c, screen_open.map(|s| s.0), screen_size).map(|p| (p, c)));
                modal_popup.or_else(|| context_menu.and_then(|c| crate::interface_parts::popup(f, c, screen_open.map(|s| s.0), screen_size).map(|p| (p, c))))
            }
            // Between games: a popup over the title or a setup screen (those
            // screens are modal forms themselves: not popups).
            (Some(f), Some(m)) if matches!(shown, Shown::Title(_)) => {
                let c = m + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4;
                (c != crate::async_popups::hidden_canvas()).then(|| crate::interface_parts::menu_popup(f, c, screen_size).map(|p| (p, c))).flatten()
            }
            _ => None,
        };
        let popup = popup_and_canvas.map(|(p, _)| p);
        // A context menu or the tile box the game opened at the map click
        // just made goes there (decided when it opens, kept while open).
        let popup_anchor = match (popup_and_canvas, ui) {
            (Some(((_, [_, _, w, h]), canvas)), Some(f)) if matches!(shown, Shown::Map(..)) && screen_open.is_none() => {
                if popup_anchor_state.is_none_or(|(c, _)| c != canvas) {
                    let form = canvas - civ3_sys::layout::BASE_FORM_DATA_CANVAS - 4;
                    let kind = f.forms.iter().find(|t| t.1 == canvas).and_then(|t| {
                        crate::interface_parts::click_popup(game.build, t.0, || {
                            inproc::Ptr32::<u32>::new(form + crate::interface_parts::CONTEXT_MENU_FLAGS).read().ok()
                        })
                    });
                    let chained = click_popup_seen.is_some_and(|t| t.elapsed() < Duration::from_secs(2));
                    let press = crate::world_input::last_press().filter(|p| chained || p.when.elapsed() < Duration::from_secs(2));
                    // The tile box shows the tile the game's map conversion
                    // gave `open_tile_info` (kept while a popup opened at the
                    // click is open: a city menu's "Terrain details" is the
                    // city's tile), not where a later pointer went.
                    let game_tile = crate::world_input::popup_tile();
                    let anchor = kind.zip(press).map(|(kind, p)| crate::interface_parts::click_anchor(kind, p.at, p.tile, game_tile, p.zoomed_out, (w, h)));
                    if let Some(a) = anchor {
                        log::info!("popup: {kind:?} at the map click{}, anchored {a:?}", if chained { " (opened from the popup there)" } else { "" });
                    }
                    popup_anchor_state = Some((canvas, anchor));
                }
                let anchor = popup_anchor_state.and_then(|(_, a)| a);
                if anchor.is_some() {
                    click_popup_seen = Some(std::time::Instant::now());
                }
                anchor
            }
            _ => {
                popup_anchor_state = None;
                None
            }
        };
        // A context menu (or the tile box) opened by a click on an open
        // screen, e.g. a city screen garrison unit's menu: the game put it
        // at that click on the screen, where it stays with the screen.
        let popup_on_screen = screen_open.is_some() && popup_and_canvas.zip(ui.as_ref()).is_some_and(|((_, canvas), f)| {
            f.forms.iter().find(|t| t.1 == canvas)
                .and_then(|t| crate::interface_parts::click_popup(game.build, t.0, || None))
                .is_some()
        });
        let popup_canvases: Vec<u32> = native_ui.as_ref().into_iter()
            .flat_map(|native| native.modal_stack).filter(|&form| form != 0)
            .map(|form| form + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4)
            .filter(|&canvas| canvas != crate::detached::hidden_canvas() && canvas != crate::async_popups::hidden_canvas())
            .chain(popup_and_canvas.map(|(_, canvas)| canvas))
            .fold(Vec::new(), |mut out, canvas| { if !out.contains(&canvas) { out.push(canvas); } out });
        let (ui_below, popup_frames) = match ui {
            Some(frame) => {
                let (below, layers) = crate::interface_parts::split_modal_stack(frame, &popup_canvases, screen_open.map(|s| s.0), screen_size);
                (Some(below), layers)
            }
            None => (None, Vec::new()),
        };
        // Unit pictures in panels are drawn in the viewing player's colours.
        let unit_colors = match shown {
            Shown::Map(world, _) => world.color_tables.get(world.player.max(0) as usize).copied().unwrap_or(0),
            Shown::Title(_) | Shown::Preview(..) => 0,
        };
        // An open city screen covers everything under it; the map shows in
        // its gap, as a view of its own (§16.8).
        let city = match (shown, &ui_below) {
            (Shown::Map(world, _), Some(frame)) if world.focus_city.is_some() && screen_open.is_none_or(|s| s.1 == "City") => {
                ui_screen::city_screen(frame, &files, files.roots(), &mut city_bands)
            }
            _ => None,
        };
        // Its map centred on the city as the screen intends (not clamped to
        // the map as the game's camera is), what the game drew over its map
        // moved with it, the pointer there moved back (`crate::city_screen`).
        let city_anchor = match (shown, city) {
            (Shown::Map(world, view), Some((band, _))) => world.focus_city.and_then(|id| world.city(id)).map(|c| {
                let game_camera = crate::city_screen::screen_state(world.focus_city, (view.camera_x, view.camera_y), presented).camera;
                let period = (if world.wrap_x { 64 * world.width } else { 0 }, if world.wrap_y { 32 * world.height } else { 0 });
                crate::city_screen::Anchor::new((c.x, c.y), screen_size, band, game_camera, period)
            }),
            _ => None,
        };
        input.city_map = city_anchor;
        let visible_ui = match city {
            Some((_, first)) => ui_below.as_ref().map(|f| {
                let mut f = ui_screen::from_draw(f, first);
                if let Some(a) = &city_anchor {
                    a.apply(&mut f);
                }
                f
            }),
            None => ui_below.clone(),
        };
        // A named screen smaller than the window (an advisor; not the city
        // screen, whose map is a view of its own), and what is over it: a
        // part of its own, which the viewer may move (out from under its
        // zoom strip).
        let screen_part = match (&visible_ui, screen_open) {
            (Some(f), Some((index, name))) if name != "City" && city.is_none() => crate::interface_parts::active_screen_part(f, index, game.build, screen_size),
            _ => None,
        };
        let (visible_ui, ui_screen_part) = match (visible_ui, screen_part) {
            (Some(f), Some((first, rect))) => {
                let (below, above) = crate::interface_parts::split_at(&f, first);
                (Some(below), Some(crate::interface_parts::clipped(&above, rect)))
            }
            (v, _) => (v, None),
        };
        // On the map's own screen: the corner's buttons and their tooltip
        // (drawn in the view the pointer is in only: the rest of the
        // interface leaves them out) and the unit's orders (reachable from
        // every view).
        let hud = civ3_sys::ui_hooks::hud_buttons(game.build);
        let on_map = matches!(shown, Shown::Map(..)) && screen_open.is_none();
        let hud_ui = visible_ui.as_ref().filter(|_| on_map).and_then(|f| crate::interface_parts::map_hud(f, game.build));
        let corner_part = hud_ui.and_then(|f| crate::interface_parts::corner_part(f, &hud.corner));
        let corner = corner_part.as_ref().map(|(r, _)| *r);
        let reachable = hud_ui.and_then(|f| crate::interface_parts::forms_rect(f, |c| hud.is_unit_order(c)));
        let hint = crate::interface_parts::capture_hint(corner.unwrap_or([0, 0, 149, 54]));
        let corner_items = match (corner, hud_ui) {
            (Some(r), Some(f)) => ui_screen::items(&crate::interface_parts::region(f, r), files.roots(), generated, unit_colors),
            _ => Vec::new(),
        };
        let visible_ui = match (visible_ui, &corner_part) {
            (Some(f), Some((_, draws))) => Some(crate::interface_parts::without(&f, draws)),
            (v, _) => v,
        };
        // The game's frame (its main screen's panels and buttons) and the
        // menus over it (screens and popups), which a viewer may scale
        // differently: between games (and over a previewed scenario) all of
        // it is menus.
        // After a game its screens show over the frame as the advisors did
        // (a part of their own, `screen_part`), the frame round them.
        let (frame_ui, menu_ui) = match (&visible_ui, shown) {
            (Some(f), Shown::Map(..)) => {
                let (frame, menus) = ui_screen::split(f, ui_screen::menus_from(f));
                (Some(frame), Some(menus))
            }
            (Some(f), Shown::Title(_)) if ended && screen_part.is_some() => {
                let (frame, menus) = ui_screen::split(f, ui_screen::menus_from(f));
                (Some(crate::interface_parts::after_game_frame(&frame, game.build)), Some(menus))
            }
            (Some(f), Shown::Title(_) | Shown::Preview(..)) => (None, Some(f.clone())),
            (None, _) => (None, None),
        };
        // The game's loading bar while it shows (loading a game, between
        // turns), a part of its own: the viewer keeps it clear of the
        // popups over it (`scene::Interface::loading_bar`).
        let (frame_ui, menu_ui, loading_bar) = match (frame_ui, menu_ui) {
            (f, Some(m)) if crate::loading_cursor::split_bar(&m).is_some() => {
                let (rest, bar, rect) = crate::loading_cursor::split_bar(&m).unwrap_or_default();
                (f, Some(rest), Some((bar, rect)))
            }
            (Some(f), m) => match crate::loading_cursor::split_bar(&f) {
                Some((rest, bar, rect)) => (Some(rest), m, Some((bar, rect))),
                None => (Some(f), m, None),
            },
            (f, m) => (f, m, None),
        };
        let bar_items = loading_bar.as_ref().map(|(f, _)| bar_memo.get((f.clone(), unit_colors), || ui_screen::items(f, files.roots(), generated, unit_colors))).unwrap_or_default();
        // The reports the game did not wait for (`crate::async_popups`),
        // the oldest few, until the viewer's player has read them.
        let (queued, queued_more) = crate::async_popups::shown();
        let queued_items: Vec<Vec<ui_screen::Item>> = queued.iter().zip(queued_item_memos.iter_mut())
            .map(|(q, memo)| memo.get((q.serial, unit_colors), || ui_screen::items(&q.frame, files.roots(), generated, unit_colors)))
            .collect();
        // Our tabs on top of the game's Preferences screen (a menu), in the
        // part of the interface that draws it (the screen may be the popup
        // split off), under what the game draws over it (a popup over it,
        // e.g. C3X's info box, and that popup's frame).
        let mut prefs_items = Some(crate::prefs::items(&prefs_draws)).filter(|i| !i.is_empty());
        let popup_canvas = modal.filter(|&m| prefs_form.is_none_or(|f| f.form != m)).map(|m| m + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4);
        // (Made again only when their frame, the tabs in it or the colours
        // change: units animating change none of them.)
        let [frame_memo, menu_memo, popup_memo, screen_memo] = &mut items_memos;
        let mut with_tabs = |memo: &mut Memo<_, _>, f: &crate::ui::Frame| {
            let at = prefs_form.as_ref().and_then(|form| crate::prefs::tabs_slot(f, form, popup_canvas));
            let extra = at.and_then(|at| prefs_items.take().map(|items| (at, items)));
            memo.get((f.clone(), extra.clone(), unit_colors), || ui_screen::items_with(f, files.roots(), generated, unit_colors, extra))
        };
        let mut ui_items = frame_ui.as_ref().map(|f| frame_memo.get((f.clone(), None, unit_colors), || ui_screen::items(f, files.roots(), generated, unit_colors))).unwrap_or_default();
        // An isolated active screen replaces the menus below it. Keeping
        // an underlying advisor here would cover the world when this
        // screen is placed in a pane of its own.
        let mut menu_items = menu_ui.as_ref().filter(|_| screen_part.is_none()).map(|f| with_tabs(menu_memo, f)).unwrap_or_default();
        let popup_layers: Vec<_> = popup_frames.iter().map(|part| {
            // Canvas identity survives pushes/pops; depth-based IDs would
            // confuse a replaced dialog with its still-open parent.
            let id = part.canvas;
            (id, part.rect, with_tabs(popup_memo, &part.frame))
        }).collect();
        let popup_items: Vec<_> = popup_layers.iter().flat_map(|(_, _, items)| items.iter().cloned()).collect();
        let popup_view = popup_layers.last().map(|(id, _, _)| *id);
        let layers_now: Vec<(u32, [i32; 4], bool)> = popup_frames.iter().map(|p| (p.canvas, p.rect, p.attached)).collect();
        if layers_now != popup_layers_logged {
            log::info!("popup layers (bottom to top; canvas, rect, attached to the one under it): {:x?} of canvases {:x?}", layers_now, popup_canvases);
            popup_layers_logged = layers_now;
        }
        let screen_items = ui_screen_part.as_ref().map(|f| with_tabs(screen_memo, f)).unwrap_or_default();
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
        let mut preview_labels: Arc<crate::preview::LabelItems> = Arc::default();
        let label_items = match shown {
            Shown::Map(world, view) if ui.is_some() && world_labels() => {
                // (Only under a screen or modal form: on the plain map the game
                // may have drawn none itself, e.g. with its map rendering off.)
                labels_not_in_game_view = (screen_open.is_some() || modal.is_some()) && !ui_items.iter().any(|i| matches!(i, ui_screen::Item::Element(_, true) | ui_screen::Item::Text(_, _, true)));
                ui_items.retain(|i| !matches!(i, ui_screen::Item::Element(_, true) | ui_screen::Item::Text(_, _, true)));
                labels_memo.get((still.clone(), view.zoomed_out), || WorldItems::new(view.zoomed_out, |reduced| {
                    let labels = crate::city_labels::labels(world, reduced, &mut label_env);
                    let everywhere = [-(1 << 24), -(1 << 24), 1 << 24, 1 << 24];
                    crate::city_labels::items(&labels, (0, 0), everywhere, true)
                }))
            }
            // (Drawn with the preview's scene, `preview::scene`.)
            // (Made with it, on the preview's thread.)
            Shown::Preview(_, Some(p)) if ui.is_some() && p.needed.is_some() => {
                preview_labels = p.labels.clone();
                WorldItems::full(preview_labels.iter().flat_map(|(_, items)| items.iter().cloned()).collect())
            }
            _ => WorldItems::default(),
        };
        // The go-to path's turn count (§10.15), with the world as the labels.
        let target_items = match shown {
            Shown::Map(world, view) => WorldItems::new(view.zoomed_out, |reduced| crate::targeting::label_items(world, reduced, &mut label_env)),
            _ => WorldItems::default(),
        };
        let stack_items = match shown {
            Shown::Map(world, view) => WorldItems::new(view.zoomed_out, |reduced|
                units::stack_items(world, reduced, &prefs.settings().accessibility, &mut label_env)),
            _ => WorldItems::default(),
        };
        let notice_items = match shown {
            Shown::Map(_, view) => {
                use crate::city_labels::Env as _;
                WorldItems::new(view.zoomed_out, |reduced| crate::unit_notices::items(&notice_state.0, reduced, crate::ui::rgb555(notice_state.1),
                    &mut |font, text| label_env.text_width(font, text)))
            }
            _ => WorldItems::default(),
        };
        let notice_layouts: Vec<_> = match shown {
            Shown::Map(_, view) => {
                use crate::city_labels::Env as _;
                notice_state.0.iter().map(|notice| WorldItems::new(view.zoomed_out, |reduced|
                    crate::unit_notices::items(std::slice::from_ref(notice), reduced, crate::ui::rgb555(notice_state.1),
                        &mut |font, text| label_env.text_width(font, text)))).collect()
            }
            _ => Vec::new(),
        };
        // The viewer's own views marked on the minimap.
        if let Shown::Map(world, view) = shown {
            // (The game's own view as the viewer shows it.)
            // The viewer's rectangles are in its last frame's world pixels:
            // with the game's zoom changed since (its zoom key), in this
            // frame's, as the viewer will show them, not at the old scale.
            let now = if view.zoomed_out { 2 } else { 1 };
            if let Some(main) = input.main_view {
                let main = scene::rescale_view_rect(main, input.main_view_downscale, now);
                ui_screen::replace_game_view_mark(&mut ui_items, world, (view.tile_x.0, view.tile_y.0, view.tile_x.1, view.tile_y.1), main, view.zoomed_out);
            }
            let views: Vec<_> = input.views.iter().map(|&v| scene::rescale_view_rect(v, input.views_downscale, now)).collect();
            ui_screen::mark_views(&mut ui_items, world, &views, view.zoomed_out, view_rect_color());
        }
        // Where the renderer may point at the minimap (not under a menu,
        // where a click is the menu's).
        let minimap = match shown {
            Shown::Map(world, view) if !crate::modal::modal_open(&game) => ui_screen::minimap_area(&ui_items, world, view.zoomed_out),
            _ => None,
        };
        // (Our tabs where no part draws the screen: on top.)
        menu_items.extend(prefs_items.take().unwrap_or_default());
        // Every detected popup is an overlay of its own. Its parent screen
        // keeps the bounds measured before the popup was split off, even if
        // the popup reaches outside it. Never merge those bounds or items:
        // fitting their union moves and rescales the screen beneath a dialog.
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
                _ => {
                    log::debug!("modal input: {button:?} at world {at:?} ({action:?}): no menu that allows it");
                    let form = game.modal_form().ok().flatten().unwrap_or(0);
                    // (No notice under a popup opened at a map click: those
                    // are for full screens.)
                    if let (Some(tile), true) = (crate::scene_map::tile_at(world, at, view.zoomed_out), form != 0 && !crate::world_input::click_popup_open(&game)) {
                        crate::modal::refuse(&game, tile, &crate::modal::Refused::OtherForm(form));
                    }
                }
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
        // The controls in their other states (hover, pressed), for the
        // viewer to draw at once (`crate::looks`), and their art.
        let arts: Vec<crate::looks::ControlArt> = native_ui.as_ref().map(|native| {
            native.arts.iter().filter(|art| !lit_orders.contains(&art.id) && controls.iter().any(|control| control.id == art.id)).cloned().collect()
        }).unwrap_or_default();
        let control_looks: Vec<crate::looks::ControlLooks> = match ui {
            Some(f) => arts.iter().filter_map(|a| crate::looks::control_looks(f, a)).collect(),
            None => Vec::new(),
        };
        mark(&mut profile, "ui items: commands and look states", &mut t);
        let looks_items = |draws: &[crate::ui::ScreenDraw]| ui_screen::items(&crate::ui::Frame { draws: draws.to_vec(), ..Default::default() }, files.roots(), generated, unit_colors);
        let mut looks_art = false;
        for (_, draws) in control_looks.iter().flat_map(|c| &c.states) {
            looks_art |= ui_screen::add_present_art(&looks_items(draws), &mut screen_art, &mut fonts, &files, &mut absent_art);
        }
        for (_, draws) in prefs_looks.iter().flat_map(|c| &c.states) {
            looks_art |= ui_screen::add_present_art(&crate::prefs::items(draws), &mut screen_art, &mut fonts, &files, &mut absent_art);
        }
        mark(&mut profile, "ui items: control source lookup", &mut t);
        if looks_art {
            screen_recompile = true;
            wanted_for.push("control states");
        }
        let plain_text: Vec<_> = if ui.is_none() {
            texts.iter().cloned().map(|run| ui_screen::Item::Text(run, [i32::MIN, i32::MIN, i32::MAX, i32::MAX], false)).collect()
        } else { Vec::new() };
        let mandatory = ui_screen::mandatory_items([&ui_items, &menu_items, &popup_items, &screen_items, &corner_items, &plain_text], &notice_items);
        if ui_screen::add_present_art(mandatory.clone(), &mut screen_art, &mut fonts, &files, &mut absent_art) {
            screen_recompile = true;
            wanted_for.push("interface");
        }
        mark(&mut profile, "ui items: visible source lookup", &mut t);
        if ui_screen::add_present_art(bar_items.iter().chain(queued_items.iter().flatten()), &mut screen_art, &mut fonts, &files, &mut absent_art) {
            screen_recompile = true;
            wanted_for.push("loading bar, queued reports");
        }
        if ui_screen::add_present_art(&hint.1, &mut screen_art, &mut fonts, &files, &mut absent_art) {
            recompile = true;
            wanted_for.push("hint");
        }
        if ui_screen::add_present_art(&label_items, &mut screen_art, &mut fonts, &files, &mut absent_art) {
            recompile = true;
            wanted_for.push("city labels");
        }
        if ui_screen::add_present_art(&target_items, &mut screen_art, &mut fonts, &files, &mut absent_art) {
            recompile = true;
            wanted_for.push("go-to turns");
        }
        if ui_screen::add_present_art(&stack_items, &mut screen_art, &mut fonts, &files, &mut absent_art) {
            recompile = true;
            wanted_for.push("stack labels");
        }
        // Visible art described before that has not arrived although
        // nothing is being built (an addition that failed or was given
        // up): asked for again at once, a few times.
        if !whole_wanted && !schedule.screen_pending() && !schedule.running() && art_thread.idle() && pending_growths.is_empty() {
            let missing = ui_screen::unready_art(mandatory.clone().filter(|i| !absent_art.covers(i)), &compiled);
            let again = art_retry.due(&missing, std::time::Instant::now());
            if !again.is_empty() {
                described_screen = described_screen.minus(&again);
                screen_recompile = true;
                wanted_for.push("visible art not resident");
            }
        }
        mark(&mut profile, "ui items+labels", &mut t);
        // Art the description does not have yet: units, text fonts.
        // (A preview's units stand still: only the frames they show, as the
        // preview's thread resolved them.)
        let needed = match shown {
            Shown::Map(world, _) => {
                let mut needed = units::Needed::of(world, &mut art);
                crate::effects::preload(&mut needed, &mut art, world);
                Some(Arc::new(needed))
            },
            Shown::Preview(_, p) => p.as_ref().and_then(|p| p.needed.clone()),
            Shown::Title(_) => None,
        };
        if let Some(needed) = needed {
            if !have.covers(&needed) {
                have.merge(&needed);
                recompile = true;
                wanted_for.push("units");
            }
        }
        if fonts.add(&compiler.files(&folders), texts) {
            recompile = true;
            screen_recompile = true;
            wanted_for.push("text");
        }
        if let Some(last) = &mut text_trace {
            trace_text(
                match shown { Shown::Map(..) => "map", Shown::Title(..) => "title", Shown::Preview(..) => "preview" },
                ui.as_ref(),
                ui_items.iter().chain(&menu_items).chain(&popup_items).chain(&screen_items).chain(&corner_items).chain(&label_items).chain(&target_items).chain(&notice_items),
                &fonts,
                &compiled,
                last,
            );
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
        // Build the art wanted once the requests settle (a new screen asks
        // for new art several times in a row), on the art thread
        // (`crate::art_schedule`): an addition of only what the art does
        // not have yet, or a whole description when the art folders changed.
        let now = std::time::Instant::now();
        if recompile || screen_recompile {
            log::debug!("art: wanted for {}", wanted_for.join(", "));
            // A replacement whole atlas carries the mandatory menu too.
            // Canceling its predecessor cannot make still-missing menu art
            // look terminally unavailable while this replacement is pending.
            schedule.want(now, screen_recompile || whole_wanted);
            if wanted_for.contains(&"art folders") {
                previews.art_wanted(schedule.asked());
            }
            art_wanted.get_or_insert(now);
            reasons.extend(wanted_for.iter().copied());
        }
        // Interface work is ready before exhaustive unit planning. A single
        // worker keeps only the latest game request and stops obsolete reads.
        // Title menus can prewarm too; disabling preload starts no planner.
        if let Some(planner) = planner.as_mut() {
            let key = Some((folders.clone(), match shown {
                Shown::Map(world, _) => Some(crate::preload::game_key(world)),
                _ => None,
            }));
            if preload_for != key {
                preload_for = key;
                preload = crate::preload::Preload::none();
                let (world, view) = match shown {
                    Shown::Map(world, view) => (crate::preload::planning_world(world), *view),
                    _ => (Civ3World::default(), Civ3View::default()),
                };
                let stop = crate::art::ArtStop {
                    started: folders_changed.load(std::sync::atomic::Ordering::Acquire), now: folders_changed.clone(),
                };
                planner.request(world, view, art_list.clone(), compiler.files(&folders), stop, budget);
            }
            let [interface, scenario, units] = planner.take();
            let received = interface.is_some() || scenario.is_some() || units.is_some();
            if let Some(plan) = interface { preload = plan; }
            for phase in [scenario, units].into_iter().flatten() { preload.append(phase); }
            if received {
                // Reserve once per additional phase. The inbox keeps its
                // high-water record, including after requests are consumed.
                let page = 2048 * 2048;
                let used = atlas_used.max(sent_bytes);
                let ahead = preload.left().min(budget.saturating_sub(used));
                let room = used.saturating_add(ahead.saturating_mul(5) / 4).min(budget.max(used));
                let index_pages = u32::try_from(room / page).unwrap_or(u32::MAX).saturating_add(1);
                presenter.reserve(0, index_pages);
            }
        }
        // What art asked for now is for: the context of its render errors.
        let art_context = match screen_open {
            Some((_, name)) => format!("{name} screen"),
            None if in_game => "map".to_owned(),
            None => "menus".to_owned(),
        };
        if let Some(token) = schedule.start(now) {
            // Fonts no screen or zoom has asked for in a while are let go
            // of (their glyphs; the art keeps them).
            let dropped = fonts.prune(FONT_KEPT);
            if dropped > 0 {
                log::debug!("art: {dropped} fonts not used for {FONT_KEPT:?} let go of");
            }
            let wanted = art_wanted.unwrap_or(now);
            let reason = std::mem::take(&mut reasons).into_iter().collect::<Vec<_>>().join(", ");
            let work = if std::mem::take(&mut whole_wanted) {
                described_units = have.clone();
                described_screen = screen_art.clone();
                described_districts = district_art.clone();
                Some(Work::Whole(compiler.whole_text(&have, &mut fonts, &screen_art, &district_art)))
            } else {
                addition(&have, &mut described_units, &screen_art, &mut described_screen, &district_art, &mut described_districts, &mut fonts).map(Work::Addition)
            };
            match work {
                // Nothing new after all (e.g. a font that had been let go of).
                None => schedule.done(now, token),
                Some(work) => {
                    let (text, append) = match &work {
                        Work::Whole(t) => (t, false),
                        Work::Addition(t) => (t, true),
                    };
                    // A renderer that compiles its own copy starts at once.
                    let generation = presenter.prepare_assets(&scene::wire::Assets { context: art_context.clone(), ..compiler.wire(&folders, text, append) });
                    if !append {
                        whole_folders.insert(generation, folders.clone());
                    }
                    let stop = matches!(work, Work::Whole(_)).then(|| crate::art::ArtStop { started: folders_changed.load(std::sync::atomic::Ordering::Acquire), now: folders_changed.clone() });
                    if !art_thread.send(crate::art::ArtJob { stop, context: art_context.clone(), ..compiler.job(&folders, work, pixels, generation, Some(token), reason, wanted) }) {
                        whole_folders.remove(&generation);
                        log::error!("the art thread is gone");
                        render_errors::report(render_errors::Kind::ArtHandOver, "the art thread is gone", format_args!("art {generation}"));
                        schedule.done(now, token);
                    }
                }
            }
        }
        mark(&mut profile, "art needs", &mut t);
        // Art made ahead, while the art thread has nothing else to do (art
        // wanted now goes first).
        // (A renderer compiling its own art has the last addition first:
        // what it is sent queues behind what it has not compiled yet.)
        if preload_ready && art_thread.idle() && pending_growths.is_empty() && !schedule.pending() && !whole_wanted {
            let have_ref = &have;
            let wanted = |item: &crate::preload::Item| match item {
                crate::preload::Item::Unit(f, colors) => have_ref.files.get(&f.path).is_some_and(|(_, c)| colors.is_subset(c)) && !have_ref.stills.contains_key(&f.path),
                crate::preload::Item::Screen(art) => screen_art.covers(art),
                crate::preload::Item::Font(style, color, backdrop) => fonts.has_style(style, *color, *backdrop),
            };
            if let Some((tier, items)) = preload.next(atlas_used.max(sent_bytes), PRELOAD_CHUNK, wanted) {
                let files = compiler.files(&folders);
                for item in items {
                    sent_bytes += item.bytes();
                    match item {
                        crate::preload::Item::Unit(f, colors) => {
                            // (Made whole, every frame.)
                            have.stills.remove(&f.path);
                            have.files.entry(f.path.clone()).or_insert_with(|| (f, Default::default())).1.extend(colors);
                        }
                        crate::preload::Item::Screen(art) => {
                            screen_art.regions.extend(art.regions);
                            screen_art.resized_regions.extend(art.resized_regions);
                            screen_art.indexed.extend(art.indexed);
                            screen_art.fills.extend(art.fills);
                            screen_art.dynamic.extend(art.dynamic);
                        }
                        crate::preload::Item::Font(style, color, backdrop) => drop(fonts.add_style(&files, style, color, backdrop)),
                    }
                }
                if let Some(text) = addition(&have, &mut described_units, &screen_art, &mut described_screen, &district_art, &mut described_districts, &mut fonts) {
                    let generation = presenter.prepare_assets(&scene::wire::Assets { context: format!("preload ({tier})"), ..compiler.wire(&folders, &text, true) });
                    let job = compiler.job(&folders, Work::Addition(text), pixels, generation, None, format!("preload ({tier})"), now);
                    if !art_thread.send(crate::art::ArtJob { context: job.reason.clone(), ..job }) {
                        log::error!("the art thread is gone");
                        render_errors::report(render_errors::Kind::ArtHandOver, "the art thread is gone", format_args!("art {generation}"));
                    }
                }
            }
        }
        // Asset readiness drives retries and diagnostics, not UI publication.
        let size = input.game_window().and_then(|w| w.client_size()).unwrap_or(input.viewport);
        let visible = mandatory.clone().filter(|i| !absent_art.covers(i));
        match ui_screen::publication(visible.clone(), &fonts, &compiled, schedule.screen_pending(), size) {
            ui_screen::Publication::Partial { pending } => {
                if partial_since.is_none() {
                    partial_since = Some(std::time::Instant::now());
                    partial_reported = false;
                    log::debug!("scene: shown while visible art loads");
                }
                retry_wanted = !pending && art_retry.would_ask(&ui_screen::unready_art(visible.clone(), &compiled));
                if !pending && !retry_wanted && !partial_reported {
                    partial_reported = true;
                    log::warn!("scene: visible art did not arrive; drawn as nothing");
                    let missing = ui_screen::missing_art(visible, &fonts, &compiled);
                    for missing in &missing { log::warn!("scene: not resident: {missing}"); }
                    render_errors::report(render_errors::Kind::VisibleArtMissing, &art_context, format_args!("drawn as nothing: {}", missing.iter().take(4).map(|m| m.to_string()).collect::<Vec<_>>().join("; ")));
                }
            }
            ui_screen::Publication::Complete => {
                retry_wanted = false;
                partial_reported = false;
                if let Some(since) = partial_since.take() {
                    log::info!("scene: all visible art arrived {} ms after the screen showed without it", since.elapsed().as_millis());
                }
            }
        }
        // Capture commits complete UI command groups at Present. Missing art
        // omits only that sprite; it never delays publishing the other commands.
        new_art = false;
        seq += 1;
        // How much smaller a previewed world is shown: about the size of the
        // window beside the screen.
        let preview_scale = match shown {
            Shown::Preview(rect, Some(p)) => crate::preview::downscale(p.area, (input.viewport.0 as i32 - rect[2], input.viewport.1 as i32)),
            _ => 1,
        };
        // Our own title menu (when the game's is not mirrored).
        let mut title_menu = Vec::new();
        let mut scene = match shown {
            Shown::Map(world, view) => {
                // (Only what units do changes most of the time.)
                let mut scene = cells_memo.get((still.clone(), *view, art_generation), || mapper.scene(world, view, (0, 0), 0));
                // World labels replace mirrored underlays, including city-site
                // lines. Draw those from the current mouse/selection state.
                if ui.is_none() || world_labels() {
                    scene.sprites = city_site::sprites(world, &compiled);
                }
                let accessibility = &prefs.settings().accessibility;
                let rings = compiler.ring_override.clone().unwrap_or_else(|| crate::rings::Rings::configured(accessibility, None));
                // Units the game slides to the next tile: where their slides
                // have them, the renderer moving them on (`crate::unit_slides`).
                let sliding = crate::unit_slides::shown(std::time::Instant::now(), world.units.iter().map(|u| (u.id, (u.x, u.y))));
                units::place_sliding(world, view, &mut art, &compiled, rings.as_ref(), &mut unit_memory, &mut scene, accessibility, &sliding, units::selected(world));
                crate::effects::place(world, view, &mut art, &compiled, &mut scene);
                // The mirrored interface has the city icons already.
                if ui.is_none() {
                    scene.sprites.extend(city_hud::sprites(world, &compiled));
                }
                // Choosing a target (§10.15): ours, whether or not the game
                // draws its map.
                scene.sprites.extend(crate::targeting::grid_sprites(world, &compiled, view.zoomed_out));
                scene.sprites.extend(crate::targeting::range_sprites(world, &compiled, view.zoomed_out));
                scene.sprites.extend(crate::targeting::sprites(world, view.zoomed_out, &mut art, &compiled));
                // The point of interest: the active unit.
                // (In the game view's pixels: halved zoomed out; the sprites
                // stay at full size, the renderer halves them.)
                scene.focus = units::focus(world);
                scene.focus_motion = units::focus_motion(world, &sliding);
                if view.zoomed_out {
                    scene.focus = scene.focus.map(|(x, y)| (x / 2, y / 2));
                    scene.focus_motion = scene.focus_motion.map(|m| scene::FocusMotion { from: (m.from.0 / 2, m.from.1 / 2), ..m });
                }
                scene
            }
            // (Made again only when the preview, its stage or the art
            // changes: a new highlight in the menu goes out at once.)
            Shown::Preview(_, Some(p)) => {
                let key = (p.number, Arc::as_ptr(&p.world) as usize, p.drawn.labels.then(|| Arc::as_ptr(&preview_labels) as usize), p.drawn, preview_scale, art_generation, fonts.revision());
                let shown_before = preview_memo.last.as_ref().filter(|(k, _)| menu_changed && *k != key).map(|(_, scene)| scene.clone());
                preview_deferred = shown_before.is_some();
                let mut scene = shown_before.unwrap_or_else(|| preview_memo.get(key, || {
                    let none = crate::preview::LabelItems::new();
                    let labels = if p.drawn.labels { &*preview_labels } else { &none };
                    crate::preview::scene_with(&p.world, preview_scale, &mapper, p.drawn.units.then_some(&mut art), &compiled, labels, &fonts)
                }));
                // While its art is still coming: the game's loading cursor
                // over it (under the menus).
                if p.loading {
                    scene.sprites.extend(crate::preview::loading_indicator(p.area, &compiled));
                }
                scene
            }
            Shown::Preview(..) => scene::Scene::empty(scene::WorldShape::default(), compiled.layers.len()),
            Shown::Title(state) => {
                // Our own title menu, unless the game's is mirrored.
                if let Some(m) = menus.as_ref().and_then(|ms| ms.menu("title").map(|m| (ms, m))).filter(|_| ui.is_none()) {
                    title_menu = m.0.sprites(m.1, state, input.viewport, u32::MAX);
                }
                scene::Scene::empty(scene::WorldShape::default(), compiled.layers.len())
            }
        };
        if matches!(shown, Shown::Map(..)) {
            scene.camera_focus = camera_focus;
            scene.camera_arrival = camera_arrival;
            scene.map_wheel_step = civ3_sys::native_camera::map_wheel_step(game).unwrap_or_default();
            scene.minimap = minimap;
            // The tiny native tile range has no authority over renderer art.
            if scene.camera_focus.is_some() {
                scene.view_mut(crate::scene_map::GAME_VIEW).view.cull = None;
            }
        }
        mark(&mut profile, "scene map+units", &mut t);
        scene.sprites.extend(fonts.sprites(texts, &compiled));
        // Labels and the interface's text also drawn from outlines at
        // every zoom (where the art has the face).
        scene.zooms = ui_screen::outline_zooms(&compiled);
        let outlines = !scene.zooms.is_empty();
        // (The art, its fonts and the items give the sprites.)
        let art_key = (art_generation, fonts.revision(), outlines);
        scene.sprites.extend(ui_sprites_memo.get((ui_items.clone(), art_key), || {
            let mut sprites = ui_screen::sprites(&ui_items, &fonts, &compiled, ui_screen::under_pass(&compiled), outlines);
            for sprite in sprites.iter_mut().filter(|s| s.after_pass != u32::MAX) {
                sprite.category = viewer_settings::clean_map::CITIES;
            }
            sprites
        }));
        let mut menu_sprites = title_menu;
        menu_sprites.extend(ui_screen::sprites(&menu_items, &fonts, &compiled, ui_screen::under_pass(&compiled), outlines));
        // Each world camera chooses the label layout for its own scale.
        // Preview labels are already part of their separately built scene.
        let label_items = if matches!(shown, Shown::Preview(..)) { WorldItems::default() } else { label_items };
        let labels = label_sprites_memo
            .get((label_items.clone(), art_key), || label_items.sprites(&fonts, &compiled, ui_screen::under_pass(&compiled), outlines))
            .into_iter()
            .map(|mut s| {
                s.not_in_view = labels_not_in_game_view.then_some(crate::scene_map::GAME_VIEW);
                s.category |= viewer_settings::clean_map::CITIES;
                s
            });
        scene.sprites.extend(labels);
        let under_units = units::unit_passes(&compiled).1;
        scene.sprites.extend(target_items.sprites(&fonts, &compiled, under_units, outlines));
        let mut stacks = stack_items.sprites(&fonts, &compiled, units::unit_passes(&compiled).0, outlines);
        for sprite in &mut stacks { sprite.category |= viewer_settings::clean_map::UNITS; }
        scene.sprites.extend(stacks);
        mark(&mut profile, "ui+label sprites", &mut t);
        // Pictures the game makes itself: the minimap (§17).
        if let Shown::Map(world, _) = shown {
            let both: Vec<ui_screen::Item> = ui_items.iter().chain(&screen_items).cloned().collect();
            // The selected source has its own key: toggling Geography or
            // Territory replaces pixels even when world, UI and art stay put.
            let minimaps: Vec<_> = both.iter().filter(|i| matches!(i, ui_screen::Item::Element(menu::screen::Element::Dynamic { name, .. }, _) if name.starts_with("minimap"))).cloned().collect();
            scene.patches = minimap_memo.get((minimaps, still.clone(), world.navigator_modes, art_generation), || {
                ui_screen::minimap_patches(&both, world, &compiled, |table| *table_colors.entry(table).or_insert_with(|| art.table_color(table, 66).unwrap_or_default()))
            });
        }
        mark(&mut profile, "minimap", &mut t);
        // Replay pixels made by our renderer from terrain and history changes.
        let all = ui_items.iter().chain(&menu_items).chain(&screen_items).chain(&popup_items);
        scene.patches.extend(ui_screen::surface_patches(all.clone(), &replay_surface, &compiled));
        // The Load Scenario screen's map of the highlighted scenario (§19.1),
        // made from the previewed world at each size the screen shows it.
        if let Shown::Preview(_, Some(p)) = &shown {
            let scenario_map = civ3_sys::ui_hooks::SCENARIO_MAP;
            let mut sizes: Vec<(u32, u32)> = all
                .clone()
                .filter_map(|i| match i {
                    ui_screen::Item::Element(menu::screen::Element::Dynamic { name, size, .. }, _) if name == scenario_map => Some(*size),
                    _ => None,
                })
                .collect();
            sizes.dedup();
            let surfaces = scenario_map_memo.get((p.number, sizes.clone()), || {
                sizes.iter().map(|&size| (scenario_map, size, crate::minimap::scenario_map(&p.world, size))).collect::<Vec<_>>()
            });
            scene.patches.extend(ui_screen::surface_patches(all, &surfaces, &compiled));
        }
        crate::scene_map::overlay_screen_sprites(&mut scene);
        // Add transient world art after screen sprites have been partitioned,
        // so its index ranges remain valid through the rest of publication.
        for (items, &(serial, duration_ms)) in notice_layouts.iter().zip(&notice_state.2) {
            let start = scene.sprites.len() as u32;
            scene.sprites.extend(crate::unit_notices::sprites(items, &fonts, &compiled, outlines));
            scene.sprite_lifetimes.push(scene::SpriteLifetime { serial, sprites: (start, scene.sprites.len() as u32), duration_ms });
        }

        crate::scene_map::overlay_menus(&mut scene, menu_sprites);
        // The parts of the interface the viewer arranges, in views of their
        // own (the corner's buttons a copy, the popup not in the interface).
        let [corner_memo, hint_memo, screen_part_memo, popup_part_memo] = &mut part_memos;
        let part_sprites = |memo: &mut Memo<_, _>, items: &[ui_screen::Item]| {
            memo.get((items.to_vec(), art_key), || ui_screen::sprites(items, &fonts, &compiled, ui_screen::under_pass(&compiled), outlines))
        };
        if let Some(r) = corner {
            crate::interface_parts::part_view(&mut scene, crate::interface_parts::CORNER_VIEW, r, part_sprites(corner_memo, &corner_items));
        }
        // The hint how to let a captured mouse go, in the corner buttons'
        // place: also while a screen hides them, its world beside it (the
        // mouse can be captured there to play on, `crate::modal`).
        let map_hint = crate::interface_parts::capture_hint_shown(corner.is_some(), matches!(shown, Shown::Map(..)), screen_open.is_some());
        if map_hint {
            crate::interface_parts::part_view(&mut scene, crate::interface_parts::HINT_VIEW, hint.0, part_sprites(hint_memo, &hint.1));
        }
        // The hint for exploring a preview world with the mouse captured
        // (the viewer puts it at the world's corner).
        let preview_hint = matches!(shown, Shown::Preview(_, Some(_)));
        if preview_hint {
            crate::interface_parts::part_view(&mut scene, crate::interface_parts::HINT_VIEW, hint.0, part_sprites(hint_memo, &hint.1));
        }
        if let Some((_, r)) = screen_part {
            crate::interface_parts::part_view(&mut scene, crate::interface_parts::SCREEN_VIEW, r, part_sprites(screen_part_memo, &screen_items));
        }
        for (id, rect, items) in &popup_layers {
            crate::interface_parts::part_view(&mut scene, *id, *rect, part_sprites(popup_part_memo, items));
        }
        // The tile boxes the viewer keeps (`crate::detached`), over the map
        // only: a screen opening over it closes them.
        if !on_map {
            crate::detached::close_all();
        }
        let detached = crate::detached::kept();
        for ((index, kept), memo) in detached.iter().enumerate().zip(detached_memos.iter_mut()) {
            let sprites = memo.get((kept.serial, art_key), || {
                ui_screen::sprites(&ui_screen::items(&kept.frame, files.roots(), generated, unit_colors), &fonts, &compiled, ui_screen::under_pass(&compiled), outlines)
            });
            crate::interface_parts::part_view(&mut scene, crate::detached::entry(index, kept).view, kept.rect, sprites);
        }
        // The reports the game did not wait for, the oldest on top.
        for (index, ((q, items), memo)) in queued.iter().zip(&queued_items).zip(queued_memos.iter_mut()).enumerate() {
            let sprites = memo.get((q.serial, art_key), || ui_screen::sprites(items, &fonts, &compiled, ui_screen::under_pass(&compiled), outlines));
            crate::interface_parts::part_view(&mut scene, crate::async_popups::view(index), q.rect, sprites);
        }
        if let Some((_, rect)) = &loading_bar {
            let sprites = bar_sprites_memo.get((bar_items.clone(), art_key), || ui_screen::sprites(&bar_items, &fonts, &compiled, ui_screen::under_pass(&compiled), outlines));
            crate::interface_parts::part_view(&mut scene, crate::loading_cursor::VIEW, *rect, sprites);
        }
        if popup.is_some() && on_map {
            // The popup's subject: older drivers' games bring it into view
            // first (the middle of the game's view stands for it). With the
            // camera parked that middle is the map's corner, and the game
            // asks for any real move itself: the focus stays.
            let middle = scene.view(crate::scene_map::GAME_VIEW).map(|v| v.view).map(|v| {
                let (w, h) = if v.viewport.0 == 0 || v.viewport.1 == 0 { input.viewport } else { v.viewport };
                (v.camera_x + w as i32 / 2, v.camera_y + h as i32 / 2)
            });
            scene.focus = crate::interface_parts::popup_focus(scene.focus, scene.camera_focus, true, middle);
        }
        scene.interface = scene::Interface {
            screen: screen_open.map(|s| s.1.to_owned()),
            screen_role: screen_open.map_or(scene::ScreenRole::Other, |s| civ3_ui::screens::role(s.1)),
            menu_fit: prefs.screen_rect().is_some() || own_title.is_some() || native_title.is_some(),
            roaming: corner.map(|_| crate::interface_parts::CORNER_VIEW),
            capture_hint: (map_hint || preview_hint).then_some(crate::interface_parts::HINT_VIEW),
            popup: popup_view,
            modal_stack: scene::ModalStack(std::array::from_fn(|index| popup_layers.get(index).map(|(id, _, _)| *id))),
            modal_attached: std::array::from_fn(|index| popup_frames.get(index).is_some_and(|part| part.attached)),
            screen_view: screen_part.map(|_| crate::interface_parts::SCREEN_VIEW),
            // Where the pointer goes when the screen opens: the city
            // screen's production box (its largest own button).
            focus_control: match (screen_open, ui, civ3_sys::ui_hooks::city_screen_button_vtable(game.build)) {
                (Some((_, "City")), Some(f), Some(vt)) => {
                    f.forms.iter().filter(|form| form.0 == vt && form.3.0 > 0 && form.3.1 > 0).max_by_key(|form| form.3.0 * form.3.1).map(|form| [form.2.0, form.2.1, form.3.0, form.3.1])
                }
                _ => None,
            },
            reachable,
            // The tooltip over the map's own screen (its panels and the
            // unit's orders), which the viewer arranges nothing around.
            tooltip: ui.as_ref().filter(|_| on_map).and_then(crate::interface_parts::tooltip),
            // The Load Scenario screen at the window's left, its scenario's
            // world beside it.
            beside: match shown {
                Shown::Preview(rect, p) => Some(scene::Beside { screen: *rect, edge: scene::Edge::Left, world: p.as_ref().map(|p| p.area) }),
                _ => None,
            },
            popup_anchor,
            popup_on_screen: popup_on_screen && popup.is_some(),
            detached: detached.iter().enumerate().map(|(index, kept)| crate::detached::entry(index, kept)).collect(),
            queued: queued.iter().enumerate().map(|(index, q)| crate::async_popups::entry(index, q)).collect(),
            queued_more,
            loading_bar: loading_bar.as_ref().map(|_| crate::loading_cursor::VIEW),
            // A unit command waits for its tile (`crate::targeting`):
            // the game's own mode, whatever gave the command.
            targeting: matches!(shown, Shown::Map(world, _) if screen_open.is_none() && popup.is_none() && crate::targeting::active(world.targeting.mode)),
            // Go-to and the stack moves only: the cursor tile by tile.
            snap_to_tiles: matches!(shown, Shown::Map(world, _) if crate::targeting::go_to(world.targeting.mode)),
            extensions: Vec::new(),
            // The game menu or the popup Escape opens on the map: the
            // viewer lets the pointer go entirely while either is open.
            releases_pointer: {
                let screen_class = screen_open.and_then(|(i, _)| ui.as_ref()?.forms.get(i).map(|f| f.0));
                let modal_popup = modal.and_then(|m| {
                    let class = inproc::Ptr32::<u32>::new(m).read().ok()?;
                    Some((class, (class == crate::async_popups::POPUP_VTABLE).then(|| crate::async_popups::popup_text_key(m)).flatten()))
                });
                crate::interface_parts::releases_pointer(game.build, screen_class, modal_popup.as_ref().map(|(c, k)| (*c, k.as_deref())))
            },
        };
        if let (Shown::Map(_, view), Some(([x, y, w, h], _))) = (shown, city) {
            let camera = city_anchor.map_or((view.camera_x, view.camera_y), |a| a.camera);
            crate::scene_map::city_view(&mut scene, camera, [x, y, w, h]);
        }
        // The renderer's settings go with every scene (sent when they change).
        scene.settings = Some(prefs.settings().clone());
        scene.settings_preview = prefs.settings_preview();
        scene.interactables = interactables.clone();
        // The city screen's build queue, reordered by dragging.
        if let Some((queue, grips)) = crate::build_queue::publish(&civ3_sys::controls::Live, &scene.interactables, scene.interface.focus_control) {
            scene.interactables.extend(grips);
            scene.interface.extensions.push(queue.extension());
        }
        if let (Shown::Map(world, _), Some(frame), Some(anchor)) = (shown, ui.as_ref(), city_anchor) {
            let eligible = crate::city_screen::native::eligibility(&game, world);
            scene.interactables.extend(crate::city_screen::tile_targets(world, frame, anchor,
                modal.map(|m| m + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4), eligible.as_ref().map(|e| e.tiles())));
        }
        (scene.cursor, scene.cursor_at) = *cursor;
        // The game's screen: a viewer with a larger window shows it scaled
        // up (e.g. the game at the desktop's logical size, `CIV3R_SCALE`).
        scene.screen_size = input.game_window().and_then(|w| w.client_size()).unwrap_or_default();
        // Where the interface's views draw the controls, and how they look
        // in their other states there (the corner's copy clipped to it).
        let looks_views = [
            (crate::scene_map::INTERFACE_VIEW, None),
            (crate::scene_map::MENU_VIEW, None),
            (crate::interface_parts::CORNER_VIEW, corner),
            (crate::interface_parts::SCREEN_VIEW, None),
        ].into_iter().chain(popup_layers.iter().map(|(id, _, _)| (*id, None))).collect::<Vec<_>>();
        scene.looks = crate::looks::scene_looks(&scene, &control_looks, &looks_views, |d, part| Some(ui_screen::sprites(&looks_items(&crate::looks::in_part(d, part)), &fonts, &compiled, ui_screen::under_pass(&compiled), outlines)));
        // Our preferences tabs' and title menu's controls.
        let prefs_looks: Vec<crate::looks::ControlLooks<Vec<menu::panel::Draw>>> = prefs_looks
            .into_iter()
            .map(|l| crate::looks::ControlLooks { id: l.id, select: l.select, shown: l.shown, draws: l.draws, states: l.states })
            .collect();
        scene.looks.extend(crate::looks::scene_looks(&scene, &prefs_looks, &looks_views, |d, _| Some(ui_screen::sprites(&crate::prefs::items(d), &fonts, &compiled, ui_screen::under_pass(&compiled), outlines))));
        if let (Some((ms, m)), Shown::Title(state)) = (own_title, shown) {
            let title_looks: Vec<crate::looks::ControlLooks<Vec<scene::FreeSprite>>> = ms
                .looks(m, state, input.viewport, u32::MAX)
                .into_iter()
                .map(|l| crate::looks::ControlLooks { id: l.id, select: scene::Select::None, shown: l.shown, draws: l.sprites, states: l.states })
                .collect();
            scene.looks.extend(crate::looks::scene_looks(&scene, &title_looks, &looks_views, |d, _| Some(d.clone())));
        }
        let looks_count = (arts.len(), control_looks.len(), scene.looks.len(), scene.looks.iter().map(|l| l.views.len()).sum::<usize>());
        if looks_count != last_looks {
            log::debug!("looks: {} buttons drawn by state, {} with other states, {} found in the scene ({} views)", looks_count.0, looks_count.1, looks_count.2, looks_count.3);
            last_looks = looks_count;
        }
        if looks_log() {
            let short = |s: &scene::LookState| format!("{:?}{}", s.pointer, if s.selected { "+sel" } else { "" });
            let detail: Vec<String> = scene
                .looks
                .iter()
                .map(|l| {
                    let views: Vec<String> = l.views.iter().map(|v| format!("v{}@{}..{}:[{}]", v.view, v.sprites.0, v.sprites.1, v.states.iter().map(|(s, d)| format!("{}={}", short(s), d.len())).collect::<Vec<_>>().join(","))).collect();
                    format!("{:#x} {} {}", l.id, short(&l.shown), views.join(" "))
                })
                .collect();
            let missing: Vec<String> = control_looks.iter().filter(|c| !scene.looks.iter().any(|l| l.id == c.id)).map(|c| format!("{:#x}", c.id)).collect();
            log::debug!("looks detail: {}; not found in the scene: {}", detail.join("; "), missing.join(" "));
        }
        scene.seq = seq;
        scene_size = (scene.cell_bytes(), scene.sprites.len(), scene.views.iter().map(|v| v.overlay.len()).sum::<usize>());
        mark(&mut profile, "interface parts", &mut t);
        if let Some(p) = &mut profile {
            p.count("scene sprites", scene_size.1);
            p.count("scene overlay sprites", scene_size.2);
        }
        // (Read while the game quit: never shown.)
        if crate::session_end::SESSION.ending() {
            continue;
        }
        // Held back while the renderer is busy: publish again next time.
        let published = presenter.publish(scene);
        if let Some(p) = &mut profile {
            let s = presenter.stats;
            p.add("publish: diff", s.diff);
            p.add("publish: encode+send", s.send);
            p.add("publish: keep sent", s.keep);
            if s.updates > 0 {
                p.count("published bytes", s.bytes);
                p.count("published updates", s.updates);
                p.count("published cell changes", s.cells);
            }
            p.event(if published { if s.updates > 0 { "published" } else { "nothing new" } } else { "renderer busy" }, 1);
        }
        mark(&mut profile, "publish (other)", &mut t);
        if published {
            if let Shown::Preview(rect, _) = shown {
                complete_preview_menu = Some(*rect);
                previews.menu_published();
                if let Some((at, before, frame)) = &preview_input {
                    if selection_before_ui > *before && ui != frame {
                        log::info!("scenario preview: input to menu with a new selection published in {:.1} ms (scene {seq})", at.elapsed().as_secs_f64() * 1e3);
                        preview_input = None;
                    } else if at.elapsed() > Duration::from_secs(2) {
                        preview_input = None;
                    }
                }
            }
            if matches!(shown, Shown::Preview(_, Some(_))) && !preview_deferred {
                previews.published(seq);
            }
            published_prefs = (prefs_draws, Some(prefs.settings().clone()), prefs.settings_preview());
            last_replay_revision = replay_revision;
        }
        last_camera_focus = published.then_some(camera_focus).flatten();
        last_camera_arrival = published.then_some(camera_arrival).flatten();
        last = published.then_some(state);
        // (Held back: again soon, the renderer is about to take the last.)
        wait = if published { POLL } else { BUSY_RETRY };
        // (The world held back for the menu: next, at once.)
        if preview_deferred {
            wait = Duration::ZERO;
        }
    }
}

/// The description entries of what is wanted (`have`, the screen art,
/// the districts, the fonts loaded) that the art does not have yet (the
/// `described_*`, the fonts' own), as an addition; from now on the art has
/// them. `None`: nothing new.
fn addition(
    have: &units::Needed,
    described_units: &mut units::Needed,
    screen_art: &menu::screen::ScreenArt,
    described_screen: &mut menu::screen::ScreenArt,
    district_art: &crate::district_art::DistrictArt,
    described_districts: &mut crate::district_art::DistrictArt,
    fonts: &mut text::Fonts,
) -> Option<String> {
    let units = have.minus(described_units);
    described_units.merge(&units);
    let screen = screen_art.minus(described_screen);
    *described_screen = screen_art.clone();
    let districts = district_art.minus(described_districts);
    *described_districts = district_art.clone();
    let new_fonts = fonts.has_new();
    let mut text = if new_fonts { fonts.addition() } else { String::new() };
    if !units.is_empty() {
        text += &units.addition();
    }
    if !screen.is_empty() {
        text += &ui_screen::description(&screen);
    }
    if !districts.is_empty() {
        text += &districts.description();
    }
    (!units.is_empty() || !screen.is_empty() || !districts.is_empty() || new_fonts).then_some(text)
}

/// `CIV3R_TEXT_TRACE=1`: distinguish text missing from the mirror from
/// text whose font or atlas art is missing. No pixels are read from the game.
fn trace_text<'a>(screen: &str, frame: Option<&crate::ui::Frame>, items: impl Iterator<Item = &'a ui_screen::Item>, fonts: &text::Fonts, compiled: &Compiled, last: &mut String) {
    use std::fmt::Write as _;
    let runs: Vec<_> = items.filter_map(|i| if let ui_screen::Item::Text(run, clip, _) = i { Some((run, clip)) } else { None }).collect();
    let raw = frame.map_or(0, |f| f.draws.iter().filter(|d| matches!(d, crate::ui::ScreenDraw::Text { .. })).count());
    let forms: Vec<_> = frame.into_iter().flat_map(|f| f.forms.iter().map(|f| (f.0, f.1))).collect();
    let mut message = format!("text trace: {screen}, forms {forms:x?}, mirrored runs {raw}, visible runs {}, loaded fonts {}, outline faces {}\n", runs.len(), fonts.len(), compiled.fonts.len());
    // Keep the end too: title-menu labels follow the title's other text.
    for (_, (run, clip)) in runs.iter().enumerate().filter(|(i, _)| *i < 64 || i.saturating_add(64) >= runs.len()) {
        let mut sprites = Vec::new();
        fonts.run_sprites(run, Some(**clip), compiled, u32::MAX, &mut sprites);
        let raster = sprites.iter().filter(|s| s.glyph.is_none()).count();
        let fallback = sprites.len() - raster;
        sprites.clear();
        fonts.run_glyphs(run, Some(**clip), compiled, u32::MAX, &mut sprites);
        let sample: String = run.text.chars().take(80).collect();
        let _ = writeln!(message, "  {:?}/{} weight {} italic {} color {:?} at ({},{}) clip {:?}: loaded {}, face {}, raster {raster}, fallback {fallback}, zoom glyphs {}, text {sample:?}", run.face, run.height, run.weight, run.italic, run.color, run.x, run.y, clip, fonts.has(run), compiled.font(&run.face).is_some(), sprites.len());
    }
    if *last != message {
        log::debug!("{message}");
        *last = message;
    }
}

/// The head of an art list (`CIV3R_ART_LIST`, `crate::preload::ArtList`).
fn art_list_header() -> &'static str {
    "# Art the game's interface showed (civ3-bridge, CIV3R_ART_LIST): screen regions, fills,\n# run-time pictures, fonts; tab-separated (menu::screen::ScreenArt::to_lines, preload::font_line).\n"

}

/// Whether the viewer's pointer moves and buttons are kept from the game:
/// while the file `CIV3R_HOLD_POINTER` names exists (for testing the looks
/// the viewer draws itself: the game then never shows them).
fn hold_pointer() -> bool {
    static FILE: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    FILE.get_or_init(|| std::env::var_os("CIV3R_HOLD_POINTER").map(Into::into)).as_ref().is_some_and(|f| f.exists())
}

/// Whether every scene's looks are logged in detail (`CIV3R_LOOKS_LOG=1`).
fn looks_log() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("CIV3R_LOOKS_LOG").as_deref() == Ok("1"))
}

/// The last result of a function of a key, kept while the key stays the
/// same (made again otherwise).
struct Memo<K, V> {
    last: Option<(K, V)>,
}

impl<K, V> Default for Memo<K, V> {
    fn default() -> Self {
        Self { last: None }
    }
}

impl<K: PartialEq, V: Clone> Memo<K, V> {
    fn get(&mut self, key: K, make: impl FnOnce() -> V) -> V {
        match &self.last {
            Some((k, v)) if *k == key => v.clone(),
            _ => {
                let v = make();
                self.last = Some((key, v.clone()));
                v
            }
        }
    }
}

/// `world` without what changes as units move and animate (where they
/// are, what is selected, the pointer's tile), which the cells, the city
/// labels and the minimap do not depend on: what their memos key on.
/// Navigator modes are keyed separately by the minimap alone.
fn still_world(world: &Civ3World) -> Civ3World {
    Civ3World { navigator_modes: Default::default(), units: Vec::new(), army_members: Vec::new(), mouse_tile: None, current_unit: None, selection: None, unit_tiles: Vec::new(), effects: Vec::new(), targeting: Default::default(), goto: None, ..world.clone() }
}

/// Adds the time since `t` to span `name` of the profile, if one is kept,
/// and restarts `t`.
fn mark(profile: &mut Option<tile_viewer::profile::Profile>, name: &str, t: &mut std::time::Instant) {
    let now = std::time::Instant::now();
    if let Some(p) = profile {
        p.add(name, now - *t);
    }
    *t = now;
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
    _world_input: Option<inproc::message::Interceptor>,
    held: inproc::window::Buttons,
    clicks: inproc::window::DoubleClicks,
    /// A world pixel the viewer asked to see (`Input::Look`), not yet acted on.
    look: Option<(i32, i32)>,
    /// What the viewer shows of the game's view where that is not all of
    /// it (`Input::MainView`).
    main_view: Option<(i32, i32, i32, i32)>,
    /// The world downscale `main_view` is in (the viewer's frame's).
    main_view_downscale: u32,
    /// World rectangles of the viewer's own views (`Input::Views`).
    views: Vec<(i32, i32, i32, i32)>,
    /// The world downscale `views` are in (the viewer's frame's).
    views_downscale: u32,
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
    /// Where the viewer last moved the game's pointer (client pixels).
    pointer: Option<(i32, i32)>,
    /// The open city screen's map as shown: the pointer over it moves to
    /// where the game has those tiles (`crate::city_screen::Anchor::to_game`).
    city_map: Option<crate::city_screen::Anchor>,
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
            self.window = inproc::window::Window::all_of_this_process()
                .into_iter()
                // Not ours, and a real window (titled, with an area).
                // (Hidden ones too: it is kept hidden, `crate::game_window`.)
                .filter(|w| {
                    let t = w.title();
                    !t.is_empty() && t != VIEWER_TITLE && w.client_size().is_some_and(|(w, h)| w >= 320 && h >= 200)
                })
                .max_by_key(|w| w.client_size().map_or(0, |(w, h)| w as u64 * h as u64));
            if let Some(w) = self.window {
                log::info!("forwarding input to {:?} ({:?})", w.title(), w.client_size());
                match crate::world_input::attach_window(w) {
                    Ok(hook) => self._world_input = Some(hook),
                    Err(e) => log::warn!("world input message dispatch unavailable: {e}"),
                }
                if self.viewport == (0, 0) {
                    self.viewport = w.client_size().unwrap_or((1024, 768));
                }
            }
        }
        self.window
    }
}

/// Hiding the game's window (see `publish`).
#[derive(Default)]
struct Hiding {
    /// How often it was found shown and hidden again.
    times: u32,
    /// Hidden but not painted whole since: once it is, all of it is
    /// painted (hidden, the game paints only what it invalidates, see
    /// `inproc::paint`; at first, never shown, it was never painted).
    repaint: bool,
    /// Whether it was seen at all.
    seen: bool,
}

/// The publisher once the game quits (`crate::session_end`): it reads
/// nothing more of the game, tells the viewer to close and waits for it to
/// say it is gone, which the game's thread waits for (bounded) before it
/// ends the process. Never returns: the process ends around it.
fn end_session(presenter: &mut Presenter) -> ! {
    let session = &crate::session_end::SESSION;
    let since = || session.since_begin().unwrap_or_default().as_millis();
    let mut told = false;
    let mut waiting = true;
    loop {
        if !told && presenter.close() {
            told = true;
            log::info!("session: the viewer is told to close, {} ms after the game began to quit", since());
        }
        presenter.forward_log();
        if waiting && presenter.take_input().iter().any(|input| matches!(input, scene::Input::Quit)) {
            log::info!("session: the viewer is gone, {} ms after the game began to quit", since());
            session.viewer_closed();
            waiting = false;
        }
        // (The process ends meanwhile, once the game's thread is done
        // waiting; with the compatibility fallback the game's own exit
        // ends it.)
        std::thread::sleep(if waiting { Duration::from_millis(2) } else { Duration::from_millis(100) });
    }
}

/// Keeps the game's window hidden (`crate::game_window` keeps it from being
/// shown; this hides it should anything show it all the same) and painted.
fn hide_window(input: &mut InputForwarder, hiding: &mut Hiding) {
    let Some(window) = input.game_window() else { return };
    if !std::mem::replace(&mut hiding.seen, true) {
        hiding.repaint = true;
        // (Its keyboard input goes on: the game asks whether it has the focus.)
        inproc::pointer::keep_focus(Some(window));
        // (Its pointer away from the edges until the viewer moves it.)
        update_game_pointer(input, window);
    }
    if !window.is_visible() {
        if std::mem::take(&mut hiding.repaint) {
            inproc::paint::repaint(window);
        }
        return;
    }
    window.hide();
    hiding.repaint = true;
    hiding.times += 1;
    match hiding.times {
        1 => log::info!("the game's window was shown, hidden again (CIV3R_GAME_WINDOW=show keeps it)"),
        n => log::debug!("the game's window was shown again, hidden ({n} times)"),
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

fn forward_input(game: &Civ3, forwarder: &mut InputForwarder, mut inputs: Vec<scene::Input>) {
    use inproc::window::MouseButton;
    use scene::{Button, Input};
    // Receipts run while the game thread is parked before combat.
    // Queueing them to that thread would deadlock the barrier.
    inputs.retain(|input| match *input {
        Input::CameraArrived { serial } => { civ3_sys::camera_arrival::receipt(serial, true); false },
        Input::CameraUnavailable { serial } => { civ3_sys::camera_arrival::receipt(serial, false); false },
        _ => true,
    });
    if inputs.is_empty() {
        return;
    }
    // The game quits: nothing goes to it any more (`crate::session_end`).
    if crate::session_end::SESSION.ending() {
        if inputs.iter().any(|input| matches!(input, Input::Quit)) {
            crate::session_end::SESSION.viewer_closed();
        }
        return;
    }
    let Some(window) = forwarder.game_window() else {
        if inputs.iter().any(|input| matches!(input, Input::Quit)) {
            if !crate::ui_trace::compatibility_fallback() { crate::ui_trace::end_process(0); }
            crate::native_camera::viewer_gone(*game, forwarder.main_view);
            crate::ui_trace::restore_original_drawing();
            inproc::hidden::set_hiding(false);
            inproc::pointer::keep_focus(None);
            forwarder.world_buttons.clear();
            forwarder.look = None;
            forwarder.plain_keys.clear();
        }
        return;
    };
    let native_world_click = crate::world_input::ready(window) && !crate::modal::modal_open(game)
        && inputs.iter().any(|input| matches!(input, Input::WorldButton { .. }));
    for input in inputs {
        crate::ui_trace::audit_semantic(crate::ui_trace::SemanticClass::Input);
        if !matches!(input, Input::Slider { action: scene::SliderAction::Preview, .. }) {
            crate::native_sliders::barrier();
        }
        if !matches!(input, Input::WorldMove { .. } | Input::UnitOrder { action: scene::UnitOrderAction::Preview, .. }
            | Input::Views { .. } | Input::MainView { .. } | Input::Zooms(_) | Input::Edge { .. } | Input::Window { .. } | Input::Resized { .. }) {
            crate::world_input::barrier();
        }
        log::trace!("input {input:?}");
        // (For testing what the viewer draws itself: the pointer never
        // reaches the game.)
        if hold_pointer() && matches!(input, Input::MouseMove { .. } | Input::MouseButton { .. }) {
            continue;
        }
        match input {
            Input::MouseMove { x: ix, y: iy } => {
                let (x, y) = forwarder.city_map.map_or((ix, iy), |m| m.to_game((ix, iy)));
                forwarder.pointer = Some((x, y));
                crate::cursor::handled::posted((x, y), (ix, iy));
                window.mouse_move(x, y, forwarder.held);
            }
            Input::MouseButton { button, down, x, y } => {
                if down && forwarder.city_map.is_some() { crate::city_screen::native::invalidate(); }
                let (x, y) = forwarder.city_map.map_or((x, y), |m| m.to_game((x, y)));
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
            Input::Slider { id, action, value } => crate::native_sliders::post(window, id, action, value),
            Input::Wheel { delta, x, y } => {
                let (x, y) = forwarder.city_map.map_or((x, y), |m| m.to_game((x, y)));
                window.wheel(delta, x, y, forwarder.held)
            }
            // (The renderer's own; its `Wheel` notches are the game's.)
            Input::Scroll { .. } => {}
            Input::Key { vk, down, text } => {
                log::debug!("input: key {vk:#x} {}", if down { "down" } else { "up" });
                key(window, vk, down, text)
            }
            // A key over a view of the viewer's own: acted on (a few of
            // them) while a menu is open that allows it, else the key it is.
            // A release goes where its press went.
            Input::WorldKey { vk, down, text, x, y } => {
                // (Under a screen the map is played on too: G aims where
                // the pointer is.)
                if !crate::modal::modal_open(game) || crate::modal::world_input(game) {
                    crate::world_input::post_move(game, window, (x, y));
                    crate::world_input::barrier();
                }
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
            Input::WorldMove { x, y } => { crate::world_input::post_move(game, window, (x, y)); }
            Input::UnitOrder { action, x, y } => { crate::world_input::post_order(game, window, action, (x, y)); }
            Input::WorldButton { button, down, x, y } => {
                // (Under a screen too: the game's thread decides what may be
                // done there, and says so where not, `crate::world_input`.)
                if crate::world_input::ready(window) {
                    let native_button = match button {
                        Button::Left => { forwarder.held.left = down; MouseButton::Left }
                        Button::Right => { forwarder.held.right = down; MouseButton::Right }
                        Button::Middle => { forwarder.held.middle = down; MouseButton::Middle }
                    };
                    let double = down && forwarder.clicks.press(native_button, x, y) && window.wants_double_clicks();
                    crate::world_input::post_button(game, window, button, down, double, (x, y));
                } else if down {
                    forwarder.world_buttons.push((button, (x, y)));
                }
            }
            Input::Resized { width, height } => forwarder.viewport = (width, height),
            // (For the preferences' readout, `crate::prefs`.)
            Input::Window { .. } => {}
            // The viewer is gone (closed, or it failed to start): the
            // game's own window is shown, rather than nothing at all.
            Input::Quit => {
                if !crate::ui_trace::compatibility_fallback() { crate::ui_trace::end_process(0); }
                // The viewer may disappear before physical modifier releases
                // arrive. Retire them in the same owned window queue before
                // its native UI becomes the fallback input target.
                let _ = inproc::keyboard::release(window);
                crate::world_input::post_order(game, window, scene::UnitOrderAction::Cancel, (0, 0));
                forwarder.world_buttons.clear();
                forwarder.look = None;
                forwarder.plain_keys.clear();
                crate::native_camera::viewer_gone(*game, forwarder.main_view);
                crate::ui_trace::restore_original_drawing();
                window.wake();
                forwarder.edge = (0, 0);
                if crate::game_window::hidden() && inproc::hidden::hiding() {
                    inproc::hidden::set_hiding(false);
                    inproc::pointer::keep_focus(None);
                    window.show();
                    log::warn!("the viewer is gone: the game's own window is shown");
                }
            }
            Input::Edge { x, y } => forwarder.edge = (x, y),
            // On the game's thread, in order with the popup's own loop.
            Input::CloseDetached { serial } => crate::detached::close(serial),
            Input::CloseQueued { serial } => {
                log::info!("input: queued report {serial} read");
                crate::async_popups::close(serial);
            }
            Input::Extension { .. } => match tile_game::production_queue::QueueEdit::of(&input) {
                Some(edit) => crate::build_queue::post(window, edit),
                None => log::debug!("input: an extension message of no extension of ours"),
            },
            Input::DismissPopup => {
                if !crate::world_input::post_dismiss(game, window) {
                    log::debug!("input: dismissing the popup opened at a map click: world input not ready");
                }
            }
            Input::Look { x, y } if !native_world_click => forwarder.look = Some((x, y)),
            Input::Look { .. } => {}
            Input::Views { views, downscale } => {
                forwarder.views = views.iter().copied().filter(|v| v.2 > 0 && v.3 > 0).collect();
                forwarder.views_downscale = downscale;
            }
            Input::MainView { x, y, w, h, downscale } => {
                forwarder.main_view = (w > 0 && h > 0).then_some((x, y, w, h));
                forwarder.main_view_downscale = downscale;
                if crate::ui_trace::requested_game_draws() {
                    crate::native_camera::renderer_view(*game, forwarder.main_view);
                    window.wake();
                }
            },
            // (Text is drawn from outlines at every zoom: no art per zoom.)
            Input::Zooms(_) | Input::SettingsPreviewDecision { .. } | Input::CameraArrived { .. } | Input::CameraUnavailable { .. } => {}
        }
    }
    update_game_pointer(forwarder, window);
}

/// What the game learns of its pointer when it asks the system
/// (`inproc::pointer`): with the viewer's pointer at its edge, the game's at
/// its own, to scroll; with the game's window hidden, where the viewer last
/// moved it but clear of the edges (the system's pointer is over another
/// window: the game would take it as resting at an edge and scroll on its
/// own); else the system's.
fn update_game_pointer(forwarder: &mut InputForwarder, window: inproc::window::Window) {
    let Some(size) = window.client_size() else { return };
    let edge = crate::edge::game_point(forwarder.edge, size);
    let rest = || {
        let zone = civ3_sys::hooks::EDGE_SCROLL_ZONE;
        let (w, h) = (size.0 as i32, size.1 as i32);
        let (x, y) = forwarder.pointer.unwrap_or((w / 2, h / 2));
        (x.clamp(zone.min(w / 2), (w - 1 - zone).max(w / 2)), y.clamp(zone.min(h / 2), (h - 1 - zone).max(h / 2)))
    };
    let point = edge.or_else(|| (crate::game_window::hidden() && inproc::hidden::hiding()).then(rest));
    if edge != forwarder.edge_point {
        match edge {
            Some((x, y)) => log::debug!("edge scroll: the viewer's pointer at {:?}, the game's at ({x}, {y})", forwarder.edge),
            None => log::debug!("edge scroll: ends"),
        }
        forwarder.edge_point = edge;
    }
    inproc::pointer::set(point.map(|(x, y)| inproc::pointer::Override { window, x, y }));
}

/// Lets the game scroll at the viewer's edge (`crate::edge`): its
/// executable's `GetFocus` and `jgl.dll`'s `GetCursorPos` (loaded later)
/// answer with `inproc::pointer`'s override while there is one. With
/// `hidden_window`, `jgl.dll`'s window also goes on painting while hidden
/// (`inproc::paint`: it paints the screen on `WM_PAINT`, which hidden
/// windows do not get). The hooks stay for the life of the process.
fn hook_game_window(hidden_window: bool) {
    match inproc::pointer::hook_focus(inproc::pe::main_module()) {
        Ok(Some(hook)) => std::mem::forget(hook),
        Ok(None) => log::warn!("edge scroll: the game does not import GetFocus"),
        Err(e) => log::warn!("edge scroll: hooking GetFocus: {e}"),
    }
    // Shift and Ctrl held in the viewer: the game (Shift+click queues a
    // build in the city screen) and C3X ask GetAsyncKeyState, which never
    // sees keys forwarded from another window.
    match inproc::keyboard::hook_async_key_state(inproc::pe::main_module()) {
        Ok(Some(hook)) => std::mem::forget(hook),
        Ok(None) => log::warn!("modifiers: the game does not import GetAsyncKeyState"),
        Err(e) => log::warn!("modifiers: hooking GetAsyncKeyState: {e}"),
    }
    std::thread::spawn(move || {
        let Some(jgl) = (0..600).find_map(|_| inproc::pe::module_base("jgl.dll").or_else(|| {
            std::thread::sleep(Duration::from_millis(200));
            None
        })) else {
            return log::warn!("jgl.dll was not loaded within 2 minutes: no edge scroll");
        };
        // Redirected as it loaded (`crate::game_window`): what that did.
        if crate::game_window::jgl_hooked() {
            for note in crate::game_window::take_notes() {
                log::info!("{note}");
            }
            return;
        }
        log::info!("jgl.dll was loaded otherwise than by the game: redirected now");
        // jgl.dll can be in the module list a moment before its headers and
        // import table are readable (seen under Wine: "cannot read 4
        // bytes"): it is tried again for a while then.
        let retried = |what: &str, mut hook: Box<dyn FnMut() -> Result<bool, inproc::hook::HookError>>| {
            let mut failed = None;
            for tries in 1..=50 {
                match hook() {
                    Ok(true) => {
                        if let Some(e) = &failed {
                            log::info!("{what}: hooked after {tries} tries (at first: {e})");
                        }
                        return;
                    }
                    Ok(false) => return log::warn!("{what}: jgl.dll does not import the function"),
                    Err(e) => {
                        if failed.is_none() {
                            log::debug!("{what}: {e}; trying again");
                        }
                        failed = Some(e);
                    }
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            log::warn!("{what}: {} (tried for 10 s)", failed.map(|e| e.to_string()).unwrap_or_default());
        };
        retried(
            "edge scroll: hooking GetCursorPos",
            Box::new(move || {
                let hook = inproc::pointer::hook_cursor_pos(jgl)?;
                let found = hook.is_some();
                if let Some(hook) = hook {
                    log::info!("edge scroll: following the viewer's pointer at its edge");
                    std::mem::forget(hook);
                }
                Ok(found)
            }),
        );
        if hidden_window {
            retried(
                "hidden game window: hooking its painting",
                Box::new(move || {
                    let hooks = inproc::paint::hook_hidden_painting(jgl)?;
                    let found = hooks.is_some();
                    if let Some(hooks) = hooks {
                        log::info!("the game's window paints while hidden");
                        std::mem::forget(hooks);
                    }
                    Ok(found)
                }),
            );
        }
    });
}
