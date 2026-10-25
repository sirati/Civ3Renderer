//! The objects of ours alive, for graphics +0x0C `release`, which deletes
//! every object jgl made (jgl keeps them in a list its base constructor
//! fills, RVA 0x1310; release empties it from the back, then deletes the
//! graphics object). The game's thread alone makes and deletes objects.

use std::cell::RefCell;

thread_local! {
    static LIVE: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

/// `object` was made.
pub fn made(object: u32) {
    LIVE.with_borrow_mut(|l| l.push(object));
}

/// `object` is going.
pub fn gone(object: u32) {
    LIVE.with_borrow_mut(|l| {
        if let Some(i) = l.iter().rposition(|&o| o == object) {
            l.remove(i);
        }
    });
}

/// The last object made that is still alive.
pub fn last() -> Option<u32> {
    LIVE.with_borrow(|l| l.last().copied())
}
