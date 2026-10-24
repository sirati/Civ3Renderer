//! The game's own window behind the viewer: what the game learns of the
//! screen (the display divided by the desktop's scale `CIV3R_SCALE`, the
//! game at the desktop's logical size: `crate::game_size`), the window kept
//! hidden from the start while a viewer shows the game
//! (`CIV3R_GAME_WINDOW=show` keeps it shown), and the game's pointer at
//! the viewer's edge (`crate::edge`).
//!
//! Set up in `DllMain`, before the game's code runs: the game's executable
//! is redirected at once, `jgl.dll` (the library that makes and paints the
//! window, loaded by the executable later) as it loads.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::ui_capture::CapturePoint;

/// What was done, for the log (most of it happens before there is one).
static NOTES: LazyLock<handoff::Mailbox<String>> = LazyLock::new(handoff::Mailbox::new);

/// `jgl.dll`'s base once it is redirected (0: not yet).
static JGL: AtomicU32 = AtomicU32::new(0);

fn note(s: String) {
    NOTES.send(s);
}

/// The notes so far, for the log.
pub fn take_notes() -> Vec<String> {
    NOTES.drain().collect()
}

/// Whether `jgl.dll` was redirected as it loaded.
pub fn jgl_hooked() -> bool {
    JGL.load(Ordering::SeqCst) != 0
}

/// The desktop's scale (`CIV3R_SCALE`, from the launcher; 1 without).
pub fn desktop_scale() -> f64 {
    let scale = std::env::var("CIV3R_SCALE").ok().and_then(|s| s.trim().parse::<f64>().ok());
    scale.filter(|&s| s.is_finite() && s >= 1.0).unwrap_or(1.0)
}

/// The desktop's scale the game's screen is divided by (more than 1), if
/// any: the game at the desktop's logical size.
fn scale() -> Option<f64> {
    Some(desktop_scale()).filter(|&s| s > 1.0)
}

/// Whether the game's window is hidden, the viewer the only window the
/// player sees: with a viewer (`CIV3R_VIEWER`), unless
/// `CIV3R_GAME_WINDOW=show` (for tests that look at both).
pub fn hidden() -> bool {
    std::env::var_os("CIV3R_VIEWER").is_some() && std::env::var("CIV3R_GAME_WINDOW").as_deref() != Ok("show")
}

/// Redirects `what` of `module` (its hooks stay for the life of the
/// process), noting how that went.
fn install<H>(what: &str, hooked: Result<Option<H>, inproc::hook::HookError>, done: &str) {
    match hooked {
        Ok(Some(h)) => {
            std::mem::forget(h);
            if !done.is_empty() {
                note(done.into());
            }
        }
        Ok(None) => note(format!("{what}: not imported")),
        Err(e) => note(format!("{what}: {e}")),
    }
}

/// The game's screen: the display, or as the game sees it with a desktop's
/// scale (`CIV3R_SCALE`).
pub fn game_screen() -> Option<(u32, u32)> {
    let display = inproc::screen::current_mode()?;
    Some(match scale() {
        Some(n) => crate::game_size::for_display_modes(display, n, &inproc::screen::display_modes()),
        None => display,
    })
}

/// In `DllMain` (see the module docs).
pub fn early() {
    let exe = inproc::pe::main_module();
    let (scale, hidden) = (scale(), hidden());
    inproc::hidden::set_hiding(hidden);
    let scale = scale.map(|n| {
        let size: inproc::screen::ScreenSize = std::sync::Arc::new(move |(w, h)| {
            let modes = inproc::screen::display_modes();
            let (w, h) = crate::game_size::for_display_modes((w.max(1) as u32, h.max(1) as u32), n, &modes);
            note(format!("the game's screen: {w}×{h}"));
            (w as i32, h as i32)
        });
        let done = format!("the game sees the display {n}× smaller (the desktop's scale, CIV3R_SCALE; at least {}×{})", crate::game_size::MIN.0, crate::game_size::MIN.1);
        install("CIV3R_SCALE: the game's GetSystemMetrics", inproc::screen::hook_scaled(exe, size.clone()), &done);
        size
    });
    if hidden {
        install("the game's window hidden: its CreateWindowExA", inproc::hidden::hook_hidden_windows(exe), "");
    }
    let loaded_scale = scale.clone();
    let loaded = inproc::loader::hook_load_library(exe, move |name, base| {
        if name == "jgl.dll" {
            hook_jgl(base, loaded_scale.clone(), hidden, CapturePoint::LoadCallback);
        }
    });
    install("jgl.dll as it loads: the game's LoadLibraryA", loaded, "");
    // A library already mapped before this DLL's early setup needs the
    // text/window hooks only. Full capture must wait for the actual game
    // load callback, or the post-CRT worker fallback, outside DllMain.
    // Register interception before the already-loaded check so neither
    // path can miss a module load.
    if let Some(base) = inproc::pe::module_base("jgl.dll") {
        hook_jgl(base, scale, hidden, CapturePoint::DllMain);
    }
}

/// `jgl.dll` just loaded (at `base`), before it runs: what it learns of the
/// screen, its window kept hidden and painting, its pointer.
fn hook_jgl(base: u32, scale: Option<inproc::screen::ScreenSize>, hidden: bool, point: CapturePoint) {
    // Every load caller finishes serialized text and UI capture before
    // returning. Claiming the remaining window setup first would let a
    // concurrent caller bypass capture still being installed.
    if !inproc::loader::prepare_before_claim(base, &JGL, || {
        match crate::ui_trace::prepare_at(base, point) {
            Ok(true) => note("UI capture installed before the game's first jgl use".into()),
            Ok(false) => {},
            Err(e) => {
                note(format!("early UI capture: {e}"));
                // This load callback is outside DllMain. Do not return into
                // uncaptured legacy execution after replacement setup fails.
                if point.allowed() && std::env::var_os("CIV3R_VIEWER").is_some() {
                    crate::ui_trace::replacement_failed(&format!("early UI capture failed: {e}"));
                }
            },
        }
    }) { return; }
    match crate::jgl_hooks::install_display(base) {
        Ok(()) => note("display modes: the game's screen is the mode it asks for; the display is never switched (jgl graphics +0x58..+0x60, and its ChangeDisplaySettingsA: shutdown, movies)".into()),
        Err(e) => note(format!("display modes left to jgl: {e}")),
    }
    if let Some(size) = scale {
        install("CIV3R_SCALE: jgl.dll's GetSystemMetrics", inproc::screen::hook_scaled(base, size), "");
    }
    if hidden {
        let done = "the game's window is kept hidden: the viewer is the only one (CIV3R_GAME_WINDOW=show keeps it)";
        install("the game's window hidden: jgl.dll's window calls", inproc::hidden::hook_hidden_windows(base), done);
        install("the game's window painting while hidden", inproc::paint::hook_hidden_painting(base), "the game's window paints while hidden");
    }
    install("edge scroll: jgl.dll's GetCursorPos", inproc::pointer::hook_cursor_pos(base), "edge scroll: following the viewer's pointer at its edge");
    // Last: the counting stubs wrap whatever the other patches left.
    crate::jgl_hooks::install_counting(base);
}
