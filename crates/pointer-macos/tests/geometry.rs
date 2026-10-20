//! Exercise the actual AppKit backend's portable region/capture logic on
//! every build host, without opening a window or moving a system pointer.
#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../../../vendor/winit/src/platform_impl/macos/cursor_confinement.rs"]
mod cursor_confinement;
