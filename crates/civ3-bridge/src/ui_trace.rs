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
/// The game's build, once the hooks are installed.
static BUILD: std::sync::OnceLock<civ3_sys::Build> = std::sync::OnceLock::new();

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

/// The mirror model's size (see `UiModel::stats`).
pub fn stats() -> Option<(usize, usize, usize)> {
    MODEL.lock().ok()?.as_ref().map(|m| m.stats())
}

/// The screen as the game last showed it, if the mirror runs.
pub fn frame() -> Option<Frame> {
    MODEL.lock().ok()?.as_ref().map(|m| m.shown_frame())
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
        emit(Event::FlcBlit { flic, chunk, canvas, x: x as i32 + cx, y: y as i32 + cy, clip: h::canvas_clip(canvas), plain: true });
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
    emit(Event::Blit { sprite, canvas, x: x as i32 + cx, y: y as i32 + cy, clip: h::canvas_clip(canvas), palette });
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

/// Region `src` (x, y, w, h) of `from` copied to (x, y) of `canvas`, within
/// the canvas's clip rectangle (the game's copies keep to it: e.g. a list
/// box redrawing one row through tiles of its background).
fn copy(from: u32, src: [i32; 4], canvas: u32, x: i32, y: i32, keyed: bool) {
    let (mut src, mut x, mut y) = (src, x, y);
    if let Some([l, t, r, b]) = h::canvas_clip(canvas) {
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
    emit(Event::Fill { canvas, rect, rgb: rgb555(color), alpha: 255, clip: h::canvas_clip(canvas) });
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
    // The game's mouse cursor, which it draws onto the screen last (docs
    // §16.5, §16.11), is not mirrored: the viewer shows the pointer itself,
    // where the mouse is now (a mirrored one would lag behind it).
    let cursor_sprite = civ3_sys::cursor::sites(game.build).map(|s| s.sprite);
    let screen = h::back_buffer(game.build).unwrap_or(0);
    let is_cursor = move |sprite: u32, canvas: u32| {
        canvas == screen && sprite != 0 && cursor_sprite.is_some_and(|at| inproc::Ptr32::<u32>::new(at).read().ok() == Some(sprite))
    };
    hook!(sprite_draw, sites.sprite_draw, move |c| {
        if !is_cursor(c.sprite, c.canvas) {
            blit(c.sprite, c.canvas, c.x, c.y, true, c.ct);
        }
        c.original()
    });
    hook!(sprite_draw_variant, sites.sprite_draw_variant, |c| {
        blit(c.sprite, c.canvas, c.x, c.y, true, c.ct);
        c.original()
    });
    hook!(sprite_draw_a4, sites.sprite_draw_a4, |c| {
        blit(c.sprite, c.canvas, c.x, c.y, true, c.ct);
        c.original()
    });
    hook!(sprite_draw_a4b, sites.sprite_draw_a4b, |c| {
        blit(c.sprite, c.canvas, c.x, c.y, true, c.ct);
        c.original()
    });
    hook!(sprite_draw_alpha, sites.sprite_draw_alpha, |c| {
        // No crop offset: the stored pixels go to (x, y).
        emit(Event::MaskedBlit { sprite: c.sprite, mask: c.mask, canvas: c.dest, x: c.x as i32, y: c.y as i32, clip: h::canvas_clip(c.dest) });
        c.original()
    });
    hook!(pcx_draw_onto, sites.pcx_draw_onto, |c| {
        if let Some((w, hh)) = obj::image_size(c.image) {
            copy(c.image, [0, 0, w, hh], c.dst, c.x as i32, c.y as i32, true);
        }
        c.original()
    });
    hook!(pcx_copy, sites.pcx_copy, |c| {
        size_of(c.dst);
        size_of(c.image);
        let src = [c.sx as i32, c.sy as i32, c.sw as i32, c.sh as i32];
        copy(c.image, src, c.dst, c.dx as i32, c.dy as i32, false);
        c.original()
    });
    hook!(pcx_copy_same, sites.pcx_copy_same, |c| {
        size_of(c.dst);
        size_of(c.image);
        let src = [c.x as i32, c.y as i32, c.w as i32, c.h as i32];
        copy(c.image, src, c.dst, src[0], src[1], false);
        c.original()
    });
    hook!(pcx_copy_rect, sites.pcx_copy_rect, |c| {
        size_of(c.dst);
        size_of(c.image);
        if let Some(src) = rect_xywh(c.rect) {
            copy(c.image, src, c.dst, src[0], src[1], false);
        }
        c.original()
    });
    hook!(pcx_copy_rects, sites.pcx_copy_rects, |c| {
        size_of(c.dst);
        size_of(c.image);
        if let (Some(src), Some(dst)) = (rect_xywh(c.src_rect), rect_xywh(c.dst_rect)) {
            copy(c.image, src, c.dst, dst[0], dst[1], false);
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
        c.original();
        emit(Event::CompositeEnd);
    });
    hook!(paint, sites.paint, |c| {
        emit(Event::PaintBegin);
        c.original();
        emit(Event::PaintEnd);
    });
    let back_buffer = h::back_buffer(game.build).unwrap_or(0);
    hook!(present, sites.present, move |c| {
        // (It draws the tooltip and the cursor onto the back buffer first.)
        c.original();
        size_of(back_buffer);
        emit(Event::Present { rect: if c.rect == 0 { None } else { obj::rect(c.rect) } });
    });
    hook!(text_worker, sites.text_worker, |c| with_text_canvas(c.image, || c.original()));
    hook!(text_draw, sites.text_draw, |c| with_text_canvas(c.image, || c.original()));
    hook!(text_draw_centered, sites.text_draw_centered, |c| with_text_canvas(c.image, || c.original()));
    hook!(text_draw_rect, sites.text_draw_rect, |c| with_text_canvas(c.image, || c.original()));
    hook!(sprite_draw_masked, sites.sprite_draw_masked, |c| {
        emit(Event::MaskedBlit { sprite: c.sprite, mask: c.mask, canvas: c.dest, x: c.x as i32, y: c.y as i32, clip: h::canvas_clip(c.dest) });
        c.original()
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
    hook_shade_later();
    hook_lines(game, &mut hooks);
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

/// `shade_rect` (see [`crate::ui::shade_alpha`]).
fn shade(c: &h::jgl_shade::Call) {
    let Some(canvas) = h::jgl_owner(c.image) else { return };
    // Palette colours are not used by the interface code we know of.
    if c.color & 0x8000_0000 == 0 {
        return;
    }
    let rect = if c.rect == 0 { obj::image_size(canvas).map(|(w, hh)| [0, 0, w, hh]) } else { rect_xywh(c.rect) };
    let Some(rect) = rect else { return };
    let alpha = crate::ui::shade_alpha((c.percent as i32).clamp(0, 100) as u32);
    if alpha == 0 {
        return;
    }
    emit(Event::Fill { canvas, rect, rgb: rgb555(c.color), alpha, clip: h::canvas_clip(canvas) });
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
        if let Some(p) = pen.filter(|p| p.canvas != 0 && p.alpha > 0) {
            let (x1, y1, x2, y2) = (c.x1 as i32, c.y1 as i32, c.x2 as i32, c.y2 as i32);
            for (rect, coverage) in crate::ui::line_fills((x1, y1), (x2, y2), p.width, crate::ui::smooth_lines()) {
                let alpha = crate::ui::covered_alpha(p.alpha, coverage);
                emit(Event::Fill { canvas: p.canvas, rect, rgb: p.rgb, alpha, clip: h::canvas_clip(p.canvas) });
            }
        }
        c.original()
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

/// A colour-table blit (the tracer's hook): unit animation frames drawn on
/// interface canvases become unit pictures (§16.9). `canvas` is the
/// destination (the blit's second canvas argument).
pub fn on_colored(sprite: u32, canvas: u32, x: u32, y: u32) {
    if let Some((flic, chunk)) = h::flc_frame(sprite) {
        // The frame's image lies at its offset from (x, y), as for map
        // units (§10.3).
        let (cx, cy) = obj::sprite_crop(sprite).map_or((0, 0), |c| (c.0, c.1));
        emit(Event::FlcBlit { flic, chunk, canvas, x: x as i32 + cx, y: y as i32 + cy, clip: h::canvas_clip(canvas), plain: false });
    }
}
