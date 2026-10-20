//! Windows has one rectangular pointer clip (`ClipCursor`). A region whose
//! exclusions all lie outside its bounds (a captured-edge line or area,
//! whose complement the layout also lists as exclusions) is exactly that
//! clip: the system holds the pointer there, moving it inside at once, and
//! the region is reported as confined. Holes within the rectangle (a
//! divider or zoom box crossing it) cannot be clipped; such a region keeps
//! the clip as an outer limit and the layout's pointer warps for the holes.

#![forbid(unsafe_code)]

#[cfg(windows)]
mod windows {
    use std::sync::Arc;

    use pointer_api::{Backend, Error, Grab, Move, Point, Region, Update};
    use winit::window::Window;

    pub struct Controller {
        // Release the custom clip before dropping the window grab.
        clip: inproc::pointer::Confinement,
        fallback: pointer_fallback::Controller,
        window: Arc<Window>,
    }

    impl Controller {
        pub fn new(window: Arc<Window>) -> Self {
            Self {
                clip: inproc::pointer::Confinement::default(),
                fallback: pointer_fallback::Controller::new(window.clone()),
                window,
            }
        }
    }

    impl Backend for Controller {
        fn grab(&mut self, on: bool) -> Result<Grab, Error> {
            self.clip.release();
            self.fallback.grab(on)
        }

        fn move_pointer(&mut self, at: Point) -> Result<Move, Error> {
            self.fallback.move_pointer(at)
        }

        fn update(
            &mut self,
            region: Option<&Region>,
            _pointer: (f64, f64),
            inside: bool,
        ) -> Update {
            let region = region.filter(|_| inside && !self.fallback.grabbed());
            let screen = region.and_then(|region| {
                let origin = self.window.inner_position().ok()?;
                super::screen_bounds(region.bounds, (origin.x, origin.y))
            });
            let clipped = self.clip.set(screen);
            // ClipCursor cannot represent exclusions inside the bounds.
            // Keep the layout's warps active for those holes.
            Update {
                confined: clipped && region.is_some_and(super::one_rectangle),
                placement: None,
            }
        }
    }

    /// The operating system's nominal cursor size at 96 DPI, in pixels:
    /// the accessibility pointer size, else the system metric.
    pub fn nominal_cursor_size() -> Option<u32> {
        inproc::pointer::cursor_base_size().or_else(inproc::pointer::cursor_metric_size)
    }
}

#[cfg(windows)]
pub use windows::{nominal_cursor_size, Controller};

/// Whether the region is just its bounds: every exclusion lies wholly
/// outside them, so one clip rectangle holds the pointer exactly.
#[cfg(any(windows, test))]
fn one_rectangle(region: &pointer_api::Region) -> bool {
    let [x, y, w, h] = region.bounds;
    let (x1, y1) = (x as i64 + w as i64, y as i64 + h as i64);
    region.exclusions.iter().all(|&[ex, ey, ew, eh]| {
        let (ex1, ey1) = (ex as i64 + ew as i64, ey as i64 + eh as i64);
        ew <= 0 || eh <= 0 || ex1 <= x as i64 || ey1 <= y as i64 || ex as i64 >= x1 || ey as i64 >= y1
    })
}

/// Physical content pixels to Win32's exclusive screen-coordinate RECT.
/// Reject unrepresentable rectangles rather than wrapping onto another
/// display or widening a one-pixel locked axis.
#[cfg(any(windows, test))]
fn screen_bounds([x, y, w, h]: pointer_api::Rect, origin: pointer_api::Point) -> Option<[i32; 4]> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let left = origin.0.checked_add(x)?;
    let top = origin.1.checked_add(y)?;
    let right = left.checked_add(w)?;
    let bottom = top.checked_add(h)?;
    Some([left, top, right, bottom])
}

#[cfg(test)]
mod tests {
    use super::{one_rectangle, screen_bounds};
    use pointer_api::{outside, Region};

    fn edge(bounds: pointer_api::Rect, mut extra: Vec<pointer_api::Rect>) -> Region {
        let mut exclusions = outside((800, 600), bounds);
        exclusions.append(&mut extra);
        Region { bounds, exclusions }
    }

    #[test]
    fn captured_edge_lines_and_areas_are_one_clip_rectangle() {
        // Horizontal-only, vertical-only, corner area, both free.
        for bounds in [[100, 300, 600, 1], [400, 50, 1, 500], [400, 300, 400, 300], [0, 0, 800, 600]] {
            assert!(one_rectangle(&edge(bounds, vec![])), "{bounds:?}");
        }
        // A divider or zoom wall beside the bounds, and an empty one.
        assert!(one_rectangle(&edge([100, 300, 600, 1], vec![[400, 0, 2, 300], [0, 301, 800, 90]])));
        assert!(one_rectangle(&edge([400, 300, 400, 300], vec![[500, 400, 0, 50]])));
    }

    #[test]
    fn exclusions_crossing_the_bounds_keep_the_warp_fallback() {
        // A divider across the whole window, crossing every shape.
        let divider = [399, 0, 2, 600];
        for bounds in [[100, 300, 600, 1], [400, 50, 1, 500], [300, 300, 500, 300], [0, 0, 800, 600]] {
            assert!(!one_rectangle(&edge(bounds, vec![divider])), "{bounds:?}");
        }
        // A zoom box overlapping a corner of the area by one pixel.
        assert!(!one_rectangle(&edge([400, 300, 400, 300], vec![[300, 200, 101, 101]])));
    }

    #[test]
    fn maps_physical_content_to_a_negative_screen_origin() {
        assert_eq!(
            screen_bounds([32, 48, 800, 600], (-1920, -80)),
            Some([-1888, -32, -1088, 568])
        );
    }

    #[test]
    fn retains_single_pixel_axes() {
        assert_eq!(
            screen_bounds([400, 20, 1, 600], (-1920, 100)),
            Some([-1520, 120, -1519, 720])
        );
        assert_eq!(
            screen_bounds([20, 400, 600, 1], (100, -1080)),
            Some([120, -680, 720, -679])
        );
    }

    #[test]
    fn rejects_overflow_in_origin_and_exclusive_edges() {
        assert_eq!(screen_bounds([1, 0, 1, 1], (i32::MAX, 0)), None);
        assert_eq!(screen_bounds([0, 0, 1, 1], (i32::MAX, 0)), None);
        assert_eq!(screen_bounds([0, 1, 1, 1], (0, i32::MAX)), None);
        assert_eq!(screen_bounds([0, 0, 1, 1], (0, i32::MAX)), None);
        assert_eq!(screen_bounds([-1, 0, 1, 1], (i32::MIN, 0)), None);
        assert_eq!(screen_bounds([0, -1, 1, 1], (0, i32::MIN)), None);
    }

    #[test]
    fn accepts_edges_at_integer_limits_and_offsets_that_cancel() {
        assert_eq!(
            screen_bounds([i32::MAX, i32::MAX, 1, 1], (-1, -1)),
            Some([i32::MAX - 1, i32::MAX - 1, i32::MAX, i32::MAX])
        );
        assert_eq!(
            screen_bounds([0, 0, 1, 1], (i32::MIN, i32::MIN)),
            Some([i32::MIN, i32::MIN, i32::MIN + 1, i32::MIN + 1])
        );
        assert_eq!(
            screen_bounds([i32::MAX, i32::MAX, 10, 10], (i32::MIN, i32::MIN)),
            Some([-1, -1, 9, 9])
        );
    }

    #[test]
    fn rejects_empty_and_inverted_regions() {
        for bounds in [[0, 0, 0, 20], [0, 0, 20, 0], [0, 0, -1, 20], [0, 0, 20, -1]] {
            assert_eq!(screen_bounds(bounds, (0, 0)), None);
        }
    }
}
