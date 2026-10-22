//! Physical-pixel pointer regions and the state used by the AppKit backend.
//!
//! This module has no AppKit dependencies so the collision and warp handling
//! can also be tested on hosts without a macOS window server.

#![forbid(unsafe_code)]

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Region {
    bounds: [f64; 4],
    holes: Vec<[f64; 4]>,
}

impl Region {
    pub fn new(bounds: [i32; 4], holes: &[[i32; 4]]) -> Option<Self> {
        if bounds[2] <= 0 || bounds[3] <= 0 {
            return None;
        }
        let rect = |[x, y, w, h]: [i32; 4]| [x as f64, y as f64, x as f64 + w as f64, y as f64 + h as f64];
        Some(Self {
            bounds: rect(bounds),
            holes: holes.iter().copied().filter(|r| r[2] > 0 && r[3] > 0).map(rect).collect(),
        })
    }

    pub fn contains(&self, (x, y): (f64, f64)) -> bool {
        let inside = |[x0, y0, x1, y1]: [f64; 4]| x >= x0 && y >= y0 && x < x1 && y < y1;
        x.is_finite() && y.is_finite() && inside(self.bounds) && !self.holes.iter().copied().any(inside)
    }

    /// Move through the region without crossing a hole, even when an event
    /// jumps across a one-pixel divider. On collision the pointer can slide
    /// along the wall. The fixed axis of a one-pixel region stays exact.
    pub fn advance(&self, from: (f64, f64), delta: (f64, f64)) -> (f64, f64) {
        if !self.contains(from) || !delta.0.is_finite() || !delta.1.is_finite() {
            return from;
        }
        let mut at = from;
        // Resolve each axis against the walls before taking the next one.
        // Each segment is inside the region, including for large diagonal
        // events that would otherwise tunnel through a thin rectangle.
        for axis in 0..2 {
            let (value, other, change, lo, hi) = if axis == 0 {
                (at.0, at.1, delta.0, self.bounds[0], self.bounds[2] - 1.0)
            } else {
                (at.1, at.0, delta.1, self.bounds[1], self.bounds[3] - 1.0)
            };
            let mut target = (value + change).clamp(lo, hi);
            for &[x0, y0, x1, y1] in &self.holes {
                let (start, end, cross0, cross1) = if axis == 0 { (x0, x1, y0, y1) } else { (y0, y1, x0, x1) };
                if other < cross0 || other >= cross1 {
                    continue;
                }
                if value < start && target >= start {
                    target = target.min(start - 1.0).max(lo);
                } else if value >= end && target < end {
                    target = target.max(end).min(hi);
                }
            }
            if axis == 0 { at.0 = target; } else { at.1 = target; }
        }
        // Overlapping or newly moved walls must never put the pointer into a
        // different forbidden rectangle. The caller releases a stale region.
        if self.contains(at) { at } else { from }
    }
}

/// A point of the flipped view (`point`, in points; origin top-left) in
/// physical view pixels, for a view `size` points large at `scale`
/// physical pixels a point.
///
/// AppKit's own y axis grows upwards. Flipped, the topmost pointer position
/// is 0, but the pointer resting on the bottom row reports the view's bottom
/// boundary itself, `y == height`: one past the last pixel row, which edge
/// scrolling, confinement and lock entry all treat as outside the window.
/// A position on a far boundary belongs to that edge's last pixel, as one
/// on a near boundary belongs to its first, so that all four edges behave
/// alike. Points further out (a drag past the window) stay where they are.
pub(super) fn view_pixels(point: (f64, f64), size: (f64, f64), scale: f64) -> (f64, f64) {
    let axis = |v: f64, len: f64| {
        let (v, len) = (v * scale, (len * scale).round());
        if len >= 1.0 && v >= len && v < len + 1.0 { len - 1.0 } else { v }
    };
    (axis(point.0, size.0), axis(point.1, size.1))
}

/// A key window can receive a capture shortcut while the system pointer is
/// over another window or the menu bar. Detachment alone would route the
/// next click there. Keep an inside pointer where it is; otherwise place it
/// in the middle of the content before accepting captured input.
pub(super) fn locked_position(at: (f64, f64), size: (f64, f64)) -> Option<(f64, f64)> {
    if !size.0.is_finite() || !size.1.is_finite() || size.0 < 1.0 || size.1 < 1.0 {
        return None;
    }
    if at.0.is_finite() && at.1.is_finite()
        && at.0 >= 0.0 && at.1 >= 0.0 && at.0 < size.0 && at.1 < size.1
    {
        Some(at)
    } else {
        Some(((size.0 / 2.0).floor(), (size.1 / 2.0).floor()))
    }
}

#[derive(Debug, Default)]
pub(super) struct PointerState {
    pub locked: bool,
    pub region: Option<Region>,
    pub position: (f64, f64),
    pub menu_depth: u32,
    menu_suspended: bool,
}

impl PointerState {
    pub fn disassociated(&self) -> bool { self.locked || self.region.is_some() }

    pub fn begin_menu(&mut self, focused: bool) -> bool {
        self.menu_depth = self.menu_depth.saturating_add(1);
        let suspend = self.menu_depth == 1 && focused;
        if suspend { self.menu_suspended = true; }
        suspend
    }

    pub fn end_menu(&mut self) -> bool {
        self.menu_depth = self.menu_depth.saturating_sub(1);
        if self.menu_depth == 0 { std::mem::take(&mut self.menu_suspended) } else { false }
    }

    pub fn release(&mut self) {
        self.locked = false;
        self.region = None;
    }
}

/// CGWarpMouseCursorPosition emits no motion event, but its displacement
/// can be included in the next NSEvent delta. Remove that displacement once
/// before either raw device motion or confinement consumes the event.
#[derive(Debug, Default)]
pub(super) struct WarpCorrection {
    pending: (f64, f64),
}

impl WarpCorrection {
    pub fn warp(&mut self, before: (f64, f64), after: (f64, f64)) {
        // Use the actual position immediately before this warp. The last
        // event delivered to this application may predate motion while a
        // different application had focus.
        self.pending.0 += before.0 - after.0;
        self.pending.1 += before.1 - after.1;
    }

    pub fn clear(&mut self) { self.pending = (0.0, 0.0); }

    pub fn motion(&mut self, delta: (f64, f64)) -> (f64, f64) {
        let correction = std::mem::take(&mut self.pending);
        (delta.0 + correction.0, delta.1 + correction.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_edges_and_single_axes_are_physical_pixels() {
        for scale in [1, 2, 3] {
            let pane = Region::new([400 * scale, 0, 400 * scale, 600 * scale], &[]).unwrap();
            let start = (450.0 * scale as f64, 250.0 * scale as f64);
            assert_eq!(pane.advance(start, (-9000.0, 9000.0)), ((400 * scale) as f64, (600 * scale - 1) as f64));
            let vertical = Region::new([451 * scale, 100 * scale, 1, 400 * scale], &[]).unwrap();
            let at = ((451 * scale) as f64, (250 * scale) as f64);
            assert_eq!(vertical.advance(at, (120.0, 30.0)), (at.0, at.1 + 30.0));
            let horizontal = Region::new([100 * scale, 251 * scale, 600 * scale, 1], &[]).unwrap();
            let at = ((250 * scale) as f64, (251 * scale) as f64);
            assert_eq!(horizontal.advance(at, (30.0, 120.0)), (at.0 + 30.0, at.1));
        }
    }

    /// The captured-edge shapes as the renderer passes them: the bounds'
    /// complement in an 800×600 window is listed as exclusions too.
    #[test]
    fn captured_edge_lines_corners_and_free_areas_with_complement_holes() {
        let complement = |[x, y, w, h]: [i32; 4]| {
            vec![[0, 0, 800, y], [0, y + h, 800, 600 - y - h], [0, y, x, h], [x + w, y, 800 - x - w, h]]
        };
        let region = |b: [i32; 4]| Region::new(b, &complement(b)).unwrap();
        // Horizontal only: the row stays exact under any motion.
        let h = region([100, 300, 600, 1]);
        let mut at = (400.0, 300.0);
        for delta in [(7.5, 3.0), (-2.25, -9.0), (9000.0, 0.4), (-9000.0, -0.6)] {
            at = h.advance(at, delta);
            assert_eq!(at.1, 300.0);
        }
        assert_eq!(at, (100.0, 300.0));
        // Vertical only.
        let v = region([400, 50, 1, 500]);
        assert_eq!(v.advance((400.0, 300.0), (5.0, 9000.0)), (400.0, 549.0));
        assert_eq!(v.advance((400.0, 300.0), (-5.0, -9000.0)), (400.0, 50.0));
        // Corner area: both axes free inside the bottom-right quadrant.
        let c = region([400, 300, 400, 300]);
        assert_eq!(c.advance((500.0, 400.0), (-9000.0, -9000.0)), (400.0, 300.0));
        assert_eq!(c.advance((500.0, 400.0), (9000.0, 9000.0)), (799.0, 599.0));
        assert_eq!(c.advance((500.0, 400.0), (12.0, -30.0)), (512.0, 370.0));
        // Both free: the whole window, a divider still holding.
        let free = Region::new([0, 0, 800, 600], &[[400, 0, 2, 600]]).unwrap();
        assert_eq!(free.advance((100.0, 100.0), (9000.0, 30.0)), (399.0, 130.0));
        assert_eq!(free.advance((100.0, 100.0), (-9000.0, -9000.0)), (0.0, 0.0));
    }

    #[test]
    fn thin_dividers_block_large_moves_but_openings_pass_them() {
        let region = Region::new([0, 0, 800, 600], &[[400, 0, 1, 200], [400, 400, 1, 200]]).unwrap();
        assert_eq!(region.advance((100.0, 100.0), (600.0, 0.0)), (399.0, 100.0));
        assert_eq!(region.advance((700.0, 100.0), (-600.0, 0.0)), (401.0, 100.0));
        assert_eq!(region.advance((100.0, 300.0), (600.0, 0.0)), (700.0, 300.0));
    }

    #[test]
    fn zoom_strips_and_boxes_hold_the_cursor_outside() {
        let strip = Region::new([0, 0, 800, 600], &[[0, 0, 800, 90]]).unwrap();
        assert_eq!(strip.advance((450.0, 300.0), (30.0, -9000.0)), (480.0, 90.0));
        let zoom = Region::new([0, 0, 800, 600], &[[300, 200, 200, 150]]).unwrap();
        assert_eq!(zoom.advance((250.0, 250.0), (450.0, 20.0)), (299.0, 270.0));
        assert_eq!(zoom.advance((550.0, 250.0), (-450.0, 20.0)), (500.0, 270.0));
        assert!(!zoom.contains((400.0, 250.0)));
    }

    #[test]
    fn invalid_regions_and_nonfinite_motion_never_warp() {
        assert_eq!(Region::new([0, 0, 0, 100], &[]), None);
        let region = Region::new([0, 0, 800, 600], &[[300, 200, 200, 150]]).unwrap();
        assert_eq!(region.advance((400.0, 250.0), (30.0, 20.0)), (400.0, 250.0));
        assert_eq!(region.advance((250.0, 250.0), (f64::NAN, 20.0)), (250.0, 250.0));
    }

    /// AppKit coordinates as a window 800×600 points large reports them:
    /// Cocoa's y from the bottom, flipped by the view (`height − y`).
    #[test]
    fn the_bottom_boundary_is_the_last_row_like_the_top_one_is_the_first() {
        let size = (800.0, 600.0);
        let flipped = |cocoa: (f64, f64)| (cocoa.0, size.1 - cocoa.1);
        for scale in [1.0, 2.0, 3.0] {
            let (w, h) = (size.0 * scale, size.1 * scale);
            // Resting on the bottom row: Cocoa y 0, the view's boundary.
            assert_eq!(view_pixels(flipped((400.0, 0.0)), size, scale), (400.0 * scale, h - 1.0));
            // Resting on the top row: Cocoa y at the top, 0.
            assert_eq!(view_pixels(flipped((400.0, 600.0)), size, scale), (400.0 * scale, 0.0));
            // Left and right boundaries alike; corners both ways.
            assert_eq!(view_pixels(flipped((0.0, 300.0)), size, scale), (0.0, 300.0 * scale));
            assert_eq!(view_pixels(flipped((800.0, 300.0)), size, scale), (w - 1.0, 300.0 * scale));
            assert_eq!(view_pixels(flipped((800.0, 0.0)), size, scale), (w - 1.0, h - 1.0));
            // Inside, fractions kept (one physical pixel from the bottom too).
            assert_eq!(view_pixels((12.25, 34.5), size, scale), (12.25 * scale, 34.5 * scale));
            assert_eq!(view_pixels((10.0, 600.0 - 1.0 / scale), size, scale), (10.0 * scale, h - 1.0));
            // Dragged further out: as it is.
            assert_eq!(view_pixels((10.0, 650.0), size, scale), (10.0 * scale, 650.0 * scale));
            assert_eq!(view_pixels((-5.0, 10.0), size, scale), (-5.0 * scale, 10.0 * scale));
            // The bottom row is inside the window's region and lock entry
            // keeps it there instead of recentring.
            let at = view_pixels(flipped((400.0, 0.0)), size, scale);
            assert!(Region::new([0, 0, w as i32, h as i32], &[]).unwrap().contains(at));
            assert_eq!(locked_position(at, (w, h)), Some(at));
        }
    }

    #[test]
    fn lock_entry_keeps_clicks_in_content_when_the_pointer_starts_outside() {
        for scale in [1.0, 2.0, 3.0] {
            let size = (800.0 * scale, 600.0 * scale);
            let center = (400.0 * scale, 300.0 * scale);
            for outside in [(-200.0, -60.0), (size.0, 10.0), (10.0, size.1), (f64::NAN, 5.0)] {
                assert_eq!(locked_position(outside, size), Some(center));
            }
            for inside in [(0.0, 0.0), (13.25, 17.5), (size.0 - 0.25, size.1 - 0.25)] {
                assert_eq!(locked_position(inside, size), Some(inside));
            }
        }
        assert_eq!(locked_position((-1.0, -1.0), (1.0, 1.0)), Some((0.0, 0.0)));
        for size in [(0.0, 600.0), (800.0, 0.0), (-1.0, 600.0), (f64::NAN, 600.0)] {
            assert_eq!(locked_position((0.0, 0.0), size), None);
        }
    }

    #[test]
    fn focus_close_and_grab_release_clear_all_constraints() {
        let mut state = PointerState { locked: true, ..Default::default() };
        assert!(state.disassociated());
        state.release();
        assert!(!state.disassociated());
        state.region = Region::new([0, 0, 800, 600], &[]);
        assert!(state.disassociated());
        state.release();
        assert!(!state.disassociated());
    }

    #[test]
    fn native_menu_tracking_releases_once_and_resumes_after_nested_menus() {
        let mut state = PointerState::default();
        assert!(state.begin_menu(true));
        state.release();
        assert_eq!(state.menu_depth, 1);
        assert!(!state.begin_menu(true));
        assert!(!state.end_menu());
        assert!(state.end_menu());
        assert_eq!(state.menu_depth, 0);
        assert!(!state.end_menu());
        assert!(!state.begin_menu(false));
        assert!(!state.end_menu());
    }

    #[test]
    fn warp_correction_uses_actual_positions_after_unobserved_mouse_motion() {
        let mut correction = WarpCorrection::default();
        assert_eq!(correction.motion((2.0, 3.0)), (2.0, 3.0));
        // This app last saw the pointer at 102,203. It moved to 700,800
        // while another app had focus, before we warped it to 400,300.
        correction.warp((700.0, 800.0), (400.0, 300.0));
        assert_eq!(correction.motion((-295.0, -493.0)), (5.0, 7.0));
        assert_eq!(correction.motion((2.0, 3.0)), (2.0, 3.0));
    }

    #[test]
    fn focus_loss_discards_warp_displacement_consumed_by_other_applications() {
        let mut correction = WarpCorrection::default();
        correction.warp((100.0, 200.0), (300.0, 400.0));
        correction.clear();
        assert_eq!(correction.motion((5.0, 7.0)), (5.0, 7.0));
    }

    #[test]
    fn multiple_warps_accumulate_only_their_explicit_displacements() {
        let mut correction = WarpCorrection::default();
        correction.warp((100.0, 200.0), (300.0, 400.0));
        // Some hardware motion happened before this second warp. It must
        // not be folded into the displacement that gets removed.
        correction.warp((310.0, 420.0), (600.0, 900.0));
        assert_eq!(correction.motion((505.0, 707.0)), (15.0, 27.0));
        assert_eq!(correction.motion((1.0, 2.0)), (1.0, 2.0));
    }

    #[test]
    fn warps_do_not_become_mouse_motion_or_get_counted_twice() {
        let mut correction = WarpCorrection::default();
        assert_eq!(correction.motion((2.0, 3.0)), (2.0, 3.0));
        correction.warp((102.0, 203.0), (400.0, 300.0));
        assert_eq!(correction.motion((303.0, 104.0)), (5.0, 7.0));
        assert_eq!(correction.motion((2.0, 3.0)), (2.0, 3.0));
        correction.warp((407.0, 310.0), (600.0, 450.0));
        correction.warp((600.0, 450.0), (200.0, 100.0));
        assert_eq!(correction.motion((-203.0, -205.0)), (4.0, 5.0));
    }
}
