//! The game quitting: the viewer stops taking the game's state and closes
//! before the game frees anything, and the process ends without waiting
//! for anything slow.
//!
//! When the game quits (its Quit confirmed, the title menu's Quit, the end
//! of a game, `ExitProcess`), it first frees its world: cities, then units
//! and players, then the map, then its program objects, and only then does
//! the C runtime call `ExitProcess`, whose DLL detach can wait on threads
//! that are already dead (`sound.dll`). A viewer that went on reading and
//! drawing the game meanwhile showed the world come apart: city labels
//! went, units turned barbarian, the map went, and only seconds later did
//! the window close.
//!
//! Now, at the first sign of quitting (the hooks of `civ3_sys::quit`, the
//! game's `ExitProcess`, our DLL's detach), on the game's thread and before
//! it frees anything, [`quit`]:
//! 1. marks the session ending ([`Session::begin`]): the publisher takes no
//!    new round of the game's state ([`Session::round`] refuses) and
//!    publishes nothing more; it tells the viewer to close
//!    (`scene::Update::Close`), which keeps its last picture until its
//!    window is gone, hands F1-F12 back to the system (macOS) and says it
//!    is gone;
//! 2. waits, at most [`CLOSE_WAIT`], for a round still reading the game to
//!    end and for the viewer to say it is gone; the game's thread is
//!    parked in our hook meanwhile, so the world stays whole;
//! 3. runs the two steps of the game's teardown that persist something
//!    (`civ3_sys::quit`: the scroll speed into `conquests.ini`, the
//!    scenario loader's temporary files deleted), with nothing of ours
//!    reading any more;
//! 4. ends the process (`inproc::process::terminate`): no world teardown
//!    on screen, no DLL detach, no GPU teardown, no join of any thread of
//!    ours. With the compatibility fallback the game's own exit runs on
//!    instead, with nothing of ours reading it.
//!
//! Nothing here takes a lock (`docs/concurrency.md`): atomics only.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
use std::time::{Duration, Instant};

/// The longest the game's thread waits, once it quits, for the viewer to
/// close and for the publisher to leave the game's memory alone; then the
/// process ends whatever they do.
pub const CLOSE_WAIT: Duration = Duration::from_millis(500);

/// This process's session.
pub static SESSION: Session = Session::new();

/// Whether the game's session is ending, who still reads the game, and
/// whether the viewer is gone.
#[derive(Debug, Default)]
pub struct Session {
    ending: AtomicBool,
    /// A publisher round that reads the game's state runs ([`Round`]).
    reading: AtomicBool,
    viewer_closed: AtomicBool,
    /// When the end began (ms since `EPOCH`, 0: not yet), for the log.
    began_ms: AtomicU64,
}

/// One publisher round that may read the game's state: while it is held,
/// [`Session::wait`] waits (bounded) before the game goes on to free it.
#[must_use = "the round reads the game only while this is held"]
pub struct Round<'a>(&'a Session);

impl Drop for Round<'_> {
    fn drop(&mut self) {
        self.0.reading.store(false, SeqCst);
    }
}

/// What [`Session::wait`] saw at its end.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Waited {
    /// No publisher round reads the game any more.
    pub publisher_stopped: bool,
    /// The viewer said it is gone.
    pub viewer_closed: bool,
    pub took: Duration,
}

impl Session {
    pub const fn new() -> Session {
        Session { ending: AtomicBool::new(false), reading: AtomicBool::new(false), viewer_closed: AtomicBool::new(false), began_ms: AtomicU64::new(0) }
    }

    /// The publisher, before it reads the game's state: `None` once the
    /// session ends (it then reads nothing more of the game).
    pub fn round(&self) -> Option<Round<'_>> {
        // (Sequentially consistent, as `begin`: either this sees the end,
        // or `wait` sees this round.)
        self.reading.store(true, SeqCst);
        if self.ending.load(SeqCst) {
            self.reading.store(false, SeqCst);
            return None;
        }
        Some(Round(self))
    }

    /// Whether the session ends: nothing of the game's state may be
    /// published (a round that began before may have read some of it).
    pub fn ending(&self) -> bool {
        self.ending.load(SeqCst)
    }

    /// Marks the session ending; true the first time.
    pub fn begin(&self) -> bool {
        if self.ending.swap(true, SeqCst) {
            return false;
        }
        self.began_ms.store(epoch_ms().max(1), SeqCst);
        true
    }

    /// How long ago the end began, if it did.
    pub fn since_begin(&self) -> Option<Duration> {
        let began = self.began_ms.load(SeqCst);
        (began != 0).then(|| Duration::from_millis(epoch_ms().saturating_sub(began)))
    }

    /// The viewer said it is gone (its `Input::Quit`).
    pub fn viewer_closed(&self) {
        self.viewer_closed.store(true, SeqCst);
    }

    /// Waits until no publisher round reads the game and the viewer is
    /// gone, at most `bound` (after [`Session::begin`]).
    pub fn wait(&self, bound: Duration) -> Waited {
        let started = Instant::now();
        loop {
            let publisher_stopped = !self.reading.load(SeqCst);
            let viewer_closed = self.viewer_closed.load(SeqCst);
            let took = started.elapsed();
            if (publisher_stopped && viewer_closed) || took >= bound {
                return Waited { publisher_stopped, viewer_closed, took };
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// Milliseconds since this module's first use.
fn epoch_ms() -> u64 {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// The game quits (`why`, to end with exit `code`): see the module docs.
/// On the game's thread, before it frees anything; `before_teardown`: its
/// teardown has not run (from the main loop's sites, not `ExitProcess`),
/// so the steps of it that persist something run here. Returns only with
/// the compatibility fallback (the game's own exit then runs on) or when
/// the end began before.
#[cfg(windows)]
pub fn quit(why: &str, code: u32, before_teardown: bool) {
    if !SESSION.begin() {
        return;
    }
    log::info!("session: {why}: the game quits; nothing more of it is read or shown, the viewer closes");
    crate::presenter::WAKE.ring();
    let waited = SESSION.wait(CLOSE_WAIT);
    log::info!(
        "session: after {} ms the publisher {} and the viewer {}",
        waited.took.as_millis(),
        if waited.publisher_stopped { "reads no more of the game" } else { "still reads (not waited for)" },
        if waited.viewer_closed { "is gone" } else { "has not said it is gone (not waited for)" },
    );
    if !crate::ui_trace::compatibility_fallback() {
        if let Some(steps) = PERSIST.get().filter(|_| before_teardown) {
            steps.save_scroll_speed.call(steps.main_screen);
            steps.delete_scenario_temp.call();
            log::info!("session: the game's teardown steps that persist ran: scroll speed saved, scenario temporary files deleted");
        }
        crate::ui_trace::end_process(code);
    }
}

/// The steps of the game's teardown that persist something
/// (`civ3_sys::quit`).
#[cfg(windows)]
struct Persist {
    main_screen: u32,
    save_scroll_speed: civ3_sys::quit::save_scroll_speed::Function,
    delete_scenario_temp: civ3_sys::quit::delete_scenario_temp::Function,
}

#[cfg(windows)]
static PERSIST: std::sync::OnceLock<Persist> = std::sync::OnceLock::new();

/// Watches for the game quitting (see the module docs): its main loop's
/// sites (`civ3_sys::quit`) where it has them, and its `ExitProcess`.
#[cfg(windows)]
pub fn install(game: civ3_sys::Civ3) {
    use civ3_sys::quit as q;
    WATCHED.store(true, SeqCst);
    static RESTART: std::sync::OnceLock<inproc::Ptr32<u8>> = std::sync::OnceLock::new();
    fn on_game_left(call: &q::game_loop_left::Call) {
        // (Restarting: to the title menu, a new or a loaded game.)
        if RESTART.get().is_some_and(|r| r.read().is_ok_and(|b| b == 0)) {
            quit("the running game is left to quit", 0, true);
        }
        call.original()
    }
    fn on_main_left(call: &q::main_loop_left::Call) {
        quit("the main loop ends", 0, true);
        call.original()
    }
    fn on_exit_process(code: u32) {
        // (After the game's own teardown: nothing of it to keep.)
        quit("ExitProcess", code, false);
    }
    match q::sites(game.build) {
        Some(sites) => 'sites: {
            // Without the teardown's persisting steps the game must run its
            // teardown itself: then only ExitProcess ends the session.
            let steps = q::save_scroll_speed::Function::at(sites.save_scroll_speed.0, sites.save_scroll_speed.1).and_then(|scroll| {
                Ok(Persist {
                    main_screen: game.build.addresses().main_screen_form,
                    save_scroll_speed: scroll,
                    delete_scenario_temp: q::delete_scenario_temp::Function::at(sites.delete_scenario_temp.0, sites.delete_scenario_temp.1)?,
                })
            });
            match steps {
                Ok(steps) => { let _ = PERSIST.set(steps); }
                Err(e) => {
                    log::warn!("session: the teardown's persisting steps are not where expected ({e}): the main loop's quit is not watched (ExitProcess is)");
                    break 'sites;
                }
            }
            let _ = RESTART.set(sites.restart);
            let game_left = q::game_loop_left::install(sites.game_loop_left.0, &sites.game_loop_left.1, on_game_left);
            let main_left = q::main_loop_left::install(sites.main_loop_left.0, &sites.main_loop_left.1, on_main_left);
            match (game_left, main_left) {
                (Ok(a), Ok(b)) => {
                    // (For the life of the process.)
                    std::mem::forget((a, b));
                    log::info!("session: the main loop's quit is watched");
                }
                (a, b) => log::warn!("session: the main loop's quit is not watched (ExitProcess is): {:?} {:?}", a.err(), b.err()),
            }
        }
        None => log::info!("session: no main loop sites for {:?}: only ExitProcess ends the session", game.build),
    }
    match inproc::process::hook_exit_process(inproc::pe::main_module(), on_exit_process) {
        Ok(Some(hook)) => std::mem::forget(hook),
        Ok(None) => log::warn!("session: the game does not import ExitProcess"),
        Err(e) => log::warn!("session: hooking ExitProcess: {e}"),
    }
}

/// Our DLL's detach while `ExitProcess` runs (someone called it past our
/// hook): every other thread is dead, the log's writer too. Unless the
/// compatibility fallback asks for the game's own exit, the process ends
/// here, before the detach of a DLL that waits for its dead threads.
/// (Atomics only: a lock or a lazy value another thread held when it was
/// killed would never be let go.)
#[cfg(windows)]
pub fn detached() {
    SESSION.ending.store(true, SeqCst);
    if WATCHED.load(SeqCst) && !crate::ui_trace::compatibility_fallback() {
        inproc::process::terminate(inproc::process::exit_code().unwrap_or(0));
    }
}

/// [`install`] ran: the renderer runs in this process.
#[cfg(windows)]
static WATCHED: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    /// A publisher as `live::publish` runs: rounds that read "the game's
    /// world" (1 while whole, 0 once freed) and publish what they read
    /// unless the session ends, then it tells the viewer to close and
    /// stops. Returns what it published.
    fn publisher(session: &'static Session, world: Arc<AtomicUsize>, freed: Arc<AtomicBool>, viewer: impl FnOnce()) -> Vec<usize> {
        let mut published = Vec::new();
        loop {
            let Some(_round) = session.round() else { break };
            assert!(!freed.load(SeqCst), "a round read the game after it freed its world");
            let read = world.load(SeqCst);
            std::thread::sleep(Duration::from_millis(3));
            // The publish point: nothing goes out once the session ends.
            if !session.ending() {
                published.push(read);
            }
        }
        viewer();
        published
    }

    #[test]
    fn after_the_quit_no_round_reads_or_publishes_the_game() {
        static S: Session = Session::new();
        let world = Arc::new(AtomicUsize::new(1));
        let freed = Arc::new(AtomicBool::new(false));
        let p = {
            let (world, freed) = (world.clone(), freed.clone());
            std::thread::spawn(move || publisher(&S, world, freed, || S.viewer_closed()))
        };
        std::thread::sleep(Duration::from_millis(20));
        // The game's thread quits, waits, then frees its world piece by
        // piece (cities, units, the map).
        assert!(S.begin());
        assert!(!S.begin(), "once");
        let waited = S.wait(CLOSE_WAIT);
        assert!(waited.publisher_stopped && waited.viewer_closed, "{waited:?}");
        freed.store(true, SeqCst);
        world.store(0, SeqCst);
        let published = p.join().unwrap();
        assert!(!published.is_empty());
        assert!(published.iter().all(|&w| w == 1), "a frame of a partly freed world was published: {published:?}");
        assert!(S.round().is_none(), "no round after the end");
    }

    #[test]
    fn the_quit_waits_no_longer_than_its_bound_for_a_stuck_publisher_or_viewer() {
        static S: Session = Session::new();
        // A round that never ends (stuck in a read), a viewer that never
        // answers.
        let round = S.round().unwrap();
        assert!(S.begin());
        let started = Instant::now();
        let waited = S.wait(Duration::from_millis(50));
        assert!(!waited.publisher_stopped && !waited.viewer_closed);
        assert!(started.elapsed() < Duration::from_millis(250), "{:?}", started.elapsed());
        drop(round);
        assert!(S.wait(Duration::ZERO).publisher_stopped);
    }

    #[test]
    fn a_closed_viewer_and_an_idle_publisher_end_the_wait_at_once() {
        static S: Session = Session::new();
        assert!(S.since_begin().is_none());
        assert!(S.begin());
        S.viewer_closed();
        let waited = S.wait(CLOSE_WAIT);
        assert!(waited.publisher_stopped && waited.viewer_closed);
        assert!(waited.took < Duration::from_millis(50), "{waited:?}");
        assert!(S.since_begin().is_some());
    }

    #[test]
    fn a_round_begun_before_the_quit_is_waited_for() {
        static S: Session = Session::new();
        let round = S.round().unwrap();
        let ender = std::thread::spawn(|| {
            S.begin();
            S.viewer_closed();
            S.wait(CLOSE_WAIT)
        });
        std::thread::sleep(Duration::from_millis(30));
        assert!(!ender.is_finished(), "the game must not go on while a round reads it");
        drop(round);
        let waited = ender.join().unwrap();
        assert!(waited.publisher_stopped && waited.took >= Duration::from_millis(25), "{waited:?}");
    }
}
