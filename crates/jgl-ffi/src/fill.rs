//! The fill group (`docs/jgl-replacement.md` §6, group 2): jgl's image
//! slots +0x44 `fill`, +0x48 `shade_rect`, +0x4C, +0x50, +0x54, +0x58
//! `replace_color`, +0x5C, +0x60 and +0x64 `line`.
//!
//! Every entry point reads its arguments into a [`jgl::FillCall`],
//! records it through the game thread's [`jgl::Jgl`] and runs jgl's
//! original only where [`jgl::PixelPolicy`] wants jgl's pixels. A
//! rectangle that is a null pointer where jgl takes none (+0x58) goes to
//! the original unrecorded.
//!
//! The image and the rectangles are read directly, for the reasons the text
//! group gives (`crate::text`): `this` is an image, and jgl's original
//! dereferences the rest unchecked.

#![cfg_attr(not(all(windows, target_arch = "x86")), allow(dead_code, unused_imports))]

use std::sync::atomic::AtomicU32;

use crate::takeover::{Takeover, TakeoverError};

#[cfg(all(windows, target_arch = "x86"))]
mod entries {
    use super::*;
    use crate::state::{guarded, read, record};
    use crate::takeover::original;
    use jgl::FillCall;
    use jgl::draw::MAX_ARGS;

    pub static FILL: AtomicU32 = AtomicU32::new(0);
    pub static SHADE: AtomicU32 = AtomicU32::new(0);
    pub static REPLACE_COLOR: AtomicU32 = AtomicU32::new(0);
    pub static LINE: AtomicU32 = AtomicU32::new(0);

    pub type M2 = extern "thiscall" fn(u32, u32, u32) -> u32;
    pub type M3 = extern "thiscall" fn(u32, u32, u32, u32) -> u32;
    pub type M4 = extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32;
    pub type M6 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32) -> u32;
    pub type M9 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32, u32, u32, u32) -> u32;

    /// Records `call` (made by `read`, which may answer `None`: an argument
    /// jgl takes but cannot be read) and says whether jgl's original runs.
    fn drawn(name: &str, read: impl FnOnce() -> Option<FillCall>) -> bool {
        guarded(name, || match read() {
            Some(call) => record(|p| p.fill(&call)).unwrap_or(true),
            None => true,
        }, || true)
    }

    fn target(this: u32) -> jgl::Surface {
        // SAFETY: `this` is the `JGL_Image` whose method this is.
        unsafe { read::surface(this) }
    }

    /// A `RECT*` argument, `None` for a null pointer.
    fn rect(at: u32) -> Option<jgl::Rect> {
        // SAFETY: a rectangle jgl's original reads unchecked when it is
        // not null.
        (at != 0).then(|| unsafe { read::rect_at(at) })
    }

    pub extern "thiscall" fn fill(this: u32, r: u32, color: u32) -> u32 {
        let jgl = drawn("fill", || Some(FillCall::Fill { target: target(this), rect: rect(r), color }));
        // jgl's fill returns 0.
        if jgl { original::<M2>(&FILL)(this, r, color) } else { 0 }
    }

    pub extern "thiscall" fn shade(this: u32, r: u32, color: u32, percent: u32) -> u32 {
        let jgl = drawn("shade_rect", || Some(FillCall::Shade { target: target(this), rect: rect(r), color, percent }));
        if jgl { original::<M3>(&SHADE)(this, r, color, percent) } else { 0 }
    }

    pub extern "thiscall" fn replace_color(this: u32, r: u32, from: u32, to: u32) -> u32 {
        let jgl = drawn("replace_color", || Some(FillCall::ReplaceColor { target: target(this), rect: rect(r)?, from, to }));
        if jgl { original::<M3>(&REPLACE_COLOR)(this, r, from, to) } else { 0 }
    }

    pub extern "thiscall" fn line(this: u32, x1: u32, y1: u32, x2: u32, y2: u32, color: u32, flag: u32) -> u32 {
        let jgl = drawn("line", || {
            Some(FillCall::Line { target: target(this), from: (x1 as i32, y1 as i32), to: (x2 as i32, y2 as i32), color, flag })
        });
        // jgl's line returns 0.
        if jgl { original::<M6>(&LINE)(this, x1, y1, x2, y2, color, flag) } else { 0 }
    }

    /// An entry point for a slot not seen called: its raw arguments.
    macro_rules! other {
        ($name:ident, $cell:ident, $slot:expr, $ty:ident, $($a:ident),*) => {
            pub static $cell: AtomicU32 = AtomicU32::new(0);

            pub extern "thiscall" fn $name(this: u32, $($a: u32),*) -> u32 {
                let jgl = drawn(stringify!($name), || {
                    let given = [$($a),*];
                    let mut args = [0; MAX_ARGS];
                    args[..given.len()].copy_from_slice(&given);
                    Some(FillCall::Other { slot: $slot, target: target(this), args })
                });
                if jgl { original::<$ty>(&$cell)(this, $($a),*) } else { 0 }
            }
        };
    }

    other!(slot_4c, SLOT_4C, 0x4C, M2, a, b);
    other!(slot_50, SLOT_50, 0x50, M3, a, b, c);
    other!(slot_54, SLOT_54, 0x54, M4, a, b, c, d);
    other!(slot_5c, SLOT_5C, 0x5C, M2, a, b);
    other!(slot_60, SLOT_60, 0x60, M9, a, b, c, d, e, f, g, h, i);
}

/// The taken-over fill group (for the life of the process, in practice).
pub struct FillGroup {
    _image: Takeover,
}

/// Takes over the fill group of jgl.dll at `base`. Its calls go to the
/// hooks the text group was installed with ([`crate::text::install`]
/// first).
pub fn install(base: u32) -> Result<FillGroup, TakeoverError> {
    install_impl(base)
}

#[cfg(all(windows, target_arch = "x86"))]
fn install_impl(base: u32) -> Result<FillGroup, TakeoverError> {
    use crate::takeover::{Entry, take_over};
    use entries::*;
    use jgl::slots::Class;
    if !crate::state::has_hooks() {
        return Err(TakeoverError::NoHooks);
    }
    let image = [
        Entry { offset: 0x44, function: fill as M2 as usize as u32, original: &FILL },
        Entry { offset: 0x48, function: shade as M3 as usize as u32, original: &SHADE },
        Entry { offset: 0x4C, function: slot_4c as M2 as usize as u32, original: &SLOT_4C },
        Entry { offset: 0x50, function: slot_50 as M3 as usize as u32, original: &SLOT_50 },
        Entry { offset: 0x54, function: slot_54 as M4 as usize as u32, original: &SLOT_54 },
        Entry { offset: 0x58, function: replace_color as M3 as usize as u32, original: &REPLACE_COLOR },
        Entry { offset: 0x5C, function: slot_5c as M2 as usize as u32, original: &SLOT_5C },
        Entry { offset: 0x60, function: slot_60 as M9 as usize as u32, original: &SLOT_60 },
        Entry { offset: 0x64, function: line as M6 as usize as u32, original: &LINE },
    ];
    Ok(FillGroup { _image: take_over(base, Class::Image, &image)? })
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn install_impl(_base: u32) -> Result<FillGroup, TakeoverError> {
    Err(inproc::hook::HookError::Unsupported.into())
}

#[cfg(test)]
mod tests {
    use jgl::slots::{Class, Group};

    /// The entry points' argument counts are jgl's (`ret n`).
    #[test]
    fn the_group_is_the_catalogue_s_fill_slots() {
        let fill: Vec<(u16, u8)> = Class::Image.slots().iter().filter(|s| s.group == Group::Fill).map(|s| (s.offset, s.stack)).collect();
        assert_eq!(fill, [(0x44, 8), (0x48, 12), (0x4C, 8), (0x50, 12), (0x54, 16), (0x58, 12), (0x5C, 8), (0x60, 36), (0x64, 24)]);
    }
}
