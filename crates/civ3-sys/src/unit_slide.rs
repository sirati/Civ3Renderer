//! The game's slide of a unit from one tile to the next (`animate movement`
//! on). PCGames `0x5BA8C0` is a `__thiscall` on the `Unit` with four stack
//! arguments, returning with `ret 0x10`:
//!
//! ```text
//! void Unit::slide(this, int to_x, int to_y, int steps, int ms)
//! ```
//!
//! `0x5BAB70` (reached from `Unit::move_to_adjacent_tile`) calls it with
//! `steps` 9 and `ms` 100 (50 in its other case, at `0x5BAE54`). The slide
//! divides `ms` by 10 while Shift is down and Caps Lock off, or Caps Lock on
//! and Shift up (`0x5BA913..0x5BA949`). It then moves the unit's picture
//! (`0x403A80`, Unit+0x27C) from its tile to `(to_x, to_y)` in `steps`
//! even steps. After each step it calls the animator's frame
//! (`Animator::draw` 0x4EED00, at `0x5BAAED`) only if the step is not yet
//! late: if the time since the start, in ms, is below `k * ms / steps`
//! for step `k`. It never waits. A fast frame finishes all steps long
//! before `ms`; a slow one is skipped until the steps catch up with the
//! clock. Afterwards it puts the picture back on the unit's tile; the
//! caller moves the unit there (`Unit::move` 0x5BCF30).

use crate::Build;

inproc::fastcall_hook! {
    /// `void Unit::slide(this, to_x, to_y, steps, ms)` (PCGames 0x5BA8C0).
    pub mod slide(unit, edx; to_x, to_y, steps, ms) -> ();
}

inproc::fastcall_hook! {
    /// The slide's call of the animator's frame, `Animator::draw(this)`
    /// (0x4EED00), at `0x5BAAED`.
    pub mod slide_frame(animator, edx;) -> ();
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sites {
    /// The slide's start and its first instructions.
    pub slide: (u32, &'static [u8]),
    /// Its `call Animator::draw`.
    pub frame: (u32, [u8; 5]),
}

pub fn sites(build: Build) -> Option<Sites> {
    match build {
        Build::PcGames => Some(Sites {
            // sub esp,34h / push ebx / push ebp
            slide: (0x5B_A8C0, &[0x83, 0xEC, 0x34, 0x53, 0x55]),
            // call 0x4EED00
            frame: (0x5B_AAED, [0xE8, 0x0E, 0x42, 0xF3, 0xFF]),
        }),
        Build::Gog | Build::Steam => None,
    }
}

/// `UnitState_Go_To`: the unit walks its path (`Unit+0x64`).
pub const GO_TO: u32 = 0x10;

/// A unit's go-to: its state (`Unit+0x64`), its destination
/// (`Unit+0xB0/0xB4`) and how many steps of its path are left
/// (`Unit+0x1C0`; the path itself, neighbour indices consumed back to
/// front, at `Unit+0xC0`). The step being taken is already taken off.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct GoTo {
    pub state: u32,
    pub dest: (i32, i32),
    pub steps: u32,
}

pub fn go_to(unit: u32) -> Result<GoTo, inproc::MemError> {
    let p = inproc::Ptr32::<u8>::new(unit);
    let state = p.offset::<u32>(0x64)?.read()?;
    let dest: [i32; 2] = p.offset::<[i32; 2]>(0xB0)?.read()?;
    let steps = p.offset::<i32>(0x1C0)?.read()?.clamp(0, 256) as u32;
    Ok(GoTo { state, dest: (dest[0], dest[1]), steps })
}

impl GoTo {
    /// How many more tiles a walk whose step goes to `to` has planned.
    pub fn planned(&self, to: (i32, i32)) -> u32 {
        if self.state == GO_TO && self.dest != to { self.steps } else { 0 }
    }
}

/// The slide's length in ms as the game takes it: `ms`, a tenth of it when
/// Shift (down) and Caps Lock (on) differ (`0x5BA913..0x5BA949`: the
/// signed division by 10 of `imul 0x66666667` / `sar 2`).
pub fn slide_ms(ms: i32, shift_down: bool, caps_on: bool) -> i32 {
    if shift_down != caps_on { ms / 10 } else { ms }
}

/// When the slide's step running at `elapsed_ms` is due, in ms after the
/// start: its threshold `k * ms / steps` (integer, as the game computes it)
/// for the first `k` whose threshold is later than `elapsed_ms`. Waiting
/// until then instead of drawing keeps the slide on the game's own
/// schedule: it then lasts `ms`, not as long as `steps` frames take.
pub fn step_due_ms(elapsed_ms: u64, ms: i32, steps: i32) -> u64 {
    let (ms, steps) = (ms.max(0) as u64, steps.max(1) as u64);
    if ms == 0 {
        return 0;
    }
    // The first k with k*ms/steps > elapsed (integer thresholds).
    let mut k = (elapsed_ms * steps / ms).max(1);
    while k * ms / steps <= elapsed_ms && k < steps {
        k += 1;
    }
    (k * ms / steps).min(ms).max(elapsed_ms.min(ms))
}

/// The world-pixel way of a slide from tile `from` to tile `to` (full
/// size: tiles 64 px apart across, 32 down; `0x5BA8E1..0x5BA8FD`). A slide
/// across the map's wrap is given a `to` beyond the map's edge, so the way
/// stays one tile long.
pub fn slide_way(from: (i32, i32), to: (i32, i32)) -> (i32, i32) {
    (64 * (to.0 - from.0), 32 * (to.1 - from.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_or_caps_lock_make_the_slide_ten_times_faster() {
        assert_eq!(slide_ms(100, false, false), 100);
        assert_eq!(slide_ms(100, true, false), 10);
        assert_eq!(slide_ms(100, false, true), 10);
        assert_eq!(slide_ms(100, true, true), 100);
        assert_eq!(slide_ms(55, true, false), 5);
    }

    #[test]
    fn steps_are_due_on_the_game_s_integer_thresholds() {
        // 9 steps over 100 ms: thresholds 11, 22, 33, 44, 55, 66, 77, 88, 100.
        assert_eq!(step_due_ms(0, 100, 9), 11);
        assert_eq!(step_due_ms(10, 100, 9), 11);
        assert_eq!(step_due_ms(11, 100, 9), 22);
        assert_eq!(step_due_ms(54, 100, 9), 55);
        assert_eq!(step_due_ms(89, 100, 9), 100);
        // Late: nothing to wait for.
        assert_eq!(step_due_ms(150, 100, 9), 100);
        // Degenerate slides never wait.
        assert_eq!(step_due_ms(0, 0, 9), 0);
        assert_eq!(step_due_ms(3, 10, 0), 10);
    }


    /// The game's own slide (`0x5BA9E1..0x5BAB11`) with frames that take
    /// `frame_ms` each: how many frames it draws and how long it lasts.
    fn original_slide(frame_ms: f64, ms: i32, steps: i32) -> (u32, f64) {
        let (mut now, mut frames) = (0.0f64, 0);
        for k in 1..steps as i64 {
            // Drawn only while the step is not late (`elapsed < k*ms/steps`).
            if (now as i64) < k * ms as i64 / steps as i64 {
                now += frame_ms;
                frames += 1;
            }
        }
        (frames, now)
    }

    /// Why a unit's move looked like two frames a second on the Mac and was
    /// gone in a blink on Linux: the slide never waits, so its length is its
    /// frames' and a slow frame drops most of its steps.
    #[test]
    fn the_original_slide_lasts_as_long_as_its_frames_and_drops_slow_ones() {
        // Native: ~1.5 ms frames, all eight drawn, over in 12 ms, not 100.
        assert_eq!(original_slide(1.5, 100, 9), (8, 12.0));
        // Emulated, ~70 ms frames: two positions a tile.
        assert_eq!(original_slide(70.0, 100, 9).0, 2);
        // ~500 ms frames: one, and the tile takes half a second.
        assert_eq!(original_slide(500.0, 100, 9), (1, 500.0));
        // Waiting until each step is due instead: always `ms`, whatever a
        // frame costs (`step_due_ms`).
        let mut now = 0u64;
        for _ in 1..9 { now = step_due_ms(now, 100, 9); }
        assert_eq!(now, 88);
        assert_eq!(step_due_ms(now, 100, 9), 100);
    }

    #[test]
    fn a_go_to_plans_its_path_s_steps_until_its_destination() {
        let walking = GoTo { state: GO_TO, dest: (20, 30), steps: 4 };
        assert_eq!(walking.planned((18, 30)), 4);
        assert_eq!(walking.planned((20, 30)), 0, "the last step");
        assert_eq!(GoTo { state: 1, ..walking }.planned((18, 30)), 0, "not on a go-to: a step on its own");
    }

    #[test]
    #[cfg(target_pointer_width = "32")]
    fn a_go_to_is_read_from_the_unit() {
        let mut unit = [0u8; 0x1C4];
        unit[0x64] = 0x10;
        unit[0xB0..0xB4].copy_from_slice(&7i32.to_le_bytes());
        unit[0xB4..0xB8].copy_from_slice(&9i32.to_le_bytes());
        unit[0x1C0..0x1C4].copy_from_slice(&3i32.to_le_bytes());
        assert_eq!(go_to(unit.as_ptr() as usize as u32).unwrap(), GoTo { state: GO_TO, dest: (7, 9), steps: 3 });
    }

    #[test]
    fn a_slide_goes_one_tile() {
        assert_eq!(slide_way((10, 20), (11, 21)), (64, 32));
        assert_eq!(slide_way((10, 20), (12, 20)), (128, 0));
        assert_eq!(slide_way((0, 4), (-1, 5)), (-64, 32), "across the wrap, beyond the edge");
    }

    #[test]
    fn the_frame_site_calls_the_animator_s_draw() {
        let s = sites(Build::PcGames).unwrap();
        let b = s.frame.1;
        assert_eq!(b[0], 0xE8);
        let target = s.frame.0.wrapping_add(5).wrapping_add(i32::from_le_bytes([b[1], b[2], b[3], b[4]]) as u32);
        assert_eq!(target, 0x4E_ED00);
    }
}
