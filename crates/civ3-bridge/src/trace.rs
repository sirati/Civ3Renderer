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

use crate::text::TextRun;

#[derive(Clone, Debug)]
pub struct Draw {
    pub sprite: Option<SpriteRef>,
    pub sprite_addr: u32,
    pub x: i32,
    pub y: i32,
    /// Sprite size.
    pub size: (u32, u32),
    pub params: Vec<u32>,
    /// Which blit: "map", "blend" (with a blend table), "colored" or
    /// "text".
    pub kind: &'static str,
}

struct State {
    /// The redraw in progress: map renderer, draws, and the time of the
    /// last draw.
    frame: Option<(u32, Vec<Draw>, std::time::Instant)>,
    /// The redraw before the one in progress (complete).
    previous: Option<(u32, Vec<Draw>)>,
    /// Draw_Tiles nesting depth.
    depth: u32,
}

/// Cap on recorded draws per redraw.
const MAX_DRAWS: usize = 20_000;

static STATE: Mutex<State> = Mutex::new(State { frame: None, previous: None, depth: 0 });
/// The running build, for resolving sprites of global arrays.
static BUILD: std::sync::OnceLock<civ3_sys::Build> = std::sync::OnceLock::new();

/// Skip the game's own map drawing (`Draw_Tiles`).
pub static GAME_MAP_OFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether map sprites are not to be blitted: the game's map rendering is
/// off and a map redraw is in progress.
fn map_blits_off() -> bool {
    GAME_MAP_OFF.load(std::sync::atomic::Ordering::Relaxed) && STATE.lock().is_ok_and(|s| s.depth > 0)
}

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

/// Text the game drew recently, newest last.
static RECENT_TEXT: Mutex<std::collections::VecDeque<(std::time::Instant, TextRun)>> =
    Mutex::new(std::collections::VecDeque::new());

/// How long text stays "recent": the game redraws what is on screen more
/// often than this.
const TEXT_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

/// Fonts text was drawn in (logged once each).
static FONTS_SEEN: Mutex<Vec<inproc::gdi::LogFont>> = Mutex::new(Vec::new());

fn record_text(call: &hooks::text_out::Call) {
    let len = (call.len as i32).clamp(0, 1024) as usize;
    let Ok(bytes) = inproc::Ptr32::<u8>::new(call.text).read_array(len) else { return };
    // Windows-1252, read as Latin-1 (the glyph set covers the same codes).
    let text: String = bytes.iter().map(|&b| b as char).collect();
    let Some(font) = inproc::gdi::dc_font(call.dc) else { return };
    if let Ok(mut seen) = FONTS_SEEN.lock()
        && !seen.contains(&font)
    {
        log::info!("text font {font:?}");
        seen.push(font.clone());
    }
    let (r, g, b) = inproc::gdi::dc_text_color(call.dc);
    let run = TextRun { x: call.x as i32, y: call.y as i32, color: [r, g, b], face: font.face, height: font.height, weight: font.weight, italic: font.italic, underline: font.underline, text, backdrop: None };
    crate::ui_trace::on_text(&run);
    if let Ok(mut recent) = RECENT_TEXT.lock() {
        let now = std::time::Instant::now();
        while recent.front().is_some_and(|(t, _)| now.duration_since(*t) > TEXT_WINDOW) {
            recent.pop_front();
        }
        recent.retain(|(_, r)| !(r.x == run.x && r.y == run.y));
        if recent.len() < 4096 {
            recent.push_back((now, run));
        }
    }
}

/// The text runs currently on screen (drawn within the last half second),
/// in drawing order; a run replaces earlier ones at the same position.
pub fn recent_texts() -> Vec<TextRun> {
    let now = std::time::Instant::now();
    RECENT_TEXT
        .lock()
        .map(|r| r.iter().filter(|(t, _)| now.duration_since(*t) <= TEXT_WINDOW).map(|(_, r)| r.clone()).collect())
        .unwrap_or_default()
}

fn request_path() -> PathBuf {
    PathBuf::from("civ3-trace.request")
}

pub fn install(game: Civ3) -> Result<Tracer, inproc::hook::HookError> {
    let _ = BUILD.set(game.build);
    let sprite = match hooks::sprite_draw_on_map_site(game.build) {
        Some((addr, prologue)) => Some(sprite_draw_on_map::install_at_start(addr, prologue, |call| {
            SPRITE_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            record(call.map_renderer, call.sprite, call.pixel_x, call.pixel_y, vec![call.p4, call.p5, call.p6, call.p7], "map");
            if map_blits_off() { 0 } else { call.original() }
        })?),
        None => {
            log::warn!("no Sprite_draw_on_map site for {:?}; tracing disabled", game.build);
            None
        }
    };
    let blend = match hooks::sprite_draw_on_map_blend_site(game.build) {
        Some((addr, prologue)) => Some(sprite_draw_on_map_blend::install_at_start(addr, prologue, |call| {
            record(call.map_renderer, call.sprite, call.pixel_x, call.pixel_y, vec![call.table, call.p5, call.p6, call.p7, call.p8], "blend");
            if map_blits_off() { 0 } else { call.original() }
        })?),
        None => None,
    };
    // Resolve the sprite against the map renderer of the redraw in progress.
    let colored = match hooks::sprite_draw_with_color_table_site(game.build) {
        Some((addr, prologue)) => Some(sprite_draw_with_color_table::install_at_start(addr, prologue, |call| {
            let mr = STATE.lock().ok().and_then(|s| s.frame.as_ref().map(|f| f.0)).unwrap_or(0);
            record(mr, call.sprite, call.pixel_x, call.pixel_y, vec![call.canvas, call.canvas2, call.table, call.p6], "colored");
            crate::ui_trace::on_colored(call.sprite, call.canvas2, call.pixel_x, call.pixel_y);
            // Inside a map redraw like the map's other blits; elsewhere a
            // unit picture the mirror recorded.
            if map_blits_off() && crate::ui_trace::skipping() || crate::ui_trace::skips_colored(call.sprite, call.canvas2) {
                0
            } else {
                call.original()
            }
        })?),
        None => None,
    };
    // The game loads its graphics library (jgl.dll) later: hook its text
    // output once it is there. The hook stays for the life of the process.
    std::thread::spawn(|| {
        for _ in 0..600 {
            if let Some(module) = inproc::pe::module_base("jgl.dll") {
                match inproc::pe::import_slot(module, "GDI32.dll", "TextOutA") {
                    Ok(Some(slot)) => match hooks::text_out::install_slot(slot, |call| {
                        record_text(call);
                        // Recorded for the mirror: not drawn while the game
                        // does not draw (measuring text still works: that
                        // is GetTextExtentPoint32).
                        if crate::ui_trace::skips_text() { 1 } else { call.original() }
                    }) {
                        Ok(installed) => {
                            log::info!("tracing text (jgl.dll TextOutA)");
                            std::mem::forget(installed);
                        }
                        Err(e) => log::warn!("hooking TextOutA: {e}"),
                    },
                    other => log::warn!("no TextOutA import in jgl.dll ({other:?})"),
                }
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    });
    let draw_tiles = draw_tiles::install_slot(hooks::draw_tiles_slot(game.build), move |call| {
        DRAW_TILES_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        {
            let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
            s.depth += 1;
            if s.depth == 1 {
                crate::ui_trace::map_redraw(call.map_renderer);
                if let Some((mr, draws, _)) = s.frame.take() {
                    s.previous = Some((mr, draws));
                }
                s.frame = Some((call.map_renderer, Vec::new(), std::time::Instant::now()));
            }
        }
        // With the game's map rendering switched off its sprites are not
        // blitted (see `map_blits_off`), but the pass still runs: it also
        // draws the city labels the interface mirror shows.
        call.original();
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
            match (&s.frame, &s.previous) {
                (Some((mr, draws, at)), _) if s.depth == 0 && at.elapsed() > std::time::Duration::from_millis(300) => {
                    Some((*mr, draws.clone()))
                }
                // Redrawn continuously (e.g. a blinking cursor): the last
                // complete redraw.
                (_, Some((mr, draws))) => Some((*mr, draws.clone())),
                _ => None,
            }
        };
        // No map on screen (title screen, menus): text only, after a moment.
        let frame = frame.or_else(|| {
            let waited = std::fs::metadata(request_path()).and_then(|m| m.modified()).ok()?.elapsed().ok()?;
            (waited > std::time::Duration::from_secs(1)).then(|| (0, Vec::new()))
        });
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
    let districts = game.map().map(|map| crate::districts::read(game, &map)).unwrap_or_default();
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
        let zoomed = game.zoomed_out().unwrap_or(false) as u8;
        let _ = writeln!(
            out,
            "view camera {} {} visible_x {} {} visible_y {} {} zoomed_out {zoomed}",
            view.camera_x, view.camera_y, view.tile_x.0, view.tile_x.1, view.tile_y.0, view.tile_y.1
        );
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
        if let Ok(d) = game.diplomacy(view.player_civ) {
            use civ3_sys::world::Relation;
            let code = |c: i32| match if c == 0 { Relation::War } else { d.relation(c) } {
                Relation::War => "w",
                Relation::Defense => "d",
                Relation::Peace => "p",
                Relation::None => "n",
            };
            let list: Vec<&str> = (0..32).map(code).collect();
            let _ = writeln!(out, "relations {}", list.join(" "));
        }
        // C3X's districts (docs §18), if it has any.
        for line in crate::world::district_lines(&districts.configs, &districts.improvements, &districts.free_improvements, &districts.districts) {
            let _ = writeln!(out, "{line}");
        }
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
        for b in &p.icon_buildings {
            let _ = writeln!(out, "icon_building {} {:#x} {:#x}", b.id, b.flags, b.obsolete_for);
        }
        for (civ, adjective) in p.adjectives.iter().enumerate().filter(|(_, a)| !a.is_empty()) {
            let _ = writeln!(out, "civ_adjective {civ} {adjective}");
        }
    }
    if let Ok(keys) = game.unit_type_keys() {
        for (i, k) in keys.iter().enumerate() {
            let _ = writeln!(out, "unit_type_key {i} {k}");
        }
    }
    if let Ok(stats) = game.unit_type_stats() {
        for (i, s) in stats.iter().enumerate() {
            let _ = writeln!(out, "unit_type_stats {i} {} {} {} {} {} {} {}", s[0], s[1], s[2], s[3], s[4], s[5], s[6]);
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
    if let Ok(Some((id, frame))) = game.selection() {
        let _ = writeln!(out, "selection {id} {frame}");
    }
    if let Ok(outposts) = game.outposts() {
        for (id, owner) in outposts {
            let _ = writeln!(out, "outpost {id} {owner}");
        }
    }
    if let Ok(Some(id)) = game.focus_city() {
        let _ = writeln!(out, "focus_city {id}");
    }
    if let Ok(Some(id)) = game.current_unit() {
        let _ = writeln!(out, "current_unit {id}");
    }
    if let Ok(Some((x, y))) = game.mouse_tile() {
        let _ = writeln!(out, "mouse_tile {x} {y}");
    }
    if let Ok(t) = game.targeting() {
        let (px, py) = t.pointer_tile.unwrap_or((-1, -1));
        let (cx, cy, r) = t.range_grid.map_or((-1, -1, -1), |g| (g.centre.0, g.centre.1, g.range));
        let _ = writeln!(out, "targeting {} {} {} {} {} {} {} {px} {py} {cx} {cy} {r}", t.mode, t.refused as u8, t.range, t.target_ok as u8, t.armed as u8, t.start.0, t.start.1);
    }
    if let Some(p) = crate::targeting::latest_path() {
        let _ = writeln!(out, "{}", p.trace_line());
    }
    if let Ok(p) = game.preferences() {
        let _ = writeln!(out, "preferences {p:#x}");
    }
    if let Ok(bits) = game.debug_mode_bits() {
        let _ = writeln!(out, "debug_mode_bits {bits:#x}");
    }
    if let Ok(keys) = game.era_keys() {
        for (i, k) in keys.iter().enumerate() {
            let _ = writeln!(out, "era_key {i} {k}");
        }
    }
    match game.drawn_units() {
        Ok(units) => {
            let _ = writeln!(out, "# unit id x y civ type damage moves container state on_top px py direction animation frame visible experience army_member army_size moving");
            let line = |kind: &str, u: &civ3_sys::DrawnUnit| {
                format!(
                    "{kind} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
                    u.id, u.x, u.y, u.civ, u.unit_type, u.damage, u.moves, u.container, u.state, u.always_on_top as u8,
                    u.pixel.0, u.pixel.1, u.direction, u.animation, u.frame, u.visible as u8, u.experience,
                    u.army_member, u.army_size, u.moving as u8
                )
            };
            for u in &units {
                let _ = writeln!(out, "{}", line("unit", u));
            }
            // The members armies show.
            for u in units.iter().filter(|u| u.army_member >= 0) {
                if let Ok(Some(m)) = game.unit(u.army_member) {
                    let _ = writeln!(out, "{}", line("army_unit", &m));
                }
            }
        }
        Err(e) => log::warn!("reading units: {e}"),
    }
    match game.cities() {
        Ok(cities) => {
            for c in cities {
                let bits: String = c.improvements.iter().map(|b| format!("{b:02x}")).collect();
                let wonder = c.improvement_in_production().filter(|&i| districts.is_wonder(i)).unwrap_or(-1);
                // The columns of the label (docs §10.8), then the names (a tab before each).
                let icons = if c.hud_icons.is_empty() { "-".to_owned() } else { c.hud_icons.iter().map(u8::to_string).collect::<Vec<_>>().join(",") };
                let _ = writeln!(
                    out,
                    "city {} {} {} {} {} {bits} {wonder} {} {} {} {} {} {} {icons} {}\t{}\t{}",
                    c.id, c.x, c.y, c.civ, c.size, c.order.0, c.order.1, c.policy, c.food_income, c.grow_turns, c.build_turns, c.veteran_icons,
                    c.order_name.as_deref().unwrap_or(""), c.name
                );
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
            let _ = writeln!(out, "# tile x y square_type square_parts overlays terrain_overlays river resource owner city fow visibility explored v3 vis_d0 visible_overlays victory_location building ruins city_area continent");
            for y in 0..map.height {
                for x in (y & 1..map.width).step_by(2) {
                    let t = &tiles[map.tile_index(x, y)];
                    let _ = writeln!(
                        out,
                        "tile {x} {y} {:#x} {:#x} {:#x} {:#x} {:#x} {} {} {} {:#x} {:#x} {:#x} {:#x} {:#x} {} {} {} {} {} {}",
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
                        t.victory_location(),
                        t.building_id(),
                        t.ruins() as u8,
                        t.city_area(),
                        t.continent()
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
    for t in recent_texts() {
        let [r, g, b] = t.color;
        let _ = writeln!(out, "text dc 0 at {} {} color {r} {g} {b} font {:?}/{}/{}/{}/{} str {}", t.x, t.y, t.face, t.height, t.weight, t.italic as u8, t.underline as u8, t.text);
    }
    // What takes the mouse (docs §16.11): kind, rectangle (x, y, w, h), id.
    if let Some(sites) = civ3_sys::controls::sites(game.build) {
        for c in civ3_sys::controls::controls(&civ3_sys::controls::Live, &sites) {
            let [x, y, w, h] = c.rect;
            let kind = match c.kind {
                civ3_sys::controls::ControlKind::Button => "button".to_owned(),
                civ3_sys::controls::ControlKind::ListItem => "list_item".to_owned(),
                civ3_sys::controls::ControlKind::List { scroll: [sx, sy, sw, sh], step } => format!("list scroll {sx} {sy} {sw} {sh} step {step}"),
            };
            let _ = writeln!(out, "control {x} {y} {w} {h} id {:#x} {kind}", c.id);
        }
        // The form tree they come from (what a missing control is part of).
        for f in civ3_sys::controls::form_tree(&civ3_sys::controls::Live, &sites) {
            let [x, y, w, h] = f.shown;
            let list = f.list.map_or(String::new(), |(p, count, first, shown, sane)| format!(" list {:#x} items {:#x} count {count} first {first} shown {shown} sane {sane}", p.list, p.items));
            let typed = f.typed.map_or(String::new(), |t| format!(" typed draw {t:#x}"));
            let _ = writeln!(out, "control_form {} {:#x} vt {:#x} status1 {:#x} visible {} {x} {y} {w} {h} children {}{list}{typed}", f.depth, f.addr, f.vtable, f.status1, f.visible as u8, f.children);
        }
    }
    // The mirrored interface (docs §16).
    if let Some(frame) = crate::ui_trace::frame() {
        for &(vtable, canvas, (x, y), (w, h)) in &frame.forms {
            let _ = writeln!(out, "ui_form {vtable:#x} {canvas:#x} {x} {y} {w} {h}");
        }
        for (canvas, range) in &frame.form_draws {
            let _ = writeln!(out, "# ui_canvas_draws {canvas:#x} {} {}", range.start, range.end);
        }
        for range in &frame.underlay_draws {
            let _ = writeln!(out, "ui_underlay {} {}", range.start, range.end);
        }
        for d in &frame.draws {
            let _ = match d {
                crate::ui::ScreenDraw::Image { path, rect: [rx, ry, rw, rh], x, y, keyed, clip: [c0, c1, c2, c3] , mask, palette } => {
                    let _ = writeln!(out, "ui_image {x} {y} {} {c0} {c1} {c2} {c3} {rx} {ry} {rw} {rh} {path}", *keyed as u8);
                    if let Some(p) = palette {
                        let _ = writeln!(out, "ui_image_palette {} {} {}", p.first, p.count, p.image);
                    }
                    match mask {
                        Some(m) => writeln!(out, "ui_image_mask {} {} {}", m.at.0, m.at.1, m.path),
                        None => Ok(()),
                    }
                }
                crate::ui::ScreenDraw::Fill { rect: [x, y, w, h], rgb: [r, g, b], alpha, clip: [c0, c1, c2, c3] } => {
                    writeln!(out, "ui_fill {x} {y} {w} {h} {r} {g} {b} {c0} {c1} {c2} {c3} {alpha}")
                }
                crate::ui::ScreenDraw::Text { run: t, clip: [c0, c1, c2, c3] } => {
                    let [r, g, b] = t.color;
                    writeln!(out, "ui_text {c0} {c1} {c2} {c3} text dc 0 at {} {} color {r} {g} {b} font {:?}/{}/{}/{}/{} str {}", t.x, t.y, t.face, t.height, t.weight, t.italic as u8, t.underline as u8, t.text)
                }
                crate::ui::ScreenDraw::Flc { path, chunk, x, y, clip: [c0, c1, c2, c3], plain } => {
                    let kind = if *plain { "ui_flc_plain" } else { "ui_flc" };
                    writeln!(out, "{kind} {x} {y} {c0} {c1} {c2} {c3} {chunk} {path}")
                }
                crate::ui::ScreenDraw::Surface { image, rect: [rx, ry, rw, rh], x, y, clip: [c0, c1, c2, c3], size: (sw, sh) } => {
                    writeln!(out, "ui_surface {image:#x} {x} {y} {c0} {c1} {c2} {c3} {rx} {ry} {rw} {rh} {sw} {sh}")
                }
            };
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
