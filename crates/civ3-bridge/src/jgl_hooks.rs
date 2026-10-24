//! The jgl takeover, wired to the bridge (`docs/jgl-replacement.md`): the
//! method groups `jgl-ffi` takes over in jgl.dll's tables, the hooks they
//! report to, and the counting mode.
//!
//! Groups taken over so far: text, fills (lines, shades) and the display
//! modes. Text runs go to the interface mirror (`ui_trace::on_text`) and to
//! the recent-text list (`trace`); fills to the interface mirror
//! (`ui_trace::on_fill`). The display modes need no hooks: the game's
//! screen is the mode it asks for, and the display is never switched, not
//! by a movie nor by jgl's shutdown either (`jgl::display`).

use civ3_ui::draw::{CanvasDraw, FillCapture};
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
    if compare::on() {
        compare::new_draw(&d);
    }
    match d {
        CanvasDraw::Fill { canvas, size, rects, rgb, alpha, clip } => crate::ui_trace::on_fill(canvas, size, &rects, rgb, alpha, clip),
    }
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
        let taken = civ3_ui::draw::mirrored_fill(call);
        if compare::on() {
            compare::asked(taken.map(|canvas| (canvas, 0)), call.target().owner, "fill");
        }
        taken.is_none_or(|canvas| !crate::ui_trace::skips_drawing(canvas, 0))
    }
}

fn hooks() -> jgl::Hooks {
    jgl::Hooks {
        text: Box::new(TextCapture::new(canvas_text)),
        fill: Box::new(FillCapture::new(canvas_draw)),
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

/// Logs the counting mode's counts (and the comparison's).
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

/// `CIV3R_JGL_COMPARE=1`: the exe detours and the `shade_rect` slot hook
/// the fill group replaces run beside the takeover, and every call
/// is compared (temporary, for the migration's equivalence check).
///
/// An old detour runs first (it is above jgl: the exe's `PCX_Image`
/// functions, or a hook over the taken-over slot): its events are caught
/// here instead of reaching the mirror, then it always calls the original,
/// which reaches the takeover. The takeover's events go to the mirror and
/// are caught too, and its pixel decision is made as usual. Per old call,
/// the two event lists and the canvases each would ask the skip decision
/// for are compared.
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
        static OLD: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
        static NEW: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
        /// The skip questions of the takeover during an old call.
        static ASKED: RefCell<Vec<Option<(u32, u32)>>> = const { RefCell::new(Vec::new()) };
    }

    static SAME: AtomicU32 = AtomicU32::new(0);
    static DIFFERENT: AtomicU32 = AtomicU32::new(0);
    /// Old calls whose image had no pixels: jgl is not called (old events
    /// made all the same: counted, not compared).
    static NO_PIXELS: AtomicU32 = AtomicU32::new(0);
    static NO_PIXELS_EVENTS: AtomicU32 = AtomicU32::new(0);
    /// Old calls by kind, and events compared.
    static PER_KIND: Mutex<BTreeMap<&'static str, (u32, u32)>> = Mutex::new(BTreeMap::new());
    /// Takeover calls outside any old function, by kind: (calls, mirrored,
    /// events), with up to 8 callers and canvases each.
    static OUTSIDE: Mutex<BTreeMap<String, (u32, u32, u32, Vec<(u32, u32)>)>> = Mutex::new(BTreeMap::new());

    /// Whether `e` is the old path's (caught, not applied). On the new
    /// path, `e` is kept to compare and applied.
    pub fn swallow(e: &Event) -> bool {
        if !on() {
            return false;
        }
        match MODE.with(Cell::get) {
            Mode::Old => {
                OLD.with_borrow_mut(|v| v.push(e.clone()));
                true
            }
            Mode::New => {
                NEW.with_borrow_mut(|v| v.push(e.clone()));
                false
            }
            Mode::Off => false,
        }
    }

    /// An old detour: `events` makes the old events (and answers the
    /// canvas and source the old path asked the skip decision for), then
    /// `original` runs (jgl's slot, taken over). `reaches`: whether the
    /// game's function calls jgl at all (its images have pixels).
    pub fn old_call<R>(kind: &'static str, reaches: bool, events: impl FnOnce() -> Option<(u32, u32)>, original: impl FnOnce() -> R) -> R {
        if MODE.with(Cell::get) != Mode::Off {
            // Nested (not expected): no comparison of the inner call.
            return original();
        }
        MODE.with(|m| m.set(Mode::Old));
        let asked_old = events();
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
        let asked_ok = asked == [asked_old];
        if old == new && asked_ok {
            SAME.fetch_add(1, Relaxed);
        } else if DIFFERENT.fetch_add(1, Relaxed) < 40 {
            log::warn!("jgl compare: {kind}: old {old:?} asked {asked_old:?}, new {new:?} asked {asked:?}");
        }
        r
    }

    /// The takeover asked about a call: `taken` the canvas (and source) it
    /// skips drawing on, `target` the owner drawn onto.
    pub fn asked(taken: Option<(u32, u32)>, target: u32, group: &'static str) {
        match MODE.with(Cell::get) {
            Mode::New => ASKED.with_borrow_mut(|v| v.push(taken)),
            Mode::Off => outside(group, taken.is_some(), target),
            Mode::Old => log::warn!("jgl compare: the takeover reached from an old detour's events"),
        }
    }

    /// A mirrored drawing made by the takeover.
    pub fn new_draw(d: &civ3_ui::draw::CanvasDraw) {
        if MODE.with(Cell::get) == Mode::Off {
            let key = match d {
                civ3_ui::draw::CanvasDraw::Fill { .. } => "fill",
            };
            if let Ok(mut o) = OUTSIDE.lock() {
                o.entry(format!("{key} draws")).or_default().2 += 1;
            }
        }
    }

    fn outside(group: &'static str, mirrored: bool, target: u32) {
        let caller = crate::ui_trace::game_caller();
        if let Ok(mut o) = OUTSIDE.lock() {
            let e = o.entry(format!("{group} at {caller:#x}")).or_default();
            e.0 += 1;
            e.1 += u32::from(mirrored);
            if e.3.len() < 8 && !e.3.iter().any(|&(_, t)| t == target) {
                e.3.push((caller, target));
            }
        }
    }

    pub fn report() {
        let kinds = PER_KIND.lock().map(|k| k.iter().map(|(k, (n, e))| format!("{k} {n} calls/{e} events")).collect::<Vec<_>>().join(", ")).unwrap_or_default();
        let outside = OUTSIDE
            .lock()
            .map(|o| {
                o.iter()
                    .map(|(k, (n, m, e, s))| {
                        let canvases = s.iter().map(|(_, t)| format!("{t:#x}")).collect::<Vec<_>>().join(" ");
                        format!("  {k}: {n} calls, {m} mirrored, {e} draws; canvases {canvases}")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        log::info!(
            "jgl compare: {} same, {} different, {} without pixels ({} old events); old calls: {kinds}\ntakeover calls outside the old functions:\n{outside}",
            SAME.load(Relaxed),
            DIFFERENT.load(Relaxed),
            NO_PIXELS.load(Relaxed),
            NO_PIXELS_EVENTS.load(Relaxed),
        );
    }
}
