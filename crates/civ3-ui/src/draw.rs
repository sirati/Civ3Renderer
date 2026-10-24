//! The game's fills, lines, translucent rectangles and copies on interface
//! canvases, as the jgl fill and copy groups get them, in the terms of the
//! interface model: rectangles of a colour and regions copied from one
//! canvas to another.
//!
//! [`FillCapture`] and [`CopyCapture`] are the hooks; [`mirrored_fill`] and
//! [`mirrored_copy`] tell which calls they take (whose pixels the
//! interface model has, so jgl's need not be drawn). What they leave out
//! (+0x38, +0x58 and the slots not seen called) jgl still draws.

use jgl::{CopyCall, CopyHooks, FillCall, FillHooks, Rect, Surface};

/// A drawing on a canvas (a `PCX_Image*`, the jgl image's owner).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanvasDraw {
    /// Rectangles (x, y, w, h) in one colour, `alpha` of 255 opaque.
    /// `size`: the canvas's, for the model to know before it (not given
    /// for translucent rectangles).
    Fill { canvas: u32, size: Option<(i32, i32)>, rects: Vec<[i32; 4]>, rgb: [u8; 3], alpha: u8, clip: Option<[i32; 4]> },
    /// Region `src` (x, y, w, h) of `from` copied to (x, y) of `canvas`,
    /// its colour key left out when `keyed`. `sizes`: the canvas's and
    /// `from`'s, for the model to know first.
    Copy { from: u32, src: [i32; 4], canvas: u32, x: i32, y: i32, keyed: bool, sizes: Option<((i32, i32), (i32, i32))>, clip: Option<[i32; 4]> },
}

/// Where the captures hand their drawings.
pub trait DrawSink {
    fn draw(&mut self, draw: CanvasDraw);
}

impl<F: FnMut(CanvasDraw)> DrawSink for F {
    fn draw(&mut self, draw: CanvasDraw) {
        self(draw)
    }
}

/// A 16-bit colour of the game's canvases (RGB555; bit 31 is a flag) as
/// 8-bit RGB.
pub fn rgb555(c: u32) -> [u8; 3] {
    let e = |v: u32| ((v << 3) | (v >> 2)) as u8;
    [e((c >> 10) & 31), e((c >> 5) & 31), e(c & 31)]
}

/// The opacity (of 255) of the game's translucent rectangle fill
/// (`JGL_Image::shade_rect`) that keeps `percent` of what is under it: the
/// colour weighs `1 − a`, what is there `a = percent·256/100` (in 1/256), per
/// 8-bit channel, rounded down — for the percentages the game uses, our
/// translucency (1/32 steps, rounded down) exactly.
pub fn shade_alpha(percent: u32) -> u8 {
    let kept = percent.min(100) * 256 / 100;
    ((256 - kept) * 255 / 256) as u8
}

/// `points` (a line's, in order) as rectangles (x, y, w, h): each run of
/// neighbours along a row or a column one rectangle.
pub fn line_runs(points: &[(i32, i32)]) -> Vec<[i32; 4]> {
    let mut out: Vec<[i32; 4]> = Vec::new();
    for &(x, y) in points {
        if let Some(r) = out.last_mut() {
            let [l, t, w, h] = *r;
            // Along the row (the run so far one high).
            if h == 1 && y == t && (x == l + w || x == l - 1) {
                *r = [l.min(x), t, w + 1, 1];
                continue;
            }
            if w == 1 && x == l && (y == t + h || y == t - 1) {
                *r = [l, t.min(y), 1, h + 1];
                continue;
            }
        }
        out.push([x, y, 1, 1]);
    }
    out
}

/// The pixels of a 1-pixel line, endpoints included (Bresenham).
pub fn line_points((x0, y0): (i32, i32), (x1, y1): (i32, i32)) -> Vec<(i32, i32)> {
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = ((x1 - x0).signum(), (y1 - y0).signum());
    let (mut x, mut y, mut err) = (x0, y0, dx + dy);
    let mut out = Vec::new();
    loop {
        out.push((x, y));
        if (x == x1 && y == y1) || out.len() > 4096 {
            return out;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// The rectangles of a jgl line (`line`, image +0x64): a horizontal or
/// vertical one is one rectangle (the exe's `hline`/`vline`), any other
/// its runs of pixels.
pub fn line_rects(from: (i32, i32), to: (i32, i32)) -> Vec<[i32; 4]> {
    let ((x1, y1), (x2, y2)) = (from, to);
    if y1 == y2 {
        vec![[x1.min(x2), y1, (x2 - x1).abs() + 1, 1]]
    } else if x1 == x2 {
        vec![[x1, y1.min(y2), 1, (y2 - y1).abs() + 1]]
    } else {
        line_runs(&line_points(from, to))
    }
}

/// A canvas's clip at a call, when it is smaller than the canvas and not
/// empty (a clip equal to the canvas clips nothing).
pub fn canvas_clip(s: &Surface) -> Option<[i32; 4]> {
    let (clip, full) = (s.clip, s.bounds);
    (clip != full && clip.right > clip.left && clip.bottom > clip.top).then_some(clip.to_array())
}

fn xywh(r: Rect) -> [i32; 4] {
    [r.left, r.top, r.right - r.left, r.bottom - r.top]
}

/// The whole of `s`, or `rect` (x, y, w, h).
fn area(s: &Surface, rect: Option<Rect>) -> [i32; 4] {
    match rect {
        Some(r) => xywh(r),
        None => {
            let (w, h) = s.size();
            [0, 0, w, h]
        }
    }
}

/// Whether a shade colour is a direct colour (bit 31): palette colours
/// are not used by the interface code we know of, and are left to jgl.
fn direct(color: u32) -> bool {
    color & 0x8000_0000 != 0
}

/// The canvas whose pixels the interface model takes from `call`: drawing
/// `call` there is left out where the model shows that canvas. `None`:
/// jgl still draws it (no owner, a call the model does not take).
pub fn mirrored_fill(call: &FillCall) -> Option<u32> {
    let owner = call.target().owner;
    let taken = match call {
        FillCall::Fill { .. } | FillCall::Line { .. } => true,
        FillCall::Shade { color, .. } => direct(*color),
        FillCall::ReplaceColor { .. } | FillCall::Other { .. } => false,
    };
    (taken && owner != 0).then_some(owner)
}

/// The canvas and the canvas copied from whose pixels the interface model
/// takes from `call` (see [`mirrored_fill`]).
pub fn mirrored_copy(call: &CopyCall) -> Option<(u32, u32)> {
    match call {
        CopyCall::Stretch { source, target, .. } | CopyCall::Keyed { source, target, .. } => {
            (source.owner != 0 && target.owner != 0).then_some((target.owner, source.owner))
        }
        CopyCall::Indexed { .. } | CopyCall::Other { .. } => None,
    }
}

/// What `call` draws in the interface model's terms.
pub fn fill_of(call: &FillCall) -> Option<CanvasDraw> {
    let canvas = mirrored_fill(call)?;
    let target = call.target();
    let (size, rects, color, alpha) = match *call {
        FillCall::Fill { rect, color, .. } => (Some(target.size()), vec![area(target, rect)], color, 255),
        FillCall::Line { from, to, color, .. } => (Some(target.size()), line_rects(from, to), color, 255),
        FillCall::Shade { rect, color, percent, .. } => {
            let alpha = shade_alpha((percent as i32).clamp(0, 100) as u32);
            if alpha == 0 {
                return None;
            }
            (None, vec![area(target, rect)], color, alpha)
        }
        FillCall::ReplaceColor { .. } | FillCall::Other { .. } => return None,
    };
    Some(CanvasDraw::Fill { canvas, size, rects, rgb: rgb555(color), alpha, clip: canvas_clip(target) })
}

/// What `call` draws in the interface model's terms.
pub fn copy_of(call: &CopyCall) -> Option<CanvasDraw> {
    let (canvas, from) = mirrored_copy(call)?;
    Some(match *call {
        CopyCall::Stretch { source, target, src, dst } => CanvasDraw::Copy {
            from,
            src: xywh(src),
            canvas,
            x: dst.left,
            y: dst.top,
            keyed: false,
            sizes: Some((target.size(), source.size())),
            clip: canvas_clip(&target),
        },
        CopyCall::Keyed { source, target, x, y } => {
            CanvasDraw::Copy { from, src: area(&source, None), canvas, x, y, keyed: true, sizes: None, clip: canvas_clip(&target) }
        }
        CopyCall::Indexed { .. } | CopyCall::Other { .. } => return None,
    })
}

/// The fill hooks of the interface.
pub struct FillCapture<S> {
    sink: S,
}

impl<S: DrawSink> FillCapture<S> {
    pub fn new(sink: S) -> FillCapture<S> {
        FillCapture { sink }
    }
}

impl<S: DrawSink> FillHooks for FillCapture<S> {
    fn fill(&mut self, call: &FillCall) {
        if let Some(d) = fill_of(call) {
            self.sink.draw(d);
        }
    }
}

/// The copy hooks of the interface.
pub struct CopyCapture<S> {
    sink: S,
}

impl<S: DrawSink> CopyCapture<S> {
    pub fn new(sink: S) -> CopyCapture<S> {
        CopyCapture { sink }
    }
}

impl<S: DrawSink> CopyHooks for CopyCapture<S> {
    fn copy(&mut self, call: &CopyCall) {
        if let Some(d) = copy_of(call) {
            self.sink.draw(d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jgl::{FontDesc, Hooks, ImageId, Jgl};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn canvas(owner: u32, clip: Rect) -> Surface {
        Surface { image: ImageId(owner + 0x100), owner, clip, bounds: Rect::new(0, 0, 200, 100) }
    }

    const FULL: Rect = Rect::new(0, 0, 200, 100);

    #[test]
    fn lines() {
        let runs = line_runs(&line_points((0, 32), (64, 0)));
        assert_eq!(runs.len(), 33);
        assert_eq!(line_rects((40, 5), (5, 5)), [[5, 5, 36, 1]]);
        assert_eq!(line_rects((10, 40), (10, 10)), [[10, 10, 1, 31]]);
        assert_eq!(line_rects((3, 3), (3, 3)), [[3, 3, 1, 1]]);
        // Axis-aligned lines as runs come out the same.
        assert_eq!(line_rects((40, 5), (5, 5)), line_runs(&line_points((40, 5), (5, 5))));
        assert_eq!(line_rects((10, 40), (10, 10)), line_runs(&line_points((10, 40), (10, 10))));
    }

    #[test]
    fn colours_and_shades() {
        assert_eq!(rgb555(0x8000_7C00), [255, 0, 0]);
        assert_eq!(rgb555(0x1F), [0, 0, 255]);
        assert_eq!(shade_alpha(100), 0);
        assert_eq!(shade_alpha(0), 255);
        assert_eq!(shade_alpha(50), 127);
    }

    #[test]
    fn fills_lines_and_shades_become_rectangles() {
        let t = canvas(0x9000, Rect::new(10, 10, 50, 50));
        let fill = fill_of(&FillCall::Fill { target: t, rect: None, color: 0x7C00 }).unwrap();
        assert_eq!(fill, CanvasDraw::Fill { canvas: 0x9000, size: Some((200, 100)), rects: vec![[0, 0, 200, 100]], rgb: [255, 0, 0], alpha: 255, clip: Some([10, 10, 50, 50]) });
        let area = fill_of(&FillCall::Fill { target: canvas(0x9000, FULL), rect: Some(Rect::new(4, 5, 14, 25)), color: 0 }).unwrap();
        assert!(matches!(area, CanvasDraw::Fill { ref rects, clip: None, .. } if rects == &vec![[4, 5, 10, 20]]));
        let line = fill_of(&FillCall::Line { target: t, from: (0, 0), to: (3, 1), color: 0, flag: 1 }).unwrap();
        assert!(matches!(line, CanvasDraw::Fill { ref rects, .. } if rects.len() == 2));
        let shade = fill_of(&FillCall::Shade { target: t, rect: Some(Rect::new(0, 0, 8, 8)), color: 0x8000_0000, percent: 60 }).unwrap();
        assert!(matches!(shade, CanvasDraw::Fill { size: None, alpha: 102, .. }));
        // A palette shade and a full shade: left to jgl / nothing to draw.
        assert_eq!(mirrored_fill(&FillCall::Shade { target: t, rect: None, color: 5, percent: 60 }), None);
        assert_eq!(fill_of(&FillCall::Shade { target: t, rect: None, color: 0x8000_0000, percent: 100 }), None);
        assert_eq!(mirrored_fill(&FillCall::Shade { target: t, rect: None, color: 0x8000_0000, percent: 100 }), Some(0x9000));
        // Not taken: jgl draws them.
        assert_eq!(mirrored_fill(&FillCall::ReplaceColor { target: t, rect: FULL, from: 1, to: 2 }), None);
        assert_eq!(mirrored_fill(&FillCall::Fill { target: canvas(0, FULL), rect: None, color: 0 }), None);
    }

    #[test]
    fn copies() {
        let (src, dst) = (canvas(0x7000, FULL), canvas(0x9000, Rect::new(0, 0, 100, 100)));
        let copy = copy_of(&CopyCall::Stretch { source: src, target: dst, src: Rect::new(10, 20, 40, 60), dst: Rect::new(5, 6, 35, 46) }).unwrap();
        assert_eq!(copy, CanvasDraw::Copy { from: 0x7000, src: [10, 20, 30, 40], canvas: 0x9000, x: 5, y: 6, keyed: false, sizes: Some(((200, 100), (200, 100))), clip: Some([0, 0, 100, 100]) });
        let keyed = copy_of(&CopyCall::Keyed { source: src, target: dst, x: -3, y: 4 }).unwrap();
        assert_eq!(keyed, CanvasDraw::Copy { from: 0x7000, src: [0, 0, 200, 100], canvas: 0x9000, x: -3, y: 4, keyed: true, sizes: None, clip: Some([0, 0, 100, 100]) });
        let indexed = CopyCall::Indexed { source: src, target: dst, src: (0, 0), at: (0, 0), size: (1, 1), mode: 0 };
        assert_eq!(mirrored_copy(&indexed), None);
        assert_eq!(copy_of(&indexed), None);
    }

    #[test]
    fn jgl_calls_reach_the_sink() {
        let out = Rc::new(RefCell::new(Vec::new()));
        let (a, b) = (out.clone(), out.clone());
        let stock = FontDesc { face: "System".into(), height: 16, weight: 700, italic: false, underline: false };
        let hooks = Hooks {
            fill: Box::new(FillCapture::new(move |d| a.borrow_mut().push(d))),
            copy: Box::new(CopyCapture::new(move |d| b.borrow_mut().push(d))),
            ..Hooks::default()
        };
        let mut j = Jgl::new(hooks, stock);
        let t = canvas(0x9000, FULL);
        assert!(j.with(|p| p.fill(&FillCall::Fill { target: t, rect: None, color: 0 })));
        j.with(|p| p.fill(&FillCall::ReplaceColor { target: t, rect: FULL, from: 0, to: 1 }));
        j.with(|p| p.copy(&CopyCall::Keyed { source: canvas(0x7000, FULL), target: t, x: 0, y: 0 }));
        let out = out.borrow();
        assert_eq!(out.len(), 2);
        assert!(matches!(out[0], CanvasDraw::Fill { canvas: 0x9000, .. }));
        assert!(matches!(out[1], CanvasDraw::Copy { from: 0x7000, canvas: 0x9000, keyed: true, .. }));
    }
}
