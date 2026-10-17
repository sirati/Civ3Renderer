//! The mirrored UI (`ui`) as our renderer draws it: image regions and fills
//! through the menu crate's run-time screens, text through the game's
//! fonts, in the game's composition order.

use std::path::PathBuf;

use menu::screen::{Element, Region, ScreenArt};
use render_desc::Compiled;

use crate::text::{Fonts, TextRun};
use crate::ui::{Frame, ScreenDraw};

/// Loader options of colour-keyed regions (palette index 255 shows what is
/// below) and of opaque ones.
const KEYED: &str = "transparent = [254, 255]";
const OPAQUE: &str = "transparent = []";

/// One thing to draw, in order; `true`: part of an underlay (drawn under
/// the fog of war, see [`sprites`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Element(Element, bool),
    Text(TextRun, [i32; 4], bool),
}

/// A path the game opened, relative to the art root it lies in (the
/// loader searches the same roots), else as it is.
pub fn relative_path(path: &str, roots: &[PathBuf]) -> String {
    let norm = |s: &str| s.replace('/', "\\").to_ascii_lowercase();
    let p = norm(path);
    for root in roots {
        let mut r = norm(&root.to_string_lossy());
        if !r.ends_with('\\') {
            r.push('\\');
        }
        if p.starts_with(&r) {
            return path[r.len()..].replace('\\', "/");
        }
    }
    // Under no known root (e.g. a trace read elsewhere): from its `Art`
    // folder on, which the art search resolves the same way.
    match p.find("\\art\\") {
        Some(i) => path[i + 1..].replace('\\', "/"),
        None => path.replace('\\', "/"),
    }
}

/// The frame as items. `generated`: names of the surfaces the game fills
/// itself that we make ourselves (others are left out); `unit_colors`: the
/// colour table of unit pictures.
pub fn items(frame: &Frame, roots: &[PathBuf], generated: &[(u32, &str)], unit_colors: i32) -> Vec<Item> {
    items_with(frame, roots, generated, unit_colors, None)
}

/// [`items`] with `extra` (index, items) among them: over the draws before
/// that index, under the others.
pub fn items_with(frame: &Frame, roots: &[PathBuf], generated: &[(u32, &str)], unit_colors: i32, extra: Option<(usize, Vec<Item>)>) -> Vec<Item> {
    let mut out = Vec::new();
    let (at, mut extra) = extra.map_or((usize::MAX, None), |(at, items)| (at, Some(items)));
    for (i, d) in frame.draws.iter().enumerate() {
        if i == at {
            out.extend(extra.take().unwrap_or_default());
        }
        let under = frame.underlay_draws.iter().any(|r| r.contains(&i));
        let element = match d {
            ScreenDraw::Image { path, rect, x, y, keyed, clip, mask, palette, opacity } => {
                let region = Region {
                    source: relative_path(path, roots),
                    rect: *rect,
                    keyed: *keyed,
                    mask: mask.as_ref().map(|m| (relative_path(&m.path, roots), (m.at.0 - rect[0], m.at.1 - rect[1]))),
                    palette: palette.clone(),
                };
                if *opacity == 255 {
                    Element::Image { region, at: (*x, *y), clip: *clip }
                } else {
                    Element::OpacityImage { region, at: (*x, *y), clip: *clip, opacity: *opacity }
                }
            },
            ScreenDraw::Fill { rect, rgb, alpha, clip } => Element::Fill { rect: *rect, rgb: *rgb, alpha: *alpha, clip: *clip },
            ScreenDraw::Text { run, clip } => {
                out.push(Item::Text(run.clone(), *clip, under));
                continue;
            }
            // Unit pictures, from our unit art in the viewing player's
            // colours (§16.9), over their shadow as on the map.
            // Leader heads: in their own colours.
            ScreenDraw::Flc { path, chunk, x, y, clip, plain: true } => {
                Element::Sprite { name: format!("Flc@{}/{chunk}", relative_path(path, roots)), at: (*x, *y), clip: *clip, scale_den: 0, palette: String::new() }
            }
            ScreenDraw::Flc { path, chunk, x, y, clip, plain: false } => {
                let path = relative_path(path, roots);
                let shadow = format!("UnitShadow@{path}/{chunk}");
                out.push(Item::Element(Element::Sprite { name: shadow, at: (*x, *y), clip: *clip, scale_den: 16, palette: String::new() }, under));
                let palette = crate::units::Recolor::civ(unit_colors).palette();
                Element::Sprite { name: format!("UnitArt@{path}/{chunk}"), at: (*x, *y), clip: *clip, scale_den: 0, palette }
            }
            ScreenDraw::Surface { image, rect, x, y, clip, size } => {
                let Some(&(_, name)) = generated.iter().find(|g| g.0 == *image) else { continue };
                let size = (size.0.max(0) as u32, size.1.max(0) as u32);
                Element::Dynamic { name: name.to_owned(), size, rect: *rect, at: (*x, *y), clip: *clip }
            }
        };
        out.push(Item::Element(element, under));
    }
    out.extend(extra.unwrap_or_default());
    out
}

/// The unit animation files `frame` shows in panels (relative paths);
/// `plain`: those in their own colours (leader heads) instead.
pub fn unit_files(frame: &Frame, roots: &[PathBuf], plain: bool) -> Vec<String> {
    let mut out: Vec<String> = frame
        .draws
        .iter()
        .filter_map(|d| match d {
            ScreenDraw::Flc { path, plain: p, .. } if *p == plain => Some(relative_path(path, roots)),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// What an opaque screen starting at draw `first` leaves of `frame`: the
/// underlays (map labels; they go with the map, which is shown in its own
/// view) and the draws from `first` on.
pub fn from_draw(frame: &Frame, first: usize) -> Frame {
    let under: Vec<usize> = frame.underlay_draws.iter().flat_map(|r| r.clone()).filter(|&i| i < first).collect();
    let mut draws: Vec<_> = under.iter().map(|&i| frame.draws[i].clone()).collect();
    let n = draws.len();
    draws.extend(frame.draws[first.min(frame.draws.len())..].iter().cloned());
    let mut underlay_draws = vec![0..n];
    underlay_draws.extend(frame.underlay_draws.iter().filter(|r| r.start >= first).map(|r| r.start - first + n..r.end - first + n));
    Frame { draws, forms: frame.forms.clone(), form_draws: Vec::new(), underlay_draws }
}

/// Where the menus start in `frame`'s draws, over the game's main screen
/// (`frame.draws.len()`: no menu). The main screen is composed first: its
/// forms filling the screen (the map, the frame around it), then its
/// buttons (small forms); the first other form starts the menus (an
/// advisor, a popup, a full-screen menu). A frame cut by [`from_draw`]
/// (an opaque screen) is all menu but its underlays.
pub fn menus_from(frame: &Frame) -> usize {
    if frame.form_draws.is_empty() {
        return frame.underlay_draws.first().filter(|r| r.start == 0).map_or(0, |r| r.end).min(frame.draws.len());
    }
    // A button: no larger than this each way.
    const BUTTON: i32 = 64;
    let screen = frame.forms.first().map(|f| (f.2, f.3));
    let mut buttons = false;
    let first_menu = frame.forms.iter().position(|&(_, _, origin, size)| {
        let full = !buttons && Some((origin, size)) == screen;
        let button = size.0 <= BUTTON && size.1 <= BUTTON;
        buttons |= button;
        !full && !button
    });
    let Some(first_menu) = first_menu else { return frame.draws.len() };
    let canvases: Vec<u32> = frame.forms[first_menu..].iter().map(|f| f.1).collect();
    frame
        .form_draws
        .iter()
        .filter(|(canvas, _)| canvases.contains(canvas))
        .map(|(_, r)| r.start)
        .min()
        .unwrap_or(frame.draws.len())
        .min(frame.draws.len())
}

/// `frame` cut at draw `at`: the draws before (with the underlays among
/// them), and those from `at` on.
pub fn split(frame: &Frame, at: usize) -> (Frame, Frame) {
    let at = at.min(frame.draws.len());
    let part = |range: std::ops::Range<usize>| {
        let clip = |r: &std::ops::Range<usize>| r.start.max(range.start)..r.end.min(range.end);
        Frame {
            draws: frame.draws[range.clone()].to_vec(),
            forms: frame.forms.clone(),
            form_draws: frame.form_draws.iter().map(|(c, r)| (*c, clip(r))).filter(|(_, r)| !r.is_empty()).map(|(c, r)| (c, r.start - range.start..r.end - range.start)).collect(),
            underlay_draws: frame.underlay_draws.iter().map(clip).filter(|r| !r.is_empty()).map(|r| r.start - range.start..r.end - range.start).collect(),
        }
    };
    (part(0..at), part(at..frame.draws.len()))
}

/// The art and fonts `items` need; whether anything was new.
pub fn add_art(items: &[Item], art: &mut ScreenArt, fonts: &mut Fonts, files: &civ3_assets::GameFiles) -> bool {
    let elements: Vec<Element> = items.iter().filter_map(|i| if let Item::Element(e, _) = i { Some(e.clone()) } else { None }).collect();
    let runs: Vec<TextRun> = items.iter().filter_map(|i| if let Item::Text(r, _, _) = i { Some(r.clone()) } else { None }).collect();
    let new_art = art.add(&elements);
    let new_fonts = fonts.add(files, &runs);
    new_art || new_fonts
}

/// The description entries of `art`.
pub fn description(art: &ScreenArt) -> String {
    art.description(KEYED, OPAQUE, |mask, (dx, dy)| {
        format!("alpha_image = \"{}\"\nalpha_offset = [{dx}, {dy}]", mask.replace('\\', "\\\\").replace('"', "\\\""))
    })
}

/// The zooms the scene has zoomed art for (`scene::Scene::zooms`): every
/// zoom, (0, 0), where text can be drawn from outlines (the art has the
/// game's face), else none.
pub fn outline_zooms(compiled: &Compiled) -> Vec<(u32, u32)> {
    if compiled.fonts.is_empty() { Vec::new() } else { vec![(0, 0)] }
}

/// Whether a screen is complete, still building, or has settled with art
/// that cannot be rendered. The last case is a terminal diagnostic/fallback,
/// never a timeout releasing a valid screen before its art is available.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    Pending,
    Unavailable,
}

/// Resolves currently visible mandatory items against adopted metadata.
/// Alternate control states, hints and world labels are not supplied here.
pub fn readiness<'a>(items: impl Iterator<Item = &'a Item>, fonts: &Fonts, compiled: &Compiled, building: bool) -> Readiness {
    let mut missing = false;
    let mut loading = false;
    for item in items {
        match item {
            Item::Element(element, _) => missing |= !element.ready(compiled),
            Item::Text(run, clip, _) if clip[0] < clip[2] && clip[1] < clip[3] => {
                if !fonts.run_ready(run, compiled) {
                    missing = true;
                    loading |= fonts.run_loading(run);
                }
            }
            Item::Text(..) => {}
        }
    }
    if !missing { Readiness::Ready } else if building || loading { Readiness::Pending } else { Readiness::Unavailable }
}

/// The free sprites of `items`, in order: underlay items after render pass
/// `under_pass` (the one before the fog of war, which the game draws over
/// them), the rest on top. With `outlines`, text comes 1:1 (`zoom_art` 1)
/// and drawn from its face's outlines (zoomed art for every zoom), for
/// views drawn at their zoom (the map's labels in zoomed views, the whole
/// interface in a scaled screen); text whose face has no outlines stays
/// 1:1 everywhere.
pub fn sprites(items: &[Item], fonts: &Fonts, compiled: &Compiled, under_pass: u32, outlines: bool) -> Vec<scene::FreeSprite> {
    let mut out = Vec::new();
    for i in items {
        let pass = |under: bool| if under { under_pass } else { u32::MAX };
        match i {
            Item::Element(e, under) => out.extend(menu::screen::sprites(std::slice::from_ref(e), compiled, pass(*under))),
            Item::Text(run, clip, under) => {
                let start = out.len();
                fonts.run_sprites(run, Some(*clip), compiled, pass(*under), &mut out);
                let low = out.len();
                if outlines && fonts.run_glyphs(run, Some(*clip), compiled, pass(*under), &mut out) {
                    for s in &mut out[start..low] {
                        s.zoom_art = 1;
                    }
                }
            }
        }
    }
    out
}

/// The render pass underlay items follow: the one before the fog of war
/// (the `fog` layer's pass), as an index into the passes.
pub fn under_pass(compiled: &Compiled) -> u32 {
    let mut passes: Vec<u32> = compiled.layers.iter().map(|l| l.pass).collect();
    passes.dedup();
    let fog = compiled.layers.iter().find(|l| l.name == "fog").map(|l| l.pass);
    match fog.and_then(|p| passes.iter().position(|&q| q == p)) {
        Some(i) if i > 0 => i as u32 - 1,
        _ => u32::MAX,
    }
}


/// Scene patches for the minimap pictures among `items`, made from
/// `world`. `table_color`: a colour table's entry 66 (8-bit RGB).
pub fn minimap_patches(
    items: &[Item],
    world: &crate::world::Civ3World,
    compiled: &Compiled,
    mut table_color: impl FnMut(i32) -> [u8; 3],
) -> Vec<scene::Patch> {
    let mut out = Vec::new();
    let mut done = Vec::new();
    for i in items {
        let Item::Element(Element::Dynamic { name, size, .. }, _) = i else { continue };
        if !name.starts_with("minimap") || done.contains(&(name.clone(), *size)) {
            continue;
        }
        done.push((name.clone(), *size));
        let colors: Vec<u16> = (0..32)
            .map(|civ| crate::minimap::from_rgb(table_color(world.color_tables.get(civ).copied().unwrap_or(0))))
            .collect();
        let rgba = crate::minimap::image(world, *size, |civ| colors.get(civ as usize).copied().unwrap_or(0));
        out.extend(menu::screen::patch(compiled, name, *size, rgba));
    }
    out
}

/// Patches for generated surfaces shown by `items`. The renderer supplies
/// RGBA pixels made from game state, including the replay map.
pub fn surface_patches<'a>(items: impl Iterator<Item = &'a Item> + Clone, surfaces: &[(&str, (u32, u32), Vec<u8>)], compiled: &Compiled) -> Vec<scene::Patch> {
    surfaces
        .iter()
        .filter(|(name, size, _)| items.clone().any(|i| matches!(i, Item::Element(Element::Dynamic { name: n, size: s, .. }, _) if n == name && s == size)))
        .filter_map(|(name, size, pixels)| menu::screen::patch(compiled, name, *size, pixels.clone()))
        .collect()
}

/// The open city screen (§16.8): where it shows the map — the transparent
/// band (palette index 255) of its background art, as the mirrored
/// interface places it — and the index of its first draw in `frame` (what
/// comes before is under it, hidden). Bands are measured once per file
/// (`bands`).
pub fn city_screen(
    frame: &Frame,
    files: &civ3_assets::GameFiles,
    roots: &[PathBuf],
    bands: &mut std::collections::HashMap<String, Option<[i32; 4]>>,
) -> Option<([i32; 4], usize)> {
    frame.draws.iter().enumerate().find_map(|(i, d)| {
        let ScreenDraw::Image { path, rect, x, y, .. } = d else { return None };
        let p = path.to_ascii_lowercase().replace('/', "\\");
        if !p.ends_with("city screen\\background.pcx") {
            return None;
        }
        let rel = relative_path(path, roots);
        let band = *bands.entry(rel.clone()).or_insert_with(|| transparent_band(files, &rel));
        let [bl, bt, br, bb] = band?;
        // The band within the drawn region, on the screen.
        let (l, t) = (bl.max(rect[0]), bt.max(rect[1]));
        let (r, b) = (br.min(rect[0] + rect[2]), bb.min(rect[1] + rect[3]));
        let first = frame.form_draws.iter().find(|(_, range)| range.contains(&i)).map_or(i, |(_, range)| range.start);
        (r > l && b > t).then(|| ([x + l - rect[0], y + t - rect[1], r - l, b - t], first))
    })
}

/// The bounding box (left, top, right, bottom; exclusive) of palette index
/// 255 in an image file.
fn transparent_band(files: &civ3_assets::GameFiles, path: &str) -> Option<[i32; 4]> {
    let img = civ3_assets::pcx::decode(&files.read(path).ok()?).ok()?;
    let (mut l, mut t, mut r, mut b) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for y in 0..img.height {
        for x in 0..img.width {
            if img.get(x, y) == 255 {
                let (x, y) = (x as i32, y as i32);
                (l, t, r, b) = (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1));
            }
        }
    }
    (r > l).then_some([l, t, r, b])
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_made_of_power_of_two_pieces() {
        assert_eq!(pieces(13), [(0, 8), (8, 4), (12, 1)]);
        assert_eq!(pieces(0), []);
    }

    #[test]
    fn extra_items_go_between_the_draws() {
        let fill = |x: i32| ScreenDraw::Fill { rect: [x, 0, 1, 1], rgb: [0, 0, 0], alpha: 255, clip: [0, 0, 10, 10] };
        let frame = Frame { draws: vec![fill(1), fill(2)], ..Default::default() };
        let extra = || vec![Item::Element(Element::Fill { rect: [9, 9, 1, 1], rgb: [1, 1, 1], alpha: 255, clip: [0, 0, 10, 10] }, false)];
        let xs = |items: Vec<Item>| -> Vec<i32> { items.iter().map(|i| if let Item::Element(Element::Fill { rect, .. }, _) = i { rect[0] } else { -1 }).collect() };
        assert_eq!(xs(items_with(&frame, &[], &[], 0, Some((1, extra())))), [1, 9, 2]);
        assert_eq!(xs(items_with(&frame, &[], &[], 0, Some((2, extra())))), [1, 2, 9]);
        assert_eq!(xs(items_with(&frame, &[], &[], 0, None)), [1, 2]);
    }

    #[test]
    fn paths_become_relative_to_their_root() {
        let roots = [PathBuf::from("C:\\Games\\Civ3\\Conquests"), PathBuf::from("C:\\Games\\Civ3")];
        assert_eq!(relative_path("C:\\Games\\Civ3\\Conquests\\Art\\Advisors\\a.pcx", &roots), "Art/Advisors/a.pcx");
        assert_eq!(relative_path("c:\\games\\civ3\\Art\\x.PCX", &roots), "Art/x.PCX");
        assert_eq!(relative_path("D:\\other\\y.pcx", &roots), "D:/other/y.pcx");
        assert_eq!(relative_path("D:\\x\\Conquests\\Art\\Civilopedia\\a.pcx", &[]), "Art/Civilopedia/a.pcx");
    }

    #[test]
    fn menus_start_at_the_first_form_beyond_the_main_screen() {
        // As traced at 1920x1080 with the domestic advisor open: the map,
        // the frame, its buttons, then the advisor (and its own buttons).
        let fill = |i| ScreenDraw::Fill { rect: [i, 0, 1, 1], rgb: [0; 3], alpha: 255, clip: [0; 4] };
        let full = (1920, 1080);
        let frame = Frame {
            draws: (0..10).map(fill).collect(),
            forms: vec![
                (0x66b3d8, 1, (0, 0), full),
                (0x66b570, 2, (0, 0), full),
                (0x6656c0, 3, (57, 12), (36, 30)),
                (0x6688b8, 4, (448, 156), (1024, 768)),
                (0x6656c0, 5, (1183, 267), (12, 12)),
            ],
            form_draws: vec![(1, 0..4), (2, 4..6), (3, 6..7), (4, 7..9), (5, 9..10)],
            underlay_draws: vec![0..4],
        };
        assert_eq!(menus_from(&frame), 7);
        let (main, menus) = split(&frame, 7);
        assert_eq!((main.draws.len(), menus.draws.len()), (7, 3));
        assert_eq!(main.underlay_draws, [0..4]);
        assert!(menus.underlay_draws.is_empty());
        assert_eq!(menus.form_draws, [(4, 0..2), (5, 2..3)]);
        // The main screen alone: no menus. A full-screen form after the
        // buttons is a menu (e.g. diplomacy).
        let main_only = Frame { forms: frame.forms[..3].to_vec(), form_draws: frame.form_draws[..3].to_vec(), ..frame.clone() };
        assert_eq!(menus_from(&main_only), 10);
        let mut over = main_only.clone();
        over.forms.push((0x123, 9, (0, 0), full));
        over.form_draws.push((9, 7..10));
        assert_eq!(menus_from(&over), 7);
        // An opaque screen (cut from its first draw): all menu but the
        // underlays carried along.
        assert_eq!(menus_from(&from_draw(&frame, 7)), 4);
    }
}

/// Where on screen the mirrored interface shows tile `tile` on the minimap
/// (to click there: the game centres its view on that tile).
pub fn minimap_point(items: &[Item], world: &crate::world::Civ3World, tile: (i32, i32)) -> Option<(i32, i32)> {
    items.iter().find_map(|i| {
        let Item::Element(Element::Dynamic { name, size, rect: [rx, ry, rw, rh], at: (x, y), clip }, _) = i else { return None };
        if name != "minimap" {
            return None;
        }
        let (px, py) = crate::minimap::pixel_of(world, *size, tile)?;
        let (px, py) = (px as i32, py as i32);
        let shown = px >= *rx && py >= *ry && px < rx + rw && py < ry + rh;
        let (sx, sy) = (x + px - rx, y + py - ry);
        let clipped = sx < clip[0] || sy < clip[1] || sx >= clip[2] || sy >= clip[3];
        (shown && !clipped).then_some((sx, sy))
    })
}

/// Marks `views` (world pixel rectangles x, y, w, h, the renderer's own
/// views) on the interface's minimap like the game marks its own view
/// (§17.7: their tile range as one-pixel lines), in `color`: fills right
/// after the minimap's picture (under its frame), made of power-of-two
/// pieces so their art stays the same as the views move.
pub fn mark_views(items: &mut Vec<Item>, world: &crate::world::Civ3World, views: &[(i32, i32, i32, i32)], zoomed_out: bool, color: [u8; 3]) {
    let Some(at) = items.iter().position(|i| matches!(i, Item::Element(Element::Dynamic { name, .. }, _) if name == "minimap")) else { return };
    let Item::Element(Element::Dynamic { size, rect: [rx, ry, _, _], at: (x, y), clip, .. }, under) = items[at].clone() else { return };
    // A tile's footprint starts at (64·x, 32·y) (half that zoomed out).
    let (sx, sy) = if zoomed_out { (32, 16) } else { (64, 32) };
    let mut fills = Vec::new();
    for &(vx, vy, vw, vh) in views.iter().filter(|v| v.2 > 0 && v.3 > 0) {
        let tiles = (vx.div_euclid(sx), vy.div_euclid(sy), (vx + vw - 1).div_euclid(sx), (vy + vh - 1).div_euclid(sy));
        for [lx, ly, lw, lh] in crate::minimap::view_rect(world, size, tiles) {
            let (px, py) = (x + lx - rx, y + ly - ry);
            for (offset, len) in pieces(lw.max(lh)) {
                let rect = if lw >= lh { [px + offset, py, len, 1] } else { [px, py + offset, 1, len] };
                fills.push(Item::Element(Element::Fill { rect, rgb: color, alpha: 255, clip }, under));
            }
        }
    }
    items.splice(at + 1..at + 1, fills);
}

/// The game's own view marked on the minimap among `items` (white lines
/// round tile range `game_tiles`, x0, y0, x1, y1) replaced by world
/// rectangle `shown` (x, y, w, h): what the renderer shows of the game's
/// view where that is not all of it (`scene::Input::MainView`).
pub fn replace_game_view_mark(items: &mut Vec<Item>, world: &crate::world::Civ3World, game_tiles: (i32, i32, i32, i32), shown: (i32, i32, i32, i32), zoomed_out: bool) {
    let Some(at) = items.iter().position(|i| matches!(i, Item::Element(Element::Dynamic { name, .. }, _) if name == "minimap")) else { return };
    let Item::Element(Element::Dynamic { size, rect: [rx, ry, _, _], at: (x, y), .. }, _) = items[at].clone() else { return };
    let lines: Vec<[i32; 4]> = crate::minimap::view_rect(world, size, game_tiles).into_iter().map(|[lx, ly, lw, lh]| [x + lx - rx, y + ly - ry, lw, lh]).collect();
    let before = items.len();
    items.retain(|i| !matches!(i, Item::Element(Element::Fill { rect, rgb: [255, 255, 255], alpha: 255, .. }, _) if lines.contains(rect)));
    if items.len() == before {
        // (Not drawn: nothing to replace.)
        return;
    }
    let at = items.iter().position(|i| matches!(i, Item::Element(Element::Dynamic { name, .. }, _) if name == "minimap")).unwrap_or(0);
    let mut marked = items[..=at].to_vec();
    mark_views(&mut marked, world, &[shown], zoomed_out, [255, 255, 255]);
    let added: Vec<Item> = marked.drain(at + 1..).collect();
    items.splice(at + 1..at + 1, added);
}

/// `len` as consecutive power-of-two pieces: (offset, length).
fn pieces(len: i32) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    let mut offset = 0;
    for bit in (0..31).rev() {
        if len & (1 << bit) != 0 {
            out.push((offset, 1 << bit));
            offset += 1 << bit;
        }
    }
    out
}

/// What the open city screen shows to be clicked that has no control of
/// its own (§16.8; the city form tests them in its code): its citizens'
/// heads (`popHeads.pcx`, clicked to change what a citizen does) and the
/// units in the city (`units_32.pcx`); none while another form is modal
/// over it (`modal_canvas`, a popup: only it takes the mouse). Their ids: the
/// city form's canvas above, the kind and place below.
pub fn city_drawn_targets(frame: &Frame, modal_canvas: Option<u32>) -> Vec<scene::Interactable> {
    let lower = |p: &str| p.to_ascii_lowercase().replace('/', "\\");
    let Some(background) = frame.draws.iter().position(|d| matches!(d, ScreenDraw::Image { path, .. } if lower(path).ends_with("city screen\\background.pcx"))) else { return vec![] };
    let Some((canvas, range)) = frame.form_draws.iter().find(|(_, r)| r.contains(&background)) else { return vec![] };
    if modal_canvas.is_some_and(|m| m != *canvas) {
        return vec![];
    }
    frame.draws[range.clone()]
        .iter()
        .filter_map(|d| match d {
            ScreenDraw::Image { path, rect, x, y, clip, .. } if lower(path).ends_with("popheads.pcx") || lower(path).ends_with("units_32.pcx") => {
                let tag = if lower(path).ends_with("popheads.pcx") { 0x0C00_0000 } else { 0x0D00_0000 };
                let [cl, ct, cr, cb] = *clip;
                let (l, t) = ((*x).max(cl), (*y).max(ct));
                let (r, b) = ((x + rect[2]).min(cr), (y + rect[3]).min(cb));
                (r > l && b > t).then(|| scene::Interactable {
                    rect: [l, t, r - l, b - t],
                    kind: scene::InteractableKind::Button,
                    id: (*canvas as u64) << 32 | tag | ((l as u32 & 0xFFF) << 12) as u64 | (t as u32 & 0xFFF) as u64,
                })
            }
            _ => None,
        })
        .collect()
}

/// The game's controls (`civ3_sys::controls`) as things to point at in the
/// viewer (button snapping): the game's client pixels are the driver's.
pub fn interactables(controls: &[civ3_sys::controls::Control]) -> Vec<scene::Interactable> {
    use civ3_sys::controls::ControlKind;
    use scene::InteractableKind;
    controls
        .iter()
        .map(|c| scene::Interactable {
            rect: c.rect,
            kind: match c.kind {
                ControlKind::Button => InteractableKind::Button,
                ControlKind::ListItem => InteractableKind::ListItem,
                ControlKind::List { scroll, step } => InteractableKind::List { scroll, step },
            },
            id: c.id,
        })
        .collect()
}

#[cfg(test)]
mod view_mark_tests {
    use super::*;

    #[test]
    fn the_games_view_mark_is_replaced_by_what_the_viewer_shows() {
        let world = crate::world::Civ3World { width: 40, height: 40, tiles: vec![Default::default(); 800], ..Default::default() };
        let clip = [0, 0, 1920, 1080];
        let minimap = Item::Element(Element::Dynamic { name: "minimap".into(), size: (200, 100), rect: [0, 0, 200, 100], at: (30, 960), clip }, false);
        let fills = |items: &[Item]| -> Vec<[i32; 4]> { items.iter().filter_map(|i| if let Item::Element(Element::Fill { rect, .. }, _) = i { Some(*rect) } else { None }).collect() };
        // The game's own mark: tiles 10..20 x 10..30.
        let game_tiles = (10, 10, 20, 30);
        let game_lines: Vec<[i32; 4]> = crate::minimap::view_rect(&world, (200, 100), game_tiles).into_iter().map(|[x, y, w, h]| [30 + x, 960 + y, w, h]).collect();
        let fill = |rect| Item::Element(Element::Fill { rect, rgb: [255, 255, 255], alpha: 255, clip }, false);
        let mut items = vec![minimap.clone()];
        items.extend(game_lines.iter().copied().map(fill));
        items.push(fill([5, 5, 3, 3]));
        // The viewer shows tiles 10..15 x 10..20 of it (a split's left half).
        replace_game_view_mark(&mut items, &world, game_tiles, (64 * 10, 32 * 10, 64 * 5, 32 * 10), false);
        let got = fills(&items);
        assert!(game_lines.iter().all(|l| !got.contains(l)), "{got:?}");
        assert!(got.contains(&[5, 5, 3, 3]));
        let mut want = vec![minimap];
        mark_views(&mut want, &world, &[(640, 320, 320, 320)], false, [255, 255, 255]);
        assert!(!fills(&want).is_empty() && fills(&want).iter().all(|w| got.contains(w)), "{got:?}");
        // Without the game's mark drawn, nothing is added.
        let mut bare = vec![items[0].clone()];
        replace_game_view_mark(&mut bare, &world, game_tiles, (0, 0, 64, 32), false);
        assert_eq!(bare.len(), 1);
    }
}

#[cfg(test)]
mod interactable_tests {
    use civ3_sys::controls::{Control, ControlKind};
    use scene::{Interactable, InteractableKind};

    #[test]
    fn the_city_screen_s_citizens_and_units_are_targets() {
        use crate::ui::{Frame, ScreenDraw};
        let image = |path: &str, rect: [i32; 4], x: i32, y: i32, clip: [i32; 4]| ScreenDraw::Image { path: path.into(), rect, x, y, keyed: true, clip, mask: None, palette: None, opacity: 255 };
        let all = [i32::MIN, i32::MIN, i32::MAX, i32::MAX];
        let frame = Frame {
            draws: vec![
                // An advisor's heads under it (another form): not the city's.
                image("Art/SmallHeads/popHeads.pcx", [0, 0, 25, 30], 5, 5, all),
                image("Art/City Screen/background.pcx", [0, 0, 1024, 768], 0, 0, all),
                image("C:/Games/Art/SmallHeads/POPHEADS.PCX", [50, 0, 25, 30], 100, 40, all),
                image("Art/SmallHeads/popHeads.pcx", [75, 0, 25, 30], 125, 40, [0, 0, 140, 768]),
                image("Art/City Screen/CityIcons.pcx", [0, 0, 16, 16], 300, 300, all),
                // A unit in the city.
                image("Art/Units/units_32.pcx", [33, 0, 32, 32], 760, 885, all),
            ],
            form_draws: vec![(0x111, 0..1), (0x740C34, 1..6)],
            ..Default::default()
        };
        let got = super::city_drawn_targets(&frame, Some(0x740C34));
        assert_eq!(got.iter().map(|i| i.rect).collect::<Vec<_>>(), [[100, 40, 25, 30], [125, 40, 15, 30], [760, 885, 32, 32]]);
        assert!(got.iter().all(|i| i.kind == InteractableKind::Button && i.id >> 32 == 0x740C34));
        assert_ne!(got[0].id, got[1].id);
        // Without the city screen: none.
        assert!(super::city_drawn_targets(&Frame { draws: frame.draws[..1].to_vec(), ..Default::default() }, None).is_empty());
        // A popup over it: only the popup takes the mouse.
        assert!(super::city_drawn_targets(&frame, Some(0x9B7784)).is_empty());
        assert_eq!(super::city_drawn_targets(&frame, None).len(), 3);
    }

    #[test]
    fn controls_become_interactables() {
        let controls = [
            Control { rect: [1, 2, 3, 4], kind: ControlKind::Button, id: 7 },
            Control { rect: [0, 0, 100, 200], kind: ControlKind::List { scroll: [0, 0, 100, 200], step: 20 }, id: 8 },
            Control { rect: [0, 0, 100, 20], kind: ControlKind::ListItem, id: 9 },
        ];
        assert_eq!(super::interactables(&controls), [
            Interactable { rect: [1, 2, 3, 4], kind: InteractableKind::Button, id: 7 },
            Interactable { rect: [0, 0, 100, 200], kind: InteractableKind::List { scroll: [0, 0, 100, 200], step: 20 }, id: 8 },
            Interactable { rect: [0, 0, 100, 20], kind: InteractableKind::ListItem, id: 9 },
        ]);
    }
}


#[cfg(test)]
mod readiness_tests {
    use super::*;

    struct Images;
    impl render_desc::ImageLoader for Images {
        fn load(&self, name: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
            if name == "missing.pcx" { return Err("source does not exist".into()) }
            Ok(vec![tile_render::RgbaImage::new(128, 128)])
        }
    }

    fn image(source: &str, size: i32) -> Item {
        Item::Element(Element::Image {
            region: Region { source: source.into(), rect: [0, 0, size, size], keyed: false, mask: None, palette: None },
            at: (0, 0), clip: [0, 0, 1920, 1080],
        }, false)
    }

    fn art(items: &[Item]) -> String {
        let mut art = ScreenArt::default();
        art.add(&items.iter().filter_map(|i| if let Item::Element(e, _) = i { Some(e.clone()) } else { None }).collect::<Vec<_>>());
        description(&art)
    }

    #[test]
    fn valid_screen_waits_for_metadata_then_becomes_complete() {
        let (mut compiled, mut growing) = render_desc::Description::parse("grid = { layout = \"orthogonal\", step = [1, 1] }").unwrap().compile_growing(&Images, 64, false).unwrap();
        let fonts = Fonts::default();
        let items = [image("player-setup.pcx", 32)];
        assert_eq!(readiness(items.iter(), &fonts, &compiled, true), Readiness::Pending);
        assert!(sprites(&items, &fonts, &compiled, u32::MAX, false).is_empty());
        let growth = growing.grow(&compiled, &art(&items), &Images).unwrap();
        compiled.apply(&growth);
        assert_eq!(readiness(items.iter(), &fonts, &compiled, false), Readiness::Ready);
        assert_eq!(sprites(&items, &fonts, &compiled, u32::MAX, false).len(), 1);
    }

    #[test]
    fn failed_and_oversized_images_settle_without_retaining_an_unrelated_screen() {
        for (source, size) in [("missing.pcx", 32), ("oversized.pcx", 128)] {
            let (mut compiled, mut growing) = render_desc::Description::parse("grid = { layout = \"orthogonal\", step = [1, 1] }").unwrap().compile_growing(&Images, 64, false).unwrap();
            let fonts = Fonts::default();
            let items = [image(source, size)];
            assert_eq!(readiness(items.iter(), &fonts, &compiled, true), Readiness::Pending);
            if let Ok(growth) = growing.grow(&compiled, &art(&items), &Images) { compiled.apply(&growth); }
            if source == "missing.pcx" {
                assert_eq!(readiness(items.iter(), &fonts, &compiled, false), Readiness::Unavailable);
            } else {
                // The compiler preserves oversized sprite IDs as its
                // logged transparent 1x1 terminal fallback.
                assert_eq!(readiness(items.iter(), &fonts, &compiled, false), Readiness::Ready);
                assert_eq!((compiled.atlas.sprites[0].width, compiled.atlas.sprites[0].height), (1, 1));
            }
        }
    }
}
