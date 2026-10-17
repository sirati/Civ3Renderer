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
use crate::world::{Civ3City, Civ3Tile, Civ3Unit, Civ3View, Civ3World, IconBuilding, WallBuilding};

/// The viewer window's title (also how we tell it from the game's window).
const VIEWER_TITLE: &str = "Civ III (reimplemented renderer)";


/// How often the publisher looks at the game.
const POLL: Duration = Duration::from_millis(33);
/// How long a font no text has asked for stays in the art (see `text::Fonts::prune`).
const FONT_KEPT: Duration = Duration::from_secs(60);
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
        effects: optional("effects", game.effects()).into_iter().map(|e| effect(&e)).collect(),
        preferences: optional("preferences", game.preferences()),
        debug_mode_bits: optional("debug_mode_bits", game.debug_mode_bits()),
        civ_adjectives: players.adjectives,
        mouse_tile: optional("mouse_tile", game.mouse_tile()),
        current_unit: optional("current_unit", game.current_unit()),
        focus_city: optional("focus_city", game.focus_city()),
        outposts: optional("outposts", game.outposts()),
        selection: optional("selection", game.selection()),
        targeting: optional("targeting", game.targeting()),
        goto: crate::targeting::latest_path(),
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
    /// The largest atlas page the renderer takes (`assets::compile_as`).
    max_page: u32,
    /// The largest sprite the art may grow by: a picture of the game's
    /// whole screen (the pages are made for it from the start).
    largest: u32,
}

impl Compiler {
    fn files(&self, folders: &ArtFolders) -> civ3_assets::GameFiles {
        assets::game_files(&self.install, folders.scenario.as_deref(), &folders.search)
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
        }
    }

    /// A job for the art thread (`generation`: `Presenter::prepare_assets`).
    #[allow(clippy::too_many_arguments)]
    fn job(&self, folders: &ArtFolders, work: Work, pixels: bool, generation: u64, token: Option<u64>, reason: String, wanted: std::time::Instant) -> ArtJob {
        ArtJob { name: self.name.clone(), files: self.files(folders), work, pixels, max_page: self.max_page, generation, token, reason, wanted, stop: None }
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
/// `civ3r_native.dll`, see `tools/install-native`).
pub fn start(game: Civ3) -> Result<(), LiveError> {
    let (name, mut text) = description()?;
    // Rings around foreign units, if asked for (`crate::rings`).
    let rings = crate::rings::Rings::from_env();
    text += &rings.as_ref().map(|r| r.description()).unwrap_or_default();
    // The generated pictures of our preferences tabs (`crate::prefs`).
    text += &crate::prefs::Prefs::description();
    // What the map shows while a command waits for its tile (`crate::targeting`).
    text += &crate::targeting::description();
    let mode = std::env::var("CIV3R_VIEWER").unwrap_or_default();
    // The native viewer: loaded first, its GPU's limits size the atlas pages.
    let native = match mode.starts_with("native") {
        true => Some(presenter::Native::load(mode == "native-fullscreen").map_err(|e| LiveError::Install(std::io::Error::other(e)))?),
        false => None,
    };
    let max_page = match &native {
        Some(n) => n.max_texture(),
        None => tile_render::gpu::max_texture_dimension(),
    };
    let max_page = max_page.unwrap_or_else(|| {
        log::warn!("the renderer's texture limit is unknown: atlas pages of at most {}", render_desc::ANY_DEVICE_PAGE);
        render_desc::ANY_DEVICE_PAGE
    });
    // (Screen pictures are at most the game's screen.)
    let largest = crate::game_window::game_screen().map_or(0, |(w, h)| w.max(h));
    let compiler = Compiler { install: install_root()?, name, text, rings, max_page, largest };
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
    crate::preview::install(&game);
    // The native viewer makes its own atlas: this side needs the tables only.
    let pixels = native.is_none();
    hook_game_window(crate::game_window::hidden());
    let first = compiler.whole_text(&units::Needed::default(), &mut text::Fonts::default(), &Default::default(), &Default::default());
    let wire = compiler.wire(&folders, &first, false);
    let started = std::time::Instant::now();
    let (compiled, growing) = assets::compile_growing(&first, compiler.files(&folders), pixels, compiler.max_page, &|| false)
        .map_err(|e| LiveError::Desc(compiler.name.clone(), e.to_string()))?;
    log::info!("live renderer: {} in {} ms ({folders:?})", art_summary(&compiled, pixels), started.elapsed().as_millis());
    let compiled = Arc::new(compiled);
    let mut presenter = if let Some(native) = native {
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
    let compiled = compiled.without_pixels();
    let art_thread = ArtThread::start(Some((compiled.without_pixels(), growing))).map_err(LiveError::Install)?;
    let timing = crate::art_schedule::Timing::GROWING;
    std::thread::Builder::new()
        .name("civ3r-publish".into())
        .spawn(move || publish(game, &compiler, folders, compiled, presenter, art_thread, timing, pixels))
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
/// labels, whether its unit art is resolved.
#[derive(Clone)]
struct PreviewShown {
    number: u64,
    world: Arc<Civ3World>,
    area: [i32; 4],
    labels: Arc<crate::preview::LabelItems>,
    needed: Option<Arc<units::Needed>>,
}

impl PreviewShown {
    fn of(c: &crate::preview::Current) -> Self {
        Self { number: c.number, world: c.preview.world.clone(), area: c.preview.area, labels: c.labels.clone(), needed: c.needed.clone() }
    }
}

/// (The same preview as made: the world is not compared tile by tile.)
impl PartialEq for PreviewShown {
    fn eq(&self, o: &Self) -> bool {
        self.number == o.number && Arc::ptr_eq(&self.world, &o.world) && Arc::ptr_eq(&self.labels, &o.labels) && self.needed.is_some() == o.needed.is_some()
    }
}

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
) {
    let mut mapper = Mapper::new(&compiled);
    let menus_of = |c: &Compiled| menu::Menus::parse(MENUS, c).inspect_err(|e| log::error!("menus: {e}")).ok();
    let mut menus = menus_of(&compiled);
    let mut last: Option<(Shown, Vec<text::TextRun>, Option<crate::ui::Frame>, Vec<scene::Interactable>, scene::Cursor)> = None;
    // The viewer is the only window the player sees: the game's own is
    // hidden (`CIV3R_GAME_WINDOW=show` keeps it shown, e.g. for looking at
    // it next to the viewer).
    let hide_game_window = crate::game_window::hidden();
    let mut hiding = Hiding::default();
    let mut seq = 0;
    let mut input = InputForwarder::default();
    let mut title = menu::MenuState::default();
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
    // Growths a renderer compiling its own art may not have yet (scenes
    // use them once it has).
    // (With the request they cover, `ArtSchedule::done` once they are used.)
    let mut pending_growths: std::collections::VecDeque<(u64, Arc<render_desc::Growth>, Option<u64>)> = Default::default();
    // What units drew last (shown while their new animation's art is built).
    let mut unit_memory = units::UnitMemory::default();
    // Art made ahead (`crate::preload`): the plan for the game shown (and
    // which game that is), the interface art to make, the budget, the
    // atlas memory the art takes (the art thread says, with the pixels;
    // else estimated from what was sent).
    let art_list = crate::preload::ArtList::parse(crate::preload::BUILTIN_LIST);
    let mut preload = crate::preload::Preload::none();
    let mut preload_for = None;
    let mut planning: Option<std::sync::mpsc::Receiver<crate::preload::Preload>> = None;
    let budget = crate::preload::budget(if pixels { PRELOAD_MIB_IN_PROCESS } else { PRELOAD_MIB_NATIVE });
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
    let mut published_prefs: (Vec<menu::panel::Draw>, Option<viewer_settings::Settings>) = (Vec::new(), None);
    // The last scene's size: bytes of cells, world sprites, overlay sprites.
    let mut scene_size = (0, 0, 0);
    // The replay picture belongs to the renderer. The game supplies semantic
    // navigator operations; revisions avoid rebuilding unchanged pixels.
    let mut replay_surface: Vec<(&str, (u32, u32), Vec<u8>)> = Vec::new();
    let mut replay_revision = 0;
    let mut last_replay_revision = 0;
    // The scenario the Load Scenario screen previews.
    let mut previews = crate::preview::Previews::default();
    // How often the art folders changed (whole descriptions for folders
    // before stop, `crate::art::ArtStop`).
    let folders_changed = Arc::new(std::sync::atomic::AtomicU64::new(0));
    // The map screen's interface: while it is composed, a game runs.
    let main_gui = civ3_sys::ui_hooks::main_gui_canvas(game.build);
    let end_of_game = civ3_sys::ui_hooks::end_of_game_forms(game.build);
    // (Measuring: when this round of the publisher began, and since when a
    // scene has been held back.)
    let mut round_began: Option<std::time::Instant> = None;
    let mut held_since: Option<std::time::Instant> = None;
    // What the last publish made of the parts of the world and of the
    // interface that did not change since (`Memo`): the world's cells, the
    // city labels, the interface's and the labels' sprites, the minimap.
    let mut cells_memo = Memo::default();
    let mut labels_memo = Memo::default();
    let mut ui_sprites_memo = Memo::default();
    let mut label_sprites_memo = Memo::default();
    let mut minimap_memo = Memo::default();
    // The interface's items (the parts of the screen's frame: the frame,
    // the menu, the popup, the screen part) and the sprites of the parts drawn
    // in views of their own (corner, hint, screen, popup).
    let mut items_memos: [Memo<_, Vec<ui_screen::Item>>; 4] = Default::default();
    let mut part_memos: [Memo<_, Vec<scene::FreeSprite>>; 4] = Default::default();
    // Bumped whenever `compiled` changes (new art, a growth): the memos of
    // what is made of the art key on it.
    let mut art_generation = 0u64;
    // Where the publisher's time goes (`CIV3R_PROFILE=1`, or the viewer's
    // `TILE_VIEWER_PROFILE=1`), logged every 5 s.
    let mut profile = (tile_viewer::profile::env_on("CIV3R_PROFILE") || tile_viewer::profile::env_on("TILE_VIEWER_PROFILE"))
        .then(|| tile_viewer::profile::Profile::new("publisher", Duration::from_secs(5)));
    // How many controls had looks last (for the log).
    let mut last_looks = (0, 0, 0, 0);
    loop {
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
        previews.wait(POLL);
        round_began = Some(std::time::Instant::now());
        if let Some(p) = &mut profile {
            p.round_start();
        }
        let mut t = std::time::Instant::now();
        // Now and then, how much everything holds (memory diagnostics).
        if last_stats.elapsed() >= Duration::from_secs(30) {
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
                screen_art.regions.len(),
                screen_art.fills.len(),
                screen_art.dynamic.len(),
                district_art.len(),
                scene_size.0 >> 10,
                scene_size.1,
                scene_size.2,
            );
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
                    presenter.adopt_assets(&c, done.generation);
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
                    if g.is_empty() {
                        if let Some(token) = done.token {
                            schedule.done(now, token);
                        }
                    } else {
                        pending_growths.push_back((done.generation, g, done.token));
                    }
                }
                Err(e) => {
                    if e.contains(crate::assets::STOPPED) {
                        log::info!("art: a whole description for art folders no longer shown stopped after {} ms", done.took.as_millis());
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
        if pending_growths.front().is_some_and(|(generation, _, _)| *generation <= ready) {
            let now = std::time::Instant::now();
            while pending_growths.front().is_some_and(|(generation, _, _)| *generation <= ready) {
                let Some((generation, g, token)) = pending_growths.pop_front() else { break };
                presenter.adopt_growth(&g, generation);
                compiled.apply(&g);
                art_generation += 1;
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
        // The game's interface, mirrored (else its text only).
        let ui = ui_trace::frame();
        mark(&mut profile, "ui frame", &mut t);
        // The world shows while the map screen's interface is composed (under
        // whatever screens are open over it). Between games the game's tables
        // may still hold one (the last game's, or the scenario the Load
        // Scenario screen loaded to describe it), which is not one to show.
        // Neither once the game has ended: the screens after it (the
        // victory screen, the replay, the top cities, the ranking, the hall of
        // fame) show over Main_GUI, round a map the game leaves black.
        let in_game = ui.as_ref().is_none_or(|f| f.forms.iter().any(|f| f.1 == main_gui) && !f.forms.iter().any(|f| end_of_game.contains(&f.0)));
        // The Load Scenario screen, and the world of the scenario it shows.
        let load_screen = ui.as_ref().filter(|_| !in_game).and_then(|f| crate::preview::screen(f, game.build));
        // (Made ready on a thread of its own: as far as it is now.)
        let preview = load_screen.and_then(|_| previews.update(&compiler.install)).cloned();
        // A scenario brings its own art (also the one previewed, once the
        // preview's thread has resolved its unit art there: until then the
        // world shows with the art before; nor the game's folders then: it
        // has loaded the scenario for the screen itself).
        let now = match (&preview, load_screen) {
            (Some(p), _) if p.needed.is_some() => {
                let (scenario, search) = p.folders();
                ArtFolders { scenario, search }
            }
            (_, Some(_)) => folders.clone(),
            _ => ArtFolders::of(&game),
        };
        let resolved_art = if preview.as_ref().is_some_and(|p| p.needed.is_some()) { previews.take_art() } else { None };
        let mut recompile = false;
        // Art for what the game's screen shows (held back without it).
        let mut screen_recompile = false;
        // What the art is rebuilt for (for the log).
        let mut wanted_for: Vec<&str> = Vec::new();
        if now != folders {
            folders = now;
            folders_changed.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            art = resolved_art.unwrap_or_else(|| units::UnitArt::new(compiler.files(&folders)));
            label_env = crate::city_labels::SystemTextEnv::new(crate::city_labels::FileEnv::new(Box::new(compiler.files(&folders))));
            labels_memo = Memo::default();
            items_memos = Default::default();
            have = units::Needed::default();
            whole_wanted = true;
            recompile = true;
            wanted_for.push("art folders");
        } else if let Some(a) = resolved_art {
            art = a;
        }
        // (Until the viewer is gone, see `forward_input`.)
        if hide_game_window && inproc::hidden::hiding() {
            hide_window(&mut input, &mut hiding);
        }
        // (No unit art is read here while the preview's thread reads it for
        // the preview shown.)
        art.set_reading(!(preview.is_some() && previews.art_pending()));
        let inputs = presenter.take_input();
        // What our preferences tabs take does not reach the game.
        let inputs = prefs.route(inputs, input.viewport);
        presenter.forward_log();
        mark(&mut profile, "preview+input", &mut t);
        let snapshot = if in_game { snapshot(&game) } else { Err(Civ3Error::NoGame) };
        mark(&mut profile, "snapshot", &mut t);
        let shown = match (snapshot, load_screen) {
            (_, Some(rect)) => Shown::Preview(rect, preview.as_ref().map(PreviewShown::of)),
            (Ok((world, view)), None) => Shown::Map(world, view),
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
                continue;
            }
        };
        ui_trace::set_skipping(matches!(shown, Shown::Map(..)));
        forward_input(&game, &mut input, inputs);
        if input.viewport == (0, 0) {
            input.game_window();
        }
        let modal = game.modal_form().ok().flatten().filter(|&m| m != 0);
        // Our tabs over the game's Preferences screen, while it is shown.
        let prefs_form = civ3_sys::ui_hooks::preferences_form(game.build);
        prefs.update(ui.as_ref(), prefs_form, input.viewport, modal);
        let prefs_draws = prefs.draws(&fonts);
        let prefs_changed = published_prefs.0 != prefs_draws || published_prefs.1.as_ref() != Some(prefs.settings());
        let texts = if ui.is_some() { Vec::new() } else { trace::recent_texts() };
        // What can be clicked (for the viewer's button snapping): our own
        // title menu's buttons, else the game's controls.
        let own_title = menus.as_ref().and_then(|ms| ms.menu("title").map(|m| (ms, m))).filter(|_| matches!(shown, Shown::Title(_)) && ui.is_none());
        let controls = match own_title {
            Some(_) => Vec::new(),
            None => civ3_sys::controls::sites(game.build).map(|s| civ3_sys::controls::controls(&civ3_sys::controls::Live, &s)).unwrap_or_default(),
        };
        // Our preferences tabs' controls are over the game's.
        let prefs_looks = prefs.looks(&fonts);
        let mut interactables: Vec<scene::Interactable> =
            prefs_looks.iter().map(|l| scene::Interactable { rect: l.rect, kind: scene::InteractableKind::Button, id: l.id }).collect();
        interactables.extend(match own_title {
            Some((ms, m)) => ms.interactables(m, input.viewport),
            None => ui_screen::interactables(&controls),
        });
        // What the open city screen tests in its code (citizens, units).
        if let (Shown::Map(world, _), Some(f)) = (&shown, ui.as_ref()) {
            if world.focus_city.is_some() {
                interactables.extend(ui_screen::city_drawn_targets(f, modal.map(|m| m + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4)));
            }
        }
        // The pointer the game shows (docs §16.11).
        let game_cursor = civ3_sys::cursor::sites(game.build).and_then(|s| civ3_sys::cursor::read(&civ3_sys::controls::Live, &s));
        let standard = inproc::pointer::last_set_cursor().and_then(inproc::pointer::standard_cursor);
        let cursor = crate::cursor::scene_cursor(game_cursor, standard, |n| compiled.set(n).map(|(s, _)| s.raw()), crate::cursor::system_arrow());
        if last.as_ref().is_none_or(|l| l.4 != cursor) {
            log::debug!("cursor: the game shows {game_cursor:?} (system pointer {standard:?}): {cursor:?}");
        }
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
            wanted_for.push("text");
        }
        let state = (shown, texts, ui, interactables, cursor);
        let unchanged = last.as_ref() == Some(&state);
        mark(&mut profile, "compare", &mut t);
        if unchanged && replay_revision == last_replay_revision && !recompile && !new_art && !schedule.pending() && input.look.is_none() && input.world_buttons.is_empty() && !prefs_changed {
            if let Some(p) = &mut profile {
                p.event("unchanged", 1);
            }
            continue;
        }
        let (shown, texts, ui, interactables, cursor) = &state;
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
        let screen_open = ui.as_ref().and_then(|f| crate::interface_parts::screen(f, civ3_sys::ui_hooks::screen_forms(game.build)));
        let screen_size = (input.viewport.0 as i32, input.viewport.1 as i32);
        // (A context menu over the map is one too, modal or not: the topmost
        // form there.)
        let context_menu = ui.as_ref().and_then(|f| f.forms.last().filter(|t| civ3_sys::controls::CONTEXT_MENU_VTABLES.contains(&t.0)).map(|t| t.1));
        let popup = match (ui, modal, shown) {
            (Some(f), m, Shown::Map(..)) => {
                let modal_popup = m.and_then(|m| crate::interface_parts::popup(f, m + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4, screen_open.map(|s| s.0), screen_size));
                modal_popup.or_else(|| context_menu.and_then(|c| crate::interface_parts::popup(f, c, screen_open.map(|s| s.0), screen_size)))
            }
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
            Shown::Title(_) | Shown::Preview(..) => 0,
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
        // A named screen smaller than the window (an advisor; not the city
        // screen, whose map is a view of its own), and what is over it: a
        // part of its own, which the viewer may move (out from under its
        // zoom strip).
        let screen_part = match (&visible_ui, screen_open, shown) {
            (Some(f), Some((index, name)), Shown::Map(..)) if name != "City" && city.is_none() => crate::interface_parts::screen_part(f, index, screen_size),
            _ => None,
        };
        let (visible_ui, ui_screen_part) = match (visible_ui, screen_part) {
            (Some(f), Some((first, _))) => {
                let (below, above) = crate::interface_parts::split_at(&f, first);
                (Some(below), Some(above))
            }
            (v, _) => (v, None),
        };
        // On the map's own screen: the corner's buttons and their tooltip
        // (drawn in the view the pointer is in only: the rest of the
        // interface leaves them out) and the unit's orders (reachable from
        // every view).
        let hud = civ3_sys::ui_hooks::hud_buttons(game.build);
        let on_map = matches!(shown, Shown::Map(..)) && screen_open.is_none();
        let hud_ui = visible_ui.as_ref().filter(|_| on_map);
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
        let (frame_ui, menu_ui) = match (&visible_ui, shown) {
            (Some(f), Shown::Map(..)) => {
                let (frame, menus) = ui_screen::split(f, ui_screen::menus_from(f));
                (Some(frame), Some(menus))
            }
            (Some(f), Shown::Title(_) | Shown::Preview(..)) => (None, Some(f.clone())),
            (None, _) => (None, None),
        };
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
        let mut menu_items = menu_ui.as_ref().map(|f| with_tabs(menu_memo, f)).unwrap_or_default();
        let mut popup_items = ui_popup.as_ref().map(|f| with_tabs(popup_memo, f)).unwrap_or_default();
        let mut screen_items = ui_screen_part.as_ref().map(|f| with_tabs(screen_memo, f)).unwrap_or_default();
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
                let labels = labels_memo.get((still.clone(), view.zoomed_out), || crate::city_labels::labels(world, view.zoomed_out, &mut label_env));
                let everywhere = [-(1 << 24), -(1 << 24), 1 << 24, 1 << 24];
                crate::city_labels::items(&labels, (0, 0), everywhere, true)
            }
            // (Drawn with the preview's scene, `preview::scene`.)
            // (Made with it, on the preview's thread.)
            Shown::Preview(_, Some(p)) if ui.is_some() => {
                preview_labels = p.labels.clone();
                preview_labels.iter().flat_map(|(_, items)| items.iter().cloned()).collect()
            }
            _ => Vec::new(),
        };
        // The go-to path's turn count (§10.15), with the world as the labels.
        let target_items = match shown {
            Shown::Map(world, view) => crate::targeting::label_items(world, view.zoomed_out, &mut label_env),
            _ => Vec::new(),
        };
        // The viewer's own views marked on the minimap.
        if let Shown::Map(world, view) = shown {
            // (The game's own view as the viewer shows it.)
            if let Some(main) = input.main_view {
                ui_screen::replace_game_view_mark(&mut ui_items, world, (view.tile_x.0, view.tile_y.0, view.tile_x.1, view.tile_y.1), main, view.zoomed_out);
            }
            ui_screen::mark_views(&mut ui_items, world, &input.views, view.zoomed_out, view_rect_color());
        }
        // (Our tabs where no part draws the screen: on top.)
        menu_items.extend(prefs_items.take().unwrap_or_default());
        // Only a popup over the map's own screen is the viewer's to place;
        // one over another screen (the Preferences screen, an advisor) stays
        // with it, with the menus.
        let popup = popup.filter(|_| on_map && !prefs.is_open());
        if popup.is_none() {
            if screen_part.is_some() { &mut screen_items } else { &mut menu_items }.append(&mut popup_items);
        }
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
        // The controls in their other states (hover, pressed), for the
        // viewer to draw at once (`crate::looks`), and their art.
        let arts: Vec<crate::looks::ControlArt> = match (ui, civ3_sys::controls::sites(game.build)) {
            (Some(_), Some(sites)) => controls.iter().filter_map(|c| control_art(&sites, c)).collect(),
            _ => Vec::new(),
        };
        let control_looks: Vec<crate::looks::ControlLooks> = match ui {
            Some(f) => arts.iter().filter_map(|a| crate::looks::control_looks(f, a)).collect(),
            None => Vec::new(),
        };
        let looks_items = |draws: &[crate::ui::ScreenDraw]| ui_screen::items(&crate::ui::Frame { draws: draws.to_vec(), ..Default::default() }, files.roots(), generated, unit_colors);
        let mut looks_art = false;
        for (_, draws) in control_looks.iter().flat_map(|c| &c.states) {
            looks_art |= ui_screen::add_art(&looks_items(draws), &mut screen_art, &mut fonts, &files);
        }
        for (_, draws) in prefs_looks.iter().flat_map(|c| &c.states) {
            looks_art |= ui_screen::add_art(&crate::prefs::items(draws), &mut screen_art, &mut fonts, &files);
        }
        if looks_art {
            screen_recompile = true;
            wanted_for.push("control states");
        }
        if ui_screen::add_art(&ui_items, &mut screen_art, &mut fonts, &files)
            | ui_screen::add_art(&menu_items, &mut screen_art, &mut fonts, &files)
            | ui_screen::add_art(&popup_items, &mut screen_art, &mut fonts, &files)
            | ui_screen::add_art(&screen_items, &mut screen_art, &mut fonts, &files)
            | ui_screen::add_art(&corner_items, &mut screen_art, &mut fonts, &files)
            | ui_screen::add_art(&hint.1, &mut screen_art, &mut fonts, &files)
        {
            screen_recompile = true;
            wanted_for.push("interface");
        }
        if ui_screen::add_art(&label_items, &mut screen_art, &mut fonts, &files) {
            recompile = true;
            wanted_for.push("city labels");
        }
        if ui_screen::add_art(&target_items, &mut screen_art, &mut fonts, &files) {
            recompile = true;
            wanted_for.push("go-to turns");
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
            wanted_for.push("text");
        }
        if let Some(last) = &mut text_trace {
            trace_text(
                match shown { Shown::Map(..) => "map", Shown::Title(..) => "title", Shown::Preview(..) => "preview" },
                ui.as_ref(),
                ui_items.iter().chain(&menu_items).chain(&popup_items).chain(&screen_items).chain(&corner_items).chain(&label_items).chain(&target_items),
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
            schedule.want(now, screen_recompile);
            if wanted_for.contains(&"art folders") {
                previews.art_wanted(schedule.asked());
            }
            art_wanted.get_or_insert(now);
            reasons.extend(wanted_for.iter().copied());
        }
        // A new game: plan the art to make ahead for it (once its units are
        // there).
        // (The plan reads the headers of every unit's art files: on a thread
        // of its own, the game goes on.)
        if let Shown::Map(world, view) = shown {
            let key = Some((folders.clone(), crate::preload::game_key(world)));
            if preload_for != key && !world.units.is_empty() && !world.unit_type_keys.is_empty() {
                preload_for = key;
                preload = crate::preload::Preload::none();
                let (tx, rx) = std::sync::mpsc::channel();
                let (world, view, list, files) = (world.clone(), *view, art_list.clone(), compiler.files(&folders));
                let spawned = std::thread::Builder::new().name("civ3r-preload-plan".into()).spawn(move || {
                    let mut art = units::UnitArt::new(files);
                    let _ = tx.send(crate::preload::Preload::plan(&world, &view, &mut art, &list, budget));
                });
                match spawned {
                    Ok(_) => planning = Some(rx),
                    Err(e) => log::error!("preload: planning: {e}"),
                }
            }
        }
        if let Some(plan) = planning.as_ref().and_then(|rx| rx.try_recv().ok()) {
            planning = None;
            preload = plan;
            // Room in the atlas for what will be made (as far as the
            // budget goes), at once: not moved again and again.
            let page = 2048 * 2048;
            let used = atlas_used.max(sent_bytes);
            let ahead = preload.left().min(budget.saturating_sub(used));
            // (Mostly indexed art; the shelves leave some room unused.)
            let index_pages = ((used + ahead * 5 / 4).min(budget.max(used)) / page) as u32 + 1;
            presenter.reserve(0, index_pages);
        }
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
                    let generation = presenter.prepare_assets(&compiler.wire(&folders, text, append));
                    let stop = matches!(work, Work::Whole(_)).then(|| crate::art::ArtStop { started: folders_changed.load(std::sync::atomic::Ordering::Acquire), now: folders_changed.clone() });
                    if !art_thread.send(crate::art::ArtJob { stop, ..compiler.job(&folders, work, pixels, generation, Some(token), reason, wanted) }) {
                        log::error!("the art thread is gone");
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
        if art_thread.idle() && pending_growths.is_empty() && !schedule.pending() && !whole_wanted {
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
                            screen_art.fills.extend(art.fills);
                            screen_art.dynamic.extend(art.dynamic);
                        }
                        crate::preload::Item::Font(style, color, backdrop) => drop(fonts.add_style(&files, style, color, backdrop)),
                    }
                }
                if let Some(text) = addition(&have, &mut described_units, &screen_art, &mut described_screen, &district_art, &mut described_districts, &mut fonts) {
                    let generation = presenter.prepare_assets(&compiler.wire(&folders, &text, true));
                    if !art_thread.send(compiler.job(&folders, Work::Addition(text), pixels, generation, None, format!("preload ({tier})"), now)) {
                        log::error!("the art thread is gone");
                    }
                }
            }
        }
        // (Nothing waits for art: what is not built yet is left out, or
        // drawn as it was, until it is.)
        let _ = &mut held_since;
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
                units::place(world, view, &mut art, &compiled, compiler.rings.as_ref(), &mut unit_memory, &mut scene);
                crate::effects::place(world, view, &mut art, &compiled, &mut scene);
                // The mirrored interface has the city icons already.
                if ui.is_none() {
                    scene.sprites.extend(city_hud::sprites(world, &compiled));
                }
                // Choosing a target (§10.15): ours, whether or not the game
                // draws its map.
                scene.sprites.extend(crate::targeting::range_sprites(world, &compiled, view.zoomed_out));
                scene.sprites.extend(crate::targeting::sprites(world, view.zoomed_out, &mut art, &compiled));
                // The point of interest: the active unit.
                // (In the game view's pixels: halved zoomed out; the sprites
                // stay at full size, the renderer halves them.)
                scene.focus = units::focus(world);
                if view.zoomed_out {
                    scene.focus = scene.focus.map(|(x, y)| (x / 2, y / 2));
                }
                scene
            }
            Shown::Preview(_, Some(p)) => crate::preview::scene(&p.world, preview_scale, &mapper, &mut art, &compiled, &preview_labels, &fonts),
            Shown::Preview(_, None) => scene::Scene::empty(scene::WorldShape::default(), compiled.layers.len()),
            Shown::Title(state) => {
                // Our own title menu, unless the game's is mirrored.
                if let Some(m) = menus.as_ref().and_then(|ms| ms.menu("title").map(|m| (ms, m))).filter(|_| ui.is_none()) {
                    title_menu = m.0.sprites(m.1, state, input.viewport, u32::MAX);
                }
                scene::Scene::empty(scene::WorldShape::default(), compiled.layers.len())
            }
        };
        mark(&mut profile, "scene map+units", &mut t);
        scene.sprites.extend(fonts.sprites(texts, &compiled));
        // Labels and the interface's text also drawn from outlines at
        // every zoom (where the art has the face).
        scene.zooms = ui_screen::outline_zooms(&compiled);
        let outlines = !scene.zooms.is_empty();
        // (The art, its fonts and the items give the sprites.)
        let art_key = (art_generation, fonts.len(), outlines);
        scene.sprites.extend(ui_sprites_memo.get((ui_items.clone(), art_key), || ui_screen::sprites(&ui_items, &fonts, &compiled, ui_screen::under_pass(&compiled), outlines)));
        let mut menu_sprites = title_menu;
        menu_sprites.extend(ui_screen::sprites(&menu_items, &fonts, &compiled, ui_screen::under_pass(&compiled), outlines));
        // The city labels go with the world (laid out in the game view's
        // pixels: zoomed out at full size, which the renderer halves).
        // (A preview's are in its scene already.)
        let label_items = if matches!(shown, Shown::Preview(..)) { Vec::new() } else { label_items };
        let mut labels: Vec<_> = label_sprites_memo
            .get((label_items.clone(), art_key), || ui_screen::sprites(&label_items, &fonts, &compiled, ui_screen::under_pass(&compiled), outlines))
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
        // The go-to path's turn count, over the fog, under the units (laid
        // out as the labels).
        let under_units = units::unit_passes(&compiled).1;
        let mut turns: Vec<_> = ui_screen::sprites(&target_items, &fonts, &compiled, under_units, outlines)
            .into_iter()
            .map(|s| scene::FreeSprite { screen: false, clip: None, ..s })
            .collect();
        if let Shown::Map(_, view) = shown {
            if view.zoomed_out {
                crate::scene_map::from_zoomed_out(&mut turns);
            }
        }
        scene.sprites.extend(turns);
        mark(&mut profile, "ui+label sprites", &mut t);
        // Pictures the game makes itself: the minimap (§17).
        if let Shown::Map(world, _) = shown {
            let both: Vec<ui_screen::Item> = ui_items.iter().chain(&screen_items).cloned().collect();
            // (Made of the minimap items, the world without its units and the art.)
            let minimaps: Vec<_> = both.iter().filter(|i| matches!(i, ui_screen::Item::Element(menu::screen::Element::Dynamic { name, .. }, _) if name.starts_with("minimap"))).cloned().collect();
            scene.patches = minimap_memo.get((minimaps, still.clone(), art_generation), || {
                ui_screen::minimap_patches(&both, world, &compiled, |table| *table_colors.entry(table).or_insert_with(|| art.table_color(table, 66).unwrap_or_default()))
            });
        }
        mark(&mut profile, "minimap", &mut t);
        // Replay pixels made by our renderer from terrain and history changes.
        let all = ui_items.iter().chain(&menu_items).chain(&screen_items).chain(&popup_items);
        scene.patches.extend(ui_screen::surface_patches(all, &replay_surface, &compiled));
        crate::scene_map::overlay_screen_sprites(&mut scene);
        crate::scene_map::overlay_menus(&mut scene, menu_sprites);
        // The parts of the interface the viewer arranges, in views of their
        // own (the corner's buttons a copy, the popup not in the interface).
        let [corner_memo, hint_memo, screen_part_memo, popup_part_memo] = &mut part_memos;
        let part_sprites = |memo: &mut Memo<_, _>, items: &[ui_screen::Item]| {
            memo.get((items.to_vec(), art_key), || ui_screen::sprites(items, &fonts, &compiled, ui_screen::under_pass(&compiled), outlines))
        };
        if let Some(r) = corner {
            crate::interface_parts::part_view(&mut scene, crate::interface_parts::CORNER_VIEW, r, part_sprites(corner_memo, &corner_items));
            // (In its place while the mouse is captured.)
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
        if let Some((_, r)) = popup {
            crate::interface_parts::part_view(&mut scene, crate::interface_parts::POPUP_VIEW, r, part_sprites(popup_part_memo, &popup_items));
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
            capture_hint: (corner.is_some() || preview_hint).then_some(crate::interface_parts::HINT_VIEW),
            popup: popup.map(|_| crate::interface_parts::POPUP_VIEW),
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
            // The Load Scenario screen at the window's left, its scenario's
            // world beside it.
            beside: match shown {
                Shown::Preview(rect, p) => Some(scene::Beside { screen: *rect, edge: scene::Edge::Left, world: p.as_ref().map(|p| p.area) }),
                _ => None,
            },
        };
        if let (Shown::Map(_, view), Some(([x, y, w, h], _))) = (shown, city) {
            crate::scene_map::city_view(&mut scene, (view.camera_x, view.camera_y), [x, y, w, h]);
        }
        // The renderer's settings go with every scene (sent when they change).
        scene.settings = Some(prefs.settings().clone());
        scene.interactables = interactables.clone();
        scene.cursor = *cursor;
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
            (crate::interface_parts::POPUP_VIEW, None),
        ];
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
            if matches!(shown, Shown::Preview(_, Some(_))) {
                previews.published(seq);
            }
            published_prefs = (prefs_draws, Some(prefs.settings().clone()));
            last_replay_revision = replay_revision;
        }
        last = published.then_some(state);
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

/// The art of `control`'s states, if it is a button the game draws by its
/// state and that state is the pointer's (not forced): the sprites of its
/// pieces as the mirror knows them.
fn control_art(sites: &civ3_sys::controls::Sites, control: &civ3_sys::controls::Control) -> Option<crate::looks::ControlArt> {
    use crate::looks::look_state;
    use civ3_sys::control_art::Select;
    let art = civ3_sys::control_art::of(&civ3_sys::controls::Live, sites, control).filter(|a| !a.forced)?;
    let sprites: Vec<u32> = art.pieces.iter().flatten().map(|(_, s)| *s).collect();
    let mut sources = ui_trace::sprite_sources(&sprites).into_iter();
    let pieces = art
        .pieces
        .iter()
        .map(|p| p.iter().filter_map(|(l, _)| Some((look_state(*l), sources.next()??))).collect())
        .collect();
    let select = match art.select {
        Select::None => scene::Select::None,
        Select::Toggle => scene::Select::Toggle,
        Select::Pick(group) => scene::Select::Pick { group: group as u64 },
    };
    Some(crate::looks::ControlArt {
        id: control.id,
        select,
        canvas: art.canvas,
        part: match art.part {
            civ3_sys::control_art::Part::Whole => crate::looks::Part::Whole,
            civ3_sys::control_art::Part::LastArt => crate::looks::Part::LastArt(control.rect),
            civ3_sys::control_art::Part::Within => crate::looks::Part::Within(control.rect),
        },
        colors: art.colors.iter().map(|(l, c)| (look_state(*l), c.iter().map(|&c| color_rgb(c)).collect())).collect(),
        state: look_state(art.state),
        pieces,
        text_offsets: art.text_offsets.iter().map(|(l, o)| (look_state(*l), *o)).collect(),
        selected_by_fields: select != scene::Select::None,
    })
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
fn still_world(world: &Civ3World) -> Civ3World {
    Civ3World { units: Vec::new(), army_members: Vec::new(), mouse_tile: None, current_unit: None, selection: None, unit_tiles: Vec::new(), effects: Vec::new(), targeting: Default::default(), goto: None, ..world.clone() }
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
    held: inproc::window::Buttons,
    clicks: inproc::window::DoubleClicks,
    /// A world pixel the viewer asked to see (`Input::Look`), not yet acted on.
    look: Option<(i32, i32)>,
    /// What the viewer shows of the game's view where that is not all of
    /// it (`Input::MainView`).
    main_view: Option<(i32, i32, i32, i32)>,
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
    /// Where the viewer last moved the game's pointer (client pixels).
    pointer: Option<(i32, i32)>,
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

fn forward_input(game: &Civ3, forwarder: &mut InputForwarder, inputs: Vec<scene::Input>) {
    use inproc::window::MouseButton;
    use scene::{Button, Input};
    if inputs.is_empty() {
        return;
    }
    let Some(window) = forwarder.game_window() else { return };
    for input in inputs {
        log::trace!("input {input:?}");
        // (For testing what the viewer draws itself: the pointer never
        // reaches the game.)
        if hold_pointer() && matches!(input, Input::MouseMove { .. } | Input::MouseButton { .. }) {
            continue;
        }
        match input {
            Input::MouseMove { x, y } => {
                forwarder.pointer = Some((x, y));
                window.mouse_move(x, y, forwarder.held);
            }
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
                log::debug!("input: key {vk:#x} {}", if down { "down" } else { "up" });
                key(window, vk, down, text)
            }
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
            // (For the preferences' readout, `crate::prefs`.)
            Input::Window { .. } => {}
            // The viewer is gone (closed, or it failed to start): the
            // game's own window is shown, rather than nothing at all.
            Input::Quit => {
                forwarder.edge = (0, 0);
                if crate::game_window::hidden() && inproc::hidden::hiding() {
                    inproc::hidden::set_hiding(false);
                    inproc::pointer::keep_focus(None);
                    window.show();
                    log::warn!("the viewer is gone: the game's own window is shown");
                }
            }
            Input::Edge { x, y } => forwarder.edge = (x, y),
            Input::Look { x, y } => forwarder.look = Some((x, y)),
            Input::Views(views) => forwarder.views = views.iter().copied().filter(|v| v.2 > 0 && v.3 > 0).collect(),
            Input::MainView { x, y, w, h } => forwarder.main_view = (w > 0 && h > 0).then_some((x, y, w, h)),
            // (Text is drawn from outlines at every zoom: no art per zoom.)
            Input::Zooms(_) => {}
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

/// A colour of the game's controls as the mirror has it: text as
/// `SetTextColor` gets it (a packed colour: bit 31 and RGB555, each channel
/// shifted up; else a palette index, black on the game's 16-bit canvases,
/// docs §16.10), fills as RGB555 expanded.
fn color_rgb(c: civ3_sys::control_art::Color) -> [u8; 3] {
    use civ3_sys::control_art::Color;
    match c {
        Color::Text(c) if c & 0x8000_0000 != 0 => [((c >> 7) & 0xF8) as u8, ((c >> 2) & 0xF8) as u8, ((c & 0x1F) << 3) as u8],
        Color::Text(_) => [0, 0, 0],
        Color::Fill(c) => crate::ui::rgb555(c),
    }
}
