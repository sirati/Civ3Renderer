//! Effects (docs §10.14): the animations the game's animator draws besides
//! units — explosions where bombardment and combat hit or miss, smoke over
//! a volcano, a city's disorder or celebration, a bomber's bombs, SAM and
//! SDI shots. Each is an `FLC_Animation` of an INI file's art, at a world
//! pixel, read from the game's state (`civ3_sys::Civ3::effects`); they are
//! drawn here from the game's FLC files, as free sprites in every view.

use std::fmt::Write as _;

use scene::Scene;

use crate::units::{self, Needed, UnitArt};
use crate::world::{Civ3Effect, Civ3View, Civ3World};

/// `AnimationType` DEFAULT (never drawn: the animator skips it), ATTACK1,
/// DEATH (drawn after the units) and VICTORY.
const DEFAULT: i32 = 1;
const ATTACK1: i32 = 3;
const DEATH: i32 = 6;
const VICTORY: i32 = 9;

/// `AnimatedEffect` smoke over a volcano and an eruption.
const SMOLDER: i32 = 9;
const ERUPTION: i32 = 10;

/// How far (zoomed-in pixels) the smoke over a volcano is moved left, by
/// the volcano's neighbour mask (table 0x72F420, added to the camera by
/// 0x5DA4C0).
const VOLCANO_SHIFT: [i32; 16] = [0, 0, -11, 11, -15, 16, 6, -12, 6, 11, 0, 2, 16, 22, 11, 8];

/// The effects the game draws now, in its order (§10.14): loaded and shown,
/// not DEFAULT, and, unless it is one of the animator's extra animations,
/// on a tile the player sees now.
pub fn drawn(world: &Civ3World) -> impl Iterator<Item = &Civ3Effect> {
    world.effects.iter().filter(move |e| {
        e.shown
            && !e.ini.is_empty()
            && e.animation != DEFAULT
            && match e.source {
                civ3_sys::EffectSource::Extra => true,
                _ => world.tile(e.tile.0, e.tile.1).is_some_and(|t| world.visible_now(t)),
            }
    })
}

/// The art the game's own effects use (the `AnimatedEffect` INIs,
/// `Units_Image_Data_load_animated_effect` 0x406A10, and the SAM and SDI
/// shots) with the animation each plays, and every air unit's bombs (its
/// VICTORY, §10.14): added before they are first shown, since building
/// art takes longer than they last.
pub fn preload(needed: &mut Needed, art: &mut UnitArt, world: &Civ3World) {
    const FIXED: [(&str, i32); 13] = [
        ("Art/Animations/Disorder/Disorder.ini", ATTACK1),
        ("Art/Animations/Fireworks/Fireworks.ini", ATTACK1),
        ("Art/Animations/Trajectory/Hit.ini", DEATH),
        ("Art/Animations/Trajectory/Hit2.ini", DEATH),
        ("Art/Animations/Trajectory/Hit3.ini", DEATH),
        ("Art/Animations/Trajectory/Hit5.ini", DEATH),
        ("Art/Animations/Trajectory/Miss.ini", DEATH),
        ("Art/Animations/Trajectory/water Miss.ini", DEATH),
        ("Art/Animations/Smolder/Smolder.ini", ATTACK1),
        ("Art/Animations/Eruption/Eruption.ini", ATTACK1),
        ("Art/Animations/Plague/Plague.ini", ATTACK1),
        ("Art/Animations/SAM/SAM.ini", DEATH),
        ("Art/Animations/SDI/SDI.ini", DEATH),
    ];
    for (ini, animation) in FIXED {
        needed.add_effect(art, ini, animation);
    }
    // The air units on the map: their bombs.
    let mut types: Vec<i32> = world.units.iter().map(|u| u.unit_type).collect();
    types.sort_unstable();
    types.dedup();
    for t in types {
        let air = world.unit_type_stats.get(t.max(0) as usize).is_some_and(|s| s[4] == 2);
        let Some(key) = world.unit_type_keys.get(t.max(0) as usize).filter(|_| air) else { continue };
        for era in &world.era_keys {
            // (Only those that have bombs: the others would show their
            // DEFAULT, which units have already.)
            let Some(ini) = art.unit_ini(key, era) else { continue };
            let default = art.effect_file(&ini, DEFAULT).map(|f| f.path.clone());
            if art.effect_file(&ini, VICTORY).is_some_and(|f| Some(&f.path) != default.as_ref()) {
                needed.add_effect(art, &ini, VICTORY);
            }
        }
    }
}

/// Puts the drawn effects into `scene` as free sprites (their frame's
/// sprite, its shadow first, as units are drawn): those that are not
/// DEATH after the fog of war and under the units (the animator draws them
/// before the units), DEATH after the units (0x4F0840), each group in the
/// game's order. Zoomed out the renderer halves them, as the game does.
pub fn place(world: &Civ3World, view: &Civ3View, art: &mut UnitArt, compiled: &render_desc::Compiled, scene: &mut Scene) {
    let (units_pass, under_units) = units::unit_passes(compiled);
    let mut under = Vec::new();
    let mut over = Vec::new();
    for e in drawn(world) {
        let Some(file) = art.effect_file(&e.ini, e.animation) else { continue };
        let chunk = units::frame_chunk(file, e.direction, e.frame);
        let at = (e.pixel.0 - volcano_shift(world, e) * if view.zoomed_out { 2 } else { 1 }, e.pixel.1);
        let (list, after_pass) = if e.animation == DEATH { (&mut over, units_pass) } else { (&mut under, under_units) };
        for (kind, scale_den) in [("EffectShadow", 16), ("EffectArt", 0)] {
            let Some((set, _)) = compiled.set(&format!("{kind}@{}/{chunk}", file.path)) else { continue };
            list.push(scene::FreeSprite {
                set: set.raw(),
                x: at.0,
                y: at.1,
                screen: false,
                after_pass,
                phase: 0,
                ticks_per_frame: 1,
                scale_den,
                tint: 0,
                zoom_art: 0,
                zoom: (0, 0),
                zoom_offset: (0, 0),
                clip: None,
                not_in_view: None,
                glyph: None,
                palette: 0,
            });
        }
    }
    if !under.is_empty() {
        scene.sprites.splice(0..0, under);
    }
    scene.sprites.extend(over);
}

/// How far the game moves an effect left (its camera right, 0x5DA4C0):
/// smoke over a volcano and an eruption, by the volcano's neighbour mask.
fn volcano_shift(world: &Civ3World, e: &Civ3Effect) -> i32 {
    let (x, y) = e.tile;
    let smoke = matches!(e.source, civ3_sys::EffectSource::Tile { effect: SMOLDER | ERUPTION });
    let volcano = world.tile(x, y).is_some_and(|t| t.terrain() == crate::rules::terrain::VOLCANO);
    if smoke && volcano { VOLCANO_SHIFT[crate::rules::hill_mask(world, x, y) as usize & 15] } else { 0 }
}

/// A game-relative art path (`Art/...`, forward slashes) from the path the
/// game resolved through its search folders (absolute, backslashes). Art
/// lives in the `Art` folder of one of those folders, which the game's
/// files (`civ3_assets::GameFiles`) search again in the same order.
pub fn art_path(resolved: &str) -> String {
    let path = resolved.replace('\\', "/");
    let lower = path.to_ascii_lowercase();
    match lower.rfind("/art/animations/").or_else(|| lower.rfind("/art/units/")).or_else(|| lower.rfind("/art/")) {
        Some(i) => path[i + 1..].to_owned(),
        None => path.trim_start_matches("./").to_owned(),
    }
}

/// The tracer's line for `e` (parsed back by `crate::world::Trace::parse`).
pub fn trace_line(e: &Civ3Effect) -> String {
    let (source, id) = match e.source {
        civ3_sys::EffectSource::City { effect } => ("city", effect),
        civ3_sys::EffectSource::Tile { effect } => ("tile", effect),
        civ3_sys::EffectSource::Extra => ("extra", -1),
    };
    let mut out = String::new();
    let _ = write!(
        out,
        "effect {source} {id} {} {} {} {} {} {} {} {}\t{}",
        e.tile.0, e.tile.1, e.pixel.0, e.pixel.1, e.direction, e.animation, e.frame, e.shown as u8, e.ini
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Civ3Tile, Trace};

    fn effect(source: civ3_sys::EffectSource, animation: i32) -> Civ3Effect {
        Civ3Effect { source, tile: (2, 2), pixel: (192, 96), direction: 5, animation, frame: 3, shown: true, ini: "Art/Animations/Trajectory/Hit.ini".into() }
    }

    #[test]
    fn art_paths_are_game_relative() {
        assert_eq!(art_path(r"C:\Games\Civilization III Complete\civ3PTW\Art\Animations\Trajectory\Hit.ini"), "Art/Animations/Trajectory/Hit.ini");
        assert_eq!(
            art_path(r"Z:\x\Art\Conquests\Conquests\WWII in the Pacific\Art\Units\B17\B17.INI"),
            "Art/Units/B17/B17.INI"
        );
        assert_eq!(art_path(r".\Art\Animations\SAM\Sam.INI"), "Art/Animations/SAM/Sam.INI");
    }

    #[test]
    fn trace_lines_round_trip() {
        let mut e = effect(civ3_sys::EffectSource::Tile { effect: 7 }, DEATH);
        e.ini = "Art/Animations/Trajectory/water Miss.ini".into();
        let extra = Civ3Effect { source: civ3_sys::EffectSource::Extra, ini: String::new(), ..effect(civ3_sys::EffectSource::Extra, VICTORY) };
        let city = effect(civ3_sys::EffectSource::City { effect: 1 }, ATTACK1);
        let text = [&e, &extra, &city].iter().map(|e| trace_line(e)).collect::<Vec<_>>().join("\n");
        let t = Trace::parse(&text).unwrap();
        assert_eq!(t.world.effects, vec![e, extra, city]);
    }

    /// The preloaded effect art compiles, and an effect's frame is a sprite
    /// of it, its smoke translucent (`CIV3_INSTALL`; skipped without an
    /// installation).
    #[test]
    fn effect_art_compiles_and_places() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        if !std::path::Path::new(&install).is_dir() {
            return eprintln!("no Civ III install, skipped");
        }
        let files = || civ3_assets::GameFiles::complete(&install);
        let mut art = UnitArt::new(files());
        let mut needed = Needed { smoke: art.smoke().to_vec(), ..Needed::default() };
        preload(&mut needed, &mut art, &Civ3World::default());
        assert!(needed.effects.contains_key("Art/Animations/Trajectory/miss.flc"), "{:?}", needed.effects.keys());
        assert_eq!(needed.smoke.len(), 16);
        let desc = format!(
            "[grid]\nlayout = \"staggered\"\nstep = [64, 32]\ncolor_bits = 5\n[loader]\nquantize = \"rgb555\"\ntransparent = [254, 255]\n\
             [[layers]]\nname = \"fog\"\npass = 5\nsets = []\n[[layers]]\nname = \"unit_body\"\npass = 6\nsets = []\n\
             [images.movement_led]\nsource = \"rects:44x8;0,200,0,255:1,1,6,6\"\n{}",
            needed.description()
        );
        let compiled = crate::assets::compile(&desc, files()).unwrap();
        // A miss on tile (2, 2), frame 11 (smoke in it), over the units.
        let mut world = Civ3World { player: 1, width: 6, height: 6, tiles: vec![Civ3Tile::default(); 18], ..Default::default() };
        world.tiles[(2 * 6 + 2) / 2].visibility = 1 << 1;
        world.effects = vec![Civ3Effect { frame: 11, ini: "Art/Animations/Trajectory/Miss.ini".into(), ..effect(civ3_sys::EffectSource::Tile { effect: 7 }, DEATH) }];
        let mut scene = Scene::empty(scene::WorldShape::default(), compiled.layers.len());
        place(&world, &Civ3View::default(), &mut art, &compiled, &mut scene);
        let (units_pass, _) = units::unit_passes(&compiled);
        assert_eq!(scene.sprites.len(), 2);
        assert_eq!((scene.sprites[0].scale_den, scene.sprites[1].scale_den), (16, 0));
        assert!(scene.sprites.iter().all(|s| s.after_pass == units_pass && (s.x, s.y) == (192, 96)));
        // Its art: indexed in the file's palette, smoke translucent.
        let (set, _) = compiled.set("EffectArt@Art/Animations/Trajectory/miss.flc/11").unwrap();
        let sprite = compiled.atlas.sprites[compiled.sets.frame(set, 0).unwrap().0 as usize];
        let row = &compiled.atlas.palettes[sprite.palette as usize * tile_render::PALETTE_SIZE as usize..][..256];
        let alpha = |i: usize| (row[i] >> 24) as u8;
        assert_eq!(alpha(224), needed.smoke[0][3]);
        assert_eq!((alpha(200), alpha(255)), (255, 0));
    }

    #[test]
    fn the_game_draws_what_it_shows_where_the_player_sees() {
        // A 6x6 map; the player (1) sees tile (2, 2) only.
        let mut world = Civ3World { player: 1, width: 6, height: 6, tiles: vec![Civ3Tile::default(); 18], ..Default::default() };
        world.tiles[(2 * 6 + 2) / 2].visibility = 1 << 1;
        let tile = |e: Civ3Effect, at: (i32, i32)| Civ3Effect { tile: at, ..e };
        let hit = effect(civ3_sys::EffectSource::Tile { effect: 3 }, DEATH);
        world.effects = vec![
            hit.clone(),
            // Fogged.
            tile(hit.clone(), (4, 2)),
            // DEFAULT, hidden, without art.
            effect(civ3_sys::EffectSource::Tile { effect: 3 }, DEFAULT),
            Civ3Effect { shown: false, ..hit.clone() },
            Civ3Effect { ini: String::new(), ..hit.clone() },
            // The extra animations: wherever they are.
            tile(effect(civ3_sys::EffectSource::Extra, VICTORY), (4, 2)),
        ];
        let shown: Vec<&Civ3Effect> = drawn(&world).collect();
        assert_eq!(shown, vec![&world.effects[0], &world.effects[5]]);
    }
}
