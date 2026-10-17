//! Wayland pointer constraints for a winit window, behind a safe API.
//!
//! Wayland lets no client move the system pointer. What a client may do is
//! constrain it (`zwp_pointer_constraints_v1`): confine it to a region of
//! its surface, or lock it in place and, while locked, hint where it is to
//! show when it is let go. [`Constraints`] does both for one window:
//!
//! - [`Constraints::confine`] keeps the pointer inside a region of the
//!   window (the window less some holes), [`Constraints::unconfine`] lets it
//!   go;
//! - [`Constraints::move_pointer`] requests a move: the pointer is locked,
//!   the hint set, and the lock let go once the compositor says it holds.
//!   The compositor may move the pointer to the hint. Take the result with
//!   [`Constraints::take_move_result`]; a committed hint does not guarantee
//!   that the compositor honored it.
//!
//! It talks over winit's own connection (constraints must be made on the
//! connection that owns the surface), on an event queue of its own, with a
//! `wl_pointer` of its own for the window's seat. winit makes constraints
//! too (`Window::set_cursor_grab`); a surface takes only one per seat, so
//! the caller drops this crate's ([`Constraints::release`]) before grabbing
//! through winit.
//!
//! Coordinates are the surface's (logical pixels); [`holes_in_surface`]
//! turns window pixels into them.
//!
//! # Compositor verification
//!
//! Sway with wlroots holds the pointer at the divider's closed stretches
//! and allows it through the gaps. Region changes apply with a surface
//! commit. The seat's pointer is recreated after pointer capability returns,
//! for example when a virtual input device reconnects.
//!
//! GNOME with Mutter has not been verified. The protocol lets a compositor
//! ignore a position hint; a move also gives up after 300 ms if the lock
//! does not activate. Fractional coordinate conversion has unit tests but
//! still needs compositor verification. In a window with client decorations,
//! confinement to the content surface can prevent reaching its title bar
//! until keyboard focus changes.

/// The latest completed pointer move request.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum MoveResult {
    /// The lock activated, this position hint was committed in surface
    /// coordinates, and the lock was released. The compositor may ignore
    /// the hint, so this does not confirm the pointer's physical position.
    Hinted((f64, f64)),
    /// The request was rejected, timed out, or was canceled before its
    /// normal lock-and-hint completion.
    Cancelled,
}

/// A rectangle in surface coordinates (logical pixels).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, (x, y): (i32, i32)) -> bool {
        x >= self.x && y >= self.y && (x as i64) < self.x as i64 + self.w as i64 && (y as i64) < self.y as i64 + self.h as i64
    }
}

/// Holes given in window pixels (`[x, y, w, h]`, physical) in surface
/// coordinates at `scale` window pixels per surface pixel: each shrunk to
/// the surface pixels it covers wholly, but never to nothing (at least one
/// surface pixel each way where it was at least one window pixel), so that
/// a thin wall at a fractional scale still holds the pointer. The pointer
/// pushed against a hole stops at most a window pixel short of it, or
/// within less than a surface pixel of it inside.
pub fn holes_in_surface(holes: &[[i32; 4]], scale: f64) -> Vec<Rect> {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let span = |from: i32, len: i32| {
        let a = (from as f64 / scale).ceil() as i32;
        let b = ((from as i64 + len as i64) as f64 / scale).floor() as i32;
        (a, (b as i64 - a as i64).clamp(1, i32::MAX as i64) as i32)
    };
    holes
        .iter()
        .filter(|h| h[2] > 0 && h[3] > 0)
        .map(|&[x, y, w, h]| {
            let ((x, w), (y, h)) = (span(x, w), span(y, h));
            Rect { x, y, w, h }
        })
        .collect()
}

/// Window pixel `p` (physical) in surface coordinates at `scale`, as the
/// compositor puts a pointer hinted there: the middle of that pixel.
pub fn point_in_surface((x, y): (i32, i32), scale: f64) -> (f64, f64) {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    ((x as f64 + 0.5) / scale, (y as f64 + 0.5) / scale)
}

/// The surface pixel window pixel `p` lies in.
pub fn pixel_in_surface((x, y): (i32, i32), scale: f64) -> (i32, i32) {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    ((x as f64 / scale).floor() as i32, (y as f64 / scale).floor() as i32)
}

/// Why there are no pointer constraints for a window.
#[derive(Debug)]
pub enum Unavailable {
    /// The window is not a Wayland one (X11, Windows, ...).
    NotWayland,
    /// The compositor has no such protocol, or no seat.
    Missing(&'static str),
    /// Talking to the compositor failed.
    Failed(String),
}

impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unavailable::NotWayland => write!(f, "not a Wayland window"),
            Unavailable::Missing(what) => write!(f, "the compositor has no {what}"),
            Unavailable::Failed(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Unavailable {}

#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "linux")]
pub use wayland::Constraints;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes_at_scale_one_are_the_window_pixels() {
        assert_eq!(holes_in_surface(&[[400, 0, 4, 300]], 1.0), vec![Rect { x: 400, y: 0, w: 4, h: 300 }]);
    }

    #[test]
    fn holes_shrink_to_the_surface_pixels_they_cover() {
        // 401..405 at 2: surface pixels 201 (402..404) only.
        assert_eq!(holes_in_surface(&[[401, 10, 4, 301]], 2.0), vec![Rect { x: 201, y: 5, w: 1, h: 150 }]);
    }

    #[test]
    fn a_thin_hole_keeps_a_surface_pixel() {
        // 400..404 at 3 covers no surface pixel wholly (133.3..134.7).
        assert_eq!(holes_in_surface(&[[400, 0, 4, 9]], 3.0), vec![Rect { x: 134, y: 0, w: 1, h: 3 }]);
        assert_eq!(holes_in_surface(&[[400, 0, 0, 9]], 3.0), vec![]);
    }

    #[test]
    fn rectangles_keep_wide_coordinates_without_overflow() {
        let r = Rect { x: i32::MAX - 1, y: 0, w: 10, h: 1 };
        assert!(r.contains((i32::MAX, 0)));
        assert!(!r.contains((i32::MIN, 0)));
        assert_eq!(holes_in_surface(&[[i32::MAX - 1, 0, 10, 1]], 2.0), vec![Rect { x: 1_073_741_823, y: 0, w: 5, h: 1 }]);
    }

    #[test]
    fn points_go_to_their_pixels_middle_and_back() {
        assert_eq!(point_in_surface((10, 20), 1.0), (10.5, 20.5));
        assert_eq!(point_in_surface((10, 20), 2.0), (5.25, 10.25));
        assert_eq!(pixel_in_surface((11, 20), 2.0), (5, 10));
        assert_eq!(pixel_in_surface((-1, 0), 2.0), (-1, 0));
        // A nonsensical scale is taken as 1.
        assert_eq!(pixel_in_surface((7, 8), 0.0), (7, 8));
    }
}
