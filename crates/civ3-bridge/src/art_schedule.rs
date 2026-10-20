//! When to build newly requested render art.
//!
//! New art is wanted whenever the game shows something the art does not
//! have yet (a unit, a font, a region of a screen's art). A new screen asks
//! for it over several frames, so a build (an addition, `crate::art`) waits
//! until the requests settle (or have waited long enough), runs one at a
//! time (off the publishing thread), and covers every request made before
//! it started.
//! [`ArtSchedule::screen_pending`] distinguishes a pending screen request
//! from a completed request whose art could not be compiled. Publication
//! reports visible-item readiness without delaying committed UI commands.

use std::time::{Duration, Instant};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Timing {
    /// Least time from the end of one rebuild to the start of the next.
    pub gap: Duration,
    /// A rebuild starts once no art was asked for this long…
    pub quiet: Duration,
    /// …or the first request waited this long.
    pub wait_max: Duration,
    /// A request for visible screen art starts once none came this long.
    /// It starts immediately so independently arriving sprites appear promptly.
    pub screen_quiet: Duration,
}

impl Timing {
    /// Art that grows (`crate::art`): an addition costs only its own art,
    /// so it starts once the requests of a frame or two are in.
    pub const GROWING: Timing =
        Timing { gap: Duration::ZERO, quiet: Duration::from_millis(35), wait_max: Duration::from_millis(150), screen_quiet: Duration::ZERO };
}

#[derive(Debug)]
pub struct ArtSchedule {
    timing: Timing,
    /// Requests so far (numbered from 1), and the last one the art covers.
    asked: u64,
    covered: u64,
    /// The last request for the screen's art.
    screen_asked: u64,
    /// The rebuild running: the last request it covers.
    running: Option<u64>,
    first_wanted: Option<Instant>,
    last_wanted: Option<Instant>,
    last_done: Option<Instant>,
}

impl ArtSchedule {
    pub fn new(timing: Timing) -> Self {
        Self { timing, asked: 0, covered: 0, screen_asked: 0, running: None, first_wanted: None, last_wanted: None, last_done: None }
    }

    /// New art is wanted (`screen`: for what the game's screen shows).
    pub fn want(&mut self, now: Instant, screen: bool) {
        self.asked += 1;
        self.first_wanted.get_or_insert(now);
        self.last_wanted = Some(now);
        if screen {
            self.screen_asked = self.asked;
        }
    }

    /// Whether art is wanted that is not built yet (nor being built).
    pub fn pending(&self) -> bool {
        self.asked > self.running.unwrap_or(self.covered)
    }

    /// Whether a screen-art request has not completed yet. This drives
    /// retries, never publication, and unrelated map requests cannot extend it.
    pub fn screen_pending(&self) -> bool {
        self.screen_asked > self.covered
    }

    /// How many requests there were so far (the last one's number, see
    /// [`ArtSchedule::start`]).
    pub fn asked(&self) -> u64 {
        self.asked
    }

    /// Whether a rebuild is running.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// Whether to start a rebuild now: `Some(token)` for [`ArtSchedule::done`].
    pub fn start(&mut self, now: Instant) -> Option<u64> {
        if self.running.is_some() || !self.pending() {
            return None;
        }
        let settled = self.last_wanted.is_some_and(|t| now.duration_since(t) >= self.timing.quiet)
            || self.first_wanted.is_some_and(|t| now.duration_since(t) >= self.timing.wait_max)
            || (self.screen_asked > self.covered && self.last_wanted.is_some_and(|t| now.duration_since(t) >= self.timing.screen_quiet));
        let rested = self.last_done.is_none_or(|t| now.duration_since(t) >= self.timing.gap);
        if !(settled && rested) {
            return None;
        }
        self.running = Some(self.asked);
        // Requests from now on wait for the next rebuild.
        self.first_wanted = None;
        Some(self.asked)
    }

    /// The rebuild `token` ended (also if it failed: its requests are
    /// not asked again).
    pub fn done(&mut self, now: Instant, token: u64) {
        if self.running == Some(token) {
            self.running = None;
        }
        self.covered = self.covered.max(token);
        self.last_done = Some(now);
        if self.pending() && self.first_wanted.is_none() {
            self.first_wanted = self.last_wanted;
        }
    }


}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Timing = Timing {
        gap: Duration::from_millis(500),
        quiet: Duration::from_millis(70),
        wait_max: Duration::from_millis(300),
        screen_quiet: Duration::from_millis(70),
    };

    fn ms(t0: Instant, n: u64) -> Instant {
        t0 + Duration::from_millis(n)
    }

    #[test]
    fn a_rebuild_waits_for_requests_to_settle() {
        let t0 = Instant::now();
        let mut s = ArtSchedule::new(T);
        assert_eq!(s.start(t0), None);
        s.want(t0, false);
        assert_eq!(s.start(ms(t0, 33)), None);
        s.want(ms(t0, 33), false);
        assert_eq!(s.start(ms(t0, 66)), None);
        assert_eq!(s.start(ms(t0, 110)), Some(2));
        assert!(s.running() && !s.pending());
        // One at a time; a request meanwhile waits for the next.
        s.want(ms(t0, 120), false);
        assert!(s.pending());
        assert_eq!(s.start(ms(t0, 400)), None);
        s.done(ms(t0, 400), 2);
        // Not before the gap since the last ended.
        assert_eq!(s.start(ms(t0, 800)), None);
        assert_eq!(s.start(ms(t0, 900)), Some(3));
        s.done(ms(t0, 950), 3);
        assert!(!s.pending());
        assert_eq!(s.start(ms(t0, 2000)), None);
    }

    #[test]
    fn requests_that_never_settle_still_get_built() {
        let t0 = Instant::now();
        let mut s = ArtSchedule::new(Timing { gap: Duration::ZERO, ..T });
        let mut started = None;
        for i in 0..20 {
            let now = ms(t0, 33 * i);
            s.want(now, false);
            if let Some(token) = s.start(now) {
                started = Some((i, token));
                break;
            }
        }
        // The first request waited 300 ms: at the tenth frame.
        assert_eq!(started, Some((10, 11)));
    }

    #[test]
    fn a_replacement_whole_atlas_keeps_missing_screen_art_pending() {
        let t0 = Instant::now();
        let mut schedule = ArtSchedule::new(Timing::GROWING);
        schedule.want(t0, true);
        let first = schedule.start(ms(t0, 40)).unwrap();
        // A scenario's optional atlas supersedes the first whole build, but
        // it also carries every mandatory menu item the first had promised.
        schedule.want(ms(t0, 50), true);
        schedule.done(ms(t0, 60), first);
        assert!(schedule.screen_pending());
        let replacement = schedule.start(ms(t0, 100)).unwrap();
        assert!(schedule.screen_pending());
        schedule.done(ms(t0, 200), replacement);
        assert!(!schedule.screen_pending());
    }

    #[test]
    fn pending_screen_requests_track_completion_independently_of_map_requests() {
        let t0 = Instant::now();
        let mut schedule = ArtSchedule::new(Timing { gap: Duration::ZERO, ..T });
        schedule.want(t0, false);
        assert!(!schedule.screen_pending());
        let token = schedule.start(ms(t0, 100)).unwrap();
        schedule.done(ms(t0, 150), token);
        schedule.want(ms(t0, 200), true);
        assert!(schedule.screen_pending());
        let token = schedule.start(ms(t0, 300)).unwrap();
        schedule.want(ms(t0, 310), false);
        assert!(schedule.screen_pending());
        schedule.done(ms(t0, 500), token);
        assert!(!schedule.screen_pending(), "map work cannot retain a completed screen request");
        assert!(schedule.pending());
    }
}
