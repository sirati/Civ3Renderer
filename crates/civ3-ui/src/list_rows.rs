//! The rows of the game's list boxes, drawn from the game's data for each
//! entry rather than taken from what the game drew (docs/smooth-lists.md,
//! "Bridge design"): ports of the rows' draw callbacks, giving the same
//! sprites at the same places, so a list can be shown whole and scrolled
//! by the viewer, entries the game never drew included.
//!
//! A row is a list of draws in the list canvas's pixels: game sprites
//! (`Sprite*`, drawn as `Sprite_draw` draws them: the bridge adds the
//! sprite's crop offset and finds its pixels as the mirror does) and text
//! (its baseline, as jgl's DCs align it).

/// What of a game `Sprite` the rows use (Civ3Conquests.h `struct Sprite`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct SpriteInfo {
    /// Its address (`Sprite*`).
    pub addr: u32,
    /// `Width3` (+0x10).
    pub width3: i32,
    /// `Width` (+0x18), `Height` (+0x1C).
    pub width: i32,
    pub height: i32,
    /// `Height2` (+0x24).
    pub height2: i32,
}

/// One thing a row draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowDraw {
    /// `Sprite_draw(sprite, canvas, x, y)`.
    Sprite { sprite: u32, x: i32, y: i32 },
    /// Text with its left end at `x` and its baseline at `baseline`.
    Text { text: String, x: i32, baseline: i32 },
}

/// The icons of the city screen's improvement rows
/// (`City_Form.City_Icons_Images`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ImprovementIcons {
    /// Upkeep: `Icon_17_Gold_Outcome`.
    pub upkeep: SpriteInfo,
    /// Tourism: `Icon_14_Gold`; C3X draws some of them as food
    /// (`Icon_15_Food`, or its own red food icon when negative) or shields
    /// (`Icon_13_Shield`, `Icon_05_Shield_Outcome` when negative).
    pub tourism: SpriteInfo,
    /// Culture: `Icon_18_Culture`.
    pub culture: SpriteInfo,
    /// Contentment: `Icon_19_Happy_Faces`.
    pub happy: SpriteInfo,
}

/// One improvement's row, as the game's data gives it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImprovementRow {
    pub improvement: i32,
    /// `City_Form.Improvement_Images_Small[improvement]`.
    pub small: SpriteInfo,
    /// Its name (BIC improvement +0x44).
    pub name: String,
    /// `City_get_improvement_maintenance` as the row asks for it (C3X:
    /// its `_for_ui` variant).
    pub upkeep: i32,
    /// The sprite of each tourism icon, in order (the tourism amount the
    /// row asks for; C3X turns the first ones into food and shields).
    pub tourism: Vec<u32>,
    /// The culture icons (0x4F8DA0 for the city's civ).
    pub culture: i32,
    /// The happy faces, if the row shows them (BIC `Happy_Faces`, doubled
    /// where the civ doubles them; none for an obsolete or other-government
    /// temple-like building).
    pub happy: i32,
}

/// Where a group's icons go: `n` icons of `width` in `room` pixels, a step
/// of `width`, or closer when they do not fit (`max((room - width) /
/// (n - 1), 1)`, at most `width`; 0x426790).
fn step(n: i32, width: i32, room: i32) -> i32 {
    if n > 1 && width * n > room {
        ((room - width) / (n - 1)).max(1).min(width)
    } else {
        width
    }
}

/// `draw_improv_icons_on_city_screen` (0x4212C0) and the row's text
/// callback (0x421210) for the entry whose rectangle is `rect` (left, top,
/// right, bottom, in the list canvas's pixels). `canvas_width`: the list
/// canvas's width; `text_cell`: the list font's `+0x10` (the text
/// routines centre a text's cell of that height and put its baseline at
/// its bottom).
pub fn improvement_row(row: &ImprovementRow, icons: &ImprovementIcons, canvas_width: i32, rect: [i32; 4], text_cell: i32) -> Vec<RowDraw> {
    let [left, top, right, bottom] = rect;
    let mut out = Vec::new();
    let small = row.small;
    out.push(RowDraw::Sprite { sprite: small.addr, x: left, y: top });
    // The rest of the row in quarters (truncated, as `cdq; and 3; sar 2`).
    let quarter = (canvas_width - 10 - small.width) / 4;
    let middle = small.height / 2 + top;
    let mut x = left + small.width - 2;
    let mut end = x + quarter;
    // Upkeep.
    let upkeep_step = step(row.upkeep, icons.upkeep.width3, end - x);
    for _ in 0..row.upkeep.max(0) {
        out.push(RowDraw::Sprite { sprite: icons.upkeep.addr, x, y: middle - icons.upkeep.height2 });
        x += upkeep_step;
    }
    // Tourism: after the upkeep icons and one more of their steps (the
    // game's own order), in the second quarter's room.
    let start = end + 1;
    end = start + quarter;
    let n = row.tourism.len() as i32;
    if n > 0 {
        x += upkeep_step;
        let tourism_step = step(n, icons.tourism.width3, end - start);
        for &sprite in &row.tourism {
            out.push(RowDraw::Sprite { sprite, x, y: middle - icons.tourism.height2 });
            x += tourism_step;
        }
    }
    // Culture, in the third quarter.
    x = end + 1;
    end = x + quarter;
    let culture_step = step(row.culture, icons.culture.width3, end - x);
    for _ in 0..row.culture.max(0) {
        out.push(RowDraw::Sprite { sprite: icons.culture.addr, x, y: middle - icons.culture.height2 });
        x += culture_step;
    }
    // Happy faces, in the fourth.
    x = end + 1;
    end = x + quarter;
    let happy_step = step(row.happy, icons.happy.width3, end - x);
    for _ in 0..row.happy.max(0) {
        out.push(RowDraw::Sprite { sprite: icons.happy.addr, x, y: middle - icons.happy.height2 });
        x += happy_step;
    }
    // The name (0x421210): in the row's upper half, right of the picture,
    // moved 2 down (`PCX_Image::draw_text_in_rect`-like 0x5FEEF0: the
    // cell centred, its baseline at the cell's bottom).
    if !row.name.is_empty() {
        let (l, t, b) = (left + small.width + 2, top + 2, (bottom - top) / 2 + top + 2);
        let _ = right;
        let cell_top = t + (b - t - text_cell) / 2;
        out.push(RowDraw::Text { text: row.name.clone(), x: l, baseline: cell_top + text_cell });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const COIN: SpriteInfo = SpriteInfo { addr: 0x746930, width3: 15, width: 15, height: 15, height2: 7 };
    const GOLD: SpriteInfo = SpriteInfo { addr: 0x7468AC, width3: 14, width: 14, height: 16, height2: 8 };
    const CULTURE: SpriteInfo = SpriteInfo { addr: 0x74695C, width3: 14, width: 14, height: 16, height2: 8 };
    const HAPPY: SpriteInfo = SpriteInfo { addr: 0x746988, width3: 13, width: 13, height: 15, height2: 7 };
    const ICONS: ImprovementIcons = ImprovementIcons { upkeep: COIN, tourism: GOLD, culture: CULTURE, happy: HAPPY };

    fn row(upkeep: i32, culture: i32, happy: i32) -> ImprovementRow {
        ImprovementRow {
            improvement: 3,
            small: SpriteInfo { addr: 0x9000, width3: 32, width: 32, height: 32, height2: 16 },
            name: "Temple".into(),
            upkeep,
            tourism: Vec::new(),
            culture,
            happy,
        }
    }

    fn sprites(draws: &[RowDraw], sprite: u32) -> Vec<(i32, i32)> {
        draws.iter().filter_map(|d| match d { RowDraw::Sprite { sprite: s, x, y } if *s == sprite => Some((*x, *y)), _ => None }).collect()
    }

    #[test]
    fn the_icons_go_in_quarters_of_the_row_right_of_the_picture() {
        // A 140-wide canvas (the Athens improvements list, run 30): the
        // picture 32 wide, quarters of (140 - 10 - 32) / 4 = 24.
        let draws = improvement_row(&row(1, 2, 1), &ICONS, 140, [0, 32, 140, 64], 7);
        assert_eq!(draws[0], RowDraw::Sprite { sprite: 0x9000, x: 0, y: 32 });
        // Upkeep from 0 + 32 - 2, at the row's middle less the icon's Height2.
        assert_eq!(sprites(&draws, COIN.addr), [(30, 48 - 7)]);
        // Culture in the third quarter: 30 + 24 + 1 + 24 + 1 = 80; two of
        // 14 do not fit its 24 pixels: (24 - 14) / 1 = 10 apart.
        assert_eq!(sprites(&draws, CULTURE.addr), [(80, 40), (90, 40)]);
        // Happy faces in the fourth: 80 + 24 + 1 = 105.
        assert_eq!(sprites(&draws, HAPPY.addr), [(105, 41)]);
        // The name in the upper half: cell (34..50) of 7, baseline 34 + 4 + 7.
        assert_eq!(draws.last(), Some(&RowDraw::Text { text: "Temple".into(), x: 34, baseline: 45 }));
    }

    #[test]
    fn icons_that_do_not_fit_their_quarter_go_closer() {
        // Six culture icons of 14 in 24 pixels: (24 - 14) / 5 = 2 apart.
        let draws = improvement_row(&row(0, 6, 0), &ICONS, 140, [0, 0, 140, 32], 7);
        assert_eq!(sprites(&draws, CULTURE.addr).iter().map(|p| p.0).collect::<Vec<_>>(), [80, 82, 84, 86, 88, 90]);
        assert!(sprites(&draws, COIN.addr).is_empty() && sprites(&draws, HAPPY.addr).is_empty());
    }

    #[test]
    fn tourism_follows_the_upkeep_icons_one_step_on() {
        let mut r = row(2, 0, 0);
        r.tourism = vec![GOLD.addr, GOLD.addr];
        let draws = improvement_row(&r, &ICONS, 140, [0, 0, 140, 32], 7);
        // Two coins of 15 in 24 pixels: 9 apart, at 30 and 39; tourism
        // from 39 + 9 + 9 = 57, two of 14 in 24: 10 apart.
        assert_eq!(sprites(&draws, COIN.addr).iter().map(|p| p.0).collect::<Vec<_>>(), [30, 39]);
        assert_eq!(sprites(&draws, GOLD.addr).iter().map(|p| p.0).collect::<Vec<_>>(), [57, 67]);
    }
}
