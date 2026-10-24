//! macOS pointer confinement. Low-level AppKit handling lives in the window
//! backend; this crate translates the common pointer contract into its safe
//! physical-pixel API.
//!
//! macOS has no system primitive that confines the pointer to a region.
//! The window backend detaches the pointer from the mouse
//! (`CGAssociateMouseAndMouseCursorPosition`) and positions it itself from
//! each motion delta (`CGWarpMouseCursorPosition`), moving it only within
//! the region: a one-pixel span keeps that axis exact, exclusions cannot be
//! crossed even by one large event. The region is only taken while the
//! pointer is already inside it; otherwise it is reported as not confined
//! and the layout's warps place the pointer first. Since this confinement
//! is the application's own, other applications, system gestures, and
//! anything else that warps the pointer can still move it; losing focus
//! releases it.
//!
//! The deltas are applied and the warps made on the window backend's pointer
//! thread (a listen-only CoreGraphics event tap), as each motion happens,
//! not on the main thread, which can be busy drawing or running the host's
//! work for tens of milliseconds. That thread also tracks the free pointer
//! ([`Backend::feed`]). If the system refuses the tap (or
//! `WINIT_POINTER_TAP=0`), the main thread places the pointer as before.

#![forbid(unsafe_code)]

#![cfg(target_os = "macos")]

use std::sync::Arc;
use pointer_api::{Backend, Error, Feed, Grab, Move, Point, Region, Update};
use winit::{platform::macos::WindowExtMacOS, window::Window};

pub struct Controller {
    window: Arc<Window>,
    fallback: pointer_fallback::Controller,
    confined: bool,
    grabbed: bool,
    thread_init: Option<fn()>,
}

impl Controller {
    pub fn new(window: Arc<Window>, thread_init: Option<fn()>) -> Self {
        Self { fallback: pointer_fallback::Controller::new(window.clone()), window, confined: false, grabbed: false, thread_init }
    }

    fn release_region(&mut self) {
        if self.confined {
            match self.window.set_cursor_confinement(None, &[]) {
                Ok(()) => self.confined = false,
                Err(error) => log::warn!("macOS pointer confinement release: {error}"),
            }
        }
    }
}

impl Backend for Controller {
    fn grab(&mut self, on: bool) -> Result<Grab, Error> {
        self.release_region();
        let result = self.fallback.grab(on)?;
        self.grabbed = on;
        Ok(result)
    }

    fn move_pointer(&mut self, at: Point) -> Result<Move, Error> { self.fallback.move_pointer(at) }

    fn update(&mut self, region: Option<&Region>, _pointer: (f64, f64), inside: bool) -> Update {
        let region = region.filter(|_| inside && !self.grabbed);
        if let Some(region) = region {
            match self.window.set_cursor_confinement(Some(region.bounds), &region.exclusions) {
                Ok(()) => self.confined = true,
                Err(error) => {
                    if self.confined { log::debug!("macOS pointer region: {error}"); }
                    self.release_region();
                }
            }
        } else {
            self.release_region();
        }
        Update { confined: self.confined && region.is_some(), placement: None }
    }

    fn feed(&self) -> Option<Feed> {
        // (The window's event tap thread: it also moves a held pointer.)
        let feed = self.window.pointer_feed(self.thread_init)?;
        Some(std::sync::Arc::new(move || feed.position()))
    }
}

impl Drop for Controller {
    fn drop(&mut self) { self.release_region(); }
}
