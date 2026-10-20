//! Portable pointer operations supplied by the window library.

#![forbid(unsafe_code)]

use std::sync::Arc;

use pointer_api::{Backend, Error, Grab, Move, Point, Region, Update};
use winit::dpi::PhysicalPosition;
use winit::window::{CursorGrabMode, Window};

/// One window's pointer grab. Dropping it releases only a grab that this
/// controller successfully acquired.
pub struct Controller {
    window: Arc<Window>,
    grabbed: bool,
}

impl Controller {
    pub fn new(window: Arc<Window>) -> Self {
        Self {
            window,
            grabbed: false,
        }
    }

    pub fn grabbed(&self) -> bool {
        self.grabbed
    }
}

impl Backend for Controller {
    fn grab(&mut self, on: bool) -> Result<Grab, Error> {
        if !on {
            self.window
                .set_cursor_grab(CursorGrabMode::None)
                .map_err(|e| Error(format!("release pointer grab: {e}")))?;
            self.grabbed = false;
            return Ok(Grab::Released);
        }
        match self.window.set_cursor_grab(CursorGrabMode::Locked) {
            Ok(()) => {
                self.grabbed = true;
                Ok(Grab::Locked)
            }
            Err(locked) => {
                self.window
                    .set_cursor_grab(CursorGrabMode::Confined)
                    .map_err(|confined| {
                        Error(format!(
                            "lock pointer: {locked}; confine pointer: {confined}"
                        ))
                    })?;
                self.grabbed = true;
                Ok(Grab::Confined)
            }
        }
    }

    fn move_pointer(&mut self, at: Point) -> Result<Move, Error> {
        self.window
            .set_cursor_position(PhysicalPosition::new(at.0, at.1))
            .map_err(|e| Error(format!("position pointer: {e}")))?;
        Ok(Move::Immediate)
    }

    fn update(&mut self, _region: Option<&Region>, _pointer: (f64, f64), _inside: bool) -> Update {
        Update::default()
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        if self.grabbed {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
        }
    }
}
