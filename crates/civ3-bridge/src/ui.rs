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
    /// A region of an image file (path as the game opened it), blended
    /// through `mask` if it has one.
    File { path: String, rect: [i32; 4], mask: Option<Mask> },
    /// A region of another surface (a canvas or temporary image).
    Surface { image: u32, rect: [i32; 4] },
}

/// An alpha mask: a file whose palette indices are inverted weights, and
/// the pixel of it that goes with the image region's top-left.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mask {
    pub path: String,
    pub at: (i32, i32),
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
    /// `sprite` blended onto `canvas` at (x, y) through the alpha sprite
    /// `mask` (button faces, the HUD frame).
    MaskedBlit { sprite: u32, mask: u32, canvas: u32, x: i32, y: i32, clip: Option<[i32; 4]> },
    /// `sprite`'s pixels drawn with their top-left at (x, y) of `canvas`,
    /// colour 255 transparent.
    Blit { sprite: u32, canvas: u32, x: i32, y: i32, clip: Option<[i32; 4]> },
    /// Region `src` (x, y, w, h) of image `from` copied to (x, y) of
    /// `canvas`; `keyed`: colour 255 transparent.
    Copy { from: u32, src: [i32; 4], canvas: u32, x: i32, y: i32, keyed: bool },
    /// `rect` (x, y, w, h) of `canvas` filled (`rgb`).
    /// `alpha`: 255 opaque, else a translucent shade (50 % = 128).
    Fill { canvas: u32, rect: [i32; 4], rgb: [u8; 3], alpha: u8, clip: Option<[i32; 4]> },
    /// Text on `canvas` (its position canvas-relative).
    Text { canvas: u32, run: TextRun },
    /// `flic` (an FLC animation) was loaded from `path`.
    FlcLoaded { flic: u32, path: String },
    /// Frame `chunk` of animation `flic` drawn with its frame's top-left at
    /// (x, y) of `canvas` (unit pictures in panels).
    FlcBlit { flic: u32, chunk: u32, canvas: u32, x: i32, y: i32, clip: Option<[i32; 4]> },
    /// A paint starts: the forms that follow are composed bottom to top.
    PaintBegin,
    /// A form's canvas put on the screen with its (0, 0) at `origin`, clipped
    /// to `clip` (left, top, right, bottom).
    Composite { canvas: u32, vtable: u32, origin: (i32, i32), size: (i32, i32), clip: [i32; 4], keyed: bool },
    /// The composition of that form is done (what it drew onto the screen
    /// meanwhile is the form itself).
    CompositeEnd,
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
    /// Frame `chunk` of the FLC animation at `path`, its top-left at (x, y).
    Flc { path: String, chunk: u32, x: i32, y: i32, clip: Option<[i32; 4]> },
}

/// One thing on the screen.
#[derive(Clone, Debug, PartialEq)]
pub enum ScreenDraw {
    /// Region `rect` (x, y, w, h) of an image file at (x, y).
    Image { path: String, rect: [i32; 4], x: i32, y: i32, keyed: bool, clip: [i32; 4], mask: Option<Mask> },
    Fill { rect: [i32; 4], rgb: [u8; 3], alpha: u8, clip: [i32; 4] },
    Text { run: TextRun, clip: [i32; 4] },
    /// Frame `chunk` of the FLC animation at `path`, its top-left at (x, y).
    Flc { path: String, chunk: u32, x: i32, y: i32, clip: [i32; 4] },
    /// Region `rect` (x, y, w, h) of a surface the game generates itself
    /// (e.g. the minimap), at (x, y).
    Surface { image: u32, rect: [i32; 4], x: i32, y: i32, clip: [i32; 4], size: (i32, i32) },
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
    /// The draws of underlays (the map canvas: drawn under the fog of war).
    pub underlay_draws: Vec<std::ops::Range<usize>>,
}

/// The game's colour key as 8-bit RGB (0x7C1F).
const KEY_RGB: [u8; 3] = [255, 0, 255];

/// Longest display list a canvas keeps (older operations are dropped).
const MAX_OPS: usize = 8192;
/// How deep copies between canvases are followed.
const MAX_DEPTH: u32 = 8;


/// One layer of the composed screen.
#[derive(Clone, Debug)]
enum Layer {
    /// A form's canvas put on the screen with its (0, 0) at `origin`.
    Form { canvas: u32, vtable: u32, origin: (i32, i32), size: (i32, i32), clip: [i32; 4], keyed: bool },
    /// Drawn straight onto the screen (frames, the tooltip, the cursor).
    Screen(Vec<Op>),
}
/// The mirrored UI state.
#[derive(Default)]
pub struct UiModel {
    /// Files of loaded images.
    files: HashMap<u32, String>,
    /// Files of loaded FLC animations.
    flcs: HashMap<u32, String>,
    /// Sizes of images and canvases.
    sizes: HashMap<u32, (i32, i32)>,
    /// Where sprites come from.
    sprites: HashMap<u32, Source>,
    /// Display lists of canvases (and every other drawn-into image).
    canvases: HashMap<u32, Vec<Op>>,
    /// What each form's canvas showed when the game last put it on the
    /// screen (each operation with what it resolves to): the frame is made
    /// of these, never of a canvas the game is still painting.
    presented: HashMap<u32, Vec<(Op, Vec<Op>)>>,
    /// What the screen is composed of, bottom to top, as the last paint
    /// composed it (forms later composed on their own update their place).
    order: Vec<Layer>,
    /// The paint in progress.
    pending: Option<Vec<Layer>>,
    /// Inside a form's composition.
    compositing: bool,
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


    /// Forgets what `image` shows.
    pub fn reset(&mut self, image: u32) {
        self.canvases.remove(&image);
    }

    /// Forgets what the underlays show: the game redraws the map under them
    /// (painting over what they showed, unseen by the mirror).
    pub fn reset_underlays(&mut self) {
        for c in &self.underlays {
            self.canvases.remove(c);
        }
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

    /// The screen now: the forms of the last paint with their canvases'
    /// current content (the game composes changed forms on their own,
    /// outside a full paint).
    pub fn frame(&self) -> Frame {
        let mut frame = Frame::default();
        for layer in &self.order {
            match layer {
                Layer::Screen(ops) => {
                    let all = [i32::MIN, i32::MIN, i32::MAX, i32::MAX];
                    frame.draws.extend(ops.iter().cloned().filter_map(|op| screen_draw(op, (0, 0), all)));
                }
                &Layer::Form { canvas, vtable, origin, size, clip, keyed } => self.compose(&mut frame, canvas, vtable, origin, size, clip, keyed),
            }
        }
        frame
    }

    /// Adds form `canvas` as composed at `origin` to `frame`.
    #[allow(clippy::too_many_arguments)]
    fn compose(&self, frame: &mut Frame, canvas: u32, vtable: u32, origin: (i32, i32), size: (i32, i32), clip: [i32; 4], keyed: bool) {
        frame.forms.push((vtable, canvas, origin, size));
        if self.skip.contains(&vtable) || (self.skipping && self.skip_canvases.contains(&canvas)) {
            return;
        }
        // The canvas never extends beyond its size.
        let clip = intersect(clip, [origin.0, origin.1, origin.0 + size.0, origin.1 + size.1]);
        if clip[2] <= clip[0] || clip[3] <= clip[1] {
            return;
        }
        let underlay = self.skipping && self.underlays.contains(&canvas);
        // A colour-keyed form shows nothing where it is black (measured: the
        // city screen clears to black and the map shows through its art).
        let black = |op: &&Op| keyed && matches!(op, Op::Fill { rgb: [0, 0, 0], alpha: 255, .. });
        let keep = |op: &&Op| (!underlay || self.underlay_keeps(canvas, op)) && !black(op);
        let Some(ops) = self.presented.get(&canvas) else { return };
        let start = frame.draws.len();
        frame.draws.extend(ops.iter().filter(|(op, _)| keep(&op)).flat_map(|(_, resolved)| resolved.iter().cloned()).filter_map(|op| screen_draw(op, origin, clip)).map(|mut d| {
            // Generated surfaces: their whole size, as far as known.
            if let ScreenDraw::Surface { image, size, .. } = &mut d {
                if let Some(&s) = self.sizes.get(image) {
                    *size = s;
                }
            }
            d
        }));
        frame.form_draws.push((canvas, start..frame.draws.len()));
        if underlay {
            frame.underlay_draws.push(start..frame.draws.len());
        }
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
                    Some(path) => Source::File { path: path.clone(), rect, mask: None },
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
            Event::Blit { sprite, canvas, x, y, clip } => {
                if let Some(source) = self.sprites.get(&sprite).cloned() {
                    // Within the canvas's clip rectangle, if it has one.
                    let op = Op::Image { source, x, y, keyed: true };
                    if let Some(op) = clip.map_or(Some(op.clone()), |c| crop_move(op, c, (0, 0), true)) {
                        self.push(canvas, op);
                    }
                }
            }
            Event::MaskedBlit { sprite, mask, canvas, x, y, clip } => {
                let face = self.sprites.get(&sprite).cloned();
                let alpha = match self.sprites.get(&mask) {
                    Some(Source::File { path, rect, .. }) => Some(Mask { path: path.clone(), at: (rect[0], rect[1]) }),
                    _ => None,
                };
                let source = match face {
                    Some(Source::File { path, rect, .. }) => Source::File { path, rect, mask: alpha },
                    Some(other) => other,
                    None => return,
                };
                let op = Op::Image { source, x, y, keyed: true };
                if let Some(op) = clip.map_or(Some(op.clone()), |c| crop_move(op, c, (0, 0), true)) {
                    self.push(canvas, op);
                }
            }
            Event::Copy { from, src, canvas, x, y, keyed } => {
                // The map canvas is ours: its content (terrain, the fog tiles drawn on it)
                // must not reach the screen through the form's own copy onto the back buffer.
                if from != canvas && self.skipping && (self.skip_canvases.contains(&from) || self.underlays.contains(&from)) {
                    return;
                }
                // A copy within a canvas (scrolling): its content moves, over
                // what was there.
                if from == canvas {
                    let window = [src[0], src[1], src[0] + src[2], src[1] + src[3]];
                    let d = (x - src[0], y - src[1]);
                    let ops = self.canvases.get(&canvas).cloned().unwrap_or_default();
                    let moved: Vec<Op> = ops.into_iter().filter_map(|op| crop_move(op, window, d, keyed)).collect();
                    if !keyed {
                        self.occlude(canvas, [x, y, src[2], src[3]]);
                    }
                    for op in moved {
                        self.push(canvas, op);
                    }
                    return;
                }
                let source = match self.files.get(&from) {
                    Some(path) => Source::File { path: path.clone(), rect: src, mask: None },
                    None => Source::Surface { image: from, rect: src },
                };
                // An opaque copy hides what it covers.
                if !keyed {
                    self.occlude(canvas, [x, y, src[2], src[3]]);
                }
                let op = Op::Image { source, x, y, keyed };
                // Copies of surfaces take their content as it is now.
                let ops = self.resolve(&op, 0);
                for op in ops {
                    self.push(canvas, op);
                }
            }
            Event::Fill { canvas, rect, rgb, alpha, clip } => {
                let rect = match clip {
                    Some(c) => {
                        let [l, t, r, b] = intersect(c, [rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]]);
                        if r <= l || b <= t {
                            return;
                        }
                        [l, t, r - l, b - t]
                    }
                    None => rect,
                };
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
                            Op::Flc { x, y, .. } => !inside(*x, *y, x + 1, y + 1),
                            Op::Image { .. } => true,
                        });
                    }
                    return;
                }
                if alpha == 255 {
                    self.occlude(canvas, rect);
                }
                self.push(canvas, Op::Fill { rect, rgb, alpha });
            }
            Event::Text { canvas, run } => self.push(canvas, Op::Text(run)),
            Event::FlcLoaded { flic, path } => {
                self.flcs.insert(flic, path);
            }
            Event::FlcBlit { flic, chunk, canvas, x, y, clip } => {
                if let Some(path) = self.flcs.get(&flic).cloned() {
                    self.push(canvas, Op::Flc { path, chunk, x, y, clip });
                }
            }
            Event::PaintBegin => self.pending = Some(Vec::new()),
            Event::Composite { canvas, vtable, origin, size, clip, keyed } => {
                self.sizes.insert(canvas, size);
                let shown = self.canvases.get(&canvas).map(|ops| ops.iter().map(|op| (op.clone(), self.resolve(op, 0))).collect()).unwrap_or_default();
                self.presented.insert(canvas, shown);
                self.compositing = true;
                let layer = Layer::Form { canvas, vtable, origin, size, clip, keyed };
                match self.pending.as_mut() {
                    Some(layers) => layers.push(layer),
                    // A form updated on its own: its place may have changed.
                    None => {
                        if let Some(l) = self.order.iter_mut().find(|l| matches!(l, Layer::Form { canvas: c, .. } if *c == canvas)) {
                            *l = layer;
                        }
                    }
                }
            }
            Event::CompositeEnd => self.compositing = false,
            Event::PaintEnd => {
                if let Some(layers) = self.pending.take() {
                    self.order = layers;
                }
            }
        }
    }

    /// Drops what an opaque `rect` (x, y, w, h) on `canvas` hides: all of it
    /// when it covers the canvas, else the operations entirely inside.
    fn occlude(&mut self, canvas: u32, rect: [i32; 4]) {
        if self.covers(canvas, rect) {
            self.canvases.remove(&canvas);
            return;
        }
        let [x, y, w, h] = rect;
        let inside = |[l, t, r, b]: [i32; 4]| l >= x && t >= y && r <= x + w && b <= y + h;
        if let Some(ops) = self.canvases.get_mut(&canvas) {
            ops.retain(|op| !inside(op_bounds(op)));
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
            // What a form's composition puts on the screen is that form.
            if self.compositing {
                return;
            }
            // Straight onto the screen: part of the paint in progress.
            let ops = self.resolve(&op, 0);
            if let Some(layers) = self.pending.as_mut() {
                match layers.last_mut() {
                    Some(Layer::Screen(screen)) => screen.extend(ops),
                    _ => layers.push(Layer::Screen(ops)),
                }
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
                Source::File { path, mask, .. } => {
                    // The mask moves along with the cut.
                    let mask = mask.map(|m| Mask { at: (m.at.0 + (l - x), m.at.1 + (t - y)), ..m });
                    Source::File { path, rect, mask }
                }
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
        Op::Flc { path, chunk, x, y, clip } => {
            let inside = x >= window[0] && x < window[2] && y >= window[1] && y < window[3];
            let clip = intersect(clip.unwrap_or(window), window);
            let clip = Some([clip[0] + dx, clip[1] + dy, clip[2] + dx, clip[3] + dy]);
            inside.then_some(Op::Flc { path, chunk, x: x + dx, y: y + dy, clip })
        }
    }
}


/// The pixels of a line of the game's line drawer, as rectangles (x, y, w,
/// h). Measured (§10.9): along the major axis every step from the start up
/// to, not including, the larger end gets `width` pixels from
/// `j·minor/major` (rounded down, `j` counted from the start) on, towards
/// the end.
pub fn line_rects((x1, y1): (i32, i32), (x2, y2): (i32, i32), width: i32) -> Vec<[i32; 4]> {
    let (dx, dy) = (x2 - x1, y2 - y1);
    let w = width.max(1);
    if dy == 0 {
        return if dx == 0 { Vec::new() } else { vec![[x1.min(x2), y1, dx.abs(), w]] };
    }
    if dx == 0 {
        return vec![[x1, y1.min(y2), w, dy.abs()]];
    }
    let mut out = Vec::new();
    let x_major = dx.abs() >= dy.abs();
    let (major, minor) = if x_major { (dx, dy) } else { (dy, dx) };
    for j in 0..major.abs() {
        let along = if major < 0 { -1 - j } else { j };
        let off = j * minor.abs() / major.abs();
        let across = if minor < 0 { -off - (w - 1) } else { off };
        out.push(if x_major { [x1 + along, y1 + across, 1, w] } else { [x1 + across, y1 + along, w, 1] });
    }
    out
}

/// Where an operation draws (left, top, right, bottom); text by its start
/// point only.
fn op_bounds(op: &Op) -> [i32; 4] {
    match op {
        Op::Image { source: Source::File { rect, .. } | Source::Surface { rect, .. }, x, y, .. } => [*x, *y, x + rect[2], y + rect[3]],
        Op::Fill { rect: [x, y, w, h], .. } => [*x, *y, x + w, y + h],
        Op::Text(run) => [run.x, run.y, run.x + 1, run.y + 1],
        Op::Flc { x, y, .. } => [*x, *y, x + 1, y + 1],
    }
}
fn intersect(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]
}

/// A resolved canvas operation on the screen.
fn screen_draw(op: Op, (ox, oy): (i32, i32), clip: [i32; 4]) -> Option<ScreenDraw> {
    match op {
        Op::Image { source: Source::File { path, rect, mask }, x, y, keyed } => {
            Some(ScreenDraw::Image { path, rect, x: x + ox, y: y + oy, keyed, clip, mask })
        }
        Op::Image { source: Source::Surface { image, rect }, x, y, .. } => {
            Some(ScreenDraw::Surface { image, rect, x: x + ox, y: y + oy, clip, size: (rect[0] + rect[2], rect[1] + rect[3]) })
        }
        Op::Fill { rect: [x, y, w, h], rgb, alpha } => Some(ScreenDraw::Fill { rect: [x + ox, y + oy, w, h], rgb, alpha, clip }),
        Op::Text(mut run) => {
            run.x += ox;
            run.y += oy;
            Some(ScreenDraw::Text { run, clip })
        }
        Op::Flc { path, chunk, x, y, clip: own } => {
            let clip = own.map_or(clip, |c| intersect(clip, [c[0] + ox, c[1] + oy, c[2] + ox, c[3] + oy]));
            Some(ScreenDraw::Flc { path, chunk, x: x + ox, y: y + oy, clip })
        }
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
        m.apply(Event::Blit { sprite: 20, canvas: 300, x: 7, y: 8, clip: None });
        m.apply(Event::Cleared { image: 10 });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (100, 200), (40, 40)));
        m.apply(Event::PaintEnd);
        assert_eq!(
            m.frame().draws,
            vec![ScreenDraw::Image { path: "Art/a.pcx".into(), rect: [5, 6, 30, 20], x: 107, y: 208, keyed: true, clip: [100, 200, 140, 240], mask: None }]
        );
    }

    #[test]
    fn a_canvas_being_repainted_shows_what_it_showed() {
        // The game clears a form's canvas and draws it again before it
        // puts it on the screen: until then the screen shows the old one.
        let mut m = UiModel::new(vec![], vec![]);
        m.apply(Event::Size { image: 300, size: (10, 10) });
        m.apply(Event::Fill { canvas: 300, rect: [0, 0, 10, 10], rgb: [1, 1, 1], alpha: 255, clip: None });
        m.apply(Event::Fill { canvas: 300, rect: [2, 2, 3, 3], rgb: [7, 7, 7], alpha: 255, clip: None });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (0, 0), (10, 10)));
        m.apply(Event::PaintEnd);
        let before = m.frame().draws;
        assert_eq!(before.len(), 2);
        m.apply(Event::Fill { canvas: 300, rect: [0, 0, 10, 10], rgb: [1, 1, 1], alpha: 255, clip: None });
        assert_eq!(m.frame().draws, before);
        m.apply(Event::Fill { canvas: 300, rect: [4, 4, 3, 3], rgb: [8, 8, 8], alpha: 255, clip: None });
        m.apply(composite(300, (0, 0), (10, 10)));
        assert_eq!(m.frame().draws.last(), Some(&ScreenDraw::Fill { rect: [4, 4, 3, 3], rgb: [8, 8, 8], alpha: 255, clip: [0, 0, 10, 10] }));
    }

    #[test]
    fn copies_between_surfaces_carry_their_content() {
        let mut m = UiModel::new(vec![], vec![]);
        m.apply(Event::Loaded { image: 10, path: "Art/bg.pcx".into(), size: (64, 64) });
        // A temporary image composed from the file, then copied (a 10x10
        // region of it) to a form's canvas.
        m.apply(Event::Copy { from: 10, src: [0, 0, 64, 64], canvas: 50, x: 0, y: 0, keyed: false });
        m.apply(Event::Fill { canvas: 50, rect: [20, 20, 4, 4], rgb: [1, 2, 3], alpha: 255, clip: None });
        m.apply(Event::Copy { from: 50, src: [18, 18, 10, 10], canvas: 300, x: 2, y: 3, keyed: false });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (0, 0), (100, 100)));
        m.apply(Event::PaintEnd);
        let clip = [0, 0, 100, 100];
        assert_eq!(
            m.frame().draws,
            vec![
                ScreenDraw::Image { path: "Art/bg.pcx".into(), rect: [18, 18, 10, 10], x: 2, y: 3, keyed: false, clip, mask: None },
                ScreenDraw::Fill { rect: [4, 5, 4, 4], rgb: [1, 2, 3], alpha: 255, clip },
            ]
        );
    }

    #[test]
    fn full_covers_reset_a_canvas_and_skipped_forms_stay_out() {
        let mut m = UiModel::new(vec![9], vec![]);
        m.apply(Event::Size { image: 300, size: (10, 10) });
        m.apply(Event::Fill { canvas: 300, rect: [1, 1, 2, 2], rgb: [0, 0, 0], alpha: 255, clip: None });
        m.apply(Event::Fill { canvas: 300, rect: [0, 0, 10, 10], rgb: [9, 9, 9], alpha: 255, clip: None });
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
        m.apply(Event::Fill { canvas: 300, rect: [0, 0, 100, 100], rgb: [0, 0, 0], alpha: 255, clip: None });
        m.apply(Event::Fill { canvas: 300, rect: [5, 5, 20, 10], rgb: [0, 0, 0], alpha: 128, clip: None });
        m.apply(Event::Blit { sprite: 21, canvas: 300, x: 1, y: 2, clip: None });
        m.apply(Event::Blit { sprite: 20, canvas: 300, x: 3, y: 4, clip: None });
        m.apply(Event::Fill { canvas: 77, rect: [0, 0, 1, 1], rgb: [9, 9, 9], alpha: 255, clip: None });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (0, 0), (100, 100)));
        m.apply(composite(77, (0, 0), (100, 100)));
        m.apply(Event::PaintEnd);
        let clip = [0, 0, 100, 100];
        assert_eq!(
            m.frame().draws,
            vec![
                ScreenDraw::Fill { rect: [5, 5, 20, 10], rgb: [0, 0, 0], alpha: 128, clip },
                ScreenDraw::Image { path: "Art\\Cities\\barracks.pcx".into(), rect: [1, 1, 23, 15], x: 1, y: 2, keyed: true, clip, mask: None },
            ]
        );
    }

    #[test]
    fn lines_follow_the_measured_rule() {
        // A tile edge as the settler preview draws it (width 2): from
        // (64, 0) to (0, 32), rows j/2 and j/2 + 1 from the start, x 0..=63.
        let rects = line_rects((64, 0), (0, 32), 2);
        assert_eq!(rects.len(), 64);
        assert_eq!(rects[0], [63, 0, 1, 2]);
        assert_eq!(rects[63], [0, 31, 1, 2]);
        // Going up: the 2 pixels reach towards the end.
        assert_eq!(line_rects((0, 0), (64, -32), 2)[2], [2, -2, 1, 2]);
        assert_eq!(line_rects((5, 7), (9, 7), 1), vec![[5, 7, 4, 1]]);
        assert_eq!(line_rects((5, 9), (5, 7), 1), vec![[5, 7, 1, 2]]);
    }

    #[test]
    fn opaque_copies_hide_what_they_cover() {
        // A label drawn on an image, copied (opaque) to a canvas; then the
        // label moves: the next copy hides the old one.
        let mut m = UiModel::new(vec![], vec![]);
        m.apply(Event::Size { image: 300, size: (100, 100) });
        let label = |m: &mut UiModel, y| {
            m.apply(Event::Fill { canvas: 50, rect: [0, 0, 100, 100], rgb: [0, 0, 0], alpha: 255, clip: None });
            m.apply(Event::Fill { canvas: 50, rect: [10, y, 20, 5], rgb: [1, 1, 1], alpha: 128, clip: None });
            m.apply(Event::Copy { from: 50, src: [0, 0, 100, 60], canvas: 300, x: 0, y: 0, keyed: false });
        };
        label(&mut m, 30);
        label(&mut m, 40);
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (0, 0), (100, 100)));
        m.apply(Event::PaintEnd);
        let frame = m.frame();
        let shades: Vec<_> = frame.draws.iter().filter(|d| matches!(d, ScreenDraw::Fill { alpha: 128, .. })).collect();
        assert_eq!(shades, vec![&ScreenDraw::Fill { rect: [10, 40, 20, 5], rgb: [1, 1, 1], alpha: 128, clip: [0, 0, 100, 100] }]);
    }

    #[test]
    fn scrolling_moves_what_a_canvas_shows() {
        // A label at y 50; the game scrolls the canvas up by 20 (copying it
        // onto itself) and draws the new strip at the bottom.
        let mut m = UiModel::new(vec![], vec![]);
        m.apply(Event::Size { image: 300, size: (100, 100) });
        m.apply(Event::Fill { canvas: 300, rect: [10, 50, 30, 5], rgb: [1, 1, 1], alpha: 128, clip: None });
        m.apply(Event::Copy { from: 300, src: [0, 20, 100, 80], canvas: 300, x: 0, y: 0, keyed: false });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (0, 0), (100, 100)));
        m.apply(Event::PaintEnd);
        let clip = [0, 0, 100, 100];
        assert_eq!(m.frame().draws, vec![ScreenDraw::Fill { rect: [10, 30, 30, 5], rgb: [1, 1, 1], alpha: 128, clip }]);
    }

    #[test]
    fn keyed_forms_show_through_black_and_their_screen_copies_are_theirs() {
        let mut m = UiModel::new(vec![], vec![]).with_screen(999);
        m.apply(Event::Size { image: 300, size: (100, 100) });
        m.apply(Event::Fill { canvas: 300, rect: [0, 0, 100, 100], rgb: [0, 0, 0], alpha: 255, clip: None });
        m.apply(Event::Fill { canvas: 300, rect: [5, 5, 5, 5], rgb: [7, 7, 7], alpha: 255, clip: Some([0, 0, 8, 8]) });
        m.apply(Event::PaintBegin);
        m.apply(Event::Composite { canvas: 300, vtable: 1, origin: (0, 0), size: (100, 100), clip: [0, 0, 100, 100], keyed: true });
        // What the composition itself puts on the screen.
        m.apply(Event::Fill { canvas: 999, rect: [0, 0, 100, 100], rgb: [0, 0, 0], alpha: 255, clip: None });
        m.apply(Event::CompositeEnd);
        m.apply(Event::PaintEnd);
        assert_eq!(m.frame().draws, vec![ScreenDraw::Fill { rect: [5, 5, 3, 3], rgb: [7, 7, 7], alpha: 255, clip: [0, 0, 100, 100] }]);
    }
}
