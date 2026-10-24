use super::*;
use std::sync::Arc;

#[test]
fn latest_publishes_and_clears_without_a_writer_waiting() {
    static VALUE: Latest<(i32, String)> = Latest::new();
    assert!(VALUE.get().is_none());
    VALUE.publish((1, "a".into()));
    let held = VALUE.load().unwrap();
    VALUE.publish((2, "b".into()));
    assert_eq!(*held, (1, "a".into()), "a reader keeps the value it loaded");
    assert_eq!(VALUE.get(), Some((2, "b".into())));
    VALUE.update(|old| old.map(|(n, s)| (n + 1, s.clone())));
    assert_eq!(VALUE.get().unwrap().0, 3);
    assert_eq!(VALUE.take().unwrap().0, 3);
    assert!(VALUE.load().is_none());
}

#[test]
fn mailbox_keeps_order_across_threads() {
    let mailbox = Arc::new(Mailbox::new());
    let senders: Vec<_> = (0..4)
        .map(|n| {
            let m = mailbox.clone();
            std::thread::spawn(move || (0..1000).for_each(|i| m.send((n, i))))
        })
        .collect();
    senders.into_iter().for_each(|s| s.join().unwrap());
    let mut last = [-1; 4];
    for (n, i) in mailbox.drain() {
        assert!(i > last[n], "each sender's commands arrive in order");
        last[n] = i;
    }
    assert_eq!(last, [999; 4]);
}

#[test]
fn moves_join_the_pending_token_until_a_barrier() {
    let posted = Posted::new();
    let mut inbox = Inbox::default();
    let (a, post) = posted.push("move 1", Some(1));
    assert!(post);
    assert_eq!(posted.push("move 2", Some(1)), (a, false));
    posted.barrier();
    let (b, post) = posted.push("move 3", Some(1));
    assert!(post && b != a);
    let (c, post) = posted.push("click", None);
    assert!(post && c != b);
    let (d, post) = posted.push("move 4", Some(1));
    assert!(post && d != c, "a key or button ends coalescing");
    assert_eq!(posted.take(a, &mut inbox), ["move 2"]);
    assert_eq!(posted.take(b, &mut inbox), ["move 3"]);
    assert_eq!(posted.take(c, &mut inbox), ["click"]);
    assert_eq!(posted.take(d, &mut inbox), ["move 4"]);
    assert!(inbox.is_empty());
}

#[test]
fn another_key_does_not_join() {
    let posted = Posted::new();
    let (a, _) = posted.push(1, Some(7));
    let (b, post) = posted.push(2, Some(8));
    assert!(post && a != b, "moves of another world or kind are separate");
    let mut inbox = Inbox::default();
    assert_eq!(posted.take(b, &mut inbox), [1, 2]);
}

#[test]
fn a_handled_token_takes_no_later_payload() {
    let posted = Posted::new();
    let mut inbox = Inbox::default();
    let (a, _) = posted.push("a", None);
    let (b, _) = posted.push("b", None);
    assert_eq!(posted.take(a, &mut inbox), ["a"], "b's message is still queued behind a key");
    assert_eq!(inbox.len(), 1);
    assert_eq!(posted.take(b, &mut inbox), ["b"]);
}

#[test]
fn a_move_after_its_token_closed_is_delivered_by_the_next() {
    let posted = Posted::new();
    let mut inbox = Inbox::default();
    let (a, _) = posted.push("move 1", Some(1));
    assert_eq!(posted.take(a, &mut inbox), ["move 1"]);
    let (b, post) = posted.push("move 2", Some(1));
    assert!(post && b != a, "a closed token is not joined");
    assert_eq!(posted.take(b, &mut inbox), ["move 2"]);
}

#[test]
fn retracted_payloads_are_dropped() {
    let posted = Posted::new();
    let mut inbox = Inbox::default();
    let (a, _) = posted.push("lost", Some(1));
    posted.retract(a);
    let (b, post) = posted.push("next", Some(1));
    assert!(post && b != a);
    assert_eq!(posted.take(b, &mut inbox), ["next"]);
}

#[test]
fn tokens_wrap_without_zero() {
    let posted = Posted::new();
    posted.next.store(u32::MAX - 1, SeqCst);
    let mut inbox = Inbox::default();
    let (a, _) = posted.push(1, None);
    let (b, _) = posted.push(2, None);
    assert_eq!((a, b), (u32::MAX, 1));
    assert_eq!(posted.take(a, &mut inbox), [1]);
    assert_eq!(posted.take(b, &mut inbox), [2]);
}

/// A producer thread posting through a real queue, a consumer handling the
/// messages in order: every payload exactly once, in order, and only the
/// moves merged.
#[test]
fn concurrent_producer_and_consumer_deliver_everything_once_in_order() {
    for round in 0..20 {
        let posted = Arc::new(Posted::new());
        let (queue, messages) = std::sync::mpsc::channel::<u32>();
        let producer = {
            let posted = posted.clone();
            std::thread::spawn(move || {
                for i in 0..5000u32 {
                    let coalescible = (i + round) % 5 != 0;
                    let (token, post) = posted.push(i, coalescible.then_some(3));
                    if post {
                        queue.send(token).unwrap();
                    }
                }
            })
        };
        let mut inbox = Inbox::default();
        let mut seen = Vec::new();
        while let Ok(token) = messages.recv() {
            seen.extend(posted.take(token, &mut inbox));
        }
        producer.join().unwrap();
        assert!(inbox.is_empty(), "the last message delivers everything");
        assert!(seen.windows(2).all(|w| w[0] < w[1]), "in order, none twice");
        for i in 0..5000u32 {
            if (i + round) % 5 == 0 {
                assert!(seen.contains(&i), "non-coalescible payload {i} delivered");
            }
        }
        assert_eq!(seen.last(), Some(&4999), "the newest move is never lost");
    }
}

#[test]
fn wake_returns_at_once_when_rung_and_collapses_rings() {
    use std::time::{Duration, Instant};
    static BELL: Wake = Wake::new();
    // Rung before waiting: at once.
    BELL.ring();
    BELL.ring();
    let t = Instant::now();
    assert!(BELL.wait(Duration::from_secs(5)));
    assert!(t.elapsed() < Duration::from_secs(1));
    // The two rings were one: the next wait times out.
    assert!(!BELL.wait(Duration::from_millis(20)));
    // Rung from another thread while waiting: woken well before the timeout.
    let ringer = std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(30));
        BELL.ring();
    });
    let t = Instant::now();
    assert!(BELL.wait(Duration::from_secs(10)));
    assert!(t.elapsed() < Duration::from_secs(5));
    ringer.join().unwrap();
    // Many rings from many threads, one waiter: none lost, none waits.
    let rings = Arc::new(AtomicU64::new(0));
    let threads: Vec<_> = (0..4).map(|_| {
        let rings = rings.clone();
        std::thread::spawn(move || for _ in 0..1000 { BELL.ring(); rings.fetch_add(1, SeqCst); })
    }).collect();
    let mut woken = 0;
    while rings.load(SeqCst) < 4000 {
        woken += usize::from(BELL.wait(Duration::from_millis(1)));
    }
    threads.into_iter().for_each(|t| t.join().unwrap());
    let _ = BELL.wait(Duration::ZERO);
    assert!(woken >= 1);
}

#[test]
fn bounded_drops_and_counts_instead_of_blocking_when_full() {
    let ring = Bounded::new(3);
    assert!((0..3).all(|i| ring.push(i)));
    // Full: the pushes return at once, dropped and counted.
    let started = std::time::Instant::now();
    assert!(!(3..10).any(|i| ring.push(i)));
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    assert_eq!(ring.dropped(), 7);
    assert_eq!(ring.drain().collect::<Vec<_>>(), [0, 1, 2], "what fitted, in order");
    assert!(ring.push(10));
    assert_eq!(ring.take_dropped(), 7);
    assert_eq!(ring.dropped(), 0);
    assert_eq!(ring.try_recv(), Some(10));
    assert!(ring.is_empty() && ring.capacity() == 3);
}
