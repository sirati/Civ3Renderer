//! Whether the game's thread answers: a heartbeat it stamps whenever it
//! looks at its messages, for the viewer's busy indicator
//! (`scene::HostStatus`, `tile_viewer::host_status`).
//!
//! The game takes every message through `jgl.dll`: its pump (PCGames
//! 0x624B90, jgl slot +0x124, docs/modal-input.md) calls `PeekMessageA`
//! and, with nothing queued, sleeps in `WaitMessage` until input comes. Both
//! of jgl's imports are redirected: each `PeekMessageA` is a beat, and the
//! time inside `WaitMessage` counts as answering (an idle game with no input
//! sleeps there for as long as the player does nothing), with a beat on the
//! way in and out. Every pump of the game goes through them (the modal
//! loop's at 0x6053B4, the human-turn loop's, the animations' and the
//! filtered ones), so a game that does not reach one for a while is busy:
//! its AI turns between pumps, an autosave, loading a scenario.
//!
//! Lock-free: two atomics, written on the game's thread, read by the
//! publisher.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::SeqCst};
use std::time::Instant;

/// Milliseconds since [`epoch`] of the last beat.
static LAST: AtomicU64 = AtomicU64::new(0);
/// Calls inside the game's message wait (nested waits counted).
static WAITING: AtomicU32 = AtomicU32::new(0);
/// Whether a source of beats is installed (else nothing is known).
static BEATING: AtomicBool = AtomicBool::new(false);

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

fn now_ms() -> u64 {
    epoch().elapsed().as_millis() as u64
}

/// The game's thread answered just now.
pub fn beat() {
    LAST.store(now_ms(), SeqCst);
}

/// The game's thread is in its message wait until the guard is dropped
/// (with a beat on the way in and out).
pub struct Waiting(());

impl Waiting {
    pub fn enter() -> Waiting {
        beat();
        WAITING.fetch_add(1, SeqCst);
        Waiting(())
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        WAITING.fetch_sub(1, SeqCst);
        beat();
    }
}

/// How long the game's thread has not answered, in milliseconds, from the
/// last beat at `last` (ms), now `now` (ms), and whether it waits for
/// messages (then it answers: 0).
pub fn stale_ms(now: u64, last: u64, waiting: bool) -> u32 {
    if waiting { 0 } else { now.saturating_sub(last).min(u32::MAX as u64) as u32 }
}

/// How long the game's thread has not answered now (ms); 0 while nothing
/// stamps beats (unknown: only the connection's silence counts then).
pub fn stale_now() -> u32 {
    if !BEATING.load(SeqCst) {
        return 0;
    }
    stale_ms(now_ms(), LAST.load(SeqCst), WAITING.load(SeqCst) > 0)
}

/// What the publisher sends the viewer (with every scene, and on its own a
/// few times a second): the game thread's staleness as of now, numbered.
pub fn status() -> scene::HostStatus {
    static SERIAL: AtomicU32 = AtomicU32::new(0);
    scene::HostStatus { stale_ms: stale_now(), serial: SERIAL.fetch_add(1, SeqCst).wrapping_add(1), render_errors: 0 }
}

#[cfg(windows)]
mod hooks {
    inproc::stdcall_hook! {
        /// `BOOL PeekMessageA(LPMSG, HWND, UINT, UINT, UINT)`.
        pub mod peek_message_a(msg, hwnd, min, max, remove) -> i32;
    }

    inproc::stdcall_hook! {
        /// `BOOL WaitMessage(void)`.
        pub mod wait_message() -> i32;
    }
}

/// Redirects `jgl.dll`'s `PeekMessageA` and `WaitMessage` (for the life of
/// the process) to stamp the heartbeat (see the module docs).
#[cfg(windows)]
pub fn install() -> Result<(), String> {
    if jgl_ffi::own::on() {
        // Our jgl's pumps tell `WindowHooks` directly.
        BEATING.store(true, SeqCst);
        beat();
        return Ok(());
    }
    let base = inproc::pe::module_base("jgl.dll").ok_or("jgl.dll is not loaded")?;
    let slot = |f: &str| inproc::pe::import_slot(base, "USER32.dll", f).map_err(|e| format!("jgl.dll's {f}: {e}"))?.ok_or(format!("jgl.dll does not import {f}"));
    let (peek, wait) = (slot("PeekMessageA")?, slot("WaitMessage")?);
    beat();
    let peek = hooks::peek_message_a::install_slot(peek, |c| {
        // jgl pumps on the game's thread only: whose hook time to count.
        inproc::hook_time::watch_current_thread();
        beat();
        // (Which pointer move the game has handled: `crate::cursor::handled`.)
        crate::cursor::handled::peeking();
        let (msg, remove) = (c.msg, c.remove);
        let got = c.original();
        if got != 0 && remove & 1 != 0 {
            if let Ok([_, message, _, lparam]) = inproc::mem::Ptr32::<[u32; 4]>::new(msg).read() {
                crate::cursor::handled::peeked(true, remove, message, lparam);
            }
        }
        got
    }).map_err(|e| format!("PeekMessageA: {e}"))?;
    let wait = hooks::wait_message::install_slot(wait, |c| {
        let _waiting = Waiting::enter();
        c.original()
    }).map_err(|e| format!("WaitMessage: {e}"))?;
    std::mem::forget((peek, wait));
    BEATING.store(true, SeqCst);
    Ok(())
}

#[cfg(windows)]
/// The same beats where our jgl is the game's (jgl.dll not loaded): its
/// pumps tell these hooks instead of jgl's imports being redirected; and
/// the screen size the game is told (`crate::game_window`).
pub struct WindowHooks {
    /// The game's screen of the display's mode, with a desktop's scale.
    pub screen: Option<inproc::screen::ScreenSize>,
}

#[cfg(windows)]
impl jgl::WindowHooks for WindowHooks {
    fn screen(&self, display: (i32, i32)) -> (i32, i32) {
        match &self.screen {
            Some(size) => inproc::screen::reported(size),
            None => display,
        }
    }

    fn peeking(&self) {
        BEATING.store(true, SeqCst);
        inproc::hook_time::watch_current_thread();
        beat();
        crate::cursor::handled::peeking();
    }

    fn peeked(&self, remove: u32, message: u32, lparam: u32) {
        crate::cursor::handled::peeked(true, remove, message, lparam);
    }

    fn waiting(&self, entering: bool) {
        BEATING.store(true, SeqCst);
        beat();
        if entering {
            WAITING.fetch_add(1, SeqCst);
        } else {
            WAITING.fetch_sub(1, SeqCst);
        }
    }
}

/// Staleness (ms) from which the game's thread counts as stalled for
/// [`StallWatch`], and below which it answers again.
const STALL_FROM_MS: u32 = 1_000;
const ANSWERS_BELOW_MS: u32 = 500;
/// How often a stall still going on is reported.
const STALL_REPORT_EVERY_MS: u64 = 10_000;

/// Notes how much of a stall of the game's thread (no message pump for
/// [`STALL_FROM_MS`] or more) it spent inside our hooks
/// (`inproc::hook_time`): the rest is the game's own code (and what the
/// emulator does for it). Polled by the publisher; its lines go to the log.
#[derive(Default)]
pub struct StallWatch {
    /// The stall in progress: the hook totals when it was noticed, at what
    /// staleness, and when it was last reported (by `now_ms`).
    current: Option<(inproc::hook_time::Snapshot, u32, u64)>,
}

impl StallWatch {
    /// With the game thread's staleness `stale` (ms) at `now` (ms, any
    /// monotonic clock) and the hook totals `hooks`: a line to log when a
    /// stall ends, or every [`STALL_REPORT_EVERY_MS`] while it lasts.
    pub fn poll(&mut self, stale: u32, now: u64, hooks: impl FnOnce() -> inproc::hook_time::Snapshot) -> Option<String> {
        match &mut self.current {
            None if stale >= STALL_FROM_MS => {
                self.current = Some((hooks(), stale, now));
                None
            }
            None => None,
            Some((start, noticed_at, _)) if stale < ANSWERS_BELOW_MS => {
                // (The stall lasted until the last beat: `stale` ago.)
                let line = describe("answers again after", start, *noticed_at, &hooks());
                self.current = None;
                Some(line)
            }
            Some((start, noticed_at, reported)) if now.saturating_sub(*reported) >= STALL_REPORT_EVERY_MS => {
                *reported = now;
                Some(describe(&format!("{stale} ms without its message pump, still going on;"), start, *noticed_at, &hooks()))
            }
            Some(_) => None,
        }
    }
}

/// How long a window of [`BusyWatch`] is, and how much of it in our hooks
/// gets it into the log.
const BUSY_WINDOW_MS: u64 = 5_000;
const BUSY_FROM_MS: u64 = 500;

/// Notes the windows of [`BUSY_WINDOW_MS`] in which the bridge's hooks took
/// [`BUSY_FROM_MS`] or more of the game's thread, stalled or not (frames of
/// a unit's move that come slowly): the rest of the window is the game's
/// own code, or waiting. Polled by the publisher; its lines go to the log.
#[derive(Default)]
pub struct BusyWatch {
    /// The window's start (by `now_ms`) and the hook totals then.
    window: Option<(u64, inproc::hook_time::Snapshot)>,
}

impl BusyWatch {
    pub fn poll(&mut self, now: u64, hooks: impl FnOnce() -> inproc::hook_time::Snapshot) -> Option<String> {
        match &self.window {
            Some((began, _)) if now.saturating_sub(*began) < BUSY_WINDOW_MS => None,
            _ => {
                let snap = hooks();
                let line = self.window.take().and_then(|(began, start)| {
                    let (ms, entries, top) = snap.since(&start, 6);
                    (ms >= BUSY_FROM_MS).then(|| {
                        let top: Vec<String> = top.iter().map(|(name, ms, calls)| format!("{name} {ms} ms in {calls} calls")).collect();
                        format!("game thread: {ms} of the last {} ms inside the bridge's hooks ({entries} entries from the game): {}", now - began, top.join(", "))
                    })
                });
                self.window = Some((now, snap));
                line
            }
        }
    }
}

fn describe(what: &str, start: &inproc::hook_time::Snapshot, noticed_at: u32, now: &inproc::hook_time::Snapshot) -> String {
    let (ms, entries, top) = now.since(start, 6);
    let top: Vec<String> = top.iter().map(|(name, ms, calls)| format!("{name} {ms} ms in {calls} calls")).collect();
    format!("game thread: {what} a stall; since it was noticed ({noticed_at} ms in) {ms} ms inside the bridge's hooks ({entries} entries from the game): {}",
        if top.is_empty() { "none".to_string() } else { top.join(", ") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_windows_are_reported_with_their_hooks() {
        use inproc::hook_time::Snapshot;
        let at = |ms: u64, calls: u64| Snapshot { nanos: ms * 1_000_000, calls, hooks: vec![("pcx_line", ms * 1_000_000, calls)] };
        let mut w = BusyWatch::default();
        assert_eq!(w.poll(0, || at(0, 0)), None, "the first window starts");
        assert_eq!(w.poll(4_000, || unreachable!()), None);
        assert_eq!(w.poll(5_000, || at(100, 10)), None, "a quiet window");
        let busy = w.poll(10_200, || at(2_100, 50)).unwrap();
        assert_eq!(busy, "game thread: 2000 of the last 5200 ms inside the bridge's hooks (40 entries from the game): pcx_line 2000 ms in 40 calls");
    }

    #[test]
    fn a_stall_is_reported_with_the_hooks_it_spent_its_time_in() {
        use inproc::hook_time::Snapshot;
        let at = |ns: u64, calls: u64| Snapshot { nanos: ns, calls, hooks: vec![("text_out", ns, calls)] };
        let mut w = StallWatch::default();
        assert_eq!(w.poll(400, 0, || at(0, 0)), None);
        assert_eq!(w.poll(1_200, 100, || at(5_000_000, 10)), None, "noticed: nothing to say yet");
        assert_eq!(w.poll(3_000, 5_000, || unreachable!()), None);
        let still = w.poll(11_000, 10_100, || at(9_000_000, 30)).unwrap();
        assert!(still.contains("11000 ms without its message pump, still going on") && still.contains("4 ms inside") && still.contains("text_out 4 ms in 20 calls"), "{still}");
        assert_eq!(w.poll(12_000, 12_000, || unreachable!()), None, "reported every 10 s");
        let end = w.poll(3, 13_000, || at(25_000_000, 40)).unwrap();
        assert!(end.starts_with("game thread: answers again after a stall; since it was noticed (1200 ms in) 20 ms inside"), "{end}");
        assert_eq!(w.poll(3, 14_000, || unreachable!()), None);
    }

    #[test]
    fn staleness_is_the_time_since_the_last_beat_unless_waiting_for_messages() {
        assert_eq!(stale_ms(1_000, 1_000, false), 0);
        assert_eq!(stale_ms(1_600, 1_000, false), 600);
        assert_eq!(stale_ms(9_000, 1_000, false), 8_000);
        // Asleep in the game's message wait (an idle game): answering.
        assert_eq!(stale_ms(9_000, 1_000, true), 0);
        // (A beat stamped after the publisher read the clock.)
        assert_eq!(stale_ms(1_000, 1_005, false), 0);
    }

    #[test]
    fn the_wait_guard_counts_as_answering_and_beats_on_leaving() {
        // (One test touches the statics: no other test runs them.)
        BEATING.store(true, SeqCst);
        LAST.store(0, SeqCst);
        {
            let _w = Waiting::enter();
            assert_eq!(stale_now(), 0);
            LAST.store(0, SeqCst);
            assert_eq!(stale_now(), 0, "inside the wait, an old beat does not count");
        }
        assert!(stale_now() < 100, "leaving the wait is a beat");
        BEATING.store(false, SeqCst);
        let (a, b) = (status(), status());
        assert_eq!((a.stale_ms, b.stale_ms), (0, 0), "nothing beating: unknown, only the connection counts");
        assert_eq!(b.serial, a.serial.wrapping_add(1));
    }
}
