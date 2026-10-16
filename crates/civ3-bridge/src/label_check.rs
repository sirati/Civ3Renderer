//! Checks [`crate::city_labels`] against the game: what the labels of a
//! tracer dump's cities are made to draw, against what the game drew (the map
//! canvas operations the interface mirror recorded, docs §16).
//!
//! The mirror records the game's calls on its map image and shows the survivors of its
//! own rules: fills are cut to the image, an opaque fill hides what lies
//! entirely within it, text starts inside the image or is gone, images are cut
//! to it. The labels are made to draw in the world and moved by the camera;
//! the same rules are applied to them before the two lists are compared,
//! operation by operation.
//!
//! Dumps from before labels were read (no city names) are first completed from
//! the mirror's own text for each city — its name, adjective, production,
//! the turns, outlines and icons — which leaves the geometry, the font
//! metrics, colours, clipping and ordering to be checked, but not the text
//! itself.

use std::path::PathBuf;

use civ3_assets::GameFiles;

use crate::city_labels::{self, Env, FileEnv, LabelOp};
use crate::text::TextRun;
use crate::ui::{ScreenDraw, line_rects};
use crate::ui_screen::relative_path;
use crate::world::{Civ3City, Trace};

/// One operation, as the mirror lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Fill { rect: [i32; 4], rgb: [u8; 3], alpha: u8 },
    Text(TextRun),
    /// The art file (relative, lower case), the region and where it lands.
    Image { path: String, rect: [i32; 4], at: (i32, i32) },
}

impl Op {
    /// Where it draws (left, top, right, bottom): text by its start point.
    fn bounds(&self) -> [i32; 4] {
        match self {
            Op::Fill { rect: [x, y, w, h], .. } => [*x, *y, x + w, y + h],
            Op::Text(t) => [t.x, t.y, t.x + 1, t.y + 1],
            Op::Image { rect, at, .. } => [at.0, at.1, at.0 + rect[2], at.1 + rect[3]],
        }
    }
}

/// The outcome for one dump.
#[derive(Debug, Default)]
pub struct Report {
    /// Cities the game drew a label for (explored, in its view) and the
    /// number of them whose label came out exactly as the game's.
    pub cities: usize,
    pub exact: usize,
    /// The operations expected, and the ones of them the game's list has, in
    /// order; the game's operations of the map canvas nobody expected.
    pub expected: usize,
    pub matched: usize,
    pub unexplained: usize,
    /// Cities of an old dump whose label is cut by the image so that its texts
    /// are gone, which leaves nothing to take them from: not checked.
    pub unchecked: usize,
    /// Of the game's operations nobody expected, those that lie where a
    /// label of a city of the world would be (its box, its icons), whether the
    /// city is checked or not: the rest of a label the image cuts.
    pub in_label_places: usize,
    /// Whether the cities' texts come from the mirror (an old dump).
    pub inferred: bool,
    /// What differs: one line each.
    pub notes: Vec<String>,
}

fn intersect(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]
}

/// The game's operations on its map image, from the mirror's lists (the
/// underlays of its frame).
fn actual(trace: &Trace, roots: &[PathBuf]) -> Vec<Op> {
    let Some(ui) = &trace.ui else { return Vec::new() };
    ui.underlay_draws
        .iter()
        .flat_map(|r| ui.draws.get(r.clone()).unwrap_or_default())
        .filter_map(|d| match d {
            ScreenDraw::Fill { rect, rgb, alpha, .. } => Some(Op::Fill { rect: *rect, rgb: *rgb, alpha: *alpha }),
            ScreenDraw::Text { run, .. } => Some(Op::Text(run.clone())),
            ScreenDraw::Image { path, rect, x, y, .. } => {
                Some(Op::Image { path: relative_path(path, roots).to_ascii_lowercase(), rect: *rect, at: (*x, *y) })
            }
            ScreenDraw::Flc { .. } | ScreenDraw::Surface { .. } => None,
        })
        .collect()
}

/// The size of the map image: the form of the map canvas (`0x9F8934`, 1920×1080).
fn image_size(trace: &Trace) -> (i32, i32) {
    let clip = trace.ui.as_ref().and_then(|ui| ui.draws.first()).and_then(|d| match d {
        ScreenDraw::Fill { clip, .. } | ScreenDraw::Image { clip, .. } | ScreenDraw::Text { clip, .. } => Some(*clip),
        _ => None,
    });
    clip.map_or((1920, 1080), |c| (c[2], c[3]))
}

/// The operations the game keeps of `ops` drawn on its map image in this
/// order, as the mirror shows them (see the module's text). Each carries its
/// tag.
fn survivors<T: Clone>(ops: Vec<(T, Op)>, (w, h): (i32, i32)) -> Vec<(T, Op)> {
    let window = [0, 0, w, h];
    let mut out: Vec<(T, Op)> = Vec::new();
    for (tag, op) in ops {
        let op = match op {
            Op::Fill { rect: [x, y, fw, fh], rgb, alpha } => {
                let [l, t, r, b] = intersect(window, [x, y, x + fw, y + fh]);
                if r <= l || b <= t {
                    continue;
                }
                Op::Fill { rect: [l, t, r - l, b - t], rgb, alpha }
            }
            Op::Image { path, rect: [rx, ry, rw, rh], at } => {
                let [l, t, r, b] = intersect(window, [at.0, at.1, at.0 + rw, at.1 + rh]);
                if r <= l || b <= t {
                    continue;
                }
                Op::Image { path, rect: [rx + (l - at.0), ry + (t - at.1), r - l, b - t], at: (l, t) }
            }
            text => text,
        };
        // An opaque fill hides what lies entirely within it.
        if let Op::Fill { rect: [x, y, fw, fh], alpha: 255, .. } = &op {
            let (x, y, fw, fh) = (*x, *y, *fw, *fh);
            out.retain(|(_, o)| {
                let [l, t, r, b] = o.bounds();
                !(l >= x && t >= y && r <= x + fw && b <= y + fh)
            });
        }
        out.push((tag, op));
    }
    // The copy onto the screen keeps text that starts inside.
    out.retain(|(_, o)| match o {
        Op::Text(t) => t.x >= 0 && t.x < w && t.y >= 0 && t.y < h,
        _ => true,
    });
    out
}

/// The operations the labels of `trace`'s world give, at screen positions:
/// each label moved to where the game puts its city's tile on the screen
/// (`city_labels::tile_to_screen`: the same place `camera` makes, and across the
/// seam of a wrapping map).
fn expected(trace: &Trace, files: &GameFiles, env: &mut dyn Env) -> Vec<(i32, Op)> {
    let world = &trace.world;
    let (half_w, half_h) = if trace.view.zoomed_out { (32, 16) } else { (64, 32) };
    let mut out = Vec::new();
    for label in city_labels::labels(world, trace.view.zoomed_out, env) {
        let Some(city) = world.city(label.city) else { continue };
        if !city_labels::in_game_view(world, &trace.view, city) {
            continue;
        }
        let (sx, sy) = city_labels::tile_to_screen(world, &trace.view, city.x, city.y);
        let (ox, oy) = (sx - half_w * city.x, sy - half_h * city.y);
        for item in &label.items {
            match item {
                LabelOp::Line { from, to, rgb, width } => {
                    for rect in line_rects((from.0 + ox, from.1 + oy), (to.0 + ox, to.1 + oy), *width) {
                        out.push((label.city, Op::Fill { rect, rgb: *rgb, alpha: 255 }));
                    }
                }
                LabelOp::Fill { rect: [x, y, w, h], rgb, alpha } => out.push((label.city, Op::Fill { rect: [x + ox, y + oy, *w, *h], rgb: *rgb, alpha: *alpha })),
                LabelOp::Text(run) => out.push((label.city, Op::Text(TextRun { x: run.x + ox, y: run.y + oy, ..run.clone() }))),
                LabelOp::Image { path, rect, at } => {
                    // The game keeps what is not transparent of the cell, at its offset.
                    let image = files.read(path).ok().and_then(|b| civ3_assets::pcx::decode(&b).ok());
                    let Some(kept) = image.and_then(|i| city_labels::trimmed(&i, *rect)) else { continue };
                    let at = (at.0 + ox + kept[0] - rect[0], at.1 + oy + kept[1] - rect[1]);
                    out.push((label.city, Op::Image { path: path.to_ascii_lowercase(), rect: kept, at }));
                }
            }
        }
    }
    out
}

/// The index of the name plate of each label in `ops`: the game's black
/// fill that darkens the map by half under the name.
fn plates(ops: &[Op]) -> Vec<usize> {
    (0..ops.len()).filter(|&i| matches!(&ops[i], Op::Fill { rect: [_, _, _, 10], rgb: [0, 0, 0], alpha: 127 })).collect()
}

/// Completes the cities of an old dump with what their labels show in the
/// mirror's operations `ops`: the texts, the outlines and the icons. Returns
/// the ids of the cities whose label is in view but cannot be read (cut by the
/// image so that no name is left).
fn infer(trace: &mut Trace, ops: &[Op]) -> Vec<i32> {
    let (tile_w, tile_h) = if trace.view.zoomed_out { (64, 32) } else { (128, 64) };
    let player = trace.world.player;
    let colour_blind = trace.world.preferences & 0x20_0000 != 0;
    let plates = plates(ops);
    let mut adjectives: Vec<(usize, String)> = Vec::new();
    let mut unreadable = Vec::new();
    let world_view = trace.view;
    let explored = |world: &crate::world::Civ3World, city: &Civ3City| {
        world.tile(city.x, city.y).is_some_and(|t| t.explored & (1u32 << (world.player & 31)) != 0)
    };
    for k in 0..trace.world.cities.len() {
        let city = trace.world.cities[k].clone();
        if !explored(&trace.world, &city) || !city_labels::in_game_view(&trace.world, &world_view, &city) {
            continue;
        }
        let (sx, sy) = city_labels::tile_to_screen(&trace.world, &world_view, city.x, city.y);
        let (centre, top) = (sx + tile_w / 2, sy + 10 + (tile_h / 4) * 7 / 2);
        // The label's plate: on the frame's row, around the city's centre.
        let found = plates.iter().position(|&i| matches!(&ops[i], Op::Fill { rect: [x, y, w, _], .. } if *y == top && *x <= centre && centre <= x + w));
        let Some(p) = found else {
            unreadable.push(city.id);
            continue;
        };
        let start = plates[p];
        // Up to the next label's frame (its lines come before its plate).
        let mut end = plates.get(p + 1).copied().unwrap_or(ops.len());
        while end > start + 1 && matches!(&ops[end - 1], Op::Fill { rgb: [0, 0, 0], alpha: 255, .. }) {
            end -= 1;
        }
        let group = &ops[start..end];
        let texts: Vec<&TextRun> = group.iter().filter_map(|o| if let Op::Text(t) = o { Some(t) } else { None }).collect();
        let own = city.civ == player;
        // "Name (Adjective) :  turns"; "--" for never.
        let split_turns = |s: &str| -> (String, Option<i32>) {
            match s.rsplit_once(" :  ") {
                Some((head, t)) => (head.to_owned(), Some(t.parse().unwrap_or(9999))),
                None => (s.to_owned(), None),
            }
        };
        let Some(name_text) = texts.iter().find(|t| t.height == -10 && t.y == top + 8) else {
            unreadable.push(city.id);
            continue;
        };
        let city = &mut trace.world.cities[k];
        let (mut name, grow) = if own { split_turns(&name_text.text) } else { (name_text.text.clone(), None) };
        if colour_blind {
            if let Some((n, adj)) = name.strip_suffix(')').and_then(|h| h.rsplit_once(" (")) {
                adjectives.push((city.civ as usize, adj.to_owned()));
                name = n.to_owned();
            }
        }
        city.name = name;
        city.grow_turns = grow;
        if let Some(t) = texts.iter().find(|t| t.height == -10 && t.y == top + 19) {
            let (item, turns) = split_turns(&t.text);
            city.order = (1, 0);
            city.order_name = Some(item);
            city.build_turns = turns;
        }
        // The outlines: a white line over the size box's top (a frame's row above the plate)
        // and over the production plate.
        let white = |y: i32| group.iter().any(|o| matches!(o, Op::Fill { rect: [_, ry, _, 1], rgb: [255, 255, 255], alpha: 255 } if *ry == y));
        city.policy = (white(top - 1) as i32) | ((white(top + 10) as i32) << 4);
        if let Some(t) = texts.iter().rev().find(|t| t.height == -16 && t.weight == 0) {
            city.food_income = match t.color {
                [248, 0, 0] => -1,
                [248, 248, 0] => 0,
                _ => 1,
            };
        }
        // The harbour, airport and barracks: their images at the places of this city's tile (a
        // city whose label is off the image may leave its icons right after this one's).
        let zoomed = trace.view.zoomed_out as i32;
        let icon_y = sy + 47 - 32 * zoomed;
        let icon = |file: &str, cell: i32, (px, py): (i32, i32)| {
            group.iter().any(|o| {
                matches!(o, Op::Image { path, rect, at } if path.ends_with(file)
                    && rect[0] - (cell + 1) == at.0 - (px + 1) && rect[1] - 2 == at.1 - (py + 1)
                    && at.0 >= px + 1 && at.1 >= py + 1 && at.0 + rect[2] <= px + 24 && at.1 + rect[3] <= py + 16)
            })
        };
        city.veteran_icons = Some(
            (icon("barracks.pcx", 1, (sx + 0x67 + zoomed * (23 - 64), icon_y - 18)) as u8 * civ3_sys::city_label::VETERAN_GROUND)
                | (icon("airandharb.pcx", 27, (sx - zoomed * 23, icon_y)) as u8 * civ3_sys::city_label::VETERAN_SEA)
                | (icon("airandharb.pcx", 1, (sx + zoomed * 23 + tile_w - 25, icon_y)) as u8 * civ3_sys::city_label::VETERAN_AIR),
        );
        city.hud_icons = Some(
            group
                .iter()
                .filter_map(|o| match o {
                    Op::Image { path, rect, .. } if path.ends_with("city icons.pcx") => Some(if rect[0] < 20 { 0 } else if rect[0] < 39 { 1 } else { 2 }),
                    _ => None,
                })
                .collect(),
        );
    }
    let world = &mut trace.world;
    for (civ, adj) in adjectives {
        if world.civ_adjectives.len() <= civ {
            world.civ_adjectives.resize(civ + 1, String::new());
        }
        world.civ_adjectives[civ] = adj;
    }
    unreadable
}

/// The length of the longest common subsequence's table and the pairs of
/// indices of one longest common subsequence of `a` and `b`.
fn common<A: PartialEq<B>, B>(a: &[A], b: &[B]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut table = vec![0u32; (n + 1) * (m + 1)];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i * (m + 1) + j] = if a[i] == b[j] { 1 + table[(i + 1) * (m + 1) + j + 1] } else { table[(i + 1) * (m + 1) + j].max(table[i * (m + 1) + j + 1]) };
        }
    }
    let (mut i, mut j, mut pairs) = (0, 0, Vec::new());
    while i < n && j < m {
        if a[i] == b[j] {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if table[(i + 1) * (m + 1) + j] >= table[i * (m + 1) + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

/// Compares the labels of `trace`'s cities with what the game drew.
/// `infer_texts`: complete the cities' texts from the game's operations (a
/// dump from before the labels were read).
pub fn check(trace: &mut Trace, files: &GameFiles, infer_texts: bool) -> Report {
    let roots = files.roots().to_vec();
    let size = image_size(trace);
    let theirs = actual(trace, &roots);
    let mut report = Report::default();
    if theirs.is_empty() {
        report.notes.push("skipped: the mirror has no operations on the map (a screen covers it)".to_owned());
        return report;
    }
    let mut unreadable = Vec::new();
    if infer_texts && trace.world.cities.iter().all(|c| c.grow_turns.is_none() && c.name.is_empty()) {
        unreadable = infer(trace, &theirs);
        report.inferred = true;
    }
    report.unchecked = unreadable.len();
    let mut env = FileEnv::new(files);
    let mut ours = expected(trace, files, &mut env);
    ours.retain(|(c, _)| !unreadable.contains(c));
    let ours = survivors(ours, size);
    let their_ops: Vec<(usize, Op)> = survivors(theirs.iter().cloned().enumerate().collect(), size);
    // Cities with a label, in the game's order.
    let cities: Vec<i32> = {
        let mut seen = Vec::new();
        for (c, _) in &ours {
            if !seen.contains(c) {
                seen.push(*c);
            }
        }
        seen
    };
    report.cities = cities.len();
    report.expected = ours.len();
    let ours_ops: Vec<&Op> = ours.iter().map(|(_, o)| o).collect();
    let theirs_only: Vec<&Op> = their_ops.iter().map(|(_, o)| o).collect();
    let pairs = common(&ours_ops, &theirs_only);
    report.matched = pairs.len();
    let matched_ours: std::collections::HashSet<usize> = pairs.iter().map(|p| p.0).collect();
    let matched_theirs: std::collections::HashSet<usize> = pairs.iter().map(|p| p.1).collect();
    for city in &cities {
        let missing: Vec<&Op> = ours.iter().enumerate().filter(|(i, (c, _))| c == city && !matched_ours.contains(i)).map(|(_, (_, o))| o).collect();
        if missing.is_empty() {
            report.exact += 1;
        } else {
            let name = trace.world.city(*city).map_or("?", |c| c.name.as_str());
            report.notes.push(format!("city {city} {name}: {} operation(s) the game does not have, first {:?}", missing.len(), missing[0]));
        }
    }
    let extra: Vec<&Op> = theirs_only.iter().enumerate().filter(|(j, _)| !matched_theirs.contains(j)).map(|(_, o)| *o).collect();
    // The map canvas of a game without cities (or before its first label) has other things on it.
    report.unexplained = if cities.is_empty() { 0 } else { extra.len() };
    let (tile_w, tile_h) = if trace.view.zoomed_out { (64, 32) } else { (128, 64) };
    let places: Vec<[i32; 4]> = trace
        .world
        .cities
        .iter()
        .flat_map(|c| {
            let (sx, sy) = city_labels::tile_to_screen(&trace.world, &trace.view, c.x, c.y);
            let (centre, top) = (sx + tile_w / 2, sy + 10 + (tile_h / 4) * 7 / 2);
            // The label's box (its plate can be 300 wide at most), and the icons around the tile's foot.
            [[centre - 160, top - 2, centre + 160, top + 24], [sx - 30, sy + 14, sx + tile_w + 30, sy + 70]]
        })
        .collect();
    let at_a_label = |op: &Op| {
        let b = op.bounds();
        places.iter().any(|p| b[0] >= p[0] && b[1] >= p[1] && b[2] <= p[2] && b[3] <= p[3])
    };
    report.in_label_places = extra.iter().filter(|op| at_a_label(op)).count().min(report.unexplained);
    // What the game drew that is not at a label first.
    let mut elsewhere: Vec<&&Op> = extra.iter().filter(|op| !at_a_label(op)).collect();
    elsewhere.truncate(4);
    for op in elsewhere.into_iter().chain(extra.iter().filter(|op| at_a_label(op)).take(4)).filter(|_| !cities.is_empty()) {
        report.notes.push(format!("game only{}: {op:?}", if at_a_label(op) { " (at a label)" } else { "" }));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_fills_hide_what_lies_within_and_the_image_cuts() {
        let text = |x, y| Op::Text(TextRun { x, y, color: [0; 3], face: "f".into(), height: -10, weight: 0, italic: false, underline: false, text: "t".into() });
        let ops = vec![
            (1, text(5, 5)),
            (1, Op::Fill { rect: [4, 4, 10, 10], rgb: [1, 2, 3], alpha: 128 }),
            (2, Op::Fill { rect: [0, 5, 20, 1], rgb: [0; 3], alpha: 255 }),
            (2, Op::Fill { rect: [-5, 0, 10, 2], rgb: [9, 9, 9], alpha: 255 }),
            (2, text(-1, 3)),
            (2, text(7, 7)),
        ];
        let kept = survivors(ops, (100, 100));
        // The text at (5, 5) lies in the line's row; the translucent plate does not (it is taller).
        assert_eq!(kept.len(), 4);
        assert_eq!(kept[1].1, Op::Fill { rect: [0, 5, 20, 1], rgb: [0; 3], alpha: 255 });
        // Cut to the image, and the text left of it is gone.
        assert_eq!(kept[2].1, Op::Fill { rect: [0, 0, 5, 2], rgb: [9, 9, 9], alpha: 255 });
        assert_eq!(kept[3].1, text(7, 7));
    }

    #[test]
    fn the_common_subsequence_keeps_order() {
        let a = [1, 2, 3, 4, 5];
        let b = [2, 9, 4, 5, 7];
        assert_eq!(common(&a, &b), vec![(1, 0), (3, 2), (4, 3)]);
    }
}

