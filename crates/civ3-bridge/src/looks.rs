//! The game's controls in their other states (`scene::Looks`): what a
//! button looks like hovered, pressed or not, so that the viewer can draw
//! that the moment the pointer asks for it rather than wait for the game to
//! redraw the button and the mirror to follow.
//!
//! A control's own draws on the composed screen are those of its canvas
//! (`ui::Frame::form_draws`). The game's fields name the sprite each piece
//! of its art is drawn from in each state (`civ3_sys::control_art`); the
//! mirror knows the image region each sprite is. A state's look is the
//! control's draws with each piece's region swapped for that state's: the
//! game's own art, placed as the game places it, label and all.

use scene::{FreeSprite, LookState, PointerState, Select};

use crate::ui::{Frame, ScreenDraw, Source};

/// A look of `civ3_sys::control_art` as the scene has it.
pub fn look_state(l: civ3_sys::control_art::Look) -> LookState {
    use civ3_sys::control_art::Pointer;
    let pointer = match l.pointer {
        Pointer::Normal => PointerState::Normal,
        Pointer::Hover => PointerState::Hover,
        Pointer::Pressed => PointerState::Pressed,
    };
    LookState { pointer, selected: l.selected }
}

/// A control's art by state, the game's sprites resolved to the image
/// regions the mirror knows them as.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlArt {
    /// The interactable's id.
    pub id: u64,
    pub select: Select,
    /// The canvas the control is drawn on.
    pub canvas: u32,
    /// Which of the canvas' draws are the control's.
    pub part: Part,
    /// The state the game's fields say it is in (preferred where its draws
    /// would fit several).
    pub state: LookState,
    /// Per piece of its art, the region it is drawn from in each state it
    /// has art for.
    pub pieces: Vec<Vec<(LookState, Source)>>,
    /// How far its text moves in a state (x, y), where it does.
    pub text_offsets: Vec<(LookState, (i32, i32))>,
    /// The colours its text and fills have in a state, where they change: the
    /// i-th colour of one state becomes the i-th of another.
    pub colors: Vec<(LookState, Vec<[u8; 3]>)>,
    /// Whether its fields say whether it is selected (its draws need not:
    /// a check box just clicked shows its hover art).
    pub selected_by_fields: bool,
}

/// Which of a canvas' draws are a control's.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// All of them.
    Whole,
    /// The last draw of its art within this rectangle of the screen (x, y,
    /// w, h): a tab or box drawn over what was there.
    LastArt([i32; 4]),
    /// All within this rectangle: a row redrawn as a whole.
    Within([i32; 4]),
}

/// A control's draws as the game shows it, and in its other states (the
/// game's draws on the screen, or another kind of draws).
#[derive(Clone, Debug, PartialEq)]
pub struct ControlLooks<D = Vec<ScreenDraw>> {
    pub id: u64,
    pub select: Select,
    pub shown: LookState,
    pub draws: D,
    pub states: Vec<(LookState, D)>,
}

/// Whether region `rect` of `path` is the image region `s`.
fn is_region(path: &str, rect: [i32; 4], s: &Source) -> bool {
    matches!(s, Source::File { retained:None,path: p, rect: r, .. } if p == path && *r == rect)
}

/// The colour of a text or fill.
fn draw_color(d: &ScreenDraw) -> Option<[u8; 3]> {
    match d {
        ScreenDraw::Text { run, .. } => Some(run.color),
        ScreenDraw::Fill { rgb, .. } => Some(*rgb),
        _ => None,
    }
}

/// Whether `d` draws the image region `s`.
fn draws_region(d: &ScreenDraw, s: &Source) -> bool {
    matches!(d, ScreenDraw::Image { path, rect, .. } | ScreenDraw::ResizedImage { path, rect, .. } if is_region(path, *rect, s))
}

impl ControlArt {
    /// The states it has art for, in order of first appearance.
    fn states(&self) -> Vec<LookState> {
        let mut out: Vec<LookState> = Vec::new();
        for s in self.pieces.iter().flatten().map(|(s, _)| s).chain(self.colors.iter().map(|(s, _)| s)) {
            if !out.contains(s) {
                out.push(*s);
            }
        }
        out
    }

    /// Whether `draws` show it in `state`: each piece's region of that state
    /// is drawn; without pieces, its first colour of that state is.
    fn shows(&self, draws: &[ScreenDraw], state: LookState) -> bool {
        if self.pieces.is_empty() {
            let Some(c) = self.colors.iter().find(|(s, _)| *s == state).and_then(|(_, c)| c.first()) else { return false };
            return draws.iter().any(|d| draw_color(d) == Some(*c));
        }
        self.pieces.iter().all(|p| p.iter().find(|(s, _)| *s == state).is_some_and(|(_, src)| draws.iter().any(|d| draws_region(d, src))))
    }

    /// `draws` (showing it in `from`) as they would be in `to`: each piece's
    /// region swapped for its region in `to`, the same size; `None` if a
    /// piece has no art for `to`, or the swap is not one region for another.
    fn swapped(&self, draws: &[ScreenDraw], from: LookState, to: LookState) -> Option<Vec<ScreenDraw>> {
        let mut swaps: Vec<(&Source, &Source)> = Vec::new();
        for p in &self.pieces {
            let a = &p.iter().find(|(s, _)| *s == from)?.1;
            let b = &p.iter().find(|(s, _)| *s == to)?.1;
            match swaps.iter().find(|(x, _)| x == &a) {
                Some((_, y)) if y != &b => return None,
                Some(_) => {}
                None => swaps.push((a, b)),
            }
        }
        let offset = |s: LookState| self.text_offsets.iter().find(|(t, _)| *t == s).map_or((0, 0), |(_, o)| *o);
        let colors = |s: LookState| self.colors.iter().find(|(t, _)| *t == s).map(|(_, c)| c);
        // Each colour of `from` to the one of `to` at its place (one each).
        let recolor: Vec<([u8; 3], [u8; 3])> = match (colors(from), colors(to)) {
            (Some(a), Some(b)) if a.len() == b.len() => {
                let pairs: Vec<_> = a.iter().copied().zip(b.iter().copied()).collect();
                if pairs.iter().any(|p| pairs.iter().any(|q| p.0 == q.0 && p.1 != q.1)) {
                    return None;
                }
                pairs
            }
            (None, None) => Vec::new(),
            _ => return None,
        };
        let recolored = |c: [u8; 3]| recolor.iter().find(|p| p.0 == c).map_or(c, |p| p.1);
        let ((fx, fy), (tx, ty)) = (offset(from), offset(to));
        let mut out = draws.to_vec();
        for d in &mut out {
            match d {
                ScreenDraw::Image { path, rect, .. } | ScreenDraw::ResizedImage { path, rect, .. } => {
                    let Some((_, to)) = swaps.iter().find(|(from, _)| is_region(path, *rect, from)) else { continue };
                    let Source::File { retained:None,path: p, rect: r, .. } = to else { return None };
                    if r[2] != rect[2] || r[3] != rect[3] {
                        return None;
                    }
                    (*path, *rect) = (p.clone(), *r);
                }
                ScreenDraw::Text { run, .. } => {
                    run.x += tx - fx;
                    run.y += ty - fy;
                    run.color = recolored(run.color);
                }
                ScreenDraw::Fill { rgb, .. } => *rgb = recolored(*rgb),
                _ => {}
            }
        }
        Some(out)
    }

    /// Its draws among `draws` (those of its canvas): all of them, or for a
    /// part, the last draw of its art within its rectangle.
    fn own(&self, draws: &[ScreenDraw]) -> Option<Vec<ScreenDraw>> {
        let at = |d: &ScreenDraw| match d {
            ScreenDraw::Image { x, y, .. } | ScreenDraw::ResizedImage { x, y, .. } | ScreenDraw::Flc { x, y, .. } | ScreenDraw::Surface { x, y, .. } | ScreenDraw::Indexed { x, y, .. } => (*x, *y),
            ScreenDraw::Fill { rect, .. } => (rect[0], rect[1]),
            ScreenDraw::Text { run, .. } => (run.x, run.y),
        };
        let inside = |d: &ScreenDraw, [x, y, w, h]: [i32; 4]| {
            let (px, py) = at(d);
            px >= x && py >= y && px < x + w && py < y + h
        };
        let art = |d: &ScreenDraw| self.pieces.iter().flatten().any(|(_, s)| draws_region(d, s));
        match self.part {
            Part::Whole => Some(draws.to_vec()),
            Part::LastArt([x, y, w, h]) => draws.iter().rev().find(|d| {
                // A control's clickable portion can be clipped or inset
                // from the art's origin, as the status orb is by one row.
                // Match the visible intersection of its state image.
                let touches = match d {
                    ScreenDraw::Image { rect, x: ix, y: iy, clip, .. } => {
                        let (l, t) = ((*ix).max(clip[0]).max(x), (*iy).max(clip[1]).max(y));
                        let (r, b) = ((ix + rect[2]).min(clip[2]).min(x + w), (iy + rect[3]).min(clip[3]).min(y + h));
                        r > l && b > t
                    }
                    ScreenDraw::ResizedImage { size: (dw, dh), x: ix, y: iy, clip, .. } => {
                        let (l, t) = ((*ix).max(clip[0]).max(x), (*iy).max(clip[1]).max(y));
                        let (r, b) = ((ix + dw).min(clip[2]).min(x + w), (iy + dh).min(clip[3]).min(y + h));
                        r > l && b > t
                    }
                    _ => false,
                };
                touches && art(d)
            }).map(|d| vec![d.clone()]),
            Part::Within(r) => {
                let own: Vec<ScreenDraw> = draws.iter().filter(|d| inside(d, r)).cloned().collect();
                (!own.is_empty() && (self.pieces.is_empty() || own.iter().any(art))).then_some(own)
            }
        }
    }
}

/// The looks of the control `art` in `frame` (none if it is not composed
/// there, or its draws show none of its states: then it is drawn otherwise,
/// e.g. disabled, and stays the game's).
pub fn control_looks(frame: &Frame, art: &ControlArt) -> Option<ControlLooks> {
    let range = frame.form_draws.iter().find(|(c, _)| *c == art.canvas)?.1.clone();
    let draws = art.own(frame.draws.get(range)?)?;
    let states = art.states();
    // The state its draws show (the fields' where they fit; one as selected
    // as the fields say before others).
    let from = if art.shows(&draws, art.state) {
        art.state
    } else {
        let fits: Vec<LookState> = states.iter().copied().filter(|&s| art.shows(&draws, s)).collect();
        fits.iter().copied().find(|s| s.selected == art.state.selected).or(fits.first().copied())?
    };
    let shown = if art.selected_by_fields { LookState { selected: art.state.selected, ..from } } else { from };
    let others = states.into_iter().filter(|&s| s != shown).filter_map(|s| Some((s, art.swapped(&draws, from, s)?))).collect::<Vec<_>>();
    (!others.is_empty()).then_some(ControlLooks { id: art.id, select: art.select, shown, draws, states: others })
}

/// Where `needle` lies in `hay` (its last place: drawn on top), as a range.
fn find(hay: &[FreeSprite], needle: &[FreeSprite]) -> Option<(u32, u32)> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).rev().find(|&i| hay[i] == needle[0] && hay[i..i + needle.len()] == *needle).map(|i| (i as u32, (i + needle.len()) as u32))
}

/// What `draws` of the screen show in a view showing part `part` of it
/// (x, y, w, h; none: all of it): clipped to it.
pub fn in_part(draws: &[ScreenDraw], part: Option<[i32; 4]>) -> Vec<ScreenDraw> {
    match part {
        Some(r) => crate::interface_parts::region(&Frame { draws: draws.to_vec(), ..Default::default() }, r).draws,
        None => draws.to_vec(),
    }
}

/// The scene's looks of `controls`: for each of `views` (a view id and, for
/// a view showing part of the screen, that rectangle x, y, w, h), where
/// its overlay draws the control and the control's sprites in its other
/// states there. `sprites_of` makes overlay sprites of draws in such a
/// view, as the scene's overlays were made, `None` where the art is not
/// complete (a state without is left out).
pub fn scene_looks<D>(
    scene: &scene::Scene,
    controls: &[ControlLooks<D>],
    views: &[(u32, Option<[i32; 4]>)],
    sprites_of: impl Fn(&D, Option<[i32; 4]>) -> Option<Vec<FreeSprite>>,
) -> Vec<scene::Looks> {
    let mut out = Vec::new();
    for c in controls {
        let mut looks = scene::Looks { id: c.id, select: c.select, shown: c.shown, views: Vec::new() };
        for &(view, part) in views {
            let Some(overlay) = scene.view(view).map(|v| &v.overlay) else { continue };
            let Some(own) = sprites_of(&c.draws, part) else { continue };
            let Some(sprites) = find(overlay, &own) else { continue };
            let states: Vec<_> = c.states.iter().filter_map(|(s, d)| Some((*s, sprites_of(d, part)?))).collect();
            if !states.is_empty() {
                looks.views.push(scene::ViewLooks { view, sprites, states });
            }
        }
        if !looks.views.is_empty() {
            out.push(looks);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NORMAL: LookState = LookState { pointer: PointerState::Normal, selected: false };
    const HOVER: LookState = LookState { pointer: PointerState::Hover, selected: false };
    const PRESSED: LookState = LookState { pointer: PointerState::Pressed, selected: false };

    fn region(y: i32) -> Source {
        Source::File { retained:None,path: "C:\\Civ3\\Art\\buttons.pcx".into(), rect: [0, y, 32, 32], mask: None, palette: None, opacity: 255 }
    }

    fn image(y: i32, at: (i32, i32)) -> ScreenDraw {
        ScreenDraw::Image { retained:None,path: "C:\\Civ3\\Art\\buttons.pcx".into(), rect: [0, y, 32, 32], x: at.0, y: at.1, keyed: true, clip: [0, 0, 1024, 768], mask: None, palette: None, opacity: 255 }
    }

    fn fill(x: i32) -> ScreenDraw {
        ScreenDraw::Fill { rect: [x, 0, 4, 4], rgb: [1, 2, 3], alpha: 255, clip: [0, 0, 1024, 768] }
    }

    fn art() -> ControlArt {
        ControlArt {
            id: 7,
            select: Select::None,
            canvas: 0x500,
            part: Part::Whole,
            colors: vec![],
            state: NORMAL,
            pieces: vec![vec![(NORMAL, region(0)), (HOVER, region(32)), (PRESSED, region(64))]],
            text_offsets: vec![],
            selected_by_fields: false,
        }
    }

    /// A panel, the button (its image and its label's fill) and what is
    /// drawn over it.
    fn frame(button_y: i32) -> Frame {
        Frame {
            overlay_draws: 0..0,
            draws: vec![fill(0), image(button_y, (100, 200)), fill(110), fill(500)],
            forms: vec![],
            form_draws: vec![(0x400, 0..1), (0x500, 1..3), (0x600, 3..4)],
            underlay_draws: vec![],
        }
    }

    #[test]
    fn a_buttons_other_states_swap_its_image() {
        let looks = control_looks(&frame(0), &art()).unwrap();
        assert_eq!(looks.shown, NORMAL);
        assert_eq!(looks.draws, [image(0, (100, 200)), fill(110)]);
        assert_eq!(looks.states, [(HOVER, vec![image(32, (100, 200)), fill(110)]), (PRESSED, vec![image(64, (100, 200)), fill(110)])]);
        // Drawn hovered (the fields still say normal): the draws decide.
        let looks = control_looks(&frame(32), &art()).unwrap();
        assert_eq!(looks.shown, HOVER);
        assert_eq!(looks.states[0], (NORMAL, vec![image(0, (100, 200)), fill(110)]));
        // Drawn otherwise (e.g. disabled art), or not composed: none.
        assert_eq!(control_looks(&frame(96), &art()), None);
        assert_eq!(control_looks(&frame(0), &ControlArt { canvas: 0x700, ..art() }), None);
    }

    #[test]
    fn status_orb_hover_art_can_start_above_its_native_click_region() {
        let f = Frame { overlay_draws: 0..0, draws: vec![fill(0), image(0, (100, 200)), fill(500)],
            forms: vec![], form_draws: vec![(0x500, 0..3)], underlay_draws: vec![] };
        let a = ControlArt { part: Part::LastArt([100, 201, 32, 28]), ..art() };
        let got = control_looks(&f, &a).unwrap();
        assert_eq!(got.draws, [image(0, (100, 200))]);
        assert_eq!(got.states, [(HOVER, vec![image(32, (100, 200))]), (PRESSED, vec![image(64, (100, 200))])]);
    }

    #[test]
    fn a_pressed_buttons_label_moves() {
        let run = crate::text::TextRun { x: 104, y: 210, color: [0, 0, 0], face: "Arial".into(), height: -12, weight: 400, italic: false, underline: false, text: "OK".into(), backdrop: None };
        let text = |dx: i32| ScreenDraw::Text { run: crate::text::TextRun { x: 104 + dx, y: 210 + dx, ..run.clone() }, clip: [0, 0, 1024, 768] };
        let f = Frame { overlay_draws: 0..0, draws: vec![image(0, (100, 200)), text(0)], forms: vec![], form_draws: vec![(0x500, 0..2)], underlay_draws: vec![] };
        let a = ControlArt { text_offsets: vec![(PRESSED, (2, 2))], ..art() };
        let looks = control_looks(&f, &a).unwrap();
        assert_eq!(looks.states[1], (PRESSED, vec![image(64, (100, 200)), text(2)]));
        assert_eq!(looks.states[0], (HOVER, vec![image(32, (100, 200)), text(0)]));
    }

    #[test]
    fn a_part_is_its_last_draw_and_its_selection_the_fields() {
        let sel = LookState { pointer: PointerState::Normal, selected: true };
        let sel_hover = LookState { pointer: PointerState::Hover, selected: true };
        // A check box: unchecked, hovered, checked (hovered checked shows checked).
        let a = ControlArt {
            select: Select::Toggle,
            part: Part::LastArt([100, 200, 16, 16]),
            state: sel_hover,
            pieces: vec![vec![(NORMAL, region(0)), (HOVER, region(32)), (sel, region(64)), (sel_hover, region(64))]],
            selected_by_fields: true,
            ..art()
        };
        // Its canvas: the box unchecked, then hovered over it, and another box.
        let f = Frame {
            overlay_draws: 0..0,
            draws: vec![image(0, (100, 200)), image(0, (100, 220)), image(32, (100, 200))],
            forms: vec![],
            form_draws: vec![(0x500, 0..3)],
            underlay_draws: vec![],
        };
        // Just clicked: checked, its hover art still drawn.
        let looks = control_looks(&f, &a).unwrap();
        assert_eq!(looks.draws, [image(32, (100, 200))]);
        assert_eq!(looks.shown, sel_hover);
        let states: Vec<_> = looks.states.iter().map(|(s, d)| (*s, d.clone())).collect();
        assert_eq!(states, [(NORMAL, vec![image(0, (100, 200))]), (HOVER, vec![image(32, (100, 200))]), (sel, vec![image(64, (100, 200))])]);
    }

    #[test]
    fn a_menu_rows_fill_text_and_shadow_change_with_its_state() {
        let run = |x: i32, color: [u8; 3]| ScreenDraw::Text {
            run: crate::text::TextRun { x, y: 20, color, face: "Arial".into(), height: -12, weight: 0, italic: false, underline: false, text: "Fortify".into(), backdrop: None },
            clip: [0, 0, 1024, 768],
        };
        let fill = |y: i32, rgb: [u8; 3]| ScreenDraw::Fill { rect: [500, y, 120, 18], rgb, alpha: 255, clip: [0, 0, 1024, 768] };
        let a = ControlArt {
            part: Part::Within([500, 12, 120, 18]),
            pieces: vec![],
            colors: vec![(NORMAL, vec![[200, 200, 200], [0, 0, 0], [90, 90, 90]]), (HOVER, vec![[0, 0, 128], [255, 255, 255], [0, 0, 0]])],
            ..art()
        };
        let f = Frame {
            overlay_draws: 0..0,
            draws: vec![fill(12, [200, 200, 200]), run(511, [90, 90, 90]), run(510, [0, 0, 0]), fill(30, [200, 200, 200])],
            forms: vec![],
            form_draws: vec![(0x500, 0..4)],
            underlay_draws: vec![],
        };
        let looks = control_looks(&f, &a).unwrap();
        assert_eq!(looks.shown, NORMAL);
        assert_eq!(looks.states, [(HOVER, vec![fill(12, [0, 0, 128]), run(511, [0, 0, 0]), run(510, [255, 255, 255])])]);
    }

    #[test]
    fn a_rows_bullet_and_text_colour_change_with_its_state() {
        let run = |x: i32, y: i32, color: [u8; 3]| ScreenDraw::Text {
            run: crate::text::TextRun { x, y, color, face: "Lucida Sans".into(), height: -16, weight: 0, italic: false, underline: false, text: "Map".into(), backdrop: None },
            clip: [0, 0, 1024, 768],
        };
        let strip = |y: i32| ScreenDraw::Image { retained:None,path: "C:\\Civ3\\Art\\bg.pcx".into(), rect: [25, y, 314, 20], x: 803, y, keyed: false, clip: [0, 0, 1024, 768], mask: None, palette: None, opacity: 255 };
        let sel = LookState { pointer: PointerState::Normal, selected: true };
        let a = ControlArt {
            select: Select::Pick { group: 9 },
            part: Part::Within([803, 189, 314, 20]),
            state: NORMAL,
            pieces: vec![vec![(NORMAL, region(0)), (HOVER, region(32)), (sel, region(64))]],
            colors: vec![(NORMAL, vec![[0, 0, 0]]), (HOVER, vec![[176, 88, 0]]), (sel, vec![[0, 112, 184]])],
            selected_by_fields: true,
            ..art()
        };
        // Two rows: the first selected, the second not.
        let f = Frame {
            overlay_draws: 0..0,
            draws: vec![strip(163), image(64, (803, 163)), run(823, 179, [0, 112, 184]), strip(189), image(0, (803, 189)), run(823, 205, [0, 0, 0])],
            forms: vec![],
            form_draws: vec![(0x500, 0..6)],
            underlay_draws: vec![],
        };
        let looks = control_looks(&f, &a).unwrap();
        assert_eq!(looks.draws, [strip(189), image(0, (803, 189)), run(823, 205, [0, 0, 0])]);
        let hover = &looks.states.iter().find(|(s, _)| *s == HOVER).unwrap().1;
        assert_eq!(hover, &[strip(189), image(32, (803, 189)), run(823, 205, [176, 88, 0])]);
        let chosen = &looks.states.iter().find(|(s, _)| *s == sel).unwrap().1;
        assert_eq!(chosen, &[strip(189), image(64, (803, 189)), run(823, 205, [0, 112, 184])]);
    }

    #[test]
    fn a_state_whose_art_is_another_size_or_missing_is_left_out() {
        let mut a = art();
        a.pieces[0][2].1 = Source::File { retained:None,path: "C:\\Civ3\\Art\\buttons.pcx".into(), rect: [0, 64, 30, 32], mask: None, palette: None, opacity: 255 };
        a.pieces[0].remove(1);
        let looks = control_looks(&frame(0), &a);
        assert_eq!(looks, None);
    }

    #[test]
    fn the_scene_says_where_each_view_draws_it() {
        let looks = control_looks(&frame(0), &art()).unwrap();
        // Sprites of draws: one per draw, its set the image's y (fills 1000 + x), at its place.
        let sprites_of = |draws: &[ScreenDraw]| -> Vec<FreeSprite> {
            draws
                .iter()
                .filter_map(|d| match d {
                    ScreenDraw::Image { rect, x, y, clip, .. } => Some(FreeSprite { set: rect[1] as u32, x: *x, y: *y, clip: Some(*clip), ..Default::default() }),
                    ScreenDraw::Fill { rect, clip, .. } => Some(FreeSprite { set: 1000 + rect[0] as u32, clip: Some(*clip), ..Default::default() }),
                    _ => None,
                })
                .collect()
        };
        let mut scene = scene::Scene::default();
        scene.view_mut(1).overlay = sprites_of(&frame(0).draws);
        // The corner view: a copy of the screen's top-left, clipped to it.
        scene.view_mut(2).overlay = sprites_of(&crate::interface_parts::region(&frame(0), [0, 0, 300, 300]).draws);
        let got = scene_looks(&scene, &[looks], &[(1, None), (2, Some([0, 0, 300, 300])), (3, None)], |d, part| Some(sprites_of(&in_part(d, part))));
        assert_eq!(got.len(), 1);
        let views: Vec<(u32, (u32, u32))> = got[0].views.iter().map(|v| (v.view, v.sprites)).collect();
        assert_eq!(views, [(1, (1, 3)), (2, (1, 3))]);
        assert_eq!(got[0].views[1].states[0].1[0].clip, Some([0, 0, 300, 300]));
        assert_eq!(got[0].views[0].states[0].1[0].set, 32);
    }
}
