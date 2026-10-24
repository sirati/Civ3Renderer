//! The histograph (F8) in a running game, as the game composes it: unlike
//! the advisors (F1..F7: forms of their 1024×768 board), a form of the
//! whole window, cleared black when its canvas is made (the clear the
//! mirror records, `ui_trace`), its board in the middle. From the frame to
//! the viewer's window: placed and sized as an advisor, in a view and
//! floating, at every window size, pixel and base scale, menu share and
//! split.

use super::*;
use crate::ui::ScreenDraw;
use tile_viewer::{layout, scaling};
use viewer_settings::{Place, Resolution, ScreenPlace, Split};

const ADVISOR: u32 = 0x66_C270;
const HISTOGRAPH: u32 = 0x66_D380;

fn fill_rgb([x, y, w, h]: [i32; 4], rgb: [u8; 3], size: (i32, i32)) -> ScreenDraw {
    ScreenDraw::Fill { rect: [x, y, w, h], rgb, alpha: 255, clip: [0, 0, size.0, size.1] }
}

/// The 1024×768 board in the middle of a game's window of `size`.
fn board(size: (i32, i32)) -> [i32; 4] {
    [(size.0 - 1024) / 2, (size.1 - 768) / 2, 1024, 768]
}

/// The map screen of a game at `size` with screen `screen` (its class)
/// open: the map's canvas (black), `Main_GUI`'s two panels, a corner
/// button, then the screen's form, its canvas's clear, its board and its
/// close button (a form of its own).
fn game(size: (i32, i32), screen: u32) -> Frame {
    let [bx, by, bw, bh] = board(size);
    let black = [0, 0, 0];
    let art = [200, 180, 140];
    let (screen_place, screen_size) = if screen == HISTOGRAPH { ((0, 0), size) } else { ((bx, by), (bw, bh)) };
    let forms = vec![
        (0x66_B3D8, 0x9F_8934, (0, 0), size),
        (0x67_1218, 0x9F_D80C, (0, 0), size),
        (0x66_B570, 0x9F_DD84, (0, 0), size),
        (0x66_56C0, 0xA0_68B4, (21, 12), (36, 30)),
        (screen, 0xB3_047C, screen_place, screen_size),
        (0x66_56C0, 0xB3_1E34, (bx + 952, by + 718), (72, 48)),
    ];
    let draws = vec![
        fill_rgb([0, 0, size.0, size.1], black, size),
        fill_rgb([5, size.1 - 131, 282, 131], art, size),
        fill_rgb([size.0 - 299, size.1 - 138, 294, 137], art, size),
        fill_rgb([21, 12, 36, 30], art, size),
        fill_rgb([screen_place.0, screen_place.1, screen_size.0, screen_size.1], black, size),
        fill_rgb([bx, by, bw, bh], art, size),
        fill_rgb([bx + 952, by + 718, 72, 48], art, size),
    ];
    let form_draws = vec![(0x9F_8934, 0..1), (0x9F_DD84, 1..3), (0xA0_68B4, 3..4), (0xB3_047C, 4..6), (0xB3_1E34, 6..7)];
    Frame { overlay_draws: 0..0, draws, forms, form_draws, underlay_draws: vec![] }
}

#[test]
fn the_histograph_in_a_game_is_its_board_as_an_advisor_is() {
    let build = civ3_sys::Build::PcGames;
    for size in [(1920, 1080), (3274, 2048), (1024, 768)] {
        for (screen, name) in [(ADVISOR, "F1"), (HISTOGRAPH, "F8")] {
            let f = game(size, screen);
            assert_eq!(active_screen(&f, build), Some((4, name)));
            let part = active_screen_part(&f, 4, build, size);
            // (In a game window of the board's size: no part, as a screen
            // of the whole window is none.)
            if size == (1024, 768) {
                assert_eq!(part, None, "{name}");
                continue;
            }
            let (first, rect) = part.unwrap_or_else(|| panic!("{name} at {size:?}: no part"));
            assert_eq!((first, rect), (4, board(size)), "{name} at {size:?}");
            // Nothing of the part outside its board: the clear of the
            // whole window is cut to it.
            let (_, above) = split_at(&f, first);
            for d in &clipped(&above, rect).draws {
                let r = reach(d).unwrap();
                assert!(r[0] >= rect[0] && r[1] >= rect[1] && r[2] <= rect[0] + rect[2] && r[3] <= rect[1] + rect[3], "{name} at {size:?}: {r:?}");
            }
        }
    }
}

/// The scene live makes of `frame` (a game at `size` with a screen open):
/// the world, `Main_GUI`'s panels in the interface, the screen's part a
/// view of its own (`active_screen_part`), or, without a part, all over the
/// frame a menu. Each sprite's rectangle is the draw's (its set an index
/// into the rectangles returned).
fn scene_of(frame: &Frame, size: (i32, i32)) -> (scene::Scene, Vec<[i32; 4]>) {
    use scene::{View, ViewState};
    let build = civ3_sys::Build::PcGames;
    let mut rects = Vec::new();
    let mut sprites = |f: &Frame| -> Vec<scene::FreeSprite> {
        f.draws.iter().filter_map(reach).map(|[l, t, r, b]| {
            rects.push([l, t, r - l, b - t]);
            scene::FreeSprite { set: rects.len() as u32 - 1, x: l, y: t, screen: true, after_pass: u32::MAX, ..Default::default() }
        }).collect()
    };
    let (index, name) = active_screen(frame, build).unwrap();
    let part = active_screen_part(frame, index, build, size);
    let (below, above) = match part {
        Some((first, rect)) => {
            let (below, above) = split_at(frame, first);
            (below, Some((rect, clipped(&above, rect))))
        }
        None => (frame.clone(), None),
    };
    // (The map's black is the world's place.)
    let (hud, menus) = crate::ui_screen::split(&without(&below, &[0]), crate::ui_screen::menus_from(&below));
    let mut scene = scene::Scene {
        views: vec![
            ViewState { id: 0, view: View { camera_x: 1000, camera_y: 500, zoom: (1, 1), ..Default::default() }, overlay: vec![] },
            ViewState { id: crate::scene_map::INTERFACE_VIEW, view: View { interface_only: true, ..Default::default() }, overlay: sprites(&hud) },
        ],
        shape: scene::WorldShape { cols: 60, rows: 62, wrap_x: true, wrap_y: false },
        interface: scene::Interface { screen: Some(name.into()), screen_role: civ3_ui::screens::role(name), ..Default::default() },
        screen_size: (size.0 as u32, size.1 as u32),
        ..Default::default()
    };
    match above {
        Some((rect, above)) => {
            let overlay = sprites(&above);
            part_view(&mut scene, SCREEN_VIEW, rect, overlay);
            scene.interface.screen_view = Some(SCREEN_VIEW);
        }
        None => crate::scene_map::overlay_menus(&mut scene, sprites(&menus)),
    }
    (scene, rects)
}

#[test]
fn the_histograph_in_a_game_is_placed_and_sized_as_an_advisor_in_a_view_and_floating() {
    for size in [(1920, 1080), (3274, 2048)] {
        for window in [(3840, 2160), (3258, 1904), (2880, 1800), (1920, 1080)] {
            for pixel_scale in [1, 2] {
                for base_ui_scale in [1.0, 2.0] {
                    for (scale_menus, menu_scale_percent) in [(false, 100.0), (true, 100.0), (true, 60.0)] {
                        for split in [Split::Standard, Split::Vertical1To1, Split::Horizontal1To1] {
                            for place in [Place::Floating, Place::LeftOrBottom, Place::RightOrTop] {
                                let resolution = Resolution { pixel_scale, base_ui_scale, scale_menus, menu_scale_percent, ..Default::default() };
                                let what = format!("game {size:?} in {window:?} ×{pixel_scale} base {base_ui_scale} menus {scale_menus}/{menu_scale_percent} {split:?} {place:?}");
                                let shown = |screen: u32| {
                                    let (scene, rects) = scene_of(&game(size, screen), size);
                                    let rect_of = |s: &scene::FreeSprite| rects.get(s.set as usize).copied();
                                    let p = scaling::plan(window, scene.screen_size, &resolution).for_scene(&scene, rect_of);
                                    let mut layout = layout::Layout::default();
                                    layout.apply_settings(&viewer_settings::Layout { split, screen_place: ScreenPlace { vertical: place, horizontal: place }, ..Default::default() });
                                    layout.driver_screen = Some(p.screen);
                                    layout.menu_scale = p.menu.zoom;
                                    layout.hud_scale = p.frame;
                                    layout.window_pixel_scale = p.factor();
                                    layout.screen_content = scene.interface.screen_view.and_then(|id| scene.view(id))
                                        .map(|v| [v.view.origin.0, v.view.origin.1, v.view.viewport.0 as i32, v.view.viewport.1 as i32]);
                                    let mut frame = layout.frame(&scene, p.canvas(), (0, 0, 0, 0), vec![], None);
                                    let arranged = scaling::arrange_panned(&p, &scene, &mut frame, &mut layout.menu_pan, rect_of);
                                    layout.set_interface_shown(arranged.shown);
                                    let [x, y, w, h] = layout.driver_rect_to_window(board(size));
                                    let worlds: Vec<[i32; 4]> = frame.cameras.iter().filter(|k| k.cells && !k.stacked)
                                        .map(|k| [k.origin.0, k.origin.1, k.viewport.0 as i32, k.viewport.1 as i32]).collect();
                                    ([x.round() as i32, y.round() as i32, w.round() as i32, h.round() as i32], worlds, p.canvas())
                                };
                                let (advisor, _, canvas) = shown(ADVISOR);
                                let (histograph, worlds, _) = shown(HISTOGRAPH);
                                assert_eq!(histograph, advisor, "{what}");
                                // Never larger than its place: in a view, that
                                // view (the world under it is the view's); floating,
                                // the canvas (a screen larger than the canvas at
                                // a large base scale is panned, as menus are).
                                let [x, y, w, h] = histograph;
                                let in_view = split != Split::Standard && place != Place::Floating;
                                let fits = w <= canvas.0 as i32 && h <= canvas.1 as i32;
                                if in_view || fits {
                                    assert!(x >= 0 && y >= 0 && x + w <= canvas.0 as i32 && y + h <= canvas.1 as i32, "{what}: {histograph:?} in {canvas:?}");
                                }
                                if in_view {
                                    let within = |o: &[i32; 4]| x >= o[0] && y >= o[1] && x + w <= o[0] + o[2] && y + h <= o[1] + o[3];
                                    assert!(worlds.len() >= 2 && worlds.iter().any(within), "{what}: {histograph:?} in none of the views {worlds:?}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
