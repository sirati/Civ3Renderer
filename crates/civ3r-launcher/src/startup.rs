//! Main-thread native startup display. The lifecycle worker owns every child.
use std::{
    ffi::OsString,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use crate::unix::{Ask, Question};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, StartCause, WindowEvent},
    keyboard::{Key, NamedKey},
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
    /// A question for the user; the answer goes back on the sender.
    Question(Question, std::sync::mpsc::Sender<bool>),
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
            super::unix::launch(args, Some(tx.clone()), worker_cancel, asker(Some(tx.clone()))).map_err(|e| e.to_string())
        }))
        .unwrap_or_else(|panic| Err(panic_message(panic)));
        let _ = tx.send(Update::Finished(result));
    });
    let mut app = Startup::new(cancel);
    app.worker = Some(worker);
    let run = event_loop.run_app(&mut app);
    app.finish_worker();
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
    let result = super::unix::launch(args, None, cancel, asker(None));
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
    /// The lifecycle worker, joined before the process ends (`exiting`).
    worker: Option<std::thread::JoinHandle<()>>,
    /// A question the worker waits on, and what was typed for it.
    question: Option<(Question, std::sync::mpsc::Sender<bool>)>,
    typed: String,
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
            worker: None,
            question: None,
            typed: String::new(),
        }
    }

    /// Return true only when there is no active lifecycle worker to clean up.
    /// Stops the lifecycle worker (unless it finished) and waits for it: it
    /// stops the game, lets the launch lock go and turns the system
    /// shortcuts on again.
    fn finish_worker(&mut self) {
        if self.result.is_none() {
            let _ = self.cancel.compare_exchange(0, 143, Ordering::Relaxed, Ordering::Relaxed);
        }
        // A question still open is a no.
        self.answer(false);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    /// Answers the open question, if any.
    fn answer(&mut self, yes: bool) {
        if let Some((_, answer)) = self.question.take() {
            let _ = answer.send(yes);
            self.typed.clear();
            self.title = if yes { "Starting Civilization III" } else { "Not starting Civilization III" }.into();
            self.reason = if yes { "Removing the lock and starting." } else { "Cancelled." }.into();
        }
    }

    /// A key typed while a question is open: `delete` and Enter confirms,
    /// Escape cancels.
    fn question_key(&mut self, key: &Key, text: Option<&str>) {
        match key {
            Key::Named(NamedKey::Enter) => {
                if confirmed(&self.typed) {
                    self.answer(true);
                } else {
                    self.typed.clear();
                }
            }
            Key::Named(NamedKey::Escape) => self.answer(false),
            Key::Named(NamedKey::Backspace) => {
                self.typed.pop();
            }
            _ => {
                if let Some(text) = text {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        if self.typed.chars().count() < 32 {
                            self.typed.push(c);
                        }
                    }
                }
            }
        }
    }

    /// The question's text as the window shows it.
    fn question_text(&self) -> Option<String> {
        let (question, _) = self.question.as_ref()?;
        Some(format!(
            "{}\n\n{}\n\nWarning: if another copy is running after all, two copies of the game will run at once.\n\nType {CONFIRM} and press Enter to {}. Press Escape or close this window to cancel.\n\n> {}_",
            question.title,
            question.text,
            question.action.to_lowercase(),
            self.typed
        ))
    }

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
        let text = self.question_text().unwrap_or_else(|| format!(
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
        ));
        let Some(window) = &self.window else { return };
        let Some(surface) = &mut self.surface else {
            return;
        };
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
                // (Closing the window says no to an open question.)
                self.answer(false);
                if self.result.is_some() {
                    event_loop.exit();
                } else {
                    self.cancel.store(143, Ordering::Relaxed);
                    self.title = "Stopping Civilization III".into();
                    self.reason = "Stopping the game and renderer started by this launcher.".into();
                    self.paint();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if self.question.is_some() && event.state == ElementState::Pressed => {
                self.question_key(&event.logical_key, event.text.as_deref());
                self.paint();
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
            Update::Question(question, answer) => {
                if self.surface.is_none() {
                    // Nothing to show it in: no.
                    let _ = answer.send(false);
                    return;
                }
                self.question = Some((question, answer));
                self.typed.clear();
                self.scroll = 0;
                if let Some(window) = &self.window {
                    let _ = window.request_inner_size(winit::dpi::LogicalSize::new(800.0, 420.0));
                    window.set_visible(true);
                    window.focus_window();
                }
            }
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
    fn exiting(&mut self, _: &ActiveEventLoop) {
        // On macOS a Quit Apple event (the Dock's Quit, System Settings' "Quit
        // & Reopen" after a permission is granted, logging out) ends the
        // process with exit() right after this: no destructor would run, the
        // game would be left running, its lock and its paused shortcuts
        // behind. So the game is stopped and all let go here first.
        self.finish_worker();
    }
    fn new_events(&mut self, _: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.paint();
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // The elapsed seconds are shown while starting: wake for each one, or
        // a slow stage looks like a hung launcher.
        let counting = self.surface.is_some()
            && self.result.is_none()
            && self.cancel.load(Ordering::Relaxed) == 0
            && renderable(self.ready, false, self.occluded);
        event_loop.set_control_flow(if counting {
            ControlFlow::WaitUntil(self.started + next_second(self.started.elapsed()))
        } else {
            ControlFlow::Wait
        });
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

/// The first whole second after `elapsed`, when the shown count changes.
fn next_second(elapsed: Duration) -> Duration {
    Duration::from_secs(elapsed.as_secs() + 1)
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
    fn the_elapsed_count_wakes_at_the_next_whole_second() {
        assert_eq!(next_second(Duration::ZERO), Duration::from_secs(1));
        assert_eq!(next_second(Duration::from_millis(1999)), Duration::from_secs(2));
        assert_eq!(next_second(Duration::from_secs(2)), Duration::from_secs(3));
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

    #[test]
    fn ending_the_app_stops_the_worker_and_waits_for_its_cleanup() {
        // What a Quit Apple event comes to (`exiting`): the worker sees the
        // cancel, cleans up (here: notes it did), and is joined before
        // AppKit's exit().
        let cancel = Arc::new(AtomicUsize::new(0));
        let cleaned = Arc::new(AtomicUsize::new(0));
        let mut app = Startup::new(cancel.clone());
        let (seen, done) = (cancel.clone(), cleaned.clone());
        app.worker = Some(std::thread::spawn(move || {
            while seen.load(Ordering::Relaxed) == 0 {
                std::thread::sleep(Duration::from_millis(1));
            }
            std::thread::sleep(Duration::from_millis(20));
            done.store(1, Ordering::Relaxed);
        }));
        let (answer, asked) = std::sync::mpsc::channel();
        app.question = Some((Question { title: "t".into(), text: "x".into(), action: "Go" }, answer));
        app.finish_worker();
        assert_eq!(cancel.load(Ordering::Relaxed), 143);
        assert_eq!(cleaned.load(Ordering::Relaxed), 1, "joined after its cleanup");
        assert_eq!(asked.recv(), Ok(false), "an open question is a no");
    }

    #[test]
    fn a_question_is_confirmed_by_typing_delete_and_enter() {
        let mut app = Startup::new(Arc::new(AtomicUsize::new(0)));
        let (answer, asked) = std::sync::mpsc::channel();
        app.question = Some((Question { title: "t".into(), text: "x".into(), action: "Remove lock and start" }, answer));
        let ch = |c: &str| Key::Character(c.into());
        for c in ["d", "e", "l", "x"] {
            app.question_key(&ch(c), Some(c));
        }
        app.question_key(&Key::Named(NamedKey::Backspace), None);
        // "del" and Enter is not enough: typing starts again.
        app.question_key(&Key::Named(NamedKey::Enter), None);
        assert!(app.question.is_some() && app.typed.is_empty());
        for c in ["d", "e", "l", "e", "t", "e"] {
            app.question_key(&ch(c), Some(c));
        }
        assert!(app.question_text().unwrap().contains("> delete_"));
        app.question_key(&Key::Named(NamedKey::Enter), None);
        assert_eq!(asked.recv(), Ok(true));
        assert!(app.question.is_none());
        // Escape says no.
        let (answer, asked) = std::sync::mpsc::channel();
        app.question = Some((Question { title: "t".into(), text: "x".into(), action: "Go" }, answer));
        app.question_key(&Key::Named(NamedKey::Escape), None);
        assert_eq!(asked.recv(), Ok(false));
        assert!(confirmed(" Delete\n") && !confirmed("yes"));
    }

    #[test]
    fn the_dialogs_answer_is_its_button() {
        let action = "Remove lock and start";
        assert_eq!(dialog_answer(true, "button returned:Remove lock and start\n", "", action), Some(true));
        assert_eq!(dialog_answer(true, "button returned:Cancel\n", "", action), Some(false));
        assert_eq!(dialog_answer(false, "", "execution error: User canceled. (-128)", action), Some(false));
        assert_eq!(dialog_answer(false, "", "osascript: no such file", action), None);
    }
}

/// What the user types to confirm a question (the user's own suggestion).
const CONFIRM: &str = "delete";

/// Asks in the terminal: the question, then `delete` and Enter to confirm.
fn ask_in_terminal(question: &Question) -> bool {
    use std::io::{BufRead, Write};
    let mut err = std::io::stderr();
    let _ = writeln!(err, "\nciv3-play-macos: {}\n{}\n", question.title, question.text);
    let _ = write!(err, "Type {CONFIRM} and press Enter to {}; anything else cancels: ", question.action.to_lowercase());
    let _ = err.flush();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).is_ok() && confirmed(&line)
}

fn confirmed(typed: &str) -> bool {
    typed.trim().eq_ignore_ascii_case(CONFIRM)
}

/// Asks in a macOS dialog (no terminal when started from the Dock or the
/// Finder): `Some(answer)`, or `None` if no dialog could be shown.
#[cfg(target_os = "macos")]
fn ask_in_dialog(question: &Question) -> Option<bool> {
    // The texts are arguments, not script source: nothing to escape.
    let output = std::process::Command::new("/usr/bin/osascript")
        .args([
            "-e", "on run argv",
            "-e", "display dialog (item 2 of argv) with title (item 1 of argv) buttons {\"Cancel\", item 3 of argv} default button \"Cancel\" cancel button \"Cancel\" with icon caution",
            "-e", "end run",
        ])
        .arg(&question.title)
        .arg(format!("{}\n\nWarning: if another copy is running after all, two copies of the game will run at once.", question.text))
        .arg(question.action)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    dialog_answer(output.status.success(), &stdout, &stderr, question.action)
}

/// osascript's answer: the button, a cancel (-128), or no dialog.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn dialog_answer(success: bool, stdout: &str, stderr: &str, action: &str) -> Option<bool> {
    if success {
        Some(stdout.trim() == format!("button returned:{action}"))
    } else if stderr.contains("(-128)") {
        Some(false)
    } else {
        eprintln!("civ3-play-macos: no dialog: {}", stderr.trim());
        None
    }
}

/// The way to ask for this launch: the terminal if there is one, else a
/// dialog (macOS), else the startup window (`window`, typing `delete` and
/// Enter), else nothing (refused; `--remove-lock` says it ahead).
fn asker(window: Option<Progress>) -> Ask {
    use std::io::IsTerminal;
    let terminal = std::io::stdin().is_terminal();
    Box::new(move |question| {
        if terminal {
            return ask_in_terminal(question);
        }
        #[cfg(target_os = "macos")]
        if let Some(answer) = ask_in_dialog(question) {
            return answer;
        }
        let Some(window) = &window else { return false };
        let (answer, wait) = std::sync::mpsc::channel();
        if window.send(Update::Question(question.clone(), answer)).is_err() {
            return false;
        }
        wait.recv().unwrap_or(false)
    })
}
