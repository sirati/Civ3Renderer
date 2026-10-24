//! Report popups the game does not wait for (`scene::Interface::queued`,
//! docs/modal-input.md "Reports the game does not wait for").
//!
//! In a single player game every popup is modal: `show_popup` (PCGames
//! 0x611460) runs `Base_Form::m68_Show_Dialog` (0x6052F0), whose loop pumps
//! the game's messages until the popup closes. Many popups only report
//! what happened ("The Babylonians are building the Hanging Gardens", a
//! wonder completed elsewhere, war declared on us): their only button is
//! OK, and the code that shows them does not look at the answer. Loading a
//! scenario shows a long run of them, and the game stops at each.
//!
//! The game itself shows exactly these without waiting in an online game:
//! their callers pass `show_popup`'s non-modal flag 0x4000 when
//! `is_online_game` (0x47B5B0) says so (`call 0x47B5B0; neg al; sbb eax,
//! eax; and eax, 0x4000` before `set_text_key_and_flags`, vtable +0x170).
//! Every popup of Main_GUI's advisor summary (0x556560, the `SUMMARY_...`
//! reports, foreign espionage caught, peace and alliances made) goes that
//! way and ignores `show_popup`'s result. [`REPORTS`] lists the text keys
//! of those callers that report something that happened (not those that
//! answer the player's own action, such as "not enough gold"), and only
//! those. A popup is accepted at once only when its key is one of them
//! and, as shown, it has nothing to choose: one button, no choice rows, no
//! list (`classify`).
//!
//! Such a popup opens as always. At the first passes of its modal loop
//! (the pump call at 0x6053B4, `crate::modal`) the bridge keeps the
//! picture the game presented (frame and advisor head included) and
//! presses Enter, which accepts it with its default as the player does.
//! The loop ends, `show_popup` returns what OK returns, and the game goes
//! on. The viewer shows the kept pictures one after the other until the
//! player has read them (`scene::Input::CloseQueued`).
//!
//! The pictures are the game's draw commands for the popup, kept as the
//! modal popups' are (the interface mirror, `crate::interface_parts`), not
//! pixels; drawing popups from their script and text data is not done yet.
//!
//! `CIV3R_ASYNC_POPUPS=off` leaves every popup modal.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::ui::Frame;

/// The generic popup's class (`PopupForm`, `get_popup_form` 0x49FCE0
/// returns one at 0x9B7510, Main_GUI's summary one is at 0x9B4E28).
pub const POPUP_VTABLE: u32 = 0x66_6408;
/// `PopupForm::text_key` (`char*`, set by `set_text_key_and_flags`,
/// 0x610D20 stores it at +0x580; the script file's path is at +0x57C).
pub const TEXT_KEY: u32 = 0x580;

/// The script keys of reports the game shows without waiting in online
/// games (see the module docs): what happened, nothing to answer. Keys
/// beginning with `SUMMARY_` are Main_GUI's advisor summaries (0x556560),
/// all of them reports, some with keys it puts together.
pub const REPORTS: &[&str] = &[
    // Main_GUI's advisor summaries with keys of their own (0x556560).
    "MAKEPEACE",
    "MUTUALPROTECTIONPACT",
    "MILITARYALLIANCE",
    "ESPIONAGE_FOREIGN_EMBASSY_ESTABLISHED",
    "ESPIONAGE_CAUGHT_STEALING_TECH",
    "ESPIONAGE_CAUGHT_STEALING_MAP",
    "ESPIONAGE_CAUGHT_MOLE",
    "ESPIONAGE_CAUGHT_STEALING_PLANS",
    "ESPIONAGE_MISSED_STEALING_PLANS",
    "ESPIONAGE_MISSED_PROPAGANDA_LOST_CITY",
    "ESPIONAGE_MISSED_PROPAGANDA",
    "ESPIONAGE_CAUGHT_SABOTAGING_SUCCESS",
    "ESPIONAGE_CAUGHT_SABOTAGING",
    "ESPIONAGE_MISSED_SABOTAGING",
    "ESPIONAGE_CAUGHT_EXPOSING_MOLE",
    "ESPIONAGE_MISSED_EXPOSING_MOLE",
    // Shown with show_popup directly, non-modal online, result unused.
    "WONDERPRODUCE",
    "CULTUREBORDERVERBOSE",
    "HAVE_LUXURY",
    "HAVE_RESOURCE",
    "HAVE_RESOURCE_NO_UNIT",
    "LOSTGOOD",
    "NOSUPPORT",
    "TRADEEMBARGO",
    "TRADEEMBARGOENDS",
    "THEY_DESTROY_CITY_GOLD",
    // The same caller's other keys (0x5644CC, 0x56461C, 0x564782: the key
    // in a register), those of cities we lost.
    "THEY_CAPTURE_CITY_GOLD",
    "THEY_RAZE_CITY_GOLD",
    "CITY_LOST_WITH_WONDER",
    "CITY_RAZED_WITH_WONDER",
    "DESTROYEDSPACESHIP",
    "THEIRNUKEFAILEDBYOTHERS",
    "THEY_RETURNED_FLAG",
    "THEY_TOOK_FLAG",
];

/// Whether popup text key `key` is a report the game does not need an
/// answer to ([`REPORTS`], or one of Main_GUI's `SUMMARY_` reports).
pub fn is_report(key: &str) -> bool {
    key.starts_with("SUMMARY_") || REPORTS.contains(&key)
}

/// What to do with a popup.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Accepted at once and queued: the one button it has (x, y, w, h on
    /// the screen).
    Queue { button: [i32; 4] },
    /// Left modal, and why.
    Modal(&'static str),
}

/// Whether the popup of class `vtable` showing text key `key`, with the
/// controls the game's hit test finds over it (`civ3_sys::controls`), is
/// accepted at once: a report ([`is_report`]) of the generic popup class
/// with exactly one button (links in its text do not count: they open the
/// Civilopedia) and nothing else to click, no choice rows or lists.
pub fn classify(key: Option<&str>, vtable: u32, controls: &[civ3_sys::controls::Control]) -> Verdict {
    use civ3_sys::controls::ControlKind;
    if vtable != POPUP_VTABLE {
        return Verdict::Modal("not the generic popup");
    }
    let Some(key) = key else { return Verdict::Modal("no text key") };
    if !is_report(key) {
        return Verdict::Modal("not a report");
    }
    let mut button = None;
    for c in controls {
        match c.kind {
            ControlKind::Button if c.text_link() => {}
            ControlKind::Button if c.content_row() => return Verdict::Modal("it has choices"),
            ControlKind::Button if button.is_none() => button = Some(c.rect),
            ControlKind::Button => return Verdict::Modal("more than one button"),
            _ => return Verdict::Modal("it has a list or a slider"),
        }
    }
    match button {
        Some(button) => Verdict::Queue { button },
        None => Verdict::Modal("no button"),
    }
}

/// A report popup the game closed, its picture kept for the viewer.
#[derive(Clone, Debug, PartialEq)]
pub struct Queued {
    pub serial: u64,
    pub key: String,
    /// What it showed, in the game's screen pixels, and its rectangle
    /// there (x, y, w, h).
    pub frame: Frame,
    pub rect: [i32; 4],
    /// Its button, from the rectangle's top-left corner.
    pub button: Option<[i32; 4]>,
}

/// At most this many wait (a new one beyond lets the oldest go).
pub const MAX_QUEUED: usize = 64;
/// The viewer gets the oldest this many: the one to read and the next,
/// which shows behind it.
pub const MAX_SHOWN: usize = 2;
/// The views of the shown ones are `FIRST_VIEW ..= FIRST_VIEW + MAX_SHOWN
/// - 1`, the oldest the highest (views are drawn by id: it is on top).
/// Above the kept tile boxes (`crate::detached::FIRST_VIEW`), below the
/// game's own popups (their canvases' addresses).
pub const FIRST_VIEW: u32 = 0x1100;

static QUEUE: handoff::Latest<Vec<Arc<Queued>>> = handoff::Latest::new();
static NEXT_SERIAL: AtomicU64 = AtomicU64::new(1);

/// Whether report popups are accepted at once.
pub fn enabled() -> bool {
    static ON: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
        let on = !std::env::var("CIV3R_ASYNC_POPUPS").is_ok_and(|v| v.eq_ignore_ascii_case("off"));
        if !on {
            log::info!("async popups: off (CIV3R_ASYNC_POPUPS=off): reports stay modal");
        }
        on
    });
    *ON
}

/// Queues a report's picture; its serial.
pub fn push(key: &str, frame: Frame, rect: [i32; 4], button: Option<[i32; 4]>) -> u64 {
    let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
    let new = Arc::new(Queued { serial, key: key.to_owned(), frame, rect, button });
    QUEUE.update(|queue| {
        let mut queue = queue.cloned().unwrap_or_default();
        if queue.len() >= MAX_QUEUED {
            let gone = queue.remove(0);
            log::warn!("async popups: more than {MAX_QUEUED} waiting: {} ({}) let go unread", gone.serial, gone.key);
        }
        queue.push(new.clone());
        Some(queue)
    });
    serial
}

/// The viewer's player read popup `serial` (`scene::Input::CloseQueued`).
pub fn close(serial: u64) {
    QUEUE.update(|queue| queue.map(|queue| queue.iter().filter(|q| q.serial != serial).cloned().collect()));
}

/// Every queued popup goes.
pub fn close_all() {
    if let Some(queue) = QUEUE.take().filter(|q| !q.is_empty()) {
        log::info!("async popups: {} unread let go", queue.len());
    }
}

/// The oldest [`MAX_SHOWN`] queued popups, oldest first, and how many wait
/// behind them.
pub fn shown() -> (Vec<Arc<Queued>>, u32) {
    let queue = QUEUE.load();
    let all = queue.as_deref().map_or(&[][..], |q| q.as_slice());
    let shown = all.iter().take(MAX_SHOWN).cloned().collect();
    (shown, all.len().saturating_sub(MAX_SHOWN) as u32)
}

/// The view of the `index`th shown popup (0: the oldest, on top).
pub fn view(index: usize) -> u32 {
    FIRST_VIEW + (MAX_SHOWN - 1 - index.min(MAX_SHOWN - 1)) as u32
}

/// The scene's entry for the `index`th shown popup.
pub fn entry(index: usize, queued: &Queued) -> scene::QueuedPopup {
    scene::QueuedPopup { view: view(index), serial: queued.serial, button: queued.button }
}

/// A button's rectangle on the screen as seen from its popup's rectangle.
pub fn relative([bx, by, bw, bh]: [i32; 4], [x, y, ..]: [i32; 4]) -> [i32; 4] {
    [bx - x, by - y, bw, bh]
}

#[cfg(windows)]
pub use live::{hidden_canvas, on_modal_pass, popup_text_key};

#[cfg(windows)]
mod live {
    use std::cell::{Cell, RefCell};
    use std::sync::atomic::{AtomicU32, Ordering};

    use civ3_sys::Civ3;

    use super::Verdict;

    const VK_RETURN: u32 = 0x0D;
    /// How long to wait for the game to present the popup (its frame, title
    /// band and advisor head are drawn straight onto the screen when it
    /// does) before keeping its canvas alone.
    const PRESENT_FOR: std::time::Duration = std::time::Duration::from_millis(1500);

    thread_local! {
        /// The popup being looked at (form, text key) and the modal loop's
        /// passes seen for it.
        static PASSES: RefCell<(u32, String, u32)> = const { RefCell::new((0, String::new(), 0)) };
        /// Since when it is the modal form.
        static OPENED: Cell<Option<std::time::Instant>> = const { Cell::new(None) };
        /// It stays modal: until another popup is the modal form.
        static LEFT: Cell<bool> = const { Cell::new(false) };
        /// Its picture is queued (serial) and Enter sent since (when): the
        /// game may not take keys yet (a popup just opened ignores them
        /// for a moment), so Enter goes again at each pass.
        static ACCEPTING: Cell<Option<(u64, std::time::Instant)>> = const { Cell::new(None) };
    }
    /// How long Enter is sent again before the popup is left modal.
    const ACCEPT_FOR: std::time::Duration = std::time::Duration::from_secs(5);
    /// The canvas of a report the game's thread is about to keep and
    /// accept (0: none): the publisher leaves it out of the scene, so the
    /// viewer never shows it as the game's modal popup.
    static HIDDEN: AtomicU32 = AtomicU32::new(0);

    /// The canvas the publisher leaves out (0: none).
    pub fn hidden_canvas() -> u32 {
        HIDDEN.load(Ordering::Acquire)
    }

    /// The text key of the generic popup `form` (its class is
    /// [`super::POPUP_VTABLE`]: the key is at [`super::TEXT_KEY`]).
    pub fn popup_text_key(form: u32) -> Option<String> {
        let at = inproc::Ptr32::<u32>::new(form + super::TEXT_KEY).read().ok().filter(|&p| p != 0)?;
        let bytes = inproc::Ptr32::<u8>::new(at).read_array(64).ok()?;
        let end = bytes.iter().position(|&b| b == 0)?;
        let key = std::str::from_utf8(&bytes[..end]).ok()?;
        key.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_').then(|| key.to_owned())
    }

    /// At a pass of the game's modal loop, before it waits for messages
    /// (the pump call at 0x6053B4): if the modal form is a report popup
    /// (`super::classify`), keep its picture and accept it with Enter;
    /// true if it closed (the loop ends: nothing to wait for).
    pub fn on_modal_pass(game: &Civ3) -> bool {
        if !super::enabled() {
            return false;
        }
        let Some(dialog) = game.dialog_state().ok().flatten() else { return false };
        // (Online the game already shows reports without waiting.)
        if dialog.modal_form == 0 || dialog.online || dialog.pbem {
            return false;
        }
        let form = dialog.modal_form;
        let Ok(vtable) = inproc::Ptr32::<u32>::new(form).read() else { return false };
        if vtable != super::POPUP_VTABLE {
            return false;
        }
        let key = popup_text_key(form);
        let identity = key.clone().unwrap_or_default();
        let passes = PASSES.with_borrow_mut(|p| {
            if p.0 == form && p.1 == identity {
                p.2 += 1;
            } else {
                *p = (form, identity.clone(), 1);
                LEFT.set(false);
                ACCEPTING.set(None);
                OPENED.set(Some(std::time::Instant::now()));
            }
            p.2
        });
        if LEFT.get() {
            return false;
        }
        if let Some((serial, since)) = ACCEPTING.get() {
            return accept(game, form, &identity, serial, since, passes);
        }
        let Some(sites) = civ3_sys::controls::sites(game.build) else { return false };
        let controls = civ3_sys::controls::controls(&civ3_sys::controls::Live, &sites);
        let button = match super::classify(key.as_deref(), vtable, &controls) {
            Verdict::Queue { button } => button,
            Verdict::Modal(why) => {
                if key.as_deref().is_some_and(super::is_report) {
                    log::info!("async popups: report {} stays modal: {why}", key.as_deref().unwrap_or("?"));
                } else {
                    log::debug!("async popups: popup {} stays modal: {why}", key.as_deref().unwrap_or("?"));
                }
                return leave();
            }
        };
        let key = key.unwrap_or_default();
        let canvas = form + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4;
        HIDDEN.store(canvas, Ordering::Release);
        let Some(window) = crate::world_input::game_window() else { return leave_because(&key, "no game window") };
        let Some(size) = window.client_size().map(|(w, h)| (w as i32, h as i32)) else { return leave_because(&key, "no window size") };
        // As the game presented it, its frame and advisor head with it;
        // else, once the game had its passes to present it, its canvas
        // alone.
        let shown = crate::ui_trace::committed_frame_now()
            .and_then(|f| crate::interface_parts::split_modal_stack(&f, &[canvas], None, size).1.pop())
            .map(|part| (part.frame, part.rect));
        // (Its rectangle in the picture's pixels, and where that is on the
        // screen: the canvas alone is drawn from (0, 0).)
        let (frame, rect, at) = match shown {
            Some((frame, rect)) => (frame, rect, (rect[0], rect[1])),
            None if OPENED.get().is_none_or(|t| t.elapsed() < PRESENT_FOR) => return false,
            None => match crate::ui_trace::canvas_frame_now(canvas, vtable) {
                Some(frame) => {
                    let (w, h) = frame.forms.first().map_or((0, 0), |f| f.3);
                    let at = civ3_sys::controls::form_rect(&civ3_sys::controls::Live, form).map_or((0, 0), |r| (r[0], r[1]));
                    (frame, [0, 0, w, h], at)
                }
                None => return leave_because(&key, "nothing drawn"),
            },
        };
        if frame.draws.is_empty() || rect[2] <= 0 || rect[3] <= 0 {
            return leave_because(&key, "an empty picture");
        }
        let serial = super::push(&key, frame, rect, Some(super::relative(button, [at.0, at.1, rect[2], rect[3]])));
        log::info!("async popups: report {key} ({serial}) queued, {}x{} at ({}, {}), modal pass {passes}", rect[2], rect[3], rect[0], rect[1]);
        let now = std::time::Instant::now();
        ACCEPTING.set(Some((serial, now)));
        accept(game, form, &key, serial, now, passes)
    }

    /// Enter, as the player accepts the popup: its default (OK), what
    /// `show_popup` then returns (0x605E00 -> vtable +0xD4 (-1) ->
    /// 0x6135B0). True once it closed; while the game does not take it,
    /// again at the next pass, for `ACCEPT_FOR`.
    fn accept(game: &Civ3, form: u32, key: &str, serial: u64, since: std::time::Instant, passes: u32) -> bool {
        let Some(window) = crate::world_input::game_window() else { return give_up(key, serial, "no game window") };
        if !window.key_now(VK_RETURN, true) {
            return give_up(key, serial, "Enter did not reach it");
        }
        window.key_now(VK_RETURN, false);
        let closed = game.modal_form().ok().flatten().is_none_or(|f| f != form);
        if !closed {
            if since.elapsed() > ACCEPT_FOR {
                return give_up(key, serial, "it stayed open after Enter");
            }
            if passes % 32 == 0 {
                let flags = |at: u32| inproc::Ptr32::<u32>::new(form + at).read().unwrap_or(0);
                log::debug!("async popups: report {key} not accepted yet (pass {passes}, flags {:#x} {:#x})", flags(0x9C), flags(0xA0));
            }
            return false;
        }
        HIDDEN.store(0, Ordering::Release);
        ACCEPTING.set(None);
        PASSES.with_borrow_mut(|p| *p = (0, String::new(), 0));
        log::info!("async popups: report {key} ({serial}) accepted after {} ms, modal pass {passes}; the game goes on", since.elapsed().as_millis());
        true
    }

    fn give_up(key: &str, serial: u64, why: &str) -> bool {
        super::close(serial);
        ACCEPTING.set(None);
        leave_because(key, why)
    }

    fn leave() -> bool {
        HIDDEN.store(0, Ordering::Release);
        LEFT.set(true);
        false
    }

    fn leave_because(key: &str, why: &str) -> bool {
        log::warn!("async popups: report {key} stays modal: {why}");
        leave()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use civ3_sys::controls::{Control, ControlKind};

    fn button(id: u64, rect: [i32; 4]) -> Control {
        Control { rect, kind: ControlKind::Button, id }
    }

    #[test]
    fn reports_with_only_an_ok_button_are_queued() {
        let ok = button(0x9B_4E28 + 0x5DC, [600, 400, 32, 32]);
        assert_eq!(classify(Some("SUMMARY_TRAVELERS_REPORT"), POPUP_VTABLE, &[ok]), Verdict::Queue { button: [600, 400, 32, 32] });
        assert_eq!(classify(Some("WONDERPRODUCE"), POPUP_VTABLE, &[ok]), Verdict::Queue { button: [600, 400, 32, 32] });
        // A link in the text (the wonder's Civilopedia entry) is no choice.
        let link = button(0x9B_4E28u64 << 32 | 0x1000_0000, [500, 380, 80, 14]);
        assert!(link.text_link());
        assert_eq!(classify(Some("WONDERPRODUCE"), POPUP_VTABLE, &[link, ok]), Verdict::Queue { button: [600, 400, 32, 32] });
    }

    #[test]
    fn popups_with_choices_or_of_other_kinds_stay_modal() {
        let ok = button(0x9B_7510 + 0x5DC, [600, 400, 32, 32]);
        let cancel = button(0x9B_7510 + 0xCB0, [640, 400, 32, 32]);
        // Not a report: the player's own action answered, a question.
        assert_eq!(classify(Some("NOTENOUGHGOLD"), POPUP_VTABLE, &[ok]), Verdict::Modal("not a report"));
        assert_eq!(classify(Some("REALLYQUIT"), POPUP_VTABLE, &[ok, cancel]), Verdict::Modal("not a report"));
        assert_eq!(classify(Some("PALACE_VIEW_UPGRADE"), POPUP_VTABLE, &[ok]), Verdict::Modal("not a report"));
        // "Zoom to city" / "Big deal" rows: choices.
        let row = button(0x9B_7510u64 << 32 | 0x0800_0001, [500, 360, 200, 16]);
        assert!(row.content_row());
        assert_eq!(classify(Some("SUMMARY_TRAVELERS_REPORT"), POPUP_VTABLE, &[row, ok]), Verdict::Modal("it has choices"));
        assert_eq!(classify(Some("SUMMARY_GOLDEN_AGE"), POPUP_VTABLE, &[ok, cancel]), Verdict::Modal("more than one button"));
        let list = Control { rect: [0, 0, 10, 10], kind: ControlKind::List { scroll: [0, 0, 10, 10], step: 1 }, id: 5 };
        assert_eq!(classify(Some("SUMMARY_GOLDEN_AGE"), POPUP_VTABLE, &[list, ok]), Verdict::Modal("it has a list or a slider"));
        assert_eq!(classify(Some("SUMMARY_GOLDEN_AGE"), POPUP_VTABLE, &[]), Verdict::Modal("no button"));
        assert_eq!(classify(None, POPUP_VTABLE, &[ok]), Verdict::Modal("no text key"));
        // The tile box, context menus, screens: other classes.
        assert_eq!(classify(Some("SUMMARY_GOLDEN_AGE"), 0x66_F5C8, &[ok]), Verdict::Modal("not the generic popup"));
    }

    #[test]
    fn the_report_list_holds_reports_only() {
        assert!(is_report("SUMMARY_THEY_DECLARE_WAR_ON_US"));
        assert!(is_report("SUMMARY_TRAVELERS_REPORT"));
        for key in ["VERBOSE_TRAVELERS_REPORT", "WONDERCHANGE", "DAWN_OF_MAN", "LOADGAME", "RETIREWARNING", "BADCITYNAME", "HURRY_NOT_ENOUGH_GOLD", "REVOLUTION"] {
            assert!(!is_report(key), "{key}");
        }
        let mut sorted = REPORTS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), REPORTS.len(), "no key twice");
    }

    #[test]
    fn queued_popups_go_in_order_the_oldest_on_top() {
        close_all();
        let a = push("SUMMARY_A", Frame::default(), [10, 20, 300, 200], Some([5, 6, 32, 32]));
        let b = push("SUMMARY_B", Frame::default(), [10, 20, 300, 200], None);
        let c = push("SUMMARY_C", Frame::default(), [10, 20, 300, 200], None);
        assert!(a < b && b < c, "serials increase");
        let (shown, more) = shown();
        assert_eq!(shown.iter().map(|q| q.serial).collect::<Vec<_>>(), [a, b]);
        assert_eq!(more, 1);
        assert!(view(0) > view(1), "the oldest drawn last, on top");
        assert_eq!(entry(0, &shown[0]), scene::QueuedPopup { view: view(0), serial: a, button: Some([5, 6, 32, 32]) });
        close(a);
        let (shown, more) = super::shown();
        assert_eq!(shown.iter().map(|q| q.serial).collect::<Vec<_>>(), [b, c]);
        assert_eq!(more, 0);
        assert_eq!(relative([110, 220, 32, 16], [100, 200, 300, 100]), [10, 20, 32, 16]);
        close_all();
        assert!(super::shown().0.is_empty());
    }
}
