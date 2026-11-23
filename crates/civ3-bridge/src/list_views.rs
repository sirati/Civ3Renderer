//! The game's list boxes as views the viewer scrolls itself
//! (`scene::ListView`, docs/smooth-lists.md "Bridge design").
//!
//! For a list the bridge can draw from the game's data (now: the city
//! screen's improvements), every entry's row is made on the game's thread
//! at each present (`civ3_sys::city_lists`, laid out by
//! `civ3_ui::list_rows`), in the list's content space: entry `k` at
//! `y = k · step`, `x` from the rows' left. The publisher shows them as a
//! view of their own, which the viewer scrolls by screen pixels, partial
//! rows cut at the list's edges, the screen's background still.
//!
//! What the game draws of such a list's rows never reaches the mirror:
//! [`install`] hooks the entry draw of every list box (0x617470) and the
//! mirror drops what is drawn inside it for these lists
//! ([`drawing_shown_row`]); everything else the list draws (its
//! background, its scroll bar) stays the game's. The game's own position
//! follows the viewer's (`Input::ListScrolled`, [`scroll_to`]), so that a
//! click on an entry reaches it.

use std::cell::Cell;
use std::sync::{Arc, Mutex, OnceLock};

use civ3_sys::city_lists as sys;

use crate::text::TextRun;
use crate::ui::ScreenDraw;

/// The view ids of list views (one a list shown, in order).
pub const FIRST_VIEW: u32 = 0x1_0000;

/// The face and size of the improvements list's font (the city screen
/// sets it with `get_font(10, 0)`, 0x421D30), its colour unselected.
const FACE: &str = crate::city_labels::FACE;
const FONT_HEIGHT: i32 = 10;

thread_local! {
    /// The list object whose entry the game's thread is drawing (0: none).
    static DRAWING: Cell<u32> = const { Cell::new(0) };
}

static FUNCTIONS: OnceLock<sys::ImprovementLists> = OnceLock::new();
static LATEST: Mutex<Option<Arc<Vec<ListContent>>>> = Mutex::new(None);

/// Whether lists are shown as views of their own (`CIV3R_LIST_VIEWS=off`:
/// the game's own rows, as before).
fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("CIV3R_LIST_VIEWS").as_deref() != Ok("off"))
}

/// A list shown whole: its rows in content pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct ListContent {
    /// The list object G (the `ListView`'s id).
    pub list: u32,
    /// The rows' rectangle on the driver's screen (x, y, w, h).
    pub rect: [i32; 4],
    /// An entry's height with the gap after it, how many there are.
    pub step: i32,
    pub count: i32,
    /// The entry the game shows first.
    pub first: i32,
    /// Every entry's draws, in content pixels (shared while they stay the
    /// same: the publisher makes their sprites again only when they change).
    pub draws: Arc<Vec<ScreenDraw>>,
}

/// The entry draw hook, kept installed.
pub struct Installed(#[allow(dead_code)] sys::entry_draw::Installed);

/// Hooks the entry draw, if the build's lists are decoded.
pub fn install(build: civ3_sys::Build) -> Result<Option<Installed>, inproc::hook::HookError> {
    if !enabled() {
        return Ok(None);
    }
    let Some(functions) = sys::ImprovementLists::find(build)? else { return Ok(None) };
    let _ = FUNCTIONS.set(functions);
    let hook = sys::entry_draw::install_inline(sys::ENTRY_DRAW, sys::ENTRY_DRAW_PROLOGUE, |c| {
        let previous = DRAWING.replace(c.this);
        c.original();
        DRAWING.set(previous);
    })?;
    log::info!("list views: the city screen's improvements are drawn from the game's data and scrolled by the viewer");
    Ok(Some(Installed(hook)))
}

/// Whether the game's thread is drawing a row of a list shown as a view of
/// its own (the mirror leaves it out).
pub fn drawing_shown_row() -> bool {
    let list = DRAWING.with(Cell::get);
    list != 0 && FUNCTIONS.get().is_some() && list == sys::IMPROVEMENT_LIST
}

/// The lists as the game's thread last made them.
pub fn latest() -> Option<Arc<Vec<ListContent>>> {
    LATEST.lock().ok()?.clone()
}

/// On the game's thread, at a present: the rows of the lists shown.
pub fn present() {
    let Some(functions) = FUNCTIONS.get() else { return };
    let mut lists: Vec<ListContent> = functions.read().and_then(|l| improvement_content(&l)).into_iter().collect();
    if let Ok(mut latest) = LATEST.lock() {
        // (The draws of a list that kept them stay the same `Arc`.)
        if let Some(old) = latest.as_deref() {
            for l in &mut lists {
                if let Some(o) = old.iter().find(|o| o.list == l.list && o.draws == l.draws) {
                    l.draws = o.draws.clone();
                }
            }
        }
        if latest.as_deref() != Some(&lists) {
            *latest = Some(Arc::new(lists));
        }
    }
}

fn sprite(s: sys::SpriteInfo) -> civ3_ui::list_rows::SpriteInfo {
    civ3_ui::list_rows::SpriteInfo { addr: s.addr, width3: s.width3, width: s.width, height: s.height, height2: s.height2 }
}

/// A game sprite drawn at (x, y) of a canvas, as the mirror shows a
/// `Sprite_draw` (its crop offset added, its pixels where the mirror knows
/// them to be from), within `clip`.
#[cfg(not(windows))]
fn sprite_draw(_sprite: u32, _x: i32, _y: i32, _clip: [i32; 4]) -> Option<ScreenDraw> {
    None
}

#[cfg(windows)]
fn sprite_draw(sprite: u32, x: i32, y: i32, clip: [i32; 4]) -> Option<ScreenDraw> {
    let (cx, cy) = civ3_sys::ui_hooks::obj::sprite_crop(sprite).map_or((0, 0), |c| (c.0, c.1));
    let source = crate::ui_trace::sprite_sources(&[sprite]).into_iter().next().flatten()?;
    let (x, y) = (x + cx, y + cy);
    Some(match source {
        crate::ui::Source::File { retained, path, rect, mask, palette, opacity } => ScreenDraw::Image { retained, path, rect, x, y, keyed: true, clip, mask, palette, opacity },
        crate::ui::Source::Indexed { retained, path, rect, palette, indices } => ScreenDraw::Indexed { retained, path, rect, x, y, clip, palette, indices },
        crate::ui::Source::Surface { .. } => return None,
    })
}

/// The improvements list's rows, every entry.
fn improvement_content(l: &sys::ImprovementList) -> Option<ListContent> {
    let layout = l.list.layout;
    let step = layout.height + layout.gap;
    let width = layout.width - 2 * layout.x0;
    let canvas_width = civ3_sys::ui_hooks::obj::image_size(l.canvas).map_or(l.list.rect[2], |s| s.0);
    let icons = civ3_ui::list_rows::ImprovementIcons { upkeep: sprite(l.icons.upkeep), tourism: sprite(l.icons.tourism), culture: sprite(l.icons.culture), happy: sprite(l.icons.happy) };
    let mut draws = Vec::new();
    for (k, row) in l.rows.iter().enumerate() {
        let Some(row) = row else { continue };
        let top = k as i32 * step;
        let rect = [0, top, width, top + layout.height];
        let clip = [0, top, canvas_width, top + step];
        let row = civ3_ui::list_rows::ImprovementRow {
            improvement: row.improvement,
            small: sprite(row.small),
            name: row.name.clone(),
            upkeep: row.upkeep,
            tourism: row.tourism.clone(),
            culture: row.culture,
            happy: row.happy,
        };
        for d in civ3_ui::list_rows::improvement_row(&row, &icons, canvas_width, rect, l.text_cell) {
            match d {
                civ3_ui::list_rows::RowDraw::Sprite { sprite, x, y } => draws.extend(sprite_draw(sprite, x, y, clip)),
                civ3_ui::list_rows::RowDraw::Text { text, x, baseline } => draws.push(ScreenDraw::Text {
                    run: TextRun { x, y: baseline, color: [0, 0, 0], face: FACE.into(), height: -FONT_HEIGHT, weight: 0, italic: false, underline: false, text, backdrop: None },
                    clip: [0, top, width, top + layout.height],
                }),
            }
        }
    }
    let [fx, fy, _, fh] = l.list.rect;
    Some(ListContent {
        list: l.list.list,
        rect: [fx + layout.x0, fy + layout.y0, width, (fh - layout.y0).max(step)],
        step,
        count: l.list.count,
        first: l.list.first,
        draws: Arc::new(draws),
    })
}

/// The game's list `list` shows from entry `first`: wheel turns over it
/// (`Base_List_Box`'s wheel moves the first entry by one per message, up
/// for a positive delta), as many as the viewer's position is away from
/// the game's; where the list is in the game's pixels.
pub fn scroll_to(list: u32, offset: i32) -> Option<(i32, (i32, i32))> {
    let lists = latest()?;
    let l = lists.iter().find(|l| l.list == list)?;
    let target = (offset / l.step.max(1)).clamp(0, (l.count - 1).max(0));
    let notches = l.first - target;
    (notches != 0).then(|| (notches, (l.rect[0] + l.rect[2] / 2, l.rect[1] + l.rect[3] / 2)))
}

/// What the publisher's items code makes the sprites of a list from: its
/// draws as a frame of their own.
pub fn frame(l: &ListContent) -> crate::ui::Frame {
    crate::ui::Frame { form_draws: vec![(0, 0..l.draws.len())], draws: l.draws.as_ref().clone(), ..Default::default() }
}

/// The list views of `lists`, over the screen's view `over`, with the
/// sprites made of each one's [`frame`].
pub fn views(scene: &mut scene::Scene, lists: &[ListContent], over: u32, sprites: &[Vec<scene::FreeSprite>]) {
    for (i, (l, sprites)) in lists.iter().zip(sprites).enumerate() {
        let overlay = sprites.iter().map(|s| scene::FreeSprite { screen: false, after_pass: u32::MAX, ..s.clone() }).collect();
        let v = scene.view_mut(FIRST_VIEW + i as u32);
        v.view = scene::View {
            origin: (l.rect[0], l.rect[1]),
            viewport: (l.rect[2].max(1) as u32, l.rect[3].max(1) as u32),
            interface_only: true,
            camera_y: l.first * l.step,
            list: Some(scene::ListView { id: u64::from(l.list), over, count: l.count.max(0) as u32, step: l.step.max(1) as u32, published: (0, l.count.max(0) as u32) }),
            ..Default::default()
        };
        v.overlay = overlay;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(first: i32) -> ListContent {
        ListContent { list: sys::IMPROVEMENT_LIST, rect: [450, 684, 140, 244], step: 32, count: 12, first, draws: Arc::new(Vec::new()) }
    }

    #[test]
    fn the_games_list_is_turned_to_the_viewers_entry() {
        *LATEST.lock().unwrap() = Some(Arc::new(vec![list(2)]));
        // The viewer at 5 entries and a bit: three notches down (negative).
        assert_eq!(scroll_to(sys::IMPROVEMENT_LIST, 5 * 32 + 20), Some((-3, (520, 806))));
        // At the game's entry: nothing.
        assert_eq!(scroll_to(sys::IMPROVEMENT_LIST, 2 * 32 + 31), None);
        // Up: positive.
        assert_eq!(scroll_to(sys::IMPROVEMENT_LIST, 0), Some((2, (520, 806))));
        // Another list: not this one's.
        assert_eq!(scroll_to(1, 0), None);
    }

    #[test]
    fn a_list_view_holds_every_entry_over_its_screen() {
        let mut scene = scene::Scene::default();
        let mut l = list(3);
        l.draws = Arc::new(vec![ScreenDraw::Fill { rect: [0, 3 * 32, 10, 10], rgb: [1, 2, 3], alpha: 255, clip: [0, 96, 140, 128] }]);
        assert_eq!(frame(&l).draws.len(), 1);
        views(&mut scene, &[l], 258, &[vec![scene::FreeSprite { screen: true, ..Default::default() }]]);
        let v = scene.view(FIRST_VIEW).unwrap();
        assert_eq!((v.view.origin, v.view.viewport, v.view.camera_y), ((450, 684), (140, 244), 96));
        assert_eq!(v.view.list, Some(scene::ListView { id: u64::from(sys::IMPROVEMENT_LIST), over: 258, count: 12, step: 32, published: (0, 12) }));
        assert!(v.overlay.iter().all(|s| !s.screen), "content space, moved by the view's camera");
    }
}
