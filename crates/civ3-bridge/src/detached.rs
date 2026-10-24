//! Popups opened at a map click that the viewer keeps on its own
//! (`scene::Interface::detached`, docs/world-input.md "Popups the viewer
//! keeps"): the right-click tile information box.
//!
//! In the game that box is modal. `open_tile_info` (PCGames 0x5AA530)
//! draws it and runs `Base_Form::m68_Show_Dialog` (0x6052F0, called at
//! 0x5AAF6D), whose loop pumps the game's messages until the box closes,
//! and while it runs the map takes no input (`[0xCAD384]` routes all of it
//! to the box). With two views of one game that would stop the other view
//! too. So when the map click that opened it came from the viewer, the
//! game opens the box as always; at the first pass of its modal loop the
//! bridge keeps what it shows (the box as the game presented it, frame and
//! all) and closes it the way the player does, with Escape (its key handler
//! 0x5AAFD0). The game is then back in its turn loop, as after a player
//! looked at the box and closed it, and every view plays on. The viewer
//! shows the kept picture in the view the click was made in until it
//! closes it there (`scene::Input::CloseDetached`).
//!
//! `CIV3R_DETACHED_POPUPS=off` leaves the box modal, as before.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::ui::Frame;

/// A popup's picture kept after the game closed the popup.
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    pub serial: u64,
    /// What it showed, in the game's screen pixels, and its rectangle
    /// there (x, y, w, h).
    pub frame: Frame,
    pub rect: [i32; 4],
    pub anchor: scene::PopupAnchor,
    /// The world pixel of the press that opened it.
    pub click: (i32, i32),
}

/// At most this many are kept (a new one beyond lets the oldest go): one
/// per view, and those the viewer has not closed yet.
pub const MAX_KEPT: usize = 4;
/// The interface-only view of the `i`th kept popup is `FIRST_VIEW + i`:
/// above the game's frame and the parts the viewer arranges (256-259),
/// below the game's own popups (their canvases' addresses).
pub const FIRST_VIEW: u32 = 0x1000;

static KEPT: handoff::Latest<Vec<Kept>> = handoff::Latest::new();
static NEXT_SERIAL: AtomicU64 = AtomicU64::new(1);

/// Whether the tile box is kept by the viewer rather than left modal.
pub fn enabled() -> bool {
    static ON: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
        let on = !std::env::var("CIV3R_DETACHED_POPUPS").is_ok_and(|v| v.eq_ignore_ascii_case("off"));
        if !on {
            log::info!("detached popups: off (CIV3R_DETACHED_POPUPS=off): the tile box stays modal");
        }
        on
    });
    *ON
}

/// Keeps a popup's picture; its serial.
pub fn keep(frame: Frame, rect: [i32; 4], anchor: scene::PopupAnchor, click: (i32, i32)) -> u64 {
    let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
    let new = Kept { serial, frame, rect, anchor, click };
    KEPT.update(|kept| {
        let mut kept = kept.cloned().unwrap_or_default();
        if kept.len() >= MAX_KEPT {
            kept.remove(0);
        }
        kept.push(new.clone());
        Some(kept)
    });
    serial
}

/// The viewer closed popup `serial` (`scene::Input::CloseDetached`).
pub fn close(serial: u64) {
    KEPT.update(|kept| kept.map(|kept| kept.iter().filter(|k| k.serial != serial).cloned().collect()));
}

/// Every kept popup goes (a screen opened over the map, another game).
pub fn close_all() {
    if let Some(kept) = KEPT.take().filter(|kept| !kept.is_empty()) {
        log::info!("detached popups: {} closed (the map is no longer shown)", kept.len());
    }
}

/// The popups kept now, oldest first (the `i`th in view `FIRST_VIEW + i`).
pub fn kept() -> Vec<Kept> {
    KEPT.get().unwrap_or_default()
}

/// The scene's entry for the `index`th kept popup.
pub fn entry(index: usize, kept: &Kept) -> scene::DetachedPopup {
    scene::DetachedPopup { view: FIRST_VIEW + index as u32, serial: kept.serial, anchor: kept.anchor, click: kept.click }
}

#[cfg(windows)]
pub use live::{hidden_canvas, on_modal_pass, scoped};

#[cfg(windows)]
mod live {
    use std::cell::Cell;
    use std::sync::atomic::{AtomicU32, Ordering};

    use civ3_sys::Civ3;

    use crate::interface_parts::{self as parts, ClickPopup};

    thread_local! {
        /// How deep the game's thread is in map buttons the viewer sent
        /// (`scoped`): a tile box opened in there was opened by its click.
        static SCOPE: Cell<u32> = const { Cell::new(0) };
        /// The tile box being kept: its form and the modal loop's passes
        /// seen so far.
        static PASSES: Cell<(u32, u32)> = const { Cell::new((0, 0)) };
        /// The box could not be kept: it stays modal until it closes.
        static GAVE_UP: Cell<bool> = const { Cell::new(false) };
    }
    /// The canvas of a tile box the game's thread is about to keep and
    /// close (0: none): the publisher leaves it out of the scene, so the
    /// viewer never takes it for a modal popup of the game's.
    static HIDDEN: AtomicU32 = AtomicU32::new(0);

    /// How many modal passes to wait for the game to present the box before
    /// keeping its canvas alone (without the frame drawn around it).
    const PRESENT_PASSES: u32 = 3;

    /// Runs `f`, a map button the viewer sent, on the game's thread: a
    /// tile box it opens is kept by the viewer (`on_modal_pass`).
    pub fn scoped<R>(game: &Civ3, f: impl FnOnce() -> R) -> R {
        let outer = SCOPE.get() == 0;
        if outer && super::enabled() {
            HIDDEN.store(tile_box_canvas(game).unwrap_or(0), Ordering::Release);
            GAVE_UP.set(false);
        }
        SCOPE.set(SCOPE.get() + 1);
        let result = f();
        SCOPE.set(SCOPE.get() - 1);
        if outer {
            HIDDEN.store(0, Ordering::Release);
            PASSES.set((0, 0));
            GAVE_UP.set(false);
        }
        result
    }

    /// The canvas of a tile box the game shows only until it is kept: not
    /// a popup of the game's for the viewer (0: none).
    pub fn hidden_canvas() -> u32 {
        HIDDEN.load(Ordering::Acquire)
    }

    /// The tile box's canvas (`open_tile_info`'s form at 0xC950A0).
    fn tile_box_canvas(game: &Civ3) -> Option<u32> {
        matches!(game.build, civ3_sys::Build::PcGames).then_some(0xC9_50A0 + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4)
    }

    /// At a pass of the game's modal loop, before it waits for messages
    /// (the pump call at 0x6053B4): if the modal form is a tile box a map
    /// click from the viewer opened, keep its picture and close it; true
    /// if it closed (the loop ends: nothing to wait for).
    pub fn on_modal_pass(game: &Civ3) -> bool {
        if SCOPE.get() == 0 || GAVE_UP.get() || !super::enabled() {
            return false;
        }
        let Some(dialog) = game.dialog_state().ok().flatten() else { return false };
        let form = dialog.modal_form;
        // (Only the box itself, opened from the map: no dialog under it.)
        if form == 0 || dialog.depth != 1 {
            return false;
        }
        let Ok(vtable) = inproc::Ptr32::<u32>::new(form).read() else { return false };
        if parts::click_popup(game.build, vtable, || None) != Some(ClickPopup::TileInfo) {
            return false;
        }
        let canvas = form + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4;
        let passes = match PASSES.get() {
            (f, n) if f == form => n + 1,
            _ => 1,
        };
        PASSES.set((form, passes));
        let Some(press) = crate::world_input::last_press() else { return give_up("no map press") };
        let Some(window) = crate::world_input::game_window() else { return give_up("no game window") };
        let Some(size) = window.client_size().map(|(w, h)| (w as i32, h as i32)) else { return give_up("no window size") };
        // As the game presented it (its frame, drawn straight onto the
        // screen around its canvas, with it), as the viewer would show the
        // modal box; else, once the game had its passes to present it, its
        // canvas alone.
        let shown = crate::ui_trace::committed_frame_now()
            .and_then(|f| parts::split_modal_stack(&f, &[canvas], None, size).1.pop())
            .map(|part| (part.frame, part.rect));
        let (frame, rect) = match shown {
            Some(shown) => shown,
            None if passes < PRESENT_PASSES => return false,
            None => match crate::ui_trace::canvas_frame_now(canvas, vtable) {
                Some(frame) => {
                    let (w, h) = frame.forms.first().map_or((0, 0), |f| f.3);
                    (frame, [0, 0, w, h])
                }
                None => return give_up("nothing drawn"),
            },
        };
        if frame.draws.is_empty() || rect[2] <= 0 || rect[3] <= 0 {
            return give_up("an empty picture");
        }
        let anchor = parts::click_anchor(ClickPopup::TileInfo, press.at, press.tile, crate::world_input::popup_tile(), press.zoomed_out, (rect[2], rect[3]));
        let serial = super::keep(frame, rect, anchor, press.at);
        if !crate::world_input::close_click_popup_now(game) {
            super::close(serial);
            return give_up("Escape did not reach it");
        }
        let closed = game.modal_form().ok().flatten().is_none_or(|f| f != form);
        if !closed {
            // (Still open: the game's loop goes on with it, modal.)
            super::close(serial);
            return give_up("it stayed open after Escape");
        }
        log::info!("detached popups: tile box {serial} kept, {}x{} at {anchor:?} for the press at {:?} (modal pass {passes}), closed in the game", rect[2], rect[3], press.at);
        true
    }

    /// The box stays modal in the game (and shown as such).
    fn give_up(why: &str) -> bool {
        log::warn!("detached popups: the tile box stays modal: {why}");
        HIDDEN.store(0, Ordering::Release);
        GAVE_UP.set(true);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(x: i32) -> scene::PopupAnchor {
        scene::PopupAnchor { world: (x, 0), offset: (0, 0), world_scaled: true }
    }

    #[test]
    fn kept_popups_are_closed_by_serial_and_the_oldest_goes_beyond_the_limit() {
        close_all();
        let first = keep(Frame::default(), [0, 0, 10, 10], anchor(1), (1, 1));
        let serials: Vec<u64> = (0..MAX_KEPT).map(|i| keep(Frame::default(), [0, 0, 10, 10], anchor(i as i32 + 2), (2, 2))).collect();
        let now = kept();
        assert_eq!(now.len(), MAX_KEPT);
        assert!(now.iter().all(|k| k.serial != first), "the oldest let go");
        assert_eq!(now.iter().map(|k| k.serial).collect::<Vec<_>>(), serials);
        assert!(serials.windows(2).all(|w| w[0] < w[1]), "serials increase");
        close(serials[1]);
        let now = kept();
        assert_eq!(now.len(), MAX_KEPT - 1);
        assert_eq!(entry(1, &now[1]), scene::DetachedPopup { view: FIRST_VIEW + 1, serial: serials[2], anchor: anchor(4), click: (2, 2) });
        close_all();
        assert!(kept().is_empty());
    }
}
