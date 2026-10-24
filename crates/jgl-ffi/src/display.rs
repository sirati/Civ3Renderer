//! The display-mode group (`jgl::display`): jgl's graphics slots +0x58
//! `set_mode`, +0x5C `set_mode_refresh` and +0x60 `set_mode_keep`. Each
//! answers a mode [`jgl::display::screen_for`] accepts with jgl's own
//! +0x60 with `keep` set (the screen fields and the window owner's
//! notice, no `ChangeDisplaySettingsA`), and passes any other request to
//! jgl's original. +0x54 (the screen's size at a depth) ends in +0x58
//! through the vtable, so it is covered too.
//!
//! jgl's import of `ChangeDisplaySettingsA` is answered as well
//! ([`jgl::display::answer_change`]): its shutdown (+0x08), a movie's
//! switch and back (+0x6C, +0x70) and +0x64 all call through it, and none
//! changes the display or sees a failure.

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

    /// `LONG ChangeDisplaySettingsA(DEVMODEA *mode, DWORD flags)`, as jgl calls it.
    pub extern "system" fn change_display_settings_a(mode: u32, flags: u32) -> i32 {
        use jgl::display::{Change, ChangeKind, answer_change};
        let change = Change { mode: (mode != 0).then(|| read_mode(mode)), flags };
        let (answer, kind) = answer_change(&change);
        match kind {
            ChangeKind::Registry => log::info!("jgl: the display's registry mode asked for (flags {flags:#x}); the display is left as it is"),
            ChangeKind::Unsaved => log::info!("jgl: its shutdown restores a mode it never saved (the display was never switched); nothing to do"),
            ChangeKind::Movie => log::info!("jgl: a movie's 640×480 switch (flags {flags:#x}); the display is left as it is"),
            ChangeKind::Mode(m) => log::info!("jgl: a switch to {}×{} at {} bits (fields {:#x}, flags {flags:#x}); the display is left as it is", m.width, m.height, m.bits, m.fields),
        }
        answer
    }

    /// A `DEVMODEA`'s mode fields (`dmFields` +40, `dmBitsPerPel` +104,
    /// `dmPelsWidth` +108, `dmPelsHeight` +112); unreadable ones are 0.
    fn read_mode(mode: u32) -> jgl::display::ChangeMode {
        let at = |offset: u32| inproc::Ptr32::<u32>::new(mode).offset::<u32>(offset).and_then(|p| p.read()).unwrap_or(0);
        jgl::display::ChangeMode { fields: at(40), bits: at(104), width: at(108), height: at(112) }
    }
}

/// The taken-over display-mode group (for the life of the process, in
/// practice).
pub struct DisplayGroup {
    _graphics: Takeover,
    /// jgl's `ChangeDisplaySettingsA` import slot.
    _changes: inproc::hook::SlotPatch,
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
    let slot = inproc::pe::import_slot(base, "USER32.dll", "ChangeDisplaySettingsA")
        .map_err(inproc::hook::HookError::from)?
        .ok_or(TakeoverError::NoImport("ChangeDisplaySettingsA"))?;
    let graphics = take_over(base, Class::Graphics, &graphics)?;
    type Change = extern "system" fn(u32, u32) -> i32;
    let changes = inproc::hook::SlotPatch::replace(slot, change_display_settings_a as Change as usize as u32)?;
    Ok(DisplayGroup { _graphics: graphics, _changes: changes })
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn install_impl(_base: u32) -> Result<DisplayGroup, TakeoverError> {
    Err(inproc::hook::HookError::Unsupported.into())
}
