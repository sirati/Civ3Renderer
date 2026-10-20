//! X11 pointer backend.
//!
//! Capture uses the window library's full-window XGrabPointer, placement its
//! physical XWarpPointer. Regions use XFixes pointer barriers (XFixes 5 or
//! later) on a connection of this crate's own, so no unsafe code is needed:
//! walls along the bounds' four edges and along each exclusion's edges
//! inside them. A one-pixel span in the bounds is a real one-pixel line.
//! Barriers only stop the pointer from crossing; they cannot pull it in.
//! They are therefore made only while the pointer is inside the region
//! (not off it, nor over an exclusion such as a divider or zoom box that
//! moved beneath it); until then the region is reported as not confined
//! and the layout's warps put the pointer back. Without XFixes 5, or if a
//! barrier fails, the layout keeps its warp fallback.
//!
//! The X server applies barriers to relative pointer motion only: absolute
//! devices (tablets, touch screens, a virtual machine's absolute pointer)
//! pass through them.

#![forbid(unsafe_code)]

#![cfg(target_os = "linux")]

use std::sync::Arc;
use pointer_api::{Backend, Error, Grab, Move, Point, Region, Update};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xfixes::{self, ConnectionExt as _};
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

pub struct Controller {
    window: Arc<Window>,
    fallback: pointer_fallback::Controller,
    barriers: Option<Barriers>,
    grabbed: bool,
}

impl Controller {
    pub fn new(window: Arc<Window>) -> Result<Self, Error> {
        let handle = window.window_handle().map_err(|error| Error(error.to_string()))?;
        let Some(id) = x11_window(handle.as_raw()) else { return Err(Error("not an X11 window".into())) };
        let barriers = Barriers::new(id)
            .inspect_err(|error| log::info!("pointer: no X11 pointer barriers ({error}); regions use warps"))
            .ok();
        Ok(Self { fallback: pointer_fallback::Controller::new(window.clone()), window, barriers, grabbed: false })
    }
}

impl Backend for Controller {
    fn grab(&mut self, on: bool) -> Result<Grab, Error> {
        // Barriers would hold a grabbed pointer in the old region.
        if let Some(b) = &mut self.barriers { b.set(None); }
        let result = self.fallback.grab(on)?;
        self.grabbed = result != Grab::Released;
        Ok(result)
    }

    fn move_pointer(&mut self, at: Point) -> Result<Move, Error> { self.fallback.move_pointer(at) }

    fn update(&mut self, region: Option<&Region>, pointer: (f64, f64), inside: bool) -> Update {
        let Some(barriers) = &mut self.barriers else { return Update::default() };
        let pointer = (pointer.0.floor() as i32, pointer.1.floor() as i32);
        let walls = region
            .filter(|r| inside && !self.grabbed && r.contains(pointer))
            .and_then(|r| {
                let origin = self.window.inner_position().ok()?;
                barrier_segments(r, (origin.x, origin.y))
            });
        let wanted = walls.is_some();
        let active = barriers.set(walls);
        Update { confined: wanted && active, placement: None }
    }
}

/// A barrier `[x1, y1, x2, y2]` in root coordinates: horizontal or
/// vertical, both ends inclusive. A vertical barrier at x blocks motion
/// between columns x - 1 and x, a horizontal one at y between rows y - 1
/// and y.
type Segment = [u16; 4];

/// This crate's barriers for one window on its own X connection; closing
/// the connection on drop destroys them too.
struct Barriers {
    conn: RustConnection,
    root: u32,
    ids: Vec<u32>,
    /// The last request and whether it succeeded, so a failure is not
    /// retried every frame.
    current: Option<(Vec<Segment>, bool)>,
}

impl Barriers {
    fn new(window: u32) -> Result<Self, String> {
        let (conn, _) = RustConnection::connect(None).map_err(|e| format!("connecting to the X server: {e}"))?;
        if conn.extension_information(xfixes::X11_EXTENSION_NAME).map_err(|e| e.to_string())?.is_none() {
            return Err("no XFixes extension".into());
        }
        let version = conn.xfixes_query_version(5, 0).map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?;
        if version.major_version < 5 {
            return Err(format!("XFixes {}.{} has no barriers", version.major_version, version.minor_version));
        }
        // Window ids are server-wide, so this connection can look up the
        // window library's window.
        let root = conn.get_geometry(window).map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?.root;
        Ok(Self { conn, root, ids: Vec::new(), current: None })
    }

    /// Replaces the barriers (`None`: removes them). Returns whether the
    /// requested barriers are in place.
    fn set(&mut self, segments: Option<Vec<Segment>>) -> bool {
        let Some(segments) = segments else {
            self.clear();
            return false;
        };
        if let Some((current, ok)) = &self.current {
            if *current == segments { return *ok; }
        }
        self.clear();
        let ok = self.create(&segments);
        if !ok { self.clear(); }
        log::debug!("pointer: X11 barriers {segments:?}: {}", if ok { "made" } else { "failed" });
        self.current = Some((segments, ok));
        ok
    }

    fn create(&mut self, segments: &[Segment]) -> bool {
        let mut cookies = Vec::with_capacity(segments.len());
        for &[x1, y1, x2, y2] in segments {
            let Ok(id) = self.conn.generate_id() else { return false };
            match self.conn.xfixes_create_pointer_barrier(id, self.root, x1, y1, x2, y2, 0u32.into(), &[]) {
                Ok(cookie) => cookies.push(cookie),
                Err(error) => {
                    log::debug!("pointer: X11 barrier: {error}");
                    return false;
                }
            }
            self.ids.push(id);
        }
        // One round trip covers every request sent before it.
        let mut ok = true;
        for cookie in cookies {
            if let Err(error) = cookie.check() {
                log::debug!("pointer: X11 barrier: {error}");
                ok = false;
            }
        }
        ok
    }

    fn clear(&mut self) {
        self.current = None;
        if self.ids.is_empty() { return; }
        for id in self.ids.drain(..) {
            // A barrier that failed to be made reports an error; ignore it.
            if let Ok(cookie) = self.conn.xfixes_delete_pointer_barrier(id) { cookie.ignore_error(); }
        }
        let _ = self.conn.flush();
    }
}

impl Drop for Barriers {
    fn drop(&mut self) { self.clear(); }
}

/// The barriers that hold the pointer in `region` (physical content
/// pixels) for a window whose content starts at `origin` on the root
/// window. The bounds' walls reach one pixel past each corner so that a
/// diagonal step cannot slip between two of them, and so that a one-pixel
/// line's end walls have a length. Exclusions are cut to the bounds; their
/// walls lie on their own edges, leaving every free pixel reachable. `None`
/// for empty bounds or coordinates outside the X protocol's range.
fn barrier_segments(region: &Region, origin: Point) -> Option<Vec<Segment>> {
    let [x, y, w, h] = region.bounds;
    if w <= 0 || h <= 0 { return None; }
    let (x0, y0) = (origin.0 as i64 + x as i64, origin.1 as i64 + y as i64);
    let (x1, y1) = (x0 + w as i64, y0 + h as i64);
    let max = u16::MAX as i64;
    if x0 < 0 || y0 < 0 || x1 > max || y1 > max { return None; }
    let c = |v: i64| v.clamp(0, max);
    let mut walls = vec![
        [c(x0 - 1), y0, c(x1 + 1), y0],
        [c(x0 - 1), y1, c(x1 + 1), y1],
        [x0, c(y0 - 1), x0, c(y1 + 1)],
        [x1, c(y0 - 1), x1, c(y1 + 1)],
    ];
    for &[ex, ey, ew, eh] in &region.exclusions {
        if ew <= 0 || eh <= 0 { continue; }
        let ex0 = (origin.0 as i64 + ex as i64).max(x0);
        let ey0 = (origin.1 as i64 + ey as i64).max(y0);
        let ex1 = (origin.0 as i64 + ex as i64 + ew as i64).min(x1);
        let ey1 = (origin.1 as i64 + ey as i64 + eh as i64).min(y1);
        if ex0 >= ex1 || ey0 >= ey1 { continue; }
        // Inclusive ends: a wall along a hole of n pixels spans n - 1,
        // except that a one-pixel hole's walls need a length.
        let (ex_last, ey_last) = ((ex1 - 1).max(ex0 + 1).min(ex1), (ey1 - 1).max(ey0 + 1).min(ey1));
        // Edges on the bounds' own edges are already walled.
        if ey0 > y0 { walls.push([ex0, ey0, ex_last, ey0]); }
        if ey1 < y1 { walls.push([ex0, ey1, ex_last, ey1]); }
        if ex0 > x0 { walls.push([ex0, ey0, ex0, ey_last]); }
        if ex1 < x1 { walls.push([ex1, ey0, ex1, ey_last]); }
    }
    Some(walls.into_iter().map(|s| s.map(|v| v as u16)).collect())
}

fn x11_window(handle: RawWindowHandle) -> Option<u32> {
    match handle {
        RawWindowHandle::Xlib(h) => u32::try_from(h.window).ok(),
        RawWindowHandle::Xcb(h) => Some(h.window.get()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{num::NonZeroU32, ptr::NonNull};
    use winit::raw_window_handle::{WaylandWindowHandle, XcbWindowHandle, XlibWindowHandle};

    #[test]
    fn selects_both_x11_handle_forms_without_claiming_wayland() {
        assert_eq!(x11_window(RawWindowHandle::Xlib(XlibWindowHandle::new(17))), Some(17));
        assert_eq!(x11_window(RawWindowHandle::Xcb(XcbWindowHandle::new(NonZeroU32::new(17).unwrap()))), Some(17));
        assert_eq!(x11_window(RawWindowHandle::Wayland(WaylandWindowHandle::new(NonNull::dangling()))), None);
    }

    /// The X server's barrier rule (xibarriers.c) for a one-pixel step
    /// from `a` to the neighbouring `b`: blocked by a vertical barrier at x
    /// between columns x - 1 and x for rows y1..=y2, a horizontal one alike.
    fn blocks(walls: &[Segment], a: (i64, i64), b: (i64, i64)) -> bool {
        walls.iter().any(|&[x1, y1, x2, y2]| {
            let [x1, y1, x2, y2] = [x1, y1, x2, y2].map(i64::from);
            if x1 == x2 && a.1 == b.1 && (y1..=y2).contains(&a.1) {
                a.0.min(b.0) == x1 - 1 && a.0.max(b.0) == x1
            } else if y1 == y2 && a.0 == b.0 && (x1..=x2).contains(&a.0) {
                a.1.min(b.1) == y1 - 1 && a.1.max(b.1) == y1
            } else {
                false
            }
        })
    }

    /// Flood fill by one-pixel steps from `start`: every pixel reachable
    /// must be in `region` (shifted by the origin) and every pixel in the
    /// region connected to the start must be reachable.
    fn reachable(walls: &[Segment], start: (i64, i64), area: (i64, i64, i64, i64)) -> Vec<(i64, i64)> {
        let mut seen = vec![start];
        let mut todo = vec![start];
        while let Some(p) = todo.pop() {
            for d in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let q = (p.0 + d.0, p.1 + d.1);
                if q.0 < area.0 || q.1 < area.1 || q.0 >= area.2 || q.1 >= area.3 { continue; }
                if !seen.contains(&q) && !blocks(walls, p, q) {
                    seen.push(q);
                    todo.push(q);
                }
            }
        }
        seen
    }

    fn check(region: &Region, start: (i32, i32)) {
        let origin = (100, 50);
        let walls = barrier_segments(region, origin).unwrap();
        for &[x1, y1, x2, y2] in &walls {
            assert!(x1 == x2 || y1 == y2, "{walls:?}");
            assert!(x1 != x2 || y1 != y2, "zero length: {walls:?}");
        }
        let at = |p: (i32, i32)| (p.0 as i64 + origin.0 as i64, p.1 as i64 + origin.1 as i64);
        let area = (origin.0 as i64 - 3, origin.1 as i64 - 3, origin.0 as i64 + 63, origin.1 as i64 + 43);
        let got = reachable(&walls, at(start), area);
        for p in &got {
            let local = ((p.0 - origin.0 as i64) as i32, (p.1 - origin.1 as i64) as i32);
            assert!(region.contains(local), "{region:?}: escaped to {local:?}");
        }
        // In these regions every allowed pixel is connected.
        for x in -3..63 { for y in -3..43 {
            if region.contains((x, y)) {
                assert!(got.contains(&at((x, y))), "{region:?}: cannot reach {:?}", (x, y));
            }
        }}
    }

    fn edge(bounds: pointer_api::Rect) -> Region {
        Region { bounds, exclusions: pointer_api::outside((60, 40), bounds) }
    }

    #[test]
    fn horizontal_only_line_is_one_row() {
        let r = edge([5, 20, 50, 1]);
        check(&r, (30, 20));
        let walls = barrier_segments(&r, (0, 0)).unwrap();
        assert_eq!(walls[..4], [[4, 20, 56, 20], [4, 21, 56, 21], [5, 19, 5, 22], [55, 19, 55, 22]]);
    }

    #[test]
    fn vertical_only_line_is_one_column() {
        check(&edge([30, 2, 1, 36]), (30, 10));
    }

    #[test]
    fn corner_area_and_single_pixel() {
        check(&edge([30, 20, 30, 20]), (40, 30));
        check(&edge([0, 39, 1, 1]), (0, 39));
    }

    #[test]
    fn both_axes_free_is_the_whole_window() {
        check(&Region { bounds: [0, 0, 60, 40], exclusions: vec![] }, (10, 10));
    }

    #[test]
    fn holes_inside_the_bounds_are_walled_and_outside_ones_ignored() {
        let r = Region {
            bounds: [0, 0, 60, 40],
            // A divider with an opening, a zoom box, a one-pixel hole, and
            // a wall wholly outside the bounds.
            exclusions: vec![[30, 0, 1, 15], [30, 25, 1, 15], [5, 5, 10, 8], [45, 30, 1, 1], [70, 0, 5, 5]],
        };
        check(&r, (2, 2));
        assert_eq!(barrier_segments(&r, (0, 0)).unwrap().len(), 4 + 3 + 3 + 4 + 4);
        // A divider crossing a horizontal line cuts it in two.
        let mut line = edge([5, 20, 50, 1]);
        line.exclusions.push([30, 0, 2, 40]);
        let walls = barrier_segments(&line, (100, 50)).unwrap();
        let got = reachable(&walls, (110, 70), (97, 47, 163, 93));
        assert!(got.contains(&(129, 70)) && !got.contains(&(130, 70)) && !got.contains(&(132, 70)));
    }

    #[test]
    fn unrepresentable_regions_make_no_barriers() {
        assert_eq!(barrier_segments(&Region { bounds: [0, 0, 0, 5], exclusions: vec![] }, (0, 0)), None);
        assert_eq!(barrier_segments(&Region { bounds: [0, 0, 5, 5], exclusions: vec![] }, (-10, 0)), None);
        assert_eq!(barrier_segments(&Region { bounds: [0, 0, 5, 5], exclusions: vec![] }, (65_534, 0)), None);
        // At the screen's top-left corner the outer walls stop at 0.
        assert_eq!(barrier_segments(&Region { bounds: [0, 0, 1, 1], exclusions: vec![] }, (0, 0)).unwrap(),
            vec![[0, 0, 2, 0], [0, 1, 2, 1], [0, 0, 0, 2], [1, 0, 1, 2]]);
    }
}
