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

/// One thing to draw, in order.
pub enum Item {
    Element(Element),
    Text(TextRun, [i32; 4]),
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
        .filter_map(|d| {
            Some(match d {
                ScreenDraw::Image { path, rect, x, y, keyed, clip } => Item::Element(Element::Image {
                    region: Region { source: relative_path(path, roots), rect: *rect, keyed: *keyed },
                    at: (*x, *y),
                    clip: *clip,
                }),
                ScreenDraw::Fill { rect, rgb, clip } => Item::Element(Element::Fill { rect: *rect, rgb: *rgb, clip: *clip }),
                ScreenDraw::Text { run, clip } => Item::Text(run.clone(), *clip),
                ScreenDraw::Surface { image, rect, x, y, clip } => {
                    let name = generated.iter().find(|g| g.0 == *image)?.1;
                    // The surface is at least as big as the region read from it.
                    let size = ((rect[0] + rect[2]).max(0) as u32, (rect[1] + rect[3]).max(0) as u32);
                    Item::Element(Element::Dynamic { name: name.to_owned(), size, rect: *rect, at: (*x, *y), clip: *clip })
                }
            })
        })
        .collect()
}

/// The art and fonts `items` need; whether anything was new.
pub fn add_art(items: &[Item], art: &mut ScreenArt, fonts: &mut Fonts, files: &civ3_assets::GameFiles) -> bool {
    let elements: Vec<Element> = items.iter().filter_map(|i| if let Item::Element(e) = i { Some(e.clone()) } else { None }).collect();
    let runs: Vec<TextRun> = items.iter().filter_map(|i| if let Item::Text(r, _) = i { Some(r.clone()) } else { None }).collect();
    let new_art = art.add(&elements);
    let new_fonts = fonts.add(files, &runs);
    new_art || new_fonts
}

/// The description entries of `art`.
pub fn description(art: &ScreenArt) -> String {
    art.description(KEYED, OPAQUE)
}

/// The free sprites of `items`, in order.
pub fn sprites(items: &[Item], fonts: &Fonts, compiled: &Compiled) -> Vec<scene::FreeSprite> {
    let mut out = Vec::new();
    for i in items {
        match i {
            Item::Element(e) => out.extend(menu::screen::sprites(std::slice::from_ref(e), compiled)),
            Item::Text(run, clip) => fonts.run_sprites(run, Some(*clip), compiled, &mut out),
        }
    }
    out
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
        let Item::Element(Element::Dynamic { name, size, .. }) = i else { continue };
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
