//! Main-thread native startup display. The lifecycle worker owns every child.
use std::{
    ffi::OsString,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

#[derive(Debug)]
pub(crate) enum Update {
    Stage(&'static str, &'static str),
    Logs(String),
    Failure(String),
    Ready,
    Interrupted(i32),
    Finished(Result<i32, String>),
}
#[derive(Clone)]
pub(crate) struct Progress(winit::event_loop::EventLoopProxy<Update>);
impl Progress {
    pub(crate) fn send(
        &self,
        update: Update,
    ) -> Result<(), winit::event_loop::EventLoopClosed<Update>> {
        self.0.send_event(update)
    }
}
pub(crate) fn stage(progress: &Option<Progress>, title: &'static str, reason: &'static str) {
    if let Some(progress) = progress {
        let _ = progress.send(Update::Stage(title, reason));
    }
}

pub fn launch(args: impl IntoIterator<Item = OsString>) -> Result<i32, Box<dyn std::error::Error>> {
    let args: Vec<_> = args.into_iter().collect();
    let graphical = std::env::var_os("CIV3R_STARTUP_WINDOW").as_deref()
        != Some(std::ffi::OsStr::new("0"))
        && !args.iter().any(|a| a == "--help" || a == "-h")
        && (cfg!(target_os = "macos")
            || std::env::var_os("DISPLAY").is_some()
            || std::env::var_os("WAYLAND_DISPLAY").is_some());
    let cancel = Arc::new(AtomicUsize::new(0));
    if !graphical {
        return launch_without_window(args, cancel);
    }
    let mut builder = EventLoop::<Update>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder
            .with_activation_policy(ActivationPolicy::Accessory)
            .with_default_menu(false);
    }
    let event_loop = match builder.build() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("startup window unavailable: {error}");
            return launch_without_window(args, cancel);
        }
    };
    let tx = Progress(event_loop.create_proxy());
    // Keep signal delivery alive even after the lifecycle worker has finished.
    let signal_tx = tx.clone();
    let signals = signals::Listener::new(cancel.clone(), move |status| {
        signal_tx.send(Update::Interrupted(status)).is_ok()
    })?;
    let worker_cancel = cancel.clone();
    let worker = std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            super::unix::launch(args, Some(tx.clone()), worker_cancel).map_err(|e| e.to_string())
        }))
        .unwrap_or_else(|panic| Err(panic_message(panic)));
        let _ = tx.send(Update::Finished(result));
    });
    let mut app = Startup::new(cancel);
    let run = event_loop.run_app(&mut app);
    if app.result.is_none() {
        let _ = app
            .cancel
            .compare_exchange(0, 143, Ordering::Relaxed, Ordering::Relaxed);
    }
    let _ = worker.join();
    // close() wakes the pipe reader. Join only after the native event loop returns.
    signals.finish();
    run?;
    completed_result(app.result.unwrap_or(Ok(143)), &app.cancel).map_err(Into::into)
}

fn launch_without_window(
    args: Vec<OsString>,
    cancel: Arc<AtomicUsize>,
) -> Result<i32, Box<dyn std::error::Error>> {
    let signals = signals::Listener::new(cancel.clone(), |_| true)?;
    let result = super::unix::launch(args, None, cancel);
    signals.finish();
    result
}

fn completed_result(result: Result<i32, String>, cancel: &AtomicUsize) -> Result<i32, String> {
    match cancel.load(Ordering::Relaxed) {
        0 => result,
        status => Ok(status as i32),
    }
}

struct Startup {
    cancel: Arc<AtomicUsize>,
    window: Option<Arc<Window>>,
    surface: Option<gpu::Display>,
    started: Instant,
    title: String,
    reason: String,
    logs: String,
    ready: bool,
    result: Option<Result<i32, String>>,
    scroll: usize,
    diagnostic: Option<String>,
    occluded: bool,
}
impl Startup {
    fn new(cancel: Arc<AtomicUsize>) -> Self {
        Self {
            cancel,
            window: None,
            surface: None,
            started: Instant::now(),
            title: "Preparing Civilization III".into(),
            reason: "Checking the private installation and launch configuration.".into(),
            logs: String::new(),
            ready: false,
            result: None,
            scroll: 0,
            diagnostic: None,
            occluded: false,
        }
    }

    /// Return true only when there is no active lifecycle worker to clean up.
    fn interrupt(&mut self, status: i32) -> bool {
        self.cancel.store(status as usize, Ordering::Relaxed);
        if self.result.is_some() {
            self.result = Some(Ok(status));
            return true;
        }
        self.title = "Stopping Civilization III".into();
        self.reason = "Stopping the game and renderer started by this launcher.".into();
        false
    }

    /// Record worker completion and decide whether the event loop must exit.
    fn complete(&mut self, result: Result<i32, String>) -> bool {
        let result = completed_result(result, &self.cancel);
        let retain_failure = result.as_ref().map_or(true, |code| *code != 0)
            && self.cancel.load(Ordering::Relaxed) == 0;
        self.result = Some(result);
        !retain_failure
    }
    fn paint(&mut self) {
        if self.cancel.load(Ordering::Relaxed) != 0 {
            return;
        }
        if !renderable(self.ready, self.result.is_some(), self.occluded) {
            return;
        }
        if let Some(window) = &self.window {
            window.set_title(&format!(
                "Civilization III: {} ({} s)",
                self.title,
                self.started.elapsed().as_secs()
            ));
        }
        let Some(window) = &self.window else { return };
        let Some(surface) = &mut self.surface else {
            return;
        };
        let text = format!(
            "{}\n\n{}\n\nElapsed: {}:{:02}\n\n{}",
            self.title,
            self.reason,
            self.started.elapsed().as_secs() / 60,
            self.started.elapsed().as_secs() % 60,
            if self.result.is_some() {
                format!(
                    "{}\n\nScroll to read all details. Close this window to dismiss.",
                    self.logs
                )
            } else {
                String::new()
            }
        );
        if let Err(error) = surface.paint(window, &text, self.scroll) {
            eprintln!("startup GPU display unavailable: {error}");
            window.set_visible(false);
            self.surface = None;
        }
    }
}
impl ApplicationHandler<Update> for Startup {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() || self.cancel.load(Ordering::Relaxed) != 0 {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Civilization III startup")
            .with_inner_size(winit::dpi::LogicalSize::new(640.0, 300.0));
        match event_loop.create_window(attributes) {
            Ok(window) => {
                let window = Arc::new(window);
                match gpu::Display::new(window.clone()) {
                    Ok(surface) => self.surface = Some(surface),
                    Err(error) => {
                        eprintln!("startup GPU display unavailable: {error}");
                        window.set_visible(false);
                    }
                }
                self.window = Some(window);
                self.paint();
            }
            Err(error) => eprintln!("startup window unavailable: {error}"),
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                if self.result.is_some() {
                    event_loop.exit();
                } else {
                    self.cancel.store(143, Ordering::Relaxed);
                    self.title = "Stopping Civilization III".into();
                    self.reason = "Stopping the game and renderer started by this launcher.".into();
                    self.paint();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => (p.y / 20.0) as f32,
                };
                self.scroll = if lines > 0.0 {
                    self.scroll.saturating_sub(lines.ceil() as usize)
                } else {
                    self.scroll.saturating_add((-lines).ceil() as usize)
                };
                self.paint();
            }
            WindowEvent::Occluded(value) => {
                self.occluded = value;
                if !value {
                    self.paint();
                }
            }
            WindowEvent::RedrawRequested | WindowEvent::Resized(_) => self.paint(),
            _ => {}
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, update: Update) {
        let visible_change = !matches!(&update, Update::Logs(_) | Update::Failure(_));
        match update {
            Update::Stage(title, reason) => {
                self.title = title.into();
                self.reason = reason.into();
            }
            Update::Logs(logs) => self.logs = logs,
            Update::Failure(message) => self.diagnostic = Some(message),
            Update::Ready => {
                self.ready = true;
                if let Some(window) = &self.window {
                    window.set_visible(false);
                }
            }
            Update::Interrupted(status) => {
                if self.interrupt(status) {
                    event_loop.exit();
                }
                return;
            }
            Update::Finished(result) => {
                if self.complete(result) {
                    event_loop.exit();
                    return;
                }
                let result = self.result.as_ref().expect("completion records its result");
                self.title = if self.ready {
                    "Civilization III stopped unexpectedly"
                } else {
                    "Civilization III could not start"
                }
                .into();
                self.scroll = 0;
                self.reason = self.diagnostic.take().unwrap_or_else(|| match &result {
                            Err(error) => error.clone(),
                            Ok(code) => format!(
                                "The game exited with code {code}. See the logs below. Close this window to dismiss."
                            ),
                        });
                if let Some(window) = &self.window {
                    let _ = window.request_inner_size(winit::dpi::LogicalSize::new(800.0, 540.0));
                    window.set_visible(true);
                }
            }
        }
        if self.result.is_some() || visible_change {
            self.paint();
        }
        if self.surface.is_none() && self.result.is_some() {
            event_loop.exit();
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
    }
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    let reason = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("unknown panic");
    format!("Launcher failed: {reason}. Owned game processes have been stopped.")
}

mod gpu;
mod signals;

fn renderable(ready: bool, finished: bool, occluded: bool) -> bool {
    !occluded && (!ready || finished)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hidden_and_occluded_windows_never_render_but_failure_can_show() {
        assert!(renderable(false, false, false));
        assert!(!renderable(true, false, false));
        assert!(renderable(true, true, false));
        for ready in [false, true] {
            for finished in [false, true] {
                assert!(!renderable(ready, finished, true));
            }
        }
    }
    #[test]
    fn completed_result_preserves_success_and_error_unless_interrupted() {
        let cancel = AtomicUsize::new(0);
        assert_eq!(completed_result(Ok(0), &cancel), Ok(0));
        assert_eq!(
            completed_result(Err("failure".into()), &cancel),
            Err("failure".into())
        );
        for status in [129, 130, 143] {
            cancel.store(status, Ordering::Relaxed);
            assert_eq!(completed_result(Ok(0), &cancel), Ok(status as i32));
            assert_eq!(
                completed_result(Err("failure".into()), &cancel),
                Ok(status as i32)
            );
        }
    }
    #[test]
    fn worker_panic_keeps_the_diagnostic() {
        assert!(
            panic_message(Box::new("registry worker failed")).contains("registry worker failed")
        );
    }
}
