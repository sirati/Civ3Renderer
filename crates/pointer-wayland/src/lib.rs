//! Wayland implementation of the physical-pixel pointer backend contract.
//!
//! Constraints use the window's own Wayland connection. Their logical
//! coordinates stay inside this crate; callers provide physical window
//! pixels, including one-pixel bounds that hold a single movement axis.

#![forbid(unsafe_code)]

#![cfg(target_os = "linux")]

use std::sync::Arc;

use pointer_api::{Backend, Error, Grab, Move, Placement, Point, Region, Update};
use winit::window::Window;

/// Pointer constraints and lock-and-hint placement for one Wayland window.
pub struct Controller {
    window: Arc<Window>,
    constraints: wl_pointer::Constraints,
    fallback: pointer_fallback::Controller,
    grabbed: bool,
    /// Keep the physical target until its result is delivered, including
    /// when a grab cancels the underlying lock before the next update.
    pending: Option<Point>,
}

impl Controller {
    pub fn new(window: Arc<Window>) -> Result<Self, wl_pointer::Unavailable> {
        let constraints = wl_pointer::Constraints::new(window.clone())?;
        let fallback = pointer_fallback::Controller::new(window.clone());
        Ok(Self {
            window,
            constraints,
            fallback,
            grabbed: false,
            pending: None,
        })
    }
}

impl Backend for Controller {
    fn grab(&mut self, on: bool) -> Result<Grab, Error> {
        // One seat/surface may have only one pointer constraint. Let go
        // before winit makes its own lock or confinement. A release must
        // let a lock-and-hint move finish after the frame that requested it.
        // Do not discard `pending`: a new grab reports cancelled placement
        // on the next update.
        if on { self.constraints.release(); }
        let grab = self.fallback.grab(on)?;
        self.grabbed = grab != Grab::Released;
        Ok(grab)
    }

    fn move_pointer(&mut self, at: Point) -> Result<Move, Error> {
        if self.grabbed {
            return self.fallback.move_pointer(at);
        }
        self.constraints
            .move_pointer(wl_pointer::point_in_surface(at, self.window.scale_factor()));
        self.pending = self.constraints.is_moving().then_some(at);
        if self.pending.is_some() {
            Ok(Move::Pending)
        } else {
            Err(Error(
                "the Wayland seat has no pointer for lock-and-hint placement".into(),
            ))
        }
    }

    fn moving(&self) -> bool {
        self.constraints.is_moving()
    }

    fn update(&mut self, region: Option<&Region>, pointer: (f64, f64), inside: bool) -> Update {
        self.constraints.dispatch();
        let placement = self
            .constraints
            .take_move_result()
            .and_then(|result| placement(&mut self.pending, result));
        // Use the newly hinted position while the corresponding cursor
        // event is still queued, just as the caller will for input state.
        let pointer = match placement {
            Some(Placement::Hinted(at)) => (at.0 as f64, at.1 as f64),
            _ => pointer,
        };
        let Some(region) = region.filter(|_| inside && self.window.has_focus() && !self.grabbed)
        else {
            self.constraints.unconfine();
            return Update {
                confined: false,
                placement,
            };
        };
        let scale = self.window.scale_factor();
        let size = self.window.inner_size();
        let size = surface_size((size.width, size.height), scale);
        match confinement(region, size, scale, pointer) {
            Some(holes) => self.constraints.confine(size, &holes),
            None => self.constraints.unconfine(),
        }
        Update {
            confined: self.constraints.is_confined(),
            placement,
        }
    }
}

fn placement(pending: &mut Option<Point>, result: wl_pointer::MoveResult) -> Option<Placement> {
    pending.take().map(|target| match result {
        wl_pointer::MoveResult::Hinted(_) => Placement::Hinted(target),
        wl_pointer::MoveResult::Cancelled => Placement::Cancelled,
    })
}

fn surface_size((w, h): (u32, u32), scale: f64) -> (i32, i32) {
    let scale = valid_scale(scale);
    (
        (w as f64 / scale).ceil() as i32,
        (h as f64 / scale).ceil() as i32,
    )
}

fn valid_scale(scale: f64) -> f64 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// The holes to confine the pointer with, or `None` to leave it free.
///
/// A divider or zoom box may have moved beneath the pointer. Leave
/// confinement off until the pointer exits it, so the compositor does not
/// relocate the pointer to another part of the region. Only an exclusion
/// over the bounds counts: a pointer outside the bounds (for example one
/// pixel off a held axis's line, or in the bounds' complement, which the
/// layout also lists as exclusions) is still confined. The compositor
/// activates the confinement once the pointer enters the region, where the
/// layout's position hint puts it.
fn confinement(
    region: &Region,
    size: (i32, i32),
    scale: f64,
    pointer: (f64, f64),
) -> Option<Vec<wl_pointer::Rect>> {
    // winit reports surface coordinates times the scale.
    let s = valid_scale(scale);
    let at = ((pointer.0 / s).floor() as i32, (pointer.1 / s).floor() as i32);
    let in_bounds = surface_bounds(region, scale).is_some_and(|b| b.contains(at));
    let over_exclusion = exclusion_holes(region, scale)
        .iter()
        .any(|hole| hole.contains(at));
    (!(in_bounds && over_exclusion)).then(|| surface_walls(region, size, scale))
}

/// The outer bounds in logical pixels. A held axis is exactly one logical
/// pixel: the one holding the position hint for its physical pixel
/// ([`wl_pointer::point_in_surface`]), even at fractional display scales.
fn surface_bounds(region: &Region, scale: f64) -> Option<wl_pointer::Rect> {
    let mut bounds = wl_pointer::holes_in_surface(&[region.bounds], scale)
        .into_iter()
        .next()?;
    let at = wl_pointer::pixel_in_surface((region.bounds[0], region.bounds[1]), scale);
    if region.bounds[2] == 1 {
        bounds.x = at.0;
        bounds.w = 1;
    }
    if region.bounds[3] == 1 {
        bounds.y = at.1;
        bounds.h = 1;
    }
    Some(bounds)
}

/// Convert exclusions once, then remove the complement of the outer
/// bounds in logical pixels.
fn surface_walls(region: &Region, size: (i32, i32), scale: f64) -> Vec<wl_pointer::Rect> {
    let mut holes = exclusion_holes(region, scale);
    let Some(bounds) = surface_bounds(region, scale) else {
        holes.push(wl_pointer::Rect {
            x: 0,
            y: 0,
            w: size.0,
            h: size.1,
        });
        return holes;
    };
    holes.extend(
        pointer_api::outside(size, [bounds.x, bounds.y, bounds.w, bounds.h])
            .into_iter()
            .map(|[x, y, w, h]| wl_pointer::Rect { x, y, w, h }),
    );
    holes
}

/// The exclusions in logical pixels. A thin exclusion beside the bounds
/// keeps at least one surface pixel and may then cover a held axis's line
/// pixel (a one-pixel column at the window's edge at 1.75, say). It never
/// overlapped the physical bounds, so it is cut back on a side where it
/// lies beyond them.
fn exclusion_holes(region: &Region, scale: f64) -> Vec<wl_pointer::Rect> {
    let holes = wl_pointer::holes_in_surface(&region.exclusions, scale);
    let Some(bounds) = surface_bounds(region, scale) else { return holes };
    let nonempty = region.exclusions.iter().filter(|e| e[2] > 0 && e[3] > 0);
    nonempty
        .zip(holes)
        .filter_map(|(&e, hole)| clear_of_bounds(hole, e, region.bounds, bounds))
        .collect()
}

/// `hole` (surface pixels, converted from physical exclusion `e`) less the
/// surface `bounds` when `e` lies wholly outside the physical bounds: cut on
/// the first side where they are apart. `None` when nothing is left.
fn clear_of_bounds(
    mut hole: wl_pointer::Rect,
    [ex, ey, ew, eh]: pointer_api::Rect,
    [bx, by, bw, bh]: pointer_api::Rect,
    bounds: wl_pointer::Rect,
) -> Option<wl_pointer::Rect> {
    let end = |a: i32, len: i32| a as i64 + len as i64;
    let (hx1, hy1) = (end(hole.x, hole.w), end(hole.y, hole.h));
    if end(ex, ew) <= bx as i64 {
        hole.w = (hx1.min(bounds.x as i64) - hole.x as i64) as i32;
    } else if ex as i64 >= end(bx, bw) {
        let x = (hole.x as i64).max(end(bounds.x, bounds.w));
        hole.w = (hx1 - x) as i32;
        hole.x = x as i32;
    } else if end(ey, eh) <= by as i64 {
        hole.h = (hy1.min(bounds.y as i64) - hole.y as i64) as i32;
    } else if ey as i64 >= end(by, bh) {
        let y = (hole.y as i64).max(end(bounds.y, bounds.h));
        hole.h = (hy1 - y) as i32;
        hole.y = y as i32;
    }
    (hole.w > 0 && hole.h > 0).then_some(hole)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked(holes: &[wl_pointer::Rect], point: Point) -> bool {
        holes.iter().any(|hole| hole.contains(point))
    }

    #[test]
    fn held_axes_use_the_hinted_pixel_at_fractional_display_scales() {
        for scale in [1.0, 1.25, 1.5, 2.0, 2.5, 3.0] {
            let size = surface_size((2400, 1800), scale);
            for bounds in [[1201, 101, 1, 1400], [101, 901, 2000, 1], [1201, 901, 1, 1]] {
                let exclusions = pointer_api::outside((2400, 1800), bounds);
                let holes = surface_walls(&Region { bounds, exclusions }, size, scale);
                let first = wl_pointer::pixel_in_surface((bounds[0], bounds[1]), scale);
                let middle = wl_pointer::pixel_in_surface(
                    (bounds[0] + bounds[2] / 2, bounds[1] + bounds[3] / 2),
                    scale,
                );
                assert!(!blocked(&holes, middle), "{scale}: {bounds:?}: {holes:?}");
                if bounds[2] == 1 {
                    assert_eq!(middle.0, first.0);
                    for dx in -3..=3 {
                        assert_eq!(
                            blocked(&holes, (first.0 + dx, middle.1)),
                            dx != 0,
                            "{scale}: vertical line"
                        );
                    }
                }
                if bounds[3] == 1 {
                    assert_eq!(middle.1, first.1);
                    for dy in -3..=3 {
                        assert_eq!(
                            blocked(&holes, (middle.0, first.1 + dy)),
                            dy != 0,
                            "{scale}: horizontal line"
                        );
                    }
                }
                assert!(blocked(&holes, (0, 0)));
            }
        }
    }

    /// The layout hints the pointer to the line's physical pixel; the
    /// compositor puts it at that pixel's middle in surface coordinates.
    /// That point must be the line's one unblocked surface pixel.
    #[test]
    fn the_position_hint_lies_on_the_held_axis_line() {
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0] {
            let size = surface_size((2400, 1800), scale);
            for v in 0..1750 {
                for bounds in [[v, 50, 1, 1700], [50, v, 2300, 1], [v, v, 1, 1]] {
                    let exclusions = pointer_api::outside((2400, 1800), bounds);
                    let holes = surface_walls(&Region { bounds, exclusions }, size, scale);
                    // A held axis at its pixel; a free one in its middle
                    // (its ends shrink to wholly covered surface pixels).
                    let (hx, hy) = wl_pointer::point_in_surface(
                        (bounds[0] + bounds[2] / 2, bounds[1] + bounds[3] / 2),
                        scale,
                    );
                    let hint = (hx.floor() as i32, hy.floor() as i32);
                    assert!(!blocked(&holes, hint), "{scale}: {bounds:?}: {hint:?}");
                    if bounds[2] == 1 {
                        assert!(hint.0 == 0 || blocked(&holes, (hint.0 - 1, hint.1)), "{scale}: {bounds:?}");
                        assert!(blocked(&holes, (hint.0 + 1, hint.1)), "{scale}: {bounds:?}");
                    }
                    if bounds[3] == 1 {
                        assert!(hint.1 == 0 || blocked(&holes, (hint.0, hint.1 - 1)), "{scale}: {bounds:?}");
                        assert!(blocked(&holes, (hint.0, hint.1 + 1)), "{scale}: {bounds:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_pointer_off_the_line_is_still_confined() {
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0] {
            let size = surface_size((2400, 1800), scale);
            let bounds = [100, 901, 2000, 1];
            let mut exclusions = pointer_api::outside((2400, 1800), bounds);
            // A divider elsewhere in the window.
            exclusions.push([1200, 0, 4, 600]);
            let region = Region { bounds, exclusions };
            let walls = surface_walls(&region, size, scale);
            for pointer in [(1000.0, 896.0), (1000.0, 906.5), (5.0, 5.0), (1201.0, 300.0)] {
                assert_eq!(
                    confinement(&region, size, scale, pointer).as_ref(),
                    Some(&walls),
                    "{scale}: {pointer:?}"
                );
            }
            // On the line itself.
            let (_, y) = wl_pointer::point_in_surface((1000, 901), scale);
            assert!(confinement(&region, size, scale, (1000.0, y * scale)).is_some());
        }
    }

    #[test]
    fn an_exclusion_over_the_pointer_inside_the_bounds_leaves_it_free() {
        let region = Region {
            bounds: [0, 0, 2400, 1800],
            exclusions: vec![[300, 1200, 600, 300]],
        };
        let size = surface_size((2400, 1800), 1.5);
        assert_eq!(confinement(&region, size, 1.5, (600.0, 1300.0)), None);
        assert!(confinement(&region, size, 1.5, (100.0, 100.0)).is_some());
        // A divider crossing a vertical line under the pointer.
        let region = Region {
            bounds: [1201, 0, 1, 1800],
            exclusions: vec![[0, 600, 2400, 2]],
        };
        assert_eq!(confinement(&region, (2400, 1800), 1.0, (1201.5, 600.5)), None);
        assert!(confinement(&region, (2400, 1800), 1.0, (1201.5, 900.0)).is_some());
    }

    #[test]
    fn physical_exclusions_are_converted_once_and_keep_thin_walls() {
        let region = Region {
            bounds: [0, 0, 2400, 1800],
            exclusions: vec![[1201, 0, 1, 900], [300, 1200, 600, 300]],
        };
        let holes = surface_walls(&region, surface_size((2400, 1800), 1.5), 1.5);
        assert_eq!(
            holes,
            vec![
                wl_pointer::Rect {
                    x: 801,
                    y: 0,
                    w: 1,
                    h: 600
                },
                wl_pointer::Rect {
                    x: 200,
                    y: 800,
                    w: 400,
                    h: 200
                },
            ]
        );
        assert!(blocked(&holes, (801, 599)));
        assert!(!blocked(&holes, (801, 600)));
        assert!(!blocked(&holes, (800, 599)));
        assert!(blocked(&holes, (599, 999)));
        assert!(!blocked(&holes, (600, 999)));
    }

    #[test]
    fn outer_bounds_complement_is_made_after_display_conversion() {
        let region = Region {
            bounds: [601, 301, 1200, 900],
            exclusions: vec![],
        };
        let size = surface_size((2400, 1800), 1.5);
        let holes = surface_walls(&region, size, 1.5);
        // Only wholly covered logical pixels remain, matching the previous
        // Wayland region conversion for physical window bounds.
        for x in 0..size.0 {
            for y in 0..size.1 {
                assert_eq!(
                    blocked(&holes, (x, y)),
                    !(401..1200).contains(&x) || !(201..800).contains(&y)
                );
            }
        }
    }

    #[test]
    fn empty_bounds_exclude_the_entire_window() {
        for bounds in [[0, 0, 0, 100], [0, 0, 100, 0], [0, 0, -1, 100]] {
            let holes = surface_walls(
                &Region {
                    bounds,
                    exclusions: vec![],
                },
                (100, 100),
                1.0,
            );
            assert!(blocked(&holes, (0, 0)));
            assert!(blocked(&holes, (99, 99)));
        }
    }

    #[test]
    fn placement_keeps_physical_target_until_completion_or_release_cancellation() {
        let mut pending = Some((1201, 901));
        // A fractional logical hint is only evidence of completion. The
        // reported point remains the original physical window coordinate.
        assert_eq!(
            placement(&mut pending, wl_pointer::MoveResult::Hinted((480.6, 360.6))),
            Some(Placement::Hinted((1201, 901)))
        );
        assert_eq!(pending, None);
        pending = Some((77, 99));
        // Constraints::release leaves Cancelled available to update, so
        // grab must retain the target instead of clearing it early.
        assert_eq!(
            placement(&mut pending, wl_pointer::MoveResult::Cancelled),
            Some(Placement::Cancelled)
        );
        assert_eq!(pending, None);
        assert_eq!(
            placement(&mut pending, wl_pointer::MoveResult::Cancelled),
            None
        );
    }

    #[test]
    fn window_size_rounds_outwards_and_invalid_scales_use_one() {
        assert_eq!(surface_size((1921, 1081), 1.5), (1281, 721));
        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(surface_size((1921, 1081), scale), (1921, 1081));
        }
    }
}
