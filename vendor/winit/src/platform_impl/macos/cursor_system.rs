//! Safe typed CoreGraphics calls used by pointer capture and placement.
//! Keep the error conversion here so the rest of winit retains its existing
//! OS error type without exposing framework types through the public API.

#![forbid(unsafe_code)]

use objc2_core_foundation::CGPoint;
use objc2_core_graphics::{
    CGAssociateMouseAndMouseCursorPosition, CGError, CGWarpMouseCursorPosition,
};

use super::OsError;
use crate::error::ExternalError;

pub(super) fn associate(connected: bool) -> Result<(), ExternalError> {
    check(CGAssociateMouseAndMouseCursorPosition(connected))
}

pub(super) fn warp((x, y): (f64, f64)) -> Result<(), ExternalError> {
    check(CGWarpMouseCursorPosition(CGPoint::new(x, y)))
}

fn check(status: CGError) -> Result<(), ExternalError> {
    if status == CGError::Success {
        Ok(())
    } else {
        Err(ExternalError::Os(os_error!(OsError::CGError(status.0))))
    }
}
