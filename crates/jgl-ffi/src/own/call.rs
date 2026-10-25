//! Calls into the game: methods of its objects (`thiscall`, the method's
//! address read from the object's vtable), as jgl makes them.

/// Calls method `slot` (vtable byte offset) of `object` with `args` (at
/// most four words): `thiscall`, the callee pops them.
///
/// # Safety
/// `object` must be an object of the game whose vtable has a method at
/// `slot` taking exactly `args.len()` stack words, and the moment must be
/// one where jgl would call it (the game's thread).
pub unsafe fn method(object: u32, slot: u16, args: &[u32]) -> u32 {
    // SAFETY: the caller's contract: the object's first word is its vtable.
    let f = unsafe {
        let vtable = std::ptr::read_unaligned(object as usize as *const u32);
        std::ptr::read_unaligned((vtable + slot as u32) as usize as *const u32) as usize
    };
    // SAFETY: the caller's contract on the method's signature.
    unsafe { function(f as u32, object, args) }
}

/// Calls the `thiscall` function at `f` on `this` with `args` (at most
/// four words).
///
/// # Safety
/// As [`method`]: `f` takes exactly `args.len()` stack words.
pub unsafe fn function(f: u32, this: u32, args: &[u32]) -> u32 {
    type M0 = extern "thiscall" fn(u32) -> u32;
    type M1 = extern "thiscall" fn(u32, u32) -> u32;
    type M2 = extern "thiscall" fn(u32, u32, u32) -> u32;
    type M3 = extern "thiscall" fn(u32, u32, u32, u32) -> u32;
    type M4 = extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32;
    let f = f as usize;
    // SAFETY: the caller's contract; each type is the arity given.
    unsafe {
        match *args {
            [] => std::mem::transmute::<usize, M0>(f)(this),
            [a] => std::mem::transmute::<usize, M1>(f)(this, a),
            [a, b] => std::mem::transmute::<usize, M2>(f)(this, a, b),
            [a, b, c] => std::mem::transmute::<usize, M3>(f)(this, a, b, c),
            [a, b, c, d] => std::mem::transmute::<usize, M4>(f)(this, a, b, c, d),
            _ => unreachable!("no method of jgl's callers takes more than four words"),
        }
    }
}

/// An object of ours: `T` (one of the layouts) zeroed, its vtable `table`,
/// for the life of the object (freed by [`free`]).
pub fn alloc<T>(table: u32) -> u32 {
    let layout = std::alloc::Layout::new::<T>();
    // SAFETY: a non-zero size (every layout has a vtable word).
    let p = unsafe { std::alloc::alloc_zeroed(layout) };
    if p.is_null() {
        return 0;
    }
    // SAFETY: `p` is `size_of::<T>()` zeroed bytes, aligned for `T`.
    unsafe { std::ptr::write(p as *mut u32, table) };
    p as usize as u32
}

/// Frees an object made by [`alloc`] with the same `T`.
///
/// # Safety
/// `object` must come from `alloc::<T>` and not be used after.
pub unsafe fn free<T>(object: u32) {
    if object != 0 {
        // SAFETY: the caller's contract.
        unsafe { std::alloc::dealloc(object as usize as *mut u8, std::alloc::Layout::new::<T>()) };
    }
}

/// A `u32` field of an object of ours or the game's.
///
/// # Safety
/// `at` must be readable.
pub unsafe fn get(at: u32) -> u32 {
    // SAFETY: the caller's contract.
    unsafe { std::ptr::read_unaligned(at as usize as *const u32) }
}

/// Writes a `u32` field.
///
/// # Safety
/// `at` must be writable: a field of an object of ours, or memory the
/// game hands jgl to write (an out argument).
pub unsafe fn put(at: u32, v: u32) {
    // SAFETY: the caller's contract.
    unsafe { std::ptr::write_unaligned(at as usize as *mut u32, v) }
}
