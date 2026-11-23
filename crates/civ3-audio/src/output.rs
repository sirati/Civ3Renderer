//! Lock-free single-threaded async native audio engine and real-time `cpal`
//! output stream.
//!
//! Uses zero locks (`no_locks` enforced) and zero `thread::sleep` / `park_timeout`:
//! - Game thread -> Audio thread commands go through a `tokio::sync::mpsc`
//!   unbounded channel (lock-free push that immediately wakes the single-threaded
//!   `tokio` current-thread runtime's `Waker`).
//! - All engine state (`AudioEngine`, `cpal::Stream`, `HostResampler`,
//!   `StatusDoubleBuffer`) lives on one native thread (`civ3r-native-audio`) and
//!   needs no `Sync` / `Send` bounds.
//! - Heavy file decoding (`symphonia` WAV/MP3 and `.amb` child WAVs) runs on a
//!   dedicated background decoder thread (`civ3r-native-audio-decode`) so
//!   neither the game thread nor the real-time mixer loop ever stalls on disk
//!   I/O or codec work.
//! - `QueryState` (`is_playing`, `get_position_ms`, playlist loaded/queued) is
//!   answered wait-free on the game thread from a double-buffered atomic
//!   [`StatusSnapshot`] (`handoff::Latest`) using the host monotonic clock
//!   (`Instant`), with a sequence-tagged lock-free overlay for in-flight
//!   fire-and-forget commands.
//! - Audio thread -> `cpal` real-time callback transfers pre-mixed stereo `f32`
//!   samples through a `#![forbid(unsafe_code)]` SPSC ring buffer built from
//!   `Box<[AtomicU32]>` + `AtomicUsize` indices (wait-free, zero allocations in
//!   the audio callback).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleFormat, Stream, StreamConfig, SupportedBufferSize};
use handoff::Latest;
use rubato::audioadapter_buffers::direct::{InterleavedSlice, SequentialSliceOfVecs};
use rubato::{
    Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use tokio::sync::mpsc;

use crate::decoder::{probe_amb_header, probe_audio_header, ProbeError};
use crate::dsp::MIX_SAMPLE_RATE;
use crate::mixer::{
    execute_decode_job, AudioEngine, DecodeJob, DecodedResult, HandleStatus, SNDERR_ALREADY_LOADED,
    SNDERR_FILE_OPEN, SNDERR_INVALID_FORMAT, SNDERR_INVALID_PARAM, SNDERR_NO_OBJECT, SNDERR_OK,
    SNDERR_UNLOADED,
};
use crate::path::resolve_game_path;
use crate::wire::{
    Op, RequestHeader, Response, SoundHandle, SoundKind, STATE_DS_BUFFER, STATE_LOADED,
    STATE_LOOPING, STATE_PLAYLIST_HAS_QUEUED,
};

/// Lock-free single-producer / single-consumer ring buffer of `f32` samples
/// stored as bit patterns in `AtomicU32`. `#![forbid(unsafe_code)]`.
struct SpscRing {
    buf: Box<[AtomicU32]>,
    mask: usize,
    read: AtomicUsize,
    write: AtomicUsize,
}

impl SpscRing {
    fn new(capacity_pow2: usize) -> Self {
        assert!(capacity_pow2.is_power_of_two() && capacity_pow2 >= 1024);
        let buf = (0..capacity_pow2).map(|_| AtomicU32::new(0)).collect();
        Self {
            buf,
            mask: capacity_pow2 - 1,
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.write
            .load(Ordering::Acquire)
            .wrapping_sub(self.read.load(Ordering::Acquire))
    }

    #[inline]
    fn free(&self) -> usize {
        self.buf.len().saturating_sub(self.len())
    }

    /// Producer (audio engine thread): pushes as many samples as fit.
    fn push_slice(&self, samples: &[f32]) -> usize {
        let avail = self.free();
        let n = samples.len().min(avail);
        if n == 0 {
            return 0;
        }
        let mut w = self.write.load(Ordering::Relaxed);
        for &s in &samples[..n] {
            self.buf[w & self.mask].store(s.to_bits(), Ordering::Relaxed);
            w = w.wrapping_add(1);
        }
        self.write.store(w, Ordering::Release);
        n
    }

    /// Consumer (real-time `cpal` callback): pops stereo `(l, r)` frames.
    #[inline]
    fn pop_stereo(&self) -> Option<(f32, f32)> {
        let r = self.read.load(Ordering::Relaxed);
        let w = self.write.load(Ordering::Acquire);
        if w.wrapping_sub(r) < 2 {
            return None;
        }
        let l = f32::from_bits(self.buf[r & self.mask].load(Ordering::Relaxed));
        let ri = f32::from_bits(self.buf[r.wrapping_add(1) & self.mask].load(Ordering::Relaxed));
        self.read.store(r.wrapping_add(2), Ordering::Release);
        Some((l, ri))
    }
}

/// Optional `rubato` host-rate converter (created only when host sample rate != 44100 Hz).
struct HostResampler {
    resampler: Async<f64>,
    in_chunk: usize,
    in_buf: Vec<f64>,
    out_planes: Vec<Vec<f64>>,
    out_interleaved: Vec<f32>,
}

impl HostResampler {
    fn new(target_rate: u32) -> Option<Self> {
        let ratio = f64::from(target_rate) / f64::from(MIX_SAMPLE_RATE);
        let in_chunk = 512usize;
        let params = SincInterpolationParameters {
            sinc_len: 64,
            f_cutoff: Some(0.95),
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };
        let resampler =
            Async::<f64>::new_sinc(ratio, 1.1, &params, in_chunk, 2, FixedAsync::Input).ok()?;
        let out_max = resampler.output_frames_max();
        Some(Self {
            resampler,
            in_chunk,
            in_buf: Vec::with_capacity(in_chunk * 2),
            out_planes: vec![vec![0.0; out_max]; 2],
            out_interleaved: Vec::with_capacity(out_max * 2),
        })
    }

    fn process(&mut self, stereo_44100: &[f32]) -> &[f32] {
        self.out_interleaved.clear();
        for &s in stereo_44100 {
            self.in_buf.push(f64::from(s));
        }
        let chunk_samples = self.in_chunk * 2;
        while self.in_buf.len() >= chunk_samples {
            let frames = self.in_chunk;
            let Ok(in_adapter) = InterleavedSlice::new(&self.in_buf[..chunk_samples], 2, frames)
            else {
                break;
            };
            let out_cap = self.out_planes[0].len();
            let Ok(mut out_adapter) = SequentialSliceOfVecs::new_mut(&mut self.out_planes, 2, out_cap)
            else {
                break;
            };
            if let Ok((_, produced)) =
                self.resampler.process_into_buffer(&in_adapter, &mut out_adapter, None)
            {
                for i in 0..produced {
                    self.out_interleaved.push(self.out_planes[0][i] as f32);
                    self.out_interleaved.push(self.out_planes[1][i] as f32);
                }
            }
            self.in_buf.drain(..chunk_samples);
        }
        &self.out_interleaved
    }
}

/// Target ALSA period duration (ms) on Linux. `cpal`'s ALSA backend constrains
/// `BufferSize::Default` to `2 × device_period` (often 512 frames = ~10.6 ms
/// under PipeWire-ALSA), which underruns easily under heavy CPU load. Requesting
/// a ~45 ms period yields a ~90 ms double-buffered ALSA ring with 4× more
/// scheduling headroom while keeping interactive click latency responsive.
#[cfg(any(target_os = "linux", test))]
const TARGET_ALSA_PERIOD_MS: u32 = 45;

/// SPSC ring capacity in interleaved `f32` samples (32,768 stereo frames =
/// ~682 ms at 48 kHz, ~743 ms at 44.1 kHz).
const SPSC_RING_CAPACITY: usize = 65_536;

/// Target audio buffered ahead in the SPSC ring (ms) so scheduling bursts on
/// the mixer thread never starve the real-time callback.
const TARGET_RING_BUFFER_MS: u64 = 120;

/// Maximum 512-frame blocks rendered per mixer wakeup to catch up rapidly after
/// a host scheduling stall.
const MAX_REFILL_CHUNKS_PER_WAKEUP: usize = 24;

/// Async timer interval while audio output is open & active (never armed while idle).
const ACTIVE_REFILL_INTERVAL_MS: u64 = 5;

/// Lock-free rate limiter for repeated `cpal` stream error callbacks (e.g.
/// ALSA xruns under extreme host CPU load). Logs the first few errors
/// immediately, then coalesces bursts into a periodic summary every 5 seconds.
struct StreamErrorLimiter {
    started: Instant,
    total: AtomicU32,
    suppressed: AtomicU32,
    last_log_ms: AtomicU64,
}

impl StreamErrorLimiter {
    const IMMEDIATE_LOGS: u32 = 3;
    const SUMMARY_INTERVAL_MS: u64 = 5_000;

    fn new() -> Self {
        Self {
            started: Instant::now(),
            total: AtomicU32::new(0),
            suppressed: AtomicU32::new(0),
            last_log_ms: AtomicU64::new(0),
        }
    }

    /// Returns `Some(suppressed_since_last)` when this error should be logged.
    fn record_at_ms(&self, now_ms: u64) -> Option<u32> {
        let count = self.total.fetch_add(1, Ordering::Relaxed) + 1;
        if count <= Self::IMMEDIATE_LOGS {
            self.last_log_ms.store(now_ms, Ordering::Relaxed);
            return Some(0);
        }
        let prev = self.last_log_ms.load(Ordering::Relaxed);
        if now_ms.saturating_sub(prev) >= Self::SUMMARY_INTERVAL_MS
            && self
                .last_log_ms
                .compare_exchange(prev, now_ms, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        {
            let skipped = self.suppressed.swap(0, Ordering::AcqRel);
            Some(skipped)
        } else {
            self.suppressed.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    fn log_error(&self, err: cpal::Error) {
        let now_ms = self.started.elapsed().as_millis() as u64;
        if let Some(skipped) = self.record_at_ms(now_ms) {
            if skipped == 0 {
                log::warn!("civ3-audio: output stream error: {err}");
            } else {
                log::warn!(
                    "civ3-audio: output stream error: {err} ({skipped} similar errors suppressed)"
                );
            }
        }
    }
}

/// Computes the preferred `cpal` stream [`BufferSize`] for `sample_rate`.
///
/// On Linux (ALSA), selects a resilient ~45 ms period clamped to the device's
/// advertised range; on macOS/Windows keeps [`BufferSize::Default`] so
/// CoreAudio/WASAPI manage their native RT buffer sizes.
#[cfg(any(target_os = "linux", test))]
fn choose_linux_buffer_size(sample_rate: u32, supported: SupportedBufferSize) -> BufferSize {
    let raw_frames = ((u64::from(sample_rate) * u64::from(TARGET_ALSA_PERIOD_MS)) / 1000) as u32;
    // Round up to a 256-frame boundary (e.g. 2048 at 44.1 kHz, 2304 at 48 kHz).
    let target = raw_frames.max(256).div_ceil(256) * 256;
    match supported {
        SupportedBufferSize::Range { min, max } if min <= max && max > 0 => {
            BufferSize::Fixed(target.clamp(min, max))
        }
        _ => BufferSize::Default,
    }
}

fn choose_output_buffer_size(sample_rate: u32, supported: SupportedBufferSize) -> BufferSize {
    #[cfg(target_os = "linux")]
    {
        choose_linux_buffer_size(sample_rate, supported)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (sample_rate, supported);
        BufferSize::Default
    }
}

fn build_cpal_stream(
    device: &cpal::Device,
    cfg: StreamConfig,
    sample_format: SampleFormat,
    ring: Arc<SpscRing>,
    err_limiter: Arc<StreamErrorLimiter>,
) -> Option<Stream> {
    let channels = cfg.channels as usize;
    let err_fn = move |err| err_limiter.log_error(err);
    match sample_format {
        SampleFormat::F32 => {
            let r = ring;
            device.build_output_stream(
                cfg,
                move |data: &mut [f32], _| {
                    for frame in data.chunks_exact_mut(channels) {
                        let (l, ri) = r.pop_stereo().unwrap_or((0.0, 0.0));
                        if channels == 1 {
                            frame[0] = 0.5 * (l + ri);
                        } else {
                            frame[0] = l;
                            frame[1] = ri;
                            for extra in &mut frame[2..] {
                                *extra = 0.0;
                            }
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        SampleFormat::I16 => {
            let r = ring;
            device.build_output_stream(
                cfg,
                move |data: &mut [i16], _| {
                    for frame in data.chunks_exact_mut(channels) {
                        let (l, ri) = r.pop_stereo().unwrap_or((0.0, 0.0));
                        let li = (l.clamp(-1.0, 1.0) * 32_767.0) as i16;
                        let rii = (ri.clamp(-1.0, 1.0) * 32_767.0) as i16;
                        if channels == 1 {
                            frame[0] = ((i32::from(li) + i32::from(rii)) / 2) as i16;
                        } else {
                            frame[0] = li;
                            frame[1] = rii;
                            for extra in &mut frame[2..] {
                                *extra = 0;
                            }
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        SampleFormat::U16 => {
            let r = ring;
            device.build_output_stream(
                cfg,
                move |data: &mut [u16], _| {
                    for frame in data.chunks_exact_mut(channels) {
                        let (l, ri) = r.pop_stereo().unwrap_or((0.0, 0.0));
                        let lu = ((l.clamp(-1.0, 1.0) * 0.5 + 0.5) * 65_535.0) as u16;
                        let ru = ((ri.clamp(-1.0, 1.0) * 0.5 + 0.5) * 65_535.0) as u16;
                        if channels == 1 {
                            frame[0] = ((u32::from(lu) + u32::from(ru)) / 2) as u16;
                        } else {
                            frame[0] = lu;
                            frame[1] = ru;
                            for extra in &mut frame[2..] {
                                *extra = 32_768;
                            }
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        other => {
            log::warn!("civ3-audio: unsupported output sample format {other:?}");
            return None;
        }
    }
    .ok()
}

struct LiveOutput {
    _stream: Stream,
    sample_rate: u32,
}

fn open_cpal_output(ring: Arc<SpscRing>) -> Option<LiveOutput> {
    if std::env::var_os("CIV3R_AUDIO_NULL").is_some() {
        log::info!("civ3-audio: CIV3R_AUDIO_NULL set; running silent headless clock");
        return None;
    }
    let host = cpal::default_host();
    let device = host.default_output_device()?;
    let default_cfg = device.default_output_config().ok()?;
    let sample_format = default_cfg.sample_format();
    let supported_buf = *default_cfg.buffer_size();
    let mut cfg: StreamConfig = default_cfg.into();
    let sample_rate = cfg.sample_rate;
    let channels = cfg.channels as usize;
    if channels == 0 {
        return None;
    }

    let preferred_buf = choose_output_buffer_size(sample_rate, supported_buf);
    cfg.buffer_size = preferred_buf;
    let err_limiter = Arc::new(StreamErrorLimiter::new());

    let (stream, used_buf) = match build_cpal_stream(
        &device,
        cfg.clone(),
        sample_format,
        ring.clone(),
        err_limiter.clone(),
    ) {
        Some(s) => (s, preferred_buf),
        None if preferred_buf != BufferSize::Default => {
            log::debug!(
                "civ3-audio: build_output_stream with {preferred_buf:?} failed; retrying BufferSize::Default"
            );
            cfg.buffer_size = BufferSize::Default;
            let s = build_cpal_stream(&device, cfg, sample_format, ring, err_limiter)?;
            (s, BufferSize::Default)
        }
        None => return None,
    };

    if let Err(e) = stream.play() {
        log::warn!("civ3-audio: failed starting audio stream: {e}");
        return None;
    }
    log::info!(
        "civ3-audio: opened cpal output ({sample_rate} Hz, {channels} ch, {sample_format:?}, buffer {used_buf:?})"
    );
    Some(LiveOutput { _stream: stream, sample_rate })
}

// ============================================================================
// Double-buffered atomic status snapshot + clock-projected QueryState
// ============================================================================

/// Read-only status table published atomically by the audio thread whenever a
/// discrete state change occurs (start, stop, load complete, seek, pause, fade,
/// EOF, or playlist promotion — never on steady-state render ticks).
#[derive(Clone, Debug, Default)]
struct StatusSnapshot {
    /// Highest command sequence number reflected in this snapshot.
    applied_seq: u64,
    /// Monotonic publish counter (increments only when the pointer flips).
    generation: u64,
    /// Global `WavePause` flag + timestamp when pause began.
    global_wave_paused: bool,
    global_paused_at_us: u64,
    /// Per-handle clock-projectable status records.
    handles: HashMap<SoundHandle, HandleStatus>,
}

/// Writer-side double buffer owned exclusively by the audio thread (`!Sync`).
///
/// Maintains two buffers: `front` (currently published via atomic pointer in
/// `published`) and `back` (staging buffer edited in place via `Arc::make_mut`
/// with delta replay so steady-state flips perform zero table allocations).
struct StatusDoubleBuffer {
    front: Arc<StatusSnapshot>,
    back: Arc<StatusSnapshot>,
    published: Arc<Latest<StatusSnapshot>>,
    applied_seq: Arc<AtomicU64>,
    prev_full_sync: bool,
    prev_deltas: Vec<(SoundHandle, Option<HandleStatus>)>,
    generation: u64,
}

impl StatusDoubleBuffer {
    fn new(published: Arc<Latest<StatusSnapshot>>, applied_seq: Arc<AtomicU64>) -> Self {
        let initial = Arc::new(StatusSnapshot::default());
        published.publish_arc(Arc::clone(&initial));
        Self {
            front: initial,
            back: Arc::new(StatusSnapshot::default()),
            published,
            applied_seq,
            prev_full_sync: false,
            prev_deltas: Vec::new(),
            generation: 0,
        }
    }

    /// Synchronizes only dirty state changes into the staging buffer and swaps
    /// the published atomic pointer once for the whole batch.
    fn sync_from_engine(
        &mut self,
        engine: &mut AudioEngine,
        applied_seq: u64,
        now_us: u64,
    ) {
        if !engine.has_dirty() {
            return;
        }
        let (clear_all, global_refresh, mut dirty_handles) = engine.take_dirty_state();
        let paused = engine.is_wave_paused();
        let paused_at_us = if paused {
            if self.front.global_wave_paused {
                self.front.global_paused_at_us
            } else {
                now_us
            }
        } else {
            0
        };

        let back = Arc::make_mut(&mut self.back);
        let full_sync = clear_all || global_refresh;
        let mut cur_deltas: Vec<(SoundHandle, Option<HandleStatus>)> = Vec::new();

        if full_sync {
            back.handles.clear();
            if !clear_all {
                for (h, st) in engine.snapshot_all_handles(now_us) {
                    back.handles.insert(h, st);
                }
            }
        } else {
            // Bring `back` up to `front` first by replaying the single previous flip.
            if self.prev_full_sync {
                back.handles.clone_from(&self.front.handles);
            } else {
                for &(h, st_opt) in &self.prev_deltas {
                    match st_opt {
                        Some(st) => {
                            back.handles.insert(h, st);
                        }
                        None => {
                            back.handles.remove(&h);
                        }
                    }
                }
            }
            dirty_handles.sort_unstable();
            dirty_handles.dedup();
            cur_deltas.reserve(dirty_handles.len());
            for h in dirty_handles {
                let st_opt = engine.snapshot_handle(h, now_us);
                match st_opt {
                    Some(st) => {
                        back.handles.insert(h, st);
                    }
                    None => {
                        back.handles.remove(&h);
                    }
                }
                cur_deltas.push((h, st_opt));
            }
        }

        self.generation = self.generation.wrapping_add(1);
        back.applied_seq = applied_seq;
        back.generation = self.generation;
        back.global_wave_paused = paused;
        back.global_paused_at_us = paused_at_us;

        std::mem::swap(&mut self.front, &mut self.back);
        self.prev_full_sync = full_sync;
        self.prev_deltas = cur_deltas;

        // Publish order: atomic pointer swap FIRST, then `applied_seq` Release store.
        // Readers load `pending` FIRST, then `published` SECOND -> strictly linearizable.
        self.published.publish_arc(Arc::clone(&self.front));
        self.applied_seq.store(applied_seq, Ordering::Release);
    }
}

/// Lightweight overlay entry for fire-and-forget commands whose `seq` has not
/// yet been absorbed into `StatusSnapshot`. Pruned automatically once
/// `applied_seq >= entry.seq`.
#[derive(Clone, Copy, Debug)]
struct PendingEntry {
    seq: u64,
    handle: SoundHandle,
    op: PendingOp,
}

#[derive(Clone, Copy, Debug)]
enum PendingOp {
    Created { kind: SoundKind },
    Deleted,
    ReleaseAll,
    StopAll,
    GlobalPause { at_us: u64 },
    GlobalResume,
    SetFlags { flags: u32 },
    SetLoop { looping: bool },
    Loaded { is_ds_buffer: bool, duration_ms: u32 },
    Unloaded,
    PlaylistQueued { duration_ms: u32 },
    Play { at_us: u64 },
    Stop,
    TogglePause { at_us: u64 },
    Seek { pos_ms: u32, at_us: u64 },
    SetFadeMs { ms: u32 },
    FadeOut { at_us: u64 },
    FadeToZero { at_us: u64, duration_ms: u32 },
}

fn apply_pending_entry(
    entry: PendingEntry,
    target_handle: SoundHandle,
    status: &mut Option<HandleStatus>,
    global_paused: &mut bool,
    global_paused_at_us: &mut u64,
) {
    match entry.op {
        PendingOp::ReleaseAll => {
            *status = None;
            *global_paused = false;
            *global_paused_at_us = 0;
            return;
        }
        PendingOp::StopAll => {
            if let Some(st) = status.as_mut() {
                st.playing = false;
                st.stop_at_us = 0;
            }
            return;
        }
        PendingOp::GlobalPause { at_us } => {
            if !*global_paused {
                *global_paused = true;
                *global_paused_at_us = at_us;
            }
            return;
        }
        PendingOp::GlobalResume => {
            *global_paused = false;
            *global_paused_at_us = 0;
            return;
        }
        _ => {}
    }
    if entry.handle != target_handle {
        return;
    }
    match entry.op {
        PendingOp::Created { kind } => {
            *status = Some(HandleStatus::new(kind));
        }
        PendingOp::Deleted => {
            *status = None;
        }
        PendingOp::SetFlags { flags } => {
            if let Some(st) = status.as_mut() {
                st.wave_flags |= flags;
                if (flags & 2) != 0 {
                    st.looping = true;
                    st.state_flags |= STATE_LOOPING;
                    if st.playing {
                        st.stop_at_us = u64::MAX;
                    }
                }
            }
        }
        PendingOp::SetLoop { looping } => {
            if let Some(st) = status.as_mut() {
                st.looping = looping;
                if looping {
                    st.state_flags |= STATE_LOOPING;
                    if st.playing {
                        st.stop_at_us = u64::MAX;
                    }
                } else {
                    st.state_flags &= !STATE_LOOPING;
                }
            }
        }
        PendingOp::Loaded { is_ds_buffer, duration_ms } => {
            if let Some(st) = status.as_mut() {
                st.state_flags |= STATE_LOADED;
                if is_ds_buffer {
                    st.state_flags |= STATE_DS_BUFFER;
                } else {
                    st.state_flags &= !STATE_DS_BUFFER;
                }
                st.duration_ms = duration_ms;
                st.report_position = is_ds_buffer;
                st.playing = false;
                st.paused = false;
                st.base_pos_ms = 0;
                st.stop_at_us = 0;
            }
        }
        PendingOp::Unloaded => {
            if let Some(st) = status.as_mut() {
                st.state_flags &= !(STATE_LOADED | STATE_DS_BUFFER);
                st.duration_ms = 0;
                st.playing = false;
                st.paused = false;
                st.report_position = false;
                st.base_pos_ms = 0;
                st.stop_at_us = 0;
            }
        }
        PendingOp::PlaylistQueued { duration_ms } => {
            if let Some(st) = status.as_mut() {
                st.state_flags |= STATE_LOADED | STATE_PLAYLIST_HAS_QUEUED;
                if !st.playing && st.duration_ms == 0 {
                    st.duration_ms = duration_ms;
                }
            }
        }
        PendingOp::Play { at_us } => {
            if let Some(st) = status.as_mut() {
                if st.kind == SoundKind::Playlist {
                    if !st.playing && (st.state_flags & STATE_PLAYLIST_HAS_QUEUED) != 0 {
                        st.state_flags &= !STATE_PLAYLIST_HAS_QUEUED;
                        st.playing = true;
                        st.paused = false;
                        st.looping = true;
                        st.report_position = true;
                        st.anchor_us = at_us;
                        st.base_pos_ms = 0;
                        st.stop_at_us = u64::MAX;
                    } else if (st.state_flags & STATE_LOADED) != 0 {
                        st.playing = true;
                        st.paused = false;
                        st.anchor_us = at_us;
                        st.stop_at_us = u64::MAX;
                    }
                } else if (st.state_flags & STATE_LOADED) != 0 {
                    let was_playing = st.playing;
                    st.playing = true;
                    st.paused = false;
                    if !was_playing {
                        st.base_pos_ms = 0;
                        st.anchor_us = at_us;
                    }
                    st.stop_at_us = if st.kind == SoundKind::Amb || st.looping || st.duration_ms == 0 {
                        u64::MAX
                    } else {
                        let rem_ms = st.duration_ms.saturating_sub(st.base_pos_ms);
                        at_us.saturating_add(u64::from(rem_ms).saturating_mul(1000))
                    };
                }
            }
        }
        PendingOp::Stop => {
            if let Some(st) = status.as_mut() {
                st.playing = false;
                st.paused = false;
                st.stop_at_us = 0;
            }
        }
        PendingOp::TogglePause { at_us } => {
            if let Some(st) = status.as_mut() {
                if st.report_position && st.playing {
                    if !st.paused {
                        let cur_pos = st.project(at_us, *global_paused, *global_paused_at_us).extra2;
                        st.base_pos_ms = cur_pos;
                        st.anchor_us = at_us;
                        st.paused = true;
                        st.stop_at_us = u64::MAX;
                    } else {
                        st.paused = false;
                        st.anchor_us = at_us;
                        st.stop_at_us = if st.looping || st.duration_ms == 0 {
                            u64::MAX
                        } else {
                            let rem_ms = st.duration_ms.saturating_sub(st.base_pos_ms);
                            at_us.saturating_add(u64::from(rem_ms).saturating_mul(1000))
                        };
                    }
                }
            }
        }
        PendingOp::Seek { pos_ms, at_us } => {
            if let Some(st) = status.as_mut() {
                if st.report_position {
                    st.base_pos_ms = pos_ms;
                    st.anchor_us = at_us;
                    if st.playing && !st.paused && !st.looping && st.duration_ms > 0 {
                        let rem_ms = st.duration_ms.saturating_sub(pos_ms);
                        st.stop_at_us = at_us.saturating_add(u64::from(rem_ms).saturating_mul(1000));
                    }
                }
            }
        }
        PendingOp::SetFadeMs { ms } => {
            if let Some(st) = status.as_mut() {
                st.fade_out_ms = ms;
            }
        }
        PendingOp::FadeOut { at_us } => {
            if let Some(st) = status.as_mut() {
                if st.playing {
                    let ms = match st.kind {
                        SoundKind::Playlist => 2000,
                        _ => st.fade_out_ms.max(1),
                    };
                    let end_us = at_us.saturating_add(u64::from(ms).saturating_mul(1000));
                    st.stop_at_us = st.stop_at_us.min(end_us);
                }
            }
        }
        PendingOp::FadeToZero { at_us, duration_ms } => {
            if let Some(st) = status.as_mut() {
                if st.playing {
                    let end_us = at_us.saturating_add(u64::from(duration_ms.max(1)).saturating_mul(1000));
                    st.stop_at_us = st.stop_at_us.min(end_us);
                }
            }
        }
        PendingOp::ReleaseAll
        | PendingOp::StopAll
        | PendingOp::GlobalPause { .. }
        | PendingOp::GlobalResume => {}
    }
}

// ============================================================================
// Commands & Async Runtime
// ============================================================================

enum Job {
    Wire {
        seq: u64,
        hdr: RequestHeader,
        path: String,
        reply: Option<std::sync::mpsc::SyncSender<Response>>,
    },
    BeginLoad {
        seq: u64,
        handle: SoundHandle,
        load_gen: u64,
        host_path: PathBuf,
        extra_flags: u32,
        want_stream: bool,
        probed_duration_ms: u32,
    },
    BeginPlaylistQueue {
        seq: u64,
        pl_handle: SoundHandle,
        track_handle: SoundHandle,
        load_gen: u64,
        host_path: PathBuf,
        probed_duration_ms: u32,
    },
}

impl Job {
    fn seq(&self) -> u64 {
        match self {
            Job::Wire { seq, .. }
            | Job::BeginLoad { seq, .. }
            | Job::BeginPlaylistQueue { seq, .. } => *seq,
        }
    }
}

struct SharedRuntimeState {
    epoch: Instant,
    alive: AtomicBool,
    headless: bool,
    published: Arc<Latest<StatusSnapshot>>,
    applied_seq: Arc<AtomicU64>,
}

impl SharedRuntimeState {
    #[inline]
    fn now_us(&self) -> u64 {
        self.epoch.elapsed().as_micros() as u64
    }
}

/// Handle to the running lock-free single-threaded async native audio engine.
pub struct AudioRuntime {
    cmd_tx: mpsc::UnboundedSender<Job>,
    shared: Arc<SharedRuntimeState>,
    next_handle: AtomicU32,
    next_seq: AtomicU64,
    next_load_gen: AtomicU64,
    pending: Latest<Vec<PendingEntry>>,
}

impl AudioRuntime {
    /// Starts the async audio engine thread (`civ3r-native-audio`) and the
    /// background asset decoder thread (`civ3r-native-audio-decode`) using
    /// `spawn` (pass `wine_unixlib::unix::spawn_native` inside `civ3r-native`,
    /// or `std::thread::Builder` in standalone tests).
    pub fn start<F>(spawn: F) -> std::io::Result<Self>
    where
        F: Fn(&str, Box<dyn FnOnce() + Send + 'static>) -> std::io::Result<()>,
    {
        Self::start_with_mode(spawn, false)
    }

    #[cfg(test)]
    fn start_headless<F>(spawn: F) -> std::io::Result<Self>
    where
        F: Fn(&str, Box<dyn FnOnce() + Send + 'static>) -> std::io::Result<()>,
    {
        Self::start_with_mode(spawn, true)
    }

    fn start_with_mode<F>(spawn: F, headless: bool) -> std::io::Result<Self>
    where
        F: Fn(&str, Box<dyn FnOnce() + Send + 'static>) -> std::io::Result<()>,
    {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Job>();
        let (decode_tx, decode_rx) = mpsc::unbounded_channel::<DecodeJob>();
        let (decoded_tx, decoded_rx) = mpsc::unbounded_channel::<DecodedResult>();

        let published = Arc::new(Latest::new());
        let applied_seq = Arc::new(AtomicU64::new(0));
        let shared = Arc::new(SharedRuntimeState {
            epoch: Instant::now(),
            alive: AtomicBool::new(true),
            headless,
            published: Arc::clone(&published),
            applied_seq: Arc::clone(&applied_seq),
        });

        let sh_audio = Arc::clone(&shared);
        spawn(
            "civ3r-native-audio",
            Box::new(move || audio_thread_main(cmd_rx, decode_tx, decoded_rx, sh_audio)),
        )?;

        spawn(
            "civ3r-native-audio-decode",
            Box::new(move || decode_thread_main(decode_rx, decoded_tx)),
        )?;

        Ok(Self {
            cmd_tx,
            shared,
            next_handle: AtomicU32::new(1),
            next_seq: AtomicU64::new(0),
            next_load_gen: AtomicU64::new(0),
            pending: Latest::new(),
        })
    }

    #[inline]
    fn alloc_handle(&self) -> SoundHandle {
        loop {
            let cur = self.next_handle.load(Ordering::Relaxed);
            // Reserve high bit 0 (1..=0x7FFF_FFFF) for caller-allocated handles.
            let h = if cur == 0 || (cur & 0x8000_0000) != 0 { 1 } else { cur };
            let next = if h == 0x7FFF_FFFF { 1 } else { h + 1 };
            if self
                .next_handle
                .compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return h;
            }
        }
    }

    #[inline]
    fn next_seq(&self) -> u64 {
        self.next_seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    #[inline]
    fn next_load_gen(&self) -> u64 {
        self.next_load_gen.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn push_pending(&self, entry: PendingEntry) {
        let done = self.shared.applied_seq.load(Ordering::Acquire);
        self.pending.update(|cur| {
            let mut v = match cur {
                Some(slice) => slice.iter().copied().filter(|e| e.seq > done).collect(),
                None => Vec::with_capacity(8),
            };
            v.push(entry);
            Some(v)
        });
    }

    /// Wait-free linearizable read of `handle`'s current status and global
    /// pause state (never communicates with the audio thread).
    fn resolve_handle_state(
        &self,
        handle: SoundHandle,
    ) -> (Option<HandleStatus>, bool, u64) {
        // Load `pending` FIRST, then `published` SECOND (reverse of writer order)
        // so no transition can fall between the snapshot and the pending overlay.
        let pending = self.pending.load();
        let snap = self.shared.published.load();

        let (snap_seq, mut status, mut global_paused, mut global_paused_at_us) = match &snap {
            Some(s) => (
                s.applied_seq,
                s.handles.get(&handle).copied(),
                s.global_wave_paused,
                s.global_paused_at_us,
            ),
            None => (0, None, false, 0),
        };

        if let Some(entries) = pending {
            for &entry in entries.iter() {
                if entry.seq > snap_seq {
                    apply_pending_entry(
                        entry,
                        handle,
                        &mut status,
                        &mut global_paused,
                        &mut global_paused_at_us,
                    );
                }
            }
        }
        (status, global_paused, global_paused_at_us)
    }

    /// Returns the monotonic snapshot publish count (useful for verifying zero
    /// periodic churn during steady-state playback).
    pub fn snapshot_generation(&self) -> u64 {
        self.shared.published.load().map_or(0, |s| s.generation)
    }

    /// Dispatches one wire request from the game thread.
    ///
    /// - Hot-path commands (`CreateSound`, `Play`, `PlayInstance`, `StartVoice`,
    ///   `Stop`, `Fade*`, `Pause`, `Set*`, `Amb*`, `WaveOpen`, etc.) are
    ///   100% fire-and-forget.
    /// - `QueryState` (`is_playing`, `get_position_ms`, playlist flags) is
    ///   answered wait-free from the double-buffered atomic snapshot + host
    ///   monotonic clock.
    /// - `Load` and `PlaylistQueueTrack` perform only cheap path resolution and
    ///   header-only validation synchronously, then decode off the game thread
    ///   on the dedicated background decoder thread.
    /// - Only rare recording/teardown barriers (`WaveStartRecord`,
    ///   `WaveStopRecord`, `WaveClose`, `ReleaseAll`) wait synchronously, waking
    ///   the async audio task immediately.
    pub fn dispatch(&self, hdr: RequestHeader, path: &str) -> Response {
        if !self.shared.alive.load(Ordering::Acquire) {
            return Response { snderr: SNDERR_NO_OBJECT, ..Default::default() };
        }
        let Some(op) = Op::from_u32(hdr.op) else {
            return Response { snderr: SNDERR_INVALID_PARAM, ..Default::default() };
        };
        let now_us = self.shared.now_us();

        match op {
            // 1. Wait-free clock-projected status query (zero audio-thread interaction).
            Op::QueryState => {
                let (status, global_paused, global_paused_at_us) =
                    self.resolve_handle_state(hdr.handle);
                match status {
                    Some(st) => st.project(now_us, global_paused, global_paused_at_us),
                    None => Response::default(),
                }
            }

            // 2. Caller-side handle allocation, fire-and-forget creation.
            Op::CreateSound => {
                let kind = SoundKind::from_u32(hdr.a as u32).unwrap_or(SoundKind::Wave);
                let handle = if hdr.handle != 0 { hdr.handle } else { self.alloc_handle() };
                let mut stamped = hdr;
                stamped.handle = handle;
                let seq = self.next_seq();
                self.push_pending(PendingEntry { seq, handle, op: PendingOp::Created { kind } });
                let _ = self.cmd_tx.send(Job::Wire {
                    seq,
                    hdr: stamped,
                    path: String::new(),
                    reply: None,
                });
                Response { snderr: SNDERR_OK, value: handle, ..Default::default() }
            }

            // 3. Fire-and-forget playback with wait-free loaded/object check.
            Op::Play | Op::PlayInstance | Op::StartVoice => {
                let (status, _, _) = self.resolve_handle_state(hdr.handle);
                let Some(st) = status else {
                    return Response { snderr: SNDERR_NO_OBJECT, ..Default::default() };
                };
                if matches!(
                    st.kind,
                    SoundKind::Wave | SoundKind::Type3 | SoundKind::Amb | SoundKind::Playlist
                ) && (st.state_flags & STATE_LOADED) == 0
                {
                    return Response { snderr: SNDERR_UNLOADED, ..Default::default() };
                }
                let seq = self.next_seq();
                self.push_pending(PendingEntry {
                    seq,
                    handle: hdr.handle,
                    op: PendingOp::Play { at_us: now_us },
                });
                let _ = self.cmd_tx.send(Job::Wire {
                    seq,
                    hdr,
                    path: String::new(),
                    reply: None,
                });
                Response { snderr: SNDERR_OK, value: 0, ..Default::default() }
            }

            // 4. Cheap path + header validation synchronously, decode off-thread.
            Op::Load => self.dispatch_load(hdr, path),
            Op::PlaylistQueueTrack => self.dispatch_playlist_queue(hdr, path),

            // 5. Rare teardown & WAV capture barriers (sync wait, instant async wake).
            Op::WaveStartRecord | Op::WaveStopRecord | Op::WaveClose | Op::ReleaseAll => {
                let seq = self.next_seq();
                match op {
                    Op::WaveClose => {
                        self.push_pending(PendingEntry { seq, handle: 0, op: PendingOp::StopAll });
                    }
                    Op::ReleaseAll => {
                        self.push_pending(PendingEntry {
                            seq,
                            handle: 0,
                            op: PendingOp::ReleaseAll,
                        });
                    }
                    _ => {}
                }
                let (tx, rx) = std::sync::mpsc::sync_channel(1);
                if self
                    .cmd_tx
                    .send(Job::Wire { seq, hdr, path: path.to_owned(), reply: Some(tx) })
                    .is_err()
                {
                    return Response { snderr: SNDERR_NO_OBJECT, ..Default::default() };
                }
                rx.recv_timeout(Duration::from_secs(5))
                    .unwrap_or(Response { snderr: SNDERR_OK, ..Default::default() })
            }

            // 6. All remaining control & setter ops: 100% fire-and-forget.
            _ => {
                let seq = self.next_seq();
                match op {
                    Op::DeleteSound => {
                        self.push_pending(PendingEntry {
                            seq,
                            handle: hdr.handle,
                            op: PendingOp::Deleted,
                        });
                    }
                    Op::Unload => {
                        self.push_pending(PendingEntry {
                            seq,
                            handle: hdr.handle,
                            op: PendingOp::Unloaded,
                        });
                    }
                    Op::Stop => {
                        self.push_pending(PendingEntry {
                            seq,
                            handle: hdr.handle,
                            op: PendingOp::Stop,
                        });
                    }
                    Op::Pause => {
                        self.push_pending(PendingEntry {
                            seq,
                            handle: hdr.handle,
                            op: PendingOp::TogglePause { at_us: now_us },
                        });
                    }
                    Op::SetLoop => {
                        self.push_pending(PendingEntry {
                            seq,
                            handle: hdr.handle,
                            op: PendingOp::SetLoop { looping: hdr.a != 0 },
                        });
                    }
                    Op::SetFlags => {
                        self.push_pending(PendingEntry {
                            seq,
                            handle: hdr.handle,
                            op: PendingOp::SetFlags { flags: hdr.a as u32 },
                        });
                    }
                    Op::SetPositionMs => {
                        self.push_pending(PendingEntry {
                            seq,
                            handle: hdr.handle,
                            op: PendingOp::Seek {
                                pos_ms: hdr.a.max(0) as u32,
                                at_us: now_us,
                            },
                        });
                    }
                    Op::SetFadeMs => {
                        self.push_pending(PendingEntry {
                            seq,
                            handle: hdr.handle,
                            op: PendingOp::SetFadeMs { ms: hdr.a.max(0) as u32 },
                        });
                    }
                    Op::FadeOut => {
                        self.push_pending(PendingEntry {
                            seq,
                            handle: hdr.handle,
                            op: PendingOp::FadeOut { at_us: now_us },
                        });
                    }
                    Op::Fade if hdr.a == 0 => {
                        self.push_pending(PendingEntry {
                            seq,
                            handle: hdr.handle,
                            op: PendingOp::FadeToZero {
                                at_us: now_us,
                                duration_ms: hdr.c.max(1) as u32,
                            },
                        });
                    }
                    Op::WavePause => {
                        self.push_pending(PendingEntry {
                            seq,
                            handle: 0,
                            op: PendingOp::GlobalPause { at_us: now_us },
                        });
                    }
                    Op::WaveResume => {
                        self.push_pending(PendingEntry {
                            seq,
                            handle: 0,
                            op: PendingOp::GlobalResume,
                        });
                    }
                    _ => {}
                }
                let _ = self.cmd_tx.send(Job::Wire {
                    seq,
                    hdr,
                    path: path.to_owned(),
                    reply: None,
                });
                Response::default()
            }
        }
    }

    fn dispatch_load(&self, hdr: RequestHeader, raw_path: &str) -> Response {
        let (status, _, _) = self.resolve_handle_state(hdr.handle);
        let Some(st) = status else {
            return Response { snderr: SNDERR_OK, ..Default::default() };
        };
        if !matches!(st.kind, SoundKind::Wave | SoundKind::Type3 | SoundKind::Amb) {
            return Response { snderr: SNDERR_OK, ..Default::default() };
        }
        let Some(host_path) = resolve_game_path(raw_path, None) else {
            log::debug!("civ3-audio: file not found: {raw_path}");
            return Response { snderr: SNDERR_FILE_OPEN, ..Default::default() };
        };

        let extra_flags = hdr.a as u32;
        if st.kind == SoundKind::Amb {
            if let Err(err) = probe_amb_header(&host_path) {
                return Response {
                    snderr: match err {
                        ProbeError::FileOpen(_) => SNDERR_FILE_OPEN,
                        ProbeError::InvalidFormat(_) => SNDERR_INVALID_FORMAT,
                    },
                    ..Default::default()
                };
            }
            let seq = self.next_seq();
            let load_gen = self.next_load_gen();
            self.push_pending(PendingEntry {
                seq,
                handle: hdr.handle,
                op: PendingOp::Loaded { is_ds_buffer: false, duration_ms: 0 },
            });
            let _ = self.cmd_tx.send(Job::BeginLoad {
                seq,
                handle: hdr.handle,
                load_gen,
                host_path,
                extra_flags,
                want_stream: false,
                probed_duration_ms: 0,
            });
            return Response::default();
        }

        let flags = st.wave_flags | extra_flags;
        let is_mp3 = host_path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mp3"));
        let want_stream = is_mp3 || (flags & 0x100) != 0;

        let probe = match probe_audio_header(&host_path, is_mp3) {
            Ok(p) => p,
            Err(ProbeError::FileOpen(_)) => {
                return Response { snderr: SNDERR_FILE_OPEN, ..Default::default() };
            }
            Err(ProbeError::InvalidFormat(msg)) => {
                log::warn!("civ3-audio: invalid audio header for {}: {msg}", host_path.display());
                return Response { snderr: SNDERR_INVALID_FORMAT, ..Default::default() };
            }
        };

        let seq = self.next_seq();
        let load_gen = self.next_load_gen();
        self.push_pending(PendingEntry {
            seq,
            handle: hdr.handle,
            op: PendingOp::Loaded { is_ds_buffer: want_stream, duration_ms: probe.duration_ms },
        });
        let _ = self.cmd_tx.send(Job::BeginLoad {
            seq,
            handle: hdr.handle,
            load_gen,
            host_path,
            extra_flags,
            want_stream,
            probed_duration_ms: probe.duration_ms,
        });
        Response {
            snderr: SNDERR_OK,
            value: u32::from(want_stream),
            extra1: probe.duration_ms,
            extra2: 0,
        }
    }

    fn dispatch_playlist_queue(&self, hdr: RequestHeader, raw_path: &str) -> Response {
        let (status, _, _) = self.resolve_handle_state(hdr.handle);
        let Some(st) = status else {
            return Response { snderr: SNDERR_NO_OBJECT, ..Default::default() };
        };
        if st.kind != SoundKind::Playlist {
            return Response { snderr: SNDERR_NO_OBJECT, ..Default::default() };
        }
        if (st.state_flags & STATE_PLAYLIST_HAS_QUEUED) != 0 {
            return Response { snderr: SNDERR_ALREADY_LOADED, ..Default::default() };
        }
        let Some(host_path) = resolve_game_path(raw_path, None) else {
            log::debug!("civ3-audio: playlist track not found: {raw_path}");
            return Response { snderr: SNDERR_FILE_OPEN, ..Default::default() };
        };
        let is_mp3 = host_path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mp3"));
        let probe = match probe_audio_header(&host_path, is_mp3) {
            Ok(p) => p,
            Err(ProbeError::FileOpen(_)) => {
                return Response { snderr: SNDERR_FILE_OPEN, ..Default::default() };
            }
            Err(ProbeError::InvalidFormat(msg)) => {
                log::warn!("civ3-audio: invalid playlist track {}: {msg}", host_path.display());
                return Response { snderr: SNDERR_INVALID_FORMAT, ..Default::default() };
            }
        };

        let track_handle = self.alloc_handle();
        let seq = self.next_seq();
        let load_gen = self.next_load_gen();
        self.push_pending(PendingEntry {
            seq,
            handle: hdr.handle,
            op: PendingOp::PlaylistQueued { duration_ms: probe.duration_ms },
        });
        let _ = self.cmd_tx.send(Job::BeginPlaylistQueue {
            seq,
            pl_handle: hdr.handle,
            track_handle,
            load_gen,
            host_path,
            probed_duration_ms: probe.duration_ms,
        });
        Response {
            snderr: SNDERR_OK,
            value: track_handle,
            extra1: probe.duration_ms,
            extra2: 0,
        }
    }
}

impl Drop for AudioRuntime {
    fn drop(&mut self) {
        self.shared.alive.store(false, Ordering::Release);
    }
}

// ============================================================================
// Single-threaded async audio thread & background decoder thread
// ============================================================================

fn decode_thread_main(
    mut rx: mpsc::UnboundedReceiver<DecodeJob>,
    tx: mpsc::UnboundedSender<DecodedResult>,
) {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread().build() else {
        return;
    };
    rt.block_on(async move {
        let mut wav_cache = HashMap::new();
        while let Some(job) = rx.recv().await {
            let res = execute_decode_job(job, &mut wav_cache);
            if tx.send(res).is_err() {
                break;
            }
        }
    });
}

enum WakeEvent {
    Cmd(Option<Job>),
    Decoded(Option<DecodedResult>),
    RefillTick,
}

fn audio_thread_main(
    cmd_rx: mpsc::UnboundedReceiver<Job>,
    decode_tx: mpsc::UnboundedSender<DecodeJob>,
    decoded_rx: mpsc::UnboundedReceiver<DecodedResult>,
    shared: Arc<SharedRuntimeState>,
) {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_time().build() {
        Ok(rt) => rt,
        Err(e) => {
            log::error!("civ3-audio: failed creating current-thread async runtime: {e}");
            shared.alive.store(false, Ordering::Release);
            return;
        }
    };
    rt.block_on(audio_async_loop(cmd_rx, decode_tx, decoded_rx, shared));
}

#[allow(clippy::too_many_arguments)]
fn process_job(
    job: Job,
    engine: &mut AudioEngine,
    decode_tx: &mpsc::UnboundedSender<DecodeJob>,
    ring: &Arc<SpscRing>,
    output: &mut Option<LiveOutput>,
    resampler: &mut Option<HostResampler>,
    wave_active: &mut bool,
    headless: bool,
    headless_clock: &mut Instant,
    refill_timer: &mut tokio::time::Interval,
    last_seq: &mut u64,
) {
    *last_seq = (*last_seq).max(job.seq());
    match job {
        Job::Wire { hdr, path, reply, .. } => {
            let op = Op::from_u32(hdr.op);
            let resp = engine.handle_request(hdr, &path);
            if let Some(tx) = reply {
                let _ = tx.send(resp);
            }
            match op {
                Some(Op::WaveOpen) => {
                    let was_active = *wave_active;
                    *wave_active = true;
                    if !headless && output.is_none() {
                        *output = open_cpal_output(Arc::clone(ring));
                        *resampler = output.as_ref().and_then(|o| {
                            if o.sample_rate != MIX_SAMPLE_RATE {
                                HostResampler::new(o.sample_rate)
                            } else {
                                None
                            }
                        });
                    }
                    *headless_clock = Instant::now();
                    if !was_active {
                        refill_timer.reset();
                    }
                }
                Some(Op::WaveClose | Op::ReleaseAll) => {
                    *wave_active = false;
                    *output = None;
                    *resampler = None;
                }
                _ => {}
            }
        }
        Job::BeginLoad {
            handle,
            load_gen,
            host_path,
            extra_flags,
            want_stream,
            probed_duration_ms,
            ..
        } => {
            if let Some(djob) = engine.begin_async_load(
                handle,
                load_gen,
                host_path,
                extra_flags,
                want_stream,
                probed_duration_ms,
            ) {
                let _ = decode_tx.send(djob);
            }
        }
        Job::BeginPlaylistQueue {
            pl_handle,
            track_handle,
            load_gen,
            host_path,
            probed_duration_ms,
            ..
        } => {
            if let Some(djob) = engine.begin_async_playlist_queue(
                pl_handle,
                track_handle,
                load_gen,
                host_path,
                probed_duration_ms,
            ) {
                let _ = decode_tx.send(djob);
            }
        }
    }
}

async fn audio_async_loop(
    mut cmd_rx: mpsc::UnboundedReceiver<Job>,
    decode_tx: mpsc::UnboundedSender<DecodeJob>,
    mut decoded_rx: mpsc::UnboundedReceiver<DecodedResult>,
    shared: Arc<SharedRuntimeState>,
) {
    let mut engine = AudioEngine::new();
    if let Some(dump_path) = std::env::var_os("CIV3R_AUDIO_DUMP") {
        let p = PathBuf::from(dump_path);
        match engine.start_recording(&p) {
            Ok(()) => log::info!("civ3-audio: dumping mix to {}", p.display()),
            Err(e) => log::warn!("civ3-audio: CIV3R_AUDIO_DUMP failed ({}): {e}", p.display()),
        }
    }

    let ring = Arc::new(SpscRing::new(SPSC_RING_CAPACITY));
    let mut output: Option<LiveOutput> = None;
    let mut resampler: Option<HostResampler> = None;
    let mut wave_active = false;

    let mut mix_f32 = vec![0.0_f32; 512 * 2];
    let mut scratch_i16 = Vec::with_capacity(512 * 2);
    let target_buffered_frames =
        |rate: u32| -> usize { ((u64::from(rate) * TARGET_RING_BUFFER_MS) / 1000) as usize };
    let mut headless_clock = Instant::now();
    let mut headless_frac_frames = 0.0_f64;

    let mut snapshot_buf =
        StatusDoubleBuffer::new(Arc::clone(&shared.published), Arc::clone(&shared.applied_seq));
    let mut last_seq = 0u64;

    let mut refill_timer = tokio::time::interval(Duration::from_millis(ACTIVE_REFILL_INTERVAL_MS));
    refill_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        if !shared.alive.load(Ordering::Relaxed) {
            break;
        }

        let event = std::future::poll_fn(|cx| {
            // 1. Incoming game commands always take priority and wake immediately.
            if let Poll::Ready(opt) = cmd_rx.poll_recv(cx) {
                return Poll::Ready(WakeEvent::Cmd(opt));
            }
            // 2. Completed background decodes next.
            if let Poll::Ready(opt) = decoded_rx.poll_recv(cx) {
                return Poll::Ready(WakeEvent::Decoded(opt));
            }
            // 3. Only arm the refill interval while audio output is open & active.
            if wave_active {
                if let Poll::Ready(_) = refill_timer.poll_tick(cx) {
                    return Poll::Ready(WakeEvent::RefillTick);
                }
            }
            Poll::Pending
        })
        .await;

        match event {
            WakeEvent::Cmd(Some(job)) => {
                process_job(
                    job,
                    &mut engine,
                    &decode_tx,
                    &ring,
                    &mut output,
                    &mut resampler,
                    &mut wave_active,
                    shared.headless,
                    &mut headless_clock,
                    &mut refill_timer,
                    &mut last_seq,
                );
            }
            WakeEvent::Cmd(None) => break,
            WakeEvent::Decoded(Some(res)) => {
                engine.finish_async_decode(res);
            }
            WakeEvent::Decoded(None) => {}
            WakeEvent::RefillTick => {}
        }

        // Drain any remaining queued commands and decoded completions in one batch.
        while let Ok(job) = cmd_rx.try_recv() {
            process_job(
                job,
                &mut engine,
                &decode_tx,
                &ring,
                &mut output,
                &mut resampler,
                &mut wave_active,
                shared.headless,
                &mut headless_clock,
                &mut refill_timer,
                &mut last_seq,
            );
        }
        while let Ok(res) = decoded_rx.try_recv() {
            engine.finish_async_decode(res);
        }

        // Refill SPSC ring (or advance headless real-time clock) while active.
        if wave_active {
            if let Some(live) = &output {
                let target_samples = target_buffered_frames(live.sample_rate) * 2;
                let mut iterations = 0;
                while ring.len() < target_samples && iterations < MAX_REFILL_CHUNKS_PER_WAKEUP {
                    iterations += 1;
                    engine.render_f32(&mut mix_f32, &mut scratch_i16);
                    if let Some(rs) = &mut resampler {
                        let converted = rs.process(&mix_f32);
                        ring.push_slice(converted);
                    } else {
                        ring.push_slice(&mix_f32);
                    }
                }
            } else {
                // Silent headless fallback: advance engine at real-time 44100 Hz so
                // `is_playing`, fades, `.amb` sequencers, and playlists advance accurately.
                let dt = headless_clock.elapsed();
                headless_clock = Instant::now();
                headless_frac_frames += dt.as_secs_f64() * f64::from(MIX_SAMPLE_RATE);
                let mut todo = (headless_frac_frames as usize).min(4410);
                headless_frac_frames -= todo as f64;
                while todo > 0 {
                    let chunk = todo.min(512);
                    scratch_i16.resize(chunk * 2, 0);
                    engine.render_frames(&mut scratch_i16);
                    todo -= chunk;
                }
            }
        }

        // Only edit & flip the double-buffered status pointer when discrete state changed.
        if engine.has_dirty() {
            let now_us = shared.now_us();
            snapshot_buf.sync_from_engine(&mut engine, last_seq, now_us);
        }
    }

    engine.stop_recording();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mixer::make_wav_header;
    use crate::wire::STATE_PLAYING;

    fn start_test_runtime() -> AudioRuntime {
        AudioRuntime::start_headless(|name, f| {
            std::thread::Builder::new().name(name.to_owned()).spawn(f).map(drop)
        })
        .expect("start AudioRuntime")
    }

    fn write_test_wav(path: &std::path::Path, sample_rate: u32, samples: &[i16]) {
        let data_bytes = (samples.len() * 2) as u32;
        let mut bytes = make_wav_header(sample_rate, 1, data_bytes).to_vec();
        bytes.extend_from_slice(bytemuck::cast_slice(samples));
        std::fs::write(path, bytes).expect("write test wav");
    }

    fn unique_tmp_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(1);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("civ3-audio-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wait_ms(ms: u64) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("test timer runtime");
        rt.block_on(async {
            tokio::time::sleep(Duration::from_millis(ms)).await;
        });
    }

    #[test]
    fn spsc_ring_transfers_stereo_samples_in_order() {
        let ring = SpscRing::new(1024);
        let input: Vec<f32> = (0..200).map(|i| i as f32 * 0.001).collect();
        assert_eq!(ring.push_slice(&input), 200);
        assert_eq!(ring.len(), 200);
        for i in 0..100 {
            let (l, r) = ring.pop_stereo().unwrap();
            assert_eq!(l, (i * 2) as f32 * 0.001);
            assert_eq!(r, (i * 2 + 1) as f32 * 0.001);
        }
        assert!(ring.pop_stereo().is_none());
    }

    #[test]
    fn linux_buffer_size_targets_45ms_clamped_to_device_range() {
        assert_eq!(
            choose_linux_buffer_size(48_000, SupportedBufferSize::Range { min: 64, max: 16_384 }),
            BufferSize::Fixed(2_304)
        );
        assert_eq!(
            choose_linux_buffer_size(44_100, SupportedBufferSize::Range { min: 64, max: 16_384 }),
            BufferSize::Fixed(2_048)
        );
        assert_eq!(
            choose_linux_buffer_size(48_000, SupportedBufferSize::Range { min: 128, max: 1_024 }),
            BufferSize::Fixed(1_024)
        );
        assert_eq!(
            choose_linux_buffer_size(48_000, SupportedBufferSize::Range { min: 4_096, max: 8_192 }),
            BufferSize::Fixed(4_096)
        );
        assert_eq!(
            choose_linux_buffer_size(48_000, SupportedBufferSize::Unknown),
            BufferSize::Default
        );
    }

    #[test]
    fn stream_error_limiter_logs_initial_burst_then_coalesces() {
        let lim = StreamErrorLimiter::new();
        assert_eq!(lim.record_at_ms(10), Some(0));
        assert_eq!(lim.record_at_ms(20), Some(0));
        assert_eq!(lim.record_at_ms(30), Some(0));
        assert_eq!(lim.record_at_ms(100), None);
        assert_eq!(lim.record_at_ms(200), None);
        assert_eq!(lim.record_at_ms(4_999), None);
        assert_eq!(lim.record_at_ms(5_030), Some(3));
        assert_eq!(lim.record_at_ms(6_000), None);
        assert_eq!(lim.record_at_ms(10_030), Some(1));
    }

    #[test]
    fn async_wakeup_and_fire_and_forget_create_and_play_are_sub_millisecond() {
        let rt = start_test_runtime();

        // Warm up the newly spawned thread with one sync barrier.
        let r = rt.dispatch(RequestHeader::new(Op::WaveStopRecord, 0), "");
        assert_eq!(r.snderr, SNDERR_OK);

        // Synchronous barrier wakes immediately via tokio mpsc (no 3 ms polling sleep).
        let t0 = Instant::now();
        for _ in 0..10 {
            let r = rt.dispatch(RequestHeader::new(Op::WaveStopRecord, 0), "");
            assert_eq!(r.snderr, SNDERR_OK);
        }
        let avg_sync_us = t0.elapsed().as_micros() / 10;
        assert!(
            avg_sync_us < 1_500,
            "sync barrier averaged {avg_sync_us} us (expected << 3 ms without sleep polling)"
        );

        // Caller-side handle allocation + fire-and-forget CreateSound.
        let t1 = Instant::now();
        let mut handles = Vec::new();
        for _ in 0..50 {
            let r = rt.dispatch(
                RequestHeader::new(Op::CreateSound, 0).with_a(SoundKind::Wave as i32),
                "",
            );
            assert_eq!(r.snderr, SNDERR_OK);
            assert_ne!(r.value, 0);
            handles.push(r.value);
        }
        // Strictly unique handles.
        let mut sorted = handles.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 50);
        // Playing an unloaded handle immediately returns SNDERR_UNLOADED without waiting.
        assert_eq!(
            rt.dispatch(RequestHeader::new(Op::Play, handles[0]), "").snderr,
            SNDERR_UNLOADED
        );
        assert_eq!(
            rt.dispatch(RequestHeader::new(Op::PlayInstance, handles[0]), "").snderr,
            SNDERR_UNLOADED
        );
        assert!(
            t1.elapsed() < Duration::from_millis(20),
            "50 fire-and-forget CreateSound + Play checks took {:?}",
            t1.elapsed()
        );
    }

    #[test]
    fn double_buffered_query_state_uses_clock_and_does_not_churn_during_steady_playback() {
        let dir = unique_tmp_dir("clock-query");
        let wav_path = dir.join("tone_600ms.wav");
        // 44100 Hz mono 16-bit WAV, 600 ms = 26460 frames (headroom for parallel tests).
        write_test_wav(&wav_path, 44_100, &vec![8_000i16; 26_460]);

        let rt = start_test_runtime();
        let h = rt
            .dispatch(
                RequestHeader::new(Op::CreateSound, 0).with_a(SoundKind::Wave as i32),
                "",
            )
            .value;

        // Streaming mode (flags = 0x100) so get_position_ms is active and trackable.
        rt.dispatch(RequestHeader::new(Op::SetFlags, h).with_a(0x100), "");
        let load = rt.dispatch(
            RequestHeader::new(Op::Load, h).with_a(0x100),
            wav_path.to_str().unwrap(),
        );
        assert_eq!(load.snderr, SNDERR_OK);
        assert_eq!(load.value, 1, "streaming flag 0x100 -> DS buffer");
        assert_eq!(load.extra1, 600, "header probe exact 600 ms duration");

        // Let background decode finish before testing steady-state zero-churn Play,
        // so finish_async_decode cannot race into the steady-state measurement window.
        let _ = rt.dispatch(RequestHeader::new(Op::WaveStopRecord, 0), "");
        wait_ms(100);

        // Fire-and-forget Play -> immediate QueryState sees STATE_PLAYING right away!
        assert_eq!(rt.dispatch(RequestHeader::new(Op::Play, h), "").snderr, SNDERR_OK);
        let q0 = rt.dispatch(RequestHeader::new(Op::QueryState, h), "");
        assert_ne!(q0.value & STATE_PLAYING, 0, "immediate is_playing after fire-and-forget Play");
        assert_ne!(q0.value & STATE_LOADED, 0);
        assert_eq!(q0.extra1, 600);

        // Sync once so Play is settled in StatusSnapshot.
        // Note: WaveOpen is intentionally NOT called yet, proving clock projection advances
        // position_ms and stops at 600 ms purely from the computer clock without render ticks!
        let _ = rt.dispatch(RequestHeader::new(Op::WaveStopRecord, 0), "");
        let gen_mid = rt.snapshot_generation();
        assert!(gen_mid >= 1, "snapshot must have published at least once");

        // Advance ~80 ms and verify get_position_ms tracks the computer clock with ZERO
        // additional snapshot flips!
        wait_ms(80);
        let q_mid = rt.dispatch(RequestHeader::new(Op::QueryState, h), "");
        assert_ne!(q_mid.value & STATE_PLAYING, 0);
        assert!(
            q_mid.extra2 >= 40 && q_mid.extra2 <= 500,
            "clock-projected position_ms={} expected ~80 ms",
            q_mid.extra2
        );
        assert_eq!(
            rt.snapshot_generation(),
            gen_mid,
            "StatusSnapshot must not flip during steady-state playback"
        );

        // Seek immediately updates projected position_ms without waiting.
        rt.dispatch(RequestHeader::new(Op::SetPositionMs, h).with_a(250), "");
        let q_seek = rt.dispatch(RequestHeader::new(Op::QueryState, h), "");
        assert!(q_seek.extra2 >= 250 && q_seek.extra2 <= 320, "seek pos={}", q_seek.extra2);

        // Pause freezes projected position_ms.
        rt.dispatch(RequestHeader::new(Op::Pause, h), "");
        let q_p1 = rt.dispatch(RequestHeader::new(Op::QueryState, h), "");
        wait_ms(40);
        let q_p2 = rt.dispatch(RequestHeader::new(Op::QueryState, h), "");
        assert_eq!(q_p1.extra2, q_p2.extra2, "position must freeze while paused");

        // Immediate Stop flips is_playing to false with zero wait.
        rt.dispatch(RequestHeader::new(Op::Stop, h), "");
        let q_stop = rt.dispatch(RequestHeader::new(Op::QueryState, h), "");
        assert_eq!(q_stop.value & STATE_PLAYING, 0, "immediate is_playing=false after Stop");

        // Non-looping Play projects natural EOF purely from computer clock after 600 ms.
        assert_eq!(rt.dispatch(RequestHeader::new(Op::Play, h), "").snderr, SNDERR_OK);
        assert_ne!(
            rt.dispatch(RequestHeader::new(Op::QueryState, h), "").value & STATE_PLAYING,
            0
        );
        wait_ms(660);
        let q_eof = rt.dispatch(RequestHeader::new(Op::QueryState, h), "");
        assert_eq!(
            q_eof.value & STATE_PLAYING,
            0,
            "non-looping playback must project to stopped after duration elapses"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nonblocking_load_validates_cheaply_and_plays_after_background_decode() {
        let dir = unique_tmp_dir("async-load");
        let wav_path = dir.join("beep.wav");
        let junk_path = dir.join("corrupt.wav");
        let missing_path = dir.join("does_not_exist.wav");
        let rec_path = dir.join("recorded.wav");
        // 44100 Hz mono 16-bit WAV, 150 ms of non-zero tone.
        let samples: Vec<i16> = (0..6_615)
            .map(|i| if (i / 20) % 2 == 0 { 12_000 } else { -12_000 })
            .collect();
        write_test_wav(&wav_path, 44_100, &samples);
        std::fs::write(&junk_path, b"corrupt audio bytes").unwrap();

        let rt = start_test_runtime();
        let h = rt
            .dispatch(
                RequestHeader::new(Op::CreateSound, 0).with_a(SoundKind::Wave as i32),
                "",
            )
            .value;

        // Cheap validation rejects missing and corrupt files synchronously.
        assert_eq!(
            rt.dispatch(RequestHeader::new(Op::Load, h), missing_path.to_str().unwrap()).snderr,
            SNDERR_FILE_OPEN
        );
        assert_eq!(
            rt.dispatch(RequestHeader::new(Op::Load, h), junk_path.to_str().unwrap()).snderr,
            SNDERR_INVALID_FORMAT
        );

        // Playlist validation: missing file, valid file, duplicate queue -> ALREADY_LOADED.
        let pl = rt
            .dispatch(
                RequestHeader::new(Op::CreateSound, 0).with_a(SoundKind::Playlist as i32),
                "",
            )
            .value;
        assert_eq!(
            rt.dispatch(
                RequestHeader::new(Op::PlaylistQueueTrack, pl),
                missing_path.to_str().unwrap()
            )
            .snderr,
            SNDERR_FILE_OPEN
        );
        let q1 = rt.dispatch(
            RequestHeader::new(Op::PlaylistQueueTrack, pl),
            wav_path.to_str().unwrap(),
        );
        assert_eq!(q1.snderr, SNDERR_OK);
        assert_ne!(q1.value, 0);
        assert_eq!(
            rt.dispatch(
                RequestHeader::new(Op::PlaylistQueueTrack, pl),
                wav_path.to_str().unwrap()
            )
            .snderr,
            SNDERR_ALREADY_LOADED
        );
        assert_eq!(rt.dispatch(RequestHeader::new(Op::Play, pl), "").snderr, SNDERR_OK);
        let pl_q = rt.dispatch(RequestHeader::new(Op::QueryState, pl), "");
        assert_ne!(pl_q.value & STATE_PLAYING, 0);
        assert_eq!(pl_q.value & STATE_PLAYLIST_HAS_QUEUED, 0);
        rt.dispatch(RequestHeader::new(Op::Stop, pl), "");

        // Valid WaveSound Load followed IMMEDIATELY by Play + recording verifies
        // pending_play starts rendering as soon as the background decoder finishes.
        rt.dispatch(RequestHeader::new(Op::WaveOpen, 0), "");
        assert_eq!(
            rt.dispatch(RequestHeader::new(Op::WaveStartRecord, 0), rec_path.to_str().unwrap())
                .snderr,
            SNDERR_OK
        );
        let load = rt.dispatch(RequestHeader::new(Op::Load, h), wav_path.to_str().unwrap());
        assert_eq!(load.snderr, SNDERR_OK);
        assert_eq!(load.value, 0);
        assert_eq!(load.extra1, 150);
        assert_eq!(rt.dispatch(RequestHeader::new(Op::Play, h), "").snderr, SNDERR_OK);

        // Fire 8 concurrent background loads on other handles while playback is active
        // to verify decoding runs off-thread without stalling the mixer or game thread.
        let burst_start = Instant::now();
        for i in 0..8u32 {
            let other_wav = dir.join(format!("extra_{i}.wav"));
            write_test_wav(&other_wav, 44_100, &vec![500i16; 22_050]);
            let oh = rt
                .dispatch(
                    RequestHeader::new(Op::CreateSound, 0).with_a(SoundKind::Wave as i32),
                    "",
                )
                .value;
            let lr = rt.dispatch(RequestHeader::new(Op::Load, oh), other_wav.to_str().unwrap());
            assert_eq!(lr.snderr, SNDERR_OK);
            assert_eq!(lr.extra1, 500);
        }
        assert!(
            burst_start.elapsed() < Duration::from_millis(100),
            "8 non-blocking Loads took {:?}",
            burst_start.elapsed()
        );

        // Wait ~180 ms for async decode + render, then flush recording.
        wait_ms(180);
        assert_eq!(rt.dispatch(RequestHeader::new(Op::WaveStopRecord, 0), "").snderr, SNDERR_OK);
        assert_eq!(rt.dispatch(RequestHeader::new(Op::WaveClose, 0), "").snderr, SNDERR_OK);

        // Verify recorded WAV captured real non-zero audio from the async-loaded sound.
        let rec_bytes = std::fs::read(&rec_path).expect("read recorded wav");
        assert!(rec_bytes.len() > 44, "recorded wav must contain PCM frames");
        let pcm: Vec<i16> = rec_bytes[44..]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert!(
            pcm.iter().any(|&s| s.abs() > 1000),
            "async Load + immediate Play must render non-zero audio"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_async_decode_is_ignored_after_unload_or_reload() {
        let dir = unique_tmp_dir("stale-gen");
        let wav_a = dir.join("a.wav");
        let wav_b = dir.join("b.wav");
        write_test_wav(&wav_a, 44_100, &vec![4_000i16; 4_410]);
        write_test_wav(&wav_b, 44_100, &vec![9_000i16; 4_410]);

        let mut engine = AudioEngine::new();
        let h = engine
            .handle_request(
                RequestHeader::new(Op::CreateSound, 0).with_a(SoundKind::Wave as i32),
                "",
            )
            .value;
        let job1 = engine
            .begin_async_load(h, 1, wav_a.clone(), 0, false, 100)
            .expect("job1");
        let job2 = engine
            .begin_async_load(h, 2, wav_b.clone(), 0, false, 100)
            .expect("job2");

        let mut cache = HashMap::new();
        let res1 = execute_decode_job(job1, &mut cache);
        let res2 = execute_decode_job(job2, &mut cache);

        // Deliver newer gen 2 first, then stale gen 1: stale gen 1 must be ignored!
        engine.finish_async_decode(res2);
        engine.finish_async_decode(res1);
        assert_eq!(engine.handle_request(RequestHeader::new(Op::Play, h), "").snderr, SNDERR_OK);
        let mut out = [0i16; 8];
        engine.render_frames(&mut out);
        assert_eq!(out[0], 9_000, "stale gen 1 decode must not overwrite newer gen 2");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn credits_fade_out_survives_channel_set_volume_and_updates_query_state() {
        let dir = unique_tmp_dir("credits-crossfade");
        let title_wav = dir.join("menu1.wav");
        let credits_wav = dir.join("credits.wav");
        let rec_path = dir.join("after_leave_credits.wav");

        // Two distinct constant-amplitude streams (44100 Hz mono 16-bit WAV, 1 s each).
        write_test_wav(&title_wav, 44_100, &vec![10_000i16; 44_100]);
        write_test_wav(&credits_wav, 44_100, &vec![6_000i16; 44_100]);

        let rt = start_test_runtime();
        rt.dispatch(RequestHeader::new(Op::WaveOpen, 0), "");

        let h_title = rt
            .dispatch(
                RequestHeader::new(Op::CreateSound, 0).with_a(SoundKind::Wave as i32),
                "",
            )
            .value;
        let h_credits = rt
            .dispatch(
                RequestHeader::new(Op::CreateSound, 0).with_a(SoundKind::Wave as i32),
                "",
            )
            .value;

        for (h, path) in [(h_title, &title_wav), (h_credits, &credits_wav)] {
            rt.dispatch(RequestHeader::new(Op::SetFlags, h).with_a(0x506), "");
            rt.dispatch(RequestHeader::new(Op::SetLoop, h).with_a(1), "");
            let load = rt.dispatch(
                RequestHeader::new(Op::Load, h).with_a(0x506),
                path.to_str().unwrap(),
            );
            assert_eq!(load.snderr, SNDERR_OK);
            assert_eq!(load.value, 1, "flags=0x506 must load as DirectSound stream");
        }

        // Start title music and settle background decode.
        assert_eq!(rt.dispatch(RequestHeader::new(Op::Play, h_title), "").snderr, SNDERR_OK);
        wait_ms(60);
        assert_ne!(
            rt.dispatch(RequestHeader::new(Op::QueryState, h_title), "").value & STATE_PLAYING,
            0
        );

        // Enter Credits: title.fade(60) -> Channel 0 SetVolume(127) on BOTH -> credits.play().
        rt.dispatch(RequestHeader::new(Op::SetFadeMs, h_title).with_a(60), "");
        rt.dispatch(RequestHeader::new(Op::FadeOut, h_title), "");
        rt.dispatch(RequestHeader::new(Op::SetVolume, h_title).with_a(127), "");
        rt.dispatch(RequestHeader::new(Op::SetVolume, h_credits).with_a(127), "");
        assert_eq!(rt.dispatch(RequestHeader::new(Op::Play, h_credits), "").snderr, SNDERR_OK);

        // Wait past the 60 ms fade-out (with generous scheduling headroom).
        wait_ms(160);
        assert_eq!(
            rt.dispatch(RequestHeader::new(Op::QueryState, h_title), "").value & STATE_PLAYING,
            0,
            "title music must report not playing after fade-out despite Channel 0 SetVolume"
        );
        assert_ne!(
            rt.dispatch(RequestHeader::new(Op::QueryState, h_credits), "").value & STATE_PLAYING,
            0,
            "credits music must be playing"
        );

        // Leave Credits: credits.fade(60) -> Channel 0 SetVolume(127) on BOTH -> title.play().
        rt.dispatch(RequestHeader::new(Op::SetFadeMs, h_credits).with_a(60), "");
        rt.dispatch(RequestHeader::new(Op::FadeOut, h_credits), "");
        rt.dispatch(RequestHeader::new(Op::SetVolume, h_title).with_a(127), "");
        rt.dispatch(RequestHeader::new(Op::SetVolume, h_credits).with_a(127), "");
        assert_eq!(rt.dispatch(RequestHeader::new(Op::Play, h_title), "").snderr, SNDERR_OK);

        // Wait past the 60 ms fade-out, then record a slice to confirm ONLY title music plays.
        wait_ms(160);
        assert_eq!(
            rt.dispatch(RequestHeader::new(Op::QueryState, h_credits), "").value & STATE_PLAYING,
            0,
            "credits music must stop after leaving Credits despite Channel 0 SetVolume"
        );
        assert_ne!(
            rt.dispatch(RequestHeader::new(Op::QueryState, h_title), "").value & STATE_PLAYING,
            0,
            "title music must be playing after leaving Credits"
        );

        assert_eq!(
            rt.dispatch(
                RequestHeader::new(Op::WaveStartRecord, 0),
                rec_path.to_str().unwrap(),
            )
            .snderr,
            SNDERR_OK
        );
        wait_ms(80);
        assert_eq!(rt.dispatch(RequestHeader::new(Op::WaveStopRecord, 0), "").snderr, SNDERR_OK);
        assert_eq!(rt.dispatch(RequestHeader::new(Op::WaveClose, 0), "").snderr, SNDERR_OK);

        let rec_bytes = std::fs::read(&rec_path).expect("read recorded wav");
        assert!(rec_bytes.len() > 44, "recorded wav must contain PCM frames");
        let pcm: Vec<i16> = rec_bytes[44..]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert!(!pcm.is_empty());
        // Title amplitude is 10000 (not 16000 = title + credits stacked).
        let tail = pcm[pcm.len() - 2];
        assert!(
            (tail - 10_000).abs() <= 5,
            "recorded tail after leaving Credits must contain only title music (~10000), got {tail}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
