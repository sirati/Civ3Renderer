//! A canvas's display list, indexed by where its operations start: the
//! game draws thousands of opaque fills onto one canvas (the map before
//! its camera is ours, a list redrawing its rows), and each one drops what
//! lies inside it. Looking those up in the cells the fill covers, instead
//! of in the whole list, keeps every draw's work proportional to what it
//! touches, not to what the canvas holds.
//!
//! Removed operations leave a hole until holes outnumber operations; then
//! the list is rebuilt, which keeps removal from the middle cheap and the
//! order of what remains (the order it is drawn in) unchanged.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use super::{Op, covered, intersect, op_bounds};

/// Cells of 64 × 64 pixels.
const CELL_SHIFT: u32 = 6;
/// Fewest holes worth a rebuild.
const MIN_HOLES: usize = 64;

/// A grid cell's key from its column and row.
fn cell_key(cx: i32, cy: i32) -> u64 {
    (u64::from(cx as u32) << 32) | u64::from(cy as u32)
}

/// Hashes cell keys (one multiply; the default hasher costs more than the
/// lookup it serves on every draw).
#[derive(Default, Clone, Copy)]
pub(super) struct CellHasher(u64);

impl Hasher for CellHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }
    fn write_u64(&mut self, i: u64) {
        self.0 = (self.0.rotate_left(5) ^ i).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

pub(super) type CellMap<V> = HashMap<u64, V, BuildHasherDefault<CellHasher>>;

/// The cells (column range, row range) holding the points of `[l, t, r, b]`
/// (inclusive), and how many there are; none for an empty range.
fn cells([l, t, r, b]: [i32; 4]) -> Option<((i32, i32), (i32, i32), i64)> {
    if r < l || b < t {
        return None;
    }
    let (x0, x1, y0, y1) = (l >> CELL_SHIFT, r >> CELL_SHIFT, t >> CELL_SHIFT, b >> CELL_SHIFT);
    Some(((x0, x1), (y0, y1), (i64::from(x1) - i64::from(x0) + 1) * (i64::from(y1) - i64::from(y0) + 1)))
}

#[derive(Clone, Debug)]
pub(super) struct DisplayList {
    /// Oldest first; `None` where an operation was removed.
    slots: Vec<Option<Op>>,
    live: usize,
    /// Slots by the cell of their bounds' top-left corner.
    grid: CellMap<Vec<u32>>,
    /// Slots whose bounds are inverted (their corner says nothing about
    /// where they lie): looked at by every search.
    odd: Vec<u32>,
    /// Changes with every change of what the list holds, unique among all
    /// lists ever made (a canvas cleared and drawn again never has an
    /// earlier revision).
    revision: u64,
    /// How many operations held copy another surface (what they show
    /// changes with it).
    surfaces: usize,
}

fn next_revision() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn copies_surface(op: &Op) -> bool {
    matches!(op, Op::Image { source: super::Source::Surface { .. }, .. })
}

impl Default for DisplayList {
    fn default() -> Self {
        DisplayList { slots: Vec::new(), live: 0, grid: CellMap::default(), odd: Vec::new(), revision: next_revision(), surfaces: 0 }
    }
}

impl std::fmt::Debug for CellHasher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CellHasher")
    }
}

impl From<Vec<Op>> for DisplayList {
    fn from(ops: Vec<Op>) -> Self {
        let mut list = DisplayList::default();
        for op in ops {
            list.push(op);
        }
        list
    }
}

impl DisplayList {
    pub fn len(&self) -> usize {
        self.live
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Op> + Clone {
        self.slots.iter().flatten()
    }

    /// See `revision`.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Whether an operation held copies another surface.
    pub fn copies_surfaces(&self) -> bool {
        self.surfaces > 0
    }

    pub fn first(&self) -> Option<&Op> {
        self.iter().next()
    }

    pub fn to_vec(&self) -> Vec<Op> {
        self.iter().cloned().collect()
    }

    pub fn push(&mut self, op: Op) {
        let slot = u32::try_from(self.slots.len()).expect("display lists stay far below 2^32 slots");
        self.index(slot, &op);
        self.surfaces += usize::from(copies_surface(&op));
        self.revision = next_revision();
        self.slots.push(Some(op));
        self.live += 1;
    }

    fn index(&mut self, slot: u32, op: &Op) {
        let [l, t, r, b] = op_bounds(op);
        if l <= r && t <= b {
            self.grid.entry(cell_key(l >> CELL_SHIFT, t >> CELL_SHIFT)).or_default().push(slot);
        } else {
            self.odd.push(slot);
        }
    }

    fn kill(&mut self, slot: u32) {
        if let Some(op) = self.slots[slot as usize].take() {
            self.live -= 1;
            self.surfaces -= usize::from(copies_surface(&op));
            self.revision = next_revision();
        }
    }

    /// Rebuilds the list once holes outnumber operations.
    fn settle(&mut self) {
        let holes = self.slots.len() - self.live;
        if holes < MIN_HOLES || holes <= self.live {
            return;
        }
        let ops: Vec<Op> = std::mem::take(&mut self.slots).into_iter().flatten().collect();
        self.grid.clear();
        self.odd.clear();
        self.live = 0;
        self.surfaces = 0;
        for op in ops {
            self.push(op);
        }
    }

    /// Removes every operation `pred` holds for. Only those whose bounds
    /// start (top-left, inclusive) within `starts` or are inverted can be
    /// among them (the caller's promise); the rest are not looked at.
    pub fn remove_where(&mut self, starts: [i32; 4], pred: impl Fn(&Op) -> bool) {
        let Self { slots, live, grid, odd, revision, surfaces } = self;
        let mut test = |slot: u32| match &slots[slot as usize] {
            Some(op) if pred(op) => {
                *surfaces -= usize::from(copies_surface(op));
                slots[slot as usize] = None;
                *live -= 1;
                *revision = next_revision();
                false
            }
            Some(_) => true,
            None => false,
        };
        odd.retain(|&s| test(s));
        if let Some(((x0, x1), (y0, y1), n)) = cells(starts) {
            if n.saturating_mul(4) > grid.len() as i64 {
                // As many cells as the list has: through all of them.
                grid.retain(|_, v| {
                    v.retain(|&s| test(s));
                    !v.is_empty()
                });
            } else {
                for cy in y0..=y1 {
                    for cx in x0..=x1 {
                        let key = cell_key(cx, cy);
                        if let Some(v) = grid.get_mut(&key) {
                            v.retain(|&s| test(s));
                            if v.is_empty() {
                                grid.remove(&key);
                            }
                        }
                    }
                }
            }
        }
        self.settle();
    }

    /// Removes the latest operation equal to `op`.
    pub fn remove_last_equal(&mut self, op: &Op) {
        let [l, t, r, b] = op_bounds(op);
        let candidates = if l <= r && t <= b { self.grid.get(&cell_key(l >> CELL_SHIFT, t >> CELL_SHIFT)) } else { Some(&self.odd) };
        let found = candidates.and_then(|v| v.iter().copied().filter(|&s| self.slots[s as usize].as_ref() == Some(op)).max());
        if let Some(slot) = found {
            self.kill(slot);
            self.settle();
        }
    }

    /// Removes the operations at positions `range` of what the list holds.
    pub fn drain(&mut self, range: std::ops::Range<usize>) {
        let doomed: Vec<u32> = self.slots.iter().enumerate().filter(|(_, s)| s.is_some()).map(|(i, _)| i as u32)
            .skip(range.start).take(range.end.saturating_sub(range.start)).collect();
        for slot in doomed {
            self.kill(slot);
        }
        self.settle();
    }

    /// Keeps the operations whose entry in `keep` (one per operation held,
    /// in order) is true.
    pub fn retain_marked(&mut self, keep: &[bool]) {
        let doomed: Vec<u32> = self.slots.iter().enumerate().filter(|(_, s)| s.is_some()).map(|(i, _)| i as u32)
            .zip(keep).filter(|(_, k)| !**k).map(|(s, _)| s).collect();
        for slot in doomed {
            self.kill(slot);
        }
        self.settle();
    }
}

/// Most cells a covering rectangle is filed under; larger ones are tested
/// by every search.
const MAX_COVER_CELLS: i64 = 64;

/// Opaque rectangles (left, top, right, bottom; none empty), in the order
/// they were added, indexed by the cells they overlap.
#[derive(Default)]
pub(super) struct Cover {
    rects: Vec<[i32; 4]>,
    grid: CellMap<Vec<u32>>,
    large: Vec<u32>,
    /// Scratch for a search.
    near: Vec<u32>,
}

/// The cells overlapped by `[l, t, r, b)` (not empty).
fn overlapped([l, t, r, b]: [i32; 4]) -> Option<((i32, i32), (i32, i32), i64)> {
    cells([l, t, r - 1, b - 1])
}

impl Cover {
    pub fn len(&self) -> usize {
        self.rects.len()
    }

    pub fn push(&mut self, rect: [i32; 4]) {
        let i = self.rects.len() as u32;
        self.rects.push(rect);
        match overlapped(rect) {
            Some(((x0, x1), (y0, y1), n)) if n <= MAX_COVER_CELLS => {
                for cy in y0..=y1 {
                    for cx in x0..=x1 {
                        self.grid.entry(cell_key(cx, cy)).or_default().push(i);
                    }
                }
            }
            _ => self.large.push(i),
        }
    }

    /// Whether the rectangles together cover `rect` (not empty): exactly
    /// what [`covered`] says of all of them in order, from those that
    /// overlap it (the others take nothing away from it).
    pub fn covers(&mut self, rect: [i32; 4]) -> bool {
        let overlaps = |c: &[i32; 4]| {
            let i = intersect(rect, *c);
            i[2] > i[0] && i[3] > i[1]
        };
        self.near.clear();
        match overlapped(rect) {
            Some(((x0, x1), (y0, y1), n)) if n <= MAX_COVER_CELLS => {
                self.near.extend_from_slice(&self.large);
                for cy in y0..=y1 {
                    for cx in x0..=x1 {
                        if let Some(v) = self.grid.get(&cell_key(cx, cy)) {
                            self.near.extend_from_slice(v);
                        }
                    }
                }
                self.near.sort_unstable();
                self.near.dedup();
            }
            _ => self.near.extend(0..self.rects.len() as u32),
        }
        let rects = &self.rects;
        self.near.retain(|&i| overlaps(&rects[i as usize]));
        !self.near.is_empty() && covered(rect, self.near.iter().map(|&i| rects[i as usize]))
    }
}

impl<'a> IntoIterator for &'a DisplayList {
    type Item = &'a Op;
    type IntoIter = std::iter::Flatten<std::slice::Iter<'a, Option<Op>>>;
    fn into_iter(self) -> Self::IntoIter {
        self.slots.iter().flatten()
    }
}
