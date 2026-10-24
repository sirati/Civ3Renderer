//! Method-table takeover: our entry points written into jgl's vtables, one
//! method group at a time, with jgl's original kept for the fallback.
//!
//! jgl's vtables are in its `.rdata`, one per class and shared by every
//! object of the class, so a slot written there takes over the method for
//! every object, existing or future. A group is only taken over if every
//! one of its slots still holds the function jgl.dll of 2004-03-17 has
//! there (`jgl::slots`): another build, or another patch in the way, leaves
//! the group to jgl.

use std::sync::atomic::{AtomicU32, Ordering};

use inproc::Ptr32;
use inproc::hook::{HookError, SlotPatch};
use jgl::slots::Class;

/// One slot to take over.
pub struct Entry {
    pub offset: u16,
    /// Our entry point.
    pub function: u32,
    /// Where the original goes; the entry point calls it from there.
    pub original: &'static AtomicU32,
}

#[derive(Debug, thiserror::Error)]
pub enum TakeoverError {
    #[error("{class:?}+{offset:#x} holds {found:#x}, expected jgl's {expected:#x}")]
    Unexpected { class: Class, offset: u16, found: u32, expected: u32 },
    #[error("{class:?}+{offset:#x} is not a slot of jgl's table")]
    NoSuchSlot { class: Class, offset: u16 },
    #[error(transparent)]
    Hook(#[from] HookError),
}

/// A taken-over group; the originals go back on drop.
pub struct Takeover {
    _patches: Vec<SlotPatch>,
}

fn slot_cell(base: u32, class: Class, offset: u16) -> Ptr32<u32> {
    Ptr32::new(base + class.vtable_rva() + offset as u32)
}

/// Takes over `entries` of `class` in jgl.dll loaded at `base`: all of
/// them or none.
pub fn take_over(base: u32, class: Class, entries: &[Entry]) -> Result<Takeover, TakeoverError> {
    for e in entries {
        let slot = class.slot(e.offset).ok_or(TakeoverError::NoSuchSlot { class, offset: e.offset })?;
        let found = slot_cell(base, class, e.offset).read().map_err(HookError::from)?;
        let expected = base + slot.rva;
        if found != expected {
            return Err(TakeoverError::Unexpected { class, offset: e.offset, found, expected });
        }
    }
    let mut patches = Vec::with_capacity(entries.len());
    for e in entries {
        // The original first: the slot may be called the moment it changes.
        e.original.store(base + class.slot(e.offset).map_or(0, |s| s.rva), Ordering::Release);
        patches.push(SlotPatch::replace(slot_cell(base, class, e.offset), e.function)?);
    }
    Ok(Takeover { _patches: patches })
}
