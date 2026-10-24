//! The jgl takeover, wired to the bridge (`docs/jgl-replacement.md`): the
//! method groups `jgl-ffi` takes over in jgl.dll's tables, the hooks they
//! report to, and the counting mode.
//!
//! Groups taken over so far: text. Its runs go to the interface mirror
//! (`ui_trace::on_text`) and to the recent-text list (`trace`).

use civ3_ui::text::{CanvasText, TextCapture};

/// The text group goes in before jgl's first draw: from the `LoadLibraryA`
/// callback, or for a module already loaded, from the bridge's setup. Two
/// loads at once both return with it in place.
static TEXT: inproc::loader::InstallOnce = inproc::loader::InstallOnce::new();

/// `CIV3R_JGL_TEXT_COMPARE=1`: the replaced `TextOutA` import hook runs
/// beside the takeover, and every run is compared (temporary, for the
/// migration's equivalence check; see [`compare`]).
pub(crate) fn text_compare() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("CIV3R_JGL_TEXT_COMPARE").as_deref() == Ok("1"))
}

fn canvas_text(t: CanvasText) {
    crate::ui_trace::audit_semantic(crate::ui_trace::SemanticClass::TextRun);
    crate::ui_trace::on_text(t.canvas, t.clip, &t.run);
    if text_compare() {
        compare::new_run(&t);
    }
    crate::trace::recent_text(t.run);
}

/// Whether jgl still draws a text run's pixels: on canvases the game reads
/// back (the mirror's keep set), and when everything is compared.
struct Pixels;

impl jgl::PixelPolicy for Pixels {
    fn original_pixels(&self, _image: jgl::ImageId, owner: u32) -> bool {
        text_compare() || !crate::ui_trace::skips_text_on(owner)
    }
}

/// Takes over jgl.dll's text group (at `base`). `Ok(true)` once in place.
pub(crate) fn install_text(base: u32) -> Result<bool, String> {
    TEXT.run(|| {
        let group = jgl_ffi::text::install(
            base,
            Box::new(|| jgl::Hooks { text: Box::new(TextCapture::new(canvas_text)), policy: Box::new(Pixels) }),
        )
        .map_err(|e| e.to_string())?;
        // For the life of the process.
        std::mem::forget(group);
        log::info!("jgl text group taken over (image +0x08, +0xA8..+0xB8; font +0x00, +0x08, +0x0C)");
        if text_compare() {
            crate::trace::hook_jgl_text(base).map_err(|e| e.to_string())?;
            log::info!("jgl text compare: the TextOutA import hook runs beside the takeover");
        }
        Ok(true)
    })
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

/// Logs the counting mode's counts (and the text comparison's).
pub(crate) fn report() {
    if let Some((counts, other)) = jgl_ffi::count::snapshot() {
        log::info!(
            "jgl calls since start ({other} from other threads than the game's; text threads {}, re-entered {}):\n{}",
            jgl_ffi::text::threads(),
            jgl_ffi::text::reentrant(),
            counts.report()
        );
    }
    if text_compare() {
        compare::report();
    }
}

/// The equivalence check of the text group against the import hook it
/// replaces: the takeover records a run first, then jgl's original calls
/// `TextOutA`, whose (old) hook makes its run the old way and compares.
pub(crate) mod compare {
    use super::CanvasText;
    use crate::text::TextRun;
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

    thread_local! {
        static LAST: RefCell<Option<CanvasText>> = const { RefCell::new(None) };
    }

    static SAME: AtomicU32 = AtomicU32::new(0);
    static DIFFERENT: AtomicU32 = AtomicU32::new(0);
    static OLD_ONLY: AtomicU32 = AtomicU32::new(0);
    static NEW_ONLY: AtomicU32 = AtomicU32::new(0);
    /// Runs drawn outside the exe's four text functions (the old path had
    /// no canvas for them): the same run, but the old path left it out of
    /// the interface.
    static NO_OLD_CANVAS: AtomicU32 = AtomicU32::new(0);

    pub fn new_run(t: &CanvasText) {
        if LAST.with_borrow_mut(|l| l.replace(t.clone())).is_some() {
            NEW_ONLY.fetch_add(1, Relaxed);
        }
    }

    /// The old import hook's run, its canvas (the exe text function's
    /// `this`, 0 outside them) and clip.
    pub fn old_run(run: &TextRun, canvas: u32, clip: Option<[i32; 4]>) {
        let Some(new) = LAST.with_borrow_mut(Option::take) else {
            OLD_ONLY.fetch_add(1, Relaxed);
            return;
        };
        let canvas_ok = canvas == new.canvas || canvas == 0;
        if canvas == 0 {
            NO_OLD_CANVAS.fetch_add(1, Relaxed);
        }
        let clip_ok = canvas == 0 || clip == Some(new.clip);
        if *run == new.run && canvas_ok && clip_ok {
            SAME.fetch_add(1, Relaxed);
        } else if DIFFERENT.fetch_add(1, Relaxed) < 40 {
            log::warn!("jgl text compare: old {run:?} on {canvas:#x} clip {clip:?}, new {:?} on {:#x} clip {:?}", new.run, new.canvas, new.clip);
        }
    }

    pub fn report() {
        log::info!(
            "jgl text compare: {} same, {} different, {} old only, {} new only, {} outside the exe text functions",
            SAME.load(Relaxed),
            DIFFERENT.load(Relaxed),
            OLD_ONLY.load(Relaxed),
            NEW_ONLY.load(Relaxed),
            NO_OLD_CANVAS.load(Relaxed)
        );
    }
}
