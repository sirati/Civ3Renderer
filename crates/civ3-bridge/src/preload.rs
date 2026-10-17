//! Art made ahead of need, so that what the game shows next is there when
//! it does: at the start of a game, every animation of the units on screen,
//! then of the other units present, the interface's art (the screens'
//! regions and the fonts a list names: `assets/civ3/preload-art.txt`),
//! then every animation of every unit type of the game in every era, in
//! the colours of the civs present. The publisher appends it to the art
//! a little at a time while the art thread has nothing else to do (art the
//! screen needs now goes first), and stops at a budget of atlas memory.

use std::collections::{BTreeSet, VecDeque};

use civ3_assets::fonts::FontStyle;

use crate::units::{self, AnimFile, Recolor, UnitArt};
use crate::world::{Civ3View, Civ3World};

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
            Item::Unit(f, _) => f.chunks as u64 * f.size.0 as u64 * f.size.1 as u64,
            Item::Screen(s) => {
                s.regions.iter().map(|r| r.rect[2].max(0) as u64 * r.rect[3].max(0) as u64).sum::<u64>()
                    + s.fills.iter().map(|(_, (w, h))| 4 * *w as u64 * *h as u64).sum::<u64>()
                    + s.dynamic.iter().map(|(_, (w, h))| 4 * *w as u64 * *h as u64).sum::<u64>()
            }
            // (About a hundred glyphs of the font's height.)
            Item::Font(style, _, _) => 100 * 4 * style.height.unsigned_abs() as u64 * style.height.unsigned_abs() as u64,
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
    format!("font\t{}\t{}\t{}\t{}\t{r},{g},{b}\t{on}", style.face, style.height, style.weight, style.italic as u8)
}

impl ArtList {
    pub fn parse(text: &str) -> ArtList {
        let rgb = |s: &str| -> Option<[u8; 3]> {
            let v: Vec<u8> = s.split(',').map(|c| c.trim().parse().ok()).collect::<Option<_>>()?;
            v.try_into().ok()
        };
        let fonts = text
            .lines()
            .filter_map(|line| match line.split('\t').collect::<Vec<_>>().as_slice() {
                ["font", face, height, weight, italic, color, on] => Some((
                    FontStyle { face: face.to_string(), height: height.parse().ok()?, weight: weight.parse().ok()?, italic: *italic == "1" },
                    rgb(color)?,
                    if *on == "-" { None } else { Some(rgb(on)?) },
                )),
                _ => None,
            })
            .collect();
        ArtList { screen: menu::screen::ScreenArt::parse_lines(text), fonts }
    }
}

/// A group of items of one priority.
struct Tier {
    name: &'static str,
    items: VecDeque<Item>,
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
pub fn game_key(world: &Civ3World) -> (i32, i32, u32, usize, usize) {
    (world.width, world.height, world.seed, world.unit_type_keys.len(), world.era_keys.len())
}

/// The budget of atlas memory for art made ahead: `CIV3R_PRELOAD_MIB`, else
/// `default_mib`; `CIV3R_PRELOAD=off`: none.
pub fn budget(default_mib: u64) -> u64 {
    if std::env::var("CIV3R_PRELOAD").as_deref() == Ok("off") {
        return 0;
    }
    std::env::var("CIV3R_PRELOAD_MIB").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(default_mib) << 20
}

impl Preload {
    /// Nothing to make.
    pub fn none() -> Preload {
        Preload { tiers: VecDeque::new(), budget: 0, stopped: None, started: std::time::Instant::now(), sent: (0, 0), finished: true }
    }

    /// The plan for the game `world` shows in `view` (`budget`: bytes).
    pub fn plan(world: &Civ3World, view: &Civ3View, art: &mut UnitArt, list: &ArtList, budget: u64) -> Preload {
        let mut tiers = VecDeque::new();
        let mut planned: BTreeSet<(String, Recolor)> = BTreeSet::new();
        // Every animation of `type_key` in `era`, in `colors`, not planned yet.
        let mut animations = |art: &mut UnitArt, type_key: &str, era: &str, colors: &[Recolor], out: &mut VecDeque<Item>| {
            let mut files: Vec<AnimFile> = Vec::new();
            for a in ANIMATIONS {
                if let Some(f) = art.file(type_key, era, a) {
                    if !files.iter().any(|g| g.path == f.path) {
                        files.push(f.clone());
                    }
                }
            }
            for f in files {
                let new: BTreeSet<Recolor> = colors.iter().copied().filter(|c| planned.insert((f.path.clone(), *c))).collect();
                if !new.is_empty() {
                    out.push_back(Item::Unit(f, new));
                }
            }
        };
        // The units on screen, then the others.
        let on_screen = |x: i32, y: i32| {
            let within = |v: i32, (lo, hi): (i32, i32), size: i32| (lo..=hi).contains(&v) || (size > 0 && ((lo..=hi).contains(&(v + size)) || (lo..=hi).contains(&(v - size))));
            within(x, view.tile_x, world.width) && within(y, view.tile_y, world.height)
        };
        let mut seen_units = VecDeque::new();
        let mut other_units = VecDeque::new();
        for u in &world.units {
            let out = if on_screen(u.x, u.y) { &mut seen_units } else { &mut other_units };
            let Some(type_key) = world.unit_type_keys.get(u.unit_type.max(0) as usize) else { continue };
            let era = units::era_of(world, u.civ).to_owned();
            animations(art, type_key, &era, &[units::recolor_of(world, u)], out);
            if let Some(member) = units::member_of(world, u) {
                if let Some(type_key) = world.unit_type_keys.get(member.unit_type.max(0) as usize) {
                    animations(art, type_key, &era, &[Recolor::civ(units::colors_of(world, u.civ))], out);
                }
            }
        }
        tiers.push_back(Tier { name: "units on screen", items: seen_units });
        tiers.push_back(Tier { name: "units present", items: other_units });
        // The interface: the list's screen art (a batch per image file, as
        // a screen draws them) and fonts.
        let mut interface = VecDeque::new();
        let mut by_file: std::collections::BTreeMap<&str, menu::screen::ScreenArt> = Default::default();
        for r in &list.screen.regions {
            by_file.entry(r.source.as_str()).or_default().regions.insert(r.clone());
        }
        interface.extend(by_file.into_values().map(Item::Screen));
        let mut rest = menu::screen::ScreenArt::default();
        rest.fills = list.screen.fills.clone();
        rest.dynamic = list.screen.dynamic.clone();
        if !rest.is_empty() {
            interface.push_back(Item::Screen(rest));
        }
        interface.extend(list.fonts.iter().map(|(s, c, b)| Item::Font(s.clone(), *c, *b)));
        tiers.push_back(Tier { name: "interface", items: interface });
        // Every unit type in every era, in the colours of the civs present
        // (with palettes, more colourings cost no pixels).
        let civs: BTreeSet<i32> = world.units.iter().map(|u| u.civ).chain(world.cities.iter().map(|c| c.civ)).collect();
        let colors: Vec<Recolor> = civs.iter().map(|&c| Recolor::civ(units::colors_of(world, c))).collect::<BTreeSet<_>>().into_iter().collect();
        let mut types = VecDeque::new();
        let player_era = units::era_of(world, world.player).to_owned();
        let mut eras: Vec<&str> = vec![player_era.as_str()];
        eras.extend(world.era_keys.iter().map(String::as_str).filter(|e| *e != player_era));
        for era in eras {
            for type_key in &world.unit_type_keys {
                animations(art, type_key, era, &colors, &mut types);
            }
        }
        tiers.push_back(Tier { name: "unit types", items: types });
        let total: u64 = tiers.iter().flat_map(|t| &t.items).map(Item::bytes).sum();
        log::info!(
            "preload: planned {} ({} MiB of pixels), budget {} MiB",
            tiers.iter().map(|t| format!("{} {}", t.items.len(), t.name)).collect::<Vec<_>>().join(", "),
            total >> 20,
            budget >> 20
        );
        Preload { tiers, budget, stopped: None, started: std::time::Instant::now(), sent: (0, 0), finished: budget == 0 }
    }

    /// Bytes of pixels the plan has left to make (estimated).
    pub fn left(&self) -> u64 {
        self.tiers.iter().flat_map(|t| &t.items).map(Item::bytes).sum()
    }

    /// The items of the next addition: from the first tier with any, about
    /// `chunk` bytes of them (at least one), unless that would take the art
    /// (`used` bytes of atlas now) past the budget: then nothing more, ever
    /// (what comes later in the order stays out). `wanted` says whether an
    /// item is wanted already (on demand): those are left out.
    pub fn next(&mut self, used: u64, chunk: u64, mut wanted: impl FnMut(&Item) -> bool) -> Option<(&'static str, Vec<Item>)> {
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
            let mut bytes = 0;
            let mut over = false;
            while let Some(item) = tier.items.front() {
                if wanted(item) {
                    tier.items.pop_front();
                    continue;
                }
                let b = item.bytes();
                if !items.is_empty() && bytes + b > chunk {
                    break;
                }
                if used + bytes + b > self.budget {
                    over = true;
                    break;
                }
                bytes += b;
                items.push(tier.items.pop_front().expect("looked at"));
            }
            let name = tier.name;
            if over {
                let left: Vec<String> = self.tiers.iter().map(|t| format!("{} {}", t.items.len(), t.name)).collect();
                let why = format!("at the budget of {} MiB (the art has {} MiB; left: {})", self.budget >> 20, used >> 20, left.join(", "));
                log::info!("preload: stopped {why}, after {:.1} s", self.started.elapsed().as_secs_f64());
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
            self.sent = (self.sent.0 + items.len(), self.sent.1 + bytes);
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

    fn file(path: &str, frames: u32) -> AnimFile {
        AnimFile { path: path.into(), frames_per_direction: 1, chunks: frames, size: (10, 10), anchor: (0, 0) }
    }

    fn preload(budget: u64) -> Preload {
        let unit = |p: &str| Item::Unit(file(p, 10), [Recolor::civ(1)].into());
        let tiers = [("a", vec![unit("a1"), unit("a2"), unit("a3")]), ("b", vec![unit("b1")])]
            .into_iter()
            .map(|(name, items)| Tier { name, items: items.into() })
            .collect();
        Preload { tiers, budget, stopped: None, started: std::time::Instant::now(), sent: (0, 0), finished: false }
    }

    #[test]
    fn items_come_by_priority_in_chunks_and_skip_what_is_wanted() {
        let mut p = preload(1 << 20);
        // 1000 bytes an item: two to a chunk of 2500.
        let (tier, items) = p.next(0, 2500, |_| false).unwrap();
        assert_eq!((tier, items.len()), ("a", 2));
        // a3 is wanted already: on to the next tier.
        let (tier, items) = p.next(2000, 2500, |i| matches!(i, Item::Unit(f, _) if f.path == "a3")).unwrap();
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
    fn art_lists_have_regions_and_fonts() {
        let style = FontStyle { face: "Lucida Sans".into(), height: 12, weight: 700, italic: false };
        let text = format!("region\tArt/a.pcx\t0,0,4,4\tk\t-\t0,0\t-\t0,0\n{}\n", font_line(&style, [1, 2, 3], Some([255, 0, 255])));
        let list = ArtList::parse(&text);
        assert_eq!(list.screen.regions.len(), 1);
        assert_eq!(list.fonts, vec![(style, [1, 2, 3], Some([255, 0, 255]))]);
        assert!(ArtList::parse(BUILTIN_LIST).screen.regions.len() + ArtList::parse(BUILTIN_LIST).fonts.len() < usize::MAX);
    }
}
