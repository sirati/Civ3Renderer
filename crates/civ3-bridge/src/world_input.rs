//! Native map interactions from renderer views outside the game's camera.
//!
//! A world move must update the game's hover and go-to target even where
//! its own window has no corresponding client pixel. Private window
//! messages preserve ordering with normal keyboard/UI input; their game
//! thread handler substitutes the tile for the native coordinate helper
//! and calls the native map handlers. Movement rules stay in the game.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Move((i32, i32)),
    Order {
        action: scene::UnitOrderAction,
        at: (i32, i32),
    },
    Button {
        button: scene::Button,
        down: bool,
        double: bool,
        at: (i32, i32),
    },
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

#[derive(Default)]
struct Queue {
    next: u32,
    events: HashMap<u32, Pending>,
    last_move: Option<u32>,
}

impl Queue {
    fn movement(event: Event) -> u8 {
        match event {
            Event::Move(_) => 1,
            Event::Order {
                action: scene::UnitOrderAction::Preview,
                ..
            } => 2,
            _ => 0,
        }
    }

    /// The boolean says whether a new OS message needs to be posted.
    /// A move can replace only a consecutive, still-pending move in the
    /// same world. Keys, buttons and UI messages call barrier first.
    fn push(&mut self, pending: Pending) -> (u32, bool) {
        if Self::movement(pending.event) != 0 {
            if let Some(token) = self.last_move
                && let Some(old) = self.events.get_mut(&token)
                && old.world == pending.world
                && Self::movement(old.event) == Self::movement(pending.event)
            {
                *old = pending;
                return (token, false);
            }
        }
        self.barrier();
        loop {
            self.next = self.next.wrapping_add(1).max(1);
            if !self.events.contains_key(&self.next) {
                break;
            }
        }
        let token = self.next;
        if Self::movement(pending.event) != 0 {
            self.last_move = Some(token);
        }
        self.events.insert(token, pending);
        (token, true)
    }

    fn barrier(&mut self) {
        self.last_move = None;
    }

    fn take(&mut self, token: u32) -> Option<Pending> {
        if self.last_move == Some(token) {
            self.last_move = None;
        }
        self.events.remove(&token)
    }
}

#[derive(Clone, Copy, Debug)]
struct Preview {
    world: World,
    unit: u32,
    tile: (i32, i32),
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
/// currently receive Escape. A modal or a changed selected unit must not
/// inherit cancellation intended for the previous map transaction.
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
    attach_window, barrier, endpoint_ready, install, leave_world, pointer_tile, post_button,
    post_move, post_order, ready,
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

    #[test]
    fn rapid_moves_keep_one_message_with_the_latest_destination() {
        let mut q = Queue::default();
        let (first, post) = q.push(movement(0));
        assert!(post);
        for x in 1..100_000 {
            assert_eq!(q.push(movement(x)), (first, false));
        }
        assert_eq!(q.events.len(), 1);
        assert_eq!(q.take(first).unwrap().event, Event::Move((99_999, 300)));
        assert!(q.take(first).is_none());
        assert!(q.push(movement(100_000)).1);
    }

    #[test]
    fn moves_cannot_cross_keys_buttons_or_ui_messages() {
        let mut q = Queue::default();
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
        assert_eq!(q.take(before_key).unwrap().event, Event::Move((1, 300)));
        assert_eq!(q.take(after_key).unwrap().event, Event::Move((2, 300)));
        assert_eq!(q.take(press).unwrap().event, button.event);
        assert_eq!(q.take(after_press).unwrap().event, Event::Move((3, 300)));
    }

    #[test]
    fn preview_updates_coalesce_but_do_not_replace_plain_hover() {
        let mut q = Queue::default();
        let preview = |x| Pending {
            world: WORLD,
            event: Event::Order {
                action: scene::UnitOrderAction::Preview,
                at: (x, 300),
            },
        };
        let (first, _) = q.push(preview(1));
        for x in 2..10_000 {
            assert_eq!(q.push(preview(x)), (first, false));
        }
        let (plain, post) = q.push(movement(10_000));
        assert!(post && plain != first);
        assert_eq!(q.take(first).unwrap().event, preview(9_999).event);
        assert_eq!(q.take(plain).unwrap().event, Event::Move((10_000, 300)));
    }

    #[test]
    fn a_confirmation_matches_the_preview_tile_unit_and_loaded_world() {
        let preview = Preview {
            world: WORLD,
            unit: 77,
            tile: (81, 51),
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
    fn modal_or_focus_cancellation_retires_the_preview_without_escaping_another_command() {
        let original = Preview {
            world: WORLD,
            unit: 77,
            tile: (81, 51),
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
        let mut q = Queue::default();
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
        assert_eq!(q.take(old).unwrap().world, WORLD);
        assert_eq!(q.take(new).unwrap().world, other.world);
    }
}

#[cfg(windows)]
mod live {
    use super::{Event, Pending, Preview, Queue, World};
    use civ3_sys::world_input as native;
    use civ3_sys::{Build, Civ3};
    use inproc::window::Window;
    use std::sync::{Mutex, OnceLock};

    struct Live {
        game: Civ3,
        message: u32,
    }
    static LIVE: OnceLock<Live> = OnceLock::new();
    static QUEUE: Mutex<Option<Queue>> = Mutex::new(None);
    static WINDOW: OnceLock<Window> = OnceLock::new();
    static ACTIVE: Mutex<Option<(World, (i32, i32))>> = Mutex::new(None);
    static PREVIEW: Mutex<Option<Preview>> = Mutex::new(None);

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
            if message.id == 0 { crate::native_camera::restore_if_requested(live.game); }
            if let Some(result) = crate::native_sliders::handle_message(message) {
                return Some(result);
            }
            if message.id == live.message {
                let pending = QUEUE
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_mut()
                    .and_then(|q| q.take(message.wparam as u32));
                if let Some(pending) = pending {
                    dispatch(live, window, pending);
                }
                return Some(0);
            }
            // Ordinary client pointer input restores the game's own pixel
            // conversion. Keyboard input keeps the last world tile so G,
            // bombard and other commands can start where the pointer rests.
            if (0x0200..=0x020E).contains(&message.id) || message.id == 0x0008 {
                clear();
            } else if ACTIVE
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .is_some_and(|(old, _)| Some(*old) != world(&live.game))
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

    fn clear() {
        native::set_virtual_tile(None);
        *ACTIVE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Semantic UI messages such as native sliders do not necessarily
    /// include a normal WM_MOUSEMOVE. Restore client lookup before them.
    pub fn leave_world() {
        clear();
    }

    pub fn pointer_tile() -> Option<(i32, i32)> {
        let live = LIVE.get()?;
        let active = *ACTIVE.lock().ok()?;
        let (old, tile) = active?;
        (Some(old) == world(&live.game) && !crate::modal::modal_open(&live.game)).then_some(tile)
    }

    /// End coalescing before posting an ordinary key, button or UI event.
    pub fn barrier() {
        if let Some(q) = QUEUE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            q.barrier();
        }
    }

    fn post(game: &Civ3, window: Window, event: Event) -> bool {
        let Some(live) = LIVE.get() else { return false };
        if !ready(window) {
            return false;
        }
        let Some(world) = world(game) else {
            return false;
        };
        let (token, send) = QUEUE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert_with(Queue::default)
            .push(Pending { world, event });
        if !send {
            return true;
        }
        if window.post_message(live.message, token) {
            return true;
        }
        QUEUE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
            .and_then(|q| q.take(token));
        log::warn!("world input: the game window rejected an input message");
        false
    }

    pub fn post_move(game: &Civ3, window: Window, at: (i32, i32)) -> bool {
        post(game, window, Event::Move(at))
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
        action: scene::UnitOrderAction,
        at: (i32, i32),
    ) -> bool {
        let posted = post(game, window, Event::Order { action, at });
        if !posted && action == scene::UnitOrderAction::Cancel {
            // Loading a save can temporarily remove the map or window.
            // Retire bookkeeping without calling native code off thread.
            PREVIEW.lock().unwrap_or_else(|e| e.into_inner()).take();
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
        *ACTIVE.lock().unwrap_or_else(|e| e.into_inner()) = Some((state, tile));
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

    fn cancel(_window: Window, safe: bool) {
        let current = LIVE
            .get()
            .and_then(|live| Some((world(&live.game)?, live.game.current_unit_ptr().ok()?)));
        let armed = LIVE.get().is_some_and(|live| {
            live.game.targeting().is_ok_and(|t| {
                t.armed
                    && matches!(
                        t.mode,
                        crate::targeting::GO_TO | crate::targeting::STACK_GO_TO
                    )
            })
        });
        let send_escape = super::retire_preview(
            &mut PREVIEW.lock().unwrap_or_else(|e| e.into_inner()),
            current,
            safe,
            armed,
        );
        if send_escape {
            let live = LIVE.get().unwrap();
            native::key(live.game.addrs.main_screen_form, 0x1B, 0x1B);
        }
    }

    fn dispatch(live: &Live, window: Window, pending: Pending) {
        let game = &live.game;
        if let Event::Order {
            action: scene::UnitOrderAction::Cancel,
            ..
        } = pending.event
        {
            // Cancellation always retires the transaction, even if a dialog
            // opened after the viewer queued it. Escape belongs only to our
            // still-active map command, never to a newly opened dialog.
            let safe = world(game) == Some(pending.world)
                && !crate::modal::modal_open(game)
                && crate::modal::native_input_allowed()
                && game
                    .dialog_state()
                    .is_ok_and(|d| d.is_some_and(|d| d.turn_flag && d.depth == 0));
            cancel(window, safe);
            return;
        }
        if world(game) != Some(pending.world) {
            PREVIEW.lock().unwrap_or_else(|e| e.into_inner()).take();
            clear();
            return;
        }
        // Unlike the old fallback, never click the minimap just to make a
        // renderer-visible tile reachable. Ordinary gameplay uses the same
        // map handlers at the supplied tile, with the existing cameras.
        if crate::modal::modal_open(game) {
            clear();
            return;
        }
        if !crate::modal::native_input_allowed() {
            return;
        }
        let Some(dialog) = game.dialog_state().ok().flatten() else {
            return;
        };
        if !dialog.turn_flag || dialog.depth != 0 {
            return;
        }
        let Some((width, height)) = window.client_size() else {
            return;
        };
        let p = native::safe_client_point((width, height));
        let at = match pending.event {
            Event::Move(at) | Event::Button { at, .. } | Event::Order { at, .. } => at,
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
            Event::Move(_) => hover(live, pending.world, tile, p),
            Event::Button {
                button: b,
                down,
                double,
                ..
            } => {
                hover(live, pending.world, tile, p);
                button(live, b, down, double, p);
            }
            Event::Order {
                action: scene::UnitOrderAction::Preview,
                ..
            } => {
                let Ok(unit) = game.current_unit_ptr() else {
                    return;
                };
                if unit == 0 {
                    cancel(window, true);
                    return;
                }
                let old = *PREVIEW.lock().unwrap_or_else(|e| e.into_inner());
                if old.is_some_and(|old| old.world != pending.world || old.unit != unit) {
                    cancel(window, true);
                    return;
                }
                if old.is_none() {
                    // Native G follows the same ability/movement checks as
                    // the key the player presses. It does not issue a move.
                    native::key(game.addrs.main_screen_form, u32::from(b'g'), 0x47);
                }
                let Ok(targeting) = game.targeting() else {
                    return;
                };
                if !targeting.armed
                    || !matches!(
                        targeting.mode,
                        crate::targeting::GO_TO | crate::targeting::STACK_GO_TO
                    )
                {
                    return;
                }
                *PREVIEW.lock().unwrap_or_else(|e| e.into_inner()) = Some(Preview {
                    world: pending.world,
                    unit,
                    tile,
                });
                hover(live, pending.world, tile, p);
            }
            Event::Order {
                action: scene::UnitOrderAction::Confirm,
                ..
            } => {
                let preview = PREVIEW.lock().unwrap_or_else(|e| e.into_inner()).take();
                let valid = preview.is_some_and(|preview| {
                    preview.confirms(pending.world, game.current_unit_ptr().unwrap_or(0), tile)
                });
                let armed = game.targeting().is_ok_and(|t| {
                    t.armed
                        && matches!(
                            t.mode,
                            crate::targeting::GO_TO | crate::targeting::STACK_GO_TO
                        )
                });
                if valid && armed {
                    hover(live, pending.world, tile, p);
                    button(live, scene::Button::Left, true, false, p);
                    button(live, scene::Button::Left, false, false, p);
                } else if armed
                    && preview.is_some_and(|preview| {
                        preview.owned_by(pending.world, game.current_unit_ptr().unwrap_or(0))
                    })
                {
                    native::key(game.addrs.main_screen_form, 0x1B, 0x1B);
                }
            }
            Event::Order {
                action: scene::UnitOrderAction::Cancel,
                ..
            } => unreachable!(),
        }
    }
}
