//! The mirrored UI (`ui`) as our renderer draws it: image regions and fills
//! through the menu crate's run-time screens, text through the game's
//! fonts, in the game's composition order.

use std::path::PathBuf;

use menu::screen::{Element, Region, ScreenArt};
use render_desc::Compiled;

use crate::text::Fonts;
use crate::ui::{Frame, ScreenDraw};
pub use civ3_ui::ui_screen::*;

/// Loader options of colour-keyed regions (palette index 255 shows what is
/// below) and of opaque ones.
const KEYED: &str = "transparent = [254, 255]";
const OPAQUE: &str = "transparent = []";

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
        if let Some(element) = crate::loading_cursor::at_draw(frame, i) {
            out.push(Item::Element(element, false));
        }
        let under = frame.underlay_draws.iter().any(|r| r.contains(&i));
        let element = match d {
            ScreenDraw::Image { retained,path, rect, x, y, keyed, clip, mask, palette, opacity } => {
                let region = Region {
                    source: relative_path(path, roots),
                    rect: *rect,
                    keyed: *keyed,
                    mask: mask.as_ref().map(|m| (relative_path(&m.path, roots), (m.at.0 - rect[0], m.at.1 - rect[1]))),
                    palette: palette.clone(),
                };
                if let Some(state)=retained.as_ref().filter(|s|!s.changes().is_empty()) {
                    assert!(mask.is_none() && *opacity==255,"unsupported transformed masked/opacity source");
                    Element::Indexed {region,indices:menu::screen::Indices {index_changes:state.changes().to_vec(),transparent:if *keyed {vec![254,255]} else {vec![]},..Default::default()},at:(*x,*y),clip:*clip}
                } else if *opacity == 255 {
                    Element::Image { region, at: (*x, *y), clip: *clip }
                } else {
                    Element::OpacityImage { region, at: (*x, *y), clip: *clip, opacity: *opacity }
                }
            },
            ScreenDraw::ResizedImage { retained,path, rect, x, y, size, sample_step, keyed, clip, mask, palette, opacity } => {
                let region = Region {
                    source: relative_path(path, roots),
                    rect: *rect,
                    keyed: *keyed,
                    mask: mask.as_ref().map(|m| (relative_path(&m.path, roots), (m.at.0 - rect[0], m.at.1 - rect[1]))),
                    palette: palette.clone(),
                };
                if let Some(state)=retained.as_ref().filter(|s|!s.changes().is_empty()) {
                    assert!(mask.is_none() && *opacity==255,"unsupported transformed masked/opacity source");
                    let resize=Some(menu::screen::IndexedResize {size:(size.0.max(0) as u32,size.1.max(0) as u32),sample_step:*sample_step});
                    Element::Indexed {region,indices:menu::screen::Indices {index_changes:state.changes().to_vec(),transparent:if *keyed {vec![254,255]} else {vec![]},resize,..Default::default()},at:(*x,*y),clip:*clip}
                } else {
                    Element::ResizedImage { region, at: (*x, *y), size: *size, sample_step: *sample_step, clip: *clip, opacity: *opacity }
                }
            }
            ScreenDraw::Fill { rect, rgb, alpha, clip } => Element::Fill { rect: *rect, rgb: *rgb, alpha: *alpha, clip: *clip },
            ScreenDraw::Indexed { retained,path, rect, x, y, clip, palette, indices } => {
                let region = Region { source: relative_path(path, roots), rect: *rect, keyed: true, mask: None, palette: palette.clone() };
                let mut indices=indices.clone();
                if let Some(state)=retained {indices.index_changes=state.changes().to_vec();}
                Element::Indexed { region, indices, at: (*x, *y), clip: *clip }
            }
            ScreenDraw::Text { run, clip } => {
                out.push(Item::Text(run.clone(), *clip, under));
                continue;
            }
            // Unit pictures, from our unit art in the viewing player's
            // colours (§16.9), over their shadow as on the map.
            // Leader heads: in their own colours.
            ScreenDraw::Flc { path, chunk, x, y, clip, plain: true } => {
                let source = relative_path(path, roots);
                Element::Sprite { name: format!("Flc@{source}/{chunk}"), at: (*x, *y), clip: *clip, scale_den: 0, palette: String::new() }
            }
            ScreenDraw::Flc { path, chunk, x, y, clip, plain: false } => {
                let source = relative_path(path, roots);
                let path = source;
                let shadow = format!("UnitShadow@{path}/{chunk}");
                out.push(Item::Element(Element::Sprite { name: shadow, at: (*x, *y), clip: *clip, scale_den: 16, palette: String::new() }, under));
                let palette = crate::units::Recolor::civ(unit_colors).palette();
                out.push(Item::Element(Element::Sprite { name: format!("UnitArt@{path}/{chunk}"), at: (*x, *y), clip: *clip, scale_den: 0, palette }, under));
                Element::PaletteAlphaSprite { name: format!("UnitSmoke@{path}/{chunk}"), at: (*x, *y), clip: *clip }
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

/// The same currently visible items drive both required-art requests and
/// publication readiness. World notices belong here too: they can introduce
/// a new generated plate while every ordinary interface item is unchanged.
pub fn mandatory_items<'a>(screen_parts: [&'a [Item]; 6], notices: &'a [Item]) -> impl Clone + Iterator<Item = &'a Item> {
    screen_parts.into_iter().chain([notices]).flatten()
}

/// The art and fonts `items` need; whether anything was new.
pub fn add_art<'a>(items: impl IntoIterator<Item = &'a Item>, art: &mut ScreenArt, fonts: &mut Fonts, files: &civ3_assets::GameFiles) -> bool {
    let (mut elements, mut runs) = (Vec::new(), Vec::new());
    for item in items {
        match item {
            Item::Element(element, _) => elements.push(element.clone()),
            Item::Text(run, _, _) => runs.push(run.clone()),
        }
    }
    let new_art = art.add(&elements);
    let new_fonts = fonts.add(files, &runs);
    new_art || new_fonts
}

/// The description entries of `art`.
pub fn description(art: &ScreenArt) -> String {
    art.description_with_backgrounds(KEYED, OPAQUE, |mask, (dx, dy)| {
        format!("alpha_image = \"{}\"\nalpha_offset = [{dx}, {dy}]", mask.replace('\\', "\\\\").replace('"', "\\\""))
    }, crate::ui_background::regions)
}

/// The zooms the scene has zoomed art for (`scene::Scene::zooms`): every
/// zoom, (0, 0), where text can be drawn from outlines (the art has the
/// game's face), else none.
pub fn outline_zooms(compiled: &Compiled) -> Vec<(u32, u32)> {
    if compiled.fonts.iter().all(|(name, _)| name == render_desc::FALLBACK_FONT) { Vec::new() } else { vec![(0, 0)] }
}

/// Whether all visible art of a screen is resident: yes, not yet but
/// asked for, or not yet and no request pending (then asked for again,
/// see [`ArtRetry`]). Art readiness drives requests and diagnostics.
/// Missing art does not delay a committed UI command group.
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
    for item in items.filter(|item| !item_ready(item, fonts, compiled)) {
        missing = true;
        if let Item::Text(run, _, _) = item { loading |= fonts.run_loading(run); }
    }
    if !missing { Readiness::Ready } else if building || loading { Readiness::Pending } else { Readiness::Unavailable }
}

fn item_ready(item: &Item, fonts: &Fonts, compiled: &Compiled) -> bool {
    match item {
        Item::Element(element, _) => crate::loading_cursor::is_optional(element) || element.ready_with_backgrounds(compiled, crate::ui_background::regions),
        Item::Text(run, clip, _) => clip[0] >= clip[2] || clip[1] >= clip[3] || fonts.run_ready(run, compiled),
    }
}

/// Bounded log lines for visible items whose art is not resident yet.
pub fn missing_art<'a>(items: impl Iterator<Item = &'a Item>, fonts: &Fonts, compiled: &Compiled) -> Vec<String> {
    items.filter(|item| !item_ready(item, fonts, compiled)).take(8).map(|item| match item {
        Item::Element(element, _) => format!("screen element {element:?}"),
        Item::Text(run, _, _) => format!("text {:?}/{} weight {} italic {} underline {}, font loading {}: {:?}", run.face, run.height, run.weight, run.italic, run.underline, fonts.run_loading(run), run.text.chars().take(100).collect::<String>()),
    }).collect()
}

/// Asset diagnostics for a committed UI group. Art not resident yet is drawn
/// as nothing (text, controls and the other art as usual) and shows in
/// the first frame after it arrives. There is no substitute picture.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Publication {
    /// Every visible item has its art.
    Complete,
    /// Published without some art; `pending`: its load is still running
    /// (otherwise [`ArtRetry`] asks for it again).
    Partial { pending: bool },
}

pub fn publication<'a>(items: impl Iterator<Item = &'a Item>, fonts: &Fonts, compiled: &Compiled, building: bool, _size: (u32, u32)) -> Publication {
    match readiness(items, fonts, compiled, building) {
        Readiness::Ready => Publication::Complete,
        Readiness::Pending => Publication::Partial { pending: true },
        Readiness::Unavailable => Publication::Partial { pending: false },
    }
}

/// The screen art of the visible `items` that the compiled art cannot
/// draw yet (to be asked for again).
pub fn unready_art<'a>(items: impl Iterator<Item = &'a Item>, compiled: &Compiled) -> ScreenArt {
    let elements: Vec<Element> = items
        .filter_map(|item| match item {
            Item::Element(element, _) if !element.ready_with_backgrounds(compiled, crate::ui_background::regions) => Some(element.clone()),
            _ => None,
        })
        .collect();
    let mut art = ScreenArt::default();
    art.add(&elements);
    art
}

/// Image files the game showed that are in none of the art folders: never
/// described (one such file would fail the whole addition it is in), drawn
/// as nothing, each reported once.
#[derive(Debug, Default)]
pub struct AbsentArt {
    sources: std::collections::HashSet<String>,
    present_sources: std::collections::HashSet<String>,
}

impl AbsentArt {
    /// The art folders changed: what was absent may be there now.
    pub fn clear(&mut self) {
        self.sources.clear();
        self.present_sources.clear();
    }

    /// Whether `item` shows a file known to be absent.
    pub fn covers(&self, item: &Item) -> bool {
        let Item::Element(element, _) = item else { return false };
        region_of(element).is_some_and(|region| self.region_absent(region))
    }

    fn region_absent(&self, region: &Region) -> bool {
        self.sources.contains(&region.source) || region.mask.as_ref().is_some_and(|(mask, _)| self.sources.contains(mask))
    }

    /// Whether `region`'s files are in the art folders (looked up once per
    /// new region; an absent file is logged once).
    fn present(&mut self, region: &Region, files: &civ3_assets::GameFiles) -> bool {
        self.present_by(region, |source| {
            if files.resolve(source).is_ok() { return true; }
            let roots: Vec<String> = files.roots().iter().map(|r| r.display().to_string()).collect();
            log::warn!("art: {source} is in none of the art folders ({}); drawn as nothing", roots.join("; "));
            render_errors::report(render_errors::Kind::ArtImage, &format!("not in the art folders; {}", render_errors::class_of(source)), format_args!("{source} (folders {})", roots.join("; ")));
            false
        })
    }

    fn present_by(&mut self, region: &Region, mut resolve: impl FnMut(&str) -> bool) -> bool {
        if self.region_absent(region) { return false; }
        let mut present = true;
        for source in std::iter::once(&region.source).chain(region.mask.as_ref().map(|(mask, _)| mask)) {
            if self.present_sources.contains(source) { continue; }
            if resolve(source) {
                self.present_sources.insert(source.clone());
            } else {
                self.sources.insert(source.clone());
                present = false;
            }
        }
        present
    }

}

/// Leaves out of `art` the regions whose files are in none of the art
/// folders `files` has (other folders than those they were shown with):
/// a whole description with one of them would fail as a whole.
pub fn retain_present(art: &mut ScreenArt, files: &civ3_assets::GameFiles) {
    retain_present_by(art, &mut |path| files.resolve(path).is_ok());
}

/// [`retain_present`] with the lookup `found` (a path in the art folders:
/// whether it is there), e.g. one remembered across folder changes.
pub fn retain_present_by(art: &mut ScreenArt, found: &mut dyn FnMut(&str) -> bool) {
    let mut present = |r: &Region| found(&r.source) && r.mask.as_ref().is_none_or(|(mask, _)| found(mask));
    let before = art.regions.len() + art.resized_regions.len() + art.indexed.len();
    art.regions.retain(|r| present(r));
    art.resized_regions.retain(|(r, _, _)| present(r));
    art.indexed.retain(|(r, _)| present(r));
    let dropped = before - art.regions.len() - art.resized_regions.len() - art.indexed.len();
    if dropped > 0 {
        log::info!("art: {dropped} screen regions shown with other art folders are not in these; left out until shown again");
    }
}

fn region_of(element: &Element) -> Option<&Region> {
    match element {
        Element::Image { region, .. } | Element::OpacityImage { region, .. } | Element::ResizedImage { region, .. } | Element::Indexed { region, .. } => Some(region),
        _ => None,
    }
}

/// [`add_art`] for files that exist only (see [`AbsentArt`]): an image
/// region is looked up once, when it is new.
pub fn add_present_art<'a>(items: impl IntoIterator<Item = &'a Item>, art: &mut ScreenArt, fonts: &mut Fonts, files: &civ3_assets::GameFiles, absent: &mut AbsentArt) -> bool {
    let kept: Vec<&Item> = items
        .into_iter()
        .filter(|item| {
            let Item::Element(element, _) = item else { return true };
            let known = match element {
                Element::Image { region, .. } | Element::OpacityImage { region, .. } => art.regions.contains(region),
                Element::ResizedImage { region, size, sample_step, .. } => art.resized_regions.contains(&(region.clone(), (size.0 as u32, size.1 as u32), *sample_step)),
                Element::Indexed { region, indices, .. } => art.indexed.iter().any(|(r, i)| i == indices && r.source == region.source && r.rect == region.rect && r.palette == region.palette),
                _ => true,
            };
            known || element.is_empty() || region_of(element).is_none_or(|region| absent.present(region, files))
        })
        .collect();
    add_art(kept, art, fonts, files)
}

/// Asks again, a few times, for visible art that was described but did
/// not arrive (an addition that failed or was given up): an on-demand
/// request like the first one, never a substitute picture.
#[derive(Debug, Default)]
pub struct ArtRetry {
    asked: std::collections::HashMap<String, (u32, std::time::Instant)>,
}

impl ArtRetry {
    /// Times one piece of art is asked for again, and how far apart.
    pub const ATTEMPTS: u32 = 3;
    pub const SPACING: std::time::Duration = std::time::Duration::from_millis(500);

    /// The art folders changed.
    pub fn clear(&mut self) {
        self.asked.clear();
    }

    /// Whether some of `missing` is still to be asked for again (now or
    /// after [`ArtRetry::SPACING`]).
    pub fn would_ask(&self, missing: &ScreenArt) -> bool {
        let open = |key: String| self.asked.get(&key).is_none_or(|(n, _)| *n < Self::ATTEMPTS);
        missing.regions.iter().any(|r| open(format!("{r:?}")))
            || missing.resized_regions.iter().any(|r| open(format!("{r:?}")))
            || missing.fills.iter().any(|r| open(format!("{r:?}")))
            || missing.dynamic.iter().any(|r| open(format!("{r:?}")))
            || missing.indexed.iter().any(|r| open(format!("{r:?}")))
    }

    /// What of `missing` to ask for again now (counted as asked).
    pub fn due(&mut self, missing: &ScreenArt, now: std::time::Instant) -> ScreenArt {
        let mut ask = |key: String| {
            let entry = self.asked.entry(key.clone()).or_insert((0, now.checked_sub(Self::SPACING).unwrap_or(now)));
            if entry.0 >= Self::ATTEMPTS || (entry.0 > 0 && now.duration_since(entry.1) < Self::SPACING) {
                return false;
            }
            entry.0 += 1;
            entry.1 = now;
            if entry.0 == Self::ATTEMPTS {
                log::warn!("art: {key} asked for {} times without arriving; not asked again", Self::ATTEMPTS);
                render_errors::report(render_errors::Kind::VisibleArtMissing, "screen art asked for again and again", format_args!("{key}: {} times", Self::ATTEMPTS));
            }
            true
        };
        ScreenArt {
            regions: missing.regions.iter().filter(|r| ask(format!("{r:?}"))).cloned().collect(),
            resized_regions: missing.resized_regions.iter().filter(|r| ask(format!("{r:?}"))).cloned().collect(),
            fills: missing.fills.iter().filter(|r| ask(format!("{r:?}"))).cloned().collect(),
            dynamic: missing.dynamic.iter().filter(|r| ask(format!("{r:?}"))).cloned().collect(),
            indexed: missing.indexed.iter().filter(|r| ask(format!("{r:?}"))).cloned().collect(),
        }
    }
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
            Item::Element(e, under) => {
                let start = out.len();
                out.extend(menu::screen::sprites_with_backgrounds(std::slice::from_ref(e), compiled, pass(*under), crate::ui_background::regions));
                if crate::loading_cursor::is_optional(e) {
                    for sprite in &mut out[start..] { sprite.ticks_per_frame = scene::loading_cursor::TICKS_PER_FRAME; }
                }
            }
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
        let rgba = crate::minimap::image_mode(world, *size, world.navigator_modes.for_surface(name), |civ| colors.get(civ as usize).copied().unwrap_or(0));
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
    use crate::text::TextRun;

    fn assert_background_pixels(actual: &tile_render::RgbaImage, expected: &tile_render::RgbaImage, context: &str) {
        let mut first = None;
        let mut count = 0;
        for (index, (a, b)) in actual.pixels.iter().zip(&expected.pixels).enumerate() {
            if a != b { count += 1; if first.is_none() { first = Some((index as u32 % actual.width, index as u32 / actual.width, a, b)); } }
        }
        assert!(first.is_none(), "{context}: {count} differing pixels; first {first:?}");
    }

    fn install() -> Option<civ3_assets::GameFiles> {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        std::path::Path::new(&install).is_dir().then(|| civ3_assets::GameFiles::complete(&install))
    }

    /// The sprite tables of a compile, as the two sides compare them.
    fn tables(c: &Compiled) -> (Vec<String>, Vec<(u32, u32, i32, i32, u32)>) {
        (c.sprite_names.clone(), c.atlas.sprites.iter().map(|s| (s.width, s.height, s.anchor_x, s.anchor_y, s.downscaled)).collect())
    }

    /// `art`'s description compiled whole and as a growth, with pixels (the
    /// native viewer's compile) and without (the bridge's): the same
    /// tables, and every element drawable from the pixel ones.
    fn compiles_alike(art: &ScreenArt, elements: &[Element], files: &civ3_assets::GameFiles) -> Compiled {
        let loader = civ3_assets::loader::Civ3Loader::new(files.clone());
        let grid = "[grid]\nlayout = 'orthogonal'\nstep = [1, 1]\n";
        let desc = render_desc::Description::parse(&format!("{grid}{}", description(art))).unwrap();
        let drawn = desc.compile(&loader, 4096).unwrap();
        assert_eq!(tables(&drawn), tables(&desc.compile_layout(&loader, 4096).unwrap()));
        let grown = [true, false].map(|pixels| {
            let (mut c, mut growing) = render_desc::Description::parse(grid).unwrap().compile_growing(&loader, 4096, pixels).unwrap();
            let growth = growing.grow(&c, &description(art), &loader).unwrap();
            c.apply(&growth);
            c
        });
        assert_eq!(tables(&grown[0]), tables(&grown[1]));
        for c in [&drawn, &grown[0]] {
            for e in elements { assert!(e.ready_with_backgrounds(c, crate::ui_background::regions), "{e:?}"); }
            assert!(c.sprite_names.iter().all(|n| c.sprite(n).is_some_and(|id| c.sprite_available(id))), "art left out");
        }
        drawn
    }

    /// The F7 screen (Wonders of the World): each wonder's owner as the
    /// leader portrait of the advisors' sheet (`art\advisors\HA_all.pcx`,
    /// cell 116,1 113×113) drawn at half size by the scaled blit (trace
    /// `ui_resized_image ... 56 56 131072 131072`). That made a resized
    /// sheet of an 8-bit (indexed) image, which the viewer's compile refused
    /// though the bridge's took it: the portrait never came, and neither did
    /// any scene after it (the screen's close, the map, a popup).
    #[test]
    fn the_wonders_screens_leader_portraits_compile_on_both_sides() {
        use render_desc::ImageLoader;
        use crate::ui::{Event, UiModel};
        let Some(files) = install() else { return eprintln!("no Civ III install, skipped") };
        let path = "Art/Advisors/HA_all.pcx";
        let mut model = UiModel::new(vec![], vec![]);
        model.apply(Event::Loaded { image: 10, path: path.into(), size: (1500, 600) });
        model.apply(Event::Sliced { sprite: 20, image: 10, rect: [116, 1, 113, 113] });
        model.apply(Event::ResizedBlit { sprite: 20, canvas: 40, x: 394, y: 117, size: (56, 56), sample_step: [131072, 131072], clip: Some([0, 0, 1024, 768]), palette: None, indices: None });
        model.apply(Event::PaintBegin);
        model.apply(Event::Composite { canvas: 40, vtable: 1, origin: (448, 156), size: (1024, 768), clip: [0, 0, 1920, 1080], keyed: false });
        model.apply(Event::PaintEnd);
        let frame = model.frame();
        assert!(matches!(frame.draws.as_slice(), [ScreenDraw::ResizedImage { size: (56, 56), .. }]), "{:?}", frame.draws);
        let elements: Vec<_> = items(&frame, &[], &[], 0).into_iter().filter_map(|i| match i { Item::Element(e, _) => Some(e), _ => None }).collect();
        let mut art = ScreenArt::default();
        art.add(&elements);
        assert_eq!(art.resized_regions.len(), 1);
        let compiled = compiles_alike(&art, &elements, &files);
        // The portrait's pixels: every other one of the cell.
        let sprites = menu::screen::sprites(&elements, &compiled, u32::MAX);
        assert_eq!(sprites.len(), 1);
        let s = compiled.atlas.sprites[compiled.sprite_names.iter().position(|n| n.contains(">56x56@131072,131072")).unwrap()];
        assert_eq!((s.width, s.height, s.palette), (56, 56, 0));
        let source = &civ3_assets::loader::Civ3Loader::new(files).load(path, &toml::from_str(KEYED).unwrap()).unwrap()[0];
        let page = &compiled.atlas.pages[s.page as usize];
        for j in 0..56 { for i in 0..56 {
            assert_eq!(page.get(s.atlas_x + i, s.atlas_y + j), source.get(116 + 2 * i, 1 + 2 * j), "({i}, {j})");
        }}
    }

    /// Every kind of element the mirror makes, of 8-bit (indexed) art and
    /// of art in colours (masked), plain, through another palette, resized,
    /// translucent, by indices (shades, smoke, a frame of an animation,
    /// resized), and run-time pictures: what the bridge's compile takes
    /// the viewer's takes, with the same tables, all of it drawable.
    #[test]
    fn every_kind_of_screen_art_compiles_alike_with_and_without_pixels() {
        let Some(files) = install() else { return eprintln!("no Civ III install, skipped") };
        use menu::screen::{IndexedResize, PaletteFrom, Region};
        let clip = [0, 0, 1024, 768];
        let region = |source: &str, rect: [i32; 4], keyed: bool| Region { source: source.into(), rect, keyed, mask: None, palette: None };
        let portrait = region("Art/Advisors/HA_all.pcx", [116, 1, 113, 113], true);
        let opaque = region("Art/Advisors/HA_all.pcx", [0, 0, 64, 64], false);
        let masked = Region { mask: Some(("Art/interface/ButtonAlpha.pcx".into(), (0, 0))), ..region("Art/interface/NormButtons.pcx", [64, 224, 32, 32], true) };
        let recoloured = Region { palette: Some(PaletteFrom { image: "Art/Units/Palettes/ntp03.pcx".into(), first: 0, count: 64 }), ..region("Art/interface/NormButtons.pcx", [0, 0, 32, 32], true) };
        let half = |r: &Region, opacity: u8| Element::ResizedImage { region: r.clone(), at: (5, 5), size: (r.rect[2] / 2, r.rect[3] / 2), sample_step: [131072, 131072], clip, opacity };
        let mut elements = Vec::new();
        for r in [&portrait, &opaque, &masked, &recoloured] {
            elements.push(Element::Image { region: r.clone(), at: (1, 2), clip });
            elements.push(Element::OpacityImage { region: r.clone(), at: (1, 2), clip, opacity: 128 });
            elements.push(half(r, 255));
            elements.push(half(r, 100));
        }
        let flc = region("Art/Units/Artillery/ArtilleryAttackA.flc", [0, 0, 64, 64], true);
        let resize = Some(IndexedResize { size: (30, 20), sample_step: [140000, 150000] });
        for (r, indices) in [
            (&portrait, crate::ui::indices::colored(&[254, 255])),
            (&portrait, crate::ui::indices::colored_shades()),
            (&portrait, menu::screen::Indices { resize, ..crate::ui::indices::colored(&[254, 255]) }),
            (&flc, menu::screen::Indices { frame: Some(3), ..crate::ui::indices::colored(&[254, 255]) }),
            (&flc, menu::screen::Indices { frame: Some(3), resize, ..crate::ui::indices::colored_smoke() }),
        ] {
            elements.push(Element::Indexed { region: r.clone(), indices, at: (3, 4), clip });
        }
        elements.push(Element::Dynamic { name: "minimap".into(), size: (40, 30), rect: [0, 0, 40, 30], at: (9, 9), clip });
        let mut art = ScreenArt::default();
        art.add(&elements);
        compiles_alike(&art, &elements, &files);
    }

    #[test]
    fn resized_flc_chunks_keep_full_source_extent_palette_and_native_sample_phase() {
        use render_desc::ImageLoader;
        use crate::ui::{Event, UiModel};
        let Ok(install) = std::env::var("CIV3_INSTALL") else { return };
        let files = civ3_assets::GameFiles::complete(&install);
        let path = "Art/Units/Artillery/ArtilleryAttackA.flc";
        let anim = civ3_assets::flc::decode(&files.read(path).unwrap()).unwrap();
        let chunk = anim.frames.iter().enumerate().skip(1).find_map(|(i, f)| f.pixels.iter().any(|index| (224..240).contains(index)).then_some(i)).expect("actual smoke frame") as u32;
        assert!(chunk > 0, "frame selection must distinguish the initial frame");
        let source_size = (anim.width as i32, anim.height as i32);
        // Native FLC_Frame_Image::load allocates the full header width/height.
        // OffsetLeft/Top affect destination placement, never source sampling.
        let ((x, y), size, step) = crate::game_render::scaled_geometry((20, 30), (7, 9, source_size.0, source_size.1), [11, 17, 32], true).unwrap();
        let mut model = UiModel::new(vec![], vec![]);
        model.apply(Event::FlcLoaded { flic: 5, path: path.into() });
        let palette = Some(menu::screen::PaletteFrom { image: "Art/Units/Palettes/ntp03.pcx".into(), first: 0, count: 64 });
        for indices in [crate::ui::indices::colored(&[254, 255]), crate::ui::indices::colored_shades(), crate::ui::indices::colored_smoke()] {
            model.apply(Event::ResizedFlcBlit { flic: 5, chunk, canvas: 40, x, y, source_size, size, sample_step: step, clip: Some([x + 2, y + 3, x + size.0, y + size.1]), palette: palette.clone(), indices });
        }
        model.apply(Event::PaintBegin);
        model.apply(Event::Composite { canvas: 40, vtable: 1, origin: (0, 0), size: (512, 512), clip: [0, 0, 512, 512], keyed: false });
        model.apply(Event::PaintEnd);
        let frame = model.frame();
        assert_eq!(frame.draws.len(), 3);
        for draw in &frame.draws { assert!(matches!(draw, ScreenDraw::Indexed { rect, indices, .. } if *rect == [0, 0, source_size.0, source_size.1] && indices.frame == Some(chunk) && indices.resize.unwrap().sample_step == step)); }
        let elements: Vec<_> = items(&frame, &[], &[], 0).into_iter().filter_map(|i| match i { Item::Element(e, _) => Some(e), _ => None }).collect();
        let mut art = ScreenArt::default(); art.add(&elements);
        assert_eq!(ScreenArt::parse_lines(&art.to_lines()), art);
        let desc = art.description("", "", |_, _| String::new());
        let loader = civ3_assets::loader::Civ3Loader::new(files);
        let compiled = render_desc::Description::parse(&format!("[grid]\nlayout = 'orthogonal'\nstep = [1, 1]\n{desc}")).unwrap().compile(&loader, 512).unwrap();
        let sprites = menu::screen::sprites(&elements, &compiled, 0);
        assert_eq!(sprites.len(), 3);
        assert_eq!(sprites[1].scale_den, 16);
        assert_eq!(sprites[2].blend, scene::SpriteBlend::PaletteAlpha256);
        let parsed = render_desc::Description::parse(&format!("[grid]\nlayout = 'orthogonal'\nstep = [1, 1]\n{desc}")).unwrap();
        for sheet in parsed.sheets.values() {
            let image = &parsed.images[&sheet.image];
            let raw = loader.load(path, &image.options).unwrap();
            let full = &raw[chunk as usize];
            let id = compiled.sprite(&format!("{}/0", sheet.group.as_ref().unwrap())).unwrap(); let sprite = compiled.atlas.sprites[id.0 as usize];
            let page = &compiled.atlas.pages[sprite.page as usize];
            for j in 0..size.1 as u32 { for i in 0..size.0 as u32 {
                assert_eq!(page.get(sprite.atlas_x + i, sprite.atlas_y + j), full.get(((u64::from(i) * u64::from(step[0])) >> 16) as u32, ((u64::from(j) * u64::from(step[1])) >> 16) as u32));
            }}
        }
    }

    #[test]
    #[ignore = "requires AMD RADV, actual Civ3 palette and read-only native table capture"]
    fn resized_smoke_palette_alpha_matches_native_tables_after_clipped_canvas_copies() {
        use render_desc::ImageLoader;
        use crate::ui::{Event, UiModel};
        let install = std::env::var("CIV3_INSTALL").expect("CIV3_INSTALL is required for native smoke oracle");
        let tables = std::fs::read(std::env::var("CIV3_NATIVE_SMOKE_TABLES").expect("CIV3_NATIVE_SMOKE_TABLES is required")).unwrap();
        assert_eq!(tables.len(), 31 * 32768 * 2);
        struct Ramp(civ3_assets::loader::Civ3Loader);
        impl ImageLoader for Ramp {
            fn load(&self, source: &str, options: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
                Ok(self.indexed(source, options)?.unwrap().into_iter().map(|f| tile_render::RgbaImage::from_fn(f.pixels.width, f.pixels.height, |x, y| f.palette[f.pixels.get(x, y) as usize])).collect())
            }
            fn indexed(&self, _: &str, options: &toml::Table) -> Result<Option<Vec<render_desc::IndexedFrame>>, String> {
                // Actual game source and palette-alpha loader, with generated
                // test indices. No game art is copied or modified.
                let mut frames = self.0.indexed("Art/Terrain/Territory.pcx", options)?.ok_or("indexed fixture")?;
                frames.truncate(1);
                frames[0].pixels = tile_render::IndexImage::from_fn(16, 32, |x, y| (224 + (x + 3 * y) % 16) as u8);
                Ok(Some(frames))
            }
        }
        let mut model = UiModel::new(vec![], vec![]);
        let expand = |q: u32| ((q << 3) | (q >> 2)) as u8;
        for y in 0..40 {
            model.apply(Event::Fill { canvas: 100, rect: [0, y, 24, 1], rgb: [expand(y as u32 % 32), expand(3 * y as u32 % 32), expand(7 * y as u32 % 32)], alpha: 255, clip: None });
        }
        model.apply(Event::Loaded { image: 10, path: "fixture.pcx".into(), size: (16, 32) });
        model.apply(Event::Sliced { sprite: 20, image: 10, rect: [0, 0, 16, 32] });
        let ((x, y), size, step) = crate::game_render::scaled_geometry((2, 3), (0, 0, 16, 32), [11, 17, 32], true).unwrap();
        assert_eq!(size, (5, 17));
        model.apply(Event::ResizedBlit { sprite: 20, canvas: 40, x, y, size, sample_step: step, clip: Some([4, 3, 7, 20]), palette: None, indices: Some(crate::ui::indices::colored_smoke()) });
        model.apply(Event::Copy { from: 40, src: [4, 3, 3, 17], canvas: 300, x: 10, y: 5, keyed: true });
        model.apply(Event::PaintBegin);
        for canvas in [100, 300] { model.apply(Event::Composite { canvas, vtable: 1, origin: (0, 0), size: (24, 40), clip: [0, 0, 24, 40], keyed: false }); }
        model.apply(Event::PaintEnd);
        let frame = model.frame();
        assert!(matches!(frame.draws.last(), Some(ScreenDraw::Indexed { x: 8, y: 5, clip: [10, 5, 13, 22], indices, .. }) if indices.resize == Some(menu::screen::IndexedResize { size: (5, 17), sample_step: step })));
        let elements: Vec<_> = items(&frame, &[], &[], 0).into_iter().filter_map(|item| match item { Item::Element(element, _) => Some(element), _ => None }).collect();
        let mut art = ScreenArt::default(); art.add(&elements);
        let text = format!("[grid]\nlayout = 'orthogonal'\nstep = [1, 1]\n{}", art.description("", "", |_, _| String::new()));
        let compiled = render_desc::Description::parse(&text).unwrap().compile(&Ramp(civ3_assets::loader::Civ3Loader::new(civ3_assets::GameFiles::complete(&install))), 256).unwrap();
        let free = menu::screen::sprites(&elements, &compiled, u32::MAX);
        assert_eq!(free.iter().filter(|s| s.blend == scene::SpriteBlend::PaletteAlpha256).count(), 1);
        let mut world = tile_render::TileWorld::new(tile_render::Grid { cols: 1, rows: 1, layout: tile_render::Layout::Orthogonal, step: (1, 1), wrap_x: false, wrap_y: false }, compiled.sets.clone());
        world.color = tile_render::ColorModel::Quantized { bits: 5 };
        world.free = free.iter().map(|s| tile_viewer::placed_sprite(&compiled, s, Some(0), None)).collect();
        let gpu = tile_render::Headless::new().expect("RADV for end-to-end native indexed smoke");
        let adapter = gpu.adapter.get_info(); assert_eq!(adapter.vendor, 0x1002); assert_eq!(adapter.backend, wgpu::Backend::Vulkan); assert!(adapter.name.to_ascii_lowercase().contains("radv"));
        let mut renderer = tile_render::TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, civ3_ui::viewer::SHADERS);
        renderer.set_atlas(&gpu.device, &gpu.queue, &compiled.atlas); renderer.set_world(&gpu.device, &world);
        let camera = tile_render::Camera { cells: false, ..tile_render::Camera::new((24, 40)) };
        let got = gpu.render(&mut renderer, &camera, 0, [0, 0, 0, 255]);
        for y in 0..40 { for x in 0..24 {
            let mut packed = ((y % 32) << 10) | ((3 * y % 32) << 5) | (7 * y % 32);
            if (10..13).contains(&x) && (5..22).contains(&y) {
                let sx = ((u64::from(x - 8) * u64::from(step[0])) >> 16) as u32;
                let sy = ((u64::from(y - 5) * u64::from(step[1])) >> 16) as u32;
                let level = 15 + (sx + 3 * sy) % 16;
                let at = (level * 32768 + packed) as usize * 2;
                packed = u32::from(u16::from_le_bytes([tables[at], tables[at + 1]])) & 0x7fff;
            }
            assert_eq!(got.get(x, y), [expand((packed >> 10) & 31), expand((packed >> 5) & 31), expand(packed & 31), 255], "native loader/resize/copy/GPU pixel{x},{y}");
        } }
    }

    #[test]
    fn full_native_background_pictures_filter_behind_separate_controls_and_text() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        if !std::path::Path::new(&install).is_dir() { return eprintln!("no Civ III assets for native mixed-art regression"); }
        let files = civ3_assets::GameFiles::complete(&install);
        let gpu = tile_render::Headless::new().expect("GPU for full native mixed-art regression");
        eprintln!("full mixed UI adapter: {:?}", gpu.adapter.get_info());
        let clip = [0, 0, 1024, 768];
        let run = TextRun::parse("text dc 0 at 332 218 color 0 0 0 font \"Lucida Sans\"/-18/0/0/0 str Separate native label").unwrap();
        let mut fonts = Fonts::default();
        fonts.add(&files, &[run.clone()]);
        let font_description = fonts.description();
        for source in ["Art/Civilopedia/civilopediaBackground.pcx", "Art/interface/x_pref background.pcx", "Art/Advisors/foreign.pcx", "Art/Advisors/wonders_background.pcx", "Art/x_title.PCX"] {
            let region = Region { source: source.into(), rect: [0, 0, 1024, 768], keyed: true, mask: None, palette: None };
            let texture = crate::ui_background::regions(&region);
            for opacity in [255, 137] {
                let full = Element::OpacityImage { region: region.clone(), at: (0, 0), clip, opacity };
                let plate = Element::Fill { rect: [280, 180, 300, 70], rgb: [240, 230, 200], alpha: 255, clip };
                let button = Element::Image { region: Region { source: "Art/interface/menuButtons.pcx".into(), rect: [1, 1, 36, 30], keyed: true, mask: None, palette: None }, at: (288, 188), clip };
                let items = vec![Item::Element(full.clone(), false), Item::Element(plate.clone(), false), Item::Element(button.clone(), false), Item::Text(run.clone(), clip, false)];
                let before = items.clone();
                let mut art = ScreenArt::default();
                art.add(&[full, plate, button]);
                let art_before = art.to_lines();
                let compile = |screen: String| crate::assets::compile(&format!("grid = {{ layout = 'orthogonal', step = [1, 1] }}\n{screen}\n{font_description}"), civ3_assets::GameFiles::with_roots(files.roots().to_vec())).unwrap();
                let compiled = compile(description(&art));
                let mut nearest = compile(description(&art));
                for sprite in &mut nearest.atlas.sprites { sprite.sampling = 0; }
                let original = compile(art.description(KEYED, OPAQUE, |_, _| String::new()));
                let world = |compiled: &Compiled, split: bool| {
                    let mut world = compiled.world(1, 1, false, false);
                    let free = if split { sprites(&items, &fonts, compiled, 0, true) } else {
                        let mut free = Vec::new();
                        for item in &items {
                            match item {
                                Item::Element(element, _) => free.extend(menu::screen::sprites(std::slice::from_ref(element), compiled, u32::MAX)),
                                Item::Text(..) => free.extend(sprites(std::slice::from_ref(item), &fonts, compiled, 0, true)),
                            }
                        }
                        free
                    };
                    world.free = free.iter().map(|sprite| tile_viewer::placed_sprite(compiled, sprite, Some(0), None)).collect();
                    world
                };
                let (split, plain, unsplit) = (world(&compiled, true), world(&nearest, true), world(&original, false));
                for mode in [tile_render::BackgroundSampling::Dither, tile_render::BackgroundSampling::Interpolate] { for (num, den) in [(1, 2), (2, 3), (3, 2)] {
                    let size = ((1024 * num + den - 1) / den, (768 * num + den - 1) / den);
                    let camera = tile_render::Camera { cells: false, hairline: true, zoomed_art: true, zoom: (num, den), background_sampling: mode, ..tile_render::Camera::new(size) };
                    let actual = tile_render::reference::render(&split, &compiled.atlas, &camera, 0, [11, 17, 23, 255]);
                    let ordinary = tile_render::reference::render(&plain, &nearest.atlas, &camera, 0, [11, 17, 23, 255]);
                    let unsplit = tile_render::reference::render(&unsplit, &original.atlas, &camera, 0, [11, 17, 23, 255]);
                    assert_background_pixels(&ordinary, &unsplit, &format!("complementary source masks changed nominal native pixels: {source}, opacity {opacity}, zoom {num}/{den}"));
                    let mut renderer = tile_render::TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, civ3_ui::viewer::SHADERS);
                    renderer.set_atlas(&gpu.device, &gpu.queue, &compiled.atlas);
                    renderer.set_world(&gpu.device, &split);
                    let gpu_image = gpu.render(&mut renderer, &camera, 0, [11, 17, 23, 255]);
                    let mut original_renderer = tile_render::TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, civ3_ui::viewer::SHADERS);
                    original_renderer.set_atlas(&gpu.device, &gpu.queue, &original.atlas);
                    let original_world = world(&original, false);
                    original_renderer.set_world(&gpu.device, &original_world);
                    let gpu_original = gpu.render(&mut original_renderer, &camera, 0, [11, 17, 23, 255]);
                    let mut changed = 0;
                    let mut gpu_changed = 0;
                    for y in 0..size.1 { for x in 0..size.0 {
                        let (sx, sy) = ((x * den / num) as i32, (y * den / num) as i32);
                        let inside_texture = texture.iter().any(|&[tx, ty, w, h]| sx >= tx && sy >= ty && sx - tx < w && sy - ty < h);
                        let overlay = (280..580).contains(&sx) && (180..250).contains(&sy);
                        if !inside_texture || overlay {
                            assert_eq!(actual.get(x, y), ordinary.get(x, y), "baked control or separate overlay changed: {source}, ({x}, {y})");
                            assert_eq!(gpu_image.get(x, y), gpu_original.get(x, y), "GPU changed nearest native controls/text: {source}, ({x}, {y})");
                        } else {
                            if actual.get(x, y) != ordinary.get(x, y) { changed += 1; }
                            if gpu_image.get(x, y) != gpu_original.get(x, y) { gpu_changed += 1; }
                            if opacity == 255 { assert_eq!(gpu_image.get(x, y), actual.get(x, y), "GPU and CPU {mode:?} background differ: {source}, zoom {num}/{den}, ({x}, {y})"); }
                        }
                    } }
                    assert!(changed > 200 && gpu_changed > 200, "native background picture left untreated: {source}, CPU{changed}/GPU{gpu_changed} changed pixels");
                    eprintln!("native mixed background {source}, {mode:?}, opacity {opacity}, zoom {num}/{den}: CPU{changed}/GPU{gpu_changed} changed texture pixels; controls/text exact");
                    if opacity == 255 && (num, den) == (1, 2) {
                        if let Some(directory) = std::env::var_os("C3R_BACKGROUND_EVIDENCE_DIR") {
                            let directory = std::path::PathBuf::from(directory);
                            std::fs::create_dir_all(&directory).unwrap();
                            let name = source.replace(['/', '\\', ' '], "_");
                            for (tag, image) in [(if mode == tile_render::BackgroundSampling::Dither { "split" } else { "split-interpolate" }, &gpu_image), ("original", &gpu_original)] {
                                image::save_buffer(directory.join(format!("{name}-{tag}.png")), image.as_bytes(), image.width, image.height, image::ColorType::Rgba8).unwrap();
                            }
                        }
                    }
                } }
                assert_eq!(items, before, "background drawing must not change captured input geometry");
                assert_eq!(art.to_lines(), art_before, "source identities and preload records stay unchanged");
            }
        }
    }

    #[test]
    fn tiny_background_masks_preserve_trimmed_nominal_pixels_at_fractional_phases_and_opacity() {
        struct Pixels;
        impl render_desc::ImageLoader for Pixels {
            fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
                Ok(vec![tile_render::RgbaImage::from_fn(40, 30, |x, y| [if (x + y) % 2 == 0 { 255 } else { 0 }, if (x + y) % 2 == 0 { 0 } else { 255 }, 255, if (x + 2 * y) % 5 == 0 { 0 } else { 117 }])])
            }
        }
        let gpu = tile_render::Headless::new().expect("GPU for tiny masked strips");
        for rectangle in [[9, 1, 1, 24], [2, 10, 32, 1], [9, 10, 1, 1]] {
            let policy = |_: &Region| vec![rectangle];
            let element = Element::OpacityImage { region: Region { source: "mixed.pcx".into(), rect: [2, 1, 32, 24], keyed: false, mask: None, palette: None }, at: (3, 5), clip: [4, 6, 33, 27], opacity: 137 };
            let mut art = ScreenArt::default();
            art.add(std::slice::from_ref(&element));
            let compile = |description: String| render_desc::Description::parse(&format!("grid = {{ layout = 'orthogonal', step = [1, 1] }}\n{description}")).unwrap().compile(&Pixels, 64).unwrap();
            let split = compile(art.description_with_backgrounds("", "", |_, _| String::new(), policy));
            let plain = compile(art.description("", "", |_, _| String::new()));
            assert!(split.atlas.sprites.iter().any(|sprite| sprite.sampling == 1 && sprite.blank & 0x7F7F_7F7F != 0), "must exercise real computed transparent margins");
            let world = |compiled: &Compiled, split: bool| {
                let free = if split { menu::screen::sprites_with_backgrounds(std::slice::from_ref(&element), compiled, u32::MAX, policy) } else { menu::screen::sprites(std::slice::from_ref(&element), compiled, u32::MAX) };
                let mut world = compiled.world(1, 1, false, false);
                world.free = free.iter().map(|sprite| tile_viewer::placed_sprite(compiled, sprite, Some(0), None)).collect();
                world
            };
            let (split_world, plain_world) = (world(&split, true), world(&plain, false));
            for mode in [tile_render::BackgroundSampling::Dither, tile_render::BackgroundSampling::Interpolate, tile_render::BackgroundSampling::Nearest] { for zoom in [(1, 2), (2, 3), (3, 7), (3, 11), (1, 8), (2, 1), (3, 2)] { for hairline in [false, true] {
                let camera = tile_render::Camera { origin: (2, 3), screen_shift: (2, 3), target_clip: Some([0, 0, 64, 48]), cells: false, hairline, zoom, background_sampling: mode, ..tile_render::Camera::new((64, 48)) };
                let expected = tile_render::reference::render(&split_world, &split.atlas, &camera, 0, [0, 0, 0, 255]);
                let nearest = tile_render::reference::render(&plain_world, &plain.atlas, &camera, 0, [0, 0, 0, 255]);
                let mut renderer = tile_render::TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, civ3_ui::viewer::SHADERS);
                renderer.set_atlas(&gpu.device, &gpu.queue, &split.atlas);
                renderer.set_world(&gpu.device, &split_world);
                let actual = gpu.render(&mut renderer, &camera, 0, [0, 0, 0, 255]);
                if mode == tile_render::BackgroundSampling::Interpolate {
                    // Means of 0/255 texels under source alpha 117: the
                    // reference's translucent blend rounds within one step.
                    let close = actual.pixels.iter().zip(&expected.pixels).all(|(a, b)| (0..4).all(|k| a[k].abs_diff(b[k]) <= 1));
                    assert!(close, "tiny mask reference mismatch beyond rounding: {mode:?}, {rectangle:?}, {zoom:?}, hairline {hairline}");
                } else {
                    assert_background_pixels(&actual, &expected, &format!("tiny mask reference mismatch: {mode:?}, {rectangle:?}, {zoom:?}, hairline {hairline}"));
                }
                if mode == tile_render::BackgroundSampling::Nearest { assert_background_pixels(&expected, &nearest, &format!("nearest background mode changed nominal pixels: {rectangle:?}, {zoom:?}, hairline {hairline}")); }
                for y in 0..48 { for x in 0..64 {
                    let sx = (((x as i64 - 2) * zoom.1 as i64).div_euclid(zoom.0 as i64) as i32 - 3).max(1) + 2;
                    let sy = (((y as i64 - 3) * zoom.1 as i64).div_euclid(zoom.0 as i64) as i32 - 5).max(1) + 1;
                    let [rx, ry, w, h] = rectangle;
                    let background = sx >= rx && sy >= ry && sx - rx < w && sy - ry < h;
                    if !background { assert_eq!(actual.get(x, y), nearest.get(x, y), "collapsed mask strip overwrote nearest UI at ({x},{y}), {rectangle:?}, {zoom:?}, hairline {hairline}"); }
                    let (a, b) = (actual.get(x, y), nearest.get(x, y));
                    assert_eq!((a[2], a[3]), (b[2], b[3]), "source alpha/uniform opacity was applied twice");
                } }
            } } }
        }
    }

    #[test]
    fn native_menu_background_capture_keeps_sampling_role_through_absolute_paths_and_canvas_copies() {
        use crate::ui::{Event, UiModel};
        struct Sizes;
        impl render_desc::ImageLoader for Sizes {
            fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
                panic!("layout metadata does not load source pixels")
            }
            fn sizes(&self, _: &str, _: &toml::Table) -> Result<Vec<(u32, u32)>, String> { Ok(vec![(1024, 768)]) }
        }
        // These source coordinates are real native load-menu/list repaint
        // crops. The captured absolute Wine path must reach the same role
        // as native preloaded art and the separately declared panel cover.
        let mut model = UiModel::new(vec![], vec![]);
        for event in [
            Event::Loaded { image: 10, path: r"Z:\Users\c3r\civ3r-runtime\game\Civilization III Complete\art\PopUpAndMenuBackground.pcx".into(), size: (1023, 327) },
            Event::Sliced { sprite: 20, image: 10, rect: [60, 44, 222, 283] },
            Event::Copied { sprite: 21, source: 20 },
            Event::Blit { sprite: 21, canvas: 40, x: 0, y: 0, clip: None, palette: None },
            Event::Copy { from: 40, src: [0, 22, 222, 18], canvas: 300, x: 529, y: 265, keyed: false },
            Event::Loaded { image: 11, path: r"C:\Games\Civ3\art\popupborders.pcx".into(), size: (1024, 768) },
            Event::Sliced { sprite: 22, image: 11, rect: [251, 1, 60, 44] },
            Event::Blit { sprite: 22, canvas: 300, x: 469, y: 199, clip: None, palette: None },
            Event::Sliced { sprite: 23, image: 11, rect: [265, 46, 46, 44] },
            Event::Blit { sprite: 23, canvas: 300, x: 483, y: 243, clip: None, palette: None },
            Event::Text { canvas: 300, run: TextRun::parse("text dc 0 at 540 280 color 0 0 0 font \"Lucida Sans\"/-14/0/0/0 str A separate native label").unwrap() },
            Event::PaintBegin,
            Event::Composite { canvas: 300, vtable: 1, origin: (0, 0), size: (1280, 800), clip: [0, 0, 1280, 800], keyed: false },
            Event::PaintEnd,
        ] { model.apply(event); }
        let captured = items(&model.frame(), &[], &[], 0);
        assert_eq!(captured.len(), 4);
        assert!(matches!(&captured[3], Item::Text(run, _, false) if run.text == "A separate native label"));
        let elements = captured.iter().filter_map(|item| match item { Item::Element(element, _) => Some(element.clone()), Item::Text(..) => None }).collect::<Vec<_>>();
        let mut art = ScreenArt::default();
        art.add(&elements);
        assert!(art.regions.iter().any(|region| region.source == "art/PopUpAndMenuBackground.pcx" && region.rect == [60, 66, 222, 18]));
        let source = format!("grid = {{ layout = 'orthogonal', step = [1, 1] }}\n{}", description(&art));
        let compiled = render_desc::Description::parse(&source).unwrap().compile_layout(&Sizes, 2048).unwrap();
        let draws = sprites(&captured[..3], &Fonts::default(), &compiled, 0, false);
        assert_eq!(draws.len(), 3, "parchment, frame and interior each keep their original geometry");
        for (draw, origin) in draws.iter().zip([(60, 66), (251, 1), (265, 46)]) {
            let set = tile_render::SpriteSetId::from_raw(draw.set);
            let id = compiled.sets.frame(set, 0).unwrap();
            let sprite = compiled.atlas.sprites[id.0 as usize];
            assert_eq!(sprite.sampling, 1, "every captured background piece, including the native frame");
            assert_eq!((sprite.sampling_origin_x, sprite.sampling_origin_y), origin);
        }
        assert_eq!((draws[0].x, draws[0].y), (529, 265));
        assert_eq!((draws[1].x, draws[1].y), (469, 199));
    }

    #[test]
    fn preloaded_native_background_frames_panels_and_pictures_are_tagged() {
        let art = crate::preload::ArtList::parse(crate::preload::BUILTIN_LIST).screen;
        let table: toml::Table = description(&art).parse().unwrap();
        let images = table["images"].as_table().unwrap();
        let sheets = table["sheets"].as_table().unwrap();
        let tagged = |source: &str, rect: [i64; 4]| {
            sheets.values().any(|sheet| {
                let path = images[sheet["image"].as_str().unwrap()]["source"].as_str().unwrap();
                let Some(origin) = sheet.get("origin").and_then(toml::Value::as_array) else { return false };
                let cell = sheet["cell"].as_array().unwrap();
                path.eq_ignore_ascii_case(source) && [origin[0].as_integer().unwrap(), origin[1].as_integer().unwrap(), cell[0].as_integer().unwrap(), cell[1].as_integer().unwrap()] == rect
                    && sheet.get("sampling").and_then(toml::Value::as_str) == Some("ui-background")
            })
        };
        // These used to be completely nearest, or only partly tagged.
        for (source, rect) in [
            ("art/x_title.pcx", [0, 0, 1024, 768]),
            ("art/popupandmenubackground.pcx", [60, 44, 222, 283]),
            ("art/popupborders.pcx", [251, 1, 60, 44]),
            ("art/popupborders.pcx", [375, 132, 60, 3]),
            ("art/popupborders.pcx", [432, 46, 3, 44]),
            ("art/advisors/culture.pcx", [0, 0, 1024, 768]),
            ("art/advisors/military.pcx", [66, 156, 958, 612]),
            ("art/advisors/dialogbox.pcx", [0, 0, 206, 132]),
            ("art/diplomacy/talk_offer.pcx", [297, 50, 438, 517]),
        ] { assert!(tagged(source, rect), "missing complete background piece: {source} {rect:?}"); }
        for source in ["art/interface/menuButtons.pcx", "art/x_title buttons.pcx", "art/civilopedia/icons/unitsmall.pcx", "art/exitBox-backgroundStates.pcx"] {
            let found: Vec<_> = sheets.values().filter(|sheet| images[sheet["image"].as_str().unwrap()]["source"].as_str().unwrap().eq_ignore_ascii_case(source)).collect();
            assert!(!found.is_empty(), "must exercise actual preloaded foreground art: {source}");
            assert!(found.iter().all(|sheet| sheet.get("sampling").is_none()), "foreground received filtering: {source}");
        }
        let builtin = render_desc::Description::parse(crate::assets::BUILTIN_DESC).unwrap();
        assert_eq!(builtin.sheets["title"].sampling, render_desc::Sampling::UiBackground, "the standalone title uses the same role as its captured copies");
    }

    #[test]
    fn every_composed_popup_piece_uses_the_selected_mode_at_small_and_large_scales() {
        struct Checker;
        impl render_desc::ImageLoader for Checker {
            fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
                Ok(vec![tile_render::RgbaImage::from_fn(1024, 768, |x, y| {
                    let v = if (x + y) % 2 == 0 { 0 } else { 255 };
                    [v, v, v, 255]
                })])
            }
        }
        let clip = [0, 0, 60, 60];
        let image = |source: &str, rect, at| Element::Image { region: Region { source: source.into(), rect, keyed: true, mask: None, palette: None }, at, clip };
        let elements = vec![
            image("Art/PopUpandMenuBackground.pcx", [0, 0, 48, 48], (6, 6)),
            image("Art/popupborders.pcx", [251, 1, 48, 6], (6, 0)),
            image("Art/popupborders.pcx", [251, 129, 48, 6], (6, 54)),
            image("Art/popupborders.pcx", [251, 46, 6, 48], (0, 6)),
            image("Art/popupborders.pcx", [429, 46, 6, 48], (54, 6)),
            image("Art/Advisors/dialogbox.pcx", [0, 0, 24, 24], (18, 18)),
            image("Art/interface/menuButtons.pcx", [1, 1, 12, 12], (24, 24)),
        ];
        let mut art = ScreenArt::default(); art.add(&elements);
        let compiled = render_desc::Description::parse(&format!("grid = {{ layout = 'orthogonal', step = [1, 1] }}\n{}", description(&art))).unwrap().compile(&Checker, 256).unwrap();
        let items: Vec<_> = elements.into_iter().map(|e| Item::Element(e, false)).collect();
        let free = sprites(&items, &Fonts::default(), &compiled, 0, false);
        assert_eq!(free.len(), 7);
        let mut world = compiled.world(1, 1, false, false);
        world.free = free.iter().map(|s| tile_viewer::placed_sprite(&compiled, s, Some(0), None)).collect();
        let gpu = tile_render::Headless::new().expect("RADV for composed popup sampling");
        let adapter = gpu.adapter.get_info();
        assert_eq!(adapter.vendor, 0x1002); assert_eq!(adapter.backend, wgpu::Backend::Vulkan);
        let mut renderer = tile_render::TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, civ3_ui::viewer::SHADERS);
        renderer.set_atlas(&gpu.device, &gpu.queue, &compiled.atlas); renderer.set_world(&gpu.device, &world);
        for (num, den) in [(1, 2), (2, 3), (1, 1), (3, 2), (2, 1), (3, 1)] {
            let camera = tile_render::Camera { cells: false, hairline: true, zoom: (num, den), background_sampling: tile_render::BackgroundSampling::Nearest, ..tile_render::Camera::new((60 * num / den, 60 * num / den)) };
            let nearest = gpu.render(&mut renderer, &camera, 0, [17, 23, 31, 255]);
            for mode in [tile_render::BackgroundSampling::Nearest, tile_render::BackgroundSampling::Dither, tile_render::BackgroundSampling::Interpolate] {
                let camera = tile_render::Camera { background_sampling: mode, ..camera };
                let actual = gpu.render(&mut renderer, &camera, 0, [17, 23, 31, 255]);
                let expected = tile_render::reference::render(&world, &compiled.atlas, &camera, 0, [17, 23, 31, 255]);
                assert_background_pixels(&actual, &expected, &format!("composed popup {mode:?}, zoom {num}/{den}"));
                let mut changed = [0; 6];
                for y in 0..actual.height { for x in 0..actual.width {
                    let (sx, sy) = (x * den / num, y * den / num);
                    if (24..36).contains(&sx) && (24..36).contains(&sy) {
                        assert_eq!(actual.get(x, y), nearest.get(x, y), "foreground control changed");
                        continue;
                    }
                    let piece = if sy < 6 { 1 } else if sy >= 54 { 2 } else if sx < 6 { 3 } else if sx >= 54 { 4 }
                        else if (18..42).contains(&sx) && (18..42).contains(&sy) { 5 } else { 0 };
                    changed[piece] += usize::from(actual.get(x, y) != nearest.get(x, y));
                } }
                if num == den || mode == tile_render::BackgroundSampling::Nearest {
                    assert_eq!(changed, [0; 6], "native size and explicit nearest must stay exact");
                } else {
                    assert!(changed.iter().all(|&n| n > 0), "{mode:?}, zoom {num}/{den}: untreated background pieces {changed:?}");
                }
            }
        }
    }

    /// A resized draw of a file the game replaced indices in (Territory's
    /// border pieces, made transparent) draws the replaced indices.
    #[test]
    fn a_resized_draw_keeps_the_file_s_index_replacements() {
        let pixels = std::sync::Arc::new(civ3_ui::selection::FilePixels { size: (4, 4), row: 4, indices: vec![0; 16], palette: [[0; 3]; 256] });
        let change = menu::screen::IndexChange { rect: [0, 0, 4, 4], from: 0, to: 255 };
        let state = civ3_ui::ui::FileState::new(pixels).replaced(change.clone());
        let frame = Frame {
            draws: vec![ScreenDraw::ResizedImage { retained: Some(state),
                path: "C:/Civ3/Art/Terrain/Territory.pcx".into(), rect: [0, 0, 4, 4], x: 10, y: 20,
                size: (8, 8), sample_step: [32768, 32768], keyed: true, clip: [0, 0, 100, 100],
                mask: None, palette: None, opacity: 255,
            }],
            ..Default::default()
        };
        let elements = items(&frame, &[PathBuf::from("C:/Civ3")], &[], 0);
        let [Item::Element(Element::Indexed { region, indices, at: (10, 20), .. }, false)] = elements.as_slice() else { panic!("{elements:?}") };
        assert_eq!(region.source, "Art/Terrain/Territory.pcx");
        assert_eq!(indices.index_changes, vec![change]);
        assert_eq!(indices.transparent, vec![254, 255]);
        assert_eq!(indices.resize, Some(menu::screen::IndexedResize { size: (8, 8), sample_step: [32768, 32768] }));
    }

    #[test]
    fn resized_screen_art_keeps_native_sampling_mask_palette_and_underlay_order() {
        let palette = menu::screen::PaletteFrom { image: "Art/colors.pcx".into(), first: 16, count: 32 };
        let frame = Frame {
            draws: vec![ScreenDraw::ResizedImage { retained:None,
                path: "C:/Civ3/Art/resource.pcx".into(), rect: [8, 4, 9, 5], x: 100, y: 50,
                size: (5, 3), sample_step: [117964, 109226], keyed: true, clip: [102, 51, 105, 53],
                mask: Some(crate::ui::Mask { retained:None,premultiplied:false,path: "C:/Civ3/Art/mask.pcx".into(), at: (11, 8) }),
                palette: Some(palette.clone()), opacity: 128,
            }],
            underlay_draws: vec![0..1],
            ..Default::default()
        };
        let elements = items(&frame, &[PathBuf::from("C:/Civ3")], &[], 0);
        assert_eq!(elements, vec![Item::Element(Element::ResizedImage {
            region: Region {
                source: "Art/resource.pcx".into(), rect: [8, 4, 9, 5], keyed: true,
                mask: Some(("Art/mask.pcx".into(), (3, 4))), palette: Some(palette),
            },
            at: (100, 50), size: (5, 3), sample_step: [117964, 109226], clip: [102, 51, 105, 53], opacity: 128,
        }, true)]);
        let mut art = ScreenArt::default();
        let Item::Element(element, _) = &elements[0] else { panic!() };
        assert!(art.add(std::slice::from_ref(element)));
        let description = description(&art);
        assert!(description.contains("resize = [5, 3]\nresize_step = [117964, 109226]"));
        assert!(description.contains("transparent = [254, 255]"));
        assert!(description.contains("palette_range = [16, 32]"));
        assert_eq!(ScreenArt::parse_lines(&art.to_lines()), art);
    }

    #[test]
    fn copied_resized_icons_compile_with_the_same_native_source_samples() {
        use crate::ui::{Event, UiModel};
        struct Pixels;
        impl render_desc::ImageLoader for Pixels {
            fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
                Ok(vec![tile_render::RgbaImage::from_fn(32, 32, |x, y| [x as u8, y as u8, 0, 255])])
            }
        }
        let mut model = UiModel::new(vec![], vec![]);
        model.apply(Event::Loaded { image: 10, path: "Art/resource.pcx".into(), size: (32, 32) });
        model.apply(Event::Sliced { sprite: 20, image: 10, rect: [8, 4, 9, 5] });
        model.apply(Event::ResizedBlit { sprite: 20, canvas: 40, x: 2, y: 3, size: (5, 3), sample_step: [117964, 109226], clip: None, palette: None, indices: None });
        model.apply(Event::Copy { from: 40, src: [4, 4, 2, 2], canvas: 300, x: 20, y: 30, keyed: false });
        model.apply(Event::PaintBegin);
        model.apply(Event::Composite { canvas: 300, vtable: 1, origin: (100, 200), size: (100, 100), clip: [100, 200, 200, 300], keyed: false });
        model.apply(Event::PaintEnd);
        let items = items(&model.frame(), &[], &[], 0);
        let mut art = ScreenArt::default();
        let Item::Element(element, _) = &items[0] else { panic!() };
        art.add(std::slice::from_ref(element));
        let compiled = render_desc::Description::parse(&format!("grid = {{ layout = \"orthogonal\", step = [1, 1] }}\n{}", description(&art))).unwrap().compile(&Pixels, 256).unwrap();
        let draws = sprites(&items, &Fonts::default(), &compiled, 0, false);
        assert_eq!(draws.len(), 1);
        let draw = &draws[0];
        assert_eq!((draw.x, draw.y, draw.clip), (118, 229, Some([120, 230, 122, 232])));
        let sprite = compiled.atlas.sprites[0];
        let page = &compiled.atlas.pages[sprite.page as usize];
        // The copied window starts at destination pixel (2, 1) of the
        // original resize. Its sampling origin must survive the copy.
        let mut shown = Vec::new();
        for y in 230..232 {
            for x in 120..122 {
                shown.push(page.get(sprite.atlas_x + (x - draw.x) as u32, sprite.atlas_y + (y - draw.y) as u32));
            }
        }
        assert_eq!(shown, [[11, 5, 0, 255], [13, 5, 0, 255], [11, 7, 0, 255], [13, 7, 0, 255]]);
    }

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

/// The interface's minimap for the renderer (`scene::Scene::minimap`): its
/// picture's place and visible part on the screen, and the world pixels
/// it spans. The game maps it linearly (§17.7, as `minimap::view_rect`):
/// tile x·w/width, y·h/(height − 1), a tile's footprint starting at
/// (64·x, 32·y) world pixels (half that zoomed out).
pub fn minimap_area(items: &[Item], world: &crate::world::Civ3World, zoomed_out: bool) -> Option<scene::Minimap> {
    let (width, height) = (world.width, world.height);
    if width <= 0 || height <= 1 {
        return None;
    }
    let (sx, sy) = if zoomed_out { (32, 16) } else { (64, 32) };
    items.iter().find_map(|i| {
        let Item::Element(Element::Dynamic { name, size, rect: [rx, ry, rw, rh], at: (x, y), clip }, _) = i else { return None };
        if name != "minimap" || size.0 == 0 || size.1 == 0 {
            return None;
        }
        // (Its drawn part, inside the clip.)
        let (left, top) = ((*x).max(clip[0]), (*y).max(clip[1]));
        let (right, bottom) = ((x + rw).min(clip[2]), (y + rh).min(clip[3]));
        (right > left && bottom > top).then_some(scene::Minimap {
            at: (x - rx, y - ry),
            size: *size,
            shown: [left, top, right - left, bottom - top],
            world: (width * sx, (height - 1) * sy),
        })
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
    // The game's rectangle drawer gives each corner to just one edge.
    // Its recorded fills are therefore shorter than view_rect's spans,
    // and partial canvas copies can split them further. Match contained
    // line segments, only in the white-fill run just after this minimap.
    let marks: Vec<usize> = items[at + 1..]
        .iter()
        .take_while(|i| matches!(i, Item::Element(Element::Fill { rgb: [255, 255, 255], alpha: 255, .. }, _)))
        .enumerate()
        .filter_map(|(offset, i)| {
            let Item::Element(Element::Fill { rect: [x, y, w, h], .. }, _) = i else { return None };
            let on_line = *w > 0 && *h > 0 && lines.iter().any(|[lx, ly, lw, lh]| x >= lx && y >= ly && x + w <= lx + lw && y + h <= ly + lh);
            on_line.then_some(at + 1 + offset)
        })
        .collect();
    if marks.is_empty() {
        // (Not drawn: nothing to replace.)
        return;
    }
    for i in marks.into_iter().rev() {
        items.remove(i);
    }
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
        .filter_map(|d| {
            let (path, x, y, w, h, clip) = match d {
                ScreenDraw::Image { path, rect, x, y, clip, .. } => (path, *x, *y, rect[2], rect[3], *clip),
                ScreenDraw::ResizedImage { path, x, y, size, clip, .. } => (path, *x, *y, size.0, size.1, *clip),
                _ => return None,
            };
            if !lower(path).ends_with("popheads.pcx") && !lower(path).ends_with("units_32.pcx") {
                return None;
            }
            let tag = if lower(path).ends_with("popheads.pcx") { 0x0C00_0000 } else { 0x0D00_0000 };
            let [cl, ct, cr, cb] = clip;
            let (l, t) = (x.max(cl), y.max(ct));
            let (r, b) = ((x + w).min(cr), (y + h).min(cb));
            (r > l && b > t).then(|| scene::Interactable { region: None, alt_right_click: false, anchor: None,
                rect: [l, t, r - l, b - t],
                kind: scene::InteractableKind::Button,
                id: (*canvas as u64) << 32 | tag | ((l as u32 & 0xFFF) << 12) as u64 | (t as u32 & 0xFFF) as u64,
            })
        })
        .collect()
}

/// Unit icons which Main_GUI's status-panel click handler selects. These
/// have no Button forms. Use only images in its real native hit strip,
/// not decorative unit art elsewhere. Adjacent icons can overlap: each
/// target stops at the next icon's x, as the game's column hit test does.
pub fn status_unit_targets(frame: &Frame, row: Option<(u32, [i32; 4])>) -> Vec<scene::Interactable> {
    let Some((canvas, [rx, ry, rw, rh])) = row else { return Vec::new() };
    let Some((_, range)) = frame.form_draws.iter().find(|(c, _)| *c == canvas) else { return Vec::new() };
    let mut icons: Vec<([i32; 4], [i32; 4])> = frame.draws[range.clone()].iter().filter_map(|d| {
        let (path, x, y, w, h, clip) = match d {
            ScreenDraw::Image { path, rect: [_, _, 32, 32], x, y, clip, .. } => (path, *x, *y, 32, 32, *clip),
            ScreenDraw::ResizedImage { path, x, y, size: (w, h), clip, .. } if (1..=32).contains(w) && (1..=32).contains(h) => (path, *x, *y, *w, *h, *clip),
            _ => return None,
        };
        if !path.to_ascii_lowercase().replace('/', "\\").ends_with("\\units_32.pcx") {
            return None;
        }
        let [cl, ct, cr, cb] = clip;
        let (l, t) = (x.max(rx).max(cl), y.max(ry).max(ct));
        let (r, b) = ((x + w).min(rx + rw).min(cr), (y + h).min(ry + rh).min(cb));
        (r > l && b > t).then_some(([x, t, w, b - t], clip))
    }).collect();
    icons.sort_unstable_by_key(|(r, _)| r[0]);
    icons.dedup();
    let first = icons.first().map_or(rx, |(r, _)| r[0]);
    icons.iter().enumerate().filter_map(|(k, (r, clip))| {
        let stride = icons.get(k + 1).map(|(next, _)| next[0] - r[0])
            .or_else(|| k.checked_sub(1).map(|previous| r[0] - icons[previous].0[0]))
            .unwrap_or(r[2]).clamp(1, r[2]);
        // The game draws the first image at L+42 but starts hit columns
        // at L+27. Preserve its native column calculation, especially in
        // dense cargo rows; clicking the image's centre can choose a later
        // unit there. No guessed unit action or shortcut is substituted.
        let x = rx + r[0] - first;
        let (l, t) = (x.max(rx).max(clip[0]), r[1].max(ry).max(clip[1]));
        let (right, bottom) = ((x + stride).min(rx + rw).min(clip[2]), (r[1] + r[3]).min(ry + rh).min(clip[3]));
        (right > l && bottom > t).then_some(scene::Interactable { region: None, alt_right_click: false, anchor: None,
            rect: [l, t, right - l, bottom - t], kind: scene::InteractableKind::Button,
            id: (canvas as u64) << 32 | 0x0E00_0000 | k as u64 })
    }).collect()
}

/// The game's controls (`civ3_sys::controls`) as things to point at in the
/// viewer (button snapping): the game's client pixels are the driver's.
pub fn interactables(controls: &[civ3_sys::controls::Control]) -> Vec<scene::Interactable> {
    use civ3_sys::controls::ControlKind;
    use scene::InteractableKind;
    controls
        .iter()
        .map(|c| scene::Interactable { region: None, alt_right_click: c.alt_right_click(), anchor: None,
            rect: c.rect,
            kind: match c.kind {
                ControlKind::Button => InteractableKind::Button,
                ControlKind::ListItem => InteractableKind::ListItem,
                ControlKind::Slider { track, value } => InteractableKind::Slider { track, value },
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
    fn the_real_games_corner_split_lines_are_replaced() {
        // Captured at 1920x1080: the game's rectangle drawer assigns each
        // corner to one edge, unlike view_rect's overlapping edge spans.
        let world = crate::world::Civ3World { width: 60, height: 60, flags: 5, wrap_x: true, tiles: vec![Default::default(); 1800], ..Default::default() };
        let clip = [0, 0, 1920, 1080];
        let minimap = Item::Element(Element::Dynamic { name: "minimap".into(), size: (230, 108), rect: [0, 0, 230, 108], at: (31, 961), clip }, false);
        let fill = |rect| Item::Element(Element::Fill { rect, rgb: [255, 255, 255], alpha: 255, clip }, false);
        let game_lines = [[81, 970, 122, 1], [80, 1033, 122, 1], [80, 970, 1, 63], [202, 971, 1, 63]];
        // A matching white decoration before the minimap is not its mark.
        let decoration = fill(game_lines[0]);
        let mut items = vec![decoration.clone(), minimap.clone()];
        items.extend(game_lines.map(fill));
        // The frame ends the minimap's mark, even if a later control uses
        // a white fill with the same rectangle.
        let frame = Item::Element(Element::Fill { rect: [30, 960, 232, 1], rgb: [200, 160, 80], alpha: 255, clip }, false);
        items.extend([frame.clone(), decoration.clone()]);
        let shown = (1376, 196, 960, 1080);
        replace_game_view_mark(&mut items, &world, (13, 5, 45, 40), shown, false);
        let mut expected = vec![minimap];
        mark_views(&mut expected, &world, &[shown], false, [255, 255, 255]);
        expected.insert(0, decoration.clone());
        expected.extend([frame, decoration]);
        assert_eq!(items, expected);
    }

    #[test]
    fn a_mark_split_into_smaller_fills_is_replaced_after_wrapping() {
        let world = crate::world::Civ3World { width: 40, height: 40, flags: 3, wrap_x: true, wrap_y: true, tiles: vec![Default::default(); 800], ..Default::default() };
        let clip = [0, 0, 1920, 1080];
        let minimap = Item::Element(Element::Dynamic { name: "minimap".into(), size: (200, 100), rect: [0, 0, 200, 100], at: (30, 960), clip }, false);
        let game_tiles = (35, 35, 5, 5);
        let mut items = vec![minimap.clone()];
        for [x, y, w, h] in crate::minimap::view_rect(&world, (200, 100), game_tiles) {
            for (offset, len) in pieces(w.max(h)) {
                let rect = if w >= h { [30 + x + offset, 960 + y, len, 1] } else { [30 + x, 960 + y + offset, 1, len] };
                items.push(Item::Element(Element::Fill { rect, rgb: [255, 255, 255], alpha: 255, clip }, false));
            }
        }
        let shown = (64 * 38, 32 * 38, 64 * 4, 32 * 4);
        replace_game_view_mark(&mut items, &world, game_tiles, shown, false);
        let mut expected = vec![minimap];
        mark_views(&mut expected, &world, &[shown], false, [255, 255, 255]);
        assert_eq!(items, expected);
    }

    #[test]
    fn the_minimap_area_points_where_the_game_puts_its_tiles() {
        let world = crate::world::Civ3World { width: 40, height: 40, tiles: vec![Default::default(); 800], ..Default::default() };
        // Its left 50 pixels clipped away.
        let minimap = Item::Element(Element::Dynamic { name: "minimap".into(), size: (200, 100), rect: [0, 0, 200, 100], at: (30, 960), clip: [80, 0, 1920, 1080] }, false);
        let items = vec![minimap];
        let area = minimap_area(&items, &world, false).unwrap();
        assert_eq!(area, scene::Minimap { at: (30, 960), size: (200, 100), shown: [80, 960, 150, 100], world: (40 * 64, 39 * 32) });
        for tile in [(10, 10), (21, 5), (38, 38), (30, 20)] {
            let p = minimap_point(&items, &world, tile).unwrap();
            let w = area.world_at(p).unwrap();
            // Within a tile of where its footprint starts (64·x, 32·y).
            assert!((w.0 - 64 * tile.0).abs() <= 64 && (w.1 - 32 * tile.1).abs() <= 32, "{tile:?}: {p:?} -> {w:?}");
        }
        // Clipped away: not pointed at. Zoomed out: half the world pixels.
        assert_eq!(area.world_at((60, 1000)), None);
        assert_eq!(minimap_area(&items, &world, true).unwrap().world, (40 * 32, 39 * 16));
        assert_eq!(minimap_area(&[], &world, false), None);
    }

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
        let image = |path: &str, rect: [i32; 4], x: i32, y: i32, clip: [i32; 4]| ScreenDraw::Image { retained:None,path: path.into(), rect, x, y, keyed: true, clip, mask: None, palette: None, opacity: 255 };
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
        let mut scaled = frame.clone();
        scaled.draws.push(ScreenDraw::ResizedImage { retained:None,
            path: "Art/Units/units_32.pcx".into(), rect: [1, 1, 49, 32], x: 800, y: 885,
            size: (31, 20), sample_step: [100352, 100352], keyed: true, clip: [805, 880, 820, 900],
            mask: None, palette: None, opacity: 255,
        });
        scaled.form_draws[1].1.end += 1;
        let targets = super::city_drawn_targets(&scaled, None);
        assert_eq!(targets.len(), 4);
        assert_eq!(targets[3].rect, [805, 885, 15, 15], "scaled unit targets use destination pixels and clipping");
        assert!(super::city_drawn_targets(&scaled, Some(0x9B7784)).is_empty());
    }

    #[test]
    fn controls_become_interactables() {
        let controls = [
            Control { rect: [1, 2, 3, 4], kind: ControlKind::Button, id: 7 },
            Control { rect: [0, 0, 100, 200], kind: ControlKind::List { scroll: [0, 0, 100, 200], step: 20 }, id: 8 },
            Control { rect: [0, 0, 100, 20], kind: ControlKind::ListItem, id: 9 },
            Control { rect: [110, 70, 406, 20], kind: ControlKind::Slider { track: [139, 80, 350, 1], value: 322_835 }, id: 10 },
        ];
        assert_eq!(super::interactables(&controls), [
            Interactable { region: None, alt_right_click: false, anchor: None, rect: [1, 2, 3, 4], kind: InteractableKind::Button, id: 7 },
            Interactable { region: None, alt_right_click: false, anchor: None, rect: [0, 0, 100, 200], kind: InteractableKind::List { scroll: [0, 0, 100, 200], step: 20 }, id: 8 },
            Interactable { region: None, alt_right_click: false, anchor: None, rect: [0, 0, 100, 20], kind: InteractableKind::ListItem, id: 9 },
            Interactable { region: None, alt_right_click: false, anchor: None, rect: [110, 70, 406, 20], kind: InteractableKind::Slider { track: [139, 80, 350, 1], value: 322_835 }, id: 10 },
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
            Ok(vec![tile_render::RgbaImage::from_fn(128, 128, |_, _| [255, 0, 0, 255])])
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
    fn minimap_mode_changes_replace_pixels_without_new_art_or_ui_draws() {
        use civ3_sys::ui_hooks::NavigatorMode;
        use crate::world::{Civ3Tile, Civ3World};
        let mut world = Civ3World {
            width: 8, height: 8, flags: 1, wrap_x: true, player: 1,
            color_tables: vec![0, 1],
            tiles: vec![Civ3Tile { square_type: 2 << 12, owner: 1, explored: 2, ..Default::default() }; 32],
            ..Default::default()
        };
        let items: Vec<_> = ["minimap", "minimap-advisor"].into_iter().map(|name| Item::Element(Element::Dynamic {
            name: name.into(), size: (32, 16), rect: [0, 0, 32, 16], at: (0, 0), clip: [0, 0, 1920, 1080],
        }, false)).collect();
        let source = format!("grid = {{ layout = \"orthogonal\", step = [1, 1] }}\n{}", art(&items));
        let compiled = render_desc::Description::parse(&source).unwrap().compile(&Images, 256).unwrap();
        let patches = |world: &Civ3World| minimap_patches(&items, world, &compiled, |_| [231, 8, 8]);
        let territory = patches(&world);
        assert_eq!(territory.len(), 2);
        let before = world.clone();
        world.navigator_modes.main = NavigatorMode::Geography;
        assert_ne!(world, before, "a mode switch must defeat the publisher's unchanged-world check");
        let geography = patches(&world);
        assert_ne!(geography[0].rgba, territory[0].rgba);
        assert_eq!(geography[1], territory[1], "the advisor keeps its independent mode");
        world.navigator_modes.main = NavigatorMode::Territory;
        assert_eq!(patches(&world), territory);
        world.navigator_modes.advisor = NavigatorMode::Geography;
        let advisor = patches(&world);
        assert_eq!(advisor[0], territory[0]);
        assert_eq!(advisor[1].rgba, geography[0].rgba);
    }

    #[test]
    fn a_new_world_notice_plate_needs_no_art_batch() {
        let compiled = render_desc::Description::parse("grid = { layout = \"orthogonal\", step = [1, 1] }").unwrap().compile_layout(&Images, 256).unwrap();
        let mut fonts = Fonts::default();
        let notices = [Item::Element(Element::Fill {
            rect: [2306, 1328, 120, 14], rgb: [0, 8, 0], alpha: 153,
            clip: [-(1 << 24), -(1 << 24), 1 << 24, 1 << 24],
        }, false)];
        let mandatory = mandatory_items([&[]; 6], &notices);
        let mut screen_art = ScreenArt::default();
        assert!(!add_art(mandatory.clone(), &mut screen_art, &mut fonts, &civ3_assets::GameFiles::with_roots(Vec::new())), "a solid plate cannot request sprite pixels");
        assert!(matches!(publication(mandatory, &fonts, &compiled, true, (1920, 1080)), Publication::Complete));
        let commands = sprites(&notices, &fonts, &compiled, u32::MAX, false);
        assert_eq!(commands.len(), 1);
        assert_eq!((commands[0].x, commands[0].y), (2306, 1328));
        assert_eq!(commands[0].solid, Some(scene::SolidRect { width: 120, height: 14, rgba: [0, 8, 0, 153] }));
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
            assert_eq!(readiness(items.iter(), &fonts, &compiled, false), Readiness::Unavailable, "{source}");
        }
    }

    /// A loader whose first load of `flaky.pcx` fails (an addition lost).
    struct Flaky(std::sync::atomic::AtomicU32);
    impl render_desc::ImageLoader for Flaky {
        fn load(&self, name: &str, options: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
            if name == "flaky.pcx" && self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 { return Err("read failed".into()) }
            render_desc::ImageLoader::load(&Images, name, options)
        }
    }

    fn placed(source: &str, at: (i32, i32)) -> Item {
        Item::Element(Element::Image {
            region: Region { source: source.into(), rect: [0, 0, 16, 16], keyed: false, mask: None, palette: None },
            at, clip: [0, 0, 1920, 1080],
        }, false)
    }

    /// Art folders with these (empty) files.
    fn folder(name: &str, files: &[&str]) -> (PathBuf, civ3_assets::GameFiles) {
        let dir = std::env::temp_dir().join(format!("c3r-ui-art-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in files { std::fs::write(dir.join(f), b"").unwrap(); }
        (dir.clone(), civ3_assets::GameFiles::with_roots(vec![dir]))
    }

    fn sets(sprites: &[scene::FreeSprite]) -> Vec<u32> {
        sprites.iter().map(|s| s.set).collect()
    }

    #[test]
    fn a_non_resident_image_is_drawn_as_nothing_and_loaded_on_demand_until_it_appears() {
        use crate::art_schedule::{ArtSchedule, Timing};
        use std::time::{Duration, Instant};
        let (dir, files) = folder("on-demand", &["shown.pcx", "late.pcx"]);
        let (mut compiled, mut growing) = render_desc::Description::parse("grid = { layout = \"orthogonal\", step = [1, 1] }").unwrap().compile_growing(&Images, 256, false).unwrap();
        let mut fonts = Fonts::default();
        let (mut screen_art, mut absent) = (ScreenArt::default(), AbsentArt::default());
        // A screen whose background is resident already.
        let shown = [placed("shown.pcx", (0, 0))];
        assert!(add_present_art(&shown, &mut screen_art, &mut fonts, &files, &mut absent));
        compiled.apply(&growing.grow(&compiled, &description(&screen_art), &Images).unwrap());
        let mut described = screen_art.clone();
        let resident = sets(&sprites(&shown, &fonts, &compiled, u32::MAX, false));
        assert_eq!(resident.len(), 1);
        // The next screen adds art that is not resident: requested at once.
        let items = [placed("shown.pcx", (0, 0)), placed("late.pcx", (40, 40))];
        let mut schedule = ArtSchedule::new(Timing::GROWING);
        let now = Instant::now();
        let wanted = add_present_art(&items, &mut screen_art, &mut fonts, &files, &mut absent);
        assert!(wanted, "a new region is an on-demand request");
        schedule.want(now, wanted);
        // Published at once: the resident art only, nothing in place of the
        // other (no generated sprite, no substitute).
        assert_eq!(publication(items.iter(), &fonts, &compiled, schedule.screen_pending(), (1920, 1080)), Publication::Partial { pending: true });
        assert_eq!(sets(&sprites(&items, &fonts, &compiled, u32::MAX, false)), resident);
        // The load completes; the next frame shows it.
        let token = schedule.start(now + Duration::from_millis(40)).unwrap();
        let addition = description(&screen_art.minus(&described));
        described = screen_art.clone();
        compiled.apply(&growing.grow(&compiled, &addition, &Images).unwrap());
        schedule.done(now + Duration::from_millis(60), token);
        assert_eq!(publication(items.iter(), &fonts, &compiled, schedule.screen_pending(), (1920, 1080)), Publication::Complete);
        assert_eq!(sprites(&items, &fonts, &compiled, u32::MAX, false).len(), 2);
        assert_eq!(described, screen_art);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn art_whose_load_failed_is_asked_for_again_and_then_appears() {
        use std::time::Instant;
        let (dir, files) = folder("retry", &["flaky.pcx"]);
        let loader = Flaky(Default::default());
        let (mut compiled, mut growing) = render_desc::Description::parse("grid = { layout = \"orthogonal\", step = [1, 1] }").unwrap().compile_growing(&loader, 256, false).unwrap();
        let mut fonts = Fonts::default();
        let (mut screen_art, mut absent, mut retry) = (ScreenArt::default(), AbsentArt::default(), ArtRetry::default());
        let items = [placed("flaky.pcx", (0, 0))];
        add_present_art(&items, &mut screen_art, &mut fonts, &files, &mut absent);
        let mut described = screen_art.clone();
        assert!(growing.grow(&compiled, &description(&screen_art), &loader).is_err(), "the first addition is lost");
        // Nothing pending any more, the art is not there: asked for again.
        assert_eq!(publication(items.iter(), &fonts, &compiled, false, (1920, 1080)), Publication::Partial { pending: false });
        let missing = unready_art(items.iter(), &compiled);
        assert!(retry.would_ask(&missing));
        let now = Instant::now();
        let again = retry.due(&missing, now);
        assert_eq!(again, screen_art);
        assert!(retry.due(&missing, now).is_empty(), "not again before the spacing");
        described = described.minus(&again);
        compiled.apply(&growing.grow(&compiled, &description(&screen_art.minus(&described)), &loader).unwrap());
        assert_eq!(publication(items.iter(), &fonts, &compiled, false, (1920, 1080)), Publication::Complete);
        assert!(unready_art(items.iter(), &compiled).is_empty());
        // Asked for a bounded number of times.
        let mut later = now;
        for _ in 1..ArtRetry::ATTEMPTS {
            later += ArtRetry::SPACING;
            assert!(!retry.due(&missing, later).is_empty());
        }
        assert!(retry.due(&missing, later + ArtRetry::SPACING).is_empty() && !retry.would_ask(&missing));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn files_in_no_art_folder_are_never_described_and_draw_nothing() {
        let (dir, files) = folder("absent", &["here.pcx"]);
        let (mut compiled, mut growing) = render_desc::Description::parse("grid = { layout = \"orthogonal\", step = [1, 1] }").unwrap().compile_growing(&Images, 256, false).unwrap();
        let mut fonts = Fonts::default();
        let (mut screen_art, mut absent) = (ScreenArt::default(), AbsentArt::default());
        let items = [placed("here.pcx", (0, 0)), placed("gone.pcx", (20, 0))];
        assert!(add_present_art(&items, &mut screen_art, &mut fonts, &files, &mut absent));
        assert_eq!(screen_art.regions.iter().map(|r| r.source.as_str()).collect::<Vec<_>>(), ["here.pcx"]);
        assert!(!absent.covers(&items[0]) && absent.covers(&items[1]));
        assert!(!add_present_art(&items, &mut screen_art, &mut fonts, &files, &mut absent), "known absent: not looked up or asked for again");
        // The addition has the existing file only, so it cannot fail.
        compiled.apply(&growing.grow(&compiled, &description(&screen_art), &Images).unwrap());
        let visible = items.iter().filter(|i| !absent.covers(i));
        assert_eq!(publication(visible, &fonts, &compiled, false, (1920, 1080)), Publication::Complete);
        assert_eq!(sprites(&items, &fonts, &compiled, u32::MAX, false).len(), 1);
        // Art kept from other folders is left out of a whole description.
        let mut kept = screen_art.clone();
        kept.regions.insert(Region { source: "other-folders.pcx".into(), rect: [0, 0, 4, 4], keyed: false, mask: None, palette: None });
        retain_present(&mut kept, &files);
        assert_eq!(kept, screen_art);
        absent.clear();
        assert!(!absent.covers(&items[1]));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn no_code_path_makes_a_missing_art_screen() {
        // Every outcome publishes the screen itself (`Publication` carries
        // no scene of its own).
        for p in [Publication::Complete, Publication::Partial { pending: true }, Publication::Partial { pending: false }] {
            match p { Publication::Complete | Publication::Partial { .. } => {} }
        }
        let needle = ["missing", " art"].concat();
        for (name, source) in [("live.rs", include_str!("live.rs")), ("ui_screen.rs", include_str!("ui_screen.rs")), ("presenter.rs", include_str!("presenter.rs"))] {
            let code: String = source.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n").to_ascii_lowercase();
            assert!(!code.contains(&format!("\"{needle}\"")), "{name} names a missing-art screen");
            assert!(!code.contains(&["viewer@", "glyphs"].concat()) || name == "presenter.rs", "{name} draws generated glyphs in place of art");
        }
    }

    #[test]
    fn every_civ_select_image_of_every_scenario_resolves_in_the_real_game() {
        let install = PathBuf::from(std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into()));
        if !install.is_dir() { return eprintln!("no Civ III install, skipped") }
        let complete = civ3_assets::GameFiles::complete(&install);
        let mut scenarios = Vec::new();
        for dir in ["Conquests/Conquests", "Conquests/Scenarios", "civ3PTW/Scenarios", "Scenarios"] {
            let Ok(dir) = civ3_assets::GameFiles::with_roots(vec![install.clone()]).resolve(dir) else { continue };
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            scenarios.extend(entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("biq"))));
        }
        scenarios.sort();
        assert!(scenarios.iter().any(|p| p.ends_with("9 WWII in the Pacific.biq")), "WWII in the Pacific among {scenarios:?}");
        let (mut checked, mut failures, mut absent) = (0, Vec::new(), Vec::new());
        for path in &scenarios {
            let Ok(biq) = civ3_assets::biq::Biq::open(path) else { eprintln!("{} does not read as a scenario, skipped", path.display()); continue };
            // Its folder as the game names it: relative to `Conquests`.
            let rel = path.with_extension("");
            let rel = rel.strip_prefix(&install).unwrap();
            let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
            let parts = if parts[0].eq_ignore_ascii_case("Conquests") { &parts[1..] } else { &parts[..] };
            let folder = parts.join("\\") + "\\";
            let files = crate::assets::game_files(&install, Some(&folder), &biq.search_folders());
            // The civ-selection screen: its background, and each civ's
            // leader head, advisor and race icons as its PediaIcons name them.
            let mut wanted = vec![("background".to_owned(), "Art/PlayerSetup/playerSetup.pcx".to_owned())];
            let pedia = std::fs::read(files.resolve("Text/PediaIcons.txt").unwrap()).unwrap();
            let mut key = String::new();
            for line in pedia.split(|&b| b == b'\n').map(|l| l.iter().map(|&b| b as char).collect::<String>()) {
                let line = line.trim();
                if line.starts_with('#') { key = line.to_ascii_uppercase(); continue; }
                if line.is_empty() || line.starts_with(';') { continue; }
                if key.starts_with("#RACE_") || key.starts_with("#ICON_RACE_") {
                    wanted.push((key.clone(), line.to_owned()));
                }
            }
            for (key, rel) in wanted {
                checked += 1;
                let Ok(full) = files.resolve(&rel) else {
                    // (A third-party scenario may name files it lacks: drawn
                    // as nothing, see `AbsentArt`; the game's own may not.)
                    let official = path.parent().is_some_and(|p| p.ends_with("Conquests/Conquests")) || path.file_name().is_some_and(|n| n.to_string_lossy().contains(" MP "));
                    if official { failures.push(format!("{}: {key} {rel}: no such file", path.display())); } else { absent.push(format!("{}: {key} {rel}", path.display())); }
                    continue;
                };
                // The game opens the path it resolved; the screen's art is
                // found again from it, with the setup screen's own art
                // folders (the game's) as with the scenario's.
                let opened = full.to_string_lossy().into_owned();
                for roots in [&complete, &files] {
                    let ours = relative_path(&opened, roots.roots());
                    let again = roots.resolve(&ours).ok().and_then(|p| std::fs::canonicalize(p).ok());
                    if again != std::fs::canonicalize(&full).ok() {
                        failures.push(format!("{}: {key} {rel}: {opened} found as {ours:?} -> {again:?}", path.display()));
                    }
                }
                // As the game may name it: relative to its working folder,
                // or under the install seen with another prefix.
                if let Ok(in_conquests) = full.strip_prefix(install.join("Conquests")) {
                    for opened in [in_conquests.to_string_lossy().replace('/', "\\"), format!("C:\\Elsewhere\\Civ3\\Conquests\\{}", in_conquests.to_string_lossy().replace('/', "\\"))] {
                        let ours = relative_path(&opened, complete.roots());
                        if complete.resolve(&ours).ok().and_then(|p| std::fs::canonicalize(p).ok()) != std::fs::canonicalize(&full).ok() {
                            failures.push(format!("{}: {key} {rel}: {opened} found as {ours:?}", path.display()));
                        }
                    }
                }
            }
        }
        eprintln!("{checked} civ-select images of {} scenarios checked", scenarios.len());
        eprintln!("{} files third-party scenarios name but lack (drawn as nothing):\n{}", absent.len(), absent.join("\n"));
        assert!(failures.is_empty(), "{} of {checked} unresolved:\n{}", failures.len(), failures.join("\n"));
    }

}

#[path = "ui_screen/content_targets.rs"]
mod content_targets;

#[cfg(test)]
#[path = "ui_screen/tile_info_tests.rs"]
mod tile_info_tests;
pub use content_targets::{content_anchors, military_unit_controls};


#[cfg(test)]
mod status_target_tests {
    use super::*;

    #[test]
    fn the_status_selection_icons_are_targets_at_the_native_hit_columns() {
        let image = |x, y, path: &str| ScreenDraw::Image { retained:None,
            path: path.into(), rect: [1, 1, 32, 32], x, y, keyed: true,
            clip: [0, 0, 1920, 1080], mask: None, palette: None, opacity: 255,
        };
        let canvas = 0x9F_DD84;
        let mut frame = Frame {
            overlay_draws: 0..0,
            draws: vec![
                // A portrait on another canvas must not become a target.
                image(742, 514, "C:/Civ3/Art/units/units_32.pcx"),
                // Icons overlap, as in a transport's status panel.
                image(742, 514, "C:/Civ3/Art/units/units_32.pcx"),
                image(758, 514, "C:/Civ3/Art/units/units_32.pcx"),
                // The unit picture and passive non-unit art are not rows.
                image(800, 620, "C:/Civ3/Art/units/units_32.pcx"),
                image(775, 514, "C:/Civ3/Art/other.pcx"),
            ], forms: vec![], form_draws: vec![(1, 0..1), (canvas, 1..5)], underlay_draws: vec![],
        };
        let row = Some((canvas, [727, 514, 247, 22]));
        let targets = status_unit_targets(&frame, row);
        assert_eq!(targets.iter().map(|c| (c.rect, c.kind, c.id)).collect::<Vec<_>>(), [
            ([727, 514, 16, 22], scene::InteractableKind::Button, (canvas as u64) << 32 | 0x0E00_0000),
            ([743, 514, 16, 22], scene::InteractableKind::Button, (canvas as u64) << 32 | 0x0E00_0001),
        ]);
        let mut scaled = frame.clone();
        scaled.draws[1] = ScreenDraw::ResizedImage { retained:None,
            path: "Art/units/units_32.pcx".into(), rect: [1, 1, 49, 32], x: 742, y: 514,
            size: (31, 20), sample_step: [100352, 100352], keyed: true, clip: [0, 0, 1920, 1080],
            mask: None, palette: None, opacity: 255,
        };
        let resized = status_unit_targets(&scaled, row);
        assert_eq!(resized.iter().map(|c| c.rect).collect::<Vec<_>>(), [[727, 514, 16, 20], [743, 514, 16, 22]]);
        scaled.form_draws[1].1.end = 2;
        assert_eq!(status_unit_targets(&scaled, row)[0].rect, [727, 514, 31, 20], "a single icon uses its actual resized extent");
        assert!(status_unit_targets(&frame, None).is_empty());
        // A hidden HUD absent from composition is never an Alt target.
        frame.form_draws.retain(|(c, _)| *c != canvas);
        assert!(status_unit_targets(&frame, row).is_empty());
    }
}

#[cfg(test)]
mod source_lookup_tests {
    use super::*;
    #[test]
    fn existing_sources_and_masks_are_resolved_once_across_new_control_regions() {
        let mut cache = AbsentArt::default();
        let mut lookups = std::collections::BTreeMap::new();
        let mut resolve = |path: &str| { *lookups.entry(path.to_owned()).or_insert(0) += 1; path != "missing.pcx" };
        for x in 0..100 {
            let region = Region { source: "controls.pcx".into(), rect: [x, 0, 10, 10], keyed: true,
                mask: Some(("mask.pcx".into(), (x, 0))), palette: None };
            assert!(cache.present_by(&region, &mut resolve));
        }
        let missing = Region { source: "missing.pcx".into(), rect: [0, 0, 10, 10], keyed: true, mask: None, palette: None };
        assert!(!cache.present_by(&missing, &mut resolve));
        assert!(!cache.present_by(&missing, &mut resolve));
        assert_eq!(lookups, std::collections::BTreeMap::from([
            ("controls.pcx".into(), 1), ("mask.pcx".into(), 1), ("missing.pcx".into(), 1)]));
        cache.clear();
        let mut changed = 0;
        assert!(cache.present_by(&missing, |_| { changed += 1; true }));
        assert_eq!(changed, 1, "a changed art-folder generation retries previously absent sources");
        let existing = Region { source: "controls.pcx".into(), ..missing };
        assert!(!cache.present_by(&existing, |_| { changed += 1; false }));
        assert_eq!(changed, 2, "positive entries also expire when art roots change");
    }
}
