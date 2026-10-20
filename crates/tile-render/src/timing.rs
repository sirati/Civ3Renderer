//! GPU time per render pass, from timestamp queries (where the adapter has
//! them, `wgpu::Features::TIMESTAMP_QUERY`), for profiling.
//!
//! Nothing ever waits for the results: each frame's timestamps are copied
//! into a readback buffer of a small ring, mapped once the GPU is done with
//! them and read on a later frame. A frame that finds no free buffer is not
//! timed. Frames that exceed the query budget are also omitted, so a partial
//! span cannot be mistaken for a complete GPU frame or create an idle gap.

use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// Timestamps a frame may write (two per pass).
const QUERIES: u32 = 1024;
/// Bounded diagnostic readbacks in flight. Callback delivery can lag several
/// rendered frames even at modest FPS, so four slots lost measurements in the
/// actual application. Thirty-two slots cost 256 KiB at the full query budget;
/// saturation still drops a sample and never delays rendering.
const RING: usize = 32;

/// Whether GPU timing is asked for (`TILE_RENDER_GPU_TIMING`, or the
/// viewer's profile, `TILE_VIEWER_PROFILE`).
pub fn requested() -> bool {
    let on = |v: &str| std::env::var(v).is_ok_and(|v| !v.is_empty() && v != "0");
    on("TILE_RENDER_GPU_TIMING") || on("TILE_VIEWER_PROFILE")
}

/// Whether every step of a view (each render pass of the world, and the
/// free sprites after it) is timed on its own, in a render pass of its
/// own (`TILE_RENDER_GPU_TIMING=passes`); else a view's steps share one.
pub fn per_pass() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("TILE_RENDER_GPU_TIMING").is_ok_and(|v| v == "passes"))
}

/// Timestamp bounds of one instrumented frame, in a single device's clock.
/// IDs include untimed attempts, so a skipped query cannot become an idle gap.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStamp {
    pub frame_id: u64,
    pub start: u64,
    pub end: u64,
    pub period_ns: f64,
}

impl FrameStamp {
    /// Gap between adjacent frame bounds. Includes pacing, presentation,
    /// uploads and other queue work; cannot isolate waiting for the CPU.
    pub fn gap_after(self, previous: Self) -> Option<f64> {
        if previous.frame_id.checked_add(1) != Some(self.frame_id)
            || self.period_ns != previous.period_ns || !self.period_ns.is_finite() || self.period_ns <= 0.0
            || self.end < self.start || previous.end < previous.start {
            return None;
        }
        Some(self.start.saturating_sub(previous.end) as f64 * self.period_ns / 1e6)
    }
}

/// One timed frame: the GPU time of each of its passes (by label, in
/// drawing order) and from the first pass's start to the last one's end,
/// in milliseconds.
#[derive(Clone, Debug, Default)]
pub struct FrameTimes {
    pub stamp: FrameStamp,
    /// Opaque caller packet ID, preserved across asynchronous readback.
    pub frame_token: Option<u64>,
    pub passes: Vec<(String, f64)>,
    pub span: f64,
    /// Per pass in drawing order: how much later it ended than the passes
    /// before it (from the first start for the first). Passes overlap on
    /// tiled GPUs, where their own durations add up to far more than the
    /// frame; these add up to its span.
    pub increments: Vec<(String, f64)>,
}

/// Cumulative diagnostic counts for this timing/device epoch. Samples remain
/// asynchronous; missing measurements are counted rather than guessed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimerStats {
    pub attempted_frames: u64,
    pub prepared_samples: u64,
    pub submitted_samples: u64,
    pub collected_samples: u64,
    pub empty_frames: u64,
    pub discarded_frames: u64,
    pub over_budget_frames: u64,
    pub readback_busy_frames: u64,
    pub map_failed_frames: u64,
    pub mapped_range_failed_frames: u64,
}

const FREE: u8 = 0;
const MAPPING: u8 = 1;
const MAPPED: u8 = 2;
const FAILED: u8 = 3;

/// Ready results in submission order, stopping at the first pending result.
fn ready_order(states: impl IntoIterator<Item = (usize, u64, u8)>) -> Vec<usize> {
    let mut ordered: Vec<_> = states.into_iter().filter(|&(_, _, state)| state != FREE).collect();
    ordered.sort_by_key(|&(_, frame, _)| frame);
    ordered.into_iter().take_while(|&(_, _, state)| matches!(state, MAPPED | FAILED)).map(|(slot, _, _)| slot).collect()
}

struct Readback {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
    labels: Vec<String>,
    frame_id: u64,
    frame_token: Option<u64>,
}

/// Timestamp queries around render passes.
pub struct GpuTimer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    ring: Vec<Readback>,
    /// Queries used by the frame being recorded, and its passes' labels.
    next: Cell<u32>,
    incomplete: Cell<bool>,
    labels: RefCell<Vec<String>>,
    /// Readbacks for encoded frames, mapped after their submission.
    pending: Vec<usize>,
    frame_id: u64,
    stats: TimerStats,
    /// Nanoseconds per timestamp tick.
    period: f64,
}

impl GpuTimer {
    /// A timer, if `device` has timestamp queries.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<GpuTimer> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let set = device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("tile-render timing"), ty: wgpu::QueryType::Timestamp, count: QUERIES });
        let bytes = QUERIES as u64 * 8;
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-render timing resolve"),
            size: bytes,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let ring = (0..RING)
            .map(|_| Readback {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("tile-render timing readback"),
                    size: bytes,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                state: Arc::new(AtomicU8::new(FREE)),
                labels: Vec::new(),
                frame_id: 0,
                frame_token: None,
            })
            .collect();
        Some(GpuTimer { set, resolve, ring, next: Cell::new(0), incomplete: Cell::new(false), labels: RefCell::new(Vec::new()), pending: Vec::new(), frame_id: 0, stats: TimerStats::default(), period: f64::from(queue.get_timestamp_period()) })
    }

    /// Counters for qualification and profiling, without polling or waiting.
    pub fn stats(&self) -> TimerStats { self.stats }

    /// The timestamp writes of a pass labelled `label` (none once the
    /// frame's queries are used up).
    pub(crate) fn pass(&self, label: impl FnOnce() -> String) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let i = self.next.get();
        if i + 2 > QUERIES {
            self.incomplete.set(true);
            return None;
        }
        self.next.set(i + 2);
        self.labels.borrow_mut().push(label());
        Some(wgpu::RenderPassTimestampWrites { query_set: &self.set, beginning_of_pass_write_index: Some(i), end_of_pass_write_index: Some(i + 1) })
    }

    /// Ends the frame recorded into `encoder`: its timestamps go to a free
    /// readback buffer (if there is none, the frame is not timed).
    pub fn finish(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.finish_with_token(encoder, None);
    }

    /// Like finish, preserving the accepted packet ID through delayed results.
    pub fn finish_frame(&mut self, encoder: &mut wgpu::CommandEncoder, frame_token: u64) {
        self.finish_with_token(encoder, Some(frame_token));
    }

    fn finish_with_token(&mut self, encoder: &mut wgpu::CommandEncoder, frame_token: Option<u64>) {
        self.frame_id = self.frame_id.wrapping_add(1);
        self.stats.attempted_frames += 1;
        let n = self.next.replace(0);
        let labels = std::mem::take(&mut *self.labels.borrow_mut());
        let incomplete = self.incomplete.replace(false);
        if incomplete {
            self.stats.over_budget_frames += 1;
            return;
        }
        if n == 0 {
            self.stats.empty_frames += 1;
            return;
        }
        let Some(slot) = self.ring.iter().position(|r| r.state.load(Ordering::Acquire) == FREE) else {
            self.stats.readback_busy_frames += 1;
            return;
        };
        self.stats.prepared_samples += 1;
        encoder.resolve_query_set(&self.set, 0..n, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.ring[slot].buffer, 0, n as u64 * 8);
        self.ring[slot].labels = labels;
        self.ring[slot].frame_id = self.frame_id;
        self.ring[slot].frame_token = frame_token;
        self.ring[slot].state.store(MAPPING, Ordering::Release);
        self.pending.push(slot);
    }

    /// Before finish: the current encoder was abandoned or contained no
    /// drawing commands. Never resolve unwritten queries or bridge this
    /// attempt with an idle gap. Already finished encoders remain pending.
    pub fn discard_frame(&mut self) {
        self.frame_id = self.frame_id.wrapping_add(1);
        self.stats.attempted_frames += 1;
        self.stats.discarded_frames += 1;
        self.next.set(0);
        self.incomplete.set(false);
        self.labels.borrow_mut().clear();
    }

    /// After the frame's submission: its readback is mapped once the GPU
    /// is done (see [`GpuTimer::collect`]).
    pub fn submitted(&mut self) {
        for slot in self.pending.drain(..) {
            self.stats.submitted_samples += 1;
            let r = &self.ring[slot];
            let n = r.labels.len() as u64 * 16;
            let state = r.state.clone();
            r.buffer.slice(..n).map_async(wgpu::MapMode::Read, move |result| {
                state.store(if result.is_ok() { MAPPED } else { FAILED }, Ordering::Release);
            });
        }
    }

    /// The frames whose times are there by now (polls the device without
    /// waiting).
    pub fn collect(&mut self, device: &wgpu::Device) -> Vec<FrameTimes> {
        let _ = device.poll(wgpu::PollType::Poll);
        let mut out = Vec::new();
        // Callbacks and ring indices need not arrive in submission order.
        // Defer a later ready result while an earlier one is still mapping.
        let order = ready_order(self.ring.iter().enumerate().map(|(i, r)| (i, r.frame_id, r.state.load(Ordering::Acquire))));
        for i in order {
            let r = &mut self.ring[i];
            match r.state.load(Ordering::Acquire) {
                FREE => continue,
                FAILED => {
                    self.stats.map_failed_frames += 1;
                    r.labels.clear();
                    r.state.store(FREE, Ordering::Release);
                    continue;
                },
                MAPPED => {},
                _ => break,
            }
            let n = r.labels.len() * 2;
            {
                let Ok(view) = r.buffer.slice(..n as u64 * 8).get_mapped_range() else {
                    // A failed result is a missing frame, not a permanently
                    // occupied slot. Its ID still breaks gap adjacency.
                    r.buffer.unmap();
                    r.labels.clear();
                    self.stats.mapped_range_failed_frames += 1;
                    r.state.store(FREE, Ordering::Release);
                    continue;
                };
                let ticks: &[u64] = bytemuck::cast_slice(&view);
                let ms = |t: u64| t as f64 * self.period / 1e6;
                let mut frame = FrameTimes::default();
                let first = ticks.iter().step_by(2).copied().min().unwrap_or(0);
                let mut ended = first;
                for (i, label) in r.labels.drain(..).enumerate() {
                    let (a, b) = (ticks[2 * i], ticks[2 * i + 1]);
                    frame.increments.push((label.clone(), ms(b.saturating_sub(ended))));
                    ended = ended.max(b);
                    frame.passes.push((label, ms(b.saturating_sub(a))));
                }
                let last = ticks.iter().skip(1).step_by(2).copied().max().unwrap_or(0);
                frame.span = ms(last.saturating_sub(first));
                frame.frame_token = r.frame_token;
                frame.stamp = FrameStamp { frame_id: r.frame_id, start: first, end: last, period_ns: self.period };
                out.push(frame);
                self.stats.collected_samples += 1;
            }
            r.buffer.unmap();
            r.state.store(FREE, Ordering::Release);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_timed_encoders_keep_tokens_and_empty_encoders_break_adjacency() {
        let gpu = crate::Headless::new().expect("GPU");
        let Some(mut timer) = GpuTimer::new(&gpu.device, &gpu.queue) else {
            eprintln!("GPU timing regression skipped: adapter lacks TIMESTAMP_QUERY");
            return;
        };
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None, size: wgpu::Extent3d { width: 8, height: 8, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let encode = |timer: &mut GpuTimer, token: u64| {
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view, depth_slice: None, resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    })], depth_stencil_attachment: None, timestamp_writes: timer.pass(|| "test".into()), occlusion_query_set: None, multiview_mask: None,
                });
            }
            timer.finish_frame(&mut encoder, token);
            encoder.finish()
        };
        let commands = [encode(&mut timer, 100), encode(&mut timer, 101), encode(&mut timer, 102)];
        gpu.queue.submit(commands);
        timer.submitted();
        let collect = |timer: &mut GpuTimer, count: usize| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let mut frames = Vec::new();
            while frames.len() < count && std::time::Instant::now() < deadline {
                frames.extend(timer.collect(&gpu.device));
                if frames.len() < count { std::thread::sleep(std::time::Duration::from_millis(1)); }
            }
            assert_eq!(frames.len(), count, "asynchronous results must arrive without a blocking readback");
            frames
        };
        let frames = collect(&mut timer, 3);
        assert_eq!(frames.iter().map(|f| f.frame_token).collect::<Vec<_>>(), [Some(100), Some(101), Some(102)]);
        assert_eq!(frames.iter().map(|f| f.stamp.frame_id).collect::<Vec<_>>(), [1, 2, 3]);
        let mut empty = gpu.device.create_command_encoder(&Default::default());
        timer.finish_frame(&mut empty, 103);
        let next = encode(&mut timer, 104);
        gpu.queue.submit([empty.finish(), next]);
        timer.submitted();
        let next = collect(&mut timer, 1).pop().unwrap();
        assert_eq!(next.stamp.frame_id, 5);
        assert_eq!(next.stamp.gap_after(frames.last().unwrap().stamp), None);
        // A busy diagnostic frame may request more pass timestamps than the
        // bounded query pool. Its partial span must not become a sample, or
        // make later untimed work look like the GPU waited for the CPU.
        for _ in 0..QUERIES / 2 { assert!(timer.pass(|| "budget".into()).is_some()); }
        assert!(timer.pass(|| "over budget".into()).is_none());
        let mut incomplete = gpu.device.create_command_encoder(&Default::default());
        timer.finish_frame(&mut incomplete, 105);
        let after_budget = encode(&mut timer, 106);
        gpu.queue.submit([incomplete.finish(), after_budget]);
        timer.submitted();
        let after_budget = collect(&mut timer, 1).pop().unwrap();
        assert_eq!(after_budget.frame_token, Some(106));
        assert_eq!(after_budget.stamp.frame_id, 7);
        assert_eq!(after_budget.stamp.gap_after(next.stamp), None);
        assert_eq!(timer.stats(), TimerStats {
            attempted_frames: 7, prepared_samples: 5, submitted_samples: 5,
            collected_samples: 5, empty_frames: 1, over_budget_frames: 1,
            ..Default::default()
        });
        // More encoders can be submitted than the asynchronous readback ring
        // can measure. The unmeasured frame is counted without a device wait.
        let commands: Vec<_> = (0..RING + 1).map(|i| encode(&mut timer, 200 + i as u64)).collect();
        gpu.queue.submit(commands);
        timer.submitted();
        let ring_frames = collect(&mut timer, RING);
        assert_eq!(ring_frames.len(), RING);
        timer.discard_frame();
        let stats = timer.stats();
        assert_eq!(stats.attempted_frames, 7 + RING as u64 + 2);
        assert_eq!(stats.prepared_samples, 5 + RING as u64);
        assert_eq!(stats.submitted_samples, stats.prepared_samples);
        assert_eq!(stats.collected_samples, stats.prepared_samples);
        assert_eq!(stats.readback_busy_frames, 1);
        assert_eq!(stats.discarded_frames, 1);
        assert_eq!(stats.map_failed_frames, 0);
        assert_eq!(stats.mapped_range_failed_frames, 0);
    }

    #[test]
    fn asynchronous_readback_order_never_attributes_a_late_frame_to_a_newer_one() {
        assert_eq!(ready_order([(0, 4, MAPPED), (1, 1, MAPPED), (2, 3, MAPPING), (3, 2, MAPPED)]), [1, 3]);
        assert_eq!(ready_order([(0, 4, MAPPED), (1, 1, FREE), (2, 3, MAPPED), (3, 2, MAPPED)]), [3, 2, 0]);
        assert!(ready_order([(0, 1, MAPPING), (1, 2, MAPPED)]).is_empty());
        assert_eq!(ready_order([(0, 1, FAILED), (1, 2, MAPPED), (2, 3, MAPPING)]), [0, 1]);
    }

    #[test]
    fn gaps_use_adjacent_device_timestamps_not_host_frame_intervals() {
        let first = FrameStamp { frame_id: 1, start: 10, end: 30, period_ns: 1_000_000.0 };
        let next = FrameStamp { frame_id: 2, start: 42, end: 50, ..first };
        assert_eq!(next.gap_after(first), Some(12.0));
        assert_eq!(FrameStamp { frame_id: 3, ..next }.gap_after(first), None);
        assert_eq!(FrameStamp { frame_id: 1, ..next }.gap_after(first), None);
        assert_eq!(FrameStamp { period_ns: 2.0, ..next }.gap_after(first), None);
        assert_eq!(FrameStamp { start: 29, ..next }.gap_after(first), Some(0.0));
        assert_eq!(FrameStamp { start: 51, ..next }.gap_after(first), None);
    }
}
