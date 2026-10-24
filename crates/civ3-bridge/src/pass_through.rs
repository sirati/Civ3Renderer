//! Whether the bridge does anything but forward the game's calls to the
//! original `ifc23_orig.dll`.
//!
//! Without `CIV3R_VIEWER` there is no renderer, so the bridge stays out of
//! the game entirely: no hooks, no patches, no threads of its own beyond
//! the one that notes this in `civ3-bridge.log`. The game then runs exactly
//! as with its own `ifc23.dll`, and one installation serves both ways of
//! starting it. `CIV3R_TRACE=on` keeps the hooks without a viewer (the
//! game drawing itself while the bridge traces it), for diagnostics.

use std::ffi::OsStr;

/// Whether to hook the game, from `CIV3R_VIEWER` and `CIV3R_TRACE`.
pub fn active(viewer: Option<&OsStr>, trace: Option<&OsStr>) -> bool {
    viewer.is_some() || trace == Some(OsStr::new("on"))
}

/// [`active`] from this process's environment.
pub fn active_here() -> bool {
    active(std::env::var_os("CIV3R_VIEWER").as_deref(), std::env::var_os("CIV3R_TRACE").as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_viewer_the_bridge_only_forwards() {
        assert!(!active(None, None));
        assert!(!active(None, Some(OsStr::new("off"))));
        assert!(!active(None, Some(OsStr::new("1"))));
    }

    #[test]
    fn a_viewer_or_an_explicit_trace_hooks_the_game() {
        assert!(active(Some(OsStr::new("1")), None));
        assert!(active(Some(OsStr::new("native")), Some(OsStr::new("off"))));
        assert!(active(None, Some(OsStr::new("on"))));
    }
}
