//! The parts of the game's interface the viewer arranges itself
//! (`scene::Interface`, docs §16.12): which screen is open (the advisors
//! F1-F8, the city screen, the palace, espionage, the game menu, the
//! Civilopedia), the top-left corner's buttons (shown in the view the
//! pointer is in), the open popup (moved into a view where it fits) and the
//! unit's order buttons (reachable from every view).

use crate::ui::{Frame, ScreenDraw};

/// The interface's views besides `scene_map::INTERFACE_VIEW` (drawn over
/// it: higher ids are on top).
pub const CORNER_VIEW: u32 = 256;
pub const POPUP_VIEW: u32 = 257;
pub const SCREEN_VIEW: u32 = 258;
pub const HINT_VIEW: u32 = 259;

/// What shows in the corner's place while its buttons cannot be used (the
/// mouse captured): a piece of the game's popup parchment, framed, and
/// "Press CTRL" in the game's font; its rectangle (x, y, w, h) and items.
pub fn capture_hint([x, y, w, h]: [i32; 4]) -> ([i32; 4], Vec<crate::ui_screen::Item>) {
    use crate::ui_screen::Item;
    use menu::screen::{Element, Region};
    let rect = [x + 8, y + 8, (w - 16).max(112), (h - 16).max(30)];
    let [rx, ry, rw, rh] = rect;
    let clip = [rx, ry, rx + rw, ry + rh];
    let parchment = Region { source: "Art/PopUpandMenuBackground.pcx".into(), rect: [0, 0, rw, rh], keyed: false, mask: None, palette: None };
    let mut items = vec![Item::Element(Element::Image { region: parchment, at: (rx, ry), clip }, false)];
    for edge in [[rx, ry, rw, 1], [rx, ry + rh - 1, rw, 1], [rx, ry, 1, rh], [rx + rw - 1, ry, 1, rh]] {
        items.push(Item::Element(Element::Fill { rect: edge, rgb: [72, 52, 24], alpha: 255, clip }, false));
    }
    let run = crate::text::TextRun {
        x: rx + 12,
        y: ry + rh / 2 + 5,
        color: [0, 0, 0],
        face: "Lucida Sans".into(),
        height: -14,
        weight: 700,
        italic: false,
        underline: false,
        text: "Press CTRL".into(),
        backdrop: None,
    };
    items.push(Item::Text(run, clip, false));
    (rect, items)
}

/// Whether the scene carries the capture hint (`scene::Interface::
/// capture_hint`), for the viewer to show while the mouse is captured: with
/// the corner's buttons on the map, and also while a screen (the city
/// screen, an advisor) hides them but the world is still shown beside it,
/// where the mouse can be captured to play on (`crate::modal`).
pub fn capture_hint_shown(corner_buttons: bool, map_shown: bool, screen_open: bool) -> bool {
    corner_buttons || (map_shown && screen_open)
}

/// A popup the game opens where a map click was made, and so belongs at
/// that click (`scene::Interface::popup_anchor`) rather than with the
/// screens.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ClickPopup {
    /// A context menu (the unit stack's list, the right-click menu).
    /// `Context_Menu_open` (PCGames 0x61A3B0) puts a top-level menu's
    /// top-left corner one pixel right of and below the mouse point; with
    /// bit 2 of its field 0x574 the menu is first moved left by its width
    /// (its top-right corner at the point). Only where that leaves the
    /// screen does the game flip it to the other side of the point.
    ContextMenu { right_aligned: bool },
    /// The right-click tile information box (terrain, its name, yields,
    /// the units there). `open_tile_info` (PCGames 0x5AA530) puts its
    /// top-left corner at the tile's screen position
    /// (`tile_to_screen_coords`) moved left by half a tile width and down
    /// by 9 less half a tile height, then keeps it on the screen.
    TileInfo,
}

/// The tile information box's form class (`open_tile_info`'s object at
/// 0xC950A0, constructed at 0x5AA400).
pub fn tile_info_vtable(build: civ3_sys::Build) -> Option<u32> {
    match build {
        civ3_sys::Build::PcGames => Some(0x66_F5C8),
        _ => None,
    }
}

/// Bit of `Context_Menu` field 0x574 that aligns a menu's right edge to
/// the point it opens at, and that field's offset.
pub const CONTEXT_MENU_FLAGS: u32 = 0x574;
pub const CONTEXT_MENU_RIGHT_ALIGNED: u32 = 2;

/// What the open popup of form class `vtable` is, if one the game places
/// at a map click (`flags`: a context menu's field 0x574).
pub fn click_popup(build: civ3_sys::Build, vtable: u32, flags: impl FnOnce() -> Option<u32>) -> Option<ClickPopup> {
    if civ3_sys::controls::CONTEXT_MENU_VTABLES.contains(&vtable) {
        return Some(ClickPopup::ContextMenu { right_aligned: flags().is_some_and(|f| f & CONTEXT_MENU_RIGHT_ALIGNED != 0) });
    }
    (tile_info_vtable(build) == Some(vtable)).then_some(ClickPopup::TileInfo)
}

/// Where popup `kind` (`size`: its width and height) goes for the map
/// click at world pixel `at` (the current zoom's view pixels) on `tile`,
/// by the game's own rule (without its flip at the screen's edges: the
/// viewer moves the view instead).
pub fn popup_anchor(kind: ClickPopup, at: (i32, i32), tile: (i32, i32), zoomed_out: bool, size: (i32, i32)) -> scene::PopupAnchor {
    match kind {
        ClickPopup::ContextMenu { right_aligned } => scene::PopupAnchor { world: at, offset: (if right_aligned { 1 - size.0 } else { 1 }, 1), world_scaled: false },
        ClickPopup::TileInfo => {
            let (w, h) = if zoomed_out { (64, 32) } else { (128, 64) };
            scene::PopupAnchor { world: (tile.0 * w / 2 - w / 2, tile.1 * h / 2 + 9 - h / 2), offset: (0, 0), world_scaled: true }
        }
    }
}

/// Where popup `kind` opened for the map press at world pixel `at` on
/// `tile` goes: [`popup_anchor`], the tile box at `game_tile` if known, the
/// tile the game's own pointer conversion gave `open_tile_info` (a city
/// menu's "Terrain details": the city's tile, the menu's), not the press's.
pub fn click_anchor(kind: ClickPopup, at: (i32, i32), tile: (i32, i32), game_tile: Option<(i32, i32)>, zoomed_out: bool, size: (i32, i32)) -> scene::PopupAnchor {
    let tile = match kind {
        ClickPopup::TileInfo => game_tile.unwrap_or(tile),
        ClickPopup::ContextMenu { .. } => tile,
    };
    popup_anchor(kind, at, tile, zoomed_out, size)
}

/// The point of interest while popup `open`: unchanged while the game's
/// camera is parked (`camera_focus` published: its real focus requests
/// say where to look, opening a menu is none). Older drivers' popups
/// stand for the subject the game brought into its own view's middle.
pub fn popup_focus(focus: Option<(i32, i32)>, camera_focus: Option<scene::CameraFocus>, open: bool, game_view_middle: Option<(i32, i32)>) -> Option<(i32, i32)> {
    if !open || camera_focus.is_some() { return focus; }
    game_view_middle.or(focus)
}

/// The screen `frame` shows that the viewer may place: the topmost form of
/// a class in `screens` (vtable, name); with its index in `frame.forms`.
pub fn screen(frame: &Frame, screens: &[(u32, &'static str)]) -> Option<(usize, &'static str)> {
    frame.forms.iter().enumerate().rev().find_map(|(i, f)| screens.iter().find(|s| s.0 == f.0).map(|s| (i, s.1)))
}

/// The active screen and its placement setting. Preferences is a modal
/// screen of its own, including over an advisor, and follows the Menu
/// setting rather than the screen below it.
pub fn active_screen(frame: &Frame, build: civ3_sys::Build) -> Option<(usize, &'static str)> {
    let screens = civ3_sys::ui_hooks::screen_forms(build);
    let preferences = civ3_sys::ui_hooks::preferences_form(build);
    frame.forms.iter().enumerate().rev().find_map(|(i, f)| {
        if preferences.is_some_and(|p| p.vtable == f.0) {
            Some((i, "Menu"))
        } else {
            screens.iter().find(|s| s.0 == f.0).map(|s| (i, s.1))
        }
    })
}

/// The map's corner and unit-order controls may roam only while no screen
/// covers the map. Preferences has its own form outside the named screens
/// that the viewer can place, but also covers these map controls.
pub fn map_hud(frame: &Frame, build: civ3_sys::Build) -> Option<&Frame> {
    active_screen(frame, build).is_none().then_some(frame)
}

/// The rectangle (x, y, w, h) around the forms composited whose canvas
/// `pick` chooses.
pub fn forms_rect(frame: &Frame, pick: impl Fn(u32) -> bool) -> Option<[i32; 4]> {
    let rects = frame.forms.iter().filter(|f| pick(f.1) && f.3.0 > 0 && f.3.1 > 0).map(|f| [f.2.0, f.2.1, f.2.0 + f.3.0, f.2.1 + f.3.1]);
    let [l, t, r, b] = rects.reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])?;
    Some([l, t, r - l, b - t])
}

/// The corner region of buttons at `buttons` (x, y, w, h) near the
/// window's top-left corner: from the corner to the buttons and as far
/// beyond them as they are from it (the panel they sit on).
pub fn corner_region([x, y, w, h]: [i32; 4]) -> [i32; 4] {
    let (mx, my) = (x.max(0), y.max(0));
    [0, 0, x + w + mx, y + h + my]
}

/// The corner's buttons (forms of the `canvases`) as a part of their own:
/// the corner region ([`corner_region`]), grown to the tooltip shown by
/// them (drawn straight onto the screen, on no canvas, next to them), and
/// the draws that are the part's (the buttons' and the tooltip's), which the
/// rest of the interface leaves out: the part shows in one view only.
pub fn corner_part(frame: &Frame, canvases: &[u32]) -> Option<([i32; 4], Vec<usize>)> {
    let [x, y, w, h] = corner_region(forms_rect(frame, |c| canvases.contains(&c))?);
    let mut draws: Vec<usize> = frame.form_draws.iter().filter(|(c, _)| canvases.contains(c)).flat_map(|(_, r)| r.clone()).collect();
    let owned = |i: usize| frame.form_draws.iter().map(|(_, r)| r).chain(&frame.underlay_draws).any(|r| r.contains(&i));
    let mut bounds = [x, y, x + w, y + h];
    let near = [x - POPUP_FRAME, y - POPUP_FRAME, x + w + POPUP_FRAME, y + h + POPUP_FRAME];
    for (i, d) in frame.draws.iter().enumerate().filter(|&(i, _)| !owned(i)) {
        let Some(r) = reach(d) else { continue };
        let c = intersect(r, near);
        if c[2] > c[0] && c[3] > c[1] {
            bounds = [bounds[0].min(r[0]), bounds[1].min(r[1]), bounds[2].max(r[2]), bounds[3].max(r[3])];
            draws.push(i);
        }
    }
    let [l, t, r, b] = bounds;
    Some(([l.max(0), t.max(0), r - l.max(0), b - t.max(0)], draws))
}

/// `frame` with `draws` left out (clipped to nothing: the indices stay).
pub fn without(frame: &Frame, draws: &[usize]) -> Frame {
    let mut out = frame.clone();
    for &i in draws {
        if let Some(d) = out.draws.get_mut(i) {
            *clip_mut(d) = [0, 0, 0, 0];
        }
    }
    out
}

/// Clip a panel's draws to its own bounds while retaining the draw indices
/// and form ownership used to insert generated controls over the panel.
pub fn clipped(frame: &Frame, [x, y, w, h]: [i32; 4]) -> Frame {
    let mut out = frame.clone();
    for draw in &mut out.draws {
        let clip = clip_mut(draw);
        *clip = intersect(*clip, [x, y, x + w, y + h]);
    }
    out
}

fn intersect(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]
}

fn clip_mut(d: &mut ScreenDraw) -> &mut [i32; 4] {
    match d {
        ScreenDraw::Image { clip, .. } | ScreenDraw::ResizedImage { clip, .. } | ScreenDraw::Fill { clip, .. } | ScreenDraw::Text { clip, .. } | ScreenDraw::Flc { clip, .. } | ScreenDraw::Surface { clip, .. }
        | ScreenDraw::Indexed { clip, .. } => clip,
    }
}

/// What `frame` shows within `rect` (x, y, w, h): the draws reaching into
/// it, clipped to it (not the underlays: the map's labels go with the map).
pub fn region(frame: &Frame, [x, y, w, h]: [i32; 4]) -> Frame {
    let rect = [x, y, x + w, y + h];
    let under = |i: usize| frame.underlay_draws.iter().any(|r| r.contains(&i));
    let draws = frame
        .draws
        .iter()
        .enumerate()
        .filter(|&(i, _)| !under(i))
        .filter_map(|(_, d)| {
            let mut d = d.clone();
            let clip = clip_mut(&mut d);
            *clip = intersect(*clip, rect);
            (clip[2] > clip[0] && clip[3] > clip[1] && crate::ui::reaches_clip(&d)).then_some(d)
        })
        .collect();
    Frame { draws, forms: frame.forms.clone(), form_draws: Vec::new(), underlay_draws: Vec::new() }
}

/// How far from a popup's canvas the frame the game draws around it (its
/// border, its title band) may reach.
const POPUP_FRAME: i32 = 48;

/// The rectangle (x0, y0, x1, y1) draw `d` reaches on the screen, clipped
/// (a text by its pen's point).
fn reach(d: &ScreenDraw) -> Option<[i32; 4]> {
    let (r, clip) = match d {
        ScreenDraw::Image { rect, x, y, clip, .. } | ScreenDraw::Surface { rect, x, y, clip, .. } => ([*x, *y, x + rect[2], y + rect[3]], clip),
        ScreenDraw::Indexed { rect, x, y, clip, indices, .. } => { let (w, h) = indices.size(*rect); ([*x, *y, x + w, y + h], clip) },
        ScreenDraw::ResizedImage { x, y, size: (w, h), clip, .. } => ([*x, *y, x + w, y + h], clip),
        ScreenDraw::Fill { rect, clip, .. } => ([rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]], clip),
        ScreenDraw::Text { run, clip } => ([run.x, run.y - 1, run.x + 1, run.y], clip),
        // Leader animations carry their exact window clip. Unit FLCs
        // belong to the world and cannot define a dialog's UI bounds.
        ScreenDraw::Flc { x, y, clip, plain: true, .. } => ([(*x).max(clip[0]), (*y).max(clip[1]), clip[2], clip[3]], clip),
        ScreenDraw::Flc { plain: false, .. } => return None,
    };
    let r = intersect(r, *clip);
    (r[2] > r[0] && r[3] > r[1]).then_some(r)
}

/// The popup open in `frame`: the modal form (its canvas `canvas`) when it
/// is composited above the screen the viewer places (`screen`, an index
/// into `frame.forms`) and is none itself. Its first draw (it and what is
/// composited over it: its controls, the cursor) and its rectangle
/// (x, y, w, h) on the screen of `size`. The frame the game draws around
/// it straight onto the screen just before it (on no form's canvas: its
/// border, its title band above it) goes with it.
pub fn popup(frame: &Frame, canvas: u32, screen: Option<usize>, size: (i32, i32)) -> Option<(usize, [i32; 4])> {
    let index = frame.forms.iter().rposition(|f| f.1 == canvas)?;
    if screen.is_some_and(|s| index <= s) {
        return None;
    }
    let (first, [x, y, w, h]) = part_from(frame, index, size, true)?;
    let bounds = popup_portraits(frame, first, [x, y, w, h]).into_iter().filter_map(|i| reach(&frame.draws[i]))
        .fold([x, y, x + w, y + h], |a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]);
    Some((first, [bounds[0], bounds[1], bounds[2] - bounds[0], bounds[3] - bounds[1]]))
}

/// How much smaller than the menu under it, each way, a modal form between
/// games is to be a popup over that menu rather than a screen of its own.
const MENU_POPUP_MARGIN: i32 = 32;
/// The game's menus (title, setup screens): 1024 by 768, in the middle of its window.
const MENU_SIZE: (i32, i32) = (1024, 768);

/// The popup open between games (as [`popup`], no screen placed): the
/// modal form `canvas` where it is a popup over the title or a setup
/// screen. Those screens are modal forms themselves, as large as the menu
/// under them (the title's 1024 by 768 content; full-window backdrops left
/// out): a popup is smaller than that each way.
///
/// In a window larger than the menu the game also draws a few small
/// pieces at the window's own edges (the version number in its bottom
/// left corner, a bar at the bottom): those are not the menu either, and
/// counting them would make the menu window-sized, so that a full setup
/// screen over the title fits "inside" it as a popup. Only pieces that
/// reach into the menu's 1024 by 768 in the window's middle count.
pub fn menu_popup(frame: &Frame, canvas: u32, size: (i32, i32)) -> Option<(usize, [i32; 4])> {
    let (first, rect) = popup(frame, canvas, None, size)?;
    let backdrop = |r: &[i32; 4]| r[0] <= 0 && r[1] <= 0 && r[2] >= size.0 && r[3] >= size.1;
    let [ml, mt] = [(size.0 - MENU_SIZE.0) / 2, (size.1 - MENU_SIZE.1) / 2];
    let larger = size.0 > MENU_SIZE.0 || size.1 > MENU_SIZE.1;
    let in_menu = |r: &[i32; 4]| !larger || (r[2] > ml && r[0] < ml + MENU_SIZE.0 && r[3] > mt && r[1] < mt + MENU_SIZE.1);
    let [l, t, r, b] = frame.draws[..first].iter().filter_map(reach).filter(|r| !backdrop(r) && in_menu(r))
        .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])?;
    (rect[2] + 2 * MENU_POPUP_MARGIN <= r - l && rect[3] + 2 * MENU_POPUP_MARGIN <= b - t).then_some((first, rect))
}

/// The advisor picture above a dialog can be drawn on the map canvas,
/// before the dialog's own form is composed. It is part of the dialog,
/// even though that canvas also owns map labels and other world overlays.
fn advisor_portrait(draw: &ScreenDraw) -> bool {
    let ScreenDraw::Image { path, .. } = draw else { return false };
    let mut parts = path.rsplit(['/', '\\']);
    let Some(name) = parts.next() else { return false };
    parts.next().is_some_and(|folder| folder.eq_ignore_ascii_case("smallheads"))
        && ["popupDOMESTIC.pcx", "popupTRADE.pcx", "popupMILITARY.pcx", "popupFOREIGN.pcx", "popupCULTURE.pcx", "popupSCIENCE.pcx"].iter().any(|p| name.eq_ignore_ascii_case(p))
}

fn popup_portraits(frame: &Frame, first: usize, [x, y, w, h]: [i32; 4]) -> Vec<usize> {
    let near = [x - POPUP_FRAME, y - POPUP_FRAME, x + w + POPUP_FRAME, y + h + POPUP_FRAME];
    let menus_from = crate::ui_screen::menus_from(frame);
    frame.draws.iter().enumerate().filter_map(|(i, draw)| {
        if !advisor_portrait(draw) { return None; }
        // Do not steal the head of an advisor underneath another dialog.
        // The map and Main_GUI canvases, and directly drawn pictures,
        // have no such UI owner.
        let map_or_unowned = i < menus_from || frame.underlay_draws.iter().any(|r| r.contains(&i))
            || !frame.form_draws.iter().any(|(_, r)| r.contains(&i));
        if i < first && !map_or_unowned { return None; }
        let r = reach(draw)?;
        let overlap = intersect(r, near);
        (overlap[2] > overlap[0] && overlap[3] > overlap[1]).then_some(i)
    }).collect()
}

/// Split off a popup and its advisor picture. Draws from the map canvas
/// are transferred individually, so the dialog never takes its background
/// with it and the picture cannot reappear in a world magnifier.
pub fn split_popup(frame: &Frame, first: usize, rect: [i32; 4]) -> (Frame, Frame) {
    // Match against the original form and border, not the already enlarged
    // rectangle, which might reach another picture farther above it.
    let source = frame.form_draws.iter().filter(|(_, r)| !r.is_empty() && r.start >= first).min_by_key(|(_, r)| r.start)
        .and_then(|(canvas, _)| frame.forms.iter().rposition(|f| f.1 == *canvas))
        .and_then(|index| part_from(frame, index, (rect[0] + rect[2], rect[1] + rect[3]), true))
        .map_or(rect, |(_, r)| r);
    let attached: Vec<_> = popup_portraits(frame, first, source).into_iter().filter(|&i| i < first).collect();
    let (below, mut above) = split_at(frame, first);
    let below = without(&below, &attached);
    let n = attached.len();
    if n > 0 {
        let mut draws: Vec<_> = attached.into_iter().map(|i| frame.draws[i].clone()).collect();
        draws.append(&mut above.draws);
        above.draws = draws;
        for (_, range) in &mut above.form_draws {
            range.start += n;
            range.end += n;
        }
    }
    (below, above)
}

/// One modal canvas, separated from both its parent and its child dialogs.
#[derive(Clone, Debug)]
pub struct PopupPart {
    pub canvas: u32,
    pub rect: [i32; 4],
    pub frame: Frame,
}

/// Remove the complete visible modal stack before measuring the named screen.
/// Split from the top down so a full-window transparent parent never includes
/// a child's art when its own visible content bounds are measured.
pub fn split_modal_stack(frame: &Frame, canvases: &[u32], screen: Option<usize>, size: (i32, i32)) -> (Frame, Vec<PopupPart>) {
    let mut below = frame.clone();
    let mut parts = Vec::new();
    for &canvas in canvases.iter().rev() {
        if parts.len() == 4 { break; }
        if parts.iter().any(|part: &PopupPart| part.canvas == canvas) { continue; }
        let Some((first, rect)) = popup(&below, canvas, screen, size) else { continue };
        let (parent, own) = split_popup(&below, first, rect);
        parts.push(PopupPart { canvas, rect, frame: own });
        below = parent;
    }
    parts.reverse();
    (below, parts)
}

/// The active screen's part. Preferences owns a screen-sized canvas,
/// while its controls and background occupy a 1024 by 768 panel. Keep that
/// panel, rather than the driver's map, as the rectangle the viewer places.
/// Its own first draw also keeps an underlying advisor or game menu out.
pub fn active_screen_part(frame: &Frame, index: usize, build: civ3_sys::Build, size: (i32, i32)) -> Option<(usize, [i32; 4])> {
    let form = frame.forms.get(index)?;
    if let Some(preferences) = civ3_sys::ui_hooks::preferences_form(build).filter(|p| p.vtable == form.0) {
        let first = frame.form_draws.iter().filter(|(c, r)| *c == form.1 && !r.is_empty()).map(|(_, r)| r.start).min()?;
        let rect = crate::prefs::screen_area(frame, &preferences, (1024, 768))?;
        return Some((first, rect));
    }
    // A game menu owns its frame rather than an advisor's shared
    // background. Keep it separate even if it fills the driver's window.
    let is_menu = civ3_sys::ui_hooks::screen_forms(build).iter().any(|s| s.0 == form.0 && s.1 == "Menu");
    if is_menu { part_from(frame, index, size, false) } else { screen_part(frame, index, size) }
}

/// The named screen (form `index` of `frame.forms`) as a part of its own,
/// where it is smaller than the screen of `size`: its first draw (it and
/// what is drawn over it) and its rectangle (x, y, w, h), its frame
/// included (as [`popup`]). The forms just below it of the same rectangle
/// are its background (the advisors' common one): they go with it.
pub fn screen_part(frame: &Frame, index: usize, size: (i32, i32)) -> Option<(usize, [i32; 4])> {
    let place = |f: &(u32, u32, (i32, i32), (i32, i32))| (f.2, f.3);
    let mut bottom = index;
    let menus_from = crate::ui_screen::menus_from(frame);
    while bottom > 0 && place(&frame.forms[bottom - 1]) == place(&frame.forms[index]) {
        let previous = frame.forms[bottom - 1].1;
        let in_menus = frame.form_draws.iter().any(|(canvas, r)| *canvas == previous && !r.is_empty() && r.start >= menus_from);
        if !in_menus { break; }
        bottom -= 1;
    }
    let (first, rect) = (bottom..=index).find_map(|i| part_from(frame, i, size, false))?;
    (rect[2] < size.0 || rect[3] < size.1).then_some((first, rect))
}

/// Form `index` of `frame.forms` from its first draw on, with the frame the
/// game draws round it straight onto the screen just before it.
fn part_from(frame: &Frame, index: usize, size: (i32, i32), modal: bool) -> Option<(usize, [i32; 4])> {
    let (_, canvas, (x, y), (w, h)) = frame.forms[index];
    let mut first = frame.form_draws.iter().find(|(c, _)| *c == canvas).map(|(_, r)| r.start)?;
    let mut bounds = [x, y, x + w, y + h];
    if x <= 0 && y <= 0 && x + w >= size.0 && y + h >= size.1 {
        // A full-window canvas does not mean a full-window dialog.
        // Diplomacy, for example, composites its small conversation,
        // leader and advisor onto one transparent full-window canvas.
        // Use only this part and what is above it, never the map/HUD
        // canvases below it, so the complete UI can fit in the opening view.
        // An opaque full-window canvas (the Hall of Fame, the victory
        // screen: a 1024 by 768 board in its middle) starts with a fill of
        // all of it, black where the form never draws. That clear is its
        // backdrop, not its size, as the cleared window under the title is
        // (`menu_popup`); measured as content it made the board's screen
        // window-sized, and the viewer shrank all of it to fit. Only for
        // modal parts: a named screen of the whole window (the city screen)
        // keeps its margins over the frame under it.
        let backdrop = |d: &ScreenDraw| modal && matches!(d, ScreenDraw::Fill { .. })
            && reach(d).is_some_and(|r| r[0] <= 0 && r[1] <= 0 && r[2] >= size.0 && r[3] >= size.1);
        let content = |all: bool| frame.draws[first..].iter().filter(|d| all || !backdrop(d)).filter_map(reach)
            .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]);
        if let Some(content) = content(false).or_else(|| content(true)) {
            bounds = content;
        }
    }
    bounds = crate::loading_cursor::extend_bounds(frame.forms[index].0, bounds);
    let near = [bounds[0] - POPUP_FRAME, bounds[1] - POPUP_FRAME, bounds[2] + POPUP_FRAME, bounds[3] + POPUP_FRAME];
    let owned = |i: usize| frame.form_draws.iter().map(|(_, r)| r).chain(&frame.underlay_draws).any(|r| r.contains(&i));
    let meets = |a: [i32; 4], b: [i32; 4]| {
        let c = intersect(a, b);
        c[2] > c[0] && c[3] > c[1]
    };
    while first > 0 && !owned(first - 1) {
        let Some(r) = reach(&frame.draws[first - 1]).filter(|&r| meets(r, near)) else { break };
        bounds = [bounds[0].min(r[0]), bounds[1].min(r[1]), bounds[2].max(r[2]), bounds[3].max(r[3])];
        first -= 1;
    }
    let [l, t, r, b] = intersect(bounds, [0, 0, size.0, size.1]);
    if r <= l || b <= t {
        return None;
    }
    Some((first, [l, t, r - l, b - t]))
}

/// `frame` split at draw `first`: what is below and what is from there on.
pub fn split_at(frame: &Frame, first: usize) -> (Frame, Frame) {
    let first = first.min(frame.draws.len());
    let below = Frame {
        draws: frame.draws[..first].to_vec(),
        forms: frame.forms.clone(),
        form_draws: frame.form_draws.iter().filter(|(_, r)| r.end <= first).cloned().collect(),
        underlay_draws: frame.underlay_draws.iter().map(|r| r.start.min(first)..r.end.min(first)).filter(|r| !r.is_empty()).collect(),
    };
    let above = Frame {
        draws: frame.draws[first..].to_vec(),
        forms: frame.forms.clone(),
        form_draws: frame.form_draws.iter().filter(|(_, r)| r.start >= first).map(|(c, r)| (*c, r.start - first..r.end - first)).collect(),
        underlay_draws: Vec::new(),
    };
    (below, above)
}

/// An interface-only view of `rect` (x, y, w, h) holding `overlay`.
pub fn part_view(scene: &mut scene::Scene, id: u32, [x, y, w, h]: [i32; 4], overlay: Vec<scene::FreeSprite>) {
    let v = scene.view_mut(id);
    v.view = scene::View { origin: (x, y), viewport: (w.max(1) as u32, h.max(1) as u32), interface_only: true, ..Default::default() };
    v.overlay = overlay.into_iter().map(|s| scene::FreeSprite { screen: true, after_pass: u32::MAX, ..s }).collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capture_hint_is_there_wherever_the_world_can_be_captured() {
        // The map with its corner buttons; a screen over it, the world
        // beside it; not between games (no world).
        assert!(capture_hint_shown(true, true, false));
        assert!(capture_hint_shown(false, true, true));
        assert!(!capture_hint_shown(false, false, true));
        assert!(!capture_hint_shown(false, false, false));
    }

    #[test]
    fn context_menus_and_the_tile_box_are_anchored_by_the_games_rule() {
        use civ3_sys::Build;
        // The unit stack's list and the right-click menu: either class.
        for vt in civ3_sys::controls::CONTEXT_MENU_VTABLES {
            assert_eq!(click_popup(Build::PcGames, vt, || Some(0)), Some(ClickPopup::ContextMenu { right_aligned: false }));
            assert_eq!(click_popup(Build::PcGames, vt, || Some(CONTEXT_MENU_RIGHT_ALIGNED)), Some(ClickPopup::ContextMenu { right_aligned: true }));
            assert_eq!(click_popup(Build::PcGames, vt, || None), Some(ClickPopup::ContextMenu { right_aligned: false }));
        }
        assert_eq!(click_popup(Build::PcGames, 0x66_F5C8, || None), Some(ClickPopup::TileInfo));
        // Advisors, dialogs, diplomacy: placed with the screens.
        for vt in [0x66_C270, 0x66_58B0, 0x66_C14C, 0] {
            assert_eq!(click_popup(Build::PcGames, vt, || Some(2)), None);
        }
        assert_eq!(click_popup(Build::Steam, 0x66_F5C8, || None), None);
        // Top-left one pixel right of and below the click; right-aligned
        // menus their top-right corner there.
        let menu = ClickPopup::ContextMenu { right_aligned: false };
        assert_eq!(popup_anchor(menu, (1234, -56), (19, 0), false, (180, 300)), scene::PopupAnchor { world: (1234, -56), offset: (1, 1), world_scaled: false });
        let right = ClickPopup::ContextMenu { right_aligned: true };
        assert_eq!(popup_anchor(right, (10, 20), (0, 0), true, (180, 300)), scene::PopupAnchor { world: (10, 20), offset: (-179, 1), world_scaled: false });
        // The tile box: half a tile left of the tile's corner, 9 less half
        // a tile down; zoomed out with the smaller tiles.
        assert_eq!(popup_anchor(ClickPopup::TileInfo, (0, 0), (5, 7), false, (256, 173)), scene::PopupAnchor { world: (5 * 64 - 64, 7 * 32 + 9 - 32), offset: (0, 0), world_scaled: true });
        assert_eq!(popup_anchor(ClickPopup::TileInfo, (0, 0), (6, 0), true, (256, 173)), scene::PopupAnchor { world: (6 * 32 - 32, 9 - 16), offset: (0, 0), world_scaled: true });
    }

    #[test]
    fn terrain_details_from_a_city_menu_anchor_at_the_city_tile_at_both_zooms() {
        // The menu opened by a right click on the city at (19, 23); the game
        // takes the tile box's tile from its pointer conversion (kept on the
        // city's tile while the menu is open), whatever the pointer did since.
        let city = (19, 23);
        for zoomed_out in [false, true] {
            let (w, h) = if zoomed_out { (64, 32) } else { (128, 64) };
            let at = (city.0 * w / 2 + 5, city.1 * h / 2 + 7);
            let want = scene::PopupAnchor { world: (city.0 * w / 2 - w / 2, city.1 * h / 2 + 9 - h / 2), offset: (0, 0), world_scaled: true };
            for press_tile in [city, (40, 2)] {
                assert_eq!(click_anchor(ClickPopup::TileInfo, at, press_tile, Some(city), zoomed_out, (256, 173)), want, "Z {zoomed_out}, press on {press_tile:?}");
            }
            // Unknown: the press's tile.
            assert_eq!(click_anchor(ClickPopup::TileInfo, at, city, None, zoomed_out, (256, 173)), want);
            // A context menu goes to its click, not to a tile.
            let menu = ClickPopup::ContextMenu { right_aligned: false };
            assert_eq!(click_anchor(menu, at, city, Some((40, 2)), zoomed_out, (180, 300)), scene::PopupAnchor { world: at, offset: (1, 1), world_scaled: false });
        }
    }

    #[test]
    fn opening_a_popup_leaves_the_focus_while_the_camera_is_parked() {
        let parked = Some(scene::CameraFocus { serial: 3, center: (4000, 1200) });
        let unit = Some((640, 320));
        let corner = Some((512, 384));
        assert_eq!(popup_focus(unit, parked, true, corner), unit);
        assert_eq!(popup_focus(None, parked, true, corner), None);
        assert_eq!(popup_focus(unit, parked, false, corner), unit);
        // Older drivers: the game's own view's middle is the subject.
        assert_eq!(popup_focus(unit, None, true, corner), corner);
        assert_eq!(popup_focus(unit, None, false, corner), unit);
    }

    #[test]
    fn a_full_canvas_diplomacy_popup_uses_its_dialog_leader_and_advisor_bounds() {
        let full = [0, 0, 1920, 1080];
        let image = |path: &str, x, y, w, h| ScreenDraw::Image {
            path: path.into(), rect: [0, 0, w, h], x, y, keyed: true,
            clip: full, mask: None, palette: None, opacity: 255,
        };
        // Native WW2 diplomacy owns a full-window canvas but only draws
        // the conversation, its leader and the separate advisor on it.
        let frame = Frame {
            forms: vec![(0x66b3d8, 1, (0, 0), (1920, 1080)), (0x66c0d8, 2, (0, 0), (1920, 1080))],
            draws: vec![
                fill(0, 0, 1920, 1080),
                image("Art/Diplomacy/talk_offer.pcx", 297, 50, 438, 517),
                ScreenDraw::Flc { path: "Art/Flics/leader.flc".into(), chunk: 1, x: 411, y: 59, clip: [411, 59, 611, 299], plain: true },
                image("Art/SmallHeads/popupForeign.pcx", 907, 0, 91, 109),
                image("Art/Advisors/diplowinbox.pcx", 807, 99, 206, 102),
            ],
            form_draws: vec![(1, 0..1), (2, 1..5)],
            underlay_draws: vec![0..1],
        };
        let (first, rect) = popup(&frame, 2, None, (1920, 1080)).unwrap();
        assert_eq!((first, rect), (1, [297, 0, 716, 567]));
        let (map, dialog) = split_popup(&frame, first, rect);
        assert_eq!(map.draws, frame.draws[..1]);
        assert_eq!(dialog.draws, frame.draws[1..]);
        assert!(dialog.underlay_draws.is_empty());
        // The same rule covers full-canvas menus and cropped screen parts.
        assert_eq!(screen_part(&frame, 1, (1920, 1080)), Some((1, rect)));
    }

    #[test]
    fn a_plain_leader_animation_defines_the_visible_bounds_of_its_full_canvas_popup() {
        let frame = Frame {
            forms: vec![(0x123, 2, (0, 0), (1920, 1080))],
            draws: vec![ScreenDraw::Flc { path: "Art/Flics/leader.flc".into(), chunk: 3, x: 411, y: 59, clip: [411, 59, 611, 299], plain: true }],
            form_draws: vec![(2, 0..1)],
            ..Default::default()
        };
        assert_eq!(popup(&frame, 2, None, (1920, 1080)), Some((0, [411, 59, 200, 240])));
        // Ordinary unit animations cannot turn a world canvas into UI bounds.
        let mut unit = frame.clone();
        if let ScreenDraw::Flc { plain, .. } = &mut unit.draws[0] { *plain = false; }
        assert_eq!(popup(&unit, 2, None, (1920, 1080)), Some((0, [0, 0, 1920, 1080])));
    }

    fn fill(x: i32, y: i32, w: i32, h: i32) -> ScreenDraw {
        ScreenDraw::Fill { rect: [x, y, w, h], rgb: [1, 2, 3], alpha: 255, clip: [0, 0, 1920, 1080] }
    }

    /// The map screen with the HUD's corner buttons and two unit orders,
    /// and a popup over it.
    fn frame() -> Frame {
        Frame {
            draws: vec![fill(0, 0, 1920, 1080), fill(0, 0, 160, 60), fill(21, 12, 36, 30), fill(836, 1030, 32, 32), fill(700, 400, 500, 300), fill(720, 600, 80, 30)],
            forms: vec![
                (0x66_B3D8, 0x9F_8934, (0, 0), (1920, 1080)),
                (0x66_56C0, 0xA0_68B4, (21, 12), (36, 30)),
                (0x66_56C0, 0xA0_765C, (57, 12), (36, 30)),
                (0x66_56C0, 0xA0_61E0, (94, 12), (34, 30)),
                (0x66_56C0, 0xA1_4A68, (836, 1030), (32, 32)),
                (0x66_56C0, 0xA1_4A68 + 0x6DC, (868, 1030), (32, 32)),
                (0x66_6408, 0x50_0000, (700, 400), (500, 300)),
                (0x66_56C0, 0x50_1000, (720, 600), (80, 30)),
            ],
            form_draws: vec![(0x9F_8934, 0..2), (0xA0_68B4, 2..3), (0xA1_4A68, 3..4), (0x50_0000, 4..5), (0x50_1000, 5..6)],
            underlay_draws: vec![],
        }
    }

    #[test]
    fn the_corner_buttons_and_the_unit_orders_are_found_by_their_canvases() {
        let f = frame();
        let hud = civ3_sys::ui_hooks::hud_buttons(civ3_sys::Build::PcGames);
        assert_eq!(hud.corner, [0xA0_68B4, 0xA0_765C, 0xA0_61E0]);
        let corner = forms_rect(&f, |c| hud.corner.contains(&c)).unwrap();
        assert_eq!(corner, [21, 12, 107, 30]);
        assert_eq!(corner_region(corner), [0, 0, 149, 54]);
        assert_eq!(forms_rect(&f, |c| hud.is_unit_order(c)), Some([836, 1030, 64, 32]));
        assert!(!hud.is_unit_order(0xA1_4A68 + 1) && !hud.is_unit_order(0xA1_4A68 + 42 * 0x6DC) && hud.is_unit_order(0xA1_4A68 + 41 * 0x6DC));
        assert_eq!(forms_rect(&f, |_| false), None);
    }

    #[test]
    fn preferences_hides_map_controls_and_closing_it_restores_them() {
        let build = civ3_sys::Build::PcGames;
        let hud = civ3_sys::ui_hooks::hud_buttons(build);
        let mut f = frame();
        let controls = |f: &Frame| {
            map_hud(f, build).map(|f| (corner_part(f, &hud.corner), forms_rect(f, |c| hud.is_unit_order(c))))
        };
        let before = controls(&f).unwrap();
        assert!(before.0.is_some() && before.1.is_some());
        let preferences = civ3_sys::ui_hooks::preferences_form(build).unwrap();
        f.forms.push((preferences.vtable, preferences.form + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4, (0, 0), (1920, 1080)));
        // Preferences is not one of the screens with a placement setting.
        // Its presence must nevertheless suppress the separately copied HUD.
        assert_eq!(screen(&f, civ3_sys::ui_hooks::screen_forms(build)), None);
        assert_eq!(controls(&f), None);
        // An info popup over Preferences must keep those controls hidden.
        f.forms.push((0x66_6408, 0x60_0000, (700, 400), (500, 300)));
        assert_eq!(controls(&f), None);
        f.forms.pop();
        f.forms.pop();
        assert_eq!(controls(&f), Some(before));
    }

    #[test]
    fn named_screens_also_hide_map_controls() {
        let build = civ3_sys::Build::PcGames;
        let mut f = frame();
        for &(vtable, _) in civ3_sys::ui_hooks::screen_forms(build) {
            f.forms.push((vtable, 0x70_0000, (448, 156), (1024, 768)));
            assert_eq!(map_hud(&f, build), None);
            f.forms.pop();
            assert!(map_hud(&f, build).is_some());
        }
    }

    #[test]
    fn the_corner_part_takes_its_buttons_and_their_tooltip_from_the_interface() {
        let mut f = frame();
        let hud = civ3_sys::ui_hooks::hud_buttons(civ3_sys::Build::PcGames);
        // A tooltip under the Civilopedia button, on no canvas.
        f.draws.push(ScreenDraw::Fill { rect: [60, 44, 90, 16], rgb: [255, 255, 255], alpha: 255, clip: [i32::MIN, i32::MIN, i32::MAX, i32::MAX] });
        let (rect, draws) = corner_part(&f, &hud.corner).unwrap();
        assert_eq!(rect, [0, 0, 150, 60]);
        assert_eq!(draws, [2, 6]);
        let rest = without(&f, &draws);
        assert!(rest.draws.iter().enumerate().all(|(i, d)| reaches_clip_of(d) != draws.contains(&i)));
        // Far from the buttons a tooltip is not theirs.
        let mut f = frame();
        f.draws.push(ScreenDraw::Fill { rect: [900, 990, 90, 16], rgb: [255, 255, 255], alpha: 255, clip: [i32::MIN, i32::MIN, i32::MAX, i32::MAX] });
        assert_eq!(corner_part(&f, &hud.corner).unwrap(), ([0, 0, 149, 54], vec![2]));
    }

    fn reaches_clip_of(d: &ScreenDraw) -> bool {
        crate::ui::reaches_clip(d)
    }

    #[test]
    fn the_corner_region_keeps_what_reaches_into_it_clipped() {
        let r = region(&frame(), [0, 0, 149, 54]);
        assert_eq!(r.draws.len(), 3);
        assert!(r.draws.iter().all(|d| matches!(d, ScreenDraw::Fill { clip: [0, 0, 149, 54], .. })), "{:?}", r.draws);
    }

    #[test]
    fn screens_are_found_by_their_forms_class() {
        let mut f = frame();
        let table = civ3_sys::ui_hooks::screen_forms(civ3_sys::Build::PcGames);
        assert_eq!(screen(&f, table), None);
        // The cultural advisor over its background, the game menu over it.
        f.forms.insert(1, (0x66_88B8, 0x9A_FFCC, (448, 156), (1024, 768)));
        f.forms.insert(2, (0x66_BBC0, 0x9B_0000, (448, 156), (1024, 768)));
        assert_eq!(screen(&f, table), Some((2, "F5")));
        f.forms.push((0x66_DD80, 0xB4_2C64, (778, 407), (364, 266)));
        assert_eq!(screen(&f, table).map(|s| s.1), Some("Menu"));
        assert!(table.iter().any(|s| s.1 == "Palace") && table.iter().any(|s| s.1 == "Help") && table.iter().any(|s| s.1 == "F8"));
        assert_eq!(table.len(), viewer_settings::Layout::SCREENS.len());
        assert!(viewer_settings::Layout::SCREENS.iter().all(|n| table.iter().any(|s| s.1 == *n)));
    }

    #[test]
    fn preferences_uses_the_menu_setting_and_its_panel_without_the_underlying_advisor() {
        let build = civ3_sys::Build::PcGames;
        let preferences = civ3_sys::ui_hooks::preferences_form(build).unwrap();
        let canvas = preferences.form + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4;
        let mut f = Frame {
            draws: vec![
                fill(0, 0, 1920, 1080),
                fill(21, 12, 36, 30),
                fill(448, 156, 1024, 768),
                fill(448, 156, 1024, 768),
                fill(778, 407, 364, 266),
                fill(0, 0, 1920, 1080),
                ScreenDraw::Image {
                    path: preferences.background.into(), rect: [0, 0, 1024, 768], x: 448, y: 156,
                    keyed: false, clip: [0, 0, 1920, 1080], mask: None, palette: None, opacity: 255,
                },
                fill(500, 800, 80, 30),
                // The info box's frame is drawn directly onto the screen.
                fill(690, 70, 420, 30),
                fill(700, 100, 400, 300),
                fill(720, 340, 80, 30),
            ],
            forms: vec![
                (0x66_B3D8, 1, (0, 0), (1920, 1080)),
                (0x66_56C0, 2, (21, 12), (36, 30)),
                (0x66_88B8, 3, (448, 156), (1024, 768)),
                (0x66_C270, 4, (448, 156), (1024, 768)),
                (0x66_DD80, 5, (778, 407), (364, 266)),
                (preferences.vtable, canvas, (0, 0), (1920, 1080)),
                (0x66_56C0, 7, (500, 800), (80, 30)),
                (0x66_6408, 8, (700, 100), (400, 300)),
                (0x66_56C0, 9, (720, 340), (80, 30)),
            ],
            form_draws: vec![(1, 0..1), (2, 1..2), (3, 2..3), (4, 3..4), (5, 4..5), (canvas, 5..7), (7, 7..8), (8, 9..10), (9, 10..11)],
            underlay_draws: vec![],
        };
        assert_eq!(active_screen(&f, build), Some((5, "Menu")));
        let (popup_first, popup_rect) = popup(&f, 8, Some(5), (1920, 1080)).unwrap();
        assert_eq!((popup_first, popup_rect), (8, [690, 70, 420, 330]));
        let (below_popup, info) = split_at(&f, popup_first);
        let (first, rect) = active_screen_part(&below_popup, 5, build, (1920, 1080)).unwrap();
        assert_eq!((first, rect), (5, [448, 156, 1024, 768]));
        let (below_screen, raw_panel) = split_at(&below_popup, first);
        let panel = clipped(&raw_panel, rect);
        // The map and its HUD remain in the lower frame. Only the current
        // panel, not the old advisor or menu, is copied into screen_view.
        assert_eq!(below_screen.draws, f.draws[..5]);
        assert_eq!(raw_panel.draws, f.draws[5..8]);
        assert_eq!(panel.form_draws, raw_panel.form_draws);
        assert!(panel.draws.iter().all(|draw| match draw {
            ScreenDraw::Fill { clip, .. } | ScreenDraw::Image { clip, .. } => *clip == [448, 156, 1472, 924],
            _ => false,
        }));
        assert_eq!(info.draws, f.draws[8..]);
        // The generated tabs have exactly one owner and remain under the
        // complete info popup, including its directly drawn frame.
        assert_eq!(crate::prefs::tabs_slot(&below_screen, &preferences, Some(8)), None);
        assert_eq!(crate::prefs::tabs_slot(&panel, &preferences, Some(8)), Some(3));
        assert_eq!(crate::prefs::tabs_slot(&info, &preferences, Some(8)), None);
        f.forms.truncate(5);
        assert_eq!(active_screen(&f, build), Some((4, "Menu")));
        assert_eq!(active_screen_part(&f, 4, build, (1920, 1080)), Some((4, [778, 407, 364, 266])));
        f.forms.truncate(4);
        assert_eq!(active_screen(&f, build), Some((3, "F1")));
    }

    #[test]
    fn preferences_bounds_follow_the_drawn_background_not_its_full_screen_canvas() {
        let build = civ3_sys::Build::PcGames;
        let preferences = civ3_sys::ui_hooks::preferences_form(build).unwrap();
        let f = Frame {
            draws: vec![fill(0, 0, 1920, 1080), ScreenDraw::Image {
                path: preferences.background.into(), rect: [100, 50, 924, 718], x: 548, y: 206,
                keyed: false, clip: [0, 0, 1920, 1080], mask: None, palette: None, opacity: 255,
            }],
            forms: vec![(0x66_B3D8, 1, (0, 0), (1920, 1080)), (preferences.vtable, 2, (0, 0), (1920, 1080))],
            form_draws: vec![(1, 0..1), (2, 1..2)],
            underlay_draws: vec![],
        };
        assert_eq!(active_screen(&f, build), Some((1, "Menu")));
        assert_eq!(active_screen_part(&f, 1, build, (1920, 1080)), Some((1, [448, 156, 1024, 768])));
        // An as-yet undrawn Preferences form supplies no empty screen view.
        let empty = Frame { draws: vec![], form_draws: vec![], ..f };
        assert_eq!(active_screen_part(&empty, 1, build, (1920, 1080)), None);
    }

    #[test]
    fn a_full_screen_game_menu_does_not_take_the_worlds_draws_with_it() {
        let build = civ3_sys::Build::PcGames;
        let f = Frame {
            draws: vec![fill(0, 0, 1024, 768), fill(0, 0, 1024, 768)],
            forms: vec![(0x66_B3D8, 1, (0, 0), (1024, 768)), (0x66_DD80, 2, (0, 0), (1024, 768))],
            form_draws: vec![(1, 0..1), (2, 1..2)],
            underlay_draws: vec![],
        };
        assert_eq!(active_screen_part(&f, 1, build, (1024, 768)), Some((1, [0, 0, 1024, 768])));
    }

    #[test]
    fn the_popup_is_split_off_with_what_is_drawn_over_it() {
        let f = frame();
        let (first, rect) = popup(&f, 0x50_0000, None, (1920, 1080)).unwrap();
        assert_eq!((first, rect), (4, [700, 400, 500, 300]));
        let (below, above) = split_at(&f, first);
        assert_eq!((below.draws.len(), above.draws.len()), (4, 2));
        assert_eq!(above.form_draws, [(0x50_0000, 0..1), (0x50_1000, 1..2)]);
        // The frame the game draws around it on no canvas, just before it
        // (a title band above it), goes with it; draws further off do not.
        let mut f = frame();
        let band = ScreenDraw::Image { path: "popupborders.pcx".into(), rect: [251, 1, 60, 35], x: 690, y: 365, keyed: true, clip: [i32::MIN, i32::MIN, i32::MAX, i32::MAX], mask: None, palette: None, opacity: 255 };
        f.draws.insert(4, fill(10, 10, 20, 20));
        f.draws.insert(5, band);
        f.form_draws = vec![(0x9F_8934, 0..2), (0xA0_68B4, 2..3), (0xA1_4A68, 3..4), (0x50_0000, 6..7), (0x50_1000, 7..8)];
        assert_eq!(popup(&f, 0x50_0000, None, (1920, 1080)), Some((5, [690, 365, 510, 335])));
        // Under a screen composited above it: not a popup of its own.
        assert_eq!(popup(&f, 0x50_0000, Some(7), (1920, 1080)), None);
        assert_eq!(popup(&f, 0x12_3456, None, (1920, 1080)), None);
    }

    /// Between games the title and the setup screens are modal forms
    /// themselves: only a form smaller than the menu under it is a popup.
    #[test]
    fn between_games_only_a_form_smaller_than_the_menu_is_a_popup() {
        let size = (1920, 1080);
        let (x, y) = (448, 156);
        // The full-window backdrop and the title, a setup screen over it,
        // a popup over that.
        let f = Frame {
            draws: vec![fill(0, 0, 1920, 1080), fill(x, y, 1024, 768), fill(x, y, 1024, 768), fill(x + 262, y + 234, 500, 300)],
            forms: vec![(0x11, 1, (0, 0), (1920, 1080)), (0x22, 2, (x, y), (1024, 768)), (0x33, 3, (x + 262, y + 234), (500, 300))],
            form_draws: vec![(1, 0..2), (2, 2..3), (3, 3..4)],
            underlay_draws: vec![],
        };
        assert_eq!(menu_popup(&f, 3, size), Some((3, [x + 262, y + 234, 500, 300])));
        assert_eq!(menu_popup(&f, 2, size), None);
        // Nothing under it but the backdrop: no menu to be over.
        let alone = Frame { draws: vec![fill(0, 0, 1920, 1080), fill(x, y, 500, 300)], forms: vec![(0x11, 1, (0, 0), (1920, 1080)), (0x33, 3, (x, y), (500, 300))],
            form_draws: vec![(1, 0..1), (3, 1..2)], underlay_draws: vec![] };
        assert_eq!(menu_popup(&alone, 3, size), None);
    }

    /// TS Neo: a 3274 by 2048 game window. Besides the backdrop and the
    /// title the game draws the version number in the window's bottom
    /// left corner and a bar at its bottom. Player Setup (and the other
    /// setup screens, Load Scenario among them) replaces the title: a full
    /// menu, never a popup over it. A real dialog over it still is one.
    #[test]
    fn setup_screens_over_the_title_in_a_large_window_are_menus_not_popups() {
        let size = (3274, 2048);
        let big = |x, y, w, h| ScreenDraw::Fill { rect: [x, y, w, h], rgb: [1, 2, 3], alpha: 255, clip: [0, 0, 3274, 2048] };
        let (x, y) = (1125, 640);
        let title = vec![big(0, 0, 3274, 2048), big(x, y, 1024, 768), big(1, 2010, 60, 20), big(1600, 2010, 80, 20)];
        let n = title.len();
        let setup = |draws: Vec<ScreenDraw>| {
            let mut all = title.clone();
            let from = all.len();
            all.extend(draws);
            let to = all.len();
            Frame { draws: all, forms: vec![(0x11, 1, (0, 0), size), (0x22, 2, (x, y), (1024, 768))],
                form_draws: vec![(1, 0..n), (2, from..to)], underlay_draws: vec![] }
        };
        // Player Setup, Load Scenario, Game setup: full 1024 by 768 forms.
        for form in [vec![big(x, y, 1024, 768)], vec![big(x, y, 1024, 768), big(x + 40, y + 60, 400, 600)]] {
            assert_eq!(menu_popup(&setup(form), 2, size), None);
        }
        // A small dialog over the title is a popup.
        let mut f = setup(vec![big(x + 262, y + 234, 500, 300)]);
        f.forms[1] = (0x22, 2, (x + 262, y + 234), (500, 300));
        assert_eq!(menu_popup(&f, 2, size), Some((n, [x + 262, y + 234, 500, 300])));
    }

    /// TS Neo: the Hall of Fame from the title in a 3274 by 2048 window. Its
    /// form (as the victory screen's) has an opaque canvas of the whole
    /// window, black where it never draws, with its 1024 by 768 board in
    /// the middle. That board is its size: as a modal part (the game's
    /// modal stack) it is placed and scaled as the title's 1024 by 768,
    /// not as a window-sized dialog shrunk to fit the viewer's canvas.
    #[test]
    fn an_opaque_full_window_canvas_is_measured_by_its_board_not_its_backdrop() {
        let size = (3274, 2048);
        let clip = [0, 0, 3274, 2048];
        let big = |x, y, w, h| ScreenDraw::Fill { rect: [x, y, w, h], rgb: [1, 2, 3], alpha: 255, clip };
        let (x, y) = (1125, 640);
        let title = vec![big(0, 0, 3274, 2048), big(x, y, 1024, 768), big(1, 2010, 60, 20), big(1600, 2010, 80, 20)];
        let n = title.len();
        let board = ScreenDraw::Image {
            path: "Art/Advisors/hall_o_fame_background.pcx".into(), rect: [0, 0, 1024, 768], x, y,
            keyed: false, clip, mask: None, palette: None, opacity: 255,
        };
        let hall = |own: Vec<ScreenDraw>| {
            let mut draws = title.clone();
            draws.extend(own);
            let to = draws.len();
            Frame { draws, forms: vec![(0x11, 1, (0, 0), size), (0x66_D1F0, 2, (0, 0), size)],
                form_draws: vec![(1, 0..n), (2, n..to)], underlay_draws: vec![] }
        };
        let black = ScreenDraw::Fill { rect: [0, 0, 3274, 2048], rgb: [0, 0, 0], alpha: 255, clip };
        let f = hall(vec![black.clone(), board.clone(), big(x + 100, y + 200, 300, 20)]);
        assert_eq!(popup(&f, 2, None, size), Some((n, [x, y, 1024, 768])));
        let (below, parts) = split_modal_stack(&f, &[2], None, size);
        assert_eq!((below.draws.len(), parts.len(), parts[0].rect), (n, 1, [x, y, 1024, 768]));
        assert_eq!(parts[0].frame.draws.len(), 3, "the backdrop stays with its screen");
        // As large as the title under it: a screen of its own, no popup.
        assert_eq!(menu_popup(&f, 2, size), None);
        // A translucent shade of the whole window is a backdrop as well.
        let shade = ScreenDraw::Fill { rect: [0, 0, 3274, 2048], rgb: [0, 0, 0], alpha: 128, clip };
        assert_eq!(popup(&hall(vec![shade, board.clone()]), 2, None, size), Some((n, [x, y, 1024, 768])));
        // A full-window picture is content; a canvas showing nothing but
        // its clear keeps the whole window.
        let picture = ScreenDraw::Image { path: "Art/wide.pcx".into(), rect: [0, 0, 3274, 2048], x: 0, y: 0, keyed: false, clip, mask: None, palette: None, opacity: 255 };
        assert_eq!(popup(&hall(vec![black.clone(), picture, board]), 2, None, size), Some((n, [0, 0, 3274, 2048])));
        assert_eq!(popup(&hall(vec![black]), 2, None, size), Some((n, [0, 0, 3274, 2048])));
    }

    fn advisor_popup_frame() -> Frame {
        let mut f = frame();
        // The advisor portrait is drawn into the map canvas, before the
        // popup form. It must not remain a world overlay after moving it.
        f.draws.insert(1, ScreenDraw::Image {
            path: "C:\\Games\\Art\\SmallHeads\\popupSCIENCE.pcx".into(), rect: [0, 0, 180, 180], x: 840, y: 220,
            keyed: true, clip: [0, 0, 1920, 1080], mask: None, palette: None, opacity: 255,
        });
        f.form_draws = vec![(0x9F_8934, 0..3), (0xA0_68B4, 3..4), (0xA1_4A68, 4..5), (0x50_0000, 5..6), (0x50_1000, 6..7)];
        f.underlay_draws = vec![0..3];
        f
    }

    #[test]
    fn an_advisor_picture_on_the_map_moves_with_the_popup_only() {
        let f = advisor_popup_frame();
        let (first, rect) = popup(&f, 0x50_0000, None, (1920, 1080)).unwrap();
        assert_eq!((first, rect), (5, [700, 220, 500, 480]));
        let (below, above) = split_popup(&f, first, rect);
        assert!(!crate::ui::reaches_clip(&below.draws[1]));
        assert_eq!(above.draws, vec![f.draws[1].clone(), f.draws[5].clone(), f.draws[6].clone()]);
        assert!(above.underlay_draws.is_empty());
        assert_eq!(above.form_draws, [(0x50_0000, 1..2), (0x50_1000, 2..3)]);
        assert_eq!(below.draws[0], f.draws[0], "the map remains behind the dialog");
        // The same art on Main_GUI is a HUD draw rather than an underlay.
        // It still belongs to the popup and must share its transform.
        let mut f = f.clone();
        f.underlay_draws.clear();
        assert_eq!(popup(&f, 0x50_0000, None, (1920, 1080)), Some((first, rect)));
        let (below, above) = split_popup(&f, first, rect);
        assert!(!crate::ui::reaches_clip(&below.draws[1]));
        assert_eq!(above.draws.iter().filter(|d| advisor_portrait(d)).count(), 1);
        // A head owned by an underlying advisor stays on that advisor.
        f.forms.insert(1, (0x66_C270, 0x77_0000, (448, 156), (1024, 768)));
        f.form_draws[0].1 = 0..1;
        f.form_draws.insert(1, (0x77_0000, 1..3));
        assert_eq!(popup(&f, 0x50_0000, None, (1920, 1080)), Some((5, [700, 400, 500, 300])));
        let (below, above) = split_popup(&f, 5, [700, 400, 500, 300]);
        assert!(crate::ui::reaches_clip(&below.draws[1]));
        assert!(!above.draws.iter().any(advisor_portrait));
    }

    #[test]
    fn grouped_advisor_picture_stays_attached_with_strip_panes_and_ui_scales() {
        use tile_render::{AtlasBuilder, Grid, LayerSpec, PlacedSprite, RgbaImage, SpriteSets, TileWorld};
        use tile_viewer::{layout, scaling};
        use viewer_settings::{Place, Resolution, ScreenPlace, Split, Zoom};
        const GREEN: [u8; 4] = [20, 180, 40, 255];
        const RED: [u8; 4] = [220, 30, 40, 255];
        const BLUE: [u8; 4] = [30, 50, 220, 255];
        let mut atlas = AtlasBuilder::new(128);
        let ground = atlas.add(&RgbaImage::from_fn(8, 8, |_, _| GREEN), (0, 0)).unwrap();
        let head = atlas.add(&RgbaImage::from_fn(20, 30, |_, _| RED), (0, 0)).unwrap();
        let body = atlas.add(&RgbaImage::from_fn(60, 40, |_, _| BLUE), (0, 0)).unwrap();
        let atlas = atlas.finish();
        let mut sets = SpriteSets::default();
        let (ground, head, body) = (sets.single(ground), sets.single(head), sets.single(body));
        let mut world = TileWorld::new(Grid { cols: 128, rows: 128, layout: tile_render::Layout::Orthogonal, wrap_x: true, wrap_y: true, step: (8, 8) }, sets);
        world.add_layer(LayerSpec::new("ground", vec![ground]));
        for place in [Place::Floating, Place::LeftOrBottom, Place::RightOrTop] {
            for pixel_scale in [1, 2, 3] {
                for menu_scale in [1.0, 1.5] {
                    let mut s = scene::Scene { screen_size: (160, 120), shape: scene::WorldShape { cols: 128, rows: 128, wrap_x: true, wrap_y: true }, ..Default::default() };
                    s.view_mut(0).view.camera_x = 100;
                    s.view_mut(0).view.camera_y = 100;
                    part_view(&mut s, POPUP_VIEW, [50, 30, 60, 70], vec![
                        scene::FreeSprite { set: head.raw(), x: 65, y: 30, ..Default::default() },
                        scene::FreeSprite { set: body.raw(), x: 50, y: 60, ..Default::default() },
                    ]);
                    let mut settings = viewer_settings::Layout { split: Split::Vertical1To1, zoom: Zoom::TopStrip, ..Default::default() };
                    if place == Place::Floating {
                        s.interface.popup = Some(POPUP_VIEW);
                    } else {
                        // A popup embedded in a named screen is merged into
                        // that screen's view by the bridge.
                        s.interface.screen = Some("F6".into());
                        s.interface.screen_view = Some(POPUP_VIEW);
                        settings.screen_place = ScreenPlace { vertical: place, horizontal: place };
                    }
                    let mut plan = scaling::plan((480, 360), s.screen_size, &Resolution { pixel_scale, ..Default::default() });
                    plan.menu.zoom = scaling::ratio(menu_scale);
                    let mut l = layout::Layout::default();
                    l.apply_settings(&settings);
                    l.driver_screen = Some(plan.screen);
                    let mut f = l.frame(&s, plan.canvas(), (0, 0, 0, 0), vec![], None);
                    scaling::arrange(&plan, &s, &mut f, |sprite| Some(if sprite.set == head.raw() { [65, 30, 20, 30] } else { [50, 60, 60, 40] }));
                    let camera = f.view_cameras[1].unwrap();
                    assert!(!f.cameras[camera as usize].cells);
                    world.free = vec![
                        PlacedSprite { screen: true, view: Some(camera), ..PlacedSprite::new(head, 65, 30) },
                        PlacedSprite { screen: true, view: Some(camera), ..PlacedSprite::new(body, 50, 60) },
                    ];
                    let image = tile_render::reference::render_views(&world, &atlas, &f.cameras, plan.canvas(), 0, [0, 0, 0, 255]);
                    let mut head_bottom = None;
                    let mut body_top = None;
                    for y in 0..plan.canvas().1 {
                        for x in 0..plan.canvas().0 {
                            match image.get(x, y) {
                                RED => head_bottom = Some(y),
                                BLUE => { body_top.get_or_insert(y); },
                                _ => {},
                            }
                        }
                    }
                    let context = format!("{place:?}, pixel scale {pixel_scale}, menu scale {menu_scale}");
                    assert!(head_bottom.is_some() && body_top.is_some(), "{context}");
                    assert_eq!(head_bottom.unwrap() + 1, body_top.unwrap(), "the portrait stays directly above its dialog: {context}");
                }
            }
        }
    }

    #[test]
    fn a_named_screen_smaller_than_the_window_is_a_part_of_its_own() {
        let mut f = frame();
        // An advisor (1024x768 in the middle) over the map screen.
        f.forms.push((0x66_C270, 0x77_0000, (448, 156), (1024, 768)));
        f.draws.push(fill(448, 156, 1024, 768));
        f.form_draws.push((0x77_0000, 6..7));
        assert_eq!(screen_part(&f, f.forms.len() - 1, (1920, 1080)), Some((6, [448, 156, 1024, 768])));
        // Over the advisors' common background (the same rectangle): from it on.
        f.forms.insert(f.forms.len() - 1, (0x66_88B8, 0x76_0000, (448, 156), (1024, 768)));
        f.draws.insert(6, fill(448, 156, 1024, 768));
        f.form_draws = vec![(0x9F_8934, 0..2), (0xA0_68B4, 2..3), (0xA1_4A68, 3..4), (0x50_0000, 4..5), (0x50_1000, 5..6), (0x76_0000, 6..7), (0x77_0000, 7..8)];
        assert_eq!(screen_part(&f, f.forms.len() - 1, (1920, 1080)), Some((6, [448, 156, 1024, 768])));
        // Covering the whole window: not.
        f.forms.push((0x66_58B0, 0x78_0000, (0, 0), (1920, 1080)));
        f.draws.push(fill(0, 0, 1920, 1080));
        f.form_draws.push((0x78_0000, 7..8));
        assert_eq!(screen_part(&f, f.forms.len() - 1, (1920, 1080)), None);
    }

    #[test]
    fn every_named_screen_keeps_its_native_part_when_a_prompt_is_split_off_outside_it() {
        let build = civ3_sys::Build::PcGames;
        let size = (1920, 1080);
        for &(vtable, name) in civ3_sys::ui_hooks::screen_forms(build) {
            let canvas = 0x710000;
            let base = Frame {
                forms: vec![(0x66_B3D8, 1, (0, 0), size), (vtable, canvas, (448, 156), (1024, 768))],
                draws: vec![fill(0, 0, 1920, 1080), fill(448, 156, 1024, 768)],
                form_draws: vec![(1, 0..1), (canvas, 1..2)],
                underlay_draws: vec![0..1],
            };
            let before = active_screen_part(&base, 1, build, size);
            for rect in [[8, 2, 400, 160], [1400, 900, 510, 170], [200, 100, 1500, 600]] {
                let mut frame = base.clone();
                let [x, y, w, h] = rect;
                frame.forms.push((0x66_B240, 0x720000, (x, y), (w, h)));
                frame.draws.push(fill(x, y, w, h));
                frame.form_draws.push((0x720000, 2..3));
                let (first, body) = popup(&frame, 0x720000, Some(1), size).unwrap();
                let (below, above) = split_popup(&frame, first, body);
                assert_eq!(active_screen_part(&below, 1, build, size), before, "{name}, {rect:?}");
                assert_eq!(below.draws, base.draws, "{name}: popup art never joins parent art");
                assert_eq!(above.draws, frame.draws[2..], "{name}: popup remains independently owned");
            }
        }
    }

    #[test]
    fn nested_full_canvas_modals_keep_separate_bounds_and_leave_the_named_screen_unchanged() {
        let size = (1920, 1080);
        let screen_canvas = 0x710000;
        let mut frame = Frame {
            forms: vec![(0x66_B3D8, 1, (0, 0), size), (0x66_F420, screen_canvas, (0, 0), size)],
            draws: vec![fill(0, 0, 1920, 1080), fill(448, 156, 1024, 768)],
            form_draws: vec![(1, 0..1), (screen_canvas, 1..2)], underlay_draws: vec![0..1],
        };
        let baseline = active_screen_part(&frame, 1, civ3_sys::Build::PcGames, size);
        let native_rects = [[300, 100, 650, 300], [8, 2, 400, 160], [900, 10, 510, 240]];
        for (index, rect) in native_rects.into_iter().enumerate() {
            let canvas = 0x720000 + index as u32 * 0x1000;
            let [x,y,w,h] = rect;
            frame.forms.push((0x66_B240, canvas, (0, 0), size));
            let start = frame.draws.len();
            frame.draws.push(fill(x,y,w,h));
            frame.form_draws.push((canvas, start..start+1));
            let canvases: Vec<_> = (0..=index).map(|i| 0x720000 + i as u32 * 0x1000).collect();
            let (below, parts) = split_modal_stack(&frame, &canvases, Some(1), size);
            assert_eq!(active_screen_part(&below, 1, civ3_sys::Build::PcGames, size), baseline);
            assert_eq!(below.draws.len(), 2);
            assert_eq!(parts.len(), index + 1);
            for (i, part) in parts.iter().enumerate() {
                assert_eq!(part.rect, native_rects[i], "parent overlay bounds never include children");
                assert_eq!(part.frame.draws.len(), 1);
            }
        }
    }

    #[test]
    fn part_views_hold_their_sprites_on_top() {
        let mut s = scene::Scene::default();
        part_view(&mut s, CORNER_VIEW, [0, 0, 149, 54], vec![scene::FreeSprite { set: 3, x: 21, y: 12, ..Default::default() }]);
        let v = s.view(CORNER_VIEW).unwrap();
        assert_eq!((v.view.origin, v.view.viewport, v.view.interface_only), ((0, 0), (149, 54), true));
        assert!(v.overlay[0].screen && v.overlay[0].after_pass == u32::MAX);
    }
}
