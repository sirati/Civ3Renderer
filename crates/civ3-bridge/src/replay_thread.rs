//! Replay graphics on a worker. The game thread enqueues ordered navigator
//! operations in a fixed-size queue and never rasterizes the replay itself.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
#[cfg(test)]
use std::sync::mpsc;

use crate::{
    replay::{Operation, ReplayMap},
    world::Civ3World,
};

const CAPACITY: usize = 4096;

type Pixels = ((u32, u32), Arc<[u8]>);

#[derive(Clone, Default)]
pub struct Picture {
    pub revision: u64,
    pub pixels: Option<Pixels>,
}

#[derive(Default)]
struct Output {
    revision: AtomicU64,
    picture: civ3r_sync::Latest<Picture>,
}

impl Output {
    /// On the worker (the only writer). Readers only clone an Arc.
    fn publish(&self, map: Option<&ReplayMap>) {
        let pixels = map.map(|map| (map.size(), Arc::from(map.rgba())));
        let revision = self.revision.load(Ordering::Acquire) + 1;
        self.picture.publish(Picture { revision, pixels });
        self.revision.store(revision, Ordering::Release);
    }
}

enum Command {
    Init((u32, u32)),
    Apply(Operation),
    Clear,
    #[cfg(test)]
    Flush(mpsc::SyncSender<()>),
}

/// All replay image construction, updates and conversion belong to this
/// worker. The game's thread only sends (never waits for the worker, even
/// when playback outruns it); the worker applies every operation, in
/// batches of at most `capacity` between publications.
pub struct ReplayThread {
    send: tokio::sync::mpsc::UnboundedSender<Command>,
    output: Arc<Output>,
}

impl ReplayThread {
    pub fn start(
        read_world: impl Fn() -> Result<Civ3World, String> + Send + 'static,
    ) -> std::io::Result<Self> {
        Self::with_capacity(read_world, CAPACITY)
    }

    fn with_capacity(
        read_world: impl Fn() -> Result<Civ3World, String> + Send + 'static,
        capacity: usize,
    ) -> std::io::Result<Self> {
        let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
        let output = Arc::new(Output::default());
        let publish = output.clone();
        std::thread::Builder::new().name("civ3r-replay".into()).spawn(move || {
            let mut map = None;
            // (Bounded sleeps, not a parked receive: `crate::BOUNDED_WAIT`.)
            while let Some(first) = crate::recv_bounded(&mut receive, std::time::Duration::from_millis(10)) {
                let mut next = Some(first);
                // Publish at least once per bounded batch even when a long
                // fast-forward keeps feeding operations continuously.
                for index in 0..=capacity {
                    let Some(command) = next.take() else { break };
                    match command {
                        Command::Init(size) => {
                            let start = std::time::Instant::now();
                            map = match read_world() {
                                Ok(world) => {
                                    let read = start.elapsed();
                                    let map = ReplayMap::new(&world, size);
                                    log::debug!("UI mirror: replay map from world state, {}x{}; terrain read {} us, build {} us on worker", size.0, size.1, read.as_micros(), start.elapsed().saturating_sub(read).as_micros());
                                    Some(map)
                                }
                                Err(e) => {
                                    log::warn!("UI mirror: replay terrain state is unavailable: {e}");
                                    None
                                }
                            };
                        }
                        Command::Apply(operation) => {
                            if let Some(map) = map.as_mut() { map.apply(&operation); }
                        }
                        Command::Clear => map = None,
                        #[cfg(test)]
                        Command::Flush(done) => {
                            publish.publish(map.as_ref());
                            let _ = done.send(());
                        }
                    }
                    // Do not dequeue beyond this batch's last iteration.
                    // An operation removed here must be applied before publish.
                    if index < capacity { next = receive.try_recv().ok(); }
                }
                publish.publish(map.as_ref());
            }
        })?;
        Ok(Self { send, output })
    }

    fn command(&self, command: Command) -> bool {
        self.send.send(command).is_ok()
    }

    pub fn init(&self, size: (u32, u32)) -> bool {
        self.command(Command::Init(size))
    }
    pub fn apply(&self, operation: Operation) -> bool {
        self.command(Command::Apply(operation))
    }
    pub fn clear(&self) -> bool {
        self.command(Command::Clear)
    }
    pub fn revision(&self) -> u64 {
        self.output.revision.load(Ordering::Acquire)
    }
    pub fn picture(&self) -> Picture {
        self.output.picture.get().unwrap_or_default()
    }

    #[cfg(test)]
    fn flush(&self) {
        let (done, receive) = mpsc::sync_channel(0);
        assert!(self.command(Command::Flush(done)));
        receive.recv().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Civ3Tile;

    fn world() -> Civ3World {
        Civ3World {
            width: 8,
            height: 8,
            flags: 3,
            tiles: vec![
                Civ3Tile {
                    square_type: 2 << 12,
                    ..Default::default()
                };
                32
            ],
            ..Default::default()
        }
    }

    #[test]
    fn bounded_worker_preserves_long_playback_and_undo_order() {
        let renderer = ReplayThread::with_capacity(|| Ok(world()), 2).unwrap();
        assert!(renderer.init((64, 56)));
        let mut expected = ReplayMap::new(&world(), (64, 56));
        for n in 0..200 {
            for op in [
                Operation::Tint {
                    x: 2,
                    y: 2,
                    color: n,
                },
                Operation::AddCity { x: 2, y: 2 },
                Operation::EraseCity { x: 2, y: 2 },
                Operation::Restore { x: 2, y: 2 },
            ] {
                expected.apply(&op);
                assert!(renderer.apply(op));
            }
        }
        renderer.flush();
        let picture = renderer.picture();
        let (size, rgba) = picture.pixels.unwrap();
        assert_eq!(size, expected.size());
        assert_eq!(&*rgba, expected.rgba());
        assert!(picture.revision > 0);
        assert!(renderer.revision() >= picture.revision);
    }

    #[test]
    fn new_replay_and_clear_replace_pixels_and_revision_together() {
        let renderer = ReplayThread::with_capacity(|| Ok(world()), 2).unwrap();
        renderer.init((64, 56));
        renderer.apply(Operation::AddCity { x: 2, y: 2 });
        renderer.flush();
        let old = renderer.picture();
        renderer.clear();
        renderer.flush();
        let empty = renderer.picture();
        assert!(empty.revision > old.revision);
        assert!(empty.pixels.is_none());
        renderer.init((32, 28));
        renderer.flush();
        let new = renderer.picture();
        assert!(new.revision > empty.revision);
        assert_eq!(new.pixels.as_ref().unwrap().0, (32, 28));
        assert_eq!(
            &*new.pixels.as_ref().unwrap().1,
            ReplayMap::new(&world(), (32, 28)).rgba()
        );
        // A retained older publication keeps its own matching pixels.
        assert_eq!(old.pixels.as_ref().unwrap().0, (64, 56));
        assert_ne!(
            &*old.pixels.as_ref().unwrap().1,
            ReplayMap::new(&world(), (64, 56)).rgba()
        );
    }
}
