//! Records exactly which map sprites the game draws, where.
//!
//! The ground truth for reimplementing the map: every `Sprite_draw_on_map`
//! call of a map redraw, resolved to (sprite array, index) and pixel
//! position. A redraw starts with `Map_Renderer::Draw_Tiles` and includes
//! what follows it (the fog-of-war pass, units) until the next one. The game
//! redraws only when something changes; a test asks for the latest redraw
//! by creating `civ3-trace.request` next to the game and gets
//! `civ3-trace.txt` (the request is removed once written).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;

use civ3_sys::hooks::{draw_tiles, sprite_draw_on_map, sprite_draw_on_map_blend, sprite_draw_with_color_table};
use civ3_sys::sprites::{SpriteRef, identify_canonical};
use civ3_sys::{Civ3, hooks};

#[derive(Clone, Debug)]
pub struct Draw {
    pub sprite: Option<SpriteRef>,
    pub sprite_addr: u32,
    pub x: i32,
    pub y: i32,
    /// Sprite size.
    pub size: (u32, u32),
    pub params: Vec<u32>,
    /// Which blit: "map", "blend" (with a blend table) or "colored".
    pub kind: &'static str,
}

struct State {
    /// The redraw in progress: map renderer, draws, and the time of the
    /// last draw.
    frame: Option<(u32, Vec<Draw>, std::time::Instant)>,
    /// Draw_Tiles nesting depth.
    depth: u32,
}

/// Cap on recorded draws per redraw.
const MAX_DRAWS: usize = 20_000;

static STATE: Mutex<State> = Mutex::new(State { frame: None, depth: 0 });
/// The running build, for resolving sprites of global arrays.
static BUILD: std::sync::OnceLock<civ3_sys::Build> = std::sync::OnceLock::new();

/// Skip the game's own map drawing (`Draw_Tiles`).
pub static GAME_MAP_OFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Calls seen, for diagnostics.
pub static SPRITE_CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub static DRAW_TILES_CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub struct Tracer {
    _draw_tiles: draw_tiles::Installed,
    _sprite: Option<sprite_draw_on_map::Installed>,
    _blend: Option<sprite_draw_on_map_blend::Installed>,
    _colored: Option<sprite_draw_with_color_table::Installed>,
}

fn record(map_renderer: u32, sprite: u32, x: u32, y: u32, params: Vec<u32>, kind: &'static str) {
    if let Ok(mut s) = STATE.lock() {
        if let Some((_, frame, at)) = s.frame.as_mut() {
            // Only map sprites say the redraw is still going: other blits
            // (UI, unit flags) run continuously.
            if kind == "map" {
                *at = std::time::Instant::now();
            }
            if frame.len() >= MAX_DRAWS {
                return;
            }
            let resolved = if map_renderer != 0 { identify_canonical(map_renderer, sprite) } else { None }
                .or_else(|| BUILD.get().and_then(|&b| civ3_sys::sprites::identify_global(b, sprite)));
            let size = civ3_sys::sprites::size(sprite).unwrap_or((0, 0));
            frame.push(Draw { sprite: resolved, sprite_addr: sprite, x: x as i32, y: y as i32, size, params, kind });
        }
    }
}

fn request_path() -> PathBuf {
    PathBuf::from("civ3-trace.request")
}

pub fn install(game: Civ3) -> Result<Tracer, inproc::hook::HookError> {
    let _ = BUILD.set(game.build);
    let sprite = match hooks::sprite_draw_on_map_site(game.build) {
        Some((addr, prologue)) => Some(sprite_draw_on_map::install_inline(addr, prologue, |call| {
            SPRITE_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            record(call.map_renderer, call.sprite, call.pixel_x, call.pixel_y, vec![call.p4, call.p5, call.p6, call.p7], "map");
            call.original()
        })?),
        None => {
            log::warn!("no Sprite_draw_on_map site for {:?}; tracing disabled", game.build);
            None
        }
    };
    let blend = match hooks::sprite_draw_on_map_blend_site(game.build) {
        Some((addr, prologue)) => Some(sprite_draw_on_map_blend::install_inline(addr, prologue, |call| {
            record(call.map_renderer, call.sprite, call.pixel_x, call.pixel_y, vec![call.table, call.p5, call.p6, call.p7, call.p8], "blend");
            call.original()
        })?),
        None => None,
    };
    // Resolve the sprite against the map renderer of the redraw in progress.
    let colored = match hooks::sprite_draw_with_color_table_site(game.build) {
        Some((addr, prologue)) => Some(sprite_draw_with_color_table::install_inline(addr, prologue, |call| {
            let mr = STATE.lock().ok().and_then(|s| s.frame.as_ref().map(|f| f.0)).unwrap_or(0);
            record(mr, call.sprite, call.pixel_x, call.pixel_y, vec![call.canvas, call.canvas2, call.table, call.p6], "colored");
            call.original()
        })?),
        None => None,
    };
    let draw_tiles = draw_tiles::install_slot(hooks::draw_tiles_slot(game.build), move |call| {
        DRAW_TILES_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        {
            let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
            s.depth += 1;
            if s.depth == 1 {
                s.frame = Some((call.map_renderer, Vec::new(), std::time::Instant::now()));
            }
        }
        // With the game's map rendering switched off, the map stays as the
        // UI left it; our renderer draws the map instead.
        if !GAME_MAP_OFF.load(std::sync::atomic::Ordering::Relaxed) {
            call.original();
        }
        STATE.lock().unwrap_or_else(|e| e.into_inner()).depth -= 1;
    })?;
    std::thread::spawn(move || serve_requests(game));
    Ok(Tracer { _draw_tiles: draw_tiles, _sprite: sprite, _blend: blend, _colored: colored })
}

/// Answers trace requests with the latest redraw, once it has been quiet
/// for a moment (so it is complete).
fn serve_requests(game: Civ3) {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if !request_path().exists() {
            continue;
        }
        let frame = {
            let s = STATE.lock().unwrap_or_else(|e| e.into_inner());
            match &s.frame {
                Some((mr, draws, at)) if s.depth == 0 && at.elapsed() > std::time::Duration::from_millis(300) => {
                    Some((*mr, draws.clone()))
                }
                _ => None,
            }
        };
        let Some((map_renderer, draws)) = frame else { continue };
        let text = format_trace(&game, map_renderer, &draws);
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
    for (name, i, path) in civ3_sys::sprites::sheet_files(map_renderer) {
        let _ = writeln!(out, "file {name} {i} {path}");
    }
    if let Ok(Some(folder)) = game.scenario_folder() {
        let _ = writeln!(out, "scenario_folder {folder}");
    }
    match game.search_roots() {
        Ok(roots) => {
            for root in roots {
                let _ = writeln!(out, "search_root {root}");
            }
        }
        Err(e) => log::warn!("reading search roots: {e}"),
    }
    if let Ok(view) = game.view() {
        let _ = writeln!(out, "view camera {} {} visible_x {} {} visible_y {} {}", view.camera_x, view.camera_y, view.tile_x.0, view.tile_x.1, view.tile_y.0, view.tile_y.1);
    }
    if let Ok(map) = game.map() {
        let _ = writeln!(out, "map {} {} wrap {} {} seed {} flags {:#x}", map.width, map.height, map.wrap_x as u8, map.wrap_y as u8, map.seed, map.flags);
    }
    if let Ok(view) = game.view() {
        let _ = writeln!(out, "player {}", view.player_civ);
    }
    if let Ok(z) = game.zoomed_out() {
        let _ = writeln!(out, "zoomed_out {}", z as u8);
    }
    if let Ok(view) = game.view() {
        if let Ok(vis) = game.resources_visible_to(view.player_civ) {
            let list: Vec<&str> = vis.iter().map(|&v| if v { "1" } else { "0" }).collect();
            let _ = writeln!(out, "resource_visible {}", list.join(" "));
        }
    }
    if let Ok(eras) = game.eras() {
        let list: Vec<String> = eras.iter().map(|e| e.to_string()).collect();
        let _ = writeln!(out, "eras {}", list.join(" "));
    }
    if let Ok(p) = game.players() {
        let join = |v: &[i32]| v.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
        let _ = writeln!(out, "color_tables {}", join(&p.color_tables));
        let _ = writeln!(out, "capitals {}", join(&p.capitals));
        let _ = writeln!(out, "races {}", join(&p.races));
        let _ = writeln!(out, "culture_groups {}", join(&p.culture_groups));
        let _ = writeln!(out, "city_sizes {} {}", p.max_town, p.max_city);
        let _ = writeln!(out, "governments {}", join(&p.governments));
        for b in &p.wall_buildings {
            let _ = writeln!(out, "wall_building {} {} {:#x}", b.id, b.government, b.obsolete_for);
        }
    }
    if let Ok(keys) = game.unit_type_keys() {
        for (i, k) in keys.iter().enumerate() {
            let _ = writeln!(out, "unit_type_key {i} {k}");
        }
    }
    if let Ok(stats) = game.unit_type_stats() {
        for (i, s) in stats.iter().enumerate() {
            let _ = writeln!(out, "unit_type_stats {i} {} {} {} {}", s[0], s[1], s[2], s[3]);
        }
    }
    if let Ok(hp) = game.experience_hit_points() {
        let list: Vec<String> = hp.iter().map(|h| h.to_string()).collect();
        let _ = writeln!(out, "experience_hit_points {}", list.join(" "));
    }
    if let Ok(tiles) = game.unit_tiles() {
        for (x, y, c) in tiles {
            let _ = writeln!(out, "unit_tile {x} {y} {c}");
        }
    }
    if let Ok(p) = game.preferences() {
        let _ = writeln!(out, "preferences {p:#x}");
    }
    if let Ok(keys) = game.era_keys() {
        for (i, k) in keys.iter().enumerate() {
            let _ = writeln!(out, "era_key {i} {k}");
        }
    }
    match game.drawn_units() {
        Ok(units) => {
            let _ = writeln!(out, "# unit id x y civ type damage moves container state on_top px py direction animation frame visible experience");
            for u in units {
                let _ = writeln!(
                    out,
                    "unit {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
                    u.id, u.x, u.y, u.civ, u.unit_type, u.damage, u.moves, u.container, u.state, u.always_on_top as u8,
                    u.pixel.0, u.pixel.1, u.direction, u.animation, u.frame, u.visible as u8, u.experience
                );
            }
        }
        Err(e) => log::warn!("reading units: {e}"),
    }
    match game.cities() {
        Ok(cities) => {
            for c in cities {
                let bits: String = c.improvements.iter().map(|b| format!("{b:02x}")).collect();
                let _ = writeln!(out, "city {} {} {} {} {} {bits}", c.id, c.x, c.y, c.civ, c.size);
            }
        }
        Err(e) => log::warn!("reading cities: {e}"),
    }
    if let Ok(icons) = game.resource_icons() {
        let list: Vec<String> = icons.iter().map(|i| i.to_string()).collect();
        let _ = writeln!(out, "resource_icons {}", list.join(" "));
    }
    // Every tile's raw fields, for correlating draws with world state.
    if let Ok(map) = game.map() {
        if let Ok(tiles) = game.tiles(&map) {
            let _ = writeln!(out, "# tile x y square_type square_parts overlays terrain_overlays river resource owner city fow visibility explored v3 vis_d0 visible_overlays victory_location");
            for y in 0..map.height {
                for x in (y & 1..map.width).step_by(2) {
                    let t = &tiles[map.tile_index(x, y)];
                    let _ = writeln!(
                        out,
                        "tile {x} {y} {:#x} {:#x} {:#x} {:#x} {:#x} {} {} {} {:#x} {:#x} {:#x} {:#x} {:#x} {} {}",
                        t.square_type(),
                        t.square_parts() as u16,
                        t.overlays(),
                        t.terrain_overlays(),
                        t.river_code(),
                        t.resource_type(),
                        t.territory_owner(),
                        t.city_id(),
                        t.fow_status(),
                        t.visibility(),
                        t.explored(),
                        t.v3(),
                        t.visibility_d0(),
                        t.visible_overlays().iter().map(|b| format!("{b:02x}")).collect::<String>(),
                        t.victory_location()
                    );
                }
            }
        }
    }
    // The game's font objects (get_font): any member that is a GDI font.
    for i in 0..40u32 {
        let base = 0xB3_DF00 + i * 0x2C;
        for k in 0..11u32 {
            let Ok(v) = inproc::Ptr32::<u32>::new(base + 4 * k).read() else { continue };
            if let Some(f) = inproc::gdi::font(v) {
                let _ = writeln!(out, "font_object {base:#x} +{:#x} {f:?}", 4 * k);
            }
        }
    }
    // Raw bytes of city tiles, for finding undecoded members.
    if let (Ok(map), Ok(cities)) = (game.map(), game.cities()) {
        if let Ok(tiles) = game.tiles(&map) {
            for c in cities {
                if let Some(t) = tiles.get(map.tile_index(c.x, c.y)) {
                    let hex: String = t.bytes().iter().map(|b| format!("{b:02x}")).collect();
                    let _ = writeln!(out, "tile_raw {} {} {hex}", c.x, c.y);
                }
            }
        }
    }
    let _ = writeln!(out, "# draw array index x y kind w h params...");
    for d in draws {
        let (array, index) = d.sprite.map_or(("?", d.sprite_addr), |s| (s.array, s.index));
        let params: Vec<String> = d.params.iter().map(|p| format!("{p:#x}")).collect();
        let _ = writeln!(out, "draw {array} {index} {} {} {} {} {} {}", d.x, d.y, d.kind, d.size.0, d.size.1, params.join(" "));
    }
    out
}
