//! Cost of the mirror's per-draw work on the game's thread (run with
//! `cargo test --release -p civ3-bridge occlusion_bench -- --ignored --nocapture`).

use super::*;
use std::time::Instant;

fn tiles(n: usize, (cw, ch): (i32, i32), (tw, th): (i32, i32)) -> Vec<[i32; 4]> {
    let cols = cw / tw;
    (0..n as i32).map(|i| [(i % cols) * tw, (i / cols) * th % ch, tw, th]).collect()
}

fn draw(m: &mut UiModel, canvas: u32, rects: &[[i32; 4]]) {
    for (i, &rect) in rects.iter().enumerate() {
        m.apply(Event::Fill { canvas, rect, rgb: [(i % 251) as u8, 3, 4], alpha: 255, clip: None });
    }
}

#[test]
#[ignore]
fn occlusion_bench() {
    for n in [8000, 20000] {
        let canvas = 0x9f8934;
        let mut m = UiModel::new(vec![], vec![]).with_screen(999);
        m.apply(Event::Size { image: 999, size: (1920, 1080) });
        m.apply(Event::Size { image: canvas, size: (3274, 2048) });
        let rects = tiles(n, (3274, 2048), (16, 16));
        let t = Instant::now();
        draw(&mut m, canvas, &rects);
        let first = t.elapsed();
        let t = Instant::now();
        draw(&mut m, canvas, &rects);
        let redraw = t.elapsed();
        let t = Instant::now();
        for _ in 0..10 {
            m.apply(Event::PaintBegin);
            m.apply(Event::Composite { canvas, vtable: 1, origin: (0, 0), size: (3274, 2048), clip: [0, 0, 1920, 1080], keyed: false });
            m.apply(Event::CompositeEnd);
            m.apply(Event::PaintEnd);
            m.apply(Event::PresentBegin);
            m.apply(Event::Present { rect: None });
        }
        let composite = t.elapsed() / 10;
        // Overlapping fills of random sizes.
        let mut seed = 1u32;
        let mut rnd = |k: i32| { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); ((seed >> 8) as i32).rem_euclid(k) };
        let random: Vec<[i32; 4]> = (0..n).map(|_| [rnd(3200), rnd(2000), 1 + rnd(64), 1 + rnd(64)]).collect();
        let mut r = UiModel::new(vec![], vec![]);
        r.apply(Event::Size { image: canvas, size: (3274, 2048) });
        let t = Instant::now();
        draw(&mut r, canvas, &random);
        draw(&mut r, canvas, &random);
        let overlapping = t.elapsed();
        eprintln!("{n} opaque fills: first {first:?}, redraw {redraw:?}, composite+present {composite:?}, ops {}; random overlapping x2 {overlapping:?}, ops {}", m.stats().1, r.stats().1);
    }
}

/// The display list of one canvas as the mirror kept it before it was
/// indexed (a plain list searched whole by every draw): what the indexed
/// one must hold after the same draws.
struct Plain {
    ops: Vec<Op>,
    compacted: usize,
    size: (i32, i32),
}

/// [`super::covered`] as it was: through every rectangle.
fn plain_covered(rect: [i32; 4], cover: &[[i32; 4]]) -> bool {
    let mut left = vec![rect];
    for c in cover {
        left = left.into_iter().flat_map(|r| subtract(r, *c)).collect();
        if left.is_empty() {
            return true;
        }
        if left.len() > MAX_FRAGMENTS {
            return false;
        }
    }
    false
}

fn plain_shown(ops: &[Op], bounds: impl Fn(&Op) -> [i32; 4], opaque: impl Fn(&Op) -> bool) -> Vec<bool> {
    let mut cover: Vec<[i32; 4]> = Vec::new();
    let mut shown = vec![true; ops.len()];
    for (i, op) in ops.iter().enumerate().rev() {
        let b = bounds(op);
        if b[2] > b[0] && b[3] > b[1] && plain_covered(b, &cover) {
            shown[i] = false;
            continue;
        }
        if cover.len() < MAX_COVER && b[2] > b[0] && b[3] > b[1] && opaque(op) {
            cover.push(b);
        }
    }
    shown
}

impl Plain {
    fn covers(&self, [x, y, w, h]: [i32; 4]) -> bool {
        let (cw, ch) = self.size;
        x <= 0 && y <= 0 && x + w >= cw && y + h >= ch
    }

    fn occlude(&mut self, rect: [i32; 4]) {
        if self.covers(rect) {
            self.ops.clear();
            return;
        }
        let [x, y, w, h] = rect;
        let inside = |[l, t, r, b]: [i32; 4]| l >= x && t >= y && r <= x + w && b <= y + h;
        self.ops.retain(|op| !inside(op_bounds(op)));
    }

    fn push(&mut self, model: &UiModel, op: Op) {
        if matches!(op, Op::Image { .. }) && model.opaque(&op) {
            let [l, t, r, b] = op_bounds(&op);
            if self.covers([l, t, r - l, b - t]) {
                self.ops.clear();
            }
        }
        let ops = &mut self.ops;
        if overdraws_itself(&op) {
            if let Some(i) = ops.iter().rposition(|o| *o == op) {
                ops.remove(i);
            }
        }
        if let Op::Flc { x, y, plain: true, .. } = &op {
            let at = (*x, *y);
            ops.retain(|o| !matches!(o, Op::Flc { x, y, plain: true, .. } if (*x, *y) == at));
        }
        if ops.len() >= MAX_OPS {
            let backdrop = usize::from(matches!(ops.first(), Some(Op::Fill { alpha: 255, .. })));
            ops.drain(backdrop..MAX_OPS / 2);
        }
        ops.push(op);
        if ops.len() >= COMPACT_MIN.max(2 * self.compacted) {
            let size = Some(self.size);
            let shown = plain_shown(ops, |op| visible_bounds(op, size), |op| model.opaque(op));
            let mut i = 0;
            ops.retain(|_| {
                i += 1;
                shown[i - 1]
            });
            self.compacted = ops.len();
        }
    }

    fn apply(&mut self, model: &UiModel, e: &Event) {
        match e.clone() {
            Event::Fill { rect, rgb, alpha, clip: None, .. } => {
                if rgb == KEY_RGB && alpha == 255 {
                    if self.covers(rect) {
                        self.ops.clear();
                    } else {
                        let [x,y,w,h]=rect;
                        let regions=subtract([0,0,self.size.0,self.size.1],[x,y,x+w,y+h]);
                        self.ops=self.ops.iter().flat_map(|op|regions.iter().filter_map(|r|crop_move(op.clone(),*r,(0,0),false))).collect();
                    }
                    self.push(model,Op::KeyClear {rect});
                    return;
                }
                if alpha == 255 {
                    self.occlude(rect);
                }
                self.push(model, Op::Fill { rect, rgb, alpha });
            }
            Event::Text { run, .. } => self.push(model, Op::Text(run, ALL)),
            Event::Blit { sprite, x, y, clip: None, .. } => {
                let source = model.sprites[&sprite].clone();
                self.push(model, Op::Image { source, x, y, keyed: true });
            }
            Event::Copy { from, src, x, y, keyed, .. } => {
                if !keyed {
                    self.occlude([x, y, src[2], src[3]]);
                }
                let source = Source::File { retained:None,path: model.files[&from].clone(), rect: src, mask: None, palette: None, opacity: 255 };
                self.push(model, Op::Image { source, x, y, keyed });
            }
            Event::FlcBlit { flic, chunk, x, y, clip, plain, .. } => {
                let path = model.flcs[&flic].clone();
                self.ops.retain(|op| !matches!(op, Op::Flc { path: p, x: px, y: py, .. } if *p == path && (*px, *py) == (x, y)));
                self.push(model, Op::Flc { path, chunk, x, y, clip, plain });
            }
            other => unreachable!("not generated: {other:?}"),
        }
    }
}

/// Random draws onto one canvas: mostly opaque fills of all sizes (some
/// inverted or empty, some covering it all), translucent ones, clears to
/// the key, text, pictures (of a file known opaque and one not), opaque
/// and keyed copies, animation frames.
fn random_draws(seed: u32, n: usize, (cw, ch): (i32, i32), canvas: u32, small_only: bool) -> Vec<Event> {
    let mut s = seed.wrapping_mul(2654435761).wrapping_add(1);
    let mut rnd = move |k: i32| {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        (s as i32).rem_euclid(k.max(1))
    };
    (0..n).map(|_| {
        let small = small_only || rnd(4) != 0;
        let (w, h) = if small { (rnd(40) - 2, rnd(40) - 2) } else { (rnd(cw + 40), rnd(ch + 40)) };
        let (x, y) = (rnd(cw + 40) - 20, rnd(ch + 40) - 20);
        let rect = [x, y, w, h];
        match rnd(20) {
            0..=8 => Event::Fill { canvas, rect, rgb: [rnd(4) as u8, 0, 0], alpha: 255, clip: None },
            9 if !small_only && rnd(8) == 0 => Event::Fill { canvas, rect: [0, 0, cw, ch], rgb: [9, 9, 9], alpha: 255, clip: None },
            9 | 10 => Event::Fill { canvas, rect, rgb: [1, 2, 3], alpha: 100 + rnd(100) as u8, clip: None },
            11 => Event::Fill { canvas, rect, rgb: KEY_RGB, alpha: 255, clip: None },
            12 | 13 => Event::Text { canvas, run: TextRun { x, y, color: [0; 3], face: "Times".into(), height: -12, weight: 400, italic: false, underline: false, text: "ab".repeat(1 + rnd(4) as usize), backdrop: None } },
            14 | 15 => Event::Blit { sprite: 20 + rnd(2) as u32, canvas, x: x - x % 8, y: y - y % 8, clip: None, palette: None },
            16 | 17 => Event::Copy { from: 10 + rnd(2) as u32, src: [rnd(8), rnd(8), 1 + rnd(30), 1 + rnd(30)], canvas, x: x - x % 4, y: y - y % 4, keyed: rnd(2) == 0 },
            _ => Event::FlcBlit { flic: 30, chunk: rnd(5) as u32, canvas, x: x - x % 16, y: y - y % 16, clip: None, plain: rnd(2) == 0 },
        }
    }).collect()
}

fn equivalence_model(size: (i32, i32), canvas: u32) -> UiModel {
    let mut m = UiModel::new(vec![], vec![]).with_opaque_files(Arc::new(|path, _| Some(path == "opaque.pcx")));
    m.apply(Event::Size { image: canvas, size });
    m.apply(Event::Loaded { image: 10, path: "opaque.pcx".into(), size: (64, 64) });
    m.apply(Event::Loaded { image: 11, path: "holes.pcx".into(), size: (64, 64) });
    m.apply(Event::Sliced { sprite: 20, image: 10, rect: [0, 0, 24, 16] });
    m.apply(Event::Sliced { sprite: 21, image: 11, rect: [0, 0, 16, 24] });
    m.apply(Event::FlcLoaded { flic: 30, path: "head.flc".into() });
    m
}

fn assert_same_as_plain(seed: u32, n: usize, size: (i32, i32), every: usize, small_only: bool) -> usize {
    let canvas = 300;
    let mut m = equivalence_model(size, canvas);
    let mut plain = Plain { ops: Vec::new(), compacted: 0, size };
    let mut most = 0;
    for (i, e) in random_draws(seed, n, size, canvas, small_only).into_iter().enumerate() {
        plain.apply(&m, &e);
        most = most.max(plain.ops.len());
        m.apply(e);
        if i % every == 0 || i + 1 == n {
            let held = m.canvases.get(&canvas).map(DisplayList::to_vec).unwrap_or_default();
            assert!(held == plain.ops, "seed {seed}, after draw {i}: {} held, {} before", held.len(), plain.ops.len());
            assert_eq!(m.compacted.get(&canvas).copied().unwrap_or(0), plain.compacted, "seed {seed}, after draw {i}");
        }
    }
    most
}

#[test]
fn indexed_display_lists_hold_what_plain_ones_did() {
    for seed in 0..40 {
        assert_same_as_plain(seed, 1500, (300, 200), 1, false);
    }
    // A large canvas, long enough to reach the cap on held operations.
    for seed in 100..101 {
        assert_same_as_plain(seed, 40_000, (3274, 2048), 97, false);
        assert!(assert_same_as_plain(seed, 30_000, (3274, 2048), 97, true) >= MAX_OPS - 1, "reaches the cap");
    }
}

#[test]
fn indexed_cover_tests_say_what_testing_every_rectangle_did() {
    let size = Some((800, 600));
    for seed in 0..30 {
        let ops: Vec<Op> = random_draws(seed, 3000, (800, 600), 1, false).into_iter().filter_map(|e| match e {
            Event::Fill { rect, rgb, alpha, .. } => Some(Op::Fill { rect, rgb, alpha }),
            Event::Text { run, .. } => Some(Op::Text(run, ALL)),
            _ => None,
        }).collect();
        let opaque = |op: &Op| matches!(op, Op::Fill { alpha: 255, .. });
        assert_eq!(shown_ops(&ops, |op| visible_bounds(op, size), opaque), plain_shown(&ops, |op| visible_bounds(op, size), opaque), "seed {seed}");
        // Unknown size: bounds unbounded by the canvas.
        assert_eq!(shown_ops(&ops, |op| visible_bounds(op, None), opaque), plain_shown(&ops, |op| visible_bounds(op, None), opaque), "seed {seed}");
    }
}

#[test]
fn an_unchanged_canvas_keeps_its_snapshot_and_a_changed_one_or_its_copied_surface_does_not() {
    let (form, temp) = (300, 400);
    let mut m = UiModel::new(vec![], vec![]);
    m.apply(Event::Size { image: temp, size: (10, 10) });
    let paint = |m: &mut UiModel| {
        m.apply(Event::PaintBegin);
        m.apply(Event::Composite { canvas: form, vtable: 1, origin: (0, 0), size: (100, 100), clip: [0, 0, 100, 100], keyed: false });
        m.apply(Event::CompositeEnd);
        m.apply(Event::PaintEnd);
        m.frame().draws
    };
    let fill = |rect: [i32; 4], rgb: [u8; 3]| ScreenDraw::Fill { rect, rgb, alpha: 255, clip: [0, 0, 100, 100] };
    m.apply(Event::Fill { canvas: form, rect: [0, 0, 50, 50], rgb: [1, 1, 1], alpha: 255, clip: None });
    assert_eq!(paint(&mut m), [fill([0, 0, 50, 50], [1, 1, 1])]);
    let snapshot = m.presented[&form].clone();
    assert_eq!(paint(&mut m), [fill([0, 0, 50, 50], [1, 1, 1])]);
    assert!(Arc::ptr_eq(&snapshot, &m.presented[&form]), "kept");
    m.apply(Event::Fill { canvas: form, rect: [50, 0, 50, 50], rgb: [2, 2, 2], alpha: 255, clip: None });
    assert_eq!(paint(&mut m), [fill([0, 0, 50, 50], [1, 1, 1]), fill([50, 0, 50, 50], [2, 2, 2])]);
    // Cleared and drawn again as it was: the same pixels, whichever list.
    m.apply(Event::Fill { canvas: form, rect: [0, 0, 100, 100], rgb: [3, 3, 3], alpha: 255, clip: None });
    assert_eq!(paint(&mut m), [fill([0, 0, 100, 100], [3, 3, 3])]);
    // A sprite of another surface: what it shows follows that surface.
    m.apply(Event::Sliced { sprite: 20, image: temp, rect: [0, 0, 10, 10] });
    m.apply(Event::Blit { sprite: 20, canvas: form, x: 5, y: 5, clip: None, palette: None });
    m.apply(Event::Fill { canvas: temp, rect: [0, 0, 10, 10], rgb: [4, 4, 4], alpha: 255, clip: None });
    assert_eq!(paint(&mut m), [fill([0, 0, 100, 100], [3, 3, 3]), fill([5, 5, 10, 10], [4, 4, 4])]);
    m.apply(Event::Fill { canvas: temp, rect: [0, 0, 10, 10], rgb: [5, 5, 5], alpha: 255, clip: None });
    assert_eq!(paint(&mut m), [fill([0, 0, 100, 100], [3, 3, 3]), fill([5, 5, 10, 10], [5, 5, 5])]);
}
