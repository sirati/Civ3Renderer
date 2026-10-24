//! The city screen's build queue (docs/civ3-map-rendering.md, "The city
//! screen's build queue"): what the open city builds and has queued, the
//! queue list's form, and the game's own functions that edit them.
//!
//! A city keeps up to [`MAX_QUEUE`] orders after the current one
//! (`City+0x1F8` count, `City+0x1FC + 8i` = `{Order_ID, Order_Type}`; the
//! current order `City+0x4C`/`+0x50`). The city screen edits the queue only
//! through `City_Form::edit_queue(type, id, index, op)` (0x41C1F0; op 1
//! sets slot `index`, appending at the end, op 0 removes it; in a network
//! game it sends the edit instead; it refuses to queue an improvement that
//! is queued or being built already) and sets what is built with
//! `City::set_production(type, id, ask_to_confirm)` (0x4AFB40, C3X wraps
//! it: the shields kept or lost on a switch, the confirmation, the city's
//! yields recomputed). A reorder is planned ([`plan`]) as such calls only.
//!
//! Only the PCGames build is decoded.

use crate::Build;
use crate::controls::Mem;

/// Orders a city keeps after the current one (`City::edit_queue` caps the
/// count at this).
pub const MAX_QUEUE: usize = 9;

/// `p_city_form`, the city screen.
pub const CITY_FORM: u32 = 0x74_09C0;
/// `City_Form.CurrentCity` (`City *`).
const FORM_CITY: u32 = 0x19EC;
/// `City_Form.Order_Queue_List` (the list object `G` of a `Base_List_Box`).
const FORM_QUEUE_LIST: u32 = 0x8364;
/// `City_Form.Order_List`, the production list.
const FORM_ORDER_LIST: u32 = 0x7C7C;
/// `City.Body.ID`, `City.Body.CivID` (a byte).
const CITY_ID: u32 = 0x1C + crate::layout::CITY_BODY_ID;
const CITY_CIV: u32 = 0x1C + crate::layout::CITY_BODY_CIVID;
/// The current order: `City.Body.Order_ID`, `Order_Type`.
const CITY_ORDER_ID: u32 = 0x1C + crate::layout::CITY_BODY_ORDER_ID;
const CITY_ORDER_TYPE: u32 = 0x1C + crate::layout::CITY_BODY_ORDER_TYPE;
/// `City.Body.Order_Queue_Count`, `Orders_Queue[9]` (`{id, type}`).
const CITY_QUEUE_COUNT: u32 = 0x1F8;
const CITY_QUEUE: u32 = 0x1FC;
/// The game's network mode (0x47B5B0 -> 0x49A070: 4 or 5 is a network game).
const NETWORK_MODE: u32 = 0x9A_FD34;
/// The viewing player (`Main_Screen_Form.Player_CivID`).
const PLAYER: u32 = 0x9F_D47C;

/// `Order_Type` of an improvement (`COT_Improvement`) and of a unit.
pub const IMPROVEMENT: i32 = 1;
pub const UNIT: i32 = 2;

/// An order: what a city builds.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Order {
    pub kind: i32,
    pub id: i32,
}

impl Order {
    /// Whether it is something built (an improvement or a unit; a city can
    /// also build nothing of these, e.g. wealth).
    pub fn built(self) -> bool {
        matches!(self.kind, IMPROVEMENT | UNIT) && self.id >= 0
    }
}

/// The open city screen's city and its orders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Queue {
    /// `City *`.
    pub city: u32,
    pub city_id: i32,
    pub civ: i32,
    pub current: Order,
    pub entries: Vec<Order>,
}

impl Queue {
    /// The city screen's city, if the screen has one.
    pub fn read(mem: &dyn Mem) -> Option<Queue> {
        let city = mem.u32(CITY_FORM + FORM_CITY).filter(|&c| c != 0)?;
        let count = mem.i32(city + CITY_QUEUE_COUNT)?;
        if !(0..=MAX_QUEUE as i32).contains(&count) {
            return None;
        }
        let entries = (0..count as u32)
            .map(|i| Some(Order { id: mem.i32(city + CITY_QUEUE + 8 * i)?, kind: mem.i32(city + CITY_QUEUE + 8 * i + 4)? }))
            .collect::<Option<Vec<_>>>()?;
        Some(Queue {
            city,
            city_id: mem.i32(city + CITY_ID)?,
            civ: i32::from(mem.u8(city + CITY_CIV)?),
            current: Order { id: mem.i32(city + CITY_ORDER_ID)?, kind: mem.i32(city + CITY_ORDER_TYPE)? },
            entries,
        })
    }

    /// What an edit is checked against: the city and all of its orders, so
    /// that one planned against an older queue is not applied to a newer.
    pub fn stamp(&self) -> u32 {
        // FNV-1a over the values.
        let mut h: u32 = 0x811C_9DC5;
        let mut eat = |v: i32| {
            for b in v.to_le_bytes() {
                h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
            }
        };
        eat(self.city as i32);
        eat(self.city_id);
        eat(self.current.kind);
        eat(self.current.id);
        eat(self.entries.len() as i32);
        for e in &self.entries {
            eat(e.kind);
            eat(e.id);
        }
        h
    }
}

/// Whether the game is a network game (where the city screen sends its
/// edits to the host rather than making them).
pub fn network_game(mem: &dyn Mem) -> bool {
    mem.u32(NETWORK_MODE).is_some_and(|m| m == 4 || m == 5)
}

/// The viewing player's civilization.
pub fn player(mem: &dyn Mem) -> Option<i32> {
    mem.i32(PLAYER)
}

/// The queue list's form part (`F = G + 4 + vb[1]`): the object the list's
/// entries are named after (`controls::ListItem` ids are `F << 32 | index`;
/// entry `i` is queue slot `i`, the one after the last the slot the next
/// queued order goes to).
pub fn queue_list_form(mem: &dyn Mem) -> Option<u32> {
    let g = CITY_FORM + FORM_QUEUE_LIST;
    let vb = mem.u32(g + 4)?;
    Some(g.wrapping_add(4).wrapping_add(mem.u32(vb.wrapping_add(4))?))
}

/// The production list's form part (as [`queue_list_form`], of
/// `City_Form.Order_List` +0x7C7C): its entries' text ends with the
/// statistics of the unit.
pub fn order_list_form(mem: &dyn Mem) -> Option<u32> {
    let g = CITY_FORM + FORM_ORDER_LIST;
    let vb = mem.u32(g + 4)?;
    Some(g.wrapping_add(4).wrapping_add(mem.u32(vb.wrapping_add(4))?))
}

/// Where a dragged queue entry goes.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Drop {
    /// Before queue slot `n` (as the queue is now; `n` = its length: at
    /// the end).
    Before(usize),
    /// Onto what the city builds now: the entry is built now, what was
    /// built goes to the front of the queue.
    Current,
}

/// One call of the game's.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// `City::set_production(type, id, ask_to_confirm = true)`.
    SetProduction(Order),
    /// `City_Form::edit_queue(0, -1, index, 0)`.
    Remove(usize),
    /// `City_Form::edit_queue(type, id, index, 1)` with `index` the count
    /// then: appended.
    Append(usize, Order),
}

/// The game's calls that move queue entry `from` as `to` says, given what
/// the city builds and has queued; `None` if that changes nothing or is
/// not possible. A reorder takes the entries from the first one that moves
/// out and appends them again in their new order (the game's queue edit
/// has no insert, and it would refuse to queue an improvement still queued
/// elsewhere). A swap sets the production first (the game may ask, and the
/// player may say no: then nothing else is done, [`Plan::checked`]), then
/// does the same with what was built at the front.
pub fn plan(current: Order, queue: &[Order], from: usize, to: Drop) -> Option<Plan> {
    if from >= queue.len() || queue.len() > MAX_QUEUE {
        return None;
    }
    let mut new: Vec<Order> = queue.to_vec();
    let moved = new.remove(from);
    let mut first = None;
    match to {
        Drop::Before(n) => {
            if n > queue.len() {
                return None;
            }
            let at = if n > from { n - 1 } else { n };
            if at == from {
                return None;
            }
            new.insert(at, moved);
        }
        Drop::Current => {
            if !moved.built() {
                return None;
            }
            first = Some(Edit::SetProduction(moved));
            if current.built() {
                new.insert(0, current);
            }
        }
    }
    let keep = queue.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let mut edits: Vec<Edit> = first.into_iter().collect();
    edits.extend((keep..queue.len()).map(|_| Edit::Remove(keep)));
    edits.extend(new.iter().enumerate().skip(keep).map(|(i, &o)| Edit::Append(i, o)));
    Some(Plan { edits, result: (match to { Drop::Current => moved, Drop::Before(_) => current }, new) })
}

/// A planned reorder: the game's calls in order, and what the city builds
/// and has queued once they are done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub edits: Vec<Edit>,
    pub result: (Order, Vec<Order>),
}

impl Plan {
    /// After the production was set: whether the game did set it (the
    /// player may have declined the switch), before the queue is touched.
    pub fn checked(&self, built_now: Order) -> bool {
        match self.edits.first() {
            Some(Edit::SetProduction(o)) => *o == built_now,
            _ => true,
        }
    }
}

inproc::thiscall_fn! {
    /// `void City_Form::edit_queue(this, int order_type, int order_id, int
    /// index, int op)` (0x41C1F0, `ret 0x10`).
    pub fn city_form_edit_queue(city_form; order_type, order_id, index, op) -> ();
}

inproc::thiscall_fn! {
    /// `void City::set_production(this, int order_type, int order_id, bool
    /// ask_to_confirm)` (0x4AFB40, `ret 0xC`; C3X wraps it).
    pub fn city_set_production(city; order_type, order_id, ask_to_confirm) -> ();
}

/// The two functions' addresses and first bytes.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sites {
    pub edit_queue: (u32, &'static [u8]),
    pub set_production: (u32, &'static [u8]),
}

pub fn sites(build: Build) -> Option<Sites> {
    match build {
        Build::PcGames => Some(Sites {
            // push ebx / push ebp / push esi / push edi / mov esi,ecx
            edit_queue: (0x41_C1F0, &[0x53, 0x55, 0x56, 0x57, 0x8B, 0xF1]),
            // sub esp,10h / push ebp / mov ebp,[esp+1Ch]
            set_production: (0x4A_FB40, &[0x83, 0xEC, 0x10, 0x55, 0x8B, 0x6C, 0x24, 0x1C]),
        }),
        Build::Gog | Build::Steam => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const fn imp(id: i32) -> Order { Order { kind: IMPROVEMENT, id } }
    const fn unit(id: i32) -> Order { Order { kind: UNIT, id } }

    /// What the game's calls do (City_Form::edit_queue's guard included).
    fn apply(current: Order, queue: &[Order], edits: &[Edit]) -> (Order, Vec<Order>) {
        let (mut current, mut queue) = (current, queue.to_vec());
        for edit in edits {
            match *edit {
                Edit::SetProduction(o) => current = o,
                Edit::Remove(i) => { assert!(i < queue.len(), "remove {i} of {queue:?}"); queue.remove(i); }
                Edit::Append(i, o) => {
                    assert_eq!(i, queue.len(), "appends at the end");
                    let refused = o.kind == IMPROVEMENT && (current == o || queue.contains(&o));
                    assert!(!refused, "the game refuses to queue {o:?} again: {current:?} {queue:?}");
                    assert!(queue.len() < MAX_QUEUE);
                    queue.push(o);
                }
            }
        }
        (current, queue)
    }

    #[test]
    fn reorders_move_one_entry_and_keep_the_rest_in_order() {
        let cur = imp(1);
        let q = [imp(2), unit(3), imp(4), unit(5)];
        let cases = [
            (0, 2, vec![unit(3), imp(2), imp(4), unit(5)]),
            (0, 4, vec![unit(3), imp(4), unit(5), imp(2)]),
            (3, 0, vec![unit(5), imp(2), unit(3), imp(4)]),
            (2, 1, vec![imp(2), imp(4), unit(3), unit(5)]),
        ];
        for (from, to, want) in cases {
            let p = plan(cur, &q, from, Drop::Before(to)).unwrap();
            assert_eq!(p.result, (cur, want.clone()), "{from} -> {to}");
            assert_eq!(apply(cur, &q, &p.edits), (cur, want), "{from} -> {to}");
        }
    }

    #[test]
    fn only_entries_from_the_first_that_moves_are_rewritten() {
        let q = [imp(2), unit(3), imp(4), unit(5)];
        let p = plan(imp(1), &q, 3, Drop::Before(2)).unwrap();
        assert_eq!(p.edits, vec![Edit::Remove(2), Edit::Remove(2), Edit::Append(2, unit(5)), Edit::Append(3, imp(4))]);
    }

    #[test]
    fn dropping_in_place_or_out_of_range_does_nothing() {
        let q = [imp(2), unit(3)];
        assert_eq!(plan(imp(1), &q, 0, Drop::Before(0)), None);
        assert_eq!(plan(imp(1), &q, 0, Drop::Before(1)), None);
        assert_eq!(plan(imp(1), &q, 1, Drop::Before(2)), None);
        assert_eq!(plan(imp(1), &q, 2, Drop::Before(0)), None);
        assert_eq!(plan(imp(1), &q, 0, Drop::Before(3)), None);
    }

    #[test]
    fn a_swap_builds_the_entry_and_queues_the_old_build_first() {
        let cur = imp(1);
        let q = [unit(2), imp(3), unit(4)];
        let p = plan(cur, &q, 1, Drop::Current).unwrap();
        assert_eq!(p.edits.first(), Some(&Edit::SetProduction(imp(3))), "production first: the player may decline");
        assert_eq!(p.result, (imp(3), vec![imp(1), unit(2), unit(4)]));
        assert_eq!(apply(cur, &q, &p.edits), p.result);
        // The improvement built before is queued only once it is no longer built.
        let set = p.edits.iter().position(|e| matches!(e, Edit::SetProduction(_))).unwrap();
        let queued = p.edits.iter().position(|e| *e == Edit::Append(0, imp(1))).unwrap();
        assert!(set < queued);
    }

    #[test]
    fn a_swap_with_a_full_queue_stays_full_and_the_last_entry_swaps_too() {
        let q: Vec<Order> = (0..9).map(|i| unit(10 + i)).collect();
        let p = plan(unit(1), &q, 8, Drop::Current).unwrap();
        let mut want = vec![unit(1)];
        want.extend(&q[..8]);
        assert_eq!(apply(unit(1), &q, &p.edits), (unit(18), want));
    }

    #[test]
    fn a_swap_when_nothing_is_built_only_builds_the_entry() {
        let none = Order { kind: 0, id: -1 };
        let q = [unit(2), imp(3)];
        let p = plan(none, &q, 0, Drop::Current).unwrap();
        assert_eq!(apply(none, &q, &p.edits), (unit(2), vec![imp(3)]));
    }

    #[test]
    fn a_declined_switch_stops_before_the_queue_changes() {
        let p = plan(imp(1), &[unit(2)], 0, Drop::Current).unwrap();
        assert!(p.checked(unit(2)));
        assert!(!p.checked(imp(1)));
        assert!(plan(imp(1), &[unit(2), unit(3)], 0, Drop::Before(2)).unwrap().checked(imp(1)));
    }

    struct Fake(HashMap<u32, u32>);
    impl Mem for Fake {
        fn u32(&self, a: u32) -> Option<u32> { self.0.get(&a).copied() }
        fn u8(&self, a: u32) -> Option<u8> { self.0.get(&(a & !3)).map(|v| (v >> (8 * (a & 3))) as u8) }
    }

    #[test]
    fn the_queue_is_read_from_the_city_screens_city() {
        let city = 0x0100_0000;
        let mut m = HashMap::from([
            (CITY_FORM + FORM_CITY, city),
            (city + CITY_ID, 17),
            (city + CITY_CIV, 3),
            (city + CITY_ORDER_ID, 5),
            (city + CITY_ORDER_TYPE, 1),
            (city + CITY_QUEUE_COUNT, 2),
            (city + CITY_QUEUE, 7), (city + CITY_QUEUE + 4, 2),
            (city + CITY_QUEUE + 8, 9), (city + CITY_QUEUE + 12, 1),
        ]);
        let q = Queue::read(&Fake(m.clone())).unwrap();
        assert_eq!((q.city_id, q.civ, q.current, q.entries.clone()), (17, 3, imp(5), vec![unit(7), imp(9)]));
        let stamp = q.stamp();
        m.insert(city + CITY_QUEUE + 8, 10);
        assert_ne!(Queue::read(&Fake(m.clone())).unwrap().stamp(), stamp, "any change of an entry changes the stamp");
        m.insert(city + CITY_QUEUE_COUNT, 10);
        assert_eq!(Queue::read(&Fake(m)), None, "a count the game never has");
    }
}
