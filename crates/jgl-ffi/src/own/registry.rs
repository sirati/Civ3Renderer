//! The objects of ours alive, for graphics +0x0C `release`, which deletes
//! every object jgl made (jgl keeps them in a list its base constructor
//! fills, RVA 0x1310; release walks its cursor, then deletes the
//! graphics object). The game's thread alone makes and deletes objects.

use std::cell::RefCell;

thread_local! {
    static LIVE: RefCell<jgl::registry::Registry> = RefCell::new(jgl::registry::Registry::default());
}

/// `object` was made.
pub fn made(object: u32) {
    LIVE.with_borrow_mut(|l| l.made(object));
}

/// `object` is going.
pub fn gone(object: u32) {
    LIVE.with_borrow_mut(|l| l.gone(object));
}

/// The object under the release cursor.
pub fn current() -> Option<u32> {
    LIVE.with_borrow(|l| l.current())
}
