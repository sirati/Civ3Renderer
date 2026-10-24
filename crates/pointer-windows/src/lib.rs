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
    use winit::window::{CursorGrabMode, Window};

    /// The system's clip, through `inproc`.
    struct System;

    impl super::SystemClip for System {
        fn current(&self) -> Option<[i32; 4]> { inproc::pointer::clip_rect() }
        fn set(&mut self, screen: Option<[i32; 4]>) -> bool { inproc::pointer::clip_cursor(screen) }
    }

    pub struct Controller {
        clip: super::Confinement,
        fallback: pointer_fallback::Controller,
        window: Arc<Window>,
    }

    impl Controller {
        pub fn new(window: Arc<Window>) -> Self {
            Self {
                clip: super::Confinement::default(),
                fallback: pointer_fallback::Controller::new(window.clone()),
                window,
            }
        }
    }

    impl Backend for Controller {
        fn grab(&mut self, on: bool) -> Result<Grab, Error> {
            self.clip.release(&mut System);
            self.fallback.grab(on)
        }

        fn move_pointer(&mut self, at: Point) -> Result<Move, Error> {
            // A locked grab is a one-pixel clip at the pointer, which
            // SetCursorPos cannot leave: lock again where it is put.
            if !self.fallback.grabbed() {
                return self.fallback.move_pointer(at);
            }
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
            let moved = self.fallback.move_pointer(at);
            self.fallback.grab(true)?;
            moved
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
            let clipped = self.clip.set(&mut System, screen);
            // ClipCursor cannot represent exclusions inside the bounds.
            // Keep the layout's warps active for those holes.
            Update {
                confined: clipped && region.is_some_and(super::one_rectangle),
                placement: None,
            }
        }
    }

    impl Drop for Controller {
        // Release the custom clip before dropping the window grab.
        fn drop(&mut self) { self.clip.release(&mut System); }
    }

    /// The operating system's nominal cursor size at 96 DPI, in pixels:
    /// the accessibility pointer size, else the system metric.
    pub fn nominal_cursor_size() -> Option<u32> {
        inproc::pointer::cursor_base_size().or_else(inproc::pointer::cursor_metric_size)
    }
}

#[cfg(windows)]
pub use windows::{nominal_cursor_size, Controller};

/// The system's one pointer clip, in screen coordinates `[left, top,
/// right, bottom]`.
#[cfg(any(windows, test))]
trait SystemClip {
    /// The clip in effect (the desktop's bounds while there is none).
    fn current(&self) -> Option<[i32; 4]>;
    /// Clips to `screen`, or releases with `None`; whether it took.
    fn set(&mut self, screen: Option<[i32; 4]>) -> bool;
}

/// This window's pointer clip. The clip is the whole system's: anyone may
/// replace or release it (the window library on every cursor change, the
/// game, the system on activation), so each [`Confinement::set`] checks it
/// is still in effect and installs it again if not. The system cuts it to
/// the desktop: what it shows once installed is what is compared, never
/// the request (else a window partly off the screen would clip again every
/// frame, and its clip never be released). Releasing removes only a clip
/// still ours.
#[cfg(any(windows, test))]
#[derive(Default)]
struct Confinement {
    requested: Option<[i32; 4]>,
    installed: Option<[i32; 4]>,
}

#[cfg(any(windows, test))]
impl Confinement {
    /// Clips to a nonempty `screen`, or releases with `None`. Whether a
    /// requested clip is in effect.
    fn set(&mut self, system: &mut impl SystemClip, screen: Option<[i32; 4]>) -> bool {
        let Some(screen) = screen.filter(|r| r[2] > r[0] && r[3] > r[1]) else {
            self.release(system);
            return false;
        };
        if self.requested == Some(screen) && self.installed.is_some() && system.current() == self.installed {
            return true;
        }
        if !system.set(Some(screen)) {
            (self.requested, self.installed) = (None, None);
            return false;
        }
        (self.requested, self.installed) = (Some(screen), system.current());
        true
    }

    /// Releases the clip if it is still the one installed here.
    fn release(&mut self, system: &mut impl SystemClip) {
        self.requested = None;
        if let Some(installed) = self.installed.take() {
            if system.current() == Some(installed) {
                system.set(None);
            }
        }
    }
}

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
    use super::{one_rectangle, screen_bounds, Confinement, SystemClip};
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

    /// The system's clip on a 1920×1080 desktop, cut to it as Windows
    /// does; counts the calls that move it (each one a mouse move).
    struct Desktop { clip: Option<[i32; 4]>, calls: usize }

    const DESKTOP: [i32; 4] = [0, 0, 1920, 1080];

    impl SystemClip for Desktop {
        fn current(&self) -> Option<[i32; 4]> { Some(self.clip.unwrap_or(DESKTOP)) }
        fn set(&mut self, screen: Option<[i32; 4]>) -> bool {
            self.calls += 1;
            self.clip = screen.map(|[l, t, r, b]| [l.max(0), t.max(0), r.min(1920), b.min(1080)]);
            true
        }
    }

    #[test]
    fn a_clip_released_by_someone_else_is_installed_again() {
        let mut system = Desktop { clip: None, calls: 0 };
        let mut clip = Confinement::default();
        let view = Some([100, 200, 900, 800]);
        assert!(clip.set(&mut system, view));
        assert!(clip.set(&mut system, view));
        assert_eq!(system.calls, 1, "unchanged: the system is left alone");
        // winit on a cursor change, or the game, releases it.
        system.clip = None;
        assert!(clip.set(&mut system, view));
        assert_eq!((system.clip, system.calls), (view, 2));
        // Another rectangle replaces it: ours again.
        system.clip = Some([0, 0, 10, 10]);
        assert!(clip.set(&mut system, view));
        assert_eq!(system.clip, view);
        // The window moved: the new place.
        assert!(clip.set(&mut system, Some([150, 230, 950, 830])));
        assert_eq!(system.clip, Some([150, 230, 950, 830]));
    }

    #[test]
    fn a_clip_cut_to_the_desktop_is_kept_and_released() {
        let mut system = Desktop { clip: None, calls: 0 };
        let mut clip = Confinement::default();
        // A windowed view partly off the left and bottom of the screen.
        let view = Some([-300, 600, 500, 1400]);
        assert!(clip.set(&mut system, view));
        assert_eq!(system.clip, Some([0, 600, 500, 1080]));
        for _ in 0..3 { assert!(clip.set(&mut system, view)); }
        assert_eq!(system.calls, 1, "no clip again each frame");
        clip.set(&mut system, None);
        assert_eq!(system.clip, None, "released though the system cut it");
    }

    #[test]
    fn release_leaves_a_clip_that_is_not_ours() {
        let mut system = Desktop { clip: None, calls: 0 };
        let mut clip = Confinement::default();
        assert!(clip.set(&mut system, Some([100, 100, 200, 200])));
        // A grab's one-pixel lock replaced it.
        system.clip = Some([150, 150, 151, 151]);
        clip.release(&mut system);
        assert_eq!(system.clip, Some([150, 150, 151, 151]));
        // Empty requests release too, and claim nothing.
        assert!(clip.set(&mut system, Some([100, 100, 200, 200])));
        assert!(!clip.set(&mut system, Some([100, 100, 100, 200])));
        assert_eq!(system.clip, None);
    }
}
