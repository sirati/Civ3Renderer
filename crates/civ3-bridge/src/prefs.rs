//! The renderer's own preferences, as tabs of the game's Preferences screen:
//! "CivIII" (the game's own preferences, mirrored as they are), "Resolution"
//! and "Layout" / "Zoom" (the viewer's settings, `viewer_settings::Settings`). The
//! tabs and their content are a panel of the menu crate
//! (`assets/civ3/preferences.toml`) laid over the mirrored screen; what the
//! panel takes of the pointer input never reaches the game.
//!
//! The bridge keeps the settings: loaded at start from
//! `Settings::default_path()`, saved there when the user accepts the draft,
//! and sent to the viewer with every scene (`scene::Scene::settings`; only
//! changes go over the wire).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

use civ3_sys::ui_hooks::PreferencesForm;
use menu::panel::{Draw, PanelEvent, PanelState, Panels, TextMetrics, TextStyle, Value};
use scene::Input;
use viewer_settings::Settings;
use viewer_settings::controls::ControlValue;

use crate::text::{Fonts, TextRun};
use crate::ui::{Frame, ScreenDraw};
use crate::ui_screen::Item;

mod native;
mod delivery;
#[cfg(windows)]
pub(crate) use delivery::handle_message;

/// The panel (`menu::panel` format).
pub const PANELS: &str = include_str!("../../../assets/civ3/preferences.toml");
/// Its panel, laid over the screen's background.
const PANEL: &str = "preferences";
const DISCARD_PANEL: &str = "preferences_discard";
const ACTION: &str = "dialog.action";
const UNDO: &str = "dialog.undo";

/// The only controls that wait for Accept: they size menus, the open
/// Preferences window among them, so a drag would move its own track and
/// change native pointer mapping. Every other control previews at once.
/// Base UI scale previews behind the dialog with its Preview button
/// ([`PREVIEW_BASE`]); the dialog keeps the accepted size.
const DEFERRED_CONTROLS: &[&str] = &["resolution.base_ui_scale", "resolution.scale_menus", "resolution.menu_scale_percent"];
/// The Base UI scale Preview button's actions (`dialog.action`) and the
/// value showing it is on.
const PREVIEW_BASE: &str = "preview_base";
const STOP_PREVIEW_BASE: &str = "stop_preview_base";
const PREVIEWING_BASE: &str = "dialog.previewing_base";
/// On (a switch value) when the game runs on macOS: macOS-only settings
/// show only then (`show_if`).
const HOST_MACOS: &str = "host.macos";

/// The value naming the tab shown, and its options.
pub const TAB: &str = "tab";
pub const TAB_GAME: &str = "civ3";
pub const TAB_RESOLUTION: &str = "resolution";
pub const TAB_LAYOUT: &str = "layout";
pub const TAB_ZOOM: &str = "zoom";
pub const TAB_CLEAN_MAP: &str = "clean_map";
pub const TAB_ACCESSIBILITY: &str = "accessibility";

/// The values the panel shows: the tab, every control of the settings
/// (`viewer_settings::controls`) by its key, the rendered and the window
/// size (`resolution.realised`, `resolution.window`, as "W×H") for a window
/// of `window` physical pixels, and the pixel scale in effect
/// (`resolution.effective`, e.g. "×3").
pub fn values(settings: &Settings, tab: &str, window: (u32, u32)) -> BTreeMap<String, Value> {
    let mut out: BTreeMap<String, Value> = settings.controls().into_iter().map(|(k, v)| (k, to_value(v))).collect();
    out.insert(TAB.into(), Value::Text(tab.into()));
    let (w, h) = settings.resolution.realised(window);
    out.insert("resolution.realised".into(), Value::Text(format!("{w}×{h}")));
    out.insert("resolution.window".into(), Value::Text(format!("{}×{}", window.0, window.1)));
    let r = &settings.resolution;
    let mut effective = r.describe_pixel_scale();
    if r.pixel_scale_in(window) < r.effective_pixel_scale() {
        effective = format!("{effective}, ×{} in this small window", r.pixel_scale_in(window));
    }
    out.insert("resolution.effective".into(), Value::Text(effective));
    out.insert("accessibility.hud_percent".into(), Value::Number(settings.accessibility.hud_scale() * 100.0));
    out.insert("accessibility.arrow_percent".into(), Value::Number(settings.accessibility.arrow_scale() * 100.0));
    for (rgb, name) in viewer_settings::accessibility::PALETTE.iter().zip(["Red", "Orange", "Gold", "Bright green", "Teal", "Cyan", "Blue", "Purple", "Magenta", "Pink", "Brown", "Sand", "White", "Silver", "Charcoal", "Black"]) {
        out.insert(format!("accessibility.color_name.{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]), Value::Text(name.into()));
    }
    out.insert("layout.zoom_controls".into(), Value::On(settings.layout.zoom != viewer_settings::Zoom::Off));
    out.insert("layout.zoom_lens_controls".into(), Value::On(!matches!(settings.layout.zoom, viewer_settings::Zoom::Off | viewer_settings::Zoom::TopStrip)));
    // Why the zoom's place in a vertical split is greyed out, if it is.
    let why = match settings.layout.zoom {
        viewer_settings::Zoom::Off => "The zoom is off",
        viewer_settings::Zoom::Beside => "The box beside the mouse goes wherever the mouse is",
        _ if !settings.layout.split.is_vertical() => "Only with a vertical split (Layout tab)",
        _ => "",
    };
    out.insert("layout.zoom_place_why".into(), Value::Text(why.into()));
    let magnification = viewer_settings::Layout::MAGNIFICATIONS[settings.layout.zoom_magnification_step()];
    out.insert("layout.zoom_magnification_label".into(), Value::Text(format!("{}×", menu::panel::format_number(magnification, 1, false))));
    out
}

fn to_value(v: ControlValue) -> Value {
    match v {
        ControlValue::Choice(c) => Value::Text(c),
        ControlValue::On(b) => Value::On(b),
        ControlValue::Number(n) => Value::Number(n),
    }
}

/// What a panel event does to the settings and the tab shown: whether the
/// settings changed (a choice, toggle or slide of a setting), and the tab
/// chosen, if one was.
pub fn apply(settings: &mut Settings, event: &PanelEvent) -> (bool, Option<String>) {
    match event {
        PanelEvent::Chose { control, value } if control == TAB => (false, Some(value.clone())),
        PanelEvent::Chose { control, value } => (settings.set_control(control, &ControlValue::Choice(value.clone())), None),
        PanelEvent::Toggled { control, on } => (settings.set_control(control, &ControlValue::On(*on)), None),
        PanelEvent::Slid { control, value } => (settings.set_control(control, &ControlValue::Number(*value)), None),
    }
}

/// Where the game's Preferences screen is, if it is shown (`frame`'s
/// forms): the top-left of its background and the panel's size (x, y, w,
/// h). The background's draw tells where it is (its screen position less
/// its source offset); without one, the background is centred on the form's
/// canvas, as the game draws it (docs §16.11).
pub fn screen_area(frame: &Frame, form: &PreferencesForm, panel_size: (u32, u32)) -> Option<[i32; 4]> {
    let &(_, canvas, (ox, oy), (cw, ch)) = frame.forms.iter().rev().find(|f| f.0 == form.vtable)?;
    let (pw, ph) = (panel_size.0 as i32, panel_size.1 as i32);
    let file = |p: &str| p.replace('/', "\\").to_ascii_lowercase().rsplit('\\').next().map(str::to_owned);
    let background = file(form.background);
    let own: Vec<std::ops::Range<usize>> = frame.form_draws.iter().filter(|(c, _)| *c == canvas).map(|(_, r)| r.clone()).collect();
    let drawn = frame.draws.iter().enumerate().rev().find_map(|(i, d)| {
        let ScreenDraw::Image { path, rect, x, y, .. } = d else { return None };
        let ours = own.is_empty() || own.iter().any(|r| r.contains(&i));
        (ours && file(path) == background).then_some((x - rect[0], y - rect[1]))
    });
    let (x, y) = drawn.unwrap_or((ox + (cw - pw) / 2, oy + (ch - ph) / 2));
    Some([x, y, pw, ph])
}

/// Whether `frame` draws the game's Preferences screen (its form's canvas
/// has draws in it).
fn shows_screen(frame: &Frame, form: &PreferencesForm) -> Option<usize> {
    let &(_, canvas, ..) = frame.forms.iter().rev().find(|f| f.0 == form.vtable)?;
    frame.form_draws.iter().filter(|(c, r)| *c == canvas && !r.is_empty()).map(|(_, r)| r.end).max()
}

/// Where our tabs go among `frame`'s draws (the index of the first draw
/// over them), if `frame` draws the game's Preferences screen: over the
/// screen and its controls, under what the game draws over them afterwards —
/// a popup over the screen (its canvas `popup`, e.g. C3X's info box) with
/// the frame the game draws onto the screen just before compositing it (on
/// no form's canvas), the tooltip, the cursor.
pub fn tabs_slot(frame: &Frame, form: &PreferencesForm, popup: Option<u32>) -> Option<usize> {
    let screen_end = shows_screen(frame, form)?;
    let owned = |i: usize| frame.form_draws.iter().map(|(_, r)| r).chain(&frame.underlay_draws).any(|r| r.contains(&i));
    let popup_start = popup.and_then(|p| frame.form_draws.iter().find(|(c, _)| *c == p).map(|(_, r)| r.start));
    let mut at = popup_start.unwrap_or(frame.draws.len()).max(screen_end);
    while at > screen_end && !owned(at - 1) {
        at -= 1;
    }
    Some(at)
}

/// Whether the game's modal form `modal` is a popup over its Preferences
/// screen (e.g. C3X's info box): modal, and not the screen itself.
fn popup_over(modal: Option<u32>, form: &PreferencesForm) -> bool {
    modal.is_some_and(|m| m != 0 && m != form.form)
}

/// A native popup gesture keeps its release even if the press dismissed it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PopupRelease { Key(u32), Button(scene::Button) }

impl PopupRelease {
    fn pressed(input: &Input) -> Option<Self> {
        match *input {
            Input::Key { vk, down: true, .. } | Input::WorldKey { vk, down: true, .. } if vk != 0 && !matches!(vk, 0x10..=0x12) => Some(Self::Key(vk)),
            Input::MouseButton { button, down: true, .. } => Some(Self::Button(button)),
            _ => None,
        }
    }
    fn released(self, input: &Input) -> bool {
        match (self, input) {
            (Self::Key(key), Input::Key { vk, down: false, .. } | Input::WorldKey { vk, down: false, .. }) => key == *vk,
            (Self::Button(expected), Input::MouseButton { button, down: false, .. }) => expected == *button,
            _ => false,
        }
    }
    fn repeated(self, input: &Input) -> bool {
        matches!((self, input), (Self::Key(key), Input::Key { vk, down: true, .. } | Input::WorldKey { vk, down: true, .. }) if key == *vk)
    }
}

/// The Preferences screen's own close buttons. PCGames Parameters_Form open
/// 0x588370 creates both with `Button` 0x60AF70 at background (960, 730)
/// (`this+0x5A0`, id 0, the round Accept) and (990, 730) (`this+0xC74`, id
/// -1, the X Cancel); their callback 0x587E70 sets the same result byte
/// (+0xEC61) as Return and Escape in 0x587EA0 before closing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeButton { Accept, Cancel }

impl NativeButton {
    const ACCEPT_OFFSET: u32 = 0x5A0;
    const CANCEL_OFFSET: u32 = 0xC74;

    fn of(form: u32, id: u64) -> Option<Self> {
        if id == u64::from(form + Self::ACCEPT_OFFSET) { Some(Self::Accept) }
        else if id == u64::from(form + Self::CANCEL_OFFSET) { Some(Self::Cancel) }
        else { None }
    }

    fn action(self) -> &'static str {
        match self { Self::Accept => "accept", Self::Cancel => "cancel" }
    }
}

fn contains([x, y, w, h]: [i32; 4], (px, py): (i32, i32)) -> bool {
    px >= x && py >= y && px < x + w && py < y + h
}

fn interactive(input: &Input) -> bool {
    matches!(input, Input::Key { .. } | Input::WorldKey { .. } | Input::MouseMove { .. }
        | Input::MouseButton { .. } | Input::WorldMove { .. } | Input::WorldButton { .. }
        | Input::Wheel { .. } | Input::Slider { .. } | Input::UnitOrder { .. })
}

/// The renderer's preferences: the settings, and the tabs over the game's
/// Preferences screen while it is open.
pub struct Prefs {
    panels: Option<Panels>,
    state: PanelState,
    settings: Settings,
    effective: Settings,
    preview: Option<scene::SettingsPreview>,
    preview_session: u64,
    preview_id: u64,
    /// Changes shown by the dialog. All but [`DEFERRED_CONTROLS`] preview before OK.
    draft: Option<Settings>,
    /// The Base UI scale Preview button is on: the draft's base previews
    /// behind the dialog, which keeps its size.
    base_preview: bool,
    native_original: Option<native::Settings>,
    native_current: Option<native::Settings>,
    native_pending: bool,
    native_mouse_held: bool,
    native_delivery: delivery::Delivery,
    /// The game's own round Accept and X Cancel buttons (screen rectangles,
    /// hit-test order), as last enumerated, and the one a left press began on.
    /// Their clicks run the draft transaction instead of the native close.
    native_buttons: Vec<([i32; 4], NativeButton)>,
    native_button_press: Option<NativeButton>,
    /// The Preferences form instance while it is open.
    form: Option<u32>,
    /// Discard confirmation is modal over our tabs and the native controls.
    confirming_discard: bool,
    /// A native close was sent; do not accept or save again before it closes.
    closing: bool,
    #[cfg_attr(not(any(windows, test)), allow(dead_code))]
    /// Accept was chosen: the round button's own callback (0x587E70, id 0)
    /// still has to run on the game's thread. Native Return would close
    /// without committing Civ III options or audio, so it is never sent.
    native_accept: bool,
    /// Swallow repeats/releases even when a key changes the dialog state.
    dialog_keys: BTreeSet<u32>,
    path: PathBuf,
    /// The tab shown.
    tab: String,
    /// Where the game's Preferences screen is while it is open (see
    /// [`screen_area`]).
    open: Option<[i32; 4]>,
    /// The viewer's window in physical pixels (`Input::Window`; the
    /// rendered size depends on it), or before it told it, the size it
    /// arranges the game's screen in.
    window: (u32, u32),
    /// A popup of the game is open over the screen (e.g. C3X's info box):
    /// the input is all the game's, the popup's buttons may lie over our
    /// tabs and panels.
    covered: bool,
    /// Input after a popup action waits for its delivered native modal state.
    covered_waiting: bool,
    covered_releases: Vec<PopupRelease>,
    deferred_inputs: VecDeque<Input>,
    /// Since when input has waited behind the popup fence: after
    /// [`DEFERRED_STALE`] its presses are dropped, never replayed late.
    deferred_since: Option<std::time::Instant>,
    /// Presses dropped that way; their releases are swallowed too.
    stale_releases: Vec<PopupRelease>,
    last_modal: Option<u32>,
    /// Whether Ctrl and Shift are held (for the reset, [`RESET_KEY`]).
    ctrl: bool,
    shift: bool,
    /// Whether the renderer told its window (`Input::Window`) rather than
    /// the driver's screen only.
    native: bool,
    /// The first run's Preferences are still to be shown
    /// (`Settings::first_run`): they open on the Resolution tab.
    first_run: bool,
    /// The game runs on macOS (under Wine), which shows the settings only
    /// it has (`HOST_MACOS`).
    host_macos: bool,
}

/// Where the renderer's settings are kept: `CIV3R_SETTINGS` (a Windows path,
/// or under Wine a Unix one too), else under Wine the Linux user's (as
/// `viewer_settings::Settings::default_path` finds them there, e.g.
/// `~/.config/civ3r/settings.toml`: a native viewer reads the same file),
/// else Windows' (`%APPDATA%\civ3r`).
#[cfg(windows)]
pub fn settings_path() -> PathBuf {
    let unix = |p: &str| p.starts_with('/').then(|| inproc::wine::dos_path(std::path::Path::new(p))).flatten();
    if let Ok(p) = std::env::var("CIV3R_SETTINGS") {
        return unix(&p).unwrap_or_else(|| PathBuf::from(p));
    }
    if inproc::wine::is_wine() {
        let base = std::env::var("XDG_CONFIG_HOME").ok().filter(|p| p.starts_with('/')).or_else(|| std::env::var("HOME").ok().filter(|h| h.starts_with('/')).map(|h| format!("{h}/.config")));
        if let Some(path) = base.and_then(|b| unix(&format!("{b}/civ3r/settings.toml"))) {
            return path;
        }
        // (Wine keeps the Unix HOME from Windows programs: it gives it as
        // WINEHOMEDIR, `\??\Z:\home\me`.)
        if let Some(home) = std::env::var("WINEHOMEDIR").ok().map(|h| h.trim_start_matches(r"\??\").to_owned()).filter(|h| h.as_bytes().get(1) == Some(&b':')) {
            return PathBuf::from(format!(r"{home}\.config\civ3r\settings.toml"));
        }
    }
    Settings::default_path()
}

/// Whether the game runs on macOS (Wine on Darwin).
fn host_macos() -> bool {
    #[cfg(windows)]
    { inproc::wine::on_macos() }
    #[cfg(not(windows))]
    { cfg!(target_os = "macos") }
}

/// How long input may wait behind the popup fence (the game thread has not
/// delivered it) before its presses are dropped instead of replayed late.
const DEFERRED_STALE: std::time::Duration = std::time::Duration::from_millis(crate::stale_input::STALE_AFTER_MS as u64);

fn is_modifier(input: &Input) -> bool {
    matches!(input, Input::Key { vk: 0x10..=0x12, .. } | Input::WorldKey { vk: 0x10..=0x12, .. })
}

const VK_BACK: u32 = 0x08;
const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
/// The key that, with Ctrl and Shift, resets the layout and resolution
/// settings (Backspace).
pub const RESET_KEY: u32 = VK_BACK;

impl Prefs {
    /// The settings saved at `path` (the defaults if there are none), and
    /// the panel.
    pub fn load(path: PathBuf) -> Self {
        // (An unreadable file is not a first run: nothing saves over it
        // unasked.)
        let settings = Settings::load(&path).unwrap_or_else(|e| {
            log::warn!("settings: {e}; using the defaults (saved when changes are accepted)");
            Settings { first_run: viewer_settings::FirstRun::done(), ..Settings::default() }
        });
        let first_run = !settings.first_run.preferences_shown;
        if first_run {
            log::info!("settings: first run, Preferences open on the Resolution tab once the title menu is up");
        }
        log::info!("settings: {}", path.display());
        let panels = Panels::parse(PANELS).inspect_err(|e| log::error!("preferences panel: {e}")).ok();
        Prefs { panels, state: PanelState::default(), effective: settings.clone(), settings, preview: None, preview_session: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64, preview_id: 0, draft: None, base_preview: false, native_original: None, native_current: None, native_pending: false, native_mouse_held: false, native_delivery: delivery::Delivery::default(), native_buttons: Vec::new(), native_button_press: None, form: None, confirming_discard: false, closing: false, native_accept: false, dialog_keys: BTreeSet::new(), path, tab: TAB_GAME.into(), open: None, window: (0, 0), covered: false, covered_waiting: false, covered_releases: Vec::new(), deferred_inputs: VecDeque::new(), deferred_since: None, stale_releases: Vec::new(), last_modal: None, ctrl: false, shift: false, native: false, first_run, host_macos: host_macos() }
    }

    /// Whether the first run's Preferences are still to be shown (the
    /// title menu should open them, `crate::first_run`).
    pub fn first_run_pending(&self) -> bool {
        self.first_run
    }

    /// The settings (sent to the viewer with every scene).
    pub fn settings(&self) -> &Settings {
        &self.effective
    }

    pub fn settings_preview(&self) -> Option<scene::SettingsPreview> { self.preview }

    /// The settings the game shows while Preferences is open: the whole
    /// draft, except [`DEFERRED_CONTROLS`] at their accepted values. While
    /// the Base UI scale preview is on, the frame and world use the draft's
    /// base and menus, the dialog among them, keep the accepted one.
    fn project_settings(&mut self) {
        let Some(draft) = &self.draft else {
            self.base_preview = false;
            self.effective = self.settings.clone();
            return;
        };
        let accepted = &self.settings.resolution;
        let mut effective = draft.clone();
        effective.first_run = self.settings.first_run.clone();
        let r = &mut effective.resolution;
        r.scale_menus = accepted.scale_menus;
        r.menu_scale_percent = accepted.menu_scale_percent;
        if self.base_preview {
            r.menu_base_ui_scale = Some(accepted.effective_base_ui_scale());
        } else {
            r.base_ui_scale = accepted.base_ui_scale;
        }
        self.effective = effective;
    }

    fn preview_multiplier(&mut self, previous: u32) {
        let Some(draft) = &self.draft else { return };
        if draft.resolution.pixel_scale != previous {
            self.state = PanelState::default();
            self.preview_id = self.preview_id.wrapping_add(1);
            self.preview = Some(scene::SettingsPreview { session: self.preview_session, id: self.preview_id, pixel_scale: draft.resolution.pixel_scale, previous_pixel_scale: previous });
        }
    }

    /// Render description entries for the panel's generated art.
    pub fn description() -> String {
        Panels::parse(PANELS).map(|p| p.description()).unwrap_or_default()
    }

    /// Whether the game's Preferences screen is open (as last seen).
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Native Preferences background in the driver's coordinate space.
    pub fn screen_rect(&self) -> Option<[i32; 4]> { self.open }

    /// Takes the dialog's input. Edits stay in its draft until Accept;
    /// cancelling a changed draft first asks whether to discard it.
    pub fn route(&mut self, inputs: Vec<Input>, window: (u32, u32)) -> Vec<Input> {
        self.route_at(inputs, window, std::time::Instant::now())
    }

    fn route_at(&mut self, inputs: Vec<Input>, window: (u32, u32), now: std::time::Instant) -> Vec<Input> {
        self.follow_window(window);
        if self.deferred_since.is_some_and(|since| now.saturating_duration_since(since) > DEFERRED_STALE) {
            self.drop_stale_deferred(now);
        }
        let mut out = Vec::new();
        let mut queued = std::mem::take(&mut self.deferred_inputs);
        queued.extend(inputs);
        for input in queued {
            if let Input::SettingsPreviewDecision { session, id, keep } = input {
                if let Some(p) = self.preview.filter(|p| p.session == session && p.id == id) {
                    if !keep { if let Some(draft) = &mut self.draft { draft.resolution.pixel_scale = p.previous_pixel_scale; } }
                    self.preview = None;
                    self.state = PanelState::default();
                    self.project_settings();
                    self.state.values = self.values();
                }
                continue;
            }
            if self.preview.is_some() && interactive(&input) {
                if let Input::Key { vk, down, .. } | Input::WorldKey { vk, down, .. } = input {
                    if matches!(vk, 16..=18) {
                        if vk == VK_CONTROL { self.ctrl = down; }
                        if vk == VK_SHIFT { self.shift = down; }
                        out.push(input);
                    }
                }
                continue;
            }
            // The release (or key repeat) of a press dropped as stale.
            if let Some(index) = self.stale_releases.iter().position(|release| release.released(&input)) {
                self.stale_releases.remove(index);
                continue;
            }
            if self.stale_releases.iter().any(|release| release.repeated(&input)) { continue; }
            if let Some(pressed) = PopupRelease::pressed(&input) {
                // A new press of a button whose release was never seen.
                self.stale_releases.retain(|release| *release != pressed);
            }
            match input {
                Input::Window { width, height, scale_milli } => {
                    // The first run's sizes, at the first window it tells
                    // (saved with the first run's marker).
                    if self.first_run && !self.native {
                        self.settings.resolution.seed_for_desktop(f64::from(scale_milli) / 1000.0, height);
                        log::info!("settings: first run on a desktop at {scale_milli}‰: {:?}", self.settings.resolution);
                        if let Some(draft) = &mut self.draft { draft.resolution = self.settings.resolution.clone(); }
                        self.project_settings();
                    }
                    (self.window, self.native) = ((width, height), true);
                }
                Input::Resized { width, height } => self.follow_window((width, height)),
                _ => {}
            }
            if self.open.is_some() && interactive(&input) {
                if (!self.covered || self.covered_waiting) && self.covered_releases.iter().any(|release| release.repeated(&input)) { continue; }
                if let Some(index) = self.covered_releases.iter().position(|release| release.released(&input)) {
                    self.covered_releases.remove(index);
                    if let Input::MouseButton { button: scene::Button::Left, down, .. } = input { self.native_mouse_held = down; }
                    if self.covered_waiting || self.covered {
                        self.covered_waiting = true;
                        self.native_delivery.edited();
                    }
                    out.push(input);
                    continue;
                }
                if self.covered_waiting {
                    // Keep the newest adjacent motion while a native queue
                    // is stalled, without discarding buttons or key events.
                    if matches!(input, Input::MouseMove { .. }) && self.deferred_inputs.back().is_some_and(|old| matches!(old, Input::MouseMove { .. })) {
                        self.deferred_inputs.pop_back();
                    }
                    self.deferred_inputs.push_back(input);
                    self.deferred_since.get_or_insert(now);
                    continue;
                }
            }
            // Keep the recovery shortcut available outside Preferences too.
            // Inside the dialog it resets the draft, like any other edit.
            if let Input::Key { vk, down, .. } | Input::WorldKey { vk, down, .. } = input {
                match vk {
                    VK_CONTROL => self.ctrl = down,
                    VK_SHIFT => self.shift = down,
                    VK_BACK if self.ctrl && self.shift => {
                        if down && !self.closing { self.reset(); }
                        continue;
                    }
                    _ => {}
                }
            }
            if let Input::Key { vk, down, .. } | Input::WorldKey { vk, down, .. } = input {
                if (!down && self.dialog_keys.remove(&vk)) || (down && self.dialog_keys.contains(&vk)) { continue; }
            }
            if self.open.is_some() && self.covered && interactive(&input) {
                // A popup can disappear on either a press or a release.
                // Forward its gesture, then inspect delivered modal state
                // before routing another independent action.
                let pressed = PopupRelease::pressed(&input);
                let boundary = pressed.is_some() || matches!(input, Input::MouseButton { down: false, .. } | Input::Wheel { .. }
                    | Input::Key { vk: 0, down: true, .. } | Input::WorldKey { vk: 0, down: true, .. });
                if boundary {
                    self.covered_waiting = true;
                    if let Some(pressed) = pressed {
                        if !self.covered_releases.contains(&pressed) { self.covered_releases.push(pressed); }
                    }
                    self.native_delivery.edited();
                }
                if let Input::MouseButton { button: scene::Button::Left, down, .. } = input { self.native_mouse_held = down; }
                out.push(input);
                continue;
            }
            let Some(area) = self.open.filter(|_| !self.covered) else {
                out.push(input);
                continue;
            };
            // Window/scene metadata and application shutdown still belong to
            // the driver. Only dialog interaction is blocked by the modal.
            if !interactive(&input) {
                out.push(input);
                continue;
            }
            if self.closing { continue; }
            if let Input::Key { vk, down, .. } | Input::WorldKey { vk, down, .. } = input {
                let action = if self.confirming_discard {
                    match vk {
                        0x59 => Some("discard"), // Y
                        0x4E | VK_ESCAPE | VK_RETURN => Some("keep"), // N, Escape, Enter
                        _ => None,
                    }
                } else {
                    match vk {
                        VK_ESCAPE => Some("cancel"),
                        VK_RETURN => Some("accept"),
                        _ => None,
                    }
                };
                if let Some(action) = action {
                    if down {
                        self.dialog_keys.insert(vk);
                        self.action(action, &mut out);
                    }
                    continue;
                }
            }
            // The game's round Accept and X Cancel: a click (press and
            // release on the same button) runs the transaction, which closes
            // through the native Accept callback or Escape. The game sees neither
            // half, so it cannot close a dirty draft by itself.
            if let Input::MouseButton { button: scene::Button::Left, down, x, y } = input {
                let hit = self.native_buttons.iter().find(|(rect, _)| contains(*rect, (x, y))).map(|&(_, b)| b);
                if !down {
                    if let Some(pressed) = self.native_button_press.take() {
                        if hit == Some(pressed) && !self.confirming_discard { self.action(pressed.action(), &mut out); }
                        continue;
                    }
                } else if let Some(button) = hit.filter(|_| !self.confirming_discard) {
                    self.native_button_press = Some(button);
                    continue;
                }
            }
            self.state.values = self.values();
            let panel_name = if self.confirming_discard { DISCARD_PANEL } else { PANEL };
            let Some((panels, panel)) = self.panels.as_ref().and_then(|p| p.panel(panel_name).map(|panel| (p, panel))) else {
                out.push(input);
                continue;
            };
            let routed = panels.input(panel, &mut self.state, area, &input);
            for event in &routed.events {
                if let PanelEvent::Chose { control, value } = event {
                    if control == ACTION {
                        self.action(value, &mut out);
                        continue;
                    }
                    if control == UNDO {
                        self.undo_slider(value);
                        continue;
                    }
                }
                if let Some(draft) = self.draft.as_mut() {
                    let previous = draft.resolution.pixel_scale;
                    let (_, tab) = apply(draft, event);
                    if let Some(tab) = tab {
                        log::debug!("preferences: tab {tab}");
                        if tab != self.tab { self.base_preview = false; }
                        self.tab = tab;
                    }
                    self.preview_multiplier(previous);
                    self.project_settings();
                }
            }
            if !routed.consumed && !self.confirming_discard {
                // A native change and Cancel may arrive in the same batch,
                // before the game has processed its forwarded input. Ask
                // conservatively until the next read confirms its values.
                let mutating = matches!(input,
                    Input::MouseButton { button: scene::Button::Left, .. }
                    | Input::Wheel { .. }
                    | Input::Slider { action: scene::SliderAction::Preview | scene::SliderAction::Cancel | scene::SliderAction::Step, .. }
                    | Input::Key { vk: 0x20..=0x28, down: true, .. }
                    | Input::WorldKey { vk: 0x20..=0x28, down: true, .. })
                    || (self.native_mouse_held && matches!(input, Input::MouseMove { .. }));
                if let Input::MouseButton { button: scene::Button::Left, down, .. } = input { self.native_mouse_held = down; }
                if mutating {
                    self.native_pending = true;
                    self.native_delivery.edited();
                }
                out.push(input);
            }
        }
        if self.deferred_inputs.is_empty() { self.deferred_since = None; }
        self.state.values = self.values();
        out
    }

    /// Input has waited behind an unacknowledged popup fence for longer
    /// than [`DEFERRED_STALE`] (the game thread is stalled): drop its
    /// presses, wheel turns and their releases rather than replay them in a
    /// burst later. Releases of presses the game already got, moves, slider
    /// transactions and metadata stay queued; the fence itself still holds.
    fn drop_stale_deferred(&mut self, now: std::time::Instant) {
        let mut kept = VecDeque::new();
        let mut dropped = 0;
        for input in std::mem::take(&mut self.deferred_inputs) {
            if let Some(index) = self.stale_releases.iter().position(|release| release.released(&input)) {
                self.stale_releases.remove(index);
            } else if self.stale_releases.iter().any(|release| release.repeated(&input)) {
            } else if let Some(pressed) = PopupRelease::pressed(&input) {
                if !self.stale_releases.contains(&pressed) { self.stale_releases.push(pressed); }
            } else if matches!(input, Input::Wheel { .. } | Input::Key { down: true, .. } | Input::WorldKey { down: true, .. } if !is_modifier(&input)) {
            } else {
                kept.push_back(input);
                continue;
            }
            dropped += 1;
        }
        if dropped != 0 {
            log::info!("preferences: dropped {dropped} inputs that waited over {} ms for the game; not replayed late", DEFERRED_STALE.as_millis());
        }
        self.deferred_since = (!kept.is_empty()).then_some(now);
        self.deferred_inputs = kept;
    }

    fn editing(&self) -> &Settings {
        self.draft.as_ref().unwrap_or(&self.settings)
    }

    fn undo_slider(&mut self, key: &str) {
        // Only sliders can be undone, and the saved value is authoritative,
        // including values loaded from disk outside today's slider steps.
        let slider = self.panels.as_ref().and_then(|p| p.panel(PANEL)).and_then(|p| p.slider_range(key));
        if slider.is_none() { return; }
        let Some(ControlValue::Number(original)) = self.settings.control(key) else { return };
        if let Some(draft) = &mut self.draft {
            draft.restore_number_control(key, &self.settings);
        }
        self.state.values.insert(key.into(), Value::Number(original));
        self.project_settings();
    }

    fn dirty(&self) -> bool {
        self.draft.as_ref().is_some_and(|draft| draft != &self.settings)
            || self.native_pending
            || self.native_original.as_ref().zip(self.native_current.as_ref()).is_some_and(|(original, current)| original != current)
    }

    fn follow_native(&mut self, capture: impl FnOnce() -> Option<native::Settings>, opening: bool) {
        // Acquire the fence before reading memory. An acknowledgment that
        // arrives during a stale read only qualifies the next snapshot.
        let settled_before_read = self.native_delivery.settled();
        let current = capture();
        if opening { self.native_original = current.clone(); }
        if current.is_some() {
            self.native_current = current;
            if settled_before_read { self.native_pending = false; }
        }
    }

    /// Post this after the routed native messages, including key envelopes.
    /// Only its game-thread acknowledgment permits a snapshot to clear dirty.
    /// A chosen Accept follows the same input into the game's queue and
    /// runs the round button's callback there, once.
    #[cfg(windows)]
    pub fn native_inputs_posted(&mut self, window: Option<inproc::window::Window>) {
        let Some(window) = window else { return };
        if let Some(receipt) = self.native_delivery.receipt() {
            if delivery::post(window, receipt.clone()) { self.native_delivery.posted(&receipt); }
        }
        if self.native_accept && self.open.is_some() && delivery::post_accept(window) { self.native_accept = false; }
    }

    /// Accept is waiting to run natively (see [`Self::native_accept`]).
    #[cfg(test)]
    fn native_accept_pending(&self) -> bool { self.native_accept }

    fn save(&self) {
        match self.settings.save(&self.path) {
            Ok(()) => log::info!("settings saved: {:?}", self.settings),
            Err(e) => log::error!("saving the settings: {e}"),
        }
    }

    fn action(&mut self, action: &str, out: &mut Vec<Input>) {
        if self.closing || self.preview.is_some() { return; }
        match action {
            PREVIEW_BASE if !self.confirming_discard => self.base_preview = true,
            STOP_PREVIEW_BASE => self.base_preview = false,
            "accept" if !self.confirming_discard => {
                // Accept also keeps an Alt slider's current draft value.
                self.state.slider_edit = None;
                if let Some(draft) = self.draft.take() {
                    if draft != self.settings {
                        self.settings = draft;
                        self.save();
                    }
                }
                self.closing = true;
                self.state = PanelState::default();
                self.native_accept = true;
            }
            "cancel" if !self.confirming_discard => {
                self.base_preview = false;
                if self.dirty() {
                    self.confirming_discard = true;
                    self.state = PanelState::default();
                } else {
                    self.draft = None;
                    self.close(VK_ESCAPE, out);
                }
            }
            "discard" if self.confirming_discard => {
                self.draft = None;
                self.confirming_discard = false;
                self.close(VK_ESCAPE, out);
            }
            "keep" if self.confirming_discard => {
                self.confirming_discard = false;
                self.state = PanelState::default();
            }
            _ => {}
        }
        self.project_settings();
        self.state.values = self.values();
    }

    fn close(&mut self, vk: u32, out: &mut Vec<Input>) {
        self.closing = true;
        self.state = PanelState::default();
        // PCGames Parameters_Form::handle_key_down at 0x587EA0 cancels on
        // Escape. Accept never comes here: native Return closes before it
        // sets the result byte, so it would discard Civ III options.
        out.extend([true, false].map(|down| Input::Key { vk, down, text: None }));
    }

    /// Restore an unfinished Alt slider inside the draft, never on disk.
    fn cancel_slider_edit(&mut self) {
        let Some(panel) = self.panels.as_ref().and_then(|p| p.panel(PANEL)) else { return };
        let Some(event) = self.state.cancel_slider(panel) else { return };
        if let Some(draft) = self.draft.as_mut() { apply(draft, &event); }
    }

    fn reset(&mut self) {
        self.cancel_slider_edit();
        let settings = self.draft.as_mut().unwrap_or(&mut self.settings);
        let before = settings.clone();
        let previous = before.resolution.pixel_scale;
        settings.layout = viewer_settings::Layout::default();
        settings.resolution = viewer_settings::Resolution::default();
        let changed = *settings != before;
        log::warn!("settings: layout and resolution reset to defaults (Ctrl+Shift+Backspace)");
        if changed && self.draft.is_none() { self.save(); }
        self.preview_multiplier(previous);
        self.project_settings();
    }

    /// Follows the game's screens: whether its Preferences screen is shown
    /// in `frame`, and where, and whether a popup is over it (`modal`: the
    /// game's modal form). It opens on the game's own tab, with nothing
    /// pressed.
    #[cfg(test)]
    fn update(&mut self, frame: Option<&Frame>, form: Option<PreferencesForm>, window: (u32, u32), modal: Option<u32>) {
        self.update_with_modal(frame, form, window, || Some(modal));
    }

    /// The outer Option distinguishes an unreadable modal pointer from a
    /// successful read saying no modal is open. Fence acquire precedes it.
    pub fn update_with_modal(&mut self, frame: Option<&Frame>, form: Option<PreferencesForm>, window: (u32, u32), capture_modal: impl FnOnce() -> Option<Option<u32>>) -> Option<u32> {
        let delivered_before_read = self.native_delivery.settled();
        let captured_modal = capture_modal();
        let modal = captured_modal.unwrap_or(self.last_modal);
        if captured_modal.is_some() { self.last_modal = modal; }
        self.follow_window(window);
        let size = self.panels.as_ref().and_then(|p| p.panel(PANEL)).map(|p| p.size()).unwrap_or((1024, 768));
        let open = frame.zip(form).and_then(|(f, form)| screen_area(f, &form, size));
        let opening = open.is_some() && self.open.is_none();
        if open.is_some() != self.open.is_some() {
            self.cancel_slider_edit();
            log::info!("preferences screen {}", if open.is_some() { "open: our tabs over it" } else { "closed" });
            self.state = PanelState::default();
            self.tab = TAB_GAME.into();
            // The first run's: on the Resolution tab, and never again
            // (saved now, whatever the user does with them).
            if opening && self.first_run {
                self.first_run = false;
                self.tab = TAB_RESOLUTION.into();
                self.settings.first_run.preferences_shown = true;
                self.save();
            }
            self.draft = open.map(|_| self.settings.clone());
            self.base_preview = false;
            self.preview = None;
            if opening {
                self.preview_session = self.preview_session.wrapping_add(1);
                self.preview_id = 0;
            }
            self.project_settings();
            self.confirming_discard = false;
            self.closing = false;
            self.native_accept = false;
            self.native_original = None;
            self.native_current = None;
            self.native_pending = false;
            self.native_mouse_held = false;
            self.native_delivery = delivery::Delivery::default();
            self.native_buttons.clear();
            self.native_button_press = None;
            self.covered_waiting = false;
            self.covered_releases.clear();
            self.deferred_inputs.clear();
            self.deferred_since = None;
        }
        if self.covered_waiting && delivered_before_read && captured_modal.is_some() {
            self.covered_waiting = false;
        }
        if open.is_some() {
            self.follow_native(|| form.and_then(|form| native::read(&civ3_sys::controls::Live, form)), opening);
        }
        self.open = open;
        self.form = open.and(form.map(|f| f.form));
        let covered = open.is_some() && form.is_some_and(|f| popup_over(modal, &f));
        if covered != self.covered {
            self.cancel_slider_edit();
            self.native_button_press = None;
            log::info!("preferences screen: {}", if covered { "a popup over it, the input is the game's" } else { "no popup over it" });
            self.state = PanelState { values: std::mem::take(&mut self.state.values), ..PanelState::default() };
        }
        self.covered = covered;
        self.project_settings();
        self.state.values = self.values();
        modal
    }

    /// The size the viewer arranges the game's screen in (`Input::Resized`),
    /// for the readout until it tells its window's (`Input::Window`).
    fn follow_window(&mut self, window: (u32, u32)) {
        if window != (0, 0) && !self.native {
            self.window = window;
        }
    }

    fn values(&self) -> BTreeMap<String, Value> {
        let mut values = values(self.editing(), &self.tab, self.window);
        values.insert(PREVIEWING_BASE.into(), Value::On(self.base_preview));
        values.insert(HOST_MACOS.into(), Value::On(self.host_macos));
        for (key, accepted) in self.settings.controls() {
            if matches!(accepted, ControlValue::Number(_)) {
                values.insert(format!("dialog.changed.{key}"), Value::On(self.editing().control(&key).as_ref() != Some(&accepted)));
            }
        }
        let mut changes = changed_controls(&self.settings, self.editing());
        if let Some((original, current)) = self.native_original.as_ref().zip(self.native_current.as_ref()) {
            changes.extend(original.changes(current));
        }
        if self.native_pending { changes.push("Civ III settings are still updating".into()); }
        values.insert("dialog.change_count".into(), Value::Text(changes.len().to_string()));
        for index in 0..6 {
            let text = if index == 5 && changes.len() > 6 {
                format!("... and {} more changes", changes.len() - 5)
            } else { changes.get(index).cloned().unwrap_or_default() };
            values.insert(format!("dialog.change{index}"), Value::Text(text));
        }
        values
    }

    /// What the panel draws now (nothing while the screen is closed), text
    /// measured with `fonts` where they are loaded.
    pub fn draws(&self, fonts: &Fonts) -> Vec<Draw> {
        let (Some(area), Some(panels)) = (self.open, self.panels.as_ref()) else { return Vec::new() };
        let Some(panel) = panels.panel(PANEL) else { return Vec::new() };
        let mut draws = panels.draw(panel, &self.state, area, &measurer(fonts));
        if self.confirming_discard {
            if let Some(confirm) = panels.panel(DISCARD_PANEL) {
                draws.extend(panels.draw(confirm, &self.state, area, &measurer(fonts)));
            }
        }
        draws
    }

    /// Keep native targets only where the visible panel leaves them
    /// reachable. This also removes their alternate hover draws before
    /// they can be painted over the panel by the viewer.
    /// It also finds the game's own Accept and Cancel buttons among them,
    /// which stay visible and clickable but run the draft transaction.
    pub fn retain_native_controls(&mut self, controls: &mut Vec<civ3_sys::controls::Control>) {
        let (Some(area), Some(panels), false) = (self.open, self.panels.as_ref(), self.covered) else { return };
        if let Some(form) = self.form {
            self.native_buttons = controls.iter().filter_map(|c| NativeButton::of(form, c.id).map(|b| (c.rect, b))).collect();
        }
        if self.confirming_discard { controls.clear(); return; }
        let Some(panel) = panels.panel(PANEL) else { return };
        controls.retain(|c| !panels.covers_any(panel, &self.state, area, c.rect));
    }

    /// The panel's controls (topmost first) with their draws now and in
    /// their other states (`menu::panel::Panels::looks`): things to point
    /// at that the viewer shows hovered or pressed at once. Content-switching
    /// choices commit their selection with the complete panel. A modal popup
    /// takes the pointer over the whole panel, including the tabs.
    pub fn looks(&self, fonts: &Fonts) -> Vec<menu::panel::ControlLooks> {
        let (Some(area), Some(panels), false) = (self.open, self.panels.as_ref(), self.covered) else { return Vec::new() };
        let Some(panel) = panels.panel(if self.confirming_discard { DISCARD_PANEL } else { PANEL }) else { return Vec::new() };
        if self.closing { return Vec::new(); }
        let mut looks = panels.looks(panel, &self.state, area, &measurer(fonts));
        // Selection changes also affect derived readouts and dependent draft controls.
        // Their selected state belongs to the complete published panel.
        for look in &mut looks {
            if matches!(look.select, scene::Select::Pick { .. } | scene::Select::Toggle) { look.select = scene::Select::None; }
        }
        looks
    }
}

/// A compact description of the draft, using control labels rather than
/// implementation field names. The dialog shows six lines and a remainder.
fn changed_controls(active: &Settings, draft: &Settings) -> Vec<String> {
    let label = |key: &str| match key {
        "resolution.pixel_scale" => "Pixel scale".into(),
        "resolution.base_ui_scale" => "Base UI scale".into(),
        "resolution.scale_menus" => "Scale menus".into(),
        "resolution.menu_scale_percent" => "Menu size".into(),
        "resolution.frame_scale" => "Game UI scale".into(),
        "resolution.world_scale" => "World scale".into(),
        "layout.split" => "View layout".into(),
        "layout.zoom" => "Zoom mode".into(),
        "layout.zoom_place" => "Zoom in a vertical split".into(),
        "layout.zoom_content" => "Zoom shows".into(),
        "layout.roles" => "View roles".into(),
        "layout.unit_buttons" => "Unit buttons".into(),
        _ => key.split('.').skip(1).collect::<Vec<_>>().join(" ").replace('_', " "),
    };
    let display = |value: &ControlValue| match value {
        ControlValue::On(on) => if *on { "On".into() } else { "Off".into() },
        ControlValue::Number(number) => menu::panel::format_number(*number, 2, false),
        ControlValue::Choice(choice) => choice.clone(),
    };
    active.controls().into_iter().filter_map(|(key, before)| {
        if key == "layout.orientation" { return None; }
        let after = draft.control(&key)?;
        (before != after).then(|| format!("{}: {} to {}", label(&key), display(&before), display(&after)))
    }).collect()
}

/// Measures text with `fonts` where they are loaded (else estimates).
fn measurer(fonts: &Fonts) -> impl Fn(&TextStyle, &str) -> TextMetrics + '_ {
    move |style: &TextStyle, text: &str| {
        fonts
            .metrics(&text_run(style, text, (0, 0)))
            .map_or_else(|| menu::panel::estimate(style, text), |(width, ascent, descent)| TextMetrics { width, ascent, descent })
    }
}

/// A run of text in a panel style, its pen at `at` on the baseline.
fn text_run(style: &TextStyle, text: &str, (x, y): (i32, i32)) -> TextRun {
    TextRun {
        x,
        y,
        color: style.color,
        face: style.face.clone(),
        height: -(style.height as i32),
        weight: style.weight,
        italic: style.italic,
        underline: false,
        text: text.to_owned(),
        backdrop: None,
    }
}

/// The panel's draws as interface items, drawn on top of the mirrored
/// screen (art regions and fills through the screen art, text through the
/// game's fonts).
pub fn items(draws: &[Draw]) -> Vec<Item> {
    draws
        .iter()
        .map(|d| match d {
            Draw::Element(e) => Item::Element(e.clone(), false),
            Draw::Text(t) => Item::Text(text_run(&t.style, &t.text, (t.x, t.y)), t.clip, false),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use viewer_settings::{Place, Roles, Split, Zoom, ZoomContent};

    const FORM: PreferencesForm = PreferencesForm { vtable: 0x66_E628, form: 0xC7_2888, background: "art\\interface\\x_pref background.pcx" };

    fn frame_with_screen(origin: (i32, i32), size: (i32, i32), background_at: Option<(i32, i32)>) -> Frame {
        let mut f = Frame::default();
        f.forms.push((0x1234, 1, (0, 0), (1920, 1080)));
        f.forms.push((FORM.vtable, 2, origin, size));
        if let Some((x, y)) = background_at {
            f.draws.push(ScreenDraw::Image {
                path: "C:\\Games\\Civ3\\civ3PTW\\Art\\interface\\x_pref background.pcx".into(),
                rect: [0, 0, 1024, 768],
                x,
                y,
                keyed: false,
                clip: [0, 0, 1920, 1080],
                mask: None,
                palette: None,
                opacity: 255,
            });
            f.form_draws.push((2, 0..1));
        }
        f
    }

    #[test]
    fn the_screen_is_found_by_its_form_and_background() {
        assert_eq!(screen_area(&Frame::default(), &FORM, (1024, 768)), None);
        let f = frame_with_screen((0, 0), (1920, 1080), Some((448, 156)));
        assert_eq!(screen_area(&f, &FORM, (1024, 768)), Some([448, 156, 1024, 768]));
        // No background drawn (yet): centred on the form's canvas.
        let f = frame_with_screen((0, 0), (1280, 1024), None);
        assert_eq!(screen_area(&f, &FORM, (1024, 768)), Some([128, 128, 1024, 768]));
        // Another form's background does not count.
        let mut f = frame_with_screen((0, 0), (1280, 1024), Some((5, 5)));
        f.form_draws = vec![(1, 0..1), (2, 1..1)];
        assert_eq!(screen_area(&f, &FORM, (1024, 768)), Some([128, 128, 1024, 768]));
    }

    #[test]
    fn values_hold_the_tab_settings_and_sizes() {
        let mut s = Settings::default();
        s.resolution.pixel_scale = 3;
        s.layout.split = Split::Vertical1To1;
        let v = values(&s, TAB_LAYOUT, (3840, 2160));
        assert_eq!(v[TAB], Value::Text("layout".into()));
        assert_eq!(v["resolution.pixel_scale"], Value::Text("3".into()));
        assert_eq!(v["resolution.realised"], Value::Text("1280×720".into()));
        assert_eq!(v["resolution.window"], Value::Text("3840×2160".into()));
        assert_eq!(v["resolution.effective"], Value::Text("×3".into()));
        // ×1 (the default, Retina too): rendered at the window's size.
        s.resolution.pixel_scale = 1;
        let v = values(&s, TAB_RESOLUTION, (3840, 2160));
        assert_eq!(v["resolution.pixel_scale"], Value::Text("1".into()));
        assert_eq!(v["resolution.realised"], Value::Text("3840×2160".into()));
        assert_eq!(v["resolution.effective"], Value::Text("×1".into()));
        assert_eq!(v["layout.split"], Value::Text("Vertical1To1".into()));
        assert_eq!(v["layout.smooth_world_movement"], Value::On(false));
        assert_eq!(v["layout.screen_place"], Value::Text("Floating".into()));
        assert!(!v.keys().any(|k| k.starts_with("layout.screens.") || k.starts_with("layout.screen_fixed.")), "no per-screen values");
        assert_eq!(v["input.snap_updates_per_second"], Value::Number(3.0));
        assert_eq!(v["display.show_fps"], Value::On(false));
        assert_eq!(v["input.map_movement"], Value::Text("EdgeScroll".into()));
    }

    #[test]
    fn events_change_settings_or_the_tab() {
        let mut s = Settings::default();
        assert_eq!(apply(&mut s, &PanelEvent::Chose { control: TAB.into(), value: TAB_RESOLUTION.into() }), (false, Some("resolution".into())));
        assert_eq!(s, Settings::default());
        assert_eq!(apply(&mut s, &PanelEvent::Chose { control: "layout.zoom".into(), value: "Mouse131".into() }), (true, None));
        assert_eq!(apply(&mut s, &PanelEvent::Toggled { control: "layout.smooth_world_movement".into(), on: true }), (true, None));
        assert_eq!(apply(&mut s, &PanelEvent::Slid { control: "layout.zoom_strip".into(), value: 0.4 }), (true, None));
        assert_eq!(apply(&mut s, &PanelEvent::Slid { control: "layout.zoom_strip".into(), value: 0.4 }), (false, None));
        assert_eq!(apply(&mut s, &PanelEvent::Chose { control: "layout.nothing".into(), value: "x".into() }), (false, None));
        assert_eq!((s.layout.zoom, s.layout.smooth_world_movement, s.layout.zoom_strip), (Zoom::Mouse131, true, 0.4));
    }

    #[test]
    fn accessibility_controls_edit_a_draft_and_accept_or_discard_together() {
        let (mut prefs, path) = open_prefs("accessibility");
        click(&mut prefs, 680, 55);
        assert_eq!(prefs.tab, TAB_ACCESSIBILITY);
        click(&mut prefs, 100, 151);
        click(&mut prefs, 122, 188);
        click(&mut prefs, 550, 150);
        click(&mut prefs, 550, 220);
        click(&mut prefs, 820, 313);
        click(&mut prefs, 100, 403);
        assert!(prefs.editing().accessibility.active_unit_arrow);
        assert!(!prefs.editing().accessibility.arrow_bobbing);
        assert!(prefs.editing().accessibility.unit_rings);
        assert!(prefs.editing().accessibility.allied.invert);
        assert!(prefs.editing().accessibility.stack_count);
        assert_eq!(prefs.settings().accessibility, prefs.editing().accessibility, "previews at once");
        assert!(!path.exists());
        let accepted = prefs.editing().accessibility.clone();
        prefs.action("accept", &mut Vec::new());
        assert_eq!(Settings::load(&path).unwrap().accessibility, accepted);
        let _ = std::fs::remove_file(path);
        let (mut prefs, _) = open_prefs("accessibility-cancel");
        click(&mut prefs, 680, 55);
        click(&mut prefs, 100, 151);
        prefs.action("cancel", &mut Vec::new());
        prefs.action("discard", &mut Vec::new());
        assert_eq!(prefs.settings().accessibility, Default::default());
    }

    #[test]
    fn clean_map_tab_lists_and_changes_every_view_flag() {
        let (mut prefs, path) = open_prefs("clean-map");
        assert!(click(&mut prefs, 440, 55).is_empty());
        assert_eq!(prefs.tab, TAB_CLEAN_MAP);
        for (column, view) in viewer_settings::clean_map::VIEWS.into_iter().enumerate() {
            for (row, (flag, _, _)) in viewer_settings::clean_map::FLAGS.into_iter().enumerate() {
                let key = format!("clean_map.{view}.{flag}");
                let at = (80 + 300 * column as i32 + 6, 266 + 29 * row as i32 + 6);
                assert!(click(&mut prefs, at.0, at.1).is_empty(), "{key}");
                assert_eq!(prefs.editing().control(&key), Some(ControlValue::On(true)), "{key}");
                assert_eq!(prefs.editing().control(&format!("clean_map.{view}.mode")), Some(ControlValue::Choice("Custom".into())));
                assert_eq!(prefs.settings(), prefs.editing(), "previews at once");
                assert!(!path.exists());
            }
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The points of art `art`'s `kind` ("labels" or "icons").
    fn art_points(table: &toml::Table, art: &str, kind: &str) -> Vec<(i32, i32)> {
        table["art"][art].get(kind).and_then(|l| l.as_array()).map_or(&[][..], |a| a.as_slice())
            .iter()
            .map(|l| {
                let a = l["at"].as_array().unwrap();
                (a[0].as_integer().unwrap() as i32, a[1].as_integer().unwrap() as i32)
            })
            .collect()
    }

    /// The panel parses, names only settings there are, and its sliders
    /// have the settings' ranges; the split pictures' labels sit in the
    /// middle of their panes.
    #[test]
    fn the_panel_matches_the_settings() {
        let panels = Panels::parse(PANELS).unwrap();
        let panel = panels.panel(PANEL).unwrap();
        assert_eq!(panel.size(), (1024, 768));
        let s = Settings::default();
        for control in panel.controls() {
            assert!(control == TAB || control == ACTION || control == UNDO || s.control(control).is_some(), "{control}");
            if let Some((min, max, step)) = panel.slider_range(control) {
                let r = viewer_settings::controls::range(control).unwrap();
                assert_eq!((min, max, step), (r.min, r.max, r.step), "{control}");
            }
        }
        for key in viewer_settings::controls::KEYS.iter().filter(|k| **k != "input.snap_ghost_fps") {
            assert!(panel.controls().contains(key), "no control for {key}");
        }
        assert!(panel.controls().contains(&"layout.screen_place"));
        assert!(!panel.controls().iter().any(|c| c.starts_with("layout.screens.")), "no per-screen places");
        let description = panels.description();
        // The generated pictures compile (they need no files).
        struct NoFiles;
        impl render_desc::ImageLoader for NoFiles {
            fn load(&self, source: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
                Err(format!("{source}: no files"))
            }
        }
        let text = format!("[grid]\nlayout = \"staggered\"\nstep = [64, 32]\n{}", Prefs::description());
        let compiled = render_desc::Description::parse(&text).unwrap().compile(&NoFiles, 1024).unwrap();
        assert!(compiled.set(&menu::panel::generated_sprite("roles_free_Vertical2To1")).is_some());
        let table: toml::Table = toml::from_str(PANELS).unwrap();
        for (split, name, variant) in [
            (render_desc::PanesSplit::Standard, "standard", "Standard"),
            (render_desc::PanesSplit::Vertical1To1, "vertical11", "Vertical1To1"),
            (render_desc::PanesSplit::Vertical2To1, "vertical21", "Vertical2To1"),
            (render_desc::PanesSplit::Horizontal1To1, "horizontal11", "Horizontal1To1"),
        ] {
            assert!(description.contains(&format!("panes:60,48,24,255:100x60:{name}\"")));
            assert!(description.contains(&format!("panes:60,48,24,255:100x60:{name}:free\"")));
            let centres = render_desc::panes_centres(split, (100, 60));
            for art in [format!("panes_{variant}"), format!("roles_free_{variant}")] {
                assert_eq!(art_points(&table, &art, "labels"), centres, "{art}");
            }
            // The unit and mouse roles: icons, no words, in the middle of
            // their panes and inside them (a pixel of the frame aside).
            let art = format!("roles_unit_{variant}");
            assert!(art_points(&table, &art, "labels").is_empty(), "{art}");
            assert_eq!(art_points(&table, &art, "icons"), centres, "{art}");
            let panes = render_desc::panes_rects(split, (100, 60));
            for ((icon, (cx, cy)), [px, py, pw, ph]) in table["art"][&art]["icons"].as_array().unwrap().iter().zip(art_points(&table, &art, "icons")).zip(panes) {
                let size = table["art"][icon["art"].as_str().unwrap()]["size"].as_array().unwrap();
                let (w, h) = (size[0].as_integer().unwrap() as i32, size[1].as_integer().unwrap() as i32);
                let (x, y) = (cx - w / 2, cy - h / 2);
                assert!(x >= px + 1 && y >= py + 1 && x + w <= px + pw - 1 && y + h <= py + ph - 1, "{art}: {icon} at {:?} in pane {:?}", [x, y, w, h], [px, py, pw, ph]);
            }
        }
    }

    /// Every checkbox of every tab, as Alt mode sees it: its whole row
    /// (box and label) draws the snapping pointer, which lands on the box.
    #[test]
    fn alt_mode_sees_every_checkbox_with_its_label() {
        use tile_viewer::snap::{pick, targets};
        let (mut prefs, _) = open_prefs("alt-checkbox-labels");
        let draft = prefs.draft.as_mut().unwrap();
        draft.accessibility.active_unit_arrow = true;
        draft.resolution.scale_menus = true;
        prefs.project_settings();
        let fonts = Fonts::default();
        let measure = measurer(&fonts);
        let mut seen = 0;
        for tab in [TAB_RESOLUTION, TAB_LAYOUT, TAB_ZOOM, TAB_CLEAN_MAP, TAB_ACCESSIBILITY] {
            prefs.tab = tab.into();
            prefs.route(vec![Input::MouseMove { x: 1000, y: 10 }], (1024, 768));
            let looks = prefs.looks(&Fonts::default());
            let items: Vec<scene::Interactable> = looks.iter().map(|l| scene::Interactable { region: l.region, alt_right_click: false, anchor: l.anchor, rect: l.rect, kind: l.kind, id: l.id }).collect();
            let shown = targets(&items, ((0, 0), (1, 1)), (1024, 768));
            for look in looks.iter().filter(|l| l.anchor.is_some()) {
                seen += 1;
                let t = shown.iter().find(|t| t.id == look.id).unwrap();
                // The label's text, as drawn.
                let label = look.draws.iter().find_map(|d| match d {
                    Draw::Text(text) => { let m = measure(&text.style, &text.text); Some([text.x, text.y - m.ascent, m.width, m.ascent + m.descent]) }
                    _ => None,
                }).unwrap_or_else(|| panic!("{tab}: {:?} has no label", look.rect));
                let [ax, ay, aw, ah] = t.area;
                assert!(ax <= label[0] as f64 && ax + aw >= (label[0] + label[2]) as f64 && ay <= label[1] as f64 && ay + ah >= (label[1] + label[3]) as f64,
                    "{tab}: the area {:?} holds the label {label:?}", t.area);
                // From far to its right on its own row, a move onto the
                // label's far end takes the checkbox, pointing at its box.
                let (lx, ly) = ((label[0] + label[2] - 2) as f64, (label[1] + label[3] / 2) as f64);
                let from = (lx + 200.0, ly);
                let k = pick(&shown, from, None, (-200.0, 0.0)).unwrap_or_else(|| panic!("{tab}: nothing picked from {from:?}"));
                assert_eq!(shown[k].id, look.id, "{tab}: a move onto the label of {:?} takes {:?}", look.rect, shown[k].rect);
                assert_eq!(shown[k].center(), (look.anchor.unwrap()[0] as f64, look.anchor.unwrap()[1] as f64), "{tab}: on its box");
            }
            // Rows of checkboxes stay apart: from a box, a move to the next
            // one down its column takes that one.
            for look in looks.iter().filter(|l| l.anchor.is_some()) {
                let below = looks.iter().filter(|o| o.anchor.is_some() && o.rect[0] == look.rect[0] && o.rect[1] > look.rect[1]).min_by_key(|o| o.rect[1]);
                let Some(below) = below else { continue };
                let (a, b) = (look.anchor.unwrap(), below.anchor.unwrap());
                let k = pick(&shown, (a[0] as f64, a[1] as f64), Some(look.id), ((b[0] - a[0]) as f64, (b[1] - a[1]) as f64)).unwrap();
                assert_eq!(shown[k].id, below.id, "{tab}: from {:?} down to {:?}", look.rect, below.rect);
            }
        }
        assert!(seen >= 40, "{seen} checkboxes");
    }

    fn open_prefs(name: &str) -> (Prefs, PathBuf) {
        let path = std::env::temp_dir().join(format!("civ3r-prefs-test-{}-{name}", std::process::id())).join("settings.toml");
        let _ = std::fs::remove_file(&path);
        let mut p = Prefs::load(path.clone());
        // (Opened by the user, not the first run's.)
        p.first_run = false;
        let f = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
        p.update(Some(&f), Some(FORM), (1024, 768), None);
        (p, path)
    }

    #[test]
    fn the_first_run_opens_on_the_resolution_tab_once_and_remembers_it() {
        let dir = std::env::temp_dir().join(format!("civ3r-prefs-test-{}-first-run", std::process::id()));
        let path = dir.join("settings.toml");
        let _ = std::fs::remove_dir_all(&dir);
        let shown = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
        // No settings file: the first run.
        let mut p = Prefs::load(path.clone());
        assert!(p.first_run_pending());
        // A Retina window: ×1, the old sizes on the sliders, at once.
        p.route(vec![Input::Window { width: 3258, height: 1904, scale_milli: 2000 }], (1024, 768));
        let seeded = p.settings().resolution.clone();
        assert_eq!((seeded.pixel_scale, seeded.frame_scale, seeded.world_scale, seeded.scale_menus), (1, 2.0, 2.0, true));
        // (Only at the first window: moved to another display, it stays.)
        p.route(vec![Input::Window { width: 1920, height: 1080, scale_milli: 1000 }], (1024, 768));
        assert_eq!(p.settings().resolution, seeded);
        p.update(Some(&Frame::default()), Some(FORM), (1024, 768), None);
        assert!(p.first_run_pending() && !path.exists(), "not before Preferences open");
        p.update(Some(&shown), Some(FORM), (1024, 768), None);
        assert_eq!((p.tab.as_str(), p.first_run_pending()), (TAB_RESOLUTION, false));
        let saved = Settings::load(&path).unwrap();
        assert!(saved.first_run.preferences_shown, "saved at once");
        assert_eq!(saved.resolution, seeded, "with the first run's sizes");
        assert!(!p.dirty(), "nothing for Cancel to ask about");
        // Closed without a change, opened again: the game's tab.
        p.update(Some(&Frame::default()), Some(FORM), (1024, 768), None);
        p.update(Some(&shown), Some(FORM), (1024, 768), None);
        assert_eq!(p.tab, TAB_GAME);
        // The next launch: not again.
        assert!(!Prefs::load(path.clone()).first_run_pending());
        // An existing install's file without the marker: not a first run.
        std::fs::write(&path, "[resolution]\npixel_scale = 2\n").unwrap();
        assert!(!Prefs::load(path.clone()).first_run_pending());
        // An unreadable one neither (nothing is saved over it unasked).
        std::fs::write(&path, "[layout]\nsplit = 7\n").unwrap();
        assert!(!Prefs::load(path.clone()).first_run_pending());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn click(p: &mut Prefs, x: i32, y: i32) -> Vec<Input> {
        let b = |down| Input::MouseButton { button: scene::Button::Left, down, x, y };
        p.route(vec![Input::MouseMove { x, y }, b(true), b(false)], (1024, 768))
    }

    #[test]
    fn function_key_capture_shows_on_macos_only() {
        let (mut p, path) = open_prefs("function-keys");
        assert!(click(&mut p, 330, 55).is_empty());
        let f = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
        // Not on macOS: no switch there.
        p.host_macos = false;
        p.update(Some(&f), Some(FORM), (1024, 768), None);
        assert!(click(&mut p, 80, 563).is_empty());
        assert!(click(&mut p, 104, 611).is_empty());
        assert!(p.editing().input.capture_function_keys && !p.editing().input.media_keys_as_function_keys);
        p.host_macos = true;
        p.update(Some(&f), Some(FORM), (1024, 768), None);
        // Capturing on by default, the media keys off: each switches.
        assert!(click(&mut p, 104, 611).is_empty());
        assert!(p.editing().input.media_keys_as_function_keys);
        assert!(click(&mut p, 80, 563).is_empty());
        assert!(!p.editing().input.capture_function_keys);
        assert!(click(&mut p, 104, 611).is_empty());
        assert!(!p.editing().input.media_keys_as_function_keys);
        assert!(click(&mut p, 80, 563).is_empty());
        assert!(p.editing().input.capture_function_keys);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn every_tab_masks_native_targets_below_its_visible_panel() {
        use civ3_sys::controls::{Control, ControlKind};
        let (mut p, path) = open_prefs("tab-targets");
        let body_targets = [[90, 160, 58, 32], [72, 115, 104, 64], [80, 217, 82, 26]];
        // Include a native target spanning the cover edge, a tab beneath
        // ours, the native header, and the game's own close button (never covered).
        let native = [
            Control { rect: [100, 140, 300, 16], kind: ControlKind::Button, id: 1 },
            Control { rect: [40, 200, 15, 20], kind: ControlKind::Button, id: 2 },
            Control { rect: [62, 42, 70, 16], kind: ControlKind::Button, id: 3 },
            Control { rect: [795, 28, 150, 28], kind: ControlKind::Button, id: 4 },
            Control { rect: [975, 718, 32, 28], kind: ControlKind::Button, id: 5 },
        ];
        for (tab, tab_x, visible_body) in [(TAB_GAME, 110, None), (TAB_RESOLUTION, 220, Some(0)), (TAB_LAYOUT, 330, Some(1)), (TAB_CLEAN_MAP, 440, Some(2)), (TAB_ZOOM, 550, None), (TAB_ACCESSIBILITY, 680, None)] {
            assert!(click(&mut p, tab_x, 55).is_empty());
            assert_eq!(p.tab, tab);
            let looks = p.looks(&Fonts::default());
            for (n, rect) in body_targets.iter().enumerate() {
                assert_eq!(looks.iter().any(|l| l.rect == *rect), visible_body == Some(n), "{tab}: {rect:?}");
            }
            for rect in [[60, 40, 105, 32], [170, 40, 105, 32], [280, 40, 105, 32], [390, 40, 105, 32], [500, 40, 105, 32]] {
                assert!(looks.iter().any(|l| l.rect == rect), "missing tab {rect:?} on {tab}");
            }
            if tab == TAB_GAME {
                assert_eq!(looks.len(), 6, "only our tabs: no text Accept or Cancel");
            }
            let mut visible = native.to_vec();
            p.retain_native_controls(&mut visible);
            let ids: Vec<_> = visible.iter().map(|c| c.id).collect();
            assert_eq!(ids, if tab == TAB_GAME { vec![1, 2, 4, 5] } else { vec![4, 5] }, "{tab}");
            // Blank painted cover consumes ordinary pointer input too,
            // while the native header remains usable on every tab.
            assert_eq!(click(&mut p, 955, 680).len(), if tab == TAB_GAME { 3 } else { 0 }, "{tab}");
            assert_eq!(click(&mut p, 880, 40).len(), 3, "{tab}");
            assert!(!looks.iter().any(|look| look.rect[1] >= 700), "no renderer buttons over the native footer");
        }
        // The game's 1024x768 panel can be centred in a larger window.
        let f = frame_with_screen((0, 0), (1920, 1080), Some((448, 156)));
        p.update(Some(&f), Some(FORM), (1920, 1080), None);
        let mut translated: Vec<_> = native.iter().map(|c| Control { rect: [c.rect[0] + 448, c.rect[1] + 156, c.rect[2], c.rect[3]], ..*c }).collect();
        p.retain_native_controls(&mut translated);
        assert_eq!(translated.iter().map(|c| c.id).collect::<Vec<_>>(), [4, 5]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn hidden_layout_controls_cannot_be_snapped_or_changed() {
        let (mut p, path) = open_prefs("hidden-layout-targets");
        assert!(click(&mut p, 330, 55).is_empty());
        assert!(click(&mut p, 230, 140).is_empty());
        let place_row = [[552, 146, 100, 48], [658, 146, 100, 48], [764, 146, 100, 48]];
        let has_row = |p: &Prefs| place_row.map(|r| p.looks(&Fonts::default()).iter().filter(|l| l.rect == r).count());
        assert_eq!(has_row(&p), [1, 1, 1]);
        // Placed on the right, menus have no mouse view to follow: the
        // checkbox is disabled and takes no clicks.
        assert!(click(&mut p, 814, 159).is_empty());
        assert_eq!(p.editing().layout.screen_place.vertical, Place::RightOrTop);
        let settings = p.editing().clone();
        assert!(click(&mut p, 560, 248).is_empty());
        assert_eq!(*p.editing(), settings);
        // The same position holds Top/Bottom in a horizontal split. Only
        // that orientation's controls should be offered.
        assert!(click(&mut p, 450, 140).is_empty());
        assert_eq!(p.editing().layout.split, Split::Horizontal1To1);
        assert_eq!(has_row(&p), [1, 1, 1]);
        let settings = p.editing().clone();
        assert!(click(&mut p, 220, 55).is_empty());
        assert!(click(&mut p, 814, 159).is_empty(), "the Resolution cover blocks the former Layout control");
        assert_eq!(*p.editing(), settings);
        assert!(!p.looks(&Fonts::default()).iter().any(|l| place_row.contains(&l.rect)));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn covered_preferences_publish_no_targets_or_hover_states() {
        use civ3_sys::controls::{Control, ControlKind};
        let (mut p, path) = open_prefs("covered-targets");
        assert!(click(&mut p, 330, 55).is_empty());
        let f = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
        p.update(Some(&f), Some(FORM), (1024, 768), Some(0x0123_4560));
        assert!(!p.draws(&Fonts::default()).is_empty(), "panel remains painted beneath the popup");
        assert!(p.looks(&Fonts::default()).is_empty(), "hidden controls must expose no snap targets or alternate hover draws");
        let popup = Control { rect: [100, 140, 300, 16], kind: ControlKind::Button, id: 7 };
        let mut controls = vec![popup];
        p.retain_native_controls(&mut controls);
        assert_eq!(controls, [popup], "native popup is above the renderer panel");
        assert_eq!(click(&mut p, 110, 150).len(), 3);
        p.update(Some(&f), Some(FORM), (1024, 768), None);
        assert!(!p.looks(&Fonts::default()).is_empty());
        p.retain_native_controls(&mut controls);
        assert!(controls.is_empty(), "ordinary native controls below the restored cover stay hidden");
        p.update(None, Some(FORM), (1024, 768), None);
        assert!(p.looks(&Fonts::default()).is_empty());
        let mut controls = vec![popup];
        p.retain_native_controls(&mut controls);
        assert_eq!(controls, [popup], "closing Preferences removes the mask");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Clicks through the tabs: the game gets what lies outside the panel,
    /// a changed setting stays in the draft, and the screen opens on the
    /// game's tab again.
    #[test]
    fn clicks_on_the_tabs_change_the_draft_without_applying_or_saving() {
        let (mut p, path) = open_prefs("clicks");
        assert!(p.is_open());
        // On the game's tab, its own controls get the clicks.
        assert_eq!(click(&mut p, 110, 150).len(), 3);
        // The Layout tab, then the vertical 2:1 split.
        assert!(click(&mut p, 330, 55).is_empty());
        assert_eq!(p.tab, TAB_LAYOUT);
        assert!(!path.exists());
        assert!(click(&mut p, 340, 140).is_empty());
        assert_eq!(p.editing().layout.split, Split::Vertical2To1);
        assert_eq!(p.settings(), p.editing(), "previews at once");
        // Roles: unit and mouse; screens on the right; the zoom right of the split.
        assert!(click(&mut p, 230, 238).is_empty());
        assert!(click(&mut p, 814, 159).is_empty());
        assert!(click(&mut p, 550, 55).is_empty());
        assert!(click(&mut p, 254, 160).is_empty());
        assert!(click(&mut p, 836, 279).is_empty());
        assert!(click(&mut p, 330, 55).is_empty());
        let draft = p.editing().clone();
        assert_eq!(draft.layout.roles, Roles::UnitMouse);
        assert_eq!(draft.layout.screen_place.vertical, Place::RightOrTop);
        assert_eq!(draft.layout.zoom_place, Place::RightOrTop);
        // Map movement: dragging, then edge scrolling again.
        assert!(click(&mut p, 400, 389).is_empty());
        assert_eq!(p.editing().input.map_movement, viewer_settings::MapMovement::Drag);
        assert!(click(&mut p, 250, 389).is_empty());
        assert_eq!(p.editing().input.map_movement, viewer_settings::MapMovement::EdgeScroll);
        assert!(click(&mut p, 560, 297).is_empty());
        assert!(p.editing().input.smooth_menu_scrolling);
        assert!(click(&mut p, 560, 297).is_empty());
        assert!(!p.editing().input.smooth_menu_scrolling);
        // Alt mode in menus, on the Accessibility tab: on by default, off, on again.
        assert!(click(&mut p, 680, 55).is_empty());
        assert!(p.editing().input.alt_in_menus);
        assert!(click(&mut p, 100, 496).is_empty());
        assert!(!p.editing().input.alt_in_menus);
        assert!(click(&mut p, 100, 496).is_empty());
        assert!(p.editing().input.alt_in_menus);
        assert_eq!(p.settings(), p.editing(), "previews at once");
        assert!(!path.exists());
        // The pixel scale on the Resolution tab.
        assert!(click(&mut p, 220, 55).is_empty());
        assert!(click(&mut p, 90 + 2 * 66 + 10, 170).is_empty());
        assert_eq!(p.editing().resolution.pixel_scale, 3);
        let preview = p.settings_preview().unwrap();
        p.route(vec![Input::SettingsPreviewDecision {session:preview.session,id:preview.id,keep:true}], (1024,768));
        let texts: Vec<String> = p.draws(&Fonts::default()).iter().filter_map(|d| if let Draw::Text(t) = d { Some(t.text.clone()) } else { None }).collect();
        assert!(texts.contains(&"Rendered at 341×256".to_string()), "{texts:?}");
        // The frame counter, on and off again by its checkbox (or its label).
        assert!(click(&mut p, 97, 495).is_empty());
        assert!(p.editing().display.show_fps);
        assert!(click(&mut p, 160, 495).is_empty());
        assert!(!p.editing().display.show_fps);
        // Downscaled menu backgrounds: smooth, nearest, dither again; a
        // draft until accepted.
        assert!(click(&mut p, 700, 495).is_empty());
        assert_eq!(p.editing().resolution.background_sampling, viewer_settings::BackgroundSampling::Interpolate);
        assert!(click(&mut p, 830, 495).is_empty());
        assert_eq!(p.editing().resolution.background_sampling, viewer_settings::BackgroundSampling::Nearest);
        assert_eq!(p.settings().resolution.background_sampling, viewer_settings::BackgroundSampling::Nearest);
        assert!(click(&mut p, 580, 495).is_empty());
        assert_eq!(p.editing().resolution.background_sampling, viewer_settings::BackgroundSampling::Dither);
        // ×1, the first.
        assert!(click(&mut p, 90 + 10, 170).is_empty());
        assert_eq!(p.editing().resolution.pixel_scale, 1);
        // Closed and opened again: the game's tab.
        p.update(Some(&Frame::default()), Some(FORM), (1024, 768), None);
        assert!(!p.is_open());
        assert_eq!(click(&mut p, 330, 55).len(), 3);
        let f = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
        p.update(Some(&f), Some(FORM), (1024, 768), None);
        assert_eq!(p.tab, TAB_GAME);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn menus_follow_the_mouse_by_default_while_screens_float() {
        let (mut p, path) = open_prefs("menu-mouse-view");
        assert!(click(&mut p, 330, 55).is_empty());
        let texts = |p: &Prefs| -> Vec<String> { p.draws(&Fonts::default()).iter().filter_map(|d| if let Draw::Text(t) = d { Some(t.text.clone()) } else { None }).collect() };
        assert!(p.editing().layout.menu_follows_pointer);
        assert_eq!(p.editing().layout.screen_place.vertical, Place::Floating);
        assert!(texts(&p).contains(&"Mouse view".to_owned()));
        assert!(click(&mut p, 560, 248).is_empty());
        assert!(!p.editing().layout.menu_follows_pointer);
        assert!(!texts(&p).contains(&"Mouse view".to_owned()));
        assert!(click(&mut p, 560, 248).is_empty());
        assert!(p.editing().layout.menu_follows_pointer);
        // A fixed place: no mouse view (the option is kept for Floating).
        assert!(click(&mut p, 340, 140).is_empty());
        assert!(click(&mut p, 814, 159).is_empty());
        assert_eq!(p.editing().layout.screen_place.vertical, Place::RightOrTop);
        assert!(!texts(&p).contains(&"Mouse view".to_owned()));
        assert!(click(&mut p, 600, 159).is_empty());
        let draft = p.editing().clone();
        assert!(draft.layout.menu_follows_pointer);
        assert_eq!(draft.layout.screen_place.vertical, Place::Floating);
        assert!(texts(&p).contains(&"Mouse view".to_owned()));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_screen_place_icons_are_generated_pictures() {
        struct NoFiles;
        impl render_desc::ImageLoader for NoFiles {
            fn load(&self, source: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> { Err(format!("no file {source}")) }
        }
        let panels = Panels::parse(PANELS).unwrap();
        let d = render_desc::Description::parse(&format!("grid = {{ layout = 'orthogonal', step = [1, 1] }}\n{}", panels.description())).unwrap();
        let compiled = d.compile(&NoFiles, 64).unwrap();
        for orientation in ["vertical", "horizontal"] {
            for place in ["Floating", "LeftOrBottom", "RightOrTop"] {
                let name = menu::panel::generated_sprite(&format!("place_{orientation}_{place}"));
                assert!(compiled.sprite(&name).is_some(), "{name}");
            }
        }
    }

    /// The Layout tab has one placement for every screen and popup, no
    /// per-screen rows; choosing it sets the current orientation's place
    /// only.
    #[test]
    fn the_layout_tab_has_one_screen_place_for_the_current_orientation() {
        let panels = Panels::parse(PANELS).unwrap();
        let panel = panels.panel(PANEL).unwrap();
        assert_eq!(panel.controls().iter().filter(|c| c.starts_with("layout.screen")).collect::<Vec<_>>(), [&"layout.screen_place"]);
        let (mut p, path) = open_prefs("one-screen-place");
        assert!(click(&mut p, 330, 55).is_empty());
        let texts = |p: &Prefs| -> Vec<String> { p.draws(&Fonts::default()).iter().filter_map(|d| if let Draw::Text(t) = d { Some(t.text.clone()) } else { None }).collect() };
        let shown = texts(&p);
        assert!(shown.contains(&"All screens and popups".to_owned()), "{shown:?}");
        for name in viewer_settings::Layout::SCREENS.iter().filter(|n| !n.starts_with('F')) {
            assert!(!shown.iter().any(|t| t == name), "no row for {name}");
        }
        // Zoom placement now has its own tab.
        assert_eq!(shown.iter().filter(|t| *t == "Floating").count(), 1, "{shown:?}");
        // The places are icon buttons (window glyphs), named below them.
        let looks = p.looks(&Fonts::default());
        for (x, value) in [(552, "Floating"), (658, "LeftOrBottom"), (764, "RightOrTop")] {
            assert!(looks.iter().any(|l| l.rect == [x, 146, 100, 48]), "{value}: a snappable button");
            let icons: Vec<String> = p.draws(&Fonts::default()).iter().filter_map(|d| match d {
                Draw::Element(menu::screen::Element::Sprite { name, .. }) if name.contains(&format!("place_vertical_{value}")) => Some(name.clone()),
                _ => None,
            }).collect();
            assert_eq!(icons.len(), 1, "{value}: {icons:?}");
        }
        // Vertical 1:1, screens on the left: the horizontal place stays.
        assert!(click(&mut p, 230, 140).is_empty());
        assert!(click(&mut p, 708, 159).is_empty());
        assert_eq!(p.editing().layout.screen_place, viewer_settings::ScreenPlace { vertical: Place::LeftOrBottom, horizontal: Place::Floating });
        // Horizontal: top, the vertical place kept.
        assert!(click(&mut p, 450, 140).is_empty());
        assert_eq!(p.editing().layout.split, Split::Horizontal1To1);
        assert_eq!(p.editing().control("layout.screen_place"), Some(ControlValue::Choice("Floating".into())));
        assert!(click(&mut p, 814, 159).is_empty());
        assert_eq!(p.editing().layout.screen_place, viewer_settings::ScreenPlace { vertical: Place::LeftOrBottom, horizontal: Place::RightOrTop });
        assert_eq!(p.settings(), p.editing(), "previews at once");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn capture_sensitivity_slider_drafts_its_limits_and_shows_the_multiplier() {
        let (mut p, path) = open_prefs("capture-sensitivity");
        assert!(click(&mut p, 330, 55).is_empty());
        assert_eq!(p.editing().input.capture_sensitivity, 3.0);
        assert!(click(&mut p, 850, 355).is_empty());
        assert_eq!(p.editing().input.capture_sensitivity, 10.0);
        assert_eq!(p.settings().input.capture_sensitivity, p.editing().input.capture_sensitivity);
        let texts: Vec<String> = p.draws(&Fonts::default()).iter().filter_map(|d| if let Draw::Text(t) = d { Some(t.text.clone()) } else { None }).collect();
        assert!(texts.contains(&"10x".to_owned()), "{texts:?}");
        assert!(click(&mut p, 553, 355).is_empty());
        assert_eq!(p.editing().input.capture_sensitivity, 0.1);
        assert_eq!(p.settings().input.capture_sensitivity, p.editing().input.capture_sensitivity);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// A popup over the screen (C3X's info box, its OK button over our
    /// Layout tab's panels): every click is the game's until it is gone.
    #[test]
    fn a_popup_over_the_screen_takes_the_input() {
        let (mut p, path) = open_prefs("popup");
        assert!(click(&mut p, 330, 55).is_empty());
        assert_eq!(p.tab, TAB_LAYOUT);
        let f = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
        // The screen itself modal: our panel's.
        p.update(Some(&f), Some(FORM), (1024, 768), Some(FORM.form));
        assert!(click(&mut p, 340, 140).is_empty());
        // C3X's popup modal over it: the game's, on our panels and tabs.
        p.update(Some(&f), Some(FORM), (1024, 768), Some(0x0123_4560));
        assert_eq!(click(&mut p, 787, 172).len(), 3);
        delivery::acknowledge_current(&p.native_delivery);
        p.update(Some(&f), Some(FORM), (1024, 768), Some(0x0123_4560));
        assert_eq!(click(&mut p, 220, 55).len(), 3);
        assert_eq!(p.tab, TAB_LAYOUT);
        // Gone after its input was delivered: ours again.
        delivery::acknowledge_current(&p.native_delivery);
        p.update(Some(&f), Some(FORM), (1024, 768), None);
        assert!(click(&mut p, 220, 55).is_empty());
        assert_eq!(p.tab, TAB_RESOLUTION);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Our tabs go over the screen and its controls, under a popup over it
    /// and the frame the game draws around that popup on no canvas.
    #[test]
    fn the_tabs_lie_under_a_popup_over_the_screen() {
        let fill = |x: i32| ScreenDraw::Fill { rect: [x, 0, 1, 1], rgb: [0, 0, 0], alpha: 255, clip: [0, 0, 1920, 1080] };
        let mut f = frame_with_screen((0, 0), (1920, 1080), Some((448, 156)));
        // The screen's checkbox (a child form), the popup's frame (no
        // canvas), the popup, the tooltip (no canvas).
        f.forms.push((0x671C70, 3, (500, 300), (20, 20)));
        f.forms.push((0x666408, 4, (635, 135), (650, 230)));
        f.draws.extend([fill(1), fill(2), fill(3), fill(4), fill(5)]);
        f.form_draws.extend([(3, 1..2), (4, 4..5)]);
        assert_eq!(tabs_slot(&f, &FORM, Some(4)), Some(2));
        // No popup: over everything but what is drawn onto the screen last.
        assert_eq!(tabs_slot(&f, &FORM, None), Some(5));
        // A popup split off: before its frame, at the end of what is left.
        let (below, above) = crate::interface_parts::split_at(&f, 4);
        assert_eq!(tabs_slot(&below, &FORM, Some(4)), Some(2));
        assert_eq!(tabs_slot(&above, &FORM, Some(4)), None);
        // The screen itself split off (it is modal): in that part.
        let (below, above) = crate::interface_parts::split_at(&f, 0);
        assert_eq!((tabs_slot(&below, &FORM, None), tabs_slot(&above, &FORM, None)), (None, Some(5)));
        assert_eq!(tabs_slot(&frame_with_screen((0, 0), (1920, 1080), None), &FORM, None), None);
    }

    /// The zoom's place in a split is greyed out (and takes no clicks)
    /// unless the split is vertical; screen places show and set the
    /// orientation's own.
    #[test]
    fn controls_follow_the_split() {
        let (mut p, path) = open_prefs("split");
        assert!(click(&mut p, 330, 55).is_empty());
        // Standard split: the zoom's place in a split does nothing.
        p.tab = TAB_ZOOM.into();
        p.draft.as_mut().unwrap().layout.zoom = Zoom::Floating131;
        p.project_settings();
        assert!(click(&mut p, 724, 279).is_empty());
        assert_eq!(p.editing().layout.zoom_place, Place::Floating);
        p.draft.as_mut().unwrap().layout.zoom = Zoom::Off;
        p.project_settings();
        assert!(click(&mut p, 330, 55).is_empty());
        // Horizontal: screens at the top ("Top" where "Right" was).
        assert!(click(&mut p, 460, 140).is_empty());
        assert_eq!(p.editing().layout.split, Split::Horizontal1To1);
        assert!(click(&mut p, 814, 159).is_empty());
        let texts: Vec<String> = p.draws(&Fonts::default()).iter().filter_map(|d| if let Draw::Text(t) = d { Some(t.text.clone()) } else { None }).collect();
        let count = |s: &str| texts.iter().filter(|t| *t == s).count();
        // Only the horizontal screen placement labels are on this tab.
        assert_eq!((count("Top"), count("Bottom"), count("Right"), count("Left")), (1, 1, 0, 0));
        // "B" in three split pictures and the free roles one; no words in the
        // unit and mouse one: the game's unit icon and pointer.
        assert_eq!((count("B"), count("Mouse"), count("Unit")), (4, 0, 0));
        let sprites: Vec<String> = p.draws(&Fonts::default()).iter().filter_map(|d| if let Draw::Element(menu::screen::Element::Sprite { name, .. }) = d { Some(name.clone()) } else { None }).collect();
        // (The split horizontal: their smaller ones.)
        assert!(["Prefs@unit_small/0", "Prefs@mouse_small/0"].iter().all(|s| sprites.iter().any(|n| n == s)), "{sprites:?}");
        let place = p.editing().layout.screen_place;
        assert_eq!((place.horizontal, place.vertical), (Place::RightOrTop, Place::Floating));
        assert!(click(&mut p, 550, 55).is_empty());
        assert_eq!(p.tab, TAB_ZOOM);
        // The height slider shows with every zoom but Off.
        assert!(!texts.iter().any(|t| t.ends_with('%')));
        // Placement pictures (centres): Off, box, wide box, at the mouse,
        // wide at the mouse, beside the mouse, top strip.
        let place = |i: i32| (142 + 112 * i, 160);
        let pick = |p: &mut Prefs, (x, y): (i32, i32)| assert!(click(p, x, y).is_empty());
        pick(&mut p, place(6));
        assert_eq!(p.editing().layout.zoom, Zoom::TopStrip);
        let texts: Vec<String> = p.draws(&Fonts::default()).iter().filter_map(|d| if let Draw::Text(t) = d { Some(t.text.clone()) } else { None }).collect();
        assert!(texts.contains(&"20%".to_string()), "{texts:?}");
        // Dragging the strip's slider to its right end: 50%.
        let drag = |x| Input::MouseMove { x, y: 421 };
        let b = |down, x| Input::MouseButton { button: scene::Button::Left, down, x, y: 421 };
        assert!(p.route(vec![b(true, 450), drag(800), b(false, 800)], (1024, 768)).is_empty());
        assert_eq!(p.editing().layout.zoom_strip, 0.5);
        assert_eq!(p.settings(), p.editing(), "previews at once");
        let texts = |p: &Prefs| -> Vec<String> { p.draws(&Fonts::default()).iter().filter_map(|d| if let Draw::Text(t) = d { Some(t.text.clone()) } else { None }).collect() };
        assert!(texts(&p).contains(&"Strip height".to_string()));
        // A box at the mouse is sized by it too: the slider shows, as the
        // box's height; to its left end: 5%.
        pick(&mut p, place(3));
        assert_eq!(p.editing().layout.zoom, Zoom::Mouse111);
        let shown = texts(&p);
        assert!(shown.contains(&"Box height".to_string()) && !shown.contains(&"Strip height".to_string()) && shown.contains(&"50%".to_string()), "{shown:?}");
        assert!(!shown.iter().any(|t| t == "Top margin" || t == "Gap when moved" || t == "Keep mouse out"), "{shown:?}");
        assert!(["Above the mouse", "Below, if no room"].iter().all(|t| shown.iter().any(|s| s == t)), "{shown:?}");
        assert!(p.route(vec![b(true, 450), drag(0), b(false, 0)], (1024, 768)).is_empty());
        assert_eq!(p.editing().layout.zoom_strip, 0.05);
        assert!(texts(&p).contains(&"5%".to_string()));
        // The floating boxes share that one slider (no Auto, no own
        // height), and have their top margin (2.5%), the gap they move below
        // their resting place by (2.5%) and the keep-the-mouse-out option.
        pick(&mut p, place(1));
        assert_eq!(p.editing().layout.zoom, Zoom::Floating131);
        let shown = texts(&p);
        assert!(shown.contains(&"Box height".to_string()) && shown.contains(&"5%".to_string()), "{shown:?}");
        // Pictures, named below them; no numbers on the placements.
        assert!(["Box", "Wide box", "At the mouse", "Beside the mouse", "Top strip", "World only", "World with menus"].iter().all(|t| shown.iter().any(|s| s == t)), "{shown:?}");
        assert!(!shown.iter().any(|t| t.contains("1:3:1") || t.contains("Floating")), "{shown:?}");
        assert!(!shown.iter().any(|t| t == "Auto"), "{shown:?}");
        assert!(["Top margin", "2.5%", "Gap when moved", "Keep mouse out"].iter().all(|t| shown.iter().any(|s| s == t)), "{shown:?}");
        assert!(p.route(vec![b(true, 450), drag(800), b(false, 800)], (1024, 768)).is_empty());
        assert_eq!(p.editing().layout.zoom_strip, 0.5);
        assert_eq!(p.settings(), p.editing());
        assert!(texts(&p).contains(&"50%".to_string()));
        assert!(!p.editing().layout.zoom_box_block_pointer);
        assert!(click(&mut p, 100, 540).is_empty());
        assert!(p.editing().layout.zoom_box_block_pointer);
        assert!(click(&mut p, 100, 540).is_empty());
        assert!(!p.editing().layout.zoom_box_block_pointer);
        let row = |down, x| Input::MouseButton { button: scene::Button::Left, down, x, y: 461 };
        let slide = |x| Input::MouseMove { x, y: 461 };
        assert!(p.route(vec![row(true, 450), slide(800), row(false, 800)], (1024, 768)).is_empty());
        assert_eq!(p.editing().layout.zoom_box_top, 0.05);
        assert!(p.route(vec![Input::MouseButton { button: scene::Button::Left, down: true, x: 450, y: 501 }, Input::MouseMove { x: 0, y: 501 }, Input::MouseButton { button: scene::Button::Left, down: false, x: 0, y: 501 }], (1024, 768)).is_empty());
        assert_eq!(p.editing().layout.zoom_box_drop, 0.05);
        assert_eq!(texts(&p).iter().filter(|s| *s == "5%").count(), 2, "{:?}", texts(&p));
        assert_eq!(p.settings(), p.editing());
        pick(&mut p, place(2));
        assert_eq!(p.editing().layout.zoom, Zoom::Floating171);
        assert!(["Box height", "50%", "Top margin", "Gap when moved"].iter().all(|t| texts(&p).iter().any(|s| s == t)));
        // The magnification, for every zoom: 2×, its steps 1.5× to 4×.
        assert!(["Magnification", "2×"].iter().all(|t| texts(&p).iter().any(|s| s == t)), "{:?}", texts(&p));
        let mag = |down, x| Input::MouseButton { button: scene::Button::Left, down, x, y: 381 };
        let along = |x| Input::MouseMove { x, y: 381 };
        assert!(p.route(vec![mag(true, 440), along(1000), mag(false, 1000)], (1024, 768)).is_empty());
        assert_eq!(p.editing().layout.zoom_magnification, 4.0);
        assert!(texts(&p).iter().any(|s| s == "4×"));
        assert!(p.route(vec![mag(true, 440), along(0), mag(false, 0)], (1024, 768)).is_empty());
        assert_eq!(p.editing().layout.zoom_magnification, 1.5);
        assert!(texts(&p).iter().any(|s| s == "1.5×"));
        assert_eq!(p.settings(), p.editing());
        assert!(p.route(vec![b(true, 450), drag(0), b(false, 0)], (1024, 768)).is_empty());
        assert_eq!(p.editing().layout.zoom_strip, 0.05);
        // What it shows: the world with menus, for this box too; then the
        // box can't hold the mouse out (it always moves away), and says so.
        assert_eq!(p.editing().layout.zoom_content, ZoomContent::World);
        pick(&mut p, (254, 279));
        assert_eq!((p.editing().layout.zoom, p.editing().layout.zoom_content), (Zoom::Floating171, ZoomContent::WorldAndMenus));
        assert_eq!(p.settings(), p.editing(), "previews at once");
        assert!(texts(&p).iter().any(|t| t == "Rendering menus doesn't render in a higher resolution. It just shows the pixels."));
        assert!(click(&mut p, 100, 540).is_empty());
        assert!(!p.editing().layout.zoom_box_block_pointer, "greyed out");
        p.route(vec![Input::MouseMove { x: 100, y: 540 }], (1024, 768));
        assert!(texts(&p).iter().any(|t| t == "A box showing menus always moves out of the mouse's way"), "{:?}", texts(&p));
        // Beside the mouse (where an older menu zoom went), menus kept.
        pick(&mut p, place(5));
        assert_eq!((p.editing().layout.zoom, p.editing().layout.zoom_content), (Zoom::Beside, ZoomContent::WorldAndMenus));
        assert!(texts(&p).iter().any(|t| t == "Box height"));
        pick(&mut p, (142, 279));
        assert_eq!(p.editing().layout.zoom_content, ZoomContent::World);
        assert!(p.editing().layout.zoom_rounded && p.editing().layout.zoom_glass);
        assert!(click(&mut p, 100, 578).is_empty());
        assert!(click(&mut p, 100, 613).is_empty());
        assert!(!p.editing().layout.zoom_rounded && !p.editing().layout.zoom_glass);
        pick(&mut p, place(0));
        assert_eq!(p.editing().layout.zoom, Zoom::Off);
        assert!(!texts(&p).iter().any(|t| t.ends_with('%') || t.ends_with("height") || t == "Drop" || t == "Top margin" || t == "Magnification" || t.ends_with('×')));
        // Off: what it shows is greyed out, and says why.
        assert!(click(&mut p, 254, 279).is_empty());
        assert_eq!(p.editing().layout.zoom_content, ZoomContent::World);
        assert!(texts(&p).iter().any(|t| t == "The zoom is off: choose a placement first"), "{:?}", texts(&p));
        assert!(click(&mut p, 330, 55).is_empty());
        assert!(click(&mut p, 438, 508).is_empty());
        assert_eq!(p.editing().layout.separator_width, 16);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The split and roles pictures' labels fit inside their panes, in the
    /// game's Lucida Sans (with an installation at `CIV3_INSTALL`).
    #[test]
    fn the_pictures_labels_fit_their_panes() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        if !std::path::Path::new(&install).join("LSANS.TTF").is_file() {
            return eprintln!("no Civ III installation, skipped");
        }
        let files = civ3_assets::GameFiles::complete(&install);
        let table: toml::Table = toml::from_str(PANELS).unwrap();
        let mut fonts = Fonts::default();
        for (split, variant) in [
            (render_desc::PanesSplit::Standard, "Standard"),
            (render_desc::PanesSplit::Vertical1To1, "Vertical1To1"),
            (render_desc::PanesSplit::Vertical2To1, "Vertical2To1"),
            (render_desc::PanesSplit::Horizontal1To1, "Horizontal1To1"),
        ] {
            let panes = render_desc::panes_rects(split, (100, 60));
            for art in [format!("panes_{variant}"), format!("roles_free_{variant}")] {
                for (label, pane) in table["art"][&art]["labels"].as_array().unwrap().iter().zip(&panes) {
                    let style = &table["styles"][label["style"].as_str().unwrap()];
                    let run = TextRun {
                        x: 0,
                        y: 0,
                        color: [0, 0, 0],
                        face: style["face"].as_str().unwrap().into(),
                        height: -(style["height"].as_integer().unwrap() as i32),
                        weight: style.get("weight").and_then(|w| w.as_integer()).unwrap_or(400) as i32,
                        italic: false,
                        underline: false,
                        text: label["text"].as_str().unwrap().into(),
                        backdrop: None,
                    };
                    fonts.add(&files, std::slice::from_ref(&run));
                    let (width, _, _) = fonts.metrics(&run).unwrap();
                    // (A pixel of the pane's frame on each side.)
                    assert!(width <= pane[2] - 2, "{art}: {:?} is {width} px wide, its pane {} px", run.text, pane[2]);
                }
            }
        }
    }

    /// Outside Preferences the emergency shortcut restores active settings
    /// immediately; inside Preferences it only edits the draft.
    #[test]
    fn ctrl_shift_backspace_resets_the_layout() {
        let path = std::env::temp_dir().join(format!("civ3r-prefs-test-{}-reset", std::process::id())).join("settings.toml");
        let mut s = Settings::default();
        s.layout.split = Split::Vertical1To1;
        s.layout.zoom = Zoom::Floating131;
        s.resolution.pixel_scale = 2;
        s.input.snap_updates_per_second = 5.0;
        s.save(&path).unwrap();
        let mut p = Prefs::load(path.clone());
        let key = |vk, down| Input::Key { vk, down, text: None };
        // Backspace alone goes to the game.
        assert_eq!(p.route(vec![key(VK_BACK, true), key(VK_BACK, false)], (1024, 768)).len(), 2);
        let out = p.route(vec![key(VK_CONTROL, true), key(VK_SHIFT, true), key(VK_BACK, true), key(VK_BACK, false), key(VK_SHIFT, false), key(VK_CONTROL, false)], (1024, 768));
        assert_eq!(out, [key(VK_CONTROL, true), key(VK_SHIFT, true), key(VK_SHIFT, false), key(VK_CONTROL, false)]);
        let saved = Settings::load(&path).unwrap();
        assert_eq!((&saved.layout, &saved.resolution), (&viewer_settings::Layout::default(), &viewer_settings::Resolution::default()));
        assert_eq!(saved.input.snap_updates_per_second, 5.0);
        assert_eq!(p.editing(), &saved);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Every piece of art the panel cuts from the game's files is there
    /// (with an installation at `CIV3_INSTALL`).
    #[test]
    fn the_panel_art_is_in_the_game_files() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        if !std::path::Path::new(&install).join("Art").is_dir() {
            return eprintln!("no Civ III installation, skipped");
        }
        let files = civ3_assets::GameFiles::complete(&install);
        let table: toml::Table = toml::from_str(PANELS).unwrap();
        for (name, art) in table["art"].as_table().unwrap() {
            let Some(image) = art.get("image").and_then(|i| i.as_str()) else { continue };
            let pcx = civ3_assets::pcx::decode(&files.read(image).unwrap_or_else(|e| panic!("{name}: {e}"))).unwrap();
            let r: Vec<i64> = art["rect"].as_array().unwrap().iter().map(|v| v.as_integer().unwrap()).collect();
            assert!(r[0] + r[2] <= pcx.width as i64 && r[1] + r[3] <= pcx.height as i64, "{name}: {r:?} outside {}x{}", pcx.width, pcx.height);
        }
    }

    #[test]
    fn alt_slider_previews_the_draft_and_restores_on_cancel_or_preferences_closure() {
        use scene::{InteractableKind, SliderAction};
        for close in ["cancel", "closed", "covered", "tab", "commit"] {
            let (mut prefs, path) = open_prefs(&format!("alt-slider-{close}"));
            assert!(click(&mut prefs, 220, 55).is_empty());
            let slider = prefs.looks(&Fonts::default()).into_iter().find(|c| matches!(c.kind, InteractableKind::Slider { .. })).unwrap();
            let initial_values = prefs.state.values.clone();
            let send = |prefs: &mut Prefs, action, value| prefs.route(vec![Input::Slider { id: slider.id, action, value }], (1024, 768));
            assert!(send(&mut prefs, SliderAction::Begin, 999_999).is_empty());
            assert_eq!(prefs.state.values, initial_values, "entry must not change or save any value");
            assert!(!path.exists());
            assert!(send(&mut prefs, SliderAction::Preview, 1_000_000).is_empty());
            let preview = prefs.editing().clone();
            assert_ne!(preview, Settings::default());
            assert_eq!(prefs.settings(), &preview, "world scale previews during the edit");
            assert!(!path.exists());
            match close {
                "cancel" => { assert!(send(&mut prefs, SliderAction::Cancel, 0).is_empty()); }
                "closed" => prefs.update(None, Some(FORM), (1024, 768), None),
                "covered" => {
                    let f = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
                    prefs.update(Some(&f), Some(FORM), (1024, 768), Some(0x1234_5678));
                }
                "tab" => { assert!(click(&mut prefs, 330, 55).is_empty()); }
                "commit" => { assert!(send(&mut prefs, SliderAction::Commit, 0).is_empty()); }
                _ => unreachable!(),
            }
            if close == "commit" {
                assert_eq!(prefs.editing(), &preview);
            } else {
                assert_eq!(prefs.editing(), &Settings::default(), "{close}");
            }
            assert_eq!(prefs.settings(), prefs.editing());
            assert!(!path.exists());
            assert!(prefs.state.slider_edit.is_none());
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        }
    }

    #[test]
    fn alt_slider_mouse_gesture_drives_real_preferences_without_native_button_presses() {
        use scene::{Interactable, InteractableKind, Scene, View, ViewState};
        use tile_viewer::{layout::Layout, snap::{Rates, SnapInput}};
        let (mut prefs, path) = open_prefs("alt-slider-mouse-driver");
        assert!(click(&mut prefs, 220, 55).is_empty());
        let slider = prefs.looks(&Fonts::default()).into_iter().find(|c| matches!(c.kind, InteractableKind::Slider { .. })).unwrap();
        let scene = Scene {
            seq: 1,
            views: vec![ViewState { id: 0, view: View { interface_only: true, ..Default::default() }, overlay: vec![] }],
            interactables: vec![Interactable { region: None, alt_right_click: false, id: slider.id, rect: slider.rect, anchor: None, kind: slider.kind }],
            ..Default::default()
        };
        let mut layout = Layout::default();
        layout.frame(&scene, (1024, 768), (0, 0, 0, 0), vec![], None);
        let mut snap = SnapInput::default();
        let rates = Rates::default();
        for down in [true, false] {
            let input = Input::Key { vk: 0x12, down, text: None };
            let routed = snap.route(input, &mut layout, &scene, 0.0, rates);
            prefs.route(routed, (1024, 768));
        }
        snap.tick(&mut layout, &scene, 1.0, rates);
        let button = |button, down| Input::MouseButton { button, down, x: 0, y: 0 };
        for down in [true, false] {
            let routed = snap.route(button(scene::Button::Left, down), &mut layout, &scene, 1.0, rates);
            assert!(!routed.iter().any(|i| matches!(i, Input::MouseButton { .. })));
            assert!(prefs.route(routed, (1024, 768)).is_empty());
        }
        assert_eq!(prefs.editing(), &Settings::default());
        assert!(!path.exists());
        let pointer = snap.snapper.pointer();
        snap.motion((1000.0, 0.0));
        let routed = snap.tick(&mut layout, &scene, 1.1, rates);
        assert!(prefs.route(routed, (1024, 768)).is_empty());
        assert_ne!(prefs.editing(), &Settings::default());
        assert_eq!(prefs.settings(), prefs.editing(), "world scale previews before Alt release");
        assert!(!path.exists());
        assert_eq!(snap.snapper.pointer(), pointer);
        for down in [true, false] {
            let routed = snap.route(button(scene::Button::Right, down), &mut layout, &scene, 1.2, rates);
            assert!(!routed.iter().any(|i| matches!(i, Input::MouseButton { .. })));
            assert!(prefs.route(routed, (1024, 768)).is_empty());
        }
        assert_eq!(prefs.editing(), &Settings::default());
        assert_eq!(prefs.settings().clone(), *prefs.editing());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    fn key(prefs: &mut Prefs, vk: u32) -> Vec<Input> {
        prefs.route([true, false].map(|down| Input::Key { vk, down, text: None }).into(), (1024, 768))
    }

    fn native_close(vk: u32) -> Vec<Input> {
        [true, false].map(|down| Input::Key { vk, down, text: None }).into()
    }

    /// The game's round Accept and X Cancel as `civ3_sys::controls` finds
    /// them (their `Button` objects in the form, background (960, 730) and
    /// (990, 730)); points inside each.
    const ACCEPT_AT: (i32, i32) = (975, 745);
    const CANCEL_AT: (i32, i32) = (1002, 742);
    fn native_buttons() -> Vec<civ3_sys::controls::Control> {
        use civ3_sys::controls::{Control, ControlKind};
        vec![
            Control { rect: [960, 730, 30, 30], kind: ControlKind::Button, id: u64::from(FORM.form + 0x5A0) },
            Control { rect: [990, 730, 24, 24], kind: ControlKind::Button, id: u64::from(FORM.form + 0xC74) },
        ]
    }

    fn with_native_buttons(name: &str) -> (Prefs, PathBuf) {
        let (mut prefs, path) = open_prefs(name);
        let mut controls = native_buttons();
        prefs.retain_native_controls(&mut controls);
        assert_eq!(controls, native_buttons(), "the game's buttons stay visible and snappable");
        (prefs, path)
    }

    /// The pointer's move reaches the game (its own hover art), then `rest`.
    fn hover_then((x, y): (i32, i32), rest: Vec<Input>) -> Vec<Input> {
        std::iter::once(Input::MouseMove { x, y }).chain(rest).collect()
    }

    #[test]
    fn the_round_native_button_commits_the_draft_and_closes_natively() {
        let (mut prefs, path) = with_native_buttons("native-accept");
        assert!(click(&mut prefs, 330, 55).is_empty());
        assert!(click(&mut prefs, 340, 140).is_empty());
        prefs.follow_native(|| Some(native::test_values(false)), true);
        prefs.follow_native(|| Some(native::test_values(true)), false);
        let draft = prefs.editing().clone();
        assert_ne!(draft, Settings::default());
        assert_eq!(&draft, prefs.settings(), "previews at once");
        // A press dragged off the button and released elsewhere is no click.
        let b = |down, (x, y): (i32, i32)| Input::MouseButton { button: scene::Button::Left, down, x, y };
        assert!(prefs.route(vec![b(true, ACCEPT_AT), b(false, (600, 740))], (1024, 768)).is_empty());
        assert_eq!(prefs.settings(), &draft);
        assert!(!path.exists());
        // A press elsewhere released on the button is the game's input.
        assert_eq!(prefs.route(vec![b(true, (600, 740)), b(false, ACCEPT_AT)], (1024, 768)), [b(true, (600, 740)), b(false, ACCEPT_AT)]);
        assert!(!path.exists());
        // A click: our tabs' draft is applied and saved, and the game
        // accepts its own (native) changes through its round button's
        // callback, never through Return.
        assert_eq!(click(&mut prefs, ACCEPT_AT.0, ACCEPT_AT.1), hover_then(ACCEPT_AT, vec![]));
        assert!(prefs.native_accept_pending());
        assert!(!prefs.confirming_discard);
        assert_eq!(prefs.settings(), &draft);
        assert_eq!(Settings::load(&path).unwrap(), draft);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_x_native_button_with_a_dirty_draft_prompts_and_no_keeps_the_dialog() {
        for native_only in [false, true] {
            let (mut prefs, path) = with_native_buttons(&format!("native-cancel-dirty-{native_only}"));
            prefs.follow_native(|| Some(native::test_values(false)), true);
            if native_only {
                prefs.follow_native(|| Some(native::test_values(true)), false);
            } else {
                click(&mut prefs, 330, 55);
                click(&mut prefs, 340, 140);
            }
            let draft = prefs.editing().clone();
            assert_eq!(click(&mut prefs, CANCEL_AT.0, CANCEL_AT.1), hover_then(CANCEL_AT, vec![]), "the game never sees the X click");
            assert!(prefs.confirming_discard);
            // The prompt is modal: the native buttons are not offered under it.
            let mut controls = native_buttons();
            prefs.retain_native_controls(&mut controls);
            assert!(controls.is_empty());
            assert!(click(&mut prefs, ACCEPT_AT.0, ACCEPT_AT.1).is_empty(), "the prompt's shade takes the click");
            assert!(prefs.confirming_discard);
            // No: back to the same draft, the dialog still open.
            assert!(click(&mut prefs, 700, 522).is_empty());
            assert!(!prefs.confirming_discard && !prefs.closing && prefs.is_open());
            assert_eq!(prefs.editing(), &draft);
            assert!(prefs.dirty());
            let mut controls = native_buttons();
            prefs.retain_native_controls(&mut controls);
            assert_eq!(controls, native_buttons());
            // X again, then Yes: discarded, closed through native Escape.
            assert_eq!(click(&mut prefs, CANCEL_AT.0, CANCEL_AT.1), hover_then(CANCEL_AT, vec![]));
            assert!(prefs.confirming_discard);
            assert_eq!(click(&mut prefs, 480, 522), native_close(VK_ESCAPE));
            assert_eq!(prefs.editing(), &Settings::default());
            assert_eq!(prefs.settings(), &Settings::default());
            assert!(!path.exists());
        }
    }

    #[test]
    fn the_x_native_button_with_a_clean_draft_closes_at_once() {
        let (mut prefs, path) = with_native_buttons("native-cancel-clean");
        prefs.follow_native(|| Some(native::test_values(false)), true);
        assert!(!prefs.dirty());
        assert_eq!(click(&mut prefs, CANCEL_AT.0, CANCEL_AT.1), hover_then(CANCEL_AT, native_close(VK_ESCAPE)));
        assert!(!prefs.confirming_discard);
        assert!(!path.exists());
        // A repeat before the game closes cannot cancel again.
        assert!(click(&mut prefs, CANCEL_AT.0, CANCEL_AT.1).is_empty());
    }

    /// QA on 3a91: native Return closes Preferences before 0x587EA0 sets
    /// the result byte, so Civ III options, audio and conquests.ini were
    /// rolled back. Every Accept must use the round button's callback.
    #[test]
    fn accept_runs_the_native_button_callback_never_return_and_cancel_never_accepts() {
        let returns = |inputs: &[Input]| inputs.iter().any(|i| matches!(i, Input::Key { vk: VK_RETURN, .. } | Input::WorldKey { vk: VK_RETURN, .. }));
        for by_key in [false, true] {
            let (mut prefs, path) = with_native_buttons(&format!("native-accept-callback-{by_key}"));
            prefs.follow_native(|| Some(native::test_values(false)), true);
            prefs.follow_native(|| Some(native::test_values(true)), false);
            let out = if by_key { key(&mut prefs, VK_RETURN) } else { click(&mut prefs, ACCEPT_AT.0, ACCEPT_AT.1) };
            assert!(!returns(&out), "Accept forwarded native Return: {out:?}");
            assert!(!out.iter().any(|i| matches!(i, Input::MouseButton { .. })), "the game's own click path is not used twice: {out:?}");
            assert!(prefs.native_accept_pending() && prefs.closing);
            // Repeats before closure neither send Return nor accept again.
            assert!(key(&mut prefs, VK_RETURN).is_empty());
            assert!(click(&mut prefs, ACCEPT_AT.0, ACCEPT_AT.1).is_empty());
            // Closing the form forgets an Accept that never ran.
            prefs.update(None, None, (1024, 768), None);
            assert!(!prefs.native_accept_pending());
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        }
        // Escape and the X never ask the game to accept.
        for by_key in [false, true] {
            let (mut prefs, path) = with_native_buttons(&format!("native-cancel-callback-{by_key}"));
            prefs.follow_native(|| Some(native::test_values(false)), true);
            let out = if by_key { key(&mut prefs, VK_ESCAPE) } else { click(&mut prefs, CANCEL_AT.0, CANCEL_AT.1) };
            assert!(out.ends_with(&native_close(VK_ESCAPE)) && !returns(&out), "{out:?}");
            assert!(!prefs.native_accept_pending());
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        }
    }

    #[test]
    fn alt_snapped_clicks_on_the_native_buttons_run_the_transaction() {
        use scene::{Interactable, InteractableKind, Scene, View, ViewState};
        use tile_viewer::{layout::Layout, snap::{Rates, SnapInput}};
        for button in [NativeButton::Accept, NativeButton::Cancel] {
            let (mut prefs, path) = with_native_buttons(&format!("native-alt-{button:?}"));
            click(&mut prefs, 330, 55);
            click(&mut prefs, 340, 140);
            let draft = prefs.editing().clone();
            let target = native_buttons()[if button == NativeButton::Accept { 0 } else { 1 }];
            let scene = Scene {
                seq: 1,
                views: vec![ViewState { id: 0, view: View { interface_only: true, ..Default::default() }, overlay: vec![] }],
                interactables: vec![Interactable { region: None, alt_right_click: false, id: target.id, rect: target.rect, anchor: None, kind: InteractableKind::Button }],
                ..Default::default()
            };
            let mut layout = Layout::default();
            layout.frame(&scene, (1024, 768), (0, 0, 0, 0), vec![], None);
            let mut snap = SnapInput::default();
            let rates = Rates::default();
            let mut forwarded = Vec::new();
            for down in [true, false] {
                let routed = snap.route(Input::Key { vk: 0x12, down, text: None }, &mut layout, &scene, 0.0, rates);
                forwarded.extend(prefs.route(routed, (1024, 768)));
            }
            forwarded.extend(prefs.route(snap.tick(&mut layout, &scene, 1.0, rates), (1024, 768)));
            for down in [true, false] {
                let routed = snap.route(Input::MouseButton { button: scene::Button::Left, down, x: 0, y: 0 }, &mut layout, &scene, 1.1, rates);
                forwarded.extend(prefs.route(routed, (1024, 768)));
            }
            assert!(!forwarded.iter().any(|i| matches!(i, Input::MouseButton { .. })), "{button:?}: {forwarded:?}");
            let keys: Vec<_> = forwarded.into_iter().filter(|i| matches!(i, Input::Key { vk: VK_RETURN | VK_ESCAPE, .. })).collect();
            match button {
                NativeButton::Accept => {
                    assert!(keys.is_empty(), "Accept never sends native Return");
                    assert!(prefs.native_accept_pending());
                    assert_eq!(prefs.settings(), &draft);
                    assert_eq!(Settings::load(&path).unwrap(), draft);
                }
                NativeButton::Cancel => {
                    assert!(keys.is_empty());
                    assert!(prefs.confirming_discard);
                    assert_eq!(prefs.settings(), prefs.editing(), "the preview stays while asking");
                    assert!(!path.exists());
                }
            }
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        }
    }

    #[test]
    fn draft_changes_preview_but_leave_accepted_settings_and_existing_file_unchanged_until_accept() {
        let (mut prefs, path) = with_native_buttons("transaction-accept");
        prefs.settings().save(&path).unwrap();
        let stored = std::fs::read(&path).unwrap();
        let active = prefs.settings().clone();
        assert!(click(&mut prefs, 330, 55).is_empty());
        assert!(click(&mut prefs, 340, 140).is_empty());
        assert!(click(&mut prefs, 850, 613).is_empty());
        let draft = prefs.editing().clone();
        assert_ne!(draft, active);
        assert_eq!(prefs.settings(), &draft, "the game previews the draft");
        assert_eq!(prefs.settings, active, "the accepted settings stay");
        assert_eq!(std::fs::read(&path).unwrap(), stored, "draft writes do not touch disk");
        assert_eq!(click(&mut prefs, ACCEPT_AT.0, ACCEPT_AT.1), hover_then(ACCEPT_AT, vec![]), "the round button accepts");
        assert!(prefs.native_accept_pending());
        assert_eq!(prefs.settings(), &draft);
        assert_eq!(Settings::load(&path).unwrap(), draft);
        // Until native closure is observed, a repeat or queued double click
        // cannot apply or save a second time.
        std::fs::remove_file(&path).unwrap();
        assert!(key(&mut prefs, VK_RETURN).is_empty());
        assert!(click(&mut prefs, ACCEPT_AT.0, ACCEPT_AT.1).is_empty());
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn dirty_cancel_confirmation_keeps_draft_on_no_and_discards_on_yes() {
        let (mut prefs, path) = with_native_buttons("transaction-discard");
        prefs.settings().save(&path).unwrap();
        let stored = std::fs::read(&path).unwrap();
        let active = prefs.settings().clone();
        assert!(click(&mut prefs, 220, 55).is_empty());
        assert!(click(&mut prefs, 232, 170).is_empty());
        let preview = prefs.settings_preview().unwrap();
        prefs.route(vec![Input::SettingsPreviewDecision {session:preview.session,id:preview.id,keep:true}], (1024,768));
        let draft = prefs.editing().clone();
        assert_eq!(click(&mut prefs, CANCEL_AT.0, CANCEL_AT.1), hover_then(CANCEL_AT, vec![]), "only the hover reaches the game");
        assert!(prefs.confirming_discard);
        let looks = prefs.looks(&Fonts::default());
        assert_eq!(looks.len(), 2, "Alt offers only Yes and No while confirmation is open");
        assert!(looks.iter().all(|look| look.rect[1] == 506));
        let texts: Vec<_> = prefs.draws(&Fonts::default()).into_iter().filter_map(|draw| match draw {
            Draw::Text(text) => Some(text.text), _ => None,
        }).collect();
        assert!(texts.iter().any(|text| text.contains("Pixel scale:")), "{texts:?}");
        assert!(click(&mut prefs, 330, 55).is_empty(), "the confirmation blocks the underlying tabs");
        assert_eq!(prefs.tab, TAB_RESOLUTION);
        assert!(click(&mut prefs, 700, 522).is_empty());
        assert!(!prefs.confirming_discard);
        assert_eq!(prefs.editing(), &draft);
        assert_eq!(prefs.settings(), &draft);
        assert_eq!(std::fs::read(&path).unwrap(), stored);
        assert!(key(&mut prefs, VK_ESCAPE).is_empty());
        assert!(prefs.confirming_discard);
        assert_eq!(click(&mut prefs, 480, 522), native_close(VK_ESCAPE));
        assert_eq!(prefs.settings(), &active);
        assert_eq!(prefs.editing(), &active);
        assert_eq!(std::fs::read(&path).unwrap(), stored);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The zoom tab's pictures are Alt targets: a snapped click (at the
    /// target's middle) picks one and previews it at once. Cancel's
    /// discard restores the accepted placement and content, unsaved; the
    /// confirmation names them.
    #[test]
    fn zoom_pictures_are_alt_targets_that_preview_and_cancel_restores() {
        let (mut prefs, path) = with_native_buttons("zoom-pictures");
        let within = |[x, y, w, h]: [i32; 4], (px, py): (i32, i32)| px >= x && py >= y && px < x + w && py < y + h;
        prefs.settings().save(&path).unwrap();
        let stored = std::fs::read(&path).unwrap();
        let active = prefs.settings().clone();
        assert_eq!((active.layout.zoom, active.layout.zoom_content), (Zoom::Off, ZoomContent::World));
        assert!(click(&mut prefs, 550, 55).is_empty());
        let target = |prefs: &Prefs, at: (i32, i32)| {
            let looks = prefs.looks(&Fonts::default());
            let look = looks.into_iter().find(|l| within(l.rect, at)).unwrap_or_else(|| panic!("no Alt target at {at:?}"));
            assert_eq!(look.kind, scene::InteractableKind::Button);
            assert_eq!(look.rect[2..], [104, 64]);
            (look.rect[0] + look.rect[2] / 2, look.rect[1] + look.rect[3] / 2)
        };
        // Off: no content targets (greyed out), every placement one.
        let looks = prefs.looks(&Fonts::default());
        assert!(!looks.iter().any(|l| within(l.rect, (142, 279))));
        for (i, zoom) in [Zoom::Off, Zoom::Floating131, Zoom::Floating171, Zoom::Mouse111, Zoom::Mouse131, Zoom::Beside, Zoom::TopStrip].into_iter().enumerate().rev() {
            let (x, y) = target(&prefs, (142 + 112 * i as i32, 160));
            assert!(click(&mut prefs, x, y).is_empty());
            assert_eq!(prefs.editing().layout.zoom, zoom);
            assert_eq!(prefs.settings(), prefs.editing(), "previews at once");
        }
        let (x, y) = target(&prefs, (142 + 112 * 3, 160));
        click(&mut prefs, x, y);
        let (x, y) = target(&prefs, (254, 279));
        assert!(click(&mut prefs, x, y).is_empty());
        assert_eq!((prefs.settings().layout.zoom, prefs.settings().layout.zoom_content), (Zoom::Mouse111, ZoomContent::WorldAndMenus));
        // The place in a split: a target once the split is vertical.
        assert!(!prefs.looks(&Fonts::default()).iter().any(|l| within(l.rect, (724, 279))));
        prefs.draft.as_mut().unwrap().layout.split = Split::Vertical1To1;
        prefs.project_settings();
        prefs.route(vec![], (1024, 768));
        let (x, y) = target(&prefs, (724, 279));
        assert!(click(&mut prefs, x, y).is_empty());
        assert_eq!(prefs.settings().layout.zoom_place, Place::LeftOrBottom);
        assert_eq!(std::fs::read(&path).unwrap(), stored, "nothing saved before Accept");
        // Cancel: the confirmation names the changes; Yes restores them.
        assert_eq!(click(&mut prefs, CANCEL_AT.0, CANCEL_AT.1), hover_then(CANCEL_AT, vec![]));
        assert!(prefs.confirming_discard);
        let texts: Vec<_> = prefs.draws(&Fonts::default()).into_iter().filter_map(|d| match d { Draw::Text(t) => Some(t.text), _ => None }).collect();
        assert!(texts.iter().any(|t| t == "Zoom mode: Off to Mouse111"), "{texts:?}");
        assert!(texts.iter().any(|t| t == "Zoom shows: World to WorldAndMenus"), "{texts:?}");
        assert_eq!(click(&mut prefs, 480, 522), native_close(VK_ESCAPE));
        assert_eq!(prefs.settings(), &active);
        assert_eq!(prefs.editing(), &active);
        assert_eq!(std::fs::read(&path).unwrap(), stored);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn clean_cancel_and_reverted_draft_close_without_confirmation_or_save() {
        for reverted in [false, true] {
            let (mut prefs, path) = open_prefs(&format!("transaction-clean-{reverted}"));
            if reverted {
                click(&mut prefs, 220, 55);
                click(&mut prefs, 100, 495);
                click(&mut prefs, 100, 495);
            }
            assert!(!prefs.dirty());
            assert_eq!(key(&mut prefs, VK_ESCAPE), native_close(VK_ESCAPE));
            assert!(!prefs.confirming_discard);
            assert!(!path.exists());
        }
    }

    #[test]
    fn confirmation_keyboard_enter_escape_and_no_keep_editing_and_yes_discards() {
        for keep in [VK_RETURN, VK_ESCAPE, 0x4E] {
            let (mut prefs, path) = open_prefs(&format!("transaction-key-{keep}"));
            click(&mut prefs, 330, 55);
            click(&mut prefs, 340, 140);
            let draft = prefs.editing().clone();
            assert!(key(&mut prefs, VK_ESCAPE).is_empty());
            assert!(prefs.confirming_discard);
            assert!(key(&mut prefs, keep).is_empty());
            assert!(!prefs.confirming_discard);
            assert_eq!(prefs.editing(), &draft);
            assert_eq!(prefs.settings(), &draft, "keeping the draft keeps its preview");
            assert!(!path.exists());
            key(&mut prefs, VK_ESCAPE);
            assert_eq!(key(&mut prefs, 0x59), native_close(VK_ESCAPE));
            assert_eq!(prefs.editing(), &Settings::default());
        }
    }

    /// Run explicitly with Civ III installed. Uses the same panel art,
    /// font metrics and screen sprites as the live bridge, on the CPU.
    /// Renders every renderer tab, the Base UI scale preview and the undo
    /// buttons and tooltip.
    #[test]
    #[ignore = "requires Civ III art; writes screenshots to C3R_PREFS_SHOTS or /tmp/c3r-prefs-undo-shots"]
    fn preferences_undo_headless_screenshots() {
        use menu::screen::{Element, Region, ScreenArt};
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        let files = civ3_assets::GameFiles::complete(&install);
        let directory = PathBuf::from(std::env::var("C3R_PREFS_SHOTS").unwrap_or_else(|_| "/tmp/c3r-prefs-undo-shots".into()));
        std::fs::create_dir_all(&directory).unwrap();
        let mut fonts = Fonts::default();
        let mut shot = |prefs: &Prefs, name: &str| {
            let runs = items(&prefs.draws(&fonts)).into_iter().filter_map(|i| match i { Item::Text(r, ..) => Some(r), _ => None }).collect::<Vec<_>>();
            fonts.add(&files, &runs);
            let background = Element::Image { region: Region { source: "Art/interface/x_pref background.pcx".into(), rect: [0, 0, 1024, 768], keyed: false, mask: None, palette: None }, at: (0, 0), clip: [0, 0, 1024, 768] };
            let mut draw_items = vec![Item::Element(background, false)];
            draw_items.extend(items(&prefs.draws(&fonts)));
            let mut art = ScreenArt::default();
            art.add(&draw_items.iter().filter_map(|i| match i { Item::Element(e, _) => Some(e.clone()), _ => None }).collect::<Vec<_>>());
            let description = format!("grid = {{ layout = 'orthogonal', step = [1, 1] }}\n{}\n{}\n{}", Prefs::description(), crate::ui_screen::description(&art), fonts.description());
            let compiled = crate::assets::compile(&description, civ3_assets::GameFiles::with_roots(files.roots().to_vec())).unwrap();
            let sprites = crate::ui_screen::sprites(&draw_items, &fonts, &compiled, 0, false);
            let scene = scene::Scene { views: vec![scene::ViewState { id: 0, view: scene::View { interface_only: true, ..Default::default() }, overlay: sprites }], ..Default::default() };
            let world = tile_viewer::scene_world(&compiled, &scene);
            let picture = tile_render::reference::render(&world, &compiled.atlas, &tile_render::Camera::new((1024, 768)), 0, [0, 0, 0, 255]);
            image::save_buffer(directory.join(format!("{name}.png")), picture.as_bytes(), picture.width, picture.height, image::ColorType::Rgba8).unwrap();
        };
        let (mut prefs, _) = open_prefs("headless-shots");
        click(&mut prefs, 220, 55);
        click(&mut prefs, 530, 377); // A changed Game frame slider.
        for (name, hover) in [("changed-slider", false), ("undo-tooltip", true)] {
            prefs.route(vec![Input::MouseMove { x: if hover { 771 } else { 100 }, y: 377 }], (1024, 768));
            shot(&prefs, name);
        }
        // Every tab, with the conditional controls shown.
        let (mut prefs, _) = open_prefs("headless-tabs");
        let draft = prefs.draft.as_mut().unwrap();
        draft.layout.split = Split::Vertical1To1;
        draft.layout.zoom = Zoom::Floating131;
        draft.display.max_fps = 120;
        draft.accessibility.active_unit_arrow = true;
        draft.accessibility.unit_rings = true;
        prefs.project_settings();
        for (tab, name) in [(TAB_RESOLUTION, "tab-resolution"), (TAB_LAYOUT, "tab-layout"), (TAB_ZOOM, "tab-zoom"),
            (TAB_CLEAN_MAP, "tab-clean-map"), (TAB_ACCESSIBILITY, "tab-accessibility")] {
            prefs.tab = tab.into();
            prefs.route(vec![Input::MouseMove { x: 1000, y: 10 }], (1024, 768));
            shot(&prefs, name);
        }
        // Base UI scale changed and previewed.
        prefs.tab = TAB_RESOLUTION.into();
        click(&mut prefs, 218, 231);
        click(&mut prefs, 621, 230);
        prefs.route(vec![Input::MouseMove { x: 1000, y: 10 }], (1024, 768));
        shot(&prefs, "tab-resolution-base-preview");
        // Accessibility sliders changed: their undo buttons.
        let draft = prefs.draft.as_mut().unwrap();
        draft.accessibility.hud_size = 3;
        draft.accessibility.arrow_size = 4;
        draft.accessibility.ring_width = 5;
        draft.input.snap_updates_per_second = 20.0;
        prefs.tab = TAB_ACCESSIBILITY.into();
        prefs.route(vec![Input::MouseMove { x: 1000, y: 10 }], (1024, 768));
        shot(&prefs, "tab-accessibility-undo");
        // A box at the mouse: its distances above and below, then changed
        // (their undo buttons).
        prefs.draft.as_mut().unwrap().layout.zoom = Zoom::Mouse131;
        prefs.tab = TAB_ZOOM.into();
        prefs.route(vec![Input::MouseMove { x: 1000, y: 10 }], (1024, 768));
        shot(&prefs, "tab-zoom-mouse");
        let draft = prefs.draft.as_mut().unwrap();
        (draft.layout.zoom_mouse_above, draft.layout.zoom_mouse_below) = (0.25, 0.05);
        prefs.route(vec![Input::MouseMove { x: 1000, y: 10 }], (1024, 768));
        shot(&prefs, "tab-zoom-mouse-undo");
        // Every placement with each content, in a vertical split.
        let (mut prefs, _) = open_prefs("headless-zoom");
        prefs.tab = TAB_ZOOM.into();
        for (zoom, name) in [(Zoom::Off, "off"), (Zoom::Floating131, "box"), (Zoom::Floating171, "wide-box"), (Zoom::Mouse111, "mouse"),
            (Zoom::Mouse131, "wide-mouse"), (Zoom::Beside, "beside"), (Zoom::TopStrip, "strip")] {
            for (content, suffix) in [(ZoomContent::World, "world"), (ZoomContent::WorldAndMenus, "menus")] {
                if zoom == Zoom::Off && content == ZoomContent::WorldAndMenus { continue; }
                let draft = prefs.draft.as_mut().unwrap();
                (draft.layout.split, draft.layout.zoom, draft.layout.zoom_content) = (Split::Vertical1To1, zoom, content);
                prefs.project_settings();
                prefs.route(vec![Input::MouseMove { x: 1000, y: 10 }], (1024, 768));
                shot(&prefs, &format!("zoom-{name}-{suffix}"));
            }
        }
        // Tooltips: a placement under the mouse; greyed-out pictures saying
        // why (the zoom off; the standard split).
        prefs.route(vec![Input::MouseMove { x: 702, y: 160 }], (1024, 768));
        shot(&prefs, "zoom-tooltip-placement");
        let draft = prefs.draft.as_mut().unwrap();
        (draft.layout.split, draft.layout.zoom) = (Split::Standard, Zoom::Off);
        prefs.project_settings();
        prefs.route(vec![Input::MouseMove { x: 254, y: 279 }], (1024, 768));
        shot(&prefs, "zoom-off-disabled-tooltip");
        prefs.draft.as_mut().unwrap().layout.zoom = Zoom::Mouse131;
        prefs.project_settings();
        prefs.route(vec![Input::MouseMove { x: 724, y: 279 }], (1024, 768));
        shot(&prefs, "zoom-standard-split-disabled-tooltip");
        let draft = prefs.draft.as_mut().unwrap();
        (draft.layout.split, draft.layout.zoom, draft.layout.zoom_content) = (Split::Vertical1To1, Zoom::Floating131, ZoomContent::WorldAndMenus);
        prefs.project_settings();
        prefs.route(vec![Input::MouseMove { x: 100, y: 540 }], (1024, 768));
        shot(&prefs, "zoom-keep-mouse-out-disabled-tooltip");
    }


    /// Find a slider through its declared geometry, as the viewer does.
    fn slider_for(prefs: &Prefs, key: &str) -> menu::panel::ControlLooks {
        let table: toml::Table = toml::from_str(PANELS).unwrap();
        let item = table["panels"][PANEL]["items"].as_array().unwrap().iter()
            .find(|i| i.get("slider").and_then(|v| v.as_str()) == Some(key)).unwrap();
        let at = item["at"].as_array().unwrap();
        prefs.looks(&Fonts::default()).into_iter().find(|look| matches!(look.kind, scene::InteractableKind::Slider { .. })
            && look.rect[..2] == [at[0].as_integer().unwrap() as i32, at[1].as_integer().unwrap() as i32]).unwrap()
    }

    fn show_slider(prefs: &mut Prefs, key: &str) {
        prefs.tab = if key.starts_with("layout.zoom") { TAB_ZOOM }
            else if key.starts_with("accessibility.") || key == "input.snap_updates_per_second" { TAB_ACCESSIBILITY }
            else if key.starts_with("resolution.") || key.starts_with("display.") { TAB_RESOLUTION }
            else { TAB_LAYOUT }.into();
        prefs.route(vec![], (1024, 768));
    }

    #[test]
    fn every_live_slider_previews_during_drag_and_rolls_back_or_accepts_without_early_save() {
        use scene::SliderAction::*;
        let panels = Panels::parse(PANELS).unwrap();
        let sliders: Vec<_> = panels.panel(PANEL).unwrap().controls().into_iter()
            .filter(|c| panels.panel(PANEL).unwrap().slider_range(c).is_some() && !DEFERRED_CONTROLS.contains(c)).collect();
        assert!(sliders.len() >= 12, "{sliders:?}");
        for control in sliders {
            for finish in ["discard", "closed", "accept"] {
                let (mut prefs, path) = open_prefs(&format!("live-{control}-{finish}"));
                prefs.settings.layout.zoom = if control.starts_with("layout.zoom_mouse") { Zoom::Mouse131 } else { Zoom::Floating131 };
                prefs.settings.display.max_fps = 120;
                prefs.settings.accessibility.unit_rings = true;
                prefs.settings.accessibility.active_unit_arrow = true;
                prefs.draft = Some(prefs.settings.clone());
                prefs.project_settings();
                prefs.settings.save(&path).unwrap();
                let stored = std::fs::read(&path).unwrap();
                let accepted = prefs.settings.clone();
                show_slider(&mut prefs, control);
                let slider = slider_for(&prefs, control);
                let send = |prefs: &mut Prefs, action, value| prefs.route(vec![Input::Slider { id: slider.id, action, value }], (1024, 768));
                send(&mut prefs, Begin, 0);
                send(&mut prefs, Preview, 1_000_000);
                assert_ne!(prefs.editing().control(control), accepted.control(control), "{control}");
                assert_eq!(prefs.settings().control(control), prefs.editing().control(control), "{control} must preview before release");
                assert_eq!(prefs.settings, accepted, "accepted settings remain unchanged");
                assert_eq!(std::fs::read(&path).unwrap(), stored);
                send(&mut prefs, Cancel, 0);
                assert_eq!(prefs.settings(), &accepted, "slider Cancel restores preview for {control}");
                send(&mut prefs, Begin, 0);
                send(&mut prefs, Preview, 1_000_000);
                send(&mut prefs, Commit, 0);
                let changed = prefs.settings().clone();
                match finish {
                    "accept" => {
                        key(&mut prefs, VK_RETURN);
                        assert_eq!(prefs.settings(), &changed);
                        assert_eq!(Settings::load(&path).unwrap(), changed);
                    }
                    "discard" => {
                        key(&mut prefs, VK_ESCAPE);
                        assert!(prefs.confirming_discard);
                        key(&mut prefs, VK_RETURN); // Keep editing retains preview.
                        assert_eq!(prefs.settings(), &changed);
                        key(&mut prefs, VK_ESCAPE);
                        key(&mut prefs, 0x59);
                        assert_eq!(prefs.settings(), &accepted);
                        assert_eq!(std::fs::read(&path).unwrap(), stored);
                    }
                    _ => {
                        prefs.update(None, None, (1024, 768), None);
                        assert_eq!(prefs.settings(), &accepted);
                        assert_eq!(std::fs::read(&path).unwrap(), stored);
                    }
                }
            }
        }
    }

    /// The settings the game shows must be the draft, but for the deferred
    /// controls (accepted values; the base the draft's while previewed).
    fn assert_projected(prefs: &Prefs, accepted: &Settings, what: &str) {
        for (key, drafted) in prefs.editing().controls() {
            let shown = prefs.settings().control(&key);
            let previewed = key == "resolution.base_ui_scale" && prefs.base_preview;
            if DEFERRED_CONTROLS.contains(&key.as_str()) && !previewed {
                assert_eq!(shown, accepted.control(&key), "{what}: {key} waits for Accept");
            } else {
                assert_eq!(shown, Some(drafted), "{what}: {key} previews at once");
            }
        }
        // (In window pixels: a pixel scale changes the canvas, not the size.)
        // (Screen-relative menus round to whole rendered rows: within 1%.)
        let menu = |s: &Settings| {
            let p = tile_viewer::scaling::plan((2048, 1536), (1024, 768), &s.resolution);
            let (n, d) = tile_viewer::scaling::times(p.menu.zoom, p.present.zoom);
            n as f64 / d as f64
        };
        assert!((menu(prefs.settings()) / menu(accepted) - 1.0).abs() < 0.01, "{what}: the dialog keeps its size");
    }

    /// Clicks or slides every control the panel shows on every renderer
    /// tab (looks recomputed after each edit, so controls a choice reveals
    /// are reached too).
    fn exercise_every_control(prefs: &mut Prefs, accepted: &Settings, path: &std::path::Path) -> BTreeSet<String> {
        use scene::{InteractableKind, SliderAction::*};
        let mut changed = BTreeSet::new();
        for tab in [TAB_RESOLUTION, TAB_ZOOM, TAB_LAYOUT, TAB_CLEAN_MAP, TAB_ACCESSIBILITY] {
            prefs.tab = tab.into();
            prefs.route(vec![], (1024, 768));
            let mut tried = BTreeSet::new();
            loop {
                // (The tab buttons lie above the panels, y < 79.)
                let Some(look) = prefs.looks(&Fonts::default()).into_iter()
                    .find(|l| l.rect[1] >= 79 && !tried.contains(&(l.id, l.rect))) else { break };
                tried.insert((look.id, look.rect));
                let before = prefs.editing().clone();
                if let InteractableKind::Slider { .. } = look.kind {
                    for value in [1_000_000, 0] {
                        prefs.route(vec![Input::Slider { id: look.id, action: Begin, value: 0 },
                            Input::Slider { id: look.id, action: Preview, value },
                            Input::Slider { id: look.id, action: Commit, value: 0 }], (1024, 768));
                        if *prefs.editing() != before { break; }
                    }
                } else {
                    let [x, y] = look.anchor.unwrap_or([look.rect[0] + look.rect[2] / 2, look.rect[1] + look.rect[3] / 2]);
                    click(prefs, x, y);
                }
                // The pixel scale's own Keep/Revert dialog: keep.
                if let Some(p) = prefs.preview {
                    prefs.route(vec![Input::SettingsPreviewDecision { session: p.session, id: p.id, keep: true }], (1024, 768));
                }
                assert_eq!(prefs.tab, tab);
                let what = format!("{tab} control at {:?}", look.rect);
                assert_projected(prefs, accepted, &what);
                assert!(!path.exists(), "{what}: nothing saved before Accept");
                for (key, value) in prefs.editing().controls() {
                    if before.control(&key) != Some(value) { changed.insert(key); }
                }
            }
        }
        changed
    }

    #[test]
    fn every_control_previews_at_once_and_reverts_on_discard_or_saves_on_accept() {
        let panels = Panels::parse(PANELS).unwrap();
        let expected: BTreeSet<String> = panels.panel(PANEL).unwrap().controls().into_iter()
            .filter(|c| *c != TAB && !c.starts_with("dialog.")).map(String::from).collect();
        for finish in ["discard", "accept", "closed"] {
            let (mut prefs, path) = open_prefs(&format!("all-controls-{finish}"));
            let s = &mut prefs.settings;
            s.layout.zoom = Zoom::Floating131;
            s.layout.split = Split::Vertical1To1;
            s.display.max_fps = 120;
            s.accessibility.unit_rings = true;
            s.accessibility.active_unit_arrow = true;
            s.resolution.scale_menus = true;
            // (The macOS-only switches shown too.)
            prefs.host_macos = true;
            prefs.draft = Some(prefs.settings.clone());
            prefs.project_settings();
            let accepted = prefs.settings.clone();
            let changed = exercise_every_control(&mut prefs, &accepted, &path);
            let missed: Vec<_> = expected.difference(&changed).collect();
            assert!(missed.is_empty(), "controls never changed by the walk: {missed:?}");
            assert_ne!(prefs.settings(), &accepted);
            let draft = prefs.editing().clone();
            match finish {
                "accept" => {
                    key(&mut prefs, VK_RETURN);
                    assert_eq!(prefs.settings(), &draft);
                    assert_eq!(Settings::load(&path).unwrap(), draft);
                }
                "discard" => {
                    key(&mut prefs, VK_ESCAPE);
                    assert!(prefs.confirming_discard);
                    key(&mut prefs, 0x59);
                    assert_eq!(prefs.settings(), &accepted);
                    assert!(!path.exists());
                }
                _ => {
                    prefs.update(None, None, (1024, 768), None);
                    assert_eq!(prefs.settings(), &accepted);
                    assert!(!path.exists());
                }
            }
        }
    }

    #[test]
    fn base_ui_preview_button_shows_the_draft_base_behind_a_fixed_dialog() {
        let button = |p: &Prefs| p.draws(&Fonts::default()).iter().find_map(|d| match d {
            Draw::Text(t) if t.text == "Preview" || t.text == "Stop preview" => Some(t.text.clone()),
            _ => None,
        });
        let plan = |s: &Settings| tile_viewer::scaling::plan((2048, 1536), (1024, 768), &s.resolution);
        for end in ["again", "tab", "cancel", "undo", "accept"] {
            let (mut prefs, path) = open_prefs(&format!("base-preview-{end}"));
            click(&mut prefs, 220, 55);
            let accepted = prefs.settings().clone();
            let rects = |p: &Prefs| p.looks(&Fonts::default()).iter().filter(|l| l.rect[1] >= 79 && l.rect[0] < 500).map(|l| l.rect).collect::<Vec<_>>();
            assert_eq!(button(&prefs).as_deref(), Some("Preview"));
            click(&mut prefs, 218, 231);
            assert_eq!(prefs.editing().resolution.base_ui_scale, 2.5);
            assert_eq!(prefs.settings().resolution.base_ui_scale, accepted.resolution.base_ui_scale, "waits for Preview");
            let before = rects(&prefs);
            click(&mut prefs, 621, 230);
            assert!(prefs.base_preview);
            assert_eq!(button(&prefs).as_deref(), Some("Stop preview"));
            assert_eq!(prefs.settings().resolution.effective_base_ui_scale(), 2.5);
            assert_eq!(plan(prefs.settings()).menu, plan(&accepted).menu, "the dialog keeps its size");
            assert_ne!(plan(prefs.settings()).frame, plan(&accepted).frame, "the game behind it scales");
            assert_eq!(rects(&prefs), before);
            assert_projected(&prefs, &accepted, "preview");
            // Dragging while previewing follows the slider.
            click(&mut prefs, 251, 231);
            assert_eq!(prefs.settings().resolution.effective_base_ui_scale(), 3.0);
            assert!(!path.exists());
            match end {
                "again" => { click(&mut prefs, 621, 230); }
                "tab" => { click(&mut prefs, 330, 55); }
                "cancel" => {
                    key(&mut prefs, VK_ESCAPE);
                    assert!(!prefs.base_preview);
                    assert_eq!(prefs.settings(), &accepted);
                    key(&mut prefs, 0x59);
                }
                "undo" => {
                    click(&mut prefs, 531, 231);
                    assert_eq!(prefs.editing().resolution.base_ui_scale, accepted.resolution.base_ui_scale);
                    assert!(prefs.base_preview, "undo keeps the preview on, now of the accepted base");
                    assert_eq!(prefs.settings().resolution.effective_base_ui_scale(), accepted.resolution.base_ui_scale);
                    click(&mut prefs, 621, 230);
                }
                _ => {
                    key(&mut prefs, VK_RETURN);
                    assert_eq!(prefs.settings().resolution.base_ui_scale, 3.0);
                    assert_eq!(prefs.settings().resolution.menu_base_ui_scale, None);
                    assert_eq!(Settings::load(&path).unwrap(), *prefs.settings());
                    continue;
                }
            }
            assert!(!prefs.base_preview, "{end} ends the preview");
            assert_eq!(prefs.settings().resolution.base_ui_scale, accepted.resolution.base_ui_scale, "{end}");
            assert_eq!(prefs.settings().resolution.menu_base_ui_scale, None, "{end}");
            if end != "cancel" { assert_eq!(button(&prefs).as_deref().filter(|_| prefs.tab == TAB_RESOLUTION), if end == "tab" { None } else { Some("Preview") }); }
            assert!(!path.exists());
        }
    }

    #[test]
    fn mouse_drags_preview_frame_and_world_while_dialog_stays_fixed_and_undo_uses_latest_accept() {
        let (mut prefs, path) = open_prefs("mouse-preview-latest-accept");
        click(&mut prefs, 220, 55);
        let opening = prefs.settings().resolution.clone();
        let menu = tile_viewer::scaling::plan((2048, 1536), (1024, 768), &opening).menu;
        for control in ["resolution.frame_scale", "resolution.world_scale"] {
            let slider = slider_for(&prefs, control);
            let [x, y, w, h] = slider.rect;
            let down = Input::MouseButton { button: scene::Button::Left, down: true, x: x+10, y: y+h/2 };
            assert!(prefs.route(vec![down], (1024, 768)).is_empty());
            assert_eq!(prefs.settings().control(control), Some(ControlValue::Number(0.5)));
            assert!(prefs.route(vec![Input::MouseMove { x: x+w/2, y: y+h/2 }], (1024, 768)).is_empty());
            assert_eq!(prefs.settings().control(control), Some(ControlValue::Number(1.75)), "motion previews before release");
            assert_eq!(tile_viewer::scaling::plan((2048, 1536), (1024, 768), &prefs.settings().resolution).menu, menu);
            assert_eq!(slider_for(&prefs, control).rect, slider.rect);
            assert!(!path.exists());
            prefs.route(vec![Input::MouseButton { button: scene::Button::Left, down: false, x: x+w/2, y: y+h/2 }], (1024, 768));
        }
        key(&mut prefs, VK_RETURN);
        let saved = std::fs::read(&path).unwrap();
        prefs.update(None, None, (1024, 768), None);
        let frame = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
        prefs.update(Some(&frame), Some(FORM), (1024, 768), None);
        click(&mut prefs, 220, 55);
        for _ in 0..2 {
            click(&mut prefs, 658, 409);
            assert_eq!(prefs.settings().resolution.world_scale, 3.0);
            click(&mut prefs, 771, 409);
            assert_eq!(prefs.settings().resolution.world_scale, 1.75, "undo uses this session's accepted value");
            assert!(!prefs.dirty(), "undoing the only edit leaves a clean dialog");
            assert_eq!(std::fs::read(&path).unwrap(), saved);
        }
    }

    #[test]
    fn every_slider_has_conditional_undo_and_the_exact_tooltip() {
        use scene::SliderAction::*;
        let table: toml::Table = toml::from_str(PANELS).unwrap();
        let items = table["panels"][PANEL]["items"].as_array().unwrap();
        let mut checked = 0;
        for item in items.iter().filter(|i| i.get("slider").is_some()) {
            let control = item["slider"].as_str().unwrap();
            let (mut prefs, path) = open_prefs(&format!("undo-{control}"));
            prefs.settings.resolution.scale_menus = true;
            prefs.settings.display.max_fps = 120;
            prefs.settings.accessibility.unit_rings = true;
            prefs.settings.accessibility.active_unit_arrow = true;
            prefs.settings.layout.zoom = if control.starts_with("layout.zoom_mouse") { Zoom::Mouse131 } else { Zoom::Floating131 };
            prefs.draft = Some(prefs.settings.clone());
            prefs.project_settings();
            let accepted = prefs.settings.clone();
            show_slider(&mut prefs, control);
            let undo = items.iter().find(|i| i.get("choice").and_then(|v| v.as_str()) == Some(UNDO)
                && i["value"].as_str() == Some(control)).unwrap();
            let at = undo["at"].as_array().unwrap();
            let at = (at[0].as_integer().unwrap() as i32, at[1].as_integer().unwrap() as i32);
            let find_undo = |p: &Prefs| p.looks(&Fonts::default()).into_iter().find(|look| look.rect == [at.0, at.1, 23, 23]);
            assert!(find_undo(&prefs).is_none(), "unchanged {control}");
            let slider = slider_for(&prefs, control);
            let range = viewer_settings::controls::range(control).unwrap();
            let value = if accepted.control(control) == Some(ControlValue::Number(range.max)) { 0 } else { 1_000_000 };
            prefs.route(vec![Input::Slider { id: slider.id, action: Begin, value: 0 },
                Input::Slider { id: slider.id, action: Preview, value },
                Input::Slider { id: slider.id, action: Commit, value: 0 }], (1024, 768));
            let look = find_undo(&prefs).unwrap_or_else(|| panic!("changed {control} needs undo"));
            assert_eq!(look.kind, scene::InteractableKind::Button, "Alt target");
            assert!(look.states.iter().any(|(state, draws)| state.pointer == scene::PointerState::Hover
                && draws.iter().any(|d| matches!(d, Draw::Text(t) if t.text == "Undo to current state"))), "immediate viewer tooltip for {control}");
            prefs.route(vec![Input::MouseMove { x: at.0 + 11, y: at.1 + 11 }], (1024, 768));
            assert!(prefs.draws(&Fonts::default()).iter().any(|d| matches!(d, Draw::Text(t) if t.text == "Undo to current state")));
            // Undo remains reachable after disabling its parent slider.
            if control == "resolution.menu_scale_percent" { prefs.draft.as_mut().unwrap().resolution.scale_menus = false; }
            if control == "display.fps_limit" { prefs.draft.as_mut().unwrap().display.max_fps = 0; }
            if control == "accessibility.arrow_size" { prefs.draft.as_mut().unwrap().accessibility.active_unit_arrow = false; }
            prefs.route(vec![], (1024, 768));
            assert!(find_undo(&prefs).is_some());
            // Unrelated draft edits survive the per-slider restore.
            prefs.draft.as_mut().unwrap().display.show_fps = !accepted.display.show_fps;
            assert!(click(&mut prefs, at.0 + 11, at.1 + 11).is_empty());
            assert_eq!(prefs.editing().control(control), accepted.control(control));
            assert_eq!(prefs.settings().control(control), accepted.control(control));
            assert!(find_undo(&prefs).is_none(), "restored {control}");
            assert_ne!(prefs.editing().display.show_fps, accepted.display.show_fps);
            assert!(!path.exists(), "undo never saves");
            checked += 1;
        }
        assert_eq!(checked, 18);
    }

    #[test]
    fn base_ui_scale_drafts_half_steps_accepts_and_can_be_discarded() {
        let (mut prefs, path) = open_prefs("base-ui-scale");
        click(&mut prefs, 220, 55);
        let resolution = prefs.settings().resolution.clone();
        for (x, value) in [(119, 1.0), (218, 2.5), (449, 6.0), (185, 2.0)] {
            assert!(click(&mut prefs, x, 231).is_empty());
            assert_eq!(prefs.editing().resolution.base_ui_scale, value);
            assert_eq!(prefs.settings().resolution, resolution, "the open dialog stays fixed");
            assert!(!path.exists());
        }
        assert_eq!(changed_controls(prefs.settings(), prefs.editing()), ["Base UI scale: 1 to 2".to_owned()]);
        key(&mut prefs, VK_ESCAPE);
        assert!(prefs.confirming_discard);
        key(&mut prefs, 0x59);
        assert_eq!(prefs.settings().resolution, resolution);
        // Open a fresh draft and accept it through the normal native transaction.
        prefs.update(None, None, (1024, 768), None);
        let frame = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
        prefs.update(Some(&frame), Some(FORM), (1024, 768), None);
        click(&mut prefs, 220, 55);
        click(&mut prefs, 218, 231);
        key(&mut prefs, VK_RETURN);
        assert_eq!(prefs.settings().resolution.base_ui_scale, 2.5);
        assert_eq!(Settings::load(&path).unwrap().resolution, prefs.settings().resolution);
    }

    #[test]
    fn scale_slider_draft_keeps_dialog_and_native_pointer_mapping_fixed() {
        use scene::{InteractableKind, SliderAction};
        let (mut prefs, path) = open_prefs("transaction-scale-mapping");
        assert!(click(&mut prefs, 220, 55).is_empty());
        assert!(click(&mut prefs, 100, 349).is_empty());
        let panel_rects = || prefs.looks(&Fonts::default()).iter().map(|look| (look.id, look.rect)).collect::<Vec<_>>();
        let before = panel_rects();
        let slider = prefs.looks(&Fonts::default()).into_iter().find(|look| {
            matches!(look.kind, InteractableKind::Slider { .. }) && look.rect[1] == 334
        }).unwrap();
        let active_resolution = prefs.settings().resolution.clone();
        let before_window = active_resolution.realised((2048, 1536));
        let before_pixel_scale = active_resolution.pixel_scale_in((2048, 1536));
        prefs.route(vec![Input::Slider { id: slider.id, action: SliderAction::Begin, value: 0 }], (1024, 768));
        for value in [0, 500_000, 750_000] {
            assert!(prefs.route(vec![Input::Slider { id: slider.id, action: SliderAction::Preview, value }], (1024, 768)).is_empty());
            assert_ne!(prefs.editing().resolution.menu_scale_percent, active_resolution.menu_scale_percent);
            assert_eq!(prefs.settings().resolution, active_resolution);
            assert_eq!(prefs.settings().resolution.realised((2048, 1536)), before_window);
            assert_eq!(prefs.settings().resolution.pixel_scale_in((2048, 1536)), before_pixel_scale);
            assert_eq!(prefs.looks(&Fonts::default()).iter().filter(|look| before.iter().any(|(id, _)| *id == look.id)).map(|look| (look.id, look.rect)).collect::<Vec<_>>(), before);
            assert!(!path.exists());
        }
        assert!(key(&mut prefs, VK_RETURN).is_empty());
        assert!(prefs.native_accept_pending());
        assert_eq!(Settings::load(&path).unwrap(), *prefs.settings());
        assert_ne!(prefs.settings().resolution.menu_scale_percent, active_resolution.menu_scale_percent);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn arrow_keys_step_the_clicked_menu_scale_slider_in_the_draft_with_the_dialog_fixed() {
        use scene::{Interactable, InteractableKind, Scene, View, ViewState};
        use tile_viewer::{layout::Layout, snap::{Rates, SnapInput}};
        let (mut prefs, path) = open_prefs("keyboard-slider-scale");
        assert!(click(&mut prefs, 220, 55).is_empty());
        assert!(click(&mut prefs, 100, 349).is_empty());
        let rects = |prefs: &Prefs| prefs.looks(&Fonts::default()).iter().map(|look| (look.id, look.rect)).collect::<Vec<_>>();
        let before = rects(&prefs);
        let slider = prefs.looks(&Fonts::default()).into_iter()
            .find(|look| matches!(look.kind, InteractableKind::Slider { .. }) && look.rect[1] == 334).unwrap();
        let scene = Scene {
            seq: 1,
            views: vec![ViewState { id: 0, view: View { interface_only: true, ..Default::default() }, overlay: vec![] }],
            interactables: vec![Interactable { region: None, alt_right_click: false, id: slider.id, rect: slider.rect, anchor: None, kind: slider.kind }],
            ..Default::default()
        };
        let mut layout = Layout::default();
        layout.frame(&scene, (1024, 768), (0, 0, 0, 0), vec![], None);
        let mut snap = SnapInput::default();
        let mut send = |prefs: &mut Prefs, input| prefs.route(snap.route(input, &mut layout, &scene, 0.0, Rates::default()), (1024, 768));
        let arrow = |vk, down| Input::Key { vk, down, text: None };
        // Nothing selected: the arrow is the game's.
        assert_eq!(send(&mut prefs, arrow(0x25, true)), [arrow(0x25, true)]);
        send(&mut prefs, arrow(0x25, false));
        assert_eq!(prefs.editing().resolution.menu_scale_percent, 100.0);
        // A click on the slider selects it (and sets it there, as before).
        let [x, y, w, h] = slider.rect;
        let at = (x + w / 2, y + h / 2);
        for down in [true, false] {
            send(&mut prefs, Input::MouseButton { button: scene::Button::Left, down, x: at.0, y: at.1 });
        }
        let start = prefs.editing().resolution.menu_scale_percent;
        assert!(start < 100.0 && start > 25.0, "{start}");
        for (vk, expected) in [(0x25, start - 1.0), (0x27, start), (0x27, start + 1.0)] {
            assert!(send(&mut prefs, arrow(vk, true)).is_empty(), "no arrow reaches the game");
            assert!(send(&mut prefs, arrow(vk, false)).is_empty());
            assert_eq!(prefs.editing().resolution.menu_scale_percent, expected);
        }
        // Shift: ten steps, clamped at the minimum.
        send(&mut prefs, arrow(0x10, true));
        for _ in 0..10 { assert!(send(&mut prefs, arrow(0x26, true)).is_empty()); }
        send(&mut prefs, arrow(0x26, false));
        send(&mut prefs, arrow(0x10, false));
        assert_eq!(prefs.editing().resolution.menu_scale_percent, 25.0);
        // The draft only: nothing applied or saved, the dialog the same size.
        assert_eq!(prefs.settings().resolution.menu_scale_percent, 100.0);
        assert_eq!(rects(&prefs).into_iter().filter(|(id, _)| before.iter().any(|(old, _)| id == old)).collect::<Vec<_>>(), before);
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn native_slider_steps_take_the_native_delivery_path() {
        use scene::SliderAction;
        let (mut prefs, path) = open_prefs("keyboard-slider-native");
        prefs.follow_native(|| Some(native::test_values(false)), true);
        let step = Input::Slider { id: 0x0123_4567, action: SliderAction::Step, value: -1 };
        assert_eq!(prefs.route(vec![step.clone()], (1024, 768)), [step]);
        assert!(prefs.native_pending, "the draft waits for the game's receipt");
        assert!(prefs.dirty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn menu_size_slider_is_enabled_only_by_its_drafted_checkbox() {
        use scene::InteractableKind;
        let (mut prefs, path) = open_prefs("transaction-menu-size");
        click(&mut prefs, 220, 55);
        let menu_slider = |prefs: &Prefs| prefs.looks(&Fonts::default()).into_iter()
            .find(|look| matches!(look.kind, InteractableKind::Slider { .. }) && look.rect[1] == 334);
        assert!(menu_slider(&prefs).is_none());
        assert!(click(&mut prefs, 270, 349).is_empty());
        assert_eq!(prefs.editing().resolution.menu_scale_percent, 100.0);
        assert!(click(&mut prefs, 100, 349).is_empty());
        assert!(prefs.editing().resolution.scale_menus);
        assert!(!prefs.settings().resolution.scale_menus);
        assert!(menu_slider(&prefs).is_some());
        for (x, percent) in [(270, 25.0), (464, 63.0), (658, 100.0)] {
            assert!(click(&mut prefs, x, 349).is_empty());
            assert_eq!(prefs.editing().resolution.menu_scale_percent, percent);
            assert_eq!(prefs.settings().resolution.menu_scale_percent, 100.0);
            assert!(!path.exists());
        }
        assert!(click(&mut prefs, 100, 349).is_empty());
        assert!(menu_slider(&prefs).is_none());
        assert!(!prefs.dirty());
    }

    #[test]
    fn confirmation_key_repeat_cannot_choose_a_different_action() {
        let (mut prefs, path) = open_prefs("transaction-key-repeat");
        click(&mut prefs, 330, 55);
        click(&mut prefs, 340, 140);
        let escape_down = Input::Key { vk: VK_ESCAPE, down: true, text: None };
        assert!(prefs.route(vec![escape_down.clone(), escape_down], (1024, 768)).is_empty());
        assert!(prefs.confirming_discard, "holding Escape must not dismiss its new confirmation");
        assert!(prefs.route(vec![Input::Key { vk: VK_ESCAPE, down: false, text: None }], (1024, 768)).is_empty());
        assert!(key(&mut prefs, 0x4E).is_empty(), "No's release must not leak into the native dialog");
        assert!(!prefs.confirming_discard);
        assert!(!path.exists());
    }

    #[test]
    fn native_only_changes_and_pending_native_clicks_also_require_discard_confirmation() {
        for pending in [false, true] {
            let (mut prefs, path) = open_prefs(&format!("transaction-native-{pending}"));
            prefs.follow_native(|| Some(native::test_values(false)), true);
            if pending {
                assert_eq!(click(&mut prefs, 110, 150).len(), 3);
                assert!(prefs.native_pending);
            } else {
                prefs.follow_native(|| Some(native::test_values(true)), false);
            }
            assert!(prefs.dirty());
            assert_eq!(prefs.editing(), prefs.settings(), "only the native option changed");
            assert!(key(&mut prefs, VK_ESCAPE).is_empty());
            assert!(prefs.confirming_discard);
            assert!(key(&mut prefs, 0x4E).is_empty());
            assert!(!prefs.confirming_discard);
            assert!(prefs.dirty());
            key(&mut prefs, VK_ESCAPE);
            assert_eq!(key(&mut prefs, 0x59), native_close(VK_ESCAPE));
            assert!(!path.exists());
        }
    }

    #[test]
    fn native_state_read_clears_a_speculative_click_when_nothing_changed() {
        let (mut prefs, path) = open_prefs("transaction-native-noop");
        prefs.follow_native(|| Some(native::test_values(false)), true);
        click(&mut prefs, 110, 150);
        assert!(prefs.dirty());
        prefs.follow_native(|| Some(native::test_values(false)), false);
        assert!(prefs.dirty(), "a successful read may still precede queued native input");
        delivery::acknowledge_current(&prefs.native_delivery);
        prefs.follow_native(|| Some(native::test_values(false)), false);
        assert!(!prefs.dirty());
        assert_eq!(key(&mut prefs, VK_ESCAPE), native_close(VK_ESCAPE));
        assert!(!path.exists());
    }

    #[test]
    fn alt_clicks_confirm_discard_or_return_to_the_same_draft() {
        use scene::{Interactable, Scene, View, ViewState};
        use tile_viewer::{layout::Layout, snap::{Rates, SnapInput}};
        for discard in [false, true] {
            let (mut prefs, path) = open_prefs(&format!("transaction-alt-confirm-{discard}"));
            click(&mut prefs, 330, 55);
            click(&mut prefs, 340, 140);
            let draft = prefs.editing().clone();
            key(&mut prefs, VK_ESCAPE);
            let looks = prefs.looks(&Fonts::default());
            assert_eq!(looks.len(), 2);
            let selected = looks.into_iter().find(|look| (look.rect[0] == 404) == discard).unwrap();
            let scene = Scene {
                seq: 1,
                views: vec![ViewState { id: 0, view: View { interface_only: true, ..Default::default() }, overlay: vec![] }],
                interactables: vec![Interactable { region: None, alt_right_click: false, id: selected.id, rect: selected.rect, anchor: None, kind: selected.kind }],
                ..Default::default()
            };
            let mut layout = Layout::default();
            layout.frame(&scene, (1024, 768), (0, 0, 0, 0), vec![], None);
            let mut snap = SnapInput::default();
            let rates = Rates::default();
            for down in [true, false] {
                let routed = snap.route(Input::Key { vk: 0x12, down, text: None }, &mut layout, &scene, 0.0, rates);
                assert!(prefs.route(routed, (1024, 768)).is_empty());
            }
            assert!(prefs.route(snap.tick(&mut layout, &scene, 1.0, rates), (1024, 768)).is_empty());
            let mut forwarded = Vec::new();
            for down in [true, false] {
                let routed = snap.route(Input::MouseButton { button: scene::Button::Left, down, x: 0, y: 0 }, &mut layout, &scene, 1.1, rates);
                forwarded.extend(prefs.route(routed, (1024, 768)));
            }
            assert_eq!(forwarded, if discard { native_close(VK_ESCAPE) } else { vec![] });
            assert!(!prefs.confirming_discard);
            assert_eq!(prefs.editing(), if discard { prefs.settings() } else { &draft });
            assert_eq!(prefs.settings(), &if discard { Settings::default() } else { draft.clone() });
            assert!(!path.exists());
        }
    }

    #[test]
    fn native_key_and_world_key_edits_before_escape_always_ask_in_the_same_batch() {
        for world in [false, true] {
            for vk in [0x20, 0x25, 0x26, 0x27, 0x28] {
                let (mut prefs, path) = open_prefs(&format!("transaction-native-key-{world}-{vk}"));
                prefs.follow_native(|| Some(native::test_values(false)), true);
                let key = |vk, down| if world { Input::WorldKey { vk, down, text: None, x: 100, y: 100 } }
                    else { Input::Key { vk, down, text: None } };
                let mutation = key(vk, true);
                let out = prefs.route(vec![mutation.clone(), key(vk, false), key(VK_ESCAPE, true), key(VK_ESCAPE, false)], (1024, 768));
                assert_eq!(out, [mutation, key(vk, false)]);
                assert!(prefs.confirming_discard, "world={world} key={vk:#x}");
                assert!(!path.exists());
            }
        }
    }

    #[test]
    fn plain_modifiers_do_not_mark_native_settings_dirty() {
        let (mut prefs, _) = open_prefs("transaction-modifiers");
        for vk in [0x10, 0x11, 0x12] { key(&mut prefs, vk); }
        assert!(!prefs.dirty());
    }

    #[test]
    fn delivery_acknowledged_during_a_stale_snapshot_only_qualifies_the_next_read() {
        let (mut prefs, _) = open_prefs("transaction-fence-snapshot-order");
        prefs.follow_native(|| Some(native::test_values(false)), true);
        click(&mut prefs, 110, 150);
        let receipt = prefs.native_delivery.receipt().unwrap();
        prefs.follow_native(|| {
            let stale = native::test_values(false);
            receipt.acknowledge(); // native edit+fence arrived after the old read
            Some(stale)
        }, false);
        assert!(prefs.native_pending, "a late acknowledgment cannot validate an already captured snapshot");
        assert!(prefs.dirty());
        prefs.follow_native(|| Some(native::test_values(true)), false);
        assert!(!prefs.native_pending, "the later read starts after the fence acquire");
        assert!(prefs.dirty(), "the acknowledged edit is now represented by its changed native value");
        assert!(key(&mut prefs, VK_ESCAPE).is_empty());
        assert!(prefs.confirming_discard);
    }

    #[test]
    fn covered_popup_dismissal_holds_later_escape_until_a_native_state_boundary() {
        for keyboard in [false, true] {
            let (mut prefs, _) = open_prefs(&format!("transaction-popup-boundary-{keyboard}"));
            click(&mut prefs, 330, 55);
            click(&mut prefs, 340, 140);
            let draft = prefs.editing().clone();
            let frame = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
            prefs.update(Some(&frame), Some(FORM), (1024, 768), Some(0x1234_5678));
            let popup = if keyboard { native_close(VK_ESCAPE) } else {
                [true, false].map(|down| Input::MouseButton { button: scene::Button::Left, down, x: 787, y: 172 }).into()
            };
            let batch = popup.iter().cloned().chain(native_close(VK_ESCAPE)).collect();
            assert_eq!(prefs.route(batch, (1024, 768)), popup, "dependent Escape must not reach the underlying dirty Preferences");
            assert!(prefs.covered_waiting);
            assert_eq!(prefs.deferred_inputs.len(), 2);
            // Even a current-looking modal read cannot release input until
            // the posted gesture and fence have actually been delivered.
            prefs.update(Some(&frame), Some(FORM), (1024, 768), None);
            assert!(prefs.route(vec![], (1024, 768)).is_empty());
            assert!(prefs.covered_waiting);
            delivery::acknowledge_current(&prefs.native_delivery);
            prefs.update(Some(&frame), Some(FORM), (1024, 768), None);
            assert!(prefs.route(vec![], (1024, 768)).is_empty());
            assert!(prefs.confirming_discard);
            assert_eq!(prefs.editing(), &draft);
            assert!(prefs.deferred_inputs.is_empty());
        }
    }

    fn dirty_covered_prefs(name: &str) -> (Prefs, Frame) {
        let (mut prefs, _) = open_prefs(name);
        click(&mut prefs, 330, 55);
        click(&mut prefs, 340, 140);
        let frame = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
        prefs.update(Some(&frame), Some(FORM), (1024, 768), Some(0x1234_5678));
        (prefs, frame)
    }

    #[test]
    fn delivered_popup_input_that_keeps_the_popup_open_replays_to_the_popup() {
        let (mut prefs, frame) = dirty_covered_prefs("transaction-popup-stays");
        let escape = native_close(VK_ESCAPE);
        assert_eq!(prefs.route(escape.iter().cloned().chain(escape.clone()).collect(), (1024, 768)), escape);
        delivery::acknowledge_current(&prefs.native_delivery);
        prefs.update(Some(&frame), Some(FORM), (1024, 768), Some(0x1234_5678));
        assert_eq!(prefs.route(vec![], (1024, 768)), native_close(VK_ESCAPE));
        assert!(!prefs.confirming_discard);
        assert!(prefs.covered_waiting);
        assert!(prefs.dirty());
    }

    #[test]
    fn input_held_behind_an_unacknowledged_popup_fence_is_dropped_not_replayed_late() {
        let (mut prefs, frame) = dirty_covered_prefs("transaction-popup-stale");
        let escape = native_close(VK_ESCAPE);
        let start = std::time::Instant::now();
        let window = (1024, 768);
        assert_eq!(prefs.route_at(escape.iter().cloned().chain(escape.clone()).collect(), window, start), escape);
        assert_eq!(prefs.deferred_inputs.len(), 2);
        // Within the bound the fence holds the dependent Escape as before.
        assert!(prefs.route_at(vec![], window, start + DEFERRED_STALE / 2).is_empty());
        assert_eq!(prefs.deferred_inputs.len(), 2);
        // The game thread stalls past the bound: the waiting Escape (press
        // and release) is dropped; later input still waits for the fence.
        let later = start + DEFERRED_STALE + std::time::Duration::from_millis(1);
        let moved = Input::MouseMove { x: 5, y: 5 };
        let click = [true, false].map(|down| Input::MouseButton { button: scene::Button::Left, down, x: 6, y: 6 });
        assert!(prefs.route_at(vec![moved.clone(), click[0].clone()], window, later).is_empty());
        assert_eq!(prefs.deferred_inputs, VecDeque::from([moved.clone(), click[0].clone()]));
        assert!(prefs.covered_waiting, "dropping stale input never releases the fence by itself");
        // Stalled again: the new press goes, the move stays; its release,
        // arriving later, is swallowed rather than sent unmatched.
        let much_later = later + DEFERRED_STALE * 2;
        assert!(prefs.route_at(vec![click[1].clone()], window, much_later).is_empty());
        assert_eq!(prefs.deferred_inputs, VecDeque::from([moved.clone()]));
        delivery::acknowledge_current(&prefs.native_delivery);
        prefs.update(Some(&frame), Some(FORM), window, None);
        prefs.route_at(vec![], window, much_later);
        assert!(!prefs.confirming_discard, "the stale Escape never reached the dialog");
        assert!(prefs.deferred_inputs.is_empty() && prefs.deferred_since.is_none() && prefs.stale_releases.is_empty());
        // Fresh input flows normally afterwards.
        assert_eq!(prefs.route_at(native_close(VK_ESCAPE), window, much_later).len(), 0);
        assert!(prefs.confirming_discard, "a fresh Escape asks about the dirty draft");
    }

    #[test]
    fn popup_boundary_requires_a_modal_read_after_ack_and_preserves_quit_and_metadata() {
        let (mut prefs, frame) = dirty_covered_prefs("transaction-popup-modal-read");
        let escape = native_close(VK_ESCAPE);
        prefs.route(escape.iter().cloned().chain(escape.clone()).collect(), (1024, 768));
        let receipt = prefs.native_delivery.receipt().unwrap();
        prefs.update_with_modal(Some(&frame), Some(FORM), (1024, 768), || {
            let stale_popup = Some(0x1234_5678);
            receipt.acknowledge();
            Some(stale_popup)
        });
        assert!(prefs.covered_waiting, "the acquire must precede the modal read, as well as the settings read");
        let metadata = Input::Window { width: 1920, height: 1080, scale_milli: 1000 };
        assert_eq!(prefs.route(vec![metadata.clone(), Input::Quit], (1024, 768)), [metadata, Input::Quit]);
        assert_eq!(prefs.deferred_inputs.len(), 2);
        prefs.update_with_modal(Some(&frame), Some(FORM), (1024, 768), || None);
        assert!(prefs.covered_waiting, "an unreadable modal pointer cannot validate a transition");
        assert!(prefs.route(vec![], (1024, 768)).is_empty());
        prefs.update_with_modal(Some(&frame), Some(FORM), (1024, 768), || Some(None));
        assert!(prefs.route(vec![], (1024, 768)).is_empty());
        assert!(prefs.confirming_discard);
    }

    #[test]
    fn previous_dialog_fence_cannot_release_reopened_preferences_popup_input() {
        let (mut prefs, frame) = dirty_covered_prefs("transaction-popup-old-dialog");
        prefs.route(native_close(VK_ESCAPE), (1024, 768));
        let old = prefs.native_delivery.receipt().unwrap();
        prefs.update(None, Some(FORM), (1024, 768), None);
        prefs.update(Some(&frame), Some(FORM), (1024, 768), None);
        click(&mut prefs, 330, 55);
        click(&mut prefs, 340, 140);
        prefs.update(Some(&frame), Some(FORM), (1024, 768), Some(0x1234_5678));
        let escape = native_close(VK_ESCAPE);
        prefs.route(escape.iter().cloned().chain(escape.clone()).collect(), (1024, 768));
        old.acknowledge();
        prefs.update(Some(&frame), Some(FORM), (1024, 768), None);
        assert!(prefs.covered_waiting);
        assert!(prefs.route(vec![], (1024, 768)).is_empty());
        assert!(!prefs.confirming_discard);
        delivery::acknowledge_current(&prefs.native_delivery);
        prefs.update(Some(&frame), Some(FORM), (1024, 768), None);
        assert!(prefs.route(vec![], (1024, 768)).is_empty());
        assert!(prefs.confirming_discard);
    }

    #[test]
    fn popup_key_release_still_reaches_native_input_after_modal_dismissal() {
        for world in [false, true] {
            let (mut prefs, frame) = dirty_covered_prefs(&format!("transaction-popup-key-release-{world}"));
            let event = |down| if world { Input::WorldKey { vk: VK_ESCAPE, down, text: None, x: 100, y: 100 } }
                else { Input::Key { vk: VK_ESCAPE, down, text: None } };
            assert_eq!(prefs.route(vec![event(true)], (1024, 768)), [event(true)]);
            delivery::acknowledge_current(&prefs.native_delivery);
            prefs.update(Some(&frame), Some(FORM), (1024, 768), None);
            // A held popup key cannot become a new Preferences cancellation.
            assert!(prefs.route(vec![event(true)], (1024, 768)).is_empty());
            assert!(!prefs.confirming_discard);
            assert_eq!(prefs.route(vec![event(false), event(true), event(false)], (1024, 768)), [event(false)]);
            assert!(prefs.confirming_discard);
            assert!(prefs.covered_releases.is_empty());
        }
    }

    #[test]
    fn clean_preferences_after_popup_dismissal_cancel_without_confirmation() {
        let (mut prefs, _) = open_prefs("transaction-popup-clean");
        let frame = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
        prefs.update(Some(&frame), Some(FORM), (1024, 768), Some(0x1234_5678));
        let escape = native_close(VK_ESCAPE);
        assert_eq!(prefs.route(escape.iter().cloned().chain(escape.clone()).collect(), (1024, 768)), escape);
        delivery::acknowledge_current(&prefs.native_delivery);
        prefs.update(Some(&frame), Some(FORM), (1024, 768), None);
        assert_eq!(prefs.route(vec![], (1024, 768)), native_close(VK_ESCAPE));
        assert!(!prefs.confirming_discard);
    }

    #[test]
    fn deferred_keys_use_their_original_modifier_order_when_replayed() {
        let (mut prefs, frame) = dirty_covered_prefs("transaction-popup-modifier-order");
        let draft = prefs.editing().clone();
        prefs.route(native_close(VK_ESCAPE), (1024, 768));
        let events = vec![Input::Key { vk: VK_BACK, down: true, text: None }, Input::Key { vk: VK_BACK, down: false, text: None },
            Input::Key { vk: VK_CONTROL, down: true, text: None }, Input::Key { vk: VK_SHIFT, down: true, text: None }];
        assert!(prefs.route(events.clone(), (1024, 768)).is_empty());
        assert!(!prefs.ctrl && !prefs.shift, "deferred modifiers must not change state before earlier events replay");
        delivery::acknowledge_current(&prefs.native_delivery);
        prefs.update(Some(&frame), Some(FORM), (1024, 768), None);
        assert_eq!(prefs.route(vec![], (1024, 768)), events);
        assert_eq!(prefs.editing(), &draft, "the earlier Backspace was never Ctrl+Shift+Backspace");
    }

    #[test]
    fn pure_text_popup_input_does_not_require_or_invent_a_key_release() {
        let (mut prefs, frame) = dirty_covered_prefs("transaction-popup-pure-text");
        let text = |c| Input::Key { vk: 0, down: true, text: Some(c) };
        assert_eq!(prefs.route(vec![text('a'), text('b')], (1024, 768)), [text('a')]);
        assert!(prefs.covered_releases.is_empty());
        delivery::acknowledge_current(&prefs.native_delivery);
        prefs.update(Some(&frame), Some(FORM), (1024, 768), Some(0x1234_5678));
        assert_eq!(prefs.route(vec![], (1024, 768)), [text('b')]);
        assert!(prefs.covered_releases.is_empty());
    }

    #[test]
    fn native_drag_move_and_release_stay_pending_after_an_unchanged_knob_press() {
        for release in [false, true] {
            let (mut prefs, _) = open_prefs(&format!("transaction-native-drag-{release}"));
            prefs.follow_native(|| Some(native::test_values(false)), true);
            let press = Input::MouseButton { button: scene::Button::Left, down: true, x: 400, y: 560 };
            assert_eq!(prefs.route(vec![press.clone()], (1024, 768)), [press]);
            delivery::acknowledge_current(&prefs.native_delivery);
            prefs.follow_native(|| Some(native::test_values(false)), false);
            assert!(!prefs.dirty(), "pressing an existing knob position may not change it yet");
            let drag = if release { Input::MouseButton { button: scene::Button::Left, down: false, x: 600, y: 560 } }
                else { Input::MouseMove { x: 600, y: 560 } };
            let inputs = std::iter::once(drag.clone()).chain(native_close(VK_ESCAPE)).collect();
            assert_eq!(prefs.route(inputs, (1024, 768)), [drag]);
            assert!(prefs.confirming_discard, "an unacknowledged drag/release can still change the native slider");
        }
    }

    #[test]
    fn tab_selection_waits_for_the_committed_panel_commands_while_hover_stays_immediate() {
        use tile_viewer::looks::{Feedback, LookScene};
        let (mut prefs, _) = open_prefs("atomic-tab-selection");
        let fonts = Fonts::default();
        let before_draws = prefs.draws(&fonts);
        let metadata = |prefs: &Prefs| {
            let controls = prefs.looks(&fonts);
            LookScene {
                interactables: controls.iter().map(|l| scene::Interactable { rect: l.rect, id: l.id, kind: l.kind, ..Default::default() }).collect(),
                looks: controls.iter().map(|l| scene::Looks { id: l.id, select: l.select, shown: l.shown,
                    views: vec![scene::ViewLooks { view: 255, sprites: (0, 1), states: l.states.iter().map(|(state, _)|
                        (*state, vec![scene::FreeSprite { set: if state.selected { 10 } else { 20 }, ..Default::default() }])).collect() }] }).collect(),
            }
        };
        let before = metadata(&prefs);
        let layout_id = before.interactables.iter().find(|i| 330 >= i.rect[0] && 330 < i.rect[0] + i.rect[2] && 55 >= i.rect[1] && 55 < i.rect[1] + i.rect[3]).unwrap().id;
        let mut feedback = Feedback::default();
        feedback.sent(&Input::MouseMove { x: 330, y: 55 }, &before, 0.0);
        feedback.replacements(&before, 0.001);
        assert!(feedback.drawn().iter().any(|(id, state)| *id == layout_id && state.pointer == scene::PointerState::Hover && !state.selected));
        for down in [true, false] { feedback.sent(&Input::MouseButton { button: scene::Button::Left, down, x: 330, y: 55 }, &before, 0.002); }
        for now in [0.003, 0.5, 2.0] {
            feedback.replacements(&before, now);
            assert!(!feedback.drawn().iter().any(|(id, state)| *id == layout_id && state.selected), "Layout cannot become selected over the committed CivIII body");
            assert_eq!(prefs.draws(&fonts), before_draws);
        }
        assert!(click(&mut prefs, 330, 55).is_empty());
        let complete_draws = prefs.draws(&fonts);
        assert_ne!(complete_draws, before_draws);
        let committed = metadata(&prefs);
        assert!(committed.looks.iter().find(|l| l.id == layout_id).unwrap().shown.selected);
        assert_eq!(prefs.tab, TAB_LAYOUT);
        feedback.replacements(&committed, 2.01);
        assert!(!feedback.drawn().iter().any(|(id, state)| *id == layout_id && !state.selected));
    }

    #[test]
    fn pixel_scale_selection_commits_with_its_derived_resolution_readout() {
        use tile_viewer::looks::{Feedback, LookScene};
        let (mut prefs, _) = open_prefs("atomic-derived-readout");
        assert!(click(&mut prefs, 220, 55).is_empty());
        let fonts = Fonts::default();
        let before_draws = prefs.draws(&fonts);
        let old = prefs.looks(&fonts).into_iter().find(|look| look.rect == [222, 160, 58, 32]).unwrap();
        assert!(!old.shown.selected);
        let scene = LookScene {
            interactables: vec![scene::Interactable { id: old.id, rect: old.rect, kind: old.kind, ..Default::default() }],
            looks: vec![scene::Looks { id: old.id, select: old.select, shown: old.shown,
                views: vec![scene::ViewLooks { view: 255, sprites: (0, 1), states: old.states.iter().map(|(state, _)|
                    (*state, vec![scene::FreeSprite { set: if state.selected { 10 } else { 20 }, ..Default::default() }])).collect() }] }],
        };
        let mut feedback = Feedback::default();
        for down in [true, false] {
            feedback.sent(&Input::MouseButton { button: scene::Button::Left, down, x: 232, y: 170 }, &scene, 0.0);
        }
        feedback.replacements(&scene, 0.01);
        assert!(!feedback.drawn().iter().any(|(_, state)| state.selected), "pixel scale cannot be selected over the old derived readout");
        assert_eq!(prefs.draws(&fonts), before_draws);
        assert!(click(&mut prefs, 232, 170).is_empty());
        assert_eq!(prefs.settings().resolution.pixel_scale, 3);
        assert_eq!(prefs.settings_preview().unwrap().pixel_scale, 3);
        let complete = prefs.draws(&fonts);
        assert!(complete.iter().any(|draw| matches!(draw, Draw::Text(t) if t.text == "Rendered at 341×256")));
        assert!(prefs.looks(&fonts).iter().find(|look| look.id == old.id).unwrap().shown.selected);
    }

    #[test]
    fn dependent_checkbox_commits_selection_and_enabled_controls_together() {
        use tile_viewer::looks::{Feedback, LookScene};
        let (mut prefs, _) = open_prefs("atomic-dependent-toggle");
        assert!(click(&mut prefs, 220, 55).is_empty());
        let fonts = Fonts::default();
        let before = prefs.looks(&fonts);
        let old = before.iter().find(|look| look.rect == [90, 339, 160, 23]).unwrap();
        assert!(!old.shown.selected);
        assert!(!before.iter().any(|look| look.rect == [260, 334, 408, 23]));
        let scene = LookScene {
            interactables: vec![scene::Interactable { id: old.id, rect: old.rect, kind: old.kind, ..Default::default() }],
            looks: vec![scene::Looks { id: old.id, select: old.select, shown: old.shown,
                views: vec![scene::ViewLooks { view: 255, sprites: (0, 1), states: old.states.iter().map(|(state, _)|
                    (*state, vec![scene::FreeSprite { set: if state.selected { 10 } else { 20 }, ..Default::default() }])).collect() }] }],
        };
        let mut feedback = Feedback::default();
        for down in [true, false] { feedback.sent(&Input::MouseButton { button: scene::Button::Left, down, x: 100, y: 316 }, &scene, 0.0); }
        feedback.replacements(&scene, 0.01);
        assert!(!feedback.drawn().iter().any(|(_, state)| state.selected), "Scale menus cannot appear enabled while its committed slider is disabled");
        assert!(!prefs.editing().resolution.scale_menus);
        assert!(click(&mut prefs, 100, 349).is_empty());
        let committed = prefs.looks(&fonts);
        assert!(committed.iter().find(|look| look.id == old.id).unwrap().shown.selected);
        assert!(committed.iter().any(|look| look.rect == [260, 334, 408, 23]));
    }

    #[test]
    fn background_sampling_preview_before_accept_preserves_unrelated_draft_and_discard() {
        let (mut p, path) = open_prefs("sampling-live-preview");
        let opening = p.settings().clone();
        p.draft.as_mut().unwrap().display.max_fps = 144;
        p.draft.as_mut().unwrap().layout.split = viewer_settings::Split::Vertical2To1;
        click(&mut p, 220, 55);
        click(&mut p, 720, 495);
        assert_eq!(p.settings().resolution.background_sampling, viewer_settings::BackgroundSampling::Interpolate,
            "sampling must preview without Accept");
        assert_eq!(p.settings().display.max_fps, 144, "pacing previews too");
        assert_eq!(p.settings().layout.split, viewer_settings::Split::Vertical2To1, "so does the layout");
        assert!(!path.exists(), "preview never persists");
        p.action("cancel", &mut Vec::new());
        p.action("discard", &mut Vec::new());
        assert_eq!(p.settings(), &opening);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn cursor_size_preview_before_accept_and_accept_persists() {
        let (mut p, path) = open_prefs("cursor-live-preview");
        click(&mut p, 220, 55);
        click(&mut p, 550, 540);
        assert_eq!(p.settings().display.cursor_size, 64, "cursor size must preview without Accept");
        assert!(!path.exists());
        p.action("accept", &mut Vec::new());
        assert_eq!(Settings::load(&path).unwrap().display.cursor_size, 64);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn multiplier_confirmation_revert_keep_and_later_discard() {
        let (mut p, path) = open_prefs("multiplier-preview");
        let opening = p.settings().clone();
        p.draft.as_mut().unwrap().resolution.pixel_scale = 3;
        p.preview_multiplier(opening.resolution.pixel_scale);
        p.project_settings();
        let first = p.settings_preview().unwrap();
        p.ctrl = true;
        let release = Input::Key {vk:VK_CONTROL,down:false,text:None};
        assert_eq!(p.route(vec![release.clone()], (1024,768)), vec![release]);
        assert!(!p.ctrl, "modifiers held before the modal must retain their releases");
        assert_eq!(p.settings_preview(),Some(first));
        assert_eq!(p.settings().resolution.pixel_scale, 3);
        p.action("accept", &mut Vec::new());
        assert!(!path.exists(), "unconfirmed multiplier cannot be accepted");
        let decision = |p: scene::SettingsPreview, keep| Input::SettingsPreviewDecision {session:p.session,id:p.id,keep};
        p.route(vec![decision(first, false)], (1024,768));
        assert_eq!(p.settings(), &opening);
        assert!(p.settings_preview().is_none());
        p.draft.as_mut().unwrap().resolution.pixel_scale = 2;
        p.preview_multiplier(opening.resolution.pixel_scale);
        p.project_settings();
        let second = p.settings_preview().unwrap();
        p.route(vec![decision(first, false)], (1024,768));
        assert_eq!(p.settings_preview(), Some(second), "stale replies cannot settle a later change");
        p.route(vec![decision(second, true)], (1024,768));
        assert_eq!(p.settings().resolution.pixel_scale, 2);
        assert!(!path.exists(), "Keep does not persist Preferences");
        p.action("cancel", &mut Vec::new());
        p.action("discard", &mut Vec::new());
        assert_eq!(p.settings(), &opening);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn sensitivity_pacing_and_combat_speed_preview_and_discard_restores_opening() {
        let (mut p, path) = open_prefs("sensitivity-preview");
        let opening = p.settings().clone();
        click(&mut p,330,55);
        click(&mut p,800,355);
        assert_ne!(p.settings().input.capture_sensitivity, opening.input.capture_sensitivity);
        assert_eq!(p.settings().input.capture_sensitivity, p.editing().input.capture_sensitivity);
        p.draft.as_mut().unwrap().display.present = viewer_settings::Present::Immediate;
        p.draft.as_mut().unwrap().display.max_fps = 240;
        p.draft.as_mut().unwrap().display.fps_limit = 240;
        p.draft.as_mut().unwrap().animation.combat_speed = 3.0;
        p.project_settings();
        assert_eq!(p.settings().display, p.editing().display);
        assert_eq!(p.settings().animation.effective_combat_speed(), 3.0);
        assert!(!path.exists());
        p.action("cancel", &mut Vec::new());
        p.action("discard", &mut Vec::new());
        assert_eq!(p.settings(), &opening);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn successive_multiplier_revert_restores_previous_kept_value_and_accept_saves_it() {
        let (mut p, path) = open_prefs("successive-multiplier-preview");
        click(&mut p,220,55);
        click(&mut p,232,170);
        let first = p.settings_preview().unwrap();
        assert_eq!(first.pixel_scale,3);
        p.route(vec![Input::SettingsPreviewDecision {session:first.session,id:first.id,keep:true}], (1024,768));
        click(&mut p,298,170);
        let second = p.settings_preview().unwrap();
        assert_eq!((second.previous_pixel_scale,second.pixel_scale),(3,4));
        p.route(vec![Input::SettingsPreviewDecision {session:second.session,id:second.id,keep:false}], (1024,768));
        assert_eq!(p.settings().resolution.pixel_scale,3);
        p.action("accept", &mut Vec::new());
        assert_eq!(Settings::load(&path).unwrap().resolution.pixel_scale,3);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

}
