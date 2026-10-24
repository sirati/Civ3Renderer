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
use std::cell::RefCell;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

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

#[derive(Clone)]
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

thread_local! {
    /// The recording, owned by the game's thread (the only one drawing).
    static STATE: RefCell<State> = const { RefCell::new(State { frame: None, previous: None, depth: 0 }) };
}
/// The request server wants a copy of the recording: the game's thread
/// publishes one with its next draw and clears this.
static WANT_FRAMES: AtomicBool = AtomicBool::new(false);
/// That copy, with when it was made.
static FRAMES: civ3r_sync::Latest<(std::time::Instant, State)> = civ3r_sync::Latest::new();

fn publish_if_wanted() {
    if WANT_FRAMES.swap(false, Ordering::AcqRel) {
        FRAMES.publish((std::time::Instant::now(), STATE.with_borrow(State::clone)));
    }
}
/// The running build, for resolving sprites of global arrays.
static BUILD: std::sync::OnceLock<civ3_sys::Build> = std::sync::OnceLock::new();

/// Skip the game's own map drawing (`Draw_Tiles`).
pub static GAME_MAP_OFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether map sprites are not to be blitted: the game's map rendering is
/// off and a map redraw is in progress.
fn map_blits_off() -> bool {
    GAME_MAP_OFF.load(std::sync::atomic::Ordering::Relaxed) && STATE.with_borrow(|s| s.depth > 0)
}

/// Calls , for diagnostics.
pub static SPRITE_CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub static DRAW_TILES_CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub struct Tracer {
    _draw_tiles: draw_tiles::Installed,
    _sprite: Option<sprite_draw_on_map::Installed>,
    _blend: Option<sprite_draw_on_map_blend::Installed>,
    _colored: Option<sprite_draw_with_color_table::Installed>,
}

/// Records a draw and reports whether it belongs to the main map pass.
fn record(map_renderer: u32, sprite: u32, x: u32, y: u32, params: Vec<u32>, kind: &'static str) -> bool {
    let in_map = STATE.with_borrow_mut(|s| {
        let in_map = s.depth > 0;
        if let Some((_, frame, at)) = s.frame.as_mut() {
            // Only map sprites say the redraw is still going: other blits
            // (UI, unit flags) run continuously.
            if kind == "map" {
                *at = std::time::Instant::now();
            }
            if frame.len() >= MAX_DRAWS {
                return in_map;
            }
            let resolved = if map_renderer != 0 { identify_canonical(map_renderer, sprite) } else { None }
                .or_else(|| BUILD.get().and_then(|&b| civ3_sys::sprites::identify_global(b, sprite)));
            let size = civ3_sys::sprites::size(sprite).unwrap_or((0, 0));
            frame.push(Draw { sprite: resolved, sprite_addr: sprite, x: x as i32, y: y as i32, size, params, kind });
        }
        in_map
    });
    publish_if_wanted();
    in_map
}

/// Text runs the game's thread drew, in order, for [`recent_texts`]: the
/// hook only sends, readers merge them into [`RECENT_TEXT`].
static NEW_TEXT: LazyLock<civ3r_sync::Mailbox<(std::time::Instant, TextRun)>> = LazyLock::new(civ3r_sync::Mailbox::new);
/// Text the game drew recently, newest last.
static RECENT_TEXT: civ3r_sync::Latest<std::collections::VecDeque<(std::time::Instant, TextRun)>> = civ3r_sync::Latest::new();

/// How long text stays "recent": the game redraws what is on screen more
/// often than this.
const TEXT_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

/// Adds `run`, drawn at `at`, to `recent`: a run replaces earlier ones at
/// the same position, older ones expire.
fn remember(recent: &mut std::collections::VecDeque<(std::time::Instant, TextRun)>, at: std::time::Instant, run: TextRun) {
    while recent.front().is_some_and(|(t, _)| at.saturating_duration_since(*t) > TEXT_WINDOW) {
        recent.pop_front();
    }
    recent.retain(|(_, r)| !(r.x == run.x && r.y == run.y));
    if recent.len() < 4096 {
        recent.push_back((at, run));
    }
}

/// A text run the game drew (any canvas), for [`recent_texts`].
pub(crate) fn recent_text(run: TextRun) {
    // The semantic notice reader owns this text in UI and text-only modes.
    if crate::unit_notices::drawing() { return; }
    NEW_TEXT.send((std::time::Instant::now(), run));
    // Unread (no reader in full UI mode): keep only the recent ones.
    if NEW_TEXT.len() > 8192 {
        let now = std::time::Instant::now();
        let runs: Vec<_> = NEW_TEXT.drain().filter(|(t, _)| now.duration_since(*t) <= TEXT_WINDOW).collect();
        runs.into_iter().rev().take(4096).rev().for_each(|r| NEW_TEXT.send(r));
    }
}

/// The text runs currently on screen (drawn within the last half second),
/// in drawing order; a run replaces earlier ones at the same position.
pub fn recent_texts() -> Vec<TextRun> {
    merge_new_text();
    let now = std::time::Instant::now();
    RECENT_TEXT
        .load()
        .map(|r| r.iter().filter(|(t, _)| now.duration_since(*t) <= TEXT_WINDOW).map(|(_, r)| r.clone()).collect())
        .unwrap_or_default()
}

fn merge_new_text() {
    let new: Vec<_> = NEW_TEXT.drain().collect();
    if new.is_empty() {
        return;
    }
    // Two readers may merge at once: each merges its own runs into the
    // newest list, in the order drawn (their times only grow).
    RECENT_TEXT.update(|old| {
        let mut recent = old.cloned().unwrap_or_default();
        let mut runs: Vec<_> = recent.drain(..).chain(new.iter().cloned()).collect();
        runs.sort_by_key(|(t, _)| *t);
        runs.into_iter().for_each(|(t, run)| remember(&mut recent, t, run));
        Some(recent)
    });
}

fn request_path() -> PathBuf {
    PathBuf::from("civ3-trace.request")
}

pub fn install(game: Civ3) -> Result<Tracer, inproc::hook::HookError> {
    let _ = BUILD.set(game.build);
    let sprite = match hooks::sprite_draw_on_map_site(game.build) {
        Some((addr, prologue)) => Some(sprite_draw_on_map::install_at_start(addr, prologue, |call| {
            SPRITE_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let in_map = record(call.map_renderer, call.sprite, call.pixel_x, call.pixel_y, vec![call.p4, call.p5, call.p6, call.p7], "map");
            // Top 5 Cities also draws through this entry, onto its form's
            // canvas at identity scale. JGL receives those blits directly,
            // bypassing the ordinary Sprite::draw hooks.
            //8060's lazy branch ignores scale/crop. Keep its file load;
            // the hooked PCX read records the source, and its hooked draw
            // captures the ordinary copy before suppressing native pixels.
            let file_source = civ3_sys::ui_hooks::obj::sprite_loads_file(call.sprite);
            let skip_ui = !file_source && !in_map
                && crate::ui_trace::on_map_sprite(call.sprite, call.map_renderer, call.pixel_x, call.pixel_y, [call.p4, call.p5, call.p6], call.p7);
            let skip = !file_source && (map_blits_off() || skip_ui);
            if !skip { crate::ui_trace::audit_map_original("map_sprite", call.sprite, call.map_renderer, &[call.pixel_x, call.pixel_y, call.p4, call.p5, call.p6, call.p7]); }
            crate::ui_trace::audit_call(crate::ui_trace::PixelClass::Map, skip, || call.original())
        })?),
        None => {
            log::warn!("no Sprite_draw_on_map site for {:?}; tracing disabled", game.build);
            None
        }
    };
    let blend = match hooks::sprite_draw_on_map_blend_site(game.build) {
        Some((addr, prologue)) => Some(sprite_draw_on_map_blend::install_at_start(addr, prologue, |call| {
            let in_map = record(call.map_renderer, call.sprite, call.pixel_x, call.pixel_y, vec![call.table, call.p5, call.p6, call.p7, call.p8], "blend");
            let skip_ui = !in_map && crate::ui_trace::on_map_blend(call.sprite, call.map_renderer, call.pixel_x, call.pixel_y, call.table, [call.p5, call.p6, call.p7], call.p8);
            let skip = map_blits_off() || skip_ui;
            if !skip { crate::ui_trace::audit_map_original("map_blend", call.sprite, call.map_renderer, &[call.pixel_x, call.pixel_y, call.table, call.p5, call.p6, call.p7, call.p8]); }
            crate::ui_trace::audit_call(crate::ui_trace::PixelClass::Map, skip, || crate::ui_trace::blend_wrapper_original(|| call.original()))
        })?),
        None => None,
    };
    // Resolve the sprite against the map renderer of the redraw in progress.
    let colored = match hooks::sprite_draw_with_color_table_site(game.build) {
        Some((addr, prologue)) => Some(sprite_draw_with_color_table::install_at_start(addr, prologue, |call| {
            let mr = STATE.with_borrow(|s| s.frame.as_ref().map(|f| f.0)).unwrap_or(0);
            record(mr, call.sprite, call.pixel_x, call.pixel_y, vec![call.canvas, call.canvas2, call.table, call.p6], "colored");
            crate::ui_trace::on_colored(call.sprite, call.canvas2, call.pixel_x, call.pixel_y, call.table, call.p6);
            // Inside a map redraw like the map's other blits; elsewhere a
            // unit picture the mirror recorded.
            let skip = map_blits_off() && crate::ui_trace::skipping() || crate::ui_trace::skips_colored(call.sprite, call.canvas2);
            if !skip { crate::ui_trace::audit_map_original("map_colored", call.sprite, call.canvas2, &[call.pixel_x, call.pixel_y, call.canvas, call.table, call.p6]); }
            crate::ui_trace::audit_call(crate::ui_trace::PixelClass::Map, skip, || call.original())
        })?),
        None => None,
    };
    // Normally game_window's early LoadLibraryA callback took jgl's text
    // group over before any jgl call. Cover injection into an
    // already-loaded module synchronously as well.
    if let Some(base) = inproc::pe::module_base("jgl.dll") {
        if let Err(e) = crate::jgl_hooks::install_text(base) {
            log::warn!("taking over jgl's text group: {e}");
        }
    }
    let draw_tiles = draw_tiles::install_slot(hooks::draw_tiles_slot(game.build), move |call| {
        DRAW_TILES_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let first = STATE.with_borrow_mut(|s| {
            s.depth += 1;
            s.depth == 1
        });
        if first {
            crate::ui_trace::map_redraw(call.map_renderer);
            STATE.with_borrow_mut(|s| {
                if let Some((mr, draws, _)) = s.frame.take() {
                    s.previous = Some((mr, draws));
                }
                s.frame = Some((call.map_renderer, Vec::new(), std::time::Instant::now()));
            });
        }
        // With the game's map rendering switched off its sprites are not
        // blitted (see `map_blits_off`), but the pass still runs: it also
        // draws the city labels the interface mirror shows.
        call.original();
        STATE.with_borrow_mut(|s| s.depth -= 1);
        publish_if_wanted();
    })?;
    // (A waiting actor: on the bridge's runtime, not a thread of its own.)
    if !crate::runtime::spawn(serve_requests(game)) {
        log::warn!("trace: no runtime for the request server");
    }
    Ok(Tracer { _draw_tiles: draw_tiles, _sprite: sprite, _blend: blend, _colored: colored })
}

/// Answers trace requests with the latest redraw, once it has been quiet
/// for a moment (so it is complete).
async fn serve_requests(game: Civ3) {
    // When the current request was first seen: only a copy of the recording
    // the game's thread made after that answers it.
    let mut asked = None;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        crate::ui_trace::serve_pixel_audit();
        if !request_path().exists() {
            asked = None;
            continue;
        }
        let asked = *asked.get_or_insert_with(std::time::Instant::now);
        WANT_FRAMES.store(true, Ordering::Release);
        let copy = FRAMES.load().filter(|c| c.0 >= asked);
        let frame = copy.as_ref().and_then(|copy| {
            let s = &copy.1;
            match (&s.frame, &s.previous) {
                (Some((mr, draws, at)), _) if s.depth == 0 && at.elapsed() > std::time::Duration::from_millis(300) => {
                    Some((*mr, draws.clone()))
                }
                // Redrawn continuously (e.g. a blinking cursor): the last
                // complete redraw.
                (_, Some((mr, draws))) => Some((*mr, draws.clone())),
                _ => None,
            }
        });
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
        WANT_FRAMES.store(false, Ordering::Release);
    }
}

fn format_trace(game: &Civ3, map_renderer: u32, draws: &[Draw]) -> String {
    let mut out = String::new();
    let districts = game.map().map(|map| crate::districts::read(game, &map)).unwrap_or_default();
    let _ = writeln!(out, "# civ3-bridge trace: map_renderer {map_renderer:#x}");
    let _ = writeln!(out, "# native_audio {:?}", civ3_sys::audio_init::diagnostics());
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
        let _ = writeln!(out, "targeting {} {} {} {} {} {} {} {px} {py} {cx} {cy} {r} {}", t.mode, t.refused as u8, t.range, t.target_ok as u8, t.armed as u8, t.start.0, t.start.1, t.grid as u8);
    }
    if let Some(p) = crate::targeting::latest_path() {
        let _ = writeln!(out, "{}", p.trace_line());
    }
    if let Some((x, y)) = crate::world_input::preselected() {
        let _ = writeln!(out, "preselected {x} {y}");
    }
    if let Ok(modes) = game.navigator_modes() {
        use civ3_sys::ui_hooks::NavigatorMode;
        let _ = writeln!(out, "navigator_modes {} {}", u8::from(modes.main == NavigatorMode::Geography), u8::from(modes.advisor == NavigatorMode::Geography));
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
        Ok(mut units) => {
            let _ = crate::unit_views::copy_captured_movement(game, &mut units);
            let _ = writeln!(out, "# unit id x y civ type damage moves container state on_top px py direction animation frame visible experience army_member army_size moving leader_kind max_move_points");
            let line = |kind: &str, u: &civ3_sys::DrawnUnit| {
                format!(
                    "{kind} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
                    u.id, u.x, u.y, u.civ, u.unit_type, u.damage, u.moves, u.container, u.state, u.always_on_top as u8,
                    u.pixel.0, u.pixel.1, u.direction, u.animation, u.frame, u.visible as u8, u.experience,
                    u.army_member, u.army_size, u.moving as u8, u.leader_kind, u.max_move_points.unwrap_or(-1)
                )
            };
            for u in &units {
                let _ = writeln!(out, "{}", line("unit", u));
            }
            // The members of the armies (the one each shows among them).
            let armies: Vec<i32> = units.iter().filter(|u| u.army_member >= 0).map(|u| u.id).collect();
            let mut members = game.units_in(&armies).unwrap_or_default();
            for u in units.iter().filter(|u| u.army_member >= 0) {
                if !members.iter().any(|m| m.id == u.army_member) {
                    members.extend(game.unit(u.army_member).ok().flatten());
                }
            }
            for m in &members {
                let _ = writeln!(out, "{}", line("army_unit", m));
            }
        }
        Err(e) => log::warn!("reading units: {e}"),
    }
    // The animations drawn besides units (docs §10.14).
    match game.effects() {
        Ok(effects) => {
            for e in &effects {
                let _ = writeln!(out, "{}", crate::effects::trace_line(&crate::live::effect(e)));
            }
        }
        Err(e) => log::warn!("reading effects: {e}"),
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
                civ3_sys::controls::ControlKind::Slider { track, value } => format!("slider track {track:?} value {value}"),
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
                crate::ui::ScreenDraw::Image { path, rect: [rx, ry, rw, rh], x, y, keyed, clip: [c0, c1, c2, c3] , mask, palette, opacity } => {
                    let _ = writeln!(out, "ui_image {x} {y} {} {c0} {c1} {c2} {c3} {rx} {ry} {rw} {rh} {path}", *keyed as u8);
                    if *opacity != 255 {
                        let _ = writeln!(out, "ui_image_opacity {opacity}");
                    }
                    if let Some(p) = palette {
                        let _ = writeln!(out, "ui_image_palette {} {} {}", p.first, p.count, p.image);
                    }
                    match mask {
                        Some(m) => writeln!(out, "ui_image_mask {} {} {}", m.at.0, m.at.1, m.path),
                        None => Ok(()),
                    }
                }
                crate::ui::ScreenDraw::Indexed { path, rect: [rx, ry, rw, rh], x, y, clip: [c0, c1, c2, c3], palette, indices } => {
                    if let Some(resize) = indices.resize {
                        let (w, h) = resize.size;
                        let [sx, sy] = resize.sample_step;
                        let _ = writeln!(out, "ui_resized_image {x} {y} 1 {c0} {c1} {c2} {c3} {rx} {ry} {rw} {rh} {w} {h} {sx} {sy} {path}");
                    } else {
                        let _ = writeln!(out, "ui_image {x} {y} 1 {c0} {c1} {c2} {c3} {rx} {ry} {rw} {rh} {path}");
                    }
                    if let Some(p) = palette { let _ = writeln!(out, "ui_image_palette {} {} {}", p.first, p.count, p.image); }
                    let transparent = indices.transparent.iter().map(u8::to_string).collect::<Vec<_>>().join(",");
                    let shade = indices.shade.iter().map(|(i, k)| format!("{i}:{k}")).collect::<Vec<_>>().join(",");
                    let _ = writeln!(out, "ui_image_indices {} {}", if transparent.is_empty() { "-" } else { &transparent }, if shade.is_empty() { "-" } else { &shade });
                    if let Some(frame) = indices.frame { let _ = writeln!(out, "ui_image_frame {frame}"); }
                    if let Some(p) = &indices.palette_alpha {
                        writeln!(out, "ui_image_palette_alpha {} {} {} {} {} {}", p.first, p.count, p.skip[0], p.skip[1], p.skip[2], p.source)
                    } else { Ok(()) }
                }
                crate::ui::ScreenDraw::ResizedImage { path, rect: [rx, ry, rw, rh], x, y, size: (w,h), sample_step:[sx,sy], keyed, clip:[c0,c1,c2,c3], mask,palette,opacity } => {
                    let _ = writeln!(out,"ui_resized_image {x} {y} {} {c0} {c1} {c2} {c3} {rx} {ry} {rw} {rh} {w} {h} {sx} {sy} {path}",*keyed as u8);
                    if *opacity !=255 { let _=writeln!(out,"ui_image_opacity {opacity}"); }
                    if let Some(p)=palette { let _=writeln!(out,"ui_image_palette {} {} {}",p.first,p.count,p.image); }
                    match mask { Some(m)=>writeln!(out,"ui_image_mask {} {} {}",m.at.0,m.at.1,m.path),None=>Ok(()) }
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
