//! Art made ahead of need, so that what the game shows next is there when
//! it does: every animation of the units on screen first (they are needed
//! now: they go ahead of the interface's art, however late their plan is
//! made), then the interface's art and fonts, then the animations of the
//! units cities complete next turn and of the other units present. Unit
//! types nobody has are not made ahead: in a network game with every civ
//! present that was 760 MiB of atlas on an 8 GB Mac, made over minutes of
//! dropped frames (user, 2026-11-23). The publisher appends it to the art
//! a little at a time while the art thread has nothing else to do (art the
//! screen needs now goes first), and stops at a budget of atlas memory.

use std::collections::{BTreeSet, VecDeque};

use civ3_assets::fonts::FontStyle;

use crate::units::{self, AnimFile, Recolor, UnitArt};
use crate::world::{Civ3View, Civ3World};

mod planner;
pub use planner::Planner;

/// The art list built in: screen regions the game's screens draw, the
/// fonts its text uses (see [`ArtList`]).
pub const BUILTIN_LIST: &str = include_str!("../../../assets/civ3/preload-art.txt");

/// `AnimationType` values a unit can show (1 DEFAULT … 18 PLANT).
const ANIMATIONS: std::ops::RangeInclusive<i32> = 1..=18;

/// One piece of art to make ahead.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// An animation file in these colourings.
    Unit(AnimFile, BTreeSet<Recolor>),
    /// Screen art: regions, fills, run-time pictures.
    Screen(menu::screen::ScreenArt),
    /// A font in a colour (on a backdrop).
    Font(FontStyle, [u8; 3], Option<[u8; 3]>),
}

impl Item {
    /// About how many bytes of atlas pixels it takes (8-bit art indexed,
    /// one byte a pixel, whatever its colourings; glyphs in colour).
    pub fn bytes(&self) -> u64 {
        match self {
            Item::Unit(f, _) => u64::from(f.chunks)
                .saturating_mul(u64::from(f.size.0))
                .saturating_mul(u64::from(f.size.1)),
            Item::Screen(s) => {
                let rgba =
                    |w: u32, h: u32| u64::from(w).saturating_mul(u64::from(h)).saturating_mul(4);
                s.regions
                    .iter()
                    .map(crate::ui_background::source_bytes)
                    .chain(s.indexed.iter().map(|(region, indices)| {
                        indices.resize.map_or_else(
                            || crate::ui_background::source_bytes(region),
                            |resize| rgba(resize.size.0, resize.size.1),
                        )
                    }))
                    .chain(s.resized_regions.iter().map(|(_, (w, h), _)| rgba(*w, *h)))
                    .chain(s.fills.iter().map(|(_, (w, h))| rgba(*w, *h)))
                    .chain(s.dynamic.iter().map(|(_, (w, h))| rgba(*w, *h)))
                    .fold(0, u64::saturating_add)
            }
            // (About a hundred glyphs of the font's height.)
            Item::Font(style, _, _) => {
                let height = u64::from(style.height.unsigned_abs());
                400u64.saturating_mul(height).saturating_mul(height)
            }
        }
    }
}

/// What the art list says the game's interface shows (lines of
/// `ScreenArt::to_lines` and `font` lines, see [`font_line`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtList {
    pub screen: menu::screen::ScreenArt,
    pub fonts: Vec<(FontStyle, [u8; 3], Option<[u8; 3]>)>,
}

/// A font as a line of an art list: `font`, face, height, weight, italic,
/// colour, backdrop (`-`: none), tab-separated.
pub fn font_line(style: &FontStyle, [r, g, b]: [u8; 3], backdrop: Option<[u8; 3]>) -> String {
    let on = backdrop.map_or("-".to_owned(), |[r, g, b]| format!("{r},{g},{b}"));
    format!(
        "font\t{}\t{}\t{}\t{}\t{r},{g},{b}\t{on}",
        style.face, style.height, style.weight, style.italic as u8
    )
}

impl ArtList {
    pub fn parse(text: &str) -> ArtList {
        let rgb = |s: &str| -> Option<[u8; 3]> {
            let v: Vec<u8> = s
                .split(',')
                .map(|c| c.trim().parse().ok())
                .collect::<Option<_>>()?;
            v.try_into().ok()
        };
        let fonts = text
            .lines()
            .filter_map(
                |line| match line.split('\t').collect::<Vec<_>>().as_slice() {
                    ["font", face, height, weight, italic, color, on] => Some((
                        FontStyle {
                            face: face.to_string(),
                            height: height.parse().ok()?,
                            weight: weight.parse().ok()?,
                            italic: *italic == "1",
                        },
                        rgb(color)?,
                        if *on == "-" { None } else { Some(rgb(on)?) },
                    )),
                    _ => None,
                },
            )
            .collect();
        ArtList {
            screen: menu::screen::ScreenArt::parse_lines(text),
            fonts,
        }
    }
}

/// A group of items of one priority.
struct Tier {
    name: &'static str,
    items: VecDeque<Item>,
    /// Needed now (the units on screen): a later phase's urgent tiers go
    /// ahead of what the plan has left (`Preload::append`).
    urgent: bool,
}

/// The art to make ahead for one game, highest priority first.
pub struct Preload {
    tiers: VecDeque<Tier>,
    /// Bytes of atlas pixels the art may take in all.
    budget: u64,
    /// Why it stopped early, if it did.
    stopped: Option<String>,
    started: std::time::Instant,
    /// Items sent, and their bytes (estimated).
    sent: (usize, u64),
    finished: bool,
}

/// What a game is, for telling another apart (a new plan).
pub fn game_key(world: &Civ3World) -> (i32, i32, u32, usize, usize, u64) {
    (
        world.width,
        world.height,
        world.seed,
        world.unit_type_keys.len(),
        world.era_keys.len(),
        units_signature(world),
    )
}

/// The unit type a city completes next turn (its order a unit, at most a
/// turn to go): its animations must be there when the turn begins.
pub fn next_turn_unit(city: &crate::world::Civ3City) -> Option<i32> {
    (city.order.0 == UNIT_ORDER && city.build_turns.is_some_and(|t| (0..=1).contains(&t))).then_some(city.order.1)
}

/// `Order_Type` of a unit (docs/civ3-map-rendering.md §10.8).
const UNIT_ORDER: i32 = 2;

/// Which kinds of units are in the game or come next turn: each unit type
/// in each civ's colours and era. A plan is made again when that changes:
/// a game's units are read after its first plan (which then had none, and
/// planned none of them: TS Neo, a bomber invisible through its first
/// attack), or a new type is built, or a civ enters an era.
fn units_signature(world: &Civ3World) -> u64 {
    use std::hash::{Hash, Hasher};
    let kinds: BTreeSet<(i32, i32, &str)> = world
        .units
        .iter()
        .map(|u| (u.unit_type, u.civ))
        .chain(world.cities.iter().filter_map(|c| next_turn_unit(c).map(|t| (t, c.civ))))
        .map(|(t, civ)| (t, civ, units::era_of(world, civ)))
        .collect();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    kinds.hash(&mut hasher);
    hasher.finish()
}

/// Planning needs unit rules and civ colours, never the world's tiles,
/// effects, UI state or full city records. Keep this bounded metadata snapshot
/// separate from scene publication so a large map is not cloned for prewarm.
pub fn planning_world(world: &Civ3World) -> Civ3World {
    Civ3World {
        player: world.player,
        debug_mode_bits: world.debug_mode_bits,
        width: world.width,
        height: world.height,
        seed: world.seed,
        eras: world.eras.clone(),
        color_tables: world.color_tables.clone(),
        era_keys: world.era_keys.clone(),
        unit_type_keys: world.unit_type_keys.clone(),
        unit_type_stats: world.unit_type_stats.clone(),
        units: world.units.clone(),
        army_members: world.army_members.clone(),
        cities: world
            .cities
            .iter()
            .map(|city| crate::world::Civ3City {
                civ: city.civ,
                // (What it completes next turn: `next_turn_unit`.)
                order: city.order,
                build_turns: city.build_turns,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

/// The interface art this installation's game showed in earlier sessions,
/// made ahead like [`BUILTIN_LIST`]: a screen that showed its art late once
/// has it ready from the next start on. Lines of an art list, in a file
/// next to the settings ([`Remembered::path`]); kept growing as new art
/// shows, never more than [`Remembered::MAX_LINES`].
#[derive(Debug, Default)]
pub struct Remembered {
    path: Option<std::path::PathBuf>,
    lines: BTreeSet<String>,
    written: usize,
}

impl Remembered {
    pub const MAX_LINES: usize = 50_000;

    /// `seen-art.txt` beside the settings file.
    pub fn path(settings: &std::path::Path) -> std::path::PathBuf {
        settings.with_file_name("seen-art.txt")
    }

    /// What `path` holds (nothing if it is missing or unreadable).
    pub fn load(path: std::path::PathBuf) -> Remembered {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let lines: BTreeSet<String> = list_lines(&text).take(Self::MAX_LINES).collect();
        Remembered { written: lines.len(), path: Some(path), lines }
    }

    /// The remembered lines, as an art list's text.
    pub fn text(&self) -> String {
        self.lines.iter().map(|l| format!("{l}\n")).collect()
    }

    /// Adds the lines of art list `text`; rewrites the file when there
    /// are new ones. `header` starts the file.
    pub fn note(&mut self, header: &str, text: &str) {
        for line in list_lines(text) {
            if self.lines.len() >= Self::MAX_LINES {
                break;
            }
            self.lines.insert(line);
        }
        let Some(path) = &self.path else { return };
        if self.lines.len() == self.written {
            return;
        }
        let temporary = path.with_extension(format!("txt.{}.tmp", std::process::id()));
        let written = std::fs::write(&temporary, format!("{header}{}", self.text())).and_then(|()| std::fs::rename(&temporary, path));
        match written {
            Ok(()) => self.written = self.lines.len(),
            Err(e) => {
                let _ = std::fs::remove_file(&temporary);
                log::warn!("remembered art {}: {e}", path.display());
                // (Not again until more art shows.)
                self.written = self.lines.len();
            }
        }
    }
}

/// The art lines of an art list's text (no comments or blank lines).
fn list_lines(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines().map(str::trim_end).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_owned)
}

/// The budget of atlas memory for art made ahead: `CIV3R_PRELOAD_MIB`, else
/// `default_mib`; `CIV3R_PRELOAD=off`: none.
pub fn budget(default_mib: u64) -> u64 {
    if std::env::var("CIV3R_PRELOAD").as_deref() == Ok("off") {
        return 0;
    }
    std::env::var("CIV3R_PRELOAD_MIB")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default_mib)
        .saturating_mul(1 << 20)
}

impl Preload {
    /// Nothing to make.
    pub fn none() -> Preload {
        Preload {
            tiers: VecDeque::new(),
            budget: 0,
            stopped: None,
            started: std::time::Instant::now(),
            sent: (0, 0),
            finished: true,
        }
    }

    /// Interface work can be admitted before any unit animation is examined.
    pub fn interface(list: &ArtList, budget: u64) -> Preload {
        if budget == 0 {
            return Self::none();
        }
        // The interface: the list's screen art (a batch per image file, as
        // a screen draws them) and fonts.
        let mut interface = VecDeque::new();
        let mut by_file: std::collections::BTreeMap<&str, menu::screen::ScreenArt> =
            Default::default();
        for r in &list.screen.regions {
            by_file
                .entry(r.source.as_str())
                .or_default()
                .regions
                .insert(r.clone());
        }
        for (region, size, step) in &list.screen.resized_regions {
            by_file
                .entry(region.source.as_str())
                .or_default()
                .resized_regions
                .insert((region.clone(), *size, *step));
        }
        for (region, indices) in &list.screen.indexed {
            by_file
                .entry(region.source.as_str())
                .or_default()
                .indexed
                .insert((region.clone(), indices.clone()));
        }
        interface.extend(by_file.into_values().map(Item::Screen));
        let mut rest = menu::screen::ScreenArt::default();
        rest.fills = list.screen.fills.clone();
        rest.dynamic = list.screen.dynamic.clone();
        if !rest.is_empty() {
            interface.push_back(Item::Screen(rest));
        }
        interface.extend(
            list.fonts
                .iter()
                .map(|(s, c, b)| Item::Font(s.clone(), *c, *b)),
        );
        // The list's common styles in its common colours too: text the
        // screens show in another colour (a tooltip, a popup) finds its
        // font there.
        let mut styles: std::collections::BTreeMap<&FontStyle, usize> = Default::default();
        let mut colours: std::collections::BTreeMap<[u8; 3], usize> = Default::default();
        for (s, c, b) in &list.fonts {
            *styles.entry(s).or_default() += 1;
            if b.is_none() {
                *colours.entry(*c).or_default() += 1;
            }
        }
        let listed: BTreeSet<(&FontStyle, [u8; 3])> = list
            .fonts
            .iter()
            .filter(|(_, _, b)| b.is_none())
            .map(|(s, c, _)| (s, *c))
            .collect();
        for (style, _) in styles.iter().filter(|(_, n)| **n >= 3) {
            for (colour, _) in colours.iter().filter(|(_, n)| **n >= 2) {
                if !listed.contains(&(*style, *colour)) {
                    interface.push_back(Item::Font((*style).clone(), *colour, None));
                }
            }
        }
        Self::from_tiers(
            [Tier {
                name: "interface",
                items: interface,
                urgent: false,
            }]
            .into(),
            budget,
        )
    }

    /// Appends a later planning phase without discarding work already admitted
    /// or resetting this plan's budget and byte counters. Its urgent tiers
    /// (the units on screen) go ahead of everything the plan has left, in
    /// their order; the rest goes after it.
    pub fn append(&mut self, phase: Preload) {
        if self.budget == 0 || self.stopped.is_some() {
            return;
        }
        let (urgent, later): (Vec<Tier>, Vec<Tier>) = phase.tiers.into_iter().partition(|t| t.urgent);
        for tier in urgent.into_iter().rev() {
            self.tiers.push_front(tier);
        }
        self.tiers.extend(later);
        self.finished = false;
    }

    fn from_tiers(tiers: VecDeque<Tier>, budget: u64) -> Preload {
        let total = tiers
            .iter()
            .flat_map(|t| &t.items)
            .map(Item::bytes)
            .fold(0, u64::saturating_add);
        log::info!(
            "preload: planned {} ({} MiB of pixels), budget {} MiB",
            tiers
                .iter()
                .map(|t| format!("{} {}", t.items.len(), t.name))
                .collect::<Vec<_>>()
                .join(", "),
            total >> 20,
            budget >> 20
        );
        Preload {
            tiers,
            budget,
            stopped: None,
            started: std::time::Instant::now(),
            sent: (0, 0),
            finished: budget == 0,
        }
    }

    /// Unit planning checks cancellation between every animation header read.
    fn units(
        world: &Civ3World,
        view: &Civ3View,
        art: &mut UnitArt,
        budget: u64,
        stop: &dyn Fn() -> bool,
    ) -> Option<Preload> {
        if budget == 0 || stop() {
            return None;
        }
        let mut tiers = VecDeque::new();
        let mut planned: BTreeSet<(String, Recolor)> = BTreeSet::new();
        // Every animation of `type_key` in `era`, in `colors`, not planned yet.
        let mut animations = |art: &mut UnitArt,
                              type_key: &str,
                              era: &str,
                              colors: &[Recolor],
                              out: &mut VecDeque<Item>| {
            let mut files: Vec<AnimFile> = Vec::new();
            for a in ANIMATIONS {
                if stop() {
                    return false;
                }
                if let Some(f) = art.file(type_key, era, a) {
                    if !files.iter().any(|g| g.path == f.path) {
                        files.push(f.clone());
                    }
                }
            }
            for f in files {
                let new: BTreeSet<Recolor> = colors
                    .iter()
                    .copied()
                    .filter(|c| planned.insert((f.path.clone(), *c)))
                    .collect();
                if !new.is_empty() {
                    out.push_back(Item::Unit(f, new));
                }
            }
            true
        };
        // The units on screen, then the others.
        let on_screen = |x: i32, y: i32| {
            let within = |v: i32, (lo, hi): (i32, i32), size: i32| {
                (lo..=hi).contains(&v)
                    || (size > 0
                        && ((lo..=hi).contains(&(v + size)) || (lo..=hi).contains(&(v - size))))
            };
            within(x, view.tile_x, world.width) && within(y, view.tile_y, world.height)
        };
        let mut seen_units = VecDeque::new();
        let mut coming = VecDeque::new();
        let mut other_units = VecDeque::new();
        // (Planned in the order the tiers go, each item in the first: the
        // units on screen, the units cities complete next turn, the rest.)
        for pass in [true, false] {
            for u in world.units.iter().filter(|u| on_screen(u.x, u.y) == pass) {
                if stop() {
                    return None;
                }
                let out = if pass { &mut seen_units } else { &mut other_units };
                let Some(type_key) = world.unit_type_keys.get(u.unit_type.max(0) as usize) else {
                    continue;
                };
                let era = units::era_of(world, u.civ).to_owned();
                if !animations(art, type_key, &era, &[units::recolor_of(world, u)], out) {
                    return None;
                }
                if let Some(member) = units::member_of(world, u) {
                    if let Some(type_key) = world.unit_type_keys.get(member.unit_type.max(0) as usize) {
                        if !animations(art, type_key, &era, &[Recolor::civ(units::colors_of(world, u.civ))], out) {
                            return None;
                        }
                    }
                }
            }
            if !pass {
                break;
            }
            // The units cities complete next turn: there when it begins.
            for c in &world.cities {
                if stop() {
                    return None;
                }
                let Some(type_key) = next_turn_unit(c).and_then(|t| world.unit_type_keys.get(t.max(0) as usize)) else {
                    continue;
                };
                let era = units::era_of(world, c.civ).to_owned();
                if !animations(art, type_key, &era, &[Recolor::civ(units::colors_of(world, c.civ))], &mut coming) {
                    return None;
                }
            }
        }
        tiers.push_back(Tier {
            name: "units on screen",
            items: seen_units,
            urgent: true,
        });
        tiers.push_back(Tier {
            name: "units built next turn",
            items: coming,
            urgent: false,
        });
        tiers.push_back(Tier {
            name: "units present",
            items: other_units,
            urgent: false,
        });
        if stop() {
            return None;
        }
        Some(Self::from_tiers(tiers, budget))
    }

    /// Bytes of pixels the plan has left to make (estimated).
    pub fn left(&self) -> u64 {
        self.tiers
            .iter()
            .flat_map(|t| &t.items)
            .map(Item::bytes)
            .fold(0, u64::saturating_add)
    }

    /// The items of the next addition: from the first tier with any, about
    /// `chunk` bytes of them (at least one), unless that would take the art
    /// (`used` bytes of atlas now) past the budget: then nothing more, ever
    /// (what comes later in the order stays out). `wanted` says whether an
    /// item is wanted already (on demand): those are left out.
    pub fn next(
        &mut self,
        used: u64,
        chunk: u64,
        mut wanted: impl FnMut(&Item) -> bool,
    ) -> Option<(&'static str, Vec<Item>)> {
        if self.finished {
            return None;
        }
        loop {
            while self.tiers.front().is_some_and(|t| t.items.is_empty()) {
                self.tiers.pop_front();
            }
            let Some(tier) = self.tiers.front_mut() else {
                self.finished = true;
                log::info!(
                    "preload: done in {:.1} s, {} items (about {} MiB of pixels); the art has {} MiB",
                    self.started.elapsed().as_secs_f64(),
                    self.sent.0,
                    self.sent.1 >> 20,
                    used >> 20
                );
                return None;
            };
            let mut items = Vec::new();
            let mut bytes = 0u64;
            let mut over = false;
            while let Some(item) = tier.items.front() {
                if wanted(item) {
                    tier.items.pop_front();
                    continue;
                }
                let b = item.bytes();
                let next_bytes = bytes.checked_add(b);
                if !items.is_empty() && next_bytes.is_none_or(|total| total > chunk) {
                    break;
                }
                // A saturated estimate cannot prove that the item fits,
                // even if the configured budget is also u64::MAX.
                if b == u64::MAX
                    || next_bytes
                        .and_then(|total| used.checked_add(total))
                        .is_none_or(|total| total > self.budget)
                {
                    over = true;
                    break;
                }
                bytes = next_bytes.expect("budget checked");
                items.push(tier.items.pop_front().expect("looked at"));
            }
            let name = tier.name;
            if over {
                let left: Vec<String> = self
                    .tiers
                    .iter()
                    .map(|t| format!("{} {}", t.items.len(), t.name))
                    .collect();
                let why = format!(
                    "at the budget of {} MiB (the art has {} MiB; left: {})",
                    self.budget >> 20,
                    used >> 20,
                    left.join(", ")
                );
                log::info!(
                    "preload: stopped {why}, after {:.1} s",
                    self.started.elapsed().as_secs_f64()
                );
                self.stopped = Some(why);
                self.finished = true;
            }
            if items.is_empty() {
                if self.finished {
                    return None;
                }
                // (All wanted already: on to the next tier.)
                continue;
            }
            self.sent = (
                self.sent.0.saturating_add(items.len()),
                self.sent.1.saturating_add(bytes),
            );
            return Some((name, items));
        }
    }

    /// Whether it has nothing more to make.
    pub fn finished(&self) -> bool {
        self.finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TS Neo: a game's first plan was made before its units were read,
    /// so it planned none of them and was never made again; the bomber an
    /// AI flew in its first turn showed nothing until its attack ended.
    #[test]
    fn units_read_late_or_coming_next_turn_plan_again() {
        use crate::world::{Civ3City, Civ3Unit};
        let mut world = Civ3World { width: 40, height: 40, seed: 7, unit_type_keys: vec!["Warrior".into(), "Bomber".into()], ..Default::default() };
        let empty = game_key(&world);
        world.units.push(Civ3Unit { unit_type: 0, civ: 1, ..Default::default() });
        let with_units = game_key(&world);
        assert_ne!(empty, with_units, "units read after the first plan: planned again");
        // A unit moving, or another of a kind already there: no new plan.
        world.units.push(Civ3Unit { unit_type: 0, civ: 1, x: 5, ..Default::default() });
        assert_eq!(game_key(&world), with_units);
        // A city completing a bomber next turn: planned again, the bomber with it.
        let mut city = Civ3City { civ: 2, order: (2, 1), build_turns: Some(3), ..Default::default() };
        world.cities.push(city.clone());
        assert_eq!(game_key(&world), with_units, "three turns to go: not yet");
        assert_eq!(next_turn_unit(&city), None);
        city.build_turns = Some(1);
        world.cities[0] = city.clone();
        assert_ne!(game_key(&world), with_units);
        assert_eq!(next_turn_unit(&city), Some(1));
        // An improvement completing: no unit.
        assert_eq!(next_turn_unit(&Civ3City { order: (1, 1), build_turns: Some(1), ..Default::default() }), None);
        // The planning snapshot keeps what the city completes.
        assert_eq!(planning_world(&world).cities[0].order, (2, 1));
        assert_eq!(planning_world(&world).cities[0].build_turns, Some(1));
    }

    #[test]
    fn remembered_art_survives_restarts_and_grows_only_with_new_lines() {
        let dir = std::env::temp_dir().join(format!("civ3r-remembered-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = Remembered::path(&dir.join("settings.toml"));
        let line = "region\tArt/Advisors/cultureometer.pcx\t1,1,72,13\tk\t-\t0,0\t-\t0,0";
        let mut first = Remembered::load(path.clone());
        assert_eq!(first.text(), "");
        first.note("# head\n", &format!("# comment\n{line}\n\n{line}\n"));
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written, format!("# head\n{line}\n"));
        // The next session starts with it, and the art list parses it.
        let mut second = Remembered::load(path.clone());
        assert_eq!(second.text(), format!("{line}\n"));
        assert_eq!(ArtList::parse(&second.text()).screen.regions.len(), 1);
        // Nothing new: the file is not rewritten.
        std::fs::write(&path, "kept").unwrap();
        second.note("# head\n", line);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "kept");
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["seen-art.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn file(path: &str, frames: u32) -> AnimFile {
        AnimFile {
            path: path.into(),
            frames_per_direction: 1,
            chunks: frames,
            size: (10, 10),
            anchor: (0, 0),
        }
    }

    fn preload(budget: u64) -> Preload {
        let unit = |p: &str| Item::Unit(file(p, 10), [Recolor::civ(1)].into());
        let tiers = [
            ("a", vec![unit("a1"), unit("a2"), unit("a3")]),
            ("b", vec![unit("b1")]),
        ]
        .into_iter()
        .map(|(name, items)| Tier {
            name,
            items: items.into(),
            urgent: false,
        })
        .collect();
        Preload {
            tiers,
            budget,
            stopped: None,
            started: std::time::Instant::now(),
            sent: (0, 0),
            finished: false,
        }
    }

    #[test]
    fn items_come_by_priority_in_chunks_and_skip_what_is_wanted() {
        let mut p = preload(1 << 20);
        // 1000 bytes an item: two to a chunk of 2500.
        let (tier, items) = p.next(0, 2500, |_| false).unwrap();
        assert_eq!((tier, items.len()), ("a", 2));
        // a3 is wanted already: on to the next tier.
        let (tier, items) = p
            .next(
                2000,
                2500,
                |i| matches!(i, Item::Unit(f, _) if f.path == "a3"),
            )
            .unwrap();
        assert_eq!((tier, items.len()), ("b", 1));
        assert!(p.next(3000, 2500, |_| false).is_none());
        assert!(p.finished());
    }

    #[test]
    fn it_stops_at_the_budget_keeping_the_order() {
        let mut p = preload(2500);
        assert_eq!(p.next(0, 10_000, |_| false).map(|(_, i)| i.len()), Some(2));
        // The third would go past it: nothing more, not even later tiers.
        assert!(p.next(2000, 10_000, |_| false).is_none());
        assert!(p.finished() && p.stopped.is_some());
    }

    #[test]
    fn indexed_preload_lists_retain_rules_and_charge_for_each_atlas_region() {
        let mut art = menu::screen::ScreenArt::default();
        let region = menu::screen::Region {
            source: "Art/resources_shadows.pcx".into(),
            rect: [51, 151, 48, 49],
            keyed: true,
            mask: None,
            palette: None,
        };
        art.indexed.insert((
            region.clone(),
            crate::ui::indices::blend(&crate::ui::indices::SHADOW_LEVELS),
        ));
        art.indexed
            .insert((region, crate::ui::indices::colored_shades()));
        let parsed = ArtList::parse(&art.to_lines());
        assert_eq!(parsed.screen, art);
        assert_eq!(Item::Screen(parsed.screen).bytes(), 2 * 48 * 49);
    }

    #[test]
    fn mixed_native_background_masks_reserve_their_full_pixel_cost_before_preloading() {
        let region = |source: &str, rect| menu::screen::Region {
            source: source.into(),
            rect,
            keyed: true,
            mask: None,
            palette: None,
        };
        let mut art = menu::screen::ScreenArt::default();
        art.regions.extend([
            region("Art/interface/x_pref background.pcx", [0, 0, 1024, 768]),
            region(
                "Art/Civilopedia/civilopediaBackground.pcx",
                [226, 130, 418, 570],
            ),
            region("Art/x_title.PCX", [0, 0, 1024, 768]),
        ]);
        assert_eq!(
            Item::Screen(art).bytes(),
            8 * 1024 * 768 + 418 * 570 + 1024 * 768
        );
    }

    #[test]
    fn oversized_art_estimates_saturate_instead_of_wrapping() {
        let region = |source: &str, rect| menu::screen::Region {
            source: source.into(),
            rect,
            keyed: true,
            mask: None,
            palette: None,
        };
        let huge = region(
            "Art/interface/x_pref background.pcx",
            [0, 0, i32::MAX, i32::MAX],
        );
        assert_eq!(crate::ui_background::source_bytes(&huge), u64::MAX);
        let mut mixed = menu::screen::ScreenArt::default();
        mixed
            .regions
            .extend([huge, region("Art/other.pcx", [0, 0, 2, 3])]);
        mixed.fills.insert(([255; 4], (1, 1)));
        assert_eq!(Item::Screen(mixed).bytes(), u64::MAX);

        // Each RGBA producer must saturate its own product before the
        // screen's estimates are added together.
        let mut resized = menu::screen::ScreenArt::default();
        resized.resized_regions.insert((
            region("Art/icon.pcx", [0, 0, 2, 2]),
            (u32::MAX, u32::MAX),
            [1, 1],
        ));
        let mut filled = menu::screen::ScreenArt::default();
        filled.fills.insert(([255; 4], (u32::MAX, u32::MAX)));
        let mut dynamic = menu::screen::ScreenArt::default();
        dynamic
            .dynamic
            .insert(("picture".into(), (u32::MAX, u32::MAX)));
        for art in [resized, filled, dynamic] {
            assert_eq!(Item::Screen(art).bytes(), u64::MAX);
        }
        let mut animation = file("huge.flc", u32::MAX);
        animation.size = (u32::MAX, u32::MAX);
        assert_eq!(
            Item::Unit(animation, [Recolor::civ(1)].into()).bytes(),
            u64::MAX
        );
        let font = FontStyle {
            face: "Lucida Sans".into(),
            height: i32::MIN,
            weight: 400,
            italic: false,
        };
        assert_eq!(Item::Font(font, [0; 3], None).bytes(), u64::MAX);
    }

    #[test]
    fn saturated_mixed_background_cannot_bypass_the_preload_budget() {
        let mut art = menu::screen::ScreenArt::default();
        art.regions.insert(menu::screen::Region {
            source: "Art/interface/x_pref background.pcx".into(),
            rect: [0, 0, i32::MAX, i32::MAX],
            keyed: true,
            mask: None,
            palette: None,
        });
        art.fills.insert(([255; 4], (1, 1)));
        for used in [0, 1] {
            let mut p = Preload {
                tiers: [Tier {
                    name: "interface",
                    items: [Item::Screen(art.clone())].into(),
                    urgent: false,
                }]
                .into(),
                budget: u64::MAX,
                stopped: None,
                started: std::time::Instant::now(),
                sent: (0, 0),
                finished: false,
            };
            assert_eq!(p.left(), u64::MAX);
            assert!(p.next(used, u64::MAX, |_| false).is_none());
            assert!(p.finished() && p.stopped.is_some());
            assert_eq!(p.sent, (0, 0));
        }
    }

    #[test]
    fn preload_totals_and_admission_cannot_wrap_near_the_integer_limit() {
        let mut animation = file("large.flc", 2);
        animation.size = (1 << 31, 1 << 31);
        let item = Item::Unit(animation, [Recolor::civ(1)].into());
        assert_eq!(item.bytes(), 1 << 63);
        let mut p = Preload {
            tiers: [Tier {
                name: "large",
                items: [item.clone(), item].into(),
                urgent: false,
            }]
            .into(),
            budget: u64::MAX,
            stopped: None,
            started: std::time::Instant::now(),
            sent: (0, 0),
            finished: false,
        };
        assert_eq!(p.left(), u64::MAX);
        // The second item cannot wrap the chunk total and join the first.
        assert_eq!(
            p.next(0, u64::MAX, |_| false).map(|(_, items)| items.len()),
            Some(1)
        );
        // Nor can it wrap the sum with the bytes already in the atlas.
        assert!(p.next(1 << 63, u64::MAX, |_| false).is_none());
        assert!(p.finished() && p.stopped.is_some());
        assert_eq!(p.sent, (1, 1 << 63));
    }

    #[test]
    fn enlarged_icon_preload_cannot_bypass_the_pixel_budget_with_its_small_source_crop() {
        let mut art = menu::screen::ScreenArt::default();
        art.resized_regions.insert((
            menu::screen::Region {
                source: "Art/icon.pcx".into(),
                rect: [0, 0, 2, 2],
                keyed: true,
                mask: None,
                palette: None,
            },
            (128, 128),
            [1024, 1024],
        ));
        let mut p = Preload {
            tiers: [Tier {
                name: "icons",
                items: [Item::Screen(art)].into(),
                urgent: false,
            }]
            .into(),
            budget: 60_000,
            stopped: None,
            started: std::time::Instant::now(),
            sent: (0, 0),
            finished: false,
        };
        assert!(p.next(0, 100_000, |_| false).is_none());
        assert!(p.finished() && p.stopped.is_some());
    }

    #[test]
    fn art_lists_have_regions_and_fonts() {
        let style = FontStyle {
            face: "Lucida Sans".into(),
            height: 12,
            weight: 700,
            italic: false,
        };
        let text = format!(
            "region\tArt/a.pcx\t0,0,4,4\tk\t-\t0,0\t-\t0,0\n{}\n",
            font_line(&style, [1, 2, 3], Some([255, 0, 255]))
        );
        let list = ArtList::parse(&text);
        assert_eq!(list.screen.regions.len(), 1);
        assert_eq!(list.fonts, vec![(style, [1, 2, 3], Some([255, 0, 255]))]);
        assert!(
            ArtList::parse(BUILTIN_LIST).screen.regions.len()
                + ArtList::parse(BUILTIN_LIST).fonts.len()
                < usize::MAX
        );
    }
}

#[cfg(test)]
mod phase_tests {
    use super::*;

    fn screen(source: &str) -> Item {
        let mut art = menu::screen::ScreenArt::default();
        art.regions.insert(menu::screen::Region { source: source.into(), rect: [0, 0, 4, 4], keyed: false, mask: None, palette: None });
        Item::Screen(art)
    }

    fn tier(name: &'static str, sources: &[&str], urgent: bool) -> Tier {
        Tier { name, items: sources.iter().map(|s| screen(s)).collect(), urgent }
    }

    #[test]
    fn the_units_on_screen_go_ahead_of_the_interface_left_however_late_their_plan() {
        let mut p = Preload::from_tiers([tier("interface", &["i1", "i2", "i3"], false)].into(), 1 << 30);
        // One interface item went before the units' plan was made.
        assert_eq!(p.next(0, 1, |_| false).map(|(t, _)| t), Some("interface"));
        p.append(Preload::from_tiers(
            [tier("units on screen", &["u1"], true), tier("units built next turn", &["n1"], false), tier("units present", &["p1"], false)].into(),
            1 << 30,
        ));
        let mut order = Vec::new();
        while let Some((tier, items)) = p.next(0, 1, |_| false) {
            order.extend(items.iter().map(|i| match i {
                Item::Screen(a) => format!("{tier}:{}", a.regions.iter().next().unwrap().source),
                _ => unreachable!(),
            }));
        }
        assert_eq!(order, ["units on screen:u1", "interface:i2", "interface:i3", "units built next turn:n1", "units present:p1"]);
    }

    #[test]
    fn unit_types_nobody_has_are_not_made_ahead() {
        // Two unit types, one unit of the first: only that type's art is
        // looked for (no type of nobody's, in no era).
        let world = Civ3World {
            unit_type_keys: vec!["PRTO_Infantry".into(), "PRTO_Tank".into()],
            era_keys: vec!["ERAS_Ancient_Times".into(), "ERAS_Modern_Era".into()],
            units: vec![crate::world::Civ3Unit { unit_type: 0, civ: 1, ..Default::default() }],
            ..Default::default()
        };
        let mut art = UnitArt::new(civ3_assets::GameFiles::with_roots(Vec::new()));
        let plan = Preload::units(&world, &Civ3View::default(), &mut art, 1 << 30, &|| false).expect("a plan");
        assert!(plan.tiers.iter().all(|t| matches!(t.name, "units on screen" | "units built next turn" | "units present")), "{:?}", plan.tiers.iter().map(|t| t.name).collect::<Vec<_>>());
    }

    #[test]
    fn resized_indexed_icons_charge_the_output_rgba_pixels_before_admission() {
        let mut list = ArtList::default();
        let region = menu::screen::Region {
            source: "tiny.pcx".into(),
            rect: [0, 0, 2, 2],
            keyed: true,
            mask: None,
            palette: None,
        };
        let indices = menu::screen::Indices {
            resize: Some(menu::screen::IndexedResize {
                size: (128, 128),
                sample_step: [1024, 1024],
            }),
            ..Default::default()
        };
        list.screen.indexed.insert((region, indices));
        assert_eq!(Item::Screen(list.screen.clone()).bytes(), 65536);
        let mut plan = Preload::interface(&list, 65535);
        assert!(plan.next(0, 1 << 20, |_| false).is_none());
        assert!(plan.finished() && plan.stopped.is_some());
    }

    #[test]
    fn interface_is_admitted_before_later_units_without_resetting_budget_accounting() {
        let mut list = ArtList::default();
        list.screen.fills.insert(([255; 4], (10, 10)));
        let mut plan = Preload::interface(&list, 1400);
        assert_eq!(
            plan.next(0, 1000, |_| false)
                .map(|(tier, items)| (tier, items.len())),
            Some(("interface", 1))
        );
        assert!(plan.next(400, 1000, |_| false).is_none());
        assert!(plan.finished());
        let item = Item::Unit(
            AnimFile {
                path: "later.flc".into(),
                frames_per_direction: 1,
                chunks: 10,
                size: (10, 10),
                anchor: (0, 0),
            },
            [Recolor::civ(1)].into(),
        );
        plan.append(Preload::from_tiers(
            [Tier {
                name: "units",
                items: [item].into(),
                urgent: false,
            }]
            .into(),
            u64::MAX,
        ));
        assert!(!plan.finished());
        assert_eq!(
            plan.budget, 1400,
            "a late phase cannot enlarge the original budget"
        );
        assert_eq!(plan.sent, (1, 400));
        assert_eq!(
            plan.next(400, 1000, |_| false)
                .map(|(tier, items)| (tier, items.len())),
            Some(("units", 1))
        );
        assert_eq!(plan.sent, (2, 1400));
    }

    #[test]
    fn appending_later_phases_cannot_restart_a_plan_stopped_at_its_cap() {
        let mut list = ArtList::default();
        list.screen.fills.insert(([255; 4], (10, 10)));
        let mut plan = Preload::interface(&list, 399);
        assert!(plan.next(0, 1000, |_| false).is_none());
        assert!(plan.stopped.is_some());
        plan.append(Preload::interface(&list, u64::MAX));
        assert!(plan.finished());
        assert_eq!(plan.sent, (0, 0));
        assert!(Preload::interface(&list, 0).finished());
    }

    #[test]
    fn interface_plan_preserves_indexed_rules_and_prewarm_fonts() {
        let mut list = ArtList::default();
        let region = menu::screen::Region {
            source: "scenario.pcx".into(),
            rect: [0, 0, 10, 10],
            keyed: true,
            mask: None,
            palette: None,
        };
        list.screen
            .indexed
            .insert((region, crate::ui::indices::colored_shades()));
        list.fonts.push((
            FontStyle {
                face: "Lucida Sans".into(),
                height: 12,
                weight: 400,
                italic: false,
            },
            [1, 2, 3],
            None,
        ));
        let mut plan = Preload::interface(&list, 1 << 20);
        let (_, items) = plan.next(0, 1 << 20, |_| false).unwrap();
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], Item::Screen(art) if art.indexed == list.screen.indexed));
        assert!(matches!(&items[1], Item::Font(..)));
    }

    #[test]
    fn planning_snapshot_omits_tiles_and_retains_unit_colours_and_army_metadata() {
        use crate::world::{Civ3City, Civ3Tile, Civ3Unit};
        let mut army_stats = [0; 7];
        army_stats[6] = 1 << 0x12; // UTA_Army, as encoded in game rule data.
        let world = Civ3World {
            player: 2,
            width: 100,
            height: 80,
            seed: 7,
            eras: vec![0, 1, 0],
            color_tables: vec![3, 4, 5],
            era_keys: vec!["ERAS_Ancient_Times".into(), "ERAS_Middle_Ages".into()],
            unit_type_keys: vec!["PRTO_Army".into()],
            unit_type_stats: vec![army_stats],
            units: vec![Civ3Unit {
                id: 7,
                civ: 1,
                army_member: Some(9),
                army_size: 2,
                ..Default::default()
            }],
            army_members: vec![Civ3Unit {
                id: 9,
                civ: 1,
                container: 7,
                ..Default::default()
            }],
            cities: vec![Civ3City {
                civ: 2,
                name: "A long city record that planning never uses".into(),
                ..Default::default()
            }],
            tiles: vec![Civ3Tile::default(); 4000],
            ..Default::default()
        };
        let compact = planning_world(&world);
        assert!(compact.tiles.is_empty());
        assert!(compact.cities[0].name.is_empty());
        assert_eq!(compact.units, world.units);
        assert_eq!(
            units::recolor_of(&compact, &compact.units[0]),
            units::recolor_of(&world, &world.units[0])
        );
        assert_eq!(
            units::member_of(&compact, &compact.units[0]),
            units::member_of(&world, &world.units[0])
        );
        assert_eq!(units::era_of(&compact, 1), units::era_of(&world, 1));
        assert_eq!(compact.cities[0].civ, world.cities[0].civ);
    }

    #[test]
    fn units_stop_before_any_animation_when_disabled_or_superseded() {
        // (One unit present: its animations' headers are what is read.)
        let world = Civ3World {
            unit_type_keys: vec!["PRTO_Infantry".into()],
            units: vec![crate::world::Civ3Unit { unit_type: 0, civ: 1, ..Default::default() }],
            ..Default::default()
        };
        let mut art = UnitArt::new(civ3_assets::GameFiles::with_roots(Vec::new()));
        let checks = std::cell::Cell::new(0);
        let stop = || {
            checks.set(checks.get() + 1);
            true
        };
        assert!(Preload::units(&world, &Civ3View::default(), &mut art, 0, &stop).is_none());
        assert_eq!(checks.get(), 0);
        assert!(Preload::units(&world, &Civ3View::default(), &mut art, 1 << 20, &stop).is_none());
        assert_eq!(checks.get(), 1);
        checks.set(0);
        let mid_scan = || {
            let next = checks.get() + 1;
            checks.set(next);
            next >= 5
        };
        assert!(
            Preload::units(&world, &Civ3View::default(), &mut art, 1 << 20, &mid_scan).is_none()
        );
        assert_eq!(
            checks.get(),
            5,
            "obsolete scans stop between individual animation headers"
        );
    }
}
