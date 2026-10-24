//! Feeds the UI mirror (`ui`) from hooks on the game's UI drawing (docs
//! §16.7): image loads, sprite slices, blits, copies, fills, lines, text and
//! the composition of forms onto the screen.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::atomic::Ordering::{Acquire, AcqRel, Relaxed, Release};

use civ3_sys::{Civ3, layout as L};
use civ3_sys::ui_hooks::{self as h, obj};
use inproc::hook::HookError;

use crate::game_render::Skipper;
use crate::text::TextRun;
use crate::ui::{Event, Frame, UiModel, rgb555};
use crate::ui_capture::{CapturePoint, OriginalDrawing};
use civ3_ui::blit::GameBlit;

static CAPTURE: std::sync::LazyLock<crate::ui_capture::UiCapture<UiTracer>> = std::sync::LazyLock::new(crate::ui_capture::UiCapture::new);
static DRAWING: OriginalDrawing = OriginalDrawing::new();
static REQUESTED_GAME_DRAWS: AtomicBool = AtomicBool::new(false);
static COMPATIBILITY: AtomicBool = AtomicBool::new(false);
static PIXEL_AUDIT: AtomicBool = AtomicBool::new(false);
static PIXEL_SKIPPED: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
static PIXEL_ORIGINAL: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
// Audit rows, written by the game's thread (the only writer), read by the
// audit snapshot (`serve_pixel_audit`).
static READBACK_REASONS: handoff::Latest<Vec<(u32, &'static str, u64)>> = handoff::Latest::new();
static READBACK_TOTALS: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];
static MAP_ORIGINAL_SAMPLES: handoff::Latest<Vec<(String, u64)>> = handoff::Latest::new();
static RESIZED_FILE_CAPTURES: AtomicU64 = AtomicU64::new(0);
static RESIZED_UNSUPPORTED: AtomicU64 = AtomicU64::new(0);
static SCALED_SKIPPED: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];
static SCALED_ORIGINAL: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];
static SEMANTIC_CALLS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

#[derive(Clone, Copy)]
pub(crate) enum PixelClass { Title, Ui, Map, Present, Readback, Empty, Replay, Unknown }
#[derive(Clone, Copy)]
pub(crate) enum SemanticClass { SceneEvent, PixelRead, TextRun, Input }

pub(crate) fn audit_semantic(class: SemanticClass) {
    if PIXEL_AUDIT.load(Relaxed) { SEMANTIC_CALLS[class as usize].fetch_add(1, Relaxed); }
}

fn audit_pixel(class: PixelClass, skip: bool) {
    if PIXEL_AUDIT.load(Relaxed) {
        (if skip { &PIXEL_SKIPPED } else { &PIXEL_ORIGINAL })[class as usize].fetch_add(1, Relaxed);
    }
}

fn audit_readback(canvas: u32, reason: &'static str) {
    if !PIXEL_AUDIT.load(Relaxed) { return; }
    audit_pixel(PixelClass::Readback, false);
    let index = match reason { "palette_indices" => 0, "native_hit_mask" => 1, _ => 2 };
    READBACK_TOTALS[index].fetch_add(1, Relaxed);
    READBACK_REASONS.update(|old| {
        let mut reasons = old.cloned().unwrap_or_default();
        if let Some(row) = reasons.iter_mut().find(|row| row.0 == canvas && row.1 == reason) { row.2 += 1; }
        else if reasons.len() < 64 { reasons.push((canvas, reason, 1)); }
        Some(reasons)
    });
}

/// Bound diagnostic samples to32 distinct original map callbacks. This
/// classifies direct sprite-wrapper calls without changing their behavior.
pub(crate) fn audit_map_original(kind: &str, sprite: u32, canvas: u32, args: &[u32]) {
    if !PIXEL_AUDIT.load(Relaxed) { return; }
    let reason = if kind == "map_sprite" && obj::sprite_loads_file(sprite) { "file_image_source" }
        else if !has_pixels(canvas) { "no_pixels" }
        else if obj::image_bits(canvas).is_some_and(|bits| bits <= 8) { "palette_indices" }
        else if obj::keyed_form_canvas(canvas) { "native_hit_mask" }
        else if with_any_skipper(|s| s.keeps(canvas)) == Some(true) { "native_pixel_source" }
        else { "unclassified" };
    let args = args.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    let sample = format!("\"kind\":\"{kind}\",\"sprite\":\"{sprite:#x}\",\"canvas\":\"{canvas:#x}\",\"reason\":\"{reason}\",\"args\":[{args}]");
    MAP_ORIGINAL_SAMPLES.update(|old| {
        let mut samples = old.cloned().unwrap_or_default();
        if let Some(row) = samples.iter_mut().find(|row| row.0 == sample) { row.1 += 1; }
        else if samples.len() < 32 { samples.push((sample.clone(), 1)); }
        Some(samples)
    });
}

pub(crate) fn audit_call(class: PixelClass, skip: bool, original: impl FnOnce() -> i32) -> i32 {
    audit_pixel(class, skip);
    crate::game_render::pixel_callback(skip, original)
}

/// Explicit private-session audit. No file checks or counter increments occur
/// normally. A request snapshots callback counts without resetting evidence.
pub(crate) fn serve_pixel_audit() {
    if !PIXEL_AUDIT.load(Relaxed) || !std::path::Path::new("civ3-pixel-audit.request").exists() { return; }
    let p = policy();
    let mut rows = vec![format!("\"policy\":{{\"game_draws\":{},\"compatibility_fallback\":{},\"skipping\":{},\"map_pixels_off\":{}}}", p.game_draws, p.fallback, skipping(), crate::trace::GAME_MAP_OFF.load(Relaxed))];
    for (i, name) in ["title", "ui", "map", "present", "readback", "empty", "replay", "unknown"].into_iter().enumerate() {
        rows.push(format!("\"{name}\":{{\"skipped\":{},\"original\":{}}}", PIXEL_SKIPPED[i].load(Relaxed), PIXEL_ORIGINAL[i].load(Relaxed)));
    }
    for (i, name) in ["scene_events", "pixel_reads", "text_runs", "inputs"].into_iter().enumerate() {
        rows.push(format!("\"{name}\":{}", SEMANTIC_CALLS[i].load(Relaxed)));
    }
    let reasons = READBACK_REASONS.load().unwrap_or_default().iter()
        .map(|(canvas, reason, calls)| format!("{{\"canvas\":\"{canvas:#x}\",\"reason\":\"{reason}\",\"calls\":{calls}}}"))
        .collect::<Vec<_>>().join(",");
    rows.push(format!("\"readback_images\":[{reasons}]"));
    rows.push(format!("\"readback_reasons\":{{\"palette_indices\":{},\"native_hit_mask\":{},\"native_pixel_source\":{}}}", READBACK_TOTALS[0].load(Relaxed), READBACK_TOTALS[1].load(Relaxed), READBACK_TOTALS[2].load(Relaxed)));
    let samples = MAP_ORIGINAL_SAMPLES.load().unwrap_or_default().iter()
        .map(|(sample, calls)| format!("{{{sample},\"calls\":{calls}}}"))
        .collect::<Vec<_>>().join(",");
    rows.push(format!("\"map_original_samples\":[{samples}]"));
    let wrappers = ["scaled_a4", "scaled_fog", "scaled_colored"].into_iter().enumerate()
        .map(|(i, name)| format!("\"{name}\":{{\"skipped\":{},\"original\":{}}}", SCALED_SKIPPED[i].load(Relaxed), SCALED_ORIGINAL[i].load(Relaxed)))
        .collect::<Vec<_>>().join(",");
    rows.push(format!("\"scaled_wrappers\":{{{wrappers}}}"));
    rows.push(format!("\"resized_capture\":{{\"file\":{},\"unsupported\":{}}}", RESIZED_FILE_CAPTURES.load(Relaxed), RESIZED_UNSUPPORTED.load(Relaxed)));
    match std::fs::write("civ3-pixel-audit.json", format!("{{{} }}\n", rows.join(","))) {
        Ok(()) => { let _ = std::fs::remove_file("civ3-pixel-audit.request"); }
        Err(error) => log::warn!("pixel audit snapshot: {error}"),
    }
}

static ORIGINAL_REFRESH: AtomicBool = AtomicBool::new(false);
static ORIGINAL_COMPOSE_ALL: AtomicBool = AtomicBool::new(false);
static ORIGINAL_PRESENT_ALL: AtomicBool = AtomicBool::new(false);
thread_local! { static ORIGINAL_REFRESHING: Cell<bool> = const { Cell::new(false) }; }
/// The game's build, once the hooks are installed.
static BUILD: std::sync::OnceLock<civ3_sys::Build> = std::sync::OnceLock::new();

/// The back buffer (`PCX_Image` 0xCACFF0), once the hooks are installed.
static BACK_BUFFER: std::sync::OnceLock<u32> = std::sync::OnceLock::new();

/// The images kept drawn, once skipping was set up (only after complete
/// hook setup, on whichever thread prepared it).
static SKIPPER_KEEPS: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
thread_local! {
    /// What drawing is skipped: the game's thread's, which alone draws.
    static SKIPPER: std::cell::RefCell<Option<Skipper>> = const { std::cell::RefCell::new(None) };
}

/// `f` of the skipper, made on first use once skipping was set up.
fn with_any_skipper<R>(f: impl FnOnce(&mut Skipper) -> R) -> Option<R> {
    SKIPPER.with_borrow_mut(|s| {
        if s.is_none() { *s = SKIPPER_KEEPS.get().map(|keep| Skipper::new(keep.clone())); }
        s.as_mut().map(f)
    })
}

/// Configure the explicit map-only override, without racing capture or
/// restoration. Called before registering the executable's load callback.
pub fn configure_map_policy() {
    let p = policy();
    REQUESTED_GAME_DRAWS.store(p.game_draws, Relaxed);
    COMPATIBILITY.store(p.fallback, Relaxed);
    PIXEL_AUDIT.store(std::env::var("CIV3R_PIXEL_AUDIT").as_deref() == Ok("on"), Relaxed);
    DRAWING.map_policy(p.initial_map_off, |off| crate::trace::GAME_MAP_OFF.store(off, Relaxed));
}

/// Restore the original renderer after viewer startup fails. Later load
/// notifications cannot reactivate suppression after this failure.
pub fn restore_original_drawing() {
    if DRAWING.restore(compatibility_fallback(), || crate::trace::GAME_MAP_OFF.store(false, Relaxed)) {
        ORIGINAL_REFRESH.store(true, Release);
    }
}

/// Service only at native UI-thread safe points. Rebuild retained canvases
/// once before composition; painting cached black canvases is insufficient.
pub fn refresh_original_drawing(game: Civ3) {
    if Painting::inside() || ORIGINAL_REFRESHING.with(Cell::get) || !ORIGINAL_REFRESH.swap(false, AcqRel) { return; }
    struct Refreshing;
    impl Drop for Refreshing {
        fn drop(&mut self) { ORIGINAL_REFRESHING.with(|v| v.set(false)); }
    }
    ORIGINAL_REFRESHING.with(|v| v.set(true));
    let _refreshing = Refreshing;
    crate::native_camera::restore_if_requested(game);
    ORIGINAL_COMPOSE_ALL.store(true, Release);
    ORIGINAL_PRESENT_ALL.store(true, Release);
    match civ3_sys::native_redraw::refresh(game.build) {
        Ok(count) => log::info!("original renderer: rebuilt {} retained form canvases and {} frame caches", count.canvases, count.frame_caches),
        Err(error) => log::warn!("original renderer canvas refresh: {error}"),
    }
}

fn policy() -> crate::game_render::StartupPolicy {
    let var = |name: &str| std::env::var(name).ok();
    crate::game_render::startup_policy(var("CIV3R_GAME_RENDER").as_deref(), var("CIV3R_VIEWER").as_deref(), var("CIV3R_GAME_WINDOW").as_deref(), var("CIV3R_UI").as_deref(), var("CIV3R_GAME_MAP").as_deref())
}

/// Whether sprites' pixels are the placeholder (`jgl_ffi::blit::placeholder`)
/// and jgl draws no blit: with a viewer, the game not drawing (no
/// `CIV3R_GAME_RENDER=on`, no compatibility renderer) and no
/// `CIV3R_JGL_COMPARE` (whose old path reads the sprites jgl made). Decided
/// once: a sprite keeps the pixels it was made with.
pub(crate) fn placeholder_sprites() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let p = policy();
        p.capture && !p.game_draws && !p.fallback
    })
}

thread_local! {
    /// Set while a sprite is cut from [`READ_BACK_ART`]: jgl makes its pixels.
    static REAL_SPRITES: Cell<bool> = const { Cell::new(false) };
}

/// Art whose sprites the game draws into 8-bit images it then reads back
/// with `get_pixel` (0x549D60.., `Art\oldinterface.pcx` into the three
/// images at 0xB3F3E0, read at 0x549FAD): no record answers those reads
/// yet, so these sprites keep jgl's pixels and their blits onto such
/// images jgl's original (`docs/jgl-replacement.md` §9).
const READ_BACK_ART: [&str; 1] = ["art/oldinterface.pcx"];

fn read_back_art(path: &str) -> bool {
    let path = path.replace('\\', "/").to_ascii_lowercase();
    READ_BACK_ART.iter().any(|a| path.ends_with(a))
}

/// Whether the sprite being made gets jgl's pixels (see [`placeholder_sprites`]).
pub(crate) fn real_sprite_pixels() -> bool {
    !placeholder_sprites() || REAL_SPRITES.with(Cell::get)
}

pub(crate) fn requested_game_draws() -> bool {
    REQUESTED_GAME_DRAWS.load(Relaxed)
}

pub(crate) fn compatibility_fallback() -> bool {
    COMPATIBILITY.load(Relaxed)
}

/// Ends the game's process now, while the game still runs. The log is
/// written first. The game's DLLs get no detach (`inproc::process::terminate`):
/// `sound.dll` would otherwise wait forever for its sound thread, and the
/// process would stay without a window, unable to quit.
pub(crate) fn end_process(code: u32) -> ! {
    log::info!("ending the game's process ({code})");
    log::logger().flush();
    inproc::process::terminate(code)
}

/// Unrecoverable replacement failure must never switch pixel rendering on.
/// Native output recovery is available only through an explicit compat mode.
pub(crate) fn replacement_failed(message: &str) {
    log::error!("{message}");
    eprintln!("civ3-renderer: {message}");
    if !compatibility_fallback() { end_process(1); }
}

fn enabled() -> bool {
    policy().capture && !DRAWING.failed()
}

/// Full capture is permitted by the actual executable load callback,
/// after CRT/C3X initialization. The DllMain path returns before attaching.
pub(crate) fn prepare_at(base: u32, point: CapturePoint) -> Result<bool, String> {
    if point.allowed() { CAPTURE.status()?; }
    if !point.allowed() || !enabled() {
        // The already-loaded DllMain path and intentional UI-off mode
        // retain the text takeover, without complete UI preparation.
        crate::jgl_hooks::install_text(base)?;
        return Ok(false);
    }
    install_configured(base, point)
}

/// Reuse early capture. For late injection only, the initialized graphics
/// object proves that both C3X and jgl loading have finished. This worker
/// never installs full capture just because jgl is mapped during DllMain.
pub fn prepare_worker(game: &Civ3) -> Result<(), String> {
    // Check permanent failure before enabled(), including after the
    // original renderer was restored. A failed startup is not UI=off.
    if !CAPTURE.worker_needs_capture(enabled)? { return Ok(()) }
    if h::ui_sites(game.build).is_none() {
        return Err(format!("unsupported interface capture for {:?}", game.build));
    }
    for _ in 0..1000 {
        if CAPTURE.status()? { return Ok(()) }
        let point = CapturePoint::Worker { graphics_initialized: h::graphics_initialized(game.build) };
        if point.allowed() {
            if let Some(base) = inproc::pe::module_base("jgl.dll") {
                return install_configured(base, point).map(|_| ());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Err("no post-CRT graphics initialization for UI capture".into())
}

fn install_configured(base: u32, point: CapturePoint) -> Result<bool, String> {
    let game_draws = requested_game_draws();
    let installed = CAPTURE.prepare(point, || {
        // Both failures belong to the normal callback transaction. A
        // worker cannot retry across game execution after either failure.
        crate::jgl_hooks::install_text(base)?;
        Civ3::attach().map_err(|e| e.to_string())
    }, |game| {
        // Base_Form is its vtable followed by Base_Form_Data. The map
        // carries city labels over our fog; the overlay carries our bars.
        let map_canvas = game.addrs.main_screen_form + 4 + L::BASE_FORM_DATA_CANVAS;
        let inspect = std::env::var("CIV3R_UI_SKIP").as_deref() == Ok("off");
        let overlay = h::unit_overlay_canvas(game.build);
        let (skip, underlays) = if inspect { (Vec::new(), Vec::new()) } else { (overlay.into_iter().collect(), vec![map_canvas]) };
        UiModel::new(Vec::new(), skip)
            .with_screen(h::back_buffer(game.build).unwrap_or(0))
            .with_generated(h::generated_surfaces(game.build).iter().map(|g| g.0).collect())
            .with_underlays(underlays, vec!["fogofwar.pcx".into()])
            .with_text_backdrops(!gdi_text::under_wine())
            .with_opaque_files(std::sync::Arc::new(crate::ui_opacity::opaque))
    }, |game| install_hooks(game, base).map_err(|e| e.to_string()))?;
    // No original pixel work is skipped until the complete installation
    // succeeded. Keep this dynamic so viewer failure also restores jgl present.
    DRAWING.activate(CAPTURE.ready(), game_draws, |skip| {
        if skip {
            SKIPPER_KEEPS.get_or_init(|| BUILD.get().map_or_else(Vec::new, |&build| h::read_back_images(build)));
        }
        crate::trace::GAME_MAP_OFF.store(crate::game_render::map_off(skip, std::env::var("CIV3R_GAME_MAP").ok().as_deref()), Relaxed);
        if installed {
            log::info!("native rendering policy: original pixels={}, compatibility fallback={}, map pixels off={}, pixel audit={}", game_draws, compatibility_fallback(), crate::trace::GAME_MAP_OFF.load(Relaxed), PIXEL_AUDIT.load(Relaxed));
        }
        if installed && skip { log::info!("game rendering off: the game's drawing is recorded, not done"); }
    });
    Ok(installed)
}

/// Whether drawing is being skipped.
pub fn skipping() -> bool {
    DRAWING.skipping()
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
fn world_canvas(canvas: u32) -> bool {
    let Some(&build) = BUILD.get() else { return false };
    let addrs = build.addresses();
    let map = addrs.main_screen_form + 4 + L::BASE_FORM_DATA_CANVAS;
    // The map renderer's own image (`bic.Map.Renderer`; PCGames 0x9C7560).
    let renderer = addrs.bic_data + L::BIC_MAP + L::MAP_RENDERER;
    canvas == map || canvas == renderer || h::unit_overlay_canvas(build) == Some(canvas)
}

/// Whether the game's drawing onto `canvas` is left unmirrored: while the
/// map is shown, the canvases of the world (see [`world_canvas`]) are the
/// renderer's, which draws all of them from the game's state (terrain,
/// grid, fog, units, the targeting overlays, city labels: §10.15 and
/// `crate::targeting`). Mirroring them was work for nothing, and per pixel
/// for lines: the map grid's 4 lines per tile cost about 20 s per map
/// redraw under FEX. Text on the map canvas is still mirrored (a few dozen
/// labels; whether the game's view shows any). `CIV3R_MAP_LABELS=mirror`
/// and `CIV3R_UI_SKIP=off` mirror the world canvases as before.
fn rerendered(canvas: u32) -> bool {
    static MIRRORED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
        std::env::var("CIV3R_MAP_LABELS").as_deref() == Ok("mirror") || std::env::var("CIV3R_UI_SKIP").as_deref() == Ok("off")
    });
    !*MIRRORED && requested_skipping() == Some(true) && world_canvas(canvas) && !title_canvas(canvas)
}

fn title_canvas(canvas: u32) -> bool {
    let Some(build) = BUILD.get() else { return false };
    let map_canvas = build.addresses().main_screen_form + 4 + L::BASE_FORM_DATA_CANVAS;
    if canvas != map_canvas { return false; }
    let visible = civ3_sys::controls::sites(*build)
        .and_then(|sites| inproc::Ptr32::<u32>::new(sites.main_gui + 0xA0).read().ok())
        .map(|status| status & 1 != 0);
    crate::game_render::retained_title_canvas(true, canvas, map_canvas, visible)
}

fn skips(canvas: u32, source: u32) -> bool {
    let title = (COMPATIBILITY.load(Relaxed) || PIXEL_AUDIT.load(Relaxed)) && title_canvas(canvas);
    let class = if title { PixelClass::Title } else { PixelClass::Ui };
    if !skipping() || (world_canvas(canvas) && !crate::trace::GAME_MAP_OFF.load(Relaxed)) {
        audit_pixel(class, false);
        return false;
    }
    if !has_pixels(canvas) || (source != 0 && !has_pixels(source)) {
        audit_pixel(PixelClass::Empty, false);
        return false;
    }
    let Some(bits) = obj::image_bits(canvas) else {
        audit_pixel(PixelClass::Unknown, false);
        return false;
    };
    if bits <= 8 {
        audit_readback(canvas, "palette_indices");
        return false;
    }
    if obj::keyed_form_canvas(canvas) {
        audit_readback(canvas, "native_hit_mask");
        return false;
    }
    if COMPATIBILITY.load(Relaxed) && title {
        audit_pixel(class, false);
        return false;
    }
    let skip = with_any_skipper(|s| s.skip(canvas)) == Some(true);
    if skip { audit_pixel(class, true); }
    else { audit_readback(canvas, "native_pixel_source"); }
    skip
}

/// What a blit of jgl's sprite group tells of a resized sprite's place
/// (none for the exe detours: the sprite then says).
#[derive(Copy, Clone, Debug, Default)]
struct Placement {
    /// x and y plus the scaled crop offset, as the scaled wrappers pass
    /// jgl's blit (the crop offset in the sprite is the scaled one then).
    at: Option<(i32, i32)>,
    /// Whether jgl compiled the sprite (its pixels may be the placeholder).
    compiled: Option<bool>,
}

/// A sprite drawn resized from the wrapper's (x, y). `placed`: where the
/// wrapper put it, when the sprite's crop offset in memory is the scaled
/// one (the scaled wrappers write it there for the call): x and y plus the
/// scaled crop offset, as jgl's blit gets them.
#[allow(clippy::too_many_arguments)]
fn resized_blit(sprite: u32, canvas: u32, x: u32, y: u32, placed: Placement, scale: [u32; 3], ct: u32, indices: Option<menu::screen::Indices>, clip: Option<[i32; 4]>) {
    if world_canvas(canvas) { return; }
    let Some(crop) = obj::sprite_crop(sprite) else { return };
    let Some(compiled) = placed.compiled.or_else(|| obj::sprite_compiled(sprite)) else { return };
    let geometry = match placed.at {
        None => crate::game_render::scaled_geometry((x as i32, y as i32), crop, scale, compiled),
        Some(at) => crate::game_render::scaled_placed(at, (crop.2, crop.3), scale, compiled),
    };
    let Some(((x, y), size, sample_step)) = geometry else { return };
    if let Some((flic, chunk)) = h::flc_frame(sprite) {
        // FLC_Frame_Image::load at 0x5F7A90 creates Sprite+8 from the full
        // header frame dimensions. 0x5F7B0C..1E copies header offsets into
        // Sprite+0x20/+0x24. Those offsets affect destination geometry
        // above; the decoded chunk starts at source coordinate 0,0.
        let indices = indices.unwrap_or_else(|| menu::screen::Indices { transparent: vec![254, 255], ..Default::default() });
        emit(Event::ResizedFlcBlit { flic, chunk, canvas, x, y, source_size: (crop.2, crop.3), size, sample_step, clip, palette: palette_from(sprite, ct).or_else(|| copied_civ_colors(ct)), indices });
        return;
    }
    if PIXEL_AUDIT.load(Relaxed) {
        let file = with_model(false, |m| matches!(m.sprite_source(sprite), Some(crate::ui::Source::File { .. }))) == Some(true);
        if file { RESIZED_FILE_CAPTURES.fetch_add(1, Relaxed); }
        else { RESIZED_UNSUPPORTED.fetch_add(1, Relaxed); }
    }
    emit(Event::ResizedBlit { sprite, canvas, x, y, size, sample_step, clip, palette: palette_from(sprite, ct).or_else(|| copied_civ_colors(ct)), indices });
}

fn with_skipper<R>(f: impl FnOnce(&mut Skipper) -> R) -> Option<R> {
    if !skipping() {
        return None;
    }
    with_any_skipper(f)
}

/// Whether jgl's pixels of a text run on `canvas` are skipped.
pub fn skips_text_on(canvas: u32) -> bool {
    canvas != 0 && skips(canvas, 0)
}

/// Whether a colour-table blit of `sprite` onto `canvas` (recorded as a
/// unit picture by [`on_colored`]) is skipped.
pub fn skips_colored(sprite: u32, canvas: u32) -> bool {
    skips(canvas, sprite)
}

thread_local! {
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
pub(crate) fn clip_of(canvas: u32) -> Option<[i32; 4]> {
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

thread_local! {
    /// The mirror model, owned by the game's thread: only it draws. Taken
    /// over from the thread that prepared capture at its first use.
    static MODEL: std::cell::RefCell<Option<UiModel>> = const { std::cell::RefCell::new(None) };
    /// What the game shows with the present being mirrored (`emit_present`).
    static PRESENTING: Cell<Option<crate::city_screen::Presented>> = const { Cell::new(None) };
}

/// Native layout and look metadata captured on the game's Present callback.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeUi {
    pub modal_stack: [u32; 4],
    pub controls: Vec<civ3_sys::controls::Control>,
    pub title: Option<civ3_sys::controls::TitleLayout>,
    pub military: Vec<civ3_sys::controls::screens::UnitStrip>,
    pub status_row: Option<(u32, [i32; 4])>,
    pub arts: Vec<crate::looks::ControlArt>,
}

impl NativeUi {
    fn read(model: &UiModel) -> Self {
        let Some(&build) = BUILD.get() else { return Self::default() };
        let Some(sites) = civ3_sys::controls::sites(build) else { return Self::default() };
        let mem = civ3_sys::controls::Live;
        let controls = civ3_sys::controls::controls(&mem, &sites);
        Self {
            modal_stack: civ3_sys::modal::modal_stack(&mem, build).unwrap_or_default(),
            title: civ3_sys::controls::title_layout(&mem, build),
            military: civ3_sys::controls::screens::military_unit_strips(&mem, &controls),
            status_row: civ3_sys::controls::status_unit_row(&mem, &sites, &controls),
            arts: controls.iter().filter_map(|control| control_art(model, &sites, control)).collect(),
            controls,
        }
    }
}

/// The art of `control`'s states, if it is a button the game draws by its
/// state and that state is the pointer's (not forced): the sprites of its
/// pieces as the mirror knows them.
fn control_art(model: &UiModel, sites: &civ3_sys::controls::Sites, control: &civ3_sys::controls::Control) -> Option<crate::looks::ControlArt> {
    use crate::looks::look_state;
    use civ3_sys::control_art::Select;
    let art = civ3_sys::control_art::of(&civ3_sys::controls::Live, sites, control).filter(|a| !a.forced)?;
    let sprites: Vec<u32> = art.pieces.iter().flatten().map(|(_, s)| *s).collect();
    let mut sources = sprites.iter().map(|&sprite| model.sprite_source(sprite).cloned());
    let pieces = art
        .pieces
        .iter()
        .map(|p| p.iter().filter_map(|(l, _)| Some((look_state(*l), sources.next()??))).collect())
        .collect();
    let select = match art.select {
        Select::None => scene::Select::None,
        Select::Toggle => scene::Select::Toggle,
        Select::Pick(group) => scene::Select::Pick { group: group as u64 },
    };
    Some(crate::looks::ControlArt {
        id: control.id,
        select,
        canvas: art.canvas,
        part: match art.part {
            civ3_sys::control_art::Part::Whole => crate::looks::Part::Whole,
            civ3_sys::control_art::Part::LastArt => crate::looks::Part::LastArt(control.rect),
            civ3_sys::control_art::Part::Within => crate::looks::Part::Within(control.rect),
        },
        colors: art.colors.iter().map(|(l, c)| (look_state(*l), c.iter().map(|&c| color_rgb(c)).collect())).collect(),
        state: look_state(art.state),
        pieces,
        text_offsets: art.text_offsets.iter().map(|(l, o)| (look_state(*l), *o)).collect(),
        selected_by_fields: select != scene::Select::None,
    })
}

/// A colour of the game's controls as the mirror has it: text as
/// `SetTextColor` gets it (a packed colour: bit 31 and RGB555, each channel
/// shifted up; else a palette index, black on the game's 16-bit canvases,
/// docs §16.10), fills as RGB555 expanded.
fn color_rgb(c: civ3_sys::control_art::Color) -> [u8; 3] {
    use civ3_sys::control_art::Color;
    match c {
        Color::Text(c) if c & 0x8000_0000 != 0 => [((c >> 7) & 0xF8) as u8, ((c >> 2) & 0xF8) as u8, ((c & 0x1F) << 3) as u8],
        Color::Text(_) => [0, 0, 0],
        Color::Fill(c) => crate::ui::rgb555(c),
    }
}

/// Keeps draw commands, their sprite sources and native metadata at the same
/// completed Present. Reader refreshes cannot expose an in-progress group.
#[derive(Default)]
struct CommittedCapture {
    model: UiModel,
    native: std::sync::Arc<NativeUi>,
    presented: Option<crate::city_screen::Presented>,
}

impl CommittedCapture {
    fn view(&mut self, model: &UiModel, present: bool, presented: Option<crate::city_screen::Presented>, sample: impl FnOnce(&UiModel) -> NativeUi) -> (UiView, bool) {
        let mut changed = false;
        if present {
            let native = sample(model);
            changed = *self.native != native || self.presented != presented;
            if *self.native != native { self.native = std::sync::Arc::new(native); }
            self.model = model.snapshot();
            self.presented = presented;
        }
        (UiView { model: self.model.snapshot(), native: self.native.clone(), stats: model.stats(), presented: self.presented }, changed)
    }
}

thread_local! {
    static COMMITTED: std::cell::RefCell<CommittedCapture> = std::cell::RefCell::new(CommittedCapture::default());
}

/// What other threads read of the mirror: published by the game's thread
/// after each present, and with its next drawing whenever a reader asked
/// for a newer one (`WANT_VIEW`). Readers never wait for the game's thread.
struct UiView {
    /// `UiModel::snapshot`: composes the screen and knows the sprites.
    model: UiModel,
    native: std::sync::Arc<NativeUi>,
    stats: (usize, usize, usize),
    /// What the game showed with the last present mirrored in `model`.
    presented: Option<crate::city_screen::Presented>,
}
static VIEW: handoff::Latest<UiView> = handoff::Latest::new();
static WANT_VIEW: AtomicBool = AtomicBool::new(false);
/// What the publisher last set skipping to (`set_skipping`): 0 not yet, 1
/// off, 2 on. The game's thread applies it at its next drawing; readers
/// compose with it at once.
static SKIPPING: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

fn requested_skipping() -> Option<bool> {
    match SKIPPING.load(Acquire) {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}

/// `f` of the model, on the game's thread; then a view is published if due.
fn with_model<R>(present: bool, f: impl FnOnce(&mut UiModel) -> R) -> Option<R> {
    // The thread that took the model over: the game's, which alone draws.
    static OWNER: std::sync::OnceLock<std::thread::ThreadId> = std::sync::OnceLock::new();
    MODEL.with_borrow_mut(|slot| {
        if slot.is_none() {
            if OWNER.get().is_some_and(|&owner| owner != std::thread::current().id()) {
                static NOTED: AtomicBool = AtomicBool::new(false);
                if !NOTED.swap(true, Relaxed) { log::warn!("UI mirror: drawing on another thread than the game's is not mirrored"); }
                render_errors::report(render_errors::Kind::UnsupportedDraw, "drawing off the game's thread", format_args!("not mirrored"));
                return None;
            }
            *slot = CAPTURE.take_model();
            if slot.is_some() {
                let _ = OWNER.set(std::thread::current().id());
            }
        }
        let model = slot.as_mut()?;
        if let Some(on) = requested_skipping() {
            model.set_skipping(on);
        }
        let result = f(model);
        if WANT_DETAIL.load(Relaxed) && WANT_DETAIL.swap(false, AcqRel) {
            DETAIL.publish(model.detail());
        }
        if present || (WANT_VIEW.load(Relaxed) && WANT_VIEW.swap(false, AcqRel)) {
            let (view, native_changed) = COMMITTED.with_borrow_mut(|committed| committed.view(model, present, PRESENTING.get(), NativeUi::read));
            VIEW.publish(view);
            // A present that changed what the window shows: the publisher
            // goes on at once (a swap and an unpark; this thread never waits).
            thread_local! { static RUNG: Cell<u64> = const { Cell::new(0) }; }
            if present && (RUNG.replace(model.shown_revision()) != model.shown_revision() || native_changed) {
                crate::presenter::WAKE.ring();
            }
        }
        Some(result)
    })
}

fn emit(e: Event) {
    audit_semantic(SemanticClass::SceneEvent);
    // MapMessage draws onto the animator's map overlay. It is rebuilt as
    // tile-anchored world art, never copied into a misplaced UI overlay.
    if crate::unit_notices::drawing() && matches!(&e, Event::Text { .. } | Event::TextClipped { .. } | Event::Fill { .. }) { return; }
    let present = matches!(e, Event::Present { .. });
    watch(&e);
    with_model(present, |m| m.apply(e));
}

/// Logs the events onto or from the canvases in `CIV3R_UI_WATCH` (hex
/// addresses, comma-separated; for finding out how a canvas is drawn).
fn watch(e: &Event) {
    static WATCHED: std::sync::LazyLock<Vec<u32>> = std::sync::LazyLock::new(|| {
        std::env::var("CIV3R_UI_WATCH").unwrap_or_default().split(',')
            .filter_map(|s| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()).collect()
    });
    if WATCHED.is_empty() { return; }
    let touched = match e {
        Event::Loaded { image, .. } | Event::Cleared { image } | Event::Sliced { image, .. } => *image,
        Event::Blit { canvas, .. } | Event::IndexedBlit { canvas, .. } | Event::ResizedBlit { canvas, .. } | Event::OpacityBlit { canvas, .. } | Event::MaskedBlit { canvas, .. }
        | Event::Fill { canvas, .. } | Event::Text { canvas, .. } | Event::TextClipped { canvas, .. } | Event::FlcBlit { canvas, .. } | Event::ResizedFlcBlit { canvas, .. } | Event::Composite { canvas, .. } => *canvas,
        Event::Copy { from, canvas, .. } => if WATCHED.contains(from) { *from } else { *canvas },
        _ => 0,
    };
    if WATCHED.contains(&touched) {
        log::info!("UI watch: {e:?} (clip {:?})", h::canvas_clip(touched));
    }
}

/// On the game's thread only: the screen as of its last present
/// (`UiModel::committed_frame`), with nothing another thread composes in
/// between (`crate::detached` keeps a popup's picture from it).
pub fn committed_frame_now() -> Option<Frame> {
    with_model(false, |m| m.committed_frame())
}

/// On the game's thread only: canvas `canvas` (of a form of class
/// `vtable`) as it is drawn now (`UiModel::canvas_frame`).
pub fn canvas_frame_now(canvas: u32, vtable: u32) -> Option<Frame> {
    with_model(false, |m| m.canvas_frame(canvas, vtable)).flatten()
}

/// The latest view, asking the game's thread for a newer one.
fn view() -> Option<std::sync::Arc<UiView>> {
    if !CAPTURE.ready() { return None }
    WANT_VIEW.store(true, Release);
    VIEW.load()
}

fn compose(view: &UiView) -> Frame {
    match requested_skipping() {
        Some(on) => {
            let mut model = view.model.snapshot();
            model.set_skipping(on);
            model.committed_frame()
        }
        None => view.model.committed_frame(),
    }
}

/// The mirror model's size (see `UiModel::stats`).
pub fn stats() -> Option<(usize, usize, usize)> {
    view().map(|v| v.stats)
}

/// The model in detail as the game's thread last described it on request
/// (`UiModel::detail`), asking it for a newer one (for diagnostics).
static DETAIL: handoff::Latest<String> = handoff::Latest::new();
static WANT_DETAIL: AtomicBool = AtomicBool::new(false);
pub fn detail() -> Option<String> {
    WANT_DETAIL.store(true, Release);
    DETAIL.get()
}

/// The screen as the game last showed it, if the mirror runs.
pub fn frame() -> Option<Frame> {
    frame_presented().map(|(frame, _)| frame)
}

/// A present, and what the game showed with it (`presented`): published
/// with the view made at this present.
fn emit_present(rect: Option<[i32; 4]>, presented: Option<crate::city_screen::Presented>) {
    PRESENTING.set(presented);
    emit(Event::Present { rect });
}

/// [`frame`], and what the game showed when it presented it (its city
/// screen's city and camera: `crate::city_screen::screen_state`), from
/// the same publication.
pub fn frame_presented() -> Option<(Frame, Option<crate::city_screen::Presented>)> {
    snapshot().map(|snapshot| (snapshot.frame, snapshot.presented))
}

#[derive(Clone, Default)]
pub(crate) struct SnapshotSources(Option<std::sync::Arc<UiView>>);

impl SnapshotSources {
    pub fn get(&self, sprites: &[u32]) -> Vec<Option<crate::ui::Source>> {
        let Some(view) = &self.0 else { return vec![None; sprites.len()] };
        sprites.iter().map(|&sprite| if sprite == 0 { None } else { view.model.sprite_source(sprite).cloned() }).collect()
    }
}

pub(crate) struct Snapshot {
    pub frame: Frame,
    pub presented: Option<crate::city_screen::Presented>,
    pub native: std::sync::Arc<NativeUi>,
    pub sources: SnapshotSources,
}

/// One immutable Present for commands, native hit geometry and look recipes.
pub(crate) fn snapshot() -> Option<Snapshot> {
    view().map(|view| Snapshot {
        frame: compose(&view), presented: view.presented, native: view.native.clone(), sources: SnapshotSources(Some(view)),
    }).or_else(|| {
        CAPTURE.ready().then(|| Snapshot { frame: Frame::default(), presented: None, native: Default::default(), sources: Default::default() })
    })
}

/// Where the pixels of each of `sprites` (game `Sprite*`) come from, as the
/// mirror knows them (`None` for 0 and unknown ones).
pub fn sprite_sources(sprites: &[u32]) -> Vec<Option<crate::ui::Source>> {
    let Some(view) = view() else { return vec![None; sprites.len()] };
    sprites.iter().map(|&s| if s == 0 { None } else { view.model.sprite_source(s).cloned() }).collect()
}

/// Text drawn by jgl's `text_out` onto `canvas` (canvas coordinates), with
/// the canvas's clip at the call.
pub fn on_text(canvas: u32, clip: [i32; 4], run: &TextRun) {
    if canvas != 0 {
        let dirty = if BACK_BUFFER.get() == Some(&canvas) { DIRTY.with(Cell::get) } else { None };
        let clip = crate::ui::content_clip(Some(clip), dirty).unwrap_or([i32::MIN, i32::MIN, i32::MAX, i32::MAX]);
        emit(Event::TextClipped { canvas, run: run.clone(), clip });
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

/// A sprite drawn at (x, y): its stored pixels start at the crop offset.
/// `ct`: the colour table it is drawn through (0: its own palette).
fn blit(sprite: u32, canvas: u32, x: u32, y: u32, add_crop: bool, ct: u32, clip: Option<[i32; 4]>) {
    if rerendered(canvas) { return; }
    let (cx, cy) = if add_crop { obj::sprite_crop(sprite).map_or((0, 0), |c| (c.0, c.1)) } else { (0, 0) };
    // A frame of an FLC animation in its own colours (a leader head).
    if let Some((flic, chunk)) = h::flc_frame(sprite) {
        emit(Event::FlcBlit { flic, chunk, canvas, x: x as i32 + cx, y: y as i32 + cy, clip, plain: true });
        return;
    }
    let palette = palette_from(sprite, ct);
    emit(Event::Blit { sprite, canvas, x: x as i32 + cx, y: y as i32 + cy, clip, palette });
}

fn palette_from(sprite: u32, ct: u32) -> Option<menu::screen::PaletteFrom> {
    // Through a civ colour table: that table's palette.
    let table = BUILD.get().and_then(|&b| h::civ_color_table(b, ct));
    if ct != 0 && table.is_none() {
        // (Once per table: which ones are not the civs'.)
        thread_local! { static SEEN: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(Vec::new()) }; }
        if SEEN.with_borrow_mut(|seen| !seen.contains(&ct) && seen.len() < 64 && { seen.push(ct); true }) {
            log::debug!("sprite {sprite:#x} drawn through colour table {ct:#x}, not a civ's");
        }
    }
    let ntp = |n: u32, count: u32| menu::screen::PaletteFrom { image: format!("Art/Units/Palettes/ntp{n:02}.pcx"), first: 0, count };
    // Else a sprite of its own colour table, recoloured by copying a civ's
    // first 64 colours into it.
    let palette = table.map(|n| ntp(n, 256)).or_else(|| own_civ_colors(sprite).map(|n| ntp(n, 64)));
    palette
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
/// Written by the game's thread (the only writer), read by the publisher.
static FORM_SURFACES: handoff::Latest<Vec<(u32, &'static str)>> = handoff::Latest::new();

/// The surfaces the game fills itself: the fixed ones and the forms' own
/// seen so far (see `ui_screen::items`).
pub fn generated_surfaces(build: civ3_sys::Build) -> Vec<(u32, &'static str)> {
    let mut out = h::generated_surfaces(build).to_vec();
    out.extend(form_surfaces());
    out
}

/// The forms' own surfaces seen so far: image and name.
pub fn form_surfaces() -> Vec<(u32, &'static str)> {
    FORM_SURFACES.get().unwrap_or_default()
}

/// A copy of `from` onto `canvas`: if `from` is a surface of the form
/// whose canvas it is, the model passes it on as it is from now on.
fn note_form_surface(from: u32, canvas: u32) {
    let Some(&build) = BUILD.get() else { return };
    if BACK_BUFFER.get() == Some(&canvas) {
        return;
    }
    let Some(s) = h::form_surface(build, from, canvas) else { return };
    let mut seen = FORM_SURFACES.get().unwrap_or_default();
    if seen.iter().any(|&(image, _)| image == from) {
        return;
    }
    seen.push((from, s.name));
    FORM_SURFACES.publish(seen);
    log::debug!("UI mirror: {} at {from:#x}", s.name);
    with_model(false, |m| m.add_generated(from));
}

/// `image` lost its pixels: if it was a form's surface, it is an image like
/// any other again (its memory may be another's next).
fn forget_form_surface(image: u32) {
    let mut seen = FORM_SURFACES.get().unwrap_or_default();
    let Some(i) = seen.iter().position(|&(i, _)| i == image) else { return };
    seen.remove(i);
    FORM_SURFACES.publish(seen);
    with_model(false, |m| m.remove_generated(image));
}

/// Region `src` (x, y, w, h) of `from` copied to (x, y) of `canvas`, within
/// the canvas's clip rectangle `clip` (the game's copies keep to it: e.g. a
/// list box redrawing one row through tiles of its background).
fn copy(from: u32, src: [i32; 4], canvas: u32, x: i32, y: i32, keyed: bool, clip: Option<[i32; 4]>) {
    if rerendered(canvas) { return; }
    note_form_surface(from, canvas);
    let (mut src, mut x, mut y) = (src, x, y);
    if let Some([l, t, r, b]) = clip {
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

/// The clip of a draw onto `canvas` from its jgl call's clip (see
/// [`clip_of`]): on the back buffer, a paint's rectangle is no clip.
fn clip_from(canvas: u32, clip: Option<[i32; 4]>) -> Option<[i32; 4]> {
    let dirty = if BACK_BUFFER.get() == Some(&canvas) { DIRTY.with(Cell::get) } else { None };
    crate::ui::content_clip(clip, dirty)
}

/// A fill, line or translucent rectangle from jgl's fill group
/// (`civ3_ui::draw`): the canvas's size first, when given, then the
/// rectangles.
pub fn on_fill(canvas: u32, size: Option<(i32, i32)>, rects: &[[i32; 4]], rgb: [u8; 3], alpha: u8, clip: Option<[i32; 4]>) {
    if rerendered(canvas) { return; }
    if let Some(size) = size {
        emit(Event::Size { image: canvas, size });
    }
    let clip = clip_from(canvas, clip);
    for &rect in rects {
        emit(Event::Fill { canvas, rect, rgb, alpha, clip });
    }
}

/// A copy from jgl's copy group (`civ3_ui::draw`): the sizes of the canvas
/// and of the one copied from first, when given.
#[allow(clippy::too_many_arguments)]
pub fn on_copy(from: u32, src: [i32; 4], canvas: u32, x: i32, y: i32, keyed: bool, sizes: Option<((i32, i32), (i32, i32))>, clip: Option<[i32; 4]>) {
    if rerendered(canvas) { return; }
    if let Some((canvas_size, from_size)) = sizes {
        emit(Event::Size { image: canvas, size: canvas_size });
        emit(Event::Size { image: from, size: from_size });
    }
    copy(from, src, canvas, x, y, keyed, clip_from(canvas, clip));
}

/// Whether jgl's drawing onto `canvas` (from `source`, 0: none), taken by
/// the interface model, is skipped.
pub fn skips_drawing(canvas: u32, source: u32) -> bool {
    skips(canvas, source)
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

/// Install all hooks while the startup capture serializes callers. The
/// model already exists and is retained across attempts and viewer startup.
fn install_hooks(game: &Civ3, jgl: u32) -> Result<UiTracer, HookError> {
    let sites = h::ui_sites(game.build).ok_or(HookError::Unsupported)?;
    let _ = BUILD.set(game.build);
    let _ = BACK_BUFFER.set(h::back_buffer(game.build).unwrap_or(0));
    let mut hooks: Vec<Box<dyn std::any::Any + Send>> = Vec::new();
    match crate::city_screen::native::install(*game) {
        Ok(hook) => hooks.push(hook),
        Err(error) => log::warn!("city work eligibility unavailable: {error}"),
    }
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
        // Sprites the game draws into images it reads back keep jgl's
        // pixels (`READ_BACK_ART`).
        let real = placeholder_sprites() && with_model(false, |m| m.file_of(c.image).is_some_and(read_back_art)) == Some(true);
        let previous = REAL_SPRITES.with(|r| r.replace(real));
        let r = c.original();
        REAL_SPRITES.with(|r| r.set(previous));
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
    // The keyed forms' hit test reads the pixel under the mouse from the
    // form's canvas. With placeholder sprites jgl draws no sprite there:
    // the interface model answers it, from what the viewer shows.
    if let Some((site, expected)) = h::keyed_hit_test_site(game.build).filter(|_| placeholder_sprites()) {
        hooks.push(Box::new(h::keyed_hit_pixel::install_call_site(site, expected, |c| keyed_hit_pixel(c.image, c.x as i32, c.y as i32).unwrap_or_else(|| c.original()))?));
    }
    // The game reads a pixel (what is under the mouse): drawing onto that
    // image goes on from now on (see `skips` for the ones known before).
    hook!(pcx_get_pixel, sites.pcx_get_pixel, |c| {
        audit_semantic(SemanticClass::PixelRead);
        if with_skipper(|s| s.reads(c.image)) == Some(true) {
            log::warn!("game rendering: the game read a pixel of image {:#x}, drawn onto with drawing skipped; drawing onto it from now on", c.image);
        }
        placeholder_read(c.image, "get_pixel");
        c.original()
    });
    hook!(form_composite, sites.form_composite, |c| {
        if let Some(p) = obj::placement(c.form) {
            emit(Event::Composite { canvas: p.canvas, vtable: p.vtable, origin: p.origin, size: p.size, clip: p.clip, keyed: p.keyed });
        }
        c.original();
        emit(Event::CompositeEnd);
    });
    let paint_game = *game;
    hook!(paint, sites.paint, move |c| {
        refresh_original_drawing(paint_game);
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
    let compose_game = *game;
    hook!(compose_all, sites.compose_all, move |c| {
        refresh_original_drawing(compose_game);
        // m73 invalidates client canvases. Modal headings and frames are
        // m80 draws straight onto the back buffer, so the first restored
        // composition must include the complete screen rectangle too.
        let full = !ORIGINAL_REFRESHING.with(Cell::get) && ORIGINAL_COMPOSE_ALL.swap(false, AcqRel);
        if full { log::debug!("original renderer: composing complete restored screen"); }
        let all = full || c.form == 0 || modal_root.is_some_and(|at| inproc::Ptr32::<u32>::new(at).read().ok() == Some(0));
        if !all || Painting::inside() {
            return if full { c.original_with(0, 0) } else { c.original() };
        }
        emit(Event::PaintBegin);
        let _painting = Painting::enter();
        updating(if full { 0 } else { c.rect }, || if full { c.original_with(0, 0) } else { c.original() });
        emit(Event::PaintEnd);
    });
    let back_buffer = h::back_buffer(game.build).unwrap_or(0);
    let presenting = *game;
    hook!(present, sites.present, move |c| {
        // It draws the tooltip and the cursor onto the back buffer first.
        let full = !ORIGINAL_REFRESHING.with(Cell::get) && !ORIGINAL_COMPOSE_ALL.load(Acquire)
            && ORIGINAL_PRESENT_ALL.swap(false, AcqRel);
        if full { log::debug!("original renderer: presenting complete restored screen"); }
        let rect = if full { 0 } else { c.rect };
        emit(Event::PresentBegin);
        updating(rect, || if full { c.original_with(0) } else { c.original() });
        size_of(back_buffer);
        // (Read on this thread, as the screen is presented: the publisher
        // composes the world it reads later only with a matching screen.)
        let presented = presenting.view().ok().map(|v| crate::city_screen::Presented {
            focus_city: presenting.focus_city().ok().flatten(),
            camera: (v.camera_x, v.camera_y),
        });
        crate::city_screen::native::present(&presenting);
        emit_present(if rect == 0 { None } else { obj::rect(rect) }, presented);
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
            let skip = skipping() && replay_terrain;
            if !skip { audit_map_original("navigator_terrain", 0, c.image, &[c.width, c.height]); }
            audit_call(PixelClass::Replay, skip, || { c.original(); 0 });
        });
        hook!(navigator_tint, sites.tint, |c| {
            let tracked = replay_operation(c.navigator, crate::replay::Operation::Tint { x: c.x as i32, y: c.y as i32, color: c.color as u16 });
            audit_call(PixelClass::Replay, tracked && skipping(), || { c.original(); 0 });
        });
        hook!(navigator_restore, sites.restore, |c| {
            let tracked = replay_operation(c.navigator, crate::replay::Operation::Restore { x: c.x as i32, y: c.y as i32 });
            audit_call(PixelClass::Replay, tracked && skipping(), || { c.original(); 0 });
        });
        hook!(navigator_add_city, sites.add_city, |c| {
            let tracked = replay_operation(c.navigator, crate::replay::Operation::AddCity { x: c.x as i32, y: c.y as i32 });
            audit_call(PixelClass::Replay, tracked && skipping(), || { c.original(); 0 });
        });
        hook!(navigator_erase_city, sites.erase_city, |c| {
            let tracked = replay_operation(c.navigator, crate::replay::Operation::EraseCity { x: c.x as i32, y: c.y as i32 });
            audit_call(PixelClass::Replay, tracked && skipping(), || { c.original(); 0 });
        });
    }
    hook_lines(game, &mut hooks);
    hook_jgl(jgl)?;
    // Start one lazy worker after fallible hook setup succeeds. Its world
    // reader runs only on a replay Init command, never during installation.
    if h::replay_sites(game.build).is_some() && REPLAY.get().is_none() {
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
    }
    if std::env::var("CIV3R_TRACE_PIXELS").as_deref() == Ok("1") {
        trace_pixel_access(jgl);
    }
    log::info!("UI mirror: {} hooks", hooks.len());
    Ok(UiTracer { _hooks: hooks })
}



/// Install jgl capture synchronously at the known loaded base. Present
/// is always taken over, including drawing-ON startup, and asks whether to
/// skip at each call, so restoring the original renderer takes effect
/// immediately.
fn hook_jgl(base: u32) -> Result<(), HookError> {
    // Fills, lines, shades and copies: jgl's own method groups, taken over.
    crate::jgl_hooks::install_fill(base)?;
    crate::jgl_hooks::install_copy(base)?;
    // Sprite blits, their state and the colour tables: the sprite group.
    crate::jgl_hooks::install_blit(base)?;
    // Invalidate and present.
    crate::jgl_hooks::install_present(base)?;
    Ok(())
}

/// Logs (once per image) who takes the pixels of an image that a skipped
/// drawing went onto: the game reading back what it did not draw, or
/// drawing over it with a drawing not mirrored (`CIV3R_TRACE_PIXELS=1`,
/// for finding such readers; see `skips` for the ones kept drawing). The
/// caller is the first return address into the game's code on the stack
/// (possibly a stale one).
fn trace_pixel_access(base: u32) {
    thread_local! { static SEEN: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(Vec::new()) }; }
    fn note(image: u32) {
        let Some(owner) = h::jgl_owner(image) else { return };
        placeholder_read(owner, "pixel_at/lock");
        if with_skipper(|s| s.was_skipped(owner) && !s.keeps(owner)) != Some(true) {
            return;
        }
        // Only the first time (the stack walk is slow).
        if !SEEN.with_borrow_mut(|seen| seen.len() < 256 && !seen.contains(&owner) && { seen.push(owner); true }) {
            return;
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

/// What a line drawer is set to: canvas, colour, opacity, width.
#[derive(Copy, Clone)]
struct Pen {
    canvas: u32,
    rgb: [u8; 3],
    alpha: u8,
    width: i32,
}

thread_local! {
    /// The line drawers' state, on the game's thread (which alone draws).
    static PENS: std::cell::RefCell<Vec<(u32, Pen)>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn with_pen(renderer: u32, f: impl FnOnce(&mut Pen)) {
    PENS.with_borrow_mut(|pens| {
        let i = match pens.iter().position(|p| p.0 == renderer) {
            Some(i) => i,
            None => {
                pens.push((renderer, Pen { canvas: 0, rgb: [0; 3], alpha: 255, width: 1 }));
                pens.len() - 1
            }
        };
        f(&mut pens[i].1);
    });
}

/// Hooks the game's line drawer (redirected by C3X: hooked over its jump).
fn hook_lines(game: &Civ3, hooks: &mut Vec<Box<dyn std::any::Any + Send>>) {
    let Some([bind, color, opacity, width, draw]) = h::line_sites(game.build) else { return };
    let mut install = |r: Result<Box<dyn std::any::Any + Send>, HookError>| match r {
        Ok(hook) => hooks.push(hook),
        Err(e) => {
            log::warn!("UI mirror: line drawer not mirrored: {e}");
            render_errors::report(render_errors::Kind::UnsupportedDraw, "the game's lines (no hook)", format_args!("{e}"));
        }
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
        if p.alpha > 0 && !rerendered(p.canvas) {
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


/// Whether the skipped canvases (the map) are left out: while a game runs.
/// Between games the title screen is drawn on the same canvas.
pub fn set_skipping(on: bool) {
    // (On the publisher: the game's thread applies it with its next drawing.)
    SKIPPING.store(if on { 2 } else { 1 }, Release);
}

/// The game starts redrawing the map (terrain over the old labels) into
/// `map_renderer`, an image it then copies onto the map canvas: what both
/// showed is replaced.
pub fn map_redraw(map_renderer: u32) {
    with_model(false, |m| {
        m.reset_underlays();
        m.reset(map_renderer);
    });
}

/// A map-sprite blit (`draw_on_map`) onto an interface canvas (Top 5
/// Cities): the main map pass is the renderer's (`placed`: see
/// [`resized_blit`]).
#[allow(clippy::too_many_arguments)]
fn map_sprite_events(sprite: u32, canvas: u32, x: u32, y: u32, placed: Placement, scale: [u32; 3], ct: u32, clip: Option<[i32; 4]>) {
    if scale[0] == scale[1] && scale[1] == scale[2] {
        if !world_canvas(canvas) { blit(sprite, canvas, x, y, true, ct, clip); }
    } else {
        resized_blit(sprite, canvas, x, y, placed, scale, ct, None, clip);
    }
}

/// A blend-table blit (`Sprite::draw_blend`, alone or through its scaled
/// wrapper) onto an interface canvas: the shadows of resources and terrain
/// buildings, e.g. in the tile information box's picture of the map.
/// `table`: the blend tables (`placed`: see [`resized_blit`]).
#[allow(clippy::too_many_arguments)]
fn map_blend_events(sprite: u32, canvas: u32, x: u32, y: u32, placed: Placement, table: u32, scale: [u32; 3], ct: u32, clip: Option<[i32; 4]>) {
    // (Nothing is mirrored onto the world's canvases: no tables to look up.)
    if world_canvas(canvas) { return; }
    match BUILD.get().and_then(|&b| blend_levels(b, table)) {
        Some(levels) => {
            let indices = crate::ui::indices::blend(levels);
            if scale[0] == scale[1] && scale[1] == scale[2] { indexed_blit(sprite, canvas, x, y, indices, None, clip); }
            else { resized_blit(sprite, canvas, x, y, placed, scale, ct, Some(indices), clip); }
        }
        // A table not known here uses the ordinary sprite path.
        _ => map_sprite_events(sprite, canvas, x, y, placed, scale, ct, clip),
    }
}

/// The levels of the set of blend tables `table` is (the value of one of
/// the game's pointers to them), if it is one.
fn blend_levels(build: civ3_sys::Build, table: u32) -> Option<&'static [u8]> {
    let t = h::blend_tables(build)?;
    let read = |at: u32| inproc::Ptr32::<u32>::new(at).read().ok().filter(|&p| p != 0);
    if table == 0 { None }
    else if read(t.shadows) == Some(table) { Some(&crate::ui::indices::SHADOW_LEVELS) }
    else if read(t.levels) == Some(table) { Some(&crate::ui::indices::SHADE_LEVELS) }
    else { None }
}

/// A sprite drawn at (x, y) (its stored pixels at the crop offset), its
/// palette indices as `indices` says.
fn indexed_blit(sprite: u32, canvas: u32, x: u32, y: u32, indices: menu::screen::Indices, palette: Option<menu::screen::PaletteFrom>, clip: Option<[i32; 4]>) {
    if rerendered(canvas) { return; }
    let (cx, cy) = obj::sprite_crop(sprite).map_or((0, 0), |c| (c.0, c.1));
    emit(Event::IndexedBlit { sprite, canvas, x: x as i32 + cx, y: y as i32 + cy, clip, palette, indices });
}

/// A colour-table blit (the tracer's hook): unit animation frames drawn on
/// interface canvases become unit pictures (§16.9). `canvas` is the
/// destination (the blit's second canvas argument); `table` its blend
/// tables, `ct` its colour table.
pub fn on_colored(sprite: u32, canvas: u32, x: u32, y: u32, table: u32, ct: u32, clip: Option<[i32; 4]>) {
    if world_canvas(canvas) { return; }
    if let Some((flic, chunk)) = h::flc_frame(sprite) {
        // The frame's image lies at its offset from (x, y), as for map
        // units (§10.3).
        let (cx, cy) = obj::sprite_crop(sprite).map_or((0, 0), |c| (c.0, c.1));
        emit(Event::FlcBlit { flic, chunk, canvas, x: x as i32 + cx, y: y as i32 + cy, clip, plain: false });
        return;
    }
    // Through the shade tables (the territory borders in the tile
    // information box): its colours, shades and the indices its file lost.
    let shades = BUILD.get().and_then(|&b| blend_levels(b, table)) == Some(&crate::ui::indices::SHADE_LEVELS[..]);
    let path = with_model(false, |m| match m.sprite_source(sprite) {
        Some(crate::ui::Source::File { path, .. }) => Some(path.clone()),
        _ => None,
    }).flatten();
    match path.filter(|_| shades) {
        Some(path) => {
            let palette = palette_from(sprite, ct).or_else(|| copied_civ_colors(ct));
            indexed_blit(sprite, canvas, x, y, crate::ui::indices::colored(crate::ui::indices::blank(&path)), palette, clip);
            indexed_blit(sprite, canvas, x, y, crate::ui::indices::colored_shades(), None, clip);
            indexed_blit(sprite, canvas, x, y, crate::ui::indices::colored_smoke(), None, clip);
        }
        None => blit(sprite, canvas, x, y, true, ct, clip),
    }
}

/// A colour table made from another with entries 64 and 65 of a civ's (the
/// territory borders' owner colours, 0x4C3B87..0x4C3BEC): those of its
/// `ntpNN.pcx`.
fn copied_civ_colors(ct: u32) -> Option<menu::screen::PaletteFrom> {
    if ct == 0 { return None; }
    let entries = h::color_table_entries(ct, 64, 2)?;
    let n = h::civ_table_with(*BUILD.get()?, 64, &entries)?;
    Some(menu::screen::PaletteFrom { image: format!("Art/Units/Palettes/ntp{n:02}.pcx"), first: 64, count: 2 })
}

thread_local! {
    /// Canvases jgl would have drawn sprites onto for the game to read
    /// back (the exe detours kept them), had their sprites pixels: with
    /// placeholder sprites they have none. Why it was kept, and whether
    /// the game was seen reading one.
    static PLACEHOLDER_LOST: std::cell::RefCell<std::collections::HashMap<u32, (&'static str, bool)>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// A blit onto `canvas` that the old decision drew with jgl, left out for
/// the placeholder sprites (`jgl_hooks`): logged once per canvas with the
/// reason it was kept.
pub(crate) fn placeholder_lost(canvas: u32) {
    let reason = if !skipping() { "game draws" }
        else if world_canvas(canvas) && !crate::trace::GAME_MAP_OFF.load(Relaxed) { "world canvas, game map on" }
        else if obj::image_bits(canvas).is_some_and(|bits| bits <= 8) { "palette_indices" }
        else if obj::keyed_form_canvas(canvas) { "native_hit_mask" }
        else if with_any_skipper(|s| s.keeps(canvas)) == Some(true) { "native_pixel_source" }
        else { "other" };
    let new = PLACEHOLDER_LOST.with_borrow_mut(|l| l.len() < 256 && !l.contains_key(&canvas) && { l.insert(canvas, (reason, false)); true });
    if new {
        log::info!("placeholder sprites: blits onto {canvas:#x} kept by the old path ({reason}) left out");
    }
}

/// The game reads pixels of `image` (`how`): if blits onto it were left
/// out for the placeholder sprites, logged once.
fn placeholder_read(image: u32, how: &str) {
    let first = PLACEHOLDER_LOST.with_borrow_mut(|l| match l.get_mut(&image) {
        Some((_, seen @ false)) => { *seen = true; true }
        _ => false,
    });
    if first {
        log::warn!("placeholder sprites: the game reads pixels of {image:#x} ({how}), whose sprite blits were left out");
    }
}

/// The pixel of keyed form canvas `canvas` at (x, y) for the game's hit
/// test, from what the interface model shows there
/// (`UiModel::shown_at`, the files' pixels from `ui_opacity::pixel`):
/// the colour key where it is see-through, else its colour as the
/// canvas's 16-bit value. `None` (jgl's pixel then) outside the canvas
/// or when the model has no answer.
fn keyed_hit_pixel(canvas: u32, x: i32, y: i32) -> Option<u32> {
    let (w, hh) = obj::image_size(canvas)?;
    if x < 0 || y < 0 || x >= w || y >= hh {
        return None;
    }
    let key = inproc::Ptr32::<u32>::new(h::COLOR_KEY).read().ok()? & 0x7FFF_FFFF;
    let shown = with_model(false, |m| m.shown_at(canvas, x, y, &crate::ui_opacity::pixel))?;
    let answer = match shown {
        crate::ui::Shown::Rgb(rgb) if rgb == crate::ui::KEY_RGB => key,
        crate::ui::Shown::Rgb([r, g, b]) => (u32::from(r >> 3) << 10) | (u32::from(g >> 3) << 5) | u32::from(b >> 3),
        // Nothing drawn: cleared to the key (the model drops what a fill
        // of the key clears); a file not read yet: the form, as a drawn
        // pixel would be.
        crate::ui::Shown::Nothing => key,
        crate::ui::Shown::Unknown => key ^ 1,
        crate::ui::Shown::NotMirrored => return None,
    };
    if HIT_LOG.with(|n| { let k = n.get(); n.set(k + 1); k < 40 }) {
        log::debug!("keyed hit test: canvas {canvas:#x} ({x}, {y}): {shown:?} -> {answer:#x}");
    }
    Some(answer)
}

thread_local! { static HIT_LOG: Cell<u32> = const { Cell::new(0) }; }

/// Whether `sprite` drawn onto `canvas` is the game's mouse cursor, which
/// it draws onto the screen last (docs §16.5, §16.11): not mirrored, the
/// viewer shows the pointer itself, where the mouse is now (a mirrored one
/// would lag behind it).
fn is_cursor(sprite: u32, canvas: u32) -> bool {
    let Some(&build) = BUILD.get() else { return false };
    canvas != 0 && Some(canvas) == h::back_buffer(build) && sprite != 0
        && civ3_sys::cursor::sites(build).is_some_and(|s| inproc::Ptr32::<u32>::new(s.sprite).read().ok() == Some(sprite))
}

/// The wrapper's own (x, y) of a blit of jgl's sprite group: what it
/// passed jgl, less the crop offset it added. `Sprite::draw` and
/// `draw_on_map` add the crop offset times the draw scale (truncated, as
/// the exe's `imul`/`idiv`), the others the crop offset in the sprite (the
/// scaled wrappers have put the scaled one there), the masked ones none.
fn wrapper_xy(b: &GameBlit) -> (u32, u32) {
    use civ3_ui::blit::Wrapper as W;
    let (cx, cy) = obj::sprite_crop(b.sprite).map_or((0, 0), |c| (c.0, c.1));
    let (dx, dy) = match b.wrapper {
        W::Alpha | W::Masked => (0, 0),
        W::Draw | W::DrawOnMap => {
            let [nx, ny, den] = b.scale.map(|v| v as i32);
            if den == 0 { (cx, cy) } else { (cx.wrapping_mul(nx).wrapping_div(den), cy.wrapping_mul(ny).wrapping_div(den)) }
        }
        _ => (cx, cy),
    };
    (b.at.0.wrapping_sub(dx) as u32, b.at.1.wrapping_sub(dy) as u32)
}

/// Where a scaled wrapper put its sprite (see [`resized_blit`]).
fn placed(b: &GameBlit) -> Placement {
    use civ3_ui::blit::Wrapper as W;
    let at = matches!(b.wrapper, W::ScaledA4 | W::ScaledA8 | W::OnMapBlend | W::ScaledColored).then_some(b.at);
    Placement { at, compiled: Some(b.compiled) }
}

/// The draws of [`civ3_ui::blit::Wrapper::ScaledColored`] onto `canvas`
/// (its second canvas) through `flags` (its blend tables).
#[allow(clippy::too_many_arguments)]
fn scaled_colored(sprite: u32, canvas: u32, x: u32, y: u32, placed: Placement, scale: [u32; 3], flags: u32, ct: u32, clip: Option<[i32; 4]>) {
    let shades = BUILD.get().and_then(|&b| blend_levels(b, flags)) == Some(&crate::ui::indices::SHADE_LEVELS[..]);
    let path = with_model(false, |m| match m.sprite_source(sprite) { Some(crate::ui::Source::File { path, .. }) => Some(path.clone()), _ => None }).flatten();
    if shades && (path.is_some() || h::flc_frame(sprite).is_some()) {
        for indices in [crate::ui::indices::colored(path.as_deref().map_or(&[254, 255][..], crate::ui::indices::blank)), crate::ui::indices::colored_shades(), crate::ui::indices::colored_smoke()] {
            resized_blit(sprite, canvas, x, y, placed, scale, ct, Some(indices), clip);
        }
    } else {
        resized_blit(sprite, canvas, x, y, placed, scale, ct, None, clip);
    }
}

/// [`civ3_ui::blit::Wrapper::Opacity`]'s draw: `opacity` the float's bits.
#[allow(clippy::too_many_arguments)]
fn opacity_blit(sprite: u32, canvas: u32, x: u32, y: u32, opacity: u32, ct: u32, clip: Option<[i32; 4]>) {
    if rerendered(canvas) { return; }
    let opacity = crate::ui::sprite_opacity(opacity);
    let (cx, cy) = obj::sprite_crop(sprite).map_or((0, 0), |p| (p.0, p.1));
    let palette = BUILD.get().and_then(|&b| h::civ_color_table(b, ct)).map(|n| menu::screen::PaletteFrom {
        image: format!("Art/Units/Palettes/ntp{n:02}.pcx"), first: 0, count: 256,
    });
    emit(Event::OpacityBlit { sprite, canvas, x: x as i32 + cx, y: y as i32 + cy, clip, palette, opacity });
}

/// A sprite through an alpha mask, at (x, y) (no crop offset).
fn masked_blit(sprite: u32, mask: u32, canvas: u32, x: u32, y: u32, clip: Option<[i32; 4]>) {
    if !rerendered(canvas) { emit(Event::MaskedBlit { sprite, mask, canvas, x: x as i32, y: y as i32, clip }); }
}

/// A blit of jgl's sprite group (`civ3_ui::blit`), from the exe function
/// `b.wrapper`: the interface mirror's events, and the map tracer's record
/// for the map's three.
pub fn on_blit(b: &GameBlit) {
    use civ3_ui::blit::Wrapper as W;
    let (x, y) = wrapper_xy(b);
    let clip = clip_from(b.canvas, b.clip);
    let (sprite, canvas, ct) = (b.sprite, b.canvas, b.ct);
    match b.wrapper {
        W::Draw => {
            if !is_cursor(sprite, canvas) { blit(sprite, canvas, x, y, true, ct, clip); }
        }
        W::Variant | W::A4 | W::A4b => blit(sprite, canvas, x, y, true, ct, clip),
        W::ScaledA4 | W::ScaledA8 => resized_blit(sprite, canvas, x, y, placed(b), b.scale, ct, None, clip),
        W::ScaledColored => scaled_colored(sprite, canvas, x, y, placed(b), b.scale, b.extra, ct, clip),
        W::Blend => map_blend_events(sprite, canvas, x, y, placed(b), b.extra, [1, 1, 1], ct, clip),
        W::Opacity => opacity_blit(sprite, canvas, x, y, b.extra, ct, clip),
        W::Alpha | W::Masked => masked_blit(sprite, b.mask, canvas, x, y, clip),
        W::DrawOnMap => {
            let [nx, ny, den] = b.scale;
            if !crate::trace::record_map(canvas, sprite, x, y, vec![nx, ny, den, ct], "map") {
                map_sprite_events(sprite, canvas, x, y, placed(b), b.scale, ct, clip);
            }
        }
        W::OnMapBlend => {
            let [nx, ny, den] = b.scale;
            if !crate::trace::record_map(canvas, sprite, x, y, vec![b.extra, nx, ny, den, ct], "blend") {
                map_blend_events(sprite, canvas, x, y, placed(b), b.extra, b.scale, ct, clip);
            }
        }
        W::WithColorTable => {
            crate::trace::record_colored(sprite, x, y, vec![b.other, canvas, b.extra, ct]);
            on_colored(sprite, canvas, x, y, b.extra, ct, clip);
        }
    }
}

/// Whether jgl's pixels of a blit of the sprite group are skipped: the
/// decision the exe detours made for the same call.
pub fn skips_blit(b: &GameBlit) -> bool {
    use civ3_ui::blit::Wrapper as W;
    let (sprite, canvas) = (b.sprite, b.canvas);
    let map = |skip: bool, kind: &str, args: &[u32]| {
        if !skip { audit_map_original(kind, sprite, canvas, args); }
        audit_pixel(PixelClass::Map, skip);
        skip
    };
    let (x, y) = wrapper_xy(b);
    match b.wrapper {
        W::DrawOnMap => {
            let skip = crate::trace::map_blits_off() || (!crate::trace::in_map_pass() && skips(canvas, sprite));
            map(skip, "map_sprite", &[x, y, b.scale[0], b.scale[1], b.scale[2], b.ct])
        }
        W::OnMapBlend => {
            let skip = crate::trace::map_blits_off() || (!crate::trace::in_map_pass() && skips(canvas, sprite));
            map(skip, "map_blend", &[x, y, b.extra, b.scale[0], b.scale[1], b.scale[2], b.ct])
        }
        W::WithColorTable => {
            let skip = crate::trace::map_blits_off() && skipping() || skips_colored(sprite, canvas);
            map(skip, "map_colored", &[x, y, b.other, b.extra, b.ct])
        }
        W::ScaledA4 | W::ScaledA8 | W::ScaledColored => {
            let skip = skips(canvas, sprite);
            if PIXEL_AUDIT.load(Relaxed) {
                let kind = match b.wrapper { W::ScaledA4 => 0, W::ScaledA8 => 1, _ => 2 };
                (if skip { &SCALED_SKIPPED } else { &SCALED_ORIGINAL })[kind].fetch_add(1, Relaxed);
            }
            skip
        }
        _ => skips(canvas, sprite),
    }
}

#[cfg(test)]
mod present_snapshot_tests {
    use super::*;
    use civ3_sys::controls::{Control, ControlKind, TitleLayout};

    fn staged(model: &mut UiModel, x: i32, label: &str) {
        model.apply(Event::Loaded { image: 10, path: format!("{label}.pcx"), size: (8, 8) });
        model.apply(Event::Sliced { sprite: 11, image: 10, rect: [0, 0, 8, 8] });
        model.apply(Event::PresentBegin);
        model.apply(Event::Fill { canvas: 999, rect: [x, 100, 120, 20], rgb: [255; 3], alpha: 255, clip: None });
        model.apply(Event::Text { canvas: 999, run: TextRun { x: x + 4, y: 114, color: [0; 3], face: "No outlines".into(), height: -12, weight: 400, italic: false, underline: false, text: label.into(), backdrop: None } });
    }

    fn native(x: i32, model: &UiModel) -> NativeUi {
        let state = scene::LookState { pointer: scene::PointerState::Normal, selected: false };
        NativeUi {
            modal_stack: [x as u32, 0, 0, 0],
            controls: vec![Control { rect: [x, 100, 120, 20], kind: ControlKind::Slider { track: [x, 110, 100, 1], value: x }, id: 77 }],
            title: Some(TitleLayout { origin: (x, 100), row_height: 20, first: 1 }),
            military: vec![civ3_sys::controls::screens::UnitStrip { form: 50, canvas: 51, side: 0, row: 2, rect: [x, 100, 120, 20], left: x }],
            status_row: Some((88, [x, 100, 120, 20])),
            arts: vec![crate::looks::ControlArt { id: 77, select: scene::Select::None, canvas: 999, part: crate::looks::Part::Whole, state,
                pieces: vec![vec![(state, model.sprite_source(11).unwrap().clone())]], text_offsets: vec![(state, (x, 0))], colors: vec![], selected_by_fields: false }],
        }
    }

    #[test]
    fn reader_refresh_before_the_first_present_has_no_commands_or_targets() {
        let mut model = UiModel::default().with_screen(999);
        staged(&mut model, 100, "new");
        let mut capture = CommittedCapture::default();
        let (view, changed) = capture.view(&model, false, None, |_| panic!("reader refresh sampled uncommitted controls"));
        assert!(!changed);
        assert!(view.model.committed_frame().draws.is_empty());
        assert_eq!(*view.native, NativeUi::default());
        assert!(view.model.sprite_source(11).is_none());
    }

    #[test]
    fn commands_controls_and_sprite_sources_advance_only_at_the_same_present() {
        let mut model = UiModel::default().with_screen(999);
        let mut capture = CommittedCapture::default();
        staged(&mut model, 100, "old");
        model.apply(Event::Present { rect: None });
        let old_native = native(100, &model);
        let old_presented = Some(crate::city_screen::Presented { focus_city: Some(1), camera: (100, 100) });
        let (old, changed) = capture.view(&model, true, old_presented, |_| old_native.clone());
        assert!(changed);
        let old_frame = old.model.committed_frame();
        let old_sources = SnapshotSources(Some(std::sync::Arc::new(old)));

        staged(&mut model, 200, "new");
        let new_native = native(200, &model);
        assert_ne!(new_native, old_native);
        let new_presented = Some(crate::city_screen::Presented { focus_city: Some(2), camera: (200, 100) });
        let (refresh, changed) = capture.view(&model, false, new_presented, |_| panic!("WANT_VIEW read live controls"));
        assert!(!changed);
        assert_eq!(refresh.model.committed_frame(), old_frame);
        assert_eq!(*refresh.native, old_native);
        assert_eq!(refresh.presented, old_presented);
        assert_eq!(refresh.model.sprite_source(11).cloned(), old_sources.get(&[11])[0]);
        assert_ne!(refresh.model.sprite_source(11), model.sprite_source(11));

        model.apply(Event::Present { rect: None });
        let (new, changed) = capture.view(&model, true, new_presented, |_| new_native.clone());
        assert!(changed);
        assert_ne!(new.model.committed_frame(), old_frame);
        assert_eq!(*new.native, new_native);
        assert_eq!(new.presented, new_presented);
        assert_eq!(new.model.sprite_source(11), model.sprite_source(11));
        assert_ne!(old_sources.get(&[11])[0], new.model.sprite_source(11).cloned(), "an older rendering packet keeps its own sprite mapping");
    }

    #[test]
    fn a_metadata_only_present_is_a_new_publication() {
        let mut model = UiModel::default().with_screen(999);
        let mut capture = CommittedCapture::default();
        staged(&mut model, 100, "same");
        model.apply(Event::Present { rect: None });
        let (old, _) = capture.view(&model, true, None, |model| native(100, model));
        model.apply(Event::Present { rect: None });
        let (new, changed) = capture.view(&model, true, None, |model| native(200, model));
        assert!(changed, "metadata-only updates must wake the publisher");
        assert_eq!(old.model.committed_frame(), new.model.committed_frame());
        assert_ne!(old.native, new.native, "native metadata participates in the publisher's unchanged key");
    }
}

