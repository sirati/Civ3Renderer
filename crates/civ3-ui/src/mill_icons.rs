//! C3X R28's resource badges on an improvement row. The producer supplies
//! the game's answers to its technology, active-building and input-resource
//! queries. These are state queries, never pixels from a canvas or HDC.

use menu::screen::{IndexedResize, Indices};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub improvement: i32,
    pub resource: i32,
    pub icon: i32,
    pub flags: u16,
    pub bonus: bool,
    pub can_generate: bool,
    pub active: bool,
    pub has_inputs: bool,
}

pub fn shown_class(flags: u16, bonus: bool) -> bool {
    (flags & 8 != 0 || !bonus) && !(flags & 16 != 0 && !bonus)
}

/// C3X keeps duplicates and config order, stopping after 16 accepted entries.
/// `local` and `yields` affect production, but do not gate this icon.
pub fn selected(improvement: i32, candidates: impl IntoIterator<Item = Candidate>) -> Vec<(i32, i32)> {
    candidates.into_iter().filter(|c| c.improvement == improvement
        && shown_class(c.flags, c.bonus)
        && c.can_generate && c.active && c.has_inputs)
        .take(16).map(|c| (c.resource, c.icon)).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Icon {
    pub resource: i32,
    pub rect: [i32; 4],
    pub at: (i32, i32),
}

/// `pixel_x/y` are the untrimmed Sprite::draw arguments at PCG 0x421315,
/// after the list has placed/scrolled its row. C3X adds 15 in x and spreads
/// the icons over 16 pixels in y, with integer rounding exactly as below.
pub fn layout(resources: &[(i32, i32)], at: (i32, i32)) -> Vec<Icon> {
    let count = resources.len().min(16);
    resources.iter().take(count).enumerate().map(|(n, &(resource, icon))| Icon {
        resource,
        rect: [9 + 50 * (icon % 6), 9 + 50 * (icon / 6), 33, 33],
        at: (at.0 + 15, at.1 + ((n * 160 / count.saturating_sub(1).max(1) + 5) / 10) as i32),
    }).collect()
}

/// TransparentBlt compares RGB after conversion into the RGB555 destination, not
/// palette-index 254/255. Retain every entry that converts to magenta.
pub fn transparent(palette: &[[u8; 3]; 256]) -> Vec<u8> {
    palette.iter().enumerate().filter_map(|(i, &[r, g, b])|
        (r >> 3 == 31 && g >> 3 == 0 && b >> 3 == 31).then_some(i as u8)).collect()
}

pub fn indices(transparent: Vec<u8>) -> Indices {
    // Original Wine TransparentBlt, 33 -> 24, independently checked on
    // both axes and against all 36 file icons. The axes have different phase.
    Indices { transparent, sample_offset: [65535, 32768], resize: Some(IndexedResize {
        size: (24, 24), sample_step: [90112; 2],
    }), ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_key_uses_destination_rgb555_instead_of_special_indices() {
        let mut palette = [[1, 2, 3]; 256];
        palette[7] = [255, 0, 255];
        palette[8] = [248, 7, 250];
        palette[9] = [247, 0, 255];
        palette[254] = [0, 0, 255];
        assert_eq!(transparent(&palette), [7, 8]);
    }

    #[test]
    fn source_oracle_selection_and_layout() {
        // tools/mill-icon-oracle extracts the original C3X R28 producer.
        // Its 65,537 cases cover both no-output and capped 16-icon rows,
        // all five mill flags, classes, predicates, duplicates and order.
        let mut hash = 0xcbf29ce484222325u64;
        let mut put = |v: i32| { for b in v.to_le_bytes() { hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3); } };
        for case in 0..=65536u32 {
            let mut seed = case;
            let candidates: Vec<_> = (0..24).map(|n| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                Candidate { improvement: if case == 65536 || seed & 1 != 0 { 7 } else { 9 }, resource: n,
                    icon: ((seed >> 5) % 36) as i32, flags: if case == 65536 { 0 } else { ((seed >> 10) & 31) as u16 },
                    bonus: case != 65536 && seed >> 15 & 3 == 0, can_generate: case == 65536 || seed >> 17 & 3 != 0,
                    active: case == 65536 || seed >> 19 & 3 != 0, has_inputs: case == 65536 || seed >> 21 & 3 != 0 }
            }).collect();
            let selected = selected(7, candidates);
            put(selected.len() as i32);
            for icon in layout(&selected, (-3, 19)) {
                for v in [icon.rect[0], icon.rect[1], icon.rect[2], icon.rect[3], icon.at.0, icon.at.1] { put(v); }
            }
        }
        assert_eq!(hash, 0x5217c163df8b908c);
    }
}
