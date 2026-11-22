//! The parts of the game's interface the viewer arranges itself
//! (`scene::Interface`, docs §16.12): which screen is open (the advisors
//! F1-F8, the city screen, the palace, espionage, the game menu, the
//! Civilopedia), the top-left corner's buttons (shown in the view the
//! pointer is in), the open popup (moved into a view where it fits) and the
//! unit's order buttons (reachable from every view).

use crate::ui::Frame;
pub use civ3_ui::interface_parts::*;

/// The tile information box's form class (`open_tile_info`'s object at
/// 0xC950A0, constructed at 0x5AA400).
pub fn tile_info_vtable(build: civ3_sys::Build) -> Option<u32> {
    match build {
        civ3_sys::Build::PcGames => Some(0x66_F5C8),
        _ => None,
    }
}

/// Bit of `Context_Menu` field 0x574 that aligns a menu's right edge to
/// the point it opens at, and that field's offset.
pub const CONTEXT_MENU_FLAGS: u32 = 0x574;
pub const CONTEXT_MENU_RIGHT_ALIGNED: u32 = 2;

/// What the open popup of form class `vtable` is, if one the game places
/// at a map click (`flags`: a context menu's field 0x574).
pub fn click_popup(build: civ3_sys::Build, vtable: u32, flags: impl FnOnce() -> Option<u32>) -> Option<ClickPopup> {
    if civ3_sys::controls::CONTEXT_MENU_VTABLES.contains(&vtable) {
        return Some(ClickPopup::ContextMenu { right_aligned: flags().is_some_and(|f| f & CONTEXT_MENU_RIGHT_ALIGNED != 0) });
    }
    (tile_info_vtable(build) == Some(vtable)).then_some(ClickPopup::TileInfo)
}

/// The active screen and its placement setting. Preferences is a modal
/// screen of its own, including over an advisor, and follows the Menu
/// setting rather than the screen below it. The screens after a game
/// (`civ3_sys::ui_hooks::end_of_game_screens`: the summary replay, the
/// ranking, the hall of fame) are screens as the advisors are while they
/// show over `Main_GUI`, as at the end of a game (the hall of fame opened
/// from the title is a part of that menu).
pub fn active_screen(frame: &Frame, build: civ3_sys::Build) -> Option<(usize, &'static str)> {
    let screens = civ3_sys::ui_hooks::screen_forms(build);
    let preferences = civ3_sys::ui_hooks::preferences_form(build);
    let main_gui = civ3_sys::ui_hooks::main_gui_canvas(build);
    let after_game: &[(u32, &str)] = if frame.forms.iter().any(|f| f.1 == main_gui) { civ3_sys::ui_hooks::end_of_game_screens(build) } else { &[] };
    frame.forms.iter().enumerate().rev().find_map(|(i, f)| {
        if preferences.is_some_and(|p| p.vtable == f.0) {
            Some((i, "Menu"))
        } else {
            screens.iter().chain(after_game).find(|s| s.0 == f.0).map(|s| (i, s.1))
        }
    })
}

/// The text key of the popup Escape opens on the map ("Oh No! Do you really
/// want to quit?"): the map's key handler 0x4E06B0 calls 0x4DFEF0 for
/// Escape (at 0x4E095C) when no command waits for its tile, which shows a
/// generic popup (`crate::async_popups::POPUP_VTABLE`) with this key. C3X
/// wraps that handler (`patch_Main_Screen_Form_handle_key_down`) and passes
/// Escape on unchanged; Escape on the map opens no other dialogue.
pub const REALLY_QUIT: &str = "REALLYQUIT";

/// Whether one of the dialogues is open that let go of the pointer
/// (`scene::Interface::releases_pointer`): the game menu, with Save, Load
/// and Quit Game (the HUD's menu button and Ctrl+M open it; the screen
/// form of class "Menu" in `screen_forms`, not Preferences, which the
/// viewer places as "Menu" too), or the popup Escape opens on the map
/// ([`REALLY_QUIT`]). `screen`: the class (vtable) of the active screen's
/// form ([`active_screen`]); `modal`: the modal form's class and, for a
/// generic popup, its text key. From the game's state as it is now.
pub fn releases_pointer(build: civ3_sys::Build, screen: Option<u32>, modal: Option<(u32, Option<&str>)>) -> bool {
    let menu = civ3_sys::ui_hooks::screen_forms(build).iter().find(|s| s.1 == "Menu").map(|s| s.0);
    let game_menu = screen.is_some_and(|class| Some(class) == menu);
    let really_quit = modal.is_some_and(|(class, key)| class == crate::async_popups::POPUP_VTABLE && key == Some(REALLY_QUIT));
    game_menu || really_quit
}

/// Whether the game has ended: the screens after it are open, or were
/// since `Main_GUI` was last composed. Its end-of-game sequence (docs
/// §16.5) shows the victory screen, the summary replay, demographics (F11),
/// the histograph (F8), the historical ranking and the hall of fame, all
/// over `Main_GUI`, then the title. The two in the middle are the screens
/// of a running game: only what came before them tells that this one has
/// ended (its world, which the game leaves black there, is not to show
/// round them).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct GameOver(bool);

impl GameOver {
    /// After a composition showing `frame`; whether the game has ended.
    pub fn update(&mut self, frame: &Frame, build: civ3_sys::Build) -> bool {
        let main_gui = civ3_sys::ui_hooks::main_gui_canvas(build);
        let end = civ3_sys::ui_hooks::end_of_game_forms(build);
        if !frame.forms.iter().any(|f| f.1 == main_gui) {
            self.0 = false;
        } else if frame.forms.iter().any(|f| end.contains(&f.0)) {
            self.0 = true;
        }
        self.0
    }

    pub fn ended(&self) -> bool {
        self.0
    }
}

/// The map's corner and unit-order controls may roam only while no screen
/// covers the map. Preferences has its own form outside the named screens
/// that the viewer can place, but also covers these map controls.
pub fn map_hud(frame: &Frame, build: civ3_sys::Build) -> Option<&Frame> {
    active_screen(frame, build).is_none().then_some(frame)
}

/// The frame round the screens after a game: `Main_GUI` and what is
/// composed over it. What the forms under it still hold (the main screen
/// form's canvas, the map's overlay: the ended game's map, its labels and
/// fog of war, which the game leaves black) is no world to show.
pub fn after_game_frame(frame: &Frame, build: civ3_sys::Build) -> Frame {
    let main_gui = civ3_sys::ui_hooks::main_gui_canvas(build);
    let Some(at) = frame.forms.iter().position(|f| f.1 == main_gui) else { return frame.clone() };
    let under: Vec<u32> = frame.forms[..at].iter().map(|f| f.1).collect();
    let draws: Vec<usize> = frame.form_draws.iter().filter(|(c, _)| under.contains(c)).map(|(_, r)| r.clone())
        .chain(frame.underlay_draws.iter().cloned()).flatten().collect();
    without(frame, &draws)
}

/// One modal canvas, separated from both its parent and its child dialogs.
#[derive(Clone, Debug)]
pub struct PopupPart {
    pub canvas: u32,
    pub rect: [i32; 4],
    pub frame: Frame,
    /// Part of the dialog under it, not a dialog of its own: the list a
    /// combo box of that dialog opened (`civ3_sys::controls::COMBO_LIST_VTABLES`).
    /// It goes where that dialog goes (`scene::Interface::modal_attached`).
    pub attached: bool,
}

/// Remove the complete visible modal stack before measuring the named screen.
/// Split from the top down so a full-window transparent parent never includes
/// a child's art when its own visible content bounds are measured.
pub fn split_modal_stack(frame: &Frame, canvases: &[u32], screen: Option<usize>, size: (i32, i32)) -> (Frame, Vec<PopupPart>) {
    let mut below = frame.clone();
    let mut parts = Vec::new();
    for &canvas in canvases.iter().rev() {
        if parts.len() == 4 { break; }
        if parts.iter().any(|part: &PopupPart| part.canvas == canvas) { continue; }
        let Some((first, rect)) = popup(&below, canvas, screen, size) else { continue };
        let (parent, own) = split_popup(&below, first, rect);
        let combo_list = frame.forms.iter().rfind(|f| f.1 == canvas).is_some_and(|f| civ3_sys::controls::COMBO_LIST_VTABLES.contains(&f.0));
        parts.push(PopupPart { canvas, rect, frame: own, attached: combo_list });
        below = parent;
    }
    parts.reverse();
    // (Attached to a dialog under it only: a combo box's list over a screen
    // is a popup of its own.)
    if let Some(bottom) = parts.first_mut() {
        bottom.attached = false;
    }
    (below, parts)
}

/// The diplomacy talk's dialog (`talk_offer.pcx`, x, y, w, h) in its popup
/// (`canvas`, its draws from `first`): the main panel the viewer centres
/// (`scene::Interface::popup_core`). The popup also takes in the foreign
/// advisor at its right, whose picture and box change as the talk goes on.
pub fn popup_core(frame: &Frame, canvas: u32, first: usize) -> Option<[i32; 4]> {
    let form = frame.forms.iter().rfind(|f| f.1 == canvas)?;
    if form.0 != civ3_sys::controls::screens::DIPLOMACY {
        return None;
    }
    // Where its regions go, whole (not as clipped: a draw the model cut
    // down where a later one covers it, or the game's redraw of a strip,
    // would make a strip of the dialog its middle).
    let talk = |path: &str| path.rsplit(['/', '\\']).next().is_some_and(|n| n.eq_ignore_ascii_case("talk_offer.pcx"));
    frame.draws.get(first..)?.iter().filter_map(|d| match d {
        crate::ui::ScreenDraw::Image { path, rect, x, y, .. } | crate::ui::ScreenDraw::Indexed { path, rect, x, y, .. } if talk(path) => {
            Some([*x, *y, x + rect[2], y + rect[3]])
        }
        _ => None,
    }).reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]).map(|[l, t, r, b]| [l, t, r - l, b - t])
}

/// A popup's rectangle while it stays open (`canvas`): as large as it has
/// been, so that what comes and goes at its edges (an advisor's changing
/// picture beside a talk) never moves or scales it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeldExtent(Option<(u32, [i32; 4])>);

impl HeldExtent {
    pub fn hold(&mut self, canvas: u32, [x, y, w, h]: [i32; 4]) -> [i32; 4] {
        let held = match self.0 {
            Some((c, [hx, hy, hw, hh])) if c == canvas => {
                let (l, t) = (x.min(hx), y.min(hy));
                [l, t, (x + w).max(hx + hw) - l, (y + h).max(hy + hh) - t]
            }
            _ => [x, y, w, h],
        };
        self.0 = Some((canvas, held));
        held
    }

    pub fn closed(&mut self) {
        self.0 = None;
    }
}

/// The active screen's part. Preferences owns a screen-sized canvas,
/// while its controls and background occupy a 1024 by 768 panel. Keep that
/// panel, rather than the driver's map, as the rectangle the viewer places.
/// Its own first draw also keeps an underlying advisor or game menu out.
pub fn active_screen_part(frame: &Frame, index: usize, build: civ3_sys::Build, size: (i32, i32)) -> Option<(usize, [i32; 4])> {
    let form = frame.forms.get(index)?;
    if let Some(preferences) = civ3_sys::ui_hooks::preferences_form(build).filter(|p| p.vtable == form.0) {
        let first = frame.form_draws.iter().filter(|(c, r)| *c == form.1 && !r.is_empty()).map(|(_, r)| r.start).min()?;
        let rect = crate::prefs::screen_area(frame, &preferences, (1024, 768))?;
        return Some((first, rect));
    }
    // A game menu owns its frame rather than an advisor's shared
    // background. Keep it separate even if it fills the driver's window.
    let is_menu = civ3_sys::ui_hooks::screen_forms(build).iter().any(|s| s.0 == form.0 && s.1 == "Menu");
    if is_menu { part_from(frame, index, size, false) } else { screen_part(frame, index, size) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::ScreenDraw;

    #[test]
    fn only_the_game_menu_and_the_quit_popup_release_the_pointer() {
        use civ3_sys::Build;
        let popup = crate::async_popups::POPUP_VTABLE;
        for build in [Build::PcGames, Build::Gog] {
            // The game menu (class 0x66DD80), with or without a popup over it.
            assert!(releases_pointer(build, Some(0x66_DD80), None));
            assert!(releases_pointer(build, Some(0x66_DD80), Some((popup, Some("GAMESAVED")))));
            // Escape on the map: the "really quit?" popup, over nothing or
            // over a screen.
            assert!(releases_pointer(build, None, Some((popup, Some(REALLY_QUIT)))));
            assert!(releases_pointer(build, Some(0x66_C270), Some((popup, Some(REALLY_QUIT)))));
            // Everything else stays as it was: the advisors, the city
            // screen, Preferences (placed as "Menu" too), other popups, a
            // popup of another class with that key.
            let preferences = civ3_sys::ui_hooks::preferences_form(build).map(|p| p.vtable);
            for screen in [Some(0x66_C270), Some(0x66_58B0), Some(0x66_B0A8), preferences, None] {
                assert!(!releases_pointer(build, screen, None), "{screen:x?}");
                assert!(!releases_pointer(build, screen, Some((popup, Some("WONDERPRODUCE")))), "{screen:x?}");
                assert!(!releases_pointer(build, screen, Some((popup, None))), "{screen:x?}");
            }
            assert!(!releases_pointer(build, None, Some((0x66_DD80 + 4, Some(REALLY_QUIT)))));
        }
    }

    #[test]
    fn context_menus_and_the_tile_box_are_anchored_by_the_games_rule() {
        use civ3_sys::Build;
        // The unit stack's list and the right-click menu: either class.
        for vt in civ3_sys::controls::CONTEXT_MENU_VTABLES {
            assert_eq!(click_popup(Build::PcGames, vt, || Some(0)), Some(ClickPopup::ContextMenu { right_aligned: false }));
            assert_eq!(click_popup(Build::PcGames, vt, || Some(CONTEXT_MENU_RIGHT_ALIGNED)), Some(ClickPopup::ContextMenu { right_aligned: true }));
            assert_eq!(click_popup(Build::PcGames, vt, || None), Some(ClickPopup::ContextMenu { right_aligned: false }));
        }
        assert_eq!(click_popup(Build::PcGames, 0x66_F5C8, || None), Some(ClickPopup::TileInfo));
        // Advisors, dialogs, diplomacy: placed with the screens.
        for vt in [0x66_C270, 0x66_58B0, 0x66_C14C, 0] {
            assert_eq!(click_popup(Build::PcGames, vt, || Some(2)), None);
        }
        assert_eq!(click_popup(Build::Steam, 0x66_F5C8, || None), None);
        // Top-left one pixel right of and below the click; right-aligned
        // menus their top-right corner there.
        let menu = ClickPopup::ContextMenu { right_aligned: false };
        assert_eq!(popup_anchor(menu, (1234, -56), (19, 0), false, (180, 300)), scene::PopupAnchor { world: (1234, -56), offset: (1, 1), world_scaled: false });
        let right = ClickPopup::ContextMenu { right_aligned: true };
        assert_eq!(popup_anchor(right, (10, 20), (0, 0), true, (180, 300)), scene::PopupAnchor { world: (10, 20), offset: (-179, 1), world_scaled: false });
        // The tile box: half a tile left of the tile's corner, 9 less half
        // a tile down; zoomed out with the smaller tiles.
        assert_eq!(popup_anchor(ClickPopup::TileInfo, (0, 0), (5, 7), false, (256, 173)), scene::PopupAnchor { world: (5 * 64 - 64, 7 * 32 + 9 - 32), offset: (0, 0), world_scaled: true });
        assert_eq!(popup_anchor(ClickPopup::TileInfo, (0, 0), (6, 0), true, (256, 173)), scene::PopupAnchor { world: (6 * 32 - 32, 9 - 16), offset: (0, 0), world_scaled: true });
    }

    #[test]
    fn a_full_canvas_diplomacy_popup_uses_its_dialog_leader_and_advisor_bounds() {
        let full = [0, 0, 1920, 1080];
        let image = |path: &str, x, y, w, h| ScreenDraw::Image { retained:None,
            path: path.into(), rect: [0, 0, w, h], x, y, keyed: true,
            clip: full, mask: None, palette: None, opacity: 255,
        };
        // Native WW2 diplomacy owns a full-window canvas but only draws
        // the conversation, its leader and the separate advisor on it.
        let frame = Frame {
            overlay_draws: 0..0,
            forms: vec![(0x66b3d8, 1, (0, 0), (1920, 1080)), (0x66c0d8, 2, (0, 0), (1920, 1080))],
            draws: vec![
                fill(0, 0, 1920, 1080),
                image("Art/Diplomacy/talk_offer.pcx", 297, 50, 438, 517),
                ScreenDraw::Flc { path: "Art/Flics/leader.flc".into(), chunk: 1, x: 411, y: 59, clip: [411, 59, 611, 299], plain: true },
                image("Art/SmallHeads/popupForeign.pcx", 907, 0, 91, 109),
                image("Art/Advisors/diplowinbox.pcx", 807, 99, 206, 102),
            ],
            form_draws: vec![(1, 0..1), (2, 1..5)],
            underlay_draws: vec![0..1],
        };
        let (first, rect) = popup(&frame, 2, None, (1920, 1080)).unwrap();
        assert_eq!((first, rect), (1, [297, 0, 716, 567]));
        let (map, dialog) = split_popup(&frame, first, rect);
        assert_eq!(map.draws, frame.draws[..1]);
        assert_eq!(dialog.draws, frame.draws[1..]);
        assert!(dialog.underlay_draws.is_empty());
        // The same rule covers full-canvas menus and cropped screen parts.
        assert_eq!(screen_part(&frame, 1, (1920, 1080)), Some((1, rect)));
        // Its main panel, the dialog, is what the viewer centres.
        assert_eq!(popup_core(&frame, 2, first), Some([297, 50, 438, 517]));
        assert_eq!(popup_core(&frame, 1, first), None, "not a talk");
        // Cut down to a strip (covered below by a later draw), and a strip
        // of it drawn again: still the whole dialog.
        let mut redrawn = frame.clone();
        if let ScreenDraw::Image { clip, .. } = &mut redrawn.draws[1] { *clip = [297, 50, 735, 58]; }
        redrawn.draws.push(ScreenDraw::Image { retained: None, path: "Art/Diplomacy/talk_offer.pcx".into(), rect: [0, 400, 438, 20], x: 297, y: 450, keyed: true, clip: full, mask: None, palette: None, opacity: 255 });
        assert_eq!(popup_core(&redrawn, 2, first), Some([297, 50, 438, 517]));
    }

    #[test]
    fn a_popups_rectangle_holds_its_largest_extent_while_it_stays_open() {
        let mut held = HeldExtent::default();
        assert_eq!(held.hold(7, [297, 0, 716, 567]), [297, 0, 716, 567]);
        // The advisor's box narrower, then wider: never smaller.
        assert_eq!(held.hold(7, [297, 50, 600, 517]), [297, 0, 716, 567]);
        assert_eq!(held.hold(7, [297, 0, 760, 567]), [297, 0, 760, 567]);
        // Another popup, or the same canvas opened anew: its own.
        assert_eq!(held.hold(8, [10, 10, 100, 100]), [10, 10, 100, 100]);
        held.closed();
        assert_eq!(held.hold(8, [20, 20, 50, 50]), [20, 20, 50, 50]);
    }

    fn fill(x: i32, y: i32, w: i32, h: i32) -> ScreenDraw {
        ScreenDraw::Fill { rect: [x, y, w, h], rgb: [1, 2, 3], alpha: 255, clip: [0, 0, 1920, 1080] }
    }

    /// The map screen with the HUD's corner buttons and two unit orders,
    /// and a popup over it.
    fn frame() -> Frame {
        Frame {
            overlay_draws: 0..0,
            draws: vec![fill(0, 0, 1920, 1080), fill(0, 0, 160, 60), fill(21, 12, 36, 30), fill(836, 1030, 32, 32), fill(700, 400, 500, 300), fill(720, 600, 80, 30)],
            forms: vec![
                (0x66_B3D8, 0x9F_8934, (0, 0), (1920, 1080)),
                (0x66_56C0, 0xA0_68B4, (21, 12), (36, 30)),
                (0x66_56C0, 0xA0_765C, (57, 12), (36, 30)),
                (0x66_56C0, 0xA0_61E0, (94, 12), (34, 30)),
                (0x66_56C0, 0xA1_4A68, (836, 1030), (32, 32)),
                (0x66_56C0, 0xA1_4A68 + 0x6DC, (868, 1030), (32, 32)),
                (0x66_6408, 0x50_0000, (700, 400), (500, 300)),
                (0x66_56C0, 0x50_1000, (720, 600), (80, 30)),
            ],
            form_draws: vec![(0x9F_8934, 0..2), (0xA0_68B4, 2..3), (0xA1_4A68, 3..4), (0x50_0000, 4..5), (0x50_1000, 5..6)],
            underlay_draws: vec![],
        }
    }

    #[test]
    fn the_corner_buttons_and_the_unit_orders_are_found_by_their_canvases() {
        let f = frame();
        let hud = civ3_sys::ui_hooks::hud_buttons(civ3_sys::Build::PcGames);
        assert_eq!(hud.corner, [0xA0_68B4, 0xA0_765C, 0xA0_61E0]);
        let corner = forms_rect(&f, |c| hud.corner.contains(&c)).unwrap();
        assert_eq!(corner, [21, 12, 107, 30]);
        assert_eq!(corner_region(corner), [0, 0, 149, 54]);
        assert_eq!(forms_rect(&f, |c| hud.is_unit_order(c)), Some([836, 1030, 64, 32]));
        assert!(!hud.is_unit_order(0xA1_4A68 + 1) && !hud.is_unit_order(0xA1_4A68 + 42 * 0x6DC) && hud.is_unit_order(0xA1_4A68 + 41 * 0x6DC));
        assert_eq!(forms_rect(&f, |_| false), None);
    }

    #[test]
    fn preferences_hides_map_controls_and_closing_it_restores_them() {
        let build = civ3_sys::Build::PcGames;
        let hud = civ3_sys::ui_hooks::hud_buttons(build);
        let mut f = frame();
        let controls = |f: &Frame| {
            map_hud(f, build).map(|f| (corner_part(f, &hud.corner), forms_rect(f, |c| hud.is_unit_order(c))))
        };
        let before = controls(&f).unwrap();
        assert!(before.0.is_some() && before.1.is_some());
        let preferences = civ3_sys::ui_hooks::preferences_form(build).unwrap();
        f.forms.push((preferences.vtable, preferences.form + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4, (0, 0), (1920, 1080)));
        // Preferences is not one of the screens with a placement setting.
        // Its presence must nevertheless suppress the separately copied HUD.
        assert_eq!(screen(&f, civ3_sys::ui_hooks::screen_forms(build)), None);
        assert_eq!(controls(&f), None);
        // An info popup over Preferences must keep those controls hidden.
        f.forms.push((0x66_6408, 0x60_0000, (700, 400), (500, 300)));
        assert_eq!(controls(&f), None);
        f.forms.pop();
        f.forms.pop();
        assert_eq!(controls(&f), Some(before));
    }

    #[test]
    fn named_screens_also_hide_map_controls() {
        let build = civ3_sys::Build::PcGames;
        let mut f = frame();
        for &(vtable, _) in civ3_sys::ui_hooks::screen_forms(build) {
            f.forms.push((vtable, 0x70_0000, (448, 156), (1024, 768)));
            assert_eq!(map_hud(&f, build), None);
            f.forms.pop();
            assert!(map_hud(&f, build).is_some());
        }
    }

    #[test]
    fn the_corner_part_takes_its_buttons_and_their_tooltip_from_the_interface() {
        let mut f = frame();
        let hud = civ3_sys::ui_hooks::hud_buttons(civ3_sys::Build::PcGames);
        // A tooltip under the Civilopedia button, on no canvas.
        f.draws.push(ScreenDraw::Fill { rect: [60, 44, 90, 16], rgb: [255, 255, 255], alpha: 255, clip: [i32::MIN, i32::MIN, i32::MAX, i32::MAX] });
        let (rect, draws) = corner_part(&f, &hud.corner).unwrap();
        assert_eq!(rect, [0, 0, 150, 60]);
        assert_eq!(draws, [2, 6]);
        let rest = without(&f, &draws);
        assert!(rest.draws.iter().enumerate().all(|(i, d)| reaches_clip_of(d) != draws.contains(&i)));
        // Far from the buttons a tooltip is not theirs.
        let mut f = frame();
        f.draws.push(ScreenDraw::Fill { rect: [900, 990, 90, 16], rgb: [255, 255, 255], alpha: 255, clip: [i32::MIN, i32::MIN, i32::MAX, i32::MAX] });
        assert_eq!(corner_part(&f, &hud.corner).unwrap(), ([0, 0, 149, 54], vec![2]));
    }

    fn reaches_clip_of(d: &ScreenDraw) -> bool {
        crate::ui::reaches_clip(d)
    }

    #[test]
    fn the_corner_region_keeps_what_reaches_into_it_clipped() {
        let r = region(&frame(), [0, 0, 149, 54]);
        assert_eq!(r.draws.len(), 3);
        assert!(r.draws.iter().all(|d| matches!(d, ScreenDraw::Fill { clip: [0, 0, 149, 54], .. })), "{:?}", r.draws);
    }

    #[test]
    fn screens_are_found_by_their_forms_class() {
        let mut f = frame();
        let table = civ3_sys::ui_hooks::screen_forms(civ3_sys::Build::PcGames);
        assert_eq!(screen(&f, table), None);
        // The cultural advisor over its background, the game menu over it.
        f.forms.insert(1, (0x66_88B8, 0x9A_FFCC, (448, 156), (1024, 768)));
        f.forms.insert(2, (0x66_BBC0, 0x9B_0000, (448, 156), (1024, 768)));
        assert_eq!(screen(&f, table), Some((2, "F5")));
        f.forms.push((0x66_DD80, 0xB4_2C64, (778, 407), (364, 266)));
        assert_eq!(screen(&f, table).map(|s| s.1), Some("Menu"));
        assert!(table.iter().any(|s| s.1 == "Palace") && table.iter().any(|s| s.1 == "Help") && table.iter().any(|s| s.1 == "F8"));
        // Every screen older settings placed one by one; and demographics
        // (F11), which came after them.
        assert!(viewer_settings::Layout::SCREENS.iter().all(|n| table.iter().any(|s| s.1 == *n)));
        assert!(table.iter().all(|s| s.1 == "F11" || viewer_settings::Layout::SCREENS.contains(&s.1)));
        // Demographics (F11) is a screen of a running game too: it is
        // placed as the advisors are and leaves the world shown.
        assert_eq!(table.iter().find(|s| s.1 == "F11").map(|s| s.0), Some(0x66_BF10));
        assert!(!civ3_sys::ui_hooks::end_of_game_forms(civ3_sys::Build::PcGames).contains(&0x66_BF10));
    }

    #[test]
    fn preferences_uses_the_menu_setting_and_its_panel_without_the_underlying_advisor() {
        let build = civ3_sys::Build::PcGames;
        let preferences = civ3_sys::ui_hooks::preferences_form(build).unwrap();
        let canvas = preferences.form + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4;
        let mut f = Frame {
            overlay_draws: 0..0,
            draws: vec![
                fill(0, 0, 1920, 1080),
                fill(21, 12, 36, 30),
                fill(448, 156, 1024, 768),
                fill(448, 156, 1024, 768),
                fill(778, 407, 364, 266),
                fill(0, 0, 1920, 1080),
                ScreenDraw::Image { retained:None,
                    path: preferences.background.into(), rect: [0, 0, 1024, 768], x: 448, y: 156,
                    keyed: false, clip: [0, 0, 1920, 1080], mask: None, palette: None, opacity: 255,
                },
                fill(500, 800, 80, 30),
                // The info box's frame is drawn directly onto the screen.
                fill(690, 70, 420, 30),
                fill(700, 100, 400, 300),
                fill(720, 340, 80, 30),
            ],
            forms: vec![
                (0x66_B3D8, 1, (0, 0), (1920, 1080)),
                (0x66_56C0, 2, (21, 12), (36, 30)),
                (0x66_88B8, 3, (448, 156), (1024, 768)),
                (0x66_C270, 4, (448, 156), (1024, 768)),
                (0x66_DD80, 5, (778, 407), (364, 266)),
                (preferences.vtable, canvas, (0, 0), (1920, 1080)),
                (0x66_56C0, 7, (500, 800), (80, 30)),
                (0x66_6408, 8, (700, 100), (400, 300)),
                (0x66_56C0, 9, (720, 340), (80, 30)),
            ],
            form_draws: vec![(1, 0..1), (2, 1..2), (3, 2..3), (4, 3..4), (5, 4..5), (canvas, 5..7), (7, 7..8), (8, 9..10), (9, 10..11)],
            underlay_draws: vec![],
        };
        assert_eq!(active_screen(&f, build), Some((5, "Menu")));
        let (popup_first, popup_rect) = popup(&f, 8, Some(5), (1920, 1080)).unwrap();
        assert_eq!((popup_first, popup_rect), (8, [690, 70, 420, 330]));
        let (below_popup, info) = split_at(&f, popup_first);
        let (first, rect) = active_screen_part(&below_popup, 5, build, (1920, 1080)).unwrap();
        assert_eq!((first, rect), (5, [448, 156, 1024, 768]));
        let (below_screen, raw_panel) = split_at(&below_popup, first);
        let panel = clipped(&raw_panel, rect);
        // The map and its HUD remain in the lower frame. Only the current
        // panel, not the old advisor or menu, is copied into screen_view.
        assert_eq!(below_screen.draws, f.draws[..5]);
        assert_eq!(raw_panel.draws, f.draws[5..8]);
        assert_eq!(panel.form_draws, raw_panel.form_draws);
        assert!(panel.draws.iter().all(|draw| match draw {
            ScreenDraw::Fill { clip, .. } | ScreenDraw::Image { clip, .. } => *clip == [448, 156, 1472, 924],
            _ => false,
        }));
        assert_eq!(info.draws, f.draws[8..]);
        // The generated tabs have exactly one owner and remain under the
        // complete info popup, including its directly drawn frame.
        assert_eq!(crate::prefs::tabs_slot(&below_screen, &preferences, Some(8)), None);
        assert_eq!(crate::prefs::tabs_slot(&panel, &preferences, Some(8)), Some(3));
        assert_eq!(crate::prefs::tabs_slot(&info, &preferences, Some(8)), None);
        f.forms.truncate(5);
        assert_eq!(active_screen(&f, build), Some((4, "Menu")));
        assert_eq!(active_screen_part(&f, 4, build, (1920, 1080)), Some((4, [778, 407, 364, 266])));
        f.forms.truncate(4);
        assert_eq!(active_screen(&f, build), Some((3, "F1")));
    }

    #[test]
    fn preferences_bounds_follow_the_drawn_background_not_its_full_screen_canvas() {
        let build = civ3_sys::Build::PcGames;
        let preferences = civ3_sys::ui_hooks::preferences_form(build).unwrap();
        let f = Frame {
            overlay_draws: 0..0,
            draws: vec![fill(0, 0, 1920, 1080), ScreenDraw::Image { retained:None,
                path: preferences.background.into(), rect: [100, 50, 924, 718], x: 548, y: 206,
                keyed: false, clip: [0, 0, 1920, 1080], mask: None, palette: None, opacity: 255,
            }],
            forms: vec![(0x66_B3D8, 1, (0, 0), (1920, 1080)), (preferences.vtable, 2, (0, 0), (1920, 1080))],
            form_draws: vec![(1, 0..1), (2, 1..2)],
            underlay_draws: vec![],
        };
        assert_eq!(active_screen(&f, build), Some((1, "Menu")));
        assert_eq!(active_screen_part(&f, 1, build, (1920, 1080)), Some((1, [448, 156, 1024, 768])));
        // An as-yet undrawn Preferences form supplies no empty screen view.
        let empty = Frame { draws: vec![], form_draws: vec![], ..f };
        assert_eq!(active_screen_part(&empty, 1, build, (1920, 1080)), None);
    }

    #[test]
    fn a_full_screen_game_menu_does_not_take_the_worlds_draws_with_it() {
        let build = civ3_sys::Build::PcGames;
        let f = Frame {
            overlay_draws: 0..0,
            draws: vec![fill(0, 0, 1024, 768), fill(0, 0, 1024, 768)],
            forms: vec![(0x66_B3D8, 1, (0, 0), (1024, 768)), (0x66_DD80, 2, (0, 0), (1024, 768))],
            form_draws: vec![(1, 0..1), (2, 1..2)],
            underlay_draws: vec![],
        };
        assert_eq!(active_screen_part(&f, 1, build, (1024, 768)), Some((1, [0, 0, 1024, 768])));
    }

    #[test]
    fn the_popup_is_split_off_with_what_is_drawn_over_it() {
        let f = frame();
        let (first, rect) = popup(&f, 0x50_0000, None, (1920, 1080)).unwrap();
        assert_eq!((first, rect), (4, [700, 400, 500, 300]));
        let (below, above) = split_at(&f, first);
        assert_eq!((below.draws.len(), above.draws.len()), (4, 2));
        assert_eq!(above.form_draws, [(0x50_0000, 0..1), (0x50_1000, 1..2)]);
        // The frame the game draws around it on no canvas, just before it
        // (a title band above it), goes with it; draws further off do not.
        let mut f = frame();
        let band = ScreenDraw::Image { retained:None,path: "popupborders.pcx".into(), rect: [251, 1, 60, 35], x: 690, y: 365, keyed: true, clip: [i32::MIN, i32::MIN, i32::MAX, i32::MAX], mask: None, palette: None, opacity: 255 };
        f.draws.insert(4, fill(10, 10, 20, 20));
        f.draws.insert(5, band);
        f.form_draws = vec![(0x9F_8934, 0..2), (0xA0_68B4, 2..3), (0xA1_4A68, 3..4), (0x50_0000, 6..7), (0x50_1000, 7..8)];
        assert_eq!(popup(&f, 0x50_0000, None, (1920, 1080)), Some((5, [690, 365, 510, 335])));
        // Under a screen composited above it: not a popup of its own.
        assert_eq!(popup(&f, 0x50_0000, Some(7), (1920, 1080)), None);
        assert_eq!(popup(&f, 0x12_3456, None, (1920, 1080)), None);
    }

    /// Between games the title and the setup screens are modal forms
    /// themselves: only a form smaller than the menu under it is a popup.
    #[test]
    fn between_games_only_a_form_smaller_than_the_menu_is_a_popup() {
        let size = (1920, 1080);
        let (x, y) = (448, 156);
        // The full-window backdrop and the title, a setup screen over it,
        // a popup over that.
        let f = Frame {
            overlay_draws: 0..0,
            draws: vec![fill(0, 0, 1920, 1080), fill(x, y, 1024, 768), fill(x, y, 1024, 768), fill(x + 262, y + 234, 500, 300)],
            forms: vec![(0x11, 1, (0, 0), (1920, 1080)), (0x22, 2, (x, y), (1024, 768)), (0x33, 3, (x + 262, y + 234), (500, 300))],
            form_draws: vec![(1, 0..2), (2, 2..3), (3, 3..4)],
            underlay_draws: vec![],
        };
        assert_eq!(menu_popup(&f, 3, size), Some((3, [x + 262, y + 234, 500, 300])));
        assert_eq!(menu_popup(&f, 2, size), None);
        // Nothing under it but the backdrop: no menu to be over.
        let alone = Frame { overlay_draws: 0..0, draws: vec![fill(0, 0, 1920, 1080), fill(x, y, 500, 300)], forms: vec![(0x11, 1, (0, 0), (1920, 1080)), (0x33, 3, (x, y), (500, 300))],
            form_draws: vec![(1, 0..1), (3, 1..2)], underlay_draws: vec![] };
        assert_eq!(menu_popup(&alone, 3, size), None);
    }

    /// TS Neo: the Hall of Fame from the title in a 3274 by 2048 window. Its
    /// form (as the victory screen's) has an opaque canvas of the whole
    /// window, black where it never draws, with its 1024 by 768 board in
    /// the middle. That board is its size: as a modal part (the game's
    /// modal stack) it is placed and scaled as the title's 1024 by 768,
    /// not as a window-sized dialog shrunk to fit the viewer's canvas.
    #[test]
    fn an_opaque_full_window_canvas_is_measured_by_its_board_not_its_backdrop() {
        let size = (3274, 2048);
        let clip = [0, 0, 3274, 2048];
        let big = |x, y, w, h| ScreenDraw::Fill { rect: [x, y, w, h], rgb: [1, 2, 3], alpha: 255, clip };
        let (x, y) = (1125, 640);
        let title = vec![big(0, 0, 3274, 2048), big(x, y, 1024, 768), big(1, 2010, 60, 20), big(1600, 2010, 80, 20)];
        let n = title.len();
        let board = ScreenDraw::Image { retained:None,
            path: "Art/Advisors/hall_o_fame_background.pcx".into(), rect: [0, 0, 1024, 768], x, y,
            keyed: false, clip, mask: None, palette: None, opacity: 255,
        };
        let hall = |own: Vec<ScreenDraw>| {
            let mut draws = title.clone();
            draws.extend(own);
            let to = draws.len();
            Frame { overlay_draws: 0..0, draws, forms: vec![(0x11, 1, (0, 0), size), (0x66_D1F0, 2, (0, 0), size)],
                form_draws: vec![(1, 0..n), (2, n..to)], underlay_draws: vec![] }
        };
        let black = ScreenDraw::Fill { rect: [0, 0, 3274, 2048], rgb: [0, 0, 0], alpha: 255, clip };
        let f = hall(vec![black.clone(), board.clone(), big(x + 100, y + 200, 300, 20)]);
        assert_eq!(popup(&f, 2, None, size), Some((n, [x, y, 1024, 768])));
        let (below, parts) = split_modal_stack(&f, &[2], None, size);
        assert_eq!((below.draws.len(), parts.len(), parts[0].rect), (n, 1, [x, y, 1024, 768]));
        assert_eq!(parts[0].frame.draws.len(), 3, "the backdrop stays with its screen");
        // As large as the title under it: a screen of its own, no popup.
        assert_eq!(menu_popup(&f, 2, size), None);
        // A translucent shade of the whole window is a backdrop as well.
        let shade = ScreenDraw::Fill { rect: [0, 0, 3274, 2048], rgb: [0, 0, 0], alpha: 128, clip };
        assert_eq!(popup(&hall(vec![shade, board.clone()]), 2, None, size), Some((n, [x, y, 1024, 768])));
        // A full-window picture is content; a canvas showing nothing but
        // its clear keeps the whole window.
        let picture = ScreenDraw::Image { retained:None,path: "Art/wide.pcx".into(), rect: [0, 0, 3274, 2048], x: 0, y: 0, keyed: false, clip, mask: None, palette: None, opacity: 255 };
        assert_eq!(popup(&hall(vec![black.clone(), picture, board]), 2, None, size), Some((n, [0, 0, 3274, 2048])));
        assert_eq!(popup(&hall(vec![black]), 2, None, size), Some((n, [0, 0, 3274, 2048])));
    }

    fn advisor_popup_frame() -> Frame {
        let mut f = frame();
        // The advisor portrait is drawn into the map canvas, before the
        // popup form. It must not remain a world overlay after moving it.
        f.draws.insert(1, ScreenDraw::Image { retained:None,
            path: "C:\\Games\\Art\\SmallHeads\\popupSCIENCE.pcx".into(), rect: [0, 0, 180, 180], x: 840, y: 220,
            keyed: true, clip: [0, 0, 1920, 1080], mask: None, palette: None, opacity: 255,
        });
        f.form_draws = vec![(0x9F_8934, 0..3), (0xA0_68B4, 3..4), (0xA1_4A68, 4..5), (0x50_0000, 5..6), (0x50_1000, 6..7)];
        f.underlay_draws = vec![0..3];
        f
    }

    #[test]
    fn an_advisor_picture_on_the_map_moves_with_the_popup_only() {
        let f = advisor_popup_frame();
        let (first, rect) = popup(&f, 0x50_0000, None, (1920, 1080)).unwrap();
        assert_eq!((first, rect), (5, [700, 220, 500, 480]));
        let (below, above) = split_popup(&f, first, rect);
        assert!(!crate::ui::reaches_clip(&below.draws[1]));
        assert_eq!(above.draws, vec![f.draws[1].clone(), f.draws[5].clone(), f.draws[6].clone()]);
        assert!(above.underlay_draws.is_empty());
        assert_eq!(above.form_draws, [(0x50_0000, 1..2), (0x50_1000, 2..3)]);
        assert_eq!(below.draws[0], f.draws[0], "the map remains behind the dialog");
        // The same art on Main_GUI is a HUD draw rather than an underlay.
        // It still belongs to the popup and must share its transform.
        let mut f = f.clone();
        f.underlay_draws.clear();
        assert_eq!(popup(&f, 0x50_0000, None, (1920, 1080)), Some((first, rect)));
        let (below, above) = split_popup(&f, first, rect);
        assert!(!crate::ui::reaches_clip(&below.draws[1]));
        assert_eq!(above.draws.iter().filter(|d| advisor_portrait(d)).count(), 1);
        // A head owned by an underlying advisor stays on that advisor.
        f.forms.insert(1, (0x66_C270, 0x77_0000, (448, 156), (1024, 768)));
        f.form_draws[0].1 = 0..1;
        f.form_draws.insert(1, (0x77_0000, 1..3));
        assert_eq!(popup(&f, 0x50_0000, None, (1920, 1080)), Some((5, [700, 400, 500, 300])));
        let (below, above) = split_popup(&f, 5, [700, 400, 500, 300]);
        assert!(crate::ui::reaches_clip(&below.draws[1]));
        assert!(!above.draws.iter().any(advisor_portrait));
    }

    /// The Science Advisor's "our Researchers need direction" as traced
    /// (WWII in the Pacific, 1920x1080): its head on a form of its own
    /// (vtable 0x671218, 148x109) whose bottom meets the dialog's frame,
    /// the dialog, and the list its tech combo box opened on top of it (a
    /// form of vtable 0x672160 inside a frame drawn onto the screen).
    fn science_advisor_frame(list_open: bool) -> Frame {
        const HEAD: u32 = 0x9B_98BC;
        let mut f = frame();
        f.draws.truncate(4);
        f.forms.truncate(6);
        f.form_draws.truncate(3);
        f.forms.push((0x67_1218, HEAD, (850, 0), (148, 109)));
        f.draws.push(ScreenDraw::Image { retained:None,
            path: "Conquests\\WWII in the Pacific\\art\\SmallHeads\\popupScience.pcx".into(), rect: [9, 494, 135, 106], x: 858, y: 3,
            keyed: true, clip: [850, 0, 998, 109], mask: None, palette: None, opacity: 255,
        });
        f.form_draws.push((HEAD, 4..5));
        // The dialog's frame (drawn onto the screen) and its form.
        f.draws.push(fill(634, 109, 370, 250));
        f.forms.push((0x66_6408, 0x9B_7784, (637, 144), (364, 212)));
        f.draws.push(fill(637, 144, 364, 212));
        f.form_draws.push((0x9B_7784, 6..7));
        if list_open {
            f.draws.push(fill(608, 250, 376, 231));
            f.forms.push((0x67_2160, 0x22_72D0, (611, 253), (370, 225)));
            f.draws.push(fill(611, 253, 370, 225));
            f.form_draws.push((0x22_72D0, 8..9));
        }
        f
    }

    #[test]
    fn an_advisor_head_on_a_form_of_its_own_goes_with_its_popup_and_a_combo_list_is_attached() {
        let size = (1920, 1080);
        let f = science_advisor_frame(false);
        // The head belongs to the dialog: the popup's rectangle reaches up to it.
        assert_eq!(popup(&f, 0x9B_7784, None, size), Some((5, [634, 3, 370, 356])));
        let (below, parts) = split_modal_stack(&f, &[0x9B_7784], None, size);
        assert_eq!(parts.len(), 1);
        assert!(!crate::ui::reaches_clip(&below.draws[4]), "the head leaves the menus under the popup");
        assert_eq!(parts[0].frame.draws.iter().filter(|d| advisor_portrait(d)).count(), 1);
        assert!(!parts[0].attached);

        // Its combo box opened: the list is a modal layer of its own on
        // top, attached to the dialog; the head stays with the dialog.
        let f = science_advisor_frame(true);
        let (below, parts) = split_modal_stack(&f, &[0x9B_7784, 0x22_72D0], None, size);
        assert_eq!(parts.iter().map(|p| (p.canvas, p.rect, p.attached)).collect::<Vec<_>>(),
            vec![(0x9B_7784, [634, 3, 370, 356], false), (0x22_72D0, [608, 250, 376, 231], true)]);
        assert!(!crate::ui::reaches_clip(&below.draws[4]));
        assert_eq!(parts[0].frame.draws.iter().filter(|d| advisor_portrait(d)).count(), 1);
        assert_eq!(parts[1].frame.draws.len(), 2, "the list's frame and form, nothing of the dialog");
        // A combo box list with no dialog under it (over a screen) is a
        // popup of its own.
        let (_, parts) = split_modal_stack(&f, &[0x22_72D0], None, size);
        assert_eq!(parts.len(), 1);
        assert!(!parts[0].attached);

        // A form holding more than the head (an advisor screen) keeps it.
        let mut f = science_advisor_frame(false);
        f.draws.insert(5, fill(850, 0, 10, 10));
        f.form_draws = f.form_draws.iter().map(|(c, r)| (*c, if *c == 0x9B_98BC { 4..6 } else if r.start >= 5 { r.start + 1..r.end + 1 } else { r.clone() })).collect();
        assert_eq!(popup(&f, 0x9B_7784, None, size), Some((6, [634, 109, 370, 250])));
    }

    #[test]
    fn grouped_advisor_picture_stays_attached_with_strip_panes_and_ui_scales() {
        use tile_render::{AtlasBuilder, Grid, LayerSpec, PlacedSprite, RgbaImage, SpriteSets, TileWorld};
        use tile_viewer::{layout, scaling};
        use viewer_settings::{Place, Resolution, ScreenPlace, Split, Zoom};
        const GREEN: [u8; 4] = [20, 180, 40, 255];
        const RED: [u8; 4] = [220, 30, 40, 255];
        const BLUE: [u8; 4] = [30, 50, 220, 255];
        let mut atlas = AtlasBuilder::new(128);
        let ground = atlas.add(&RgbaImage::from_fn(8, 8, |_, _| GREEN), (0, 0)).unwrap();
        let head = atlas.add(&RgbaImage::from_fn(20, 30, |_, _| RED), (0, 0)).unwrap();
        let body = atlas.add(&RgbaImage::from_fn(60, 40, |_, _| BLUE), (0, 0)).unwrap();
        let atlas = atlas.finish();
        let mut sets = SpriteSets::default();
        let (ground, head, body) = (sets.single(ground), sets.single(head), sets.single(body));
        let mut world = TileWorld::new(Grid { cols: 128, rows: 128, layout: tile_render::Layout::Orthogonal, wrap_x: true, wrap_y: true, step: (8, 8) }, sets);
        world.add_layer(LayerSpec::new("ground", vec![ground]));
        for place in [Place::Floating, Place::LeftOrBottom, Place::RightOrTop] {
            for pixel_scale in [1, 2, 3] {
                for menu_scale in [1.0, 1.5] {
                    let mut s = scene::Scene { screen_size: (160, 120), shape: scene::WorldShape { cols: 128, rows: 128, wrap_x: true, wrap_y: true }, ..Default::default() };
                    s.view_mut(0).view.camera_x = 100;
                    s.view_mut(0).view.camera_y = 100;
                    part_view(&mut s, POPUP_VIEW, [50, 30, 60, 70], vec![
                        scene::FreeSprite { set: head.raw(), x: 65, y: 30, ..Default::default() },
                        scene::FreeSprite { set: body.raw(), x: 50, y: 60, ..Default::default() },
                    ]);
                    let mut settings = viewer_settings::Layout { split: Split::Vertical1To1, zoom: Zoom::TopStrip, ..Default::default() };
                    if place == Place::Floating {
                        s.interface.popup = Some(POPUP_VIEW);
                    } else {
                        // A popup embedded in a named screen is merged into
                        // that screen's view by the bridge.
                        s.interface.screen = Some("F6".into());
                        s.interface.screen_view = Some(POPUP_VIEW);
                        settings.screen_place = ScreenPlace { vertical: place, horizontal: place };
                    }
                    let mut plan = scaling::plan((480, 360), s.screen_size, &Resolution { pixel_scale, ..Default::default() });
                    plan.menu.zoom = scaling::ratio(menu_scale);
                    let mut l = layout::Layout::default();
                    l.apply_settings(&settings);
                    l.driver_screen = Some(plan.screen);
                    let mut f = l.frame(&s, plan.canvas(), (0, 0, 0, 0), vec![], None);
                    scaling::arrange(&plan, &s, &mut f, |sprite| Some(if sprite.set == head.raw() { [65, 30, 20, 30] } else { [50, 60, 60, 40] }));
                    let camera = f.view_cameras[1].unwrap();
                    assert!(!f.cameras[camera as usize].cells);
                    world.free = vec![
                        PlacedSprite { screen: true, view: Some(camera), ..PlacedSprite::new(head, 65, 30) },
                        PlacedSprite { screen: true, view: Some(camera), ..PlacedSprite::new(body, 50, 60) },
                    ];
                    let image = tile_render::reference::render_views(&world, &atlas, &f.cameras, plan.canvas(), 0, [0, 0, 0, 255]);
                    let mut head_bottom = None;
                    let mut body_top = None;
                    for y in 0..plan.canvas().1 {
                        for x in 0..plan.canvas().0 {
                            match image.get(x, y) {
                                RED => head_bottom = Some(y),
                                BLUE => { body_top.get_or_insert(y); },
                                _ => {},
                            }
                        }
                    }
                    let context = format!("{place:?}, pixel scale {pixel_scale}, menu scale {menu_scale}");
                    assert!(head_bottom.is_some() && body_top.is_some(), "{context}");
                    assert_eq!(head_bottom.unwrap() + 1, body_top.unwrap(), "the portrait stays directly above its dialog: {context}");
                }
            }
        }
    }

    #[test]
    fn a_named_screen_smaller_than_the_window_is_a_part_of_its_own() {
        let mut f = frame();
        // An advisor (1024x768 in the middle) over the map screen.
        f.forms.push((0x66_C270, 0x77_0000, (448, 156), (1024, 768)));
        f.draws.push(fill(448, 156, 1024, 768));
        f.form_draws.push((0x77_0000, 6..7));
        assert_eq!(screen_part(&f, f.forms.len() - 1, (1920, 1080)), Some((6, [448, 156, 1024, 768])));
        // Over the advisors' common background (the same rectangle): from it on.
        f.forms.insert(f.forms.len() - 1, (0x66_88B8, 0x76_0000, (448, 156), (1024, 768)));
        f.draws.insert(6, fill(448, 156, 1024, 768));
        f.form_draws = vec![(0x9F_8934, 0..2), (0xA0_68B4, 2..3), (0xA1_4A68, 3..4), (0x50_0000, 4..5), (0x50_1000, 5..6), (0x76_0000, 6..7), (0x77_0000, 7..8)];
        assert_eq!(screen_part(&f, f.forms.len() - 1, (1920, 1080)), Some((6, [448, 156, 1024, 768])));
        // Covering the whole window: not.
        f.forms.push((0x66_58B0, 0x78_0000, (0, 0), (1920, 1080)));
        f.draws.push(fill(0, 0, 1920, 1080));
        f.form_draws.push((0x78_0000, 7..8));
        assert_eq!(screen_part(&f, f.forms.len() - 1, (1920, 1080)), None);
    }

    #[test]
    fn every_named_screen_keeps_its_native_part_when_a_prompt_is_split_off_outside_it() {
        let build = civ3_sys::Build::PcGames;
        let size = (1920, 1080);
        for &(vtable, name) in civ3_sys::ui_hooks::screen_forms(build) {
            let canvas = 0x710000;
            let base = Frame {
                overlay_draws: 0..0,
                forms: vec![(0x66_B3D8, 1, (0, 0), size), (vtable, canvas, (448, 156), (1024, 768))],
                draws: vec![fill(0, 0, 1920, 1080), fill(448, 156, 1024, 768)],
                form_draws: vec![(1, 0..1), (canvas, 1..2)],
                underlay_draws: vec![0..1],
            };
            let before = active_screen_part(&base, 1, build, size);
            for rect in [[8, 2, 400, 160], [1400, 900, 510, 170], [200, 100, 1500, 600]] {
                let mut frame = base.clone();
                let [x, y, w, h] = rect;
                frame.forms.push((0x66_B240, 0x720000, (x, y), (w, h)));
                frame.draws.push(fill(x, y, w, h));
                frame.form_draws.push((0x720000, 2..3));
                let (first, body) = popup(&frame, 0x720000, Some(1), size).unwrap();
                let (below, above) = split_popup(&frame, first, body);
                assert_eq!(active_screen_part(&below, 1, build, size), before, "{name}, {rect:?}");
                assert_eq!(below.draws, base.draws, "{name}: popup art never joins parent art");
                assert_eq!(above.draws, frame.draws[2..], "{name}: popup remains independently owned");
            }
        }
    }

    #[test]
    fn nested_full_canvas_modals_keep_separate_bounds_and_leave_the_named_screen_unchanged() {
        let size = (1920, 1080);
        let screen_canvas = 0x710000;
        let mut frame = Frame {
            overlay_draws: 0..0,
            forms: vec![(0x66_B3D8, 1, (0, 0), size), (0x66_F420, screen_canvas, (0, 0), size)],
            draws: vec![fill(0, 0, 1920, 1080), fill(448, 156, 1024, 768)],
            form_draws: vec![(1, 0..1), (screen_canvas, 1..2)], underlay_draws: vec![0..1],
        };
        let baseline = active_screen_part(&frame, 1, civ3_sys::Build::PcGames, size);
        let native_rects = [[300, 100, 650, 300], [8, 2, 400, 160], [900, 10, 510, 240]];
        for (index, rect) in native_rects.into_iter().enumerate() {
            let canvas = 0x720000 + index as u32 * 0x1000;
            let [x,y,w,h] = rect;
            frame.forms.push((0x66_B240, canvas, (0, 0), size));
            let start = frame.draws.len();
            frame.draws.push(fill(x,y,w,h));
            frame.form_draws.push((canvas, start..start+1));
            let canvases: Vec<_> = (0..=index).map(|i| 0x720000 + i as u32 * 0x1000).collect();
            let (below, parts) = split_modal_stack(&frame, &canvases, Some(1), size);
            assert_eq!(active_screen_part(&below, 1, civ3_sys::Build::PcGames, size), baseline);
            assert_eq!(below.draws.len(), 2);
            assert_eq!(parts.len(), index + 1);
            for (i, part) in parts.iter().enumerate() {
                assert_eq!(part.rect, native_rects[i], "parent overlay bounds never include children");
                assert_eq!(part.frame.draws.len(), 1);
            }
        }
    }

    /// The forms a retired game composed at 1920×1080 (the bridge's traces
    /// of its end screens, `ui_form`): the main screen form, the map's
    /// overlay, `Main_GUI` and (some of) its buttons, then `screen` and
    /// its own forms; each drew one fill of its rectangle.
    fn after_game(screen: &[(u32, u32, (i32, i32), (i32, i32))]) -> Frame {
        let mut forms = vec![
            (0x66_B3D8, 0x9F_8934, (0, 0), (1920, 1080)),
            (0x67_1218, 0x9F_D80C, (0, 0), (1920, 1080)),
            (0x66_B570, 0x9F_DD84, (0, 0), (1920, 1080)),
            (0x66_56C0, 0xA0_765C, (57, 12), (36, 30)),
            (0x66_56C0, 0xA0_61E0, (94, 12), (34, 30)),
            (0x66_56C0, 0xA0_68B4, (21, 12), (36, 30)),
            (0x66_56C0, 0xA2_1F0C, (836, 1030), (32, 32)),
        ];
        forms.extend_from_slice(screen);
        // The map black, Main_GUI's panels at the edges, the forms.
        let mut draws = vec![fill(0, 0, 1920, 1080), fill(5, 949, 282, 131), fill(1621, 942, 294, 137)];
        let mut form_draws = vec![(0x9F_8934, 0..1), (0x9F_DD84, 1..3)];
        for f in &forms[3..] {
            form_draws.push((f.1, draws.len()..draws.len() + 1));
            draws.push(fill(f.2.0, f.2.1, f.3.0, f.3.1));
        }
        Frame { overlay_draws: 0..0, draws, forms, form_draws, underlay_draws: vec![] }
    }

    const REPLAY: [(u32, u32, (i32, i32), (i32, i32)); 4] = [
        (0x66_E838, 0xC8_87FC, (448, 156), (1024, 768)),
        (0x67_2328, 0xC8_B1B8, (834, 594), (100, 24)),
        (0x67_18E8, 0xC8_C580, (978, 624), (320, 208)),
        (0x66_56C0, 0xC8_CBF4, (1400, 874), (72, 48)),
    ];
    const VICTORY: (u32, u32, (i32, i32), (i32, i32)) = (0x66_B888, 0x22_98AC, (0, 0), (1920, 1080));
    const TOP5: (u32, u32, (i32, i32), (i32, i32)) = (0x66_BF10, 0xA3_2E04, (448, 156), (1024, 768));
    const HISTOGRAPH: (u32, u32, (i32, i32), (i32, i32)) = (0x66_D380, 0xB3_047C, (448, 156), (1024, 768));
    const RANKING: (u32, u32, (i32, i32), (i32, i32)) = (0x66_B700, 0xE0_A1A8, (448, 156), (1024, 768));
    const HALL_OF_FAME: (u32, u32, (i32, i32), (i32, i32)) = (0x66_D1F0, 0xB2_C8AC, (0, 0), (1024, 768));

    #[test]
    fn the_screens_after_a_game_are_screens_placed_as_the_advisors() {
        let build = civ3_sys::Build::PcGames;
        let size = (1920, 1080);
        // The replay, the ranking, the hall of fame: their own rectangle as a
        // part, with what is composed over them (the replay's list, combo box
        // and close button); the frame under them stays below.
        for (screen, name, rect) in [(&REPLAY[..], "Replay", [448, 156, 1024, 768]), (&[RANKING][..], "Ranking", [448, 156, 1024, 768]), (&[HALL_OF_FAME][..], "Hall of Fame", [0, 0, 1024, 768])] {
            let f = after_game(screen);
            let index = f.forms.iter().position(|form| form.0 == screen[0].0).unwrap();
            assert_eq!(active_screen(&f, build), Some((index, name)));
            let (first, part) = active_screen_part(&f, index, build, size).unwrap();
            assert_eq!(part, rect, "{name}");
            assert_eq!(first, 3 + 4, "{name}: from its own first draw, Main_GUI's panels below");
        }
        // Demographics ("Top 5 Cities") and the histograph: the advisors F11
        // and F8, after a game as during one.
        for (form, name) in [(TOP5, "F11"), (HISTOGRAPH, "F8")] {
            let f = after_game(&[form]);
            assert_eq!(active_screen(&f, build), Some((7, name)));
            assert_eq!(active_screen_part(&f, 7, build, size).map(|p| p.1), Some([448, 156, 1024, 768]));
        }
        // The frame round them: Main_GUI's panels, not the ended game's map.
        let f = after_game(&REPLAY);
        let kept = after_game_frame(&f, build);
        assert!(reach(&kept.draws[0]).is_none(), "the main screen form's map");
        assert!(kept.draws[1..].iter().all(|d| reach(d).is_some()));
        // The victory screen is none (its form is the whole window, black
        // round its board): as before, a popup over the rest.
        assert_eq!(active_screen(&after_game(&[VICTORY]), build), None);
        // The hall of fame opened from the title (no Main_GUI) is a part of
        // the title's menu, as before.
        let mut title = after_game(&[HALL_OF_FAME]);
        title.forms.retain(|f| f.0 != 0x66_B570);
        assert_eq!(active_screen(&title, build), None);
    }

    #[test]
    fn a_game_has_ended_from_its_first_end_screen_until_the_title() {
        let build = civ3_sys::Build::PcGames;
        let mut over = GameOver::default();
        let during = after_game(&[]);
        // A running game: its demographics and histograph do not end it.
        for f in [&during, &after_game(&[TOP5]), &after_game(&[HISTOGRAPH]), &during] {
            assert!(!over.update(f, build));
        }
        // Retired: the victory screen, the replay, then demographics and the
        // histograph, the ranking, the hall of fame, all of the game's end.
        for screen in [&[VICTORY][..], &REPLAY[..], &[TOP5], &[HISTOGRAPH], &during.forms[..0], &[RANKING], &[HALL_OF_FAME]] {
            assert!(over.update(&after_game(screen), build), "{screen:x?}");
        }
        assert!(over.ended());
        // The title (Main_GUI no longer composed): over; the next game runs.
        let mut title = after_game(&[]);
        title.forms.retain(|f| f.0 != 0x66_B570);
        assert!(!over.update(&title, build));
        assert!(!over.update(&after_game(&[TOP5]), build));
    }
}

#[cfg(test)]
#[path = "interface_parts/histograph_tests.rs"]
mod histograph_tests;
