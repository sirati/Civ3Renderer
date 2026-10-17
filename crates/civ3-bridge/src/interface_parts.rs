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

/// The popup open in `frame`: the modal form (its canvas `canvas`) when it
/// is composited above the screen the viewer places (`screen`, an index
/// into `frame.forms`) and is none itself. Its first draw (it and what is
/// composited over it: its controls, the cursor) and its rectangle
/// (x, y, w, h) on the screen of `size`.
pub fn popup(frame: &Frame, canvas: u32, screen: Option<usize>, size: (i32, i32)) -> Option<(usize, [i32; 4])> {
    let index = frame.forms.iter().rposition(|f| f.1 == canvas)?;
    if screen.is_some_and(|s| index <= s) {
        return None;
    }
    let (_, _, (x, y), (w, h)) = frame.forms[index];
    let [l, t, r, b] = intersect([x, y, x + w, y + h], [0, 0, size.0, size.1]);
    if r <= l || b <= t {
        return None;
    }
    let first = frame.form_draws.iter().find(|(c, _)| *c == canvas).map(|(_, r)| r.start)?;
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
        // Under a screen composited above it: not a popup of its own.
        assert_eq!(popup(&f, 0x50_0000, Some(7), (1920, 1080)), None);
        assert_eq!(popup(&f, 0x12_3456, None, (1920, 1080)), None);
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
