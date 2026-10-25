//! The traits a consumer implements to receive jgl's drawing.
//!
//! One trait per method group (`docs/jgl-replacement.md` §4). Methods have
//! empty defaults. Arguments are plain values and borrowed slices: what a
//! hook needs is in its record, never behind a pointer into the game.

use crate::blit::{BlitCall, Scale, SpriteCall, TableCall};
use crate::draw::{CopyCall, FillCall};
use crate::present::PresentCall;
use crate::surface::SurfaceCall;
use crate::text::{FontDesc, TextColor};
use crate::{ImageId, Rect};

/// One text run (image slot +0xB8 `text_out`), with the image's selected
/// font and colour resolved.
#[derive(Copy, Clone, Debug)]
pub struct TextDraw<'a> {
    pub image: ImageId,
    /// The image's back pointer to its owner (+0x80), opaque to jgl.
    pub owner: u32,
    /// The reference point `TextOutA` gets: jgl's DCs align text to its
    /// left and baseline (`SetTextAlign(TA_BASELINE)` at image creation).
    pub x: i32,
    pub y: i32,
    /// The characters as passed (Windows-1252), at most
    /// [`crate::record::MAX_TAIL`].
    pub bytes: &'a [u8],
    pub font: &'a FontDesc,
    pub color: TextColor,
    /// The image's clip rectangle at the call.
    pub clip: Rect,
}

/// The text group: image slots +0xA8..+0xB8, font slots.
pub trait TextHooks {
    fn text(&mut self, _draw: &TextDraw<'_>) {}
    /// A font jgl created (or one read from GDI), first seen.
    fn font(&mut self, _desc: &FontDesc) {}
}

/// The fill group: image slots +0x44..+0x64.
pub trait FillHooks {
    fn fill(&mut self, _call: &FillCall) {}
}

/// The copy group: image slots +0x38, +0x40, +0x84, +0x8C.
pub trait CopyHooks {
    fn copy(&mut self, _call: &CopyCall) {}
}

/// The sprite group: blits (sprite slots +0x38..+0xA0), the sprites'
/// state slots and the draw scale (graphics +0x9C).
pub trait BlitHooks {
    fn blit(&mut self, _call: &BlitCall) {}
    fn sprite(&mut self, _call: &SpriteCall) {}
    fn scale(&mut self, _scale: &Scale) {}
}

/// The colour tables' slots that change them.
pub trait PaletteHooks {
    fn table(&mut self, _call: &TableCall) {}
}

/// The present group: graphics +0x30 `invalidate` and +0xA4 `present`.
pub trait PresentHooks {
    fn present(&mut self, _call: &PresentCall) {}
}

/// The surface group: objects made and deleted (graphics +0x74..+0x94).
pub trait SurfaceHooks {
    fn surface(&mut self, _call: &SurfaceCall) {}
}

/// Whether jgl's own pixel work still runs for a drawing call (only while
/// jgl.dll is loaded: canvases the game reads back, calls the consumer does
/// not take, compare mode). Asked synchronously by the producer, so it
/// must be cheap.
pub trait PixelPolicy {
    /// Whether this image holds data written and read directly by the caller.
    /// Owned images preserve plain memory fills and equal-size copies only
    /// for these images. This does not request rasterisation of draw records.
    fn preserve_image_data(&self, _image: ImageId, _owner: u32) -> bool {
        false
    }

    /// A text run (`text_out`) on `image`.
    fn original_pixels(&self, _image: ImageId, _owner: u32) -> bool {
        true
    }

    fn original_fill(&self, _call: &FillCall) -> bool {
        true
    }

    fn original_copy(&self, _call: &CopyCall) -> bool {
        true
    }

    fn original_blit(&self, _call: &BlitCall) -> bool {
        true
    }

    /// Whether jgl makes sprites' pixels (`init`, `duplicate` copy them);
    /// if not, they all point at one placeholder picture: nothing should
    /// draw them.
    fn original_sprite_pixels(&self) -> bool {
        true
    }

    /// Whether jgl copies the back buffer to its window (+0xA4). jgl's
    /// +0x30 `invalidate` always runs while the window is jgl's: the
    /// window's painting depends on it.
    fn original_present(&self, _call: &PresentCall) -> bool {
        true
    }
}

/// The window group (graphics +0x04 window, the pumps +0x120..+0x150,
/// +0x2C pointer) where our graphics object replaces jgl's: what the host
/// wants of the window and of the game's waits for messages. Called on the
/// game's thread, outside any record (a pump dispatches messages, which
/// call the game, which calls jgl again), so `&self`.
pub trait WindowHooks: Send + Sync {
    /// The screen size the game is told (what jgl asks `GetSystemMetrics`
    /// for), the display's mode being `display`.
    fn screen(&self, display: (i32, i32)) -> (i32, i32) {
        display
    }
    /// A pump is about to peek at the queue.
    fn peeking(&self) {}
    /// A peek took a message (`remove`: its `PM_*` flags).
    fn peeked(&self, _remove: u32, _message: u32, _lparam: u32) {}
    /// The game's thread waits for messages (`true`), or is back (`false`).
    fn waiting(&self, _entering: bool) {}
    /// The game's window was made.
    fn window_made(&self, _hwnd: u32) {}
}

/// Window hooks that change nothing.
pub struct NoWindowHooks;

impl WindowHooks for NoWindowHooks {}

/// Every hook a consumer gives [`crate::Jgl`].
pub struct Hooks {
    pub text: Box<dyn TextHooks>,
    pub fill: Box<dyn FillHooks>,
    pub copy: Box<dyn CopyHooks>,
    pub blit: Box<dyn BlitHooks>,
    pub palette: Box<dyn PaletteHooks>,
    pub present: Box<dyn PresentHooks>,
    pub surface: Box<dyn SurfaceHooks>,
    pub policy: Box<dyn PixelPolicy>,
}

/// Hooks that do nothing and keep jgl's pixels.
pub struct NoHooks;

impl TextHooks for NoHooks {}
impl FillHooks for NoHooks {}
impl CopyHooks for NoHooks {}
impl BlitHooks for NoHooks {}
impl PaletteHooks for NoHooks {}
impl PresentHooks for NoHooks {}
impl SurfaceHooks for NoHooks {}
impl PixelPolicy for NoHooks {}

impl Default for Hooks {
    fn default() -> Hooks {
        Hooks {
            text: Box::new(NoHooks),
            fill: Box::new(NoHooks),
            copy: Box::new(NoHooks),
            blit: Box::new(NoHooks),
            palette: Box::new(NoHooks),
            present: Box::new(NoHooks),
            surface: Box::new(NoHooks),
            policy: Box::new(NoHooks),
        }
    }
}
