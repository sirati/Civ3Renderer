//! The Civilization III interface as the game draws it through jgl: the
//! consumer of `jgl`'s hooks for interface surfaces.
//!
//! The interface model still lives in `civ3-bridge` (`ui.rs`,
//! `ui_trace.rs`); it moves here group by group as its input comes from jgl
//! hooks instead of exe detours (`docs/jgl-replacement.md` §2). The text
//! group is the first.

#![forbid(unsafe_code)]

pub mod text;
