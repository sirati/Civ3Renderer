//! Reached keyed-read regressions from /tmp/c3r-live-integrate.
use super::*;
use civ3_ui::ui::{Event, FileSample, KeySources, UiModel, KEY_RGB};

fn cleared(canvas: u32, size: (i32, i32)) -> UiModel {
    let mut model = UiModel::new(vec![], vec![]);
    model.apply(Event::Size { image: canvas, size });
    model.apply(Event::Fill { canvas, rect: [0, 0, size.0, size.1], rgb: KEY_RGB, alpha: 255, clip: None });
    model
}

#[test]
fn recorded_first_city_caption_is_queried_in_canvas_coordinates() {
    // Exact run from the failure dump. The viewer places the 1024x768
    // advisor at (448,156) on the 1920x1080 display.
    let run = TextRun { x: 111, y: 329, color: [0; 3], face: "Lucida Sans".into(), height: -12, weight: 0, italic: true, underline: false, text: "Alcácer do...".into(), backdrop: None };
    let fixture: toml::Table = include_str!("../tests/fixtures/keyed/first-city-glyphs.toml").parse().unwrap();
    let int = |t: &toml::Table, k: &str| t[k].as_integer().unwrap() as i32;
    let mut font = GlyphFont { ascent: int(&fixture, "ascent"), underline: (1, 1), glyphs: BTreeMap::new() };
    for glyph in fixture["glyphs"].as_array().unwrap() {
        let g = glyph.as_table().unwrap();
        let pixels = |key: &str| g[key].as_array().unwrap().iter().map(|p| {
            let p = p.as_array().unwrap();
            std::array::from_fn(|i| p[i].as_integer().unwrap() as u8)
        }).collect::<Vec<[u8; 3]>>();
        // The fixture holds the renderer's dark/light sheet cells. Build
        // precisely the same loaded coverage atlas as fonts::from_cache.
        let dark = pixels("dark");
        let light = pixels("light");
        let rgba = gdi_text::paint(&dark, &light, run.color);
        let width = int(g, "width") as u32;
        let image = tile_render::RgbaImage::from_fn(width.max(1), int(g, "height") as u32, |x, y| {
            if x < width { rgba[(y * width + x) as usize] } else { [0; 4] }
        });
        font.glyphs.insert(char::from_u32(int(g, "code") as u32).unwrap(), Glyph { left: int(g, "left"), advance: int(g, "advance"), extent: int(g, "extent"), image });
    }
    let mut fonts = Fonts::default();
    fonts.fonts.insert(key(&run), Some(font));
    let compiled = readiness_tests::fixture_art(&[], "");
    let screen = TextRun { x: run.x + 448, y: run.y + 156, ..run.clone() };
    fonts.publish_hit(&screen, &compiled);
    let local = keyed_pixel(&run, 132, 324);
    assert_eq!(local, Some(None), "the recorded read lies in the caption key");
    assert_eq!(local, keyed_pixel(&screen, 580, 480));
    let mut model = cleared(0xa38e4c, (1024, 768));
    model.apply(Event::TextClipped { canvas: 0xa38e4c, run: run.clone(), clip: [0, 0, 1024, 768] });
    assert_eq!(model.keyed_at(0xa38e4c, 132, 324, &crate::ui_opacity::KeyFiles), local.map(|p| p.is_some()));
    // Check ink and empty space around the actual read, with the same
    // retained source used after a form move or a clipped canvas copy.
    for y in 313..333 { for x in 100..200 {
        assert_eq!(keyed_pixel(&run, x, y), keyed_pixel(&screen, x + 448, y + 156));
    }}
}

#[test]
#[cfg(not(windows))]
fn recorded_blank_hud_reads_the_viewers_resolved_animation() {
    const CANVAS: u32 = 0x9fdd84;
    const OPENED: &str = r"C:\Games\Civilization III Complete\Art\Units\Worker\WorkerDefault.flc";
    let mut model = cleared(CANVAS, (1920, 1080));
    // PCX samples at (229,68), derived from the recorded box-right draw.
    for (image, sprite, path) in [(1, 11, "box right color.pcx"), (2, 12, "box right alpha.pcx")] {
        model.apply(Event::Loaded { image, path: path.into(), size: (294, 137) });
        model.apply(Event::Sliced { sprite, image, rect: [0, 24, 294, 64] });
    }
    model.apply(Event::MaskedBlit { sprite: 11, mask: 12, background: 1, canvas: CANVAS, x: 1621, y: 966, clip: None });
    model.apply(Event::FlcLoaded { flic: 3, path: OPENED.into() });
    model.apply(Event::FlcBlit { flic: 3, chunk: 33, canvas: CANVAS, x: 1729, y: 969, clip: Some([1180, 966, 1920, 1030]), plain: false });
    struct Samples;
    impl KeySources for Samples {
        fn file(&self, path: &str, x: i32, y: i32, _: Option<&menu::screen::PaletteFrom>) -> Option<FileSample> {
            assert_eq!((x, y), (229, 68));
            match path {
                "box right color.pcx" => Some(FileSample { index: 230, rgb: [255, 255, 232] }),
                "box right alpha.pcx" => Some(FileSample { index: 0, rgb: [255; 3] }),
                _ => None,
            }
        }
        fn flc(&self, path: &str, frame: u32, x: i32, y: i32, plain: bool) -> Option<Option<FileSample>> {
            assert_eq!((path, frame, x, y, plain), (OPENED, 33, 121, 41, false));
            crate::ui_opacity::KeyFiles.flc(path, frame, x, y, plain)
        }
    }
    assert_eq!(model.keyed_at(CANVAS, 1850, 1010, &Samples), None, "unknown animation identity must not guess a key");
    // Frame headers preserve the file's recorded 50x51 extent and frame 33.
    // No pixel body is needed: (121,41) is outside that source.
    let mut bytes = vec![0; 128];
    bytes[4..6].copy_from_slice(&0xAF12u16.to_le_bytes());
    bytes[6..8].copy_from_slice(&34u16.to_le_bytes());
    bytes[8..10].copy_from_slice(&50u16.to_le_bytes());
    bytes[10..12].copy_from_slice(&51u16.to_le_bytes());
    bytes[12..14].copy_from_slice(&8u16.to_le_bytes());
    for _ in 0..34 {
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&0xF1FAu16.to_le_bytes());
        bytes.extend_from_slice(&[0; 10]);
    }
    let frame = model.canvas_frame(CANVAS, 0).unwrap();
    let items = crate::ui_screen::items(&frame, &[std::path::PathBuf::from(r"C:\Games\Civilization III Complete")], &[], 0);
    let source = "Art/Units/Worker/WorkerDefault.flc";
    assert!(items.iter().any(|i| matches!(i, crate::ui_screen::Item::Element(menu::screen::Element::Sprite { name, .. }, _) if name == &format!("UnitArt@{source}/33"))));
    civ3_assets::source_cache::animation(source, "keyed-hud-recorded-extent.flc", &bytes).unwrap();
    assert_eq!(model.keyed_at(CANVAS, 1850, 1010, &Samples), Some(true));
}

fn science() -> (TextRun, Fonts, Compiled) {
    let fixture: toml::Table = include_str!("../tests/fixtures/keyed/science-outlines.toml").parse().unwrap();
    let int = |t: &toml::Table, k: &str| t[k].as_integer().unwrap() as i32;
    let glyphs = fixture["glyphs"].as_array().unwrap();
    let curves: Vec<_> = glyphs.iter().map(|g| {
        let g = g.as_table().unwrap();
        let curves = g["curves"].as_array().unwrap().iter().map(|q| {
            std::array::from_fn(|i| {
                let p = q.as_array().unwrap()[i].as_array().unwrap();
                (p[0].as_integer().unwrap() as f64, p[1].as_integer().unwrap() as f64)
            })
        }).collect();
        (char::from_u32(int(g, "code") as u32).unwrap(), curves, int(g, "font_advance"))
    }).collect();
    let face = tile_render::gpu_text::Face::from_curves(int(&fixture,"units_per_em") as u32, &curves);
    let run = TextRun { x: 385, y: 100, color: [0;3], face: "Recorded science".into(), height: -12, weight: 0, italic: false, underline: false, text: fixture["text"].as_str().unwrap().into(), backdrop: None };
    let fonts = Fonts::default();
    fonts.set_game_layout(&run, gdi_text::TextLayout { advances: glyphs.iter().map(|g|int(g.as_table().unwrap(),"advance")).collect(), ascent: 12, descent: 3, underline: (1,1) });
    let mut compiled = readiness_tests::fixture_art(&[], "");
    compiled.fonts.push((run.face.clone(), Arc::new(face)));
    (run,fonts,compiled)
}

#[test]
fn recorded_science_outline_read_uses_each_characters_measured_pen() {
    let (run, fonts, compiled) = science();
    let mut sprites = Vec::new();
    fonts.run_sprites(&run, None, &compiled, 0, &mut sprites);
    assert_eq!(sprites.len(), run.text.chars().count());
    let ink: Vec<_> = (88..104).flat_map(|y| (385..470).map(move |x|(x,y))).filter(|&(x,y)| keyed_pixel(&run,x,y).is_some_and(|p|p.is_some())).collect();
    assert!(ink.iter().any(|&(x,_)|x>430), "later glyphs must not be overlaid at the first pen");
    for (x,y) in ink {
        let covering = sprites.iter().filter(|s| {
            let g = s.glyph.unwrap();
            let face = &compiled.fonts[g.font as usize].1;
            let text = tile_render_shared::text::TextGlyph { glyph: face.record(face.glyph(g.ch).unwrap()), em: g.em64, advance: g.advance, ..Default::default() };
            let place = tile_render_shared::text::place(&face.words,&text,s.x,s.y,1,1).unwrap();
            tile_render_shared::text::keyed(tile_render_shared::text::coverage_all_channels(&face.words,&place,x,y,0),0,0xff00ff).is_some()
        }).count();
        assert!(covering>0, "query ink ({x},{y}) must belong to a viewer glyph at its emitted pen");
    }
    let moved = TextRun { x: run.x+448, y:run.y+156, ..run.clone() };
    for y in 88..104 { for x in 380..475 {
        assert_eq!(keyed_pixel(&run,x,y),keyed_pixel(&moved,x+448,y+156));
    }}
}

#[test]
fn unretained_native_text_never_claims_exact_coverage_inside_its_clip() {
    let (mut run, fonts, compiled) = science();
    run.x=0; run.y=0;
    let source=fonts.hit_source(&run,&compiled).unwrap();
    let hit=HitRun {run:run.clone(),source,accuracy:KeyAccuracy::Unavailable};
    assert_eq!(hit.pixel(3,-4),None);
    assert_eq!(hit.pixel(100,20),None,"outline emptiness does not prove Wine emptiness");
    struct Missing;
    impl KeySources for Missing {
        fn file(&self,_:&str,_:i32,_:i32,_:Option<&menu::screen::PaletteFrom>)->Option<FileSample> {None}
    }
    let mut model=cleared(1,(120,40));
    run.x=10;run.y=20;
    model.apply(Event::TextClipped {canvas:1,run,clip:[10,8,90,24]});
    assert_eq!(model.keyed_at(1,13,16,&Missing),None);
    assert_eq!(model.keyed_at(1,100,30,&Missing),Some(false),"outside the recorded clip remains known");
    model.apply(Event::Fill {canvas:1,rect:[12,15,2,2],rgb:[0;3],alpha:255,clip:None});
    assert_eq!(model.keyed_at(1,13,16,&Missing),Some(true),"a later opaque record establishes its own pixels");
}

#[test]
fn recorded_padded_console_button_retains_unknown_corner_after_contiguous_clear() {
    use jgl::{FillCall, ImageId, Rect, Surface};
    let target=Surface {image:ImageId(1),owner:0xa0b3d0,row:16,clip:Rect::new(0,0,15,15),bounds:Rect::new(0,0,15,15)};
    let call=FillCall::Fill {target,rect:None,color:0x80007c1f};
    let civ3_ui::draw::CanvasDraw::Fill {canvas,size,rects,rgb,alpha,clip}=civ3_ui::draw::fill_of(&call).unwrap() else {panic!()};
    assert_eq!(rects,[[0,0,15,14],[0,14,1,1]]);
    assert_eq!(target.fill_regions(Some(target.bounds)), [target.bounds], "the explicit rectangle path visits each row");
    let mut model=UiModel::new(vec![],vec![]);
    model.apply(Event::Size {image:canvas,size:size.unwrap()});
    for rect in rects { model.apply(Event::Fill {canvas,rect,rgb,alpha,clip}); }
    // The recorded consoleButtons.pcx slice is [1,1,14,15]. Its opaque
    // file pixels overwrite columns 0..13, including the incomplete row.
    let pixels=Arc::new(civ3_ui::selection::FilePixels {size:(49,49),row:49,indices:vec![0;49*49],palette:[[16,32,48];256]});
    model.apply(Event::Loaded {image:2,path:"consoleButtons.pcx".into(),size:(49,49)});
    model.loaded_pixels(2,pixels);
    model.apply(Event::Sliced {sprite:3,image:2,rect:[1,1,14,15]});
    model.apply(Event::Blit {sprite:3,canvas,x:0,y:0,clip:None,palette:None});
    assert_eq!(model.keyed_at(canvas,13,14,&crate::ui_opacity::KeyFiles),Some(true));
    assert_eq!(model.keyed_at(canvas,14,13,&crate::ui_opacity::KeyFiles),Some(false));
    assert_eq!(model.keyed_at(canvas,14,14,&crate::ui_opacity::KeyFiles),None,"the legacy 0x4210 is uninitialized content");
    model.apply(Event::Fill {canvas,rect:[14,14,1,1],rgb:KEY_RGB,alpha:255,clip:None});
    assert_eq!(model.keyed_at(canvas,14,14,&crate::ui_opacity::KeyFiles),Some(false),"a later known writer resolves that pixel");
}
