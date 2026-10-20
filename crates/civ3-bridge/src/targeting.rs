//! What the game shows on its map while a unit command waits for a target
//! tile (docs §10.15), made from the game's state so that every view shows
//! it, whether or not the game draws its own map:
//!
//! - bombarding (land, sea, air), precision strikes, air drops, airlifts,
//!   recon, rebasing and the other modes: the tile frame of `Art/Cursor.pcx`
//!   (1, 220) on the tile under the mouse when the command takes it (in
//!   range and a target) (0x4DC540);
//! - go-to (one unit or a stack): the path the game's path finder found,
//!   red 1-pixel lines from tile centre to tile centre, the selection
//!   cursor (a still frame) on the tile under the mouse (0x4E46A0), and how
//!   many turns it takes in white bold Lucida Sans 16 with a black shadow,
//!   a `+` after it for a stack (0x4E4780);
//! - with a range (bombarding, air missions): every tile in range outlined
//!   in red, 1-pixel lines along its diamond's edges, under the fog of war
//!   (the map's grid pass, `Map_Renderer::m36`, 0x4C57C0);
//!
//! The pointer (the command's own cell of `Art/Cursor.pcx`) is the game's
//! cursor, which the scene carries anyway (`crate::cursor`).
//!
//! The frame and the go-to overlay go over the fog of war and under the
//! units (the animator draws the units after them).

use std::fmt::Write as _;

use render_desc::Compiled;

use crate::city_labels::Env;
use crate::text::TextRun;
use crate::ui_screen::Item;
use crate::world::Civ3World;

/// `Unit_Mode_Actions` (see `civ3_sys::targeting`).
pub use civ3_sys::targeting::{GO_TO, STACK_GO_TO};

/// `Unit_Mode_Actions` values are below this (the pointer cells of
/// `Art/Cursor.pcx`, one a mode).
const MODES: i32 = 23;
const TARGET_SET: &str = "TargetTile/0";
/// The go-to path's colour: pure red (RGB555 0x7C00).
const PATH_RGB: [u8; 3] = [255, 0, 0];
/// The go-to marker's frame of the cursor animation (0x4E46B2: always 1).
const MARKER_FRAME: i32 = 1;
/// The turn count's font: `get_font(16, 1)` = Lucida Sans 16 bold.
const LABEL_FACE: &str = "Lucida Sans";
const LABEL_SIZE: i32 = 16;
const WHITE: [u8; 3] = [248, 248, 248];
const BLACK: [u8; 3] = [0, 0, 0];
/// `font[+0x10]` of the label's font (see `city_labels::text_height`).
const LABEL_TEXT_HEIGHT: i32 = 14;

/// The go-to path from the selected unit to the tile under the mouse, as
/// the game's path finder (`Trade_Net::set_unit_path`) gives it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GotoPath {
    /// The selected unit (id) and the tile the path goes to.
    pub unit: i32,
    pub to: (i32, i32),
    /// The tiles from `to` back to the start, both included.
    pub tiles: Vec<(i32, i32)>,
    /// Its length in move points, the unit's move points per turn
    /// (`Unit::get_max_move_points`) and those it has used this turn.
    pub length: i32,
    pub max_moves: i32,
    pub moves_used: i32,
}

impl GotoPath {
    /// How many turns the label says (0x4E486D..0x4E48B6): 0 for a path of
    /// no length, else the turns after this one's moves are used up, plus
    /// one.
    pub fn turns(&self) -> i32 {
        if self.length <= 0 {
            return 0;
        }
        let max = self.max_moves.max(1);
        let left = (max - self.moves_used).clamp(0, 9999);
        (self.length - left + max - 1) / max + 1
    }

    /// A trace line (see `world::Trace::parse`).
    pub fn trace_line(&self) -> String {
        let mut out = format!("goto_path {} {} {} {} {} {}", self.unit, self.to.0, self.to.1, self.length, self.max_moves, self.moves_used);
        for (x, y) in &self.tiles {
            let _ = write!(out, " {x} {y}");
        }
        out
    }

    pub fn parse(fields: &[i64]) -> Option<GotoPath> {
        let v: Vec<i32> = fields.iter().map(|&f| f as i32).collect();
        let [unit, tx, ty, length, max_moves, moves_used] = *v.get(..6)? else { return None };
        let tiles = v[6..].chunks_exact(2).map(|c| (c[0], c[1])).collect();
        Some(GotoPath { unit, to: (tx, ty), tiles, length, max_moves, moves_used })
    }
}

/// Whether `mode` waits for a target tile (0 and `IDC_ARROW` do not).
pub fn active(mode: i32) -> bool {
    (1..MODES).contains(&mode)
}

/// Whether `mode` is a move's (one unit's or a stack's go-to, its target
/// showing the turns): the viewer shows the cursor tile by tile for it
/// (`scene::Interface::snap_to_tiles`). Air units' rebase (0xA, 0x13) is
/// not one.
pub fn go_to(mode: i32) -> bool {
    mode == GO_TO || mode == STACK_GO_TO
}

/// The tile the go-to overlay is about: the one under the mouse the go-to
/// command keeps (`field_4E80[3..4]`), while it waits for it (0x4EF187).
fn go_to_target(world: &Civ3World) -> Option<(i32, i32)> {
    let t = &world.targeting;
    (go_to(t.mode) && t.armed).then_some(world.mouse_tile).flatten()
}

/// The tile the frame marks: the one under the pointer while a command
/// other than go-to waits for a tile and takes that one (0x4DC540).
fn framed(world: &Civ3World) -> Option<(i32, i32)> {
    let t = &world.targeting;
    (active(t.mode) && !go_to(t.mode) && t.target_ok).then_some(t.pointer_tile).flatten()
}

/// The path to show: the one found for the selected unit to the tile under
/// the mouse (0x4E4350: none to the start tile itself).
fn path(world: &Civ3World) -> Option<&GotoPath> {
    let to = go_to_target(world)?;
    let p = world.goto.as_ref()?;
    (p.to == to && Some(p.unit) == world.current_unit && to != world.targeting.start).then_some(p)
}

/// The eight steps between neighbouring tile centres (`civ3_sys::targeting::
/// neighbor_diff` order), in tile units.
const STEPS: [(i32, i32); 8] = [(1, -1), (2, 0), (1, 1), (0, 2), (-1, 1), (-2, 0), (-1, -1), (0, -2)];

/// The pixels of a line from (0, 0) to `to`, both ends included, as the
/// game's `PCX_Image::draw_line` (0x5FFFB0) sets them (Bresenham).
fn line_points(to: (i32, i32)) -> Vec<(i32, i32)> {
    let (dx, dy) = (to.0.abs(), -to.1.abs());
    let (sx, sy) = (to.0.signum(), to.1.signum());
    let (mut x, mut y, mut err) = (0, 0, dx + dy);
    let mut out = vec![(0, 0)];
    while (x, y) != to {
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
        out.push((x, y));
    }
    out
}

/// The edges of a tile's diamond the range outline draws (0x4C57C0), from
/// the box's top-left in tiles of `w × h`: left to top, top to right,
/// bottom to right, left to bottom.
fn diamond_edges(w: i32, h: i32) -> [((i32, i32), (i32, i32)); 4] {
    [((0, h / 2), (w / 2, 0)), ((w / 2, 0), (w, h / 2)), ((w / 2, h), (w, h / 2)), ((0, h / 2), (w / 2, h))]
}

/// A line image's description entries: `name` drawing the pixels of the
/// line from (0, 0) to `to` in `rgb`, anchored at its start, drawn at its
/// size in views showing the world smaller.
fn line_entries(out: &mut String, name: &str, group: &str, index: usize, to: (i32, i32), rgb: [u8; 3]) {
    pixel_entries(out, name, group, index, &line_points(to), rgb);
}

/// The description entries of an image of the pixels `points` in `rgb`,
/// anchored at (0, 0) of the points' coordinates, drawn at its size in
/// views showing the world smaller.
fn pixel_entries(out: &mut String, name: &str, group: &str, index: usize, points: &[(i32, i32)], [r, g, b]: [u8; 3]) {
    let (x0, y0) = (points.iter().map(|p| p.0).min().unwrap_or(0), points.iter().map(|p| p.1).min().unwrap_or(0));
    let (x1, y1) = (points.iter().map(|p| p.0).max().unwrap_or(0), points.iter().map(|p| p.1).max().unwrap_or(0));
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    let rects: Vec<String> = points.iter().map(|(x, y)| format!("{r},{g},{b},255:{},{},1,1", x - x0, y - y0)).collect();
    let _ = writeln!(
        out,
        "[images.{name}]\nsource = \"rects:{w}x{h};{}\"\n\n[sheets.{name}]\nimage = \"{name}\"\ncell = [{w}, {h}]\ngrid = [1, 1]\nanchor = [{x0}, {y0}]\ngroup = \"{group}\"\nfirst_index = {index}\nscaling = \"keep\"\n",
        rects.join(";")
    );
}

/// The step's line in the view's pixels: a tile is 128×64, zoomed out
/// 64×32 (`half`).
fn step_vector((dx, dy): (i32, i32), half: bool) -> (i32, i32) {
    let n = if half { 2 } else { 1 };
    (64 * dx / n, 32 * dy / n)
}

/// Render description entries: the tile frame from `Art/Cursor.pcx`, the
/// path's lines (`GotoLine@{1,2}/{step}`, from a tile centre) and the range
/// outline (`RangeTile@{1,2}/0`, from a tile box's top-left), 1:1 and
/// zoomed out, drawn at their size in both.
pub fn description() -> String {
    let mut out = String::from("\n# Targeting overlays (generated, docs §10.15).\n");
    out.push_str("[images.targeting_frame]\nsource = \"Art/Cursor.pcx\"\ntransparent = [255]\n\n");
    out.push_str(
        "[sheets.targeting_tile]\nimage = \"targeting_frame\"\norigin = [1, 220]\ncell = [128, 64]\ngrid = [1, 1]\ngroup = \"TargetTile\"\nsample_from = \"box\"\n\n",
    );
    out.push_str(&lines_description());
    out
}

/// The description entries of the lines (the go-to path's steps and the
/// range outline's tiles; 1:1 and zoomed out).
fn lines_description() -> String {
    let mut out = String::new();
    for (zoom, half) in [(1, false), (2, true)] {
        for (d, &step) in STEPS.iter().enumerate() {
            line_entries(&mut out, &format!("goto_line_{zoom}_{d}"), &format!("GotoLine@{zoom}"), d, step_vector(step, half), PATH_RGB);
        }
        let n = if half { 2 } else { 1 };
        pixel_entries(&mut out, &format!("range_tile_{zoom}"), &format!("RangeTile@{zoom}"), 0, &range_outline(128 / n, 64 / n), PATH_RGB);
    }
    out
}

fn free(compiled: &Compiled, name: &str, (x, y): (i32, i32), after_pass: u32) -> Option<scene::FreeSprite> {
    let (set, _) = compiled.set(name)?;
    Some(scene::FreeSprite { set: set.raw(), x, y, after_pass, ticks_per_frame: 1, ..Default::default() })
}

/// The world sprites (full-size world pixels, as the units), drawn over the
/// fog of war and under the units (the animator draws the units after
/// them, 0x4EF1B1) in this order: the path's lines, the go-to marker
/// (0x4E46A0: the selection cursor's frame 1 of direction 5 in the viewer's
/// colours, centred on the tile) or the tile frame. `zoomed_out`: the game's
/// view is the half-size one (the lines are drawn 1 pixel wide there too).
pub fn sprites(world: &Civ3World, zoomed_out: bool, art: &mut crate::units::UnitArt, compiled: &Compiled) -> Vec<scene::FreeSprite> {
    let under_units = crate::units::unit_passes(compiled).1;
    let mut out = Vec::new();
    let centre = |(x, y): (i32, i32)| (64 * x + 64, 32 * y + 32);
    if let Some(to) = go_to_target(world) {
        if let Some(p) = path(world) {
            let zoom = if zoomed_out { 2 } else { 1 };
            for pair in p.tiles.windows(2) {
                let step = step_between(pair[0], pair[1], world);
                if let Some(d) = step.and_then(|s| STEPS.iter().position(|&t| t == s)) {
                    out.extend(free(compiled, &format!("GotoLine@{zoom}/{d}"), centre(pair[0]), under_units));
                }
            }
        }
        if let Some((name, palette)) = crate::units::cursor_sprite(world, art, compiled, MARKER_FRAME) {
            out.extend(free(compiled, &name, centre(to), under_units).map(|s| scene::FreeSprite { palette, ..s }));
        }
        if let Some(name) = crate::units::cursor_smoke_sprite(world, art, compiled, MARKER_FRAME) {
            out.extend(free(compiled, &name, centre(to), under_units).map(|s| scene::FreeSprite { blend: scene::SpriteBlend::PaletteAlpha256, ..s }));
        }
    }
    if let Some((x, y)) = framed(world) {
        out.extend(free(compiled, TARGET_SET, (64 * x, 32 * y), under_units));
    }
    out
}

/// The pixels the range outline sets for one tile, from its box's
/// top-left in tiles of `w × h`: its diamond's four edges, one pixel to the
/// right (measured against the game, 1:1 and zoomed out).
fn range_outline(w: i32, h: i32) -> Vec<(i32, i32)> {
    let mut out: Vec<(i32, i32)> = diamond_edges(w, h)
        .into_iter()
        .flat_map(|(from, to)| line_points((to.0 - from.0, to.1 - from.1)).into_iter().map(move |(x, y)| (from.0 + x + 1, from.1 + y)))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The range outline (see the module's documentation): a world sprite per
/// tile in range (full-size world pixels, at the tile box's top-left),
/// drawn after the pass before the fog of war. A range beyond the map's
/// size takes the whole map.
pub fn range_sprites(world: &Civ3World, compiled: &Compiled, zoomed_out: bool) -> Vec<scene::FreeSprite> {
    let Some(grid) = world.targeting.range_grid else { return Vec::new() };
    let Some((set, _)) = compiled.set(&format!("RangeTile@{}/0", if zoomed_out { 2 } else { 1 })) else { return Vec::new() };
    let pass = crate::ui_screen::under_pass(compiled);
    // How far it reaches in tile units either way, at most across the map
    // (once round a map that wraps).
    let window = |size: i32, wraps: bool| {
        let r = 2 * grid.range.clamp(0, size.max(0)) + 1;
        if wraps && 2 * r + 1 > size { (-(size / 2), size - size / 2 - 1) } else { (-r, r) }
    };
    let ((x0, x1), (y0, y1)) = (window(world.width, world.wrap_x), window(world.height, world.wrap_y));
    let (cx, cy) = grid.centre;
    let mut out = Vec::new();
    for dy in y0..=y1 {
        for dx in x0..=x1 {
            let (x, y) = (cx + dx, cy + dy);
            if world.tile(x, y).is_none() || distance(world, (cx, cy), (x, y)) > grid.range {
                continue;
            }
            out.push(scene::FreeSprite { set: set.raw(), x: 64 * x, y: 32 * y, after_pass: pass, ticks_per_frame: 1, ..Default::default() });
        }
    }
    out
}

/// `Map::get_distance` (0x44A950): half the sum of the coordinate
/// differences, each the shorter way round a wrapping map.
fn distance(world: &Civ3World, (x1, y1): (i32, i32), (x2, y2): (i32, i32)) -> i32 {
    let axis = |d: i32, n: i32, wraps: bool| {
        let d = d.abs();
        if wraps && d > n / 2 { n - d } else { d }
    };
    (axis(x1 - x2, world.width, world.wrap_x) + axis(y1 - y2, world.height, world.wrap_y)) / 2
}

/// The step from tile `a` to its neighbour `b` (across a wrapping edge as a
/// single step); `None` if they are not neighbours.
fn step_between(a: (i32, i32), b: (i32, i32), world: &Civ3World) -> Option<(i32, i32)> {
    let near = |d: i32, n: i32, wraps: bool| if wraps && n > 0 { (d + n / 2).rem_euclid(n) - n / 2 } else { d };
    let s = (near(b.0 - a.0, world.width, world.wrap_x), near(b.1 - a.1, world.height, world.wrap_y));
    STEPS.contains(&s).then_some(s)
}

/// The turn count as text items (`ui_screen::Item`), laid out in the game
/// view's pixels (zoomed out: the half-size ones, see
/// `scene_map::from_zoomed_out`) as `PCX_Image::draw_text_in_rect`
/// (0x5FF0D0) centres it in the tile's box: the black shadow 2 pixels right
/// and down of the white text, which sits a pixel above where the routine
/// puts a plain line (measured).
pub fn label_items(world: &Civ3World, zoomed_out: bool, env: &mut dyn Env) -> Vec<Item> {
    let Some(p) = path(world) else { return Vec::new() };
    let mut text = p.turns().to_string();
    if world.targeting.mode == STACK_GO_TO {
        text.push('+');
    }
    let font = civ3_assets::fonts::FontStyle { face: LABEL_FACE.to_owned(), height: LABEL_SIZE, weight: 700, italic: false };
    let width = env.text_width(&font, &text);
    let (x, y) = p.to;
    let (w, h) = if zoomed_out { (64, 32) } else { (128, 64) };
    let (left, top) = (w / 2 * x, h / 2 * y);
    let pen = ((w - width) / 2 + left, (h - LABEL_TEXT_HEIGHT) / 2 + top + LABEL_TEXT_HEIGHT - 1);
    let everywhere = [-(1 << 24), -(1 << 24), 1 << 24, 1 << 24];
    [(BLACK, (pen.0 + 2, pen.1 + 2)), (WHITE, pen)]
        .into_iter()
        .map(|(color, (x, y))| {
            let face = font.face.clone();
            let run = TextRun { x, y, color, face, height: -LABEL_SIZE, weight: 700, italic: false, underline: false, text: text.clone(), backdrop: None };
            Item::Text(run, everywhere, true)
        })
        .collect()
}

#[cfg(windows)]
pub use live::{install, latest_path, on_game_thread};

/// The go-to path, found on the game's own thread (the path finder is the
/// game's; it must not run beside the game's code): from the main screen's
/// 66 ms timer in the human player's turn, whenever the target, the unit or
/// its moves change, and again now and then (what lies on the way may
/// change).
#[cfg(windows)]
mod live {
    use std::cell::{Cell, RefCell};
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    use civ3_sys::Civ3;
    use civ3_sys::targeting::{self as t, max_move_points, set_unit_path};

    use super::{GotoPath, go_to};

    /// How long a path stays fresh when nothing it depends on changed.
    const REFRESH: Duration = Duration::from_millis(500);
    // Pointer messages may arrive at 240 Hz. The native path finder runs
    // on the game thread, so limit recomputation while the target moves.
    // The game timer takes the latest target after the last pointer move.
    const MIN_REFRESH: Duration = Duration::from_millis(33);

    struct Live {
        game: Civ3,
        trade_net: u32,
        set_unit_path: set_unit_path::Function,
        max_move_points: max_move_points::Function,
    }

    /// What a path was found for: unit, start, target, moves used.
    type Key = (u32, (i32, i32), (i32, i32), i32);

    static LIVE: OnceLock<Live> = OnceLock::new();
    thread_local! {
        /// What the last path was found for and when (on the game's thread).
        static FOUND: RefCell<Option<(Key, Instant, Option<GotoPath>)>> = const { RefCell::new(None) };
    }
    /// The path last found, for the publisher.
    static LATEST: civ3r_sync::Latest<GotoPath> = civ3r_sync::Latest::new();

    pub fn install(game: Civ3) -> Result<(), String> {
        let sites = t::sites(game.build).ok_or_else(|| format!("no path finder sites for {:?}", game.build))?;
        let e = |what: &'static str| move |err: inproc::hook::HookError| format!("{what}: {err}");
        let live = Live {
            game,
            trade_net: sites.trade_net,
            set_unit_path: set_unit_path::Function::at(sites.set_unit_path.0, sites.set_unit_path.1).map_err(e("Trade_Net::set_unit_path"))?,
            max_move_points: max_move_points::Function::at(sites.max_move_points.0, sites.max_move_points.1).map_err(e("Unit::get_max_move_points"))?,
        };
        LIVE.set(live).map_err(|_| "installed twice".to_owned())?;
        let tick = t::screen_tick::install_at_start(sites.screen_tick.0, sites.screen_tick.1, |call| {
            call.original();
            on_game_thread();
        })
        .map_err(e("the main screen's timer"))?;
        // (Stays for the life of the process.)
        std::mem::forget(tick);
        Ok(())
    }

    /// The path last found, if it is still about the game's state.
    pub fn latest_path() -> Option<GotoPath> {
        LATEST.get()
    }

    /// Finds the path the game shows now, if it changed (on the game's
    /// thread, from the main screen's timer).
    pub fn on_game_thread() {
        let Some(live) = LIVE.get() else { return };
        let game = &live.game;
        let Ok(targeting) = game.targeting() else { return };
        let (Ok(Some(to)), Ok(unit)) = (game.mouse_tile(), game.current_unit_ptr()) else { return clear() };
        if !go_to(targeting.mode) || !targeting.armed || unit == 0 {
            return clear();
        }
        let used = t::moves_used(unit).unwrap_or(0);
        let key = (unit, targeting.start, to, used);
        let now = Instant::now();
        if FOUND.with_borrow(|latest| latest.as_ref().is_some_and(|(k, at, _)| {
                let elapsed = now.duration_since(*at);
                (*k == key && elapsed < REFRESH) || (k.0 == key.0 && elapsed < MIN_REFRESH)
            })) {
            return;
        }
        let path = find(live, unit, targeting.start, to, used);
        FOUND.with_borrow_mut(|latest| {
            if latest.as_ref().is_none_or(|(_, _, p)| *p != path) {
                log::debug!("go-to path from {:?} to {to:?}: {path:?}", targeting.start);
                match &path {
                    Some(p) => LATEST.publish(p.clone()),
                    None => LATEST.clear(),
                }
            }
            *latest = Some((key, now, path));
        });
    }

    fn clear() {
        FOUND.set(None);
        LATEST.clear();
    }

    /// 0x4E4350: the path finder from `from` to `to` for the unit, with the
    /// main screen's flags, as the viewing player.
    fn find(live: &Live, unit: u32, from: (i32, i32), to: (i32, i32), used: i32) -> Option<GotoPath> {
        let game = &live.game;
        if from == to {
            return None;
        }
        let place = game.unit_at_ptr(unit).ok()?;
        let civ = game.player_civ().ok()?;
        let map = game.map().ok()?;
        // The game writes this out parameter during the call. Cell permits
        // that write while the storage stays alive on this thread's stack.
        let length = Cell::new(-1i32);
        let out = length.as_ptr() as usize as u32;
        let found = live.set_unit_path.call(live.trade_net, from.0 as u32, from.1 as u32, to.0 as u32, to.1 as u32, unit, civ as u32, t::PATH_FLAGS, out) as i32;
        let length = length.get();
        if found <= 0 {
            log::debug!("go-to path from {from:?} to {to:?}: none ({found}, length {length})");
            return None;
        }
        let tiles = t::walk(|x, y| t::direction(live.trade_net, x, y).unwrap_or(0), from, to, (map.width, map.height), (map.wrap_x, map.wrap_y));
        let max_moves = live.max_move_points.call(unit) as i32;
        Some(GotoPath { unit: place.id, to, tiles, length, max_moves, moves_used: used })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mission_validity_controls_both_the_target_frame_and_cursor() {
        use civ3_sys::cursor::GameCursor;

        // Mission groups from Main_Screen_Form::check_action_target at
        // 0x4DC360: drop, bombard, airlift, rebase, precision and recon.
        for mode in [4, 6, 7, 15, 16, 8, 10, 19, 11, 17, 22] {
            let mut world = Civ3World::default();
            world.targeting.mode = mode;
            world.targeting.pointer_tile = Some((12, 8));
            // Move between valid and invalid targets while the game's real
            // pointer stays on a button. Both visuals must follow validity.
            for valid in [true, false, true] {
                world.targeting.target_ok = valid;
                assert_eq!(framed(&world), valid.then_some((12, 8)), "mode {mode}");
                let cursor = crate::cursor::targeting(Some(GameCursor::System), mode, valid, true);
                let cell = if valid { mode as u32 } else { crate::cursor::NOT_HERE };
                assert_eq!(cursor, Some(GameCursor::Sprite(cell)), "mode {mode}");
                assert!(!go_to(mode), "mission {mode} must not request tile snapping");
                assert_eq!(crate::cursor::targeting(Some(GameCursor::System), mode, valid, false), Some(GameCursor::System));
            }
        }
    }

    #[test]
    fn only_single_and_stack_moves_request_tile_snapping() {
        let moves: Vec<_> = (-1..=0x18).filter(|&mode| go_to(mode)).collect();
        assert_eq!(moves, [1, 0x12]);
        assert!(!go_to(0x7f00)); // IDC_ARROW while no command waits.
    }

    #[test]
    fn turns_count_this_turns_moves_first() {
        let p = |length, max_moves, moves_used| GotoPath { length, max_moves, moves_used, ..Default::default() };
        // A unit with 1 move (3 move points), none used: a path of 3 takes
        // this turn and shows 1.
        assert_eq!(p(3, 3, 0).turns(), 1);
        assert_eq!(p(1, 3, 0).turns(), 1);
        assert_eq!(p(4, 3, 0).turns(), 2);
        assert_eq!(p(6, 3, 0).turns(), 2);
        assert_eq!(p(7, 3, 0).turns(), 3);
        // Moves used this turn push it on.
        assert_eq!(p(3, 3, 3).turns(), 2);
        assert_eq!(p(0, 3, 0).turns(), 0);
    }

    #[test]
    fn lines_join_both_ends() {
        for &s in &STEPS {
            for half in [false, true] {
                let v = step_vector(s, half);
                let pts = line_points(v);
                assert_eq!(pts.first(), Some(&(0, 0)));
                assert_eq!(pts.last(), Some(&v));
                assert_eq!(pts.len() as i32, v.0.abs().max(v.1.abs()) + 1);
            }
        }
    }

    #[test]
    fn paths_round_trip_through_trace_lines() {
        let p = GotoPath { unit: 31, to: (85, 33), tiles: vec![(85, 33), (83, 33), (81, 33)], length: 6, max_moves: 21, moves_used: 3 };
        let line = p.trace_line();
        let fields: Vec<i64> = line.split_whitespace().skip(1).map(|f| f.parse().unwrap()).collect();
        assert_eq!(GotoPath::parse(&fields), Some(p));
    }

    #[test]
    fn range_outlines_stay_on_the_map() {
        struct NoFiles;
        impl render_desc::ImageLoader for NoFiles {
            fn load(&self, source: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
                Err(format!("no file {source}"))
            }
        }
        let desc = format!("grid = {{ layout = \"staggered\", step = [64, 32] }}\n{}", lines_description());
        let compiled = render_desc::Description::parse(&desc).unwrap().compile(&NoFiles, 4096).unwrap();
        let tiles = vec![crate::world::Civ3Tile::default(); 50 * 40];
        let mut world = Civ3World { width: 100, height: 40, wrap_x: true, tiles, ..Default::default() };
        let at = |world: &Civ3World| {
            let mut v: Vec<(i32, i32)> = range_sprites(world, &compiled, false).iter().map(|s| (s.x, s.y)).collect();
            v.sort_unstable();
            v
        };
        world.targeting.range_grid = Some(civ3_sys::world::RangeGrid { centre: (50, 20), range: 1 });
        let one = at(&world);
        assert_eq!(one.len(), 9);
        assert!(one.contains(&(64 * 50, 32 * 20)) && one.contains(&(64 * 52, 32 * 20)) && one.contains(&(64 * 49, 32 * 19)));
        // Across the seam of the map that wraps east-west.
        world.targeting.range_grid = Some(civ3_sys::world::RangeGrid { centre: (0, 20), range: 1 });
        assert_eq!(at(&world).len(), 9);
        // A range far beyond the map takes every tile once.
        world.targeting.range_grid = Some(civ3_sys::world::RangeGrid { centre: (50, 20), range: i32::MAX / 2 });
        let all = at(&world);
        assert_eq!(all.len(), 50 * 40);
        assert!(all.windows(2).all(|w| w[0] != w[1]));
    }

    #[test]
    fn steps_across_the_seam_are_single_steps() {
        let world = Civ3World { width: 100, height: 50, wrap_x: true, ..Default::default() };
        assert_eq!(step_between((99, 11), (1, 11), &world), Some((2, 0)));
        assert_eq!(step_between((0, 10), (99, 11), &world), Some((-1, 1)));
        assert_eq!(step_between((10, 10), (14, 10), &world), None);
    }
}
