//! The renderer's own preferences, as tabs of the game's Preferences screen:
//! "CivIII" (the game's own preferences, mirrored as they are), "Resolution"
//! and "Layout" (the viewer's settings, `viewer_settings::Settings`). The
//! tabs and their content are a panel of the menu crate
//! (`assets/civ3/preferences.toml`) laid over the mirrored screen; what the
//! panel takes of the pointer input never reaches the game.
//!
//! The bridge keeps the settings: loaded at start from
//! `Settings::default_path()`, saved there whenever the user changes one,
//! and sent to the viewer with every scene (`scene::Scene::settings`; only
//! changes go over the wire).

use std::collections::BTreeMap;
use std::path::PathBuf;

use civ3_sys::ui_hooks::PreferencesForm;
use menu::panel::{Draw, PanelEvent, PanelState, Panels, TextMetrics, TextStyle, Value};
use scene::Input;
use viewer_settings::Settings;
use viewer_settings::controls::ControlValue;

use crate::text::{Fonts, TextRun};
use crate::ui::{Frame, ScreenDraw};
use crate::ui_screen::Item;

/// The panel (`menu::panel` format).
pub const PANELS: &str = include_str!("../../../assets/civ3/preferences.toml");
/// Its panel, laid over the screen's background.
const PANEL: &str = "preferences";

/// The value naming the tab shown, and its options.
pub const TAB: &str = "tab";
pub const TAB_GAME: &str = "civ3";
pub const TAB_RESOLUTION: &str = "resolution";
pub const TAB_LAYOUT: &str = "layout";

/// The values the panel shows: the tab, every control of the settings
/// (`viewer_settings::controls`) by its key, the rendered and the window
/// size (`resolution.realised`, `resolution.window`, as "W×H") for a window
/// of `window` physical pixels on a desktop scaled `desktop` times, and the
/// pixel scale in effect (`resolution.effective`, e.g. "Auto (×2)").
pub fn values(settings: &Settings, tab: &str, window: (u32, u32), desktop: f64) -> BTreeMap<String, Value> {
    let mut out: BTreeMap<String, Value> = settings.controls().into_iter().map(|(k, v)| (k, to_value(v))).collect();
    out.insert(TAB.into(), Value::Text(tab.into()));
    let (w, h) = settings.resolution.realised(window, desktop);
    out.insert("resolution.realised".into(), Value::Text(format!("{w}×{h}")));
    out.insert("resolution.window".into(), Value::Text(format!("{}×{}", window.0, window.1)));
    let r = &settings.resolution;
    let mut effective = r.describe_pixel_scale(desktop);
    if r.pixel_scale_in(window, desktop) < r.effective_pixel_scale(desktop) {
        effective = format!("{effective}, ×{} in this small window", r.pixel_scale_in(window, desktop));
    }
    out.insert("resolution.effective".into(), Value::Text(effective));
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

/// The renderer's preferences: the settings, and the tabs over the game's
/// Preferences screen while it is open.
pub struct Prefs {
    panels: Option<Panels>,
    state: PanelState,
    settings: Settings,
    path: PathBuf,
    /// The tab shown.
    tab: String,
    /// Where the game's Preferences screen is while it is open (see
    /// [`screen_area`]).
    open: Option<[i32; 4]>,
    /// The viewer's window in physical pixels and the desktop's scale
    /// (`Input::Window`; the rendered size depends on them), or before it
    /// told them, the size it arranges the game's screen in.
    window: (u32, u32),
    /// A popup of the game is open over the screen (e.g. C3X's info box):
    /// the input is all the game's, the popup's buttons may lie over our
    /// tabs and panels.
    covered: bool,
    /// Whether Ctrl and Shift are held (for the reset, [`RESET_KEY`]).
    ctrl: bool,
    shift: bool,
    /// The desktop's scale at the renderer's window, and whether the
    /// renderer told its window (`Input::Window`) rather than the driver's
    /// screen only.
    desktop: f64,
    native: bool,
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

const VK_BACK: u32 = 0x08;
const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;
/// The key that, with Ctrl and Shift, resets the layout and resolution
/// settings (Backspace).
pub const RESET_KEY: u32 = VK_BACK;

impl Prefs {
    /// The settings saved at `path` (the defaults if there are none), and
    /// the panel.
    pub fn load(path: PathBuf) -> Self {
        let settings = Settings::load(&path).unwrap_or_else(|e| {
            log::warn!("settings: {e}; using the defaults (saved over it once one is changed)");
            Settings::default()
        });
        log::info!("settings: {}", path.display());
        let panels = Panels::parse(PANELS).inspect_err(|e| log::error!("preferences panel: {e}")).ok();
        Prefs { panels, state: PanelState::default(), settings, path, tab: TAB_GAME.into(), open: None, window: (0, 0), covered: false, ctrl: false, shift: false, desktop: 1.0, native: false }
    }

    /// The settings (sent to the viewer with every scene).
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Render description entries for the panel's generated art.
    pub fn description() -> String {
        Panels::parse(PANELS).map(|p| p.description()).unwrap_or_default()
    }

    /// Whether the game's Preferences screen is open (as last seen).
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Takes what is the panel's of `inputs` (while the screen is open) and
    /// acts on it — a changed setting is saved at once — and gives back the
    /// rest, for the game. `window`: the window's size as last reported.
    pub fn route(&mut self, inputs: Vec<Input>, window: (u32, u32)) -> Vec<Input> {
        self.follow_window(window);
        for input in &inputs {
            match *input {
                Input::Window { width, height, scale_milli } => {
                    (self.window, self.desktop, self.native) = ((width, height), scale_milli.max(1) as f64 / 1000.0, true);
                }
                Input::Resized { width, height } => self.follow_window((width, height)),
                _ => {}
            }
        }
        let mut changed = false;
        let mut out = Vec::new();
        for input in inputs {
            // The way out of settings that make the game unusable, wherever
            // the user is: Ctrl+Shift+Backspace (neither press nor release
            // reaches the game).
            if let Input::Key { vk, down, .. } | Input::WorldKey { vk, down, .. } = input {
                match vk {
                    VK_CONTROL => self.ctrl = down,
                    VK_SHIFT => self.shift = down,
                    VK_BACK if self.ctrl && self.shift => {
                        if down {
                            changed |= self.reset();
                        }
                        continue;
                    }
                    _ => {}
                }
            }
            // (While a popup of the game is over the screen, the input is
            // all the game's.)
            let (Some(area), Some(panels), false) = (self.open, self.panels.as_ref(), self.covered) else {
                out.push(input);
                continue;
            };
            let Some(panel) = panels.panel(PANEL) else {
                out.push(input);
                continue;
            };
            self.state.values = self.values();
            let routed = panels.input(panel, &mut self.state, area, &input);
            for event in &routed.events {
                let (settings, tab) = apply(&mut self.settings, event);
                changed |= settings;
                if let Some(tab) = tab {
                    log::debug!("preferences: tab {tab}");
                    self.tab = tab;
                }
            }
            if !routed.consumed {
                out.push(input);
            }
        }
        self.state.values = self.values();
        if changed {
            match self.settings.save(&self.path) {
                Ok(()) => log::info!("settings saved: {:?}", self.settings),
                Err(e) => log::error!("saving the settings: {e}"),
            }
        }
        out
    }

    /// Puts the layout and the resolution back to their defaults (the
    /// pointer settings stay); whether that changed them.
    fn reset(&mut self) -> bool {
        let before = self.settings.clone();
        self.settings.layout = viewer_settings::Layout::default();
        self.settings.resolution = viewer_settings::Resolution::default();
        log::warn!("settings: the layout and the resolution reset to their defaults (Ctrl+Shift+Backspace)");
        self.settings != before
    }

    /// Follows the game's screens: whether its Preferences screen is shown
    /// in `frame`, and where, and whether a popup is over it (`modal`: the
    /// game's modal form). It opens on the game's own tab, with nothing
    /// pressed.
    pub fn update(&mut self, frame: Option<&Frame>, form: Option<PreferencesForm>, window: (u32, u32), modal: Option<u32>) {
        self.follow_window(window);
        let size = self.panels.as_ref().and_then(|p| p.panel(PANEL)).map(|p| p.size()).unwrap_or((1024, 768));
        let open = frame.zip(form).and_then(|(f, form)| screen_area(f, &form, size));
        if open.is_some() != self.open.is_some() {
            log::info!("preferences screen {}", if open.is_some() { "open: our tabs over it" } else { "closed" });
            self.state = PanelState::default();
            self.tab = TAB_GAME.into();
        }
        self.open = open;
        let covered = open.is_some() && form.is_some_and(|f| popup_over(modal, &f));
        if covered != self.covered {
            log::info!("preferences screen: {}", if covered { "a popup over it, the input is the game's" } else { "no popup over it" });
            self.state = PanelState { values: std::mem::take(&mut self.state.values), ..PanelState::default() };
        }
        self.covered = covered;

        self.state.values = self.values();
    }

    /// The size the viewer arranges the game's screen in (`Input::Resized`),
    /// for the readout until it tells its window's (`Input::Window`).
    fn follow_window(&mut self, window: (u32, u32)) {
        if window != (0, 0) && !self.native {
            self.window = window;
        }
    }

    fn values(&self) -> BTreeMap<String, Value> {
        values(&self.settings, &self.tab, self.window, self.desktop)
    }

    /// What the panel draws now (nothing while the screen is closed), text
    /// measured with `fonts` where they are loaded.
    pub fn draws(&self, fonts: &Fonts) -> Vec<Draw> {
        let (Some(area), Some(panels)) = (self.open, self.panels.as_ref()) else { return Vec::new() };
        let Some(panel) = panels.panel(PANEL) else { return Vec::new() };
        let measure = |style: &TextStyle, text: &str| {
            fonts
                .metrics(&text_run(style, text, (0, 0)))
                .map_or_else(|| menu::panel::estimate(style, text), |(width, ascent, descent)| TextMetrics { width, ascent, descent })
        };
        panels.draw(panel, &self.state, area, &measure)
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
    use viewer_settings::{Place, Roles, Split, Zoom};

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
        let v = values(&s, TAB_LAYOUT, (3840, 2160), 2.0);
        assert_eq!(v[TAB], Value::Text("layout".into()));
        assert_eq!(v["resolution.pixel_scale"], Value::Text("3".into()));
        assert_eq!(v["resolution.realised"], Value::Text("1280×720".into()));
        assert_eq!(v["resolution.window"], Value::Text("3840×2160".into()));
        assert_eq!(v["resolution.effective"], Value::Text("×3".into()));
        // Auto on a desktop at 200 %: rendered at half the window.
        s.resolution.pixel_scale = viewer_settings::Resolution::AUTO;
        let v = values(&s, TAB_RESOLUTION, (3840, 2160), 2.0);
        assert_eq!(v["resolution.pixel_scale"], Value::Text("auto".into()));
        assert_eq!(v["resolution.realised"], Value::Text("1920×1080".into()));
        assert_eq!(v["resolution.effective"], Value::Text("Auto (×2)".into()));
        // ×1 at 200 %: rendered at the window's size, at once.
        s.resolution.pixel_scale = 1;
        let v = values(&s, TAB_RESOLUTION, (3840, 2160), 2.0);
        assert_eq!(v["resolution.realised"], Value::Text("3840×2160".into()));
        assert_eq!(v["resolution.effective"], Value::Text("×1".into()));
        assert_eq!(v["layout.split"], Value::Text("Vertical1To1".into()));
        assert_eq!(v["layout.smooth_world_movement"], Value::On(false));
        assert_eq!(v["layout.screens.Palace"], Value::Text("Floating".into()));
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
            assert!(control == TAB || s.control(control).is_some(), "{control}");
            if let Some((min, max, step)) = panel.slider_range(control) {
                let r = viewer_settings::controls::range(control).unwrap();
                assert_eq!((min, max, step), (r.min, r.max, r.step), "{control}");
            }
        }
        for key in viewer_settings::controls::KEYS.iter().filter(|k| **k != "input.snap_ghost_fps") {
            assert!(panel.controls().contains(key), "no control for {key}");
        }
        for kind in viewer_settings::Layout::SCREENS {
            assert!(panel.controls().contains(&format!("layout.screens.{kind}").as_str()), "{kind}");
        }
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

    fn open_prefs(name: &str) -> (Prefs, PathBuf) {
        let path = std::env::temp_dir().join(format!("civ3r-prefs-test-{}-{name}", std::process::id())).join("settings.toml");
        let _ = std::fs::remove_file(&path);
        let mut p = Prefs::load(path.clone());
        let f = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
        p.update(Some(&f), Some(FORM), (1024, 768), None);
        (p, path)
    }

    fn click(p: &mut Prefs, x: i32, y: i32) -> Vec<Input> {
        let b = |down| Input::MouseButton { button: scene::Button::Left, down, x, y };
        p.route(vec![Input::MouseMove { x, y }, b(true), b(false)], (1024, 768))
    }

    /// Clicks through the tabs: the game gets what lies outside the panel,
    /// a setting changed is saved at once, and the screen opens on the
    /// game's tab again.
    #[test]
    fn clicks_on_the_tabs_change_and_save_settings() {
        let (mut p, path) = open_prefs("clicks");
        assert!(p.is_open());
        // On the game's tab, its own controls get the clicks.
        assert_eq!(click(&mut p, 110, 150).len(), 3);
        // The Layout tab, then the vertical 2:1 split.
        assert!(click(&mut p, 330, 55).is_empty());
        assert_eq!(p.tab, TAB_LAYOUT);
        assert!(!path.exists());
        assert!(click(&mut p, 340, 140).is_empty());
        assert_eq!(p.settings().layout.split, Split::Vertical2To1);
        assert_eq!(Settings::load(&path).unwrap().layout.split, Split::Vertical2To1);
        // Roles: unit and mouse; F2 on the right; the zoom right of the split.
        assert!(click(&mut p, 230, 440).is_empty());
        assert!(click(&mut p, 900, 120 + 31 + 10).is_empty());
        assert!(click(&mut p, 340, 240).is_empty());
        let saved = Settings::load(&path).unwrap();
        assert_eq!(saved.layout.roles, Roles::UnitMouse);
        assert_eq!(saved.layout.screens["F2"].vertical, Place::RightOrTop);
        assert_eq!(saved.layout.zoom_place, Place::RightOrTop);
        // Map movement: dragging, then edge scrolling again.
        assert!(click(&mut p, 400, 635).is_empty());
        assert_eq!(Settings::load(&path).unwrap().input.map_movement, viewer_settings::MapMovement::Drag);
        assert!(click(&mut p, 250, 635).is_empty());
        assert_eq!(Settings::load(&path).unwrap().input.map_movement, viewer_settings::MapMovement::EdgeScroll);
        // The game's OK button (bottom right) is not covered.
        assert_eq!(click(&mut p, 985, 735).len(), 3);
        // The pixel scale on the Resolution tab.
        assert!(click(&mut p, 220, 55).is_empty());
        assert!(click(&mut p, 156 + 2 * 66 + 10, 170).is_empty());
        assert_eq!(Settings::load(&path).unwrap().resolution.pixel_scale, 3);
        let texts: Vec<String> = p.draws(&Fonts::default()).iter().filter_map(|d| if let Draw::Text(t) = d { Some(t.text.clone()) } else { None }).collect();
        assert!(texts.contains(&"Rendered at 341×256".to_string()), "{texts:?}");
        // The frame counter, on and off again by its checkbox (or its label).
        assert!(click(&mut p, 97, 495).is_empty());
        assert!(Settings::load(&path).unwrap().display.show_fps);
        assert!(click(&mut p, 160, 495).is_empty());
        assert!(!Settings::load(&path).unwrap().display.show_fps);
        // Auto, the first.
        assert!(click(&mut p, 90 + 10, 170).is_empty());
        assert_eq!(Settings::load(&path).unwrap().resolution.pixel_scale, viewer_settings::Resolution::AUTO);
        // Closed and opened again: the game's tab.
        p.update(Some(&Frame::default()), Some(FORM), (1024, 768), None);
        assert!(!p.is_open());
        assert_eq!(click(&mut p, 330, 55).len(), 3);
        let f = frame_with_screen((0, 0), (1024, 768), Some((0, 0)));
        p.update(Some(&f), Some(FORM), (1024, 768), None);
        assert_eq!(p.tab, TAB_GAME);
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
        assert_eq!(click(&mut p, 220, 55).len(), 3);
        assert_eq!(p.tab, TAB_LAYOUT);
        // Gone: ours again.
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

    /// The zoom placement is greyed out (and takes no clicks) unless the
    /// split is vertical; screen places show and set the orientation's own.
    #[test]
    fn controls_follow_the_split() {
        let (mut p, path) = open_prefs("split");
        assert!(click(&mut p, 330, 55).is_empty());
        // Standard split: the zoom placement does nothing.
        assert!(click(&mut p, 340, 240).is_empty());
        assert_eq!(p.settings().layout.zoom_place, Place::Floating);
        // Horizontal: F1 at the top ("Top" where "Right" was).
        assert!(click(&mut p, 460, 140).is_empty());
        assert_eq!(p.settings().layout.split, Split::Horizontal1To1);
        assert!(click(&mut p, 900, 130).is_empty());
        let texts: Vec<String> = p.draws(&Fonts::default()).iter().filter_map(|d| if let Draw::Text(t) = d { Some(t.text.clone()) } else { None }).collect();
        let count = |s: &str| texts.iter().filter(|t| *t == s).count();
        // (One "Right": the zoom placement's, greyed out.)
        assert_eq!((count("Top"), count("Bottom"), count("Right"), count("Left")), (13, 13, 1, 1));
        // "B" in three split pictures and the free roles one; no words in the
        // unit and mouse one: the game's unit icon and pointer.
        assert_eq!((count("B"), count("Mouse"), count("Unit")), (4, 0, 0));
        let sprites: Vec<String> = p.draws(&Fonts::default()).iter().filter_map(|d| if let Draw::Element(menu::screen::Element::Sprite { name, .. }) = d { Some(name.clone()) } else { None }).collect();
        // (The split horizontal: their smaller ones.)
        assert!(["Prefs@unit_small/0", "Prefs@mouse_small/0"].iter().all(|s| sprites.iter().any(|n| n == s)), "{sprites:?}");
        let f1 = p.settings().layout.screens["F1"];
        assert_eq!((f1.horizontal, f1.vertical), (Place::RightOrTop, Place::Floating));
        // The height slider shows only with the strip or a box at the mouse.
        assert!(!texts.iter().any(|t| t.ends_with('%')));
        assert!(click(&mut p, 430, 330).is_empty());
        assert_eq!(p.settings().layout.zoom, Zoom::TopStrip);
        let texts: Vec<String> = p.draws(&Fonts::default()).iter().filter_map(|d| if let Draw::Text(t) = d { Some(t.text.clone()) } else { None }).collect();
        assert!(texts.contains(&"25%".to_string()), "{texts:?}");
        // Dragging the strip's slider to its right end: 50%.
        let drag = |x| Input::MouseMove { x, y: 360 };
        let b = |down, x| Input::MouseButton { button: scene::Button::Left, down, x, y: 360 };
        assert!(p.route(vec![b(true, 300), drag(800), b(false, 800)], (1024, 768)).is_empty());
        assert_eq!(p.settings().layout.zoom_strip, 0.5);
        assert_eq!(Settings::load(&path).unwrap().layout.zoom_strip, 0.5);
        let texts = |p: &Prefs| -> Vec<String> { p.draws(&Fonts::default()).iter().filter_map(|d| if let Draw::Text(t) = d { Some(t.text.clone()) } else { None }).collect() };
        assert!(texts(&p).contains(&"Strip height".to_string()));
        // A box at the mouse is sized by it too: the slider shows, as the
        // box's height; to its left end: 5%.
        assert!(click(&mut p, 140, 330).is_empty());
        assert_eq!(p.settings().layout.zoom, Zoom::Mouse111);
        let shown = texts(&p);
        assert!(shown.contains(&"Box height".to_string()) && !shown.contains(&"Strip height".to_string()) && shown.contains(&"50%".to_string()), "{shown:?}");
        assert!(p.route(vec![b(true, 300), drag(0), b(false, 0)], (1024, 768)).is_empty());
        assert_eq!(p.settings().layout.zoom_strip, 0.05);
        assert!(texts(&p).contains(&"5%".to_string()));
        // Not for a floating box.
        assert!(click(&mut p, 290, 300).is_empty());
        assert_eq!(p.settings().layout.zoom, Zoom::Floating131);
        assert!(!texts(&p).iter().any(|t| t.ends_with('%') || t.ends_with("height")));
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

    /// Ctrl+Shift+Backspace puts the layout and resolution back, saved, with
    /// the screen open or not; the keys do not reach the game.
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
        assert_eq!(p.settings(), &saved);
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
}
