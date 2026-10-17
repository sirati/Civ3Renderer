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
use std::sync::Arc;

use crate::text::TextRun;

/// Where a sprite's pixels come from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Source {
    /// A region of an image file (path as the game opened it), blended
    /// through `mask` if it has one.
    /// `palette`: an image file whose palette replaces the image's own (a
    /// colour table the game drew it through).
    File { path: String, rect: [i32; 4], mask: Option<Mask>, palette: Option<menu::screen::PaletteFrom>, opacity: u8 },
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
    /// `palette`: see [`Source::File`].
    Blit { sprite: u32, canvas: u32, x: i32, y: i32, clip: Option<[i32; 4]>, palette: Option<menu::screen::PaletteFrom> },
    /// A colour-keyed file sprite drawn with a uniform opacity (0..255).
    OpacityBlit { sprite: u32, canvas: u32, x: i32, y: i32, clip: Option<[i32; 4]>, palette: Option<menu::screen::PaletteFrom>, opacity: u8 },
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
    /// `plain`: in the file's own colours, no shadow (leader heads), else
    /// a unit picture.
    FlcBlit { flic: u32, chunk: u32, canvas: u32, x: i32, y: i32, clip: Option<[i32; 4]>, plain: bool },
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
    /// A present starts: what it draws onto the back buffer (the tooltip;
    /// the game's mouse cursor too, which the tracer leaves out) lies over
    /// everything composed and is drawn anew by every present (`present`,
    /// 0x606680; docs §16.5).
    PresentBegin,
    /// The back buffer's `rect` (left, top, right, bottom; all of it if
    /// none) copied to the window: from now on the screen shows it there.
    Present { rect: Option<[i32; 4]> },
}

/// What a canvas shows: drawing operations, oldest first.
#[derive(Clone, Debug, PartialEq)]
enum Op {
    /// An image region at (x, y); `keyed`: colour 255 transparent.
    Image { source: Source, x: i32, y: i32, keyed: bool },
    Fill { rect: [i32; 4], rgb: [u8; 3], alpha: u8 },
    Text(TextRun),
    /// Frame `chunk` of the FLC animation at `path`, its top-left at (x, y).
    Flc { path: String, chunk: u32, x: i32, y: i32, clip: Option<[i32; 4]>, plain: bool },
}

/// One thing on the screen.
#[derive(Clone, Debug, PartialEq)]
pub enum ScreenDraw {
    /// Region `rect` (x, y, w, h) of an image file at (x, y).
    Image { path: String, rect: [i32; 4], x: i32, y: i32, keyed: bool, clip: [i32; 4], mask: Option<Mask>, palette: Option<menu::screen::PaletteFrom>, opacity: u8 },
    Fill { rect: [i32; 4], rgb: [u8; 3], alpha: u8, clip: [i32; 4] },
    Text { run: TextRun, clip: [i32; 4] },
    /// Frame `chunk` of the FLC animation at `path`, its top-left at (x, y).
    Flc { path: String, chunk: u32, x: i32, y: i32, clip: [i32; 4], plain: bool },
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


/// What a form's canvas showed when it was put on the back buffer: each
/// operation with what it resolves to.
type Snapshot = Arc<Vec<(Op, Vec<Op>)>>;

/// The whole screen, as a clip rectangle.
const ALL: [i32; 4] = [i32::MIN, i32::MIN, i32::MAX, i32::MAX];
/// Most pieces a layer of the shown screen is kept in (beyond, it shows
/// what the back buffer has).
const MAX_PIECES: usize = 16;

/// One layer of the composed screen.
#[derive(Clone, Debug, PartialEq)]
enum Layer {
    /// A form's canvas put on the screen with its (0, 0) at `origin`.
    Form { canvas: u32, vtable: u32, origin: (i32, i32), size: (i32, i32), clip: [i32; 4], keyed: bool },
    /// Drawn straight onto the back buffer, during a paint or between
    /// paints (the frames of framed forms, e.g. a popup's border).
    Screen(Vec<Op>),
    /// What a present drew onto the screen over everything (the tooltip):
    /// always the top layer.
    Overlay(Vec<Op>),
}

/// Part of a layer as the window shows it: the layer as it was presented,
/// within `region` (left, top, right, bottom) of the screen.
#[derive(Clone, Debug)]
struct Piece {
    layer: Layer,
    /// A form's content (none for a screen layer, or a form never composed).
    ops: Option<Snapshot>,
    region: [i32; 4],
}

impl Piece {
    /// Whether `self` shows the same as `other` (outside the region).
    fn same(&self, other: &Piece) -> bool {
        let ops = match (&self.ops, &other.ops) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (a, b) => a.is_none() && b.is_none(),
        };
        ops && self.layer == other.layer
    }

    /// Where the layer can show (left, top, right, bottom); the whole screen
    /// for one drawn straight onto it.
    fn extent(&self) -> [i32; 4] {
        match self.layer {
            Layer::Form { origin, size, clip, .. } => intersect(clip, [origin.0, origin.1, origin.0 + size.0, origin.1 + size.1]),
            Layer::Screen(_) | Layer::Overlay(_) => ALL,
        }
    }
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
    presented: HashMap<u32, Snapshot>,
    /// What the screen is composed of, bottom to top, as the last paint
    /// composed it (forms later composed on their own update their place).
    order: Vec<Layer>,
    /// What the window shows, per layer of the back buffer as last
    /// presented (bottom to top): the game puts a form on the back buffer
    /// whole, but copies only a rectangle of it to the window, so a form it
    /// composes while redrawing it (e.g. the cultural advisor around its
    /// graph) shows there only in that rectangle, and the rest of the window
    /// keeps what was presented before. `None` until the first present: the
    /// back buffer is what shows then.
    shown: Option<Vec<Vec<Piece>>>,
    /// The paint in progress.
    pending: Option<Vec<Layer>>,
    /// What the last present drew over everything (see [`Layer::Overlay`]).
    overlay: Vec<Op>,
    /// Inside a present: what it has drawn so far.
    presenting: Option<Vec<Op>>,
    /// Inside a form's composition.
    compositing: bool,
    /// Form classes (vtables) whose canvases we render ourselves.
    skip: Vec<u32>,
    /// Canvases left out of every paint.
    skip_canvases: Vec<u32>,
    /// The back buffer: writes land in its composition even between
    /// paints. A present alone makes them visible in the window.
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
    /// Canvases cleared to the colour key as a whole (nothing opaque has
    /// covered them since): text drawn on them where nothing is drawn yet
    /// has the key as its backdrop ([`TextRun::backdrop`]).
    keyed_clear: Vec<u32>,
    /// Whether text gets backdrops (Windows blends text edges with what is
    /// under them; Wine's GDI draws the game's text without smoothing).
    text_backdrops: bool,
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

    /// `image` is a surface passed on as it is from now on (one found
    /// while the game runs: a form's own, made with the form).
    pub fn add_generated(&mut self, image: u32) {
        if !self.generated.contains(&image) {
            self.generated.push(image);
        }
    }

    /// `image` is no surface passed on as it is any more.
    pub fn remove_generated(&mut self, image: u32) {
        self.generated.retain(|&g| g != image);
    }

    /// Whether text drawn on a canvas cleared to the colour key has the key
    /// as its backdrop (see `keyed_clear`).
    pub fn with_text_backdrops(mut self, on: bool) -> Self {
        self.text_backdrops = on;
        self
    }

    /// `screen`: the back buffer.
    pub fn with_screen(mut self, screen: u32) -> Self {
        self.screen = screen;
        self
    }

    /// How much the model holds: canvases, their operations, the presented
    /// ones (for diagnostics).
    pub fn stats(&self) -> (usize, usize, usize) {
        let ops = self.canvases.values().map(Vec::len).sum();
        let presented = self.presented.values().map(|v| v.iter().map(|(_, r)| r.len()).sum::<usize>()).sum();
        (self.canvases.len(), ops, presented)
    }

    /// Where the pixels of `sprite` (a game `Sprite*`) come from, if known.
    pub fn sprite_source(&self, sprite: u32) -> Option<&Source> {
        self.sprites.get(&sprite)
    }

    /// The back buffer now: the forms of the last paint with what their
    /// canvases showed when last put on it (the game composes changed forms
    /// on their own, outside a full paint).
    pub fn frame(&self) -> Frame {
        let overlay = Layer::Overlay(self.overlay.clone());
        self.compose_layers(self.order.iter().chain([&overlay]).map(|layer| [self.piece(layer)]))
    }

    /// The screen now: what the window shows (see `shown`), as of the last
    /// present.
    pub fn shown_frame(&self) -> Frame {
        match &self.shown {
            Some(layers) => self.compose_layers(layers),
            None => self.frame(),
        }
    }

    /// `layer` as it is on the back buffer now, everywhere.
    fn piece(&self, layer: &Layer) -> Piece {
        let ops = match layer {
            Layer::Form { canvas, .. } => self.presented.get(canvas).cloned(),
            Layer::Screen(_) | Layer::Overlay(_) => None,
        };
        Piece { layer: layer.clone(), ops, region: ALL }
    }

    /// The screen made of `layers` (bottom to top), each in pieces.
    fn compose_layers<L: AsRef<[Piece]>>(&self, layers: impl IntoIterator<Item = L>) -> Frame {
        let mut frame = Frame::default();
        for pieces in layers {
            let pieces = pieces.as_ref();
            match pieces.last().map(|p| &p.layer) {
                Some(Layer::Screen(_) | Layer::Overlay(_)) => {
                    // In pieces, each has what reaches into it.
                    let split = pieces.len() > 1;
                    for p in pieces {
                        if let Layer::Screen(ops) | Layer::Overlay(ops) = &p.layer {
                            frame.draws.extend(ops.iter().cloned().filter_map(|op| screen_draw(op, (0, 0), p.region)).filter(|d| !split || reaches_clip(d)));
                        }
                    }
                }
                Some(&Layer::Form { canvas, vtable, origin, size, .. }) => self.compose(&mut frame, canvas, vtable, origin, size, pieces),
                None => {}
            }
        }
        frame
    }

    /// Adds form `canvas` (last composed at `origin`) to `frame`, from what
    /// `pieces` of it show.
    fn compose(&self, frame: &mut Frame, canvas: u32, vtable: u32, origin: (i32, i32), size: (i32, i32), pieces: &[Piece]) {
        frame.forms.push((vtable, canvas, origin, size));
        if self.skip.contains(&vtable) || (self.skipping && self.skip_canvases.contains(&canvas)) {
            return;
        }
        let parts: Vec<_> = pieces
            .iter()
            .filter_map(|p| {
                let Layer::Form { origin, clip, keyed, .. } = p.layer else { return None };
                // The canvas never extends beyond its size.
                let clip = intersect(p.extent(), intersect(clip, p.region));
                (clip[2] > clip[0] && clip[3] > clip[1]).then_some((p.ops.as_ref()?, origin, clip, keyed))
            })
            .collect();
        if parts.is_empty() {
            return;
        }
        let underlay = self.skipping && self.underlays.contains(&canvas);
        let start = frame.draws.len();
        // In pieces, each has what reaches into it.
        let split = parts.len() > 1;
        for (ops, origin, clip, keyed) in parts {
            // A colour-keyed form shows nothing where it is black (measured: the
            // city screen clears to black and the map shows through its art).
            let black = |op: &&Op| keyed && matches!(op, Op::Fill { rgb: [0, 0, 0], alpha: 255, .. });
            let keep = |op: &&Op| (!underlay || self.underlay_keeps(canvas, op)) && !black(op);
            let draws = ops.iter().filter(|(op, _)| keep(&op)).flat_map(|(_, resolved)| resolved.iter().cloned()).filter_map(|op| screen_draw(op, origin, clip));
            frame.draws.extend(draws.filter(|d| !split || reaches_clip(d)).map(|mut d| {
                // Generated surfaces: their whole size, as far as known.
                if let ScreenDraw::Surface { image, size, .. } = &mut d {
                    if let Some(&s) = self.sizes.get(image) {
                        *size = s;
                    }
                }
                d
            }));
        }
        frame.form_draws.push((canvas, start..frame.draws.len()));
        if underlay {
            frame.underlay_draws.push(start..frame.draws.len());
        }
    }

    /// The back buffer's `rect` (all of it if none) is copied to the window:
    /// the layers being composed (or else those of the last paint) show
    /// there now, with what the present drew over them, and what was shown
    /// before stays elsewhere.
    fn present(&mut self, rect: Option<[i32; 4]>) {
        let overlay = Layer::Overlay(self.overlay.clone());
        let now: Vec<Piece> = self.pending.as_ref().unwrap_or(&self.order).iter().chain([&overlay]).map(|l| self.piece(l)).collect();
        // All of the back buffer: everywhere.
        let whole = |r: [i32; 4]| self.sizes.get(&self.screen).is_some_and(|&(w, h)| r[0] <= 0 && r[1] <= 0 && r[2] >= w && r[3] >= h);
        let region = rect.filter(|&r| !whole(r)).unwrap_or(ALL);
        let shown = match self.shown.take() {
            // The same layers: each in its pieces.
            Some(old) if old.len() == now.len() && old.iter().zip(&now).all(|(o, n)| o.last().is_some_and(|o| same_slot(&o.layer, &n.layer))) => {
                old.into_iter().zip(now).map(|(before, new)| present_layer(before, new, region)).collect()
            }
            // Other layers (a form opened or closed): the back buffer.
            _ => now.into_iter().map(|p| vec![p]).collect(),
        };
        self.shown = Some(shown);
    }

    pub fn apply(&mut self, e: Event) {
        match e {
            Event::Loaded { image, path, size } => {
                self.files.insert(image, path);
                self.sizes.insert(image, size);
                self.canvases.remove(&image);
                self.keyed_clear.retain(|&c| c != image);
            }
            Event::Cleared { image } => {
                self.files.remove(&image);
                self.sizes.remove(&image);
                self.canvases.remove(&image);
                self.keyed_clear.retain(|&c| c != image);
            }
            Event::Size { image, size } => {
                self.sizes.insert(image, size);
            }
            Event::Sliced { sprite, image, rect } => {
                let source = match self.files.get(&image) {
                    Some(path) => Source::File { path: path.clone(), rect, mask: None, palette: None, opacity: 255 },
                    None => Source::Surface { image, rect },
                };
                self.sprites.insert(sprite, source);
            }
            Event::Copied { sprite, source } => {
                if let Some(s) = self.sprites.get(&source).cloned() {
                    self.sprites.insert(sprite, s);
                } else {
                    self.sprites.remove(&sprite);
                }
            }
            Event::Reset { sprite } => {
                self.sprites.remove(&sprite);
            }
            Event::Blit { sprite, canvas, x, y, clip, palette } => {
                if let Some(mut source) = self.sprites.get(&sprite).cloned() {
                    if let (Source::File { palette: p, .. }, Some(table)) = (&mut source, palette) {
                        *p = Some(table);
                    }
                    // Within the canvas's clip rectangle, if it has one.
                    let op = Op::Image { source, x, y, keyed: true };
                    if let Some(op) = clip.map_or(Some(op.clone()), |c| crop_move(op, c, (0, 0), true)) {
                        self.push(canvas, op);
                    }
                }
            }
            Event::OpacityBlit { sprite, canvas, x, y, clip, palette, opacity } => {
                let Some(Source::File { path, rect, mask, palette: own_palette, opacity: own_opacity }) = self.sprites.get(&sprite).cloned() else { return };
                let opacity = (u32::from(opacity) * u32::from(own_opacity) / 255) as u8;
                if opacity == 0 {
                    return;
                }
                let source = Source::File { path, rect, mask, palette: palette.or(own_palette), opacity };
                let op = Op::Image { source, x, y, keyed: true };
                if let Some(op) = clip.map_or(Some(op.clone()), |c| crop_move(op, c, (0, 0), true)) {
                    self.push(canvas, op);
                }
            }
            Event::MaskedBlit { sprite, mask, canvas, x, y, clip } => {
                let face = self.sprites.get(&sprite).cloned();
                let alpha = match self.sprites.get(&mask) {
                    Some(Source::File { path, rect, .. }) => Some(Mask { path: path.clone(), at: (rect[0], rect[1]) }),
                    _ => None,
                };
                let source = match face {
                    Some(Source::File { path, rect, palette, opacity, .. }) => Source::File { path, rect, mask: alpha, palette, opacity },
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
                    Some(path) => Source::File { path: path.clone(), rect: src, mask: None, palette: None, opacity: 255 },
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
                        if !self.keyed_clear.contains(&canvas) {
                            self.keyed_clear.push(canvas);
                        }
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
            Event::Text { canvas, mut run } => {
                // On the bare key: Windows blends the text's edges with it.
                if self.text_backdrops && self.keyed_clear.contains(&canvas) && !self.drawn_under(canvas, &run) {
                    run.backdrop = Some(KEY_RGB);
                }
                self.push(canvas, Op::Text(run))
            }
            Event::FlcLoaded { flic, path } => {
                self.flcs.insert(flic, path);
            }
            Event::FlcBlit { flic, chunk, canvas, x, y, clip, plain } => {
                if let Some(path) = self.flcs.get(&flic).cloned() {
                    // The next frame of an animation replaces the one there
                    // (a leader head is redrawn every few frames).
                    if let Some(ops) = self.canvases.get_mut(&canvas) {
                        ops.retain(|op| !matches!(op, Op::Flc { path: p, x: px, y: py, .. } if *p == path && (*px, *py) == (x, y)));
                    }
                    self.push(canvas, Op::Flc { path, chunk, x, y, clip, plain });
                }
            }
            Event::PaintBegin => self.pending = Some(Vec::new()),
            Event::Composite { canvas, vtable, origin, size, clip, keyed } => {
                self.sizes.insert(canvas, size);
                let shown: Vec<_> = self.canvases.get(&canvas).map(|ops| ops.iter().map(|op| (op.clone(), self.resolve(op, 0))).collect()).unwrap_or_default();
                // The same content keeps its snapshot (what the window shows
                // of it is then known to be unchanged).
                if self.presented.get(&canvas).is_none_or(|old| **old != shown) {
                    self.presented.insert(canvas, Arc::new(shown));
                }
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
                self.compositing = false;
                if let Some(layers) = self.pending.take() {
                    self.order = layers;
                    // Snapshots belong to the composed back buffer, not to
                    // every form ever opened. Partial presents keep their
                    // old snapshots alive through `shown`'s Arcs.
                    self.presented.retain(|canvas, _| self.order.iter().any(|layer| matches!(layer, Layer::Form { canvas: c, .. } if c == canvas)));
                }
            }
            Event::PresentBegin => self.presenting = Some(Vec::new()),
            Event::Present { rect } => {
                // What this present drew over everything replaces what the
                // last one drew (without a `PresentBegin`, e.g. an older
                // trace, it stays part of the paint).
                if let Some(ops) = self.presenting.take() {
                    self.overlay = ops;
                }
                self.present(rect)
            }
        }
    }

    /// Drops what an opaque `rect` (x, y, w, h) on `canvas` hides: all of it
    /// when it covers the canvas, else the operations entirely inside.
    fn occlude(&mut self, canvas: u32, rect: [i32; 4]) {
        if self.covers(canvas, rect) {
            self.canvases.remove(&canvas);
            self.keyed_clear.retain(|&c| c != canvas);
            return;
        }
        let [x, y, w, h] = rect;
        let inside = |[l, t, r, b]: [i32; 4]| l >= x && t >= y && r <= x + w && b <= y + h;
        if let Some(ops) = self.canvases.get_mut(&canvas) {
            ops.retain(|op| !inside(op_bounds(op)));
        }
    }

    /// Whether `canvas` has something drawn where `run` goes (by a guess of
    /// its size), under it.
    fn drawn_under(&self, canvas: u32, run: &TextRun) -> bool {
        let h = run.height.abs().max(1);
        let w = h * 2 / 3 * run.text.chars().count() as i32;
        let [l, t, r, b] = [run.x, run.y - h, run.x + w, run.y + h / 3 + 1];
        self.canvases.get(&canvas).is_some_and(|ops| {
            ops.iter().filter(|op| !matches!(op, Op::Text(_))).any(|op| {
                let [ol, ot, or, ob] = op_bounds(op);
                ol < r && ot < b && or > l && ob > t
            })
        })
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
            let ops = self.resolve(&op, 0);
            // Drawn by a present: over everything, until the next present.
            if let Some(overlay) = self.presenting.as_mut() {
                overlay.extend(ops);
                return;
            }
            // A back-buffer write is part of the current composition,
            // including after a paint has ended. It reaches the window at
            // the next present, and is not a transient present overlay.
            let layers = self.pending.as_mut().unwrap_or(&mut self.order);
            if !matches!(layers.last(), Some(Layer::Screen(_))) {
                layers.push(Layer::Screen(Vec::new()));
            }
            let Some(Layer::Screen(screen)) = layers.last_mut() else { unreachable!() };
            for op in ops {
                retain_op(screen, op);
            }
            return;
        }
        retain_op(self.canvases.entry(canvas).or_default(), op);
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

/// Retains a draw without accumulating repeated opaque pictures or an
/// unbounded history, for both canvases and direct back-buffer writes.
fn retain_op(ops: &mut Vec<Op>, op: Op) {
    // The same picture again where it was: the earlier one shows nothing
    // the new one does not cover. Some forms draw themselves again over
    // what they drew without clearing it first (the advisors' common
    // background, every few seconds); their lists, every copy of them,
    // and so the screen would grow without end.
    if overdraws_itself(&op) {
        if let Some(i) = ops.iter().rposition(|o| *o == op) {
            ops.remove(i);
        }
    }
    if ops.len() >= MAX_OPS {
        ops.drain(..MAX_OPS / 2);
    }
    ops.push(op);
}

/// Whether drawing `op` over itself shows the same as drawing it once: an
/// image (opaque, or colour-keyed: where it is transparent neither copy
/// draws) or an opaque fill. Not what blends with what is under it (masked
/// images, translucent fills, text) nor a surface the game keeps changing.
fn overdraws_itself(op: &Op) -> bool {
    match op {
        Op::Image { source: Source::File { mask, opacity, .. }, .. } => mask.is_none() && *opacity == 255,
        Op::Fill { alpha, .. } => *alpha == 255,
        Op::Image { source: Source::Surface { .. }, .. } | Op::Text(_) | Op::Flc { .. } => false,
    }
}

/// Whether `d` may draw inside its clip rectangle (text and animations by a
/// generous guess of their size).
pub(crate) fn reaches_clip(d: &ScreenDraw) -> bool {
    let overlaps = |[l, t, r, b]: [i32; 4], c: &[i32; 4]| l < c[2] && t < c[3] && r > c[0] && b > c[1];
    match d {
        ScreenDraw::Image { rect, x, y, clip, .. } | ScreenDraw::Surface { rect, x, y, clip, .. } => overlaps([*x, *y, x + rect[2], y + rect[3]], clip),
        ScreenDraw::Fill { rect: [x, y, w, h], clip, .. } => overlaps([*x, *y, x + w, y + h], clip),
        ScreenDraw::Flc { x, y, clip, .. } => *x < clip[2] && *y < clip[3],
        ScreenDraw::Text { run, clip } => {
            let h = run.height.abs().max(16);
            let w = h * run.text.chars().count() as i32;
            overlaps([run.x - w, run.y - h, run.x + w, run.y + 2 * h], clip)
        }
    }
}

/// Whether two layers are the same one of the screen (a form by its
/// canvas, wherever it is).
fn same_slot(a: &Layer, b: &Layer) -> bool {
    match (a, b) {
        (Layer::Form { canvas: x, .. }, Layer::Form { canvas: y, .. }) => x == y,
        (Layer::Screen(_), Layer::Screen(_)) | (Layer::Overlay(_), Layer::Overlay(_)) => true,
        _ => false,
    }
}

/// What the window shows of a layer after `region` of the back buffer, where
/// it is `new`, is presented: `new` there and what it showed (`before`)
/// elsewhere.
fn present_layer(before: Vec<Piece>, new: Piece, region: [i32; 4]) -> Vec<Piece> {
    let e = new.extent();
    let covered = region[0] <= e[0] && region[1] <= e[1] && region[2] >= e[2] && region[3] >= e[3];
    if covered || before.iter().all(|p| p.same(&new)) {
        return vec![new];
    }
    let mut pieces: Vec<Piece> = before
        .into_iter()
        .flat_map(|p| subtract(p.region, region).into_iter().map(move |r| Piece { region: r, ..p.clone() }))
        .collect();
    pieces.push(Piece { region: intersect(new.region, region), ..new.clone() });
    if pieces.len() > MAX_PIECES {
        return vec![new];
    }
    pieces
}

/// The parts of `a` outside `b` (both left, top, right, bottom), as up to
/// four rectangles.
fn subtract(a: [i32; 4], b: [i32; 4]) -> Vec<[i32; 4]> {
    let i = intersect(a, b);
    if i[2] <= i[0] || i[3] <= i[1] {
        return vec![a];
    }
    let parts = [
        [a[0], a[1], a[2], i[1]], // above
        [a[0], i[3], a[2], a[3]], // below
        [a[0], i[1], i[0], i[3]], // left
        [i[2], i[1], a[2], i[3]], // right
    ];
    parts.into_iter().filter(|r| r[2] > r[0] && r[3] > r[1]).collect()
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
                Source::File { path, mask, palette, opacity, .. } => {
                    // The mask moves along with the cut.
                    let mask = mask.map(|m| Mask { at: (m.at.0 + (l - x), m.at.1 + (t - y)), ..m });
                    Source::File { path, rect, mask, palette, opacity }
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
        Op::Flc { path, chunk, x, y, clip, plain } => {
            let inside = x >= window[0] && x < window[2] && y >= window[1] && y < window[3];
            let clip = intersect(clip.unwrap_or(window), window);
            let clip = Some([clip[0] + dx, clip[1] + dy, clip[2] + dx, clip[3] + dy]);
            inside.then_some(Op::Flc { path, chunk, x: x + dx, y: y + dy, clip, plain })
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

/// The pixels of a line of the game's line drawer as rectangles (x, y, w,
/// h) with how much of each the line covers (of 256). With `smooth`, as
/// Windows' OpenGL draws it (the renderer turns on `GL_LINE_SMOOTH` with
/// blending, 0x627710): a line along a row or column is a band `width`
/// wide centred on the pixel edge at its coordinate, so a 1-pixel line at y
/// covers half of rows y - 1 and y (measured on the Windows VM: the city
/// label frames, black at full opacity, halve what is under them on two rows
/// and columns each). Otherwise, and for slanted lines, [`line_rects`]
/// (under Wine C3X draws the lines with GDI+, measured to match it).
pub fn line_fills(from: (i32, i32), to: (i32, i32), width: i32, smooth: bool) -> Vec<([i32; 4], u32)> {
    let ((x1, y1), (x2, y2)) = (from, to);
    let straight = (x1 == x2) != (y1 == y2);
    if !smooth || !straight {
        return line_rects(from, to, width).into_iter().map(|r| (r, 256)).collect();
    }
    // The band across the line, in half pixels: [c - w, c + w) around the
    // edge at 2·c.
    let w = width.max(1);
    let band = |c: i32| -> Vec<(i32, i32, u32)> {
        let (lo, hi) = (2 * c - w, 2 * c + w);
        let mut out: Vec<(i32, i32, u32)> = Vec::new();
        for j in lo.div_euclid(2)..(hi + 1).div_euclid(2) {
            let covered = (hi.min(2 * j + 2) - lo.max(2 * j)) as u32 * 128;
            // Rows (or columns) covered alike go into one rectangle.
            match out.last_mut() {
                Some((_, n, c)) if *c == covered => *n += 1,
                _ => out.push((j, 1, covered)),
            }
        }
        out
    };
    if y1 == y2 {
        let (l, len) = (x1.min(x2), (x2 - x1).abs());
        band(y1).into_iter().map(|(y, n, c)| ([l, y, len, n], c)).collect()
    } else {
        let (t, len) = (y1.min(y2), (y2 - y1).abs());
        band(x1).into_iter().map(|(x, n, c)| ([x, t, n, len], c)).collect()
    }
}

/// Whether the game's line drawer draws smooth lines (see [`line_fills`]):
/// on Windows, not in Wine (where C3X, as installed, draws them with GDI+).
pub fn smooth_lines() -> bool {
    static SMOOTH: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SMOOTH.get_or_init(|| cfg!(windows) && !gdi_text::under_wine())
}

/// `alpha` (of 255) of which `coverage` (of 256) is drawn.
pub fn covered_alpha(alpha: u8, coverage: u32) -> u8 {
    (alpha as u32 * coverage.min(256) / 256) as u8
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
        Op::Image { source: Source::File { path, rect, mask, palette, opacity }, x, y, keyed } => {
            Some(ScreenDraw::Image { path, rect, x: x + ox, y: y + oy, keyed, clip, mask, palette, opacity })
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
        Op::Flc { path, chunk, x, y, clip: own, plain } => {
            let clip = own.map_or(clip, |c| intersect(clip, [c[0] + ox, c[1] + oy, c[2] + ox, c[3] + oy]));
            Some(ScreenDraw::Flc { path, chunk, x: x + ox, y: y + oy, clip, plain })
        }
    }
}

/// A canvas's clip rectangle `clip` for what is drawn onto it, unless it is
/// just `dirty`, the rectangle of the back buffer a paint or present is
/// updating: no clip then (see `ui_trace`). A framed form inside another
/// (e.g. a list box's scroll bar in a popup) is clipped to its parent's
/// rectangle within that one: kept so, in a partial paint a little too
/// much.
pub fn content_clip(clip: Option<[i32; 4]>, dirty: Option<[i32; 4]>) -> Option<[i32; 4]> {
    if clip.is_some() && clip == dirty { None } else { clip }
}

/// A 16-bit colour of the game's canvases (RGB555; bit 31 is a flag) as
/// 8-bit RGB.
pub fn rgb555(c: u32) -> [u8; 3] {
    let e = |v: u32| ((v << 3) | (v >> 2)) as u8;
    [e((c >> 10) & 31), e((c >> 5) & 31), e(c & 31)]
}

/// Uniform sprite opacity passed as the raw bits of a C `float`.
/// Invalid/nonpositive values draw nothing; values above one are opaque.
pub fn sprite_opacity(bits: u32) -> u8 {
    let opacity = f32::from_bits(bits);
    if opacity.is_nan() { 0 } else { (opacity.clamp(0.0, 1.0) * 255.0).round() as u8 }
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
        m.apply(Event::Blit { sprite: 20, canvas: 300, x: 7, y: 8, clip: None, palette: None });
        m.apply(Event::Cleared { image: 10 });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (100, 200), (40, 40)));
        m.apply(Event::PaintEnd);
        assert_eq!(
            m.frame().draws,
            vec![ScreenDraw::Image { path: "Art/a.pcx".into(), rect: [5, 6, 30, 20], x: 107, y: 208, keyed: true, clip: [100, 200, 140, 240], mask: None, palette: None, opacity: 255 }]
        );
    }

    #[test]
    fn fading_file_sprites_keep_opacity_through_clipped_canvas_copies() {
        let mut m = UiModel::new(vec![], vec![]);
        m.apply(Event::Loaded { image: 10, path: "Art/bubble.pcx".into(), size: (100, 50) });
        m.apply(Event::Sliced { sprite: 20, image: 10, rect: [0, 0, 100, 50] });
        m.apply(Event::OpacityBlit { sprite: 20, canvas: 40, x: 2, y: 3, clip: None, palette: None, opacity: 128 });
        m.apply(Event::Copy { from: 40, src: [8, 8, 10, 10], canvas: 300, x: 20, y: 30, keyed: false });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (100, 200), (100, 100)));
        m.apply(Event::PaintEnd);
        assert!(matches!(&m.frame().draws[..], [ScreenDraw::Image { rect: [6, 5, 10, 10], x: 120, y: 230, opacity: 128, .. }]));
        for (value, expected) in [(0.0_f32, 0), (0.5, 128), (1.0, 255), (-1.0, 0), (2.0, 255), (f32::NAN, 0)] {
            assert_eq!(sprite_opacity(value.to_bits()), expected);
        }
    }

    #[test]
    fn closed_forms_release_snapshots_without_changing_presented_pixels() {
        let mut m = UiModel::new(vec![], vec![]).with_screen(999);
        for canvas in 300..400 {
            m.apply(Event::Text { canvas, run: text(1, 10, "Retained") });
            m.apply(Event::PaintBegin);
            m.apply(composite(canvas, (0, 0), (100, 100)));
            m.apply(Event::CompositeEnd);
            m.apply(Event::PaintEnd);
            assert_eq!(m.presented.len(), 1);
            m.apply(Event::Present { rect: None });
            m.apply(Event::Cleared { image: canvas });
        }
        let shown = m.shown_frame();
        m.apply(Event::PaintBegin);
        m.apply(Event::PaintEnd);
        assert!(m.presented.is_empty());
        assert_eq!(m.shown_frame(), shown);
        m.apply(Event::Present { rect: None });
        assert!(m.shown_frame().draws.is_empty());
    }

    #[test]
    fn copying_an_empty_sprite_does_not_keep_the_previous_image() {
        let mut m = UiModel::new(vec![], vec![]);
        m.apply(Event::Loaded { image: 10, path: "Art/a.pcx".into(), size: (10, 10) });
        m.apply(Event::Sliced { sprite: 20, image: 10, rect: [0, 0, 10, 10] });
        m.apply(Event::Copied { sprite: 20, source: 12345 });
        m.apply(Event::Blit { sprite: 20, canvas: 300, x: 0, y: 0, clip: None, palette: None });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (0, 0), (10, 10)));
        m.apply(Event::PaintEnd);
        assert!(m.frame().draws.is_empty());
    }

    #[test]
    fn a_leader_heads_next_frame_replaces_the_last() {
        // The head is drawn onto its own image, which the form copies.
        let mut m = UiModel::new(vec![], vec![]);
        m.apply(Event::FlcLoaded { flic: 5, path: "Art/Flics/Al_01.FLC".into() });
        for chunk in [3, 4] {
            m.apply(Event::FlcBlit { flic: 5, chunk, canvas: 40, x: 0, y: 0, clip: None, plain: true });
        }
        m.apply(Event::Copy { from: 40, src: [0, 0, 200, 240], canvas: 300, x: 411, y: 59, keyed: false });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (0, 0), (1920, 1080)));
        m.apply(Event::PaintEnd);
        assert_eq!(
            m.frame().draws,
            vec![ScreenDraw::Flc { path: "Art/Flics/Al_01.FLC".into(), chunk: 4, x: 411, y: 59, clip: [411, 59, 611, 299], plain: true }]
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
                ScreenDraw::Image { path: "Art/bg.pcx".into(), rect: [18, 18, 10, 10], x: 2, y: 3, keyed: false, clip, mask: None, palette: None, opacity: 255 },
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
        m.apply(Event::Blit { sprite: 21, canvas: 300, x: 1, y: 2, clip: None, palette: None });
        m.apply(Event::Blit { sprite: 20, canvas: 300, x: 3, y: 4, clip: None, palette: None });
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
                ScreenDraw::Image { path: "Art\\Cities\\barracks.pcx".into(), rect: [1, 1, 23, 15], x: 1, y: 2, keyed: true, clip, mask: None, palette: None, opacity: 255 },
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
    fn smooth_lines_straddle_the_pixel_edge() {
        // A city label's frame on Windows (1 wide): the top edge at y 642
        // from x 367 to 505 halves rows 641 and 642, x 367..505; the left
        // edge columns 366 and 367 (measured).
        assert_eq!(line_fills((367, 642), (505, 642), 1, true), vec![([367, 641, 138, 2], 128)]);
        assert_eq!(line_fills((367, 642), (367, 663), 1, true), vec![([366, 642, 2, 21], 128)]);
        // Two wide: the two rows around the edge, whole; three: half rows on
        // both sides of a whole one.
        assert_eq!(line_fills((0, 5), (4, 5), 2, true), vec![([0, 4, 4, 2], 256)]);
        assert_eq!(line_fills((0, 5), (4, 5), 3, true), vec![([0, 3, 4, 1], 128), ([0, 4, 4, 2], 256), ([0, 6, 4, 1], 128)]);
        // Slanted lines, and every line without smoothing: the measured rule.
        assert_eq!(line_fills((64, 0), (0, 32), 2, true).len(), 64);
        assert_eq!(line_fills((5, 7), (9, 7), 1, false), vec![([5, 7, 4, 1], 256)]);
        assert_eq!(covered_alpha(255, 128), 127);
        assert_eq!(covered_alpha(128, 256), 128);
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

    fn text(x: i32, y: i32, s: &str) -> TextRun {
        TextRun { x, y, color: [0, 0, 0], face: "Times".into(), height: -12, weight: 400, italic: false, underline: false, text: s.into(), backdrop: None }
    }

    /// The cultural advisor (measured): every few seconds it clears its
    /// canvas and draws it again; once its graph is drawn it composes the
    /// screen and presents only the graph's rectangle, then draws its text,
    /// and a paint presents all of it.
    fn advisor_redraw(m: &mut UiModel, partial_present: Option<[i32; 4]>) -> (Frame, Frame) {
        m.apply(Event::Fill { canvas: 300, rect: [0, 0, 200, 100], rgb: [1, 1, 1], alpha: 255, clip: None });
        m.apply(Event::Fill { canvas: 300, rect: [10, 10, 30, 20], rgb: [5, 5, 5], alpha: 255, clip: None });
        // The graph is composed (outside a paint) and presented.
        m.apply(composite(400, (0, 0), (1000, 800)));
        m.apply(composite(300, (100, 100), (200, 100)));
        m.apply(Event::CompositeEnd);
        m.apply(Event::Present { rect: partial_present });
        let partial = (m.frame(), m.shown_frame());
        m.apply(Event::Text { canvas: 300, run: text(150, 60, "Culture") });
        m.apply(Event::Fill { canvas: 300, rect: [120, 40, 20, 5], rgb: [6, 6, 6], alpha: 255, clip: None });
        partial
    }

    fn advisor() -> UiModel {
        let mut m = UiModel::new(vec![], vec![]).with_screen(999);
        m.apply(Event::Size { image: 999, size: (1000, 800) });
        m.apply(Event::Size { image: 300, size: (200, 100) });
        m.apply(Event::Fill { canvas: 400, rect: [0, 0, 1000, 800], rgb: [2, 2, 2], alpha: 255, clip: None });
        advisor_redraw(&mut m, None);
        m.apply(Event::PaintBegin);
        m.apply(composite(400, (0, 0), (1000, 800)));
        m.apply(composite(300, (100, 100), (200, 100)));
        m.apply(Event::Present { rect: Some([100, 100, 300, 200]) });
        m.apply(Event::PaintEnd);
        m
    }

    fn texts(f: &Frame) -> Vec<String> {
        f.draws.iter().filter_map(|d| if let ScreenDraw::Text { run, .. } = d { Some(run.text.clone()) } else { None }).collect()
    }

    #[test]
    fn a_form_composed_mid_redraw_shows_only_where_it_was_presented() {
        let mut m = advisor();
        let complete = m.shown_frame();
        assert_eq!(texts(&complete), vec!["Culture"]);
        // Its graph's rectangle presented: on the back buffer the advisor has
        // no text yet, but the window still shows it outside that rectangle.
        let (back, shown) = advisor_redraw(&mut m, Some([110, 110, 140, 130]));
        assert!(texts(&back).is_empty());
        assert_eq!(texts(&shown), vec!["Culture"]);
        let clips: Vec<_> = shown.draws.iter().filter_map(|d| if let ScreenDraw::Text { clip, .. } = d { Some(*clip) } else { None }).collect();
        // (Of the pieces above, below, left and right of the presented
        // rectangle, only the one below reaches the text.)
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0], [100, 130, 300, 200]);
        // In the rectangle the window shows the new graph.
        assert!(shown.draws.contains(&ScreenDraw::Fill { rect: [110, 110, 30, 20], rgb: [5, 5, 5], alpha: 255, clip: [110, 110, 140, 130] }));
        // Forms whose content did not change stay whole.
        assert_eq!(shown.forms.len(), 2);
        assert_eq!(shown.form_draws.len(), 2);
        // The paint then presents the whole advisor: complete again, in one piece.
        m.apply(Event::PaintBegin);
        m.apply(composite(400, (0, 0), (1000, 800)));
        m.apply(composite(300, (100, 100), (200, 100)));
        m.apply(Event::Present { rect: Some([100, 100, 300, 200]) });
        m.apply(Event::PaintEnd);
        assert_eq!(m.shown_frame(), complete);
        assert_eq!(m.shown_frame(), m.frame());
    }

    /// The game draws its tooltip in every present (over everything, onto
    /// the back buffer) and copies a rectangle of the back buffer to the
    /// window; presents of rectangles composed on their own come between
    /// the paints.
    #[test]
    fn a_tooltip_is_drawn_by_every_present_and_goes_with_it() {
        let mut m = UiModel::new(vec![], vec![]).with_screen(999);
        m.apply(Event::Size { image: 999, size: (1000, 800) });
        m.apply(Event::Fill { canvas: 400, rect: [0, 0, 1000, 800], rgb: [2, 2, 2], alpha: 255, clip: None });
        let present = |m: &mut UiModel, tip: Option<(i32, &str)>, rect: Option<[i32; 4]>| {
            m.apply(Event::PresentBegin);
            if let Some((x, s)) = tip {
                m.apply(Event::Fill { canvas: 999, rect: [x, 500, 150, 14], rgb: [255, 255, 255], alpha: 255, clip: None });
                m.apply(Event::Text { canvas: 999, run: text(x + 3, 511, s) });
            }
            m.apply(Event::Present { rect });
        };
        let paint = |m: &mut UiModel, tip: Option<(i32, &str)>, rect: [i32; 4]| {
            m.apply(Event::PaintBegin);
            m.apply(composite(400, (0, 0), (1000, 800)));
            m.apply(Event::CompositeEnd);
            present(m, tip, Some(rect));
            m.apply(Event::PaintEnd);
        };
        let at = |x: i32| Some([x - 1, 499, x + 152, 515]);
        // The texts shown: their pen inside their clip.
        let texts = |f: &Frame| -> Vec<String> {
            let shown = |run: &TextRun, c: &[i32; 4]| run.x >= c[0] && run.x < c[2] && run.y - 2 >= c[1] && run.y - 2 < c[3];
            f.draws.iter().filter_map(|d| if let ScreenDraw::Text { run, clip } = d { shown(run, clip).then(|| run.text.clone()) } else { None }).collect()
        };
        let boxes = |f: &Frame| f.draws.iter().filter(|d| matches!(d, ScreenDraw::Fill { rgb: [255, 255, 255], .. })).count();
        paint(&mut m, None, [0, 0, 1000, 800]);
        // The pointer rests on a button: the tooltip appears in a present
        // of its rectangle outside a paint, box and text.
        present(&mut m, Some((100, "Fortify")), at(100));
        assert_eq!((texts(&m.shown_frame()), boxes(&m.shown_frame())), (vec!["Fortify".to_string()], 1));
        // The button is painted (its rectangle): the tooltip stays whole.
        paint(&mut m, Some((100, "Fortify")), [100, 520, 132, 552]);
        assert_eq!((texts(&m.shown_frame()), boxes(&m.shown_frame())), (vec!["Fortify".to_string()], 1));
        // The whole screen presented (an animation): still there.
        present(&mut m, Some((100, "Fortify")), None);
        assert_eq!(texts(&m.shown_frame()), ["Fortify"]);
        // On the next button: the old one's place presented without it,
        // then the new one's; no trace of the old text is left.
        present(&mut m, Some((300, "Bombard")), at(100));
        present(&mut m, Some((300, "Bombard")), at(300));
        assert_eq!(texts(&m.shown_frame()), ["Bombard"]);
        present(&mut m, Some((300, "Bombard")), None);
        assert_eq!((texts(&m.shown_frame()), boxes(&m.shown_frame())), (vec!["Bombard".to_string()], 1));
        // Off the buttons: its place presented with no tooltip.
        present(&mut m, None, at(300));
        assert!(texts(&m.shown_frame()).is_empty());
        assert!(texts(&m.frame()).is_empty());
        present(&mut m, None, None);
        assert_eq!(boxes(&m.shown_frame()), 0);
    }

    /// A present updating a rectangle smaller than the tooltip (the
    /// pointer's, a button's) draws the whole tooltip onto the back buffer,
    /// clipped by the game to that rectangle (which the tracer drops, see
    /// [`content_clip`]): the box stays whole on the screen, every pixel of
    /// it, not cut to the rectangle nor left to its text.
    #[test]
    fn a_tooltip_box_stays_whole_through_presents_smaller_than_it() {
        let mut m = UiModel::new(vec![], vec![]).with_screen(999);
        m.apply(Event::Size { image: 999, size: (1000, 800) });
        m.apply(Event::Fill { canvas: 400, rect: [0, 0, 1000, 800], rgb: [2, 2, 2], alpha: 255, clip: None });
        let tip = [100, 500, 150, 14];
        let present = |m: &mut UiModel, rect: [i32; 4]| {
            // The game clips the back buffer to the rectangle it updates.
            let clip = content_clip(Some(rect), Some(rect));
            m.apply(Event::PresentBegin);
            m.apply(Event::Fill { canvas: 999, rect: [tip[0] - 1, tip[1] - 1, tip[2] + 2, tip[3] + 2], rgb: [0, 0, 0], alpha: 255, clip });
            m.apply(Event::Fill { canvas: 999, rect: tip, rgb: [255, 255, 255], alpha: 255, clip });
            m.apply(Event::Text { canvas: 999, run: text(tip[0] + 3, tip[1] + 11, "Fortify") });
            m.apply(Event::Present { rect: Some(rect) });
        };
        // The pixels of the box the white fills of `f` reach, of all of it.
        let box_shown = |f: &Frame| {
            let white: Vec<[i32; 4]> = f
                .draws
                .iter()
                .filter_map(|d| match d {
                    ScreenDraw::Fill { rect: [x, y, w, h], rgb: [255, 255, 255], clip, .. } => Some(intersect([*x, *y, x + w, y + h], *clip)),
                    _ => None,
                })
                .collect();
            let [x, y, w, h] = tip;
            let pixels = (y..y + h).flat_map(|py| (x..x + w).map(move |px| (px, py)));
            let reached = pixels.filter(|&(px, py)| white.iter().any(|r| px >= r[0] && px < r[2] && py >= r[1] && py < r[3])).count();
            (reached, (w * h) as usize)
        };
        let whole = (tip[2] * tip[3]) as usize;
        m.apply(Event::PaintBegin);
        m.apply(composite(400, (0, 0), (1000, 800)));
        m.apply(Event::CompositeEnd);
        m.apply(Event::Present { rect: None });
        m.apply(Event::PaintEnd);
        // It appears in a present of only the pointer's rectangle, inside it.
        present(&mut m, [120, 502, 140, 512]);
        assert_eq!(box_shown(&m.frame()), (whole, whole));
        // Its own rectangle presented: whole on the screen.
        present(&mut m, [tip[0] - 1, tip[1] - 1, tip[0] + tip[2] + 1, tip[1] + tip[3] + 1]);
        assert_eq!(box_shown(&m.shown_frame()), (whole, whole));
        // The pointer moves on the button: only its rectangle, across the
        // box's edge, then inside it.
        for r in [[200, 505, 232, 537], [130, 503, 146, 511]] {
            present(&mut m, r);
            assert_eq!(box_shown(&m.shown_frame()), (whole, whole), "after a present of {r:?}");
        }
        // The button under it painted (a rectangle smaller than the box).
        m.apply(Event::PaintBegin);
        m.apply(composite(400, (0, 0), (1000, 800)));
        m.apply(Event::CompositeEnd);
        present(&mut m, [140, 498, 172, 530]);
        m.apply(Event::PaintEnd);
        assert_eq!(box_shown(&m.shown_frame()), (whole, whole));
        assert_eq!(box_shown(&m.frame()), (whole, whole));
    }

    #[test]
    fn the_rectangle_being_updated_is_no_clip() {
        let (r, parent) = ([100, 520, 132, 552], [90, 500, 140, 540]);
        assert_eq!(content_clip(Some(r), Some(r)), None);
        assert_eq!(content_clip(Some(parent), Some(r)), Some(parent));
        assert_eq!(content_clip(Some(r), None), Some(r));
        assert_eq!(content_clip(None, Some(r)), None);
    }

    /// A popup's frame, drawn onto the screen in every paint, stays when a
    /// paint updates only a rectangle inside the popup.
    #[test]
    fn a_frame_drawn_onto_the_screen_stays_through_partial_paints() {
        let mut m = UiModel::new(vec![], vec![]).with_screen(999);
        m.apply(Event::Size { image: 999, size: (1000, 800) });
        m.apply(Event::Fill { canvas: 400, rect: [0, 0, 1000, 800], rgb: [2, 2, 2], alpha: 255, clip: None });
        m.apply(Event::Fill { canvas: 500, rect: [0, 0, 300, 200], rgb: [3, 3, 3], alpha: 255, clip: None });
        let paint = |m: &mut UiModel, rect: [i32; 4]| {
            m.apply(Event::PaintBegin);
            m.apply(composite(400, (0, 0), (1000, 800)));
            m.apply(Event::CompositeEnd);
            m.apply(Event::Fill { canvas: 999, rect: [97, 97, 306, 3], rgb: [8, 74, 41], alpha: 255, clip: None });
            m.apply(composite(500, (100, 100), (300, 200)));
            m.apply(Event::CompositeEnd);
            m.apply(Event::PresentBegin);
            m.apply(Event::Present { rect: Some(rect) });
            m.apply(Event::PaintEnd);
        };
        let frames = |f: &Frame| f.draws.iter().filter(|d| matches!(d, ScreenDraw::Fill { rgb: [8, 74, 41], .. })).count();
        paint(&mut m, [0, 0, 1000, 800]);
        assert_eq!(frames(&m.shown_frame()), 1);
        paint(&mut m, [150, 150, 200, 170]);
        assert_eq!(frames(&m.shown_frame()), 1);
        assert_eq!(frames(&m.frame()), 1);
    }

    /// A direct back-buffer copy can cover the HUD after composition has
    /// finished. It is retained until another paint, not until the next
    /// tooltip present. Its source may already be reused before present.
    #[test]
    fn writes_between_paints_are_retained_back_buffer_content() {
        let mut m = UiModel::new(vec![], vec![]).with_screen(999);
        m.apply(Event::Size { image: 999, size: (1000, 800) });
        m.apply(Event::Fill { canvas: 400, rect: [0, 0, 1000, 800], rgb: [2, 2, 2], alpha: 255, clip: None });
        let paint = |m: &mut UiModel| {
            m.apply(Event::PaintBegin);
            m.apply(composite(400, (0, 0), (1000, 800)));
            m.apply(Event::CompositeEnd);
            m.apply(Event::PaintEnd);
        };
        // Evaluate the opaque fills at sample pixels independently of the
        // model's layers, including the clipping of partial presents.
        let pixel = |f: &Frame, x: i32, y: i32| {
            f.draws.iter().fold(None, |under, d| match d {
                ScreenDraw::Fill { rect: [l, t, w, h], rgb, alpha: 255, clip }
                    if x >= *l && x < l + w && y >= *t && y < t + h
                        && x >= clip[0] && x < clip[2] && y >= clip[1] && y < clip[3] => Some(*rgb),
                _ => under,
            })
        };
        paint(&mut m);
        m.apply(Event::Present { rect: None });
        let before = m.shown_frame();
        assert_eq!(pixel(&before, 5, 5), Some([2, 2, 2]));
        m.apply(Event::Fill { canvas: 600, rect: [0, 0, 1000, 800], rgb: [0, 0, 0], alpha: 255, clip: None });
        m.apply(Event::Fill { canvas: 600, rect: [100, 100, 400, 300], rgb: [7, 7, 7], alpha: 255, clip: None });
        m.apply(Event::Copy { from: 600, src: [0, 0, 1000, 800], canvas: 999, x: 0, y: 0, keyed: false });
        m.apply(Event::Cleared { image: 600 });
        assert_eq!(m.shown_frame(), before, "writing is not a present");
        assert_eq!(pixel(&m.frame(), 5, 5), Some([0, 0, 0]));
        assert_eq!(pixel(&m.frame(), 200, 200), Some([7, 7, 7]));
        m.apply(Event::PresentBegin);
        m.apply(Event::Fill { canvas: 999, rect: [10, 10, 20, 10], rgb: [8, 8, 8], alpha: 255, clip: None });
        m.apply(Event::Present { rect: None });
        assert_eq!(pixel(&m.shown_frame(), 15, 15), Some([8, 8, 8]));
        m.apply(Event::PresentBegin);
        m.apply(Event::Present { rect: None });
        assert_eq!(pixel(&m.shown_frame(), 15, 15), Some([0, 0, 0]), "only the tooltip expires");
        assert_eq!(pixel(&m.shown_frame(), 200, 200), Some([7, 7, 7]));
        // A later direct write changes the back buffer everywhere, but a
        // partial present updates only the requested window rectangle.
        m.apply(Event::Fill { canvas: 999, rect: [0, 0, 1000, 800], rgb: [9, 9, 9], alpha: 255, clip: None });
        m.apply(Event::Present { rect: Some([100, 100, 300, 300]) });
        assert_eq!(pixel(&m.frame(), 5, 5), Some([9, 9, 9]));
        assert_eq!(pixel(&m.shown_frame(), 5, 5), Some([0, 0, 0]));
        assert_eq!(pixel(&m.shown_frame(), 200, 200), Some([9, 9, 9]));
        paint(&mut m);
        m.apply(Event::PresentBegin);
        m.apply(Event::Present { rect: None });
        assert_eq!(pixel(&m.shown_frame(), 5, 5), Some([2, 2, 2]));
        assert_eq!(pixel(&m.shown_frame(), 200, 200), Some([2, 2, 2]));
    }

    #[test]
    fn repeated_direct_back_buffer_writes_do_not_accumulate() {
        let mut m = UiModel::new(vec![], vec![]).with_screen(999);
        for _ in 0..MAX_OPS + 1 {
            m.apply(Event::Fill { canvas: 999, rect: [0, 0, 1000, 800], rgb: [0, 0, 0], alpha: 255, clip: None });
        }
        assert_eq!(m.frame().draws.len(), 1);
        m.apply(Event::PresentBegin);
        m.apply(Event::Present { rect: None });
        assert_eq!(m.shown_frame().draws.len(), 1);
    }

    #[test]
    fn a_present_of_all_of_it_shows_the_back_buffer() {
        let mut m = advisor();
        let (back, shown) = advisor_redraw(&mut m, Some([0, 0, 1000, 800]));
        assert_eq!(shown, back);
        let (back, shown) = advisor_redraw(&mut m, None);
        assert_eq!(shown, back);
    }

    #[test]
    fn until_the_first_present_the_back_buffer_shows() {
        let mut m = UiModel::new(vec![], vec![]);
        m.apply(Event::Fill { canvas: 300, rect: [1, 1, 2, 2], rgb: [3, 3, 3], alpha: 255, clip: None });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (0, 0), (10, 10)));
        m.apply(Event::PaintEnd);
        assert_eq!(m.shown_frame().draws.len(), 1);
        assert_eq!(m.shown_frame(), m.frame());
    }

    #[test]
    fn a_form_opened_by_a_paint_shows_at_its_present() {
        // The present comes before the end of the paint: the forms it
        // composed are what it shows.
        let mut m = advisor();
        m.apply(Event::Fill { canvas: 500, rect: [0, 0, 5, 5], rgb: [9, 9, 9], alpha: 255, clip: None });
        m.apply(Event::PaintBegin);
        m.apply(composite(400, (0, 0), (1000, 800)));
        m.apply(composite(300, (100, 100), (200, 100)));
        m.apply(composite(500, (500, 500), (5, 5)));
        m.apply(Event::Present { rect: Some([500, 500, 505, 505]) });
        let shown = m.shown_frame();
        m.apply(Event::PaintEnd);
        assert_eq!(shown.forms.len(), 3);
        assert!(shown.draws.contains(&ScreenDraw::Fill { rect: [500, 500, 5, 5], rgb: [9, 9, 9], alpha: 255, clip: [500, 500, 505, 505] }));
        assert_eq!(m.shown_frame(), shown);
    }

    #[test]
    fn a_form_composed_only_outside_a_paint_shows_once_composed_as_one() {
        // The end of game's victory screen runs a loop of its own: it
        // composes every form and presents, never painting. Composed alone,
        // a form the last paint did not have stays out of the screen; the
        // bridge reports that whole composition as a paint, and it shows.
        let mut m = advisor();
        m.apply(Event::Fill { canvas: 600, rect: [0, 0, 1000, 800], rgb: [7, 7, 7], alpha: 255, clip: None });
        let victory = ScreenDraw::Fill { rect: [0, 0, 1000, 800], rgb: [7, 7, 7], alpha: 255, clip: ALL };
        let frame = |m: &UiModel| {
            let mut f = m.shown_frame();
            for d in &mut f.draws {
                if let ScreenDraw::Fill { clip, .. } = d {
                    *clip = ALL;
                }
            }
            f
        };
        for _ in 0..2 {
            m.apply(composite(400, (0, 0), (1000, 800)));
            m.apply(composite(300, (100, 100), (200, 100)));
            m.apply(composite(600, (0, 0), (1000, 800)));
            m.apply(Event::Present { rect: None });
        }
        assert!(!frame(&m).draws.contains(&victory));
        m.apply(Event::PaintBegin);
        m.apply(composite(400, (0, 0), (1000, 800)));
        m.apply(composite(300, (100, 100), (200, 100)));
        m.apply(composite(600, (0, 0), (1000, 800)));
        m.apply(Event::PaintEnd);
        m.apply(Event::Present { rect: None });
        assert_eq!(m.shown_frame().forms.len(), 3);
        assert!(frame(&m).draws.contains(&victory));
    }

    #[test]
    fn a_form_drawn_again_over_itself_does_not_grow() {
        // The advisors' common background (measured: F5): every few seconds
        // its art, head and tabs are drawn again without the canvas being
        // cleared; the cultural advisor copies it into its own canvas.
        let mut m = UiModel::new(vec![], vec![]);
        m.apply(Event::Size { image: 300, size: (100, 100) });
        m.apply(Event::Size { image: 400, size: (100, 100) });
        m.apply(Event::Loaded { image: 10, path: "Art/Advisors/culture.pcx".into(), size: (100, 100) });
        m.apply(Event::Loaded { image: 11, path: "Art/SmallHeads/popupCULTURE.pcx".into(), size: (50, 50) });
        m.apply(Event::Sliced { sprite: 20, image: 10, rect: [0, 0, 100, 100] });
        m.apply(Event::Sliced { sprite: 21, image: 11, rect: [0, 0, 20, 20] });
        let redraw = |m: &mut UiModel| {
            m.apply(Event::Blit { sprite: 20, canvas: 300, x: 0, y: 0, clip: None, palette: None });
            m.apply(Event::Blit { sprite: 21, canvas: 300, x: 70, y: 2, clip: None, palette: None });
            m.apply(Event::Fill { canvas: 300, rect: [5, 5, 3, 3], rgb: [1, 2, 3], alpha: 255, clip: None });
            m.apply(Event::Fill { canvas: 300, rect: [30, 30, 5, 5], rgb: [0, 0, 0], alpha: 128, clip: None });
            m.apply(Event::Copy { from: 300, src: [0, 0, 100, 100], canvas: 400, x: 0, y: 0, keyed: false });
            m.apply(Event::Text { canvas: 400, run: text(10, 20, "Culture") });
            m.apply(Event::PaintBegin);
            m.apply(composite(400, (0, 0), (100, 100)));
            m.apply(Event::PaintEnd);
            m.frame().draws
        };
        let once = redraw(&mut m);
        let twice = redraw(&mut m);
        // The translucent fill is drawn as often as the game drew it (the
        // earlier one now under the background, as in the game).
        let shades = |d: &[ScreenDraw]| d.iter().filter(|d| matches!(d, ScreenDraw::Fill { alpha: 128, .. })).count();
        assert_eq!(shades(&once), 1);
        assert_eq!(shades(&twice), 2);
        // The pictures and the opaque fill once each, however often.
        for _ in 0..20 {
            redraw(&mut m);
        }
        let many = redraw(&mut m);
        let images = |d: &[ScreenDraw]| d.iter().filter(|d| matches!(d, ScreenDraw::Image { .. })).count();
        let opaque = |d: &[ScreenDraw]| d.iter().filter(|d| matches!(d, ScreenDraw::Fill { alpha: 255, .. })).count();
        assert_eq!((images(&once), images(&many)), (2, 2));
        assert_eq!((opaque(&once), opaque(&many)), (1, 1));
        // In the order they were last drawn: the background under the head.
        let paths: Vec<_> = many.iter().filter_map(|d| if let ScreenDraw::Image { path, .. } = d { Some(path.as_str()) } else { None }).collect();
        assert_eq!(paths, ["Art/Advisors/culture.pcx", "Art/SmallHeads/popupCULTURE.pcx"]);
        assert_eq!(many.iter().filter(|d| matches!(d, ScreenDraw::Text { .. })).count(), 1);
    }

    #[test]
    fn text_on_a_canvas_cleared_to_the_key_has_the_key_as_its_backdrop() {
        // The domestic advisor (F1): its canvas is cleared to the colour
        // key, its panels lie on the form below; its text goes on the key,
        // the speech bubble's on the bubble's picture.
        let mut m = UiModel::new(vec![], vec![]).with_text_backdrops(true);
        m.apply(Event::Size { image: 300, size: (200, 100) });
        m.apply(Event::Loaded { image: 10, path: "Art/Advisors/dialogbox.pcx".into(), size: (50, 30) });
        m.apply(Event::Sliced { sprite: 20, image: 10, rect: [0, 0, 50, 30] });
        m.apply(Event::Fill { canvas: 300, rect: [0, 0, 200, 100], rgb: KEY_RGB, alpha: 255, clip: None });
        m.apply(Event::Blit { sprite: 20, canvas: 300, x: 140, y: 10, clip: None, palette: None });
        m.apply(Event::Text { canvas: 300, run: text(10, 20, "Expenses: 2") });
        m.apply(Event::Text { canvas: 300, run: text(145, 25, "Build") });
        m.apply(Event::PaintBegin);
        m.apply(composite(300, (0, 0), (200, 100)));
        m.apply(Event::PaintEnd);
        let backdrops: Vec<_> = m.frame().draws.iter().filter_map(|d| if let ScreenDraw::Text { run, .. } = d { Some((run.text.clone(), run.backdrop)) } else { None }).collect();
        assert_eq!(backdrops, [("Expenses: 2".to_string(), Some(KEY_RGB)), ("Build".to_string(), None)]);
        // Once something opaque covers the canvas, text is on that.
        m.apply(Event::Fill { canvas: 300, rect: [0, 0, 200, 100], rgb: [9, 9, 9], alpha: 255, clip: None });
        m.apply(Event::Text { canvas: 300, run: text(10, 60, "Treasury") });
        m.apply(composite(300, (0, 0), (200, 100)));
        assert!(m.frame().draws.iter().all(|d| !matches!(d, ScreenDraw::Text { run, .. } if run.backdrop.is_some())));
        // Without backdrops (Wine): never.
        let mut w = UiModel::new(vec![], vec![]);
        w.apply(Event::Size { image: 300, size: (200, 100) });
        w.apply(Event::Fill { canvas: 300, rect: [0, 0, 200, 100], rgb: KEY_RGB, alpha: 255, clip: None });
        w.apply(Event::Text { canvas: 300, run: text(10, 20, "Expenses: 2") });
        w.apply(composite(300, (0, 0), (200, 100)));
        assert!(w.frame().draws.iter().all(|d| !matches!(d, ScreenDraw::Text { run, .. } if run.backdrop.is_some())));
    }

    #[test]
    fn rectangles_subtract_into_bands() {
        assert_eq!(subtract([0, 0, 10, 10], [20, 20, 30, 30]), vec![[0, 0, 10, 10]]);
        assert_eq!(subtract([0, 0, 10, 10], [-5, -5, 15, 15]), Vec::<[i32; 4]>::new());
        assert_eq!(subtract([0, 0, 10, 10], [2, 3, 4, 5]), vec![[0, 0, 10, 3], [0, 5, 10, 10], [0, 3, 2, 5], [4, 3, 10, 5]]);
        assert_eq!(subtract([0, 0, 10, 10], [0, 0, 10, 4]), vec![[0, 4, 10, 10]]);
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
