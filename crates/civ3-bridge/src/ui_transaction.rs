//! UI transactions commit rendering commands and final layout, independently
//! of sprite residency. The capture model stages forms and overlays until
//! Present, then exposes that complete group through `committed_frame`.
//! The renderer keeps its previous committed group while capture is in progress.
//! Missing art omits only the affected sprite. It never blocks other UI commands.
//! Glyph pens come from measured text metadata before glyph sheets arrive.
//! GPU submission and compatible descriptor generations belong to the viewer.

#[cfg(test)]
mod tests {
    use crate::text::{Fonts, TextRun};
    use crate::ui::{Event, ScreenDraw, UiModel};
    use crate::ui_screen::Item;
    use menu::screen::{Element, ScreenArt};

    const ALL: [i32; 4] = [-(1 << 24), -(1 << 24), 1 << 24, 1 << 24];

    fn background(model: &mut UiModel, x: i32) {
        model.apply(Event::Fill { canvas: 999, rect: [x, 100, 120, 20], rgb: [255, 255, 225], alpha: 255, clip: None });
    }

    fn text(model: &mut UiModel, x: i32, value: &str) {
        model.apply(Event::Text { canvas: 999, run: TextRun { x: x + 4, y: 114, color: [0; 3], face: "No outlines".into(), height: -12, weight: 400, italic: false, underline: false, text: value.into(), backdrop: None } });
    }

    #[test]
    fn first_present_commits_background_and_text_together() {
        let mut model = UiModel::default().with_screen(999);
        model.apply(Event::PresentBegin);
        background(&mut model, 100);
        assert!(model.snapshot().committed_frame().draws.is_empty());
        text(&mut model, 100, "Fortify");
        assert!(model.snapshot().committed_frame().draws.is_empty());
        model.apply(Event::Present { rect: None });
        let frame = model.snapshot().committed_frame();
        assert!(matches!(&frame.draws[..], [ScreenDraw::Fill { .. }, ScreenDraw::Text { run, .. }] if run.text == "Fortify" && run.x == 104));
    }

    #[test]
    fn an_initial_painted_form_waits_for_its_present() {
        let mut model = UiModel::default().with_screen(999);
        model.apply(Event::Fill { canvas: 100, rect: [0, 0, 120, 20], rgb: [255, 255, 225], alpha: 255, clip: None });
        model.apply(Event::Text { canvas: 100, run: TextRun { x: 4, y: 14, color: [0; 3], face: "No outlines".into(), height: -12, weight: 400, italic: false, underline: false, text: "Fortify".into(), backdrop: None } });
        model.apply(Event::PaintBegin);
        model.apply(Event::Composite { canvas: 100, vtable: 1, origin: (100, 100), size: (120, 20), clip: ALL, keyed: false });
        model.apply(Event::PaintEnd);
        assert!(!model.frame().draws.is_empty(), "explicit backbuffer inspection remains available");
        assert!(model.snapshot().committed_frame().draws.is_empty());
        model.apply(Event::Present { rect: None });
        assert_eq!(model.committed_frame(), model.frame());
    }

    #[test]
    fn a_new_group_never_replaces_the_old_group_before_commit() {
        let mut model = UiModel::default().with_screen(999);
        model.apply(Event::PresentBegin);
        background(&mut model, 100);
        text(&mut model, 100, "Old");
        model.apply(Event::Present { rect: None });
        let old = model.committed_frame();
        model.apply(Event::PresentBegin);
        background(&mut model, 200);
        assert_eq!(model.snapshot().committed_frame(), old);
        text(&mut model, 200, "New");
        assert_eq!(model.snapshot().committed_frame(), old);
        model.apply(Event::Present { rect: None });
        let new = model.committed_frame();
        assert!(matches!(&new.draws[..], [ScreenDraw::Fill { rect: [200, 100, 120, 20], .. }, ScreenDraw::Text { run, .. }] if run.text == "New" && run.x == 204));
    }

    #[test]
    fn updating_a_form_keeps_its_previous_committed_commands() {
        let mut model = UiModel::default().with_screen(999);
        let paint = |model: &mut UiModel, label: &str| {
            model.apply(Event::Fill { canvas: 100, rect: [0, 0, 120, 20], rgb: [255, 255, 225], alpha: 255, clip: None });
            model.apply(Event::Text { canvas: 100, run: TextRun { x: 4, y: 14, color: [0; 3], face: "No outlines".into(), height: -12, weight: 400, italic: false, underline: false, text: label.into(), backdrop: None } });
            model.apply(Event::PaintBegin);
            model.apply(Event::Composite { canvas: 100, vtable: 1, origin: (100, 100), size: (120, 20), clip: ALL, keyed: false });
        };
        paint(&mut model, "Old");
        model.apply(Event::PaintEnd);
        model.apply(Event::Present { rect: None });
        let old = model.committed_frame();
        paint(&mut model, "New");
        assert_eq!(model.snapshot().committed_frame(), old);
        model.apply(Event::PaintEnd);
        assert_eq!(model.snapshot().committed_frame(), old);
        model.apply(Event::Present { rect: None });
        assert!(model.committed_frame().draws.iter().any(|draw| matches!(draw, ScreenDraw::Text { run, .. } if run.text == "New")));
        assert!(!model.committed_frame().draws.iter().any(|draw| matches!(draw, ScreenDraw::Text { run, .. } if run.text == "Old")));
    }

    #[test]
    fn first_committed_commands_include_background_and_final_text_with_an_icon_missing() {
        let mut model = UiModel::default().with_screen(999);
        model.apply(Event::Loaded { image: 20, path: "Missing optional icon.pcx".into(), size: (8, 8) });
        model.apply(Event::Sliced { sprite: 21, image: 20, rect: [0, 0, 8, 8] });
        model.apply(Event::PresentBegin);
        background(&mut model, 100);
        let run = TextRun { x: 104, y: 114, color: [0; 3], face: "Readiness fixture".into(), height: -12, weight: 400, italic: false, underline: false, text: "AB".into(), backdrop: None };
        model.apply(Event::Text { canvas: 999, run: run.clone() });
        model.apply(Event::Blit { sprite: 21, canvas: 999, x: 200, y: 100, clip: None, palette: None });
        assert!(model.snapshot().committed_frame().draws.is_empty());
        model.apply(Event::Present { rect: None });
        let frame = model.committed_frame();
        assert_eq!(frame.draws.len(), 3, "the complete command group includes the unresolved icon");
        let items = crate::ui_screen::items(&frame, &[], &[], 0);
        let compiled = crate::text::readiness_tests::fixture_art(&[], "AB");
        let fonts = Fonts::default();
        fonts.set_game_layout(&run, gdi_text::TextLayout { advances: vec![6, 7], ascent: 9, descent: 3, underline: (1, 1) });
        let sprites = crate::ui_screen::sprites(&items, &fonts, &compiled, u32::MAX, false);
        assert_eq!(sprites.len(), 3, "background and two glyph commands need no glyph sheets or icon pixels");
        assert!(sprites[0].glyph.is_none());
        assert_eq!((sprites[0].x, sprites[0].y), (100, 100));
        assert_eq!(sprites[1..].iter().map(|s| (s.x, s.y, s.glyph.unwrap().advance)).collect::<Vec<_>>(), [(104, 114, 6), (110, 114, 7)]);
    }

    struct Images;
    impl render_desc::ImageLoader for Images {
        fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
            Ok(vec![tile_render::RgbaImage::from_fn(128, 128, |_, _| [255, 255, 225, 255])])
        }
    }

    #[test]
    fn a_missing_sprite_omits_only_itself_from_the_committed_group() {
        let background = Item::Element(Element::Fill { rect: [100, 100, 120, 20], rgb: [255, 255, 225], alpha: 255, clip: ALL }, false);
        let missing = Item::Element(Element::Sprite { name: "Slow or failed sprite".into(), at: (200, 100), clip: ALL, scale_den: 0, palette: String::new() }, false);
        let mut art = ScreenArt::default();
        let mut fonts = Fonts::default();
        crate::ui_screen::add_art([&background], &mut art, &mut fonts, &civ3_assets::GameFiles::with_roots(Vec::new()));
        let description = format!("grid = {{ layout = \"orthogonal\", step = [1, 1] }}\n{}", crate::ui_screen::description(&art));
        let compiled = render_desc::Description::parse(&description).unwrap().compile(&Images, 256).unwrap();
        let expected = crate::ui_screen::sprites(std::slice::from_ref(&background), &fonts, &compiled, u32::MAX, false);
        assert!(!expected.is_empty());
        let items = [background, missing];
        assert_eq!(crate::ui_screen::sprites(&items, &fonts, &compiled, u32::MAX, false), expected);
        for building in [false, true] {
            assert!(matches!(crate::ui_screen::publication(items.iter(), &fonts, &compiled, building, (1024, 768)), crate::ui_screen::Publication::Partial { .. }));
            assert_eq!(crate::ui_screen::sprites(&items, &fonts, &compiled, u32::MAX, false), expected, "neither slow nor failed art hides the rest");
        }
    }
}
