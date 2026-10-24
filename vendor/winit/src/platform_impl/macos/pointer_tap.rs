//! The pointer's own thread on macOS: a listen-only CoreGraphics event tap
//! on a thread of its own, so pointer motion is handled however long the
//! main thread is busy (drawing, or a host application's own work, such as
//! Wine's in its process).
//!
//! AppKit delivers motion only to the main thread. While a region holds the
//! pointer, the pointer is detached from the mouse
//! (`CGAssociateMouseAndMouseCursorPosition`) and placed from each motion
//! delta; done on the main thread, the pointer stood still whenever that
//! thread was busy. Here the tap's thread applies each delta to the region
//! ([`Region::advance`]) and warps the pointer (`CGWarpMouseCursorPosition`,
//! callable from any thread) as soon as the event happens. The main thread
//! only publishes the region and the view's place on screen, and reads the
//! position back for its `CursorMoved` events.
//!
//! The tap also tracks the free pointer in view pixels, for readers that
//! sample the pointer on their own clock ([`PointerFeed`]).
//!
//! Nothing here locks: the region, the geometry and the position are
//! `ArcSwap` values and atomics. If the tap cannot be created (for
//! example, the system refuses it), [`PointerTap::start`] returns `None`
//! and the main thread places the pointer itself as before.
//! `WINIT_POINTER_TAP=0` turns the tap off.

#![deny(unsafe_code)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwapOption;
use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

use super::cursor_confinement::{view_pixels, Region, WarpCorrection};
use super::cursor_system;

/// Where the view is on the screen: its top-left corner in global display
/// points (y down, as CoreGraphics counts), its size in points, and its
/// physical pixels a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Geometry {
    pub origin: (f64, f64),
    pub size: (f64, f64),
    pub scale: f64,
}

impl Geometry {
    /// A global display point in physical view pixels (as `mouse_motion`
    /// reports a point of the view).
    pub fn to_view(&self, global: (f64, f64)) -> (f64, f64) {
        view_pixels((global.0 - self.origin.0, global.1 - self.origin.1), self.size, self.scale)
    }

    /// Physical view pixels as a global display point (for a warp).
    pub fn to_global(&self, view: (f64, f64)) -> (f64, f64) {
        let scale = self.scale.max(f64::MIN_POSITIVE);
        (self.origin.0 + view.0 / scale, self.origin.1 + view.1 / scale)
    }
}

/// A region the tap holds the pointer in, from `start` on. `generation`
/// tells a newly set region (or a warp of the main thread's) from the one
/// the tap is moving the pointer in.
#[derive(Debug)]
pub(super) struct Held {
    pub region: Region,
    pub start: (f64, f64),
    pub generation: u64,
}

/// The pointer as the tap last placed or saw it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Placed {
    /// Physical view pixels.
    pub at: (f64, f64),
    /// The [`Held::generation`] it was placed in; 0 for a free pointer.
    pub generation: u64,
}

#[derive(Debug, Default)]
pub(super) struct Shared {
    held: ArcSwapOption<Held>,
    geometry: ArcSwapOption<Geometry>,
    placed: ArcSwapOption<Placed>,
    /// Counts every motion the tap handled (a reader's change check).
    motions: AtomicU64,
    /// The tap runs. Cleared when it ends (or could not start).
    live: AtomicBool,
    /// Asked to end (the view is gone).
    stop: AtomicBool,
    generations: AtomicU64,
}

impl Shared {
    /// The tap's state after one motion `delta` (points) at `location`
    /// (global display points): where the pointer is now, and the warp to
    /// make (global points) if the region moved it. Pure, for tests.
    pub fn advance(
        held: Option<&Held>,
        geometry: Option<&Geometry>,
        last: Option<Placed>,
        location: (f64, f64),
        delta: (f64, f64),
    ) -> (Option<Placed>, Option<(f64, f64)>) {
        let Some(geometry) = geometry else { return (last, None) };
        let Some(held) = held else {
            return (Some(Placed { at: geometry.to_view(location), generation: 0 }), None);
        };
        let from = match last {
            Some(p) if p.generation == held.generation => p.at,
            _ => held.start,
        };
        let scale = geometry.scale;
        let next = held.region.advance(from, (delta.0 * scale, delta.1 * scale));
        let warp = (next != from).then(|| geometry.to_global(next));
        (Some(Placed { at: next, generation: held.generation }), warp)
    }
}

/// The tap's thread and what it shares with the main thread. Dropping it
/// ends the thread (within [`POLL`]).
#[derive(Debug)]
pub(super) struct PointerTap {
    shared: Arc<Shared>,
}

/// How often the tap's run loop checks whether it is to end, or whether the
/// system disabled the tap (a slow callback) and it is to be enabled again.
const POLL: Duration = Duration::from_millis(250);

impl PointerTap {
    /// `thread_init` runs first on the new thread (a host's policy for threads
    /// of its process it does not know, such as blocked signals).
    pub fn start(thread_init: Option<fn()>) -> Option<Self> {
        if std::env::var_os("WINIT_POINTER_TAP").is_some_and(|v| v == "0") {
            tracing::info!("pointer tap: off (WINIT_POINTER_TAP=0)");
            return None;
        }
        let shared = Arc::new(Shared::default());
        let thread_shared = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("winit-pointer".into())
            .spawn(move || {
                if let Some(init) = thread_init {
                    init();
                }
                run(thread_shared)
            });
        if let Err(error) = spawned {
            tracing::warn!("pointer tap: no thread: {error}");
            return None;
        }
        // (Not waited for: until the tap runs, `live` is false and the main
        // thread places the pointer itself.)
        Some(Self { shared })
    }

    pub fn shared(&self) -> Arc<Shared> {
        self.shared.clone()
    }

    pub fn live(&self) -> bool {
        self.shared.live.load(Ordering::Acquire)
    }

    pub fn set_geometry(&self, geometry: Geometry) {
        if self.shared.geometry.load().as_deref() != Some(&geometry) {
            self.shared.geometry.store(Some(Arc::new(geometry)));
        }
    }

    /// Holds the pointer in `region` from `start` on (physical view
    /// pixels): a new generation, so the tap starts there. `None` lets go.
    pub fn hold(&self, region: Option<Region>, start: (f64, f64)) {
        match region {
            Some(region) => {
                let generation = self.shared.generations.fetch_add(1, Ordering::AcqRel) + 1;
                self.shared.held.store(Some(Arc::new(Held { region, start, generation })));
            },
            None => self.shared.held.store(None),
        }
    }

    /// The main thread put the pointer at `at` itself (a warp): the held
    /// region goes on from there.
    pub fn placed(&self, at: (f64, f64)) {
        if let Some(held) = self.shared.held.load_full() {
            self.hold(Some(held.region.clone()), at);
        }
    }

    /// Where the tap holds the pointer in the region it holds it in now,
    /// if it moved it since that region was set.
    pub fn held_position(&self) -> Option<(f64, f64)> {
        let held = self.shared.held.load();
        let held = held.as_deref()?;
        let placed = self.shared.placed.load();
        Some(match placed.as_deref() {
            Some(p) if p.generation == held.generation => p.at,
            _ => held.start,
        })
    }
}

impl Drop for PointerTap {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
    }
}

/// A thread-safe view of the pointer as the tap last saw it.
#[derive(Clone, Debug)]
pub struct PointerFeed {
    pub(super) shared: Arc<Shared>,
}

impl PointerFeed {
    /// The pointer in physical view pixels, if the tap runs and saw it.
    pub fn position(&self) -> Option<(f64, f64)> {
        if !self.shared.live.load(Ordering::Acquire) {
            return None;
        }
        self.shared.placed.load().as_deref().map(|p| p.at)
    }

    /// How many motions the tap has handled.
    pub fn motions(&self) -> u64 {
        self.shared.motions.load(Ordering::Acquire)
    }
}

fn run(shared: Arc<Shared>) {
    let disabled = Arc::new(AtomicBool::new(false));
    let callback_shared = shared.clone();
    let callback_disabled = disabled.clone();
    let correction = std::cell::RefCell::new(WarpCorrection::default());
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok();
    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::TailAppendEventTap,
        CGEventTapOptions::ListenOnly,
        vec![
            CGEventType::MouseMoved,
            CGEventType::LeftMouseDragged,
            CGEventType::RightMouseDragged,
            CGEventType::OtherMouseDragged,
        ],
        move |_proxy, kind, event: &CGEvent| {
            match kind {
                CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                    callback_disabled.store(true, Ordering::Release);
                },
                CGEventType::MouseMoved
                | CGEventType::LeftMouseDragged
                | CGEventType::RightMouseDragged
                | CGEventType::OtherMouseDragged => {
                    motion(&callback_shared, event, source.as_ref(), &mut correction.borrow_mut());
                },
                _ => {},
            }
            None
        },
    );
    let Ok(tap) = tap else {
        tracing::warn!("pointer tap: the system refused an event tap; the main thread places the pointer");
        return;
    };
    let Ok(loop_source) = tap.mach_port.create_runloop_source(0) else {
        tracing::warn!("pointer tap: the system refused an event tap; the main thread places the pointer");
        return;
    };
    let run_loop = CFRunLoop::get_current();
    let mode = default_mode();
    run_loop.add_source(&loop_source, mode);
    tap.enable();
    shared.live.store(true, Ordering::Release);
    tracing::info!("pointer tap: motion handled on its own thread");
    while !shared.stop.load(Ordering::Acquire) {
        CFRunLoop::run_in_mode(mode, POLL, false);
        if disabled.swap(false, Ordering::AcqRel) {
            tracing::debug!("pointer tap: the system disabled the tap; enabled again");
            tap.enable();
        }
    }
    shared.live.store(false, Ordering::Release);
    run_loop.remove_source(&loop_source, mode);
}

fn default_mode() -> core_foundation::runloop::CFRunLoopMode {
    // SAFETY: an immutable CoreFoundation constant string, set before any
    // code of ours runs; reading the extern static is how the crate offers it.
    #[allow(unsafe_code)]
    unsafe {
        kCFRunLoopDefaultMode
    }
}

fn motion(shared: &Shared, event: &CGEvent, source: Option<&CGEventSource>, correction: &mut WarpCorrection) {
    let location = event.location();
    let delta = (
        event.get_double_value_field(EventField::MOUSE_EVENT_DELTA_X),
        event.get_double_value_field(EventField::MOUSE_EVENT_DELTA_Y),
    );
    // (A warp of ours can show in the next delta: taken out once.)
    let delta = correction.motion(delta);
    let held = shared.held.load();
    let geometry = shared.geometry.load();
    let last = shared.placed.load().as_deref().copied();
    let (placed, warp) = Shared::advance(held.as_deref(), geometry.as_deref(), last, (location.x, location.y), delta);
    if let Some(placed) = placed {
        shared.placed.store(Some(Arc::new(placed)));
    }
    if let Some(to) = warp {
        let before = source
            .and_then(|s| CGEvent::new(s.clone()).ok())
            .map(|e| (e.location().x, e.location().y))
            .unwrap_or(to);
        match cursor_system::warp(to) {
            Ok(()) => correction.warp(before, to),
            Err(error) => tracing::warn!("pointer tap: warp failed: {error}"),
        }
    }
    shared.motions.fetch_add(1, Ordering::AcqRel);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> Geometry {
        Geometry { origin: (100.0, 50.0), size: (800.0, 600.0), scale: 2.0 }
    }

    #[test]
    fn a_free_pointer_is_reported_in_view_pixels() {
        let g = geometry();
        let (placed, warp) = Shared::advance(None, Some(&g), None, (110.0, 60.0), (3.0, 0.0));
        assert_eq!(placed, Some(Placed { at: (20.0, 20.0), generation: 0 }));
        assert_eq!(warp, None);
        // The bottom boundary is the last row, as `mouse_motion` has it.
        let (placed, _) = Shared::advance(None, Some(&g), None, (110.0, 650.0), (0.0, 0.0));
        assert_eq!(placed.unwrap().at, (20.0, 1199.0));
        assert_eq!(g.to_global((20.0, 20.0)), (110.0, 60.0));
    }

    #[test]
    fn a_held_pointer_moves_by_deltas_and_stays_on_the_edge_it_is_pushed_against() {
        let g = geometry();
        let held = Held { region: Region::new([0, 0, 1600, 1200], &[[800, 0, 4, 1200]]).unwrap(), start: (10.0, 300.0), generation: 3 };
        let (p, warp) = Shared::advance(Some(&held), Some(&g), None, (0.0, 0.0), (-2.0, 0.0));
        assert_eq!(p, Some(Placed { at: (6.0, 300.0), generation: 3 }));
        assert_eq!(warp, Some((103.0, 200.0)));
        // Pushed further into the left edge: held at 0, no more warps once there.
        let (p, _) = Shared::advance(Some(&held), Some(&g), p, (0.0, 0.0), (-50.0, 0.0));
        assert_eq!(p.unwrap().at, (0.0, 300.0));
        let (p2, warp) = Shared::advance(Some(&held), Some(&g), p, (0.0, 0.0), (-50.0, 0.0));
        assert_eq!((p2, warp), (p, None));
        // A position of an older region does not carry over.
        let stale = Some(Placed { at: (700.0, 10.0), generation: 2 });
        assert_eq!(Shared::advance(Some(&held), Some(&g), stale, (0.0, 0.0), (0.0, 0.0)).0.unwrap().at, (10.0, 300.0));
        // The divider still holds.
        let (p, _) = Shared::advance(Some(&held), Some(&g), Some(Placed { at: (790.0, 300.0), generation: 3 }), (0.0, 0.0), (100.0, 0.0));
        assert_eq!(p.unwrap().at, (799.0, 300.0));
    }
}
