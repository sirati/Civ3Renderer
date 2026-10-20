//! Native pointer runtime probe. Run only on a test desktop:
//!
//! `cargo run -p pointer-control --example pointer_qa -- --run`
//!
//! Commands arrive on stdin, with one command per line. Keep stdin open when
//! driving through SSH or a FIFO. Every output line starts with elapsed ms.
//! `case bounds|horizontal|vertical|divider|strip|box|none` selects a region
//! and requests its safe starting point. Inject motion to obtain a fresh native
//! sample, then `arm` to measure. Intentional warps invalidate prior samples.
//! `capture on|off`, `move X Y`, `focus`, `disarm`, and `quit` are also available.
//!
//! Cursor samples are native winit CursorMoved events in physical content
//! pixels, not predicted positions. Raw deltas are logged separately. A backend
//! that cannot install native regions reports `confined=false`; this probe
//! deliberately does not hide that limitation with application-side warps.
//! Focus loss releases the controller. Focus regain does not restore a capture.
//! Escape always releases constraints and exits. The title identifies the case.

use pointer_control::{Controller, Grab, Point, Region, Update};
use std::{
    io::{self, BufRead, Write},
    sync::Arc,
    time::Instant,
};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

#[derive(Clone, Copy, Debug)]
enum Case {
    None,
    Bounds,
    Horizontal,
    Vertical,
    Divider,
    Strip,
    Box,
}
impl Case {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Self::None,
            "bounds" => Self::Bounds,
            "horizontal" => Self::Horizontal,
            "vertical" => Self::Vertical,
            "divider" => Self::Divider,
            "strip" => Self::Strip,
            "box" => Self::Box,
            _ => return None,
        })
    }
    fn region(self) -> Option<Region> {
        let mut r = Region {
            bounds: [40, 40, 720, 520],
            exclusions: vec![],
        };
        match self {
            Self::None => return None,
            Self::Bounds => r.bounds = [80, 80, 640, 440],
            Self::Horizontal => r.bounds = [80, 300, 640, 1],
            Self::Vertical => r.bounds = [400, 80, 1, 440],
            // The central opening permits passage between the two sides.
            Self::Divider => r.exclusions = vec![[390, 40, 20, 220], [390, 340, 20, 220]],
            Self::Strip => r.exclusions = vec![[40, 40, 720, 100]],
            Self::Box => r.exclusions = vec![[300, 160, 200, 120]],
        }
        Some(r)
    }
    fn start(self) -> Point {
        match self {
            Self::Horizontal => (200, 300),
            Self::Vertical => (400, 320),
            _ => (200, 320),
        }
    }
}

struct Probe {
    started: Instant,
    window: Option<Arc<Window>>,
    pointer: Controller,
    case: Case,
    region: Option<Region>,
    focused: bool,
    inside: bool,
    captured: bool,
    last: Option<(f64, f64)>,
    position_hint: (f64, f64),
    sampled: Option<Instant>,
    previous: Option<(f64, f64)>,
    update: Update,
    armed: bool,
    samples: u64,
    violations: u64,
    crossing_candidates: u64,
}
impl Probe {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            window: None,
            pointer: Controller::default(),
            case: Case::None,
            region: None,
            focused: false,
            inside: false,
            captured: false,
            last: None,
            position_hint: (0.0, 0.0),
            sampled: None,
            previous: None,
            update: Update::default(),
            armed: false,
            samples: 0,
            violations: 0,
            crossing_candidates: 0,
        }
    }
    fn log(&self, message: impl std::fmt::Display) {
        println!("{} {message}", self.started.elapsed().as_millis());
        let _ = io::stdout().flush();
    }
    fn update(&mut self) {
        let next = self.pointer.update(
            self.region.as_ref(),
            self.position_hint,
            self.focused && self.inside,
        );
        if next != self.update {
            self.log(format_args!("UPDATE {next:?}"));
            self.update = next;
        }
    }
    fn release(&mut self) {
        let update = self.pointer.update(None, self.position_hint, false);
        let grab = self.pointer.grab(false);
        self.log(format_args!("RELEASE update={update:?} grab={grab:?}"));
        self.captured = false;
        self.update = update;
        self.previous = None;
    }
    fn report(&self) {
        self.log(format_args!(
            "REPORT case={:?} focused={} inside={} captured={} native_confined={} armed={} samples={} outside={} crossing_candidates={} last_native={:?} sample_age_ms={:?}",
            self.case, self.focused, self.inside, self.captured, self.update.confined,
            self.armed, self.samples, self.violations, self.crossing_candidates, self.last,
            self.sampled.map(|t| t.elapsed().as_millis()),
        ));
    }
    fn command(&mut self, text: &str, event_loop: &ActiveEventLoop) {
        self.log(format_args!("COMMAND {text}"));
        let fields: Vec<_> = text.split_whitespace().collect();
        match fields.as_slice() {
            ["case", name] if Case::parse(name).is_some() => {
                self.release();
                self.armed = false;
                self.case = Case::parse(name).unwrap();
                self.region = self.case.region();
                self.log(format_args!("CASE {:?} region={:?} start={:?}", self.case, self.region, self.case.start()));
                if let Some(window) = &self.window {
                    window.set_title(&format!("pointer QA: {:?}", self.case));
                }
                self.move_pointer(self.case.start());
                self.update();
            }
            ["move", x, y] => match (x.parse(), y.parse()) {
                (Ok(x), Ok(y)) => {
                    self.move_pointer((x, y));
                }
                _ => self.log("ERROR move requires two signed integers"),
            },
            ["capture", state @ ("on" | "off")] => {
                let on = *state == "on";
                let result = self.pointer.grab(on);
                self.captured = result.as_ref().is_ok_and(|g| *g != Grab::Released);
                self.log(format_args!("CAPTURE requested={on} result={result:?}"));
                self.update();
            }
            ["arm"] if self.last.is_none() => {
                self.log("ERROR arm needs a fresh native CursorMoved sample after placement; inject motion first");
            }
            ["arm"] => {
                self.armed = true;
                self.samples = 0; self.violations = 0; self.crossing_candidates = 0;
                self.previous = self.last;
                self.log("ARMED use external physical mouse injection; do not use move to test a barrier");
                self.report();
            }
            ["disarm"] => { self.armed = false; self.previous = None; self.log("DISARMED"); }
            ["report"] => self.report(),
            ["focus"] => { if let Some(window) = &self.window { window.focus_window(); } }
            ["quit"] => { self.release(); self.report(); event_loop.exit(); }
            [] => (),
            _ => self.log("ERROR commands: case NAME; move X Y; capture on|off; arm; disarm; report; focus; quit"),
        }
    }
    fn move_pointer(&mut self, at: Point) {
        // CGWarpMouseCursorPosition need not emit CursorMoved. Never reuse a
        // previous case's sample or fabricate native evidence from the target.
        self.armed = false;
        self.last = None;
        self.sampled = None;
        self.previous = None;
        self.position_hint = (at.0 as f64, at.1 as f64);
        let result = self.pointer.move_pointer(at);
        self.log(format_args!(
            "MOVE requested={at:?} result={result:?} native_sample_invalidated=true"
        ));
    }
    fn sample(&mut self, at: (f64, f64)) {
        self.last = Some(at);
        self.position_hint = at;
        self.sampled = Some(Instant::now());
        let observed = self.armed && self.focused && self.inside && !self.captured;
        let allowed = self.region.as_ref().is_none_or(|r| {
            inside(r.bounds, at) && !r.exclusions.iter().any(|&hole| inside(hole, at))
        });
        let candidate = observed
            && self.previous.is_some_and(|prev| {
                self.region
                    .as_ref()
                    .is_some_and(|r| r.exclusions.iter().any(|&hole| crosses(prev, at, hole)))
            });
        if observed {
            self.samples += 1;
            self.violations += u64::from(!allowed);
            self.crossing_candidates += u64::from(candidate);
        }
        self.log(format_args!("CURSOR x={:.3} y={:.3} allowed={allowed} segment_through_hole={candidate} measured={observed} native_confined={}", at.0, at.1, self.update.confined));
        self.previous = Some(at);
    }
}

// Native positions can have subpixel coordinates. One physical pixel is
// represented by the full half-open pixel interval, not a rounded point.
fn inside([x, y, w, h]: [i32; 4], (px, py): (f64, f64)) -> bool {
    w > 0
        && h > 0
        && px >= x as f64
        && py >= y as f64
        && px < x as f64 + w as f64
        && py < y as f64 + h as f64
}

// Liang-Barsky clipping identifies straight endpoint segments through holes.
// This is a trace-review candidate, never a failure: the native backend can
// slide around a corner within a single event, a path endpoints cannot prove.
// Use the open interior so boundary tangents are not candidates.
fn crosses(a: (f64, f64), b: (f64, f64), [x, y, w, h]: [i32; 4]) -> bool {
    if w <= 0 || h <= 0 {
        return false;
    }
    let mut low: f64 = 0.0;
    let mut high: f64 = 1.0;
    for (origin, delta, min, max) in [
        (a.0, b.0 - a.0, x as f64, x as f64 + w as f64),
        (a.1, b.1 - a.1, y as f64, y as f64 + h as f64),
    ] {
        if delta == 0.0 {
            if origin <= min || origin >= max {
                return false;
            }
        } else {
            let t0 = (min - origin) / delta;
            let t1 = (max - origin) / delta;
            low = low.max(t0.min(t1));
            high = high.min(t0.max(t1));
            if low >= high {
                return false;
            }
        }
    }
    high > 0.0 && low < 1.0
}

impl ApplicationHandler<String> for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("pointer QA: None")
            .with_inner_size(PhysicalSize::new(800, 600))
            .with_resizable(false)
            .with_position(PhysicalPosition::new(80, 80));
        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window = Arc::new(window);
                self.pointer.attach(window.clone());
                self.focused = window.has_focus();
                self.log(format_args!(
                    "READY size={:?} inner_origin={:?} outer_origin={:?} scale={} focused={}",
                    window.inner_size(),
                    window.inner_position(),
                    window.outer_position(),
                    window.scale_factor(),
                    self.focused
                ));
                self.window = Some(window);
            }
            Err(error) => {
                self.log(format_args!("FATAL create window: {error}"));
                event_loop.exit();
            }
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, command: String) {
        self.command(&command, event_loop);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.sample((position.x, position.y));
                self.update();
            }
            WindowEvent::CursorEntered { .. } => {
                self.inside = true;
                self.log("ENTER");
                self.update();
            }
            WindowEvent::CursorLeft { .. } => {
                self.inside = false;
                self.log("LEAVE");
                self.update();
            }
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                self.log(format_args!("FOCUS {focused}"));
                if !focused {
                    self.release();
                } else {
                    self.update();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.log(format_args!("BUTTON {button:?} {state:?}"))
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.log(format_args!("KEY {:?}", event.logical_key));
                if event.logical_key == Key::Named(NamedKey::Escape) {
                    self.release();
                    self.report();
                    event_loop.exit();
                }
            }
            WindowEvent::Moved(position) => self.log(format_args!("WINDOW_MOVED {position:?}")),
            WindowEvent::Resized(size) => self.log(format_args!("RESIZED {size:?}")),
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                self.release();
                self.report();
                event_loop.exit();
            }
            _ => (),
        }
    }
    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.log(format_args!(
                "RAW dx={:.3} dy={:.3} captured={}",
                delta.0, delta.1, self.captured
            ));
        }
    }
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        self.update();
    }
    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.release();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().skip(1).collect::<Vec<_>>() != ["--run"] {
        println!(
            "Run on an isolated test desktop: pointer_qa --run\nCommands on stdin: case bounds|horizontal|vertical|divider|strip|box|none; arm; report; capture on|off; move X Y; focus; disarm; quit"
        );
        return Ok(());
    }
    let event_loop = EventLoop::<String>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    std::thread::Builder::new()
        .name("pointer-qa-commands".into())
        .spawn(move || {
            for line in io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                if proxy.send_event(line).is_err() {
                    return;
                }
            }
            let _ = proxy.send_event("quit".into());
        })?;
    event_loop.run_app(&mut Probe::new())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intentional_placement_cannot_create_or_reuse_native_evidence() {
        let mut probe = Probe::new();
        probe.sample((200.0, 100.0));
        probe.armed = true;
        probe.move_pointer((600, 400));
        assert!(!probe.armed);
        assert!(probe.last.is_none());
        assert!(probe.sampled.is_none());
        assert!(probe.previous.is_none());
        probe.sample((601.0, 400.0));
        assert_eq!(probe.last, Some((601.0, 400.0)));
        assert_eq!(probe.samples, 0);
    }
    #[test]
    fn native_subpixel_positions_and_single_pixel_axis() {
        assert!(inside([400, 80, 1, 440], (400.75, 100.5)));
        assert!(!inside([400, 80, 1, 440], (401.0, 100.5)));
        assert!(!inside([400, 80, 1, 440], (399.99, 100.5)));
    }
    #[test]
    fn segment_candidates_distinguish_hole_opening_and_boundary() {
        let holes = Case::Divider.region().unwrap().exclusions;
        assert!(crosses((200.0, 200.0), (600.0, 200.0), holes[0]));
        assert!(!crosses((200.0, 300.0), (600.0, 300.0), holes[0]));
        assert!(!crosses((200.0, 260.0), (600.0, 260.0), holes[0]));
        assert!(!crosses((389.0, 100.0), (389.0, 500.0), holes[0]));
        assert!(crosses((400.0, 100.0), (400.0, 500.0), holes[0]));
    }
}
