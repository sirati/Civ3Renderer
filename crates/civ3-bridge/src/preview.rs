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
use std::sync::{Arc, Mutex, OnceLock};

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
    static RULES: OnceLock<Mutex<Option<Arc<Rules>>>> = OnceLock::new();
    let cell = RULES.get_or_init(|| Mutex::new(None));
    let mut slot = cell.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = slot.as_ref() {
        return Ok(r.clone());
    }
    let path = install.join("Conquests").join("conquests.biq");
    let biq = Biq::open(&path).map_err(|e| PreviewError::Read(path.clone(), e))?;
    let rules = Arc::new(biq.rules().ok_or_else(|| PreviewError::Rules(format!("{path:?} has none")))?);
    *slot = Some(rules.clone());
    Ok(rules)
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
    let revealed = crate::revealed::revealed(&map, &players, biq.map_visible());
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

static SELECTED: Mutex<Selected> = Mutex::new(Selected { number: 0, file: None, at: None });

/// The file the Load Scenario screen last highlighted.
pub fn selected() -> Selected {
    SELECTED.lock().unwrap_or_else(|e| e.into_inner()).clone()
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
        let job = {
            let mut s = SELECTED.lock().unwrap_or_else(|e| e.into_inner());
            *s = Selected { number: s.number + 1, file: name.filter(|n| !n.is_empty()), at: Some(std::time::Instant::now()) };
            Job { number: s.number, file: s.file.clone() }
        };
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
    /// The art its units need, in its art folders (none until the preview
    /// thread has resolved it: meanwhile the world shows with the art there
    /// is).
    pub needed: Option<Arc<crate::units::Needed>>,
}

impl Current {
    /// Its art folders.
    pub fn folders(&self) -> (Option<String>, Vec<String>) {
        (self.preview.scenario_folder.clone(), self.preview.search_folders.clone())
    }
}

/// What the preview thread sends back.
enum Made {
    /// The world (first, to show at once).
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
static ASKER: Mutex<Option<(Arc<std::sync::atomic::AtomicU64>, std::sync::mpsc::Sender<Job>)>> = Mutex::new(None);

/// Asks the preview thread for `job` at once (none running: the publisher
/// asks when it sees the selection).
#[cfg(windows)]
fn ask_now(job: Job) {
    if let Some((latest, jobs)) = ASKER.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        latest.store(job.number, std::sync::atomic::Ordering::Release);
        let _ = jobs.send(job);
    }
}

/// The thread making previews: the latest job only.
struct Worker {
    latest: Arc<std::sync::atomic::AtomicU64>,
    jobs: std::sync::mpsc::Sender<Job>,
    made: std::sync::mpsc::Receiver<Made>,
}

impl Worker {
    fn start(install: PathBuf) -> std::io::Result<Worker> {
        let latest = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let (jobs, job_rx) = std::sync::mpsc::channel::<Job>();
        let (made_tx, made) = std::sync::mpsc::channel();
        let newest = latest.clone();
        std::thread::Builder::new().name("civ3r-preview".into()).spawn(move || {
            while let Ok(mut job) = job_rx.recv() {
                // (Of those waiting, the latest.)
                while let Ok(j) = job_rx.try_recv() {
                    job = j;
                }
                let superseded = || newest.load(std::sync::atomic::Ordering::Acquire) != job.number;
                if superseded() {
                    continue;
                }
                let mut gone = false;
                make(&install, &job, &superseded, &mut |m| gone |= made_tx.send(m).is_err());
                if gone {
                    break;
                }
            }
        })?;
        *ASKER.lock().unwrap_or_else(|e| e.into_inner()) = Some((latest.clone(), jobs.clone()));
        Ok(Worker { latest, jobs, made })
    }

    fn ask(&self, job: Job) {
        self.latest.store(job.number, std::sync::atomic::Ordering::Release);
        let _ = self.jobs.send(job);
    }
}

/// The scenario previewed: the one the Load Scenario screen last loaded,
/// made ready on a thread of its own (`civ3r-preview`): reading the file,
/// making its world and labels, resolving its unit art. The publisher only
/// asks and takes what is ready, never waits: the world shown meanwhile is
/// the one before (none for a while after the screen opens); a newer
/// selection supersedes what is being made (it stops between stages).
#[derive(Default)]
pub struct Previews {
    worker: Option<Worker>,
    /// The selection asked for, and the latest preview made for it (or
    /// before it).
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
    art_from: Option<u64>,
}

impl Previews {
    /// The world of the scenario the screen shows now, as far as it is
    /// ready (`None`: none, or one without a map of its own).
    pub fn update(&mut self, install: &Path) -> Option<&Current> {
        let Selected { number, file, at } = selected();
        if number != self.asked {
            self.asked = number;
            (self.loaded_at, self.published, self.art_from) = (at, false, None);
            if self.worker.is_none() {
                match Worker::start(install.to_path_buf()) {
                    Ok(w) => self.worker = Some(w),
                    Err(e) => log::error!("scenario preview: no thread for it: {e}"),
                }
            }
            // (Unless the hook asked for it already.)
            if let Some(w) = self.worker.as_ref().filter(|w| w.latest.load(std::sync::atomic::Ordering::Acquire) != number) {
                w.ask(Job { number, file });
            }
        }
        for made in std::mem::take(&mut self.early) {
            self.take(made);
        }
        while let Some(made) = self.worker.as_ref().and_then(|w| w.made.try_recv().ok()) {
            self.take(made);
        }
        self.current.as_ref()
    }

    fn take(&mut self, made: Made) {
        match made {
            // (A preview for an older selection: gone by.)
            Made::World { number, .. } | Made::Labels { number, .. } | Made::Art { number, .. } if number != self.asked => {}
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

    /// Waits at most `timeout` for the preview thread to send something (the
    /// publisher's pause between its rounds: it goes on at once when a
    /// preview stage is ready; without the thread, the whole pause).
    pub fn wait(&mut self, timeout: std::time::Duration) {
        match self.worker.as_ref().map(|w| w.made.recv_timeout(timeout)) {
            Some(Ok(made)) => self.early.push(made),
            Some(Err(std::sync::mpsc::RecvTimeoutError::Timeout)) => {}
            _ => std::thread::sleep(timeout),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_keeps_its_size_beside_its_city_in_a_world_shown_smaller() {
        let mut s = vec![scene::FreeSprite { x: 64 * 7 + 64 - 30, y: 32 * 25 + 66, ..Default::default() }];
        place_label(&mut s, (7, 25), 8);
        // Divided by 8 by the renderer: 30 left of the tile's bottom corner,
        // 2 below it, as at full size.
        let (cx, cy) = ((64 * 7 + 64) / 8, (32 * 25 + 64) / 8);
        assert_eq!((s[0].x / 8, s[0].y / 8), (cx - 30, cy + 2));
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
        assert_eq!(explored, 3178);
        assert!(w.tiles.iter().filter(|t| t.explored & bit != 0).all(|t| w.visible_now(t)));
        assert!(w.tiles.iter().filter(|t| t.explored & bit == 0).all(|t| !w.visible_now(t)));
        // Units on tiles seen only.
        assert!(w.units.iter().all(|u| w.tile(u.x, u.y).is_some_and(|t| w.visible_now(t))));
        let mut env = crate::city_labels::FileEnv::new(Box::new(crate::assets::game_files(&install, None, &[])));
        // Every city a label (all of them are the players', and explored);
        // one on a tile not explored none.
        assert_eq!(labels(w, &mut env).len(), w.cities.len());
        let mut hidden = (**w).clone();
        let (x, y) = (hidden.cities[0].x, hidden.cities[0].y);
        let i = hidden.index(x, y);
        hidden.tiles[i].explored = 0;
        assert_eq!(labels(&hidden, &mut env).len(), w.cities.len() - 1);
        // The area: the tiles revealed (columns 4..139, rows 6..134) and a
        // tile around.
        assert_eq!(p.area, [64 * 4 - 64, 32 * 6 - 64, 64 * 134 + 128 + 128, 32 * 127 + 64 + 128]);
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
        let mut p = Previews { worker: Some(worker), asked: 20, ..Default::default() };
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
    let view = view(world);
    let mut scene = mapper.scene(world, &view, (0, 0), 0);
    unexplored_black(world, compiled, &mut scene);
    crate::units::place(world, &view, art, compiled, None, &mut crate::units::UnitMemory::default(), &mut scene);
    let game_view = &mut scene.view_mut(crate::scene_map::GAME_VIEW).view;
    (game_view.downscale, game_view.cull) = (n.max(1), None);
    // (Their text drawn from outlines at the zoom the viewer shows it at.)
    scene.zooms = crate::ui_screen::outline_zooms(compiled);
    let outlines = !scene.zooms.is_empty();
    for (tile, items) in labels {
        let mut sprites: Vec<scene::FreeSprite> = crate::ui_screen::sprites(items, fonts, compiled, crate::ui_screen::under_pass(compiled), outlines)
            .into_iter()
            .map(|s| scene::FreeSprite { screen: false, clip: None, category: viewer_settings::clean_map::CITIES, ..s })
            .collect();
        place_label(&mut sprites, *tile, n);
        scene.sprites.extend(sprites);
    }
    scene
}
