//! The viewer's on-screen count: how many render errors happened during
//! the last second (repeats of types already logged included), and when
//! its notice shows.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// The count's window.
pub const WINDOW: Duration = Duration::from_secs(1);
/// How long the notice stays after the count fell to none (so that a single
/// error can be read): it shows the last count meanwhile.
pub const HOLD: Duration = Duration::from_secs(1);
/// The number shown changes at most this often (a count changing every
/// frame cannot be read).
pub const REFRESH: Duration = Duration::from_millis(250);

/// Errors per moment in the last [`WINDOW`], from running totals: this
/// side's ([`crate::total`]) and the driver's (sent with its host status,
/// `scene::HostStatus::render_errors`, wrapping).
#[derive(Debug, Default)]
pub struct Rolling {
    events: VecDeque<(Instant, u64)>,
    local: u64,
    remote: u32,
}

impl Rolling {
    /// The totals at `now` (`remote`: none from a driver that sends none).
    /// Errors before the first call count as at the first call.
    pub fn note(&mut self, now: Instant, local: u64, remote: Option<u32>) {
        let mut delta = local.saturating_sub(self.local);
        self.local = self.local.max(local);
        if let Some(r) = remote {
            delta += u64::from(r.wrapping_sub(self.remote));
            self.remote = r;
        }
        if delta > 0 {
            self.events.push_back((now, delta));
        }
        while self.events.front().is_some_and(|&(at, _)| now.saturating_duration_since(at) >= WINDOW) {
            self.events.pop_front();
        }
    }

    /// The errors in the window before `now`.
    pub fn count(&self, now: Instant) -> u64 {
        self.events.iter().filter(|&&(at, _)| now.saturating_duration_since(at) < WINDOW).map(|&(_, n)| n).sum()
    }
}

/// Whether the notice shows, and the number it shows.
#[derive(Debug, Default)]
pub struct Notice {
    shown: Option<Shown>,
}

#[derive(Debug)]
struct Shown {
    value: u64,
    refreshed: Instant,
    last_counted: Instant,
}

impl Notice {
    /// The number to show at `now` for a count of `count` (`None`: hidden).
    pub fn update(&mut self, now: Instant, count: u64) -> Option<u64> {
        match (&mut self.shown, count) {
            (None, 0) => {}
            (None, n) => self.shown = Some(Shown { value: n, refreshed: now, last_counted: now }),
            (Some(s), 0) => {
                if now.saturating_duration_since(s.last_counted) >= HOLD {
                    self.shown = None;
                }
            }
            (Some(s), n) => {
                s.last_counted = now;
                if now.saturating_duration_since(s.refreshed) >= REFRESH {
                    (s.value, s.refreshed) = (n, now);
                }
            }
        }
        self.value()
    }

    pub fn value(&self) -> Option<u64> {
        self.shown.as_ref().map(|s| s.value)
    }

    /// When to look again while it shows (it may change or go then).
    pub fn wake(&self) -> Option<Instant> {
        self.shown.as_ref().map(|s| (s.refreshed + REFRESH).min(s.last_counted + HOLD))
    }
}

/// The notice's text for `n` errors.
pub fn notice_text(n: u64) -> String {
    if n == 1 { "During the last second, 1 rendering error happened.".into() } else { format!("During the last second, {n} rendering errors happened.") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    #[test]
    fn the_count_rolls_over_the_last_second() {
        let t0 = Instant::now();
        let mut r = Rolling::default();
        r.note(t0, 0, None);
        assert_eq!(r.count(t0), 0);
        r.note(ms(t0, 100), 3, None);
        r.note(ms(t0, 600), 5, Some(4));
        assert_eq!(r.count(ms(t0, 600)), 9, "both sides, repeats included");
        assert_eq!(r.count(ms(t0, 1099)), 9);
        assert_eq!(r.count(ms(t0, 1100)), 6, "the first three are a second old");
        r.note(ms(t0, 1700), 5, Some(4));
        assert_eq!(r.count(ms(t0, 1700)), 0);
        // The driver's total wraps.
        let mut r = Rolling::default();
        r.note(t0, 0, Some(u32::MAX - 1));
        r.note(ms(t0, 10), 0, Some(2));
        assert_eq!(r.count(ms(t0, 10)), u64::from(u32::MAX - 1) + 4);
    }

    #[test]
    fn the_notice_refreshes_holds_then_goes() {
        let t0 = Instant::now();
        let mut n = Notice::default();
        assert_eq!(n.update(t0, 0), None);
        assert_eq!(n.wake(), None);
        assert_eq!(n.update(t0, 2), Some(2), "shows at once");
        assert_eq!(n.update(ms(t0, 100), 7), Some(2), "a new number waits for its refresh");
        assert_eq!(n.update(ms(t0, 250), 7), Some(7));
        assert_eq!(n.wake(), Some(ms(t0, 500)));
        assert_eq!(n.update(ms(t0, 300), 0), Some(7), "held");
        assert_eq!(n.update(ms(t0, 1249), 0), Some(7));
        assert_eq!(n.update(ms(t0, 1250), 0), None, "gone a hold after the last count");
        assert_eq!(n.update(ms(t0, 1300), 1), Some(1));
    }

    #[test]
    fn texts() {
        assert_eq!(notice_text(1), "During the last second, 1 rendering error happened.");
        assert_eq!(notice_text(12), "During the last second, 12 rendering errors happened.");
    }
}
