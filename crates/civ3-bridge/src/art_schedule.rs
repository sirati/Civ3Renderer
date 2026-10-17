//! When to rebuild the render art, and when to hold a scene back meanwhile.
//!
//! New art is wanted whenever the game shows something the description
//! does not have yet (a unit, a font, a region of a screen's art). A new
//! screen asks for it over several frames, so a rebuild waits until the
//! requests settle (or have waited long enough), runs one at a time (off the
//! publishing thread), and covers every request made before it started.
//! While art the game's *screen* needs is missing, the scene is held back:
//! the viewer keeps showing the last complete one rather than a screen with
//! empty panels, for at most [`Timing::hold_max`].

use std::time::{Duration, Instant};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Timing {
    /// Least time from the end of one rebuild to the start of the next.
    pub gap: Duration,
    /// A rebuild starts once no art was asked for this long…
    pub quiet: Duration,
    /// …or the first request waited this long.
    pub wait_max: Duration,
    /// Longest a scene is held back for missing screen art.
    pub hold_max: Duration,
}

impl Timing {
    /// A renderer that builds its own atlas (the native viewer): the
    /// driver's tables are cheap, rebuild as soon as requests settle.
    pub const OWN_ATLAS: Timing =
        Timing { gap: Duration::ZERO, quiet: Duration::from_millis(70), wait_max: Duration::from_millis(300), hold_max: Duration::from_secs(4) };
    /// A renderer given the driver's atlas (in-process): each rebuild makes
    /// a new atlas in a 32-bit process (from the sheets of the last, the new
    /// ones loaded), at most two a second.
    pub const SHARED_ATLAS: Timing =
        Timing { gap: Duration::from_millis(500), quiet: Duration::from_millis(70), wait_max: Duration::from_millis(300), hold_max: Duration::from_secs(4) };
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
    /// Since when screen art has been missing.
    missing_since: Option<Instant>,
}

impl ArtSchedule {
    pub fn new(timing: Timing) -> Self {
        Self { timing, asked: 0, covered: 0, screen_asked: 0, running: None, first_wanted: None, last_wanted: None, last_done: None, missing_since: None }
    }

    /// New art is wanted (`screen`: for what the game's screen shows).
    pub fn want(&mut self, now: Instant, screen: bool) {
        self.asked += 1;
        self.first_wanted.get_or_insert(now);
        self.last_wanted = Some(now);
        if screen {
            self.screen_asked = self.asked;
            self.missing_since.get_or_insert(now);
        }
    }

    /// Whether art is wanted that is not built yet (nor being built).
    pub fn pending(&self) -> bool {
        self.asked > self.running.unwrap_or(self.covered)
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
            || self.first_wanted.is_some_and(|t| now.duration_since(t) >= self.timing.wait_max);
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
        if self.screen_asked <= self.covered {
            self.missing_since = None;
        }
        if self.pending() && self.first_wanted.is_none() {
            self.first_wanted = self.last_wanted;
        }
    }

    /// Whether to hold the scene back: the screen's art is not built yet
    /// (for at most `hold_max`).
    pub fn hold(&self, now: Instant) -> bool {
        self.screen_asked > self.covered && self.missing_since.is_some_and(|t| now.duration_since(t) < self.timing.hold_max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Timing = Timing {
        gap: Duration::from_millis(500),
        quiet: Duration::from_millis(70),
        wait_max: Duration::from_millis(300),
        hold_max: Duration::from_secs(4),
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
    fn missing_screen_art_holds_the_scene_until_built_or_too_long() {
        let t0 = Instant::now();
        let mut s = ArtSchedule::new(Timing { gap: Duration::ZERO, ..T });
        // Map art (a unit) does not hold anything back.
        s.want(t0, false);
        assert!(!s.hold(t0));
        let token = s.start(ms(t0, 100)).unwrap();
        s.done(ms(t0, 150), token);
        // A screen's art does, until a rebuild covering it ends.
        s.want(ms(t0, 200), true);
        assert!(s.hold(ms(t0, 200)));
        let token = s.start(ms(t0, 300)).unwrap();
        s.want(ms(t0, 310), false);
        assert!(s.hold(ms(t0, 400)));
        s.done(ms(t0, 500), token);
        assert!(!s.hold(ms(t0, 500)));
        // Never built (too slow): shown anyway after hold_max.
        s.want(ms(t0, 1000), true);
        assert!(s.hold(ms(t0, 4999)));
        assert!(!s.hold(ms(t0, 5000)));
    }
}
