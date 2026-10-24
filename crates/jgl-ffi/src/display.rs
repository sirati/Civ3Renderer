//! The display-mode group (`jgl::display`): jgl's graphics slots +0x58
//! `set_mode`, +0x5C `set_mode_refresh` and +0x60 `set_mode_keep`. Each
//! answers a mode [`jgl::display::screen_for`] accepts with jgl's own
//! +0x60 with `keep` set (the screen fields and the window owner's
//! notice, no `ChangeDisplaySettingsA`), and passes any other request to
//! jgl's original. +0x54 (the screen's size at a depth) ends in +0x58
//! through the vtable, so it is covered too.

#![cfg_attr(not(all(windows, target_arch = "x86")), allow(dead_code, unused_imports))]

use std::sync::atomic::AtomicU32;

use crate::takeover::{Takeover, TakeoverError};

#[cfg(all(windows, target_arch = "x86"))]
mod entries {
    use super::*;
    use crate::takeover::original;
    use jgl::display::{Mode, screen_for};

    pub static SET_MODE: AtomicU32 = AtomicU32::new(0);
    pub static SET_MODE_REFRESH: AtomicU32 = AtomicU32::new(0);
    pub static SET_MODE_KEEP: AtomicU32 = AtomicU32::new(0);

    pub type M3 = extern "thiscall" fn(u32, u32, u32, u32) -> u32;
    pub type M4 = extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32;
    pub type M5 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32) -> u32;

    /// The game's screen for the mode it asks for, if it is one.
    fn screen(slot: &str, width: u32, height: u32, bits: u32) -> Option<Mode> {
        let asked = Mode { width: width as i32, height: height as i32, bits: bits as i32 };
        let screen = screen_for(asked);
        match screen {
            Some(m) => log::info!("jgl {slot}: the game's screen is {}×{} at {} bits; the display is left as it is", m.width, m.height, m.bits),
            None => log::warn!("jgl {slot}: {asked:?} is no mode; jgl's own answer"),
        }
        screen
    }

    /// jgl's +0x60 with `keep` set: the screen fields, the owner told.
    fn keep(this: u32, m: Mode) -> u32 {
        original::<M4>(&SET_MODE_KEEP)(this, m.width as u32, m.height as u32, m.bits as u32, 1)
    }

    pub extern "thiscall" fn set_mode(this: u32, width: u32, height: u32, bits: u32) -> u32 {
        match screen("set_mode", width, height, bits) {
            Some(m) => keep(this, m),
            None => original::<M3>(&SET_MODE)(this, width, height, bits),
        }
    }

    pub extern "thiscall" fn set_mode_refresh(this: u32, width: u32, height: u32, bits: u32, keep_display: u32, refresh: u32) -> u32 {
        match screen("set_mode_refresh", width, height, bits) {
            Some(m) => keep(this, m),
            None => original::<M5>(&SET_MODE_REFRESH)(this, width, height, bits, keep_display, refresh),
        }
    }

    pub extern "thiscall" fn set_mode_keep(this: u32, width: u32, height: u32, bits: u32, keep_display: u32) -> u32 {
        match screen("set_mode_keep", width, height, bits) {
            Some(m) => keep(this, m),
            None => original::<M4>(&SET_MODE_KEEP)(this, width, height, bits, keep_display),
        }
    }
}

/// The taken-over display-mode group (for the life of the process, in
/// practice).
pub struct DisplayGroup {
    _graphics: Takeover,
}

/// Takes over the display-mode group of jgl.dll at `base`. Once per
/// process.
pub fn install(base: u32) -> Result<DisplayGroup, TakeoverError> {
    install_impl(base)
}

#[cfg(all(windows, target_arch = "x86"))]
fn install_impl(base: u32) -> Result<DisplayGroup, TakeoverError> {
    use crate::takeover::{Entry, take_over};
    use entries::*;
    use jgl::slots::Class;
    let graphics = [
        Entry { offset: 0x58, function: set_mode as M3 as usize as u32, original: &SET_MODE },
        Entry { offset: 0x5C, function: set_mode_refresh as M5 as usize as u32, original: &SET_MODE_REFRESH },
        Entry { offset: 0x60, function: set_mode_keep as M4 as usize as u32, original: &SET_MODE_KEEP },
    ];
    Ok(DisplayGroup { _graphics: take_over(base, Class::Graphics, &graphics)? })
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn install_impl(_base: u32) -> Result<DisplayGroup, TakeoverError> {
    Err(inproc::hook::HookError::Unsupported.into())
}
