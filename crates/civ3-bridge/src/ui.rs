//! Mirroring the game's user interface (docs §16), independent of where the
//! events come from (hooks in the game, or a trace).
//!
//! The game draws its UI into retained canvases, one per form or control,
//! and composes them onto the screen when it paints. We follow that: every
//! drawing event lands in its canvas' display list, and a paint turns the
//! composed canvases into screen draws, each an image region of a game file
//! (or a fill, or text) at a screen position, clipped. Canvases whose
//! content we render ourselves (the map) are left out.

use std::collections::HashMap;

use crate::text::TextRun;

/// Where a sprite's pixels come from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Source {
    /// A region of an image file (path as the game opened it).
    File { path: String, rect: [i32; 4] },
    /// A region of another surface (a canvas or temporary image).
    Surface { image: u32, rect: [i32; 4] },
}

/// One drawing event (game addresses identify images, sprites and forms).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// `image` now holds the file at `path`, `size` pixels.
    Loaded { image: u32, path: String, size: (i32, i32) },
    /// `image` lost its pixels.
    Cleared { image: u32 },
    /// `image` (e.g. a canvas) is `size` pixels.
    Size { image: u32, size: (i32, i32) },
    /// `sprite` was cut from `image`: its stored pixels are `rect` (x, y,
    /// w, h) of it.
    Sliced { sprite: u32, image: u32, rect: [i32; 4] },
    /// `sprite` is a copy of `source`.
    Copied { sprite: u32, source: u32 },
    /// `sprite` lost its pixels.
    Reset { sprite: u32 },
    /// `sprite`'s pixels drawn with their top-left at (x, y) of `canvas`,
    /// colour 255 transparent.
    Blit { sprite: u32, canvas: u32, x: i32, y: i32 },
    /// Region `src` (x, y, w, h) of image `from` copied to (x, y) of
    /// `canvas`; `keyed`: colour 255 transparent.
    Copy { from: u32, src: [i32; 4], canvas: u32, x: i32, y: i32, keyed: bool },
    /// `rect` (x, y, w, h) of `canvas` filled (`rgb`).
    /// `alpha`: 255 opaque, else a translucent shade (50 % = 128).
    Fill { canvas: u32, rect: [i32; 4], rgb: [u8; 3], alpha: u8 },
    /// Text on `canvas` (its position canvas-relative).
    Text { canvas: u32, run: TextRun },
    /// A paint starts: the forms that follow are composed bottom to top.
    PaintBegin,
    /// A form's canvas put on the screen with its (0, 0) at `origin`, clipped
    /// to `clip` (left, top, right, bottom).
    Composite { canvas: u32, vtable: u32, origin: (i32, i32), size: (i32, i32), clip: [i32; 4], keyed: bool },
    /// The paint is complete.
    PaintEnd,
}

/// What a canvas shows: drawing operations, oldest first.
#[derive(Clone, Debug, PartialEq)]
enum Op {
    /// An image region at (x, y); `keyed`: colour 255 transparent.
    Image { source: Source, x: i32, y: i32, keyed: bool },
    Fill { rect: [i32; 4], rgb: [u8; 3], alpha: u8 },
    Text(TextRun),
}

/// One thing on the screen.
#[derive(Clone, Debug, PartialEq)]
pub enum ScreenDraw {
    /// Region `rect` (x, y, w, h) of an image file at (x, y).
    Image { path: String, rect: [i32; 4], x: i32, y: i32, keyed: bool, clip: [i32; 4] },
    Fill { rect: [i32; 4], rgb: [u8; 3], alpha: u8, clip: [i32; 4] },
    Text { run: TextRun, clip: [i32; 4] },
    /// Region `rect` (x, y, w, h) of a surface the game generates itself
    /// (e.g. the minimap), at (x, y).
    Surface { image: u32, rect: [i32; 4], x: i32, y: i32, clip: [i32; 4] },
}

/// A composed screen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub draws: Vec<ScreenDraw>,
    /// The forms composed, bottom to top: vtable, canvas, origin, size
    /// (mirrored or not; for finding out what is what).
    pub forms: Vec<(u32, u32, (i32, i32), (i32, i32))>,
    /// Per composed form: its canvas and the range of `draws` it made.
    pub form_draws: Vec<(u32, std::ops::Range<usize>)>,
}

/// The game's colour key as 8-bit RGB (0x7C1F).
const KEY_RGB: [u8; 3] = [255, 0, 255];

/// Longest display list a canvas keeps (older operations are dropped).
const MAX_OPS: usize = 8192;
/// How deep copies between canvases are followed.
const MAX_DEPTH: u32 = 8;

/// The mirrored UI state.
#[derive(Default)]
pub struct UiModel {
    /// Files of loaded images.
    files: HashMap<u32, String>,
    /// Sizes of images and canvases.
    sizes: HashMap<u32, (i32, i32)>,
    /// Where sprites come from.
    sprites: HashMap<u32, Source>,
    /// Display lists of canvases (and every other drawn-into image).
    canvases: HashMap<u32, Vec<Op>>,
    /// The paint in progress.
    painting: Option<Frame>,
    /// The last complete paint.
    frame: Frame,
    /// Form classes (vtables) whose canvases we render ourselves.
    skip: Vec<u32>,
    /// Canvases left out of every paint.
    skip_canvases: Vec<u32>,
    /// The back buffer: drawing into it during a paint lands on the screen
    /// directly (frames, the tooltip, the mouse cursor).
    screen: u32,
    /// Whether `skip_canvases` are left out now.
    skipping: bool,
    /// Canvases under the interface whose own content we render (the map):
    /// while skipping, mirrored without full-canvas fills and without
    /// images from files containing one of `underlay_art` (lowercase).
    underlays: Vec<u32>,
    underlay_art: Vec<String>,
    /// Surfaces the game fills itself (pixel by pixel): not followed,
    /// passed on as [`ScreenDraw::Surface`].
    generated: Vec<u32>,
}

impl UiModel {
    /// `skip_vtables`: forms not mirrored (e.g. the map); `skip_canvases`:
    /// canvases not mirrored.
    pub fn new(skip_vtables: Vec<u32>, skip_canvases: Vec<u32>) -> Self {
        Self { skip: skip_vtables, skip_canvases, skipping: true, ..Default::default() }
    }

    /// Leave the skipped canvases out (or not, e.g. between games).
    pub fn set_skipping(&mut self, on: bool) {
        self.skipping = on;
    }

    /// `canvases`: underlays (see the field); `art`: their images left out.
    pub fn with_underlays(mut self, canvases: Vec<u32>, art: Vec<String>) -> Self {
        self.underlays = canvases;
        self.underlay_art = art;
        self
    }


    /// `generated`: surfaces passed on as they are.
    pub fn with_generated(mut self, generated: Vec<u32>) -> Self {
        self.generated = generated;
        self
    }

    /// `screen`: the back buffer.
    pub fn with_screen(mut self, screen: u32) -> Self {
        self.screen = screen;
        self
    }

    /// The last complete screen.
    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    pub fn apply(&mut self, e: Event) {
        match e {
            Event::Loaded { image, path, size } => {
                self.files.insert(image, path);
                self.sizes.insert(image, size);
                self.canvases.remove(&image);
            }
            Event::Cleared { image } => {
                self.files.remove(&image);
                self.sizes.remove(&image);
                self.canvases.remove(&image);
            }
            Event::Size { image, size } => {
                self.sizes.insert(image, size);
            }
            Event::Sliced { sprite, image, rect } => {
                let source = match self.files.get(&image) {
                    Some(path) => Source::File { path: path.clone(), rect },
                    None => Source::Surface { image, rect },
                };
                self.sprites.insert(sprite, source);
            }
            Event::Copied { sprite, source } => {
                if let Some(s) = self.sprites.get(&source).cloned() {
                    self.sprites.insert(sprite, s);
                }
            }
            Event::Reset { sprite } => {
                self.sprites.remove(&sprite);
            }
            Event::Blit { sprite, canvas, x, y } => {
                if let Some(source) = self.sprites.get(&sprite).cloned() {
                    self.push(canvas, Op::Image { source, x, y, keyed: true });
                }
            }
            Event::Copy { from, src, canvas, x, y, keyed } => {
                // The map canvas is ours: its content (terrain, the fog tiles drawn on it)
                // must not reach the screen through the form's own copy onto the back buffer.
                if self.skipping && (self.skip_canvases.contains(&from) || self.underlays.contains(&from)) {
                    return;
                }
                let source = match self.files.get(&from) {
                    Some(path) => Source::File { path: path.clone(), rect: src },
                    None => Source::Surface { image: from, rect: src },
                };
                // An opaque copy over the whole canvas replaces it.
                if !keyed && self.covers(canvas, [x, y, src[2], src[3]]) {
                    self.canvases.remove(&canvas);
                }
                // A copy of a canvas into itself would refer to its own
                // (changing) content.
                if from == canvas {
                    return;
                }
                let op = Op::Image { source, x, y, keyed };
                // Copies of surfaces take their content as it is now.
                let ops = self.resolve(&op, 0);
                for op in ops {
                    self.push(canvas, op);
                }
            }
            Event::Fill { canvas, rect, rgb, alpha } => {
                // The colour key (255, 0, 255) clears to transparent: nothing to draw.
                if rgb == KEY_RGB && alpha == 255 {
                    if self.covers(canvas, rect) {
                        self.canvases.remove(&canvas);
                    } else if let Some(ops) = self.canvases.get_mut(&canvas) {
                        // Clearing part of an overlay (the game redraws e.g. the unit
                        // selection every frame): drop what lies inside.
                        let [x, y, w, h] = rect;
                        let inside = |l: i32, t: i32, r: i32, b: i32| l >= x && t >= y && r <= x + w && b <= y + h;
                        ops.retain(|op| match op {
                            Op::Fill { rect: [fx, fy, fw, fh], .. } => !inside(*fx, *fy, fx + fw, fy + fh),
                            Op::Text(run) => !inside(run.x, run.y, run.x + 1, run.y + 1),
                            Op::Image { .. } => true,
                        });
                    }
                    return;
                }
                if alpha == 255 && self.covers(canvas, rect) {
                    self.canvases.remove(&canvas);
                }
                self.push(canvas, Op::Fill { rect, rgb, alpha });
            }
            Event::Text { canvas, run } => self.push(canvas, Op::Text(run)),
            Event::PaintBegin => self.painting = Some(Frame::default()),
            Event::Composite { canvas, vtable, origin, size, clip, keyed: _ } => {
                if let Some(f) = self.painting.as_mut() {
                    f.forms.push((vtable, canvas, origin, size));
                }
                if self.skip.contains(&vtable) || (self.skipping && self.skip_canvases.contains(&canvas)) {
                    return;
                }
                if self.painting.is_none() {
                    return;
                }
                // The canvas never extends beyond its size.
                let clip = intersect(clip, [origin.0, origin.1, origin.0 + size.0, origin.1 + size.1]);
                if clip[2] <= clip[0] || clip[3] <= clip[1] {
                    return;
                }
                self.sizes.insert(canvas, size);
                let underlay = self.skipping && self.underlays.contains(&canvas);
                let keep = |op: &&Op| !underlay || self.underlay_keeps(canvas, op);
                let ops = self.canvases.get(&canvas).map(|ops| ops.iter().filter(keep).flat_map(|op| self.resolve(op, 0)).collect::<Vec<_>>());
                if let Some(frame) = self.painting.as_mut() {
                    let start = frame.draws.len();
                    frame.draws.extend(ops.into_iter().flatten().filter_map(|op| screen_draw(op, origin, clip)));
                    frame.form_draws.push((canvas, start..frame.draws.len()));
                }
            }
            Event::PaintEnd => {
                if let Some(f) = self.painting.take() {
                    self.frame = f;
                }
            }
        }
    }

    /// Whether an underlay shows `op` over our own rendering.
    fn underlay_keeps(&self, canvas: u32, op: &Op) -> bool {
        match op {
            Op::Fill { rect, .. } => !self.covers(canvas, *rect),
            Op::Image { source: Source::File { path, .. }, .. } => {
                let p = path.to_ascii_lowercase();
                !self.underlay_art.iter().any(|a| p.contains(a.as_str()))
            }
            _ => true,
        }
    }

    fn push(&mut self, canvas: u32, op: Op) {
        if self.generated.contains(&canvas) {
            return;
        }
        if canvas == self.screen && canvas != 0 {
            let ops = self.resolve(&op, 0);
            if let Some(frame) = self.painting.as_mut() {
                frame.draws.extend(ops.into_iter().filter_map(|op| screen_draw(op, (0, 0), [i32::MIN, i32::MIN, i32::MAX, i32::MAX])));
            }
            return;
        }
        let ops = self.canvases.entry(canvas).or_default();
        if ops.len() >= MAX_OPS {
            ops.drain(..MAX_OPS / 2);
        }
        ops.push(op);
    }

    /// Whether `rect` (x, y, w, h) covers all of `canvas` (when its size is
    /// known).
    fn covers(&self, canvas: u32, [x, y, w, h]: [i32; 4]) -> bool {
        self.sizes.get(&canvas).is_some_and(|&(cw, ch)| x <= 0 && y <= 0 && x + w >= cw && y + h >= ch)
    }

    /// `op` with references to other surfaces replaced by their content
    /// (clipped to the copied region, moved to where it was copied to).
    fn resolve(&self, op: &Op, depth: u32) -> Vec<Op> {
        let Op::Image { source: Source::Surface { image, rect: [sx, sy, sw, sh] }, x, y, keyed } = op else {
            return vec![op.clone()];
        };
        if self.generated.contains(image) {
            return vec![op.clone()];
        }
        if depth >= MAX_DEPTH {
            return Vec::new();
        }
        let Some(ops) = self.canvases.get(image) else { return Vec::new() };
        let (dx, dy) = (x - sx, y - sy);
        let window = [*sx, *sy, sx + sw, sy + sh];
        let mut out = Vec::new();
        for inner in ops {
            for inner in self.resolve(inner, depth + 1) {
                if let Some(moved) = crop_move(inner, window, (dx, dy), *keyed) {
                    out.push(moved);
                }
            }
        }
        out
    }
}

/// `op` limited to `window` (left, top, right, bottom, in its canvas) and
/// moved by `d`. Images are cut to the window; text is kept if it starts
/// inside.
fn crop_move(op: Op, window: [i32; 4], (dx, dy): (i32, i32), keyed: bool) -> Option<Op> {
    match op {
        Op::Image { source, x, y, keyed: k } => {
            let (path_or_image, [rx, ry, rw, rh]) = match &source {
                Source::File { rect, .. } | Source::Surface { rect, .. } => (source.clone(), *rect),
            };
            let [l, t, r, b] = intersect(window, [x, y, x + rw, y + rh]);
            if r <= l || b <= t {
                return None;
            }
            let rect = [rx + (l - x), ry + (t - y), r - l, b - t];
            let source = match path_or_image {
                Source::File { path, .. } => Source::File { path, rect },
                Source::Surface { image, .. } => Source::Surface { image, rect },
            };
            // Through a keyed copy, transparent pixels stay transparent.
            Some(Op::Image { source, x: l + dx, y: t + dy, keyed: k || keyed })
        }
        Op::Fill { rect: [x, y, w, h], rgb, alpha } => {
            let [l, t, r, b] = intersect(window, [x, y, x + w, y + h]);
            (r > l && b > t).then(|| Op::Fill { rect: [l + dx, t + dy, r - l, b - t], rgb, alpha })
        }
        Op::Text(mut run) => {
            let inside = run.x >= window[0] && run.x < window[2] && run.y >= window[1] && run.y < window[3];
            run.x += dx;
            run.y += dy;
            inside.then_some(Op::Text(run))
        }
    }
}

fn intersect(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]
}

/// A resolved canvas operation on the screen.
fn screen_draw(op: Op, (ox, oy): (i32, i32), clip: [i32; 4]) -> Option<ScreenDraw> {
    match op {
        Op::Image { source: Source::File { path, rect }, x, y, keyed } => {
            Some(ScreenDraw::Image { path, rect, x: x + ox, y: y + oy, keyed, clip })
        }
        Op::Image { source: Source::Surface { image, rect }, x, y, .. } => Some(ScreenDraw::Surface { image, rect, x: x + ox, y: y + oy, clip }),
        Op::Fill { rect: [x, y, w, h], rgb, alpha } => Some(ScreenDraw::Fill { rect: [x + ox, y + oy, w, h], rgb, alpha, clip }),
        Op::Text(mut run) => {
            run.x += ox;
            run.y += oy;
            Some(ScreenDraw::Text { run, clip })
        }
    }
}

/// A 16-bit colour of the game's canvases (RGB555; bit 31 is a flag) as
/// 8-bit RGB.
pub fn rgb555(c: u32) -> [u8; 3] {
    let e = |v: u32| ((v << 3) | (v >> 2)) as u8;
    [e((c >> 10) & 31), e((c >> 5) & 31), e(c & 31)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composite(canvas: u32, origin: (i32, i32), size: (i32, i32)) -> Event {
        Event::Composite { canvas, vtable: 1, origin, size, clip: [0, 0, 1024, 768], keyed: false }
    }

    #[test]
    fn blits_reach_the_screen_through_their_form() {
        let mut m = UiModel::new(vec![], vec![]);
        m.apply(Event::Loaded { image: 10, path: "Art/a.pcx".into(), size: (100, 50) });
        m.apply(Event::Sliced { sprite: 20, image: 10, rect: [5, 6, 30, 20] });
        m.apply(Event::Blit { sprite: 20, canvas: 300, x: 7, y: 8 });
        m.apply(Event::Cleared { image: 10 });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (100, 200), (40, 40)));
        m.apply(Event::PaintEnd);
        assert_eq!(
            m.frame().draws,
            vec![ScreenDraw::Image { path: "Art/a.pcx".into(), rect: [5, 6, 30, 20], x: 107, y: 208, keyed: true, clip: [100, 200, 140, 240] }]
        );
    }

    #[test]
    fn copies_between_surfaces_carry_their_content() {
        let mut m = UiModel::new(vec![], vec![]);
        m.apply(Event::Loaded { image: 10, path: "Art/bg.pcx".into(), size: (64, 64) });
        // A temporary image composed from the file, then copied (a 10x10
        // region of it) to a form's canvas.
        m.apply(Event::Copy { from: 10, src: [0, 0, 64, 64], canvas: 50, x: 0, y: 0, keyed: false });
        m.apply(Event::Fill { canvas: 50, rect: [20, 20, 4, 4], rgb: [1, 2, 3], alpha: 255 });
        m.apply(Event::Copy { from: 50, src: [18, 18, 10, 10], canvas: 300, x: 2, y: 3, keyed: false });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (0, 0), (100, 100)));
        m.apply(Event::PaintEnd);
        let clip = [0, 0, 100, 100];
        assert_eq!(
            m.frame().draws,
            vec![
                ScreenDraw::Image { path: "Art/bg.pcx".into(), rect: [18, 18, 10, 10], x: 2, y: 3, keyed: false, clip },
                ScreenDraw::Fill { rect: [4, 5, 4, 4], rgb: [1, 2, 3], alpha: 255, clip },
            ]
        );
    }

    #[test]
    fn full_covers_reset_a_canvas_and_skipped_forms_stay_out() {
        let mut m = UiModel::new(vec![9], vec![]);
        m.apply(Event::Size { image: 300, size: (10, 10) });
        m.apply(Event::Fill { canvas: 300, rect: [1, 1, 2, 2], rgb: [0, 0, 0], alpha: 255 });
        m.apply(Event::Fill { canvas: 300, rect: [0, 0, 10, 10], rgb: [9, 9, 9], alpha: 255 });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (0, 0), (10, 10)));
        m.apply(Event::Composite { canvas: 300, vtable: 9, origin: (0, 0), size: (10, 10), clip: [0, 0, 10, 10], keyed: false });
        m.apply(Event::PaintEnd);
        assert_eq!(m.frame().draws, vec![ScreenDraw::Fill { rect: [0, 0, 10, 10], rgb: [9, 9, 9], alpha: 255, clip: [0, 0, 10, 10] }]);
    }

    #[test]
    fn underlays_keep_labels_and_drop_our_art() {
        let mut m = UiModel::new(vec![], vec![77]).with_underlays(vec![300], vec!["fogofwar.pcx".into()]);
        m.apply(Event::Size { image: 300, size: (100, 100) });
        m.apply(Event::Loaded { image: 10, path: "Art\\Terrain\\FogOfWar.pcx".into(), size: (64, 64) });
        m.apply(Event::Loaded { image: 11, path: "Art\\Cities\\barracks.pcx".into(), size: (64, 64) });
        m.apply(Event::Sliced { sprite: 20, image: 10, rect: [0, 0, 8, 8] });
        m.apply(Event::Sliced { sprite: 21, image: 11, rect: [1, 1, 23, 15] });
        m.apply(Event::Fill { canvas: 300, rect: [0, 0, 100, 100], rgb: [0, 0, 0], alpha: 255 });
        m.apply(Event::Fill { canvas: 300, rect: [5, 5, 20, 10], rgb: [0, 0, 0], alpha: 128 });
        m.apply(Event::Blit { sprite: 21, canvas: 300, x: 1, y: 2 });
        m.apply(Event::Blit { sprite: 20, canvas: 300, x: 3, y: 4 });
        m.apply(Event::Fill { canvas: 77, rect: [0, 0, 1, 1], rgb: [9, 9, 9], alpha: 255 });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (0, 0), (100, 100)));
        m.apply(composite(77, (0, 0), (100, 100)));
        m.apply(Event::PaintEnd);
        let clip = [0, 0, 100, 100];
        assert_eq!(
            m.frame().draws,
            vec![
                ScreenDraw::Fill { rect: [5, 5, 20, 10], rgb: [0, 0, 0], alpha: 128, clip },
                ScreenDraw::Image { path: "Art\\Cities\\barracks.pcx".into(), rect: [1, 1, 23, 15], x: 1, y: 2, keyed: true, clip },
            ]
        );
    }
}
