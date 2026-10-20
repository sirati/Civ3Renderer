//! The operating system's nominal cursor size in physical window pixels.
//!
//! - Linux: `XCURSOR_SIZE`, else the desktop's setting (KDE
//!   `kcminputrc` `[Mouse] cursorSize`, GNOME `gsettings get
//!   org.gnome.desktop.interface cursor-size`, looked up once), else 24.
//!   Wayland sizes are logical and multiplied by the window's scale; X11
//!   sizes are already physical.
//! - Windows: the accessibility pointer size (`CursorBaseSize`), else
//!   `SM_CXCURSOR` at 96 DPI, else 32, times the window's scale.
//! - macOS: 32 points times the accessibility cursor size (1 to 4) times
//!   the backing scale.
//!
//! The parsing and arithmetic are pure functions below, tested on any host.

use winit::window::Window;

/// Sizes beyond this are taken as misconfiguration and ignored.
const MAX_SIZE: u32 = 1024;
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
const LINUX_DEFAULT: u32 = 24;
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
const WINDOWS_DEFAULT: u32 = 32;
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
const MACOS_POINTS: f64 = 32.0;

/// The operating system's nominal cursor size for `window` in physical
/// pixels; never 0.
pub fn system_cursor_size(window: &Window) -> u32 {
    platform(window).max(1)
}

#[cfg(target_os = "linux")]
fn platform(window: &Window) -> u32 {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let env = std::env::var("XCURSOR_SIZE").ok();
    let size = linux_size(env.as_deref(), desktop_size());
    let wayland = window
        .window_handle()
        .is_ok_and(|h| matches!(h.as_raw(), RawWindowHandle::Wayland(_)));
    if wayland { scaled(size as f64, window.scale_factor()) } else { size }
}

#[cfg(windows)]
fn platform(window: &Window) -> u32 {
    windows_size(pointer_windows::nominal_cursor_size(), window.scale_factor())
}

#[cfg(target_os = "macos")]
fn platform(window: &Window) -> u32 {
    macos_size(winit::platform::macos::accessibility_cursor_size(), window.scale_factor())
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
fn platform(window: &Window) -> u32 {
    scaled(LINUX_DEFAULT as f64, window.scale_factor())
}

/// The desktop's cursor size setting, read once per process.
#[cfg(target_os = "linux")]
fn desktop_size() -> Option<u32> {
    static SIZE: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
    *SIZE.get_or_init(|| {
        let kde = || {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .filter(|v| !v.is_empty())
                .map(std::path::PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
            parse_kcminputrc(&std::fs::read_to_string(config.join("kcminputrc")).ok()?)
        };
        let gnome = || {
            let out = std::process::Command::new("gsettings")
                .args(["get", "org.gnome.desktop.interface", "cursor-size"])
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .ok()?;
            out.status.success().then_some(())?;
            parse_gsettings(&String::from_utf8_lossy(&out.stdout))
        };
        let on_kde = std::env::var("XDG_CURRENT_DESKTOP")
            .is_ok_and(|d| d.split(':').any(|d| d.eq_ignore_ascii_case("KDE")));
        let size = if on_kde { kde().or_else(gnome) } else { gnome().or_else(kde) };
        log::debug!("pointer: desktop cursor size setting {size:?}");
        size
    })
}

/// A positive cursor size in pixels.
fn parse_size(text: &str) -> Option<u32> {
    text.trim().parse::<u32>().ok().filter(|&s| (1..=MAX_SIZE).contains(&s))
}

/// `cursorSize` in the `[Mouse]` group of KDE's `kcminputrc`.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn parse_kcminputrc(text: &str) -> Option<u32> {
    let mut mouse = false;
    let mut size = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            mouse = line == "[Mouse]";
        } else if let Some((key, value)) = line.split_once('=').filter(|_| mouse) {
            // Keys may carry flags such as `cursorSize[$e]`.
            let key = key.trim();
            let key = key.split_once('[').map_or(key, |(k, _)| k);
            if key == "cursorSize" {
                size = parse_size(value);
            }
        }
    }
    size
}

/// `gsettings get` output for a uint: `uint32 48`, or just `48`.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn parse_gsettings(text: &str) -> Option<u32> {
    let text = text.trim();
    parse_size(text.rsplit_once(' ').map_or(text, |(_, v)| v))
}

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn linux_size(env: Option<&str>, desktop: Option<u32>) -> u32 {
    env.and_then(parse_size).or(desktop).unwrap_or(LINUX_DEFAULT)
}

/// `size` logical pixels at `scale`, rounded; at least one pixel. An
/// invalid scale counts as 1.
fn scaled(size: f64, scale: f64) -> u32 {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    (size * scale).round().clamp(1.0, u32::MAX as f64) as u32
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn windows_size(nominal: Option<u32>, scale: f64) -> u32 {
    let size = nominal.filter(|&s| (1..=MAX_SIZE).contains(&s)).unwrap_or(WINDOWS_DEFAULT);
    scaled(size as f64, scale)
}

/// macOS's accessibility factor is 1 (normal) to 4; an unset preference
/// reads as 0.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn macos_size(factor: f64, scale: f64) -> u32 {
    let factor = if factor.is_finite() { factor.clamp(1.0, 4.0) } else { 1.0 };
    scaled(MACOS_POINTS * factor, scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xcursor_size_wins_when_valid() {
        assert_eq!(linux_size(Some("48"), Some(32)), 48);
        assert_eq!(linux_size(Some(" 36\n"), None), 36);
        for bad in ["", "0", "-24", "big", "24px", "99999"] {
            assert_eq!(linux_size(Some(bad), Some(32)), 32, "{bad:?}");
            assert_eq!(linux_size(Some(bad), None), 24, "{bad:?}");
        }
        assert_eq!(linux_size(None, None), 24);
    }

    #[test]
    fn kcminputrc_reads_only_the_mouse_group() {
        let rc = "[Keyboard]\ncursorSize=99\n\n[Mouse]\ncursorTheme=breeze_cursors\ncursorSize=36\n\n[Other]\ncursorSize=12\n";
        assert_eq!(parse_kcminputrc(rc), Some(36));
        assert_eq!(parse_kcminputrc("[Mouse]\ncursorSize[$e]= 48 \n"), Some(48));
        assert_eq!(parse_kcminputrc("[Mouse]\ncursorTheme=breeze\n"), None);
        assert_eq!(parse_kcminputrc("[Mouse]\ncursorSize=\n"), None);
        assert_eq!(parse_kcminputrc("cursorSize=30\n"), None);
        assert_eq!(parse_kcminputrc(""), None);
    }

    #[test]
    fn gsettings_output_with_and_without_a_type() {
        assert_eq!(parse_gsettings("uint32 48\n"), Some(48));
        assert_eq!(parse_gsettings("48"), Some(48));
        assert_eq!(parse_gsettings("int32 32\n"), Some(32));
        assert_eq!(parse_gsettings("uint32 0"), None);
        assert_eq!(parse_gsettings(""), None);
        assert_eq!(parse_gsettings("No such schema “org.gnome.desktop.interface”"), None);
    }

    #[test]
    fn scaling_rounds_to_physical_pixels() {
        assert_eq!(scaled(24.0, 1.0), 24);
        assert_eq!(scaled(24.0, 1.25), 30);
        assert_eq!(scaled(24.0, 1.5), 36);
        assert_eq!(scaled(24.0, 1.75), 42);
        assert_eq!(scaled(24.0, 2.0), 48);
        for bad in [0.0, -2.0, f64::NAN, f64::INFINITY] {
            assert_eq!(scaled(24.0, bad), 24);
        }
        assert_eq!(scaled(1.0, 0.1), 1);
    }

    #[test]
    fn windows_base_size_scales_with_dpi() {
        assert_eq!(windows_size(None, 1.0), 32);
        assert_eq!(windows_size(Some(32), 1.5), 48);
        assert_eq!(windows_size(Some(64), 1.25), 80);
        assert_eq!(windows_size(Some(0), 2.0), 64);
        assert_eq!(windows_size(Some(5000), 1.0), 32);
    }

    #[test]
    fn macos_accessibility_factor_is_clamped_and_scaled() {
        assert_eq!(macos_size(0.0, 2.0), 64); // unset preference
        assert_eq!(macos_size(1.0, 1.0), 32);
        assert_eq!(macos_size(1.5, 2.0), 96);
        assert_eq!(macos_size(2.25, 1.0), 72);
        assert_eq!(macos_size(9.0, 2.0), 256);
        assert_eq!(macos_size(f64::NAN, 1.0), 32);
    }
}
