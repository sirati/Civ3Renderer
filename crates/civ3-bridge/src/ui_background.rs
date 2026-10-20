//! Pure texture regions of the native game UI. Mixed art keeps its
//! original samples outside these individually inspected source rectangles.

use menu::screen::Region;

/// Absolute source-image rectangles, before crop/clip/canvas placement.
/// Masks and substituted palettes may turn texture into a label or icon.
pub(crate) fn regions(region: &Region) -> Vec<[i32; 4]> {
    if region.mask.is_some() || region.palette.is_some() { return Vec::new(); }
    let path = region.source.split(['/', '\\']).filter(|part| !part.is_empty()).collect::<Vec<_>>().join("/");
    let texture: &[[i32; 4]] = match path.to_ascii_lowercase().as_str() {
        "art/advisors/culture.pcx" => &[[745, 329, 28, 200], [748, 312, 208, 376], [456, 350, 248, 16], [456, 394, 248, 16], [456, 438, 248, 16], [456, 482, 248, 16], [456, 526, 248, 16], [456, 570, 248, 16], [456, 614, 248, 16], [456, 658, 248, 16]],
        "art/advisors/domestic.pcx" => &[[745, 329, 28, 35], [745, 398, 22, 16], [745, 431, 22, 16], [745, 464, 22, 16], [104, 340, 844, 12], [104, 390, 844, 12], [104, 436, 844, 12], [104, 482, 844, 12], [104, 528, 844, 12], [104, 574, 844, 12], [104, 620, 844, 12], [104, 666, 844, 12]],
        "art/advisors/foreign.pcx" => &[[745, 329, 28, 200], [104, 104, 688, 584], [792, 256, 160, 432]],
        "art/advisors/military.pcx" => &[[745, 428, 28, 35], [745, 365, 22, 16], [745, 398, 22, 16], [745, 497, 22, 16], [104, 312, 412, 16], [564, 312, 388, 16]],
        "art/advisors/science_ancient.pcx" => &[[745, 329, 28, 200], [704, 256, 248, 432], [640, 104, 152, 88]],
        "art/advisors/science_industrial_new.pcx" => &[[745, 329, 28, 35], [745, 395, 28, 134], [520, 104, 272, 104], [104, 104, 24, 584], [928, 256, 24, 432]],
        "art/advisors/science_middle.pcx" => &[[872, 256, 80, 432], [104, 104, 56, 584]],
        "art/advisors/science_modern.pcx" => &[[840, 256, 112, 432], [104, 104, 56, 584]],
        "art/advisors/trade.pcx" => &[[745, 428, 28, 35], [745, 365, 22, 16], [745, 398, 22, 16], [745, 497, 22, 16], [564, 572, 388, 24], [564, 616, 388, 24], [564, 660, 388, 24]],
        "art/advisors/wonders_background.pcx" => &[[104, 104, 824, 584]],
        "art/civilopedia/civilopediab.pcx" => &[[224, 132, 416, 212], [224, 388, 416, 20], [344, 440, 296, 56], [344, 536, 296, 64], [344, 660, 296, 24], [712, 344, 96, 288]],
        "art/civilopedia/civilopediabackground.pcx" => &[[226, 130, 418, 570], [712, 352, 96, 272]],
        "art/interface/x_pref background.pcx" => &[[4, 4, 1016, 68], [4, 84, 36, 676], [984, 84, 36, 620], [4, 704, 936, 60], [128, 136, 256, 132], [428, 136, 216, 132], [688, 136, 240, 76], [688, 256, 240, 12], [128, 336, 256, 140], [428, 336, 216, 140], [688, 336, 240, 140], [128, 544, 112, 92], [688, 544, 240, 92]],
        "art/popupandmenubackground.pcx" => &[[0, 0, 1023, 327]],
        "art/popupborders.pcx" => &[[265, 43, 46, 2], [313, 43, 60, 2], [375, 43, 36, 2], [265, 46, 46, 44], [313, 46, 60, 44], [375, 46, 36, 44]],
        _ => &[],
    };
    texture.to_vec()
}

/// The native preload list normally stores indexed art. A mixed source
/// makes two masked copies, and either may require RGBA if every palette
/// index remains in use. Reserve that worst case before scheduling it.
pub(crate) fn source_bytes(region: &Region) -> u64 {
    let [x, y, w, h] = region.rect;
    if w <= 0 || h <= 0 { return 0; }
    let rects = regions(region);
    let whole = rects.iter().any(|&[tx, ty, tw, th]| x >= tx && y >= ty && i64::from(x) + i64::from(w) <= i64::from(tx + tw) && i64::from(y) + i64::from(h) <= i64::from(ty + th));
    let partial = !whole && rects.iter().any(|&[tx, ty, tw, th]| i64::from(x) < i64::from(tx + tw) && i64::from(y) < i64::from(ty + th) && i64::from(x) + i64::from(w) > i64::from(tx) && i64::from(y) + i64::from(h) > i64::from(ty));
    u64::from(w as u32).saturating_mul(u64::from(h as u32)).saturating_mul(if partial { 8 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui_background(region: &Region) -> bool {
        let [x, y, w, h] = region.rect;
        w > 0 && h > 0 && regions(region).iter().any(|&[tx, ty, tw, th]| x >= tx && y >= ty && i64::from(x) + i64::from(w) <= i64::from(tx + tw) && i64::from(y) + i64::from(h) <= i64::from(ty + th))
    }

    #[test]
    fn ui_background_sampling_requires_an_unmodified_known_texture_crop() {
        let parchment = Region {
            source: "Art/PopUpandMenuBackground.pcx".into(),
            rect: [0, 0, 1023, 327], keyed: false, mask: None, palette: None,
        };
        for source in ["Art/PopUpandMenuBackground.pcx", r"art\POPUPANDMENUBACKGROUND.PCX"] {
            for rect in [[0, 0, 1023, 327], [12, 34, 50, 67], [1022, 326, 1, 1]] {
                for keyed in [false, true] {
                    assert!(ui_background(&Region { source: source.into(), rect, keyed, ..parchment.clone() }));
                }
            }
        }
        for rect in [[-1, 0, 10, 10], [0, -1, 10, 10], [0, 0, 0, 10], [0, 0, 10, -1], [1, 0, 1023, 327], [0, 1, 1023, 327], [i32::MAX, 0, i32::MAX, 1]] {
            assert!(!ui_background(&Region { rect, ..parchment.clone() }), "outside the known texture: {rect:?}");
        }
        let civilopedia = Region { source: "Art/Civilopedia/civilopediaBackground.pcx".into(), ..parchment.clone() };
        for rect in [[226, 130, 398, 570], [226, 130, 418, 570], [240, 150, 16, 16]] {
            assert!(ui_background(&Region { rect, ..civilopedia.clone() }), "verified native texture crop {rect:?}");
        }
        for rect in [[172, 11, 677, 734], [225, 130, 418, 570], [226, 129, 418, 570], [226, 130, 419, 570], [226, 130, 418, 571]] {
            assert!(!ui_background(&Region { rect, ..civilopedia.clone() }), "must not sample across a native texture boundary {rect:?}");
        }
        for (source, rect) in [
            ("Art/popupborders.pcx", [265, 46, 46, 44]),
            ("Art/popupborders.pcx", [375, 46, 36, 44]),
            ("Art/popupborders.pcx", [313, 43, 60, 2]),
            ("Art/Advisors/culture.pcx", [745, 362, 28, 35]),
            ("Art/Advisors/foreign.pcx", [745, 494, 28, 35]),
            ("Art/Advisors/science_ancient.pcx", [745, 362, 28, 35]),
            ("Art/Advisors/science_industrial_new.pcx", [745, 395, 28, 35]),
            ("Art/Advisors/domestic.pcx", [745, 398, 22, 16]),
            ("Art/Advisors/military.pcx", [745, 428, 28, 35]),
            ("Art/Advisors/trade.pcx", [745, 365, 22, 16]),
        ] {
            assert!(ui_background(&Region { source: source.into(), rect, ..parchment.clone() }), "verified native background: {source} {rect:?}");
        }
        for (source, rect) in [
            ("Art/popupborders.pcx", [251, 1, 60, 44]),
            ("Art/popupborders.pcx", [375, 46, 60, 44]),
            ("Art/popupborders.pcx", [265, 43, 46, 4]),
            ("Art/Advisors/domestic.pcx", [745, 362, 28, 35]),
            ("Art/Advisors/domestic.pcx", [745, 365, 22, 16]),
            ("Art/Advisors/military.pcx", [745, 329, 28, 35]),
            ("Art/Advisors/military.pcx", [745, 464, 22, 16]),
            ("Art/Advisors/trade.pcx", [745, 332, 22, 16]),
            ("Art/Advisors/science_industrial_new.pcx", [745, 362, 28, 35]),
            ("Art/Advisors/science_industrial_new.pcx", [745, 365, 22, 16]),
        ] {
            assert!(!ui_background(&Region { source: source.into(), rect, ..parchment.clone() }), "border, separator or arrow art: {source} {rect:?}");
        }
        // These contain graphics, labels, controls or borders. Size and an
        // opaque draw are not enough to classify any of them as texture.
        for source in [
            "Art/title.pcx", "Art/x_title.PCX", "Art/interface/x_pref background.pcx",
            "Art/Civilopedia/civilopediaBackground.pcx", "Art/city screen/background.pcx",
            "Art/WorldSetup/background.pcx", "Art/Advisors/wonders_background.pcx",
            "Art/Credits/credits_background.pcx", "Art/popupborders.pcx",
            "Art/interface/menuButtons.pcx", "Art/Terrain/xtgc.pcx",
            "Art/Other/PopUpandMenuBackground.pcx", "Other/Art/PopUpandMenuBackground.pcx",
            "Art/PopUpandMenuBackground.pcx/other",
        ] {
            assert!(!ui_background(&Region { source: source.into(), rect: [0, 0, 16, 16], ..parchment.clone() }), "mixed or unknown art: {source}");
        }
        assert!(!ui_background(&Region {
            mask: Some(("text-mask.pcx".into(), (2, 3))), ..parchment.clone()
        }), "a texture masked into a label must retain nearest sampling");
        assert!(!ui_background(&Region {
            palette: Some(menu::screen::PaletteFrom { image: "colors.pcx".into(), first: 0, count: 256 }), ..parchment
        }), "palette substitutions keep their original sampling");
    }


}
