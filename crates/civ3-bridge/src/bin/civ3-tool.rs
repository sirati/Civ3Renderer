//! Offline Civ III renderer tools, working from tracer dumps.
//!
//!   civ3-tool drawdiff TRACE [--viewport WxH]
//!       compares the sprites we draw with the ones the game drew (same
//!       names, positions and order), for the sprite arrays our render
//!       description covers
//!   civ3-tool render TRACE OUT.png [--viewport WxH]
//!       renders the trace's world on the GPU
//!   civ3-tool pixeldiff TRACE GAME.png [DIFF.png]
//!       renders the trace's world and compares it with a screenshot of the
//!       game taken at the same moment, ignoring the screen rectangles of
//!       game draws we do not model (units, cities, borders); DIFF.png shows
//!       differing pixels in magenta and ignored areas darkened
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
        for path in t.ui.as_ref().map(|f| ui_screen::unit_files(f, &[])).unwrap_or_default() {
            needed.add_file(&mut a, &path, unit_colors);
        }
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
        text.push_str(&fonts.description());
        art = Some((a, fonts));
    }
    let scale_down = if trace.is_some_and(|t| t.view.zoomed_out) { 2 } else { 1 };
    Ok((assets::compile_scaled(&text, files(), scale_down).map_err(|e| e.to_string())?, art))
}

/// The scene of a trace, units included.
fn trace_scene(trace: &Trace, compiled: &Compiled, art: Option<&mut (units::UnitArt, text::Fonts)>, viewport: (u32, u32)) -> scene::Scene {
    let mut scene = Mapper::new(compiled).scene(&trace.world, &trace.view, viewport, 0);
    if let Some((art, fonts)) = art {
        if trace.ui.is_none() {
            scene.sprites = city_site::sprites(&trace.world, compiled);
        }
        scene.sprites.extend(units::sprites(&trace.world, art, compiled));
        match &trace.ui {
            // The mirrored interface has the city icons and all text.
            Some(frame) => {
                let roots: Vec<std::path::PathBuf> = Vec::new();
                let unit_colors = trace.world.color_tables.get(trace.world.player.max(0) as usize).copied().unwrap_or(0);
                let items = ui_screen::items(frame, &roots, GENERATED, unit_colors);
                scene.sprites.extend(ui_screen::sprites(&items, fonts, compiled, ui_screen::under_pass(compiled)));
                scene.patches = ui_screen::minimap_patches(&items, &trace.world, compiled, |t| art.table_color(t, 66).unwrap_or_default());
            }
            None => {
                scene.sprites.extend(city_hud::sprites(&trace.world, compiled));
                scene.sprites.extend(fonts.sprites(&trace.texts, compiled));
            }
        }
        if trace.view.zoomed_out {
            ifc23::scene_map::zoom_out(&mut scene.sprites);
        }
    }
    ifc23::scene_map::overlay_screen_sprites(&mut scene);
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
            let (compiled, mut art) = compile(Some(&trace))?;
            let size = viewport(args)?;
            let scene = trace_scene(&trace, &compiled, art.as_mut(), size);
            let gpu = tile_render::Headless::new().map_err(|e| e.to_string())?;
            let config = tile_viewer::ViewerConfig { overhang: (0, 64, 0, 64), ..Default::default() };
            let img = tile_viewer::render_offscreen(&gpu, &compiled, &scene, size, &config);
            image::save_buffer(out, img.as_bytes(), img.width, img.height, image::ExtendedColorType::Rgba8)
                .map_err(|e| e.to_string())?;
            println!("{out}");
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
        _ => Err("usage: civ3-tool drawdiff TRACE | render TRACE OUT.png | pixeldiff TRACE GAME.png [DIFF.png] | minimap TRACE GAME.png  [--viewport WxH]".into()),
    }
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
    let size_of = |name: &str| index_of.get(name).map(|&i| {
        let s = compiled.atlas.sprites[i];
        (s.width as i32, s.height as i32)
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
    let config = tile_viewer::ViewerConfig { overhang: (0, 128, 0, 128), ..Default::default() };
    let ours = tile_viewer::render_offscreen(&gpu, &compiled, &scene, viewport, &config);

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
            if g[..3] == o[..3] {
                same += 1;
            } else {
                diff.put_pixel(x, y, image::Rgba([255, 0, 255, 255]));
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
    if let Some(path) = out {
        diff.save(path).map_err(|e| e.to_string())?;
        println!("{path}");
    }
    Ok(if same == compared { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// The surfaces traces of the PCGames build name (the minimap).
const GENERATED: &[(u32, &str)] = civ3_sys::ui_hooks::generated_surfaces_pcg();
