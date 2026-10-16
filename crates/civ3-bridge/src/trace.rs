//! Records exactly which map sprites the game draws, where.
//!
//! The ground truth for reimplementing the map: every `Sprite_draw_on_map`
//! call made while `Map_Renderer::Draw_Tiles` runs, resolved to (sprite
//! array, index) and pixel position. The game redraws the whole map only
//! when something changes, so the last complete redraw is always kept; a
//! test asks for it by creating `civ3-trace.request` next to the game, and
//! gets `civ3-trace.txt` (the request is removed once written).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;

use civ3_sys::hooks::{draw_tiles, sprite_draw_on_map};
use civ3_sys::sprites::{SpriteRef, identify};
use civ3_sys::{Civ3, hooks};

#[derive(Clone, Debug)]
pub struct Draw {
    pub sprite: Option<SpriteRef>,
    pub sprite_addr: u32,
    pub x: i32,
    pub y: i32,
    pub params: [u32; 4],
}

struct State {
    /// The Draw_Tiles call in progress.
    frame: Option<Vec<Draw>>,
    /// Draw_Tiles nesting depth.
    depth: u32,
    /// The last complete redraw, as text.
    last: Option<String>,
}

static STATE: Mutex<State> = Mutex::new(State { frame: None, depth: 0, last: None });

/// Calls seen, for diagnostics.
pub static SPRITE_CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub static DRAW_TILES_CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub struct Tracer {
    _draw_tiles: draw_tiles::Installed,
    _sprite: Option<sprite_draw_on_map::Installed>,
}

fn request_path() -> PathBuf {
    PathBuf::from("civ3-trace.request")
}

pub fn install(game: Civ3) -> Result<Tracer, inproc::hook::HookError> {
    let sprite = match hooks::sprite_draw_on_map_site(game.build) {
        Some((addr, prologue)) => Some(sprite_draw_on_map::install_inline(addr, prologue, |call| {
            SPRITE_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if let Ok(mut s) = STATE.lock() {
                if let Some(frame) = s.frame.as_mut() {
                    frame.push(Draw {
                        sprite: identify(call.map_renderer, call.sprite),
                        sprite_addr: call.sprite,
                        x: call.pixel_x as i32,
                        y: call.pixel_y as i32,
                        params: [call.p4, call.p5, call.p6, call.p7],
                    });
                }
            }
            call.original()
        })?),
        None => {
            log::warn!("no Sprite_draw_on_map site for {:?}; tracing disabled", game.build);
            None
        }
    };
    let draw_tiles = draw_tiles::install_slot(hooks::draw_tiles_slot(game.build), move |call| {
        DRAW_TILES_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        {
            let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
            s.depth += 1;
            if s.depth == 1 {
                s.frame = Some(Vec::new());
            }
        }
        call.original();
        let finished = {
            let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
            s.depth -= 1;
            if s.depth == 0 { s.frame.take() } else { None }
        };
        if let Some(draws) = finished {
            // Formatted here, on the game thread, so the view matches.
            let text = format_trace(&game, call.map_renderer, &draws);
            STATE.lock().unwrap_or_else(|e| e.into_inner()).last = Some(text);
        }
    })?;
    std::thread::spawn(serve_requests);
    Ok(Tracer { _draw_tiles: draw_tiles, _sprite: sprite })
}

fn serve_requests() {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if !request_path().exists() {
            continue;
        }
        let last = STATE.lock().unwrap_or_else(|e| e.into_inner()).last.clone();
        let Some(text) = last else { continue };
        match std::fs::write("civ3-trace.txt", text) {
            Ok(()) => log::info!("wrote trace"),
            Err(e) => log::error!("writing trace: {e}"),
        }
        let _ = std::fs::remove_file(request_path());
    }
}

fn format_trace(game: &Civ3, map_renderer: u32, draws: &[Draw]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# civ3-bridge trace: map_renderer {map_renderer:#x}");
    if let Ok(view) = game.view() {
        let _ = writeln!(out, "view camera {} {} visible_x {} {} visible_y {} {}", view.camera_x, view.camera_y, view.tile_x.0, view.tile_x.1, view.tile_y.0, view.tile_y.1);
    }
    if let Ok(map) = game.map() {
        let _ = writeln!(out, "map {} {} wrap {} {}", map.width, map.height, map.wrap_x as u8, map.wrap_y as u8);
    }
    // Every tile's raw fields, for correlating draws with world state.
    if let Ok(map) = game.map() {
        if let Ok(tiles) = game.tiles(&map) {
            let _ = writeln!(out, "# tile x y square_type square_parts overlays terrain_overlays river resource owner city fow visibility");
            for y in 0..map.height {
                for x in (y & 1..map.width).step_by(2) {
                    let t = &tiles[map.tile_index(x, y)];
                    let _ = writeln!(
                        out,
                        "tile {x} {y} {:#x} {:#x} {:#x} {:#x} {:#x} {} {} {} {:#x} {:#x}",
                        t.square_type(),
                        t.square_parts() as u16,
                        t.overlays(),
                        t.terrain_overlays(),
                        t.river_code(),
                        t.resource_type(),
                        t.territory_owner(),
                        t.city_id(),
                        t.fow_status(),
                        t.visibility()
                    );
                }
            }
        }
    }
    let _ = writeln!(out, "# array index x y p4 p5 p6 p7");
    for d in draws {
        let (array, index) = d.sprite.map_or(("?", d.sprite_addr), |s| (s.array, s.index));
        let [a, b, c, e] = d.params;
        let _ = writeln!(out, "draw {array} {index} {} {} {a:#x} {b:#x} {c:#x} {e:#x}", d.x, d.y);
    }
    out
}
