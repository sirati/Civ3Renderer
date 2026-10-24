//! The unsafe shim between the game and [`jgl`].
//!
//! Everything about jgl.dll that needs `unsafe` is here, and nothing else:
//!
//! - [`layout`]: the `#[repr(C)]` layouts of jgl's five classes, with
//!   offsets checked against `jgl.dll` by the tests;
//! - [`takeover`]: writing our entry points into jgl's method tables,
//!   keeping the originals to fall back on;
//! - [`text`], [`fill`], [`copy`], [`blit`]: the text, fill, copy and
//!   sprite groups' `extern "thiscall"` entry points. Each reads its raw
//!   arguments, makes safe values, calls [`jgl::Jgl`] (the game thread's one, `state`), and
//!   calls jgl's original where the answer says so;
//! - [`display`]: the display-mode group's entry points (the display is
//!   never switched);
//! - [`count`]: the counting mode's generated stubs.
//!
//! No policy lives here: what to record, what the state means and whether
//! jgl's pixels are still needed is decided by `jgl` and its hooks.
//!
//! See `docs/jgl-replacement.md`.

pub mod blit;
pub mod copy;
pub mod count;
pub mod display;
pub mod fill;
pub mod layout;
mod state;
pub mod takeover;
pub mod text;

/// The jgl.dll this shim is written against, at its preferred base.
pub const PREFERRED_BASE: u32 = 0x1000_0000;
