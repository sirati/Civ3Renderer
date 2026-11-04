//! Reached keyed-read regressions from /tmp/c3r-live-integrate.
use super::*;
use civ3_ui::ui::{Event, FileSample, KeySources, UiModel, KEY_RGB};

fn cleared(canvas: u32, size: (i32, i32)) -> UiModel {
    let mut model = UiModel::new(vec![], vec![]);
    model.apply(Event::Size { image: canvas, size });
    model.apply(Event::Fill { canvas, rect: [0, 0, size.0, size.1], rgb: KEY_RGB, alpha: 255, clip: None });
    model
}

/// Compile the renderer's own atlas description from supplied glyph sources.
/// The loader returns those source images, so these fixtures need no host fonts.
fn glyph_art(fonts: &Fonts) -> Compiled { glyph_art_filtered(fonts, &|_| true, 256) }

fn glyph_art_filtered(fonts: &Fonts, keep: &dyn Fn(&str) -> bool, page_size: u32) -> Compiled {
    glyph_art_growing(fonts, keep, page_size).0
}

fn glyph_art_growing(fonts: &Fonts, keep: &dyn Fn(&str) -> bool, page_size: u32) -> (Compiled, render_desc::Growing) {
    struct Loaded<'a>(&'a Fonts);
    impl render_desc::ImageLoader for Loaded<'_> {
        fn load(&self, source: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
            for ((style, [r, g, b], backdrop), font) in &self.0.fonts {
                let Some(font) = font else { continue };
                let on = backdrop.map_or(String::new(), |[r, g, b]| format!("/on={r},{g},{b}"));
                let name = format!("font:{}/{}/{r},{g},{b}/{}/{}{on}", style.face, style.height, style.weight, u8::from(style.italic));
                if source == name {
                    return Ok(charset().map(|c| font.glyphs.get(&c).map(|g| g.image.clone()).unwrap_or_else(|| tile_render::RgbaImage::new(1, 1))).collect());
                }
            }
            Err(format!("unknown synthetic glyph source {source}"))
        }
    }
    let description = format!("grid = {{ layout = \"orthogonal\", step = [1, 1] }}\n{}", Fonts::entries(&fonts.fonts, false));
    let mut description: toml::Table = description.parse().unwrap();
    description.get_mut("sheets").unwrap().as_table_mut().unwrap().retain(|_, value| {
        let sheet = value.as_table().unwrap();
        let name = format!("{}/{}", sheet["group"].as_str().unwrap(), sheet["first_index"].as_integer().unwrap());
        keep(&name)
    });
    render_desc::Description::parse(&toml::to_string(&description).unwrap()).unwrap().compile_growing(&Loaded(fonts), page_size, true).unwrap()
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
    let compiled = glyph_art(&fonts);
    let screen = TextRun { x: run.x + 448, y: run.y + 156, ..run.clone() };
    fonts.publish_hit_resources(&compiled);
    let local = keyed_pixel(&run, 132, 324);
    assert_eq!(local, Some(None), "the recorded read lies in the caption key");
    assert_eq!(local, keyed_pixel(&screen, 580, 480));
    let mut model = cleared(0xa38e4c, (1024, 768));
    model.apply(Event::TextClipped { canvas: 0xa38e4c, run: run.clone(), clip: [0, 0, 1024, 768] });
    assert_eq!(model.keyed_at(0xa38e4c, 132, 324, &crate::ui_opacity::KeyFiles), local.map(|p| p.is_some()));
    // A native hit can ask for a new string before the publisher emits it.
    // The loaded font is already a renderer source for this run too.
    let fresh=TextRun {text:"....".into(),..run.clone()};
    let source=fonts.hit_source(&TextRun {x:0,y:0,..fresh.clone()},&HIT_RESOURCES.load().unwrap()).unwrap();
    let expected=HitRun {run:TextRun {x:0,y:0,..fresh.clone()},source};
    for y in 313..333 { for x in 100..150 {
        assert_eq!(keyed_pixel(&fresh,x,y),expected.pixel(x-fresh.x,y-fresh.y));
    }}
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
    let mut ink = 0;
    for y in 80..110 { for x in 380..485 {
        let rendered = sprites.iter().filter_map(|s| {
            let g = s.glyph.unwrap();
            let face = &compiled.fonts[g.font as usize].1;
            let flags=tile_render_shared::text::TEXT_BOLD*u32::from(g.bold)
                |tile_render_shared::text::TEXT_ITALIC*u32::from(g.italic);
            let text = tile_render_shared::text::TextGlyph { glyph: face.record(face.glyph(g.ch).unwrap()), em: g.em64, advance: g.advance, flags, ..Default::default() };
            let place = tile_render_shared::text::place(&face.words,&text,s.x,s.y,1,1).unwrap();
            // These are the viewer's coverage and key-mix functions, applied
            // to its emitted instances. Check empty pixels as well as ink.
            tile_render_shared::text::keyed(tile_render_shared::text::coverage_all_channels(&face.words,&place,x,y,0),0,0xff00ff)
                .map(|rgb|rgb.map(|c|c as u8))
        }).last();
        assert_eq!(keyed_pixel(&run,x,y),Some(rendered),"viewer pixel ({x},{y})");
        if rendered.is_some() && x>430 {ink+=1;}
    }}
    assert!(ink>0,"later glyphs must not be overlaid at the first pen");
    let moved = TextRun { x: run.x+448, y:run.y+156, ..run.clone() };
    for y in 88..104 { for x in 380..475 {
        assert_eq!(keyed_pixel(&run,x,y),keyed_pixel(&moved,x+448,y+156));
    }}
}

#[test]
fn retained_text_answers_from_the_renderer_without_native_certification() {
    let (run, fonts, compiled) = science();
    fonts.publish_hit_resources(&compiled);
    for y in 80..110 { for x in 380..480 {
        assert!(keyed_pixel(&run,x,y).is_some());
    }}
    assert_eq!(keyed_pixel(&TextRun {face:"missing source".into(),..run}, 0, 0),None);
}

#[test]
fn recorded_padded_console_button_leaves_unwritten_corner_transparent() {
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
    assert_eq!(model.shown_at(canvas,14,14,&|_,_,_,_|None),civ3_ui::ui::Shown::Nothing);
    assert_eq!(model.keyed_at(canvas,14,14,&crate::ui_opacity::KeyFiles),Some(false),"the viewer draws nothing at the unwritten corner");
    // A subsequent frame texture operation must use the same transparent
    // base, rather than adding Unknown solely for the padded tail.
    assert_eq!(model.tile_under(2,[1,1,15,1],canvas,[0,14,15,1],0xfd,None),Ok(()));
    assert_eq!(model.keyed_at(canvas,14,14,&crate::ui_opacity::KeyFiles),Some(false));
    model.apply(Event::Fill {canvas,rect:[14,14,1,1],rgb:KEY_RGB,alpha:255,clip:None});
    assert_eq!(model.keyed_at(canvas,14,14,&crate::ui_opacity::KeyFiles),Some(false),"a later known writer resolves that pixel");
}

fn hit_atlas_fixture() -> (TextRun, Fonts, Compiled) {
    let run = TextRun { x: 31, y: 27, color: [0; 3], face: "Hit cache fixture".into(), height: -12, weight: 400, italic: false, underline: false, text: "x".into(), backdrop: None };
    let font = GlyphFont {
        ascent: 1,
        underline: (1, 1),
        glyphs: BTreeMap::from([('x', Glyph { left: 0, advance: 2, extent: 1,
            image: tile_render::RgbaImage::from_fn(1, 1, |_, _| [255; 4]),
        })]),
    };
    let mut fonts = Fonts::default();
    fonts.insert(key(&run), Ok(font));
    let compiled = glyph_art(&fonts);
    (run, fonts, compiled)
}

#[test]
fn atlas_underline_hits_follow_the_drawn_sheet_segments_and_fitted_pens() {
    let (mut run, fonts, _) = hit_atlas_fixture();
    run.underline = true;
    run.text = "xx".into();
    // Fitted pens are three pixels apart, while each actual underline
    // sheet is two pixels wide. The one-pixel gap must stay transparent.
    fonts.set_game_width(&run, 6);
    let tag = Fonts::tag(&key(&run));
    struct Generated;
    impl render_desc::ImageLoader for Generated {
        fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
            Err("only generated solids are used".into())
        }
    }
    let description = format!(r#"
grid = {{ layout = "orthogonal", step = [1, 1] }}
[images.glyph]
source = "solid:255,255,255,255:1x1"
[sheets.glyph]
image = "glyph"
cell = [1, 1]
anchor = [0, -1]
group = "Text@{tag}"
first_index = 120
[images.line]
source = "solid:0,0,0,255:2x1"
[sheets.line]
image = "line"
cell = [2, 1]
anchor = [0, 1]
group = "TextLine@{tag}"
first_index = 2
"#);
    let compiled = render_desc::Description::parse(&description).unwrap().compile(&Generated, 64).unwrap();
    let mut sprites = Vec::new();
    fonts.run_sprites(&run, None, &compiled, 0, &mut sprites);
    assert_eq!(sprites.len(), 4, "two glyphs and their two underline sheets");
    assert!(sprites.iter().all(|s| s.glyph.is_none()), "exercise atlas drawing");
    assert_eq!(sprites.iter().map(|s| s.x).collect::<Vec<_>>(), [31,31,34,34]);
    for y in run.y..=run.y + 2 {
        for x in run.x - 1..=run.x + 6 {
            let underlined = y == run.y + 1 && ((31..33).contains(&x) || (34..36).contains(&x));
            let drawn = sprites.iter().any(|s| {
                let id = compiled.sets.frame(tile_render::SpriteSetId::from_raw(s.set), 0).unwrap();
                let sprite = &compiled.atlas.sprites[id.0 as usize];
                let gx = x - s.x - sprite.anchor_x;
                let gy = y - s.y - sprite.anchor_y;
                gx >= 0 && gy >= 0 && gx < sprite.width as i32 && gy < sprite.height as i32
                    && compiled.atlas.pages[sprite.page as usize].get(sprite.atlas_x + gx as u32, sprite.atlas_y + gy as u32)[3] != 0
            });
            assert_eq!(drawn, underlined, "actual underline sprite ({x},{y})");
            assert_eq!(keyed_pixel(&run, x, y), Some(underlined.then_some(run.color)), "underline ({x},{y})");
        }
    }
}

#[test]
fn hit_placement_eviction_rebuilds_from_resources_without_retaining_labels() {
    let (run, fonts, compiled) = hit_atlas_fixture();
    fonts.publish_hit_resources(&compiled);
    let resources = HIT_RESOURCES.load().unwrap();
    let mut cache = HitCache::new(resources.clone());
    let original = cache.pixel(&run, run.x, run.y - 1);
    assert_eq!(original, Some(Some(run.color)));
    for len in 2..128 {
        let label = TextRun { text: "x".repeat(len), ..run.clone() };
        assert_eq!(cache.pixel(&label, label.x, label.y - 1), original);
        fonts.publish_hit_resources(&compiled);
        assert!(Arc::ptr_eq(&resources, &HIT_RESOURCES.load().unwrap()), "draw history must not change the resource snapshot");
    }
    assert_eq!(cache.pixel(&run, run.x, run.y - 1), original, "an evicted label remains supported");
    let long = TextRun { text: "x".repeat(HIT_LABEL_BYTES + 1), ..run.clone() };
    assert_eq!(cache.pixel(&long, long.x, long.y - 1), original);
    assert!(cache.run.is_none(), "oversized placement must stay transient");
    assert_eq!(cache.pixel(&run, run.x, run.y - 1), original);
    assert_eq!(resources.atlases.len(), 1);
}

#[test]
fn reloaded_atlas_and_equal_revision_publishers_replace_cached_hits() {
    let (run, mut fonts, compiled) = hit_atlas_fixture();
    fonts.publish_hit_resources(&compiled);
    assert_eq!(keyed_pixel(&run, run.x, run.y - 1), Some(Some(run.color)));
    let mut replacement = fonts.fonts[&key(&run)].clone().unwrap();
    replacement.glyphs.get_mut(&'x').unwrap().image = tile_render::RgbaImage::from_fn(1, 1, |_, _| [0; 4]);
    fonts.insert(key(&run), Ok(replacement));
    fonts.publish_hit_resources(&compiled);
    assert_eq!(keyed_pixel(&run, run.x, run.y - 1), Some(None), "a reload must invalidate derived placement");
    let (_, mut fresh, _) = hit_atlas_fixture();
    // A separate publisher can reach exactly the same numeric revision.
    let fresh_font = fresh.fonts[&key(&run)].clone().unwrap();
    fresh.insert(key(&run), Ok(fresh_font));
    assert_eq!(fonts.revision, fresh.revision);
    fresh.publish_hit_resources(&compiled);
    assert_eq!(keyed_pixel(&run, run.x, run.y - 1), Some(Some(run.color)), "revision numbers belong to their publisher");
}

#[test]
fn outline_replacement_invalidates_existing_label_placement() {
    let (run, fonts, mut compiled) = science();
    fonts.publish_hit_resources(&compiled);
    let ink = (80..110).flat_map(|y| (380..485).map(move |x| (x, y)))
        .find(|&(x, y)| keyed_pixel(&run, x, y).is_some_and(|pixel| pixel.is_some())).unwrap();
    let blank = tile_render::gpu_text::Face::from_curves(12, &[]);
    compiled.fonts.iter_mut().find(|(name, _)| name == &run.face).unwrap().1 = Arc::new(blank);
    fonts.publish_hit_resources(&compiled);
    assert_eq!(HIT_RESOURCES.load().unwrap().faces.iter().find(|(name, _)| name == &run.face).unwrap().1.glyph_count(), 0);
    assert_eq!(HitCache::new(HIT_RESOURCES.load().unwrap()).pixel(&run, ink.0, ink.1), Some(None));
    assert_eq!(keyed_pixel(&run, ink.0, ink.1), Some(None), "a face replacement must not retain old outlines");
}


fn fallback_fixture(text: &str) -> (TextRun, Fonts, Arc<tile_render::gpu_text::Face>) {
    let run = TextRun { x: 31, y: 27, color: [0; 3], face: "Synthetic fallback".into(), height: -12,
        weight: 700, italic: true, underline: true, text: text.into(), backdrop: None };
    let glyphs = text.chars().map(|c| (c, Glyph { left: -1, advance: 4, extent: 3,
        image: tile_render::RgbaImage::from_fn(3, 4, |x, y| if (x + y) % 3 == 0 { [0; 4] } else { [255; 4] }),
    })).collect();
    let mut fonts = Fonts::default();
    fonts.insert(key(&run), Ok(GlyphFont { ascent: 4, underline: (1, 1), glyphs }));
    let curves = vec![[(0., 0.), (1.5, 0.), (3., 0.)], [(3., 0.), (3., 2.), (3., 4.)],
        [(3., 4.), (1.5, 4.), (0., 4.)], [(0., 4.), (0., 2.), (0., 0.)]];
    let glyphs: Vec<_> = text.chars().map(|c| (outline_char(c), curves.clone(), 4)).collect();
    let face = Arc::new(tile_render::gpu_text::Face::from_curves(12, &glyphs));
    (run, fonts, face)
}

/// Evaluate the actual emitted renderer instances using their atlas texels
/// or the renderer's shared outline coverage. No alternative layout is used.
fn assert_hit_instances(run: &TextRun, fonts: &Fonts, compiled: &Compiled) -> (Vec<scene::FreeSprite>, usize) {
    let mut sprites = Vec::new();
    fonts.run_sprites(run, None, compiled, 0, &mut sprites);
    let mut ink = 0;
    for y in run.y - 25..run.y + 10 {
        for x in run.x - 5..run.x + 70 {
            let rendered = sprites.iter().filter_map(|s| {
                if let Some(g) = s.glyph {
                    let face = &compiled.fonts[g.font as usize].1;
                    if let Some((top, height)) = g.bar {
                        return (x >= s.x && x < s.x + g.advance && y >= s.y + top && y < s.y + top + height).then_some(run.color);
                    }
                    if face.glyph_count() == 0 { return None; }
                    let flags = tile_render_shared::text::TEXT_BOLD * u32::from(g.bold)
                        | tile_render_shared::text::TEXT_ITALIC * u32::from(g.italic);
                    let text = tile_render_shared::text::TextGlyph {
                        glyph: face.record(face.glyph(g.ch).unwrap_or(0)), em: g.em64, advance: g.advance, flags, ..Default::default()
                    };
                    let place = tile_render_shared::text::place(&face.words, &text, s.x, s.y, 1, 1).unwrap();
                    let (l, t, r, b) = tile_render_shared::text::bounds(&face.words, &place);
                    if x < l || y < t || x >= r || y >= b { return None; }
                    let color = s.tint;
                    let key = g.key.unwrap_or(KEY_RGB);
                    let key = u32::from(key[0]) << 16 | u32::from(key[1]) << 8 | u32::from(key[2]);
                    tile_render_shared::text::keyed(tile_render_shared::text::coverage_all_channels(&face.words, &place, x, y, color), color, key)
                        .map(|rgb| rgb.map(|c| c as u8))
                } else {
                    let id = compiled.sets.frame(tile_render::SpriteSetId::from_raw(s.set), 0)?;
                    if !compiled.sprite_available(id) { return None; }
                    let source = &compiled.atlas.sprites[id.0 as usize];
                    let (gx, gy) = (x - s.x - source.anchor_x, y - s.y - source.anchor_y);
                    if gx < 0 || gy < 0 || gx >= source.width as i32 || gy >= source.height as i32 { return None; }
                    let p = compiled.atlas.pages[source.page as usize].get(source.atlas_x + gx as u32, source.atlas_y + gy as u32);
                    match tile_render::Blend::from_scale_den(s.scale_den, s.tint) {
                        tile_render::Blend::Subpixel { color } => keyed_mix(color, [p[0], p[1], p[2]], KEY_RGB),
                        tile_render::Blend::Alpha => (p[3] != 0).then_some([p[0], p[1], p[2]]),
                        other => panic!("unexpected text blend {other:?}"),
                    }
                }
            }).last();
            assert_eq!(keyed_pixel(run, x, y), Some(rendered), "emitted pixel ({x},{y}) for {:?}", run.text);
            ink += usize::from(rendered.is_some());
        }
    }
    (sprites, ink)
}

#[test]
fn atlas_upload_and_partial_set_changes_select_the_same_hit_instances() {
    let (run, fonts, face) = fallback_fixture("AB");
    let mut waiting = readiness_tests::fixture_art(&[], "");
    waiting.fonts.push((run.face.clone(), face.clone()));
    let (sprites, ink) = assert_hit_instances(&run, &fonts, &waiting);
    assert!(ink > 0);
    assert!(sprites.iter().all(|s| s.glyph.is_some()), "not uploaded yet uses outline fallback");
    let atlases = HIT_RESOURCES.load().unwrap().atlases.clone();
    let mut uploaded = glyph_art(&fonts);
    uploaded.fonts.push((run.face.clone(), face.clone()));
    let (sprites, ink) = assert_hit_instances(&run, &fonts, &uploaded);
    assert!(ink > 0);
    assert!(sprites.iter().all(|s| s.glyph.is_none()), "uploaded glyphs use the atlas");
    assert!(Arc::ptr_eq(&atlases, &HIT_RESOURCES.load().unwrap().atlases), "art changes must reuse loaded glyph images");
    let mut partial = glyph_art_filtered(&fonts, &|name| name.ends_with("/65"), 256);
    partial.fonts.push((run.face.clone(), face));
    let (sprites, ink) = assert_hit_instances(&run, &fonts, &partial);
    assert!(ink > 0);
    assert_eq!(sprites.len(), 1, "later missing glyphs and absent underline sheets emit nothing");
    assert!(sprites[0].glyph.is_none());
}

#[test]
fn unavailable_atlas_placeholders_do_not_answer_from_loaded_glyph_pixels() {
    let (run, fonts, _) = fallback_fixture("A");
    let compiled = glyph_art_filtered(&fonts, &|name| name.starts_with("Text@"), 2);
    let tag = Fonts::tag(&key(&run));
    assert!(compiled.set(&format!("Text@{tag}/65")).is_some());
    assert!(!compiled.set_available(&format!("Text@{tag}/65")));
    let (sprites, ink) = assert_hit_instances(&run, &fonts, &compiled);
    assert_eq!(sprites.len(), 1);
    assert_eq!(ink, 0, "the actual unavailable placeholder has no texels");
}

#[test]
fn legacy_c1_and_nearest_metrics_hits_follow_outline_fallback() {
    let (run, mut fonts, face) = fallback_fixture("A\u{92}B");
    let font = fonts.fonts.get_mut(&key(&run)).unwrap().as_mut().unwrap();
    let old = font.glyphs.get_mut(&'\u{92}').unwrap();
    old.advance = 0;
    old.image.pixels.fill([0; 4]);
    let mut compiled = glyph_art(&fonts);
    compiled.fonts.push((run.face.clone(), face));
    let (sprites, ink) = assert_hit_instances(&run, &fonts, &compiled);
    assert!(ink > 0);
    assert!(sprites.iter().all(|s| s.glyph.is_some()));
    assert!(sprites.iter().any(|s| s.glyph.is_some_and(|g| g.ch == '’' && g.advance > 0)), "the source face supplies the legacy advance");
    let enlarged = TextRun { height: -18, color: [32, 48, 64], ..run.clone() };
    assert!(!fonts.has(&enlarged));
    let (sprites, ink) = assert_hit_instances(&enlarged, &fonts, &compiled);
    assert!(ink > 0);
    assert!(sprites.iter().all(|s| s.glyph.is_some()));
    assert!(sprites.iter().any(|s| s.glyph.is_some_and(|g| g.bar == Some((1, 1)))));
}

#[test]
fn measured_layout_hits_ignore_atlas_upload_and_keep_missing_glyph_underlines() {
    let (run, fonts, face) = fallback_fixture("AB");
    fonts.set_game_layout(&run, gdi_text::TextLayout { advances: vec![5, 7], ascent: 4, descent: 1, underline: (2, 2) });
    let mut compiled = glyph_art(&fonts);
    compiled.fonts.push((run.face.clone(), face));
    let (sprites, ink) = assert_hit_instances(&run, &fonts, &compiled);
    assert!(ink > 0);
    assert_eq!(sprites.len(), 4);
    assert!(sprites.iter().all(|s| s.glyph.is_some()));
    let chars = vec![('A', Vec::new(), 4)];
    compiled.fonts.iter_mut().find(|(name, _)| name == &run.face).unwrap().1 = Arc::new(tile_render::gpu_text::Face::from_curves(12, &chars));
    let (sprites, ink) = assert_hit_instances(&run, &fonts, &compiled);
    assert!(ink > 0, "measured underline segments survive missing cmap entries");
    assert_eq!(sprites.len(), 3, "missing B emits its underline but no glyph-zero substitute");
}

#[test]
fn alternate_size_hits_scale_native_outline_instances_without_host_assets() {
    const CHILD: &str = "CIV3R_TEXT_SIZE_FIXTURE";
    if std::env::var_os(CHILD).is_none() {
        let directory = std::env::temp_dir().join(format!("c3r-text-size-fixture-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let style = FontStyle { face: "Synthetic fallback".into(), height: 12, weight: 700, italic: true };
        std::fs::write(directory.join(format!("{}.toml", civ3_assets::fonts::cache_stem(&style))), "").unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "text::keyed_read_tests::alternate_size_hits_scale_native_outline_instances_without_host_assets"])
            .env(CHILD, "1").env("CIV3R_FONT_CACHE", &directory).output().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
        assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let (native, fonts, face) = fallback_fixture("AB");
    let run = TextRun { height: -19, ..native.clone() };
    assert_eq!(key(&run).0.height, 12, "the synthetic sheet catalogue supplies only the native size");
    let mut compiled = glyph_art(&fonts);
    compiled.fonts.push((run.face.clone(), face));
    let (sprites, ink) = assert_hit_instances(&run, &fonts, &compiled);
    assert!(ink > 0);
    assert!(sprites.iter().all(|s| s.glyph.is_some()));
    assert!(sprites.iter().any(|s| s.glyph.is_some_and(|g| g.bar == Some((2, 2)))));
    fonts.set_game_layout(&native, gdi_text::TextLayout { advances: vec![5, 7], ascent: 4, descent: 1, underline: (2, 1) });
    let (sprites, ink) = assert_hit_instances(&run, &fonts, &compiled);
    assert!(ink > 0);
    assert!(sprites.iter().any(|s| s.glyph.is_some_and(|g| g.bar == Some((3, 2)))));
}


#[test]
fn same_size_set_replacement_invalidates_the_hit_availability_snapshot() {
    let (mut run, fonts, _) = fallback_fixture("A");
    run.underline = false;
    let (mut compiled, mut growing) = glyph_art_growing(&fonts, &|_| true, 256);
    let (_, ink) = assert_hit_instances(&run, &fonts, &compiled);
    assert!(ink > 0);
    let old = HIT_RESOURCES.load().unwrap();
    let sprite_count = compiled.atlas.sprites.len();
    struct Unused;
    impl render_desc::ImageLoader for Unused {
        fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> { Err("growth has no images".into()) }
    }
    let tag = Fonts::tag(&key(&run));
    let growth = growing.grow(&compiled, &format!("[sets.\"Text@{tag}/65\"]\nframes = []\n"), &Unused).unwrap();
    compiled.apply(&growth);
    assert_eq!(compiled.atlas.sprites.len(), sprite_count);
    let (_, ink) = assert_hit_instances(&run, &fonts, &compiled);
    assert_eq!(ink, 0, "the replacement emits an empty set");
    assert!(!Arc::ptr_eq(&old, &HIT_RESOURCES.load().unwrap()), "availability follows resource versions, not sprite counts");
    let snapshot = compiled.without_pixels();
    fonts.publish_hit_resources(&snapshot);
    let current = HIT_RESOURCES.load().unwrap();
    fonts.publish_hit_resources(&compiled);
    assert!(Arc::ptr_eq(&current, &HIT_RESOURCES.load().unwrap()), "metadata copies preserve the published resource snapshot");
}
