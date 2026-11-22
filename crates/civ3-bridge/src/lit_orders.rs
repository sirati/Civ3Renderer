//! The order buttons drawn lit while their command waits for a tile, as
//! the game's mode says (`civ3_sys::lit_orders`), whether the command came
//! from a click on the button, the viewer's button snapping or a key.
//!
//! The mirrored interface is the game's drawing: a button the mode lights
//! gets its art swapped there, one image region for another of the same
//! size, before anything is made of the frame. Such a button has no looks
//! of its own then (`crate::looks`): the viewer would draw it hovered or
//! not over the lit art, as for a button the game latches.

use civ3_sys::lit_orders::Lit;

use crate::ui::{Frame, ScreenDraw, Source};

/// Every command's order button in every state (`images`, the game's
/// table of them, `civ3_sys::lit_orders::order_images`), drawn as one of
/// `buttons` (their forms) is drawn in `frame`: so that the art of each
/// is made before any button shows it, its hover state included. `None`
/// while no order button is drawn.
pub fn all_states(frame: &Frame, buttons: &[u32], images: &[u32], sources: impl Fn(&[u32]) -> Vec<Option<Source>>) -> Option<Vec<ScreenDraw>> {
    let template = buttons.iter().find_map(|&b| {
        let canvas = b + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4;
        let range = frame.form_draws.iter().find(|(c, _)| *c == canvas).map(|(_, r)| r.clone())?;
        frame.draws.get(range)?.iter().find(|d| matches!(d, ScreenDraw::Image { .. })).cloned()
    })?;
    Some(sources(images).into_iter().flatten().filter_map(|source| {
        let Source::File { retained, path, rect, .. } = source else { return None };
        let mut draw = template.clone();
        if let ScreenDraw::Image { retained: r, path: p, rect: re, .. } = &mut draw {
            (*r, *p, *re) = (retained, path, rect);
        }
        Some(draw)
    }).collect())
}

/// `frame` with each of `lit` drawn from its sprite to show: of its draws
/// (those of its canvas), each from another of its sprites is drawn from
/// that one instead. `sources` resolves sprites to the image regions the
/// mirror knows them as. The buttons whose drawing changed.
pub fn apply(frame: &mut Frame, lit: &[Lit], sources: impl Fn(&[u32]) -> Vec<Option<Source>>) -> Vec<u32> {
    let mut changed = Vec::new();
    for l in lit {
        let canvas = l.button + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4;
        let Some(range) = frame.form_draws.iter().find(|(c, _)| *c == canvas).map(|(_, r)| r.clone()) else { continue };
        let region = |s: Option<Source>| match s {
            Some(Source::File { path, rect, .. }) => Some((path, rect)),
            _ => None,
        };
        let Some((to_path, to_rect)) = sources(&[l.show]).into_iter().next().and_then(region) else { continue };
        let from: Vec<(String, [i32; 4])> = sources(&l.sprites).into_iter().filter_map(region).filter(|(p, r)| (p, r) != (&to_path, &to_rect)).collect();
        let mut any = false;
        for d in frame.draws.get_mut(range).into_iter().flatten() {
            if let ScreenDraw::Image { path, rect, .. } | ScreenDraw::ResizedImage { path, rect, .. } = d {
                if rect[2] == to_rect[2] && rect[3] == to_rect[3] && from.iter().any(|(p, r)| p == path && r == rect) {
                    (*path, *rect) = (to_path.clone(), to_rect);
                    any = true;
                }
            }
        }
        if any {
            changed.push(l.button);
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUTTON: u32 = 0xA0_0000;
    const CANVAS: u32 = BUTTON + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4;

    fn image(y: i32) -> ScreenDraw {
        ScreenDraw::Image { retained:None,path: "Art/interface.pcx".into(), rect: [0, y, 32, 32], x: 500, y: 900, keyed: true, clip: [0, 0, 1024, 768], mask: None, palette: None, opacity: 255 }
    }

    /// Sprites 1, 2, 3: rows 0, 32, 64 of the file; 9 is unknown.
    fn sources(sprites: &[u32]) -> Vec<Option<Source>> {
        sprites.iter().map(|&s| (1..=3).contains(&s).then(|| Source::File { retained:None,path: "Art/interface.pcx".into(), rect: [0, 32 * (s as i32 - 1), 32, 32], mask: None, palette: None, opacity: 255 })).collect()
    }

    fn frame(draws: Vec<ScreenDraw>) -> Frame {
        let n = draws.len();
        // (Another canvas' draw first: not the button's.)
        let mut all = vec![image(0)];
        all.extend(draws);
        Frame { draws: all, form_draws: vec![(7, 0..1), (CANVAS, 1..n + 1)], ..Default::default() }
    }

    #[test]
    fn every_commands_art_in_every_state_is_drawn_as_a_shown_order_button() {
        // No order button drawn yet: nothing to go by.
        assert_eq!(all_states(&frame(vec![]), &[0xB0_0000], &[1, 2, 3, 9], sources), None);
        // One drawn: each known image as that button draws, in its region
        // (9, unknown, left out).
        let draws = all_states(&frame(vec![image(0)]), &[0xB0_0000, BUTTON], &[1, 2, 3, 9], sources).unwrap();
        assert_eq!(draws, [image(0), image(32), image(64)]);
    }

    #[test]
    fn the_button_lit_by_the_mode_is_drawn_from_its_highlighted_art() {
        let lit = [Lit { button: BUTTON, sprites: vec![1, 2, 3], show: 3 }];
        // Drawn normal (the command came from a key) or hovered.
        for y in [0, 32] {
            let mut f = frame(vec![image(y), ScreenDraw::Fill { rect: [500, 900, 2, 2], rgb: [1, 2, 3], alpha: 255, clip: [0, 0, 1024, 768] }]);
            assert_eq!(apply(&mut f, &lit, sources), [BUTTON]);
            assert_eq!(f.draws[0], image(0), "another canvas' draw stays");
            assert_eq!(f.draws[1], image(64));
        }
        // Lit already (the game's own click): unchanged, its looks stay.
        let mut f = frame(vec![image(64)]);
        assert!(apply(&mut f, &lit, sources).is_empty());
        assert_eq!(f.draws[1], image(64));
    }

    #[test]
    fn a_button_not_composed_or_with_unknown_art_stays_the_games() {
        let mut f = Frame { draws: vec![image(0)], form_draws: vec![(7, 0..1)], ..Default::default() };
        assert!(apply(&mut f, &[Lit { button: BUTTON, sprites: vec![1, 3], show: 3 }], sources).is_empty());
        let mut f = frame(vec![image(0)]);
        assert!(apply(&mut f, &[Lit { button: BUTTON, sprites: vec![1, 3], show: 9 }], sources).is_empty());
        assert_eq!(f.draws[1], image(0));
    }

    #[test]
    fn a_stack_button_left_lit_is_put_back() {
        let mut f = frame(vec![image(64)]);
        assert_eq!(apply(&mut f, &[Lit { button: BUTTON, sprites: vec![1, 2, 3], show: 1 }], sources), [BUTTON]);
        assert_eq!(f.draws[1], image(0));
    }
}
