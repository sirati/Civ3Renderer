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

fn intersect(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]
}

fn clip_mut(d: &mut ScreenDraw) -> &mut [i32; 4] {
    match d {
        ScreenDraw::Image { clip, .. } | ScreenDraw::Fill { clip, .. } | ScreenDraw::Text { clip, .. } | ScreenDraw::Flc { clip, .. } | ScreenDraw::Surface { clip, .. } => clip,
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
        ScreenDraw::Fill { rect, clip, .. } => ([rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]], clip),
        ScreenDraw::Text { run, clip } => ([run.x, run.y - 1, run.x + 1, run.y], clip),
        ScreenDraw::Flc { .. } => return None,
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
    part_from(frame, index, size)
}

/// The named screen (form `index` of `frame.forms`) as a part of its own,
/// where it is smaller than the screen of `size`: its first draw (it and
/// what is drawn over it) and its rectangle (x, y, w, h), its frame
/// included (as [`popup`]). The forms just below it of the same rectangle
/// are its background (the advisors' common one): they go with it.
pub fn screen_part(frame: &Frame, index: usize, size: (i32, i32)) -> Option<(usize, [i32; 4])> {
    let place = |f: &(u32, u32, (i32, i32), (i32, i32))| (f.2, f.3);
    let mut bottom = index;
    while bottom > 0 && place(&frame.forms[bottom - 1]) == place(&frame.forms[index]) {
        bottom -= 1;
    }
    let (first, rect) = (bottom..=index).find_map(|i| part_from(frame, i, size))?;
    (rect[2] < size.0 || rect[3] < size.1).then_some((first, rect))
}

/// Form `index` of `frame.forms` from its first draw on, with the frame the
/// game draws round it straight onto the screen just before it.
fn part_from(frame: &Frame, index: usize, size: (i32, i32)) -> Option<(usize, [i32; 4])> {
    let (_, canvas, (x, y), (w, h)) = frame.forms[index];
    let mut first = frame.form_draws.iter().find(|(c, _)| *c == canvas).map(|(_, r)| r.start)?;
    let mut bounds = [x, y, x + w, y + h];
    let near = [x - POPUP_FRAME, y - POPUP_FRAME, x + w + POPUP_FRAME, y + h + POPUP_FRAME];
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
        let band = ScreenDraw::Image { path: "popupborders.pcx".into(), rect: [251, 1, 60, 35], x: 690, y: 365, keyed: true, clip: [i32::MIN, i32::MIN, i32::MAX, i32::MAX], mask: None, palette: None };
        f.draws.insert(4, fill(10, 10, 20, 20));
        f.draws.insert(5, band);
        f.form_draws = vec![(0x9F_8934, 0..2), (0xA0_68B4, 2..3), (0xA1_4A68, 3..4), (0x50_0000, 6..7), (0x50_1000, 7..8)];
        assert_eq!(popup(&f, 0x50_0000, None, (1920, 1080)), Some((5, [690, 365, 510, 335])));
        // Under a screen composited above it: not a popup of its own.
        assert_eq!(popup(&f, 0x50_0000, Some(7), (1920, 1080)), None);
        assert_eq!(popup(&f, 0x12_3456, None, (1920, 1080)), None);
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
    fn part_views_hold_their_sprites_on_top() {
        let mut s = scene::Scene::default();
        part_view(&mut s, CORNER_VIEW, [0, 0, 149, 54], vec![scene::FreeSprite { set: 3, x: 21, y: 12, ..Default::default() }]);
        let v = s.view(CORNER_VIEW).unwrap();
        assert_eq!((v.view.origin, v.view.viewport, v.view.interface_only), ((0, 0), (149, 54), true));
        assert!(v.overlay[0].screen && v.overlay[0].after_pass == u32::MAX);
    }
}
