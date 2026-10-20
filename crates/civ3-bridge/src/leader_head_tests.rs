//! Leader heads in motion. The game's animator is the model here
//! (`docs/civ3-map-rendering.md` §16.13, decoded from the executable). It
//! drives the mirror with the shipped heads. The frame shown must be the
//! one the game picked, at the game's own pace, whatever the renderer's
//! frame rate. Needs an installation (`CIV3_INSTALL`); skipped otherwise.

use crate::ui::{Event, ScreenDraw, UiModel};
use civ3_assets::flc;

/// MSVC's `rand` (0x64A152).
struct Rand(u32);

impl Rand {
    fn next(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
        ((self.0 >> 16) & 0x7FFF) as i32
    }
}

/// 0x4C8C20: a target never rests inside a blink.
fn snap(v: i32) -> i32 {
    for (lo, hi, split) in [(4, 16, 10), (14, 26, 20), (74, 86, 80), (94, 106, 100)] {
        if v > lo && v < hi {
            return if v >= split { hi } else { lo };
        }
    }
    v
}

/// The blink frames of the shipped heads: where `snap` keeps targets away.
fn blinking(frame: i32) -> bool {
    [10, 20, 80, 100].iter().any(|b| (frame - b).abs() <= 1)
}

/// The leader-head object (0x478 bytes).
struct Head {
    total: i32,
    cur: i32,
    tgt: i32,
    lo: i32,
    hi: i32,
    acc: u32,
    countdown: i32,
    pending: i32,
    nudged: bool,
    forward: bool,
}

impl Head {
    /// 0x4C8240, then 0x4C85B0 (`acc = 0`).
    fn new(total: i32) -> Head {
        let mut h = Head { total, cur: total / 2, tgt: 0, lo: 0, hi: 0, acc: 0, countdown: -1, pending: -1, nudged: false, forward: true };
        h.aim(snap(total / 2));
        h
    }

    fn aim(&mut self, tgt: i32) {
        self.tgt = tgt;
        self.lo = (tgt - 5).max(0);
        self.hi = (tgt + 5).min(self.total - 1);
        self.lo = self.lo.min(self.hi);
    }

    /// 0x4C88A0: attitude `p` (0..100).
    fn pose(&mut self, p: i32) {
        self.aim(snap((self.total - 1) * (100 - p).clamp(4, 96) / 100));
        self.pending = -1;
    }

    /// 0x4C85C0, with the milliseconds since the last call.
    fn step(&mut self, elapsed: u32, rand: &mut Rand) {
        self.acc += elapsed;
        let threshold = if (self.tgt - self.cur).abs() <= 10 { 100 } else { 50 };
        if self.acc <= threshold {
            return;
        }
        if self.cur == self.tgt {
            if self.nudged {
                self.nudged = false;
                self.countdown = 4;
            } else if self.countdown > 0 {
                self.countdown -= 1;
            } else if self.countdown == 0 {
                if self.pending >= 0 {
                    let p = self.pending;
                    self.pose(p);
                }
                self.countdown = -1;
            }
            let span = self.hi - self.lo;
            let pick = |rand: &mut Rand| match (span + 1) as u16 as i32 {
                0 => 0,
                n => rand.next() % n,
            };
            self.tgt = if self.tgt == self.hi {
                self.lo + pick(rand) - span / 2
            } else if self.tgt == self.lo {
                self.lo + span / 2 + pick(rand)
            } else if rand.next() % 2 == 0 {
                self.hi
            } else {
                self.lo
            };
        }
        if self.cur < self.tgt {
            self.forward = true;
            self.cur += 1;
        } else if self.cur > self.tgt {
            self.forward = false;
            self.cur -= 1;
        }
        self.acc = 0;
    }

    /// What 0x4C8420 draws: the forward file at `cur`, or the reverse
    /// file at `total − 1 − cur`.
    fn drawn(&self) -> (bool, u32) {
        let last = self.total - 1;
        let frame = if self.forward { self.cur } else { last - self.cur };
        (self.forward, frame.clamp(0, last) as u32)
    }
}

const FORWARD: u32 = 5;
const REVERSE: u32 = 6;
const HEAD: u32 = 40;
const DIALOG: u32 = 300;
const BACKGROUND: u32 = 10;

fn install() -> Option<std::path::PathBuf> {
    let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    let path = std::path::PathBuf::from(install);
    path.join("Art/Flics/Al_01.FLC").is_file().then_some(path)
}

/// The diplomacy dialog with its background, before the head is drawn.
fn dialog() -> UiModel {
    let mut m = UiModel::new(vec![], vec![]);
    m.apply(Event::FlcLoaded { flic: FORWARD, path: "Art/Flics/Al_01.FLC".into() });
    m.apply(Event::FlcLoaded { flic: REVERSE, path: "Art/Flics/Al_02.FLC".into() });
    m.apply(Event::Loaded { image: BACKGROUND, path: "Art/Diplomacy/talk_offer.pcx".into(), size: (438, 517) });
    m.apply(Event::Copy { from: BACKGROUND, src: [0, 0, 438, 517], canvas: DIALOG, x: 297, y: 50, keyed: true });
    m
}

/// One pass of the dialog's idle callback (0x51B310): the head's frame
/// onto its image, the image onto the dialog, the dialog presented.
fn idle(m: &mut UiModel, head: &Head) {
    let (forward, chunk) = head.drawn();
    let flic = if forward { FORWARD } else { REVERSE };
    m.apply(Event::FlcBlit { flic, chunk, canvas: HEAD, x: 0, y: 0, clip: None, plain: true });
    m.apply(Event::Copy { from: HEAD, src: [0, 0, 200, 240], canvas: DIALOG, x: 411, y: 59, keyed: true });
    m.apply(Event::PaintBegin);
    m.apply(Event::Composite { canvas: DIALOG, vtable: 1, origin: (0, 0), size: (1920, 1080), clip: [0, 0, 1920, 1080], keyed: false });
    m.apply(Event::PaintEnd);
}

/// The forward frame the renderer shows (exactly one head is drawn).
fn shown(m: &UiModel, total: i32) -> i32 {
    let heads: Vec<_> = m.frame().draws.into_iter().filter_map(|d| match d {
        ScreenDraw::Flc { path, chunk, x: 411, y: 59, plain: true, .. } => Some((path, chunk as i32)),
        _ => None,
    }).collect();
    assert_eq!(heads.len(), 1, "one head frame on the screen, not {heads:?}");
    let (path, chunk) = &heads[0];
    if path.ends_with("_01.FLC") { *chunk } else { total - 1 - chunk }
}

/// What a renderer at `fps` shows over `seconds` of a game passing its
/// idle loop every `idle_ms`: (time ms, forward frame) whenever it changes.
fn watch(total: i32, pose: i32, idle_ms: u32, fps: u32, seconds: u32) -> Vec<(f64, i32)> {
    let mut m = dialog();
    let mut head = Head::new(total);
    head.pose(pose);
    let mut rand = Rand(1);
    let (mut game_ms, mut seen) = (0u32, Vec::new());
    idle(&mut m, &head);
    let frames = fps * seconds;
    for k in 0..=frames {
        let t = k as f64 * 1000.0 / fps as f64;
        while ((game_ms + idle_ms) as f64) <= t {
            game_ms += idle_ms;
            head.step(idle_ms, &mut rand);
            idle(&mut m, &head);
        }
        let frame = shown(&m, total);
        assert_eq!(frame, head.cur, "at {t} ms the renderer shows the game's frame");
        if seen.last().is_none_or(|&(_, f)| f != frame) {
            seen.push((t, frame));
        }
    }
    seen
}

#[test]
fn the_reverse_file_is_the_forward_file_backwards() {
    let Some(install) = install() else { return eprintln!("no Civ III install, skipped") };
    let mut n = 0;
    for civ in ["Al", "Ab", "Xe", "Zu", "Ce", "Ca"] {
        let file = |k: u32| {
            let dir = install.join("Art/Flics");
            let name = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned())
                .find(|f| f.eq_ignore_ascii_case(&format!("{civ}_0{k}.flc"))).unwrap();
            flc::decode(&std::fs::read(dir.join(name)).unwrap()).unwrap()
        };
        let (forward, reverse) = (file(1), file(2));
        let total = forward.header_frames;
        assert_eq!(total, 121, "{civ}");
        // (One more frame in the file: the ring frame back to the first.)
        assert_eq!(forward.frames.len(), total + 1, "{civ}");
        for j in 0..total {
            assert!(reverse.frames[j].pixels == forward.frames[total - 1 - j].pixels, "{civ}: reverse {j}");
        }
        // The file's own speed is not the game's pace (100 ms a frame).
        assert!(forward.speed_ms < 100, "{civ}: {} ms", forward.speed_ms);
        n += 1;
    }
    assert_eq!(n, 6);
}

#[test]
fn the_head_moves_at_the_games_pace_at_any_frame_rate() {
    let Some(install) = install() else { return eprintln!("no Civ III install, skipped") };
    let forward = flc::decode(&std::fs::read(install.join("Art/Flics/Al_01.FLC")).unwrap()).unwrap();
    let total = forward.header_frames as i32;
    // Polite (about frame 26: sweeps reach the blink at 20) and neutral.
    for (pose, idle_ms) in [(78, 50), (78, 1), (50, 50)] {
        let runs: Vec<(u32, Vec<(f64, i32)>)> = [60, 144, 240, 1000].into_iter().map(|fps| (fps, watch(total, pose, idle_ms, fps, 120))).collect();
        let (_, first) = &runs[0];
        let frames = |seen: &[(f64, i32)]| seen.iter().map(|&(_, f)| f).collect::<Vec<_>>();
        for (fps, seen) in &runs {
            // Every frame the game showed, in order, at every rate.
            assert_eq!(frames(seen), frames(first), "{fps} fps, pose {pose}, idle {idle_ms} ms");
            // At the moment the game showed it (within one renderer frame).
            for (a, b) in seen.iter().zip(first) {
                assert!((a.0 - b.0).abs() <= 1000.0 / 60.0 + 1e-6, "{fps} fps: {a:?} vs {b:?}");
            }
            // One frame at a time, never faster than the game's 100 ms
            // near its target (50 ms when far), not the file's 71 ms.
            for w in seen.windows(2) {
                assert_eq!((w[1].1 - w[0].1).abs(), 1, "{fps} fps: {w:?}");
                assert!(w[1].0 - w[0].0 >= 50.0 - 1000.0 / *fps as f64, "{fps} fps: {w:?}");
            }
        }
        let changes = first.len() as f64 - 1.0;
        let per_second = changes / 120.0;
        // A step once more than 100 ms have passed (50 ms more than 10
        // frames from the target), at the next idle pass: 150/100 ms with
        // passes every 50 ms, 101/51 ms with passes every millisecond.
        let (near, far) = if idle_ms == 50 { (150.0, 100.0) } else { (101.0, 51.0) };
        assert!(per_second <= 1000.0 / far && per_second >= 0.95 * 1000.0 / near, "pose {pose}, idle {idle_ms} ms: {per_second} frames/s");
        // Blinks: entering a blink frame, as often as the game's sweeps
        // cross one, the same at every rate.
        let blinks = |seen: &[(f64, i32)]| seen.windows(2).filter(|w| blinking(w[1].1) && !blinking(w[0].1)).count();
        let counts: Vec<usize> = runs.iter().map(|(_, s)| blinks(s)).collect();
        assert!(counts.iter().all(|&c| c == counts[0]), "pose {pose}: {counts:?}");
        eprintln!("pose {pose}, idle {idle_ms} ms: {per_second:.2} frames/s, {} blinks a minute", counts[0] as f64 / 2.0);
        if pose == 50 {
            assert_eq!(counts[0], 0, "a neutral head never reaches a blink");
        } else {
            assert!(counts[0] > 0, "a polite head blinks now and then");
        }
    }
}

#[test]
fn ten_minutes_of_a_head_keep_one_frame_and_the_dialog() {
    let mut m = dialog();
    let mut head = Head::new(121);
    head.pose(78);
    let mut rand = Rand(7);
    // 20 idle passes a second (traces show 15–20) for ten minutes.
    for _ in 0..12_000 {
        head.step(50, &mut rand);
        idle(&mut m, &head);
    }
    let draws = m.frame().draws;
    assert_eq!(shown(&m, 121), head.cur);
    assert!(draws.iter().any(|d| matches!(d, ScreenDraw::Image { path, .. } if path.ends_with("talk_offer.pcx"))), "the background stays");
    assert_eq!(draws.len(), 2, "{draws:?}");
}

#[test]
fn a_head_turning_back_drops_its_frame_from_the_other_file() {
    let mut m = dialog();
    for (flic, chunk) in [(FORWARD, 30), (REVERSE, 120 - 29), (FORWARD, 30)] {
        m.apply(Event::FlcBlit { flic, chunk, canvas: HEAD, x: 0, y: 0, clip: None, plain: true });
        m.apply(Event::Copy { from: HEAD, src: [0, 0, 200, 240], canvas: DIALOG, x: 411, y: 59, keyed: true });
        m.apply(Event::PaintBegin);
        m.apply(Event::Composite { canvas: DIALOG, vtable: 1, origin: (0, 0), size: (1920, 1080), clip: [0, 0, 1920, 1080], keyed: false });
        m.apply(Event::PaintEnd);
        let flcs: Vec<_> = m.frame().draws.into_iter().filter(|d| matches!(d, ScreenDraw::Flc { .. })).collect();
        assert_eq!(flcs.len(), 1, "{flcs:?}");
        assert!(matches!(&flcs[0], ScreenDraw::Flc { path, chunk: c, .. } if *c == chunk && path.ends_with(if flic == FORWARD { "_01.FLC" } else { "_02.FLC" })));
    }
}
