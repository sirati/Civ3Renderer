//! Background pictures of the native UI, including their frames and tiles.
//! Foreground art from other files keeps nearest sampling. Controls baked
//! into a background picture are split off on the same source pixel grid.

use menu::screen::Region;

/// Absolute source-image rectangles, before crop/clip/canvas placement.
/// A mask or substituted palette can turn a background into foreground art.
pub(crate) fn regions(region: &Region) -> Vec<[i32; 4]> {
    if region.mask.is_some() || region.palette.is_some() { return Vec::new(); }
    let path = region.source.split(['/', '\\']).filter(|part| !part.is_empty()).collect::<Vec<_>>().join("/").to_ascii_lowercase();
    match path.as_str() {
        // Include the entire frame atlas, not just its parchment interiors.
        "art/popupandmenubackground.pcx" | "art/popupborders.pcx" |
        "art/advisors/dialogbox.pcx" | "art/advisors/diplowinbox.pcx" |
        "art/advisors/wondersbox.pcx" | "art/advisors/foreigntab.pcx" |
        "art/title.pcx" | "art/x_title.pcx" | "art/gold_title.pcx" |
        "art/civ3complete_title_bkgrnd.pcx" | "art/histograph-top5_final.pcx" |
        "art/histograph-win5_final.pcx" |
        "art/advisors/culture.pcx" | "art/advisors/domestic.pcx" |
        "art/advisors/foreign.pcx" | "art/advisors/military.pcx" |
        "art/advisors/science_ancient.pcx" | "art/advisors/science_middle.pcx" |
        "art/advisors/science_industrial.pcx" | "art/advisors/science_industrial_new.pcx" |
        "art/advisors/science_modern.pcx" | "art/advisors/trade.pcx" |
        "art/advisors/wonders_background.pcx" | "art/advisors/hall_o_fame_background.pcx" |
        "art/advisors/demographics.pcx" | "art/advisors/camprecbg.pcx" |
        "art/civilopedia/b-cover.pcx" | "art/civilopedia/civilopediabackground.pcx" |
        "art/civilopedia/civilopediaa.pcx" | "art/civilopedia/civilopediab.pcx" |
        "art/civilopedia/civilopediac.pcx" | "art/civilopedia/civilopediad.pcx" |
        "art/civilopedia/civilopediae.pcx" | "art/civilopedia/civilopediaf.pcx" |
        "art/civilopedia/civilopediag.pcx" | "art/civilopedia/f-lower.pcx" |
        "art/civilopedia/f-upper.pcx" | "art/civilopedia/civilopediaa-bottom-coverup.pcx" |
        "art/civilopedia/civilopediaa-top-coverup.pcx" | "art/civilopedia/civilopediac-coverup.pcx" |
        "art/civilopedia/civilopediad-coverup.pcx" | "art/civilopedia/civilopediae-coverup.pcx" |
        "art/civilopedia/civilopediaf-coverup.pcx" |
        "art/interface/x_pref background.pcx" | "art/city screen/background.pcx" |
        "art/city screen/governorback.pcx" | "art/worldsetup/background.pcx" |
        "art/playersetup/playersetup.pcx" | "art/diplomacy/talk_offer.pcx" |
        "art/diplomacy/consider.pcx" | "art/diplomacy/counter.pcx" | "art/diplomacy/mpcounter.pcx" |
        "art/credits/credits_background.pcx" | "art/credits/credit_back.pcx" |
        "art/credits/complete_credit_back.pcx" | "art/espionage/x_espionagebkg.pcx" |
        "art/multiplayer/x_filters background.pcx" | "art/multiplayer/x_lobby background.pcx" |
        "art/multiplayer/x_setup background.pcx" | "art/multiplayer/x_staging background.pcx" |
        "art/scenario mgmt/x_scenario mgmt back.pcx" | "art/palaceview/bkgr.pcx" |
        "art/rankview/ranking_background.pcx" | "art/replay/replay_background.pcx" |
        "art/spaceship/shipbackground.pcx" | "art/wonder splash/wonderbackground.pcx" |
        "art/victoryscreen/conquer_victory.pcx" | "art/victoryscreen/culture_victory.pcx" |
        "art/victoryscreen/defeat.pcx" | "art/victoryscreen/diplomatic_victory.pcx" |
        "art/victoryscreen/domination_victory.pcx" | "art/victoryscreen/histograph_victory.pcx" |
        "art/victoryscreen/space_victory.pcx" => {},
        _ => return Vec::new(),
    };
    // Keep actual controls sharp, rather than excluding most of each
    // background because the same source also contains some controls.
    let mut controls = Vec::new();
    match path.as_str() {
        "art/interface/x_pref background.pcx" => {
            for (x, ys) in [(102, &[142, 164, 186, 208, 230, 252, 344, 366, 388, 410, 432, 454, 554, 584, 614][..]),
                (399, &[142, 164, 186, 208, 230, 252, 344, 366, 388, 410, 432, 454][..]),
                (657, &[142, 164, 344, 366, 388][..])] {
                controls.extend(ys.iter().map(|&y| [x, y, 16, 16]));
            }
            controls.extend([[719, 224, 153, 25], [259, 551, 408, 23], [259, 581, 408, 23], [259, 611, 408, 23], [953, 715, 66, 46]]);
        },
        "art/civilopedia/civilopediabackground.pcx" | "art/civilopedia/civilopediaa.pcx" |
        "art/civilopedia/civilopediab.pcx" | "art/civilopedia/civilopediac.pcx" |
        "art/civilopedia/civilopediad.pcx" | "art/civilopedia/civilopediae.pcx" |
        "art/civilopedia/civilopediaf.pcx" | "art/civilopedia/civilopediag.pcx" => controls.extend([[411, 29, 48, 32], [726, 681, 66, 42]]),
        "art/advisors/domestic.pcx" => controls.extend([[558, 82, 239, 80]]),
        _ => {},
    }
    // Subtract controls from this crop, so an unrelated crop stays one
    // background sprite. The role also covers replacement art of any size.
    let [_, _, w, h] = region.rect;
    if w <= 0 || h <= 0 { return Vec::new(); }
    let mut background = vec![region.rect];
    for [cx, cy, cw, ch] in controls {
        background = background.into_iter().flat_map(|[x, y, w, h]| {
            let (x, y, w, h) = (i64::from(x), i64::from(y), i64::from(w), i64::from(h));
            let (l, t, r, b) = (x.max(i64::from(cx)), y.max(i64::from(cy)), (x + w).min(i64::from(cx + cw)), (y + h).min(i64::from(cy + ch)));
            if l >= r || t >= b { return vec![[x as i32, y as i32, w as i32, h as i32]]; }
            [[x, y, w, t - y], [x, b, w, y + h - b], [x, t, l - x, b - t], [r, t, x + w - r, b - t]]
                .into_iter().filter(|rect| rect[2] > 0 && rect[3] > 0).map(|rect| rect.map(|v| v as i32)).collect()
        }).collect();
    }
    background
}

/// The native preload list normally stores indexed art. A mixed source
/// makes two masked copies, and either may require RGBA if every palette
/// index remains in use. Reserve that worst case before scheduling it.
pub(crate) fn source_bytes(region: &Region) -> u64 {
    let [x, y, w, h] = region.rect;
    if w <= 0 || h <= 0 { return 0; }
    let rects = regions(region);
    let whole = rects.iter().any(|&[tx, ty, tw, th]| x >= tx && y >= ty && i64::from(x) + i64::from(w) <= (i64::from(tx) + i64::from(tw)) && i64::from(y) + i64::from(h) <= (i64::from(ty) + i64::from(th)));
    let partial = !whole && rects.iter().any(|&[tx, ty, tw, th]| i64::from(x) < (i64::from(tx) + i64::from(tw)) && i64::from(y) < (i64::from(ty) + i64::from(th)) && i64::from(x) + i64::from(w) > i64::from(tx) && i64::from(y) + i64::from(h) > i64::from(ty));
    u64::from(w as u32).saturating_mul(u64::from(h as u32)).saturating_mul(if partial { 8 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(source: &str, rect: [i32; 4]) -> Region {
        Region { source: source.into(), rect, keyed: true, mask: None, palette: None }
    }

    fn background(source: &str, point: (i32, i32)) -> bool {
        regions(&region(source, [0, 0, 1024, 768])).iter().any(|&[x, y, w, h]| point.0 >= x && point.1 >= y && point.0 - x < w && point.1 - y < h)
    }

    #[test]
    fn complete_background_pictures_include_their_edges_and_panels() {
        for source in ["Art/title.pcx", "Art/x_title.PCX", "Art/city screen/background.pcx", "Art/WorldSetup/background.pcx",
            "Art/Advisors/wonders_background.pcx", "Art/Credits/credits_background.pcx", "Art/Diplomacy/talk_offer.pcx"] {
            assert_eq!(regions(&region(source, [0, 0, 1024, 768])), vec![[0, 0, 1024, 768]], "whole background picture: {source}");
        }
        let replacement = region("Art/x_title.PCX", [2048, 1024, 3000, 2000]);
        assert_eq!(regions(&replacement), vec![replacement.rect], "replacement background art is not limited to native dimensions");
        assert_eq!(source_bytes(&region("Art/x_title.PCX", [i32::MAX, i32::MAX, 2, 3])), 6);
        for source in ["Art/PopUpandMenuBackground.pcx", r"art\POPUPANDMENUBACKGROUND.PCX"] {
            assert_eq!(regions(&region(source, [12, 34, 50, 67])), vec![[12, 34, 50, 67]]);
        }
        for (source, points) in [
            ("Art/interface/x_pref background.pcx", &[(0, 0), (50, 100), (500, 300), (1023, 767)][..]),
            ("Art/Civilopedia/civilopediaBackground.pcx", &[(172, 11), (200, 110), (700, 500), (848, 744)][..]),
            ("Art/popupborders.pcx", &[(251, 1), (251, 91), (432, 46), (375, 132)][..]),
            ("Art/Advisors/domestic.pcx", &[(0, 0), (100, 300), (745, 365), (1023, 767)][..]),
        ] {
            for &point in points { assert!(background(source, point), "untreated background {source} at {point:?}"); }
        }
    }

    #[test]
    fn foreground_controls_and_modified_or_unknown_art_keep_nearest() {
        for (source, point) in [
            ("Art/interface/x_pref background.pcx", (109, 149)),
            ("Art/interface/x_pref background.pcx", (406, 171)),
            ("Art/interface/x_pref background.pcx", (400, 562)),
            ("Art/Civilopedia/civilopediaBackground.pcx", (425, 40)),
            ("Art/Advisors/domestic.pcx", (770, 100)),
        ] { assert!(!background(source, point), "baked control {source} at {point:?}"); }
        for source in ["Art/interface/menuButtons.pcx", "Art/exitBox-backgroundStates.pcx", "Art/Terrain/xtgc.pcx",
            "Art/Other/PopUpandMenuBackground.pcx", "Other/Art/PopUpandMenuBackground.pcx", "Art/PopUpandMenuBackground.pcx/other"] {
            assert!(regions(&region(source, [0, 0, 16, 16])).is_empty(), "foreground or unknown art: {source}");
        }
        let parchment = region("Art/PopUpandMenuBackground.pcx", [0, 0, 1023, 327]);
        assert!(regions(&Region { mask: Some(("text-mask.pcx".into(), (2, 3))), ..parchment.clone() }).is_empty());
        assert!(regions(&Region { palette: Some(menu::screen::PaletteFrom { image: "colors.pcx".into(), first: 0, count: 256 }), ..parchment }).is_empty());
    }
}
