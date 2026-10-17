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
#[derive(Clone, Debug)]
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
    let mut out = Vec::new();
    for (i, d) in frame.draws.iter().enumerate() {
        let under = frame.underlay_draws.iter().any(|r| r.contains(&i));
        let element = match d {
            ScreenDraw::Image { path, rect, x, y, keyed, clip, mask, palette } => Element::Image {
                region: Region {
                    source: relative_path(path, roots),
                    rect: *rect,
                    keyed: *keyed,
                    mask: mask.as_ref().map(|m| (relative_path(&m.path, roots), (m.at.0 - rect[0], m.at.1 - rect[1]))),
                    palette: palette.clone(),
                },
                at: (*x, *y),
                clip: *clip,
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
    fn paths_become_relative_to_their_root() {
        let roots = [PathBuf::from("C:\\Games\\Civ3\\Conquests"), PathBuf::from("C:\\Games\\Civ3")];
        assert_eq!(relative_path("C:\\Games\\Civ3\\Conquests\\Art\\Advisors\\a.pcx", &roots), "Art/Advisors/a.pcx");
        assert_eq!(relative_path("c:\\games\\civ3\\Art\\x.PCX", &roots), "Art/x.PCX");
        assert_eq!(relative_path("D:\\other\\y.pcx", &roots), "D:/other/y.pcx");
        assert_eq!(relative_path("D:\\x\\Conquests\\Art\\Civilopedia\\a.pcx", &[]), "Art/Civilopedia/a.pcx");
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
mod interactable_tests {
    use civ3_sys::controls::{Control, ControlKind};
    use scene::{Interactable, InteractableKind};

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
