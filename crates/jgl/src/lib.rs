//! `jgl.dll`, the drawing library of Civilization III, as a safe Rust API.
//!
//! The game reaches jgl only through virtual methods of five object classes
//! (graphics, image, sprite, colour table, font; see [`slots`]). The unsafe
//! shim `jgl-ffi` puts entry points into those method tables, reads each
//! call's raw arguments and calls a [`Jgl`] here. This crate does what jgl
//! does with them: it keeps jgl's per-object state (the font and text
//! colour each image has selected, the fonts that exist), encodes every
//! call as a compact [`record`], and turns the records into calls on the
//! [`hooks`] traits that a game-specific consumer implements.
//!
//! Nothing here knows the game. An image's owner (its back pointer to the
//! game's own image object) is an opaque number passed through.
//!
//! See `docs/jgl-replacement.md`.

#![forbid(unsafe_code)]

pub mod blit;
pub mod counts;
pub mod display;
pub mod draw;
pub mod hooks;
mod machine;
pub mod present;
pub mod surface;
pub mod record;
pub mod slots;
pub mod text;

pub use blit::{BlitCall, Scale, SpriteCall, SpriteId, SpriteRef, TableCall, TableId, TableRef};
pub use draw::{CopyCall, FillCall, Surface};
pub use hooks::{BlitHooks, CopyHooks, FillHooks, Hooks, PaletteHooks, PixelPolicy, PresentHooks, SurfaceHooks, TextDraw, TextHooks};
pub use surface::SurfaceCall;
pub use present::PresentCall;
pub use machine::{Consumer, Jgl, Producer};
pub use text::{FontDesc, TextColor};

/// A `JGL_Image` (surface), by its address in the game's process.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageId(pub u32);

/// A jgl font object, by its address. [`FontId::STOCK`] (0) is the stock
/// font a fresh device context has and `deselect_font` selects.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontId(pub u32);

impl FontId {
    /// GDI's `SYSTEM_FONT`: what a new image's DC draws text in, and what
    /// image slot +0xAC (`deselect_font`) puts back.
    pub const STOCK: FontId = FontId(0);
}

/// A rectangle as GDI has it: left and top inside, right and bottom out.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect { left, top, right, bottom }
    }

    pub const fn to_array(self) -> [i32; 4] {
        [self.left, self.top, self.right, self.bottom]
    }
}
