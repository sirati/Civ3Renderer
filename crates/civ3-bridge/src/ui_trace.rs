//! Feeds the UI mirror (`ui`) from hooks on the game's UI drawing (docs
//! §16.7): image loads, sprite slices, blits, copies, fills, lines, text and
//! the composition of forms onto the screen.

use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Relaxed;

use civ3_sys::{Civ3, layout as L};
use civ3_sys::ui_hooks::{self as h, obj};
use inproc::hook::HookError;

use crate::game_render::Skipper;
use crate::text::TextRun;
use crate::ui::{Event, Frame, UiModel, rgb555};

static MODEL: Mutex<Option<UiModel>> = Mutex::new(None);
/// The game's build, once the hooks are installed.
static BUILD: std::sync::OnceLock<civ3_sys::Build> = std::sync::OnceLock::new();

/// The back buffer (`PCX_Image` 0xCACFF0), once the hooks are installed.
static BACK_BUFFER: std::sync::OnceLock<u32> = std::sync::OnceLock::new();

/// Whether the game is to skip its own drawing (`crate::game_render`); set
/// before [`install`].
static GAME_DRAWS: AtomicBool = AtomicBool::new(true);
/// What is skipped, once the hooks are installed with the game not drawing
/// (none: everything is drawn).
static SKIPPER: Mutex<Option<Skipper>> = Mutex::new(None);
/// Whether [`SKIPPER`] is set (read without its lock).
static SKIPPING: AtomicBool = AtomicBool::new(false);

/// Whether the game draws itself (`CIV3R_GAME_RENDER`): if not, the hooks
/// installed afterwards skip the drawing they record.
pub fn set_game_draws(on: bool) {
    GAME_DRAWS.store(on, Relaxed);
}

/// Whether drawing is being skipped.
pub fn skipping() -> bool {
    SKIPPING.load(Relaxed)
}

/// Whether an image or sprite has pixels (its `JGL_Image*` / `JGLSprite*`
/// at +4): drawings without them do nothing (and return an error code).
fn has_pixels(object: u32) -> bool {
    inproc::Ptr32::<u32>::new(object.wrapping_add(4)).read().is_ok_and(|p| p != 0)
}

/// Whether the game's drawing onto `canvas` from `source` (0: none), just
/// recorded, is skipped: always run when one of them has no pixels (the
/// game's own early return, its result) or the canvas is kept.
///
/// Drawing onto images of palette indices goes on too (only 16-bit
/// canvases reach the screen; 8-bit images are what the game reads, e.g.
/// its maps of the regions under the mouse), and onto the canvases of
/// colour-keyed forms (the game reads them to tell whether a click lands
/// on the form: the HUD over the map).
fn skips(canvas: u32, source: u32) -> bool {
    if !skipping() || !has_pixels(canvas) || (source != 0 && !has_pixels(source)) {
        return false;
    }
    if obj::image_bits(canvas).is_none_or(|b| b <= 8) || obj::keyed_form_canvas(canvas) {
        return false;
    }
    SKIPPER.lock().ok().is_some_and(|mut s| s.as_mut().is_some_and(|s| s.skip(canvas)))
}

/// Runs the game's drawing `original` onto `canvas` from `source` unless
/// it is skipped (then: 0, the game's success).
fn draw(canvas: u32, source: u32, original: impl FnOnce() -> i32) -> i32 {
    if skips(canvas, source) { 0 } else { original() }
}

fn with_skipper<R>(f: impl FnOnce(&mut Skipper) -> R) -> Option<R> {
    if !skipping() {
        return None;
    }
    SKIPPER.lock().ok()?.as_mut().map(f)
}

/// Whether the text the game's text functions are writing now (`TextOutA`,
/// just recorded) is skipped: only text drawn through them (recorded with
/// its canvas) is.
pub fn skips_text() -> bool {
    let canvas = TEXT_CANVAS.with(Cell::get);
    canvas != 0 && skips(canvas, 0)
}

/// Whether a colour-table blit of `sprite` onto `canvas` (recorded as a
/// unit picture by [`on_colored`]) is skipped.
pub fn skips_colored(sprite: u32, canvas: u32) -> bool {
    h::flc_frame(sprite).is_some() && skips(canvas, sprite)
}

thread_local! {
    /// The canvas the game's text functions are drawing on (their `this`),
    /// for attributing `TextOutA` calls.
    static TEXT_CANVAS: Cell<u32> = const { Cell::new(0) };
    /// The rectangle of the screen being painted or presented (left, top,
    /// right, bottom), while it is.
    static DIRTY: Cell<Option<[i32; 4]>> = const { Cell::new(None) };
}

/// The clip rectangle of a draw onto `canvas` (see `h::canvas_clip`). On
/// the back buffer, the rectangle a paint or present is updating is no
/// clip of the content: the game clips the frames of framed forms (m80) and
/// the tooltip to it, but what it draws there is all of them, and which
/// part of the back buffer reaches the window is the model's to follow
/// (`UiModel::present`). Kept clipped, a paint of one button's rectangle
/// would leave a popup without its frame and a tooltip without its box.
fn clip_of(canvas: u32) -> Option<[i32; 4]> {
    let dirty = if BACK_BUFFER.get() == Some(&canvas) { DIRTY.with(Cell::get) } else { None };
    crate::ui::content_clip(h::canvas_clip(canvas), dirty)
}

thread_local! {
    /// How many paints (whole compositions of the screen) are running.
    static PAINTS: Cell<u32> = const { Cell::new(0) };
}

/// A paint running while this lives.
struct Painting;

impl Painting {
    fn enter() -> Painting {
        PAINTS.with(|p| p.set(p.get() + 1));
        Painting
    }

    fn inside() -> bool {
        PAINTS.with(Cell::get) > 0
    }
}

impl Drop for Painting {
    fn drop(&mut self) {
        PAINTS.with(|p| p.set(p.get().saturating_sub(1)));
    }
}

/// Runs `f` with `rect` (a `RECT*`, 0 for all of it) as the rectangle being
/// updated.
fn updating<R>(rect: u32, f: impl FnOnce() -> R) -> R {
    let r = if rect == 0 { None } else { obj::rect(rect) };
    let outer = DIRTY.with(|d| d.replace(r));
    let out = f();
    DIRTY.with(|d| d.set(outer));
    out
}

fn emit(e: Event) {
    if let Ok(mut m) = MODEL.lock() {
        if let Some(m) = m.as_mut() {
            m.apply(e);
        }
    }
}

/// The mirror model's size (see `UiModel::stats`).
pub fn stats() -> Option<(usize, usize, usize)> {
    MODEL.lock().ok()?.as_ref().map(|m| m.stats())
}

/// The screen as the game last showed it, if the mirror runs.
pub fn frame() -> Option<Frame> {
    MODEL.lock().ok()?.as_ref().map(|m| m.shown_frame())
}

/// Where the pixels of each of `sprites` (game `Sprite*`) come from, as the
/// mirror knows them (`None` for 0 and unknown ones).
pub fn sprite_sources(sprites: &[u32]) -> Vec<Option<crate::ui::Source>> {
    let model = MODEL.lock().ok();
    let model = model.as_ref().and_then(|m| m.as_ref());
    sprites.iter().map(|&s| model.filter(|_| s != 0).and_then(|m| m.sprite_source(s)).cloned()).collect()
}

/// Text drawn with `TextOutA` (canvas coordinates): part of the canvas the
/// text functions are drawing on, if any.
pub fn on_text(run: &TextRun) {
    let canvas = TEXT_CANVAS.with(Cell::get);
    if canvas != 0 {
        emit(Event::Text { canvas, run: run.clone() });
    }
}

fn read_c_string(at: u32) -> Option<String> {
    let bytes = inproc::Ptr32::<u8>::new(at).read_array(260).ok()?;
    let end = bytes.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

/// Tells the model a canvas' size before an operation that may cover it.
fn size_of(canvas: u32) -> Option<(i32, i32)> {
    let size = obj::image_size(canvas)?;
    emit(Event::Size { image: canvas, size });
    Some(size)
}

fn rect_xywh(at: u32) -> Option<[i32; 4]> {
    let [l, t, r, b] = obj::rect(at)?;
    Some([l, t, r - l, b - t])
}

/// A sprite drawn at (x, y): its stored pixels start at the crop offset.
/// `ct`: the colour table it is drawn through (0: its own palette).
fn blit(sprite: u32, canvas: u32, x: u32, y: u32, add_crop: bool, ct: u32) {
    let (cx, cy) = if add_crop { obj::sprite_crop(sprite).map_or((0, 0), |c| (c.0, c.1)) } else { (0, 0) };
    // A frame of an FLC animation in its own colours (a leader head).
    if let Some((flic, chunk)) = h::flc_frame(sprite) {
        emit(Event::FlcBlit { flic, chunk, canvas, x: x as i32 + cx, y: y as i32 + cy, clip: clip_of(canvas), plain: true });
        return;
    }
    // Through a civ colour table: that table's palette.
    let table = BUILD.get().and_then(|&b| h::civ_color_table(b, ct));
    if ct != 0 && table.is_none() {
        // (Once per table: which ones are not the civs'.)
        static SEEN: Mutex<Vec<u32>> = Mutex::new(Vec::new());
        if let Ok(mut seen) = SEEN.lock() {
            if !seen.contains(&ct) && seen.len() < 64 {
                seen.push(ct);
                log::debug!("sprite {sprite:#x} drawn through colour table {ct:#x}, not a civ's");
            }
        }
    }
    let ntp = |n: u32, count: u32| menu::screen::PaletteFrom { image: format!("Art/Units/Palettes/ntp{n:02}.pcx"), first: 0, count };
    // Else a sprite of its own colour table, recoloured by copying a civ's
    // first 64 colours into it.
    let palette = table.map(|n| ntp(n, 256)).or_else(|| own_civ_colors(sprite).map(|n| ntp(n, 64)));
    emit(Event::Blit { sprite, canvas, x: x as i32 + cx, y: y as i32 + cy, clip: clip_of(canvas), palette });
}

/// Which civ's colours `sprite`'s own colour table holds (its first 64), if
/// it has one and they are a civ's; else those of the palette it was cut
/// with.
fn own_civ_colors(sprite: u32) -> Option<u32> {
    // The civ tables do not change once loaded.
    static CIVS: std::sync::OnceLock<Vec<Vec<u32>>> = std::sync::OnceLock::new();
    let civs = match CIVS.get() {
        Some(c) => c,
        None => {
            let loaded = h::civ_table_colors(*BUILD.get()?)?;
            CIVS.get_or_init(|| loaded)
        }
    };
    let civ = |colors: Vec<u32>| civs.iter().position(|c| *c == colors).map(|n| n as u32);
    h::sprite_own_colors(sprite).and_then(civ).or_else(|| h::sprite_cut_colors(sprite).and_then(civ))
}

/// Replay graphics live on a worker, separate from the game's hook thread.
static REPLAY: std::sync::OnceLock<crate::replay_thread::ReplayThread> = std::sync::OnceLock::new();
static REPLAY_NAVIGATOR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
// Navigator initialization itself calls political-map operations. Replay
// starts from terrain only, then records the history operations afterwards.
static REPLAY_READY: AtomicBool = AtomicBool::new(false);

pub fn replay_revision() -> u64 {
    REPLAY.get().map_or(0, |replay| replay.revision())
}

/// Pixels and revision from the same worker publication. Copying the Arc
/// is brief; conversion and image updates never hold a game-thread lock.
pub fn replay_picture() -> crate::replay_thread::Picture {
    REPLAY.get().map(|replay| replay.picture()).unwrap_or_default()
}

fn replay_operation(navigator: u32, operation: crate::replay::Operation) -> bool {
    if navigator == 0 || navigator != REPLAY_NAVIGATOR.load(Relaxed) { return false; }
    // Initialization's live-ownership paints are ignored. Their original
    // rasterizers are still skipped; only later history enters our map.
    if REPLAY_READY.load(Relaxed) && !REPLAY.get().is_some_and(|replay| replay.apply(operation)) {
        static FAILED: AtomicBool = AtomicBool::new(false);
        if !FAILED.swap(true, Relaxed) { log::error!("UI mirror: replay worker is no longer receiving operations"); }
    }
    true
}

/// The forms' own surfaces seen so far (`h::form_surface`): image and name.
static FORM_SURFACES: Mutex<Vec<(u32, &'static str)>> = Mutex::new(Vec::new());

/// The surfaces the game fills itself: the fixed ones and the forms' own
/// seen so far (see `ui_screen::items`).
pub fn generated_surfaces(build: civ3_sys::Build) -> Vec<(u32, &'static str)> {
    let mut out = h::generated_surfaces(build).to_vec();
    out.extend(form_surfaces());
    out
}

/// The forms' own surfaces seen so far: image and name.
pub fn form_surfaces() -> Vec<(u32, &'static str)> {
    FORM_SURFACES.lock().map(|s| s.clone()).unwrap_or_default()
}

/// A copy of `from` onto `canvas`: if `from` is a surface of the form
/// whose canvas it is, the model passes it on as it is from now on.
fn note_form_surface(from: u32, canvas: u32) {
    let Some(&build) = BUILD.get() else { return };
    if BACK_BUFFER.get() == Some(&canvas) {
        return;
    }
    let Some(s) = h::form_surface(build, from, canvas) else { return };
    {
        let Ok(mut seen) = FORM_SURFACES.lock() else { return };
        if seen.iter().any(|&(image, _)| image == from) {
            return;
        }
        seen.push((from, s.name));
    }
    log::debug!("UI mirror: {} at {from:#x}", s.name);
    if let Ok(mut m) = MODEL.lock() {
        if let Some(m) = m.as_mut() {
            m.add_generated(from);
        }
    }
}

/// `image` lost its pixels: if it was a form's surface, it is an image like
/// any other again (its memory may be another's next).
fn forget_form_surface(image: u32) {
    {
        let Ok(mut seen) = FORM_SURFACES.lock() else { return };
        let Some(i) = seen.iter().position(|&(i, _)| i == image) else { return };
        seen.remove(i);
    }
    if let Ok(mut m) = MODEL.lock() {
        if let Some(m) = m.as_mut() {
            m.remove_generated(image);
        }
    }
}

/// Region `src` (x, y, w, h) of `from` copied to (x, y) of `canvas`, within
/// the canvas's clip rectangle (the game's copies keep to it: e.g. a list
/// box redrawing one row through tiles of its background).
fn copy(from: u32, src: [i32; 4], canvas: u32, x: i32, y: i32, keyed: bool) {
    note_form_surface(from, canvas);
    let (mut src, mut x, mut y) = (src, x, y);
    if let Some([l, t, r, b]) = clip_of(canvas) {
        let (dl, dt) = ((l - x).max(0), (t - y).max(0));
        let (w, hh) = ((src[2] - dl).min(r - x - dl), (src[3] - dt).min(b - y - dt));
        if w <= 0 || hh <= 0 {
            return;
        }
        src = [src[0] + dl, src[1] + dt, w, hh];
        (x, y) = (x + dl, y + dt);
    }
    emit(Event::Copy { from, src, canvas, x, y, keyed });
}

fn fill(canvas: u32, rect: [i32; 4], color: u32) {
    size_of(canvas);
    emit(Event::Fill { canvas, rect, rgb: rgb555(color), alpha: 255, clip: clip_of(canvas) });
}

/// Records the initial content before the form draws into a new canvas.
fn initialized_canvas(image: u32, result: i32) -> i32 {
    // Software JGL canvases allocate zeroed DIBs. Opaque copies include
    // unwritten black pixels, for example around a centered picture.
    // Palette-index images have different zero colours and stay excluded.
    if result == 0 && obj::image_bits(image) == Some(16) {
        if let Some((width, height)) = size_of(image) {
            emit(Event::Fill { canvas: image, rect: [0, 0, width, height], rgb: [0, 0, 0], alpha: 255, clip: None });
        }
    }
    result
}

/// The hooks; they stay installed while this lives.
pub struct UiTracer {
    _hooks: Vec<Box<dyn std::any::Any + Send>>,
}

/// Installs the hooks. `skip_canvases`: canvases left out while a game runs
/// (we draw what they show); `underlays`: canvases under the interface whose
/// own content we draw, mirrored without the art in `underlay_art` (§16).
pub fn install(game: &Civ3, skip_canvases: Vec<u32>, underlays: Vec<u32>, underlay_art: Vec<String>) -> Result<UiTracer, HookError> {
    let sites = h::ui_sites(game.build).ok_or(HookError::Unsupported)?;
    let _ = BUILD.set(game.build);
    let _ = BACK_BUFFER.set(h::back_buffer(game.build).unwrap_or(0));
    *MODEL.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(
            UiModel::new(Vec::new(), skip_canvases)
                .with_screen(h::back_buffer(game.build).unwrap_or(0))
                .with_generated(h::generated_surfaces(game.build).iter().map(|g| g.0).collect())
                .with_underlays(underlays, underlay_art)
                .with_text_backdrops(!gdi_text::under_wine()),
        );
    let mut hooks: Vec<Box<dyn std::any::Any + Send>> = Vec::new();
    macro_rules! hook {
        ($module:ident, $site:expr, $handler:expr) => {
            hooks.push(Box::new(h::$module::install_at_start($site.0, $site.1, $handler)?));
        };
    }

    hook!(pcx_read_file, sites.pcx_read_file, |c| {
        let path = read_c_string(c.path);
        let r = c.original();
        if r == 0 {
            with_skipper(|s| s.loaded(c.image));
            if let (Some(path), Some(size)) = (path, obj::image_size(c.image)) {
                emit(Event::Loaded { image: c.image, path, size });
            }
        }
        r
    });
    hook!(pcx_clear, sites.pcx_clear, |c| {
        forget_form_surface(c.image);
        emit(Event::Cleared { image: c.image });
        with_skipper(|s| s.cleared(c.image));
        c.original()
    });
    hooks.push(Box::new(h::pcx_create::install_call_site(sites.form_canvas_create.0, sites.form_canvas_create.1,
        |c| initialized_canvas(c.image, c.original()))?));
    hooks.push(Box::new(h::pcx_create_sized::install_call_site(sites.form_canvas_size.0, sites.form_canvas_size.1,
        |c| initialized_canvas(c.image, c.original()))?));
    hooks.push(Box::new(h::pcx_create_resized::install_call_site(sites.form_canvas_recreate.0, sites.form_canvas_recreate.1,
        |c| initialized_canvas(c.image, c.original()))?));
    hook!(sprite_slice, sites.sprite_slice, |c| {
        // The game takes these pixels: drawing onto an image it made
        // itself goes on from now on.
        if with_skipper(|s| s.reads(c.image)) == Some(true) {
            log::warn!("game rendering: a sprite cut from image {:#x}, drawn onto with drawing skipped; drawing onto it from now on", c.image);
        }
        let r = c.original();
        if obj::sprite_has_pixels(c.sprite) {
            if let Some((cx, cy, w, hh)) = obj::sprite_crop(c.sprite) {
                let rect = [c.x as i32 + cx, c.y as i32 + cy, w, hh];
                emit(Event::Sliced { sprite: c.sprite, image: c.image, rect });
            }
        }
        r
    });
    hook!(sprite_copy, sites.sprite_copy, |c| {
        let r = c.original();
        emit(Event::Copied { sprite: c.sprite, source: c.source });
        r
    });
    hook!(sprite_reset, sites.sprite_reset, |c| {
        emit(Event::Reset { sprite: c.sprite });
        c.original()
    });
    // The game's mouse cursor, which it draws onto the screen last (docs
    // §16.5, §16.11), is not mirrored: the viewer shows the pointer itself,
    // where the mouse is now (a mirrored one would lag behind it).
    let cursor_sprite = civ3_sys::cursor::sites(game.build).map(|s| s.sprite);
    let screen = h::back_buffer(game.build).unwrap_or(0);
    let is_cursor = move |sprite: u32, canvas: u32| {
        canvas == screen && sprite != 0 && cursor_sprite.is_some_and(|at| inproc::Ptr32::<u32>::new(at).read().ok() == Some(sprite))
    };
    // A sprite read from a file at every draw (its pixels are 0 then)
    // draws through `read_file` and `draw_onto`, both hooked: it runs.
    hook!(sprite_draw, sites.sprite_draw, move |c| {
        if !is_cursor(c.sprite, c.canvas) {
            blit(c.sprite, c.canvas, c.x, c.y, true, c.ct);
        }
        draw(c.canvas, c.sprite, || c.original())
    });
    hook!(sprite_draw_variant, sites.sprite_draw_variant, |c| {
        blit(c.sprite, c.canvas, c.x, c.y, true, c.ct);
        draw(c.canvas, c.sprite, || c.original())
    });
    hook!(sprite_draw_a4, sites.sprite_draw_a4, |c| {
        blit(c.sprite, c.canvas, c.x, c.y, true, c.ct);
        draw(c.canvas, c.sprite, || c.original())
    });
    hook!(sprite_draw_a4b, sites.sprite_draw_a4b, |c| {
        blit(c.sprite, c.canvas, c.x, c.y, true, c.ct);
        draw(c.canvas, c.sprite, || c.original())
    });
    hook!(sprite_draw_opacity, sites.sprite_draw_opacity, |c| {
        let opacity = crate::ui::sprite_opacity(c.opacity);
        let (cx, cy) = obj::sprite_crop(c.sprite).map_or((0, 0), |p| (p.0, p.1));
        let palette = BUILD.get().and_then(|&b| h::civ_color_table(b, c.ct)).map(|n| menu::screen::PaletteFrom {
            image: format!("Art/Units/Palettes/ntp{n:02}.pcx"), first: 0, count: 256,
        });
        emit(Event::OpacityBlit { sprite: c.sprite, canvas: c.canvas, x: c.x as i32 + cx, y: c.y as i32 + cy, clip: clip_of(c.canvas), palette, opacity });
        draw(c.canvas, c.sprite, || c.original())
    });
    hook!(sprite_draw_alpha, sites.sprite_draw_alpha, |c| {
        // No crop offset: the stored pixels go to (x, y).
        emit(Event::MaskedBlit { sprite: c.sprite, mask: c.mask, canvas: c.dest, x: c.x as i32, y: c.y as i32, clip: clip_of(c.dest) });
        draw(c.dest, c.sprite, || c.original())
    });
    hook!(pcx_draw_onto, sites.pcx_draw_onto, |c| {
        if let Some((w, hh)) = obj::image_size(c.image) {
            copy(c.image, [0, 0, w, hh], c.dst, c.x as i32, c.y as i32, true);
        }
        draw(c.dst, c.image, || c.original())
    });
    hook!(pcx_copy, sites.pcx_copy, |c| {
        size_of(c.dst);
        size_of(c.image);
        let src = [c.sx as i32, c.sy as i32, c.sw as i32, c.sh as i32];
        copy(c.image, src, c.dst, c.dx as i32, c.dy as i32, false);
        draw(c.dst, c.image, || c.original())
    });
    hook!(pcx_copy_same, sites.pcx_copy_same, |c| {
        size_of(c.dst);
        size_of(c.image);
        let src = [c.x as i32, c.y as i32, c.w as i32, c.h as i32];
        copy(c.image, src, c.dst, src[0], src[1], false);
        draw(c.dst, c.image, || c.original())
    });
    hook!(pcx_copy_rect, sites.pcx_copy_rect, |c| {
        size_of(c.dst);
        size_of(c.image);
        if let Some(src) = rect_xywh(c.rect) {
            copy(c.image, src, c.dst, src[0], src[1], false);
        }
        draw(c.dst, c.image, || c.original())
    });
    hook!(pcx_copy_rects, sites.pcx_copy_rects, |c| {
        size_of(c.dst);
        size_of(c.image);
        if let (Some(src), Some(dst)) = (rect_xywh(c.src_rect), rect_xywh(c.dst_rect)) {
            copy(c.image, src, c.dst, dst[0], dst[1], false);
        }
        draw(c.dst, c.image, || c.original())
    });
    // The game reads a pixel (what is under the mouse): drawing onto that
    // image goes on from now on (see `skips` for the ones known before).
    hook!(pcx_get_pixel, sites.pcx_get_pixel, |c| {
        if with_skipper(|s| s.reads(c.image)) == Some(true) {
            log::warn!("game rendering: the game read a pixel of image {:#x}, drawn onto with drawing skipped; drawing onto it from now on", c.image);
        }
        c.original()
    });
    hook!(pcx_fill, sites.pcx_fill, |c| {
        if let Some((w, hh)) = obj::image_size(c.image) {
            fill(c.image, [0, 0, w, hh], c.color);
        }
        draw(c.image, 0, || c.original())
    });
    hook!(pcx_fill_area, sites.pcx_fill_area, |c| {
        if let Some(rect) = rect_xywh(c.rect) {
            fill(c.image, rect, c.color);
        }
        draw(c.image, 0, || c.original())
    });
    hook!(pcx_hline, sites.pcx_hline, |c| {
        let (x1, x2) = (c.x1 as i32, c.x2 as i32);
        fill(c.image, [x1.min(x2), c.y as i32, (x2 - x1).abs() + 1, 1], c.color);
        draw(c.image, 0, || c.original())
    });
    hook!(pcx_vline, sites.pcx_vline, |c| {
        let (y1, y2) = (c.y1 as i32, c.y2 as i32);
        fill(c.image, [c.x as i32, y1.min(y2), 1, (y2 - y1).abs() + 1], c.color);
        draw(c.image, 0, || c.original())
    });
    hook!(pcx_line, sites.pcx_line, |c| {
        for (x, y) in line_points((c.x1 as i32, c.y1 as i32), (c.x2 as i32, c.y2 as i32)) {
            fill(c.image, [x, y, 1, 1], c.color);
        }
        draw(c.image, 0, || c.original())
    });
    hook!(form_composite, sites.form_composite, |c| {
        if let Some(p) = obj::placement(c.form) {
            emit(Event::Composite { canvas: p.canvas, vtable: p.vtable, origin: p.origin, size: p.size, clip: p.clip, keyed: p.keyed });
        }
        c.original();
        emit(Event::CompositeEnd);
    });
    hook!(paint, sites.paint, |c| {
        emit(Event::PaintBegin);
        let _painting = Painting::enter();
        updating(c.rect, || c.original());
        emit(Event::PaintEnd);
    });
    // Every form composed outside a paint is a paint too: a screen with
    // a loop of its own (the victory screen at the end of a game) composes
    // and presents itself, and its form is on the screen only that way.
    // (Asked for one form's part, it composes all of them unless the draw
    // list starts at a modal form.)
    let modal_root = h::modal_draw_root(game.build);
    hook!(compose_all, sites.compose_all, move |c| {
        let all = c.form == 0 || modal_root.is_some_and(|at| inproc::Ptr32::<u32>::new(at).read().ok() == Some(0));
        if !all || Painting::inside() {
            return c.original();
        }
        emit(Event::PaintBegin);
        let _painting = Painting::enter();
        updating(c.rect, || c.original());
        emit(Event::PaintEnd);
    });
    let back_buffer = h::back_buffer(game.build).unwrap_or(0);
    hook!(present, sites.present, move |c| {
        // It draws the tooltip and the cursor onto the back buffer first.
        emit(Event::PresentBegin);
        updating(c.rect, || c.original());
        size_of(back_buffer);
        emit(Event::Present { rect: if c.rect == 0 { None } else { obj::rect(c.rect) } });
    });
    hook!(text_worker, sites.text_worker, |c| with_text_canvas(c.image, || c.original()));
    hook!(text_draw, sites.text_draw, |c| with_text_canvas(c.image, || c.original()));
    hook!(text_draw_centered, sites.text_draw_centered, |c| with_text_canvas(c.image, || c.original()));
    hook!(text_draw_rect, sites.text_draw_rect, |c| with_text_canvas(c.image, || c.original()));
    hook!(sprite_draw_masked, sites.sprite_draw_masked, |c| {
        emit(Event::MaskedBlit { sprite: c.sprite, mask: c.mask, canvas: c.dest, x: c.x as i32, y: c.y as i32, clip: clip_of(c.dest) });
        draw(c.dest, c.sprite, || c.original())
    });
    if let Some(site) = h::flc_load_site(game.build) {
        hook!(flc_load, site, |c| {
            let path = read_c_string(c.path);
            let r = c.original();
            if let (0, Some(path)) = (r, path) {
                emit(Event::FlcLoaded { flic: c.flic, path });
            }
            r
        });
    }
    if let Some(sites) = h::replay_sites(game.build) {
        let game = *game;
        match crate::replay_thread::ReplayThread::start(move || {
            // Replay needs only terrain and geometry, without live units,
            // diplomacy, fonts, districts or other full-scene state.
            let map = game.map().map_err(|e| e.to_string())?;
            let tiles = game.tiles(&map).map_err(|e| e.to_string())?.iter()
                .map(|tile| crate::world::Civ3Tile { square_type: tile.square_type(), ..Default::default() }).collect();
            Ok(crate::world::Civ3World {
                width: map.width, height: map.height, flags: map.flags,
                wrap_x: map.wrap_x, wrap_y: map.wrap_y, tiles,
                ..Default::default()
            })
        }) {
            Ok(worker) => { let _ = REPLAY.set(worker); }
            Err(e) => log::error!("UI mirror: cannot start replay worker: {e}"),
        }
        hook!(navigator_init, sites.init, move |c| {
            let replay = h::replay_navigator(game.build, c.navigator);
            if replay {
                REPLAY_READY.store(false, Relaxed);
                REPLAY_NAVIGATOR.store(c.navigator, Relaxed);
                if let Some(worker) = REPLAY.get() { worker.clear(); }
            } else if REPLAY_NAVIGATOR.load(Relaxed) == c.navigator {
                // A freed replay form's memory can become another navigator.
                REPLAY_READY.store(false, Relaxed);
                REPLAY_NAVIGATOR.store(0, Relaxed);
                if let Some(worker) = REPLAY.get() { worker.clear(); }
            }
            // Geometry, hit testing and replay state still initialize.
            // Only the terrain rasterizer below is skipped in render-off mode.
            c.original();
            if replay {
                let ready = h::navigator_size(c.navigator)
                    .is_some_and(|size| REPLAY.get().is_some_and(|worker| worker.init(size)));
                REPLAY_READY.store(ready, Relaxed);
                if !ready { log::warn!("UI mirror: replay dimensions or worker unavailable"); }
            }
        });
        hook!(navigator_terrain, sites.terrain, |c| {
            let nav = REPLAY_NAVIGATOR.load(Relaxed);
            let replay_terrain = nav != 0 && c.image == nav.wrapping_add(L::NAVIGATOR_DATA_PCX_1)
                && BUILD.get().is_some_and(|&build| h::replay_navigator(build, nav));
            if !(skipping() && replay_terrain) { c.original(); }
        });
        hook!(navigator_tint, sites.tint, |c| {
            let tracked = replay_operation(c.navigator, crate::replay::Operation::Tint { x: c.x as i32, y: c.y as i32, color: c.color as u16 });
            if !(tracked && skipping()) { c.original(); }
        });
        hook!(navigator_restore, sites.restore, |c| {
            let tracked = replay_operation(c.navigator, crate::replay::Operation::Restore { x: c.x as i32, y: c.y as i32 });
            if !(tracked && skipping()) { c.original(); }
        });
        hook!(navigator_add_city, sites.add_city, |c| {
            let tracked = replay_operation(c.navigator, crate::replay::Operation::AddCity { x: c.x as i32, y: c.y as i32 });
            if !(tracked && skipping()) { c.original(); }
        });
        hook!(navigator_erase_city, sites.erase_city, |c| {
            let tracked = replay_operation(c.navigator, crate::replay::Operation::EraseCity { x: c.x as i32, y: c.y as i32 });
            if !(tracked && skipping()) { c.original(); }
        });
    }
    hook_lines(game, &mut hooks);
    if !GAME_DRAWS.load(Relaxed) {
        *SKIPPER.lock().unwrap_or_else(|e| e.into_inner()) = Some(Skipper::new(h::read_back_images(game.build)));
        SKIPPING.store(true, Relaxed);
        log::info!("game rendering off: the game's drawing is recorded, not done");
    }
    hook_jgl_later(std::env::var("CIV3R_TRACE_PIXELS").is_ok_and(|v| v == "1"));
    log::info!("UI mirror: {} hooks", hooks.len());
    Ok(UiTracer { _hooks: hooks })
}


/// Hooks jgl.dll once the game has loaded it (the hooks stay for the life
/// of the process): its translucent rectangle fill, its window update (to
/// skip it while the game does not draw) and, with `trace_pixels`, the
/// handing out of image pixels (see [`trace_pixel_access`]).
fn hook_jgl_later(trace_pixels: bool) {
    std::thread::spawn(move || {
        for _ in 0..600 {
            if let Some(base) = inproc::pe::module_base("jgl.dll") {
                let (slot, expected) = h::jgl_shade_slot(base);
                if slot.read().ok() != Some(expected) {
                    return log::warn!("UI mirror: unknown jgl.dll, translucent fills not mirrored");
                }
                match h::jgl_shade::install_slot(slot, |c| {
                    let recorded = shade(c);
                    match recorded {
                        Some(canvas) if skips(canvas, 0) => 0,
                        _ => c.original(),
                    }
                }) {
                    Ok(installed) => std::mem::forget(installed),
                    Err(e) => log::warn!("UI mirror: hooking translucent fills: {e}"),
                }
                if skipping() {
                    let (slot, expected) = h::jgl_present_slot(base);
                    if slot.read().ok() != Some(expected) {
                        log::warn!("game rendering: unknown jgl.dll, the window is still updated");
                    } else {
                        match h::jgl_present::install_slot(slot, |c| if skipping() { 0 } else { c.original() }) {
                            Ok(installed) => std::mem::forget(installed),
                            Err(e) => log::warn!("game rendering: hooking the window update: {e}"),
                        }
                    }
                }
                if trace_pixels {
                    trace_pixel_access(base);
                }
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    });
}

/// Logs (once per image) who takes the pixels of an image that a skipped
/// drawing went onto: the game reading back what it did not draw, or
/// drawing over it with a drawing not mirrored (`CIV3R_TRACE_PIXELS=1`,
/// for finding such readers; see `skips` for the ones kept drawing). The
/// caller is the first return address into the game's code on the stack
/// (possibly a stale one).
fn trace_pixel_access(base: u32) {
    static SEEN: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    fn note(image: u32) {
        let Some(owner) = h::jgl_owner(image) else { return };
        if with_skipper(|s| s.was_skipped(owner) && !s.keeps(owner)) != Some(true) {
            return;
        }
        // Only the first time (the stack walk is slow).
        match SEEN.lock() {
            Ok(mut seen) if seen.len() < 256 && !seen.contains(&owner) => seen.push(owner),
            _ => return,
        }
        let caller = game_caller();
        log::warn!("game rendering: pixels of image {owner:#x} (drawn onto with drawing skipped) taken from {caller:#x}");
    }
    let [(pixel_at, at_fn), (lock, lock_fn)] = h::jgl_pixel_slots(base);
    if pixel_at.read().ok() != Some(at_fn) || lock.read().ok() != Some(lock_fn) {
        return log::warn!("pixel trace: unknown jgl.dll");
    }
    let r1 = h::jgl_pixel_at::install_slot(pixel_at, |c| {
        note(c.image);
        c.original()
    });
    let r2 = h::jgl_lock::install_slot(lock, |c| {
        note(c.image);
        c.original()
    });
    match (r1, r2) {
        (Ok(a), Ok(b)) => {
            std::mem::forget((a, b));
            log::info!("pixel trace: on");
        }
        (Err(e), _) | (_, Err(e)) => log::warn!("pixel trace: {e}"),
    }
}

/// The first return address into the game's exe found on this thread's
/// stack (scanning up from here), or 0.
fn game_caller() -> u32 {
    let here = 0u32;
    let sp = &here as *const u32 as usize as u32;
    // As far as the stack goes (its top may be nearer than the most asked for).
    let Some(words) = [1024, 256, 64].into_iter().find_map(|n| inproc::Ptr32::<u32>::new(sp).read_array(n).ok()) else { return 0 };
    let code = |a: u32, n: usize| inproc::Ptr32::<u8>::new(a).read_array(n).ok();
    words
        .into_iter()
        .find(|&ra| {
            (0x40_1000..0x66_5000).contains(&ra)
                && code(ra.wrapping_sub(6), 6).is_some_and(|b| b[1] == 0xE8 || b[0] == 0xFF || b[3] == 0xFF || b[4] == 0xFF)
        })
        .unwrap_or(0)
}

/// `shade_rect` (see [`crate::ui::shade_alpha`]); the canvas, if recorded
/// (or nothing to draw).
fn shade(c: &h::jgl_shade::Call) -> Option<u32> {
    let canvas = h::jgl_owner(c.image)?;
    // Palette colours are not used by the interface code we know of.
    if c.color & 0x8000_0000 == 0 {
        return None;
    }
    let rect = if c.rect == 0 { obj::image_size(canvas).map(|(w, hh)| [0, 0, w, hh]) } else { rect_xywh(c.rect) };
    let rect = rect?;
    let alpha = crate::ui::shade_alpha((c.percent as i32).clamp(0, 100) as u32);
    if alpha != 0 {
        emit(Event::Fill { canvas, rect, rgb: rgb555(c.color), alpha, clip: clip_of(canvas) });
    }
    Some(canvas)
}

/// What a line drawer is set to: canvas, colour, opacity, width.
#[derive(Copy, Clone)]
struct Pen {
    canvas: u32,
    rgb: [u8; 3],
    alpha: u8,
    width: i32,
}

static PENS: Mutex<Vec<(u32, Pen)>> = Mutex::new(Vec::new());

fn with_pen(renderer: u32, f: impl FnOnce(&mut Pen)) {
    if let Ok(mut pens) = PENS.lock() {
        let i = match pens.iter().position(|p| p.0 == renderer) {
            Some(i) => i,
            None => {
                pens.push((renderer, Pen { canvas: 0, rgb: [0; 3], alpha: 255, width: 1 }));
                pens.len() - 1
            }
        };
        f(&mut pens[i].1);
    }
}

/// Hooks the game's line drawer (redirected by C3X: hooked over its jump).
fn hook_lines(game: &Civ3, hooks: &mut Vec<Box<dyn std::any::Any + Send>>) {
    let Some([bind, color, opacity, width, draw]) = h::line_sites(game.build) else { return };
    let mut install = |r: Result<Box<dyn std::any::Any + Send>, HookError>| match r {
        Ok(hook) => hooks.push(hook),
        Err(e) => log::warn!("UI mirror: line drawer not mirrored: {e}"),
    };
    install(h::line_bind::install_over_jmp(bind, |c| {
        with_pen(c.renderer, |p| p.canvas = c.canvas);
        c.original()
    }).map(|i| Box::new(i) as _));
    install(h::line_color::install_over_jmp(color, |c| {
        with_pen(c.renderer, |p| p.rgb = rgb555(c.rgb555));
        c.original()
    }).map(|i| Box::new(i) as _));
    install(h::line_opacity::install_over_jmp(opacity, |c| {
        with_pen(c.renderer, |p| p.alpha = c.alpha.min(255) as u8);
        c.original()
    }).map(|i| Box::new(i) as _));
    install(h::line_width::install_over_jmp(width, |c| {
        with_pen(c.renderer, |p| p.width = (c.width as i32).clamp(1, 16));
        c.original()
    }).map(|i| Box::new(i) as _));
    install(h::line_draw::install_over_jmp(draw, |c| {
        let mut pen = None;
        with_pen(c.renderer, |p| pen = Some(*p));
        let Some(p) = pen.filter(|p| p.canvas != 0) else { return c.original() };
        if p.alpha > 0 {
            let (x1, y1, x2, y2) = (c.x1 as i32, c.y1 as i32, c.x2 as i32, c.y2 as i32);
            for (rect, coverage) in crate::ui::line_fills((x1, y1), (x2, y2), p.width, crate::ui::smooth_lines()) {
                let alpha = crate::ui::covered_alpha(p.alpha, coverage);
                emit(Event::Fill { canvas: p.canvas, rect, rgb: p.rgb, alpha, clip: clip_of(p.canvas) });
            }
        }
        if !skips(p.canvas, 0) {
            c.original()
        }
    }).map(|i| Box::new(i) as _));
}
fn with_text_canvas<R>(canvas: u32, f: impl FnOnce() -> R) -> R {
    let outer = TEXT_CANVAS.with(|c| c.replace(canvas));
    let r = f();
    TEXT_CANVAS.with(|c| c.set(outer));
    r
}

/// The pixels of a 1-pixel line, endpoints included (Bresenham).
fn line_points((x0, y0): (i32, i32), (x1, y1): (i32, i32)) -> Vec<(i32, i32)> {
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

/// Whether the skipped canvases (the map) are left out: while a game runs.
/// Between games the title screen is drawn on the same canvas.
pub fn set_skipping(on: bool) {
    if let Ok(mut m) = MODEL.lock() {
        if let Some(m) = m.as_mut() {
            m.set_skipping(on);
        }
    }
}

/// The game starts redrawing the map (terrain over the old labels) into
/// `map_renderer`, an image it then copies onto the map canvas: what both
/// showed is replaced.
pub fn map_redraw(map_renderer: u32) {
    if let Ok(mut m) = MODEL.lock() {
        if let Some(m) = m.as_mut() {
            m.reset_underlays();
            m.reset(map_renderer);
        }
    }
}

/// An unscaled map-sprite blit onto an interface canvas (Top 5 Cities).
/// The tracer excludes the main map pass, which the renderer draws itself.
/// Returns whether the game's own drawing of this recorded blit is skipped.
pub fn on_map_sprite(sprite: u32, canvas: u32, x: u32, y: u32, ct: u32) -> bool {
    blit(sprite, canvas, x, y, true, ct);
    skips(canvas, sprite)
}

/// A colour-table blit (the tracer's hook): unit animation frames drawn on
/// interface canvases become unit pictures (§16.9). `canvas` is the
/// destination (the blit's second canvas argument).
pub fn on_colored(sprite: u32, canvas: u32, x: u32, y: u32) {
    if let Some((flic, chunk)) = h::flc_frame(sprite) {
        // The frame's image lies at its offset from (x, y), as for map
        // units (§10.3).
        let (cx, cy) = obj::sprite_crop(sprite).map_or((0, 0), |c| (c.0, c.1));
        emit(Event::FlcBlit { flic, chunk, canvas, x: x as i32 + cx, y: y as i32 + cy, clip: clip_of(canvas), plain: false });
    }
}
