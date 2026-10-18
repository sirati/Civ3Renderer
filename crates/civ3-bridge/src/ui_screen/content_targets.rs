//! Pointer positions on semantic rows, derived from their drawn content.
use civ3_sys::controls::Control;
use scene::Interactable;

use crate::text::{Fonts, TextRun};
use crate::ui::{Frame, ScreenDraw};

/// Set a pointer tip at an icon's lower right corner or at the end of its
/// text. Only existing game controls qualify. Draws never create targets.
pub fn content_anchors(frame: &Frame, fonts: &Fonts, controls: &[Control], targets: &mut [Interactable]) {
    annotate(frame, controls, targets, |run| fonts.ink_bounds(run));
}

fn intersection([x, y, w, h]: [i32; 4], [l, t, r, b]: [i32; 4]) -> Option<[i32; 4]> {
    let (left, top) = (x.max(l), y.max(t));
    let (right, bottom) = (x.saturating_add(w).min(r), y.saturating_add(h).min(b));
    (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}

fn annotate(frame: &Frame, controls: &[Control], targets: &mut [Interactable], text_bounds: impl Fn(&TextRun) -> Option<[i32; 4]>) {
    for control in controls.iter().filter(|c| c.content_row()) {
        let Some(target) = targets.iter_mut().find(|t| t.id == control.id) else {
            continue;
        };
        let [x, y, w, h] = control.rect;
        if w <= 0 || h <= 0 {
            continue;
        }
        let clip = [x, y, x.saturating_add(w), y.saturating_add(h)];
        let owner = (control.id >> 32) as u32;
        let canvas = owner.checked_add(civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4);
        let draws = canvas
            .and_then(|canvas| frame.form_draws.iter().find(|(c, _)| *c == canvas))
            .and_then(|(_, range)| frame.draws.get(range.clone()))
            .unwrap_or(&[]);
        let mut icon: Option<[i32; 4]> = None;
        let mut text: Option<[i32; 4]> = None;
        for draw in draws {
            match draw {
                ScreenDraw::Image {
                    path,
                    rect,
                    x,
                    y,
                    clip: image_clip,
                    opacity,
                    ..
                } if *opacity != 0 && rect[2] <= h.saturating_mul(2) && rect[3] <= h.saturating_mul(2) && !path.to_ascii_lowercase().contains("background") => {
                    // A row's icon, never a large panel behind the row.
                    if let Some(r) = intersection([*x, *y, rect[2], rect[3]], *image_clip).and_then(|r| intersection(r, clip)) {
                        if icon.is_none_or(|old| (r[0], -(r[2] as i64 * r[3] as i64)) < (old[0], -(old[2] as i64 * old[3] as i64))) {
                            icon = Some(r);
                        }
                    }
                }
                ScreenDraw::Text { run, clip: text_clip } => {
                    if let Some(r) = text_bounds(run).and_then(|r| intersection(r, *text_clip)).and_then(|r| intersection(r, clip)) {
                        if text.is_none_or(|old| (r[0] + r[2], r[1]) > (old[0] + old[2], old[1])) {
                            text = Some(r);
                        }
                    }
                }
                _ => {}
            }
        }
        target.anchor = if let Some([x, y, w, h]) = icon {
            Some([x + w - 1, y + h - 1])
        } else if let Some([x, y, w, h]) = text {
            Some([x + w - 1, y + h / 2])
        } else if control.text_link() {
            // The link table already gives its exact rendered text region.
            Some([x + w - 1, y + h / 2])
        } else {
            None
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use civ3_sys::controls::ControlKind;
    const FORM: u32 = 0x10_0000;
    fn row(index: u32, y: i32, link: bool) -> Control {
        Control {
            rect: [20, y, 400, 70],
            kind: if link { ControlKind::Button } else { ControlKind::ListItem },
            id: (FORM as u64) << 32 | (if link { 0x1000_0000 } else { 0 }) | index as u64,
        }
    }
    fn label(text: &str, x: i32, baseline: i32) -> ScreenDraw {
        ScreenDraw::Text {
            run: TextRun {
                x,
                y: baseline,
                text: text.into(),
                face: "fixture".into(),
                height: 16,
                color: [255; 3],
                weight: 400,
                italic: false,
                underline: false,
                backdrop: None,
            },
            clip: [0, 0, 1024, 768],
        }
    }
    fn bounds(run: &TextRun) -> Option<[i32; 4]> {
        Some([run.x, run.y - 12, run.text.len() as i32 * 7, 16])
    }
    fn targets(controls: &[Control]) -> Vec<Interactable> {
        crate::ui_screen::interactables(controls)
    }
    #[test]
    fn civilopedia_icons_and_scenario_text_snap_to_their_content() {
        let controls = [row(0, 20, false), row(1, 100, false), row(2, 180, true)];
        let draws = vec![
            ScreenDraw::Image {
                path: "Art/Civilopedia/Terrain.pcx".into(),
                rect: [0, 0, 64, 64],
                x: 22,
                y: 22,
                keyed: true,
                clip: [0, 0, 1024, 768],
                mask: None,
                palette: None,
                opacity: 255,
            },
            label("Terrain", 100, 60),
            label("A scenario", 22, 130),
            label("Units", 22, 210),
        ];
        let frame = Frame {
            form_draws: vec![(FORM + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4, 0..draws.len())],
            draws,
            ..Default::default()
        };
        let mut got = targets(&controls);
        annotate(&frame, &controls, &mut got, bounds);
        assert_eq!(
            got.iter().map(|t| t.anchor).collect::<Vec<_>>(),
            [Some([85, 85]), Some([91, 126]), Some([56, 206])]
        );
        assert_eq!(
            got.iter().map(|t| t.rect).collect::<Vec<_>>(),
            controls.map(|c| c.rect),
            "anchors preserve real game hit boxes"
        );
    }
    #[test]
    fn clip_to_visible_content_and_ignore_other_forms_and_decorations() {
        let control = row(0, 20, false);
        let draws = vec![
            ScreenDraw::Image {
                path: "Art/Civilopedia/Terrain.pcx".into(),
                rect: [0, 0, 64, 64],
                x: 22,
                y: 22,
                keyed: true,
                clip: [0, 0, 50, 45],
                mask: None,
                palette: None,
                opacity: 255,
            },
            label("Unrelated", 200, 60),
        ];
        let frame = Frame {
            form_draws: vec![(FORM + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4, 0..1), (0x999, 1..2)],
            draws,
            ..Default::default()
        };
        let mut got = targets(&[control]);
        annotate(&frame, &[control], &mut got, bounds);
        assert_eq!(got[0].anchor, Some([49, 44]));
        assert_eq!(got.len(), 1);
        let mut only_decoration = Vec::new();
        annotate(&frame, &[], &mut only_decoration, bounds);
        assert!(only_decoration.is_empty());
    }
    #[test]
    fn exact_rich_text_hit_rect_is_a_valid_anchor_before_fonts_arrive() {
        let control = row(0, 180, true);
        let mut got = targets(&[control]);
        annotate(&Frame::default(), &[control], &mut got, |_| None);
        assert_eq!(got[0].anchor, Some([419, 215]));
    }
}
