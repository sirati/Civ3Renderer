//! `CIV3R_JGL_COMPARE=1`: the present group (graphics +0x30, +0xA4)
//! compared call by call with the layers around it (temporary, for the
//! takeover's check, `docs/jgl-replacement.md` §9).
//!
//! - present: the exe's `present(rect)` (0x606680) calls jgl's +0xA4 once
//!   with its rectangle (or none when it restores the whole screen). Each
//!   exe call must bring exactly one `Present` record, with that
//!   rectangle, the exe's back buffer as the image copied from, and the
//!   decision the old slot hook made (`skipping()`).
//! - invalidate: jgl's +0x30 is `InvalidateRect(window, rect, FALSE)`.
//!   Each `Invalidate` record must be followed by one call of jgl's
//!   `InvalidateRect` import with the same rectangle.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

pub static ON: AtomicBool = AtomicBool::new(false);

struct Counts {
    exe_presents: AtomicU64,
    present_records: AtomicU64,
    present_same: AtomicU64,
    present_differ: AtomicU64,
    present_missing: AtomicU64,
    present_extra: AtomicU64,
    invalidate_records: AtomicU64,
    invalidate_same: AtomicU64,
    invalidate_differ: AtomicU64,
    invalidate_missing: AtomicU64,
    import_other: AtomicU64,
}

static C: Counts = Counts {
    exe_presents: AtomicU64::new(0),
    present_records: AtomicU64::new(0),
    present_same: AtomicU64::new(0),
    present_differ: AtomicU64::new(0),
    present_missing: AtomicU64::new(0),
    present_extra: AtomicU64::new(0),
    invalidate_records: AtomicU64::new(0),
    invalidate_same: AtomicU64::new(0),
    invalidate_differ: AtomicU64::new(0),
    invalidate_missing: AtomicU64::new(0),
    import_other: AtomicU64::new(0),
};

thread_local! {
    /// The policy's last answer for a present.
    static DECISION: Cell<bool> = const { Cell::new(true) };
    /// Inside the exe's present: its rectangle and the records seen.
    static PRESENT: Cell<Option<(Option<[i32; 4]>, u32)>> = const { Cell::new(None) };
    /// An invalidate recorded, its import call not yet seen.
    static INVALIDATE: Cell<Option<Option<[i32; 4]>>> = const { Cell::new(None) };
    static BACK_BUFFER: Cell<u32> = const { Cell::new(0) };
}

pub fn decided(original: bool) {
    DECISION.set(original);
}

pub fn on() -> bool {
    ON.load(Relaxed)
}

fn rect(r: Option<jgl::Rect>) -> Option<[i32; 4]> {
    r.map(|r| r.to_array())
}

/// The exe's present starts, with the rectangle it passes on to jgl.
pub fn exe_present_begin(rect: u32, back_buffer: u32) {
    BACK_BUFFER.set(back_buffer);
    let r = if rect == 0 { None } else { civ3_sys::ui_hooks::obj::rect(rect) };
    PRESENT.set(Some((r, 0)));
}

pub fn exe_present_end() {
    let n = C.exe_presents.fetch_add(1, Relaxed) + 1;
    match PRESENT.take() {
        Some((_, 0)) => {
            C.present_missing.fetch_add(1, Relaxed);
        }
        Some((_, 1)) => {}
        Some(_) => {
            C.present_extra.fetch_add(1, Relaxed);
        }
        None => {}
    }
    if n % 500 == 0 {
        report();
    }
}

/// A record of the group, with the decision the policy made for it.
pub fn record(call: &jgl::PresentCall) {
    if !on() {
        return;
    }
    let original = DECISION.get();
    match *call {
        jgl::PresentCall::Present { rect: r, back } => {
            C.present_records.fetch_add(1, Relaxed);
            let old_decision = !crate::ui_trace::skipping();
            let Some((expected, seen)) = PRESENT.get() else {
                C.present_extra.fetch_add(1, Relaxed);
                return;
            };
            PRESENT.set(Some((expected, seen + 1)));
            let same = expected == rect(r) && back.map(|b| b.owner) == Some(BACK_BUFFER.get()) && old_decision == original;
            if same {
                C.present_same.fetch_add(1, Relaxed);
            } else {
                let differ = C.present_differ.fetch_add(1, Relaxed);
                if differ < 20 {
                    log::warn!("jgl compare: present differs: exe rect {expected:?}, record {:?} back {:?} (exe {:#x}), decision old {old_decision} new {original}", rect(r), back.map(|b| b.owner), BACK_BUFFER.get());
                }
            }
        }
        jgl::PresentCall::Invalidate { rect: r } => {
            C.invalidate_records.fetch_add(1, Relaxed);
            if INVALIDATE.replace(Some(rect(r))).is_some() {
                C.invalidate_missing.fetch_add(1, Relaxed);
            }
        }
    }
}

/// jgl's `InvalidateRect` import was called.
pub fn import_invalidate(rect: u32) {
    let seen = if rect == 0 { None } else { civ3_sys::ui_hooks::obj::rect(rect) };
    match INVALIDATE.take() {
        Some(expected) if expected == seen => {
            C.invalidate_same.fetch_add(1, Relaxed);
        }
        Some(expected) => {
            let differ = C.invalidate_differ.fetch_add(1, Relaxed);
            if differ < 20 {
                log::warn!("jgl compare: invalidate differs: record {expected:?}, InvalidateRect {seen:?}");
            }
        }
        None => {
            C.import_other.fetch_add(1, Relaxed);
        }
    }
}

pub fn report() {
    if !on() {
        return;
    }
    log::info!(
        "jgl compare (present group): exe presents {}, present records {} (same {}, differ {}, missing {}, extra {}); invalidate records {} (same {}, differ {}, import never called {}), InvalidateRect calls without a record {}",
        C.exe_presents.load(Relaxed),
        C.present_records.load(Relaxed),
        C.present_same.load(Relaxed),
        C.present_differ.load(Relaxed),
        C.present_missing.load(Relaxed),
        C.present_extra.load(Relaxed),
        C.invalidate_records.load(Relaxed),
        C.invalidate_same.load(Relaxed),
        C.invalidate_differ.load(Relaxed),
        C.invalidate_missing.load(Relaxed),
        C.import_other.load(Relaxed),
    );
}
