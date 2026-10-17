//! Playing on the map from a view of the viewer's own while one of the
//! game's menus is open (docs/modal-input.md, approach (a)).
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
}

/// The keys acted on (Windows virtual-key codes); nothing else reaches the
/// game while a menu is open: the keypad (with Num Lock on or off) and the
/// arrows move the selected unit, F fortifies, S sentries, Space skips its
/// turn, W makes it wait.
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
    #[error("the open form {0:#x} is not an advisor or the Civilopedia")]
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

/// What [`gate`] looks at.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct GateState {
    /// The modal form, and the forms acting is allowed under.
    pub modal_form: u32,
    pub allowed_forms: [u32; 2],
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

/// Whether an action may run now: an advisor or the Civilopedia is the
/// only dialog, opened from the human-turn loop (so no end of turn or AI
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
pub use live::{install, modal_open, queue, world_input};

#[cfg(windows)]
mod live {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering::SeqCst};
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    use civ3_sys::Civ3;
    use civ3_sys::modal::{self as m, animation_pump_a, animation_pump_b, human_pump_goto, human_pump_idle, modal_pump};

    use super::{Action, GateState, Pending, Refused, gate, may_enter, selection_outcome, step};

    static QUEUE: Mutex<VecDeque<Pending>> = Mutex::new(VecDeque::new());
    static RUNNING: AtomicBool = AtomicBool::new(false);
    static HUMAN_PUMPS: AtomicI32 = AtomicI32::new(0);
    static ANIMATION_PUMPS: AtomicI32 = AtomicI32::new(0);
    static LIVE: OnceLock<Live> = OnceLock::new();

    struct Live {
        game: Civ3,
        globals: m::Globals,
        find_visible_unit: m::find_visible_unit::Function,
        unit_select: m::unit_select::Function,
        perform_action_on_tile: m::perform_action_on_tile::Function,
        issue_command: m::issue_command::Function,
        unit_can_perform_command: m::unit_can_perform_command::Function,
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

    /// Whether a modal form is open (the map takes no input then).
    pub fn modal_open(game: &Civ3) -> bool {
        game.modal_form().ok().flatten().is_some_and(|f| f != 0)
    }

    /// Whether world input is acted on now (a form actions are allowed
    /// under is open): else it is the game's as plain input.
    pub fn world_input(game: &Civ3) -> bool {
        let Some(g) = LIVE.get().map(|l| l.globals) else { return false };
        game.modal_form().ok().flatten().is_some_and(|f| f == g.advisor_form || f == g.civilopedia_form)
    }

    /// Queues `action` for the game's thread.
    pub fn queue(action: Action) {
        if let Ok(mut q) = QUEUE.lock() {
            q.push_back(Pending { at: Instant::now(), action });
        }
    }

    fn gate_state(live: &Live) -> Result<GateState, Refused> {
        let d = live.game.dialog_state()?.ok_or(Refused::Unsupported)?;
        Ok(GateState {
            modal_form: d.modal_form,
            allowed_forms: [live.globals.advisor_form, live.globals.civilopedia_form],
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

    /// Runs one queued action, if the game is in a state to (on the game's
    /// thread, from the modal loop). No lock is held while the game runs:
    /// it may open a dialog of its own (e.g. to confirm a war) and come
    /// back here from that dialog's loop.
    fn run_pending() {
        let Some(live) = LIVE.get() else { return };
        // (The lock only for taking it out.)
        let next = QUEUE.lock().ok().and_then(|mut q| q.pop_front());
        let Some(pending) = next else { return };
        if !pending.fresh(Instant::now()) {
            return log::debug!("modal input: dropping {:?}: {}", pending.action, Refused::Expired);
        }
        if let Err(r) = gate_state(live).and_then(|s| gate(&s).map(|()| s)) {
            return log::debug!("modal input: dropping {:?}: {r}", pending.action);
        }
        if RUNNING.compare_exchange(false, true, SeqCst, SeqCst).is_err() {
            return log::debug!("modal input: dropping {:?}: {}", pending.action, Refused::Busy);
        }
        let _running = Running;
        match act(live, pending.action) {
            Ok(done) => log::info!("modal input: {done}"),
            // What the player asked for and the game said no to.
            Err(r @ Refused::NotSelected { .. }) => log::info!("modal input: {r}"),
            Err(r) => log::debug!("modal input: not doing {:?}: {r}", pending.action),
        }
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
            allowed_forms: [0x9A_FD58, 0x9E_85B0],
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
    fn gate_lets_through_an_advisor_in_the_players_turn() {
        assert_eq!(gate(&open()), Ok(()));
        assert_eq!(gate(&GateState { modal_form: 0x9E_85B0, ..open() }), Ok(()));
    }

    #[test]
    fn gate_refuses_everything_else() {
        let cases = [
            (GateState { modal_form: 0, ..open() }, Refused::NoMenu),
            (GateState { modal_form: 0x74_09C0, ..open() }, Refused::OtherForm(0x74_09C0)),
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
    fn queued_actions_expire() {
        let t = Instant::now();
        let p = Pending { at: t, action: Action::Command(Command::Wait) };
        assert!(p.fresh(t + Duration::from_millis(100)));
        assert!(!p.fresh(t + EXPIRES + Duration::from_millis(1)));
    }
}
