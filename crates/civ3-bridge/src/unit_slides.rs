//! Units sliding from tile to tile, shown by the renderer at its own frame
//! rate (docs/independent-animations.md, "Unit slides").
//!
//! With `animate movement` on, the game slides a moving unit across to the
//! next tile (`civ3_sys::unit_slide`): nine steps over 100 ms (a tenth of
//! that with Shift), each followed by a whole animator frame, unless that
//! step is already late. The game never waits: the slide lasts as long as
//! its frames take. Natively (Linux) nine frames take about 12 ms, so the
//! unit jumps; emulated (FEX on the Mac) a frame takes so long that most
//! are skipped, and the unit shows in one or two places a tile.
//!
//! Here the game's thread records each slide when it starts (the unit,
//! its tile, the way to the next, its length after the Shift rule). The
//! slide draws one frame, at its first step (the unit under the moving one
//! joins the animator's list, the walk starts), and waits until each later
//! step is due instead of drawing a frame for it: it takes its 100 ms in
//! the game as well, unless that one frame takes longer.
//! The unit's walk (its slides in a row, with how many tiles its go-to path
//! still plans) goes to the renderer that moment, not with the next scene
//! (`crate::presenter::push`, `scene::Update::Walk`); the publisher draws
//! the unit to be moved (`scene::Motion`) and the renderer plays the walk
//! on its own clock from the game's starts (`tile_viewer::piece_motions`).
//! A game with its own pixels shown (no parked camera, the comparison modes) keeps its
//! frames.

use std::time::{Duration, Instant};

/// One slide, as the game started it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Slide {
    /// Increasing, never 0; a walk is told as its first slide's.
    pub serial: u64,
    pub unit: i32,
    /// The unit's tile and the world pixel of its anchor (full size) when
    /// the slide began.
    pub from_tile: (i32, i32),
    pub from_pixel: (i32, i32),
    /// The way to the next tile (world pixels, full size).
    pub way: (i32, i32),
    pub started: Instant,
    /// Its length (ms), after the Shift rule.
    pub ms: u32,
    /// How many more tiles the unit's path plans after this one.
    pub planned: u32,
}

/// How a slide has the publisher draw its unit.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Shown {
    /// On its way (from `from`, the world pixel of its first tile's
    /// anchor), drawn where the game has it: the renderer moves it along
    /// its walk.
    Sliding { serial: u64, from: (i32, i32) },
    /// At the end of its way, before the game has moved it there.
    Held((i32, i32)),
}

/// How long after its end a slide still holds its unit at the next tile,
/// while the game has not moved it there yet (the slide puts its picture
/// back on the old tile; `Unit::move` follows).
const HOLD: Duration = Duration::from_millis(500);

/// How long after its end a slide still has its unit drawn to be moved
/// (and the publisher publishing): the renderer may play it a little
/// behind the game.
const AFTER: Duration = Duration::from_millis(400);

/// Slides of a unit at most this far apart are one walk (as
/// `tile_viewer::piece_motions` plays them).
const WALK: Duration = Duration::from_millis(600);

/// How many of a walk's tiles are told (its last ones).
const TOLD: usize = 64;

impl Slide {
    /// How this slide shows its unit at `now`, the unit standing on
    /// `tile`; `None` once it no longer decides that.
    pub fn shown(&self, now: Instant, tile: (i32, i32)) -> Option<Shown> {
        let elapsed = now.saturating_duration_since(self.started);
        let length = Duration::from_millis(self.ms.into());
        let end = (self.from_pixel.0 + self.way.0, self.from_pixel.1 + self.way.1);
        if tile == self.from_tile && elapsed >= length.mul_f64(8.0 / 9.0) && elapsed < length + HOLD {
            return Some(Shown::Held(end));
        }
        (elapsed < length + AFTER).then_some(Shown::Sliding { serial: self.serial, from: self.from_pixel })
    }
}

/// The slides started lately, newest last.
#[derive(Clone, Default)]
pub struct Slides(Vec<Slide>);

impl Slides {
    /// A new slide; old ones are let go, and the unit's from before its walk.
    pub fn record(&mut self, slide: Slide) {
        let age = |s: &Slide| slide.started.saturating_duration_since(s.started);
        let walk = self.0.iter().rev().find(|s| s.unit == slide.unit).is_some_and(|s| age(s) <= WALK);
        self.0.retain(|s| age(s) < KEEP && (s.unit != slide.unit || walk));
        self.0.push(slide);
    }

    /// The unit's walk ends with its last slide (the game stopped it).
    pub fn ends(&mut self, unit: i32) {
        if let Some(last) = self.0.iter_mut().rev().find(|s| s.unit == unit) {
            last.planned = 0;
        }
    }

    /// Whether a slide still has its unit drawn to be moved at `now`.
    pub fn busy(&self, now: Instant) -> bool {
        self.0.iter().any(|s| now.saturating_duration_since(s.started) < Duration::from_millis(u64::from(s.ms)) + AFTER)
    }

    /// How the slides show their units at `now`, given each unit's (id,
    /// tile): by unit id.
    pub fn shown(&self, now: Instant, units: impl IntoIterator<Item = (i32, (i32, i32))>) -> std::collections::HashMap<i32, Shown> {
        if self.0.is_empty() {
            return Default::default();
        }
        units.into_iter().filter_map(|(id, tile)| {
            let slide = self.0.iter().rev().find(|s| s.unit == id)?;
            Some((id, slide.shown(now, tile)?))
        }).collect()
    }

    /// The walk of `unit` as it is now (`scene::Walk`): its slides at most
    /// [`WALK`] apart, up to the newest (the last [`TOLD`] of them).
    pub fn walk(&self, unit: i32) -> Option<scene::Walk> {
        let mut slides: Vec<&Slide> = self.0.iter().filter(|s| s.unit == unit).collect();
        let newest = *slides.last()?;
        let first = slides.windows(2).rposition(|w| w[1].started.saturating_duration_since(w[0].started) > WALK).map_or(0, |i| i + 1);
        slides.drain(..first.max(slides.len().saturating_sub(TOLD)));
        let origin = slides[0].from_pixel;
        Some(scene::Walk {
            key: unit as u32,
            serial: slides[0].serial,
            origin,
            tiles: slides.iter().map(|s| scene::WalkTile { start_ms: clock_ms(s.started), way: s.way, ms: s.ms }).collect(),
            planned: newest.planned,
        })
    }
}

/// Ms since this process's first call: the clock of the walks' starts
/// (`scene::WalkTile::start_ms`).
pub fn clock_ms(at: Instant) -> u64 {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    at.saturating_duration_since(*EPOCH.get_or_init(Instant::now)).as_millis() as u64
}

/// The game's slides: the game thread alone records them (`record`), the
/// publisher reads the latest copy.
static SLIDES: handoff::Latest<Slides> = handoff::Latest::new();
static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How long a slide is kept after it began.
const KEEP: Duration = Duration::from_secs(5);

fn next_serial() -> u64 {
    SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

fn slides() -> std::sync::Arc<Slides> {
    SLIDES.load().unwrap_or_default()
}

/// Changes the slides (the game's thread, the only writer: the read and
/// the publish do not race) and hands the unit's walk as it is now to the
/// viewer at once (`crate::presenter::push`).
fn change(unit: i32, f: impl FnOnce(&mut Slides)) {
    let mut slides = SLIDES.load().map(|s| (*s).clone()).unwrap_or_default();
    f(&mut slides);
    let walk = slides.walk(unit);
    SLIDES.publish(slides);
    if let Some(walk) = walk {
        crate::presenter::push(vec![scene::Update::Walk(walk)]);
    }
}

/// Whether a slide still has its unit drawn to be moved at `now`: the
/// publisher then goes on publishing, though the game's state may look
/// unchanged to it (a unit beyond the game's camera is read at the
/// animator's frames, which the slide no longer draws).
pub fn busy(now: Instant) -> bool {
    slides().busy(now)
}

/// How the game's slides show their units at `now`, given each unit's
/// (id, tile): by unit id.
pub fn shown(now: Instant, units: impl IntoIterator<Item = (i32, (i32, i32))>) -> std::collections::HashMap<i32, Shown> {
    slides().shown(now, units)
}

#[cfg(windows)]
pub use live::install;

#[cfg(windows)]
mod live {
    use std::cell::Cell;
    use std::time::Instant;

    use civ3_sys::Civ3;
    use civ3_sys::unit_slide::{self as sys, slide, slide_frame};

    use super::*;

    thread_local! {
        /// The slide running on the game's thread: when it began, its
        /// length (ms) and steps, and whether its one frame was drawn.
        static RUNNING: Cell<Option<(Instant, i32, i32, bool)>> = const { Cell::new(None) };
    }

    /// Records the game's slides and keeps them on their schedule (module
    /// docs). Only while the renderer owns the map (the parked camera).
    pub fn install(game: Civ3) -> Result<(), String> {
        let sites = sys::sites(game.build).ok_or_else(|| format!("no unit slide for {:?}", game.build))?;
        let e = |what: &str, error: inproc::hook::HookError| format!("{what}: {error}");
        let start = slide::install_at_start(sites.slide.0, sites.slide.1, move |call| {
            if !civ3_sys::native_camera::active() {
                return call.original();
            }
            let (shift, caps) = inproc::keyboard::shift_and_caps_lock();
            let ms = sys::slide_ms(call.ms as i32, shift, caps);
            let started = Instant::now();
            match game.drawn_unit_at(call.unit) {
                Ok(u) => {
                    let to = (call.to_x as i32, call.to_y as i32);
                    // (The path's planned tiles: the walk plays on without
                    // waiting to learn of each next one.)
                    let planned = sys::go_to(call.unit).map_or(0, |g| g.planned(to));
                    let slide = Slide {
                        serial: next_serial(),
                        unit: u.id,
                        from_tile: (u.x, u.y),
                        from_pixel: (64 * u.x + 64, 32 * u.y + 32),
                        way: sys::slide_way((u.x, u.y), to),
                        started,
                        ms: ms.max(0) as u32,
                        planned,
                    };
                    log::debug!("unit slide: unit {} from {:?} by {:?} in {} ms ({} steps, {planned} planned)", slide.unit, slide.from_tile, slide.way, slide.ms, call.steps);
                    // Told to the viewer now, not with the next scene.
                    change(u.id, |s| s.record(slide));
                }
                Err(error) => log::debug!("unit slide: the unit at {:#x}: {error}", call.unit),
            }
            // The renderer learns of it now, not at the next poll.
            crate::presenter::WAKE.ring();
            let previous = RUNNING.replace(Some((started, ms, call.steps as i32, false)));
            call.original();
            RUNNING.set(previous);
            crate::presenter::WAKE.ring();
        }).map_err(|err| e("unit slide", err))?;
        let frame = slide_frame::install_call_site(sites.frame.0, &sites.frame.1, |call| {
            match RUNNING.get().filter(|_| civ3_sys::native_camera::active()) {
                // The renderer draws the slide; a step waits its turn. The
                // first draws one frame: its list of units has the one
                // under the moving unit, its animation tick starts the
                // moving unit's walk and reads both for the publisher
                // (`crate::unit_views`).
                Some((started, ms, steps, drawn)) => {
                    if !drawn {
                        RUNNING.set(Some((started, ms, steps, true)));
                        call.original();
                    }
                    let elapsed = started.elapsed();
                    let due = Duration::from_millis(sys::step_due_ms(elapsed.as_millis() as u64, ms, steps));
                    if let Some(wait) = due.checked_sub(elapsed).filter(|w| !w.is_zero()) {
                        // (The game's own schedule, not our hook's time.)
                        let _schedule = inproc::hook_time::Original::enter();
                        std::thread::sleep(wait);
                    }
                }
                None => call.original(),
            }
        }).map_err(|err| e("unit slide frame", err))?;
        // After each move: a walk the game stops (its go-to over, or no
        // moves left this turn) is told to end with its last tile.
        let moves_site = civ3_sys::targeting::sites(game.build).ok_or("no unit movement getter")?.max_move_points;
        let max_moves = civ3_sys::targeting::max_move_points::Function::at(moves_site.0, moves_site.1).map_err(|err| e("unit max movement", err))?;
        let _ = civ3_sys::ai_camera_focus::AFTER_MOVE.set(Box::new(move |unit| {
            let Ok(u) = game.drawn_unit_at(unit) else { return };
            let slides = slides();
            let Some(walk) = slides.walk(u.id) else { return };
            if walk.planned == 0 || walk.tiles.last().is_some_and(|t| clock_ms(Instant::now()) > t.start_ms + 1000) {
                return;
            }
            let left = max_moves.call(unit) as i32 - inproc::Ptr32::<i32>::new(unit + 0x50).read().unwrap_or(0);
            let going = sys::go_to(unit).is_ok_and(|g| g.state == sys::GO_TO && g.steps > 0);
            if !going || left <= 0 {
                log::debug!("unit slide: unit {} stops its walk ({left} moves left, going on: {going})", u.id);
                change(u.id, |s| s.ends(u.id));
            }
        }));
        std::mem::forget(start);
        std::mem::forget(frame);
        clock_ms(Instant::now());
        log::info!("unit slides: the renderer plays walks told the moment each tile starts; the game's slide keeps its schedule, drawing one frame");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slide(started: Instant) -> Slide {
        Slide { serial: 3, unit: 7, from_tile: (10, 20), from_pixel: (704, 672), way: (64, 32), started, ms: 100, planned: 2 }
    }

    const SLIDING: Shown = Shown::Sliding { serial: 3, from: (704, 672) };

    #[test]
    fn a_slide_has_its_unit_drawn_to_be_moved_until_a_moment_after_its_end() {
        let t0 = Instant::now();
        let s = slide(t0);
        for ms in [0, 25, 88] {
            assert_eq!(s.shown(t0 + Duration::from_millis(ms), (10, 20)), Some(SLIDING), "{ms} ms in");
        }
        // The game moved it: still on its walk until a while after.
        assert_eq!(s.shown(t0 + Duration::from_millis(120), (11, 21)), Some(SLIDING));
        assert_eq!(s.shown(t0 + Duration::from_millis(499), (11, 21)), Some(SLIDING));
        assert_eq!(s.shown(t0 + Duration::from_millis(500), (11, 21)), None);
    }

    #[test]
    fn a_finished_slide_holds_its_unit_at_the_next_tile_until_the_game_moves_it() {
        let t0 = Instant::now();
        let s = slide(t0);
        assert_eq!(s.shown(t0 + Duration::from_millis(89), (10, 20)), Some(Shown::Held((768, 704))));
        assert_eq!(s.shown(t0 + Duration::from_millis(599), (10, 20)), Some(Shown::Held((768, 704))));
        assert_eq!(s.shown(t0 + Duration::from_millis(600), (10, 20)), None, "a move that never followed lets go");
    }

    #[test]
    fn the_publisher_is_kept_busy_through_a_slide_and_a_while_after() {
        let t0 = Instant::now();
        let mut slides = Slides::default();
        assert!(!slides.busy(t0));
        slides.record(slide(t0));
        assert!(slides.busy(t0 + Duration::from_millis(499)));
        assert!(!slides.busy(t0 + Duration::from_millis(500)));
    }

    #[test]
    fn a_walk_is_its_unit_s_slides_in_a_row_with_the_newest_plan() {
        let t0 = Instant::now();
        let mut slides = Slides::default();
        let at = |n: u64, ms: u64, planned: u32| Slide { serial: n, started: t0 + Duration::from_millis(ms), planned, ..slide(t0) };
        assert_eq!(slides.walk(7), None);
        slides.record(at(1, 0, 2));
        slides.record(Slide { unit: 8, ..at(2, 50, 0) });
        slides.record(Slide { from_pixel: (768, 704), ..at(3, 130, 1) });
        let w = slides.walk(7).unwrap();
        assert_eq!((w.key, w.serial, w.origin, w.planned), (7, 1, (704, 672), 1));
        assert_eq!(w.tiles.iter().map(|t| (t.start_ms, t.way, t.ms)).collect::<Vec<_>>(),
            [(clock_ms(t0), (64, 32), 100), (clock_ms(t0 + Duration::from_millis(130)), (64, 32), 100)]);
        // The game stops it: nothing more planned.
        slides.ends(7);
        assert_eq!(slides.walk(7).unwrap().planned, 0);
        // A second later: a new walk; the old one is let go.
        slides.record(at(4, 2000, 0));
        let w = slides.walk(7).unwrap();
        assert_eq!((w.serial, w.tiles.len()), (4, 1));
    }

    #[test]
    fn the_newest_slide_of_a_unit_decides_how_it_is_drawn() {
        let t0 = Instant::now();
        let mut slides = Slides::default();
        slides.record(slide(t0));
        slides.record(Slide { serial: 4, from_tile: (11, 21), from_pixel: (768, 704), started: t0 + Duration::from_millis(110), ..slide(t0) });
        let shown = slides.shown(t0 + Duration::from_millis(135), [(7, (11, 21)), (5, (1, 1))]);
        assert_eq!(shown.get(&7), Some(&Shown::Sliding { serial: 4, from: (768, 704) }));
        assert!(!shown.contains_key(&5), "units without a slide are left alone");
    }
}
