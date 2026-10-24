//! Serial-bound presentation barriers on the native simulation thread.
//! Receipts arrive directly from the publisher, never its native window queue.
use std::{cell::Cell, marker::PhantomData, rc::Rc, sync::atomic::{AtomicU64, Ordering}, time::{Duration, Instant}};

#[derive(Clone, Copy, Default)]
struct Combat { pair: Option<(u32, u32)> }
thread_local! { static COMBAT: Cell<Combat> = Cell::new(Combat { pair: None }); }

pub struct CombatScope { previous: Combat, _thread: PhantomData<Rc<()>> }
impl Drop for CombatScope { fn drop(&mut self) { COMBAT.set(self.previous); } }
/// Nesting preserves an outer move/ZoC scope across an ordinary fight.
pub fn combat_scope() -> CombatScope { CombatScope { previous: COMBAT.replace(Combat::default()), _thread: PhantomData } }
/// Repeated visibility queries inside one fight do not create new barriers.
pub fn first_visible_event(attacker: u32, defender: u32) -> bool {
    COMBAT.with(|state| {
        let pair = Some((attacker, defender));
        if state.get().pair == pair { false } else { state.set(Combat { pair }); true }
    })
}

struct Barrier {
    pending: AtomicU64,
    arrived: AtomicU64,
    failed: AtomicU64,
    wake: handoff::Wake,
}
impl Barrier {
    const fn new() -> Self { Self { pending: AtomicU64::new(0), arrived: AtomicU64::new(0), failed: AtomicU64::new(0), wake: handoff::Wake::new() } }
    fn arm(&self, serial: u64) { self.pending.store(serial, Ordering::Release); }
    fn receipt(&self, serial: u64, arrived: bool) {
        if serial == 0 || self.pending.load(Ordering::Acquire) != serial { return; }
        if arrived { self.arrived.fetch_max(serial, Ordering::Release); }
        else { self.failed.fetch_max(serial, Ordering::Release); }
        self.wake.ring();
    }
    fn outcome(&self, serial: u64) -> Option<bool> {
        if self.pending.load(Ordering::Acquire) != serial || self.failed.load(Ordering::Acquire) >= serial { Some(false) }
        else if self.arrived.load(Ordering::Acquire) >= serial { Some(true) }
        else { None }
    }
    fn retire(&self, serial: u64) { let _ = self.pending.compare_exchange(serial, 0, Ordering::AcqRel, Ordering::Acquire); }
    fn wait(&self, serial: u64, watchdog: Duration) -> bool {
        self.wait_before_park(serial, watchdog, || {})
    }
    fn wait_before_park(&self, serial: u64, watchdog: Duration, mut before_park: impl FnMut()) -> bool {
        let deadline = Instant::now() + watchdog;
        loop {
            if let Some(arrived) = self.outcome(serial) { self.retire(serial); return arrived; }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() { self.receipt(serial, false); self.retire(serial); return false; }
            before_park();
            self.wake.wait(remaining);
        }
    }
}
static BARRIER: Barrier = Barrier::new();
/// Arm before publishing the matching CameraFocus/CameraArrival snapshot.
pub(crate) fn arm(serial: u64) { BARRIER.arm(serial); }
/// May run on the publisher while the game's own thread is parked.
pub fn receipt(serial: u64, arrived: bool) { BARRIER.receipt(serial, arrived); }
pub fn viewer_gone() { let serial = BARRIER.pending.load(Ordering::Acquire); BARRIER.receipt(serial, false); }
/// The deadline recovers from a missing viewer/device failure notification.
/// Healthy arrival releases immediately; this is never an animation delay.
pub(crate) fn wait(serial: u64) -> bool {
    if !crate::native_camera::active() { BARRIER.retire(serial); return false; }
    BARRIER.wait(serial, Duration::from_secs(2))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_duplicate_superseded_and_failed_receipts_cannot_release_a_new_request() {
        let b = Barrier::new();
        b.arm(4); b.receipt(3, true); b.receipt(5, true); assert_eq!(b.outcome(4), None);
        b.receipt(4, true); b.receipt(4, true); assert_eq!(b.outcome(4), Some(true));
        b.arm(5); b.receipt(4, true); assert_eq!(b.outcome(5), None);
        assert_eq!(b.outcome(4), Some(false));
        assert!(!b.wait(4, Duration::ZERO)); assert_eq!(b.pending.load(Ordering::Acquire), 5);
        b.receipt(5, false); assert_eq!(b.outcome(5), Some(false));
        b.arm(6); assert_eq!(b.outcome(6), None);
        assert!(!b.wait(6, Duration::ZERO));
        assert_eq!(b.pending.load(Ordering::Acquire), 0);
        b.receipt(6, true); b.arm(7); assert_eq!(b.outcome(7), None);
        b.receipt(7, true); assert!(b.wait(7, Duration::ZERO));
        assert_eq!(b.pending.load(Ordering::Acquire), 0);
    }
    #[test]
    fn a_real_waiter_is_released_by_the_matching_other_thread_receipt() {
        let barrier = std::sync::Arc::new(Barrier::new());
        barrier.arm(12);
        let (ready, entered) = std::sync::mpsc::sync_channel(1);
        let waiting = barrier.clone();
        let thread = std::thread::spawn(move || {
            let mut ready = Some(ready);
            waiting.wait_before_park(12, Duration::from_secs(1), || {
                if let Some(ready) = ready.take() { ready.send(()).unwrap(); }
            })
        });
        entered.recv_timeout(Duration::from_secs(1)).unwrap();
        barrier.receipt(11, true);
        assert_eq!(barrier.outcome(12), None);
        assert!(!thread.is_finished(), "a stale receipt cannot finish the pending wait");
        barrier.receipt(12, true);
        assert!(thread.join().unwrap());
        assert_eq!(barrier.pending.load(Ordering::Acquire), 0);
    }

    #[test]
    fn nested_combat_contexts_gate_once_and_restore_the_outer_pair() {
        let _outer = combat_scope();
        assert!(first_visible_event(1, 2)); assert!(!first_visible_event(1, 2));
        { let _inner = combat_scope(); assert!(first_visible_event(1, 2)); assert!(!first_visible_event(1, 2)); }
        assert!(!first_visible_event(1, 2)); assert!(first_visible_event(1, 3));
    }
}
