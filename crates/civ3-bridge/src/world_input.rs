//! Native map interactions from renderer views outside the game's camera.
//!
//! A world move must update the game's hover and go-to target even where
//! its own window has no corresponding client pixel. Private window
//! messages preserve ordering with normal keyboard/UI input; their game
//! thread handler substitutes the tile for the native coordinate helper
//! and calls the native map handlers. Movement rules stay in the game.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Move((i32, i32)),
    Order {
        action: scene::WorldOrderAction,
        at: (i32, i32),
    },
    Button {
        button: scene::Button,
        down: bool,
        double: bool,
        at: (i32, i32),
    },
    /// Close the popup the game opened at a map click, if it is still open
    /// (`scene::Input::DismissPopup`).
    #[cfg_attr(not(windows), allow(dead_code))]
    Dismiss,
}

/// An event's map and viewing player. Stale clicks after loading another
/// game must never select or move a unit in the replacement world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct World {
    tiles: u32,
    seed: u32,
    player: i32,
}

#[derive(Clone, Copy, Debug)]
struct Pending {
    world: World,
    event: Event,
}

/// The events waiting for their private message, handed from the thread
/// posting them to the game's thread without a lock (`handoff::Posted`).
#[derive(Default)]
struct Queue(handoff::Posted<Pending>);

impl Queue {
    fn movement(event: Event) -> u8 {
        match event {
            Event::Move(_) => 1,
            Event::Order {
                action: scene::WorldOrderAction::Preview,
                ..
            } => 2,
            _ => 0,
        }
    }

    /// Which moves may share one message: of one kind, in one world.
    fn key(pending: &Pending) -> Option<u32> {
        let kind = Self::movement(pending.event);
        let w = pending.world;
        (kind != 0).then(|| {
            (w.tiles ^ w.seed.rotate_left(11) ^ (w.player as u32).rotate_left(22)).wrapping_mul(0x9E37_79B1) ^ u32::from(kind)
        })
    }

    /// The boolean says whether a new OS message needs to be posted.
    /// A move can replace only a consecutive, still-pending move in the
    /// same world. Keys, buttons and UI messages call barrier first.
    fn push(&self, pending: Pending) -> (u32, bool) {
        self.0.push(pending, Self::key(&pending))
    }

    fn barrier(&self) {
        self.0.barrier();
    }

    /// The message for `token` was not posted.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn retract(&self, token: u32) {
        self.0.retract(token);
    }

    /// On the game's thread: the events up to `token`'s message, a run of
    /// joined moves as its latest.
    fn take(&self, token: u32, inbox: &mut handoff::Inbox<Pending>) -> Vec<Pending> {
        self.0.take(token, inbox)
    }
}

#[derive(Clone, Copy, Debug)]
struct Preview {
    world: World,
    unit: u32,
    tile: (i32, i32),
    /// The command (`Unit_Mode_Actions`) the tile was last given to: the
    /// go-to, or an order that took the tile in its place.
    mode: i32,
}

impl Preview {
    fn owned_by(self, world: World, unit: u32) -> bool {
        unit != 0 && self.world == world && self.unit == unit
    }
    fn confirms(self, world: World, unit: u32, tile: (i32, i32)) -> bool {
        self.owned_by(world, unit) && self.tile == tile
    }
}

/// Clear a pending gesture regardless of whether the native command can
/// currently receive cancellation. A modal or a changed selected unit must not
/// inherit cancellation intended for the previous map transaction.
/// What becomes of a preselected destination (`Preview`) as the game's
/// command changes (`Hold::of`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hold {
    /// The command waits for the same tile.
    Keep,
    /// An order took the tile in the go-to's place (Move All, Road to, Rail
    /// to, Colony to), or its go-to lost the tile: given the tile again.
    Retarget,
    /// Another unit, world or command (Fortify, Sentry, Disband...), or
    /// none: the preselection is gone, the game's state left as it is.
    Retire,
}

impl Hold {
    /// `current`: the world and selected unit; `mode`, `armed`: the game's
    /// command and its go-to flag; `target`: the go-to's tile
    /// (`field_4E80[3..4]`); `pointer`: the map's pointer tile (the virtual
    /// one), which the other orders take and keys act on: back on the
    /// preselected one after a press made it none (`live::pin`).
    fn of(preview: Preview, current: Option<(World, u32)>, mode: i32, armed: bool, target: Option<(i32, i32)>, pointer: Option<(i32, i32)>) -> Hold {
        if !current.is_some_and(|(world, unit)| preview.owned_by(world, unit)) || !crate::targeting::carries(mode, armed) {
            return Hold::Retire;
        }
        let on_tile = pointer == Some(preview.tile) && (!crate::targeting::go_to(mode) || target == Some(preview.tile));
        if mode == preview.mode && on_tile { Hold::Keep } else { Hold::Retarget }
    }
}

fn retire_preview(
    preview: &mut Option<Preview>,
    current: Option<(World, u32)>,
    safe: bool,
    armed: bool,
) -> bool {
    preview.take().is_some_and(|preview| {
        safe && armed && current.is_some_and(|(world, unit)| preview.owned_by(world, unit))
    })
}

/// What a map press does while a full screen (an advisor, the
/// Civilopedia, the city screen) is open, `crate::modal`. The game's own
/// map handlers would also open cities and menus (a double click zooms
/// into a city, the right button opens the context menu or the
/// Civilopedia, 0x4EB540): those are not reachable from there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScreenPress {
    /// To the game's own handler: the right button cancels the command
    /// waiting for its tile.
    Native,
    /// A left click with go-to armed picks the destination, through the
    /// game's handler (`perform_action_on_tile`), if it is no attack.
    Confirm,
    /// Select the player's unit there (`Unit::select`, no camera move).
    Select,
    /// Send the selected unit there (as with an advisor open, v1).
    SendTo,
    /// A command other than go-to waits for its tile: not taken here.
    Refuse,
    Ignore,
}

fn screen_press(button: scene::Button, down: bool, go_to_armed: bool, other_mode: bool) -> ScreenPress {
    use scene::Button::{Left, Middle, Right};
    match (button, down) {
        (Middle, _) => ScreenPress::Ignore,
        (Right, _) if go_to_armed || other_mode => ScreenPress::Native,
        (Left, true) if other_mode => ScreenPress::Refuse,
        (Left, true) if go_to_armed => ScreenPress::Confirm,
        (Left, true) => ScreenPress::Select,
        (Right, true) => ScreenPress::SendTo,
        (Left | Right, false) => ScreenPress::Ignore,
    }
}

/// A button's bit in a mask of buttons.
#[cfg_attr(not(windows), allow(dead_code))]
fn button_bit(b: scene::Button) -> u8 {
    match b {
        scene::Button::Left => 0,
        scene::Button::Right => 1,
        scene::Button::Middle => 2,
    }
}

/// The Escape key (a Windows virtual-key code).
#[cfg_attr(not(windows), allow(dead_code))]
const VK_ESCAPE: u32 = 0x1B;

fn native_tile(
    at: (i32, i32),
    zoomed_out: bool,
    size: (i32, i32),
    wrap: (bool, bool),
) -> Option<(i32, i32)> {
    civ3_sys::world::tile_under(at, zoomed_out, size, wrap)
}

#[cfg(windows)]
pub use live::{
    attach_window, barrier, click_popup_open, close_click_popup_now, endpoint_ready, game_window, hold_preselection, install, last_press, leave_world,
    pointer_tile, popup_tile, post_button, post_dismiss, post_move, post_order, preselected, ready, wake, Press,
};

#[cfg(test)]
mod tests {
    use super::*;
    const WORLD: World = World {
        tiles: 0x1000,
        seed: 2,
        player: 1,
    };
    fn movement(x: i32) -> Pending {
        Pending {
            world: WORLD,
            event: Event::Move((x, 300)),
        }
    }
    /// What one message delivers, which must be one event.
    fn only(taken: Vec<Pending>) -> Pending {
        assert_eq!(taken.len(), 1, "{taken:?}");
        taken[0]
    }

    #[test]
    fn under_a_screen_presses_select_send_and_confirm_but_never_open_anything() {
        use scene::Button::{Left, Middle, Right};
        use ScreenPress::*;
        // Nothing armed: the left button selects, the right one sends; their
        // releases do nothing (no city zoom, context menu or Civilopedia).
        assert_eq!(screen_press(Left, true, false, false), Select);
        assert_eq!(screen_press(Left, false, false, false), Ignore);
        assert_eq!(screen_press(Right, true, false, false), SendTo);
        assert_eq!(screen_press(Right, false, false, false), Ignore);
        assert_eq!(screen_press(Middle, true, false, false), Ignore);
        // Go-to armed: the left button picks the destination, the right one
        // cancels as the game's own handler does.
        assert_eq!(screen_press(Left, true, true, false), Confirm);
        assert_eq!(screen_press(Left, false, true, false), Ignore);
        assert_eq!(screen_press(Right, true, true, false), Native);
        assert_eq!(screen_press(Right, false, true, false), Native);
        // Another command armed (bombard...): told, or cancelled.
        assert_eq!(screen_press(Left, true, false, true), Refuse);
        assert_eq!(screen_press(Right, true, false, true), Native);
    }

    #[test]
    fn rapid_moves_keep_one_message_with_the_latest_destination() {
        let q = Queue::default();
        let mut inbox = handoff::Inbox::default();
        let (first, post) = q.push(movement(0));
        assert!(post);
        for x in 1..100_000 {
            assert_eq!(q.push(movement(x)), (first, false));
        }
        assert_eq!(only(q.take(first, &mut inbox)).event, Event::Move((99_999, 300)));
        assert!(q.take(first, &mut inbox).is_empty());
        assert!(q.push(movement(100_000)).1);
    }

    #[test]
    fn moves_cannot_cross_keys_buttons_or_ui_messages() {
        let q = Queue::default();
        let mut inbox = handoff::Inbox::default();
        let (before_key, _) = q.push(movement(1));
        q.barrier(); // A key or ordinary UI event was posted here.
        let (after_key, post) = q.push(movement(2));
        assert!(post && before_key != after_key);
        let button = Pending {
            world: WORLD,
            event: Event::Button {
                button: scene::Button::Left,
                down: true,
                double: false,
                at: (2, 300),
            },
        };
        let (press, _) = q.push(button);
        let (after_press, post) = q.push(movement(3));
        assert!(post && after_press != after_key);
        assert_eq!(only(q.take(before_key, &mut inbox)).event, Event::Move((1, 300)));
        assert_eq!(only(q.take(after_key, &mut inbox)).event, Event::Move((2, 300)));
        assert_eq!(only(q.take(press, &mut inbox)).event, button.event);
        assert_eq!(only(q.take(after_press, &mut inbox)).event, Event::Move((3, 300)));
    }

    #[test]
    fn preview_updates_coalesce_but_do_not_replace_plain_hover() {
        let q = Queue::default();
        let mut inbox = handoff::Inbox::default();
        let preview = |x| Pending {
            world: WORLD,
            event: Event::Order {
                action: scene::WorldOrderAction::Preview,
                at: (x, 300),
            },
        };
        let (first, _) = q.push(preview(1));
        for x in 2..10_000 {
            assert_eq!(q.push(preview(x)), (first, false));
        }
        let (plain, post) = q.push(movement(10_000));
        assert!(post && plain != first);
        assert_eq!(only(q.take(first, &mut inbox)).event, preview(9_999).event);
        assert_eq!(only(q.take(plain, &mut inbox)).event, Event::Move((10_000, 300)));
    }

    #[test]
    fn a_confirmation_matches_the_preview_tile_unit_and_loaded_world() {
        let preview = Preview {
            world: WORLD,
            unit: 77,
            tile: (81, 51),
            mode: crate::targeting::GO_TO,
        };
        assert!(preview.confirms(WORLD, 77, (81, 51)));
        assert!(!preview.confirms(WORLD, 0, (81, 51)));
        assert!(!preview.confirms(WORLD, 78, (81, 51)));
        assert!(!preview.confirms(WORLD, 77, (83, 51)));
        assert!(!preview.confirms(
            World {
                tiles: 0x2000,
                ..WORLD
            },
            77,
            (81, 51)
        ));
        assert!(!preview.confirms(World { player: 2, ..WORLD }, 77, (81, 51)));
    }

    #[test]
    fn modal_or_focus_cancellation_retires_the_preview_without_canceling_another_command() {
        let original = Preview {
            world: WORLD,
            unit: 77,
            tile: (81, 51),
            mode: crate::targeting::GO_TO,
        };
        for (current, safe, armed) in [
            (Some((WORLD, 77)), false, true), // A dialog or animation owns input.
            (Some((WORLD, 78)), true, true),  // Another unit has a new G order.
            (Some((WORLD, 77)), true, false), // Another command replaced G.
            (None, true, true),               // The map was unloaded.
        ] {
            let mut pending = Some(original);
            assert!(!retire_preview(&mut pending, current, safe, armed));
            assert!(
                pending.is_none(),
                "a later preview must start a new transaction"
            );
        }
        let mut pending = Some(original);
        assert!(retire_preview(&mut pending, Some((WORLD, 77)), true, true));
        assert!(pending.is_none());
    }

    #[test]
    fn a_preselected_destination_stays_with_the_orders_that_take_its_tile() {
        use crate::targeting::{COLONY_TO, GO_TO, RAIL_TO, ROAD_TO, STACK_GO_TO};
        let tile = (81, 51);
        let preview = Preview { world: WORLD, unit: 77, tile, mode: GO_TO };
        let here = Some((WORLD, 77));
        // Hovering, keys and clicks that change nothing: the go-to keeps it.
        assert_eq!(Hold::of(preview, here, GO_TO, true, Some(tile), Some(tile)), Hold::Keep);
        // Move All (X, Ctrl-X): the stack's go-to, which starts at the
        // unit's own tile (0x4DABC4): given the tile again.
        assert_eq!(Hold::of(preview, here, STACK_GO_TO, true, Some((80, 50)), Some(tile)), Hold::Retarget);
        assert_eq!(Hold::of(Preview { mode: STACK_GO_TO, ..preview }, here, STACK_GO_TO, true, Some(tile), Some(tile)), Hold::Keep);
        // Road to, Rail to, Colony to: their mode, the go-to's flag left set
        // or not; the map's pointer tile is the preselected one.
        for mode in [ROAD_TO, RAIL_TO, COLONY_TO] {
            for armed in [false, true] {
                assert_eq!(Hold::of(preview, here, mode, armed, None, Some(tile)), Hold::Retarget, "{mode:#x}");
                assert_eq!(Hold::of(Preview { mode, ..preview }, here, mode, armed, None, Some(tile)), Hold::Keep, "{mode:#x}");
            }
        }
        // A press on the interface left the map's pointer off the map.
        assert_eq!(Hold::of(preview, here, GO_TO, true, Some(tile), None), Hold::Retarget);
        assert_eq!(Hold::of(Preview { mode: ROAD_TO, ..preview }, here, ROAD_TO, true, None, None), Hold::Retarget);
        // The go-to lost its tile (a hover elsewhere): given it again.
        assert_eq!(Hold::of(preview, here, GO_TO, true, Some((83, 51)), Some(tile)), Hold::Retarget);
        // Another command (Fortify, Sentry, Disband: no mode; bombard 6),
        // the go-to's flag cleared by a map click, another unit or world.
        for (current, mode, armed) in [
            (here, 0, false),
            (here, 0x7F00, false),
            (here, 0x6, false),
            (here, GO_TO, false),
            (here, STACK_GO_TO, false),
            (Some((WORLD, 78)), GO_TO, true),
            (Some((World { seed: 3, ..WORLD }, 77)), GO_TO, true),
            (None, GO_TO, true),
        ] {
            assert_eq!(Hold::of(preview, current, mode, armed, Some(tile), Some(tile)), Hold::Retire, "{current:?} {mode:#x} {armed}");
        }
    }

    #[test]
    fn renderer_world_targets_use_the_current_normal_or_z_coordinate_scale() {
        for tile in [(81, 51), (2, 2), (99, 19), (0, 0)] {
            let normal = (tile.0 * 64 + 64, tile.1 * 32 + 32);
            let half = (normal.0 / 2, normal.1 / 2);
            assert_eq!(
                native_tile(normal, false, (100, 60), (true, false)),
                Some(tile)
            );
            assert_eq!(
                native_tile(half, true, (100, 60), (true, false)),
                Some(tile)
            );
        }
        // Wrapped copies of the same far-off-camera tile remain identical.
        assert_eq!(
            native_tile(
                (151 * 64 + 64, 19 * 32 + 32),
                false,
                (100, 60),
                (true, false)
            ),
            Some((51, 19))
        );
        assert_eq!(
            native_tile(
                (151 * 32 + 32, 19 * 16 + 16),
                true,
                (100, 60),
                (true, false)
            ),
            Some((51, 19))
        );
        assert_eq!(
            native_tile((64, -100), false, (100, 60), (false, false)),
            None
        );
    }

    #[test]
    fn pending_moves_from_a_previous_world_are_separate() {
        let q = Queue::default();
        let mut inbox = handoff::Inbox::default();
        let (old, _) = q.push(movement(1));
        let other = Pending {
            world: World {
                tiles: 0x2000,
                ..WORLD
            },
            ..movement(2)
        };
        let (new, post) = q.push(other);
        assert!(post && old != new);
        assert_eq!(only(q.take(old, &mut inbox)).world, WORLD);
        assert_eq!(only(q.take(new, &mut inbox)).world, other.world);
    }
}

#[cfg(windows)]
mod live {
    use super::{Event, Hold, Pending, Preview, Queue, World, VK_ESCAPE};
    use civ3_sys::world_input as native;
    use civ3_sys::{Build, Civ3};
    use inproc::window::Window;
    use std::cell::{Cell, RefCell};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{LazyLock, OnceLock};

    struct Live {
        game: Civ3,
        message: u32,
    }
    static LIVE: OnceLock<Live> = OnceLock::new();
    /// Filled by the publisher, taken by the game's thread (no lock).
    static QUEUE: LazyLock<Queue> = LazyLock::new(Queue::default);
    static WINDOW: OnceLock<Window> = OnceLock::new();
    /// The world tile under the pointer, as the game's thread last set it.
    static ACTIVE: handoff::Latest<(World, (i32, i32))> = handoff::Latest::new();
    /// Set by another thread to retire the preview at the game thread's
    /// next use of it.
    static RETIRE_PREVIEW: AtomicBool = AtomicBool::new(false);
    static LAST_PRESS: handoff::Latest<Press> = handoff::Latest::new();
    /// The preselected destination's tile, for drawing (`Civ3World::preselected`).
    static PRESELECTED: handoff::Latest<(i32, i32)> = handoff::Latest::new();
    thread_local! {
        /// Received events whose messages are still queued.
        static INBOX: RefCell<handoff::Inbox<Pending>> = RefCell::new(handoff::Inbox::default());
        /// The go-to preview this map transaction armed (game thread only).
        static PREVIEW: Cell<Option<Preview>> = const { Cell::new(None) };
        /// Buttons (`super::button_bit`) whose press only dismissed a popup
        /// opened at a map click: their release does nothing either.
        static SWALLOWED: Cell<u8> = const { Cell::new(0) };
    }

    fn set_preview(preview: Option<Preview>) {
        PREVIEW.set(preview);
        match preview {
            Some(p) => PRESELECTED.publish(p.tile),
            None => PRESELECTED.clear(),
        }
    }

    fn preview() -> Option<Preview> {
        if RETIRE_PREVIEW.swap(false, Ordering::AcqRel) {
            set_preview(None);
        }
        PREVIEW.get()
    }

    fn take_preview() -> Option<Preview> {
        let p = preview();
        set_preview(None);
        p
    }

    /// The tile of the destination a right drag or long press preselected,
    /// while the game's command waits for it (any thread).
    pub fn preselected() -> Option<(i32, i32)> {
        PRESELECTED.get()
    }

    /// Keeps a preselected destination with the game's command (on the
    /// game's thread: its timer, and before each map event). An order that
    /// takes the tile in the go-to's place (Move All, Road to, Rail to,
    /// Colony to, from its button or its key) is given the same tile, as
    /// the pointer resting on it would; any other command ends it, the
    /// game's state left as that command made it (`Hold`).
    pub fn hold_preselection() {
        let (Some(live), Some(window)) = (LIVE.get(), WINDOW.get().copied()) else { return };
        let Some(p) = preview() else { return };
        let game = &live.game;
        // (The map is the game's only in its turn, no dialog open.)
        if crate::modal::modal_open(game) || !game.dialog_state().is_ok_and(|d| d.is_some_and(|d| d.turn_flag && d.depth == 0)) {
            return;
        }
        let Ok(t) = game.targeting() else { return };
        let current = world(game).zip(game.current_unit_ptr().ok());
        match Hold::of(p, current, t.mode, t.armed, game.mouse_tile().ok().flatten(), native::virtual_tile()) {
            Hold::Keep => {}
            Hold::Retire => {
                log::info!("world input: the preselected destination {:?} ends with command {:#x}", p.tile, t.mode);
                set_preview(None);
            }
            Hold::Retarget => {
                let Some(size) = window.client_size() else { return };
                log::info!("world input: command {:#x} takes the preselected destination {:?}", t.mode, p.tile);
                set_preview(Some(Preview { mode: t.mode, ..p }));
                hover(live, p.world, p.tile, native::safe_client_point(size));
            }
        }
    }

    /// While a destination is preselected the game's map takes its pointer
    /// as resting there: a move over the interface keeps the command's
    /// tile, and a press reaching the map itself (beside the interface's
    /// controls) is off the map, never an order to another tile.
    fn pin(preview: Preview, message: u32) {
        if message == WM_MOUSEMOVE {
            native::set_virtual_tile(Some(preview.tile));
        } else {
            native::set_virtual_invalid();
        }
    }

    /// The last map button given to the game's own handlers: the world
    /// pixel and tile, whether zoomed out, when. A menu it opens belongs
    /// there (`interface_parts::popup_anchor`).
    #[derive(Copy, Clone, Debug)]
    pub struct Press {
        pub at: (i32, i32),
        pub tile: (i32, i32),
        pub zoomed_out: bool,
        pub when: std::time::Instant,
    }

    pub fn last_press() -> Option<Press> {
        LAST_PRESS.get()
    }

    /// Whether the game's modal form is a popup it opens at a map click (a
    /// context menu, the tile information box,
    /// `crate::interface_parts::ClickPopup`). While one is open the map's
    /// virtual tile stays the one it was opened for: its entries act on
    /// it (a city's "Terrain details" opens `open_tile_info`, which takes
    /// its tile from the map's pointer conversion).
    const WM_MOUSEMOVE: u32 = 0x0200;

    pub fn click_popup_open(game: &Civ3) -> bool {
        let Some(form) = game.modal_form().ok().flatten().filter(|&f| f != 0) else { return false };
        let Ok(vtable) = inproc::Ptr32::<u32>::new(form).read() else { return false };
        crate::interface_parts::click_popup(game.build, vtable, || {
            inproc::Ptr32::<u32>::new(form + crate::interface_parts::CONTEXT_MENU_FLAGS).read().ok()
        })
        .is_some()
    }

    /// The tile the game's map pointer conversion gives while a popup
    /// opened at a map click is open (the tile it was opened for, which
    /// the tile information box shows), if it is the renderer's.
    pub fn popup_tile() -> Option<(i32, i32)> {
        native::virtual_tile()
    }

    /// A map event the game was not given, and why: logged once a second
    /// per reason (map input in a state where it never arrives is a bug).
    fn dropped(why: &'static str, detail: impl FnOnce() -> String) {
        thread_local! { static LAST: std::cell::RefCell<std::collections::HashMap<&'static str, std::time::Instant>> = Default::default(); }
        let now = std::time::Instant::now();
        let due = LAST.with_borrow_mut(|last| {
            let due = last.get(why).is_none_or(|t| now.duration_since(*t) >= std::time::Duration::from_secs(1));
            if due { last.insert(why, now); }
            due
        });
        if due {
            log::info!("world input: map event not acted on: {why} ({})", detail());
        }
    }

    fn world(game: &Civ3) -> Option<World> {
        let map = game.map().ok()?;
        Some(World {
            tiles: map.tiles_address(),
            seed: map.seed,
            player: game.player_civ().ok()?,
        })
    }

    pub fn install(game: Civ3) -> Result<(), String> {
        if game.build != Build::PcGames {
            return Err(format!("world input is not decoded for {:?}", game.build));
        }
        let message = inproc::message::register("civ3-renderer/virtual-world-input/v1");
        if message == 0 {
            return Err("registering world-input message failed".into());
        }
        native::validate_sites().map_err(|e| e.to_string())?;
        let hook =
            native::install_tile_hook(game.addrs.main_screen_form).map_err(|e| e.to_string())?;
        let recenter = native::install_bring_tile_hook().map_err(|e| e.to_string())?;
        LIVE.set(Live { game, message })
            .map_err(|_| "world input installed twice".to_owned())?;
        std::mem::forget(hook);
        std::mem::forget(recenter);
        log::info!("world input: native hover, selection and targeting beyond the game camera");
        Ok(())
    }

    /// Register once when the hidden game window becomes available.
    pub fn attach_window(window: Window) -> Result<inproc::message::Interceptor, String> {
        let live = LIVE.get().ok_or("world input is not installed")?;
        let interceptor = inproc::message::intercept(window, move |message| {
            if message.id == 0 {
                crate::native_camera::restore_if_requested(live.game);
                crate::ui_trace::refresh_original_drawing(live.game);
            }
            // Input the game reaches only after a stall is dropped, never
            // replayed in a burst (`crate::stale_input`).
            if !crate::stale_input::live::admit_native(message) {
                return Some(0);
            }
            if let Some(result) = crate::native_sliders::handle_message(message) {
                return Some(result);
            }
            if let Some(result) = crate::prefs::handle_message(message) {
                return Some(result);
            }
            if let Some(result) = crate::build_queue::handle_message(message) {
                return Some(result);
            }
            if let Some(result) = crate::ui_wheel::handle_message(message) {
                return Some(result);
            }
            if message.id == live.message {
                // (No borrow lasts into dispatch: native handlers may run a
                // nested message loop that comes back here.)
                let pending = INBOX.with_borrow_mut(|inbox| QUEUE.take(message.wparam as u32, inbox));
                let fresh = |pending: &Pending| match pending.event {
                    Event::Button { button, down, at, .. } => crate::stale_input::live::admit_world_button(button, down, at),
                    _ => true,
                };
                for pending in pending.into_iter().filter(fresh) {
                    dispatch(live, window, pending);
                }
                return Some(0);
            }
            // Ordinary client pointer input restores the game's own pixel
            // conversion. Keyboard input keeps the last world tile so G,
            // bombard and other commands can start where the pointer rests.
            // (Not while a popup opened at a map click is open: its entries
            // act on the tile it was opened for, `click_popup_open`.)
            if (0x0200..=0x020E).contains(&message.id) || message.id == 0x0008 {
                if click_popup_open(&live.game) {
                } else if let Some(preview) = preview() {
                    pin(preview, message.id);
                } else {
                    clear();
                }
            } else if ACTIVE
                .load()
                .is_some_and(|active| Some(active.0) != world(&live.game))
            {
                clear();
            }
            None
        })
        .map_err(|e| e.to_string())?;
        WINDOW
            .set(window)
            .map_err(|_| "world input window attached twice".to_owned())?;
        Ok(interceptor)
    }

    pub fn endpoint_ready() -> bool {
        WINDOW
            .get()
            .is_some_and(|window| inproc::message::is_intercepted(*window))
    }

    pub fn ready(window: Window) -> bool {
        WINDOW.get() == Some(&window) && inproc::message::is_intercepted(window)
    }

    /// Wakes the game's thread if it waits for a message (a screen's loop
    /// has more to do, `crate::modal`).
    pub fn wake() {
        if let Some(window) = WINDOW.get() {
            window.wake();
        }
    }

    fn clear() {
        native::set_virtual_tile(None);
        ACTIVE.clear();
    }

    /// Semantic UI messages such as native sliders do not necessarily
    /// include a normal WM_MOUSEMOVE. Restore client lookup before them.
    pub fn leave_world() {
        clear();
    }

    pub fn pointer_tile() -> Option<(i32, i32)> {
        let live = LIVE.get()?;
        let (old, tile) = ACTIVE.get()?;
        (Some(old) == world(&live.game) && !crate::modal::modal_open(&live.game)).then_some(tile)
    }

    /// End coalescing before posting an ordinary key, button or UI event.
    pub fn barrier() {
        QUEUE.barrier();
    }

    fn post(game: &Civ3, window: Window, event: Event) -> bool {
        let Some(live) = LIVE.get() else { return false };
        if !ready(window) {
            return false;
        }
        let Some(world) = world(game) else {
            return false;
        };
        let (token, send) = QUEUE.push(Pending { world, event });
        if !send {
            return true;
        }
        if window.post_message(live.message, token) {
            return true;
        }
        QUEUE.retract(token);
        log::warn!("world input: the game window rejected an input message");
        false
    }

    pub fn post_move(game: &Civ3, window: Window, at: (i32, i32)) -> bool {
        post(game, window, Event::Move(at))
    }
    pub fn post_dismiss(game: &Civ3, window: Window) -> bool {
        post(game, window, Event::Dismiss)
    }
    pub fn post_button(
        game: &Civ3,
        window: Window,
        button: scene::Button,
        down: bool,
        double: bool,
        at: (i32, i32),
    ) -> bool {
        post(
            game,
            window,
            Event::Button {
                button,
                down,
                double,
                at,
            },
        )
    }
    pub fn post_order(
        game: &Civ3,
        window: Window,
        action: scene::WorldOrderAction,
        at: (i32, i32),
    ) -> bool {
        let posted = post(game, window, Event::Order { action, at });
        if !posted && action == scene::WorldOrderAction::Cancel {
            // Loading a save can temporarily remove the map or window.
            // Retire bookkeeping without calling native code off thread.
            RETIRE_PREVIEW.store(true, Ordering::Release);
        }
        posted
    }

    fn tile(game: &Civ3, at: (i32, i32)) -> Option<(i32, i32)> {
        let map = game.map().ok()?;
        super::native_tile(
            at,
            game.zoomed_out().ok()?,
            (map.width, map.height),
            (map.wrap_x, map.wrap_y),
        )
    }

    fn hover(live: &Live, state: World, tile: (i32, i32), p: (i32, i32)) {
        native::set_virtual_tile(Some(tile));
        ACTIVE.publish((state, tile));
        native::hover(live.game.addrs.main_screen_form, p.0, p.1);
        // The target/path are now the game's own validated values. Refresh
        // the renderer's copied path on this same thread, after hover.
        crate::targeting::on_game_thread();
    }

    fn button(live: &Live, button: scene::Button, down: bool, double: bool, p: (i32, i32)) {
        let screen = live.game.addrs.main_screen_form;
        match (button, down) {
            (scene::Button::Left, true) => native::left_down(screen, p.0, p.1, double, 0),
            (scene::Button::Left, false) => {
                let selecting = live
                    .game
                    .targeting()
                    .is_ok_and(|t| !crate::targeting::active(t.mode));
                let _camera = selecting.then(native::suppress_recenter);
                native::left_up(screen, p.0, p.1, 0);
            }
            (scene::Button::Right, true) => native::right_down(screen, p.0, p.1, double, 0),
            (scene::Button::Right, false) => native::right_up(screen, p.0, p.1, 0),
            (scene::Button::Middle, _) => {}
        }
    }

    /// A map press while a full screen is open (`super::screen_press`).
    fn screen_button(live: &Live, b: scene::Button, down: bool, tile: (i32, i32), p: (i32, i32)) {
        let game = &live.game;
        let (go_to_armed, other_mode) = game.targeting().map_or((false, false), |t| {
            let go_to = matches!(t.mode, crate::targeting::GO_TO | crate::targeting::STACK_GO_TO);
            (go_to && t.armed, crate::targeting::active(t.mode) && !go_to)
        });
        let press = super::screen_press(b, down, go_to_armed, other_mode);
        log::debug!("world input under a screen: {b:?} {} at {tile:?}: {press:?}", if down { "down" } else { "up" });
        let result = match press {
            super::ScreenPress::Ignore => return,
            super::ScreenPress::Native => {
                button(live, b, down, false, p);
                crate::targeting::on_game_thread();
                return;
            }
            super::ScreenPress::Refuse => Err(crate::modal::Refused::NotGoTo),
            super::ScreenPress::Confirm => crate::modal::may_enter_now(tile).map(|()| {
                // (The path the game showed, before the click retires it.)
                let path = crate::targeting::latest_path().filter(|path| path.to == tile);
                button(live, scene::Button::Left, true, false, p);
                button(live, scene::Button::Left, false, false, p);
                crate::modal::after_world_order(path);
                format!("go-to destination {tile:?} picked")
            }),
            super::ScreenPress::Select => crate::modal::run_now(crate::modal::Action::Select(tile)),
            super::ScreenPress::SendTo => crate::modal::run_now(crate::modal::Action::GoTo(tile)),
        };
        match result {
            Ok(done) => log::info!("world input under a screen: {done}"),
            Err(refused) => {
                log::debug!("world input under a screen: {press:?} at {tile:?}: {refused}");
                crate::modal::refuse(game, tile, &refused);
            }
        }
        crate::targeting::on_game_thread();
    }

    fn cancel_native(live: &Live, window: Window) {
        let Some(size) = window.client_size() else { return };
        let p = native::safe_client_point(size);
        // Civ3's Escape key opens the quit dialog even while G is armed.
        // Its ordinary right click cancels G without issuing an order or
        // opening that dialog. Keep the native press and release paired.
        button(live, scene::Button::Right, true, false, p);
        button(live, scene::Button::Right, false, false, p);
        crate::targeting::on_game_thread();
    }

    fn cancel(window: Window, safe: bool) {
        let current = LIVE
            .get()
            .and_then(|live| Some((world(&live.game)?, live.game.current_unit_ptr().ok()?)));
        let armed = LIVE.get().is_some_and(|live| live.game.targeting().is_ok_and(|t| crate::targeting::carries(t.mode, t.armed)));
        let mut retiring = take_preview();
        let cancel_order = super::retire_preview(
            &mut retiring,
            current,
            safe,
            armed,
        );
        if cancel_order {
            cancel_native(LIVE.get().unwrap(), window);
        }
    }

    /// Closes the popup the game opened at a map click the way its player
    /// does, with Escape delivered synchronously; whether one was
    /// open. Only from the game's thread: the check and the key are in
    /// order with that loop's messages, so the key never reaches the map.
    fn dismiss_click_popup(game: &Civ3, window: Window) -> bool {
        if !click_popup_open(game) {
            return false;
        }
        if !window.key_now(VK_ESCAPE, true) { return false; }
        window.key_now(VK_ESCAPE, false);
        true
    }

    /// The game's window, once attached (on any thread; its input only on
    /// the game's).
    pub fn game_window() -> Option<Window> {
        WINDOW.get().copied()
    }

    /// Closes the popup the game opened at a map click now, as the player
    /// does (`dismiss_click_popup`); from the game's thread only.
    pub fn close_click_popup_now(game: &Civ3) -> bool {
        game_window().is_some_and(|window| dismiss_click_popup(game, window))
    }

    fn dispatch(live: &Live, window: Window, pending: Pending) {
        let game = &live.game;
        if pending.event == Event::Dismiss {
            if dismiss_click_popup(game, window) {
                log::info!("world input: the popup opened at a map click dismissed");
            }
            return;
        }
        // The release of a press that only dismissed such a popup: nothing.
        if let Event::Button { button: b, down: false, .. } = pending.event {
            let bit = 1 << super::button_bit(b);
            if SWALLOWED.get() & bit != 0 {
                SWALLOWED.set(SWALLOWED.get() & !bit);
                return;
            }
        }
        if let Event::Order {
            action: scene::WorldOrderAction::Cancel,
            ..
        } = pending.event
        {
            // Cancellation always retires the transaction, even if a dialog
            // opened after the viewer queued it. Native cancellation belongs
            // only to our active map command, never to a newly opened dialog.
            let safe = world(game) == Some(pending.world)
                && if crate::modal::modal_open(game) {
                    // (A screen the map may be played under, only.)
                    crate::modal::screen_gate().is_ok()
                } else {
                    crate::modal::native_input_allowed()
                        && game
                            .dialog_state()
                            .is_ok_and(|d| d.is_some_and(|d| d.turn_flag && d.depth == 0))
                };
            cancel(window, safe);
            return;
        }
        if world(game) != Some(pending.world) {
            dropped("the event's world (map, seed, viewing player) is not the game's", || format!("event {:?}, game {:?}", pending.world, world(game)));
            take_preview();
            clear();
            return;
        }
        // Unlike the old fallback, never click the minimap just to make a
        // renderer-visible tile reachable. Ordinary gameplay uses the same
        // map handlers at the supplied tile, with the existing cameras.
        // Under a full screen the map is played from the screen's loop
        // (`crate::modal::screen_gate`); a press refused there is explained
        // on the map.
        let under_screen = crate::modal::modal_open(game);
        if under_screen && click_popup_open(game) {
            // A press on the world while a context menu or the tile box is
            // open closes it, as a click beside it does in the game, and
            // does nothing else (no notice: those are for full screens).
            // The map's tile stays the popup's.
            if let Event::Button { button: b, down: true, .. } = pending.event
                && dismiss_click_popup(game, window)
            {
                SWALLOWED.set(SWALLOWED.get() | 1 << super::button_bit(b));
                log::info!("world input: a map press dismissed the popup opened at a map click");
            }
            return;
        }
        if under_screen {
            if let Err(refused) = crate::modal::screen_gate() {
                if let Event::Button { down: true, at, .. } | Event::Order { action: scene::WorldOrderAction::Confirm, at } = pending.event
                    && let Some(t) = tile(game, at)
                {
                    crate::modal::refuse(game, t, &refused);
                }
                clear();
                return;
            }
        } else {
            if !crate::modal::native_input_allowed() {
                dropped("not in the human-turn loop's pump, or an animation or action runs", || format!("{}; game calls {}", crate::modal::pump_state(), crate::modal::game_calls_on_stack()));
                return;
            }
            let Some(dialog) = game.dialog_state().ok().flatten() else {
                dropped("the game's dialog state is unreadable", String::new);
                return;
            };
            if !dialog.turn_flag || dialog.depth != 0 {
                dropped("not the human player's turn at the top level", || format!("{dialog:?}"));
                return;
            }
        }
        let Some((width, height)) = window.client_size() else {
            return;
        };
        let p = native::safe_client_point((width, height));
        let at = match pending.event {
            Event::Move(at) | Event::Button { at, .. } | Event::Order { at, .. } => at,
            Event::Dismiss => return,
        };
        let Some(tile) = tile(game, at) else {
            clear();
            native::set_virtual_invalid();
            native::hover(game.addrs.main_screen_form, p.0, p.1);
            if let Err(e) = native::clear_pointer_target(game.addrs.main_screen_form) {
                log::debug!("world input: clearing off-map target: {e}");
            }
            crate::targeting::on_game_thread();
            return;
        };
        match pending.event {
            // (A key's point: with a destination preselected the key acts on it.)
            Event::Move(_) => match preview() {
                Some(preview) => native::set_virtual_tile(Some(preview.tile)),
                None => hover(live, pending.world, tile, p),
            },
            Event::Button {
                button: b,
                down,
                double,
                at,
            } => {
                hover(live, pending.world, tile, p);
                // (Before the handler: a menu it opens runs its own loop.)
                LAST_PRESS.publish(Press {
                    at,
                    tile,
                    zoomed_out: game.zoomed_out().unwrap_or(false),
                    when: std::time::Instant::now(),
                });
                if under_screen {
                    screen_button(live, b, down, tile, p);
                } else {
                    // (A tile box it opens is the viewer's to keep.)
                    crate::detached::scoped(game, || button(live, b, down, double, p));
                }
            }
            Event::Order {
                action: scene::WorldOrderAction::Preview,
                ..
            } => {
                hold_preselection();
                let Ok(unit) = game.current_unit_ptr() else {
                    return;
                };
                if unit == 0 {
                    cancel(window, true);
                    return;
                }
                let old = preview();
                if old.is_some_and(|old| old.world != pending.world || old.unit != unit) {
                    cancel(window, true);
                    return;
                }
                if old.is_none() {
                    // Native G follows the same ability/movement checks as
                    // the key the player presses. It does not issue a move.
                    // (Under a screen, the go-to command G gives: the map's
                    // keys are off while the city screen shows.)
                    if under_screen {
                        if let Err(refused) = crate::modal::run_now(crate::modal::Action::ArmGoTo) {
                            crate::modal::refuse(game, tile, &refused);
                        }
                    } else {
                        native::key(game.addrs.main_screen_form, u32::from(b'g'), 0x47);
                    }
                }
                let Ok(targeting) = game.targeting() else {
                    return;
                };
                if !crate::targeting::carries(targeting.mode, targeting.armed) {
                    return;
                }
                set_preview(Some(Preview {
                    world: pending.world,
                    unit,
                    tile,
                    mode: targeting.mode,
                }));
                hover(live, pending.world, tile, p);
            }
            Event::Order {
                action: scene::WorldOrderAction::Confirm,
                ..
            } => {
                let preview = take_preview();
                let valid = preview.is_some_and(|preview| {
                    preview.confirms(pending.world, game.current_unit_ptr().unwrap_or(0), tile)
                });
                // (The go-to, or an order that took its tile: the map's click
                // gives the game's own `perform_action_on_tile` its mode.)
                let mode = game.targeting().map_or(0, |t| if crate::targeting::carries(t.mode, t.armed) { t.mode } else { 0 });
                let armed = mode != 0;
                let attack = under_screen && valid && crate::targeting::go_to(mode) && match crate::modal::may_enter_now(tile) {
                    Ok(()) => false,
                    Err(refused) => {
                        crate::modal::refuse(game, tile, &refused);
                        true
                    }
                };
                if valid && armed && !attack {
                    hover(live, pending.world, tile, p);
                    let path = crate::targeting::latest_path().filter(|path| path.to == tile);
                    button(live, scene::Button::Left, true, false, p);
                    button(live, scene::Button::Left, false, false, p);
                    if under_screen {
                        crate::modal::after_world_order(path);
                    }
                } else if armed
                    && preview.is_some_and(|preview| {
                        preview.owned_by(pending.world, game.current_unit_ptr().unwrap_or(0))
                    })
                {
                    cancel_native(live, window);
                }
            }
            Event::Order {
                action: scene::WorldOrderAction::Cancel,
                ..
            }
            | Event::Dismiss => unreachable!(),
        }
    }
}
