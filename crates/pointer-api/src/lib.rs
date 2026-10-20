//! Shared pointer backend contract. No game, rendering or platform types.

#![forbid(unsafe_code)]

/// A rectangle `[x, y, width, height]` in physical window pixels.
pub type Rect = [i32; 4];
pub type Point = (i32, i32);

#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    /// Outer confinement rectangle. A one-pixel span holds that axis.
    pub bounds: Rect,
    /// Areas which the pointer cannot enter or cross.
    pub exclusions: Vec<Rect>,
}

impl Region {
    pub fn contains(&self, at: Point) -> bool {
        contains(self.bounds, at) && !self.exclusions.iter().any(|&r| contains(r, at))
    }
}

pub fn contains([x, y, w, h]: Rect, at: Point) -> bool {
    w > 0 && h > 0 && at.0 >= x && at.1 >= y && (at.0 as i64) < x as i64 + w as i64 && (at.1 as i64) < y as i64 + h as i64
}

/// Complement of a rectangle within a physical window.
pub fn outside((w, h): (i32, i32), [x, y, rw, rh]: Rect) -> Vec<Rect> {
    let (w, h) = (w.max(0), h.max(0));
    let (x0, y0) = (x.clamp(0, w), y.clamp(0, h));
    let (x1, y1) = (((x as i64 + rw as i64).clamp(x0 as i64, w as i64)) as i32, ((y as i64 + rh as i64).clamp(y0 as i64, h as i64)) as i32);
    [[0, 0, w, y0], [0, y1, w, h - y1], [0, y0, x0, y1 - y0], [x1, y0, w - x1, y1 - y0]]
        .into_iter().filter(|r| r[2] > 0 && r[3] > 0).collect()
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Grab { Released, Locked, Confined }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Move { Immediate, Pending }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    /// The compositor committed the hint. It can still ignore its position.
    Hinted(Point),
    Cancelled,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Update {
    pub confined: bool,
    pub placement: Option<Placement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for Error {}

/// Backends own one window and release their own constraints on drop.
/// Every region and placement uses physical content-area pixels. The
/// backend converts to display coordinates exactly once.
pub trait Backend {
    /// Release custom regions before asking the window system to grab.
    fn grab(&mut self, on: bool) -> Result<Grab, Error>;
    fn move_pointer(&mut self, at: Point) -> Result<Move, Error>;
    fn moving(&self) -> bool { false }
    /// `None` releases region confinement. `inside` includes keyboard focus.
    fn update(&mut self, region: Option<&Region>, pointer: (f64, f64), inside: bool) -> Update;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_excludes_holes_and_keeps_a_single_pixel_axis() {
        let r = Region { bounds: [400, 0, 1, 600], exclusions: vec![[300, 0, 200, 90]] };
        assert!(r.contains((400, 90)));
        assert!(!r.contains((399, 90)));
        assert!(!r.contains((401, 90)));
        assert!(!r.contains((400, 89)));
    }

    /// The four shapes a captured-edge region takes in an 800×600 window,
    /// with the complement of the bounds also passed as exclusions (as the
    /// layout does).
    #[test]
    fn captured_edge_regions_hold_exactly_the_locked_axes() {
        let region = |bounds| Region { bounds, exclusions: outside((800, 600), bounds) };
        // Horizontal movement only: row 300.
        let h = region([100, 300, 600, 1]);
        assert!(h.contains((100, 300)) && h.contains((699, 300)));
        for at in [(100, 299), (100, 301), (99, 300), (700, 300), (400, 299), (400, 301)] {
            assert!(!h.contains(at), "{at:?}");
        }
        // Vertical movement only: column 400.
        let v = region([400, 50, 1, 500]);
        assert!(v.contains((400, 50)) && v.contains((400, 549)));
        for at in [(399, 300), (401, 300), (400, 49), (400, 550)] {
            assert!(!v.contains(at), "{at:?}");
        }
        // Both axes held at a corner: one pixel.
        let c = region([0, 599, 1, 1]);
        assert!(c.contains((0, 599)));
        for at in [(1, 599), (0, 598), (1, 598)] {
            assert!(!c.contains(at), "{at:?}");
        }
        // Both axes free: the whole window, holes still excluded.
        let free = Region { bounds: [0, 0, 800, 600], exclusions: vec![[400, 0, 1, 200]] };
        assert!(free.contains((0, 0)) && free.contains((799, 599)) && free.contains((400, 200)));
        assert!(!free.contains((400, 199)) && !free.contains((800, 0)) && !free.contains((-1, 0)));
    }

    #[test]
    fn complements_cover_every_point_except_the_requested_region() {
        for r in [[3, 2, 1, 6], [2, 4, 6, 1], [2, 2, 5, 6], [-3, -2, 8, 6]] {
            let holes = outside((10, 10), r);
            for x in 0..10 { for y in 0..10 {
                assert_eq!(holes.iter().any(|&hole| contains(hole, (x, y))), !contains(r, (x, y)));
            }}
        }
    }

    #[test]
    fn large_rectangles_do_not_overflow() {
        assert!(contains([i32::MAX - 1, 0, 10, 1], (i32::MAX, 0)));
        assert!(!contains([i32::MAX - 1, 0, 10, 1], (i32::MIN, 0)));
        assert_eq!(outside((20, 20), [i32::MAX - 1, 0, 10, 1]), vec![[0, 1, 20, 19], [0, 0, 20, 1]]);
    }
}
