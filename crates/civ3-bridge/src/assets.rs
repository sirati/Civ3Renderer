//! Where the game's art comes from, and compiling the render description
//! against it.

use std::path::{Path, PathBuf};

use civ3_assets::GameFiles;
use civ3_assets::loader::Civ3Loader;
use render_desc::{Compiled, DescError, Description};

/// The render description built into the bridge.
pub const BUILTIN_DESC: &str = include_str!("../../../assets/civ3/civ3.toml");

/// The art search roots of `install` ("Civilization III Complete") while
/// a scenario is loaded: its art folder and search folders (as the game
/// names them, relative to `Conquests` or `Conquests/Conquests`, with
/// backslashes) first, then Conquests, Play the World and the base game.
pub fn game_files(install: &Path, scenario_folder: Option<&str>, search_roots: &[String]) -> GameFiles {
    let mut files = GameFiles::complete(install);
    let base = GameFiles::with_roots(vec![install.to_path_buf()]);
    let scenario: Vec<PathBuf> = scenario_folder
        .into_iter()
        .chain(search_roots.iter().map(String::as_str))
        .filter_map(|folder| {
            let rel = folder.replace('\\', "/");
            let rel = rel.trim_matches('/');
            ["Conquests/Conquests", "Conquests", ""].iter().find_map(|b| base.resolve(&format!("{b}/{rel}")).ok())
        })
        .collect();
    for dir in scenario.into_iter().rev() {
        files.push_front(dir);
    }
    files
}

/// Compiles a description (TOML text) against the game's art, for any
/// device (pages of `render_desc::page_size`, at most
/// `render_desc::ANY_DEVICE_PAGE`).
pub fn compile(text: &str, files: GameFiles) -> Result<Compiled, DescError> {
    compile_as(text, files, Pixels::Made(None), render_desc::ANY_DEVICE_PAGE)
}

/// What a compile makes of the art (see [`compile_as`]).
pub enum Pixels<'a> {
    /// The atlas, reusing the sheets of the cache's last compile, if any
    /// (`render_desc::SheetCache`).
    Made(Option<&'a mut render_desc::SheetCache>),
    /// Only the tables (sprite names, sizes, sets, layers:
    /// `Compiled::without_pixels`), from the art files' headers: what a
    /// driver needs whose renderer makes its own atlas.
    TablesOnly,
}

/// [`compile`], with or without the pixels, for a renderer whose atlas
/// pages may be at most `max_page` wide and high (its device's limit): the
/// pages are as large as the largest sprite needs, else 2048² (small enough
/// for 32-bit processes and software renderers; see
/// `render_desc::page_size`). A renderer making its own atlas from the same
/// text must use the same limit.
pub fn compile_as(text: &str, files: GameFiles, pixels: Pixels, max_page: u32) -> Result<Compiled, DescError> {
    let desc = Description::parse(text)?;
    let loader = Civ3Loader::new(files);
    match pixels {
        Pixels::Made(Some(cache)) => desc.compile_cached(&loader, max_page, cache),
        Pixels::Made(None) => desc.compile(&loader, max_page),
        Pixels::TablesOnly => desc.compile_layout(&loader, max_page),
    }
}

/// [`compile_as`], to be grown (`render_desc::Growing`): with the pixels
/// (`pixels`), else the tables only; given up (an error, [`STOPPED`]) as
/// soon as `stop` says so (e.g. the art of a scenario no longer shown).
pub fn compile_growing(text: &str, files: GameFiles, pixels: bool, max_page: u32, stop: &dyn Fn() -> bool) -> Result<(Compiled, render_desc::Growing), DescError> {
    Description::parse(text)?.compile_growing(&render_desc::StopWhen::new(Civ3Loader::new(files), stop), max_page, pixels)
}

/// What a compile given up says (see [`compile_growing`]).
pub const STOPPED: &str = render_desc::CANCELLED;

/// The growth of `compiled` (grown by `growing`) by `addition`.
pub fn grow(growing: &mut render_desc::Growing, compiled: &Compiled, addition: &str, files: GameFiles) -> Result<render_desc::Growth, DescError> {
    growing.grow(compiled, addition, &Civ3Loader::new(files))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The built-in description compiled against the game's art
    /// (`CIV3_INSTALL`; skipped without an installation) and drawn by a view
    /// showing the world at half size (the zoomed-out view): terrain sprites
    /// are cropped to the box of their non-255 pixels (columns 1..126 of the
    /// cell), and sampled from there (§3.1).
    #[test]
    fn zoomed_out_terrain_is_sampled_from_its_box() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        if !Path::new(&install).is_dir() {
            return eprintln!("no Civ III install, skipped");
        }
        let compiled = compile(BUILTIN_DESC, GameFiles::complete(&install)).unwrap();
        let name = "Std_Terrain_Images/0";
        let s = compiled.atlas.sprites[compiled.sprite_names.iter().position(|n| n == name).unwrap()];
        // One copy of the art: full size, with the box to sample.
        assert_eq!((s.width, s.height), (128, 64));
        assert_eq!((s.box_x, s.box_y, s.box_w, s.box_h), (1, 0, 126, 64));
        // The tables alone agree with the full compile.
        let layout = compile_as(BUILTIN_DESC, GameFiles::complete(&install), Pixels::TablesOnly, render_desc::ANY_DEVICE_PAGE).unwrap();
        assert_eq!(layout.sprite_names, compiled.sprite_names);
        let sizes = |c: &Compiled| c.atlas.sprites.iter().map(|s| (s.width, s.height, s.anchor_x, s.anchor_y, s.downscaled)).collect::<Vec<_>>();
        assert_eq!(sizes(&layout), sizes(&compiled));
        // The sprite drawn at half size into a picture of half the cell.
        let mut world = compiled.world(1, 1, false, false);
        // (Composed as it is, so that clear stays clear.)
        world.color = tile_render::ColorModel::Direct;
        // (At the world position that puts its corner, its anchor halved, at 0.)
        let p = tile_render::placed(&s, 2);
        world.free.push(tile_render::PlacedSprite::new(compiled.set(name).unwrap().0, -2 * p.corner_x, -2 * p.corner_y));
        let camera = tile_render::Camera { downscale: 2, ..tile_render::Camera::new((64, 32)) };
        let small = tile_render::reference::render(&world, &compiled.atlas, &camera, 0, [0; 4]);
        // Cell column 1 (the left tip of the diamond) is sample 0; the 126
        // columns of the box make 63 samples, so the last column stays clear
        // (taking every second pixel from the cell's corner it would not).
        assert_eq!(small.get(0, 16)[3], 255);
        assert_eq!(small.get(63, 16)[3], 0);
        // The top row has two opaque pixels, cell columns 63 and 64: the odd
        // one is sampled (sample 31), the even one is not (sample 32).
        assert_eq!(small.get(31, 0)[3], 255);
        assert_eq!(small.get(32, 0)[3], 0);
        // (The terrain is indexed art: a palette index a texel, through its row.)
        assert_ne!(s.palette, 0);
        let index = compiled.atlas.index_pages[s.page as usize].get(s.atlas_x + 63, s.atlas_y) as u32;
        let color = compiled.atlas.palettes[(s.palette * tile_render::PALETTE_SIZE + index) as usize].to_le_bytes();
        assert_eq!(small.get(31, 0), color);
        // The mouse cursors (§16.11), their hot spots from the marks beside
        // their cells (the tables alone have them too, see above).
        let anchor = |name: &str| {
            let s = compiled.atlas.sprites[compiled.sets.frame(compiled.set(name).unwrap().0, 0).unwrap().0 as usize];
            (s.width, s.height, s.anchor_x, s.anchor_y)
        };
        assert_eq!(anchor("Viewer@cursors/0"), (32, 32, -1, -1));
        assert_eq!(anchor("Viewer@cursors/9"), (32, 32, -16, -16));
        assert_eq!(anchor("Viewer@cursors/14"), (32, 32, -15, -16));
        assert_eq!(compiled.sets_with_prefix("Viewer@capture/").map(|(n, _)| n).collect::<Vec<_>>(), ["Viewer@capture/9"]);
    }

    /// A compile no longer wanted ends at the next sheet it loads.
    #[test]
    fn a_compile_no_longer_wanted_stops() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        if !Path::new(&install).is_dir() {
            return eprintln!("no Civ III install, skipped");
        }
        let loads = std::cell::Cell::new(0);
        let stop = || {
            loads.set(loads.get() + 1);
            loads.get() > 3
        };
        let started = std::time::Instant::now();
        let Err(e) = compile_growing(BUILTIN_DESC, GameFiles::complete(&install), true, render_desc::ANY_DEVICE_PAGE, &stop) else { panic!("not stopped") };
        assert!(e.to_string().contains(STOPPED), "{e}");
        assert_eq!(loads.get(), 4);
        // (A whole compile takes far longer.)
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    /// The colours a sprite of `c` is drawn in, texel by texel.
    fn drawn(c: &Compiled, name: &str) -> Option<Vec<[u8; 4]>> {
        let s = c.atlas.sprites[c.sprite(name)?.0 as usize];
        let mut out = Vec::new();
        for y in s.atlas_y..s.atlas_y + s.height {
            for x in s.atlas_x..s.atlas_x + s.width {
                out.push(if s.palette == 0 {
                    c.atlas.pages[s.page as usize].get(x, y)
                } else {
                    let i = c.atlas.index_pages[s.page as usize].get(x, y) as u32;
                    c.atlas.palettes[(s.palette * tile_render::PALETTE_SIZE + i) as usize].to_le_bytes()
                });
            }
        }
        Some(out)
    }

    /// A compile that reuses the sheets of the last one draws what a fresh
    /// one does when the art folders changed in between (a scenario's own
    /// city art in place of the game's: same file names, other files).
    #[test]
    fn cached_compiles_follow_the_art_folders() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        let install = Path::new(&install);
        if !install.join("Conquests/Conquests/WWII in the Pacific").is_dir() {
            return eprintln!("no Civ III install with the WWII scenario, skipped");
        }
        let desc = Description::parse(BUILTIN_DESC).unwrap();
        let scenario = || game_files(install, Some("Conquests\\9 WWII in the Pacific\\"), &["WWII in the Pacific".to_owned()]);
        let mut cache = render_desc::SheetCache::new();
        desc.compile_cached(&Civ3Loader::new(game_files(install, None, &[])), 2048, &mut cache).unwrap();
        let cached = desc.compile_cached(&Civ3Loader::new(scenario()), 2048, &mut cache).unwrap();
        let fresh = desc.compile(&Civ3Loader::new(scenario()), 2048).unwrap();
        let stale: Vec<&String> = fresh.sprite_names.iter().filter(|n| drawn(&cached, n) != drawn(&fresh, n)).collect();
        assert!(stale.is_empty(), "{} sprites kept the art of the folders before: {:?}", stale.len(), &stale[..stale.len().min(10)]);
        // (The scenario has city art of its own.)
        let default = desc.compile(&Civ3Loader::new(game_files(install, None, &[])), 2048).unwrap();
        assert_ne!(drawn(&default, "City_Images/15"), drawn(&fresh, "City_Images/15"));
    }
}
