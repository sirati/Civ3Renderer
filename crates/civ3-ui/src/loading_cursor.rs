//! The loading cursor beside the native loading bar: the wait cursor's
//! hourglass, turning (`scene::loading_cursor`, `assets/viewer/
//! loading-cursor.toml`). Optional art: a missing system cursor leaves
//! clear frames, never a pending or failed UI group.
use crate::ui::Frame;

// PcGames constructor0x628c90 stores this class; draw0x628eb0 paints the
// two complementary native text clips. It is not inferred from colours.
pub const FORM: u32 = 0x67_24F8;
const GAP: i32 = 4;
/// The square the turning hourglass stays in.
const ENVELOPE: i32 = scene::loading_cursor::ENVELOPE as i32;

/// The loading cursor's description, appended to the game's. It loads
/// the system's wait cursor, which is clear where there is none, so it
/// cannot break the first compile or hold a mandatory UI group pending.
pub fn description() -> &'static str {
    include_str!("../../../assets/viewer/loading-cursor.toml")
}

/// Place the hourglass to the left of the bar, centred on its native form.
/// Keeping this beside its form's first draw preserves popup painter order.
pub fn at_draw(frame: &Frame, draw: usize) -> Option<menu::screen::Element> {
    frame.forms.iter().find_map(|&(class, canvas, (x, y), (w, h))| {
        (class == FORM && w > 0 && h > 0 && frame.form_draws.iter().any(|(c, r)| *c == canvas && r.start == draw && !r.is_empty()))
            .then(|| {
                let slot = (x - GAP - ENVELOPE, y + h / 2 - ENVELOPE / 2);
                // (Its frames' anchor, where the hourglass turns: the middle.)
                let at = (slot.0 + ENVELOPE / 2, slot.1 + ENVELOPE / 2);
                menu::screen::Element::Sprite { name: scene::loading_cursor::SET.into(), at,
                    clip: [slot.0, slot.1, slot.0 + ENVELOPE, slot.1 + ENVELOPE], scale_den: 0, palette: String::new() }
            })
    })
}

/// The interface-only view of the loading bar split off the rest
/// (`split_bar`, `scene::Interface::loading_bar`): above every popup's
/// (their canvases' addresses), so that where it cannot be kept clear of
/// them it shows in front.
pub const VIEW: u32 = 0xFFFF_0100;

/// The loading bar (the form of class [`FORM`], Main_GUI+0x138F0) on its
/// own: `frame` without it, its draws alone (all others clipped to nothing,
/// the indices kept, so that its hourglass goes with it, `at_draw`), and
/// its rectangle (x, y, w, h) with the hourglass's slot. None while no bar
/// is drawn.
pub fn split_bar(frame: &Frame) -> Option<(Frame, Frame, [i32; 4])> {
    let &(class, canvas, (x, y), (w, h)) = frame.forms.iter().rev().find(|f| f.0 == FORM && f.3 .0 > 0 && f.3 .1 > 0)?;
    let mine: Vec<usize> = frame.form_draws.iter().filter(|(c, r)| *c == canvas && !r.is_empty()).flat_map(|(_, r)| r.clone()).collect();
    if mine.is_empty() {
        return None;
    }
    let others: Vec<usize> = (0..frame.draws.len()).filter(|i| !mine.contains(i)).collect();
    let mut rest = crate::interface_parts::without(frame, &mine);
    rest.form_draws.retain(|(c, _)| *c != canvas);
    let mut bar = crate::interface_parts::without(frame, &others);
    bar.form_draws.retain(|(c, _)| *c == canvas);
    bar.underlay_draws.clear();
    bar.overlay_draws = 0..0;
    let [l, t, r, b] = extend_bounds(class, [x, y, x + w, y + h]);
    Some((rest, bar, [l, t, r - l, b - t]))
}

pub fn is_optional(element: &menu::screen::Element) -> bool {
    matches!(element, menu::screen::Element::Sprite { name, .. } if name == scene::loading_cursor::SET)
}

pub fn extend_bounds(class: u32, bounds: [i32; 4]) -> [i32; 4] {
    if class == FORM { [bounds[0] - ENVELOPE - GAP, bounds[1], bounds[2], bounds[3]] } else { bounds }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loading_cursor_is_optional_and_only_widens_the_loading_form() {
        assert!(description().contains("system-cursor:32514/32"), "the wait cursor's hourglass");
        assert!(!description().contains("Cursor.flc"), "not the unit-selection ring");
        assert!(is_optional(&menu::screen::Element::Sprite { name: scene::loading_cursor::SET.into(), at: (0, 0), clip: [0; 4], scale_den: 0, palette: String::new() }));
        assert_eq!(extend_bounds(0x66b3d8, [0, 0, 1024, 768]), [0, 0, 1024, 768], "parent/advisor bounds never grow");
        assert_eq!(extend_bounds(FORM, [100, 0, 1024, 768]), [64, 0, 1024, 768]);
    }
}
