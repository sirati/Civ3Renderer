#![forbid(unsafe_code)]

//! Hand-off primitives for code that shares a process with the game.
//!
//! The game's thread runs foreign code and must never wait for one of
//! ours. Nothing here blocks: there are no locks to wait on and every
//! send is unbounded. See `docs/concurrency.md` for where each is used.
//!
//! - [`Latest`]: the newest value of something one side produces and
//!   others read whenever they like (a watch channel without its lock).
//! - [`Mailbox`]: a FIFO of commands for a thread that polls it at safe
//!   points, reachable from a `static` because the receiving end is
//!   shared (`try_recv` takes `&self`).
//! - [`Posted`]: payloads for private window messages. The message
//!   carries a token; the game thread's handler takes what was sent for
//!   it, in order, with consecutive pointer moves merged.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::SeqCst};

use arc_swap::ArcSwapOption;

/// The newest value published, read without waiting by any thread.
///
/// Readers get an `Arc` of the value as it was published. A writer never
/// waits for readers and readers never wait for a writer.
pub struct Latest<T>(ArcSwapOption<T>);

impl<T> Latest<T> {
    /// Nothing published yet. Usable in a `static`.
    pub const fn new() -> Self {
        Self(ArcSwapOption::const_empty())
    }

    pub fn publish(&self, value: T) {
        self.0.store(Some(Arc::new(value)));
    }

    pub fn publish_arc(&self, value: Arc<T>) {
        self.0.store(Some(value));
    }

    pub fn clear(&self) {
        self.0.store(None);
    }

    /// Replaces the value and returns what was there.
    pub fn take(&self) -> Option<Arc<T>> {
        self.0.swap(None)
    }

    pub fn load(&self) -> Option<Arc<T>> {
        self.0.load_full()
    }

    /// Publishes `f` of the current value. With several writers, `f` may
    /// run again on a newer value; keep it free of side effects.
    pub fn update(&self, mut f: impl FnMut(Option<&T>) -> Option<T>) {
        self.0.rcu(|old| f(old.as_deref()).map(Arc::new));
    }
}

impl<T: Clone> Latest<T> {
    pub fn get(&self) -> Option<T> {
        self.0.load().as_deref().cloned()
    }
}

impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Commands for one thread that drains them at points it chooses.
///
/// Any thread may send; sending never blocks. The receiving thread polls
/// with [`Mailbox::try_recv`] or [`Mailbox::drain`] and so never parks,
/// which keeps senders on the channel's lock-free path.
pub struct Mailbox<T> {
    tx: crossbeam_channel::Sender<T>,
    rx: crossbeam_channel::Receiver<T>,
}

impl<T> Mailbox<T> {
    pub fn new() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx }
    }

    pub fn send(&self, value: T) {
        // Both ends live in `self`: the channel cannot be disconnected.
        let _ = self.tx.send(value);
    }

    pub fn try_recv(&self) -> Option<T> {
        self.rx.try_recv().ok()
    }

    /// Everything sent so far.
    pub fn drain(&self) -> impl Iterator<Item = T> + '_ {
        self.rx.try_iter()
    }

    pub fn len(&self) -> usize {
        self.rx.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rx.is_empty()
    }
}

impl<T> Default for Mailbox<T> {
    fn default() -> Self {
        Self::new()
    }
}

enum Entry<T> {
    Item(u32, T),
    Retract(u32),
}

/// What the consuming thread received but has not handled yet (its
/// messages are still queued). Keep it in that thread's `thread_local!`.
pub struct Inbox<T> {
    entries: Vec<(u32, Option<T>)>,
}

impl<T> Default for Inbox<T> {
    fn default() -> Self {
        Self { entries: Vec::new() }
    }
}

impl<T> Inbox<T> {
    pub fn len(&self) -> usize {
        self.entries.iter().filter(|e| e.1.is_some()).count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Whether token `a` was handed out no later than `b` (wrapping).
fn not_after(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) <= 0
}

/// Payloads of private window messages, one token per posted message.
///
/// The producer sends the payload here and posts the token; the window's
/// thread takes the payload when the message arrives, so it keeps its
/// place among the window's other queued input. Coalescible payloads (a
/// pointer move) join the newest still-unhandled token of the same `key`
/// instead of posting another message: [`Posted::take`] then hands out
/// only the last of them.
///
/// Exactly-once delivery without a shared map: the consumer closes a
/// token before draining, and a producer that merged into a token checks
/// afterwards that it was still open. If it was closed meanwhile, the
/// producer posts a new token, and the payload is delivered with whichever
/// of the two messages arrives after it was sent (both take everything up
/// to their own token).
pub struct Posted<T> {
    mailbox: Mailbox<Entry<T>>,
    next: AtomicU32,
    /// The token coalescible payloads may join, with their key: `token <<
    /// 32 | key`; 0 when none may.
    open: AtomicU64,
}

impl<T> Default for Posted<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Posted<T> {
    pub fn new() -> Self {
        Self { mailbox: Mailbox::new(), next: AtomicU32::new(0), open: AtomicU64::new(0) }
    }

    fn token(&self) -> u32 {
        loop {
            let t = self.next.fetch_add(1, SeqCst).wrapping_add(1);
            if t != 0 {
                return t;
            }
        }
    }

    /// Sends `item`. With `key`, it may join the open token of the same key.
    /// Returns its token and whether a message carrying it must be posted.
    pub fn push(&self, item: T, key: Option<u32>) -> (u32, bool) {
        if let Some(key) = key {
            let open = self.open.load(SeqCst);
            if open != 0 && open as u32 == key {
                let token = (open >> 32) as u32;
                self.mailbox.send(Entry::Item(token, item));
                if self.open.load(SeqCst) == open {
                    return (token, false);
                }
                // The consumer closed it meanwhile: a later message delivers
                // the payload if the closed one's drain missed it.
                let token = self.token();
                self.open.store(u64::from(token) << 32 | u64::from(key), SeqCst);
                return (token, true);
            }
        }
        let token = self.token();
        self.mailbox.send(Entry::Item(token, item));
        self.open.store(key.map_or(0, |key| u64::from(token) << 32 | u64::from(key)), SeqCst);
        (token, true)
    }

    /// Nothing sent from now on joins an earlier payload.
    pub fn barrier(&self) {
        self.open.store(0, SeqCst);
    }

    fn close(&self, token: u32) {
        let open = self.open.load(SeqCst);
        if open != 0 && (open >> 32) as u32 == token {
            // Fails only if a producer moved on to another token already.
            let _ = self.open.compare_exchange(open, 0, SeqCst, SeqCst);
        }
    }

    /// The message for `token` could not be posted: drop its payloads.
    pub fn retract(&self, token: u32) {
        self.close(token);
        self.mailbox.send(Entry::Retract(token));
    }

    /// On the consuming thread, for the message carrying `token`: every
    /// payload up to it not yet taken, oldest first, merged payloads as
    /// their last.
    pub fn take(&self, token: u32, inbox: &mut Inbox<T>) -> Vec<T> {
        self.close(token);
        for entry in self.mailbox.drain() {
            match entry {
                Entry::Item(t, item) => {
                    // A merged payload replaces the one before it.
                    if let Some(last) = inbox.entries.iter_mut().rev().find(|e| e.0 == t)
                        && last.1.is_some()
                    {
                        last.1 = Some(item);
                    } else {
                        inbox.entries.push((t, Some(item)));
                    }
                }
                Entry::Retract(t) => inbox.entries.iter_mut().filter(|e| e.0 == t).for_each(|e| e.1 = None),
            }
        }
        let mut out = Vec::new();
        inbox.entries.retain_mut(|(t, item)| {
            if not_after(*t, token) {
                out.extend(item.take());
                false
            } else {
                true
            }
        });
        out
    }
}

/// Wakes one thread of ours that waits for something to do (the
/// publisher): any thread may [`ring`](Wake::ring) it, the game's
/// included, which then never waits (one atomic swap and, when the bell
/// was not rung yet, an unpark: a futex or `WakeByAddressSingle`, no
/// lock). Rings while the waiter is busy collapse into one wake-up.
pub struct Wake {
    waiter: std::sync::OnceLock<std::thread::Thread>,
    rung: std::sync::atomic::AtomicBool,
}

impl Wake {
    /// Usable in a `static`.
    pub const fn new() -> Self {
        Self { waiter: std::sync::OnceLock::new(), rung: std::sync::atomic::AtomicBool::new(false) }
    }

    /// Something to do for the waiter.
    pub fn ring(&self) {
        if !self.rung.swap(true, SeqCst) {
            if let Some(t) = self.waiter.get() {
                t.unpark();
            }
        }
    }

    /// Waits until rung or `timeout` passed; whether it was rung. Only one
    /// thread waits on a `Wake`: the first that does.
    pub fn wait(&self, timeout: std::time::Duration) -> bool {
        let me = self.waiter.get_or_init(std::thread::current);
        debug_assert_eq!(me.id(), std::thread::current().id(), "one waiter per Wake");
        let until = std::time::Instant::now() + timeout;
        loop {
            if self.rung.swap(false, SeqCst) {
                return true;
            }
            let left = until.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return false;
            }
            // (Wakes early on a ring; a spurious wake-up loops.)
            std::thread::park_timeout(left);
        }
    }
}

impl Default for Wake {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
