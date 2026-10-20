//! Process-lifetime UI capture, installed before the game's first drawing.
//!
//! No locks: preparation and restoration are small atomic state machines.
//! The only waits are at startup, while one thread (the game's load
//! callback or the late-injection worker) prepares capture and another
//! arrives: the latter yields until the outcome is published, exactly once
//! per process. Afterwards every check is a single atomic load.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::ui::UiModel;

/// Only the real executable load callback proves readiness before the
/// first factory call. A worker needs the initialized graphics object.
/// Finding a mapped library under DllMain does not prove CRT/C3X readiness.
#[derive(Clone, Copy)]
pub(crate) enum CapturePoint {
    LoadCallback,
    DllMain,
    Worker { graphics_initialized: bool },
}

impl CapturePoint {
    pub(crate) fn allowed(self) -> bool {
        matches!(self, Self::LoadCallback | Self::Worker { graphics_initialized: true })
    }
}

/// Puts `state` back to `to` if the owner unwinds, so that waiters go on.
struct Release<'a> { state: &'a AtomicU8, from: u8, to: u8 }

impl Drop for Release<'_> {
    fn drop(&mut self) {
        let _ = self.state.compare_exchange(self.from, self.to, Ordering::AcqRel, Ordering::Acquire);
    }
}

const IDLE: u8 = 0;
const BUSY: u8 = 1;
const FAILED: u8 = 2;

/// Coordinate suppression with successful capture and viewer startup.
/// Restoration is permanent and serializes against concurrent activation.
pub(crate) struct OriginalDrawing {
    /// IDLE, FAILED (restored for good), or BUSY while one of the three
    /// operations runs its callback.
    state: AtomicU8,
    skipping: AtomicBool,
}

impl OriginalDrawing {
    pub(crate) const fn new() -> Self {
        Self { state: AtomicU8::new(IDLE), skipping: AtomicBool::new(false) }
    }

    /// Runs `f` (given whether restoration happened) with no other of
    /// these operations running; `f` says whether restoration is now done.
    fn serialized<R>(&self, f: impl FnOnce(bool) -> (R, bool)) -> R {
        let failed = loop {
            match self.state.compare_exchange(IDLE, BUSY, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break false,
                Err(FAILED) if self.state.compare_exchange(FAILED, BUSY, Ordering::AcqRel, Ordering::Acquire).is_ok() => break true,
                Err(_) => std::thread::yield_now(),
            }
        };
        let release = Release { state: &self.state, from: BUSY, to: if failed { FAILED } else { IDLE } };
        let (result, now_failed) = f(failed);
        std::mem::forget(release);
        self.state.store(if failed || now_failed { FAILED } else { IDLE }, Ordering::Release);
        result
    }

    pub(crate) fn failed(&self) -> bool {
        loop {
            match self.state.load(Ordering::Acquire) {
                BUSY => std::thread::yield_now(),
                state => return state == FAILED,
            }
        }
    }

    pub(crate) fn skipping(&self) -> bool {
        self.skipping.load(Ordering::Relaxed)
    }

    pub(crate) fn activate(&self, capture_ready: bool, game_draws: bool, configure: impl FnOnce(bool)) {
        self.serialized(|failed| {
            if !failed {
                let skip = capture_ready && !game_draws;
                configure(skip);
                self.skipping.store(skip, Ordering::Relaxed);
            }
            ((), false)
        });
    }

    /// Apply map-only policy without overwriting concurrent full capture
    /// activation or permanent restoration. Used by early DLL setup too.
    pub(crate) fn map_policy(&self, explicit: bool, configure: impl FnOnce(bool)) {
        self.serialized(|failed| {
            configure(self.skipping() || (!failed && explicit));
            ((), false)
        });
    }

    pub(crate) fn restore(&self, allow_native: bool, restore: impl FnOnce()) -> bool {
        self.serialized(|_| {
            if !allow_native { return (false, true) }
            self.skipping.store(false, Ordering::Relaxed);
            restore();
            (true, true)
        })
    }
}

const WAITING: u8 = 0;
const PREPARING: u8 = 1;
const READY: u8 = 2;
const STARTUP_FAILED: u8 = 3;

/// Capture ownership and the result of the normal game's first load.
/// A failed normal callback may return into unrecorded game execution, so
/// no later callback or worker may turn that failure into complete capture.
///
/// The model it makes goes to the thread that draws ([`UiCapture::take_model`]):
/// only that thread changes it; others read its published snapshots.
pub(crate) struct UiCapture<T> {
    /// WAITING, PREPARING (one caller owns preparation), READY or
    /// STARTUP_FAILED (with `failure`).
    state: AtomicU8,
    failure: OnceLock<String>,
    ready: AtomicBool,
    /// Whether the model was made (it is kept across a failed late retry).
    made: AtomicBool,
    model: civ3r_sync::Mailbox<UiModel>,
    /// Installed hooks stay for the life of the process.
    _hooks: std::marker::PhantomData<fn() -> T>,
}

impl<T> UiCapture<T> {
    pub(crate) fn new() -> Self {
        Self {
            state: AtomicU8::new(WAITING),
            failure: OnceLock::new(),
            ready: AtomicBool::new(false),
            made: AtomicBool::new(false),
            model: civ3r_sync::Mailbox::new(),
            _hooks: std::marker::PhantomData,
        }
    }

    pub(crate) fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Whether a model was made (it may still wait for its thread).
    #[cfg(test)]
    pub(crate) fn made(&self) -> bool {
        self.made.load(Ordering::Acquire)
    }

    /// The model made by preparation, for the thread that applies drawing
    /// to it (once; `None` before and after).
    pub(crate) fn take_model(&self) -> Option<UiModel> {
        self.model.try_recv()
    }

    /// The settled state: waits while another caller prepares (startup).
    fn settled(&self) -> u8 {
        loop {
            match self.state.load(Ordering::Acquire) {
                PREPARING => std::thread::yield_now(),
                state => return state,
            }
        }
    }

    fn failure(&self) -> String {
        self.failure.get().cloned().unwrap_or_default()
    }

    /// Workers check this before any disabled-viewer shortcut. Restoring
    /// original drawing must never hide an earlier normal-startup gap.
    pub(crate) fn status(&self) -> Result<bool, String> {
        match self.settled() {
            READY => Ok(true),
            STARTUP_FAILED => Err(self.failure()),
            _ => Ok(false),
        }
    }

    /// The actual worker entry decision. Check failure before intentional
    /// disablement, after any preparation in progress published its outcome.
    pub(crate) fn worker_needs_capture(&self, enabled: impl FnOnce() -> bool) -> Result<bool, String> {
        match self.settled() {
            STARTUP_FAILED => Err(self.failure()),
            READY => Ok(false),
            _ => Ok(enabled()),
        }
    }

    /// Serialize the whole preparation path, including attachment and
    /// required text capture before the model/hooks. Every caller waits
    /// until success or permanent normal-startup failure is published.
    pub(crate) fn prepare<S>(
        &self,
        point: CapturePoint,
        setup: impl FnOnce() -> Result<S, String>,
        model: impl FnOnce(&S) -> UiModel,
        hooks: impl FnOnce(&S) -> Result<T, String>,
    ) -> Result<bool, String> {
        if !point.allowed() { return Ok(false) }
        loop {
            match self.state.compare_exchange(WAITING, PREPARING, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break,
                Err(READY) => return Ok(false),
                Err(STARTUP_FAILED) => return Err(self.failure()),
                Err(_) => std::thread::yield_now(),
            }
        }
        let release = Release { state: &self.state, from: PREPARING, to: WAITING };
        let result = (|| {
            let setup = setup()?;
            if !self.made.swap(true, Ordering::AcqRel) {
                let mut model = model(&setup);
                // The title uses the map canvas before the publisher
                // can identify a running game. Preserve startup copies.
                model.set_skipping(false);
                // Sent before hooks can emit events.
                self.model.send(model);
            }
            hooks(&setup)
        })();
        std::mem::forget(release);
        match result {
            Ok(hooks) => {
                std::mem::forget(hooks);
                self.ready.store(true, Ordering::Release);
                self.state.store(READY, Ordering::Release);
                Ok(true)
            }
            Err(error) => {
                if matches!(point, CapturePoint::LoadCallback) {
                    let error = format!("normal startup UI capture failed: {error}; an uncaptured execution gap prevents later recovery");
                    let _ = self.failure.set(error.clone());
                    self.state.store(STARTUP_FAILED, Ordering::Release);
                    Err(error)
                } else {
                    // A separately identified late injection has no normal
                    // startup callback to preserve. Do not publish readiness.
                    self.state.store(WAITING, Ordering::Release);
                    Err(error)
                }
            }
        }
    }

    #[cfg(test)]
    fn install(
        &self,
        point: CapturePoint,
        model: impl FnOnce() -> UiModel,
        hooks: impl FnOnce() -> Result<T, String>,
    ) -> Result<bool, String> {
        self.prepare(point, || Ok(()), |_| model(), |_| hooks())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{Event, Frame, ScreenDraw, Source};
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    fn title_model() -> UiModel {
        UiModel::new(vec![], vec![301])
            .with_underlays(vec![300], vec!["fogofwar.pcx".into()])
            .with_generated(vec![700])
            .with_screen(900)
    }

    thread_local! {
        /// The model as the drawing thread owns it: this test's thread, for
        /// the capture at the address kept with it.
        static MODEL: std::cell::RefCell<Option<(usize, UiModel)>> = const { std::cell::RefCell::new(None) };
    }

    fn with_model<T, R>(capture: &UiCapture<T>, f: impl FnOnce(&mut Option<UiModel>) -> R) -> R {
        let id = capture as *const UiCapture<T> as usize;
        MODEL.with_borrow_mut(|slot| {
            if slot.as_ref().is_none_or(|(owner, _)| *owner != id) {
                *slot = capture.take_model().map(|m| (id, m));
            }
            let mut model = slot.take().map(|(_, m)| m);
            let result = f(&mut model);
            *slot = model.map(|m| (id, m));
            result
        })
    }

    fn emit<T>(capture: &UiCapture<T>, event: Event) {
        with_model(capture, |m| if let Some(model) = m.as_mut() { model.apply(event) })
    }

    fn startup_title<T>(capture: &UiCapture<T>) {
        for event in [
            Event::Loaded { image: 10, path: "Art/Title.pcx".into(), size: (1920, 1080) },
            Event::Sliced { sprite: 20, image: 10, rect: [0, 0, 1920, 1080] },
            Event::Blit { sprite: 20, canvas: 300, x: 0, y: 0, clip: None, palette: None },
            Event::Copy { from: 300, src: [0, 0, 1920, 1080], canvas: 400, x: 0, y: 0, keyed: false },
            Event::PaintBegin,
            Event::Composite { canvas: 400, vtable: 1, origin: (0, 0), size: (1920, 1080), clip: [0, 0, 1920, 1080], keyed: false },
            Event::CompositeEnd,
            Event::PaintEnd,
            Event::PresentBegin,
            Event::Present { rect: None },
        ] { emit(capture, event) }
    }

    fn shown<T>(capture: &UiCapture<T>) -> Frame {
        with_model(capture, |m| {
            let model = m.as_ref().unwrap();
            let frame = model.shown_frame();
            assert_eq!(model.snapshot().shown_frame(), frame, "another thread composes the same screen from a snapshot");
            frame
        })
    }

    fn assert_title(frame: &Frame) {
        assert!(matches!(&frame.draws[..], [ScreenDraw::Image { path, rect: [0, 0, 1920, 1080], x: 0, y: 0, .. }] if path == "Art/Title.pcx"), "missing passive startup title: {:?}", frame.draws);
    }

    #[test]
    fn title_capture_precedes_viewer_and_preserves_underlay_copies() {
        let capture = UiCapture::new();
        capture.install(CapturePoint::LoadCallback, title_model, || Ok::<_, String>(())).unwrap();
        // All startup drawing happens before the viewer/worker gets a turn.
        startup_title(&capture);
        assert_title(&shown(&capture));
        assert_eq!(capture.install(CapturePoint::Worker { graphics_initialized: true }, || panic!("model reset"), || panic!("hooks installed twice")), Ok::<_, String>(false));
        assert_title(&shown(&capture));
    }

    #[test]
    fn later_startup_keeps_files_sprites_generated_surfaces_and_presented_snapshot() {
        let capture = UiCapture::new();
        capture.install(CapturePoint::LoadCallback, title_model, || Ok::<_, String>(())).unwrap();
        startup_title(&capture);
        with_model(&capture, |m| m.as_mut().unwrap().add_generated(701));
        // Retained canvas content changes after presenting. The shown Arc
        // snapshot must continue showing the title until another present.
        emit(&capture, Event::Fill { canvas: 400, rect: [0, 0, 1920, 1080], rgb: [1, 2, 3], alpha: 255, clip: None });
        let before = shown(&capture);
        let stats = with_model(&capture, |m| m.as_ref().unwrap().stats());
        capture.install(CapturePoint::Worker { graphics_initialized: true }, title_model, || Ok::<_, String>(())).unwrap();
        assert_eq!(shown(&capture), before);
        assert_title(&before);
        with_model(&capture, |state| {
        let model = state.as_mut().unwrap();
        assert_eq!(model.stats(), stats);
        assert!(matches!(model.sprite_source(20), Some(Source::File { path, .. }) if path == "Art/Title.pcx"));
        model.apply(Event::Sliced { sprite: 21, image: 10, rect: [1, 2, 3, 4] });
        assert!(matches!(model.sprite_source(21), Some(Source::File { path, rect: [1, 2, 3, 4], .. }) if path == "Art/Title.pcx"));
        model.apply(Event::Size { image: 701, size: (10, 10) });
        model.apply(Event::Copy { from: 701, src: [0, 0, 10, 10], canvas: 401, x: 0, y: 0, keyed: false });
        model.apply(Event::PaintBegin);
        model.apply(Event::Composite { canvas: 401, vtable: 1, origin: (0, 0), size: (10, 10), clip: [0, 0, 10, 10], keyed: false });
        model.apply(Event::PaintEnd);
        assert!(matches!(&model.frame().draws[..], [ScreenDraw::Surface { image: 701, .. }]));
        });
    }

    #[test]
    fn dllmain_and_a_mapped_library_cannot_initialize_full_capture() {
        let capture = UiCapture::<()>::new();
        for point in [CapturePoint::DllMain, CapturePoint::Worker { graphics_initialized: false }] {
            assert_eq!(capture.install(point, || panic!("capture before CRT/C3X"), || panic!("hooks under loader lock")), Ok::<_, String>(false));
            assert!(!capture.ready());
            assert!(!capture.made());
        }
        assert_eq!(capture.install(CapturePoint::Worker { graphics_initialized: true }, title_model, || Ok::<_, String>(())), Ok(true));
    }

    #[test]
    fn true_late_injection_retry_keeps_history_without_claiming_normal_startup() {
        let capture = UiCapture::new();
        let result = capture.install(CapturePoint::Worker { graphics_initialized: true }, title_model, || {
            startup_title(&capture);
            Err::<(), _>("hook installation failed".into())
        });
        assert_eq!(result, Err("hook installation failed".into()));
        assert!(!capture.ready());
        assert_title(&shown(&capture));
        assert_eq!(capture.install(CapturePoint::Worker { graphics_initialized: true }, || panic!("reset on retry"), || Ok::<_, String>(())), Ok(true));
        assert!(capture.ready());
        assert_title(&shown(&capture));
    }

    #[test]
    fn normal_hook_failure_cannot_recover_after_unrecorded_game_execution() {
        struct HookGuard(Arc<AtomicBool>);
        impl Drop for HookGuard {
            fn drop(&mut self) { self.0.store(false, Ordering::Relaxed); }
        }
        let capture = UiCapture::<HookGuard>::new();
        let active = Arc::new(AtomicBool::new(false));
        let first = capture.prepare(CapturePoint::LoadCallback, || Ok(()), |_| title_model(), |_| {
            let _hooks = HookGuard(active.clone());
            active.store(true, Ordering::Relaxed);
            emit(&capture, Event::Loaded { image: 11, path: "Art/Partial.pcx".into(), size: (1, 1) });
            Err("final required hook failed".into())
        });
        let failure = first.unwrap_err();
        assert!(failure.contains("uncaptured execution gap"));
        assert!(!active.load(Ordering::Relaxed), "failed setup must roll its hooks back");
        // The callback has returned. The game initializes its baked title
        // while the rolled-back hooks are absent. No repaint occurs later.
        if active.load(Ordering::Relaxed) { startup_title(&capture); }
        let before = with_model(&capture, |m| m.as_ref().unwrap().stats());
        assert!(shown(&capture).draws.is_empty());
        for point in [CapturePoint::Worker { graphics_initialized: true }, CapturePoint::LoadCallback] {
            let retry = capture.prepare(point, || Ok(()), |_| panic!("replaced partial model"), |_| {
                active.store(true, Ordering::Relaxed);
                Ok(HookGuard(active.clone()))
            });
            assert_eq!(retry, Err(failure.clone()));
            assert_eq!(capture.status(), Err(failure.clone()));
            assert!(!capture.ready() && !active.load(Ordering::Relaxed));
            assert_eq!(with_model(&capture, |m| m.as_ref().unwrap().stats()), before);
            assert!(shown(&capture).draws.is_empty(), "uncaptured title was never reconstructed");
        }
        let drawing = OriginalDrawing::new();
        drawing.activate(capture.ready(), false, |skip| assert!(!skip));
        drawing.restore(true, || {});
        // Exercise the same worker entry decision as prepare_worker,
        // including the drawing-restored enabled=false case.
        assert!(drawing.failed());
        assert_eq!(capture.worker_needs_capture(|| !drawing.failed()), Err(failure));
        assert!(!drawing.skipping());
    }

    #[test]
    fn normal_attach_or_text_preparation_failure_is_permanent_before_model_creation() {
        for error in ["game attachment failed", "required TextOutA hook failed"] {
            let capture = UiCapture::<()>::new();
            let result = capture.prepare(CapturePoint::LoadCallback, || Err::<(), _>(error.into()), |_| panic!("model before preparation"), |_| panic!("hooks before preparation"));
            let failure = result.unwrap_err();
            assert!(failure.contains(error));
            assert!(!capture.ready() && !capture.made());
            for point in [CapturePoint::LoadCallback, CapturePoint::Worker { graphics_initialized: true }] {
                assert_eq!(capture.prepare(point, || panic!("retried failed normal preparation"), |_: &()| title_model(), |_| Ok(())), Err(failure.clone()));
            }
        }
    }

    #[test]
    fn ready_capture_ignores_later_redundant_preparation_failures() {
        let capture = UiCapture::<()>::new();
        capture.prepare(CapturePoint::LoadCallback, || Ok(()), |_| title_model(), |_| Ok(())).unwrap();
        startup_title(&capture);
        let before = shown(&capture);
        for point in [CapturePoint::LoadCallback, CapturePoint::Worker { graphics_initialized: true }] {
            assert_eq!(capture.prepare(point, || panic!("attachment after capture was ready"), |_: &()| panic!("model replaced"), |_| Err("late redundant failure".into())), Ok(false));
            assert_eq!(capture.status(), Ok(true));
            assert_eq!(capture.worker_needs_capture(|| panic!("disablement checked after readiness")), Ok(false));
            assert_eq!(shown(&capture), before);
        }
    }

    #[test]
    fn worker_waiting_on_failed_normal_preparation_observes_the_same_gap() {
        let capture = Arc::new(UiCapture::<()>::new());
        let (entered, preparing) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let (started, observing) = mpsc::channel();
        let (done, status) = mpsc::channel();
        let timeout = Duration::from_secs(5);
        let normal = {
            let capture = capture.clone();
            std::thread::spawn(move || capture.prepare(CapturePoint::LoadCallback, || {
                entered.send(()).unwrap();
                released.recv_timeout(timeout).unwrap();
                Err::<(), _>("attachment failed".into())
            }, |_| title_model(), |_| Ok(())))
        };
        preparing.recv_timeout(timeout).unwrap();
        let worker = {
            let capture = capture.clone();
            std::thread::spawn(move || {
                started.send(()).unwrap();
                done.send(capture.status()).unwrap();
            })
        };
        observing.recv_timeout(timeout).unwrap();
        let premature = status.recv_timeout(Duration::from_millis(50));
        release.send(()).unwrap();
        let failure = normal.join().unwrap().unwrap_err();
        worker.join().unwrap();
        assert!(matches!(premature, Err(mpsc::RecvTimeoutError::Timeout)), "worker bypassed serialized preparation: {premature:?}");
        assert_eq!(status.recv_timeout(timeout).unwrap(), Err(failure));
        assert!(!capture.ready() && !capture.made());
    }

    #[test]
    fn concurrent_callers_wait_for_complete_hook_setup() {
        let capture = Arc::new(UiCapture::new());
        let (entered, installing) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let (started, attempting) = mpsc::channel();
        let (returned, result) = mpsc::channel();
        let timeout = Duration::from_secs(5);
        let first = {
            let capture = capture.clone();
            std::thread::spawn(move || capture.install(CapturePoint::LoadCallback, title_model, || {
                entered.send(()).unwrap();
                released.recv_timeout(timeout).unwrap();
                Ok::<_, String>(())
            }))
        };
        installing.recv_timeout(timeout).unwrap();
        assert!(!capture.ready());
        let second = {
            let capture = capture.clone();
            std::thread::spawn(move || {
                started.send(()).unwrap();
                let installed = capture.install(CapturePoint::LoadCallback, || panic!("concurrent model reset"), || panic!("duplicate setup"));
                returned.send((installed, capture.ready())).unwrap();
            })
        };
        attempting.recv_timeout(timeout).unwrap();
        let premature = result.recv_timeout(Duration::from_millis(50));
        release.send(()).unwrap();
        let first_result = first.join().unwrap();
        second.join().unwrap();
        assert!(matches!(premature, Err(mpsc::RecvTimeoutError::Timeout)), "another caller returned before hooks completed: {premature:?}");
        assert_eq!(first_result, Ok(true));
        assert_eq!(result.recv_timeout(timeout).unwrap(), (Ok::<_, String>(false), true));
    }

    #[test]
    fn map_only_policy_waits_for_activation_and_cannot_override_restoration() {
        let drawing = Arc::new(OriginalDrawing::new());
        let map_off = Arc::new(AtomicBool::new(false));
        let (entered, activating) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let (started, applying) = mpsc::channel();
        let (done, completed) = mpsc::channel();
        let timeout = Duration::from_secs(5);
        let activation = {
            let (drawing, map_off) = (drawing.clone(), map_off.clone());
            std::thread::spawn(move || drawing.activate(true, false, |skip| {
                map_off.store(skip, Ordering::Relaxed);
                entered.send(()).unwrap();
                released.recv_timeout(timeout).unwrap();
            }))
        };
        activating.recv_timeout(timeout).unwrap();
        // Reproduce the former init race exactly: map suppression is set,
        // but the published UI suppression bit has not yet changed.
        assert!(map_off.load(Ordering::Relaxed));
        assert!(!drawing.skipping());
        let policy = {
            let (drawing, map_off) = (drawing.clone(), map_off.clone());
            std::thread::spawn(move || {
                started.send(()).unwrap();
                drawing.map_policy(false, |off| map_off.store(off, Ordering::Relaxed));
                done.send(()).unwrap();
            })
        };
        applying.recv_timeout(timeout).unwrap();
        let premature = completed.recv_timeout(Duration::from_millis(50));
        release.send(()).unwrap();
        activation.join().unwrap();
        policy.join().unwrap();
        assert!(matches!(premature, Err(mpsc::RecvTimeoutError::Timeout)), "map policy bypassed activation: {premature:?}");
        assert!(drawing.skipping() && map_off.load(Ordering::Relaxed));
        drawing.restore(true, || map_off.store(false, Ordering::Relaxed));
        drawing.map_policy(true, |off| map_off.store(off, Ordering::Relaxed));
        assert!(!drawing.skipping() && !map_off.load(Ordering::Relaxed));
        // Explicit UI=off inspection still works before viewer failure.
        let drawing = OriginalDrawing::new();
        drawing.map_policy(true, |off| assert!(off));
        assert!(!drawing.skipping());
    }

    #[test]
    fn replacement_failure_cannot_enable_native_pixels_or_run_recovery_callback() {
        let drawing = OriginalDrawing::new();
        drawing.activate(true, false, |skip| assert!(skip));
        assert!(!drawing.restore(false, || panic!("default recovery callback ran")));
        assert!(drawing.failed() && drawing.skipping());
        drawing.activate(true, true, |_| panic!("failed replacement reenabled pixel rendering"));
        assert!(drawing.skipping());
        drawing.map_policy(false, |off| assert!(off));
    }

    #[test]
    fn suppression_requires_complete_capture_and_viewer_failure_restores_drawing() {
        let disabled = UiCapture::<()>::new();
        assert_eq!(disabled.worker_needs_capture(|| false), Ok(false));
        assert_eq!(disabled.status(), Ok(false));
        let drawing = OriginalDrawing::new();
        drawing.activate(false, false, |skip| assert!(!skip));
        assert!(!drawing.skipping());
        drawing.activate(true, true, |skip| assert!(!skip));
        assert!(!drawing.skipping());
        drawing.activate(true, false, |skip| assert!(skip));
        assert!(drawing.skipping());
        drawing.restore(true, || {});
        assert!(drawing.failed());
        assert!(!drawing.skipping());
        drawing.activate(true, false, |_| panic!("failed viewer reactivated suppression"));
        assert!(!drawing.skipping());
    }
}
