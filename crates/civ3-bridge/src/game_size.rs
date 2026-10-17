//! The game's resolution: the display's divided by the desktop's scale (the
//! game at the desktop's logical size; its interface is drawn at its own
//! pixel size times the viewer's pixel scale), chosen when the game starts
//! (it takes its screen size then, `KeepRes=1`), no smaller than the game
//! takes, and one of the display's modes.

/// The smallest screen the game runs at.
pub const MIN: (u32, u32) = (1024, 768);

/// The game's screen on a display of `display` pixels scaled `n` times:
/// the display `n` times smaller, but no smaller than [`MIN`] (`n` lowered
/// as far as needed, keeping the display's shape) and no larger than the
/// display; when smaller, in whole multiples of 8 pixels each way (the
/// game fails at sizes such as 1365×768).
pub fn for_display(display: (u32, u32), n: f64) -> (u32, u32) {
    let (w, h) = (display.0.max(1) as f64, display.1.max(1) as f64);
    let most = (w / MIN.0 as f64).min(h / MIN.1 as f64).max(1.0);
    let n = if n.is_finite() { n.clamp(1.0, most) } else { 1.0 };
    if n == 1.0 {
        return (display.0.max(1), display.1.max(1));
    }
    let side = |v: f64, least: u32| ((v / n).round() as u32 / 8 * 8).max(least);
    (side(w, MIN.0), side(h, MIN.1))
}

/// [`for_display`] where the display offers `modes`: the game switches the
/// display to the size it takes (Wine emulates the modes it lists) and
/// fails at one there is no mode of, so the mode nearest that size, about
/// the display's shape and no smaller than [`MIN`]; [`for_display`] if
/// none is (or no modes are known).
pub fn for_display_modes(display: (u32, u32), n: f64, modes: &[(u32, u32)]) -> (u32, u32) {
    let want = for_display(display, n);
    if want == (display.0.max(1), display.1.max(1)) {
        return want;
    }
    let shape = display.0.max(1) as f64 / display.1.max(1) as f64;
    // (Wanted before the game's least: the display divided as asked.)
    let n = if n.is_finite() { n.max(1.0) } else { 1.0 };
    let area = display.0 as f64 / n * (display.1 as f64 / n);
    let fits = |&&(w, h): &&(u32, u32)| w >= MIN.0 && h >= MIN.1 && w <= display.0 && h <= display.1 && ((w as f64 / h as f64) / shape - 1.0).abs() <= 0.12;
    let off = |&(w, h): &(u32, u32)| ((w as f64 * h as f64) / area).ln().abs();
    modes.iter().filter(fits).min_by(|a, b| off(a).total_cmp(&off(b))).copied().unwrap_or(want)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_display_divided_but_never_below_the_games_least() {
        assert_eq!(for_display((3840, 2160), 2.0), (1920, 1080));
        assert_eq!(for_display((3840, 2160), 1.5), (2560, 1440));
        assert_eq!(for_display((3840, 2160), 1.0), (3840, 2160));
        // ×3 of 2160 would be 720 high: as small as the game goes, about
        // the same shape, in multiples of 8.
        assert_eq!(for_display((3840, 2160), 3.0), (1360, 768));
        assert_eq!(for_display((1920, 1080), 2.0), (1360, 768));
        assert_eq!(for_display((2880, 1620), 1.5), (1920, 1080));
        // A display smaller than that: as it is.
        assert_eq!(for_display((800, 600), 2.0), (800, 600));
        assert_eq!(for_display((1920, 1080), f64::NAN), (1920, 1080));
    }

    #[test]
    fn the_display_mode_nearest_what_the_pixel_scale_asks() {
        // Wine's modes on a 3840×2160 display (some of them).
        let modes = [(3840, 2160), (640, 480), (1024, 768), (1280, 720), (1280, 768), (1600, 900), (1920, 1080), (2560, 1440), (2880, 1620), (1280, 1024)];
        assert_eq!(for_display_modes((3840, 2160), 2.0, &modes), (1920, 1080));
        assert_eq!(for_display_modes((3840, 2160), 1.5, &modes), (2560, 1440));
        assert_eq!(for_display_modes((3840, 2160), 1.0, &modes), (3840, 2160));
        // ×3 (1280×720 is below the game's least): 1280×768.
        assert_eq!(for_display_modes((3840, 2160), 3.0, &modes), (1280, 768));
        assert_eq!(for_display_modes((3840, 2160), 6.0, &modes), (1280, 768));
        // No modes known: the size itself.
        assert_eq!(for_display_modes((3840, 2160), 3.0, &[]), (1360, 768));
    }
}
