//! GPU time per render pass, from timestamp queries (where the adapter has
//! them, `wgpu::Features::TIMESTAMP_QUERY`), for profiling.
//!
//! Nothing ever waits for the results: each frame's timestamps are copied
//! into a readback buffer of a small ring, mapped once the GPU is done with
//! them and read on a later frame. A frame that finds no free buffer is not
//! timed.

use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// Timestamps a frame may write (two per pass).
const QUERIES: u32 = 1024;
/// Readback buffers in flight.
const RING: usize = 4;

/// Whether GPU timing is asked for (`TILE_RENDER_GPU_TIMING`, or the
/// viewer's profile, `TILE_VIEWER_PROFILE`).
pub fn requested() -> bool {
    let on = |v: &str| std::env::var(v).is_ok_and(|v| !v.is_empty() && v != "0");
    on("TILE_RENDER_GPU_TIMING") || on("TILE_VIEWER_PROFILE")
}

/// One timed frame: the GPU time of each of its passes (by label, in
/// drawing order) and from the first pass's start to the last one's end,
/// in milliseconds.
#[derive(Clone, Debug, Default)]
pub struct FrameTimes {
    pub passes: Vec<(String, f64)>,
    pub span: f64,
}

const FREE: u8 = 0;
const MAPPING: u8 = 1;
const MAPPED: u8 = 2;

struct Readback {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
    labels: Vec<String>,
}

/// Timestamp queries around render passes.
pub struct GpuTimer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    ring: Vec<Readback>,
    /// Queries used by the frame being recorded, and its passes' labels.
    next: Cell<u32>,
    labels: RefCell<Vec<String>>,
    /// The readback the last frame went to (mapped once it is submitted).
    pending: Option<usize>,
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
            })
            .collect();
        Some(GpuTimer { set, resolve, ring, next: Cell::new(0), labels: RefCell::new(Vec::new()), pending: None, period: f64::from(queue.get_timestamp_period()) })
    }

    /// The timestamp writes of a pass labelled `label` (none once the
    /// frame's queries are used up).
    pub(crate) fn pass(&self, label: impl FnOnce() -> String) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let i = self.next.get();
        if i + 2 > QUERIES {
            return None;
        }
        self.next.set(i + 2);
        self.labels.borrow_mut().push(label());
        Some(wgpu::RenderPassTimestampWrites { query_set: &self.set, beginning_of_pass_write_index: Some(i), end_of_pass_write_index: Some(i + 1) })
    }

    /// Ends the frame recorded into `encoder`: its timestamps go to a free
    /// readback buffer (if there is none, the frame is not timed).
    pub fn finish(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let n = self.next.replace(0);
        let labels = std::mem::take(&mut *self.labels.borrow_mut());
        self.pending = None;
        if n == 0 {
            return;
        }
        let Some(slot) = self.ring.iter().position(|r| r.state.load(Ordering::Acquire) == FREE) else { return };
        encoder.resolve_query_set(&self.set, 0..n, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.ring[slot].buffer, 0, n as u64 * 8);
        self.ring[slot].labels = labels;
        self.ring[slot].state.store(MAPPING, Ordering::Release);
        self.pending = Some(slot);
    }

    /// After the frame's submission: its readback is mapped once the GPU
    /// is done (see [`GpuTimer::collect`]).
    pub fn submitted(&mut self) {
        let Some(slot) = self.pending.take() else { return };
        let r = &self.ring[slot];
        let n = r.labels.len() as u64 * 16;
        let state = r.state.clone();
        r.buffer.slice(..n).map_async(wgpu::MapMode::Read, move |result| {
            state.store(if result.is_ok() { MAPPED } else { FREE }, Ordering::Release);
        });
    }

    /// The frames whose times are there by now (polls the device without
    /// waiting).
    pub fn collect(&mut self, device: &wgpu::Device) -> Vec<FrameTimes> {
        let _ = device.poll(wgpu::PollType::Poll);
        let mut out = Vec::new();
        for r in &mut self.ring {
            if r.state.load(Ordering::Acquire) != MAPPED {
                continue;
            }
            let n = r.labels.len() * 2;
            {
                let Ok(view) = r.buffer.slice(..n as u64 * 8).get_mapped_range() else { continue };
                let ticks: &[u64] = bytemuck::cast_slice(&view);
                let ms = |t: u64| t as f64 * self.period / 1e6;
                let mut frame = FrameTimes::default();
                for (i, label) in r.labels.drain(..).enumerate() {
                    let (a, b) = (ticks[2 * i], ticks[2 * i + 1]);
                    frame.passes.push((label, ms(b.saturating_sub(a))));
                }
                let first = ticks.iter().step_by(2).copied().min().unwrap_or(0);
                let last = ticks.iter().skip(1).step_by(2).copied().max().unwrap_or(0);
                frame.span = ms(last.saturating_sub(first));
                out.push(frame);
            }
            r.buffer.unmap();
            r.state.store(FREE, Ordering::Release);
        }
        out
    }
}
