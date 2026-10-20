//! Offline Civ III renderer tools, working from tracer dumps.
//!
//!   civ3-tool drawdiff TRACE [--viewport WxH]
//!       compares the sprites we draw with the ones the game drew (same
//!       names, positions and order), for the sprite arrays our render
//!       description covers
//!   civ3-tool render TRACE OUT.png [--viewport WxH] [--reference]
//!       renders the trace's world on the GPU, or with the CPU reference
//!   civ3-tool stats TRACE
//!       how big the atlas compiled for the trace is
//!   civ3-tool preview SCENARIO.biq OUT.png [--viewport WxH]
//!       renders a scenario's world as the Load Scenario screen previews it
//!       (beside the screen's place, left empty)
//!   civ3-tool pixeldiff TRACE GAME.png [DIFF.png]
//!       renders the trace's world and compares it with a screenshot of the
//!       game taken at the same moment, ignoring the screen rectangles of
//!       game draws we do not model (units, cities, borders); DIFF.png shows
//!       differing pixels in magenta (violet under our own unit, label and
//!       interface sprites) and ignored areas darkened. A second
//!       line gives the share for the terrain alone: the pixels none of our
//!       unit, label or interface sprites covers
//!
//!   civ3-tool bench TRACE [--frames N] [--rebuild] [--viewport WxH]
//!       draws the trace's scene as the viewer arranges it, N times
//!       (default 300) into an offscreen target, and prints where the time
//!       goes: CPU per step and, with TILE_RENDER_GPU_TIMING=1 where the
//!       adapter has timestamp queries, GPU per pass; --rebuild builds and
//!       uploads the world anew every frame (as for every new scene)
//!
//! Environment: CIV3_INSTALL (game folder), CIV3_DESC (render description,
//! default assets/civ3/civ3.toml).

use std::collections::HashSet;
use std::process::ExitCode;

use civ3_assets::GameFiles;
use ifc23::assets;
use ifc23::scene_map::Mapper;
use ifc23::{city_hud, city_site, text, ui_screen, units};
use ifc23::world::Trace;
use render_desc::Compiled;
use tile_render::reference;

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("civ3-tool: {e}");
            ExitCode::from(2)
        }
    }
}

fn viewport(args: &[String]) -> Result<(u32, u32), String> {
    let Some(i) = args.iter().position(|a| a == "--viewport") else { return Ok((1920, 1080)) };
    let v = args.get(i + 1).ok_or("--viewport needs WxH")?;
    let (w, h) = v.split_once('x').ok_or("--viewport needs WxH")?;
    Ok((w.parse().map_err(|_| "bad width")?, h.parse().map_err(|_| "bad height")?))
}

/// The description compiled against the game's art (the trace's scenario
/// folders first), with the unit art the trace's world needs.
fn compile(trace: Option<&Trace>) -> Result<(Compiled, Option<(units::UnitArt, text::Fonts)>), String> {
    let install = std::env::var("CIV3_INSTALL")
        .unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    let desc_path = std::env::var("CIV3_DESC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/civ3/civ3.toml").into());
    let mut text = std::fs::read_to_string(&desc_path).map_err(|e| format!("{desc_path}: {e}"))?;
    let files = || match trace {
        Some(t) => assets::game_files(install.as_ref(), t.scenario_folder.as_deref(), &t.search_roots),
        None => GameFiles::complete(&install),
    };
    let mut art = None;
    if let Some(t) = trace {
        let mut a = units::UnitArt::new(files());
        let mut needed = units::Needed::of(&t.world, &mut a);
        let unit_colors = t.world.color_tables.get(t.world.player.max(0) as usize).copied().unwrap_or(0);
        for path in t.ui.as_ref().map(|f| ui_screen::unit_files(f, &[], false)).unwrap_or_default() {
            needed.add_file(&mut a, &path, unit_colors);
        }
        needed.set_plain(&mut a, &t.ui.as_ref().map(|f| ui_screen::unit_files(f, &[], true)).unwrap_or_default());
        text.push_str(&needed.description());
        let mut fonts = text::Fonts::default();
        // CIV3_TEXT=0: leave the game's text out.
        if std::env::var("CIV3_TEXT").as_deref() != Ok("0") {
            fonts.add(&files(), &t.texts);
        }
        // The mirrored interface: its art and text.
        if let Some(frame) = &t.ui {
            let mut screen = menu::screen::ScreenArt::default();
            ui_screen::add_art(&ui_screen::items(frame, &[], GENERATED, unit_colors), &mut screen, &mut fonts, &files());
            text.push_str(&ui_screen::description(&screen));
        }
        // The go-to path's turn count (§10.14).
        let mut env = ifc23::city_labels::FileEnv::new(Box::new(files()));
        let target_items = ifc23::world_items::WorldItems::new(t.view.zoomed_out, |reduced| ifc23::targeting::label_items(&t.world, reduced, &mut env));
        let runs: Vec<text::TextRun> = target_items.iter().filter_map(|i| if let ui_screen::Item::Text(r, _, _) = i { Some(r.clone()) } else { None }).collect();
        fonts.add(&files(), &runs);
        text.push_str(&fonts.description());
        text.push_str(&ifc23::targeting::description());
        // C3X's districts on the map (as live describes them).
        let mut districts = ifc23::district_art::DistrictArt::default();
        districts.add(&ifc23::district_art::slices(&t.world));
        text.push_str(&districts.description());
        art = Some((a, fonts));
    }
    // (One copy of the art for both zooms: a zoomed-out trace's view draws
    // it at half size.)
    Ok((assets::compile(&text, files()).map_err(|e| e.to_string())?, art))
}

/// The scene of a trace, units included.
fn trace_scene(trace: &Trace, compiled: &Compiled, art: Option<&mut (units::UnitArt, text::Fonts)>, viewport: (u32, u32)) -> scene::Scene {
    let mut scene = Mapper::new(compiled).scene(&trace.world, &trace.view, viewport, 0);
    // An open city screen, as live handles it.
    let city = match (trace.world.focus_city, &trace.ui) {
        (Some(_), Some(frame)) => {
            let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
            let files = assets::game_files(install.as_ref(), trace.scenario_folder.as_deref(), &trace.search_roots);
            ui_screen::city_screen(frame, &files, &[], &mut Default::default())
        }
        _ => None,
    };
    let ui = match city {
        Some((_, first)) => trace.ui.as_ref().map(|f| ui_screen::from_draw(f, first)),
        None => trace.ui.clone(),
    };
    if let Some((art, fonts)) = art {
        if ui.is_none() {
            scene.sprites = city_site::sprites(&trace.world, compiled);
        }
        if std::env::var("CIV3_UNITS_FREE").as_deref() == Ok("1") {
            scene.sprites.extend(units::free_sprites(&trace.world, art, compiled, None));
        } else {
            units::place(&trace.world, &trace.view, art, compiled, None, &mut units::UnitMemory::default(), &mut scene);
        }
        ifc23::effects::place(&trace.world, &trace.view, art, compiled, &mut scene);

        // Choosing a target (§10.15), as live draws it.
        scene.sprites.extend(ifc23::targeting::range_sprites(&trace.world, compiled, trace.view.zoomed_out));
        scene.sprites.extend(ifc23::targeting::sprites(&trace.world, trace.view.zoomed_out, art, compiled));
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        let files = assets::game_files(install.as_ref(), trace.scenario_folder.as_deref(), &trace.search_roots);
        let mut env = ifc23::city_labels::FileEnv::new(Box::new(files));
        let target_items = ifc23::world_items::WorldItems::new(trace.view.zoomed_out, |reduced| ifc23::targeting::label_items(&trace.world, reduced, &mut env));
        scene.sprites.extend(target_items.sprites(fonts, compiled, units::unit_passes(compiled).1, false));
        match &ui {
            // The mirrored interface has the city icons and all text.
            Some(frame) => {
                let roots: Vec<std::path::PathBuf> = Vec::new();
                let unit_colors = trace.world.color_tables.get(trace.world.player.max(0) as usize).copied().unwrap_or(0);
                let items = ui_screen::items(frame, &roots, GENERATED, unit_colors);
                scene.sprites.extend(ui_screen::sprites(&items, fonts, compiled, ui_screen::under_pass(compiled), false));
                scene.patches = ui_screen::minimap_patches(&items, &trace.world, compiled, |t| art.table_color(t, 66).unwrap_or_default());
            }
            None => {
                scene.sprites.extend(city_hud::sprites(&trace.world, compiled));
                scene.sprites.extend(fonts.sprites(&trace.texts, compiled));
            }
        }
        // (Zoomed out the world sprites stay at full size: the game view
        // shows them halved.)
    }
    ifc23::scene_map::overlay_screen_sprites(&mut scene);
    if let Some((rect, _)) = city {
        ifc23::scene_map::city_view(&mut scene, (trace.view.camera_x, trace.view.camera_y), rect);
    }
    scene
}

fn load_trace(path: &str) -> Result<Trace, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut trace = Trace::parse(&text).map_err(|e| e.to_string())?;
    // CIV3_UNIT_FRAME_SHIFT=n: units n animation frames later (screenshots
    // are taken a moment after the trace while units keep animating).
    if let Some(n) = std::env::var("CIV3_UNIT_FRAME_SHIFT").ok().and_then(|v| v.parse::<i32>().ok()) {
        for u in &mut trace.world.units {
            u.frame += n;
        }
    }
    if let Some(n) = std::env::var("CIV3_CURSOR_FRAME_SHIFT").ok().and_then(|v| v.parse::<i32>().ok()) {
        if let Some(sel) = &mut trace.world.selection {
            sel.1 += n;
        }
    }
    Ok(trace)
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    match args.first().map(String::as_str) {
        Some("drawdiff") => {
            let trace = load_trace(args.get(1).ok_or("drawdiff TRACE")?)?;
            drawdiff(&trace, viewport(args)?)
        }
        Some("render") => {
            let trace = load_trace(args.get(1).ok_or("render TRACE OUT.png")?)?;
            let out = args.get(2).ok_or("render TRACE OUT.png")?;
            let (mut compiled, mut art) = compile(Some(&trace))?;
            let size = viewport(args)?;
            let scene = trace_scene(&trace, &compiled, art.as_mut(), size);
            let config = tile_viewer::ViewerConfig { overhang: ifc23::scene_map::OVERHANG, ..Default::default() };
            let img = if args.iter().any(|arg| arg == "--reference") {
                for patch in &scene.patches {
                    let Some(id) = compiled.sprite(&patch.sprite) else { continue };
                    if patch.rgba.len() != (patch.width * patch.height * 4) as usize { continue; }
                    let image = tile_render::RgbaImage::from_fn(patch.width, patch.height, |x, y| {
                        let i = ((y * patch.width + x) * 4) as usize;
                        [patch.rgba[i], patch.rgba[i + 1], patch.rgba[i + 2], patch.rgba[i + 3]]
                    });
                    compiled.atlas.patch(id, &image);
                }
                let world = tile_viewer::scene_world(&compiled, &scene);
                let cameras = tile_viewer::scene_cameras(&scene, size, config.overhang);
                reference::render_views(&world, &compiled.atlas, &cameras, size, scene.tick, config.clear)
            } else {
                let gpu = tile_render::Headless::new().map_err(|e| e.to_string())?;
                tile_viewer::render_offscreen(&gpu, &compiled, &scene, size, &config)
            };
            image::save_buffer(out, img.as_bytes(), img.width, img.height, image::ExtendedColorType::Rgba8)
                .map_err(|e| e.to_string())?;
            println!("{out}");
            Ok(ExitCode::SUCCESS)
        }
        Some("bench") => {
            let trace = load_trace(args.get(1).ok_or("bench TRACE [--frames N] [--rebuild]")?)?;
            let frames = args.iter().position(|a| a == "--frames").and_then(|i| args.get(i + 1)).and_then(|n| n.parse().ok()).unwrap_or(300);
            let rebuild = args.iter().any(|a| a == "--rebuild");
            let (compiled, mut art) = compile(Some(&trace))?;
            let size = viewport(args)?;
            let scene = trace_scene(&trace, &compiled, art.as_mut(), (0, 0));
            bench(&compiled, &scene, size, frames, rebuild)
        }
        Some("stats") => {
            let trace = load_trace(args.get(1).ok_or("stats TRACE")?)?;
            let (compiled, _) = compile(Some(&trace))?;
            let a = &compiled.atlas;
            let (rgba, index) = a.page_bytes();
            let indexed = a.sprites.iter().filter(|s| s.palette != 0).count();
            let rows = a.palettes.len() / tile_render::PALETTE_SIZE as usize;
            println!(
                "atlas: {} sprites ({indexed} indexed), {} colour pages ({} MiB), {} index pages ({} MiB), {} palette rows ({} own, {} named; {} KiB)",
                a.sprites.len(),
                a.pages.len(),
                rgba >> 20,
                a.index_pages.len(),
                index >> 20,
                rows,
                a.named_palettes - 1,
                rows as u32 - a.named_palettes,
                a.palettes.len() * 4 >> 10
            );
            Ok(ExitCode::SUCCESS)
        }
        Some("pixeldiff") => {
            let usage = "pixeldiff TRACE GAME.png [DIFF.png]";
            let trace = load_trace(args.get(1).ok_or(usage)?)?;
            let game = image::open(args.get(2).ok_or(usage)?).map_err(|e| e.to_string())?.to_rgba8();
            pixeldiff(&trace, &game, args.get(3).map(String::as_str))
        }
        Some("minimap") => {
            // Our minimap against the game's (at 1920x1080: 230x108 at
            // (31, 961), docs §17.2).
            let usage = "minimap TRACE GAME.png [X Y W H]";
            let trace = load_trace(args.get(1).ok_or(usage)?)?;
            let game = image::open(args.get(2).ok_or(usage)?).map_err(|e| e.to_string())?.to_rgba8();
            let n = |i: usize, d: u32| args.get(i).and_then(|v| v.parse().ok()).unwrap_or(d);
            let (x0, y0, w, h) = (n(3, 31), n(4, 961), n(5, 230), n(6, 108));
            let (_, art) = compile(Some(&trace))?;
            let art = art.ok_or("no unit art")?.0;
            let colors = |civ: i32| {
                let table = trace.world.color_tables.get(civ as usize).copied().unwrap_or(0);
                ifc23::minimap::from_rgb(art.table_color(table, 66).unwrap_or_default())
            };
            let ours = ifc23::minimap::image(&trace.world, (w, h), colors);
            let (mut same, mut white) = (0, 0);
            for y in 0..h {
                for x in 0..w {
                    let i = ((y * w + x) * 4) as usize;
                    let g = game.get_pixel(x0 + x, y0 + y).0;
                    same += (ours[i..i + 3] == g[..3]) as u32;
                    // The viewport rectangle (interface drawing, not ours here).
                    white += (ours[i..i + 3] != g[..3] && g[..3] == [255, 255, 255]) as u32;
                }
            }
            let pct = 100.0 * same as f64 / (w * h) as f64;
            println!("minimap: {same} of {} pixels identical ({pct:.2}%); {white} of the others white in the game", w * h);
            Ok(ExitCode::SUCCESS)
        }
        Some("preview") => {
            let usage = "preview SCENARIO.biq OUT.png [--viewport WxH]";
            preview(args.get(1).ok_or(usage)?, args.get(2).ok_or(usage)?, viewport(args)?)
        }
        Some("labels") => {
            let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");
            let paths: Vec<&String> = args[1..].iter().filter(|a| !a.starts_with('-')).collect();
            if paths.is_empty() {
                return Err("labels TRACE... [--verbose]".into());
            }
            labels(&paths, verbose)
        }
        _ => Err("usage: civ3-tool drawdiff TRACE | render TRACE OUT.png | preview SCENARIO.biq OUT.png | pixeldiff TRACE GAME.png [DIFF.png] | minimap TRACE GAME.png | labels TRACE... [-v]  [--viewport WxH]".into()),
    }
}

/// Per layer: the cells camera `camera` draws something of, the pixels
/// their sprites' rectangles cover (fragments shaded) and of those the
/// non-blank ones.
fn layer_stats(compiled: &Compiled, world: &tile_render::TileWorld, camera: &tile_render::Camera) {
    let a = &compiled.atlas;
    let mut cache: std::collections::HashMap<u32, (u64, u64)> = Default::default();
    let mut texels = |id: u32| -> (u64, u64) {
        *cache.entry(id).or_insert_with(|| {
            let Some(d) = a.sprites.get(id as usize) else { return (0, 0) };
            let mut solid = 0u64;
            for y in 0..d.height {
                for x in 0..d.width {
                    let (px, py) = (d.atlas_x + x, d.atlas_y + y);
                    let alpha = if d.palette == 0 {
                        a.pages.get(d.page as usize).map_or(255, |p| p.pixels[(py * p.width + px) as usize][3])
                    } else {
                        let idx = a.index_pages.get(d.page as usize).map_or(0, |p| p.pixels[(py * p.width + px) as usize]) as u32;
                        (a.palettes.get((d.palette * 256 + idx) as usize).copied().unwrap_or(0) >> 24) as u8
                    };
                    solid += (alpha != 0) as u64;
                }
            }
            ((d.width * d.height) as u64, solid)
        })
    };
    let g = world.grid;
    let (w, h) = (camera.viewport.0 as i32, camera.viewport.1 as i32);
    let mut total = (0u64, 0u64);
    for (li, spec) in world.layers().iter().enumerate() {
        let (mut cells, mut area, mut solid) = (0u64, 0u64, 0u64);
        for row in 0..g.rows {
            for col in 0..g.cols {
                let (ox, oy) = g.origin(col as i32, row as i32);
                let (sx, sy) = (ox - camera.x, oy - camera.y);
                if sx < -256 || sy < -256 || sx > w + 256 || sy > h + 256 {
                    continue;
                }
                let v = world.get(tile_render::LayerId(li), col, row);
                let v = v & ((1 << spec.palette_shift().max(if spec.palette_bits == 0 { 32 } else { 0 }).min(31)) - 1).max(if spec.palette_bits == 0 { u32::MAX } else { 0 });
                let Some(&set) = spec.lut.get(v as usize) else { continue };
                let Some(id) = world.sets.frame(set, 0) else { continue };
                let Some(d) = a.sprites.get(id.0 as usize) else { continue };
                // The part on the screen.
                let (x0, y0) = (sx + spec.offset.0 + d.anchor_x, sy + spec.offset.1 + d.anchor_y);
                let (x1, y1) = (x0 + d.width as i32, y0 + d.height as i32);
                let on = ((x1.min(w) - x0.max(0)).max(0) as u64) * ((y1.min(h) - y0.max(0)).max(0) as u64);
                if on == 0 {
                    continue;
                }
                let (all, nonblank) = texels(id.0);
                cells += 1;
                area += on;
                solid += if all > 0 { nonblank * on / all } else { 0 };
            }
        }
        if cells > 0 {
            println!("layer {li:>2} {:<28} pass {} cells {cells:>5} pixels {area:>9} ({:.2} screens) non-blank {:.0}%", spec.name, spec.pass, area as f64 / (w * h) as f64, 100.0 * solid as f64 / area.max(1) as f64);
        }
        total.0 += area;
        total.1 += solid;
    }
    println!("all layers: {:.2} screens of fragments, {:.2} of them non-blank", total.0 as f64 / (w * h) as f64, total.1 as f64 / (w * h) as f64);
}

/// `SCALE_INVERT` as free sprites carry it.
fn tile_render_shared_scale_invert() -> u32 {
    tile_render::Blend::Invert.scale_den()
}

/// Draws `scene` `frames` times as the viewer arranges it and prints where
/// the time goes (see the `bench` command).
fn bench(compiled: &Compiled, scene: &scene::Scene, size: (u32, u32), frames: u32, rebuild: bool) -> Result<ExitCode, String> {
    use std::time::Instant;
    let gpu = tile_render::Headless::new().map_err(|e| e.to_string())?;
    println!("adapter: {:?}", gpu.adapter.get_info());
    let reuse_setting = std::env::var("TILE_RENDER_REUSE").ok();
    let reuse_enabled = reuse_setting.as_deref() != Some("0");
    println!(
        "bench config: {}x{} pixels, {frames} frames, fixed scene tick {}, rebuild {rebuild}, TILE_RENDER_REUSE={} ({})",
        size.0, size.1, scene.tick, reuse_setting.as_deref().unwrap_or("<unset>"),
        if reuse_enabled { "reuse enabled" } else { "reuse disabled" },
    );
    println!("atlas: {} sprites, {} + {} pages", compiled.atlas.sprites.len(), compiled.atlas.pages.len(), compiled.atlas.index_pages.len());
    let config = tile_viewer::ViewerConfig { overhang: ifc23::scene_map::OVERHANG, ..Default::default() };
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let mut renderer = tile_render::TileRenderer::new(&gpu.device, format);
    let timed = renderer.enable_timing(&gpu.device, &gpu.queue);
    println!("GPU timing: {}", if timed { "on" } else { "off (TILE_RENDER_GPU_TIMING=1, timestamp queries)" });
    renderer.set_atlas(&gpu.device, &gpu.queue, &compiled.atlas);
    let frame = tile_viewer::arranged_frame(compiled, scene, size, &config);
    let world = tile_viewer::scene_world_for(compiled, scene, &frame.view_cameras);
    renderer.set_world(&gpu.device, &world);
    tile_viewer::apply_patches(&mut renderer, &gpu.queue, compiled, &scene.patches);
    println!(
        "scene: {}x{} cells, {} layers, {} world sprites, overlays {:?}; {} cameras, {} free sprites",
        scene.shape.cols,
        scene.shape.rows,
        scene.layers.len(),
        scene.sprites.len(),
        scene.views.iter().map(|v| v.overlay.len()).collect::<Vec<_>>(),
        frame.cameras.len(),
        world.free.len()
    );
    // What the free sprites are: per view, by kind, the pixels they cover
    // (first frames, clipped), and the largest.
    for (vi, v) in scene.views.iter().enumerate() {
        let mut kinds: std::collections::BTreeMap<String, (usize, i64)> = Default::default();
        let mut largest: Vec<(i64, String)> = Vec::new();
        for s in &v.overlay {
            let kind = match (s.glyph.is_some(), s.scale_den) {
                (true, _) => "outline glyph".to_owned(),
                (false, 0) => "alpha".to_owned(),
                (false, d) if d == tile_render_shared_scale_invert() => "invert".to_owned(),
                (false, d) => format!("scale/subpixel {d}"),
            };
            let sprite = compiled.sets.frame(tile_render::SpriteSetId::from_raw(s.set), 0).and_then(|id| compiled.atlas.sprites.get(id.0 as usize).map(|d| (id, *d)));
            let area = sprite.map_or(0, |(_, d)| {
                let (mut x0, mut y0) = (s.x + d.anchor_x, s.y + d.anchor_y);
                let (mut x1, mut y1) = (x0 + d.width as i32, y0 + d.height as i32);
                if let Some([cx0, cy0, cx1, cy1]) = s.clip {
                    (x0, y0, x1, y1) = (x0.max(cx0), y0.max(cy0), x1.min(cx1), y1.min(cy1));
                }
                ((x1 - x0).max(0) as i64) * ((y1 - y0).max(0) as i64)
            });
            let e = kinds.entry(format!("{kind}{}", if sprite.is_some_and(|(_, d)| d.palette != 0) { " indexed" } else { "" })).or_default();
            e.0 += 1;
            e.1 += area;
            if let Some((id, _)) = sprite {
                largest.push((area, compiled.sprite_names.get(id.0 as usize).cloned().unwrap_or_default()));
            }
        }
        largest.sort_by(|a, b| b.0.cmp(&a.0));
        println!("view {vi} (id {}): {:?}", v.id, kinds);
        println!("  largest: {:?}", &largest[..largest.len().min(12)]);
    }
    if std::env::var("BENCH_LAYERS").is_ok() {
        layer_stats(compiled, &world, &frame.cameras[0]);
    }
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bench target"),
        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut profile = tile_viewer::profile::Profile::new("bench", std::time::Duration::ZERO);
    let (mut whole_frames, mut reused_frames) = (0u32, 0u32);
    for i in 0..frames {
        profile.round_start();
        if rebuild {
            let w = profile.time("world build", || tile_viewer::scene_world_for(compiled, scene, &frame.view_cameras));
            profile.time("upload world", || renderer.update_world(&gpu.device, &gpu.queue, &w));
        }
        let t = Instant::now();
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        renderer.render_composed(&gpu.device, &gpu.queue, &mut encoder, &target, &frame.cameras, &frame.copies, scene.tick, config.clear);
        if renderer.reused() {
            reused_frames += 1;
            profile.event("frames drawn on the last picture", 1);
        } else {
            whole_frames += 1;
            profile.event("frames drawn whole", 1);
        }
        if let Some(timer) = renderer.timer() {
            timer.finish(&mut encoder);
        }
        profile.add("encode", t.elapsed());
        let t = Instant::now();
        gpu.queue.submit([encoder.finish()]);
        profile.add("submit", t.elapsed());
        let t = Instant::now();
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        profile.add("wait for GPU", t.elapsed());
        if let Some(timer) = renderer.timer() {
            timer.submitted();
            let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
            // (The first frames compile pipelines.)
            for f in timer.collect(&gpu.device).into_iter().filter(|_| i >= 5) {
                profile.add_ms("gpu frame span", f.span);
                profile.add_ms("gpu passes sum", f.passes.iter().map(|(_, ms)| ms).sum());
                profile.count("gpu passes", f.passes.len());
                for (label, ms) in f.passes {
                    profile.add_ms(&format!("gpu {label}"), ms);
                }
            }
        }
        profile.round_end();
    }
    println!("bench frames: {whole_frames} whole redraws, {reused_frames} reused pictures");
    println!("{}", profile.report().unwrap_or_default());
    Ok(ExitCode::SUCCESS)
}

/// Renders the world of scenario `file` as the Load Scenario screen previews
/// it (`ifc23::preview`), beside the screen's place (a 1024×768 dialog in
/// the middle of the window, left empty), as the viewer arranges it.
fn preview(file: &str, out: &str, size: (u32, u32)) -> Result<ExitCode, String> {
    let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    let desc_path = std::env::var("CIV3_DESC").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/civ3/civ3.toml").into());
    let mut text = std::fs::read_to_string(&desc_path).map_err(|e| format!("{desc_path}: {e}"))?;
    let p = ifc23::preview::load(install.as_ref(), file).map_err(|e| e.to_string())?;
    let files = || assets::game_files(install.as_ref(), p.scenario_folder.as_deref(), &p.search_folders);
    let mut art = units::UnitArt::new(files());
    text.push_str(&units::Needed::still(&p.world, &mut art).description());
    let mut env = ifc23::city_labels::FileEnv::new(Box::new(files()));
    let labels = ifc23::preview::label_items(&p.world, &mut env);
    let items: Vec<ui_screen::Item> = labels.iter().flat_map(|(_, items)| items.iter().cloned()).collect();
    let (mut screen, mut fonts) = (menu::screen::ScreenArt::default(), text::Fonts::default());
    ui_screen::add_art(&items, &mut screen, &mut fonts, &files());
    text.push_str(&fonts.description());
    text.push_str(&ui_screen::description(&screen));
    let compiled = assets::compile(&text, files()).map_err(|e| e.to_string())?;
    let dialog = [(size.0 as i32 - 1024) / 2, (size.1 as i32 - 768) / 2, 1024, 768];
    let n = ifc23::preview::downscale(p.area, (size.0 as i32 - dialog[2], size.1 as i32));
    let mut scene = ifc23::preview::scene(&p.world, n, &Mapper::new(&compiled), &mut art, &compiled, &labels, &fonts);
    scene.interface.beside = Some(scene::Beside { screen: dialog, edge: scene::Edge::Left, world: Some(p.area) });
    ifc23::scene_map::overlay_screen_sprites(&mut scene);
    let gpu = tile_render::Headless::new().map_err(|e| e.to_string())?;
    let config = tile_viewer::ViewerConfig { overhang: ifc23::scene_map::OVERHANG, ..Default::default() };
    let img = tile_viewer::render_arranged(&gpu, &compiled, &scene, size, &config);
    image::save_buffer(out, img.as_bytes(), img.width, img.height, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
    let w = &p.world;
    println!("{out}: {}x{} map shown 1/{n}, {} cities, {} units", w.width, w.height, w.cities.len(), w.units.len());
    Ok(ExitCode::SUCCESS)
}

/// Compares the city labels our rules draw with the game's own, per dump (see
/// `ifc23::label_check`).
fn labels(paths: &[&String], verbose: bool) -> Result<ExitCode, String> {
    let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    let (mut cities, mut exact, mut expected, mut matched, mut unexplained, mut dumps, mut unchecked) = (0, 0, 0, 0, 0, 0, 0);
    let mut in_places = 0;
    let (mut own_cities, mut own_exact) = (0, 0);
    for path in paths {
        let mut trace = load_trace(path)?;
        let files = assets::game_files(install.as_ref(), trace.scenario_folder.as_deref(), &trace.search_roots);
        let report = ifc23::label_check::check(&mut trace, &files, true);
        let name = std::path::Path::new(path.as_str()).file_name().map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned());
        println!(
            "{name}: {}/{} labels exact; operations {}/{} (game only: {}){}{}",
            report.exact,
            report.cities,
            report.matched,
            report.expected,
            report.unexplained,
            if report.unchecked > 0 { format!("; {} cut by the image, not checked", report.unchecked) } else { String::new() },
            if report.inferred { "; texts from the mirror" } else { "" }
        );
        if verbose || report.exact != report.cities {
            for note in &report.notes {
                println!("    {note}");
            }
        }
        if report.cities > 0 {
            dumps += 1;
        }
        cities += report.cities;
        exact += report.exact;
        expected += report.expected;
        matched += report.matched;
        unexplained += report.unexplained;
        unchecked += report.unchecked;
        in_places += report.in_label_places;
        if !report.inferred {
            own_cities += report.cities;
            own_exact += report.exact;
        }
    }
    println!("{dumps} dumps with labels: {exact}/{cities} labels exact, {matched}/{expected} operations matched, {unexplained} game operations not expected ({in_places} of them in the places of labels), {unchecked} labels cut by the image not checked");
    if own_cities > 0 {
        println!("of them from dumps with the cities' texts: {own_exact}/{own_cities}");
    }
    // What the game drew that was not expected (cut labels, other drawings of the canvas) is
    // only counted; the exit status is about the labels we made.
    Ok(if exact == cities { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// Arrays the game draws through blits the tracer does not hook yet.
const NOT_TRACED: &[&str] = &["Fog_Of_War_Images", "Territory_Shade"];

/// The game's array name of one of our sprites ("Territory_Images@3/2" is a
/// recoloured "Territory_Images/2").
fn game_name(name: &str) -> String {
    match name.split_once('@') {
        Some((array, rest)) => format!("{array}{}", &rest[rest.find('/').unwrap_or(rest.len())..]),
        None => name.to_owned(),
    }
}

/// A draw, named, and whether it touches the viewport.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Named {
    name: String,
    x: i32,
    y: i32,
}

fn drawdiff(trace: &Trace, viewport: (u32, u32)) -> Result<ExitCode, String> {
    let (compiled, _) = compile(Some(trace))?;
    let scene = Mapper::new(&compiled).scene(&trace.world, &trace.view, viewport, 0);
    let world = tile_viewer::scene_world(&compiled, &scene);
    let camera = tile_viewer::scene_cameras(&scene, viewport, (0, 128, 0, 128)).remove(0);

    let index_of: std::collections::HashMap<&str, usize> =
        compiled.sprite_names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    // Game names ("Territory_Images/2") to one of our sprites of that size.
    let index_of: std::collections::HashMap<String, usize> =
        index_of.into_iter().map(|(n, i)| (game_name(n), i)).collect();
    // (At the view's scale: zoomed out, sprites cover half their cell.)
    let size_of = |name: &str| index_of.get(name).map(|&i| {
        let s = compiled.atlas.sprites[i];
        let n = if s.scaling == tile_render::KEEP_SIZE { 1 } else { camera.downscale.max(1) };
        ((s.width / n) as i32, (s.height / n) as i32)
    });
    let visible = |name: &str, x: i32, y: i32| {
        size_of(name).is_some_and(|(w, h)| x + w > 0 && y + h > 0 && x < viewport.0 as i32 && y < viewport.1 as i32)
    };
    // Arrays our description covers and the tracer sees: compare only those.
    let groups: HashSet<&str> = compiled
        .sprite_names
        .iter()
        .filter_map(|n| n.split_once(['/', '@']).map(|(g, _)| g))
        .filter(|g| !NOT_TRACED.contains(g))
        .collect();

    let ours: Vec<Named> = reference::draw_list(&world, &compiled.atlas, &camera, 0)
        .into_iter()
        .map(|d| Named { name: compiled.sprite_names[d.sprite.0 as usize].clone(), x: d.x, y: d.y })
        .filter(|d| groups.contains(d.name.split(['/', '@']).next().unwrap_or("")))
        .map(|d| Named { name: game_name(&d.name), ..d })
        .filter(|d| visible(&d.name, d.x, d.y))
        .collect();
    let mut unresolved = 0;
    let theirs: Vec<Named> = trace
        .draws
        .iter()
        .filter_map(|d| match &d.sprite {
            Some((array, index)) if groups.contains(array.as_str()) => Some(Named { name: format!("{array}/{index}"), x: d.x, y: d.y }),
            Some(_) => None,
            None => {
                unresolved += 1;
                None
            }
        })
        .filter(|d| visible(&d.name, d.x, d.y))
        .collect();

    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort();
    println!("arrays compared: {}", groups.join(", "));
    println!("game draws: {}  ours: {}  (unresolved game draws: {unresolved})", theirs.len(), ours.len());
    let common = ours.iter().zip(&theirs).take_while(|(a, b)| a == b).count();
    if common == ours.len() && common == theirs.len() {
        println!("IDENTICAL: same sprites, positions and order");
        return Ok(ExitCode::SUCCESS);
    }
    println!("first difference at draw {common}:");
    for k in common..(common + 5) {
        println!("  game {:?}\n  ours {:?}", theirs.get(k), ours.get(k));
    }
    let ours_set: HashSet<_> = ours.iter().map(|d| (&d.name, d.x, d.y)).collect();
    let theirs_set: HashSet<_> = theirs.iter().map(|d| (&d.name, d.x, d.y)).collect();
    println!("only in game: {}  only ours: {}", theirs_set.difference(&ours_set).count(), ours_set.difference(&theirs_set).count());
    let mut by_array: std::collections::BTreeMap<(&str, &str), usize> = Default::default();
    for d in theirs_set.difference(&ours_set) {
        *by_array.entry(("game-only", d.0.split('/').next().unwrap())).or_default() += 1;
    }
    for d in ours_set.difference(&theirs_set) {
        *by_array.entry(("ours-only", d.0.split('/').next().unwrap())).or_default() += 1;
    }
    for ((side, array), n) in &by_array {
        println!("  {side:9} {array}: {n}");
    }
    for d in theirs_set.difference(&ours_set).take(5) {
        println!("  game-only {d:?}");
    }
    let mut extra: Vec<_> = ours_set.difference(&theirs_set).collect();
    extra.sort_by_key(|d| !d.0.starts_with("Resources"));
    extra.reverse();
    for d in extra.into_iter().take(5) {
        println!("  ours-only {d:?}");
    }
    Ok(ExitCode::FAILURE)
}

fn pixeldiff(trace: &Trace, game: &image::RgbaImage, out: Option<&str>) -> Result<ExitCode, String> {
    let viewport = (game.width(), game.height());
    let (compiled, mut art) = compile(Some(trace))?;
    let scene = trace_scene(trace, &compiled, art.as_mut(), viewport);
    let gpu = tile_render::Headless::new().map_err(|e| e.to_string())?;
    let config = tile_viewer::ViewerConfig { overhang: ifc23::scene_map::OVERHANG, ..Default::default() };
    let ours = tile_viewer::render_offscreen(&gpu, &compiled, &scene, viewport, &config);
    // The map alone: the pixels no unit, label or interface sprite of ours
    // covers are the terrain's (layers of the world: terrain, objects,
    // cities, districts, fog of war).
    let terrain_of = {
        let mut bare = scene.clone();
        bare.sprites.clear();
        // (The units' layers too.)
        for (layer, cells) in compiled.layers.iter().zip(&mut bare.layers) {
            if units::SLOTS.iter().any(|s| layer.name.starts_with(&format!("{s}_"))) {
                cells.values.iter_mut().for_each(|v| *v = 0);
            }
        }
        bare.patches.clear();
        bare.views.iter_mut().for_each(|v| v.overlay.clear());
        tile_viewer::render_offscreen(&gpu, &compiled, &bare, viewport, &config)
    };

    // Pixels the game drew with sprites we do not model.
    let groups: HashSet<&str> = compiled.sprite_names.iter().filter_map(|n| n.split_once(['/', '@']).map(|(g, _)| g)).collect();
    let (w, h) = viewport;
    let mut ignored = vec![false; (w * h) as usize];
    let mut unsized_draws = 0;
    for d in &trace.draws {
        let modelled = d.sprite.as_ref().is_some_and(|(array, _)| groups.contains(array.as_str()));
        // Colour-table blits also draw off the map (other canvases).
        let on_map = d.kind != "colored" || d.params.first() == Some(&trace.map_renderer);
        if modelled || !on_map {
            continue;
        }
        let Some((sw, sh)) = d.size else {
            unsized_draws += 1;
            continue;
        };
        for y in d.y.max(0)..(d.y + sh as i32).min(h as i32) {
            for x in d.x.max(0)..(d.x + sw as i32).min(w as i32) {
                ignored[(y as u32 * w + x as u32) as usize] = true;
            }
        }
    }
    let (mut same, mut compared) = (0u64, 0u64);
    let (mut terrain_same, mut terrain_compared) = (0u64, 0u64);
    let mut diff = game.clone();
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let g = game.get_pixel(x, y).0;
            if ignored[i] {
                diff.put_pixel(x, y, image::Rgba([g[0] / 4, g[1] / 4, g[2] / 4, 255]));
                continue;
            }
            compared += 1;
            let o = ours.get(x, y);
            let terrain = o == terrain_of.get(x, y);
            terrain_compared += terrain as u64;
            if g[..3] == o[..3] {
                same += 1;
                terrain_same += terrain as u64;
            } else {
                // Magenta where the terrain differs, violet under our unit,
                // label and interface sprites.
                diff.put_pixel(x, y, image::Rgba(if terrain { [255, 0, 255, 255] } else { [100, 0, 255, 255] }));
            }
        }
    }
    println!(
        "compared {compared} of {} pixels ({} ignored{}): {:.3}% identical, {} differ",
        w * h,
        w as u64 * h as u64 - compared,
        if unsized_draws > 0 { format!(", {unsized_draws} unmodelled draws without size not masked") } else { String::new() },
        100.0 * same as f64 / compared.max(1) as f64,
        compared - same
    );
    println!(
        "terrain (the {terrain_compared} of them no unit, label or interface sprite of ours covers): {:.3}% identical, {} differ",
        100.0 * terrain_same as f64 / terrain_compared.max(1) as f64,
        terrain_compared - terrain_same
    );
    if let Some(path) = out {
        diff.save(path).map_err(|e| e.to_string())?;
        println!("{path}");
    }
    Ok(if same == compared { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// The surfaces traces of the PCGames build name (the minimap).
const GENERATED: &[(u32, &str)] = civ3_sys::ui_hooks::generated_surfaces_pcg();
