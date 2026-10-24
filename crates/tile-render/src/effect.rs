//! Effects drawn over views and screen copies by other crates (e.g. a
//! lens's glass rim). The renderer calls a view effect's GPU part
//! ([`ViewEffect`], made by [`crate::Shaders::effect`]) at fixed points of
//! a frame, and its CPU oracle ([`crate::reference::EffectOracle`]) at the
//! same points; it knows nothing of what they draw.

use crate::camera::Camera;
use crate::renderer::{Present, ScreenCopy};
use crate::timing::GpuTimer;

/// A view's ([`Camera::effect`]) or a screen copy's
/// ([`ScreenCopy::effect`]) effect, as the effect's crate describes it:
/// opaque to the renderer but for `lengths`, which are in composition
/// pixels and grow with a view drawn into a larger canvas
/// ([`Camera::boxed`], [`ScreenCopy::boxed`]).
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Effect {
    pub lengths: [f32; 2],
    pub variant: u32,
}

impl Effect {
    /// The effect of a view drawn `k` times as large each way.
    pub fn scaled(self, k: f32) -> Effect {
        Effect { lengths: self.lengths.map(|l| l * k), ..self }
    }
}

/// Draw controls (uniform buffers) a frame writes are kept this many times
/// over: an effect's buffers indexed by [`EffectContext::slot`] are not in
/// use by a frame still on the GPU.
pub const CONTROL_SLOTS: usize = crate::renderer::CONTROL_SLOTS;

/// What a [`ViewEffect`] draws with at one point of a frame.
pub struct EffectContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    /// The frame's control slot (`0..CONTROL_SLOTS`).
    pub slot: usize,
    /// The canvas's quantized colour depth (0: direct colour), and whether
    /// its quantized values are stored as integer FP16 (1) rather than
    /// biased UNORM8 (0) (see `tile_render_shared::QUANTIZED_BIAS`).
    pub bits: u32,
    pub quantized_float: u32,
    /// GPU timing, when on (labelled render passes).
    pub timer: Option<&'a GpuTimer>,
}

impl EffectContext<'_> {
    /// The timestamp writes of a render pass labelled `label`, when timing.
    pub fn timed(&self, label: impl FnOnce() -> String) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        self.timer.and_then(|t| t.pass(label))
    }
}

/// Where a screen copy was just drawn in the output pass
/// ([`ViewEffect::over_copy`]).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CopyPlacement {
    /// Its index among the frame's copies (`0..MAX_COPIES`).
    pub index: usize,
    /// The target pixels it was drawn to (x, y, width, height).
    pub on_target: [u32; 4],
    /// The target pixels the whole picture is shown in (x, y, width, height).
    pub picture: [u32; 4],
    /// The source texel at (`dst`), and target pixels per source texel: as
    /// the output pass drew the copy.
    pub src: (i32, i32),
    pub dst: (i32, i32),
    pub zoom: (u32, u32),
    /// The target's format.
    pub format: wgpu::TextureFormat,
}

/// A view effect's GPU part. A frame calls `begin_frame`, then for each
/// view with an effect `before_view` and `after_view` around its drawing;
/// once its canvas is shown on the target, `presented`, then `over_copy`
/// for each screen copy with an effect, after drawing the copy.
pub trait ViewEffect: Send {
    /// GPU memory it holds, in bytes.
    fn bytes(&self) -> u64;

    /// A new frame, whose views from `keep_before` on are drawn (those
    /// before it are the last frame's, its picture reused).
    fn begin_frame(&mut self, keep_before: usize);

    /// Before view `view` (`camera`, which has an effect) is drawn into
    /// `canvas`.
    fn before_view(&mut self, cx: &EffectContext<'_>, encoder: &mut wgpu::CommandEncoder, canvas: &wgpu::Texture, camera: &Camera, view: usize);

    /// After it was drawn.
    fn after_view(&mut self, cx: &EffectContext<'_>, encoder: &mut wgpu::CommandEncoder, canvas: &wgpu::Texture, camera: &Camera, view: usize);

    /// After the canvas (`composed`) was shown on `target` as `present` says.
    fn presented(&self, cx: &EffectContext<'_>, encoder: &mut wgpu::CommandEncoder, target: &wgpu::Texture, composed: &wgpu::Texture, present: Present);

    /// Over screen copy `copy` (which has an effect), just drawn into
    /// `pass` (onto the target) from `source` (the canvas, presented as
    /// `present` says) where `placement` says. The pass's pipeline, bind
    /// groups and scissor are the effect's to change.
    fn over_copy(&self, cx: &EffectContext<'_>, pass: &mut wgpu::RenderPass<'_>, copy: &ScreenCopy, placement: CopyPlacement, source: &wgpu::Texture, present: Present);
}
