//! One reusable planner. Requests and completed phases each occupy one slot;
//! changing games cancels the current scan without parking the publisher.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use civ3_assets::GameFiles;
use civ3r_sync::{Latest, Wake};
use menu::screen::{Region, ScreenArt};

use super::{ArtList, Preload};
use crate::art::ArtStop;
use crate::units::UnitArt;
use crate::world::{Civ3View, Civ3World};

struct Request {
    epoch: u64,
    world: Civ3World,
    view: Civ3View,
    list: ArtList,
    files: GameFiles,
    folders: ArtStop,
    budget: u64,
}

struct Ready {
    epoch: u64,
    folders: ArtStop,
    plan: Preload,
}

#[derive(Default)]
struct Shared {
    requests: Latest<Request>,
    interface: Latest<Ready>,
    scenario: Latest<Ready>,
    units: Latest<Ready>,
    epoch: AtomicU64,
    closed: AtomicBool,
    wake: Wake,
}

/// At most one worker, one pending game, and one result per planning phase.
/// File headers are read only on the worker. No GPU or publisher waits occur.
pub struct Planner {
    shared: Arc<Shared>,
    interface_taken: bool,
    scenario_taken: bool,
}

impl Planner {
    pub fn new() -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("civ3r-preload-plan".into())
            .spawn(move || run(worker))?;
        Ok(Self {
            shared,
            interface_taken: false,
            scenario_taken: false,
        })
    }

    pub fn request(
        &mut self,
        world: Civ3World,
        view: Civ3View,
        list: ArtList,
        files: GameFiles,
        folders: ArtStop,
        budget: u64,
    ) {
        self.cancel();
        if budget == 0 {
            return;
        }
        let epoch = self.shared.epoch.load(Ordering::Acquire);
        self.shared.requests.publish(Request {
            epoch,
            world,
            view,
            list,
            files,
            folders,
            budget,
        });
        self.shared.wake.ring();
    }

    pub fn cancel(&mut self) {
        self.interface_taken = false;
        self.scenario_taken = false;
        self.shared.epoch.fetch_add(1, Ordering::AcqRel);
        self.shared.requests.clear();
        self.shared.interface.clear();
        self.shared.scenario.clear();
        self.shared.units.clear();
    }

    /// File scans alone do not require a publisher scene rebuild or a GPU
    /// upload status query. A completed phase wakes it when work is available.
    pub fn ready(&self) -> bool {
        self.shared.interface.load().is_some()
            || self.shared.scenario.load().is_some()
            || self.shared.units.load().is_some()
    }

    /// Interface is always taken before units, even when both finished
    /// between publisher rounds. Late canceled results are never adopted.
    pub fn take(&mut self) -> [Option<Preload>; 3] {
        let take = |slot: &Latest<Ready>| {
            // These slots have one consumer and the worker publishes owned
            // values without retaining an Arc, so taking never copies a plan.
            let ready = Arc::try_unwrap(slot.take()?).ok()?;
            (ready.epoch == self.shared.epoch.load(Ordering::Acquire)
                && !ready.folders.superseded())
            .then_some(ready.plan)
        };
        let interface = take(&self.shared.interface);
        self.interface_taken |= interface.is_some();
        let scenario = self
            .interface_taken
            .then(|| take(&self.shared.scenario))
            .flatten();
        self.scenario_taken |= scenario.is_some();
        let units = self
            .scenario_taken
            .then(|| take(&self.shared.units))
            .flatten();
        [interface, scenario, units]
    }
}

impl Drop for Planner {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
        self.cancel();
        self.shared.wake.ring();
    }
}

fn run(shared: Arc<Shared>) {
    while !shared.closed.load(Ordering::Acquire) {
        let Some(request) = shared.requests.take() else {
            shared.wake.wait(crate::BOUNDED_WAIT);
            continue;
        };
        let stop = || {
            shared.closed.load(Ordering::Acquire)
                || shared.epoch.load(Ordering::Acquire) != request.epoch
                || request.folders.superseded()
        };
        if stop() {
            continue;
        }
        let mut sources = Sources::new(&request.files);
        let mut list = request.list.clone();
        if !sources.retain(&mut list.screen, &stop) {
            continue;
        }
        shared.interface.publish(Ready {
            epoch: request.epoch,
            folders: request.folders.clone(),
            plan: Preload::interface(&list, request.budget),
        });
        crate::presenter::WAKE.ring();
        // Scenario-specific small icons arrive before unit enumeration too.
        let Some(icons) = sources.scenario_icons(&stop) else {
            continue;
        };
        if stop() {
            continue;
        }
        shared.scenario.publish(Ready {
            epoch: request.epoch,
            folders: request.folders.clone(),
            plan: Preload::interface(
                &ArtList {
                    screen: icons,
                    fonts: Vec::new(),
                },
                request.budget,
            ),
        });
        crate::presenter::WAKE.ring();
        if stop() {
            continue;
        }
        let mut art = UnitArt::new(request.files.clone());
        if let Some(plan) = Preload::units(
            &request.world,
            &request.view,
            &mut art,
            request.budget,
            &stop,
        ) {
            if !stop() {
                shared.units.publish(Ready {
                    epoch: request.epoch,
                    folders: request.folders.clone(),
                    plan,
                });
                crate::presenter::WAKE.ring();
            }
        }
    }
}

/// Header metadata only. Atlas pixels remain under the normal loader's
/// chunk admission, upload acknowledgments and memory caps.
struct Sources<'a> {
    files: &'a GameFiles,
    sizes: BTreeMap<String, Option<(u32, u32)>>,
}

impl<'a> Sources<'a> {
    fn new(files: &'a GameFiles) -> Self {
        Self {
            files,
            sizes: BTreeMap::new(),
        }
    }

    fn size(&mut self, name: &str) -> Option<(u32, u32)> {
        let normalized = name.replace('\\', "/").to_ascii_lowercase();
        *self.sizes.entry(normalized).or_insert_with(|| {
            let mut header = [0; 128];
            std::fs::File::open(self.files.resolve(name).ok()?)
                .ok()?
                .read_exact(&mut header)
                .ok()?;
            let (width, height) = civ3_assets::pcx::dimensions(&header).ok()?;
            // Inverted PCX bounds wrap in the generic header reader. Reject
            // them here, before speculative source regions are admitted.
            (width > 0 && height > 0 && width <= 65536 && height <= 65536)
                .then_some((width, height))
        })
    }

    fn region(&mut self, region: &Region) -> bool {
        let fits = |rect: [i32; 4], size: (u32, u32)| {
            let [x, y, w, h] = rect;
            x >= 0
                && y >= 0
                && w > 0
                && h > 0
                && i64::from(x) + i64::from(w) <= i64::from(size.0)
                && i64::from(y) + i64::from(h) <= i64::from(size.1)
        };
        if !self
            .size(&region.source)
            .is_some_and(|s| fits(region.rect, s))
        {
            return false;
        }
        // Alpha masks repeat independently of source bounds, so any offset
        // is valid once their nonempty source image exists.
        if let Some((mask, _)) = &region.mask {
            if self.size(mask).is_none() {
                return false;
            }
        }
        region.palette.as_ref().is_none_or(|p| {
            p.first.checked_add(p.count).is_some_and(|end| end <= 256)
                && self.size(&p.image).is_some()
        })
    }

    fn retain(&mut self, art: &mut ScreenArt, stop: &dyn Fn() -> bool) -> bool {
        art.regions.retain(|r| !stop() && self.region(r));
        art.resized_regions
            .retain(|(r, _, _)| !stop() && self.region(r));
        art.indexed.retain(|(r, indices)| {
            !stop()
                && self.region(r)
                && indices
                    .palette_alpha
                    .as_ref()
                    .is_none_or(|p| self.size(&p.source).is_some())
        });
        !stop()
    }

    fn scenario_icons(&mut self, stop: &dyn Fn() -> bool) -> Option<ScreenArt> {
        let mut art = ScreenArt::default();
        let Ok(path) = self.files.resolve("Text/PediaIcons.txt") else {
            return Some(art);
        };
        let Ok(file) = std::fs::File::open(path) else {
            return Some(art);
        };
        let mut text = Vec::new();
        if file.take(4 << 20).read_to_end(&mut text).is_err() {
            return Some(art);
        }
        let mut names = BTreeSet::new();
        for line in text.split(|&b| b == b'\n') {
            if stop() {
                return None;
            }
            let name: String = line.iter().map(|&b| char::from(b)).collect();
            let name = name.trim().replace('\\', "/");
            if name.starts_with(['#', ';']) || !name.to_ascii_lowercase().ends_with(".pcx") {
                continue;
            }
            if names.len() >= 8192 {
                break;
            }
            if !names.insert(name.to_ascii_lowercase()) {
                continue;
            }
            let Some((w, h)) = self.size(&name) else {
                continue;
            };
            // Whole small Pedia images are the exact regions used for unit,
            // building, advance and resource icons. Large sheets and arbitrary
            // future crops are deliberately left to normal on-demand loading.
            if w > 256 || h > 256 {
                continue;
            }
            for keyed in [true, false] {
                art.regions.insert(Region {
                    source: name.clone(),
                    rect: [0, 0, w as i32, h as i32],
                    keyed,
                    mask: None,
                    palette: None,
                });
            }
        }
        Some(art)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use menu::screen::PaletteFrom;

    struct Fixture(std::path::PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "civ3r-preload-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn pcx_header(&self, name: &str, width: u16, height: u16) {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut header = [0u8; 128];
            header[0] = 10;
            header[2] = 1;
            header[3] = 8;
            header[8..10].copy_from_slice(&(width - 1).to_le_bytes());
            header[10..12].copy_from_slice(&(height - 1).to_le_bytes());
            header[65] = 1;
            std::fs::write(path, header).unwrap();
        }
        fn files(&self) -> GameFiles {
            GameFiles::with_roots(vec![self.0.clone()])
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn region(source: &str, rect: [i32; 4]) -> Region {
        Region {
            source: source.into(),
            rect,
            keyed: true,
            mask: None,
            palette: None,
        }
    }

    fn folders() -> ArtStop {
        ArtStop {
            started: 0,
            now: Arc::new(AtomicU64::new(0)),
        }
    }

    fn planner() -> Planner {
        Planner {
            shared: Arc::new(Shared::default()),
            interface_taken: false,
            scenario_taken: false,
        }
    }

    #[test]
    fn speculative_source_regions_require_valid_bounds_and_dependencies() {
        let fixture = Fixture::new();
        fixture.pcx_header("Art/Small.pcx", 32, 20);
        fixture.pcx_header("Art/mask.pcx", 8, 8);
        let files = fixture.files();
        let mut sources = Sources::new(&files);
        assert!(sources.region(&region(r"art\small.PCX", [0, 0, 32, 20])));
        for rect in [
            [0, 0, 33, 20],
            [-1, 0, 32, 20],
            [0, 0, 0, 20],
            [i32::MAX, 0, i32::MAX, 20],
        ] {
            assert!(!sources.region(&region("Art/Small.pcx", rect)));
        }
        let mut masked = region("Art/Small.pcx", [0, 0, 8, 8]);
        masked.mask = Some(("Art/mask.pcx".into(), (0, 0)));
        assert!(sources.region(&masked));
        masked.mask.as_mut().unwrap().1 = (100, -40);
        assert!(
            sources.region(&masked),
            "the loader repeats alpha masks at arbitrary offsets"
        );
        masked.mask.as_mut().unwrap().0 = "missing-mask.pcx".into();
        assert!(!sources.region(&masked));
        let mut palette = region("Art/Small.pcx", [0, 0, 8, 8]);
        palette.palette = Some(PaletteFrom {
            image: "missing.pcx".into(),
            first: 0,
            count: 64,
        });
        assert!(!sources.region(&palette));
        palette.palette.as_mut().unwrap().image = "Art/Small.pcx".into();
        assert!(sources.region(&palette));
        palette.palette.as_mut().unwrap().first = 250;
        assert!(!sources.region(&palette));
    }

    #[test]
    fn scenario_icons_use_real_small_image_dimensions_and_ignore_sheets_missing_and_duplicates() {
        let fixture = Fixture::new();
        fixture.pcx_header("Art/Icons/New unit.pcx", 32, 24);
        fixture.pcx_header("Art/Icons/large.pcx", 128, 128);
        fixture.pcx_header("Art/sheet.pcx", 1024, 768);
        std::fs::create_dir_all(fixture.0.join("Text")).unwrap();
        std::fs::write(fixture.0.join("Text/PediaIcons.txt"), "#ICON_PRTO_new\nArt\\Icons\\New unit.pcx\nart/icons/NEW UNIT.PCX\n; ignored.pcx\nArt/Icons/large.pcx\nArt/sheet.pcx\nArt/missing.pcx\n").unwrap();
        let files = fixture.files();
        let icons = Sources::new(&files).scenario_icons(&|| false).unwrap();
        assert_eq!(
            icons.regions.len(),
            4,
            "two transparency modes for each of two real small icons"
        );
        assert!(
            icons
                .regions
                .contains(&region("Art/Icons/New unit.pcx", [0, 0, 32, 24]))
        );
        assert!(
            icons
                .regions
                .contains(&region("Art/Icons/large.pcx", [0, 0, 128, 128]))
        );
        assert!(Sources::new(&files).scenario_icons(&|| true).is_none());
    }

    #[test]
    fn requests_coalesce_to_one_game_and_zero_budget_queues_nothing() {
        let mut planner = planner();
        for seed in 0..1000 {
            planner.request(
                Civ3World {
                    seed,
                    ..Default::default()
                },
                Civ3View::default(),
                ArtList::default(),
                GameFiles::with_roots(Vec::new()),
                folders(),
                1024,
            );
        }
        assert!(planner.shared.requests.load().is_some());
        assert!(
            !planner.ready(),
            "scanning alone cannot force scene preparation"
        );
        let last = planner.shared.requests.take().unwrap();
        assert_eq!(last.world.seed, 999);
        assert_eq!(last.epoch, planner.shared.epoch.load(Ordering::Acquire));
        assert!(planner.shared.requests.take().is_none());
        planner.request(
            Civ3World::default(),
            Civ3View::default(),
            ArtList::default(),
            GameFiles::with_roots(Vec::new()),
            folders(),
            0,
        );
        assert!(planner.shared.requests.take().is_none());
        assert!(!planner.ready());
    }

    #[test]
    fn phases_cannot_overtake_the_interface_or_revive_canceled_folders() {
        let mut planner = planner();
        planner.request(
            Civ3World::default(),
            Civ3View::default(),
            ArtList::default(),
            GameFiles::with_roots(Vec::new()),
            folders(),
            1024,
        );
        let epoch = planner.shared.epoch.load(Ordering::Acquire);
        let stop = folders();
        let ready = || Ready {
            epoch,
            folders: stop.clone(),
            plan: Preload::interface(&ArtList::default(), 1024),
        };
        planner.shared.scenario.publish(ready());
        planner.shared.units.publish(ready());
        assert!(
            planner.take().iter().all(Option::is_none),
            "a mid-take publisher race cannot lose a later phase"
        );
        planner.shared.interface.publish(ready());
        assert!(planner.take().iter().all(Option::is_some));
        planner.cancel();
        planner.shared.interface.publish(ready());
        assert!(planner.take().iter().all(Option::is_none));
        let epoch = planner.shared.epoch.load(Ordering::Acquire);
        stop.now.fetch_add(1, Ordering::Release);
        planner.shared.interface.publish(Ready {
            epoch,
            folders: stop,
            plan: Preload::none(),
        });
        assert!(planner.take().iter().all(Option::is_none));
    }

    #[test]
    fn real_wwii_scenario_prewarm_is_header_only_and_stays_under_admission_cap() {
        let install =
            std::path::PathBuf::from(std::env::var("CIV3_INSTALL").unwrap_or_else(|_| {
                "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into()
            }));
        if !install.is_dir() {
            return eprintln!("no Civ III installation, skipped");
        }
        let preview =
            crate::preview::load(&install, "Conquests/Conquests/9 WWII European Front 6 .biq")
                .unwrap();
        let files = crate::assets::game_files(
            &install,
            preview.scenario_folder.as_deref(),
            &preview.search_folders,
        );
        let mut sources = Sources::new(&files);
        let mut list = ArtList::parse(super::super::BUILTIN_LIST);
        assert!(sources.retain(&mut list.screen, &|| false));
        let icons = sources.scenario_icons(&|| false).unwrap();
        assert!(
            icons.regions.len() > 1000,
            "scenario PediaIcons extend the fixed Greece manifest"
        );
        assert!(
            icons
                .regions
                .iter()
                .all(|r| r.rect[2] <= 256 && r.rect[3] <= 256)
        );
        let budget = 12 << 20;
        let mut plan = Preload::interface(&list, budget);
        plan.append(Preload::interface(
            &ArtList {
                screen: icons,
                fonts: Vec::new(),
            },
            budget,
        ));
        let mut used = 0u64;
        while let Some((_, items)) = plan.next(used, 3 << 20, |_| false) {
            used += items.iter().map(super::super::Item::bytes).sum::<u64>();
            assert!(used <= budget);
        }
        assert!(plan.finished());
    }
}
