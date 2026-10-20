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
    scene::HostStatus { stale_ms: stale_now(), serial: SERIAL.fetch_add(1, SeqCst).wrapping_add(1) }
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
    let base = inproc::pe::module_base("jgl.dll").ok_or("jgl.dll is not loaded")?;
    let slot = |f: &str| inproc::pe::import_slot(base, "USER32.dll", f).map_err(|e| format!("jgl.dll's {f}: {e}"))?.ok_or(format!("jgl.dll does not import {f}"));
    let (peek, wait) = (slot("PeekMessageA")?, slot("WaitMessage")?);
    beat();
    let peek = hooks::peek_message_a::install_slot(peek, |c| {
        beat();
        c.original()
    }).map_err(|e| format!("PeekMessageA: {e}"))?;
    let wait = hooks::wait_message::install_slot(wait, |c| {
        let _waiting = Waiting::enter();
        c.original()
    }).map_err(|e| format!("WaitMessage: {e}"))?;
    std::mem::forget((peek, wait));
    BEATING.store(true, SeqCst);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
