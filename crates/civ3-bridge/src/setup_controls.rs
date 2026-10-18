//! The native title menu tests text rectangles directly, without creating
//! button forms. Match its eleven rows to the captured, translated labels.

use crate::{text::TextRun, ui::{Frame, ScreenDraw}};
use civ3_sys::controls::{Control, ControlKind, TitleLayout};

pub fn title(frame: &Frame, layout: TitleLayout, mut text_width: impl FnMut(&TextRun) -> i32) -> Vec<Control> {
    let mut controls = Vec::new();
    for row in layout.first..11 {
        let (x, y) = (layout.origin.0, layout.origin.1 + row as i32 * layout.row_height);
        let Some(run) = frame.draws.iter().find_map(|draw| match draw {
            ScreenDraw::Text { run, clip } if run.x == x + 13 && run.y >= y && run.y < y + layout.row_height
                && !run.text.trim().is_empty() && run.x >= clip[0] && run.x < clip[2] && run.y >= clip[1] && run.y < clip[3] => Some(run),
            _ => None,
        }) else { continue };
        let width = text_width(run);
        if width <= 0 { continue; }
        controls.push(Control {
            rect: [x, y, width.saturating_add(20), layout.row_height],
            kind: ControlKind::Button,
            id: (0x9F_86C0u64 << 32) | 0x6000_0000 | row as u64,
        });
    }
    controls
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{Event, UiModel};

    fn run(x: i32, y: i32, text: &str) -> TextRun {
        TextRun { x, y, text: text.into(), face: "Lucida Sans".into(), height: -18,
            weight: 0, italic: false, underline: false, color: [248; 3], backdrop: None }
    }

    #[test]
    fn passive_startup_title_rows_use_the_native_positions_and_translated_extents() {
        // The title is first painted before the publisher exists, then
        // remains unchanged. No button children or hover events are needed.
        let mut model = UiModel::new(vec![], vec![]);
        let layout = TitleLayout { origin: (648, 442), row_height: 38, first: 1 };
        let labels = ["Play Last World", "Neues Spiel", "Quick Start", "Load Game", "Conquests!", "Civ-Content", "Hall of Fame", "Preferences", "Multiplayer", "Credits", "Exit"];
        model.apply(Event::Size { image: 300, size: (1920, 1080) });
        for (row, label) in labels.iter().enumerate() {
            model.apply(Event::Text { canvas: 300, run: run(661, 467 + row as i32 * 38, label) });
        }
        // A copyright line must not become an Alt target.
        model.apply(Event::Text { canvas: 300, run: run(764, 912, "All rights reserved.") });
        model.apply(Event::PaintBegin);
        model.apply(Event::Composite { canvas: 300, vtable: 0x66B3D8, origin: (0, 0), size: (1920, 1080), clip: [0, 0, 1920, 1080], keyed: false });
        model.apply(Event::CompositeEnd);
        model.apply(Event::PaintEnd);
        model.apply(Event::PresentBegin);
        model.apply(Event::Present { rect: None });
        let frame = model.shown_frame();
        let controls = title(&frame, layout, |run| if run.text == "Neues Spiel" { 113 } else { 87 });
        assert_eq!(controls.len(), 10);
        assert_eq!(controls[0].rect, [648, 480, 133, 38]);
        assert_eq!(controls[9].rect, [648, 822, 107, 38]);
        assert_eq!(title(&model.shown_frame(), layout, |_| 87).len(), 10);
        assert_eq!(title(&frame, TitleLayout { first: 0, ..layout }, |_| 87).len(), 11);
        assert!(title(&Frame::default(), layout, |_| 87).is_empty());
    }

    #[test]
    fn clipped_or_unmeasurable_title_labels_are_not_targets() {
        let layout = TitleLayout { origin: (0, 0), row_height: 38, first: 1 };
        let frame = Frame { draws: vec![ScreenDraw::Text { run: run(13, 49, "New Game"), clip: [100, 0, 300, 768] }], ..Default::default() };
        assert!(title(&frame, layout, |_| 50).is_empty());
        let frame = Frame { draws: vec![ScreenDraw::Text { run: run(13, 49, "New Game"), clip: [0, 0, 300, 768] }], ..Default::default() };
        assert!(title(&frame, layout, |_| 0).is_empty());
    }

    #[test]
    fn native_title_and_setup_targets_click_the_same_pixels_at_ui_scales_one_through_six() {
        use scene::{Button, FreeSprite, Input, Scene, View, ViewState};
        use tile_viewer::{layout::Layout, scaling, snap};
        let layout = TitleLayout { origin: (648, 442), row_height: 38, first: 1 };
        let frame = Frame { draws: vec![ScreenDraw::Text { run: run(661, 505, "New Game"), clip: [0, 0, 1920, 1080] }], ..Default::default() };
        let mut controls = title(&frame, layout, |_| 113);
        // A native civilization/difficulty/aggression hit region near the
        // middle of its actual screen, as read from the indexed hit mask.
        controls.push(Control { rect: [940, 520, 40, 40], kind: ControlKind::Button, id: 0x1050000042 });
        let items = crate::ui_screen::interactables(&controls);
        let scene = Scene { screen_size: (1920, 1080), views: vec![ViewState {
            id: 255, view: View { interface_only: true, menu: true, ..Default::default() },
            overlay: vec![FreeSprite { screen: true, ..Default::default() }],
        }], ..Default::default() };
        for pixel_scale in 1..=6 {
            for menu_scale in [0.5, 1.0] {
                let plan = scaling::plan((3840, 2160), 2.0, scene.screen_size,
                    &viewer_settings::Resolution { pixel_scale, menu_scale, ..Default::default() });
                let mut l = Layout::default();
                l.driver_screen = Some(plan.screen);
                let mut f = l.frame(&scene, plan.canvas(), (0, 0, 0, 0), vec![], None);
                l.set_interface_shown(scaling::arrange(&plan, &scene, &mut f, |_| Some([0, 0, 1920, 1080])).shown);
                let targets = snap::targets_mapped(&items, |r| l.driver_rect_to_window(r), plan.canvas());
                assert_eq!(targets.len(), 2, "pixel scale {pixel_scale}, menu scale {menu_scale}");
                for target in targets {
                    let p = target.center();
                    let source = controls.iter().find(|c| c.id == target.id).unwrap().rect;
                    let input = Input::MouseButton { button: Button::Left, down: true, x: p.0 as i32, y: p.1 as i32 };
                    let result = l.route(input);
                    let Some(Input::MouseButton { x, y, .. }) = result.first() else { panic!("missing native input {result:?}") };
                    assert!(*x >= source[0] && *x < source[0] + source[2] && *y >= source[1] && *y < source[1] + source[3],
                        "{pixel_scale}, {menu_scale}: native hit {source:?}, actual {result:?}");
                    l.route(Input::MouseButton { button: Button::Left, down: false, x: p.0 as i32, y: p.1 as i32 });
                }
            }
        }
    }

}
