//! The present group (`jgl::present`, `docs/jgl-replacement.md` §6, group
//! 5): jgl's graphics slots +0x30 `invalidate` and +0xA4 `present`.
//!
//! Both record a [`jgl::PresentCall`]. `invalidate` then always runs jgl's
//! original (`InvalidateRect` on jgl's window: the window, and the paint
//! its invalidation brings, are jgl's until the window group is ours).
//! `present` runs it only where [`jgl::PixelPolicy::original_present`]
//! wants the back buffer on jgl's window, and answers 0 (jgl's answer)
//! otherwise.
//!
//! The rectangle is read directly: jgl's originals pass it to
//! `InvalidateRect` and `IntersectRect` unchecked. The back buffer is the
//! image present copies from: `[[this + 0x148] + 4]`, as jgl's body reads
//! it.

#![cfg_attr(not(all(windows, target_arch = "x86")), allow(dead_code, unused_imports))]

use std::sync::atomic::AtomicU32;

use crate::takeover::{Takeover, TakeoverError};

#[cfg(all(windows, target_arch = "x86"))]
mod entries {
    use super::*;
    use crate::layout::JglGraphics;
    use crate::state::{guarded, read, record};
    use crate::takeover::{Entry, original};
    use jgl::PresentCall;
    use std::mem::offset_of;

    pub static INVALIDATE: AtomicU32 = AtomicU32::new(0);
    pub static PRESENT: AtomicU32 = AtomicU32::new(0);

    pub type M1 = extern "thiscall" fn(u32, u32) -> u32;

    /// A `RECT*` argument, `None` for a null pointer.
    fn rect(at: u32) -> Option<jgl::Rect> {
        // SAFETY: a rectangle jgl's original reads unchecked when it is
        // not null.
        (at != 0).then(|| unsafe { read::rect_at(at) })
    }

    /// The image `present` copies from, if the graphics object has one.
    fn back_buffer(this: u32) -> Option<jgl::Surface> {
        // SAFETY: `this` is the graphics object whose method this is
        // (0x14C bytes); its +0x148 and that object's +4 are what jgl's
        // `present` reads unchecked, both tested for null here first.
        unsafe {
            let owner = read::u32_at(this + offset_of!(JglGraphics, owner) as u32);
            if owner == 0 {
                return None;
            }
            let image = read::u32_at(owner + 4);
            (image != 0).then(|| read::surface(image))
        }
    }

    /// Records `call`; whether jgl's original runs.
    fn recorded(name: &str, call: impl FnOnce() -> PresentCall) -> bool {
        guarded(name, || record(|p| p.present(&call())).unwrap_or(true), || true)
    }

    pub extern "thiscall" fn invalidate(this: u32, r: u32) -> u32 {
        recorded("invalidate", || PresentCall::Invalidate { rect: rect(r) });
        original::<M1>(&INVALIDATE)(this, r)
    }

    pub extern "thiscall" fn present(this: u32, r: u32) -> u32 {
        let jgl = recorded("present", || PresentCall::Present { rect: rect(r), back: back_buffer(this) });
        // jgl's present returns 0.
        if jgl { original::<M1>(&PRESENT)(this, r) } else { 0 }
    }

    /// The group's graphics slots.
    pub fn graphics() -> Vec<Entry> {
        vec![
            Entry { offset: 0x30, function: invalidate as M1 as usize as u32, original: &INVALIDATE },
            Entry { offset: 0xA4, function: present as M1 as usize as u32, original: &PRESENT },
        ]
    }
}

/// The taken-over present group (for the life of the process, in
/// practice).
pub struct PresentGroup {
    _graphics: Takeover,
}

/// Takes over the present group of jgl.dll at `base`. Its calls go to the
/// hooks the text group was installed with ([`crate::text::install`]
/// first).
pub fn install(base: u32) -> Result<PresentGroup, TakeoverError> {
    install_impl(base)
}

#[cfg(all(windows, target_arch = "x86"))]
fn install_impl(base: u32) -> Result<PresentGroup, TakeoverError> {
    use crate::takeover::take_over;
    use jgl::slots::Class;
    if !crate::state::has_hooks() {
        return Err(TakeoverError::NoHooks);
    }
    Ok(PresentGroup { _graphics: take_over(base, Class::Graphics, &entries::graphics())? })
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn install_impl(_base: u32) -> Result<PresentGroup, TakeoverError> {
    Err(inproc::hook::HookError::Unsupported.into())
}

#[cfg(test)]
mod tests {
    use jgl::slots::{Class, Group};

    /// The entry points' argument counts are jgl's (`ret 4`).
    #[test]
    fn the_group_is_the_catalogue_s_present_slots() {
        let present: Vec<(u16, u8)> = Class::Graphics.slots().iter().filter(|s| s.group == Group::Present).map(|s| (s.offset, s.stack)).collect();
        assert_eq!(present, [(0x30, 4), (0xA4, 4)]);
    }
}
