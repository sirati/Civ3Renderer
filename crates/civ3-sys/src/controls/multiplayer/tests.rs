use super::*;
use crate::controls::advisors::cell_kind;
use crate::controls::{Sites, controls};
use std::collections::HashMap;

#[derive(Default)]
struct Fake(HashMap<u32, u8>);

impl Fake {
    fn put(&mut self, at: u32, v: u32) {
        for (i, b) in v.to_le_bytes().iter().enumerate() {
            self.0.insert(at + i as u32, *b);
        }
    }
    fn byte(&mut self, at: u32, v: u8) {
        self.0.insert(at, v);
    }
    /// An inclusive RECT (left, top, right, bottom).
    fn rect(&mut self, at: u32, r: [i32; 4]) {
        for (i, v) in r.into_iter().enumerate() {
            self.put(at + 4 * i as u32, v as u32);
        }
    }
    /// A shown root form at the screen's (ox, oy), `w`×`h`, and modal.
    fn form(&mut self, f: u32, vtable: u32, (ox, oy): (i32, i32), (w, h): (i32, i32)) {
        self.put(f, vtable);
        self.put(f + 0x9C, 0x100_0000);
        self.put(f + 0xA0, 1);
        self.put(f + 0x130, 0);
        self.rect(f + 0x1AC, [ox, oy, ox + w, oy + h]);
        self.rect(f + 0x1BC, [0; 4]);
        self.rect(f + 0x1DC, [0, 0, 1920, 1080]);
        self.put(f + 0x22C, 0);
        self.put(f + 0x380, 0);
        self.put(0x100, f);
        self.put(0x200, 1);
        self.put(0x300, f);
        self.put(0x304, 0);
    }
}

impl Mem for Fake {
    fn u32(&self, addr: u32) -> Option<u32> {
        let b: Vec<u8> = (0..4).map(|i| self.0.get(&(addr + i)).copied()).collect::<Option<_>>()?;
        Some(u32::from_le_bytes(b.try_into().unwrap()))
    }
    fn u8(&self, addr: u32) -> Option<u8> {
        self.0.get(&addr).copied()
    }
}

fn sites() -> Sites {
    Sites { roots: 0x100, root_count: 0x200, modal: 0x300, companion: 0x304, main_gui: 0x9F_DB10, advisor: 0x9A_FD58 }
}

const ORIGIN: (i32, i32) = (448, 156);

fn inside([l, t, r, b]: [i32; 4], (x, y): (i32, i32)) -> bool {
    x >= l && x <= r && y >= t && y <= b
}

/// Every edge pixel of a rectangle and every fifth one inside it.
fn points([x, y, w, h]: [i32; 4]) -> impl Iterator<Item = (i32, i32)> {
    (y..y + h)
        .flat_map(move |py| (x..x + w).map(move |px| (px, py)))
        .filter(move |&(px, py)| px == x || py == y || px == x + w - 1 || py == y + h - 1 || ((px - x) % 5 == 0 && (py - y) % 5 == 0))
}

// ---- Setup ----

const SETUP_FORM: u32 = 0x91_C388;
const RULE_FLAG_VALUES: [u32; 18] = [0x1, 0x2, 0x4, 0x8, 0x10, 0x1_0000, 0x40, 0x80, 0x100, 0x200, 0x400, 0x800, 0x1000, 0x2000, 0x4000, 0x8000, 0x2_0000, 0x4_0000];

/// The setup screen as 0x485DC0 sets it up, the local player in slot 1
/// of a network game, `role` the session's (2: host).
fn setup_fixture(role: u32) -> Fake {
    let f = SETUP_FORM;
    let mut m = Fake::default();
    m.form(f, SETUP, ORIGIN, (1024, 768));
    m.rect(f + S_RENAME, [0x17D, 0x18F, 0x28F, 0x1A6]);
    m.rect(f + S_LAUNCH, [0x29F, 0x18F, 0x38B, 0x1A6]);
    m.rect(f + S_NAME, [0x2A, 0x74, 0x13D, 0x90]);
    // 0x486737: the local slot's box.
    m.rect(f + S_READY, [0x3C9, 0x56 + 35, 0x3E1, 0x56 + 35 + 0x17]);
    m.byte(f + S_FIXED_PLAYERS, 0);
    // 0x485DC0: the rows' player number buttons.
    for k in 0..SLOTS {
        let top = 0x78 + 0x23 * k as i32;
        m.rect(f + S_PLAYER_BUTTONS + 16 * k, [0x172, top, 0x188, top + 0x16]);
        // The host's row human, the others open.
        m.put(SLOT_KINDS + SLOT_SIZE * k, if k == 0 { 2 } else { 1 });
    }
    // A new game: not loaded.
    m.put(LOADED_A, 0);
    m.put(LOADED_B, -1i32 as u32);
    m.byte(f + S_COUNTDOWN, 0);
    m.put(f + S_RULES_SCROLL, 0);
    m.byte(NETWORK_GAME, 1);
    m.put(LOCAL_ID, 77);
    for k in 1..=SLOTS {
        m.put(SLOT_IDS + SLOT_SIZE * (k - 1), if k == 1 { 77 } else { 0 });
        m.byte(SLOT_READY + SLOT_SIZE * k, 0);
    }
    m.put(CURRENT_PLAYER, 1);
    m.put(SESSION_ROLE, role);
    m.put(SETUP_STATE, 0);
    for (i, v) in RULE_FLAG_VALUES.iter().enumerate() {
        m.put(RULE_FLAGS + 4 * i as u32, *v);
    }
    m.put(RULES_A, 1);
    m.put(RULES_B, 0);
    m.put(RULES_STATE, 0);
    m.byte(RULES_LOCKED, 0);
    m
}

/// What the setup's click (0x48BE80) does at the form's pixel `p`, for
/// the host or not: Rename, Ready, the name, Launch, or rule `i` (as
/// 0x48E4E0 finds it).
#[derive(Clone, Debug, PartialEq)]
enum Native {
    Rename,
    Ready,
    Name,
    Launch,
    Rule(u32),
    Player(u32),
}

fn native(m: &Fake, p: (i32, i32)) -> Vec<Native> {
    let f = SETUP_FORM;
    let rect = |at: u32| crate::controls::rect_at(m, f + at).unwrap();
    // The local slot, as the click finds it.
    let slot = if m.u8(NETWORK_GAME).unwrap() == 0 {
        m.u32(CURRENT_PLAYER).unwrap()
    } else {
        (1..=8).find(|k| m.u32(SLOT_IDS + SLOT_SIZE * (k - 1)) == m.u32(LOCAL_ID)).unwrap_or(0)
    };
    let ready = m.u8(SLOT_READY + SLOT_SIZE * slot).unwrap() != 0;
    let mut out = Vec::new();
    let fixed = m.u8(f + S_FIXED_PLAYERS).unwrap() != 0;
    let loaded = m.i32(LOADED_A).unwrap() > 0 && m.i32(LOADED_B).unwrap() > -1;
    let state = m.i32(SETUP_STATE).unwrap();
    if !ready && !fixed && inside(rect(S_RENAME), p) {
        out.push(Native::Rename);
    }
    if !matches!(state, 3 | 4) && inside(rect(S_READY), p) {
        out.push(Native::Ready);
    }
    // 0x48C1DD: the own row's button (+0x6206C + 16·slot).
    if !ready && (loaded || fixed) && inside(rect(S_NAME + 16 * slot), p) {
        out.push(Native::Player(slot - 1));
    }
    if !matches!(m.u32(SESSION_ROLE).unwrap(), 2 | 4) {
        return out;
    }
    if inside(rect(S_NAME), p) {
        out.push(Native::Name);
    }
    if inside(rect(S_LAUNCH), p) && m.u8(f + S_COUNTDOWN).unwrap() == 0 {
        out.push(Native::Launch);
    }
    if !ready && inside([0x168, 0x1BE, 0x3EC, 0x24E], p) {
        let scroll = m.i32(f + S_RULES_SCROLL).unwrap();
        for i in 0..18u32 {
            let flag = m.u32(RULE_FLAGS + 4 * i).unwrap();
            let open = (m.u32(RULES_A).unwrap() != 0 && flag & 0x1001F != 0) || (m.u32(RULES_B).unwrap() != 0 && flag & 0x6FFE0 != 0) || m.i32(RULES_STATE).unwrap() == 3;
            let open = open && flag != 0x20000 && m.u8(RULES_LOCKED).unwrap() == 0;
            let row = (i % 9) as i32 - scroll;
            if !(0..5).contains(&row) || !open {
                continue;
            }
            let top = (row % 5) * 23 + 0x1D0;
            let left = if i >= 9 { 0x2A0 } else { 0x2A0 - 290 };
            if inside([left, top, left + 0xFC, top + 0x14], p) {
                out.push(Native::Rule(i));
                break;
            }
        }
    }
    // 0x48CFBF: every human row's (0x910234 + 0x63C·k: number, kind),
    // where drawn: the click also acts in the setup's state 3 or 4 with
    // nothing drawn (and crashes the game there), no control.
    if !ready && (fixed || loaded) {
        for k in 0..8u32 {
            let kind = m.u32(0x910234 + 0x63C * k + 4).unwrap();
            if matches!(kind, 2 | 3) && inside(rect(0x62088 - 0xC + 16 * k), p) && !out.contains(&Native::Player(k)) {
                out.push(Native::Player(k));
            }
        }
    }
    out
}

fn setup_found(m: &Fake) -> Vec<(Native, [i32; 4])> {
    controls(m, &sites())
        .into_iter()
        .filter(|c| c.id >> 32 == SETUP_FORM as u64)
        .map(|c| {
            let index = c.id as u32 & 0xF_FFFF;
            let what = match (cell_kind(c.id), index) {
                (Some(CELL_BUTTON), 0) => Native::Ready,
                (Some(CELL_BUTTON), 1) => Native::Rename,
                (Some(CELL_BUTTON), 2) => Native::Launch,
                (Some(CELL_BUTTON), 3) => Native::Name,
                (Some(CELL_RULE), i) => Native::Rule(i),
                (Some(CELL_PLAYER), k) => Native::Player(k),
                other => panic!("an unexpected control {other:?}"),
            };
            assert_eq!(c.kind, ControlKind::Button);
            (what, c.rect)
        })
        .collect()
}

/// Every pixel of every control found is the native click's, and every
/// control the native click acts on was found.
fn check_setup(m: &Fake, context: &str) -> Vec<Native> {
    let found = setup_found(m);
    for (what, r) in &found {
        for (x, y) in points(*r) {
            assert_eq!(native(m, (x - ORIGIN.0, y - ORIGIN.1)), vec![what.clone()], "{context}: {what:?} at ({x}, {y})");
        }
    }
    for y in (0..768).step_by(8) {
        for x in (0..1024).step_by(8) {
            for hit in native(m, (x, y)) {
                assert!(found.iter().any(|(w, r)| *w == hit && inside([r[0], r[1], r[0] + r[2] - 1, r[1] + r[3] - 1], (x + ORIGIN.0, y + ORIGIN.1))),
                    "{context}: the native click acts on {hit:?} at ({x}, {y}): found");
            }
        }
    }
    found.into_iter().map(|(w, _)| w).collect()
}

#[test]
fn the_hosts_setup_has_rename_ready_name_launch_and_the_open_rules_shown() {
    let mut m = setup_fixture(2);
    // Rules A (flags in 0x1001F): rules 0..5; five shown, 0..4.
    let found = check_setup(&m, "host");
    assert_eq!(found, [Native::Rename, Native::Ready, Native::Name, Native::Launch, Native::Rule(0), Native::Rule(1), Native::Rule(2), Native::Rule(3), Native::Rule(4)]);
    // Rules B too: the right column (9..13 shown; 16, 0x20000, never).
    m.put(RULES_B, 1);
    let found = check_setup(&m, "host, both rule sets");
    assert!(found.contains(&Native::Rule(9)) && found.contains(&Native::Rule(13)) && !found.contains(&Native::Rule(14)));
    // Scrolled four rows: 4..8 and 13..17, less 16 (0x20000).
    m.put(SETUP_FORM + S_RULES_SCROLL, 4);
    let rules: Vec<u32> = check_setup(&m, "scrolled").into_iter().filter_map(|w| if let Native::Rule(i) = w { Some(i) } else { None }).collect();
    assert_eq!(rules, [4, 5, 6, 7, 8, 13, 14, 15, 17]);
    // The state where every rule is open (3): Ready no more.
    m.put(SETUP_FORM + S_RULES_SCROLL, 0);
    m.put(SETUP_STATE, 3);
    m.put(RULES_STATE, 3);
    let found = check_setup(&m, "state 3");
    assert!(!found.contains(&Native::Ready));
    // Locked rules: none of them.
    m.byte(RULES_LOCKED, 1);
    assert!(check_setup(&m, "locked").iter().all(|w| !matches!(w, Native::Rule(_))));
}

#[test]
fn ready_the_host_keeps_ready_and_launch_and_a_joining_player_has_only_rename_and_ready() {
    let mut m = setup_fixture(2);
    m.byte(SLOT_READY + SLOT_SIZE, 1);
    assert_eq!(check_setup(&m, "ready host"), [Native::Ready, Native::Name, Native::Launch]);
    // The countdown runs: no Launch.
    m.byte(SETUP_FORM + S_COUNTDOWN, 1);
    assert_eq!(check_setup(&m, "countdown"), [Native::Ready, Native::Name]);
    // A player who joined (role 1).
    let mut m = setup_fixture(1);
    assert_eq!(check_setup(&m, "joined"), [Native::Rename, Native::Ready]);
    m.byte(SLOT_READY + SLOT_SIZE, 1);
    assert_eq!(check_setup(&m, "joined, ready"), [Native::Ready]);
    // Not a network game: the current player's slot (here 3, not ready
    // while 1 is).
    let mut m = setup_fixture(2);
    m.byte(NETWORK_GAME, 0);
    m.put(CURRENT_PLAYER, 3);
    m.byte(SLOT_READY + SLOT_SIZE * 3, 0);
    m.byte(SLOT_READY + SLOT_SIZE, 1);
    assert!(check_setup(&m, "hotseat").contains(&Native::Rename));
}

// ---- Lobby ----

const LOBBY_FORM: u32 = 0x7C_D400;

/// The lobby as 0x47E520 sets it up, with games named `names` ("" an
/// empty entry) and the list scrolled by `scroll`.
fn lobby_fixture(names: &[&str], scroll: i32) -> Fake {
    let f = LOBBY_FORM;
    let mut m = Fake::default();
    m.form(f, LOBBY, ORIGIN, (1024, 768));
    for i in 0..6u32 {
        let top = [0x90, 0xB1, 0xD2, 0xF3, 0x114, 0x135][i as usize];
        m.rect(f + L_RECTS + 16 * i, [0x19, top, 0xCD, top + 0x18]);
    }
    for (i, (l, r)) in [(0xE4, 0x1A3), (0x1AA, 0x232), (0x239, 0x2C1), (0x2C7, 0x31F), (0x327, 0x37E), (0x385, 0x3C9)].into_iter().enumerate() {
        m.rect(f + L_RECTS + 16 * (6 + i as u32), [l, 0x5A, r, 0x72]);
    }
    m.rect(f + L_RECTS + 16 * 12, [0x19, 0x5A, 0xCD, 0x72]);
    m.rect(f + L_RECTS + 16 * 13, [0x1A, 0x48, 0x2A + 0x2C + 0x40, 0x58]);
    m.put(f + L_SCROLL, scroll as u32);
    m.byte(EMPTY_NAME, 0);
    for i in 0..=L_GAMES_LAST {
        let name = names.get(i as usize).copied().unwrap_or("");
        for (k, b) in name.bytes().chain([0]).enumerate() {
            m.byte(f + L_GAMES + L_GAME_SIZE * i + k as u32, b);
        }
    }
    m
}

/// What the lobby's click (0x481640) does at the form's pixel `p`: one of
/// its rectangles, or the game list's entry it selects.
fn lobby_native(m: &Fake, named: &[bool], (x, y): (i32, i32)) -> Option<(bool, u32)> {
    let f = LOBBY_FORM;
    for i in [0u32, 2, 1, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13] {
        if inside(crate::controls::rect_at(m, f + L_RECTS + 16 * i).unwrap(), (x, y)) {
            return Some((false, i));
        }
    }
    if x > 0xEC && x < 0x3C2 && y > 0x84 && y < 0x232 {
        let mut n = (y - 0x84) / 24 + m.i32(f + L_SCROLL).unwrap();
        let mut i = 0;
        while n > 0 && i < 0x7FF {
            if named[i as usize] {
                n -= 1;
            }
            i += 1;
        }
        return named[i as usize].then_some((true, i));
    }
    None
}

fn check_lobby(m: &Fake, context: &str) -> Vec<u32> {
    let named: Vec<bool> = (0..=L_GAMES_LAST).map(|i| m.u8(LOBBY_FORM + L_GAMES + L_GAME_SIZE * i).unwrap() != 0).collect();
    let found: Vec<_> = controls(m, &sites()).into_iter().filter(|c| c.id >> 32 == LOBBY_FORM as u64).collect();
    let what = |c: &Control| {
        let index = c.id as u32 & 0xF_FFFF;
        match cell_kind(c.id) {
            Some(CELL_GAME) => (true, index),
            Some(CELL_HEADER) => (false, 6 + index),
            Some(CELL_BUTTON) => (false, index),
            other => panic!("an unexpected control {other:?}"),
        }
    };
    for c in &found {
        assert_eq!(c.kind == ControlKind::ListItem, what(c).0, "{context}: {c:?}");
        for (x, y) in points(c.rect) {
            assert_eq!(lobby_native(m, &named, (x - ORIGIN.0, y - ORIGIN.1)), Some(what(c)), "{context}: {c:?} at ({x}, {y})");
        }
    }
    for y in (0..768).step_by(8) {
        for x in (0..1024).step_by(8) {
            if let Some(hit) = lobby_native(m, &named, (x, y)) {
                assert!(found.iter().any(|c| what(c) == hit), "{context}: the native click acts on {hit:?} at ({x}, {y}): found");
            }
        }
    }
    found.iter().filter(|c| c.kind == ControlKind::ListItem).map(|c| c.id as u32 & 0xF_FFFF).collect()
}

#[test]
fn the_lobbys_buttons_titles_and_listed_games_hit_its_native_tests() {
    // Button 4 takes no click: not a control.
    let m = lobby_fixture(&["a", "b", "c"], 0);
    assert_eq!(check_lobby(&m, "three games"), [0, 1, 2]);
    assert!(controls(&m, &sites()).iter().all(|c| !(cell_kind(c.id) == Some(CELL_BUTTON) && c.id as u32 & 0xF_FFFF == 4)));
    // Scrolled by one: the second and third.
    assert_eq!(check_lobby(&lobby_fixture(&["a", "b", "c"], 1), "scrolled"), [1, 2]);
    // An empty entry between two: the click selects nothing past it.
    assert_eq!(check_lobby(&lobby_fixture(&["a", "", "c"], 0), "a gap"), [0]);
    // No games: no rows, the buttons all there.
    let m = lobby_fixture(&[], 0);
    assert_eq!(check_lobby(&m, "none"), Vec::<u32>::new());
    assert_eq!(controls(&m, &sites()).len(), 13);
}

#[test]
fn the_player_number_buttons_are_there_in_a_loaded_game_or_a_scenario_with_fixed_players_only() {
    // A new standard game: none drawn, none listed, also in the setup's
    // state 3 or 4, where the host's click would act (and, with no player
    // data, crash the game: 0x48D182).
    let mut m = setup_fixture(2);
    assert!(setup_found(&m).iter().all(|(w, _)| !matches!(w, Native::Player(_))));
    for state in [3, 4] {
        m.put(SETUP_STATE, state);
        assert!(setup_found(&m).iter().all(|(w, _)| !matches!(w, Native::Player(_))), "state {state}");
    }
    m.put(SETUP_STATE, 0);
    // Two humans joined a loaded game: the host reaches both their rows.
    m.put(LOADED_A, 3);
    m.put(LOADED_B, 0);
    m.put(SLOT_KINDS + SLOT_SIZE * 2, 3);
    let players: Vec<u32> = check_setup(&m, "loaded, host").into_iter().filter_map(|w| if let Native::Player(k) = w { Some(k) } else { None }).collect();
    assert_eq!(players, [0, 2]);
    // The host ready: none of them (Ready and Launch stay).
    m.byte(SLOT_READY + SLOT_SIZE, 1);
    assert_eq!(check_setup(&m, "loaded, host ready"), [Native::Ready, Native::Name, Native::Launch]);
    // A player who joined (in slot 3): their own row's only.
    let mut m = setup_fixture(1);
    m.put(LOADED_A, 3);
    m.put(LOADED_B, 0);
    m.put(LOCAL_ID, 5);
    m.put(SLOT_IDS + SLOT_SIZE * 2, 5);
    m.put(SLOT_KINDS + SLOT_SIZE * 2, 3);
    assert_eq!(check_setup(&m, "loaded, joined"), [Native::Rename, Native::Ready, Native::Player(2)]);
    // A scenario with fixed players: no Rename, the own row's button.
    m.put(LOADED_A, 0);
    m.byte(SETUP_FORM + S_FIXED_PLAYERS, 1);
    assert_eq!(check_setup(&m, "scenario, joined"), [Native::Ready, Native::Player(2)]);
}
