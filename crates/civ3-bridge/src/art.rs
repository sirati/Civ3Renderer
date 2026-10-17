//! Building the render art on a thread of its own, so that the publisher
//! goes on publishing with the art it has meanwhile: the first art is a
//! whole description, compiled to be grown (`render_desc::Growing`); what
//! the game shows later is appended to it as additions (only the new art's
//! sprites are compiled and uploaded, what was there keeps its ids), until
//! the art folders change (a scenario's art): then a whole description
//! again.

use std::sync::Arc;
use std::time::{Duration, Instant};

use render_desc::{Compiled, Growing, Growth};

/// What a job compiles.
pub enum Work {
    /// A whole description (its text).
    Whole(String),
    /// An addition to the art compiled so far (its text).
    Addition(String),
}

/// One job for the art thread.
pub struct ArtJob {
    /// The description's name, for messages.
    pub name: String,
    pub files: civ3_assets::GameFiles,
    pub work: Work,
    /// With the atlas pixels (for a renderer given them), else the tables.
    pub pixels: bool,
    /// The generation scenes refer to once it is adopted
    /// (`Presenter::prepare_assets`).
    pub generation: u64,
    /// See `ArtSchedule::start` (none: art built ahead, `crate::preload`).
    pub token: Option<u64>,
    /// What it is for (for the log).
    pub reason: String,
    /// When what it builds was first wanted.
    pub wanted: Instant,
}

/// What a job built.
pub enum Built {
    /// A whole description (with the pixels if asked for).
    Whole(Compiled),
    /// A growth of the art compiled before (with the new pixels if asked
    /// for).
    Growth(Arc<Growth>),
}

/// A job done.
pub struct ArtDone {
    pub generation: u64,
    pub token: Option<u64>,
    pub built: Result<Built, String>,
    pub took: Duration,
    pub reason: String,
    pub wanted: Instant,
    /// Atlas pages the art uses now (with pixels: colour, index), sprites
    /// it has.
    pub pages: (u32, u32),
    pub sprites: usize,
}

/// The art compiled so far, to be grown: its tables and how it grows.
type State = Option<(Compiled, Growing)>;

impl ArtJob {
    fn run(self, state: &mut State) -> ArtDone {
        let started = Instant::now();
        let built = match &self.work {
            Work::Whole(text) => crate::assets::compile_growing(text, self.files, self.pixels).map(|(c, growing)| {
                *state = Some((c.without_pixels(), growing));
                Built::Whole(c)
            }),
            Work::Addition(text) => match state.as_mut() {
                Some((compiled, growing)) => crate::assets::grow(growing, compiled, text, self.files).map(|g| {
                    compiled.apply(&g);
                    Built::Growth(Arc::new(g))
                }),
                None => Ok(Built::Growth(Arc::new(Growth::default()))),
            },
        }
        .map_err(|e| format!("{}: {e}", self.name));
        let (pages, sprites) = state.as_ref().map_or(((0, 0), 0), |(c, g)| (g.pages(), c.atlas.sprites.len()));
        ArtDone { generation: self.generation, token: self.token, built, took: started.elapsed(), reason: self.reason, wanted: self.wanted, pages, sprites }
    }
}

/// Runs art jobs on a thread of its own, one at a time, in order.
pub struct ArtThread {
    jobs: std::sync::mpsc::Sender<ArtJob>,
    pub done: std::sync::mpsc::Receiver<ArtDone>,
    /// Jobs sent and not done yet.
    in_flight: usize,
}

impl ArtThread {
    /// `state`: the art compiled so far (its tables) and how it grows.
    pub fn start(mut state: State) -> std::io::Result<ArtThread> {
        let (jobs, job_rx) = std::sync::mpsc::channel::<ArtJob>();
        let (done_tx, done) = std::sync::mpsc::channel();
        std::thread::Builder::new().name("civ3r-art".into()).spawn(move || {
            for job in job_rx {
                if done_tx.send(job.run(&mut state)).is_err() {
                    break;
                }
            }
        })?;
        Ok(ArtThread { jobs, done, in_flight: 0 })
    }

    /// Queues `job`; false if the thread is gone.
    pub fn send(&mut self, job: ArtJob) -> bool {
        let sent = self.jobs.send(job).is_ok();
        self.in_flight += sent as usize;
        sent
    }

    /// A job done, if one is.
    pub fn try_done(&mut self) -> Option<ArtDone> {
        let done = self.done.try_recv().ok()?;
        self.in_flight = self.in_flight.saturating_sub(1);
        Some(done)
    }

    /// Whether no job is queued or running.
    pub fn idle(&self) -> bool {
        self.in_flight == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "grid = { layout = \"orthogonal\", step = [8, 8] }\n\
        [images.a]\nsource = \"solid:1,2,3,255:8x8\"\n[sheets.a]\nimage = \"a\"\ncell = [8, 8]\ngroup = \"A\"\n\
        [[layers]]\nname = \"l\"\nsets = [\"A/*\", \"B@*\"]\n";

    fn job(work: Work, generation: u64) -> ArtJob {
        ArtJob {
            name: "test".into(),
            files: civ3_assets::GameFiles::with_roots(Vec::new()),
            work,
            pixels: true,
            generation,
            token: None,
            reason: "test".into(),
            wanted: Instant::now(),
        }
    }

    #[test]
    fn additions_grow_the_whole_description_in_order() {
        let mut t = ArtThread::start(None).unwrap();
        assert!(t.idle());
        t.send(job(Work::Whole(BASE.into()), 1));
        t.send(job(Work::Addition("[images.b]\nsource = \"solid:9,9,9,255:4x4\"\n[sheets.b]\nimage = \"b\"\ncell = [4, 4]\ngroup = \"B@x\"\n".into()), 2));
        assert!(!t.idle());
        let mut got = Vec::new();
        while got.len() < 2 {
            if let Some(d) = t.try_done() {
                got.push(d);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(t.idle());
        let Ok(Built::Whole(mut c)) = got.remove(0).built else { panic!("a whole description first") };
        let done = got.remove(0);
        assert_eq!((done.generation, done.sprites), (2, 2));
        let Ok(Built::Growth(g)) = done.built else { panic!("then a growth") };
        assert_eq!(g.atlas.blits.len(), 1);
        c.apply(&g);
        assert_eq!(c.value("l", "B@x/0"), Some(2));
    }
}
