//! Native art jobs and the whole description they belong to.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};

use scene::wire::Assets;

pub(super) struct ArtQueue {
    next: Mutex<VecDeque<Assets>>,
    ready: Condvar,
    newest_whole: AtomicU64,
}

impl ArtQueue {
    pub const fn new() -> Self {
        Self { next: Mutex::new(VecDeque::new()), ready: Condvar::new(), newest_whole: AtomicU64::new(0) }
    }

    pub fn push(&self, a: Assets) {
        let mut next = self.next.lock().unwrap_or_else(|e| e.into_inner());
        if self.superseded(a.generation) {
            return;
        }
        if !a.append {
            self.newest_whole.store(a.generation, Ordering::Release);
            next.clear();
        }
        next.push_back(a);
        self.ready.notify_one();
    }

    pub fn pop(&self) -> Assets {
        let mut next = self.next.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(a) = next.pop_front() {
                return a;
            }
            next = self.ready.wait(next).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// Additions after this generation share its whole description, so
    /// they remain valid. A newer whole supersedes both it and its additions.
    pub fn superseded(&self, generation: u64) -> bool {
        self.newest_whole.load(Ordering::Acquire) > generation
    }

    /// Publishes a result only if no newer whole has arrived, even if it
    /// arrived during the last file load or atlas packing. This check and
    /// publication share the queue lock with `push`, so a newer request
    /// cannot slip between them. The callback must not use the queue.
    pub fn with_current(&self, generation: u64, publish: impl FnOnce()) -> bool {
        let _next = self.next.lock().unwrap_or_else(|e| e.into_inner());
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
        assert!(queue.next.lock().unwrap().is_empty());
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
