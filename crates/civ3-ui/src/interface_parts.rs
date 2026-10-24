//! The parts of the game's interface the viewer arranges itself
//! (`scene::Interface`, docs §16.12), from the frame alone: the tooltip,
//! the corner's buttons, the open popup and the named screen's part,
//! where a popup opened at a map click goes, and the capture hint. Which
//! form is which screen, by build (the game's tables), stays with the
//! bridge's `interface_parts`.

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

/// The rectangle (x, y, w, h) of the tooltip in `frame`: what the game's
/// present drew over everything (`Frame::overlay_draws`), none if nothing.
pub fn tooltip(frame: &Frame) -> Option<[i32; 4]> {
    let [l, t, r, b] = frame.draws.get(frame.overlay_draws.clone())?.iter().filter_map(reach)
        .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])?;
    Some([l, t, r - l, b - t])
}

/// The tooltip's draws (`Frame::overlay_draws`) clipped to [`tooltip`]'s
/// rectangle: its text is drawn in its box, where the game sized it to
/// hold it, and the viewer knows all of it by that rectangle (a run's
/// letters reach above and below their baseline further than the box,
/// so unclipped, they were not the tooltip's).
pub fn clip_tooltip(frame: &mut Frame) {
    let Some([x, y, w, h]) = tooltip(frame) else { return };
    let range = frame.overlay_draws.clone();
    for d in frame.draws.get_mut(range).into_iter().flatten() {
        let clip = clip_mut(d);
        *clip = intersect(*clip, [x, y, x + w, y + h]);
    }
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

pub fn intersect(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]
}

pub fn clip_mut(d: &mut ScreenDraw) -> &mut [i32; 4] {
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
    Frame { overlay_draws: 0..0, draws, forms: frame.forms.clone(), form_draws: Vec::new(), underlay_draws: Vec::new() }
}

/// How far from a popup's canvas the frame the game draws around it (its
/// border, its title band) may reach.
const POPUP_FRAME: i32 = 48;

/// The rectangle (x0, y0, x1, y1) draw `d` reaches on the screen, clipped
/// (a text by its pen's point).
pub fn reach(d: &ScreenDraw) -> Option<[i32; 4]> {
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
pub fn advisor_portrait(draw: &ScreenDraw) -> bool {
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
        if i < first && !map_or_unowned && !head_form(frame, i, first) { return None; }
        let r = reach(draw)?;
        let overlap = intersect(r, near);
        (overlap[2] > overlap[0] && overlap[3] > overlap[1]).then_some(i)
    }).collect()
}

/// Draw `i` (an advisor picture before the popup's first draw `first`) is
/// on a form of its own that the game composes for the popup's head: the
/// form holds advisor pictures only, and no other form draws between it
/// and the popup (only the popup's own frame, drawn straight onto the
/// screen, or another such head form). The Science Advisor's "our
/// Researchers need direction" shows its head that way: a 148x109 form
/// (vtable 0x671218) whose bottom edge meets the dialog's top border,
/// composed just before the dialog. An advisor screen's head under a
/// dialog is not: that form also holds the screen.
fn head_form(frame: &Frame, i: usize, first: usize) -> bool {
    let owner = |i: usize| frame.form_draws.iter().find(|(_, r)| r.contains(&i)).map(|(c, _)| *c);
    let only_heads = |canvas: u32| frame.form_draws.iter().filter(|(c, _)| *c == canvas)
        .flat_map(|(_, r)| r.clone()).all(|j| frame.draws.get(j).is_some_and(advisor_portrait));
    let Some(canvas) = owner(i) else { return false };
    only_heads(canvas) && (i + 1..first).all(|j| owner(j).is_none_or(only_heads))
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
pub fn part_from(frame: &Frame, index: usize, size: (i32, i32), modal: bool) -> Option<(usize, [i32; 4])> {
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
        // window-sized, and the viewer shrank all of it to fit. For a
        // modal part every fill of the whole window is such a backdrop.
        // For a named screen the black clear its canvas starts with: the
        // histograph (F8) in a game is a canvas of the whole window,
        // cleared black when it is made (`ui_trace::initialized_canvas`),
        // its board in the middle; measured by that clear it was no part
        // at all, and the viewer fitted the whole black window into the
        // screen's place, a small board in wide black margins. Art that
        // covers the window after it (the city screen's, an advisor's
        // board in a window of its size) still measures the screen.
        let whole = |d: &ScreenDraw| matches!(d, ScreenDraw::Fill { .. })
            && reach(d).is_some_and(|r| r[0] <= 0 && r[1] <= 0 && r[2] >= size.0 && r[3] >= size.1);
        let clear = frame.draws[first..].iter().take_while(|d| whole(d) && matches!(d, ScreenDraw::Fill { rgb: [0, 0, 0], .. })).count();
        let backdrop = |i: usize, d: &ScreenDraw| if modal { whole(d) } else { i < clear };
        let content = |all: bool| frame.draws[first..].iter().enumerate().filter(|&(i, d)| all || !backdrop(i, d)).filter_map(|(_, d)| reach(d))
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
        overlay_draws: 0..0,
        draws: frame.draws[..first].to_vec(),
        forms: frame.forms.clone(),
        form_draws: frame.form_draws.iter().filter(|(_, r)| r.end <= first).cloned().collect(),
        underlay_draws: frame.underlay_draws.iter().map(|r| r.start.min(first)..r.end.min(first)).filter(|r| !r.is_empty()).collect(),
    };
    let above = Frame {
        overlay_draws: 0..0,
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
            Frame { overlay_draws: 0..0, draws: all, forms: vec![(0x11, 1, (0, 0), size), (0x22, 2, (x, y), (1024, 768))],
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

    #[test]
    fn part_views_hold_their_sprites_on_top() {
        let mut s = scene::Scene::default();
        part_view(&mut s, CORNER_VIEW, [0, 0, 149, 54], vec![scene::FreeSprite { set: 3, x: 21, y: 12, ..Default::default() }]);
        let v = s.view(CORNER_VIEW).unwrap();
        assert_eq!((v.view.origin, v.view.viewport, v.view.interface_only), ((0, 0), (149, 54), true));
        assert!(v.overlay[0].screen && v.overlay[0].after_pass == u32::MAX);
    }
}
