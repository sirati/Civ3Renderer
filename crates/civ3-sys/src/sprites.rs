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

/// Resolves `sprite` against the `Map_Renderer` at `map_renderer`.
pub fn identify(map_renderer: u32, sprite: u32) -> Option<SpriteRef> {
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
            return Some(SpriteRef { array: name, index: rel / L::SPRITE_SIZE });
        }
    }
    None
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
