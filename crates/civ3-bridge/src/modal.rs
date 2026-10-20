//! Playing on the map from a view of the viewer's own while one of the
//! game's full screens is open (an advisor, the Civilopedia, the city
//! screen; docs/modal-input.md, approach (a)).
//!
//! The viewer sends presses and keys on world pixels it shows but the
//! game's window takes no input for (`Input::WorldButton`,
//! `Input::WorldKey`). The publisher turns them into [`Action`]s and
//! queues them; the game's own thread runs them from the modal loop's
//! message pump, which is patched to call [`run_pending`] first. Actions
//! are the game's tile-level functions (select, go to, the unit commands),
//! never faked clicks, and only run when [`gate`] says the game is in a
//! state where its own map handler could have run them.

use std::time::{Duration, Instant};

/// How long a queued action may wait for the game to get to it.
pub const EXPIRES: Duration = Duration::from_millis(1500);

/// A direction on the map, as on the screen (the keypad's layout).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Dir {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl Dir {
    /// The step in tile coordinates (tiles have `x + y` even; `x` counts
    /// half tile widths, `y` half tile heights).
    pub fn delta(self) -> (i32, i32) {
        match self {
            Dir::N => (0, -2),
            Dir::NE => (1, -1),
            Dir::E => (2, 0),
            Dir::SE => (1, 1),
            Dir::S => (0, 2),
            Dir::SW => (-1, 1),
            Dir::W => (-2, 0),
            Dir::NW => (-1, -1),
        }
    }
}

/// A unit command the map's keys give.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Fortify,
    Sentry,
    Skip,
    Wait,
}

impl Command {
    /// Its `Unit_Command_Values`.
    pub fn value(self) -> u32 {
        use civ3_sys::modal::command;
        match self {
            Command::Fortify => command::FORTIFY,
            Command::Sentry => command::SENTRY,
            Command::Skip => command::SKIP_TURN,
            Command::Wait => command::WAIT,
        }
    }
}

/// What the player asked for on the map.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Select the player's own unit on this tile.
    Select((i32, i32)),
    /// Send the selected unit to this tile (moves now if it is next to it).
    GoTo((i32, i32)),
    /// Move the selected unit one tile.
    Step(Dir),
    Command(Command),
    /// Arm go-to for the selected unit (the map's G key): the pointer then
    /// previews the path, a left click picks the destination.
    ArmGoTo,
}

/// The keys acted on (Windows virtual-key codes); nothing else reaches the
/// game while a menu is open: the keypad (with Num Lock on or off) and the
/// arrows move the selected unit, F fortifies, S sentries, Space skips its
/// turn, W makes it wait, G arms go-to.
pub fn key_action(vk: u32) -> Option<Action> {
    Some(match vk {
        // Numpad 8, Up
        0x68 | 0x26 => Action::Step(Dir::N),
        // Numpad 9, Page Up
        0x69 | 0x21 => Action::Step(Dir::NE),
        // Numpad 6, Right
        0x66 | 0x27 => Action::Step(Dir::E),
        // Numpad 3, Page Down
        0x63 | 0x22 => Action::Step(Dir::SE),
        // Numpad 2, Down
        0x62 | 0x28 => Action::Step(Dir::S),
        // Numpad 1, End
        0x61 | 0x23 => Action::Step(Dir::SW),
        // Numpad 4, Left
        0x64 | 0x25 => Action::Step(Dir::W),
        // Numpad 7, Home
        0x67 | 0x24 => Action::Step(Dir::NW),
        0x46 => Action::Command(Command::Fortify),
        0x53 => Action::Command(Command::Sentry),
        0x20 => Action::Command(Command::Skip),
        0x57 => Action::Command(Command::Wait),
        0x47 => Action::ArmGoTo,
        _ => return None,
    })
}

/// A press on `tile`: the left button selects, the right one sends the
/// selected unit there (the game's own right button opens a menu, so there
/// is no convention to follow).
pub fn button_action(button: scene::Button, tile: (i32, i32)) -> Option<Action> {
    match button {
        scene::Button::Left => Some(Action::Select(tile)),
        scene::Button::Right => Some(Action::GoTo(tile)),
        scene::Button::Middle => None,
    }
}

/// Why an action was not run.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("no menu is open")]
    NoMenu,
    #[error("the open form {0:#x} is not an advisor, the Civilopedia or the city screen")]
    OtherForm(u32),
    #[error("{0} dialogs are open")]
    Nested(i32),
    #[error("not in the human player's turn loop")]
    NotInTurn,
    #[error("an animation is running")]
    Animating,
    #[error("a network game")]
    Network,
    #[error("an action is already running")]
    Busy,
    #[error("this build is not decoded")]
    Unsupported,
    #[error("no unit of the player's on {0:?}")]
    NoOwnUnit((i32, i32)),
    #[error("no unit selected")]
    NoSelection,
    #[error("{0:?} holds a foreign unit")]
    ForeignUnit((i32, i32)),
    #[error("{0:?} holds a foreign city")]
    ForeignCity((i32, i32)),
    #[error("off the map")]
    OffMap,
    #[error("the unit cannot {0:?} now")]
    CannotDo(Command),
    /// A targeting command other than go-to waits for its tile (bombard,
    /// airlift, ...): not taken while a screen is open.
    #[error("a command other than go-to waits for its target")]
    NotGoTo,
    #[error("the unit cannot go to now")]
    CannotGoTo,
    /// The game was asked to select a unit and did not (e.g. one without
    /// moves left); `selected` is the unit still selected.
    #[error("the game refused to select unit {unit} at {tile:?} (no moves left?); selected: {selected:?}")]
    NotSelected { unit: i32, tile: (i32, i32), selected: Option<i32> },
    #[error("expired")]
    Expired,
    #[error("reading the game: {0}")]
    Read(String),
}

impl From<civ3_sys::Civ3Error> for Refused {
    fn from(e: civ3_sys::Civ3Error) -> Self {
        Refused::Read(e.to_string())
    }
}

/// What the player is told on the map where a press of theirs was not acted
/// on (`None`: nothing, e.g. an empty tile, or a moment's wait for an
/// animation). `diplomacy`: the open form is the diplomacy screen.
pub fn refusal_notice(refused: &Refused, diplomacy: bool) -> Option<&'static str> {
    Some(match refused {
        Refused::OtherForm(_) if diplomacy => "Not during diplomacy",
        Refused::OtherForm(_) => "Not while this window is open",
        Refused::Nested(_) => "Close the dialog first",
        Refused::NotInTurn => "Only during your turn",
        Refused::Network => "Not in a network game",
        Refused::ForeignUnit(_) | Refused::ForeignCity(_) => "Close the screen to attack",
        Refused::NoSelection => "Select a unit first",
        Refused::NotSelected { .. } => "This unit cannot be selected now",
        Refused::CannotDo(_) | Refused::CannotGoTo => "The unit cannot do that now",
        Refused::NotGoTo => "Close the screen for this order",
        Refused::NoMenu | Refused::Animating | Refused::Busy | Refused::Unsupported | Refused::NoOwnUnit(_) | Refused::OffMap
        | Refused::Expired | Refused::Read(_) => return None,
    })
}

/// How long a refusal notice shows.
pub const NOTICE_FOR: Duration = Duration::from_millis(2500);

/// What [`gate`] looks at.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct GateState {
    /// The modal form, and the forms acting is allowed under (an advisor,
    /// the Civilopedia, the city screen).
    pub modal_form: u32,
    pub allowed_forms: [u32; 3],
    pub depth: i32,
    /// The human-turn loop's flag, and how many of its pumps are running
    /// (the menu was opened from one of them).
    pub turn_flag: bool,
    pub human_pumps: i32,
    /// How many mid-animation pumps are running.
    pub animation_pumps: i32,
    pub online: bool,
    pub pbem: bool,
    pub running: bool,
}

/// Whether an action may run now: an advisor, the Civilopedia or the city
/// screen is the only dialog, opened from the human-turn loop (so no end of turn or AI
/// turn can run under it) outside any animation, in a game that is not
/// played over the network, and no action is running already.
pub fn gate(s: &GateState) -> Result<(), Refused> {
    if s.modal_form == 0 {
        return Err(Refused::NoMenu);
    }
    if !s.allowed_forms.contains(&s.modal_form) {
        return Err(Refused::OtherForm(s.modal_form));
    }
    if s.depth != 1 {
        return Err(Refused::Nested(s.depth));
    }
    if s.online || s.pbem {
        return Err(Refused::Network);
    }
    if !s.turn_flag || s.human_pumps <= 0 {
        return Err(Refused::NotInTurn);
    }
    if s.animation_pumps > 0 {
        return Err(Refused::Animating);
    }
    if s.running {
        return Err(Refused::Busy);
    }
    Ok(())
}

/// The tile one step from `tile`, on a map `size` tiles big (in the game's
/// coordinates) that wraps as `wrap` says.
pub fn step((x, y): (i32, i32), dir: Dir, size: (i32, i32), wrap: (bool, bool)) -> Option<(i32, i32)> {
    let (dx, dy) = dir.delta();
    let axis = |v: i32, n: i32, wraps: bool| {
        if wraps {
            Some(v.rem_euclid(n.max(1)))
        } else {
            (0..n).contains(&v).then_some(v)
        }
    };
    Some((axis(x + dx, size.0, wrap.0)?, axis(y + dy, size.1, wrap.1)?))
}

/// Whether the player's unit may be sent onto `tile`, with units of
/// `unit_civs` and a city of `city_civ` there: never onto a foreign unit
/// or city (that would be an attack).
pub fn may_enter(player: i32, tile: (i32, i32), mut unit_civs: impl Iterator<Item = i32>, city_civ: Option<i32>) -> Result<(), Refused> {
    if unit_civs.any(|c| c != player) {
        return Err(Refused::ForeignUnit(tile));
    }
    if city_civ.is_some_and(|c| c != player) {
        return Err(Refused::ForeignCity(tile));
    }
    Ok(())
}

/// A go-to being walked under a screen, one tile a pass of the screen's
/// loop, along the path the game's path finder showed for it, each step the
/// game's own move to a neighbouring tile (`perform_action_on_tile`, as a
/// click next to the unit). The game's own order goes on from where the
/// unit stops (out of moves this turn, blocked).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Walk {
    pub unit: u32,
    pub id: i32,
    /// From the destination back to the start (`targeting::GotoPath`).
    pub path: Vec<(i32, i32)>,
    pub dest: (i32, i32),
    /// The unit's move points a turn.
    pub max_moves: i32,
    pub steps: u32,
}

/// Most steps one go-to takes under a screen.
pub const MAX_WALK_STEPS: u32 = 64;

/// The next tile from `at` on a path running from the destination back to
/// the start; `None` at the destination or off the path.
pub fn next_step(path: &[(i32, i32)], at: (i32, i32)) -> Option<(i32, i32)> {
    let i = path.iter().position(|&t| t == at)?;
    i.checked_sub(1).map(|j| path[j])
}

/// Whether a walk takes another step: its unit still selected, with move
/// points left this turn, not walked too far.
pub fn walk_goes_on(walk: &Walk, selected: u32, id: i32, moves_used: i32) -> bool {
    selected == walk.unit && id == walk.id && moves_used < walk.max_moves && walk.steps < MAX_WALK_STEPS
}

/// An action waiting for the game's thread.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub at: Instant,
    pub action: Action,
}

impl Pending {
    pub fn fresh(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.at) <= EXPIRES
    }
}

#[cfg(windows)]
pub use live::{after_world_order, install, may_enter_now, modal_open, native_input_allowed, notices, queue, refuse, run_now, screen_gate, world_input};

#[cfg(windows)]
mod live {
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering::SeqCst};
    use std::cell::Cell;
    use std::sync::{LazyLock, OnceLock};
    use std::time::Instant;

    use civ3_sys::Civ3;
    use civ3_sys::modal::{self as m, animation_pump_a, animation_pump_b, human_pump_goto, human_pump_idle, modal_pump};

    use super::{Action, GateState, Pending, Refused, Walk, gate, may_enter, refusal_notice, selection_outcome, step, walk_goes_on};

    /// Actions for the game's thread, taken one at a time by the modal loop.
    static QUEUE: LazyLock<civ3r_sync::Mailbox<Pending>> = LazyLock::new(civ3r_sync::Mailbox::new);
    static RUNNING: AtomicBool = AtomicBool::new(false);
    static HUMAN_PUMPS: AtomicI32 = AtomicI32::new(0);
    static ANIMATION_PUMPS: AtomicI32 = AtomicI32::new(0);
    static LIVE: OnceLock<Live> = OnceLock::new();
    thread_local! {
        /// The go-to being walked under a screen (game thread only).
        static WALK: Cell<Option<Walk>> = const { Cell::new(None) };
    }
    /// Refusal notices: since when, the tile, the text. Written by the game's
    /// thread (the only writer), read by the publisher.
    static NOTICES: civ3r_sync::Latest<Vec<(Instant, (i32, i32), &'static str)>> = civ3r_sync::Latest::new();

    struct Live {
        game: Civ3,
        globals: m::Globals,
        find_visible_unit: m::find_visible_unit::Function,
        unit_select: m::unit_select::Function,
        perform_action_on_tile: m::perform_action_on_tile::Function,
        issue_command: m::issue_command::Function,
        unit_can_perform_command: m::unit_can_perform_command::Function,
        city_form_draw: m::city_form_draw::Function,
    }

    /// The patched pump calls; they stay patched while this lives.
    pub struct Installed {
        _modal: modal_pump::Installed,
        _human: (human_pump_goto::Installed, human_pump_idle::Installed),
        _animation: (animation_pump_a::Installed, animation_pump_b::Installed),
    }

    /// Counts the runs of a pump call site while they last (also when the
    /// game unwinds through it, which it does not).
    struct Count(&'static AtomicI32);

    impl Count {
        fn new(c: &'static AtomicI32) -> Count {
            c.fetch_add(1, SeqCst);
            Count(c)
        }
    }

    impl Drop for Count {
        fn drop(&mut self) {
            self.0.fetch_sub(1, SeqCst);
        }
    }

    fn on_human_goto(call: &human_pump_goto::Call) -> u32 {
        let _c = Count::new(&HUMAN_PUMPS);
        call.original()
    }

    fn on_human_idle(call: &human_pump_idle::Call) -> u32 {
        let _c = Count::new(&HUMAN_PUMPS);
        call.original()
    }

    fn on_animation_a(call: &animation_pump_a::Call) -> u32 {
        let _c = Count::new(&ANIMATION_PUMPS);
        call.original()
    }

    fn on_animation_b(call: &animation_pump_b::Call) -> u32 {
        let _c = Count::new(&ANIMATION_PUMPS);
        call.original()
    }

    fn on_modal(call: &modal_pump::Call) -> u32 {
        run_pending();
        call.original()
    }

    /// Checks the game's functions and patches the pump calls (the turn's
    /// and the animations' first, so the counts are right when the modal
    /// loop's is reached).
    pub fn install(game: Civ3) -> Result<Installed, String> {
        let (Some(sites), Some(fns), Some(globals)) = (m::pump_sites(game.build), m::function_sites(game.build), m::globals(game.build)) else {
            return Err(format!("no modal sites for {:?}", game.build));
        };
        let e = |what: &'static str| move |err: inproc::hook::HookError| format!("{what}: {err}");
        let live = Live {
            game,
            globals,
            find_visible_unit: m::find_visible_unit::Function::at(fns.find_visible_unit.0, fns.find_visible_unit.1).map_err(e("find_visible_unit"))?,
            unit_select: m::unit_select::Function::at(fns.unit_select.0, fns.unit_select.1).map_err(e("Unit::select"))?,
            perform_action_on_tile: m::perform_action_on_tile::Function::at(fns.perform_action_on_tile.0, fns.perform_action_on_tile.1)
                .map_err(e("perform_action_on_tile"))?,
            issue_command: m::issue_command::Function::at(fns.issue_command.0, fns.issue_command.1).map_err(e("issue_command"))?,
            unit_can_perform_command: m::unit_can_perform_command::Function::at(fns.unit_can_perform_command.0, fns.unit_can_perform_command.1)
                .map_err(e("can_perform_command"))?,
            city_form_draw: m::city_form_draw::Function::at(fns.city_form_draw.0, fns.city_form_draw.1).map_err(e("City_Form::draw"))?,
        };
        LIVE.set(live).map_err(|_| "installed twice".to_owned())?;
        let human = (
            human_pump_goto::install(sites.human_goto.0, &sites.human_goto.1, on_human_goto).map_err(e("human-turn pump"))?,
            human_pump_idle::install(sites.human_idle.0, &sites.human_idle.1, on_human_idle).map_err(e("human-turn pump"))?,
        );
        let animation = (
            animation_pump_a::install(sites.animation[0].0, &sites.animation[0].1, on_animation_a).map_err(e("animation pump"))?,
            animation_pump_b::install(sites.animation[1].0, &sites.animation[1].1, on_animation_b).map_err(e("animation pump"))?,
        );
        let modal = modal_pump::install(sites.modal.0, &sites.modal.1, on_modal).map_err(e("modal pump"))?;
        log::info!("modal input: pump calls patched");
        Ok(Installed { _modal: modal, _human: human, _animation: animation })
    }

    /// Whether direct map handlers can run from the native input queue.
    /// An input message can also arrive during an animation's nested pump;
    /// let the game finish that animation before accepting a virtual action.
    pub fn native_input_allowed() -> bool {
        LIVE.get().is_some() && HUMAN_PUMPS.load(SeqCst) > 0
            && ANIMATION_PUMPS.load(SeqCst) == 0 && !RUNNING.load(SeqCst)
    }

    /// Whether a modal form is open (the map takes no input then).
    pub fn modal_open(game: &Civ3) -> bool {
        game.modal_form().ok().flatten().is_some_and(|f| f != 0)
    }

    /// Whether world input is acted on now (a form actions are allowed
    /// under is open): else it is the game's as plain input.
    pub fn world_input(game: &Civ3) -> bool {
        let Some(g) = LIVE.get().map(|l| l.globals) else { return false };
        game.modal_form().ok().flatten().is_some_and(|f| [g.advisor_form, g.civilopedia_form, g.city_form].contains(&f))
    }

    /// Whether the map may be played now while a screen is open (on the
    /// game's thread, between its messages): see [`super::gate`].
    pub fn screen_gate() -> Result<(), Refused> {
        let live = LIVE.get().ok_or(Refused::Unsupported)?;
        gate(&gate_state(live)?)
    }

    /// Queues `action` for the game's thread.
    pub fn queue(action: Action) {
        QUEUE.send(Pending { at: Instant::now(), action });
    }

    /// Tells the player on the map, at `tile`, why a press there was not
    /// acted on (if it is worth saying, `super::refusal_notice`).
    pub fn refuse(game: &Civ3, tile: (i32, i32), refused: &Refused) {
        let diplomacy = LIVE.get().is_some_and(|l| game.modal_form().ok().flatten() == Some(l.globals.diplo_form));
        let Some(text) = refusal_notice(refused, diplomacy) else { return };
        log::info!("modal input: told the player at {tile:?}: {text} ({refused})");
        let now = Instant::now();
        let mut n = NOTICES.get().unwrap_or_default();
        n.retain(|&(at, t, _)| t != tile && now.saturating_duration_since(at) < super::NOTICE_FOR);
        n.push((now, tile, text));
        NOTICES.publish(n);
    }

    /// The refusal notices showing now, as the game's own map notices are
    /// shown (their font and colours from the game's notice records), with
    /// when each was told.
    pub fn notices(game: &Civ3) -> Vec<(Instant, crate::unit_notices::Notice)> {
        let Some(n) = NOTICES.load() else { return Vec::new() };
        let now = Instant::now();
        n.iter().filter(|&&(at, _, _)| now.saturating_duration_since(at) < super::NOTICE_FOR).filter_map(|&(at, tile, text)| Some((at, crate::unit_notices::synthetic(game, tile, text)?))).collect()
    }

    fn gate_state(live: &Live) -> Result<GateState, Refused> {
        let d = live.game.dialog_state()?.ok_or(Refused::Unsupported)?;
        Ok(GateState {
            modal_form: d.modal_form,
            allowed_forms: [live.globals.advisor_form, live.globals.civilopedia_form, live.globals.city_form],
            depth: d.depth,
            turn_flag: d.turn_flag,
            human_pumps: HUMAN_PUMPS.load(SeqCst),
            animation_pumps: ANIMATION_PUMPS.load(SeqCst),
            online: d.online,
            pbem: d.pbem,
            running: RUNNING.load(SeqCst),
        })
    }

    /// Clears `RUNNING` when the action ends.
    struct Running;

    impl Drop for Running {
        fn drop(&mut self) {
            RUNNING.store(false, SeqCst);
        }
    }

    fn begin() -> Result<Running, Refused> {
        RUNNING.compare_exchange(false, true, SeqCst, SeqCst).map(|_| Running).map_err(|_| Refused::Busy)
    }

    /// Runs `action` now, on the game's thread between its messages (the
    /// virtual map input's handler, `crate::world_input`), if the game is in
    /// a state to.
    pub fn run_now(action: Action) -> Result<String, Refused> {
        let live = LIVE.get().ok_or(Refused::Unsupported)?;
        gate(&gate_state(live)?)?;
        let running = begin()?;
        let done = act(live, action);
        drop(running);
        if done.is_ok() {
            after(live);
        }
        done
    }

    /// After a go-to the game was given through its own map handler under a
    /// screen (`crate::world_input`), with the path it showed for it: the
    /// unit walks that path from the screen's loop (`walk`), the screen
    /// shows the change.
    pub fn after_world_order(path: Option<crate::targeting::GotoPath>) {
        let Some(live) = LIVE.get() else { return };
        after(live);
        let Some(path) = path else { return };
        let Ok(unit) = live.game.current_unit_ptr() else { return };
        let Ok(place) = (unit != 0).then(|| live.game.unit_at_ptr(unit)).ok_or(()).and_then(|r| r.map_err(|_| ())) else { return };
        if place.id != path.unit || place.state != m::UNIT_STATE_GO_TO || (place.x, place.y) == path.to {
            return;
        }
        log::info!("modal input: unit {} walks to {:?} under the screen along {} tiles", place.id, path.to, path.tiles.len());
        WALK.set(Some(Walk { unit, id: place.id, dest: path.to, path: path.tiles, max_moves: path.max_moves, steps: 0 }));
        crate::world_input::wake();
    }

    /// What follows an action under a screen: the city screen redrawn (as
    /// it redraws itself after ordering a unit, 0x4228BE).
    fn after(live: &Live) {
        redraw_city(live);
    }

    fn redraw_city(live: &Live) {
        if live.game.modal_form().ok().flatten() == Some(live.globals.city_form) {
            live.city_form_draw.call(live.globals.city_form);
        }
    }

    /// Runs one queued action, if the game is in a state to (on the game's
    /// thread, from the modal loop), else walks a go-to one step. No lock
    /// is held while the game runs: it may open a dialog of its own (e.g.
    /// to confirm a war) and come back here from that dialog's loop.
    fn run_pending() {
        let Some(live) = LIVE.get() else { return };
        let next = QUEUE.try_recv();
        let Some(pending) = next else { return walk(live) };
        let tile = || match pending.action {
            Action::Select(t) | Action::GoTo(t) => Some(t),
            _ => live.game.current_unit_ptr().ok().filter(|&u| u != 0).and_then(|u| live.game.unit_at_ptr(u).ok()).map(|p| (p.x, p.y)),
        };
        if !pending.fresh(Instant::now()) {
            return log::debug!("modal input: dropping {:?}: {}", pending.action, Refused::Expired);
        }
        if let Err(r) = gate_state(live).and_then(|s| gate(&s).map(|()| s)) {
            if let Some(t) = tile() {
                refuse(&live.game, t, &r);
            }
            return log::debug!("modal input: dropping {:?}: {r}", pending.action);
        }
        let Ok(running) = begin() else {
            return log::debug!("modal input: dropping {:?}: {}", pending.action, Refused::Busy);
        };
        let done = act(live, pending.action);
        drop(running);
        match done {
            Ok(done) => {
                log::info!("modal input: {done}");
                after(live);
            }
            Err(r) => {
                match r {
                    // What the player asked for and the game said no to.
                    Refused::NotSelected { .. } => log::info!("modal input: {r}"),
                    _ => log::debug!("modal input: not doing {:?}: {r}", pending.action),
                }
                if let Some(t) = tile() {
                    refuse(&live.game, t, &r);
                }
            }
        }
    }

    /// Puts a walk back unless a newer one was started meanwhile.
    fn keep(w: Walk) {
        let newer = WALK.take();
        WALK.set(Some(newer.unwrap_or(w)));
    }

    /// One step of the go-to being walked under a screen: the game's own
    /// move to the next tile of the path it showed (`perform_action_on_tile`
    /// next to the unit, which moves it now, 0x4E6F2A, with its movement
    /// rules and animation). When the unit stops (arrived, out of move
    /// points, blocked, no longer selected), the go-to order is given again
    /// from there for the rest, which the game walks on later turns as usual.
    fn walk(live: &Live) {
        let Some(mut w) = WALK.take() else { return };
        if gate_state(live).and_then(|s| gate(&s)).is_err() {
            // (A dialog over the screen, an animation: later.)
            if live.game.modal_form().ok().flatten().is_some_and(|f| f != 0) {
                keep(w);
            }
            return;
        }
        let game = &live.game;
        let (Ok(selected), Ok(place)) = (game.current_unit_ptr(), game.unit_at_ptr(w.unit)) else { return };
        let used = game.unit(place.id).ok().flatten().map_or(i32::MAX, |u| u.moves);
        let at = (place.x, place.y);
        let next = super::next_step(&w.path, at);
        let foreign = next.and_then(|t| game.player_civ().ok().map(|p| may_enter_tile(live, p, t))).and_then(Result::err);
        if !walk_goes_on(&w, selected, place.id, used) || next.is_none() || foreign.is_some() {
            if let Some(r) = foreign.as_ref() {
                refuse(game, next.unwrap_or(at), r);
            }
            let rest = at != w.dest && selected == w.unit && place.id == w.id;
            if rest && let Ok(running) = begin() {
                // The rest of the way: the game's own far go-to order.
                live.perform_action_on_tile.call(game.addrs.main_screen_form, m::ACTION_GO_TO, w.dest.0 as u32, w.dest.1 as u32);
                drop(running);
                redraw_city(live);
            }
            let state = game.unit(w.id).ok().flatten().map(|u| u.state);
            return log::info!("modal input: unit {} walked {} steps under the screen to {at:?} ({}, state {state:?})", w.id, w.steps,
                if at == w.dest { "arrived" } else { "the rest on later turns" });
        }
        let Some(to) = next else { return };
        let Ok(running) = begin() else { return keep(w) };
        live.perform_action_on_tile.call(game.addrs.main_screen_form, m::ACTION_GO_TO, to.0 as u32, to.1 as u32);
        drop(running);
        let now = game.unit(w.id).ok().flatten().map(|u| (u.x, u.y));
        w.steps += 1;
        redraw_city(live);
        log::debug!("modal input: unit {} steps from {at:?} to {to:?}: now at {now:?}", w.id);
        if now == Some(at) {
            // Not moved (blocked, refused): stop there, the order for the rest.
            w.max_moves = 0;
        }
        keep(w);
        crate::world_input::wake();
    }

    /// The selected unit (`Unit *`) and where it is; it must be the player's.
    fn selected(live: &Live, player: i32) -> Result<(u32, civ3_sys::modal::UnitPlace), Refused> {
        let unit = live.game.current_unit_ptr()?;
        if unit == 0 {
            return Err(Refused::NoSelection);
        }
        let place = live.game.unit_at_ptr(unit)?;
        if place.civ != player {
            return Err(Refused::NoSelection);
        }
        Ok((unit, place))
    }

    fn may_enter_tile(live: &Live, player: i32, tile: (i32, i32)) -> Result<(), Refused> {
        let units = live.game.units_on(tile.0, tile.1)?;
        let city = live.game.cities()?.into_iter().find(|c| (c.x, c.y) == tile).map(|c| c.civ);
        may_enter(player, tile, units.iter().map(|u| u.civ), city)
    }

    /// Whether the viewing player's selected unit may be sent onto `tile`
    /// (not an attack), for go-to orders given through the game's own map
    /// handler (`crate::world_input`).
    pub fn may_enter_now(tile: (i32, i32)) -> Result<(), Refused> {
        let live = LIVE.get().ok_or(Refused::Unsupported)?;
        may_enter_tile(live, live.game.player_civ()?, tile)
    }

    /// Where unit `id` is now.
    fn now_at(live: &Live, id: i32) -> Option<(i32, i32)> {
        live.game.unit(id).ok().flatten().map(|u| (u.x, u.y))
    }

    fn go(live: &Live, player: i32, from: civ3_sys::modal::UnitPlace, to: (i32, i32)) -> Result<String, Refused> {
        may_enter_tile(live, player, to)?;
        if (from.x, from.y) == to {
            return Ok(format!("unit {} is at {to:?} already", from.id));
        }
        let screen = live.game.addrs.main_screen_form;
        live.perform_action_on_tile.call(screen, m::ACTION_GO_TO, to.0 as u32, to.1 as u32);
        Ok(format!("unit {} sent from {:?} to {to:?}: now at {:?}", from.id, (from.x, from.y), now_at(live, from.id)))
    }

    fn act(live: &Live, action: Action) -> Result<String, Refused> {
        let game = &live.game;
        let player = game.player_civ()?;
        let screen = game.addrs.main_screen_form;
        match action {
            Action::Select(tile) => {
                let unit = live.find_visible_unit.call(screen, tile.0 as u32, tile.1 as u32, 0);
                if unit == 0 {
                    return Err(Refused::NoOwnUnit(tile));
                }
                let place = game.unit_at_ptr(unit)?;
                if place.civ != player {
                    return Err(Refused::NoOwnUnit(tile));
                }
                if game.current_unit_ptr()? == unit {
                    return Ok(format!("unit {} at {tile:?} is selected already", place.id));
                }
                live.unit_select.call(unit);
                let now = game.current_unit_ptr()?;
                selection_outcome(place.id, tile, now == unit, game.current_unit()?)
            }
            Action::GoTo(tile) => {
                let (_, from) = selected(live, player)?;
                go(live, player, from, tile)
            }
            Action::Step(dir) => {
                let (_, from) = selected(live, player)?;
                let map = game.map()?;
                let to = step((from.x, from.y), dir, (map.width, map.height), (map.wrap_x, map.wrap_y)).ok_or(Refused::OffMap)?;
                go(live, player, from, to)
            }
            Action::Command(command) => {
                let (unit, place) = selected(live, player)?;
                if live.unit_can_perform_command.call(unit, command.value()) & 0xFF == 0 {
                    return Err(Refused::CannotDo(command));
                }
                let done = live.issue_command.call(screen, command.value(), unit) & 0xFF != 0;
                let state = game.unit(place.id).ok().flatten().map(|u| u.state);
                Ok(format!("{command:?} for unit {} at {:?}: {done}, state {:?} -> {state:?}", place.id, (place.x, place.y), place.state))
            }
            Action::ArmGoTo => {
                let (unit, place) = selected(live, player)?;
                // The map's G key does nothing while the city screen shows
                // (`handle_key_down` returns at 0x4E06CC): the command it
                // gives, as holding the button on the unit gives it.
                let go_to = civ3_sys::modal::command::GO_TO;
                if live.unit_can_perform_command.call(unit, go_to) & 0xFF == 0 {
                    return Err(Refused::CannotGoTo);
                }
                live.issue_command.call(screen, go_to, unit);
                let armed = game.targeting().is_ok_and(|t| t.armed && t.mode == civ3_sys::targeting::GO_TO);
                Ok(format!("go-to for unit {} at {:?}: armed {armed}", place.id, (place.x, place.y)))
            }
        }
    }
}

/// What became of asking the game to select `unit` on `tile`: whether it
/// is the selected unit now (`taken`), and which unit is (`selected`).
pub fn selection_outcome(unit: i32, tile: (i32, i32), taken: bool, selected: Option<i32>) -> Result<String, Refused> {
    if taken {
        Ok(format!("selected unit {unit} at {tile:?}"))
    } else {
        Err(Refused::NotSelected { unit, tile, selected })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> GateState {
        GateState {
            modal_form: 0x9A_FD58,
            allowed_forms: [0x9A_FD58, 0x9E_85B0, 0x74_09C0],
            depth: 1,
            turn_flag: true,
            human_pumps: 1,
            animation_pumps: 0,
            online: false,
            pbem: false,
            running: false,
        }
    }

    #[test]
    fn gate_lets_through_an_advisor_the_civilopedia_and_the_city_screen_in_the_players_turn() {
        assert_eq!(gate(&open()), Ok(()));
        assert_eq!(gate(&GateState { modal_form: 0x9E_85B0, ..open() }), Ok(()));
        assert_eq!(gate(&GateState { modal_form: 0x74_09C0, ..open() }), Ok(()));
    }

    #[test]
    fn gate_refuses_everything_else() {
        let cases = [
            (GateState { modal_form: 0, ..open() }, Refused::NoMenu),
            // Diplomacy, a popup.
            (GateState { modal_form: 0xA3_51C0, ..open() }, Refused::OtherForm(0xA3_51C0)),
            (GateState { modal_form: 0x9B_7510, ..open() }, Refused::OtherForm(0x9B_7510)),
            // A popup over the city screen.
            (GateState { modal_form: 0x74_09C0, depth: 2, ..open() }, Refused::Nested(2)),
            (GateState { depth: 2, ..open() }, Refused::Nested(2)),
            (GateState { online: true, ..open() }, Refused::Network),
            (GateState { pbem: true, ..open() }, Refused::Network),
            (GateState { turn_flag: false, ..open() }, Refused::NotInTurn),
            (GateState { human_pumps: 0, ..open() }, Refused::NotInTurn),
            (GateState { animation_pumps: 1, ..open() }, Refused::Animating),
            (GateState { running: true, ..open() }, Refused::Busy),
        ];
        for (state, refused) in cases {
            assert_eq!(gate(&state), Err(refused), "{state:?}");
        }
    }

    #[test]
    fn only_allowed_keys_act() {
        assert_eq!(key_action(0x66), Some(Action::Step(Dir::E)));
        assert_eq!(key_action(0x26), Some(Action::Step(Dir::N)));
        assert_eq!(key_action(0x23), Some(Action::Step(Dir::SW)));
        assert_eq!(key_action(0x46), Some(Action::Command(Command::Fortify)));
        assert_eq!(key_action(0x53), Some(Action::Command(Command::Sentry)));
        assert_eq!(key_action(0x20), Some(Action::Command(Command::Skip)));
        assert_eq!(key_action(0x57), Some(Action::Command(Command::Wait)));
        assert_eq!(key_action(0x47), Some(Action::ArmGoTo));
        // Enter (end of turn), Escape, the F-keys, B (build city), the
        // keypad's 5 and letters with other meanings do nothing.
        for vk in [0x0D, 0x1B, 0x70, 0x71, 0x7B, 0x42, 0x65, 0x0C, 0x41, 0x44, 0x10, 0x11] {
            assert_eq!(key_action(vk), None, "{vk:#x}");
        }
    }

    #[test]
    fn a_refused_selection_says_so() {
        assert_eq!(selection_outcome(77, (17, 39), true, Some(77)), Ok("selected unit 77 at (17, 39)".into()));
        let refused = selection_outcome(357, (18, 34), false, Some(175));
        assert_eq!(refused, Err(Refused::NotSelected { unit: 357, tile: (18, 34), selected: Some(175) }));
        let message = refused.unwrap_err().to_string();
        assert!(message.contains("refused to select unit 357") && message.contains("Some(175)"), "{message}");
    }

    #[test]
    fn buttons_select_and_send() {
        assert_eq!(button_action(scene::Button::Left, (3, 5)), Some(Action::Select((3, 5))));
        assert_eq!(button_action(scene::Button::Right, (3, 5)), Some(Action::GoTo((3, 5))));
        assert_eq!(button_action(scene::Button::Middle, (3, 5)), None);
    }

    #[test]
    fn steps_keep_to_the_grid_and_the_map() {
        for d in [Dir::N, Dir::NE, Dir::E, Dir::SE, Dir::S, Dir::SW, Dir::W, Dir::NW] {
            let (dx, dy) = d.delta();
            assert_eq!((dx + dy).rem_euclid(2), 0, "{d:?}");
        }
        assert_eq!(step((10, 10), Dir::NE, (100, 50), (true, false)), Some((11, 9)));
        // Across the seam of a map that wraps east-west.
        assert_eq!(step((99, 11), Dir::E, (100, 50), (true, false)), Some((1, 11)));
        assert_eq!(step((0, 10), Dir::W, (100, 50), (true, false)), Some((98, 10)));
        // Not past the poles.
        assert_eq!(step((10, 0), Dir::N, (100, 50), (true, false)), None);
        assert_eq!(step((10, 49), Dir::SE, (100, 50), (true, false)), None);
        assert_eq!(step((0, 10), Dir::W, (100, 50), (false, false)), None);
    }

    #[test]
    fn no_moves_onto_foreigners() {
        assert_eq!(may_enter(1, (4, 4), [1, 1].into_iter(), Some(1)), Ok(()));
        assert_eq!(may_enter(1, (4, 4), [].into_iter(), None), Ok(()));
        assert_eq!(may_enter(1, (4, 4), [1, 3].into_iter(), None), Err(Refused::ForeignUnit((4, 4))));
        // Barbarians are civ 0.
        assert_eq!(may_enter(1, (4, 4), [0].into_iter(), None), Err(Refused::ForeignUnit((4, 4))));
        assert_eq!(may_enter(1, (4, 4), [].into_iter(), Some(2)), Err(Refused::ForeignCity((4, 4))));
    }

    #[test]
    fn refused_presses_are_explained_where_it_helps() {
        assert_eq!(refusal_notice(&Refused::OtherForm(0xA3_51C0), true), Some("Not during diplomacy"));
        assert_eq!(refusal_notice(&Refused::OtherForm(0x9B_7510), false), Some("Not while this window is open"));
        assert_eq!(refusal_notice(&Refused::Nested(2), false), Some("Close the dialog first"));
        assert_eq!(refusal_notice(&Refused::ForeignUnit((1, 1)), false), Some("Close the screen to attack"));
        assert_eq!(refusal_notice(&Refused::ForeignCity((1, 1)), false), Some("Close the screen to attack"));
        assert_eq!(refusal_notice(&Refused::NoSelection, false), Some("Select a unit first"));
        assert_eq!(refusal_notice(&Refused::NotGoTo, false), Some("Close the screen for this order"));
        assert_eq!(refusal_notice(&Refused::NotInTurn, false), Some("Only during your turn"));
        // Nothing to say: an empty tile, a moment's wait.
        for r in [Refused::NoOwnUnit((2, 2)), Refused::Animating, Refused::Busy, Refused::Expired, Refused::NoMenu, Refused::OffMap] {
            assert_eq!(refusal_notice(&r, false), None, "{r:?}");
        }
    }

    #[test]
    fn a_walk_follows_the_shown_path_while_the_unit_is_selected_and_can_move() {
        // The path as the game's path finder gives it: destination first.
        let path = vec![(8, 30), (9, 29), (11, 29), (13, 29), (15, 29), (17, 29)];
        assert_eq!(next_step(&path, (17, 29)), Some((15, 29)));
        assert_eq!(next_step(&path, (9, 29)), Some((8, 30)));
        assert_eq!(next_step(&path, (8, 30)), None, "arrived");
        assert_eq!(next_step(&path, (20, 20)), None, "pushed off the path");
        let w = Walk { unit: 0x1000, id: 7, path, dest: (8, 30), max_moves: 3, steps: 0 };
        assert!(walk_goes_on(&w, 0x1000, 7, 1));
        // Another unit selected, another unit at that address, out of moves.
        assert!(!walk_goes_on(&w, 0x2000, 7, 1));
        assert!(!walk_goes_on(&w, 0x1000, 8, 1));
        assert!(!walk_goes_on(&w, 0x1000, 7, 3));
        // Not forever.
        assert!(!walk_goes_on(&Walk { steps: MAX_WALK_STEPS, ..w }, 0x1000, 7, 1));
    }

    #[test]
    fn queued_actions_expire() {
        let t = Instant::now();
        let p = Pending { at: t, action: Action::Command(Command::Wait) };
        assert!(p.fresh(t + Duration::from_millis(100)));
        assert!(!p.fresh(t + EXPIRES + Duration::from_millis(1)));
    }
}
