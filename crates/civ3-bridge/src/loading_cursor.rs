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

    #[test]
    fn the_loading_bar_splits_off_with_its_hourglass_and_leaves_the_rest() {
        let fill = |x: i32| crate::ui::ScreenDraw::Fill { rect: [x, 80, 20, 24], rgb: [255; 3], alpha: 255, clip: [0, 0, 1024, 768] };
        let frame = Frame {
            forms: vec![(0x66b3d8, 7, (0, 0), (1024, 768)), (FORM, 40, (100, 80), (200, 24))],
            form_draws: vec![(7, 0..2), (40, 2..4)],
            draws: vec![fill(0), fill(500), fill(100), fill(150)],
            ..Default::default()
        };
        assert!(split_bar(&Frame { forms: frame.forms[..1].to_vec(), ..frame.clone() }).is_none(), "no bar, nothing split");
        let (rest, bar, rect) = split_bar(&frame).unwrap();
        assert_eq!(rect, [64, 80, 236, 24], "the bar and its hourglass's slot");
        let shown = |f: &Frame| (0..f.draws.len()).filter(|&i| crate::ui_screen::items(f, &[], &[], 0).len() > 0 && match &f.draws[i] {
            crate::ui::ScreenDraw::Fill { clip, .. } => clip[2] > clip[0],
            _ => false,
        }).collect::<Vec<_>>();
        assert_eq!(shown(&rest), [0, 1]);
        assert_eq!(shown(&bar), [2, 3]);
        assert!((0..4).all(|i| at_draw(&rest, i).is_none()), "no hourglass left behind");
        assert!(at_draw(&bar, 2).is_some(), "the hourglass goes with the bar");
    }

    #[test]
    fn hourglass_frames_turn_in_twelve_steps_and_sit_left_of_the_loading_bar() {
        // The system's wait cursor from the Wine prefix if there is one
        // (clear frames if not).
        let prefix = std::env::var("WINEPREFIX").unwrap_or_else(|_| "/mnt/datasets/winvm/wine-civ3".into());
        let files = civ3_assets::GameFiles::with_roots(vec![std::path::Path::new(&prefix).join("drive_c/Games")]);
        let desc = render_desc::Description::parse(&format!("grid = {{ layout = \"orthogonal\", step = [1, 1] }}\n{}", description())).unwrap();
        let loader = civ3_assets::loader::Civ3Loader::new(files);
        let compiled = desc.compile(&loader, 256).unwrap();
        let layout = desc.compile_layout(&loader, 256).unwrap();
        assert_eq!(compiled.sprite_names, layout.sprite_names, "sizes without pixels match the pixels");
        let set = compiled.set(scene::loading_cursor::SET).unwrap().0;
        for frame in 0..scene::loading_cursor::FRAME_COUNT {
            let id = compiled.sets.frame(set, frame).unwrap();
            assert_eq!(compiled.sprite_names[id.0 as usize], format!("Viewer@loading_cursor_frame/{frame}"));
            let art = &compiled.atlas.sprites[id.0 as usize];
            assert_eq!((art.width, art.height), (scene::loading_cursor::FRAME_SIDE, scene::loading_cursor::FRAME_SIDE));
            assert_eq!((art.anchor_x, art.anchor_y), (-23, -23), "hot spot in the middle");
        }
        assert!(compiled.sets.frame(set, scene::loading_cursor::FRAME_COUNT).is_none(), "twelve steps, no more");

        let frame = Frame {
            forms: vec![(FORM, 40, (100, 80), (200, 24))],
            form_draws: vec![(40, 0..1)],
            draws: vec![crate::ui::ScreenDraw::Fill { rect: [100, 80, 200, 24], rgb: [255; 3], alpha: 255, clip: [100, 80, 300, 104] }],
            ..Default::default()
        };
        let items = crate::ui_screen::items(&frame, &[], &[], 0);
        let fonts = crate::text::Fonts::default();
        let sprites = crate::ui_screen::sprites(&items, &fonts, &compiled, u32::MAX, false);
        assert_eq!(sprites[0].ticks_per_frame, scene::loading_cursor::TICKS_PER_FRAME, "turned by the viewer's clock");
        let id = compiled.sets.frame(tile_render::SpriteSetId::from_raw(sprites[0].set), 0).unwrap();
        let art = &compiled.atlas.sprites[id.0 as usize];
        // The 32x32 slot: left of the form with a gap, centred on its middle.
        assert_eq!((sprites[0].x + art.anchor_x, sprites[0].y + art.anchor_y), (57, 69));
        assert_eq!((sprites[0].x, sprites[0].y), (80, 92), "turning about the form's middle row");
        assert_eq!(sprites[0].clip, Some([64, 76, 96, 108]), "hourglass on the left, not over the progress text");
        let absent = crate::text::readiness_tests::fixture_art(&[], "");
        assert_eq!(crate::ui_screen::readiness(items.iter(), &fonts, &absent, false), crate::ui_screen::Readiness::Ready);
        assert_eq!(crate::ui_screen::sprites(&items, &fonts, &absent, u32::MAX, false).len(), 1, "missing optional hourglass leaves the progress background visible");

        // With Wine's hourglass at hand: frame 0 is it, upright and whole.
        let dll = std::path::Path::new(&prefix).join("drive_c/windows/syswow64/user32.dll");
        if !dll.is_file() { return eprintln!("no Wine prefix: hourglass pixels not checked"); }
        let cursor = civ3_assets::syscursor::find(&[dll], 32514, 32).unwrap();
        let first = &compiled.atlas.sprites[compiled.sets.frame(set, 0).unwrap().0 as usize];
        let page = &compiled.atlas.pages[first.page as usize];
        let opaque = (0..46 * 46).filter(|i| page.get(first.atlas_x + i % 46, first.atlas_y + i / 46)[3] != 0).count();
        let source = cursor.rgba.chunks(4).filter(|p| p[3] != 0).count();
        assert_eq!(opaque, source, "upright frame: the hourglass's own pixels");
    }
}

#[cfg(test)]
mod gpu_tests {
    use crate::text::{Fonts, TextRun};
    use crate::ui::{Event, UiModel};
    use std::sync::Arc;

    /// Stitch independent unmasked text/background captures. Pixel membership
    /// comes from inverse-nearest integer arithmetic, never the shader helper.
    #[test]
    fn gpu_loading_label_crossing_glyphs_keeps_native_inverse_clips_at_each_scale() {
        struct NoFiles;
        impl render_desc::ImageLoader for NoFiles {
            fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> { Err("no asset needed".into()) }
        }
        let mut compiled = render_desc::Description::parse("grid = { layout = \"orthogonal\", step = [1, 1] }").unwrap().compile(&NoFiles, 64).unwrap();
        let line = |a: (f64, f64), b: (f64, f64)| [a, ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), b];
        let points = [(0.0, 0.0), (0.0, 700.0), (600.0, 700.0), (600.0, 0.0)];
        let curves = (0..4).map(|i| line(points[i], points[(i + 1) % 4])).collect();
        compiled.fonts.push(("Progress fixture".into(), Arc::new(tile_render::gpu_text::Face::from_curves(1000, &[('A', curves, 700)]))));
        let fonts = Fonts::default();
        let run = |color| TextRun { x: 20, y: 25, color, face: "Progress fixture".into(), height: -20, weight: 400, italic: false, underline: false, text: "AAAAA".into(), backdrop: None };
        fonts.set_game_layout(&run([0, 0, 255]), gdi_text::TextLayout { advances: vec![14; 5], ascent: 20, descent: 4, underline: (1, 1) });
        let gpu = tile_render::Headless::new().expect("GPU clipping correctness lease");
        eprintln!("inverse loading text adapter: {:?}", gpu.adapter.get_info());
        let mut renderer = tile_render::TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, tile_viewer::SHADERS);
        renderer.set_atlas(&gpu.device, &gpu.queue, &compiled.atlas);
        let clear = [13, 29, 47, 255];
        let mut compared_pixels = 0;
        for zoom in [(1, 1), (3, 2), (2, 1), (2, 3)] {
            let viewport = (134 * zoom.0 / zoom.1, 50 * zoom.0 / zoom.1);
            let size = (viewport.0 + 24, viewport.1 + 16);
            for (origin, screen_shift) in [((0, 0), (0, 0)), ((7, 11), (3, 5)), ((17, 5), (-11, 9))] {
                let camera = tile_render::Camera { cells: false, zoom, hairline: true, origin, screen_shift, ..tile_render::Camera::new(viewport) };
                let mut render = |boundary: i32, clipped: bool, white: bool, text: bool, label_clip: [i32; 4]| {
                    let mut model = UiModel::new(vec![], vec![]);
                    model.apply(Event::Size { image: 40, size: (128, 40) });
                    model.apply(Event::Fill { canvas: 40, rect: [0, 0, 128, 40], rgb: if !clipped && white { [0, 0, 255] } else { [255; 3] }, alpha: 255, clip: None });
                    if clipped {
                        model.apply(Event::Fill { canvas: 40, rect: [0, 0, boundary, 40], rgb: [0, 0, 255], alpha: 255, clip: None });
                        if text {
                            model.apply(Event::TextClipped { canvas: 40, run: run([255; 3]), clip: [label_clip[0], label_clip[1], boundary.min(label_clip[2]), label_clip[3]] });
                            model.apply(Event::TextClipped { canvas: 40, run: run([0, 0, 255]), clip: [boundary.max(label_clip[0]), label_clip[1], label_clip[2], label_clip[3]] });
                        }
                    } else if text {
                        model.apply(Event::Text { canvas: 40, run: run(if white { [255; 3] } else { [0, 0, 255] }) });
                    }
                    model.apply(Event::PaintBegin);
                    model.apply(Event::Composite { canvas: 40, vtable: 0, origin: (3, 5), size: (128, 40), clip: [3, 5, 131, 45], keyed: false });
                    model.apply(Event::PaintEnd);
                    let items = crate::ui_screen::items(&model.snapshot().frame(), &[], &[], 0);
                    let mut world = compiled.world(1, 1, false, false);
                    world.free = crate::ui_screen::sprites(&items, &fonts, &compiled, u32::MAX, false).iter()
                        .map(|sprite| tile_viewer::placed_sprite(&compiled, sprite, Some(0), None)).collect();
                    renderer.set_world(&gpu.device, &world);
                    gpu.render_views(&mut renderer, &[camera], size, 0, clear)
                };
                let full = [0, 0, 128, 40];
                let empty = render(0, false, false, true, full);
                let filled = render(128, false, true, true, full);
                let empty_bg = render(0, false, false, false, full);
                let filled_bg = render(128, false, true, false, full);
                for label_clip in [full, [17, 14, 93, 23]] {
                    for boundary in [0, 24, 35, 48, 61, 74, 87, 128] {
                        let actual = render(boundary, true, false, true, label_clip);
                        let mut text_pixels = 0;
                        for y in 0..size.1 {
                            for x in 0..size.0 {
                                // Independent native sample under this target pixel.
                                let native_x = ((i64::from(x) - i64::from(origin.0)) * i64::from(zoom.1)).div_euclid(i64::from(zoom.0)) + i64::from(origin.0 - screen_shift.0) - 3;
                                let native_y = ((i64::from(y) - i64::from(origin.1)) * i64::from(zoom.1)).div_euclid(i64::from(zoom.0)) + i64::from(origin.1 - screen_shift.1) - 5;
                                let in_text_clip = native_x >= i64::from(label_clip[0]) && native_x < i64::from(label_clip[2]) && native_y >= i64::from(label_clip[1]) && native_y < i64::from(label_clip[3]);
                                let expected = match (native_x < i64::from(boundary), in_text_clip) {
                                    (true, true) => filled.get(x, y), (false, true) => empty.get(x, y),
                                    (true, false) => filled_bg.get(x, y), (false, false) => empty_bg.get(x, y),
                                };
                                assert_eq!(actual.get(x, y), expected, "inverse native text mismatch, boundary {boundary}, zoom{zoom:?}, origin{origin:?}, screen_shift{screen_shift:?}, clip{label_clip:?}, pixel({x},{y}), native({native_x},{native_y})");
                                if empty.get(x, y) != empty_bg.get(x, y) { text_pixels += 1; }
                            }
                        }
                        assert!(text_pixels > 10, "the fixture must draw visible glyph coverage");
                        compared_pixels += actual.pixels.len();
                    }
                }
            }
        }
        assert_eq!(compared_pixels, 3374976);
        eprintln!("inverse loading text exact comparisons: {compared_pixels} pixels, four scales, signed origin/shift and both clip ends");
    }
}
