//! Identifying the map sprites the game draws.
//!
//! Every terrain, overlay and improvement sprite of the main map is an
//! element of one of `Map_Renderer`'s `Sprite` arrays (or of the two heap
//! arrays it points to, for resources), so a `Sprite*` names a sheet and an
//! index by address arithmetic alone.

use inproc::Ptr32;

use crate::layout as L;

/// A sprite of the map renderer, by array (member name in
/// `Civ3Conquests.h`) and index.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SpriteRef {
    pub array: &'static str,
    pub index: u32,
}

/// Arrays reached through a pointer member instead of being embedded.
const POINTER_ARRAYS: &[&str] = &["Resources", "ResourcesShadows"];
/// Upper bound on the length of the pointed-to arrays (BIQ resource count).
const MAX_POINTED: u32 = 256;

/// Runs of consecutive arrays the game indexes as one array (docs §6):
/// the first member names the run, later members continue its numbering.
const RUNS: &[&[&str]] = &[
    &["Mountains_Images", "Mountains_Forests_Images", "Mountains_Jungles_Images", "Mountains_Snow_Images"],
    &["Volcanos_Images", "Volcanos_Forests_Images", "Volcanos_Jungles_Images", "Volcanos_Snow_Images"],
    &["Hills_Images", "Hills_Forests_Images", "Hills_Jungle_Images"],
    &["Irrigation_Desert_Images", "Irrigation_Plains_Images", "Irrigation_Images", "Irrigation_Tundra_Images"],
    // Interleaved: slot 2m = deltaRivers cell m, 2m+1 = mtnRivers cell m.
    &["Delta_Rivers_Images", "Mountain_Rivers_Images"],
];

/// Like [`identify`], but members of a run are reported as their run's
/// first array with a continuous index (`Mountains_Snow_Images/2` becomes
/// `Mountains_Images/50`), which is how the game's code indexes them.
pub fn identify_canonical(map_renderer: u32, sprite: u32) -> Option<SpriteRef> {
    let r = identify(map_renderer, sprite)?;
    for run in RUNS {
        if let Some(pos) = run.iter().position(|&a| a == r.array) {
            let before: u32 = run[..pos]
                .iter()
                .map(|a| L::MAP_RENDERER_FIELDS.iter().find(|f| f.0 == *a).map_or(0, |f| f.2 / L::SPRITE_SIZE))
                .sum();
            return Some(SpriteRef { array: run[0], index: before + r.index });
        }
    }
    Some(r)
}

/// Width and height of the sprite at `sprite`.
pub fn size(sprite: u32) -> Option<(u32, u32)> {
    let base = Ptr32::<u8>::new(sprite);
    let w: i32 = base.offset::<i32>(L::SPRITE_WIDTH).ok()?.read().ok()?;
    let h: i32 = base.offset::<i32>(L::SPRITE_HEIGHT).ok()?.read().ok()?;
    Some((w.max(0) as u32, h.max(0) as u32))
}

/// Sprite arrays outside the `Map_Renderer`: (name, address, length).
fn global_arrays(build: crate::Build) -> &'static [(&'static str, u32, u32)] {
    match build {
        // Cities (docs §8): index = size class·20 + era·5 + culture group.
        // Ruins (docs §6.7b): DESTROY.PCX, three cells.
        crate::Build::PcGames => &[("City_Images", 0xB3_CE60, 80), ("Ruins", 0xB3_E790, 3)],
        crate::Build::Gog | crate::Build::Steam => &[],
    }
}

/// Resolves a sprite of one of the global arrays.
pub fn identify_global(build: crate::Build, sprite: u32) -> Option<SpriteRef> {
    global_arrays(build).iter().find_map(|&(array, base, len)| {
        let offset = sprite.checked_sub(base)?;
        (offset % L::SPRITE_SIZE == 0 && offset / L::SPRITE_SIZE < len).then(|| SpriteRef { array, index: offset / L::SPRITE_SIZE })
    })
}

/// Resolves `sprite` against the `Map_Renderer` at `map_renderer`.
pub fn identify(map_renderer: u32, sprite: u32) -> Option<SpriteRef> {
    // The pointed-to arrays have no known length: of those that could hold
    // the sprite, the one starting closest below it (the heap may place
    // one right after another).
    let mut pointed: Option<SpriteRef> = None;
    for &(name, offset, size) in L::MAP_RENDERER_FIELDS {
        if name.starts_with("PCX_") {
            continue; // file names, not sprites
        }
        let (base, len) = if POINTER_ARRAYS.contains(&name) {
            let p: Ptr32<u32> = Ptr32::<u8>::new(map_renderer).offset(offset).ok()?;
            let Ok(base) = p.read() else { continue };
            if base == 0 {
                continue;
            }
            (base, MAX_POINTED * L::SPRITE_SIZE)
        } else {
            (map_renderer.wrapping_add(offset), size)
        };
        let rel = sprite.wrapping_sub(base);
        if rel < len && rel % L::SPRITE_SIZE == 0 {
            let found = SpriteRef { array: name, index: rel / L::SPRITE_SIZE };
            if !POINTER_ARRAYS.contains(&name) {
                return Some(found);
            }
            if pointed.is_none_or(|p| found.index < p.index) {
                pointed = Some(found);
            }
        }
    }
    pointed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_arrays_resolve_by_offset() {
        let mr = 0x1000_0000;
        let (_, terrain, _) = L::MAP_RENDERER_FIELDS.iter().find(|f| f.0 == "Std_Terrain_Images").unwrap();
        // Sheet 2, image 7 of the nine 81-image terrain sheets.
        let sprite = mr + terrain + (2 * 81 + 7) * L::SPRITE_SIZE;
        assert_eq!(identify(mr, sprite), Some(SpriteRef { array: "Std_Terrain_Images", index: 2 * 81 + 7 }));
        assert_eq!(identify(mr, sprite + 4), None);
    }
}

/// The file names the map renderer loaded its sheets from, as
/// (member name, element index, path), read from the running game.
pub fn sheet_files(map_renderer: u32) -> Vec<(&'static str, u32, String)> {
    let mut out = Vec::new();
    for &(name, offset, size) in L::MAP_RENDERER_FIELDS {
        if !name.starts_with("PCX_") {
            continue;
        }
        let Ok(p) = Ptr32::<u8>::new(map_renderer).offset::<u8>(offset) else { continue };
        let Ok(bytes) = p.read_array(size as usize) else { continue };
        for (i, s) in bytes.chunks(260).enumerate() {
            let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
            out.push((name, i as u32, s[..end].iter().map(|&b| b as char).collect()));
        }
    }
    out
}
