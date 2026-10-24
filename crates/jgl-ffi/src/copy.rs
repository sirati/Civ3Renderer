//! The copy group (`docs/jgl-replacement.md` §6, group 3): jgl's image
//! slots +0x38 (an 8-bit image through its colour table), +0x40 `copy`
//! (`StretchBlt`), +0x84 `keyed_draw_onto` and +0x8C.
//!
//! As in the fill group (`crate::fill`), every entry point reads its
//! arguments into a [`jgl::CopyCall`], records it and runs jgl's original
//! only where [`jgl::PixelPolicy`] wants jgl's pixels. A null target image
//! or rectangle goes to the original unrecorded (jgl answers it with an
//! error code). `this` is the image copied from, the first argument the
//! image copied onto.

#![cfg_attr(not(all(windows, target_arch = "x86")), allow(dead_code, unused_imports))]

use std::sync::atomic::AtomicU32;

use crate::takeover::{Takeover, TakeoverError};

#[cfg(all(windows, target_arch = "x86"))]
mod entries {
    use super::*;
    use crate::state::{guarded, read, record};
    use crate::takeover::original;
    use jgl::CopyCall;

    pub static INDEXED: AtomicU32 = AtomicU32::new(0);
    pub static STRETCH: AtomicU32 = AtomicU32::new(0);
    pub static KEYED: AtomicU32 = AtomicU32::new(0);
    pub static SLOT_8C: AtomicU32 = AtomicU32::new(0);

    pub type M3 = extern "thiscall" fn(u32, u32, u32, u32) -> u32;
    pub type M4 = extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32;
    pub type M8 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32, u32, u32) -> u32;

    /// Records `call` (made by `read`; `None`: an argument that cannot be
    /// read) and says whether jgl's original runs.
    fn drawn(name: &str, read: impl FnOnce() -> Option<CopyCall>) -> bool {
        guarded(name, || match read() {
            Some(call) => record(|p| p.copy(&call)).unwrap_or(true),
            None => true,
        }, || true)
    }

    /// An image argument, `None` for a null pointer.
    fn surface(image: u32) -> Option<jgl::Surface> {
        // SAFETY: `this`, or an image argument jgl's original calls methods
        // of unchecked when it is not null.
        (image != 0).then(|| unsafe { read::surface(image) })
    }

    fn rect(at: u32) -> Option<jgl::Rect> {
        // SAFETY: a rectangle jgl's original reads unchecked when it is
        // not null.
        (at != 0).then(|| unsafe { read::rect_at(at) })
    }

    #[allow(clippy::too_many_arguments)]
    pub extern "thiscall" fn indexed(this: u32, dst: u32, sx: u32, sy: u32, x: u32, y: u32, w: u32, h: u32, mode: u32) -> u32 {
        let jgl = drawn("copy_38", || {
            Some(CopyCall::Indexed {
                source: surface(this)?,
                target: surface(dst)?,
                src: (sx as i32, sy as i32),
                at: (x as i32, y as i32),
                size: (w as i32, h as i32),
                mode,
            })
        });
        if jgl { original::<M8>(&INDEXED)(this, dst, sx, sy, x, y, w, h, mode) } else { 0 }
    }

    pub extern "thiscall" fn stretch(this: u32, dst: u32, src_rect: u32, dst_rect: u32) -> u32 {
        let jgl = drawn("copy", || {
            Some(CopyCall::Stretch { source: surface(this)?, target: surface(dst)?, src: rect(src_rect)?, dst: rect(dst_rect)? })
        });
        // jgl's copy returns 0 when it copied.
        if jgl { original::<M3>(&STRETCH)(this, dst, src_rect, dst_rect) } else { 0 }
    }

    pub extern "thiscall" fn keyed(this: u32, dst: u32, x: u32, y: u32) -> u32 {
        let jgl = drawn("keyed_draw_onto", || Some(CopyCall::Keyed { source: surface(this)?, target: surface(dst)?, x: x as i32, y: y as i32 }));
        if jgl { original::<M3>(&KEYED)(this, dst, x, y) } else { 0 }
    }

    pub extern "thiscall" fn slot_8c(this: u32, a: u32, b: u32, c: u32, d: u32) -> u32 {
        let jgl = drawn("keyed_draw_onto_8c", || Some(CopyCall::Other { slot: 0x8C, this: surface(this)?, args: [a, b, c, d] }));
        if jgl { original::<M4>(&SLOT_8C)(this, a, b, c, d) } else { 0 }
    }
}

/// The taken-over copy group (for the life of the process, in practice).
pub struct CopyGroup {
    _image: Takeover,
}

/// Takes over the copy group of jgl.dll at `base`. Its calls go to the
/// hooks the text group was installed with ([`crate::text::install`]
/// first).
pub fn install(base: u32) -> Result<CopyGroup, TakeoverError> {
    install_impl(base)
}

/// The slots this group takes over. +0x3C and +0x74 (copy slots of the
/// catalogue) are not seen called and stay jgl's.
pub const SLOTS: [u16; 4] = [0x38, 0x40, 0x84, 0x8C];

#[cfg(all(windows, target_arch = "x86"))]
fn install_impl(base: u32) -> Result<CopyGroup, TakeoverError> {
    use crate::takeover::{Entry, take_over};
    use entries::*;
    use jgl::slots::Class;
    if !crate::state::has_hooks() {
        return Err(TakeoverError::NoHooks);
    }
    let image = [
        Entry { offset: SLOTS[0], function: indexed as M8 as usize as u32, original: &INDEXED },
        Entry { offset: SLOTS[1], function: stretch as M3 as usize as u32, original: &STRETCH },
        Entry { offset: SLOTS[2], function: keyed as M3 as usize as u32, original: &KEYED },
        Entry { offset: SLOTS[3], function: slot_8c as M4 as usize as u32, original: &SLOT_8C },
    ];
    Ok(CopyGroup { _image: take_over(base, Class::Image, &image)? })
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn install_impl(_base: u32) -> Result<CopyGroup, TakeoverError> {
    Err(inproc::hook::HookError::Unsupported.into())
}

#[cfg(test)]
mod tests {
    use super::SLOTS;
    use jgl::slots::{Class, Group};

    /// The entry points' argument counts are jgl's (`ret n`).
    #[test]
    fn the_slots_are_copy_slots_with_their_argument_counts() {
        let stacks: Vec<(u16, u8)> = SLOTS
            .iter()
            .map(|&o| {
                let s = Class::Image.slot(o).unwrap();
                assert_eq!(s.group, Group::Copy);
                (o, s.stack)
            })
            .collect();
        assert_eq!(stacks, [(0x38, 32), (0x40, 12), (0x84, 12), (0x8C, 16)]);
    }
}
