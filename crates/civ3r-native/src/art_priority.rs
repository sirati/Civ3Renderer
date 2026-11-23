//! The native art thread's priority: art made ahead (`crate::preload` on
//! the bridge's side, an addition whose context starts with "preload")
//! is compiled in the background, below the game, the publisher and the
//! viewer, so making it never costs the player frames (user, 2026-11-23:
//! joining a network game on an 8 GB Mac dropped to a few frames a second
//! while preload ran). Art a screen wants now goes at normal priority.
//!
//! The thread sets its own priority, lowered and raised again job by job:
//! a quality-of-service class on macOS, a thread priority on Windows.
//! (Linux leaves it: a thread may not raise its nice value back there.)

/// Whether an art request is art made ahead.
pub fn ahead(context: &str) -> bool {
    context.starts_with("preload")
}

#[cfg(target_os = "macos")]
pub fn set_background(background: bool) {
    // <sys/qos.h>: QOS_CLASS_USER_INITIATED 0x19, QOS_CLASS_UTILITY 0x11.
    unsafe extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
    }
    let class = if background { 0x11 } else { 0x19 };
    // SAFETY: sets the calling thread's own class; no pointers.
    let r = unsafe { pthread_set_qos_class_self_np(class, 0) };
    if r != 0 {
        log::debug!("civ3r-native: the art thread's QoS class stays ({r})");
    }
}

#[cfg(windows)]
pub fn set_background(background: bool) {
    // THREAD_PRIORITY_BELOW_NORMAL -1, THREAD_PRIORITY_NORMAL 0.
    unsafe extern "system" {
        fn GetCurrentThread() -> isize;
        fn SetThreadPriority(thread: isize, priority: i32) -> i32;
    }
    // SAFETY: the pseudo handle of the calling thread; no pointers.
    let ok = unsafe { SetThreadPriority(GetCurrentThread(), if background { -1 } else { 0 }) };
    if ok == 0 {
        log::debug!("civ3r-native: the art thread's priority stays");
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn set_background(_background: bool) {}

#[cfg(test)]
mod tests {
    #[test]
    fn only_art_made_ahead_goes_to_the_background() {
        assert!(super::ahead("preload (units on screen)"));
        assert!(super::ahead("preload (interface)"));
        assert!(!super::ahead("map"));
        assert!(!super::ahead("F7 screen"));
    }
}
