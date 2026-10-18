//! Process-lifetime UI capture, installed before the game's first drawing.

use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};

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

/// Coordinate suppression with successful capture and viewer startup.
/// Restoration is permanent and serializes against concurrent activation.
pub(crate) struct OriginalDrawing {
    failed: Mutex<bool>,
    skipping: AtomicBool,
}

impl OriginalDrawing {
    pub(crate) const fn new() -> Self {
        Self { failed: Mutex::new(false), skipping: AtomicBool::new(false) }
    }

    pub(crate) fn failed(&self) -> bool {
        *self.failed.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn skipping(&self) -> bool {
        self.skipping.load(Ordering::Relaxed)
    }

    pub(crate) fn activate(&self, capture_ready: bool, game_draws: bool, configure: impl FnOnce(bool)) {
        let failed = self.failed.lock().unwrap_or_else(|e| e.into_inner());
        if *failed { return }
        let skip = capture_ready && !game_draws;
        configure(skip);
        self.skipping.store(skip, Ordering::Relaxed);
    }

    /// Apply map-only policy without overwriting concurrent full capture
    /// activation or permanent restoration. Used by early DLL setup too.
    pub(crate) fn map_policy(&self, explicit: bool, configure: impl FnOnce(bool)) {
        let failed = self.failed.lock().unwrap_or_else(|e| e.into_inner());
        configure(!*failed && (explicit || self.skipping()));
    }

    pub(crate) fn restore(&self, restore: impl FnOnce()) {
        let mut failed = self.failed.lock().unwrap_or_else(|e| e.into_inner());
        *failed = true;
        self.skipping.store(false, Ordering::Relaxed);
        restore();
    }
}

/// Serialize complete hook installation, retain its guards and reuse the
/// exact model when the viewer starts. A failed attempt may be retried.
pub(crate) struct UiCapture<T> {
    installed: Mutex<Option<T>>,
    ready: AtomicBool,
    pub(crate) model: Mutex<Option<UiModel>>,
}

impl<T> UiCapture<T> {
    pub(crate) const fn new() -> Self {
        Self { installed: Mutex::new(None), ready: AtomicBool::new(false), model: Mutex::new(None) }
    }

    pub(crate) fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Every caller waits for all hooks, including callers that find an
    /// earlier installation in progress. Hooks may emit into the model
    /// during setup, so its lock is released before installing them.
    pub(crate) fn install<E>(
        &self,
        point: CapturePoint,
        model: impl FnOnce() -> UiModel,
        hooks: impl FnOnce() -> Result<T, E>,
    ) -> Result<bool, E> {
        if !point.allowed() { return Ok(false) }
        let mut installed = self.installed.lock().unwrap_or_else(|e| e.into_inner());
        if installed.is_some() { return Ok(false) }
        {
            let mut state = self.model.lock().unwrap_or_else(|e| e.into_inner());
            state.get_or_insert_with(|| {
                let mut model = model();
                // The title uses the map canvas before the publisher can
                // identify a running game. Preserve its copies from startup.
                model.set_skipping(false);
                model
            });
        }
        *installed = Some(hooks()?);
        self.ready.store(true, Ordering::Release);
        Ok(true)
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

    fn emit<T>(capture: &UiCapture<T>, event: Event) {
        if let Some(model) = capture.model.lock().unwrap().as_mut() { model.apply(event) }
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
        capture.model.lock().unwrap().as_ref().unwrap().shown_frame()
    }

    fn assert_title(frame: &Frame) {
        assert!(matches!(&frame.draws[..], [ScreenDraw::Image { path, rect: [0, 0, 1920, 1080], x: 0, y: 0, .. }] if path == "Art/Title.pcx"), "missing passive startup title: {:?}", frame.draws);
    }

    #[test]
    fn title_capture_precedes_viewer_and_preserves_underlay_copies() {
        let capture = UiCapture::new();
        capture.install(CapturePoint::LoadCallback, title_model, || Ok::<_, ()>(())).unwrap();
        // All startup drawing happens before the viewer/worker gets a turn.
        startup_title(&capture);
        assert_title(&shown(&capture));
        assert_eq!(capture.install(CapturePoint::Worker { graphics_initialized: true }, || panic!("model reset"), || panic!("hooks installed twice")), Ok::<_, ()>(false));
        assert_title(&shown(&capture));
    }

    #[test]
    fn later_startup_keeps_files_sprites_generated_surfaces_and_presented_snapshot() {
        let capture = UiCapture::new();
        capture.install(CapturePoint::LoadCallback, title_model, || Ok::<_, ()>(())).unwrap();
        startup_title(&capture);
        capture.model.lock().unwrap().as_mut().unwrap().add_generated(701);
        // Retained canvas content changes after presenting. The shown Arc
        // snapshot must continue showing the title until another present.
        emit(&capture, Event::Fill { canvas: 400, rect: [0, 0, 1920, 1080], rgb: [1, 2, 3], alpha: 255, clip: None });
        let before = shown(&capture);
        let stats = capture.model.lock().unwrap().as_ref().unwrap().stats();
        capture.install(CapturePoint::Worker { graphics_initialized: true }, title_model, || Ok::<_, ()>(())).unwrap();
        assert_eq!(shown(&capture), before);
        assert_title(&before);
        let mut state = capture.model.lock().unwrap();
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
    }

    #[test]
    fn dllmain_and_a_mapped_library_cannot_initialize_full_capture() {
        let capture = UiCapture::<()>::new();
        for point in [CapturePoint::DllMain, CapturePoint::Worker { graphics_initialized: false }] {
            assert_eq!(capture.install(point, || panic!("capture before CRT/C3X"), || panic!("hooks under loader lock")), Ok::<_, ()>(false));
            assert!(!capture.ready());
            assert!(capture.model.lock().unwrap().is_none());
        }
        assert_eq!(capture.install(CapturePoint::Worker { graphics_initialized: true }, title_model, || Ok::<_, ()>(())), Ok(true));
    }

    #[test]
    fn failed_setup_is_not_ready_and_retry_keeps_captured_history() {
        let capture = UiCapture::new();
        let result = capture.install(CapturePoint::LoadCallback, title_model, || {
            startup_title(&capture);
            Err::<(), _>("hook installation failed")
        });
        assert_eq!(result, Err("hook installation failed"));
        assert!(!capture.ready());
        assert_title(&shown(&capture));
        assert_eq!(capture.install(CapturePoint::LoadCallback, || panic!("reset on retry"), || Ok::<_, &str>(())), Ok(true));
        assert!(capture.ready());
        assert_title(&shown(&capture));
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
                Ok::<_, ()>(())
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
        assert_eq!(result.recv_timeout(timeout).unwrap(), (Ok::<_, ()>(false), true));
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
        drawing.restore(|| map_off.store(false, Ordering::Relaxed));
        drawing.map_policy(true, |off| map_off.store(off, Ordering::Relaxed));
        assert!(!drawing.skipping() && !map_off.load(Ordering::Relaxed));
        // Explicit UI=off inspection still works before viewer failure.
        let drawing = OriginalDrawing::new();
        drawing.map_policy(true, |off| assert!(off));
        assert!(!drawing.skipping());
    }

    #[test]
    fn suppression_requires_complete_capture_and_viewer_failure_restores_drawing() {
        let drawing = OriginalDrawing::new();
        drawing.activate(false, false, |skip| assert!(!skip));
        assert!(!drawing.skipping());
        drawing.activate(true, true, |skip| assert!(!skip));
        assert!(!drawing.skipping());
        drawing.activate(true, false, |skip| assert!(skip));
        assert!(drawing.skipping());
        drawing.restore(|| {});
        assert!(drawing.failed());
        assert!(!drawing.skipping());
        drawing.activate(true, false, |_| panic!("failed viewer reactivated suppression"));
        assert!(!drawing.skipping());
    }
}
