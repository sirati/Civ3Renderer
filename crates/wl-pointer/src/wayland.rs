//! The constraints over winit's Wayland connection (the only unsafe code:
//! taking winit's `wl_display` and `wl_surface`).

use std::sync::Arc;
use std::time::{Duration, Instant};

use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_compositor::WlCompositor, wl_pointer::WlPointer, wl_region::WlRegion, wl_registry::WlRegistry, wl_seat::WlSeat, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::wp::pointer_constraints::zv1::client::zwp_confined_pointer_v1::{self, ZwpConfinedPointerV1};
use wayland_protocols::wp::pointer_constraints::zv1::client::zwp_locked_pointer_v1::{self, ZwpLockedPointerV1};
use wayland_protocols::wp::pointer_constraints::zv1::client::zwp_pointer_constraints_v1::{Lifetime, ZwpPointerConstraintsV1};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use winit::window::Window;

use crate::{Rect, Unavailable};

/// How long a move waits for the compositor to lock the pointer (it does
/// not while the pointer is outside the window, or the window not focused).
const LOCK_WAIT: Duration = Duration::from_millis(300);

/// The region the pointer is confined to: the window (surface size) less
/// the holes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Region {
    size: (i32, i32),
    holes: Vec<Rect>,
}

/// A move under way: the pointer locked (asked to be), the hint set.
struct Moving {
    lock: ZwpLockedPointerV1,
    id: u64,
    to: (f64, f64),
    since: Instant,
}

/// What the compositor said (by constraint: the ids they were made with).
#[derive(Default)]
struct State {
    confined: Option<u64>,
    locked: Option<u64>,
}

/// Pointer constraints for one winit window (see the crate's docs). All of
/// it on the thread running winit's event loop; [`Constraints::dispatch`]
/// on every turn of the loop.
pub struct Constraints {
    queue: EventQueue<State>,
    state: State,
    conn: Connection,
    surface: WlSurface,
    compositor: WlCompositor,
    pointer: WlPointer,
    constraints: ZwpPointerConstraintsV1,
    /// The confinement made, and the region it is to have (none: not
    /// confined).
    confined: Option<(ZwpConfinedPointerV1, u64)>,
    wanted: Option<Region>,
    moving: Option<Moving>,
    next_id: u64,
    /// Keeps winit's connection and surface alive (dropped last).
    _window: Arc<Window>,
}

impl Constraints {
    /// Pointer constraints for `window` (its first seat's pointer), where
    /// it is a Wayland window and the compositor has them.
    pub fn new(window: Arc<Window>) -> Result<Self, Unavailable> {
        let display = window.display_handle().map_err(|_| Unavailable::NotWayland)?.as_raw();
        let handle = window.window_handle().map_err(|_| Unavailable::NotWayland)?.as_raw();
        let (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(handle)) = (display, handle) else {
            return Err(Unavailable::NotWayland);
        };
        // SAFETY: the pointers are winit's `wl_display` and `wl_surface`.
        // winit's Wayland window holds its connection (its `WindowState`
        // keeps a `Connection`) and its surface for as long as it lives;
        // `Constraints` holds the window (`_window`, dropped after all else)
        // and destroys its own objects (made through this backend) first,
        // so both pointers stay valid for as long as the backend and the
        // surface's id are used.
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let conn = Connection::from_backend(backend);
        // SAFETY: as above, the surface lives as long as the window does.
        let surface = unsafe { ObjectId::from_ptr(WlSurface::interface(), handle.surface.as_ptr().cast()) }
            .map_err(|_| Unavailable::Failed("the window's surface is no wl_surface".into()))?;
        let surface = WlSurface::from_id(&conn, surface).map_err(|_| Unavailable::Failed("the window's surface is not known to its connection".into()))?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(|e| Unavailable::Failed(format!("listing the compositor's globals: {e}")))?;
        let qh = queue.handle();
        let constraints: ZwpPointerConstraintsV1 = globals.bind(&qh, 1..=1, ()).map_err(|_| Unavailable::Missing("pointer constraints (zwp_pointer_constraints_v1)"))?;
        let compositor: WlCompositor = globals.bind(&qh, 1..=4, ()).map_err(|_| Unavailable::Missing("wl_compositor"))?;
        let seat: WlSeat = globals.bind(&qh, 1..=5, ()).map_err(|_| Unavailable::Missing("seat"))?;
        let pointer = seat.get_pointer(&qh, ());
        let mut state = State::default();
        queue.roundtrip(&mut state).map_err(|e| Unavailable::Failed(format!("{e}")))?;
        Ok(Constraints {
            queue,
            state,
            conn,
            surface,
            compositor,
            pointer,
            constraints,
            confined: None,
            wanted: None,
            moving: None,
            next_id: 0,
            _window: window,
        })
    }

    /// Confines the pointer to the window (`size`, surface pixels) less
    /// `holes` (the compositor holds it there once the window has it), or
    /// changes the region it is confined to.
    pub fn confine(&mut self, size: (i32, i32), holes: &[Rect]) {
        let region = Region { size, holes: holes.to_vec() };
        if self.wanted.as_ref() == Some(&region) {
            return;
        }
        log::debug!("wl-pointer: confining the pointer to {:?} less {:?}", region.size, region.holes);
        self.wanted = Some(region);
        if self.moving.is_some() {
            // (Confined once the move is done.)
            return;
        }
        match &self.confined {
            Some((confined, _)) => {
                let region = self.region();
                confined.set_region(region.as_ref());
                if let Some(r) = region {
                    r.destroy();
                }
                self.surface.commit();
            }
            None => self.make_confinement(),
        }
        self.flush();
    }

    /// Lets the pointer go anywhere again.
    pub fn unconfine(&mut self) {
        if self.wanted.take().is_some() {
            log::debug!("wl-pointer: the pointer goes anywhere again");
        }
        self.drop_confinement();
        self.flush();
    }

    /// Drops every constraint of this crate's at once (a move under way
    /// too): before winit constrains the pointer.
    pub fn release(&mut self) {
        self.wanted = None;
        self.drop_confinement();
        if let Some(m) = self.moving.take() {
            m.lock.destroy();
        }
        self.flush();
    }

    /// Whether the compositor holds the pointer in the region now.
    pub fn is_confined(&self) -> bool {
        self.confined.as_ref().is_some_and(|(_, id)| self.state.confined == Some(*id))
    }

    /// Whether a move is under way.
    pub fn is_moving(&self) -> bool {
        self.moving.is_some()
    }

    /// Moves the pointer to `to` (surface coordinates): it is locked, the
    /// hint set, and once it is locked let go (in `dispatch`), the
    /// compositor moving it there. A confinement is dropped meanwhile.
    pub fn move_pointer(&mut self, to: (f64, f64)) {
        if let Some(m) = &mut self.moving {
            m.to = to;
            m.lock.set_cursor_position_hint(to.0, to.1);
            self.surface.commit();
            self.flush();
            return;
        }
        self.drop_confinement();
        let id = self.next_id();
        let lock = self.constraints.lock_pointer(&self.surface, &self.pointer, None, Lifetime::Oneshot, &self.queue.handle(), id);
        lock.set_cursor_position_hint(to.0, to.1);
        self.surface.commit();
        self.moving = Some(Moving { lock, id, to, since: Instant::now() });
        self.flush();
    }

    /// Takes what the compositor said (winit's loop reads the connection;
    /// this takes this crate's share), and finishes a move once the pointer
    /// is locked (or not locked for too long).
    pub fn dispatch(&mut self) {
        if let Err(e) = self.queue.dispatch_pending(&mut self.state) {
            log::warn!("wl-pointer: {e}");
        }
        let Some(m) = &self.moving else { return };
        let locked = self.state.locked == Some(m.id);
        if !locked && m.since.elapsed() < LOCK_WAIT {
            return;
        }
        let Some(m) = self.moving.take() else { return };
        if locked {
            // (Again, now that it holds: the hint is taken on unlocking.)
            m.lock.set_cursor_position_hint(m.to.0, m.to.1);
            self.surface.commit();
            log::debug!("wl-pointer: pointer moved to {:?}", m.to);
        } else {
            log::debug!("wl-pointer: the compositor did not lock the pointer to move it to {:?} (not over the window?)", m.to);
        }
        m.lock.destroy();
        if self.wanted.is_some() {
            self.make_confinement();
        }
        self.flush();
    }

    fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// The wanted region as a `wl_region` (none: the whole surface).
    fn region(&self) -> Option<WlRegion> {
        let wanted = self.wanted.as_ref()?;
        let region = self.compositor.create_region(&self.queue.handle(), ());
        region.add(0, 0, wanted.size.0.max(1), wanted.size.1.max(1));
        for h in &wanted.holes {
            region.subtract(h.x, h.y, h.w, h.h);
        }
        Some(region)
    }

    fn make_confinement(&mut self) {
        let id = self.next_id();
        let region = self.region();
        let confined = self.constraints.confine_pointer(&self.surface, &self.pointer, region.as_ref(), Lifetime::Persistent, &self.queue.handle(), id);
        if let Some(r) = region {
            r.destroy();
        }
        self.confined = Some((confined, id));
    }

    fn drop_confinement(&mut self) {
        if let Some((confined, _)) = self.confined.take() {
            confined.destroy();
        }
        self.state.confined = None;
    }

    fn flush(&self) {
        if let Err(e) = self.conn.flush() {
            log::debug!("wl-pointer: flushing: {e}");
        }
    }
}

impl Drop for Constraints {
    fn drop(&mut self) {
        self.release();
        self.constraints.destroy();
        if self.pointer.version() >= 3 {
            self.pointer.release();
        }
        self.flush();
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpConfinedPointerV1, u64> for State {
    fn event(state: &mut Self, _: &ZwpConfinedPointerV1, event: zwp_confined_pointer_v1::Event, id: &u64, _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwp_confined_pointer_v1::Event::Confined => {
                log::debug!("wl-pointer: the compositor confines the pointer");
                state.confined = Some(*id);
            }
            zwp_confined_pointer_v1::Event::Unconfined if state.confined == Some(*id) => {
                log::debug!("wl-pointer: the compositor no longer confines the pointer");
                state.confined = None;
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpLockedPointerV1, u64> for State {
    fn event(state: &mut Self, _: &ZwpLockedPointerV1, event: zwp_locked_pointer_v1::Event, id: &u64, _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwp_locked_pointer_v1::Event::Locked => state.locked = Some(*id),
            zwp_locked_pointer_v1::Event::Unlocked if state.locked == Some(*id) => state.locked = None,
            _ => {}
        }
    }
}

delegate_noop!(State: ZwpPointerConstraintsV1);
delegate_noop!(State: WlCompositor);
delegate_noop!(State: WlRegion);
delegate_noop!(State: ignore WlSeat);
// (Its own pointer's events: winit takes the pointer's events from its own.)
delegate_noop!(State: ignore WlPointer);
