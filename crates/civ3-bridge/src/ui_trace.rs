//! Feeds the UI mirror (`ui`) from hooks on the game's UI drawing (docs
//! §16.7): image loads, sprite slices, blits, copies, fills, lines, text and
//! the composition of forms onto the screen.

use std::cell::Cell;
use std::sync::Mutex;

use civ3_sys::Civ3;
use civ3_sys::ui_hooks::{self as h, obj};
use inproc::hook::HookError;

use crate::text::TextRun;
use crate::ui::{Event, Frame, UiModel, rgb555};

static MODEL: Mutex<Option<UiModel>> = Mutex::new(None);

thread_local! {
    /// The canvas the game's text functions are drawing on (their `this`),
    /// for attributing `TextOutA` calls.
    static TEXT_CANVAS: Cell<u32> = const { Cell::new(0) };
}

fn emit(e: Event) {
    if let Ok(mut m) = MODEL.lock() {
        if let Some(m) = m.as_mut() {
            m.apply(e);
        }
    }
}

/// The last composed screen, if the mirror runs.
pub fn frame() -> Option<Frame> {
    MODEL.lock().ok()?.as_ref().map(|m| m.frame().clone())
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
fn blit(sprite: u32, canvas: u32, x: u32, y: u32, add_crop: bool) {
    let (cx, cy) = if add_crop { obj::sprite_crop(sprite).map_or((0, 0), |c| (c.0, c.1)) } else { (0, 0) };
    emit(Event::Blit { sprite, canvas, x: x as i32 + cx, y: y as i32 + cy });
}

fn fill(canvas: u32, rect: [i32; 4], color: u32) {
    size_of(canvas);
    emit(Event::Fill { canvas, rect, rgb: rgb555(color), alpha: 255 });
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
    *MODEL.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(
            UiModel::new(Vec::new(), skip_canvases)
                .with_screen(h::back_buffer(game.build).unwrap_or(0))
                .with_generated(h::generated_surfaces(game.build).iter().map(|g| g.0).collect())
                .with_underlays(underlays, underlay_art),
        );
    let mut hooks: Vec<Box<dyn std::any::Any + Send>> = Vec::new();
    macro_rules! hook {
        ($module:ident, $site:expr, $handler:expr) => {
            hooks.push(Box::new(h::$module::install_inline($site.0, $site.1, $handler)?));
        };
    }

    hook!(pcx_read_file, sites.pcx_read_file, |c| {
        let path = read_c_string(c.path);
        let r = c.original();
        if r == 0 {
            if let (Some(path), Some(size)) = (path, obj::image_size(c.image)) {
                emit(Event::Loaded { image: c.image, path, size });
            }
        }
        r
    });
    hook!(pcx_clear, sites.pcx_clear, |c| {
        emit(Event::Cleared { image: c.image });
        c.original()
    });
    hook!(sprite_slice, sites.sprite_slice, |c| {
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
    hook!(sprite_draw, sites.sprite_draw, |c| {
        blit(c.sprite, c.canvas, c.x, c.y, true);
        c.original()
    });
    hook!(sprite_draw_variant, sites.sprite_draw_variant, |c| {
        blit(c.sprite, c.canvas, c.x, c.y, true);
        c.original()
    });
    hook!(sprite_draw_a4, sites.sprite_draw_a4, |c| {
        blit(c.sprite, c.canvas, c.x, c.y, true);
        c.original()
    });
    hook!(sprite_draw_a4b, sites.sprite_draw_a4b, |c| {
        blit(c.sprite, c.canvas, c.x, c.y, true);
        c.original()
    });
    hook!(sprite_draw_alpha, sites.sprite_draw_alpha, |c| {
        blit(c.sprite, c.dest, c.x, c.y, false);
        c.original()
    });
    hook!(pcx_draw_onto, sites.pcx_draw_onto, |c| {
        if let Some((w, hh)) = obj::image_size(c.image) {
            emit(Event::Copy { from: c.image, src: [0, 0, w, hh], canvas: c.dst, x: c.x as i32, y: c.y as i32, keyed: true });
        }
        c.original()
    });
    hook!(pcx_copy, sites.pcx_copy, |c| {
        size_of(c.dst);
        let src = [c.sx as i32, c.sy as i32, c.sw as i32, c.sh as i32];
        emit(Event::Copy { from: c.image, src, canvas: c.dst, x: c.dx as i32, y: c.dy as i32, keyed: false });
        c.original()
    });
    hook!(pcx_copy_same, sites.pcx_copy_same, |c| {
        size_of(c.dst);
        let src = [c.x as i32, c.y as i32, c.w as i32, c.h as i32];
        emit(Event::Copy { from: c.image, src, canvas: c.dst, x: src[0], y: src[1], keyed: false });
        c.original()
    });
    hook!(pcx_copy_rect, sites.pcx_copy_rect, |c| {
        size_of(c.dst);
        if let Some(src) = rect_xywh(c.rect) {
            emit(Event::Copy { from: c.image, src, canvas: c.dst, x: src[0], y: src[1], keyed: false });
        }
        c.original()
    });
    hook!(pcx_copy_rects, sites.pcx_copy_rects, |c| {
        size_of(c.dst);
        if let (Some(src), Some(dst)) = (rect_xywh(c.src_rect), rect_xywh(c.dst_rect)) {
            emit(Event::Copy { from: c.image, src, canvas: c.dst, x: dst[0], y: dst[1], keyed: false });
        }
        c.original()
    });
    hook!(pcx_fill, sites.pcx_fill, |c| {
        if let Some((w, hh)) = obj::image_size(c.image) {
            fill(c.image, [0, 0, w, hh], c.color);
        }
        c.original()
    });
    hook!(pcx_fill_area, sites.pcx_fill_area, |c| {
        if let Some(rect) = rect_xywh(c.rect) {
            fill(c.image, rect, c.color);
        }
        c.original()
    });
    hook!(pcx_hline, sites.pcx_hline, |c| {
        let (x1, x2) = (c.x1 as i32, c.x2 as i32);
        fill(c.image, [x1.min(x2), c.y as i32, (x2 - x1).abs() + 1, 1], c.color);
        c.original()
    });
    hook!(pcx_vline, sites.pcx_vline, |c| {
        let (y1, y2) = (c.y1 as i32, c.y2 as i32);
        fill(c.image, [c.x as i32, y1.min(y2), 1, (y2 - y1).abs() + 1], c.color);
        c.original()
    });
    hook!(pcx_line, sites.pcx_line, |c| {
        for (x, y) in line_points((c.x1 as i32, c.y1 as i32), (c.x2 as i32, c.y2 as i32)) {
            fill(c.image, [x, y, 1, 1], c.color);
        }
        c.original()
    });
    hook!(form_composite, sites.form_composite, |c| {
        if let Some(p) = obj::placement(c.form) {
            emit(Event::Composite { canvas: p.canvas, vtable: p.vtable, origin: p.origin, size: p.size, clip: p.clip, keyed: p.keyed });
        }
        c.original()
    });
    hook!(paint, sites.paint, |c| {
        emit(Event::PaintBegin);
        c.original();
        emit(Event::PaintEnd);
    });
    hook!(text_worker, sites.text_worker, |c| with_text_canvas(c.image, || c.original()));
    hook!(text_draw, sites.text_draw, |c| with_text_canvas(c.image, || c.original()));
    hook!(text_draw_centered, sites.text_draw_centered, |c| with_text_canvas(c.image, || c.original()));
    hook!(text_draw_rect, sites.text_draw_rect, |c| with_text_canvas(c.image, || c.original()));
    hook!(sprite_draw_masked, sites.sprite_draw_masked, |c| {
        blit(c.sprite, c.dest, c.x, c.y, false);
        c.original()
    });
    hook_shade_later();
    log::info!("UI mirror: {} hooks", hooks.len());
    Ok(UiTracer { _hooks: hooks })
}


/// Hooks jgl.dll's translucent rectangle fill once the game has loaded
/// it (the hook stays for the life of the process).
fn hook_shade_later() {
    std::thread::spawn(|| {
        for _ in 0..600 {
            if let Some(base) = inproc::pe::module_base("jgl.dll") {
                let (slot, expected) = h::jgl_shade_slot(base);
                if slot.read().ok() != Some(expected) {
                    return log::warn!("UI mirror: unknown jgl.dll, translucent fills not mirrored");
                }
                match h::jgl_shade::install_slot(slot, |c| {
                    shade(c);
                    c.original()
                }) {
                    Ok(installed) => std::mem::forget(installed),
                    Err(e) => log::warn!("UI mirror: hooking translucent fills: {e}"),
                }
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    });
}

/// `shade_rect`: the colour weighs `1 − a`, what is there `a = percent·256/100`
/// (in 1/256), per 8-bit channel, rounded down — for the percentages the
/// game uses, our translucency (1/32 steps, rounded down) exactly.
fn shade(c: &h::jgl_shade::Call) {
    let Some(canvas) = h::jgl_owner(c.image) else { return };
    // Palette colours are not used by the interface code we know of.
    if c.color & 0x8000_0000 == 0 {
        return;
    }
    let rect = if c.rect == 0 { obj::image_size(canvas).map(|(w, hh)| [0, 0, w, hh]) } else { rect_xywh(c.rect) };
    let Some(rect) = rect else { return };
    let kept = (c.percent as i32).clamp(0, 100) * 256 / 100;
    let alpha = ((256 - kept) * 255 / 256) as u8;
    if alpha == 0 {
        return;
    }
    emit(Event::Fill { canvas, rect, rgb: rgb555(c.color), alpha });
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
