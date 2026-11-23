//! Lock-free native audio thread and real-time `cpal` output stream.
//!
//! Uses zero locks (`no_locks` enforced):
//! - Game thread -> Audio thread commands go through `handoff::Mailbox`.
//! - Synchronous commands (`Load`, `CreateSound`, `QueryState`, etc.) receive
//!   their reply via a single-shot `std::sync::mpsc::sync_channel(1)`.
//! - Audio thread -> `cpal` real-time callback transfers pre-mixed stereo `f32`
//!   samples through a `#![forbid(unsafe_code)]` SPSC ring buffer built from
//!   `Box<[AtomicU32]>` + `AtomicUsize` indices (wait-free, zero allocations in
//!   the audio callback).
//! - When the host audio device runs at a sample rate other than 44100 Hz
//!   (e.g. 48000 Hz), `rubato::Async::<f64>` resamples each rendered block on
//!   the audio thread before pushing into the SPSC ring.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleFormat, Stream, StreamConfig, SupportedBufferSize};
use handoff::Mailbox;
use rubato::audioadapter_buffers::direct::{InterleavedSlice, SequentialSliceOfVecs};
use rubato::{
    Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

use crate::dsp::MIX_SAMPLE_RATE;
use crate::mixer::{AudioEngine, SNDERR_NO_OBJECT, SNDERR_OK};
use crate::wire::{Op, RequestHeader, Response};

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

struct Job {
    hdr: RequestHeader,
    path: String,
    reply: Option<std::sync::mpsc::SyncSender<Response>>,
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

/// Handle to the running lock-free native audio thread.
pub struct AudioRuntime {
    mailbox: Arc<Mailbox<Job>>,
    alive: Arc<AtomicBool>,
}

impl AudioRuntime {
    /// Starts the audio thread using `spawn` (pass `wine_unixlib::unix::spawn_native`
    /// inside `civ3r-native`, or `std::thread::spawn` in standalone tests).
    pub fn start<F>(spawn: F) -> std::io::Result<Self>
    where
        F: FnOnce(Box<dyn FnOnce() + Send + 'static>) -> std::io::Result<()>,
    {
        let mailbox = Arc::new(Mailbox::<Job>::new());
        let alive = Arc::new(AtomicBool::new(true));
        let mb = mailbox.clone();
        let al = alive.clone();
        spawn(Box::new(move || audio_thread_main(mb, al)))?;
        Ok(Self { mailbox, alive })
    }

    /// Sends one wire request to the audio thread. Fire-and-forget commands
    /// return immediately without blocking; query/load commands wait boundedly
    /// for their response.
    pub fn dispatch(&self, hdr: RequestHeader, path: &str) -> Response {
        if !self.alive.load(Ordering::Acquire) {
            return Response { snderr: SNDERR_NO_OBJECT, ..Default::default() };
        }
        let needs_reply = matches!(
            Op::from_u32(hdr.op),
            Some(
                Op::CreateSound
                    | Op::Load
                    | Op::QueryState
                    | Op::PlaylistQueueTrack
                    | Op::WaveOpen
                    | Op::WaveClose
                    | Op::WaveStartRecord
                    | Op::WaveStopRecord
                    | Op::ReleaseAll
                    | Op::Play
                    | Op::PlayInstance
            )
        );
        if needs_reply {
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            self.mailbox.send(Job { hdr, path: path.to_owned(), reply: Some(tx) });
            rx.recv_timeout(Duration::from_secs(5))
                .unwrap_or(Response { snderr: SNDERR_OK, ..Default::default() })
        } else {
            self.mailbox.send(Job { hdr, path: path.to_owned(), reply: None });
            Response::default()
        }
    }
}

fn audio_thread_main(mailbox: Arc<Mailbox<Job>>, alive: Arc<AtomicBool>) {
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

    loop {
        // 1. Drain all pending game commands immediately.
        for job in mailbox.drain() {
            let op = Op::from_u32(job.hdr.op);
            let resp = engine.handle_request(job.hdr, &job.path);
            if let Some(tx) = job.reply {
                let _ = tx.send(resp);
            }
            match op {
                Some(Op::WaveOpen) => {
                    wave_active = true;
                    if output.is_none() {
                        output = open_cpal_output(ring.clone());
                        resampler = output.as_ref().and_then(|o| {
                            if o.sample_rate != MIX_SAMPLE_RATE {
                                HostResampler::new(o.sample_rate)
                            } else {
                                None
                            }
                        });
                    }
                    headless_clock = Instant::now();
                }
                Some(Op::WaveClose | Op::ReleaseAll) => {
                    wave_active = false;
                    output = None;
                    resampler = None;
                }
                _ => {}
            }
        }

        // 2. Render audio to keep ring buffer topped up (or advance headless clock).
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

        // Short sleep keeps CPU near 0% while guaranteeing <5ms command latency.
        std::thread::sleep(Duration::from_millis(3));
        if !alive.load(Ordering::Relaxed) {
            break;
        }
    }
    engine.stop_recording();
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn rubato_host_resampler_converts_44100_to_48000() {
        let mut rs = HostResampler::new(48_000).expect("create rubato resampler");
        let block = vec![0.25_f32; 512 * 2];
        let mut total_out_frames = 0usize;
        for _ in 0..10 {
            total_out_frames += rs.process(&block).len() / 2;
        }
        // 5120 input frames at 44100 -> ~5573 output frames at 48000 (within filter delay)
        assert!(total_out_frames > 5400 && total_out_frames < 5700, "got {total_out_frames}");
    }

    #[test]
    fn linux_buffer_size_targets_45ms_and_clamps_to_supported_range() {
        // 48 kHz * 45 ms = 2160 -> rounded up to 256-frame boundary = 2304.
        assert_eq!(
            choose_linux_buffer_size(48_000, SupportedBufferSize::Range { min: 64, max: 16_384 }),
            BufferSize::Fixed(2304)
        );
        // 44.1 kHz * 45 ms = 1984 -> rounded up to 256-frame boundary = 2048.
        assert_eq!(
            choose_linux_buffer_size(44_100, SupportedBufferSize::Range { min: 64, max: 16_384 }),
            BufferSize::Fixed(2048)
        );
        // Hardware with smaller max clamps cleanly to max.
        assert_eq!(
            choose_linux_buffer_size(48_000, SupportedBufferSize::Range { min: 64, max: 1024 }),
            BufferSize::Fixed(1024)
        );
        // Unknown range falls back to Default.
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
        // Burst within 5s window is suppressed.
        assert_eq!(lim.record_at_ms(100), None);
        assert_eq!(lim.record_at_ms(2_000), None);
        assert_eq!(lim.record_at_ms(4_999), None);
        // After 5s window elapses, emits summary with 3 suppressed.
        assert_eq!(lim.record_at_ms(5_100), Some(3));
        assert_eq!(lim.record_at_ms(6_000), None);
    }
}
