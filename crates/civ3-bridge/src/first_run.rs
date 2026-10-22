//! The first run with the renderer: once the title menu is up (not while
//! the game or its art is loading), Preferences open by themselves, on the
//! Resolution tab (`crate::prefs`, which also saves that they were shown:
//! `viewer_settings::FirstRun`). They are opened as the user would, by a
//! click on the title menu's Preferences row (`crate::setup_controls`).

use scene::{Button, Input};
use std::time::{Duration, Instant};

/// How long the title menu stays up, untouched, before Preferences open.
const SETTLE: Duration = Duration::from_millis(1500);
/// How long a click waits for Preferences to open before another.
const RETRY: Duration = Duration::from_secs(5);
/// Clicks per session at most (the marker is saved only once they open,
/// so a later launch tries again).
const ATTEMPTS: u32 = 3;

/// What the publisher knows this round.
#[derive(Copy, Clone, Debug, Default)]
pub struct Round {
    /// Preferences are still to be shown (`Prefs::first_run_pending`).
    pub pending: bool,
    /// The title menu is shown and nothing loads: the viewer has its
    /// window, the art is built, no popup is over it, Preferences closed.
    pub ready: bool,
    /// The Preferences row of the title menu, in the game's pixels.
    pub target: Option<[i32; 4]>,
}

#[derive(Debug, Default)]
pub struct FirstRun {
    since: Option<Instant>,
    clicked: Option<Instant>,
    attempts: u32,
}

impl FirstRun {
    /// The input to post to the game this round (the click on the row),
    /// after the user's `inputs`: a press of the user's starts the wait
    /// again (they are using the menu themselves).
    pub fn inputs(&mut self, round: Round, inputs: &[Input], now: Instant) -> Vec<Input> {
        let target = round.target.filter(|_| round.pending && round.ready);
        let Some([x, y, w, h]) = target else {
            self.since = None;
            return Vec::new();
        };
        if inputs.iter().any(|i| matches!(i, Input::MouseButton { down: true, .. } | Input::Key { down: true, .. })) {
            self.since = Some(now);
            return Vec::new();
        }
        let since = *self.since.get_or_insert(now);
        let waited = self.clicked.is_some_and(|at| now.duration_since(at) < RETRY);
        if now.duration_since(since) < SETTLE || waited || self.attempts >= ATTEMPTS {
            return Vec::new();
        }
        self.attempts += 1;
        self.clicked = Some(now);
        let (x, y) = (x + w / 2, y + h / 2);
        log::info!("first run: opening Preferences from the title menu (click {} at {x},{y})", self.attempts);
        let button = |down| Input::MouseButton { button: Button::Left, down, x, y };
        vec![Input::MouseMove { x, y }, button(true), button(false)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: [i32; 4] = [648, 708, 120, 38];

    fn round(pending: bool, ready: bool) -> Round {
        Round { pending, ready, target: Some(ROW) }
    }

    #[test]
    fn clicks_the_preferences_row_once_the_title_menu_has_settled() {
        let t0 = Instant::now();
        let mut f = FirstRun::default();
        assert!(f.inputs(round(true, true), &[], t0).is_empty());
        assert!(f.inputs(round(true, true), &[], t0 + SETTLE / 2).is_empty());
        let click = f.inputs(round(true, true), &[], t0 + SETTLE);
        assert_eq!(click, [
            Input::MouseMove { x: 708, y: 727 },
            Input::MouseButton { button: Button::Left, down: true, x: 708, y: 727 },
            Input::MouseButton { button: Button::Left, down: false, x: 708, y: 727 },
        ]);
        // Opened: no longer pending, nothing more.
        assert!(f.inputs(round(false, true), &[], t0 + SETTLE * 10).is_empty());
    }

    #[test]
    fn nothing_while_loading_or_without_the_row_or_once_shown() {
        let t0 = Instant::now();
        for r in [round(true, false), round(false, true), Round { target: None, ..round(true, true) }] {
            let mut f = FirstRun::default();
            for k in 0..10 {
                assert!(f.inputs(r, &[], t0 + SETTLE * k).is_empty(), "{r:?}");
            }
        }
        // Loading again resets the wait.
        let mut f = FirstRun::default();
        f.inputs(round(true, true), &[], t0);
        f.inputs(round(true, false), &[], t0 + SETTLE / 2);
        assert!(f.inputs(round(true, true), &[], t0 + SETTLE).is_empty());
        assert!(!f.inputs(round(true, true), &[], t0 + SETTLE * 2).is_empty());
    }

    #[test]
    fn the_users_own_presses_restart_the_wait_and_clicks_are_bounded() {
        let t0 = Instant::now();
        let mut f = FirstRun::default();
        f.inputs(round(true, true), &[], t0);
        let press = Input::MouseButton { button: Button::Left, down: true, x: 1, y: 1 };
        assert!(f.inputs(round(true, true), &[press], t0 + SETTLE).is_empty());
        assert!(f.inputs(round(true, true), &[], t0 + SETTLE * 3 / 2).is_empty());
        let mut at = t0 + SETTLE * 2;
        let mut clicks = 0;
        for _ in 0..20 {
            clicks += usize::from(!f.inputs(round(true, true), &[], at).is_empty());
            at += RETRY / 2;
        }
        assert_eq!(clicks, ATTEMPTS as usize, "retried while Preferences did not open, then given up");
    }
}
