//! The jgl takeover, wired to the bridge (`docs/jgl-replacement.md`): the
//! method groups `jgl-ffi` takes over in jgl.dll's tables, the hooks they
//! report to, and the counting mode.
//!
//! Groups taken over so far: text, fills (lines, shades), copies, sprites
//! (blits, their state, colour tables) and the display modes. Text runs go
//! to the interface mirror (`ui_trace::on_text`) and to the recent-text
//! list (`trace`); fills and copies to the interface mirror
//! (`ui_trace::on_fill`, `ui_trace::on_copy`); blits, named by the exe
//! function that made them (`civ3_ui::blit`), to the interface mirror and
//! the map tracer (`ui_trace::on_blit`). The display modes need no hooks: the game's
//! screen is the mode it asks for, and the display is never switched, not
//! by a movie nor by jgl's shutdown either (`jgl::display`).

use civ3_ui::blit::{BlitCapture, GameBlit};
use civ3_ui::draw::{CanvasDraw, CopyCapture, FillCapture};
use civ3_ui::text::{CanvasText, TextCapture};

/// The text group goes in before jgl's first draw: from the `LoadLibraryA`
/// callback, or for a module already loaded, from the bridge's setup. Two
/// loads at once both return with it in place.
static TEXT: inproc::loader::InstallOnce = inproc::loader::InstallOnce::new();

fn canvas_text(t: CanvasText) {
    crate::ui_trace::audit_semantic(crate::ui_trace::SemanticClass::TextRun);
    crate::ui_trace::on_text(t.canvas, t.clip, &t.run);
    crate::trace::recent_text(t.run);
}

fn canvas_draw(d: CanvasDraw) {
    match d {
        CanvasDraw::Fill { canvas, size, rects, rgb, alpha, clip } => crate::ui_trace::on_fill(canvas, size, &rects, rgb, alpha, clip),
        CanvasDraw::Copy { from, src, canvas, x, y, keyed, sizes, clip } => crate::ui_trace::on_copy(from, src, canvas, x, y, keyed, sizes, clip),
    }
}

/// A blit an exe wrapper made, to the interface mirror and the map tracer.
fn game_blit(b: &GameBlit, _call: &jgl::BlitCall) {
    crate::ui_trace::on_blit(b);
}

/// Whether jgl still draws a call's pixels: on canvases the game reads
/// back (the mirror's keep set), and calls the interface mirror does not
/// take.
struct Pixels;

impl jgl::PixelPolicy for Pixels {
    fn original_pixels(&self, _image: jgl::ImageId, owner: u32) -> bool {
        !crate::ui_trace::skips_text_on(owner)
    }

    fn original_fill(&self, call: &jgl::FillCall) -> bool {
        civ3_ui::draw::mirrored_fill(call).is_none_or(|canvas| !crate::ui_trace::skips_drawing(canvas, 0))
    }

    fn original_copy(&self, call: &jgl::CopyCall) -> bool {
        civ3_ui::draw::mirrored_copy(call).is_none_or(|(canvas, source)| !crate::ui_trace::skips_drawing(canvas, source))
    }

    fn original_blit(&self, call: &jgl::BlitCall) -> bool {
        let game = civ3_ui::blit::game_blit(call);
        // With placeholder sprites jgl draws no blit: the interface model
        // has them all (an unknown caller's too: nothing to draw). Else
        // the decision the exe detours made.
        // (Asked either way: it notes the canvases drawing onto was
        // skipped, which tells when the game reads one back.)
        let decided = game.as_ref().is_some_and(crate::ui_trace::skips_blit);
        let skipped = crate::ui_trace::placeholder_sprites() || decided;
        if compare::on() {
            compare::asked(call, game.is_some(), skipped);
        }
        !skipped
    }

    fn original_sprite_pixels(&self) -> bool {
        !crate::ui_trace::placeholder_sprites()
    }
}

fn hooks() -> jgl::Hooks {
    jgl::Hooks {
        text: Box::new(TextCapture::new(canvas_text)),
        fill: Box::new(FillCapture::new(canvas_draw)),
        copy: Box::new(CopyCapture::new(canvas_draw)),
        blit: Box::new(BlitCapture::new(game_blit)),
        palette: Box::new(jgl::hooks::NoHooks),
        policy: Box::new(Pixels),
    }
}

/// Takes over jgl.dll's text group (at `base`). `Ok(true)` once in place.
pub(crate) fn install_text(base: u32) -> Result<bool, String> {
    TEXT.run(|| {
        let group = jgl_ffi::text::install(base, Box::new(hooks)).map_err(|e| e.to_string())?;
        // For the life of the process.
        std::mem::forget(group);
        log::info!("jgl text group taken over (image +0x08, +0xA8..+0xB8; font +0x00, +0x08, +0x0C)");
        Ok(true)
    })
}

/// A group that cannot be taken over, as the interface capture's hook
/// errors are: another jgl.dll (or another patch in its tables) fails the
/// capture's installation, as the `shade_rect` slot hook did before.
fn takeover_error(e: jgl_ffi::takeover::TakeoverError) -> inproc::hook::HookError {
    use jgl_ffi::takeover::TakeoverError as E;
    log::error!("jgl: {e}");
    match e {
        E::Hook(e) => e,
        E::Unexpected { found, expected, .. } => {
            inproc::hook::HookError::UnexpectedCode { addr: found, found: found.to_le_bytes().to_vec(), expected: expected.to_le_bytes().to_vec() }
        }
        E::NoSuchSlot { .. } | E::NoImport(_) | E::NoHooks => inproc::hook::HookError::Unsupported,
    }
}

/// Takes over jgl.dll's fill group (at `base`): with the interface capture,
/// whose mirror its calls feed. After the text group.
pub(crate) fn install_fill(base: u32) -> Result<(), inproc::hook::HookError> {
    let group = jgl_ffi::fill::install(base).map_err(takeover_error)?;
    std::mem::forget(group);
    log::info!("jgl fill group taken over (image +0x44..+0x64)");
    Ok(())
}

/// Takes over jgl.dll's copy group (at `base`), as [`install_fill`].
pub(crate) fn install_copy(base: u32) -> Result<(), inproc::hook::HookError> {
    let group = jgl_ffi::copy::install(base).map_err(takeover_error)?;
    std::mem::forget(group);
    log::info!("jgl copy group taken over (image +0x38, +0x40, +0x84, +0x8C)");
    Ok(())
}

/// Takes over jgl.dll's display-mode group (at `base`), before the game
/// sets its mode: without it a display with no mode of the game's size
/// (shorter than 768 lines) ends the game at start. Once per load.
pub(crate) fn install_display(base: u32) -> Result<(), String> {
    let group = jgl_ffi::display::install(base).map_err(|e| e.to_string())?;
    // For the life of the process.
    std::mem::forget(group);
    Ok(())
}

/// `CIV3R_JGL_COUNT=1`: counting stubs in every slot of jgl.dll (at
/// `base`), reported every 60 s. After all other slot patches.
pub(crate) fn install_counting(base: u32) {
    if std::env::var("CIV3R_JGL_COUNT").as_deref() != Ok("1") {
        return;
    }
    match jgl_ffi::count::install(base, inproc::process::thread_id()) {
        Ok(()) => {
            log::info!("jgl counting mode: every slot of jgl.dll's five vtables counts its calls");
            let spawned = std::thread::Builder::new().name("civ3r-jgl-count".into()).spawn(|| loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
                report();
            });
            if let Err(e) = spawned {
                log::warn!("jgl counting mode: no report thread: {e}");
            }
        }
        Err(e) => log::warn!("jgl counting mode: {e}"),
    }
}

/// Logs the counting mode's counts.
pub(crate) fn report() {
    if let Some((counts, other)) = jgl_ffi::count::snapshot() {
        log::info!(
            "jgl calls since start ({other} from other threads than the game's; drawing threads {}, re-entered {}):\n{}",
            jgl_ffi::text::threads(),
            jgl_ffi::text::reentrant(),
            counts.report()
        );
    }
    if compare::on() {
        compare::report();
    }
}


/// Takes over jgl.dll's sprite group (at `base`): the blits, the sprites'
/// state slots, the draw scale and the colour tables, as [`install_fill`].
pub(crate) fn install_blit(base: u32) -> Result<(), inproc::hook::HookError> {
    let group = jgl_ffi::blit::install(base).map_err(takeover_error)?;
    std::mem::forget(group);
    log::info!("jgl sprite group taken over (sprite +0x04, +0x0C..+0x18, +0x38..+0xA0; colour table +0x04..+0x20; graphics +0x9C)");
    Ok(())
}

/// `CIV3R_JGL_COMPARE=1`: the exe detours on the `Sprite` drawing
/// functions (and the map tracer's three) that the sprite group replaces
/// run beside the takeover, and every call is compared (temporary, for the
/// migration's equivalence check).
///
/// An old detour runs first (it is above jgl: the exe's function): its
/// events and map-trace records are caught here instead of reaching the
/// mirror and the tracer, and its skip decision is noted; then it always
/// calls the original, which reaches the takeover. The takeover's events
/// and records go on and are caught too, as is its pixel decision. Per old
/// call, the two lists and the two decisions are compared.
pub(crate) mod compare {
    use crate::ui::Event;
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

    pub fn on() -> bool {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ON.get_or_init(|| std::env::var("CIV3R_JGL_COMPARE").as_deref() == Ok("1"))
    }

    #[derive(Copy, Clone, PartialEq, Eq)]
    enum Mode {
        Off,
        Old,
        New,
    }

    thread_local! {
        static MODE: Cell<Mode> = const { Cell::new(Mode::Off) };
        static OLD: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static NEW: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        /// The takeover's pixel decisions during an old call: skipped?
        static ASKED: RefCell<Vec<bool>> = const { RefCell::new(Vec::new()) };
    }

    static SAME: AtomicU32 = AtomicU32::new(0);
    static DIFFERENT: AtomicU32 = AtomicU32::new(0);
    /// Old calls that do not reach jgl (no pixels, a sprite read from a
    /// file): their old events are counted, not compared.
    static NO_PIXELS: AtomicU32 = AtomicU32::new(0);
    static NO_PIXELS_EVENTS: AtomicU32 = AtomicU32::new(0);
    /// Old calls by kind: (calls, events, records).
    static PER_KIND: Mutex<BTreeMap<&'static str, (u32, u32)>> = Mutex::new(BTreeMap::new());
    /// Takeover blits outside any old function, by slot and call site:
    /// (calls, with a wrapper, skipped).
    static OUTSIDE: Mutex<BTreeMap<(u16, u32), (u32, bool, u32)>> = Mutex::new(BTreeMap::new());

    fn keep(s: String) -> bool {
        match MODE.with(Cell::get) {
            Mode::Old => {
                OLD.with_borrow_mut(|v| v.push(s));
                true
            }
            Mode::New => {
                NEW.with_borrow_mut(|v| v.push(s));
                false
            }
            Mode::Off => false,
        }
    }

    /// Whether `e` is the old path's (caught, not applied). On the new
    /// path, `e` is kept to compare and applied.
    pub fn swallow(e: &Event) -> bool {
        on() && MODE.with(Cell::get) != Mode::Off && keep(format!("{e:?}"))
    }

    /// As [`swallow`], for a map-trace record.
    pub fn swallow_record(map_renderer: u32, sprite: u32, x: u32, y: u32, params: &[u32], kind: &str) -> bool {
        on() && MODE.with(Cell::get) != Mode::Off && keep(format!("trace {kind} {map_renderer:#x} {sprite:#x} {x} {y} {params:?}"))
    }

    /// An old detour: `events` makes the old events (and answers whether
    /// the old path skipped the game's drawing), then `original` runs (the
    /// exe's function, reaching jgl's taken-over slot). `reaches`: whether
    /// the function calls jgl at all.
    pub fn old_call<R>(kind: &'static str, reaches: bool, events: impl FnOnce() -> bool, original: impl FnOnce() -> R) -> R {
        if MODE.with(Cell::get) != Mode::Off {
            // Nested (a scaled function calling its unscaled one): the
            // outer call compares.
            return original();
        }
        MODE.with(|m| m.set(Mode::Old));
        let skipped_old = events();
        MODE.with(|m| m.set(Mode::New));
        let r = original();
        MODE.with(|m| m.set(Mode::Off));
        let old = OLD.with_borrow_mut(std::mem::take);
        let new = NEW.with_borrow_mut(std::mem::take);
        let asked = ASKED.with_borrow_mut(std::mem::take);
        if let Ok(mut k) = PER_KIND.lock() {
            let e = k.entry(kind).or_default();
            e.0 += 1;
            e.1 += old.len() as u32;
        }
        if !reaches {
            NO_PIXELS.fetch_add(1, Relaxed);
            NO_PIXELS_EVENTS.fetch_add(old.len() as u32, Relaxed);
            if !new.is_empty() || !asked.is_empty() {
                log::warn!("jgl compare: {kind} without pixels reached the takeover: {new:?} {asked:?}");
                DIFFERENT.fetch_add(1, Relaxed);
            }
            return r;
        }
        if old == new && asked == [skipped_old] {
            SAME.fetch_add(1, Relaxed);
        } else if DIFFERENT.fetch_add(1, Relaxed) < 40 {
            log::warn!("jgl compare: {kind}: old {old:?} skipped {skipped_old}, new {new:?} skipped {asked:?}");
        }
        r
    }

    /// The takeover decided a blit's pixels: `skipped`, from the exe
    /// wrapper `wrapper` (none: not one of the exe's).
    pub fn asked(call: &jgl::BlitCall, wrapper: bool, skipped: bool) {
        match MODE.with(Cell::get) {
            Mode::New => ASKED.with_borrow_mut(|v| v.push(skipped)),
            Mode::Off => {
                if let Ok(mut o) = OUTSIDE.lock() {
                    let e = o.entry((call.slot, call.caller)).or_default();
                    e.0 += 1;
                    e.1 = wrapper;
                    e.2 += u32::from(skipped);
                }
            }
            Mode::Old => log::warn!("jgl compare: the takeover reached from an old detour's events"),
        }
    }

    pub fn report() {
        let kinds = PER_KIND.lock().map(|k| k.iter().map(|(k, (n, e))| format!("{k} {n} calls/{e} events")).collect::<Vec<_>>().join(", ")).unwrap_or_default();
        let outside = OUTSIDE
            .lock()
            .map(|o| {
                o.iter()
                    .map(|((slot, at), (n, w, s))| format!("  sprite +{slot:#x} from {at:#x}: {n} calls, wrapper {w}, {s} skipped"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        log::info!(
            "jgl compare: {} same, {} different, {} not reaching jgl ({} old events); old calls: {kinds}\ntakeover blits outside the old functions:\n{outside}",
            SAME.load(Relaxed),
            DIFFERENT.load(Relaxed),
            NO_PIXELS.load(Relaxed),
            NO_PIXELS_EVENTS.load(Relaxed),
        );
    }
}
