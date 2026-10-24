//! The traits a consumer implements to receive jgl's drawing.
//!
//! One trait per method group (`docs/jgl-replacement.md` §4). Methods have
//! empty defaults. Arguments are plain values and borrowed slices: what a
//! hook needs is in its record, never behind a pointer into the game.

use crate::draw::{CopyCall, FillCall};
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

/// Whether jgl's own pixel work still runs for a drawing call (only while
/// jgl.dll is loaded: canvases the game reads back, calls the consumer does
/// not take, compare mode). Asked synchronously by the producer, so it
/// must be cheap.
pub trait PixelPolicy {
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
}

/// Every hook a consumer gives [`crate::Jgl`].
pub struct Hooks {
    pub text: Box<dyn TextHooks>,
    pub fill: Box<dyn FillHooks>,
    pub copy: Box<dyn CopyHooks>,
    pub policy: Box<dyn PixelPolicy>,
}

/// Hooks that do nothing and keep jgl's pixels.
pub struct NoHooks;

impl TextHooks for NoHooks {}
impl FillHooks for NoHooks {}
impl CopyHooks for NoHooks {}
impl PixelPolicy for NoHooks {}

impl Default for Hooks {
    fn default() -> Hooks {
        Hooks { text: Box::new(NoHooks), fill: Box::new(NoHooks), copy: Box::new(NoHooks), policy: Box::new(NoHooks) }
    }
}
