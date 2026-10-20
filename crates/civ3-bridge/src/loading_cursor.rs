//! Optional game-art animation beside the native loading bar.
use crate::ui::Frame;

pub const SOURCE: &str = "Art/Animations/Cursor/Cursor.flc";
pub const BAR_SET: &str = "Viewer@loading_bar/0";
// PcGames constructor0x628c90 stores this class; draw0x628eb0 paints the
// two complementary native text clips. It is not inferred from colours.
pub const FORM: u32 = 0x67_24F8;
const GAP: i32 = 4;
const WIDTH: i32 = 46;
const HEIGHT: i32 = 23;

/// Include the optional asset only when its native header is supported.
/// The same resulting description crosses to the renderer, so missing art
/// cannot break the first compile or hold a mandatory UI group pending.
pub fn description(files: &civ3_assets::GameFiles) -> &'static str {
    let supported = files.read(SOURCE).ok().and_then(|bytes| civ3_assets::flc::layout(&bytes).ok())
        .is_some_and(|layout| (layout.width, layout.height) == (93, 46)
            && u16::from_le_bytes([layout.header[6], layout.header[7]]) == scene::loading_cursor::FRAME_COUNT as u16
            && layout.frames >= scene::loading_cursor::FRAME_COUNT as usize);
    if supported { include_str!("../../../assets/civ3/loading-cursor.toml") } else { "" }
}

/// Place the motif to the left of the bar, centred on its native form.
/// Keeping this beside its form's first draw preserves popup painter order.
pub fn at_draw(frame: &Frame, draw: usize) -> Option<menu::screen::Element> {
    frame.forms.iter().find_map(|&(class, canvas, (x, y), (w, h))| {
        (class == FORM && w > 0 && h > 0 && frame.form_draws.iter().any(|(c, r)| *c == canvas && r.start == draw && !r.is_empty()))
            .then(|| {
                let at = (x - GAP - WIDTH, y + h / 2 - HEIGHT / 2);
                menu::screen::Element::Sprite { name: BAR_SET.into(), at,
                    clip: [at.0, at.1, at.0 + WIDTH, at.1 + HEIGHT], scale_den: 0, palette: String::new() }
            })
    })
}

pub fn is_optional(element: &menu::screen::Element) -> bool {
    matches!(element, menu::screen::Element::Sprite { name, .. } if name == BAR_SET)
}

pub fn extend_bounds(class: u32, bounds: [i32; 4]) -> [i32; 4] {
    if class == FORM { [bounds[0] - WIDTH - GAP, bounds[1], bounds[2], bounds[3]] } else { bounds }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_optional_loading_art_does_not_change_a_description() {
        let files = civ3_assets::GameFiles::with_roots(Vec::new());
        assert_eq!(description(&files), "");
        assert!(is_optional(&menu::screen::Element::Sprite { name: BAR_SET.into(), at: (0, 0), clip: [0; 4], scale_den: 0, palette: String::new() }));
        assert_eq!(extend_bounds(0x66b3d8, [0, 0, 1024, 768]), [0, 0, 1024, 768], "parent/advisor bounds never grow");
    }

    #[test]
    fn game_cursor_asset_has_exact_display_order_transparency_and_shared_frames() {
        let root = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        let files = civ3_assets::GameFiles::complete(&root);
        if files.resolve(SOURCE).is_err() { return eprintln!("game cursor fixture unavailable"); }
        let text = description(&files);
        assert!(!text.is_empty());
        let desc = render_desc::Description::parse(&format!("grid = {{ layout = \"orthogonal\", step = [1, 1] }}\n{text}")).unwrap();
        let loader = civ3_assets::loader::Civ3Loader::new(files.clone());
        let compiled = desc.compile(&loader, 256).unwrap();
        let layout = desc.compile_layout(&loader, 256).unwrap();
        assert_eq!(compiled.sprite_names, layout.sprite_names);
        let animation = civ3_assets::flc::decode(&files.read(SOURCE).unwrap()).unwrap();
        assert_eq!((animation.speed_ms, animation.header_frames), (83, 30));
        for (set_name, size) in [(scene::loading_cursor::SET, (93, 46)), (BAR_SET, (46, 23))] {
            let set = compiled.set(set_name).unwrap().0;
            for frame in 0..30 {
                let sprite = compiled.sets.frame(set, frame).unwrap();
                let expected = compiled.sprite_names.iter().position(|name| name == &format!("{}/{}", if set_name == BAR_SET { "Viewer@loading_bar_frame" } else { "Viewer@loading_cursor_frame" }, frame)).unwrap();
                assert_eq!(sprite.0 as usize, expected);
                let art = &compiled.atlas.sprites[expected];
                assert_eq!((art.width, art.height), size);
            }
            assert!(compiled.sets.frame(set, 30).is_none(), "closing delta frame must never display");
        }
        // The shared native frames retain every palette pixel, including
        // clear magenta, before the GPU chooses an animation phase.
        let native = compiled.set(scene::loading_cursor::SET).unwrap().0;
        for frame in 0..30 {
            let id = compiled.sets.frame(native, frame).unwrap();
            let art = &compiled.atlas.sprites[id.0 as usize];
            assert_eq!(art.palette, 0);
            let source = &animation.frames[frame as usize];
            for (i, &index) in source.pixels.iter().enumerate() {
                let rgb = source.palette.get(index);
                let expected = if index == 255 { [0; 4] } else { [rgb.0, rgb.1, rgb.2, 255] };
                let actual = compiled.atlas.pages[art.page as usize].get(art.atlas_x + i as u32 % 93, art.atlas_y + i as u32 / 93);
                // Transparent RGB is immaterial; alpha and every visible
                // source palette colour must be exact.
                assert_eq!(actual[3], expected[3]);
                if index != 255 { assert_eq!(actual, expected); }
            }
        }
        let frame = Frame {
            forms: vec![(FORM, 40, (100, 80), (200, 24))],
            form_draws: vec![(40, 0..1)],
            draws: vec![crate::ui::ScreenDraw::Fill { rect: [100, 80, 200, 24], rgb: [255; 3], alpha: 255, clip: [100, 80, 300, 104] }],
            ..Default::default()
        };
        let items = crate::ui_screen::items(&frame, &[], &[], 0);
        let fonts = crate::text::Fonts::default();
        let sprites = crate::ui_screen::sprites(&items, &fonts, &compiled, u32::MAX, false);
        assert_eq!(sprites[0].ticks_per_frame, scene::loading_cursor::TICKS_PER_FRAME);
        let id = compiled.sets.frame(tile_render::SpriteSetId::from_raw(sprites[0].set), 0).unwrap();
        let art = &compiled.atlas.sprites[id.0 as usize];
        assert_eq!((sprites[0].x + art.anchor_x, sprites[0].y + art.anchor_y), (50, 81), "ring is on the left, not over the progress text");
        assert_eq!(sprites[0].clip, Some([50, 81, 96, 104]));
        let absent = crate::text::readiness_tests::fixture_art(&[], "");
        assert_eq!(crate::ui_screen::readiness(items.iter(), &fonts, &absent, false), crate::ui_screen::Readiness::Ready);
        assert_eq!(crate::ui_screen::sprites(&items, &fonts, &absent, u32::MAX, false).len(), 1, "missing optional ring leaves the progress background visible");
        let palette = &animation.frames[0].palette;
        assert_eq!(palette.get(255), civ3_assets::Rgb(255, 0, 255));
        assert!(animation.frames[0].pixels.contains(&255));
        assert_ne!(animation.frames[0].pixels, animation.frames[1].pixels);
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
        let mut renderer = tile_render::TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
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
