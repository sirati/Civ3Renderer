//! Native art jobs and the whole description they belong to.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};

use civ3r_sync::{Latest, Mailbox};
use scene::wire::Assets;

/// Pushed on the game's thread, popped by the art thread. Pushing never
/// waits: the art thread parks rather than waiting in the channel, and is
/// unparked by each push.
pub(super) struct ArtQueue {
    // lock-free: `LazyLock` only makes the channel on first use.
    next: LazyLock<Mailbox<Assets>>,
    /// The thread waiting in `pop`.
    popper: Latest<std::thread::Thread>,
    newest_whole: AtomicU64,
}

impl ArtQueue {
    pub const fn new() -> Self {
        Self { next: LazyLock::new(Mailbox::new), popper: Latest::new(), newest_whole: AtomicU64::new(0) }
    }

    pub fn push(&self, a: Assets) {
        if self.superseded(a.generation) {
            return;
        }
        if !a.append {
            // What waits before it is dropped by `pop` as superseded.
            self.newest_whole.store(a.generation, Ordering::Release);
        }
        self.next.send(a);
        if let Some(t) = self.popper.load() {
            t.unpark();
        }
    }

    pub fn pop(&self) -> Assets {
        let me = std::thread::current();
        if self.popper.load().is_none_or(|t| t.id() != me.id()) {
            self.popper.publish(me);
        }
        loop {
            while let Some(a) = self.next.try_recv() {
                if !self.superseded(a.generation) {
                    return a;
                }
            }
            std::thread::park();
        }
    }

    /// Additions after this generation share its whole description, so
    /// they remain valid. A newer whole supersedes both it and its additions.
    pub fn superseded(&self, generation: u64) -> bool {
        self.newest_whole.load(Ordering::Acquire) > generation
    }

    /// Publishes a result only if no newer whole has arrived, even if it
    /// arrived during the last file load or atlas packing. A newer whole
    /// pushed after the check is published after this, by the same thread,
    /// so it still ends up the viewer's.
    pub fn with_current(&self, generation: u64, publish: impl FnOnce()) -> bool {
        if self.superseded(generation) {
            false
        } else {
            publish();
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(generation: u64, append: bool) -> Assets {
        Assets { generation, append, ..Default::default() }
    }

    #[test]
    fn a_new_whole_replaces_queued_work_and_keeps_its_own_additions() {
        let queue = ArtQueue::new();
        queue.push(job(1, false));
        let running = queue.pop();
        queue.push(job(2, true));
        queue.push(job(3, false));
        queue.push(job(4, true));
        assert!(queue.superseded(running.generation));
        assert!(queue.superseded(2));
        assert!(!queue.superseded(3) && !queue.superseded(4));
        assert_eq!(queue.pop().generation, 3);
        assert_eq!(queue.pop().generation, 4);
        // A late addition of the old whole cannot enter the new queue.
        queue.push(job(2, true));
        assert!(queue.next.is_empty());
    }

    #[test]
    fn a_parked_art_thread_wakes_for_each_push() {
        let queue = ArtQueue::new();
        std::thread::scope(|s| {
            let q = &queue;
            let art = s.spawn(move || (1..=20).map(|_| q.pop().generation).collect::<Vec<_>>());
            for generation in 1..=20 {
                std::thread::sleep(std::time::Duration::from_micros(300));
                queue.push(job(generation, true));
            }
            assert_eq!(art.join().unwrap(), (1..=20).collect::<Vec<_>>());
        });
    }

    #[test]
    fn a_compile_finishing_after_a_new_whole_cannot_publish_its_result() {
        let queue = ArtQueue::new();
        queue.push(job(1, false));
        let (loaded_tx, loaded) = std::sync::mpsc::channel();
        let (finish_tx, finish) = std::sync::mpsc::channel();
        std::thread::scope(|s| {
            let running_queue = &queue;
            let worker = s.spawn(move || {
                let running = running_queue.pop();
                loaded_tx.send(()).unwrap();
                finish.recv().unwrap();
                // Simulate cancellation after the final loader check: the
                // completed atlas must still never reach the viewer.
                running_queue.with_current(running.generation, || panic!("obsolete atlas published"))
            });
            loaded.recv().unwrap();
            queue.push(job(3, false));
            finish_tx.send(()).unwrap();
            assert!(!worker.join().unwrap());
        });
        let latest = queue.pop();
        let mut given = None;
        assert!(queue.with_current(latest.generation, || given = Some(latest.generation)));
        assert_eq!(given, Some(3));
    }
}
