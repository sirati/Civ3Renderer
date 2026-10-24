//! The world of the scenario highlighted on the game's Load Scenario
//! screen, shown beside it (docs §19).
//!
//! The screen loads the highlighted scenario into the game's tables to show
//! its players, description and map size (`civ3_sys::menus`); a hook on
//! that call tells us which file it is. We read the file ourselves
//! (`civ3_assets::biq`, on a thread of our own: [`Previews`]) and make a
//! world of it as a nation that does not exist would see it if it had every
//! player's start (`crate::revealed`): what they have explored, what they
//! see; no player's highlights, no active unit.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use civ3_assets::biq::{Biq, Map, Owner, Player, Rules};

use crate::world::{Civ3City, Civ3Tile, Civ3Unit, Civ3World, IconBuilding, WallBuilding};

/// A scenario's world, ready to show.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    /// The file, as the game named it.
    pub file: String,
    pub world: Arc<Civ3World>,
    /// Where the game would look for the scenario's art: its own folder
    /// (the file's path without the extension) and its search folders.
    pub scenario_folder: Option<String>,
    pub search_folders: Vec<String>,
    /// The part of the world to show (x, y, w, h, world pixels at full
    /// size): the tiles revealed (`crate::revealed::revealed_box`).
    pub area: [i32; 4],
}

/// Why a scenario shows no world.
#[derive(Debug, thiserror::Error)]
pub enum PreviewError {
    #[error("{0}: {1}")]
    Read(PathBuf, std::io::Error),
    #[error("{0}: no map of its own (the game makes one)")]
    NoMap(PathBuf),
    #[error("no rules: {0}")]
    Rules(String),
}

/// The game's own rules (`Conquests/conquests.biq`), for scenarios without
/// theirs; read once.
fn default_rules(install: &Path) -> Result<Arc<Rules>, PreviewError> {
    // (A failed read is tried again; two threads racing read the same.)
    static RULES: OnceLock<Arc<Rules>> = OnceLock::new();
    if let Some(r) = RULES.get() {
        return Ok(r.clone());
    }
    let path = install.join("Conquests").join("conquests.biq");
    let biq = Biq::open(&path).map_err(|e| PreviewError::Read(path.clone(), e))?;
    let rules = Arc::new(biq.rules().ok_or_else(|| PreviewError::Rules(format!("{path:?} has none")))?);
    Ok(RULES.get_or_init(|| rules).clone())
}

/// The file `name` (as the game passed it: absolute, or relative to its
/// working folder or to the install).
fn resolve(install: &Path, name: &str) -> PathBuf {
    let native = name.replace('\\', std::path::MAIN_SEPARATOR_STR);
    let path = PathBuf::from(&native);
    if path.is_absolute() || path.exists() {
        return std::path::absolute(&path).unwrap_or(path);
    }
    [install.join("Conquests"), install.to_path_buf()].into_iter().map(|b| b.join(&native)).find(|p| p.exists()).unwrap_or(path)
}

/// The scenario's own art folder as the game names it (relative to the
/// install, the extension dropped, e.g. `Conquests\9 WWII in the Pacific\`).
fn scenario_folder(install: &Path, file: &Path) -> Option<String> {
    let stem = file.with_extension("");
    let rel = stem.strip_prefix(install).ok()?;
    let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    // (The game's folders are relative to the install's Conquests folder.)
    let parts = match parts.split_first() {
        Some((first, rest)) if first.eq_ignore_ascii_case("Conquests") => rest.to_vec(),
        _ => parts,
    };
    (!parts.is_empty()).then(|| parts.join("\\") + "\\")
}

/// Reads the scenario `name` the game highlighted.
pub fn load(install: &Path, name: &str) -> Result<Preview, PreviewError> {
    let path = resolve(install, name);
    let biq = Biq::open(&path).map_err(|e| PreviewError::Read(path.clone(), e))?;
    let map = biq.map().ok_or_else(|| PreviewError::NoMap(path.clone()))?;
    let rules = match biq.rules() {
        Some(r) => Arc::new(r),
        None => default_rules(install)?,
    };
    let players = biq.players();
    let revealed = crate::revealed::revealed(&map, &players, &biq.playable_races(), biq.map_visible());
    let world = world(&map, &players, &rules, &revealed);
    let area = area(&map, &revealed);
    Ok(Preview { file: name.to_owned(), world: Arc::new(world), scenario_folder: scenario_folder(install, &path), search_folders: biq.search_folders(), area })
}

/// Pixels around the tiles revealed that a preview shows too: what reaches
/// above them (mountains, units) and below (city names).
const AREA_MARGIN: (i32, i32) = (64, 64);

/// The part of `map` a preview shows (see [`Preview::area`]): the tiles
/// revealed (all of the map if none are).
pub fn area(map: &Map, revealed: &crate::revealed::Revealed) -> [i32; 4] {
    let all = crate::revealed::TileBox { x: 0, y: 0, w: map.width, h: map.height };
    let b = crate::revealed::revealed_box(map, &revealed.explored).unwrap_or(all);
    crate::revealed::box_pixels(b, AREA_MARGIN)
}

/// One of the world's civilizations, numbered as the world counts them.
#[derive(Copy, Clone, Debug)]
struct Civ {
    race: i32,
    color: i32,
    era: i32,
    government: i32,
}

/// The scenario's civilizations: the barbarians (0), then its players in
/// order (the game numbers them only when the scenario starts), then any
/// race that owns something without being a player.
struct Civs {
    list: Vec<Civ>,
}

impl Civs {
    fn new(players: &[Player], rules: &Rules) -> Civs {
        let race_color = |race: i32| usize::try_from(race).ok().and_then(|r| rules.races.get(r)).map_or(0, |r| r.default_color);
        let mut list = vec![Civ { race: 0, color: 0, era: 0, government: -1 }];
        for p in players.iter().take(31) {
            let color = if p.custom { p.color } else { race_color(p.race) };
            list.push(Civ { race: p.race, color, era: if p.custom { p.initial_era } else { 0 }, government: if p.custom { p.government } else { -1 } });
        }
        Civs { list }
    }

    /// The civ playing `race` (a new one if none does yet).
    fn of_race(&mut self, race: i32, rules: &Rules) -> i32 {
        if race == 0 {
            return 0;
        }
        if let Some(i) = self.list.iter().position(|c| c.race == race) {
            return i as i32;
        }
        if self.list.len() >= 32 {
            return 0;
        }
        let color = usize::try_from(race).ok().and_then(|r| rules.races.get(r)).map_or(0, |r| r.default_color);
        self.list.push(Civ { race, color, era: 0, government: -1 });
        self.list.len() as i32 - 1
    }

    fn of(&mut self, owner: Owner, rules: &Rules) -> i32 {
        match owner {
            Owner::None | Owner::Barbarians => 0,
            Owner::Player(p) if (0..31).contains(&p) && (p as usize + 1) < self.list.len() => p + 1,
            Owner::Player(_) => 0,
            Owner::Race(r) => self.of_race(r, rules),
        }
    }
}

/// `Tile.Overlays` bit of a road (docs §6.6): every city tile has one.
const ROAD: u32 = 1;

/// The world of a scenario's map as a nation that does not exist sees it:
/// it has explored the tiles `revealed` says and sees those it says it sees
/// (the rest under the fog of war, the units there hidden), owns nothing
/// and has no unit; every resource shows.
pub fn world(map: &Map, players: &[Player], rules: &Rules, revealed: &crate::revealed::Revealed) -> Civ3World {
    let seen = |x: i32, y: i32| map.tile(x, y).is_some() && revealed.in_sight.get(((y * map.width + x) / 2) as usize).copied().unwrap_or(false);
    let mut civs = Civs::new(players, rules);
    let cities: Vec<Civ3City> = map
        .cities
        .iter()
        .enumerate()
        .map(|(id, c)| {
            let mut improvements = [0u8; 32];
            for &b in c.buildings.iter().filter(|b| (0..256).contains(*b)) {
                improvements[b as usize >> 3] |= 1 << (b & 7);
            }
            // Walls the city has without the building (a scenario setting).
            let walls = rules.buildings.iter().position(|b| b.bombard_defence > 0);
            if let (true, Some(w)) = (c.has_walls, walls) {
                if !c.buildings.iter().any(|&b| rules.buildings.get(b.max(0) as usize).is_some_and(|b| b.bombard_defence > 0)) {
                    improvements[w >> 3] |= 1 << (w & 7);
                }
            }
            Civ3City { id: id as i32, x: c.x, y: c.y, civ: civs.of(c.owner, rules), size: c.size, improvements, name: c.name.clone(), ..Default::default() }
        })
        .collect();
    // The capitals: the cities with a palace.
    let mut capitals = vec![-1; 32];
    for (c, src) in cities.iter().zip(&map.cities) {
        if src.has_palace {
            capitals[c.civ.clamp(0, 31) as usize] = c.id;
        }
    }
    // One unit shown per tile: the best defender, as the game shows a stack.
    let defence = |t: i32| usize::try_from(t).ok().and_then(|t| rules.unit_types.get(t)).map_or(0, |u| u.defence);
    let mut top: std::collections::BTreeMap<(i32, i32), usize> = std::collections::BTreeMap::new();
    for (i, u) in map.units.iter().enumerate().filter(|(_, u)| seen(u.x, u.y)) {
        let best = top.entry((u.y, u.x)).or_insert(i);
        if defence(u.unit_type) > defence(map.units[*best].unit_type) {
            *best = i;
        }
    }
    let units = top
        .values()
        .map(|&i| {
            let u = &map.units[i];
            Civ3Unit {
                id: i as i32,
                x: u.x,
                y: u.y,
                civ: civs.of(u.owner, rules),
                unit_type: u.unit_type,
                experience: u.experience,
                container: -1,
                // Idle, facing the viewer, at the tile's centre (§10).
                pixel: (64 * u.x + 64, 32 * u.y + 32),
                direction: 3,
                animation: 1,
                visible: true,
                ..Default::default()
            }
        })
        .collect();
    let outposts = map
        .colonies
        .iter()
        .enumerate()
        .filter(|(_, c)| c.kind == 3)
        .map(|(i, c)| (i as i32, civs.of(c.owner, rules)))
        .collect();
    let tiles = map
        .tiles
        .iter()
        .map(|t| Civ3Tile {
            square_type: u32::from(t.terrain) << 8,
            square_parts: u16::from(t.texture_file) << 8 | u16::from(t.texture_location),
            overlays: t.overlays | if t.city >= 0 { ROAD } else { 0 },
            terrain_overlays: t.bonuses << 16,
            river: t.river,
            resource: t.resource,
            owner: t.border_race().map_or(0, |r| civs.of_race(r, rules)) as i8,
            city: t.city,
            // (What the viewer has explored and sees: below, once it has its
            // number.)
            victory_location: t.victory_location,
            building: Some(t.colony).filter(|&c| c >= 0),
            ruins: t.ruin,
            city_area: -1,
            continent: t.continent,
            ..Default::default()
        })
        .collect::<Vec<Civ3Tile>>();
    // The viewer: a civ that does not exist (the first number no civ has).
    let player = if civs.list.len() < 32 { civs.list.len() as i32 } else { 0 };
    // What it has explored, and sees now (the rest under the fog of war,
    // where it knows the improvements it saw).
    let bit = 1u32 << player;
    let mut tiles = tiles;
    for (i, t) in tiles.iter_mut().enumerate() {
        let (explored, seen) = (revealed.explored.get(i).copied().unwrap_or(false), revealed.in_sight.get(i).copied().unwrap_or(false));
        t.explored = if explored { bit } else { 0 };
        t.fow_status = if explored && seen { bit } else { 0 };
        t.visible_overlays[player as usize] = (t.overlays & 0xFF) as u8;
    }
    let n = 32;
    let column = |f: fn(&Civ) -> i32, missing: i32| -> Vec<i32> { (0..n).map(|i| civs.list.get(i).map_or(missing, f)).collect() };
    let mut eras = column(|c| c.era, 0);
    // The viewer's era (fortresses, colonies): the scenario's latest.
    eras[player as usize] = civs.list.iter().map(|c| c.era).max().unwrap_or(0).clamp(0, 3);
    let race = |r: i32| usize::try_from(r).ok().and_then(|r| rules.races.get(r));
    Civ3World {
        player,
        resource_icons: rules.resources.iter().map(|r| r.icon).collect(),
        resource_visible: vec![true; rules.resources.len()],
        eras,
        color_tables: column(|c| c.color, 0),
        capitals,
        races: column(|c| c.race, -1),
        culture_groups: rules.races.iter().map(|r| r.culture_group).collect(),
        max_town: rules.max_town,
        max_city: rules.max_city,
        civ_adjectives: (0..n).map(|i| civs.list.get(i).and_then(|c| race(c.race)).map_or(String::new(), |r| r.adjective.clone())).collect(),
        governments: column(|c| c.government, -1),
        wall_buildings: rules
            .buildings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.bombard_defence > 0)
            .map(|(id, b)| WallBuilding { id: id as i32, government: b.government, obsolete_for: 0 })
            .collect(),
        icon_buildings: rules
            .buildings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.flags & civ3_sys::IconBuilding::FLAGS != 0)
            .map(|(id, b)| IconBuilding { id: id as i32, flags: b.flags & civ3_sys::IconBuilding::FLAGS, obsolete_for: 0 })
            .collect(),
        cities,
        outposts,
        units,
        unit_type_keys: rules.unit_types.iter().map(|u| u.key.clone()).collect(),
        era_keys: rules.eras.iter().map(|e| e.key.clone()).collect(),
        // (No unit statistics: the units show without health bars, LEDs or
        // stack lines, which keep their size in a world shown small.)
        width: map.width,
        height: map.height,
        wrap_x: map.flags & 1 != 0,
        wrap_y: map.flags & 2 != 0,
        seed: map.seed as u32,
        flags: map.flags,
        tiles,
        ..Default::default()
    }
}

/// A scenario the Load Scenario screen loaded for its preview.
#[derive(Clone, Debug, PartialEq)]
pub struct Selected {
    /// How many it has loaded (a new number whenever it loads one, also the
    /// same again).
    pub number: u64,
    pub file: Option<String>,
    /// When the game was done loading it (what the times in the log count
    /// from).
    pub at: Option<std::time::Instant>,
}

/// Written only by the game's thread (its Load Scenario hook).
static SELECTED: handoff::Latest<Selected> = handoff::Latest::new();

/// The next selection after the latest, of `file`, loaded now.
fn select(file: Option<String>) -> Selected {
    let number = SELECTED.load().map_or(0, |s| s.number) + 1;
    let s = Selected { number, file, at: Some(std::time::Instant::now()) };
    SELECTED.publish(s.clone());
    s
}

/// The file the Load Scenario screen last highlighted.
pub fn selected() -> Selected {
    SELECTED.get().unwrap_or(Selected { number: 0, file: None, at: None })
}

/// Follows the Load Scenario screen's preview loads
/// (`civ3_sys::menus::scenario_preview`); the hook stays for the life of
/// the process.
#[cfg(windows)]
pub fn install(game: &civ3_sys::Civ3) {
    use civ3_sys::menus;
    let Some((site, bytes)) = menus::scenario_preview_site(game.build) else {
        return log::info!("scenario preview: not known for this build");
    };
    let hook = menus::scenario_preview::install_call_site(site, &bytes, |call| {
        let started = std::time::Instant::now();
        let result = call.original();
        let name = inproc::Ptr32::<u8>::new(call.path).read_array(260).ok().map(|b| b.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect::<String>());
        log::info!("scenario preview: the game loaded {name:?} in {} ms ({result})", started.elapsed().as_millis());
        let s = select(name.filter(|n| !n.is_empty()));
        let job = Job { number: s.number, file: s.file };
        // (The preview thread starts on it at once, if it runs.)
        ask_now(job);
        result
    });
    match hook {
        Ok(h) => std::mem::forget(h),
        Err(e) => log::warn!("scenario preview: not following the Load Scenario screen: {e}"),
    }
}

/// A preview made ready on the preview thread ([`Previews`]): the world,
/// its city labels and, once resolved in the scenario's own art folders,
/// the unit art it needs.
#[derive(Clone)]
pub struct Current {
    /// The selection it is for (`Selected::number`).
    pub number: u64,
    pub preview: Preview,
    pub labels: Arc<LabelItems>,
    /// The art its units need, in its art folders. The optional world stays
    /// hidden until this art and its labels are available in adopted assets.
    pub needed: Option<Arc<crate::units::Needed>>,
}

impl Current {
    /// Its art folders.
    pub fn folders(&self) -> (Option<String>, Vec<String>) {
        (self.preview.scenario_folder.clone(), self.preview.search_folders.clone())
    }

    /// Whether the resolved preview art exists in adopted assets. The caller
    /// also checks that those assets were compiled for this scenario's folders.
    /// Missing optional art never delays the menu or the available preview terrain.
    pub fn art_ready(&self, compiled: &render_desc::Compiled, fonts: &crate::text::Fonts) -> bool {
        let Some(needed) = &self.needed else { return false };
        let units = needed.files.iter().all(|(path, (file, colors))| {
            let frame_ready = |k| {
                compiled.set_available(&format!("UnitShadow@{path}/{k}"))
                    && compiled.set_available(&format!("UnitSmoke@{path}/{k}"))
                    && colors.iter().all(|color| {
                        let kind = color.art_kind();
                        compiled.set_available(&format!("UnitArt@{path}{kind}/{k}")) && compiled.palette(&color.palette()).is_some()
                    })
            };
            match needed.stills.get(path) {
                Some(frames) => frames.iter().copied().all(frame_ready),
                None => (0..file.chunks).all(frame_ready),
            }
        });
        let plain = needed.plain.iter().all(|(path, file)| (0..file.chunks).all(|k| compiled.set_available(&format!("Flc@{path}/{k}"))));
        let effects = needed.effects.iter().all(|(path, file)| (0..file.chunks).all(|k| {
            compiled.set_available(&format!("EffectArt@{path}/{k}")) && compiled.set_available(&format!("EffectShadow@{path}/{k}")) && compiled.set_available(&format!("EffectSmoke@{path}/{k}"))
        }));
        units && plain && effects && self.labels_ready(compiled, fonts)
    }

    fn labels_ready(&self, compiled: &render_desc::Compiled, fonts: &crate::text::Fonts) -> bool {
        crate::ui_screen::readiness(self.labels.iter().flat_map(|(_, items)| items.iter()), fonts, compiled, false) == crate::ui_screen::Readiness::Ready
    }

    /// What of it the screen draws now (`own_art`: the art of its own
    /// folders is being made, see `live`). The world goes out as soon as
    /// it is made, in whatever art is there (the previous scenario's or the
    /// game's terrain); nothing waits for art. Its labels once their glyphs
    /// are there; its units once their art is resolved in its folders (until
    /// then, and where their sprites are not made yet, nothing in their
    /// place). Each part comes in as its art lands.
    pub fn drawn(&self, own_art: bool, compiled: &render_desc::Compiled, fonts: &crate::text::Fonts) -> Drawn {
        let units = own_art && self.needed.is_some();
        Drawn { labels: units && !self.labels.is_empty() && self.labels_ready(compiled, fonts), units }
    }
}

/// What of a preview the screen draws ([`Current::drawn`]): the world
/// always, its labels and its units once there.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Drawn {
    pub labels: bool,
    pub units: bool,
}

/// What the preview thread sends back.
enum Made {
    /// The world first, for preparing its labels and art before publication.
    World { number: u64, result: Result<Preview, String>, took: std::time::Duration },
    /// Then its city labels (their text measured).
    Labels { number: u64, labels: LabelItems, took: std::time::Duration },
    /// Then the unit art it needs, resolved in its art folders.
    Art { number: u64, art: Box<crate::units::UnitArt>, needed: crate::units::Needed, took: std::time::Duration },
}

/// A scenario to make ready.
struct Job {
    number: u64,
    file: Option<String>,
}

impl Job {
    /// Hook and publisher enqueue concurrently; arrival order is not selection order.
    fn newest(mut self, queued: &mut tokio::sync::mpsc::UnboundedReceiver<Job>) -> Self {
        while let Ok(job) = queued.try_recv() {
            if job.number >= self.number { self = job; }
        }
        self
    }
}

/// Makes the preview of `job` (in `install`): the world first, then its
/// art; stops as soon as `superseded` says a newer selection came (only the
/// latest matters), sending nothing more. Each stage is sent as it is done.
fn make(install: &Path, job: &Job, superseded: &dyn Fn() -> bool, send: &mut dyn FnMut(Made)) {
    let started = std::time::Instant::now();
    let number = job.number;
    let Some(file) = &job.file else {
        return send(Made::World { number, result: Err("no file".into()), took: started.elapsed() });
    };
    let preview = match load(install, file) {
        Ok(p) => p,
        Err(e) => return send(Made::World { number, result: Err(e.to_string()), took: started.elapsed() }),
    };
    if superseded() {
        return;
    }
    let (folder, search) = (preview.scenario_folder.clone(), preview.search_folders.clone());
    let files = || crate::assets::game_files(install, folder.as_deref(), &search);
    let world = preview.world.clone();
    send(Made::World { number, result: Ok(preview), took: started.elapsed() });
    if superseded() {
        return;
    }
    let mut env = crate::city_labels::SystemTextEnv::new(crate::city_labels::FileEnv::new(Box::new(files())));
    let labels = label_items(&world, &mut env);
    send(Made::Labels { number, labels, took: started.elapsed() });
    if superseded() {
        return;
    }
    let mut art = crate::units::UnitArt::new(files());
    let needed = crate::units::Needed::still(&world, &mut art);
    if superseded() {
        return;
    }
    send(Made::Art { number, art: Box::new(art), needed, took: started.elapsed() });
}

/// How the Load Scenario hook asks the preview thread for the scenario the
/// game just loaded (once the thread runs): its latest job and its queue.
#[allow(clippy::type_complexity)]
static ASKER: handoff::Latest<(Arc<std::sync::atomic::AtomicU64>, tokio::sync::mpsc::UnboundedSender<Job>)> = handoff::Latest::new();

/// Asks the preview thread for `job` at once (none running: the publisher
/// asks when it sees the selection).
#[cfg(windows)]
fn ask_now(job: Job) {
    if let Some(asker) = ASKER.load() {
        let (latest, jobs) = &*asker;
        // A publisher snapshot can lag a newer native hook. Never let its
        // submission cancel the newer selection that is already running.
        if latest.fetch_max(job.number, std::sync::atomic::Ordering::AcqRel) <= job.number {
            let _ = jobs.send(job);
        }
    }
}

/// The thread making previews: the latest job only.
struct Worker {
    latest: Arc<std::sync::atomic::AtomicU64>,
    // (tokio: the game thread's send never touches a lock, even with the
    // worker parked in its receive.)
    jobs: tokio::sync::mpsc::UnboundedSender<Job>,
    made: std::sync::mpsc::Receiver<Made>,
}

impl Worker {
    fn start(install: PathBuf) -> std::io::Result<Worker> {
        let latest = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let (jobs, mut job_rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
        let (made_tx, made) = std::sync::mpsc::channel();
        let newest = latest.clone();
        std::thread::Builder::new().name("civ3r-preview".into()).spawn(move || {
            // (Bounded sleeps, not a parked receive: `crate::BOUNDED_WAIT`.)
            while let Some(mut job) = crate::recv_bounded(&mut job_rx, std::time::Duration::from_millis(5)) {
                // (Of those waiting, the latest.)
                job = job.newest(&mut job_rx);
                let superseded = || newest.load(std::sync::atomic::Ordering::Acquire) != job.number;
                if superseded() {
                    continue;
                }
                let mut gone = false;
                make(&install, &job, &superseded, &mut |m| {
                    gone |= made_tx.send(m).is_err();
                    // (The publisher shows it at once.)
                    crate::presenter::WAKE.ring();
                });
                if gone {
                    break;
                }
            }
        })?;
        ASKER.publish((latest.clone(), jobs.clone()));
        Ok(Worker { latest, jobs, made })
    }

    fn ask(&self, job: Job) -> bool {
        if self.latest.fetch_max(job.number, std::sync::atomic::Ordering::AcqRel) > job.number {
            return true;
        }
        self.jobs.send(job).is_ok()
    }
}

/// The scenario previewed: the one the Load Scenario screen last loaded,
/// made ready on a thread of its own (`civ3r-preview`): reading the file,
/// making its world and labels, resolving its unit art. The publisher only
/// asks and takes what is ready, never waits. A newer selection clears the
/// optional world and supersedes what is being made. Closing the screen
/// discards staged results and cancels work between stages.
#[derive(Default)]
pub struct Previews {
    /// Open menu status is separate from native generation0 before its first hook.
    active: bool,
    worker: Option<Worker>,
    /// The selection asked for, and the latest preview made for it.
    asked: u64,
    current: Option<Current>,
    /// Its unit art, to be taken (`take_art`).
    art: Option<Box<crate::units::UnitArt>>,
    /// What came while the publisher waited ([`Previews::wait`]), not taken
    /// yet.
    early: Vec<Made>,
    /// For the log: when the game loaded the scenario asked for, whether
    /// its world was published yet, and the art request
    /// (`ArtSchedule::asked`) whose build brings its own art.
    loaded_at: Option<std::time::Instant>,
    published: bool,
    /// Whether a scene with the menu showing the selection asked for went
    /// out ([`Previews::menu_published`]).
    menu_out: bool,
    art_from: Option<u64>,
}

impl Previews {
    /// The world of the scenario the screen shows now, as far as it is
    /// ready (`None`: none, or one without a map of its own).
    pub fn update(&mut self, install: &Path) -> Option<&Current> {
        let selection = selected();
        self.select(&selection);
        self.receive();
        // A failed worker must not leave a closed sender installed forever.
        // Retry the current selection too, not just the next native click.
        if self.asked != 0 {
            if self.worker.is_none() {
                match Worker::start(install.to_path_buf()) {
                    Ok(w) => self.worker = Some(w),
                    Err(e) => log::error!("scenario preview: no thread for it: {e}"),
                }
            }
            if let Some(w) = self.worker.as_ref().filter(|w| w.latest.load(std::sync::atomic::Ordering::Acquire) != selection.number) {
                if !w.ask(Job { number: selection.number, file: selection.file }) {
                    log::warn!("scenario preview: worker closed, retrying the current selection");
                    self.worker = None;
                }
            }
        }
        self.current.as_ref()
    }

    /// Drain only this worker's stages. A disconnected worker is retired;
    /// the next update starts a fresh channel and resubmits the selection.
    fn receive(&mut self) {
        for made in std::mem::take(&mut self.early) { self.take(made); }
        loop {
            let Some(worker) = &self.worker else { break };
            match worker.made.try_recv() {
                Ok(made) => self.take(made),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::warn!("scenario preview: worker exited, retrying the current selection");
                    self.worker = None;
                    break;
                }
            }
        }
    }

    fn select(&mut self, selection: &Selected) -> bool {
        self.active = true;
        if selection.number == self.asked {
            return false;
        }
        self.asked = selection.number;
        self.current = None;
        self.art = None;
        (self.loaded_at, self.published, self.art_from, self.menu_out) = (selection.at, false, None, false);
        true
    }

    /// The screen closed. No retained preview or late worker reply may
    /// bring its optional world back over the title or another menu.
    pub fn close(&mut self) {
        self.active = false;
        if let Some(worker) = &self.worker {
            worker.latest.store(0, std::sync::atomic::Ordering::Release);
            while worker.made.try_recv().is_ok() {}
        }
        self.asked = 0;
        self.current = None;
        self.art = None;
        self.early.clear();
        (self.loaded_at, self.published, self.art_from, self.menu_out) = (None, false, None, false);
    }

    fn take(&mut self, made: Made) {
        let number = match &made { Made::World { number, .. } | Made::Labels { number, .. } | Made::Art { number, .. } => *number };
        if !self.active || number < self.asked { return; }
        // The native hook can start and finish a newer selection while the
        // publisher still holds an older selected() snapshot. Keep those
        // stages until the next update observes that selection. Dropping
        // them loses the only result, since Worker.latest already matches.
        if number > self.asked {
            let queued = self.early.first().map(|made| match made { Made::World { number, .. } | Made::Labels { number, .. } | Made::Art { number, .. } => *number });
            if queued.is_some_and(|queued| queued > number) { return; }
            if queued.is_some_and(|queued| queued < number) { self.early.clear(); }
            log::debug!("scenario preview: staging generation {number} until publisher observes it (currently {})", self.asked);
            self.early.push(made);
            return;
        }
        match made {
            Made::World { number, result, took } => {
                self.art = None;
                self.current = match result {
                    Ok(preview) => {
                        let w = &preview.world;
                        log::info!(
                            "scenario preview: {}: {}x{} map, {} cities, {} units shown, area {:?}, art {:?} {:?}; made in {} ms, {} ms after the game loaded it",
                            preview.file,
                            w.width,
                            w.height,
                            w.cities.len(),
                            w.units.len(),
                            preview.area,
                            preview.scenario_folder,
                            preview.search_folders,
                            took.as_millis(),
                            self.since_loaded()
                        );
                        Some(Current { number, preview, labels: Arc::default(), needed: None })
                    }
                    Err(e) => {
                        log::info!("scenario preview: {e}");
                        None
                    }
                };
            }
            Made::Labels { labels, took, .. } => {
                if let Some(c) = &mut self.current {
                    log::info!("scenario preview: {}: {} city labels in {} ms", c.preview.file, labels.len(), took.as_millis());
                    c.labels = Arc::new(labels);
                }
            }
            Made::Art { art, needed, took, .. } => {
                if let Some(c) = &mut self.current {
                    log::info!("scenario preview: {}: unit art resolved ({} files) in {} ms", c.preview.file, needed.files.len(), took.as_millis());
                    c.needed = Some(Arc::new(needed));
                    self.art = Some(art);
                }
            }
        }
    }

    /// Waits at most `timeout` for something to show (the publisher's pause
    /// between its rounds): it goes on at once when a preview stage is
    /// ready (the preview thread rings `presenter::WAKE`), as when the game
    /// presents a new frame or input arrives.
    pub fn wait(&mut self, timeout: std::time::Duration) {
        crate::presenter::WAKE.wait(timeout);
    }

    /// The unit art the preview thread resolved for the preview shown, in
    /// its art folders, once (the publisher's from then on).
    pub fn take_art(&mut self) -> Option<crate::units::UnitArt> {
        self.art.take().map(|a| *a)
    }

    /// Whether the preview shown is still waiting for its unit art (until
    /// then the publisher reads none: the thread making it does).
    pub fn art_pending(&self) -> bool {
        self.current.as_ref().is_some_and(|c| c.needed.is_none())
    }

    /// How long since the game loaded the scenario shown, in milliseconds.
    fn since_loaded(&self) -> u128 {
        self.loaded_at.map_or(0, |t| t.elapsed().as_millis())
    }

    /// A scene with the screen's menu went out (with whatever preview).
    pub fn menu_published(&mut self) {
        self.menu_out = true;
    }

    /// The selection asked for (`Selected::number`).
    pub fn asked(&self) -> u64 {
        self.asked
    }

    /// Whether the menu showing the selection asked for went out.
    pub fn menu_shown(&self) -> bool {
        self.menu_out
    }

    /// Whether the selection asked for was made at least `rest` ago.
    pub fn rested(&self, rest: std::time::Duration) -> bool {
        self.loaded_at.is_none_or(|t| t.elapsed() >= rest)
    }

    /// A scene with the scenario's world went out (scene `seq`).
    pub fn published(&mut self, seq: u64) {
        if let (Some(c), false) = (&self.current, self.published) {
            if c.number == self.asked {
                self.published = true;
                log::info!("scenario preview: {} first published {} ms after the game loaded it (scene {seq})", c.preview.file, self.since_loaded());
            }
        }
    }

    /// Art was asked for the scenario shown: request `asked` and later
    /// bring it.
    pub fn art_wanted(&mut self, asked: u64) {
        if self.current.is_some() {
            self.art_from.get_or_insert(asked);
        }
    }

    /// Art covering request `token` is used now.
    pub fn art_done(&mut self, token: u64) {
        if let (Some(c), Some(from)) = (&self.current, self.art_from) {
            if token >= from {
                self.art_from = None;
                log::info!("scenario preview: {}'s own art used {} ms after the game loaded it", c.preview.file, self.since_loaded());
            }
        }
    }
}

/// The Load Scenario screen's rectangle (x, y, w, h), while `frame` shows it.
pub fn screen(frame: &crate::ui::Frame, build: civ3_sys::Build) -> Option<[i32; 4]> {
    let form = civ3_sys::menus::load_scenario_form(build)?;
    let (_, _, (x, y), (w, h)) = *frame.forms.iter().rev().find(|f| f.0 == form)?;
    Some([x, y, w, h])
}

/// The game's view of a preview world: all of it (the viewer fits it
/// beside the screen, `scene::Beside`).
pub fn view(world: &Civ3World) -> crate::world::Civ3View {
    crate::world::Civ3View { camera_x: 0, camera_y: 0, tile_x: (-2, world.width + 2), tile_y: (-2, world.height + 2), zoomed_out: false }
}

/// How many times smaller a preview world is shown (`View::downscale`) in
/// `room` (pixels): a power of two (the map's 64×32 grid steps divide by it)
/// at which its `area` (x, y, w, h, world pixels; [`Preview::area`]) is
/// nearest to filling the room either way (the viewer zooms the rest,
/// `scene::Beside`). The city labels are laid out for it ([`place_label`]).
pub fn downscale(area: [i32; 4], room: (i32, i32)) -> u32 {
    let (w, h) = (area[2].max(1) as f64, area[3].max(1) as f64);
    let (rw, rh) = (room.0.max(1) as f64, room.1.max(1) as f64);
    // The zoom that fits the area shown n times smaller, and how far from 1.
    let off = |n: u32| {
        let z = (rw / (w / n as f64)).min(rh / (h / n as f64));
        if z >= 1.0 { z } else { 1.0 / z }
    };
    [1, 2, 4, 8, 16, 32].into_iter().min_by(|&a, &b| off(a).total_cmp(&off(b))).unwrap_or(1)
}

/// Places the sprites of a city label laid out at full size for the city
/// of tile (x, y) so that it keeps its size beside the city in a world shown
/// `n` times smaller (the renderer divides world positions by `n`): moved
/// with the tile's bottom corner, its parts as far from it as before.
pub fn place_label(sprites: &mut [scene::FreeSprite], (x, y): (i32, i32), n: u32) {
    let n = n.max(1) as i32;
    let anchor = (64 * x + 64, 32 * y + 64);
    for s in sprites.iter_mut().filter(|s| !s.screen) {
        s.x = anchor.0 + (s.x - anchor.0) * n;
        s.y = anchor.1 + (s.y - anchor.1) * n;
    }
}

/// Whether a preview's art is still coming (`ready`: all of it there):
/// its unit art still resolved on its thread (`resolving`), its own art
/// not asked for yet (`asking`: the menu not shown or rested yet, or what
/// it needs not in the art), or art being built or adopted (`building`).
/// Art that never arrives (left out, failed) does not keep it loading.
pub fn loading(ready: bool, resolving: bool, asking: bool, building: bool) -> bool {
    !ready && (resolving || asking || building)
}

/// The set of the preview's loading indicator: the wait cursor the game
/// shows (the system's `IDC_WAIT`, as `civ3_assets::syscursor` reads it;
/// the game has no wait cursor art) 8x as large.
pub const LOADING_SET: &str = "Viewer@loading/0";

/// The loading indicator over a preview world whose art is still coming:
/// in the middle of its `area` (world pixels at full size, the part the
/// viewer fits beside the screen), over all of the world (under the
/// menus, which are views of their own), at its own size in a world shown
/// smaller (the set keeps its size, centred on its anchor). None without
/// its art in the description.
pub fn loading_indicator(area: [i32; 4], compiled: &render_desc::Compiled) -> Option<scene::FreeSprite> {
    let (set, _) = compiled.set(LOADING_SET)?;
    Some(scene::FreeSprite { set: set.raw(), x: area[0] + area[2] / 2, y: area[1] + area[3] / 2, after_pass: u32::MAX, ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The indicator shows while any of the preview's art is still coming
    /// and goes once it is all there, or once nothing more can come.
    #[test]
    fn a_preview_loads_until_its_art_is_there_or_nothing_more_comes() {
        // Its units' art resolved on its thread, its art not asked for yet,
        // or being built: loading.
        assert!(loading(false, true, false, false));
        assert!(loading(false, false, true, false));
        assert!(loading(false, false, false, true));
        // All there: done, whatever else is being built.
        assert!(!loading(true, false, false, true));
        // Missing optional art after the builds ended: not loading forever.
        assert!(!loading(false, false, false, false));
    }

    /// The loading indicator: the wait cursor, 256x256 (32x32 at 8x,
    /// nearest pixel) centred on its anchor, in the middle of the area
    /// shown, on top of the world; none without its art.
    #[test]
    fn the_loading_indicator_is_the_wait_cursor_8x_in_the_middle() {
        let Some(install) = install() else { return eprintln!("no Civ III install, skipped") };
        let compiled = crate::assets::compile(crate::assets::BUILTIN_DESC, crate::assets::game_files(&install, None, &[])).unwrap();
        let s = loading_indicator([100, 40, 800, 600], &compiled).unwrap();
        assert_eq!((s.x, s.y, s.screen, s.after_pass, s.category), (500, 340, false, u32::MAX, 0));
        let id = compiled.sets.frame(compiled.set(LOADING_SET).unwrap().0, 0).unwrap();
        let sprite = compiled.atlas.sprites[id.0 as usize];
        assert_eq!((sprite.width, sprite.height, sprite.anchor_x, sprite.anchor_y), (256, 256, -128, -128));
        assert_eq!(sprite.scaling, tile_render::KEEP_SIZE);
        // The cursor cell's pixels, each an 8x8 block (an integer factor:
        // nearest pixel is exact).
        let page = &compiled.atlas.pages[sprite.page as usize];
        let at = |x: u32, y: u32| page.get(sprite.atlas_x + x, sprite.atlas_y + y);
        let opaque = (0..256u32).flat_map(|y| (0..256u32).map(move |x| (x, y))).filter(|&(x, y)| {
            assert_eq!(at(x, y), at(x / 8 * 8, y / 8 * 8), "pixel ({x}, {y})");
            at(x, y)[3] != 0
        }).count();
        // (Its pixels where the system has the cursor: Wine's hourglass.)
        let files = crate::assets::game_files(&install, None, &[]);
        if civ3_assets::syscursor::find(&civ3_assets::syscursor::user32_candidates(files.roots()), 32514, 32).is_ok() {
            assert!(opaque > 64 * 100, "the hourglass: {opaque} pixels");
        }
    }

    #[test]
    fn a_label_keeps_its_size_beside_its_city_in_a_world_shown_smaller() {
        let mut s = vec![scene::FreeSprite { x: 64 * 7 + 64 - 30, y: 32 * 25 + 66, ..Default::default() }];
        place_label(&mut s, (7, 25), 8);
        // Divided by 8 by the renderer: 30 left of the tile's bottom corner,
        // 2 below it, as at full size.
        let (cx, cy) = ((64 * 7 + 64) / 8, (32 * 25 + 64) / 8);
        assert_eq!((s[0].x / 8, s[0].y / 8), (cx - 30, cy + 2));
    }

    fn made_world(number: u64) -> Made {
        Made::World {
            number,
            result: Ok(Preview {
                file: format!("{number}.biq"), world: Arc::default(), scenario_folder: None, search_folders: Vec::new(), area: [0, 0, 128, 64],
            }),
            took: std::time::Duration::ZERO,
        }
    }

    fn made_art(number: u64) -> Made {
        Made::Art {
            number, art: Box::new(crate::units::UnitArt::new(civ3_assets::GameFiles::with_roots(Vec::new()))),
            needed: Default::default(), took: std::time::Duration::ZERO,
        }
    }

    fn selection(number: u64) -> Selected {
        Selected { number, file: Some(format!("{number}.biq")), at: Some(std::time::Instant::now()) }
    }

    #[test]
    fn late_stale_enqueue_cannot_discard_the_newest_queued_scenario() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tx.send(Job { number: 3, file: selection(3).file }).unwrap();
        tx.send(Job { number: 2, file: selection(2).file }).unwrap();
        let picked = Job { number: 1, file: selection(1).file }.newest(&mut rx);
        assert_eq!(picked.number, 3, "selection generation, not enqueue arrival, defines latest");
    }

    #[test]
    fn stale_publisher_submission_never_cancels_newer_native_selection() {
        let (jobs, mut jobs_rx) = tokio::sync::mpsc::unbounded_channel();
        let (_made_tx, made) = std::sync::mpsc::channel();
        let latest = Arc::new(std::sync::atomic::AtomicU64::new(3));
        let worker = Worker { latest: latest.clone(), jobs, made };
        assert!(worker.ask(Job { number: 2, file: selection(2).file }));
        assert_eq!(latest.load(std::sync::atomic::Ordering::Acquire), 3);
        assert!(jobs_rx.try_recv().is_err(), "stale A/B jobs must not overtake native C");
        assert!(worker.ask(Job { number: 4, file: selection(4).file }));
        assert_eq!(jobs_rx.try_recv().unwrap().number, 4);
        drop(jobs_rx);
        assert!(!worker.ask(Job { number: 5, file: selection(5).file }), "failed worker submission must be observable");
    }

    #[test]
    fn future_stage_storage_keeps_only_latest_generation() {
        let mut previews = Previews::default(); previews.select(&selection(1));
        previews.take(made_world(2)); previews.take(made_art(2));
        previews.take(made_world(3)); previews.take(made_art(2)); previews.take(made_art(3));
        assert_eq!(previews.early.len(), 2, "future scenario worlds must not accumulate while publisher lags");
        previews.select(&selection(3)); previews.receive();
        assert_eq!(previews.current.as_ref().unwrap().number, 3);
        assert!(previews.take_art().is_some());
    }

    #[test]
    fn first_hook_result_survives_open_menu_with_generation_zero_snapshot() {
        let mut previews = Previews::default();
        assert!(!previews.select(&Selected { number: 0, file: None, at: None }));
        previews.take(made_world(1)); previews.take(made_art(1));
        previews.select(&selection(1)); previews.receive();
        assert_eq!(previews.current.as_ref().unwrap().number, 1);
        assert!(previews.take_art().is_some());
        previews.close();
        previews.take(made_world(2));
        assert!(previews.early.is_empty() && previews.current.is_none(), "closed menus reject even future generations");
    }

    #[test]
    fn hook_can_finish_newer_selection_before_publisher_samples_it() {
        let mut previews = Previews::default();
        previews.select(&selection(1));
        // The hook has already selected B and its worker finished. The
        // publisher still holds A's selected() snapshot for this round.
        previews.take(made_world(2)); previews.take(made_art(2));
        assert!(previews.current.is_none());
        previews.select(&selection(2));
        previews.receive();
        assert_eq!(previews.current.as_ref().map(|c| c.number), Some(2), "future stages must survive until publisher observes the native selection");
        assert!(previews.take_art().is_some());
    }

    #[test]
    fn disconnected_worker_is_retired_so_the_same_selection_can_restart() {
        let (jobs, jobs_rx) = tokio::sync::mpsc::unbounded_channel();
        let (made_tx, made) = std::sync::mpsc::channel();
        let latest = Arc::new(std::sync::atomic::AtomicU64::new(7));
        let mut previews = Previews { worker: Some(Worker { latest, jobs, made }), ..Default::default() };
        previews.select(&selection(7));
        previews.take(made_world(7));
        drop(jobs_rx);
        drop(made_tx); // A failed/exited worker, including cancellation failure.
        previews.receive();
        assert!(previews.worker.is_none(), "closed worker must be restartable without another selection or game restart");
        assert!(!previews.select(&selection(7)), "native selection itself has not changed");
        // A replacement owns a fresh result channel. It can finish this same
        // selection even if the previous worker failed after its World stage.
        let (jobs, _jobs_rx) = tokio::sync::mpsc::unbounded_channel();
        let (made_tx, made) = std::sync::mpsc::channel();
        previews.worker = Some(Worker { latest: Arc::new(std::sync::atomic::AtomicU64::new(0)), jobs, made });
        assert!(previews.worker.as_ref().unwrap().ask(Job { number: 7, file: selection(7).file }));
        made_tx.send(made_world(7)).unwrap(); made_tx.send(made_art(7)).unwrap();
        previews.receive();
        assert_eq!(previews.current.as_ref().unwrap().number, 7);
        assert!(!previews.art_pending());
        assert!(previews.take_art().is_some());
    }

    #[test]
    fn rapid_abc_out_of_order_stages_revisits_failure_and_title_resume_recover() {
        let mut previews = Previews::default();
        // A, B and C are highlighted before any worker stage finishes.
        for number in 1..=3 { assert!(previews.select(&selection(number))); }
        previews.take(made_world(2)); previews.take(made_art(2));
        previews.take(made_world(1));
        assert!(previews.current.is_none());
        previews.take(made_world(3));
        previews.take(made_art(1)); // Cold A art arrives after C's world.
        assert!(previews.art_pending());
        previews.take(made_art(3));
        assert_eq!(previews.current.as_ref().unwrap().number, 3);
        assert!(previews.take_art().is_some());
        // Revisit the same file, with a fresh native selection generation.
        let mut revisit = selection(4); revisit.file = selection(1).file;
        previews.select(&revisit);
        previews.take(Made::World { number: 4, result: Err("canceled/read failed".into()), took: std::time::Duration::ZERO });
        assert!(previews.current.is_none());
        let mut retry = selection(5); retry.file = revisit.file;
        previews.select(&retry);
        previews.take(made_art(3)); previews.take(made_world(4));
        previews.take(made_world(5)); previews.take(made_art(5));
        assert_eq!(previews.current.as_ref().unwrap().number, 5);
        assert!(!previews.art_pending());
        previews.close(); // Return to title while optional work is late.
        previews.take(made_world(5)); previews.take(made_art(5));
        assert!(previews.current.is_none() && previews.take_art().is_none());
        assert!(previews.select(&retry)); // Resume without another native load.
        previews.take(made_world(5)); previews.take(made_art(5));
        assert_eq!(previews.current.as_ref().unwrap().number, 5);
        assert!(previews.take_art().is_some());
    }

    #[test]
    fn a_pending_or_failed_selection_cannot_keep_an_older_preview_or_its_art() {
        let mut previews = Previews::default();
        assert!(previews.select(&selection(1)));
        previews.take(made_world(1));
        previews.take(made_art(1));
        assert!(previews.current.is_some() && previews.art.is_some());
        assert!(previews.select(&selection(2)));
        assert!(previews.current.is_none() && previews.art.is_none());
        previews.take(made_world(1));
        previews.take(made_art(1));
        assert!(previews.current.is_none() && previews.art.is_none(), "late stages belong to the old selection");
        previews.take(Made::World { number: 2, result: Err("no map".into()), took: std::time::Duration::ZERO });
        assert!(previews.current.is_none() && previews.art.is_none());
        previews.select(&selection(3));
        previews.take(made_world(3));
        assert_eq!(previews.current.as_ref().map(|c| c.number), Some(3));
        assert!(previews.art_pending());
    }

    #[test]
    fn closing_the_scenario_menu_cancels_work_and_rejects_late_stages() {
        let (jobs, _jobs_rx) = tokio::sync::mpsc::unbounded_channel();
        let (made_tx, made) = std::sync::mpsc::channel();
        let latest = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let worker = Worker { latest: latest.clone(), jobs, made };
        let mut previews = Previews { worker: Some(worker), ..Default::default() };
        previews.select(&selection(1));
        previews.take(made_world(1));
        previews.take(made_art(1));
        previews.early.push(made_world(1));
        made_tx.send(made_art(1)).unwrap();
        previews.close();
        assert_eq!(latest.load(std::sync::atomic::Ordering::Acquire), 0);
        assert!(previews.early.is_empty() && previews.current.is_none() && previews.take_art().is_none());
        assert!(previews.worker.as_ref().unwrap().made.try_recv().is_err());
        previews.take(made_world(1));
        previews.take(made_art(1));
        assert!(previews.current.is_none() && previews.take_art().is_none());
        assert!(previews.select(&selection(1)), "reopening may ask for the same native selection again");
    }

    #[test]
    fn optional_preview_art_can_remain_unavailable_while_the_menu_is_complete() {
        use crate::ui_screen::{Item, Publication};
        struct Images;
        impl render_desc::ImageLoader for Images {
            fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
                Ok(vec![tile_render::RgbaImage::from_fn(1, 1, |_, _| [255; 4])])
            }
            fn palette(&self, _: &str, _: &toml::Table) -> Result<Vec<[u8; 4]>, String> {
                Ok(vec![[255; 4]; 256])
            }
        }
        let menu = Item::Element(menu::screen::Element::Fill {
            rect: [0, 0, 32, 32], rgb: [0, 8, 0], alpha: 255, clip: [0, 0, 32, 32],
        }, false);
        let mut screen_art = menu::screen::ScreenArt::default();
        let mut fonts = crate::text::Fonts::default();
        let files = civ3_assets::GameFiles::with_roots(Vec::new());
        crate::ui_screen::add_art([&menu], &mut screen_art, &mut fonts, &files);
        let source = format!("grid = {{ layout = \"orthogonal\", step = [1, 1] }}\n{}", crate::ui_screen::description(&screen_art));
        let (mut compiled, mut growing) = render_desc::Description::parse(&source).unwrap().compile_growing(&Images, 64, false).unwrap();
        let mut previews = Previews::default();
        previews.select(&selection(1));
        previews.take(made_world(1));
        let current = previews.current.as_mut().unwrap();
        assert!(!current.art_ready(&compiled, &fonts), "its art is not ready before art resolution (the world is drawn meanwhile: `drawn`)");
        let file = crate::units::AnimFile { path: "unit.flc".into(), frames_per_direction: 1, chunks: 2, size: (1, 1), anchor: (0, 0) };
        current.needed = Some(Arc::new(crate::units::Needed {
            files: [(file.path.clone(), (file, [crate::units::Recolor::civ(0)].into()))].into(),
            stills: [("unit.flc".into(), [1].into())].into(), ..Default::default()
        }));
        assert!(!current.art_ready(&compiled, &fonts));
        for building in [false, true] {
            assert!(matches!(crate::ui_screen::publication([&menu].into_iter(), &fonts, &compiled, building, (320, 200)), Publication::Complete));
        }
        // A prepared preview has only its selected still frame. Unused frames
        // do not prevent publication, and a failed or missing body does.
        let preview_art = r#"
[palettes."ntp@0"]
source = "solid:255,255,255,255:1x1"
[images.unit]
source = "solid:255,255,255,255:1x1"
[sheets.body]
image = "unit"
cell = [1, 1]
group = "UnitArt@unit.flc"
first_index = 1
[sheets.shadow]
image = "unit"
cell = [1, 1]
group = "UnitShadow@unit.flc"
first_index = 1
[sheets.smoke]
image = "unit"
cell = [1, 1]
group = "UnitSmoke@unit.flc"
first_index = 1
"#;
        let growth = growing.grow(&compiled, preview_art, &Images).unwrap();
        compiled.apply(&growth);
        assert!(current.art_ready(&compiled, &fonts));
        current.labels = Arc::new(vec![((0, 0), vec![Item::Element(menu::screen::Element::Fill {
            rect: [0, 0, 8, 8], rgb: [255, 255, 255], alpha: 255, clip: [0, 0, 8, 8],
        }, false)])]);
        assert!(current.art_ready(&compiled, &fonts), "solid labels need no adopted art");
        current.labels = Arc::new(vec![((0, 0), vec![Item::Element(menu::screen::Element::Sprite {
            name: "missing label".into(), at: (0, 0), clip: [0, 0, 8, 8], scale_den: 0, palette: String::new(),
        }, false)])]);
        assert!(!current.art_ready(&compiled, &fonts), "optional image labels need adopted art too");
        assert!(matches!(crate::ui_screen::publication([&menu].into_iter(), &fonts, &compiled, false, (320, 200)), Publication::Complete));
    }

    fn install() -> Option<PathBuf> {
        let install = PathBuf::from(std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into()));
        install.is_dir().then_some(install)
    }

    /// WWII in the Pacific as the screen previews it (with an installation at
    /// `CIV3_INSTALL`): it reveals the entire map.
    #[test]
    fn a_scenario_that_reveals_its_map_shows_all_of_it() {
        let Some(install) = install() else { return eprintln!("no Civ III install, skipped") };
        let p = load(&install, "Conquests\\9 WWII in the Pacific.biq").unwrap();
        assert_eq!(p.scenario_folder.as_deref(), Some("Conquests\\9 WWII in the Pacific\\"));
        assert_eq!(p.search_folders, ["WWII in the Pacific"]);
        // All of it (the map's pixels, and a tile round them).
        assert_eq!(p.area, [-64, -64, 64 * 136 + 64 + 128, 32 * 120 + 32 + 128]);
        let w = &p.world;
        // Barbarians, five players, and the viewer: the first civ there is not.
        assert_eq!(w.player, 6);
        assert!(w.tiles.iter().all(|t| w.visible_now(t) && t.explored & (1 << w.player) != 0));
        // China is the fourth player (LEAD 3): civ 4, colour table 3, its
        // cities, borders and capital (Chunking has the palace).
        let lanchow = &w.cities[0];
        assert_eq!((lanchow.name.as_str(), lanchow.civ), ("Lanchow", 4));
        assert_eq!(w.color_tables[4], 3);
        assert_eq!(w.tile(7, 25).map(|t| (t.owner, t.city, t.overlays & ROAD)), Some((4, 0, ROAD)));
        assert_eq!(w.capitals[4], 3);
        assert_eq!(w.civ_adjectives[4], "Chinese");
        assert_eq!(w.eras[w.player as usize], 3);
        // The stray tile's tile: a mountain on grassland.
        assert_eq!(w.tile(0, 0).map(|t| (t.real_terrain(), t.terrain())), Some((2, 6)));
        // One unit per tile with units, none selected or active.
        let tiles: std::collections::BTreeSet<(i32, i32)> = w.units.iter().map(|u| (u.x, u.y)).collect();
        assert_eq!(tiles.len(), w.units.len());
        assert!(w.selection.is_none() && w.current_unit.is_none() && w.unit_type_stats.is_empty());
    }

    /// Rise of Rome as the screen previews it: what its players have
    /// explored at the start only, the rest black; the units and labels
    /// there only; the area round what is revealed.
    #[test]
    fn a_scenario_shows_what_its_players_start_with() {
        let Some(install) = install() else { return eprintln!("no Civ III install, skipped") };
        let p = load(&install, "Conquests\\2 Rise of Rome.biq").unwrap();
        let w = &p.world;
        let bit = 1u32 << w.player;
        let explored = w.tiles.iter().filter(|t| t.explored & bit != 0).count();
        // The five civs a human may play only (3178 with the computer's
        // three: `revealed::tests`).
        assert_eq!(explored, 2274);
        assert!(w.tiles.iter().filter(|t| t.explored & bit != 0).all(|t| w.visible_now(t)));
        assert!(w.tiles.iter().filter(|t| t.explored & bit == 0).all(|t| !w.visible_now(t)));
        // Units on tiles seen only.
        assert!(w.units.iter().all(|u| w.tile(u.x, u.y).is_some_and(|t| w.visible_now(t))));
        let mut env = crate::city_labels::FileEnv::new(Box::new(crate::assets::game_files(&install, None, &[])));
        // A label for every city explored (the playable civs'), none for
        // the computer's only civs' unexplored ones; one on a tile not
        // explored none.
        let known = |w: &Civ3World| w.cities.iter().filter(|c| w.tile(c.x, c.y).is_some_and(|t| t.explored & bit != 0)).count();
        assert!(known(w) < w.cities.len());
        assert_eq!(labels(w, &mut env).len(), known(w));
        let mut hidden = (**w).clone();
        let c = w.cities.iter().find(|c| w.tile(c.x, c.y).is_some_and(|t| t.explored & bit != 0)).unwrap();
        let i = hidden.index(c.x, c.y);
        hidden.tiles[i].explored = 0;
        assert_eq!(labels(&hidden, &mut env).len(), known(w) - 1);
        // The area: the tiles revealed (columns 4..139, rows 49..120; with
        // the computer's civs rows 6..134) and a
        // tile around.
        assert_eq!(p.area, [64 * 4 - 64, 32 * 49 - 64, 64 * 134 + 128 + 128, 32 * 71 + 64 + 128]);
    }

    /// The world of a scenario is drawn as soon as it is made: with the
    /// game's own art only (its folders' art, its labels and its units not
    /// grown yet), its cells are there, its units and labels not (nothing
    /// in their place), and its labels come in two layouts once there.
    #[test]
    fn a_preview_world_is_drawn_before_its_art_is_grown() {
        let Some(install) = install() else { return eprintln!("no Civ III install, skipped") };
        let p = load(&install, "Conquests\\2 Rise of Rome.biq").unwrap();
        let mut previews = Previews::default();
        previews.select(&selection(1));
        previews.take(Made::World { number: 1, result: Ok(p), took: std::time::Duration::ZERO });
        let current = previews.current.clone().unwrap();
        assert!(previews.art_pending());
        let compiled = crate::assets::compile(crate::assets::BUILTIN_DESC, crate::assets::game_files(&install, None, &[])).unwrap();
        let fonts = crate::text::Fonts::default();
        assert!(!current.art_ready(&compiled, &fonts));
        assert_eq!(current.drawn(false, &compiled, &fonts), Drawn::default());
        let mapper = crate::scene_map::Mapper::new(&compiled);
        let scene = scene_with(&current.preview.world, 8, &mapper, None, &compiled, &LabelItems::new(), &fonts);
        assert!(scene.shape.cols > 0 && scene.layers.iter().any(|l| l.values.iter().any(|&v| v != 0)));
        assert!(scene.sprites.is_empty(), "no units or labels before their art");
        assert_eq!(scene.view(crate::scene_map::GAME_VIEW).map(|v| v.view.downscale), Some(8));
        // Its unit art resolved for the scenario's folders: the units are
        // drawn as far as their sprites are there.
        let mut art = crate::units::UnitArt::new(crate::assets::game_files(&install, current.preview.scenario_folder.as_deref(), &current.preview.search_folders));
        let with_units = scene_with(&current.preview.world, 8, &mapper, Some(&mut art), &compiled, &LabelItems::new(), &fonts);
        assert_eq!(with_units.layers.len(), scene.layers.len());
    }

    /// What the publisher does for a preview when only the menu changed
    /// (a new highlight): before, the world's scene was made again every
    /// round; now it is kept (`live`'s preview memo). Prints the times
    /// (`cargo test -- --ignored --nocapture preview_timings`).
    #[test]
    #[ignore]
    fn preview_timings() {
        let Some(install) = install() else { return eprintln!("no Civ III install, skipped") };
        let compiled = crate::assets::compile(crate::assets::BUILTIN_DESC, crate::assets::game_files(&install, None, &[])).unwrap();
        let mapper = crate::scene_map::Mapper::new(&compiled);
        let fonts = crate::text::Fonts::default();
        for name in ["1 Mesopotamia", "2 Rise of Rome", "4 Middle Ages", "8 Napoleonic Europe", "9 WWII European Front 6 ", "9 WWII in the Pacific"] {
            let t = std::time::Instant::now();
            let p = load(&install, &format!("Conquests\\{name}.biq")).unwrap();
            let loaded = t.elapsed();
            let n = downscale(p.area, (1280 - 640, 720));
            let t = std::time::Instant::now();
            let made = scene_with(&p.world, n, &mapper, None, &compiled, &LabelItems::new(), &fonts);
            let built = t.elapsed();
            let mut memo: Option<(u64, scene::Scene)> = Some((1, made));
            let t = std::time::Instant::now();
            let kept = memo.as_mut().filter(|(k, _)| *k == 1).map(|(_, s)| s.clone());
            let reused = t.elapsed();
            assert!(kept.is_some());
            // The game thread's part of a selection: a publication and a send.
            let worker = Worker::start(install.clone()).unwrap();
            let t = std::time::Instant::now();
            {
                let s = select(Some(format!("Conquests\\{name}.biq")));
                worker.ask(Job { number: s.number, file: s.file });
            }
            let hook = t.elapsed();
            eprintln!(
                "{name}: worker load {} ms; publisher scene per round before {:.2} ms, kept after {:.3} ms; hook {:.3} ms",
                loaded.as_millis(),
                built.as_secs_f64() * 1e3,
                reused.as_secs_f64() * 1e3,
                hook.as_secs_f64() * 1e3
            );
        }
    }

    #[test]
    fn a_scenario_without_known_starts_draws_black() {
        let Some(install) = install() else { return eprintln!("no Civ III install, skipped") };
        let p = load(&install, "Conquests\\7 Sengoku - Sword of the Shogun.biq").unwrap();
        assert!(p.world.tiles.iter().all(|t| t.explored == 0));
        let files = || crate::assets::game_files(&install, p.scenario_folder.as_deref(), &p.search_folders);
        let compiled = crate::assets::compile(crate::assets::BUILTIN_DESC, files()).unwrap();
        let mut art = crate::units::UnitArt::new(files());
        let scene = scene(&p.world, 8, &crate::scene_map::Mapper::new(&compiled), &mut art, &compiled, &Vec::new(), &Default::default());
        let world = tile_viewer::scene_world(&compiled, &scene);
        // Include the map's top and left edges. Unexplored tall terrain
        // must not leak into the black outside the fog's own tile boxes.
        let camera = tile_render::Camera { x: -32, y: -32, downscale: 8, overhang: crate::scene_map::OVERHANG, ..tile_render::Camera::new((256, 128)) };
        let image = tile_render::reference::render(&world, &compiled.atlas, &camera, 0, [0, 0, 0, 255]);
        assert!(image.as_bytes().chunks_exact(4).all(|p| p == [0, 0, 0, 255]));
    }

    #[test]
    fn the_downscale_fits_the_area_into_the_room() {
        // WWII in the Pacific beside the dialog at 1920×1080: 8 (16 would
        // show it half as big as the room).
        assert_eq!(downscale([0, 0, 64 * 136 + 64, 32 * 120 + 32], (892, 1080)), 8);
        assert_eq!(downscale([0, 0, 704, 352], (892, 1080)), 1);
        assert_eq!(downscale([-100, -100, 64 * 362, 32 * 362], (300, 200)), 32);
        // Part of a big map: as big as that part takes.
        assert_eq!(downscale([6000, 3000, 1800, 2000], (892, 1080)), 2);
    }

    /// Previews made one after the other on their thread, each superseded
    /// by the next while it is made: only the latest comes back, the others
    /// stop where they are.
    #[test]
    fn a_newer_selection_supersedes_the_preview_being_made() {
        let Some(install) = install() else { return eprintln!("no Civ III install, skipped") };
        let job = |number, file: &str| Job { number, file: Some(format!("Conquests\\{file}")) };
        // Superseded at once: nothing.
        let mut made = Vec::new();
        make(&install, &job(1, "2 Rise of Rome.biq"), &|| true, &mut |m| made.push(m));
        assert!(made.is_empty());
        // Superseded after its world: the world only, no labels or art.
        let checks = std::cell::Cell::new(0);
        make(&install, &job(2, "2 Rise of Rome.biq"), &|| { checks.set(checks.get() + 1); checks.get() > 1 }, &mut |m| made.push(m));
        assert!(matches!(made.as_slice(), [Made::World { number: 2, result: Ok(_), .. }]));
        // Not superseded: both.
        made.clear();
        make(&install, &job(3, "9 WWII in the Pacific.biq"), &|| false, &mut |m| made.push(m));
        assert!(matches!(made.as_slice(), [Made::World { number: 3, .. }, Made::Labels { number: 3, .. }, Made::Art { number: 3, .. }]));
        // On the thread: many asked quickly, the latest is what is shown.
        let worker = Worker::start(install.clone()).unwrap();
        let names = ["1 Mesopotamia.biq", "2 Rise of Rome.biq", "3 Fall of Rome.biq", "4 Middle Ages.biq", "9 WWII European Front 6 .biq", "Intro3 New Alliances.biq"];
        for (k, name) in names.iter().enumerate() {
            worker.ask(job(10 + k as u64, name));
        }
        let mut seen = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            match worker.made.recv_timeout(std::time::Duration::from_secs(30)) {
                Ok(Made::World { number, .. }) => seen.push(("world", number)),
                Ok(Made::Labels { number, .. }) => seen.push(("labels", number)),
                Ok(Made::Art { number, .. }) => {
                    seen.push(("art", number));
                    if number == 15 {
                        break;
                    }
                }
                Err(e) => panic!("{e}: {seen:?}"),
            }
            assert!(std::time::Instant::now() < deadline);
        }
        // What came back: the last asked last, with its art; anything older
        // before it, and no art of an older one after the last's world.
        assert_eq!(seen.last(), Some(&("art", 15)));
        let last_world = seen.iter().position(|s| *s == ("world", 15)).unwrap();
        assert!(seen[last_world..].iter().all(|s| s.1 == 15), "{seen:?}");
        assert!(seen.len() < 3 * names.len(), "{seen:?}");
        // And the publisher's side: the selection asked for is the one shown.
        let mut p = Previews { worker: Some(worker), ..Default::default() };
        p.select(&selection(20));
        p.worker.as_ref().unwrap().ask(job(19, "2 Rise of Rome.biq"));
        p.worker.as_ref().unwrap().ask(job(20, "Intro3 New Alliances.biq"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while p.art_pending() || p.current.is_none() {
            if let Ok(m) = p.worker.as_ref().unwrap().made.recv_timeout(std::time::Duration::from_secs(1)) {
                p.take(m);
            }
            assert!(std::time::Instant::now() < deadline);
        }
        assert_eq!(p.current.as_ref().map(|c| (c.number, c.preview.file.as_str())), Some((20, "Conquests\\Intro3 New Alliances.biq")));
        assert!(p.take_art().is_some() && p.take_art().is_none());
    }
}

/// The labels of a preview world's cities: the name only, in the owner's
/// label colour over a shadow, centred below the city (the game's own
/// labels, with their plates and icons, would cover a map shown this small).
/// Laid out at full size like the game's ([`place_label`] keeps them so).
pub fn labels(world: &Civ3World, env: &mut dyn crate::city_labels::Env) -> Vec<crate::city_labels::CityLabel> {
    use crate::city_labels::{CityLabel, FACE, LabelOp};
    let font = civ3_assets::fonts::FontStyle { face: FACE.to_owned(), height: 10, weight: 700, italic: false };
    let bit = 1u32 << (world.player & 31);
    world
        .cities
        .iter()
        // (Those on tiles explored.)
        .filter(|c| !c.name.is_empty() && world.tile(c.x, c.y).is_some_and(|t| t.explored & bit != 0))
        .map(|c| {
            let width = env.text_width(&font, &c.name);
            let color = env.table_color(world.color_tables.get(c.civ.clamp(0, 31) as usize).copied().unwrap_or(0));
            let run = |dx: i32, color: [u8; 3]| crate::text::TextRun {
                x: 64 * c.x + 64 - width / 2 + dx,
                y: 32 * c.y + 64 + 10 + dx,
                color,
                face: font.face.clone(),
                height: -font.height,
                weight: font.weight,
                italic: false,
                underline: false,
                text: c.name.clone(),
                backdrop: None,
            };
            CityLabel { city: c.id, items: vec![LabelOp::Text(run(1, [0, 0, 0])), LabelOp::Text(run(0, color))] }
        })
        .collect()
}

/// The items of the labels of a preview world (see [`labels`]), per city
/// tile: what `ui_screen::add_art` makes the art of, and [`scene`] draws.
pub type LabelItems = Vec<((i32, i32), Vec<crate::ui_screen::Item>)>;

pub fn label_items(world: &Civ3World, env: &mut dyn crate::city_labels::Env) -> LabelItems {
    let everywhere = [-(1 << 24), -(1 << 24), 1 << 24, 1 << 24];
    labels(world, env)
        .into_iter()
        .filter_map(|label| {
            let city = world.city(label.city)?;
            Some(((city.x, city.y), crate::city_labels::items(std::slice::from_ref(&label), (0, 0), everywhere, true)))
        })
        .collect()
}

/// The scene of a preview world shown `n` times smaller (see [`downscale`]):
/// all of it in the game's view (`scene_map::GAME_VIEW`, which the viewer
/// fits beside the screen), its units, and its labels (`labels`, laid out to
/// keep their size).
/// Leaves out what the cells of vertices among unexplored tiles only would
/// draw but the fog of war: the fog covers them, but not what reaches past
/// the map's edges, where it is not drawn (tall terrain and the edge rows'
/// and columns' halves showed there as stray bits around the black).
fn unexplored_black(world: &Civ3World, compiled: &render_desc::Compiled, scene: &mut scene::Scene) {
    let fog = compiled.layer_index("fog");
    let bit = 1u32 << (world.player & 31);
    let explored = |x: i32, y: i32| world.tile(x, y).is_some_and(|t| t.explored & bit != 0);
    let (cols, rows) = (scene.shape.cols, scene.shape.rows);
    for row in 0..rows {
        for col in 0..cols {
            // (The cell's vertex is the one above tile (x, y): between the
            // tiles north, west, east of it and the tile itself.)
            let (x, y) = (2 * col as i32 + (row as i32 & 1), row as i32);
            if [(x, y - 2), (x - 1, y - 1), (x + 1, y - 1), (x, y)].into_iter().any(|(tx, ty)| explored(tx, ty)) {
                continue;
            }
            for (_, layer) in scene.layers.iter_mut().enumerate().filter(|(k, _)| Some(*k) != fog) {
                if let Some(v) = layer.values.get_mut((row * cols + col) as usize) {
                    *v = 0;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn scene(
    world: &Civ3World,
    n: u32,
    mapper: &crate::scene_map::Mapper,
    art: &mut crate::units::UnitArt,
    compiled: &render_desc::Compiled,
    labels: &LabelItems,
    fonts: &crate::text::Fonts,
) -> scene::Scene {
    scene_with(world, n, mapper, Some(art), compiled, labels, fonts)
}

/// [`scene`], its units only with `art` (none: their art not resolved
/// yet; the world is drawn without them, in the art the description has).
/// The labels are laid out twice where `n` is not 2: for the world shown
/// `n` times smaller (fitted beside the screen) and for the half-size world
/// the viewer explores it at (the game's zoomed-out view), each shown only
/// at its own scale (`CATEGORY_NOT_HALF_RESOLUTION`, `CATEGORY_HALF_RESOLUTION`).
#[allow(clippy::too_many_arguments)]
pub fn scene_with(
    world: &Civ3World,
    n: u32,
    mapper: &crate::scene_map::Mapper,
    art: Option<&mut crate::units::UnitArt>,
    compiled: &render_desc::Compiled,
    labels: &LabelItems,
    fonts: &crate::text::Fonts,
) -> scene::Scene {
    let view = view(world);
    let mut scene = mapper.scene(world, &view, (0, 0), 0);
    unexplored_black(world, compiled, &mut scene);
    if let Some(art) = art {
        crate::units::place(world, &view, art, compiled, None, &mut crate::units::UnitMemory::default(), &mut scene);
    }
    let n = n.max(1);
    let game_view = &mut scene.view_mut(crate::scene_map::GAME_VIEW).view;
    (game_view.downscale, game_view.cull) = (n, None);
    // (Their text drawn from outlines at the zoom the viewer shows it at.)
    scene.zooms = crate::ui_screen::outline_zooms(compiled);
    let outlines = !scene.zooms.is_empty();
    let layouts: &[(u32, u32)] = if n == 2 { &[(2, 0)] } else { &[(n, tile_render::CATEGORY_NOT_HALF_RESOLUTION), (2, tile_render::CATEGORY_HALF_RESOLUTION)] };
    for (tile, items) in labels {
        let sprites = crate::ui_screen::sprites(items, fonts, compiled, crate::ui_screen::under_pass(compiled), outlines);
        for &(at, layout) in layouts {
            let mut sprites: Vec<scene::FreeSprite> = sprites
                .iter()
                .map(|s| scene::FreeSprite { screen: false, clip: None, category: viewer_settings::clean_map::CITIES | layout, ..s.clone() })
                .collect();
            place_label(&mut sprites, *tile, at);
            scene.sprites.extend(sprites);
        }
    }
    scene
}
