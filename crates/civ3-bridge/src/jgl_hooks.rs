//! The jgl takeover, wired to the bridge (`docs/jgl-replacement.md`): the
//! method groups `jgl-ffi` takes over in jgl.dll's tables, the hooks they
//! report to, and the counting mode.
//!
//! Groups taken over so far: text, fills (lines, shades), copies, sprites
//! (blits, their state, colour tables), present and invalidate, and the
//! display modes. Text runs go
//! to the interface mirror (`ui_trace::on_text`) and to the recent-text
//! list (`trace`); fills and copies to the interface mirror
//! (`ui_trace::on_fill`, `ui_trace::on_copy`); blits, named by the exe
//! function that made them (`civ3_ui::blit`), to the interface mirror and
//! the map tracer (`ui_trace::on_blit`). Present and invalidate are
//! records nothing consumes yet; jgl copies the back buffer to its window
//! only while the game draws itself. The display modes need no hooks: the game's
//! screen is the mode it asks for, and the display is never switched, not
//! by a movie nor by jgl's shutdown either (`jgl::display`).

use civ3_ui::blit::{BlitCapture, GameBlit};
use civ3_ui::draw::CanvasDraw;
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

struct Fills;

impl jgl::FillHooks for Fills {
    fn fill(&mut self, call: &jgl::FillCall) {
        crate::ui_selection::fill(call);
        crate::option_masks::fill(call);
        if let jgl::FillCall::ReplaceColor {target,rect,from,to}=*call {
            crate::ui_trace::replace_indices(target,rect,from,to);
        } else if matches!(call,jgl::FillCall::Other {..}) {
            crate::ui_trace::unanswered_canvas(call.target().owner);
        }
        if let Some(draw) = civ3_ui::draw::fill_of(call) { canvas_draw(draw); }
    }
}

struct Copies;
impl jgl::CopyHooks for Copies {
    fn copy(&mut self, call: &jgl::CopyCall) {
        match call {
            jgl::CopyCall::Indexed { source,target,src,at,size,mode } => crate::ui_trace::tile_under(*source,*target,*src,*at,*size,*mode),
            _ => {}
        }
        if let Some(draw) = civ3_ui::draw::copy_of(call) { canvas_draw(draw); }
    }
}

/// Whether jgl still draws a call's pixels: on canvases the game reads
/// back (the mirror's keep set), and calls the interface mirror does not
/// take.
struct Pixels;

impl jgl::PixelPolicy for Pixels {
    fn preserve_image_data(&self, image: jgl::ImageId, owner: u32) -> bool {
        crate::ui_trace::preserve_image_data(image, owner)
    }

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
        // (A sprite made before the takeover has jgl's pixels: the old
        // decision.)
        let skipped = (crate::ui_trace::placeholder_sprites() && call.sprite.placeholder) || decided;
        if !decided && skipped {
            if let Some(g) = &game {
                crate::ui_trace::placeholder_lost(g.canvas);
            }
        }
        !skipped
    }

    fn original_sprite_pixels(&self) -> bool {
        crate::ui_trace::real_sprite_pixels()
    }

    /// The back buffer goes to jgl's window only while the game draws
    /// itself (as the `present` slot hook it replaces decided).
    fn original_present(&self, _call: &jgl::PresentCall) -> bool {
        let skip = crate::ui_trace::skipping();
        crate::ui_trace::audit_call(crate::ui_trace::PixelClass::Present, skip, || 0);
        !skip
    }
}

fn hooks() -> jgl::Hooks {
    jgl::Hooks {
        text: Box::new(TextCapture::new(canvas_text)),
        fill: Box::new(Fills),
        copy: Box::new(Copies),
        blit: Box::new(BlitCapture::new(game_blit)),
        palette: Box::new(jgl::hooks::NoHooks),
        present: Box::new(jgl::hooks::NoHooks),
        surface: Box::new(jgl::hooks::NoHooks),
        policy: Box::new(Pixels),
    }
}

/// Whether our jgl stands in for jgl.dll, which is then never loaded:
/// whenever our renderer draws the game (`docs/jgl-replacement.md` §8).
/// `CIV3R_JGL=original` keeps jgl.dll, for comparing with it. The plain
/// game without a viewer always has jgl.dll.
pub(crate) fn own_mode() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        crate::pass_through::active_here()
            && crate::ui_trace::placeholder_sprites()
            && std::env::var("CIV3R_JGL").as_deref() != Ok("original")
    })
}

/// Puts our jgl in place (`jgl_ffi::own::install`): every group's hooks
/// as for jgl.dll, the window's from `crate::heartbeat`. From `DllMain`,
/// before the game loads jgl.dll.
pub(crate) fn install_own(screen: Option<inproc::screen::ScreenSize>) -> Result<bool, String> {
    TEXT.run(|| {
        jgl_ffi::own::install(Box::new(hooks), Box::new(crate::heartbeat::WindowHooks { screen })).map_err(|e| e.to_string())?;
        Ok(true)
    })
}

/// Takes over jgl.dll's text group (at `base`). `Ok(true)` once in place.
pub(crate) fn install_text(base: u32) -> Result<bool, String> {
    // In replacement mode the bridge installed the complete set of safe
    // hooks while it was still in DllMain.  There is no jgl image to patch;
    // `base == 0` is deliberately passed through the capture setup so the
    // same transaction can be used for both modes.
    if jgl_ffi::own::on() {
        return Ok(true);
    }
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
    if let Some((counts, ours, other)) = jgl_ffi::count::snapshot() {
        log::info!(
            "jgl calls since start ({other} from other threads than the game's; drawing threads {}, re-entered {}){}:\n{}",
            jgl_ffi::text::threads(),
            jgl_ffi::text::reentrant(),
            if ours.is_some() { ", through jgl's method tables" } else { "" },
            counts.report()
        );
        if let Some(ours) = ours {
            log::info!("jgl calls through our method tables:\n{}", ours.report());
        }
    }
}


/// Takes over jgl.dll's sprite group (at `base`): the blits, the sprites'
/// state slots, the draw scale and the colour tables, as [`install_fill`].
pub(crate) fn install_blit(base: u32) -> Result<(), inproc::hook::HookError> {
    let group = jgl_ffi::blit::install(base).map_err(takeover_error)?;
    std::mem::forget(group);
    log::info!("jgl sprite group taken over (sprite +0x00..+0x18, +0x38..+0xA0; colour table +0x04..+0x20; graphics +0x9C)");
    Ok(())
}

/// Takes over jgl.dll's present group (at `base`): graphics +0x30
/// `invalidate` (recorded, then jgl's) and +0xA4 `present` (jgl's only
/// while the game draws itself), as [`install_fill`].
pub(crate) fn install_present(base: u32) -> Result<(), inproc::hook::HookError> {
    let group = jgl_ffi::present::install(base).map_err(takeover_error)?;
    std::mem::forget(group);
    log::info!("jgl present group taken over (graphics +0x30, +0xA4)");
    Ok(())
}

/// Takes over jgl.dll's factories and deletes (at `base`) and gives every
/// object made from then on our method tables (`jgl_ffi::surface`). After
/// every other group, the display modes' too, and before the counting
/// mode. Once per load.
pub(crate) fn install_surface(base: u32) -> Result<(), String> {
    let group = jgl_ffi::surface::install(base).map_err(|e| e.to_string())?;
    std::mem::forget(group);
    Ok(())
}
