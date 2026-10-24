//! The parts of the game's interface the viewer arranges itself
//! (`scene::Interface`, docs §16.12), as far as they are the frame's
//! geometry: what of it is the tooltip, a popup, a screen. The tables of
//! the game's forms (which screen is which, by build) stay with the bridge
//! (`civ3-bridge`'s `interface_parts`).

use crate::ui::{Frame, ScreenDraw};

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

pub fn intersect(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]
}

pub fn clip_mut(d: &mut ScreenDraw) -> &mut [i32; 4] {
    match d {
        ScreenDraw::Image { clip, .. } | ScreenDraw::ResizedImage { clip, .. } | ScreenDraw::Fill { clip, .. } | ScreenDraw::Text { clip, .. } | ScreenDraw::Flc { clip, .. } | ScreenDraw::Surface { clip, .. }
        | ScreenDraw::Indexed { clip, .. } => clip,
    }
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
