//! How fast the game's animator draws its frames, and where a frame's time
//! goes: for telling a slow unit-movement animation that our hooks make
//! slow from one the game (or its emulator) makes slow, or one that waits.
//!
//! The animator's draw (0x4EED00) calls the main map's animation tick
//! (`advance_animations`, hooked in [`crate::unit_views`]) once a frame,
//! also inside the blocking loop that steps a unit across a tile
//! (0x5BAB70). Each such tick calls [`Steps::frame`] with a [`Sample`]: a
//! clock read, the thread's CPU time, its memory access counts, and a
//! snapshot of the hook totals while a window is worth reporting.
//!
//! Frames go into windows of one kind (a unit moving or not); a window
//! ends after 5 s, when the kind changes, or when the frames stop for a
//! second. A window of movement, or one with slow frames, goes to the log:
//! the frames, their mean and longest interval, how much of a frame the
//! thread worked (CPU time), spent in our hooks and read memory, and the
//! hooks that took most.

use inproc::hook_time::Snapshot;

/// A frame interval from which the animator counts as stopped (a gap, not
/// a slow frame).
const GAP_NS: u64 = 1_000_000_000;
/// How long a window of frames runs before it is reported.
const WINDOW_NS: u64 = 5_000_000_000;
/// A frame interval from which a frame counts as slow: a window with one
/// is reported even without movement.
const SLOW_NS: u64 = 100_000_000;

/// The game thread at one frame.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Sample {
    /// When (ns, monotonic).
    pub now: u64,
    /// A unit moving across a tile in this frame.
    pub moving: bool,
    /// The thread's CPU time so far (ns), if known.
    pub cpu: Option<u64>,
    /// The thread's memory reads and writes so far
    /// (`inproc::mem::thread_accesses`).
    pub accesses: (u64, u64),
}

/// One window of frames.
struct Window {
    first: Sample,
    last: Sample,
    /// The hook totals at its first frame.
    hooks_first: Snapshot,
    /// The hook totals at its last frame, kept once the window is worth
    /// reporting.
    hooks_last: Option<Snapshot>,
    /// Frame intervals in it.
    frames: u32,
    slow: u32,
    max: u64,
}

impl Window {
    fn worth_reporting(&self) -> bool {
        self.first.moving || self.slow > 0
    }

    fn report(&self) -> Option<String> {
        let end = self.hooks_last.as_ref()?;
        if self.frames == 0 || !self.worth_reporting() {
            return None;
        }
        let n = u64::from(self.frames);
        let ms = |ns: u64| ns as f64 / 1e6;
        let span = self.last.now.saturating_sub(self.first.now);
        let hooks = end.nanos.saturating_sub(self.hooks_first.nanos);
        let mut top: Vec<(&str, u64, u64)> = end.hooks.iter().enumerate().map(|(i, &(name, ns, calls))| {
            let (ns0, calls0) = self.hooks_first.hooks.get(i).map_or((0, 0), |h| (h.1, h.2));
            (name, ns.saturating_sub(ns0), calls.saturating_sub(calls0))
        }).filter(|h| h.2 > 0).collect();
        top.sort_by_key(|h| std::cmp::Reverse(h.1));
        top.truncate(8);
        let top: Vec<String> = top.iter()
            .map(|(name, ns, calls)| format!("{name} {:.2} ms in {:.1} calls", ms(ns / n), *calls as f64 / n as f64))
            .collect();
        let cpu = match (self.first.cpu, self.last.cpu) {
            (Some(a), Some(b)) => format!("{:.1} ms of it working", ms(b.saturating_sub(a) / n)),
            _ => "CPU time unknown".to_string(),
        };
        Some(format!(
            "animation: {} frames {} over {:.0} ms ({} slow): {:.1} ms a frame (longest {:.1}), {cpu}, {:.1} ms in the bridge's hooks, {} memory reads and {} writes; a frame's hooks: {}",
            self.frames, if self.first.moving { "with a unit moving" } else { "without movement" },
            ms(span), self.slow, ms(span / n), ms(self.max), ms(hooks / n),
            self.last.accesses.0.saturating_sub(self.first.accesses.0) / n,
            self.last.accesses.1.saturating_sub(self.first.accesses.1) / n,
            if top.is_empty() { "none".to_string() } else { top.join(", ") }))
    }
}

/// The animator's frames on the game thread.
#[derive(Default)]
pub struct Steps {
    window: Option<Window>,
}

impl Steps {
    /// A frame; `hooks` reads the hook totals. A line to log when a window
    /// of frames is done.
    pub fn frame(&mut self, at: Sample, hooks: impl Fn() -> Snapshot) -> Option<String> {
        let mut line = None;
        if let Some(mut w) = self.window.take() {
            let interval = at.now.saturating_sub(w.last.now);
            if interval >= GAP_NS || at.moving != w.first.moving {
                // The window ends at its last frame.
                line = w.report();
            } else {
                w.frames += 1;
                w.slow += u32::from(interval >= SLOW_NS);
                w.max = w.max.max(interval);
                w.last = at;
                if w.worth_reporting() {
                    w.hooks_last = Some(hooks());
                }
                if at.now.saturating_sub(w.first.now) >= WINDOW_NS {
                    line = w.report();
                    // (The next window begins at this frame.)
                    self.window = Some(Window { first: at, last: at, hooks_first: w.hooks_last.unwrap_or_else(&hooks), hooks_last: None, frames: 0, slow: 0, max: 0 });
                } else {
                    self.window = Some(w);
                }
            }
        }
        if self.window.is_none() {
            self.window = Some(Window { first: at, last: at, hooks_first: hooks(), hooks_last: None, frames: 0, slow: 0, max: 0 });
        }
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1_000_000;

    fn hooks(hook_ms: u64, calls: u64) -> Snapshot {
        Snapshot { nanos: hook_ms * MS, calls, hooks: vec![("pcx_line", hook_ms * MS, calls), ("present", 0, 0)] }
    }

    fn at(ms: u64, moving: bool) -> Sample {
        Sample { now: ms * MS, moving, cpu: Some(ms * MS / 10), accesses: (ms * 10, ms / 100) }
    }

    #[test]
    fn fast_frames_without_movement_are_not_reported_and_take_few_snapshots() {
        let mut s = Steps::default();
        let snaps = std::cell::Cell::new(0);
        let read = || { snaps.set(snaps.get() + 1); hooks(0, 0) };
        for f in 0..400u64 {
            assert_eq!(s.frame(at(f * 16, false), read), None);
        }
        // One per window begun (the first, and one every 5 s).
        assert!(snaps.get() <= 3, "{}", snaps.get());
    }

    #[test]
    fn a_window_of_movement_reports_a_frame_s_interval_work_hooks_and_reads() {
        let mut s = Steps::default();
        let mut out = Vec::new();
        assert_eq!(s.frame(at(0, false), || hooks(0, 0)), None);
        for f in 1..=4u64 {
            // 500 ms a frame, 400 of it in pcx_line (130 calls).
            out.extend(s.frame(at(f * 500, true), || hooks((f - 1) * 400, (f - 1) * 130)));
        }
        // The unit arrives: the moving window (3 intervals) ends.
        out.extend(s.frame(at(2100, false), || hooks(1300, 400)));
        assert_eq!(out.len(), 1, "{out:?}");
        let line = &out[0];
        assert!(line.starts_with("animation: 3 frames with a unit moving over 1500 ms (3 slow): 500.0 ms a frame (longest 500.0), 50.0 ms of it working, 400.0 ms in the bridge's hooks, 5000 memory reads and 5 writes"), "{line}");
        assert!(line.contains("pcx_line 400.00 ms in 130.0 calls"), "{line}");
        assert!(!line.contains("present"), "hooks not called are left out: {line}");
    }

    #[test]
    fn a_long_movement_is_reported_every_five_seconds_and_a_gap_ends_it() {
        let mut s = Steps::default();
        let mut out = Vec::new();
        for f in 0..=12u64 {
            out.extend(s.frame(at(f * 500, true), || hooks(f * 100, f)));
        }
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(out[0].starts_with("animation: 10 frames with a unit moving over 5000 ms"), "{}", out[0]);
        // 3 s later, after other work of the game's: the rest (2 frames).
        let rest = s.frame(at(9000, true), || hooks(5000, 99)).unwrap();
        assert!(rest.starts_with("animation: 2 frames with a unit moving over 1000 ms"), "{rest}");
        assert!(rest.contains("100.0 ms in the bridge's hooks"), "the gap's own hook time is not counted: {rest}");
    }
}
