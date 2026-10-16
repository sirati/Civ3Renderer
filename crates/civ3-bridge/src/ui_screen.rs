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
/// itself that we make ourselves (others are left out).
pub fn items(frame: &Frame, roots: &[PathBuf], generated: &[(u32, &str)]) -> Vec<Item> {
    frame
        .draws
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let under = frame.underlay_draws.iter().any(|r| r.contains(&i));
            Some(match d {
                ScreenDraw::Image { path, rect, x, y, keyed, clip } => Item::Element(Element::Image {
                    region: Region { source: relative_path(path, roots), rect: *rect, keyed: *keyed },
                    at: (*x, *y),
                    clip: *clip,
                }, under),
                ScreenDraw::Fill { rect, rgb, alpha, clip } => {
                    Item::Element(Element::Fill { rect: *rect, rgb: *rgb, alpha: *alpha, clip: *clip }, under)
                }
                ScreenDraw::Text { run, clip } => Item::Text(run.clone(), *clip, under),
                ScreenDraw::Surface { image, rect, x, y, clip } => {
                    let name = generated.iter().find(|g| g.0 == *image)?.1;
                    // The surface is at least as big as the region read from it.
                    let size = ((rect[0] + rect[2]).max(0) as u32, (rect[1] + rect[3]).max(0) as u32);
                    Item::Element(Element::Dynamic { name: name.to_owned(), size, rect: *rect, at: (*x, *y), clip: *clip }, under)
                }
            })
        })
        .collect()
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
    art.description(KEYED, OPAQUE)
}

/// The free sprites of `items`, in order: underlay items after render pass
/// `under_pass` (the one before the fog of war, which the game draws over
/// them), the rest on top.
pub fn sprites(items: &[Item], fonts: &Fonts, compiled: &Compiled, under_pass: u32) -> Vec<scene::FreeSprite> {
    let mut out = Vec::new();
    for i in items {
        let pass = |under: bool| if under { under_pass } else { u32::MAX };
        match i {
            Item::Element(e, under) => out.extend(menu::screen::sprites(std::slice::from_ref(e), compiled, pass(*under))),
            Item::Text(run, clip, under) => fonts.run_sprites(run, Some(*clip), compiled, pass(*under), &mut out),
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
        if name != "minimap" || done.contains(size) {
            continue;
        }
        done.push(*size);
        let colors: Vec<u16> = (0..32)
            .map(|civ| crate::minimap::from_rgb(table_color(world.color_tables.get(civ).copied().unwrap_or(0))))
            .collect();
        let rgba = crate::minimap::image(world, *size, |civ| colors.get(civ as usize).copied().unwrap_or(0));
        out.extend(menu::screen::patch(compiled, name, *size, rgba));
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_become_relative_to_their_root() {
        let roots = [PathBuf::from("C:\\Games\\Civ3\\Conquests"), PathBuf::from("C:\\Games\\Civ3")];
        assert_eq!(relative_path("C:\\Games\\Civ3\\Conquests\\Art\\Advisors\\a.pcx", &roots), "Art/Advisors/a.pcx");
        assert_eq!(relative_path("c:\\games\\civ3\\Art\\x.PCX", &roots), "Art/x.PCX");
        assert_eq!(relative_path("D:\\other\\y.pcx", &roots), "D:/other/y.pcx");
        assert_eq!(relative_path("D:\\x\\Conquests\\Art\\Civilopedia\\a.pcx", &[]), "Art/Civilopedia/a.pcx");
    }
}
