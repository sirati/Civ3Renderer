//! Offline Civ III renderer tools, working from tracer dumps.
//!
//!   civ3-tool drawdiff TRACE [--viewport WxH]
//!       compares the sprites we draw with the ones the game drew (same
//!       names, positions and order), for the sprite arrays our render
//!       description covers
//!   civ3-tool render TRACE OUT.png [--viewport WxH]
//!       renders the trace's world on the GPU
//!
//! Environment: CIV3_INSTALL (game folder), CIV3_DESC (render description,
//! default assets/civ3/civ3.toml).

use std::collections::HashSet;
use std::process::ExitCode;

use civ3_assets::GameFiles;
use civ3_assets::loader::Civ3Loader;
use ifc23::scene_map::Mapper;
use ifc23::world::Trace;
use render_desc::{Compiled, Description};
use tile_render::reference;

fn main() -> ExitCode {
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

fn compile() -> Result<Compiled, String> {
    let install = std::env::var("CIV3_INSTALL")
        .unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    let desc_path = std::env::var("CIV3_DESC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/civ3/civ3.toml").into());
    let text = std::fs::read_to_string(&desc_path).map_err(|e| format!("{desc_path}: {e}"))?;
    let desc = Description::parse(&text).map_err(|e| e.to_string())?;
    desc.compile(&Civ3Loader::new(GameFiles::complete(install)), 4096).map_err(|e| e.to_string())
}

fn load_trace(path: &str) -> Result<Trace, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    Trace::parse(&text).map_err(|e| e.to_string())
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
            let compiled = compile()?;
            let scene = Mapper::new(&compiled).scene(&trace.world, &trace.view, viewport(args)?, 0);
            let gpu = tile_render::Headless::new().map_err(|e| e.to_string())?;
            let config = tile_viewer::ViewerConfig { overhang: (0, 64, 0, 64), ..Default::default() };
            let img = tile_viewer::render_offscreen(&gpu, &compiled, &scene, &config);
            image::save_buffer(out, img.as_bytes(), img.width, img.height, image::ExtendedColorType::Rgba8)
                .map_err(|e| e.to_string())?;
            println!("{out}");
            Ok(ExitCode::SUCCESS)
        }
        _ => Err("usage: civ3-tool drawdiff TRACE | render TRACE OUT.png  [--viewport WxH]".into()),
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
    let compiled = compile()?;
    let scene = Mapper::new(&compiled).scene(&trace.world, &trace.view, viewport, 0);
    let world = tile_viewer::scene_world(&compiled, &scene);
    let camera = tile_viewer::scene_camera(&scene, (0, 128, 0, 128));

    let index_of: std::collections::HashMap<&str, usize> =
        compiled.sprite_names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let size_of = |name: &str| index_of.get(name).map(|&i| {
        let s = compiled.atlas.sprites[i];
        (s.width as i32, s.height as i32)
    });
    let visible = |name: &str, x: i32, y: i32| {
        size_of(name).is_some_and(|(w, h)| x + w > 0 && y + h > 0 && x < viewport.0 as i32 && y < viewport.1 as i32)
    };
    // Arrays our description covers: compare only those.
    let groups: HashSet<&str> = compiled.sprite_names.iter().filter_map(|n| n.split_once('/').map(|(g, _)| g)).collect();

    let ours: Vec<Named> = reference::draw_list(&world, &compiled.atlas, &camera, 0)
        .into_iter()
        .map(|d| Named { name: compiled.sprite_names[d.sprite.0 as usize].clone(), x: d.x, y: d.y })
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
    for d in theirs_set.difference(&ours_set).take(5) {
        println!("  game-only {d:?}");
    }
    for d in ours_set.difference(&theirs_set).take(5) {
        println!("  ours-only {d:?}");
    }
    Ok(ExitCode::FAILURE)
}
