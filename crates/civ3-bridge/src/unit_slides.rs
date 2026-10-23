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
//! The publisher draws a sliding unit where its slide began and tells the
//! renderer the slide once (`scene::Motion`: the way and the length); the
//! renderer moves the unit along all of it itself, frame by frame, on its
//! own clock from when the telling arrived (a publisher round after the
//! game's start: the picture trails the game by that, never skips). A game with its
//! own pixels shown (no parked camera, the comparison modes) keeps its
//! frames.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One slide, as the game started it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Slide {
    /// Identifies this slide's telling (`scene::Motion::serial`):
    /// increasing, never 0.
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
}

/// How a slide shows its unit in one publication.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Shown {
    /// On its way: drawn at `from` (world pixels, full size), the renderer
    /// moving it by `way` over `ms` from the slide's start (`serial`).
    Sliding { serial: u64, from: (i32, i32), way: (i32, i32), ms: u32 },
    /// At the end of its way, before the game has moved it there.
    Held((i32, i32)),
}

/// How long after its end a slide still holds its unit at the next tile,
/// while the game has not moved it there yet (the slide puts its picture
/// back on the old tile; `Unit::move` follows).
const HOLD: Duration = Duration::from_millis(500);

/// How long after its end a slide is still told: the renderer's clock
/// starts when the telling arrives, as late as a publisher round (tens of
/// ms emulated), and holds the unit at the end of the way once through.
const AFTER: Duration = Duration::from_millis(250);

impl Slide {
    /// How this slide shows its unit at `now`, the unit standing on
    /// `tile`; `None` once it no longer decides that.
    pub fn shown(&self, now: Instant, tile: (i32, i32)) -> Option<Shown> {
        let elapsed = now.saturating_duration_since(self.started);
        let length = Duration::from_millis(self.ms.into());
        if elapsed < length + AFTER {
            return Some(Shown::Sliding { serial: self.serial, from: self.from_pixel, way: self.way, ms: self.ms });
        }
        let end = (self.from_pixel.0 + self.way.0, self.from_pixel.1 + self.way.1);
        (tile == self.from_tile && elapsed < length + HOLD).then_some(Shown::Held(end))
    }
}

/// The slides started lately, newest last.
#[derive(Default)]
pub struct Slides(Vec<Slide>);

impl Slides {
    /// A new slide; the unit's earlier ones and old ones are let go.
    pub fn record(&mut self, slide: Slide) {
        self.0.retain(|s| s.unit != slide.unit && slide.started.saturating_duration_since(s.started) < KEEP);
        self.0.push(slide);
    }

    /// Whether a slide is told at `now`.
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
}

/// The game's slides (game thread in, publisher out).
static SLIDES: Mutex<Slides> = Mutex::new(Slides(Vec::new()));
static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How long a slide is kept after it began.
const KEEP: Duration = Duration::from_secs(5);

fn next_serial() -> u64 {
    SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

fn slides() -> std::sync::MutexGuard<'static, Slides> {
    SLIDES.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether a slide is told at `now`: the publisher then goes on
/// publishing, though the game's state may look unchanged to it (a unit
/// beyond the game's camera is read at the animator's frames, which the
/// slide no longer draws).
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
                    let slide = Slide {
                        serial: next_serial(),
                        unit: u.id,
                        from_tile: (u.x, u.y),
                        from_pixel: (64 * u.x + 64, 32 * u.y + 32),
                        way: sys::slide_way((u.x, u.y), (call.to_x as i32, call.to_y as i32)),
                        started,
                        ms: ms.max(0) as u32,
                    };
                    log::debug!("unit slide: unit {} from {:?} by {:?} in {} ms ({} steps)", slide.unit, slide.from_tile, slide.way, slide.ms, call.steps);
                    slides().record(slide);
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
        std::mem::forget(start);
        std::mem::forget(frame);
        log::info!("unit slides: the renderer moves sliding units; the game's slide keeps its schedule, drawing one frame");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slide(started: Instant) -> Slide {
        Slide { serial: 3, unit: 7, from_tile: (10, 20), from_pixel: (704, 672), way: (64, 32), started, ms: 100 }
    }

    const SLIDING: Shown = Shown::Sliding { serial: 3, from: (704, 672), way: (64, 32), ms: 100 };

    #[test]
    fn a_slide_is_told_once_from_its_start_until_a_moment_after_its_end() {
        let t0 = Instant::now();
        let s = slide(t0);
        for ms in [0, 25, 99, 150, 349] {
            assert_eq!(s.shown(t0 + Duration::from_millis(ms), (10, 20)), Some(SLIDING), "{ms} ms in");
        }
        // The game moved it meanwhile: still the same telling.
        assert_eq!(s.shown(t0 + Duration::from_millis(120), (11, 21)), Some(SLIDING));
    }

    #[test]
    fn after_its_telling_a_slide_holds_the_unit_at_the_next_tile_until_the_game_moves_it() {
        let t0 = Instant::now();
        let s = slide(t0);
        assert_eq!(s.shown(t0 + Duration::from_millis(350), (10, 20)), Some(Shown::Held((768, 704))));
        assert_eq!(s.shown(t0 + Duration::from_millis(599), (10, 20)), Some(Shown::Held((768, 704))));
        // Moved: the game's own position is right again.
        assert_eq!(s.shown(t0 + Duration::from_millis(350), (11, 21)), None);
        // A move that never followed lets go after a while.
        assert_eq!(s.shown(t0 + Duration::from_millis(600), (10, 20)), None);
    }

    #[test]
    fn the_publisher_is_kept_busy_through_a_telling() {
        let t0 = Instant::now();
        let mut slides = Slides::default();
        assert!(!slides.busy(t0));
        slides.record(slide(t0));
        assert!(slides.busy(t0 + Duration::from_millis(50)));
        assert!(slides.busy(t0 + Duration::from_millis(349)));
        assert!(!slides.busy(t0 + Duration::from_millis(350)));
    }

    #[test]
    fn the_newest_slide_of_a_unit_decides() {
        let t0 = Instant::now();
        let mut slides = Slides::default();
        slides.record(slide(t0));
        let next = Slide { serial: 4, from_tile: (11, 21), from_pixel: (768, 704), started: t0 + Duration::from_millis(110), ..slide(t0) };
        slides.record(next);
        let now = t0 + Duration::from_millis(135);
        let shown = slides.shown(now, [(7, (11, 21)), (5, (1, 1))]);
        assert_eq!(shown.get(&7), Some(&Shown::Sliding { serial: 4, from: (768, 704), way: (64, 32), ms: 100 }));
        assert!(!shown.contains_key(&5), "units without a slide are left alone");
        assert_eq!(slides.0.len(), 1, "the older slide was let go");
    }
}
