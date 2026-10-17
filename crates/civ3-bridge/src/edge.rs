//! Scrolling the game's map while the viewer's pointer rests at its
//! window's edge (`scene::Input::Edge`), as the game does at its own
//! screen's edge: the game polls the pointer and the keyboard focus
//! (`civ3_sys::hooks::EDGE_SCROLL_ZONE`), so the bridge has those report the
//! pointer at the game's edge meanwhile (`inproc::pointer`).

use civ3_sys::hooks::EDGE_SCROLL_ZONE;

/// Where the game's pointer is to be (a client pixel of its window of
/// `size`) for the viewer's pointer at the window edge `edge` (as in
/// `Input::Edge`): as far from the game's edge as the viewer's is from its
/// own, in the middle along an axis at no edge; `None` where the game does
/// not scroll.
pub fn game_point(edge: (i32, i32), size: (u32, u32)) -> Option<(i32, i32)> {
    let axis = |e: i32, size: u32| -> Option<i32> {
        let size = size as i32;
        let from = e.abs() - 1;
        match e {
            _ if size <= 0 || from >= EDGE_SCROLL_ZONE.min(size) => None,
            0 => None,
            e if e < 0 => Some(from),
            _ => Some(size - 1 - from),
        }
    };
    match (axis(edge.0, size.0), axis(edge.1, size.1)) {
        (None, None) => None,
        (x, y) => Some((x.unwrap_or(size.0 as i32 / 2), y.unwrap_or(size.1 as i32 / 2))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_pointer_keeps_the_viewers_distance_from_the_edge() {
        let size = (1920, 1080);
        assert_eq!(game_point((0, 0), size), None);
        // The outermost pixels, each edge.
        assert_eq!(game_point((-1, 0), size), Some((0, 540)));
        assert_eq!(game_point((1, 0), size), Some((1919, 540)));
        assert_eq!(game_point((0, -1), size), Some((960, 0)));
        assert_eq!(game_point((0, 1), size), Some((960, 1079)));
        // Further in; a corner.
        assert_eq!(game_point((4, 0), size), Some((1916, 540)));
        assert_eq!(game_point((-16, 16), size), Some((15, 1064)));
        // Beyond the game's zone (the viewer's may be wider): no scrolling
        // along that axis.
        assert_eq!(game_point((17, 0), size), None);
        assert_eq!(game_point((17, -2), size), Some((960, 1)));
        assert_eq!(game_point((1, 0), (0, 0)), None);
    }
}
