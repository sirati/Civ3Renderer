//! The game thread's [`Jgl`] that every taken-over group records through,
//! and what the entry points of all groups share.
//!
//! One `Jgl` per thread (the game's, in practice: the counting mode checks
//! it), made on the thread's first call from the hooks factory the text
//! group was installed with. The groups installed later record into the
//! same one, so the consumer sees every call in the order the game made
//! them.

#![cfg_attr(not(all(windows, target_arch = "x86")), allow(dead_code, unused_imports))]

use std::cell::RefCell;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use jgl::{FontDesc, Hooks, Jgl, Producer};

/// Makes the hooks of one thread's [`Jgl`] (the game's thread, in
/// practice; see [`threads`]).
pub type HooksFactory = Box<dyn Fn() -> Hooks + Send + Sync>;

pub(crate) static FACTORY: OnceLock<HooksFactory> = OnceLock::new();
pub(crate) static STOCK: OnceLock<FontDesc> = OnceLock::new();
/// Threads that made a [`Jgl`] (more than one breaks the single-producer
/// assumption: each would see only its own calls' state).
static THREADS: AtomicU32 = AtomicU32::new(0);
/// Calls that arrived while this thread's `Jgl` was busy (a hook calling
/// back into jgl): passed to jgl's original, not recorded.
static REENTRANT: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static JGL: RefCell<Option<Jgl>> = const { RefCell::new(None) };
}

/// How many threads have drawn through the takeover.
pub fn threads() -> u32 {
    THREADS.load(Ordering::Relaxed)
}

/// Calls passed on unrecorded because a hook re-entered jgl.
pub fn reentrant() -> u32 {
    REENTRANT.load(Ordering::Relaxed)
}

/// Whether the hooks factory is in place (the text group is installed).
pub(crate) fn has_hooks() -> bool {
    FACTORY.get().is_some()
}

pub(crate) fn stock() -> FontDesc {
    STOCK.get().cloned().unwrap_or(FontDesc { face: "System".into(), height: 16, weight: 700, italic: false, underline: false })
}

/// Runs `f` on this thread's producer and consumes its records. `None`:
/// not installed, or re-entered from a hook.
pub(crate) fn record<R>(f: impl FnOnce(&mut Producer) -> R) -> Option<R> {
    JGL.try_with(|cell| {
        let Ok(mut jgl) = cell.try_borrow_mut() else {
            REENTRANT.fetch_add(1, Ordering::Relaxed);
            return None;
        };
        if jgl.is_none() {
            let factory = FACTORY.get()?;
            if THREADS.fetch_add(1, Ordering::Relaxed) > 0 {
                log::warn!("jgl: a second thread draws (thread {:?})", std::thread::current().id());
            }
            *jgl = Some(Jgl::new(factory(), stock()));
        }
        jgl.as_mut().map(|j| j.with(f))
    })
    .ok()
    .flatten()
}

/// Runs `f` and never unwinds into the game: a panic logs and is answered
/// with `fallback`.
pub(crate) fn guarded<R>(name: &str, f: impl FnOnce() -> R, fallback: impl FnOnce() -> R) -> R {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            log::error!("jgl {name} panicked; calling jgl's original");
            fallback()
        }
    }
}

#[cfg(all(windows, target_arch = "x86"))]
pub(crate) mod read {
    use crate::layout::{JglImage, Rect32};
    use jgl::{ImageId, Surface};
    use std::mem::offset_of;

    /// A `u32` at `at`.
    ///
    /// # Safety
    /// `at` must be readable: a field of the object the entry point was
    /// called on, or memory jgl's original reads unchecked.
    pub unsafe fn u32_at(at: u32) -> u32 {
        // SAFETY: the caller's contract.
        unsafe { std::ptr::read_unaligned(at as usize as *const u32) }
    }

    /// Writes `v` at `at`.
    ///
    /// # Safety
    /// `at` must be a writable field of the object the entry point was
    /// called on.
    pub unsafe fn set_u32(at: u32, v: u32) {
        // SAFETY: the caller's contract.
        unsafe { std::ptr::write_unaligned(at as usize as *mut u32, v) }
    }

    /// A `RECT` at `at`.
    ///
    /// # Safety
    /// As [`u32_at`], for 16 bytes.
    pub unsafe fn rect_at(at: u32) -> jgl::Rect {
        // SAFETY: the caller's contract; `Rect32` is plain data.
        let r: Rect32 = unsafe { std::ptr::read_unaligned(at as usize as *const Rect32) };
        r.into()
    }

    /// A field of a `JGL_Image`.
    ///
    /// # Safety
    /// `image` must be a `JGL_Image`: the `this` of one of its methods, or
    /// an image argument jgl's original dereferences unchecked.
    pub unsafe fn image_field(image: u32, offset: usize) -> u32 {
        // SAFETY: the field is inside the image's 0x4D4 bytes.
        unsafe { u32_at(image + offset as u32) }
    }

    /// An image's owner, clip and bounds.
    ///
    /// # Safety
    /// As [`image_field`].
    pub unsafe fn surface(image: u32) -> Surface {
        // SAFETY: the caller's contract; the fields are inside the image.
        unsafe {
            Surface {
                image: ImageId(image),
                owner: image_field(image, offset_of!(JglImage, owner)),
                row: image_field(image, 0x40) as i32,
                clip: rect_at(image + offset_of!(JglImage, clip) as u32),
                bounds: rect_at(image + offset_of!(JglImage, image_rect) as u32),
            }
        }
    }
}
