use crate::paths::PathMapper;
use civ3_assets::{GameFiles, loader::Civ3Loader};
use scene::wire::Assets;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, mpsc};

const MAX_JOBS: usize = 8;
const MAX_BYTES: usize = viewer_transport::MAX_FRAME;

/// What is queued since the last whole description (its additions), so
/// that a whole description frees the backlog of the one it replaces.
#[derive(Default)]
struct Backlog {
    jobs: AtomicUsize,
    bytes: AtomicUsize,
}

struct Job {
    assets: Assets,
    backlog: Arc<Backlog>,
}

/// Art jobs from the session thread to the art worker. Pushing never
/// waits; the worker owns the queue itself ([`JobQueue`]).
pub struct ArtJobs {
    /// `None` wakes the worker to stop.
    tx: crossbeam_channel::Sender<Option<Job>>,
    rx: crossbeam_channel::Receiver<Option<Job>>,
    backlog: handoff::Latest<Backlog>,
    newest: AtomicU64,
    stopped: AtomicBool,
}

impl Default for ArtJobs {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let backlog = handoff::Latest::new();
        backlog.publish(Backlog::default());
        Self { tx, rx, backlog, newest: AtomicU64::new(0), stopped: AtomicBool::new(false) }
    }
}

fn size(a: &Assets) -> usize {
    a.description
        .len()
        .saturating_add(a.roots.iter().map(String::len).sum::<usize>())
}

impl ArtJobs {
    /// From one thread (the session's): the backlog check and the reset
    /// by a whole description are not atomic with other pushers.
    pub fn push(&self, a: Assets) -> Result<(), String> {
        if a.generation == 0 {
            return Err("art generation is zero".into());
        }
        if self.superseded(a.generation) {
            return Ok(());
        }
        if !a.append {
            // The worker drops what was queued before it.
            self.backlog.publish(Backlog::default());
            self.newest.store(a.generation, Ordering::Release);
        }
        let backlog = self.backlog.load().expect("published on creation");
        let bytes = size(&a);
        if backlog.jobs.load(Ordering::Acquire) >= MAX_JOBS
            || backlog.bytes.load(Ordering::Acquire).saturating_add(bytes) > MAX_BYTES
        {
            return Err("native art backlog exhausted".into());
        }
        backlog.jobs.fetch_add(1, Ordering::AcqRel);
        backlog.bytes.fetch_add(bytes, Ordering::AcqRel);
        let _ = self.tx.send(Some(Job { assets: a, backlog }));
        Ok(())
    }
    fn superseded(&self, generation: u64) -> bool {
        self.stopped.load(Ordering::Acquire) || self.newest.load(Ordering::Acquire) > generation
    }
    /// Publishes art of `generation` unless superseded. Only the worker
    /// publishes, in job order, so art superseded right after the check
    /// is replaced by the newer whole when that is done.
    fn with_current(&self, generation: u64, publish: impl FnOnce()) -> bool {
        if self.superseded(generation) {
            false
        } else {
            publish();
            true
        }
    }
    pub fn close(&self) {
        self.stopped.store(true, Ordering::Release);
        let _ = self.tx.send(None);
    }
    /// The worker's end. There is one: jobs taken by one are not seen by
    /// another.
    fn queue(&self) -> JobQueue {
        JobQueue { rx: self.rx.clone(), jobs: VecDeque::new(), stopped: false }
    }
}

/// The jobs received by the worker and not started, owned by its thread.
struct JobQueue {
    rx: crossbeam_channel::Receiver<Option<Job>>,
    jobs: VecDeque<Job>,
    stopped: bool,
}

impl JobQueue {
    fn take(&mut self, job: Option<Job>) {
        match job {
            None => self.stopped = true,
            Some(job) => {
                if !job.assets.append {
                    self.jobs.clear();
                }
                self.jobs.push_back(job);
            }
        }
    }
    /// The next job, waiting for one; none once closed.
    fn pop(&mut self) -> Option<Assets> {
        loop {
            while let Ok(job) = self.rx.try_recv() {
                self.take(job);
            }
            if self.stopped {
                return None;
            }
            if let Some(job) = self.jobs.pop_front() {
                job.backlog.jobs.fetch_sub(1, Ordering::AcqRel);
                job.backlog.bytes.fetch_sub(size(&job.assets), Ordering::AcqRel);
                return Some(job.assets);
            }
            // Blocks only this worker thread; senders never wait.
            match self.rx.recv() {
                Ok(job) => self.take(job),
                Err(_) => return None,
            }
        }
    }
}

/// Shared between the art worker and the session thread.
#[derive(Default)]
pub struct State {
    /// Set once by the worker with the first whole description; later
    /// art is published through it.
    pub assets: OnceLock<tile_viewer::Assets>,
    pub processed: AtomicU64,
}

pub fn worker(
    jobs: Arc<ArtJobs>,
    state: Arc<State>,
    mapper: PathMapper,
    max_page: u32,
    initial: mpsc::SyncSender<Result<tile_viewer::Assets, String>>,
    errors: mpsc::SyncSender<viewer_transport::Message>,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("viewer-art".into())
        .spawn(move || {
            let mut art: Option<(render_desc::Compiled, render_desc::Growing)> = None;
            let mut announced = false;
            let mut queue = jobs.queue();
            while let Some(a) = queue.pop() {
                let stop = || jobs.superseded(a.generation);
                // The render error a failed compile is (`render_errors`).
                let mut failed: Option<(render_errors::Kind, String)> = None;
                let result = (|| -> Result<(), String> {
                    let roots = a
                        .roots
                        .iter()
                        .map(|p| mapper.resolve(p))
                        .collect::<Result<Vec<_>, _>>()?;
                    let loader = render_desc::StopWhen::new(
                        Civ3Loader::new(GameFiles::with_roots(roots)),
                        &stop,
                    );
                    let started = std::time::Instant::now();
                    if a.append {
                        let (compiled, growing) = art
                            .as_mut()
                            .ok_or("an art addition arrived before a whole description")?;
                        let growth = growing
                            .grow(compiled, &a.description, &loader)
                            .map_err(|e| { failed = Some(e.render_error()); e.to_string() })?;
                        compiled.apply(&growth);
                        jobs.with_current(a.generation, || {
                            if let Some(assets) = state.assets.get() {
                                assets.grow_with_reserve(
                                    Arc::new(growth),
                                    a.generation,
                                    0,
                                    a.reserve,
                                );
                            }
                        });
                    } else {
                        let desc = render_desc::Description::parse(&a.description)
                            .map_err(|e| { failed = Some(e.render_error()); e.to_string() })?;
                        let (compiled, growing) = desc
                            .compile_growing(&loader, max_page, true)
                            .map_err(|e| { failed = Some(e.render_error()); e.to_string() })?;
                        let tables = compiled.without_pixels();
                        let accepted = jobs.with_current(a.generation, || {
                            // Only this thread sets it.
                            if let Some(assets) = state.assets.get() {
                                assets.set_with_reserve(
                                    Arc::new(compiled),
                                    a.generation,
                                    0,
                                    a.reserve,
                                );
                            } else {
                                let compiled = Arc::new(compiled);
                                let assets = tile_viewer::Assets::new(compiled.clone());
                                assets.set_with_reserve(compiled, a.generation, 0, a.reserve);
                                let _ = state.assets.set(assets);
                            }
                        });
                        if accepted {
                            art = Some((tables, growing));
                            if !announced {
                                announced = true;
                                let assets = state.assets.get().unwrap().clone();
                                if initial.send(Ok(assets)).is_err() {
                                    return Err("viewer stopped before startup".into());
                                }
                            }
                        }
                    }
                    log::info!(
                        "native art {} prepared in {} ms",
                        a.generation,
                        started.elapsed().as_millis()
                    );
                    Ok(())
                })();
                if let Err(error) = result {
                    if !stop() {
                        log::error!("native art {}: {error}", a.generation);
                        let (kind, class) = failed.take().unwrap_or((render_errors::Kind::ArtHandOver, "its folders or order".into()));
                        render_errors::report(kind, &format!("{}; {class}", if a.context.is_empty() { "art" } else { &a.context }), format_args!("art {}: {error}", a.generation));
                        let _ = errors.try_send(viewer_transport::Message::Error(format!(
                            "art {}: {error}",
                            a.generation
                        )));
                        if !announced {
                            let _ = initial.send(Err(error));
                        }
                        let _ = errors.try_send(viewer_transport::Message::Quit);
                        jobs.close();
                        break;
                    }
                }
                state.processed.fetch_max(a.generation, Ordering::Release);
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn job(generation: u64, append: bool) -> Assets {
        Assets {
            generation,
            append,
            ..Default::default()
        }
    }
    #[test]
    fn whole_replaces_old_jobs_but_preserves_subsequent_growth() {
        let queue = ArtJobs::default();
        let mut worker = queue.queue();
        queue.push(job(1, false)).unwrap();
        let running = worker.pop().unwrap();
        queue.push(job(2, true)).unwrap();
        queue.push(job(3, false)).unwrap();
        queue.push(job(4, true)).unwrap();
        assert!(queue.superseded(running.generation));
        assert!(!queue.with_current(1, || panic!("published retired art")));
        assert_eq!(worker.pop().unwrap().generation, 3);
        assert_eq!(worker.pop().unwrap().generation, 4);
    }
    #[test]
    fn growth_queue_has_a_fixed_bound_and_close_wakes_worker() {
        let queue = Arc::new(ArtJobs::default());
        let mut worker = queue.queue();
        queue.push(job(1, false)).unwrap();
        for generation in 2..=8 {
            queue.push(job(generation, true)).unwrap();
        }
        assert!(queue.push(job(9, true)).is_err());
        queue.push(job(10, false)).unwrap();
        assert_eq!(worker.pop().unwrap().generation, 10);
        let q = queue.clone();
        let worker = std::thread::spawn(move || assert!(q.queue().pop().is_none()));
        queue.close();
        worker.join().unwrap();
    }
    #[test]
    fn started_jobs_leave_the_backlog_and_a_waiting_worker_gets_the_next() {
        let queue = Arc::new(ArtJobs::default());
        let (taken_tx, taken) = mpsc::channel();
        let q = queue.clone();
        let worker = std::thread::spawn(move || {
            let mut jobs = q.queue();
            while let Some(a) = jobs.pop() {
                taken_tx.send(a.generation).unwrap();
            }
        });
        queue.push(job(1, false)).unwrap();
        assert_eq!(taken.recv_timeout(std::time::Duration::from_secs(10)), Ok(1));
        for round in 0..3u64 {
            for i in 0..MAX_JOBS as u64 {
                queue.push(job(2 + round * 100 + i, true)).unwrap();
            }
            for i in 0..MAX_JOBS as u64 {
                assert_eq!(taken.recv_timeout(std::time::Duration::from_secs(10)), Ok(2 + round * 100 + i));
            }
            // Taken jobs no longer count against the bound; wait until
            // the worker has counted the last one out.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while queue.backlog.load().unwrap().jobs.load(Ordering::Acquire) != 0 {
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
        }
        queue.close();
        worker.join().unwrap();
    }
}
