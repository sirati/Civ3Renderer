//! The city screen's view of the map (docs §16.8), kept consistent with the
//! interface it is shown with.
//!
//! The game frames the city's surroundings with its main map camera
//! (`City_Form_open` brings the tile two rows south of the city into the
//! middle of the map viewport) and draws the screen's map overlay (the work
//! area's outline, the worked tiles' yields, the tile frame under the
//! pointer) relative to that camera. Our view does not copy the game's
//! camera: it always centres the city as the city screen intends, also at
//! the edges of a map that does not wrap, where the game keeps its camera on
//! the map (we can draw beyond it). What the game drew over its map moves by
//! the difference ([`Anchor::shift`]), and the pointer over the map moves
//! back by it before the game sees it ([`Anchor::to_game`]), so the world,
//! the overlay and the clicks agree.
//!
//! The mirrored interface is the screen the game last presented; the world
//! is read afterwards. The city in focus and the game camera are taken as
//! they were when the screen was presented ([`screen_state`]): a game thread
//! stalled between the two must not show one city's map, fog or overlay
//! under another city's screen.

use crate::ui::{Frame, ScreenDraw};

/// What the game showed when it presented a screen: the city in focus (the
/// city screen's) and its main map camera (full-size world pixels, the
/// city screen forces the normal zoom).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Presented {
    pub focus_city: Option<i32>,
    pub camera: (i32, i32),
}

/// The city screen state to compose: that of the screen the game presented
/// (`presented`), else (no mirror) the world's as read (its focus city
/// `world_focus`, the game camera `world_camera`).
pub fn screen_state(world_focus: Option<i32>, world_camera: (i32, i32), presented: Option<Presented>) -> Presented {
    presented.unwrap_or(Presented { focus_city: world_focus, camera: world_camera })
}

/// The camera (window top-left, full-size world pixels) that shows city
/// tile `city` as the city screen frames it on a game window of `screen`
/// pixels: the tile two rows south of it in the middle, i.e. the city
/// tile's centre (64·x + 64, 32·y + 32) at (w/2, h/2 − 64). Not clamped
/// to the map.
pub fn camera((x, y): (i32, i32), (w, h): (i32, i32)) -> (i32, i32) {
    (64 * x + 64 - w / 2, 32 * y + 32 - (h / 2 - 64))
}

/// How the city screen's map is shown: our camera, the band of the window
/// the map shows in (x, y, w, h), and how far what the game drew over its
/// map moves.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub camera: (i32, i32),
    pub band: [i32; 4],
    pub shift: (i32, i32),
}

impl Anchor {
    /// City tile `city` on a game window of `screen` pixels, the map shown
    /// in `band`; the game's own camera at `game` (full size). `period`:
    /// the world's size in pixels along the axes it wraps on (0: no wrap),
    /// so a camera the game wrapped differently moves nothing.
    pub fn new(city: (i32, i32), screen: (i32, i32), band: [i32; 4], game: (i32, i32), period: (i32, i32)) -> Self {
        let camera = camera(city, screen);
        let axis = |d: i32, p: i32| if p > 0 { (d + p / 2).rem_euclid(p) - p / 2 } else { d };
        let shift = (axis(game.0 - camera.0, period.0), axis(game.1 - camera.1, period.1));
        Anchor { camera, band, shift }
    }

    fn contains(&self, (x, y): (i32, i32)) -> bool {
        let [bx, by, bw, bh] = self.band;
        x >= bx && y >= by && x < bx + bw && y < by + bh
    }

    /// Window point `p` (as shown) where the game has it: moved back over
    /// the map, elsewhere as it is.
    pub fn to_game(&self, p: (i32, i32)) -> (i32, i32) {
        if self.contains(p) { (p.0 - self.shift.0, p.1 - self.shift.1) } else { p }
    }

    /// Moves what the game drew over its map in `frame` with the map: the
    /// underlays (the map canvas's own labels) and the draws centred in the
    /// band and reaching at most a tile beyond it (not the screen's panels
    /// round it, nor anything as large as the band), kept within the band.
    pub fn apply(&self, frame: &mut Frame) {
        if self.shift == (0, 0) {
            return;
        }
        let [bx, by, bw, bh] = self.band;
        let band = [bx, by, bx + bw, by + bh];
        let (dx, dy) = self.shift;
        let under: Vec<bool> = (0..frame.draws.len()).map(|i| frame.underlay_draws.iter().any(|r| r.contains(&i))).collect();
        for (d, under) in frame.draws.iter_mut().zip(under) {
            let Some(r) = reach(d) else { continue };
            let (cx, cy) = ((r[0] + r[2]) / 2, (r[1] + r[3]) / 2);
            let within = cx >= band[0] && cy >= band[1] && cx < band[2] && cy < band[3]
                && r[0] >= band[0] - REACH && r[1] >= band[1] - REACH && r[2] <= band[2] + REACH && r[3] <= band[3] + REACH
                && r[2] - r[0] < bw && r[3] - r[1] < bh;
            if !under && !within {
                continue;
            }
            let clip = match d {
                ScreenDraw::Image { x, y, clip, .. } | ScreenDraw::ResizedImage { x, y, clip, .. } | ScreenDraw::Surface { x, y, clip, .. } | ScreenDraw::Flc { x, y, clip, .. }
                | ScreenDraw::Indexed { x, y, clip, .. } => {
                    (*x, *y) = (*x + dx, *y + dy);
                    clip
                }
                ScreenDraw::Fill { rect, clip, .. } => {
                    (rect[0], rect[1]) = (rect[0] + dx, rect[1] + dy);
                    clip
                }
                ScreenDraw::Text { run, clip } => {
                    (run.x, run.y) = (run.x + dx, run.y + dy);
                    clip
                }
            };
            let moved = [clip[0].saturating_add(dx), clip[1].saturating_add(dy), clip[2].saturating_add(dx), clip[3].saturating_add(dy)];
            *clip = if under { moved } else { [moved[0].max(band[0]), moved[1].max(band[1]), moved[2].min(band[2]), moved[3].min(band[3])] };
        }
    }
}

/// How far beyond the map's band what the game drew over its map reaches
/// at most: a tile.
const REACH: i32 = 64;

/// The rectangle (x0, y0, x1, y1) draw `d` covers, clipped (a text by its
/// pen's point).
fn reach(d: &ScreenDraw) -> Option<[i32; 4]> {
    let (r, clip) = match d {
        ScreenDraw::Image { rect, x, y, clip, .. } | ScreenDraw::Surface { rect, x, y, clip, .. } => ([*x, *y, x + rect[2], y + rect[3]], clip),
        ScreenDraw::Indexed { rect, x, y, clip, indices, .. } => { let (w, h) = indices.size(*rect); ([*x, *y, x + w, y + h], clip) },
        ScreenDraw::ResizedImage { x, y, size: (w, h), clip, .. } => ([*x, *y, x + w, y + h], clip),
        ScreenDraw::Fill { rect, clip, .. } => ([rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]], clip),
        ScreenDraw::Text { run, clip } => ([run.x, run.y - 1, run.x + 1, run.y], clip),
        ScreenDraw::Flc { .. } => return None,
    };
    let r = [r[0].max(clip[0]), r[1].max(clip[1]), r[2].min(clip[2]), r[3].min(clip[3])];
    (r[2] > r[0] && r[3] > r[1]).then_some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (i32, i32) = (1920, 1080);
    const BAND: [i32; 4] = [448, 306, 1024, 320];

    fn icon(x: i32, y: i32) -> ScreenDraw {
        ScreenDraw::Image { path: "Art/City Screen/CityIcons.pcx".into(), rect: [0, 0, 16, 16], x, y, keyed: true, clip: [0, 0, 1920, 1080], mask: None, palette: None, opacity: 255 }
    }

    /// Where the game's tile → screen conversion (0x4E3BD0, plus half a
    /// tile) puts tile `t`'s centre with its camera at `game`.
    fn game_centre(t: (i32, i32), game: (i32, i32), period: i32) -> (i32, i32) {
        let x = 64 * t.0 + 64 - game.0;
        (if period > 0 { (x + period / 2).rem_euclid(period) - period / 2 } else { x }, 32 * t.1 + 32 - game.1)
    }

    #[test]
    fn the_city_tile_is_where_the_city_screen_puts_it_for_every_row_parity_zoom_and_seam() {
        // Width 100 (x coordinates), wrapping; height 60, not wrapping.
        let period = (64 * 100, 0);
        for city in [(10, 20), (11, 21), (0, 30), (1, 31), (98, 2), (99, 57), (50, 0), (51, 59)] {
            let ours = camera(city, SCREEN);
            assert_eq!(game_centre(city, ours, period.0), (960, 476), "{city:?}");
            // The game's camera (unclamped, or wrapped by a period), and
            // clamped onto a map that does not wrap.
            for game in [ours, (ours.0 + period.0, ours.1), (ours.0 - period.0, ours.1), (ours.0.max(0), ours.1.max(0)), (ours.0, ours.1.min(60 * 32 - 1080))] {
                let a = Anchor::new(city, SCREEN, BAND, game, period);
                // An icon the game centred on the city tile shows on our
                // city tile, and a click there reaches the game's.
                let at = game_centre(city, game, period.0);
                let mut f = Frame { draws: vec![icon(at.0 - 8, at.1 - 8)], ..Default::default() };
                a.apply(&mut f);
                let ScreenDraw::Image { x, y, .. } = &f.draws[0] else { unreachable!() };
                if inside(BAND, at) {
                    assert_eq!((x + 8, y + 8), (960, 476), "{city:?}, game camera {game:?}");
                }
                let ours_shown = a.to_game((960, 476));
                assert_eq!(ours_shown, (960 - a.shift.0, 476 - a.shift.1));
                assert_eq!(game_centre(city, game, period.0), ours_shown, "{city:?}, game camera {game:?}");
            }
        }
        // A wrapped camera moves nothing.
        let a = Anchor::new((1, 1), SCREEN, BAND, (camera((1, 1), SCREEN).0 + period.0, camera((1, 1), SCREEN).1), period);
        assert_eq!(a.shift, (0, 0));
    }

    fn inside([x, y, w, h]: [i32; 4], p: (i32, i32)) -> bool {
        p.0 >= x && p.1 >= y && p.0 < x + w && p.1 < y + h
    }

    #[test]
    fn only_the_maps_overlay_moves_and_stays_within_the_band() {
        let a = Anchor { camera: (0, 0), band: BAND, shift: (64, -32) };
        let background = ScreenDraw::Image { path: "Art/City Screen/background.pcx".into(), rect: [0, 0, 1024, 768], x: 448, y: 156, keyed: true, clip: [0, 0, 1920, 1080], mask: None, palette: None, opacity: 255 };
        let button = icon(1353, 672);
        let edge = icon(1460, 400);
        let label = icon(10, 10);
        let mut f = Frame { draws: vec![label.clone(), background.clone(), button.clone(), icon(900, 400), edge], underlay_draws: vec![0..1], ..Default::default() };
        a.apply(&mut f);
        assert_eq!(f.draws[1], background);
        assert_eq!(f.draws[2], button);
        let ScreenDraw::Image { x, y, .. } = f.draws[0] else { unreachable!() };
        assert_eq!((x, y), (74, -22), "the map's labels go with the map");
        let ScreenDraw::Image { x, y, clip, .. } = f.draws[3] else { unreachable!() };
        assert_eq!((x, y, clip), (964, 368, [448, 306, 1472, 626]));
        let ScreenDraw::Image { x, clip, .. } = f.draws[4] else { unreachable!() };
        assert_eq!((x, clip[2]), (1524, 1472), "moved past the band's edge it is cut there");
        assert_eq!(a.to_game((500, 400)), (436, 432));
        assert_eq!(a.to_game((500, 700)), (500, 700), "the panel below the map takes the pointer where it is");
    }

    #[test]
    fn a_world_and_an_interface_of_different_cities_are_not_composed() {
        // City A (id 3) at (10, 20), city B (id 4) at (31, 25). The world
        // was read while the game thread was switching from A to B (or
        // stalled there): it still names A, with A's camera; the screen
        // the game presented is B's, drawn with B's camera.
        let (a, b) = ((10, 20), (31, 25));
        let presented_b = Presented { focus_city: Some(4), camera: camera(b, SCREEN) };
        let state = screen_state(Some(3), camera(a, SCREEN), Some(presented_b));
        assert_eq!(state, presented_b, "B's map and fog under B's screen, never A's");
        let anchor = Anchor::new(b, SCREEN, BAND, state.camera, (6400, 0));
        assert_eq!((anchor.camera, anchor.shift), (camera(b, SCREEN), (0, 0)));
        // B's overlay (centred on B's tile by the game) stays on B's tile.
        let mut f = Frame { draws: vec![icon(952, 468)], ..Default::default() };
        anchor.apply(&mut f);
        assert_eq!(f.draws, [icon(952, 468)]);
        // The map's own frame presented while the world already has A open
        // (the screen not shown yet): no city screen state at all.
        let state = screen_state(Some(3), camera(a, SCREEN), Some(Presented { focus_city: None, camera: (0, 0) }));
        assert_eq!(state.focus_city, None, "no city fog without its screen");
        // A closed screen still shown: its city, with its camera.
        assert_eq!(screen_state(None, (0, 0), Some(presented_b)), presented_b);
        // No mirror: the world as read.
        assert_eq!(screen_state(Some(3), (5, 6), None), Presented { focus_city: Some(3), camera: (5, 6) });
    }
}

/// Native City_Form::handle_left_click (0x420500) converts a map click to
/// a work-area index; City::0x4BB9E0 removes an existing worker, or
/// autoassigns on index zero. These are tile actions, independent of yield
/// images (C3X districts can suppress those images).
pub fn worked_tile_targets(world: &crate::world::Civ3World, frame: &Frame, anchor: Anchor, modal_canvas: Option<u32>) -> Vec<scene::Interactable> {
    tile_targets(world, frame, anchor, modal_canvas, None)
}

pub fn tile_targets(world: &crate::world::Civ3World, frame: &Frame, anchor: Anchor, modal_canvas: Option<u32>, eligible: Option<&[bool]>) -> Vec<scene::Interactable> {
    let Some(city) = world.focus_city.and_then(|id| world.city(id)) else { return vec![] };
    if city.civ != world.player && world.debug_mode_bits & 8 == 0 {
        return vec![];
    }
    let Some(background) = frame.draws.iter().position(|d| matches!(d, ScreenDraw::Image { path, .. }
        if path.to_ascii_lowercase().replace('/', "\\").ends_with("city screen\\background.pcx"))) else { return vec![] };
    let ScreenDraw::Image { x, y, rect, clip, .. } = &frame.draws[background] else { unreachable!() };
    let visible = [(*x).max(clip[0]), (*y).max(clip[1]), (x + rect[2]).min(clip[2]), (y + rect[3]).min(clip[3])];
    let Some((canvas, _)) = frame.form_draws.iter().find(|(_, r)| r.contains(&background)) else { return vec![] };
    if modal_canvas.is_some_and(|m| m != *canvas) {
        return vec![];
    }
    world.district_configs.work_area().iter().enumerate().filter_map(|(index, &(dx, dy))| {
        let (x, y) = (city.x + i32::from(dx), city.y + i32::from(dy));
        let tile = world.tile(x, y)?;
        if index != 0 && i32::from(tile.city_area) != city.id && !(tile.city_area == -1 && eligible.is_some_and(|e| e.get(index) == Some(&true))) {
            return None;
        }
        // Use the local, unwrapped work-area offset so seam tiles stay next
        // to this city. The existing city input inverse restores the native
        // camera coordinates before the ordinary mouse event reaches Civ3.
        let (cx, cy) = (64 * x + 64 - anchor.camera.0, 32 * y + 32 - anchor.camera.1);
        if !anchor.contains((cx, cy)) || cx < visible[0] || cy < visible[1] || cx >= visible[2] || cy >= visible[3] {
            return None;
        }
        let [bx, by, bw, bh] = anchor.band;
        // An interior rectangle of the diamond; every advertised point is
        // inside this tile. Keep the click itself at the exact tile centre.
        let (l, t, r, b) = ((cx - 32).max(bx).max(visible[0]), (cy - 16).max(by).max(visible[1]), (cx + 32).min(bx + bw).min(visible[2]), (cy + 16).min(by + bh).min(visible[3]));
        Some(scene::Interactable {
            rect: [l, t, r - l, b - t], anchor: Some([cx, cy]), alt_right_click: false,
            kind: scene::InteractableKind::Button,
            id: (u64::from(*canvas) << 32) | 0x0F00_0000 | index as u64,
        })
    }).collect()
}

#[cfg(test)]
mod worked_targets_tests {
    use super::*;
    use crate::world::{Civ3City, Civ3Tile, Civ3World};

    fn fixture() -> (Civ3World, Frame, Anchor) {
        let mut world = Civ3World { width: 40, height: 30, player: 1, focus_city: Some(7),
            cities: vec![Civ3City { id: 7, x: 10, y: 10, civ: 1, ..Default::default() }],
            tiles: vec![Civ3Tile { city_area: -1, ..Default::default() }; 600], ..Default::default() };
        let i = world.index(12, 10);
        world.tiles[i].city_area = 7;
        let frame = Frame { draws: vec![ScreenDraw::Image { path: "Art/City Screen/background.pcx".into(), rect: [0, 0, 1024, 768], x: 448, y: 156,
            keyed: true, clip: [0, 0, 1920, 1080], mask: None, palette: None, opacity: 255 }], form_draws: vec![(0x740C34, 0..1)], ..Default::default() };
        let anchor = Anchor::new((10, 10), (1920, 1080), [448, 306, 1024, 320], (0, 0), (0, 0));
        (world, frame, anchor)
    }

    #[test]
    fn actual_worked_tiles_and_centre_use_native_clicks_after_camera_inverse() {
        let (world, frame, anchor) = fixture();
        let targets = worked_tile_targets(&world, &frame, anchor, None);
        assert_eq!(targets.len(), 2, "no yield-image dependency and no unworked or other-city target");
        let centres: Vec<_> = targets.iter().map(|t| t.anchor.unwrap()).collect();
        assert!(centres.contains(&[960, 476]));
        assert!(centres.contains(&[1088, 476]));
        for t in targets {
            assert_eq!(t.kind, scene::InteractableKind::Button);
            assert!(!t.alt_right_click);
            assert_eq!(t.id >> 32, 0x740C34);
            let [x, y] = t.anchor.unwrap();
            let native = anchor.to_game((x, y));
            assert_eq!(native, (x - anchor.shift.0, y - anchor.shift.1));
            assert!(native == (704, 352) || native == (832, 352));
        }
        let moved = Anchor { shift: (123, -75), ..anchor };
        assert_eq!(worked_tile_targets(&world, &frame, moved, None), worked_tile_targets(&world, &frame, anchor, None), "independent city view does not follow native camera");
    }

    #[test]
    fn hidden_modal_foreign_and_outside_tiles_are_not_targets() {
        let (mut world, mut frame, mut anchor) = fixture();
        assert!(worked_tile_targets(&world, &frame, anchor, Some(99)).is_empty());
        assert_eq!(worked_tile_targets(&world, &frame, anchor, Some(0x740C34)).len(), 2);
        world.cities[0].civ = 2;
        assert!(worked_tile_targets(&world, &frame, anchor, None).is_empty());
        world.debug_mode_bits = 8;
        assert_eq!(worked_tile_targets(&world, &frame, anchor, None).len(), 2);
        anchor.band = [448, 306, 550, 320];
        assert_eq!(worked_tile_targets(&world, &frame, anchor, None).len(), 1);
        let ScreenDraw::Image { clip, .. } = &mut frame.draws[0] else { unreachable!() };
        *clip = [0, 0, 1, 1];
        assert!(worked_tile_targets(&world, &frame, anchor, None).is_empty());
        frame.draws.clear();
        assert!(worked_tile_targets(&world, &frame, anchor, None).is_empty());
    }

    #[test]
    fn work_radius_wrap_and_non_wrapping_edges_follow_actual_tiles() {
        let (mut world, frame, _) = fixture();
        world.cities[0].x = 0;
        let anchor = Anchor::new((0, 10), (1920, 1080), [448, 306, 1024, 320], (0, 0), (2560, 0));
        let i = world.index(38, 10);
        world.tiles[i].city_area = 7;
        assert_eq!(worked_tile_targets(&world, &frame, anchor, None).len(), 1);
        world.wrap_x = true;
        let targets = worked_tile_targets(&world, &frame, anchor, None);
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().any(|t| t.anchor == Some([832, 476])));
        world.tiles[i].city_area = 8;
        assert_eq!(worked_tile_targets(&world, &frame, anchor, None).len(), 1);
    }
}

#[cfg(any(test, all(windows, target_arch = "x86")))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct WorkIdentity {
    map: (u32, i32, i32, u32), player: i32,
    city: (u32, i32, i32, i32, i32),
}
#[cfg(any(test, all(windows, target_arch = "x86")))]
struct WorkSnapshot { identity: WorkIdentity, epoch: u64, eligible: Vec<bool> }
#[cfg(any(test, all(windows, target_arch = "x86")))]
impl WorkSnapshot {
    fn agrees(&self, current: &WorkIdentity, world: &crate::world::Civ3World, epoch: u64) -> bool {
        let Some(city) = world.focus_city.and_then(|id| world.city(id)) else { return false; };
        self.epoch == epoch && self.identity == *current && self.identity.player == world.player
            && self.identity.map.1 == world.width && self.identity.map.2 == world.height && self.identity.map.3 == world.seed
            && self.identity.city.1 == city.id && self.identity.city.2 == city.x && self.identity.city.3 == city.y && self.identity.city.4 == city.civ
    }
}

/// Eligibility captured by a native city paint, then made visible only
/// when the matching native screen is presented. Viewer frames never call
/// the getter. Mouse actions clear both copies before dispatch.
#[cfg(all(windows, target_arch = "x86"))]
pub mod native {
    use civ3_sys::{Civ3, layout as L};
    use inproc::Ptr32;
    use std::sync::{Arc, atomic::{AtomicU64, Ordering::SeqCst}};

    use super::{WorkIdentity as Identity, WorkSnapshot as Snapshot};
    static EPOCH: AtomicU64 = AtomicU64::new(0);
    static PAINTED: civ3r_sync::Latest<Snapshot> = civ3r_sync::Latest::new();
    static PRESENTED: civ3r_sync::Latest<Snapshot> = civ3r_sync::Latest::new();

    fn identity(game: &Civ3) -> Option<Identity> {
        let map = game.map().ok()?;
        let player = game.view().ok()?.player_civ;
        let slot = civ3_sys::ui_hooks::focus_city_slot(game.build)?;
        let city: Ptr32<u8> = Ptr32::<Ptr32<u8>>::new(slot).read().ok()?;
        if city.is_null() { return None; }
        let body = city.offset::<u8>(L::CITY_BODY).ok()?;
        Some(Identity { map: (map.tiles_address(), map.width, map.height, map.seed), player,
            city: (city.raw(), body.offset::<i32>(L::CITY_BODY_ID).ok()?.read().ok()?,
                body.offset::<i16>(L::CITY_BODY_X).ok()?.read().ok()?.into(),
                body.offset::<i16>(L::CITY_BODY_Y).ok()?.read().ok()?.into(),
                body.offset::<i8>(L::CITY_BODY_CIVID).ok()?.read().ok()?.into()) })
    }

    pub fn invalidate() { EPOCH.fetch_add(1, SeqCst); PAINTED.clear(); PRESENTED.clear(); }

    pub fn present(game: &Civ3) {
        if let Some(snapshot) = PAINTED.load().filter(|s| s.epoch == EPOCH.load(SeqCst) && Some(&s.identity) == identity(game).as_ref()) {
            PRESENTED.publish_arc(snapshot);
        } else { PAINTED.clear(); PRESENTED.clear(); }
    }

    pub struct Eligibility(Arc<Snapshot>);
    impl Eligibility { pub fn tiles(&self) -> &[bool] { &self.0.eligible } }

    /// Memory-only validation on the publisher, including map generation,
    /// player, native city identity and the world being composed.
    pub fn eligibility(game: &Civ3, world: &crate::world::Civ3World) -> Option<Eligibility> {
        let s = PRESENTED.load()?;
        if !s.agrees(&identity(game)?, world, EPOCH.load(SeqCst)) { return None; }
        Some(Eligibility(s))
    }

    pub fn install(game: Civ3) -> Result<Box<dyn std::any::Any + Send>, inproc::hook::HookError> {
        let (controls, draw) = civ3_sys::city_work::sites(game.build).ok_or(inproc::hook::HookError::Unsupported)?;
        let getter = civ3_sys::city_work::controls_tile::Function::at(controls.0, controls.1)?;
        // All 193 indices are bounded by C3X's own wrapper. Vanilla rejects
        // >=21 immediately. This avoids guessing an unsupported mod's radius.
        let hook = civ3_sys::city_work::draw_yields::install_at_start(draw.0, draw.1, move |call| {
            let epoch = EPOCH.load(SeqCst);
            let result = call.original();
            let Some(id) = identity(&game) else { invalidate(); return result; };
            let eligible = (0..193).map(|index| getter.call(id.city.0, index, 1) & 0xFF != 0).collect();
            if identity(&game).as_ref() == Some(&id) { PAINTED.publish(Snapshot { identity: id, epoch, eligible }); }
            else { invalidate(); }
            result
        })?;
        Ok(Box::new(hook))
    }
}

#[cfg(test)]
mod eligible_targets_tests {
    use super::*;
    #[test]
    fn native_eligibility_adds_only_unworked_tiles_and_never_another_citys_worker() {
        use crate::world::{Civ3City, Civ3Tile, Civ3World};
        let mut world = Civ3World { width: 40, height: 30, player: 1, focus_city: Some(7),
            cities: vec![Civ3City { id: 7, x: 10, y: 10, civ: 1, ..Default::default() }],
            tiles: vec![Civ3Tile { city_area: -1, ..Default::default() }; 600], ..Default::default() };
        let frame = Frame { draws: vec![ScreenDraw::Image { path: "Art/City Screen/background.pcx".into(), rect: [0, 0, 1024, 768], x: 448, y: 156,
            keyed: true, clip: [0, 0, 1920, 1080], mask: None, palette: None, opacity: 255 }], form_draws: vec![(0x740C34, 0..1)], ..Default::default() };
        let anchor = Anchor::new((10, 10), (1920, 1080), [448, 306, 1024, 320], (0, 0), (0, 0));
        let mut eligible = vec![false; 21];
        eligible[2] = true; // East tile (12,10), admitted by native controls_tile.
        let got = tile_targets(&world, &frame, anchor, None, Some(&eligible));
        assert_eq!(got.len(), 2);
        assert!(got.iter().any(|t| t.anchor == Some([1088, 476])));
        assert_eq!(tile_targets(&world, &frame, anchor, None, None).len(), 1, "absent or invalidated snapshot never guesses unworked eligibility");
        eligible[2] = false;
        assert_eq!(tile_targets(&world, &frame, anchor, None, Some(&eligible)).len(), 1, "native hostile/disabled rejection is authoritative");
        eligible[2] = true;
        let index = world.index(12, 10);
        world.tiles[index].city_area = 8;
        assert_eq!(tile_targets(&world, &frame, anchor, None, Some(&eligible)).len(), 1, "cannot take another city's worker");
        world.tiles[index].city_area = 7;
        eligible[2] = false;
        assert_eq!(tile_targets(&world, &frame, anchor, None, Some(&eligible)).len(), 2, "native removal path precedes assignment eligibility");
    }
}

#[cfg(test)]
mod eligibility_identity_tests {
    use super::*;
    #[test]
    fn a_capture_rejects_click_invalidation_map_city_and_owner_reuse() {
        use crate::world::{Civ3City, Civ3World};
        let mut world = Civ3World { width: 40, height: 30, seed: 99, player: 1, focus_city: Some(7),
            cities: vec![Civ3City { id: 7, x: 10, y: 10, civ: 1, ..Default::default() }], ..Default::default() };
        let identity = WorkIdentity { map: (0x1000, 40, 30, 99), player: 1, city: (0x2000, 7, 10, 10, 1) };
        let snapshot = WorkSnapshot { identity: identity.clone(), epoch: 42, eligible: vec![true; 21] };
        assert!(snapshot.agrees(&identity, &world, 42));
        assert_eq!(snapshot.eligible.len(), 21);
        assert!(!snapshot.agrees(&identity, &world, 43), "click invalidation rejects racing old paint/present snapshots");
        let mut next = identity.clone(); next.map.0 += 4;
        assert!(!snapshot.agrees(&next, &world, 42), "same map dimensions and seed cannot reuse an old loaded world");
        next = identity.clone(); next.city.0 += 4;
        assert!(!snapshot.agrees(&next, &world, 42), "reused city id cannot reuse an old native city object");
        next = identity.clone(); next.player = 2;
        assert!(!snapshot.agrees(&next, &world, 42));
        world.cities[0].civ = 2;
        assert!(!snapshot.agrees(&identity, &world, 42));
        world.cities[0].civ = 1; world.cities[0].x = 12;
        assert!(!snapshot.agrees(&identity, &world, 42));
        world.cities[0].x = 10; world.focus_city = None;
        assert!(!snapshot.agrees(&identity, &world, 42));
    }
}
