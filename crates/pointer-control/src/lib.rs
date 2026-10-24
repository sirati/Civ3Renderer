//! One pointer API for the renderer. Backend selection, display coordinate
//! conversion, cursor capture and platform constraint lifetimes stay here
//! and in the individual backend crates.

#![forbid(unsafe_code)]

mod cursor_size;

pub use cursor_size::system_cursor_size;

use std::sync::Arc;
use winit::window::Window;

pub use pointer_api::{Error, Feed, Grab, Move, Placement, Point, Rect, Region, Update};
use pointer_api::Backend;

#[derive(Default)]
pub struct Controller {
    backend: Option<Box<dyn Backend>>,
}

impl Controller {
    /// `thread_init` runs first on any thread a backend starts (a host's
    /// policy for threads of its process).
    pub fn attach(&mut self, window: Arc<Window>, thread_init: Option<fn()>) {
        let _ = thread_init;
        #[cfg(target_os = "linux")]
        match pointer_wayland::Controller::new(window.clone()) {
            Ok(backend) => {
                log::info!("pointer: Wayland constraints and lock-and-hint movement");
                self.backend = Some(Box::new(backend));
                return;
            }
            Err(error) => log::debug!("pointer: window pointer fallback ({error})"),
        }
        #[cfg(target_os = "linux")]
        if let Ok(backend) = pointer_x11::Controller::new(window.clone()) {
            log::info!("pointer: X11 pointer barriers and physical pointer warps");
            self.backend = Some(Box::new(backend));
            return;
        }
        #[cfg(windows)]
        {
            self.backend = Some(Box::new(pointer_windows::Controller::new(window)));
            return;
        }
        #[cfg(target_os = "macos")]
        {
            self.backend = Some(Box::new(pointer_macos::Controller::new(window, thread_init)));
            return;
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            self.backend = Some(Box::new(pointer_fallback::Controller::new(window)));
        }
    }

    pub fn grab(&mut self, on: bool) -> Result<Grab, Error> {
        self.backend.as_mut().ok_or_else(unattached)?.grab(on)
    }

    pub fn move_pointer(&mut self, at: Point) -> Result<Move, Error> {
        self.backend.as_mut().ok_or_else(unattached)?.move_pointer(at)
    }

    pub fn moving(&self) -> bool { self.backend.as_ref().is_some_and(|b| b.moving()) }

    pub fn update(&mut self, region: Option<&Region>, pointer: (f64, f64), inside: bool) -> Update {
        self.backend.as_mut().map_or_else(Update::default, |b| b.update(region, pointer, inside))
    }

    /// See [`pointer_api::Backend::feed`].
    pub fn feed(&self) -> Option<Feed> { self.backend.as_ref().and_then(|b| b.feed()) }
}

fn unattached() -> Error { Error("pointer controller has no window".into()) }

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Mock(Rc<RefCell<Vec<&'static str>>>);
    impl Backend for Mock {
        fn grab(&mut self, on: bool) -> Result<Grab, Error> {
            self.0.borrow_mut().push(if on { "grab" } else { "release" });
            Ok(if on { Grab::Locked } else { Grab::Released })
        }
        fn move_pointer(&mut self, _: Point) -> Result<Move, Error> { Ok(Move::Pending) }
        fn moving(&self) -> bool { true }
        fn update(&mut self, region: Option<&Region>, _: (f64, f64), inside: bool) -> Update {
            self.0.borrow_mut().push(if inside && region.is_some() { "region" } else { "unconfine" });
            Update { confined: inside && region.is_some(), placement: Some(Placement::Cancelled) }
        }
    }

    #[test]
    fn common_contract_preserves_capture_pending_moves_and_releases() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut pointer = Controller { backend: Some(Box::new(Mock(calls.clone()))) };
        let r = Region { bounds: [400, 0, 1, 600], exclusions: vec![[0, 0, 800, 90]] };
        assert!(pointer.update(Some(&r), (400.0, 100.0), true).confined);
        assert_eq!(pointer.grab(true), Ok(Grab::Locked));
        assert_eq!(pointer.move_pointer((400, 100)), Ok(Move::Pending));
        assert!(pointer.moving());
        assert_eq!(pointer.update(None, (400.0, 100.0), false).placement, Some(Placement::Cancelled));
        assert_eq!(pointer.grab(false), Ok(Grab::Released));
        assert_eq!(*calls.borrow(), ["region", "grab", "unconfine", "release"]);
    }

    #[test]
    fn unattached_controller_cannot_claim_capture_or_pointer_placement() {
        let mut p = Controller::default();
        assert!(p.grab(true).is_err());
        assert!(p.move_pointer((10, 20)).is_err());
        assert!(!p.moving());
        assert_eq!(p.update(None, (10.0, 20.0), false), Update::default());
    }
}
