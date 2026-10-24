//! The present group (`docs/jgl-replacement.md` §6, group 5): jgl's
//! graphics slots +0x30 `invalidate` and +0xA4 `present`, as values.
//!
//! What the slots do [V, jgl.dll disassembled]:
//!
//! - +0x30 `invalidate(rect)` (RVA 0x3BBB0) is
//!   `InvalidateRect(window, rect, FALSE)` on the graphics window (+0x144):
//!   the window is to be painted again (the game's forms call it, exe
//!   `Base_Form` m73). A null rectangle is the whole window.
//! - +0xA4 `present(rect)` (RVA 0x3BAA0) copies the back buffer onto the
//!   window: the back buffer is the image of the game's own image at
//!   +0x148 (its `+4`); jgl takes its DC (image +0x28), sets its colour
//!   table (+0xEC), `BitBlt`s `rect` of it (all of it if null) to the
//!   window's DC (+0x138) and releases the DC (+0x2C). It returns 0. The
//!   exe calls it once (0x6068AE), after drawing its pointer sprite onto
//!   the back buffer (0x60689C).

use crate::{Rect, Surface};

/// A call of the present group.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PresentCall {
    /// +0x30: `rect` of the window is to be painted again (`None`: all of
    /// it).
    Invalidate { rect: Option<Rect> },
    /// +0xA4: `rect` of the back buffer (`None`: all of it) to the window.
    /// `back`: the back buffer (`None` if the graphics object has none).
    Present { rect: Option<Rect>, back: Option<Surface> },
}
