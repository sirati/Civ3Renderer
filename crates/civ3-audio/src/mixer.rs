//! Deterministic 44100 Hz stereo `f64` audio mixer implementing `sound.dll`'s
//! software mixer, DirectSound streaming buffers, fades, looping, `.amb` unit
//! animation sequencer, and Type 8 music playlists.
//!
//! Shared identically between live `cpal` playback ([`crate::output`]) and
//! offline numerical equality tests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::amb::{parse_amb, AmbAction, AmbSequencer, ResolvedProgram};
use crate::decoder::{decode_wav_file, DecodedWav, StreamDecoder};
use crate::dsp::{
    clamp_i16, ds_pan_gains, ds_volume_gain, pitch_ratio_from_cents, sw_pan_gains,
    sw_pan_gains_from_fixed, sw_volume_fixed, MIX_SAMPLE_RATE,
};
use crate::path::{resolve_amb_wav, resolve_game_path, resolve_output_path};
use crate::wire::{
    Op, RequestHeader, Response, SoundHandle, SoundKind, STATE_DS_BUFFER, STATE_LOADED,
    STATE_LOOPING, STATE_PLAYING, STATE_PLAYLIST_HAS_QUEUED,
};

/// Maximum simultaneous polyphonic software voices per `WaveSound`
/// (`sound.dll` allocates 16 slots at `inner + 0x238`, stride `0x110`).
pub const MAX_VOICES_PER_SOUND: usize = 16;

/// Coalescing window (milliseconds) for repeated starts of the exact same sound
/// sample. When the game triggers the same sound file again within this window
/// while the previous start is still playing (e.g. many cities celebrating "We
/// Love the King Day" on the same turn), the duplicate start is suppressed and
/// the already-playing voice continues cleanly without phase/clipping artefacts.
pub const SAME_SOUND_DEDUP_MS: u32 = 50;

/// [`SAME_SOUND_DEDUP_MS`] expressed in 44100 Hz mixer frames.
pub const SAME_SOUND_DEDUP_FRAMES: u64 =
    (SAME_SOUND_DEDUP_MS as u64 * MIX_SAMPLE_RATE as u64) / 1000;

/// `SNDERR` constants matching `sound.dll`.
pub const SNDERR_OK: u32 = 0;
pub const SNDERR_NOT_INIT: u32 = 1;
pub const SNDERR_FILE_OPEN: u32 = 8;
pub const SNDERR_INVALID_PARAM: u32 = 0x0A;
pub const SNDERR_UNSUPPORTED: u32 = 0x0B;
pub const SNDERR_ALREADY_PLAYING: u32 = 0x0E;
pub const SNDERR_ALREADY_LOADED: u32 = 0x0F;
pub const SNDERR_INVALID_FORMAT: u32 = 0x12;
pub const SNDERR_NO_OBJECT: u32 = 0x13;
pub const SNDERR_UNLOADED: u32 = 0x14;
pub const SNDERR_EMPTY_SLOT: u32 = 0x24;

#[derive(Clone, Debug)]
struct FadeState {
    /// Current normalized level in `[0.0, 1.0]`.
    current: f64,
    /// Target normalized level in `[0.0, 1.0]`.
    target: f64,
    /// Increment per 44100 Hz sample frame.
    step_per_frame: f64,
    /// Stop voice/stream automatically once target 0.0 is reached.
    stop_at_zero: bool,
}

impl FadeState {
    fn new(start: f64, target: f64, duration_ms: u32, stop_at_zero: bool) -> Self {
        let frames = (f64::from(duration_ms.max(1)) * f64::from(MIX_SAMPLE_RATE)) / 1000.0;
        let step_per_frame = (target - start) / frames;
        Self { current: start, target, step_per_frame, stop_at_zero }
    }

    /// Advances by 1 frame; returns `true` if fade reached completion.
    #[inline]
    fn step(&mut self) -> bool {
        self.current += self.step_per_frame;
        let done = if self.step_per_frame >= 0.0 {
            self.current >= self.target
        } else {
            self.current <= self.target
        };
        if done {
            self.current = self.target;
        }
        done
    }
}

/// One active polyphonic voice on the software mixer bus.
#[derive(Clone, Debug)]
struct SwVoice {
    wav: Arc<DecodedWav>,
    pos: f64,
    step: f64,
    volume: i32,
    pan: i32,
    looping: bool,
    fade: Option<FadeState>,
    /// Optional owner tag for `.amb` gated Note-Off matching: `(amb_handle, ch, note)`.
    amb_gate: Option<(SoundHandle, u8, u8)>,
    /// Parent sound handle (used to report `is_playing` and stop non-ringout voices).
    owner: SoundHandle,
    /// Engine frame counter (`AudioEngine::total_frames_rendered`) when this voice started.
    started_at_frame: u64,
}

impl SwVoice {
    /// Renders one 44100 Hz stereo frame into `(sum_l, sum_r)`. Returns `false`
    /// if the voice finished and should be removed.
    #[inline]
    fn mix_frame(&mut self, sum_l: &mut f64, sum_r: &mut f64) -> bool {
        let total_frames = self.wav.frame_count();
        if total_frames == 0 {
            return false;
        }

        // Handle looping / end-of-buffer.
        let (loop_start, loop_end) = self.wav.loop_range.unwrap_or((0, total_frames));
        if self.looping && loop_end > loop_start {
            let span = (loop_end - loop_start) as f64;
            while self.pos >= loop_end as f64 {
                self.pos -= span;
            }
        } else if self.pos >= total_frames as f64 {
            return false;
        }

        // Compute linear gains.
        let (gl, gr) = if let Some(fade) = &mut self.fade {
            let fixed = (fade.current * 65_536.0).round() as i32;
            let gains = sw_pan_gains_from_fixed(fixed, self.pan);
            let done = fade.step();
            if done {
                let stop_now = fade.stop_at_zero && fade.target <= 0.0;
                self.fade = None;
                if stop_now {
                    return false;
                }
            }
            gains
        } else {
            sw_pan_gains(self.volume, self.pan)
        };

        let i0 = self.pos as usize;
        let frac = self.pos - i0 as f64;
        let i1 = if i0 + 1 < total_frames {
            i0 + 1
        } else if self.looping && loop_end > loop_start {
            loop_start
        } else {
            i0
        };

        if self.wav.channels == 1 {
            let s0 = f64::from(self.wav.samples[i0]);
            let s1 = f64::from(self.wav.samples[i1]);
            let s = s0 + frac * (s1 - s0);
            *sum_l += s * gl;
            *sum_r += s * gr;
        } else {
            let ch = usize::from(self.wav.channels);
            let l0 = f64::from(self.wav.samples[i0 * ch]);
            let r0 = f64::from(self.wav.samples[i0 * ch + 1]);
            let l1 = f64::from(self.wav.samples[i1 * ch]);
            let r1 = f64::from(self.wav.samples[i1 * ch + 1]);
            let sl = l0 + frac * (l1 - l0);
            let sr = r0 + frac * (r1 - r0);
            *sum_l += sl * gl;
            *sum_r += sr * gr;
        }

        self.pos += self.step;
        true
    }
}

/// Active DirectSound-semantics streaming track (MP3 music or large streaming WAV).
struct StreamState {
    decoder: StreamDecoder,
    /// Resampler phase if source stream rate != 44100 Hz.
    src_pos: f64,
    cur_frame: (i16, i16),
    next_frame: (i16, i16),
    primed: bool,
    playing: bool,
    paused: bool,
    looping: bool,
    volume: i32,
    pan: i32,
    fade: Option<FadeState>,
    /// Optional next stream handle chained seamlessly at EOF (Type 8 playlist).
    chained_next: Option<SoundHandle>,
    /// Optional owning playlist handle notified when `chained_next` promotes.
    playlist_owner: Option<SoundHandle>,
}

impl StreamState {
    fn new(decoder: StreamDecoder) -> Self {
        Self {
            decoder,
            src_pos: 0.0,
            cur_frame: (0, 0),
            next_frame: (0, 0),
            primed: false,
            playing: false,
            paused: false,
            looping: false,
            volume: 127,
            pan: 0,
            fade: None,
            chained_next: None,
            playlist_owner: None,
        }
    }

    fn rewind(&mut self) {
        self.decoder.rewind();
        self.src_pos = 0.0;
        self.primed = false;
    }

    /// Reads one 44100 Hz stereo sample from the stream (fast-path when source
    /// is 44100 Hz, linear interpolation otherwise). Returns `None` on EOF.
    fn pull_44100_frame(&mut self) -> Option<(f64, f64)> {
        let rate = self.decoder.sample_rate();
        if rate == MIX_SAMPLE_RATE {
            if let Some((l, r)) = self.decoder.next_frame() {
                return Some((f64::from(l), f64::from(r)));
            }
            if self.looping && self.chained_next.is_none() {
                self.rewind();
                return self.decoder.next_frame().map(|(l, r)| (f64::from(l), f64::from(r)));
            }
            return None;
        }

        if !self.primed {
            self.cur_frame = self.decoder.next_frame()?;
            self.next_frame = self.decoder.next_frame().unwrap_or(self.cur_frame);
            self.src_pos = 0.0;
            self.primed = true;
        }
        let step = f64::from(rate) / f64::from(MIX_SAMPLE_RATE);
        while self.src_pos >= 1.0 {
            self.src_pos -= 1.0;
            self.cur_frame = self.next_frame;
            match self.decoder.next_frame() {
                Some(f) => self.next_frame = f,
                None => {
                    if self.looping && self.chained_next.is_none() {
                        self.rewind();
                        self.cur_frame = self.decoder.next_frame()?;
                        self.next_frame = self.decoder.next_frame().unwrap_or(self.cur_frame);
                        self.src_pos = 0.0;
                        break;
                    } else {
                        return None;
                    }
                }
            }
        }
        let frac = self.src_pos;
        let l = f64::from(self.cur_frame.0)
            + frac * (f64::from(self.next_frame.0) - f64::from(self.cur_frame.0));
        let r = f64::from(self.cur_frame.1)
            + frac * (f64::from(self.next_frame.1) - f64::from(self.cur_frame.1));
        self.src_pos += step;
        Some((l, r))
    }
}

struct WaveObject {
    flags: u32,
    volume: i32,
    pan: i32,
    pitch_cents: i32,
    looping: bool,
    fade_out_ms: u32,
    fade_in_ms: u32,
    wav: Option<Arc<DecodedWav>>,
    stream: Option<StreamState>,
    /// Set when `fade_out` was requested on a SW-mixed sound.
    active_fade: Option<FadeState>,
}

impl WaveObject {
    fn new() -> Self {
        Self {
            flags: 0,
            volume: 127,
            pan: 0,
            pitch_cents: 0,
            looping: false,
            fade_out_ms: 1000,
            fade_in_ms: 1000,
            wav: None,
            stream: None,
            active_fade: None,
        }
    }

    fn is_ds_stream(&self) -> bool {
        self.stream.is_some()
    }

    fn is_loaded(&self) -> bool {
        self.wav.is_some() || self.stream.is_some()
    }

    fn duration_ms(&self) -> u32 {
        if let Some(w) = &self.wav {
            w.duration_ms()
        } else if let Some(s) = &self.stream {
            s.decoder.duration_ms()
        } else {
            0
        }
    }

    fn position_ms(&self) -> u32 {
        if let Some(s) = &self.stream {
            s.decoder.position_ms()
        } else {
            0
        }
    }
}

struct AmbObject {
    sequencer: Option<AmbSequencer>,
    child_wavs: Vec<Arc<DecodedWav>>,
    root_notes: Vec<u8>,
    flags_58: u32,
    ms_per_frame: u32,
    volume: i32,
    pan: i32,
    looping: bool,
}

impl AmbObject {
    fn new() -> Self {
        Self {
            sequencer: None,
            child_wavs: Vec::new(),
            root_notes: Vec::new(),
            flags_58: 0,
            ms_per_frame: 0,
            volume: 127,
            pan: 0,
            looping: false,
        }
    }
}

struct PlaylistObject {
    current: Option<SoundHandle>,
    queued: Option<SoundHandle>,
    volume: i32,
    scaled_volume: i32,
}

impl PlaylistObject {
    fn new() -> Self {
        Self { current: None, queued: None, volume: 127, scaled_volume: 127 }
    }
}

enum SoundSlot {
    Wave(WaveObject),
    Amb(AmbObject),
    Playlist(PlaylistObject),
    Stub,
}

/// Optional WAV file capture writer (matches `WaveDevice::slot 21/22` and
/// `CIV3R_AUDIO_DUMP`). Periodically flushes the RIFF header so the WAV stays
/// readable even if a live session is terminated by signal.
struct WavRecorder {
    file: std::fs::File,
    data_bytes: u32,
    bytes_since_flush: u32,
}

impl WavRecorder {
    const HEADER_FLUSH_BYTES: u32 = MIX_SAMPLE_RATE * 4; // ~1 second of stereo i16

    fn create(path: &Path) -> std::io::Result<Self> {
        use std::io::Write;
        let mut file = std::fs::File::create(path)?;
        // Write placeholder 44-byte RIFF/WAVE header (44100 Hz, 16-bit stereo).
        let hdr = make_wav_header(MIX_SAMPLE_RATE, 2, 0);
        file.write_all(&hdr)?;
        Ok(Self { file, data_bytes: 0, bytes_since_flush: 0 })
    }

    fn write_samples(&mut self, samples: &[i16]) {
        use std::io::Write;
        let bytes: &[u8] = bytemuck::cast_slice(samples);
        if self.file.write_all(bytes).is_ok() {
            let n = bytes.len() as u32;
            self.data_bytes = self.data_bytes.saturating_add(n);
            self.bytes_since_flush = self.bytes_since_flush.saturating_add(n);
            if self.bytes_since_flush >= Self::HEADER_FLUSH_BYTES {
                self.flush_header();
                self.bytes_since_flush = 0;
            }
        }
    }

    fn flush_header(&mut self) {
        use std::io::{Seek, SeekFrom, Write};
        let hdr = make_wav_header(MIX_SAMPLE_RATE, 2, self.data_bytes);
        if self.file.seek(SeekFrom::Start(0)).is_ok() {
            let _ = self.file.write_all(&hdr);
            let _ = self.file.seek(SeekFrom::End(0));
            let _ = self.file.flush();
        }
    }

    fn finish(mut self) {
        self.flush_header();
    }
}

/// Builds a canonical 44-byte RIFF PCM WAV header.
pub fn make_wav_header(sample_rate: u32, channels: u16, data_bytes: u32) -> [u8; 44] {
    let mut h = [0u8; 44];
    let block_align = channels * 2;
    let byte_rate = sample_rate * u32::from(block_align);
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&data_bytes.saturating_add(36).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&block_align.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

/// Core deterministic Civ3 audio engine.
pub struct AudioEngine {
    next_handle: SoundHandle,
    sounds: HashMap<SoundHandle, SoundSlot>,
    sw_voices: Vec<SwVoice>,
    /// Cache of decoded short WAVs keyed by canonical host path so repeated
    /// `.amb` and SFX loads are near-instant and share immutable `Arc<DecodedWav>`.
    wav_cache: HashMap<PathBuf, Arc<DecodedWav>>,
    master_volume: i32,
    wave_open: bool,
    midi_open: bool,
    wave_paused: bool,
    midi_paused: bool,
    base_dir: Option<PathBuf>,
    amb_rng_seed: u32,
    amb_actions_scratch: Vec<AmbAction>,
    recorder: Option<WavRecorder>,
    /// Monotonic 44100 Hz frame clock used for deterministic voice deduplication.
    total_frames_rendered: u64,
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            next_handle: 1,
            sounds: HashMap::new(),
            sw_voices: Vec::with_capacity(64),
            wav_cache: HashMap::new(),
            master_volume: 127,
            wave_open: false,
            midi_open: false,
            wave_paused: false,
            midi_paused: false,
            base_dir: None,
            amb_rng_seed: 1,
            amb_actions_scratch: Vec::with_capacity(32),
            recorder: None,
            total_frames_rendered: 0,
        }
    }

    /// Sets an explicit base directory for resolving relative game paths
    /// (defaults to process CWD when unset).
    pub fn set_base_dir(&mut self, dir: impl Into<PathBuf>) {
        self.base_dir = Some(dir.into());
    }

    /// Sets a deterministic seed for `.amb` random pitch/volume variations
    /// (useful for reproducible equality tests).
    pub fn set_rng_seed(&mut self, seed: u32) {
        self.amb_rng_seed = seed;
    }

    /// Starts recording all rendered 44100 Hz 16-bit stereo frames to `path`.
    pub fn start_recording(&mut self, path: &Path) -> std::io::Result<()> {
        self.stop_recording();
        self.recorder = Some(WavRecorder::create(path)?);
        Ok(())
    }

    /// Finalizes and closes any active WAV recording.
    pub fn stop_recording(&mut self) {
        if let Some(rec) = self.recorder.take() {
            rec.finish();
        }
    }

    fn alloc_handle(&mut self) -> SoundHandle {
        let h = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1).max(1);
        h
    }

    fn resolve_path(&self, raw: &str) -> Option<PathBuf> {
        resolve_game_path(raw, self.base_dir.as_deref())
    }

    fn load_cached_wav(&mut self, path: &Path) -> Result<Arc<DecodedWav>, u32> {
        let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(cached) = self.wav_cache.get(&key) {
            return Ok(cached.clone());
        }
        let decoded = decode_wav_file(&key).map_err(|e| {
            log::warn!("civ3-audio: failed decoding WAV {}: {e}", key.display());
            SNDERR_INVALID_FORMAT
        })?;
        let arc = Arc::new(decoded);
        if self.wav_cache.len() < 2048 {
            self.wav_cache.insert(key, arc.clone());
        }
        Ok(arc)
    }

    /// Returns `true` if an active software voice for the exact same decoded
    /// sample (`Arc::ptr_eq`) was already started within [`SAME_SOUND_DEDUP_FRAMES`].
    fn has_recent_voice_for_wav(&self, wav: &Arc<DecodedWav>) -> bool {
        self.sw_voices.iter().any(|v| {
            Arc::ptr_eq(&v.wav, wav)
                && self.total_frames_rendered.saturating_sub(v.started_at_frame)
                    < SAME_SOUND_DEDUP_FRAMES
        })
    }

    /// Returns whether `handle` is currently playing (SW voice, active stream,
    /// active `.amb` sequencer, or active playlist track).
    pub fn is_playing(&self, handle: SoundHandle) -> bool {
        match self.sounds.get(&handle) {
            Some(SoundSlot::Wave(w)) => {
                if let Some(s) = &w.stream {
                    s.playing
                } else {
                    self.sw_voices.iter().any(|v| v.owner == handle)
                }
            }
            Some(SoundSlot::Amb(a)) => {
                a.sequencer.as_ref().is_some_and(|s| s.playing)
                    || self.sw_voices.iter().any(|v| v.owner == handle)
            }
            Some(SoundSlot::Playlist(p)) => p.current.is_some_and(|h| self.is_playing(h)),
            _ => false,
        }
    }

    /// Returns an iterator of `(handle, is_playing)` for syncing atomic state.
    pub fn playing_snapshot(&self, out: &mut Vec<(SoundHandle, bool)>) {
        out.clear();
        for &h in self.sounds.keys() {
            out.push((h, self.is_playing(h)));
        }
    }

    /// Executes one wire request against the engine and returns its [`Response`].
    pub fn handle_request(&mut self, hdr: RequestHeader, path: &str) -> Response {
        let Some(op) = Op::from_u32(hdr.op) else {
            return Response { snderr: SNDERR_INVALID_PARAM, ..Default::default() };
        };

        match op {
            Op::InitTimer => Response::default(),
            Op::ReleaseAll => {
                self.sw_voices.clear();
                self.sounds.clear();
                self.wave_open = false;
                self.midi_open = false;
                self.stop_recording();
                Response::default()
            }
            Op::WaveOpen => {
                self.wave_open = true;
                self.wave_paused = false;
                Response::default()
            }
            Op::WaveClose => {
                self.sw_voices.clear();
                for slot in self.sounds.values_mut() {
                    if let SoundSlot::Wave(w) = slot {
                        if let Some(s) = &mut w.stream {
                            s.playing = false;
                        }
                    }
                }
                self.wave_open = false;
                self.stop_recording();
                Response::default()
            }
            Op::WavePause => {
                self.wave_paused = true;
                Response::default()
            }
            Op::WaveResume => {
                self.wave_paused = false;
                Response::default()
            }
            Op::WaveSetMasterVolume => {
                self.master_volume = hdr.a.clamp(0, 127);
                Response::default()
            }
            Op::WaveStartRecord => {
                let target = resolve_output_path(path, self.base_dir.as_deref())
                    .unwrap_or_else(|| PathBuf::from(path.replace('\\', "/")));
                match self.start_recording(&target) {
                    Ok(()) => Response::default(),
                    Err(_) => Response { snderr: SNDERR_FILE_OPEN, ..Default::default() },
                }
            }
            Op::WaveStopRecord => {
                self.stop_recording();
                Response::default()
            }
            Op::MidiOpen => {
                self.midi_open = true;
                self.midi_paused = false;
                Response::default()
            }
            Op::MidiClose => {
                for slot in self.sounds.values_mut() {
                    if let SoundSlot::Amb(a) = slot {
                        if let Some(seq) = &mut a.sequencer {
                            seq.stop();
                        }
                    }
                }
                self.midi_open = false;
                Response::default()
            }
            Op::MidiPause => {
                self.midi_paused = true;
                Response::default()
            }
            Op::MidiResume => {
                self.midi_paused = false;
                Response::default()
            }

            Op::CreateSound => {
                let kind = SoundKind::from_u32(hdr.a as u32).unwrap_or(SoundKind::Wave);
                let h = self.alloc_handle();
                let slot = match kind {
                    SoundKind::Wave | SoundKind::Type3 => SoundSlot::Wave(WaveObject::new()),
                    SoundKind::Amb => SoundSlot::Amb(AmbObject::new()),
                    SoundKind::Playlist => SoundSlot::Playlist(PlaylistObject::new()),
                    _ => SoundSlot::Stub,
                };
                self.sounds.insert(h, slot);
                Response { snderr: SNDERR_OK, value: h, ..Default::default() }
            }
            Op::DeleteSound => {
                self.delete_sound(hdr.handle);
                Response::default()
            }
            Op::Load => self.cmd_load(hdr.handle, path, hdr.a as u32),
            Op::Unload => {
                self.cmd_unload(hdr.handle);
                Response::default()
            }
            Op::Play => self.cmd_play(hdr.handle),
            Op::Stop => {
                self.cmd_stop(hdr.handle, hdr.a != 0);
                Response::default()
            }
            Op::FadeOut => self.cmd_fade_out(hdr.handle),
            Op::FadeIn => self.cmd_fade_in(hdr.handle),
            Op::Fade => self.cmd_fade(hdr.handle, hdr.a, hdr.b, hdr.c),
            Op::Pause => {
                if let Some(SoundSlot::Wave(w)) = self.sounds.get_mut(&hdr.handle) {
                    if let Some(s) = &mut w.stream {
                        s.paused = !s.paused;
                    }
                }
                Response::default()
            }
            Op::SetVolume => {
                self.cmd_set_volume(hdr.handle, hdr.a);
                Response::default()
            }
            Op::SetPan => {
                self.cmd_set_pan(hdr.handle, hdr.a);
                Response::default()
            }
            Op::SetLoop => {
                self.cmd_set_loop(hdr.handle, hdr.a != 0);
                Response::default()
            }
            Op::SetPitch => {
                self.cmd_set_pitch(hdr.handle, hdr.a);
                Response::default()
            }
            Op::SetFlags => {
                if let Some(SoundSlot::Wave(w)) = self.sounds.get_mut(&hdr.handle) {
                    let f = hdr.a as u32;
                    w.flags |= f;
                    if (f & 2) != 0 {
                        w.looping = true;
                        if let Some(s) = &mut w.stream {
                            s.looping = true;
                        }
                    }
                }
                Response::default()
            }
            Op::SetFadeMs => {
                if let Some(SoundSlot::Wave(w)) = self.sounds.get_mut(&hdr.handle) {
                    w.fade_out_ms = (hdr.a.max(0)) as u32;
                }
                Response::default()
            }
            Op::SetFadeInMs => {
                if let Some(SoundSlot::Wave(w)) = self.sounds.get_mut(&hdr.handle) {
                    w.fade_in_ms = (hdr.a.max(0)) as u32;
                }
                Response::default()
            }
            Op::SetPositionMs => {
                if let Some(SoundSlot::Wave(w)) = self.sounds.get_mut(&hdr.handle) {
                    if let Some(s) = &mut w.stream {
                        s.decoder.seek_ms(hdr.a.max(0) as u32);
                        s.primed = false;
                    }
                }
                Response::default()
            }
            Op::QueryState => self.cmd_query_state(hdr.handle),
            Op::PlayInstance => {
                let res = self.cmd_play(hdr.handle);
                Response { snderr: res.snderr, value: 0, ..Default::default() }
            }
            Op::StartVoice => self.cmd_play(hdr.handle),

            Op::AmbSyncMeta => {
                if let Some(SoundSlot::Amb(a)) = self.sounds.get_mut(&hdr.handle) {
                    a.flags_58 = hdr.a as u32;
                    a.ms_per_frame = hdr.b as u32;
                    if let Some(seq) = &mut a.sequencer {
                        seq.flags_58 = a.flags_58;
                        seq.ms_per_frame = a.ms_per_frame;
                    }
                }
                Response::default()
            }
            Op::AmbSetAnimFrame => {
                let frame = hdr.a as u32;
                let flags_58 = hdr.b as u32;
                let ms_per_frame = hdr.c as u32;
                self.cmd_amb_set_anim_frame(hdr.handle, frame, flags_58, ms_per_frame);
                Response::default()
            }

            Op::PlaylistQueueTrack => self.cmd_playlist_queue_track(hdr.handle, path),
            Op::PlaylistSetScaledVolume => {
                self.cmd_playlist_set_scaled_volume(hdr.handle, hdr.a);
                Response::default()
            }
            Op::PlaylistFade => self.cmd_playlist_fade(hdr.handle, hdr.a, hdr.c),
        }
    }

    fn delete_sound(&mut self, handle: SoundHandle) {
        self.sw_voices.retain(|v| v.owner != handle);
        if let Some(SoundSlot::Playlist(p)) = self.sounds.remove(&handle) {
            if let Some(cur) = p.current {
                self.delete_sound(cur);
            }
            if let Some(q) = p.queued {
                self.delete_sound(q);
            }
        }
    }

    fn cmd_unload(&mut self, handle: SoundHandle) {
        self.sw_voices.retain(|v| v.owner != handle);
        match self.sounds.get_mut(&handle) {
            Some(SoundSlot::Wave(w)) => {
                w.wav = None;
                w.stream = None;
                w.active_fade = None;
            }
            Some(SoundSlot::Amb(a)) => {
                a.sequencer = None;
                a.child_wavs.clear();
                a.root_notes.clear();
            }
            _ => {}
        }
    }

    fn cmd_load(&mut self, handle: SoundHandle, raw_path: &str, extra_flags: u32) -> Response {
        let Some(host_path) = self.resolve_path(raw_path) else {
            log::debug!("civ3-audio: file not found: {raw_path}");
            return Response { snderr: SNDERR_FILE_OPEN, ..Default::default() };
        };

        let is_amb = matches!(self.sounds.get(&handle), Some(SoundSlot::Amb(_)));
        if is_amb {
            return self.load_amb(handle, &host_path, extra_flags);
        }

        let Some(SoundSlot::Wave(w)) = self.sounds.get(&handle) else {
            return Response { snderr: SNDERR_OK, ..Default::default() };
        };
        let flags = w.flags | extra_flags;
        let is_mp3 = host_path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mp3"));
        let want_stream = is_mp3 || (flags & 0x100) != 0;

        if want_stream {
            match StreamDecoder::open(&host_path) {
                Ok(dec) => {
                    let dur = dec.duration_ms();
                    if let Some(SoundSlot::Wave(w)) = self.sounds.get_mut(&handle) {
                        let mut st = StreamState::new(dec);
                        st.looping = w.looping;
                        st.volume = w.volume;
                        st.pan = w.pan;
                        w.wav = None;
                        w.stream = Some(st);
                    }
                    Response { snderr: SNDERR_OK, value: 1, extra1: dur, ..Default::default() }
                }
                Err(e) => {
                    log::warn!("civ3-audio: stream open failed for {}: {e}", host_path.display());
                    Response { snderr: SNDERR_INVALID_FORMAT, ..Default::default() }
                }
            }
        } else {
            match self.load_cached_wav(&host_path) {
                Ok(wav) => {
                    let dur = wav.duration_ms();
                    if let Some(SoundSlot::Wave(w)) = self.sounds.get_mut(&handle) {
                        w.stream = None;
                        w.wav = Some(wav);
                    }
                    Response { snderr: SNDERR_OK, value: 0, extra1: dur, ..Default::default() }
                }
                Err(err) => Response { snderr: err, ..Default::default() },
            }
        }
    }

    fn load_amb(&mut self, handle: SoundHandle, amb_path: &Path, flags_58: u32) -> Response {
        let Ok(bytes) = std::fs::read(amb_path) else {
            return Response { snderr: SNDERR_FILE_OPEN, ..Default::default() };
        };
        let script = match parse_amb(&bytes) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("civ3-audio: failed parsing .amb {}: {e}", amb_path.display());
                return Response { snderr: SNDERR_INVALID_FORMAT, ..Default::default() };
            }
        };

        let relative = (flags_58 & 1) != 0;
        let mut child_wavs: Vec<Arc<DecodedWav>> = Vec::new();
        let mut root_notes: Vec<u8> = Vec::new();
        let mut wav_index_by_path: HashMap<PathBuf, usize> = HashMap::new();
        let mut resolved_programs: Vec<ResolvedProgram> = Vec::new();

        for prgm in &script.programs {
            let kmap = script
                .keymaps
                .iter()
                .find(|k| k.name == prgm.keymap_name)
                .or_else(|| {
                    script
                        .keymaps
                        .iter()
                        .find(|k| k.name.eq_ignore_ascii_case(&prgm.keymap_name))
                });
            let mut note_map = [None; 128];
            let mut all_wavs = Vec::new();
            let (kmap_flags, release_ms) = if let Some(km) = kmap {
                for entry in &km.entries {
                    if let Some(wp) = resolve_amb_wav(amb_path, &entry.wav_name, relative) {
                        let idx = if let Some(&existing) = wav_index_by_path.get(&wp) {
                            existing
                        } else if let Ok(wav) = self.load_cached_wav(&wp) {
                            let idx = child_wavs.len();
                            root_notes.push(wav.root_note);
                            child_wavs.push(wav);
                            wav_index_by_path.insert(wp, idx);
                            idx
                        } else {
                            continue;
                        };
                        all_wavs.push(idx);
                        let lo = entry.low_note.min(entry.high_note) as usize;
                        let hi = entry.high_note.max(entry.low_note).min(127) as usize;
                        for slot in &mut note_map[lo..=hi] {
                            *slot = Some(idx);
                        }
                    }
                }
                (km.flags, km.release_ms)
            } else {
                (0, 250)
            };

            resolved_programs.push(ResolvedProgram {
                id: prgm.id,
                flags: prgm.flags,
                pitch_min: prgm.pitch_min,
                pitch_max: prgm.pitch_max,
                vol_min: prgm.vol_min,
                vol_max: prgm.vol_max,
                kmap_flags,
                release_ms,
                note_map,
                all_wavs,
            });
        }

        let seed = self.amb_rng_seed;
        self.amb_rng_seed = self.amb_rng_seed.wrapping_add(1);
        let mut seq = AmbSequencer::new(&script, resolved_programs, seed);
        if let Some(SoundSlot::Amb(a)) = self.sounds.get_mut(&handle) {
            a.flags_58 |= flags_58;
            seq.flags_58 = a.flags_58;
            seq.ms_per_frame = a.ms_per_frame;
            seq.volume = a.volume;
            seq.pan = a.pan;
            seq.looping = a.looping;
            a.sequencer = Some(seq);
            a.child_wavs = child_wavs;
            a.root_notes = root_notes;
        }
        Response::default()
    }

    fn cmd_play(&mut self, handle: SoundHandle) -> Response {
        match self.sounds.get_mut(&handle) {
            Some(SoundSlot::Wave(w)) => {
                if let Some(st) = &mut w.stream {
                    st.volume = w.volume;
                    st.pan = w.pan;
                    st.looping = w.looping;
                    st.paused = false;
                    if !st.playing {
                        st.fade = None;
                        st.rewind();
                        st.playing = true;
                    }
                    return Response::default();
                }
                let Some(wav) = w.wav.clone() else {
                    return Response { snderr: SNDERR_UNLOADED, ..Default::default() };
                };
                if self.has_recent_voice_for_wav(&wav) {
                    return Response::default();
                }
                // Enforce 16-voice polyphony cap per WaveSound.
                let active_count = self.sw_voices.iter().filter(|v| v.owner == handle).count();
                if active_count >= MAX_VOICES_PER_SOUND {
                    if let Some(idx) = self.sw_voices.iter().position(|v| v.owner == handle) {
                        self.sw_voices.remove(idx);
                    }
                }
                let (volume, pan, looping, pitch_cents, fade) = match self.sounds.get_mut(&handle) {
                    Some(SoundSlot::Wave(w)) => {
                        (w.volume, w.pan, w.looping, w.pitch_cents, w.active_fade.take())
                    }
                    _ => return Response { snderr: SNDERR_NO_OBJECT, ..Default::default() },
                };
                let ratio = pitch_ratio_from_cents(pitch_cents);
                let step = (f64::from(wav.sample_rate) / f64::from(MIX_SAMPLE_RATE)) * ratio;
                let voice = SwVoice {
                    wav,
                    pos: 0.0,
                    step,
                    volume,
                    pan,
                    looping,
                    fade,
                    amb_gate: None,
                    owner: handle,
                    started_at_frame: self.total_frames_rendered,
                };
                self.sw_voices.push(voice);
                Response::default()
            }
            Some(SoundSlot::Amb(a)) => {
                let Some(seq) = &mut a.sequencer else {
                    return Response { snderr: SNDERR_UNLOADED, ..Default::default() };
                };
                seq.flags_58 = a.flags_58;
                seq.ms_per_frame = a.ms_per_frame;
                seq.volume = a.volume;
                seq.pan = a.pan;
                seq.looping = a.looping;
                seq.start();
                Response::default()
            }
            Some(SoundSlot::Playlist(_)) => self.cmd_playlist_play(handle),
            Some(SoundSlot::Stub) => Response::default(),
            None => Response { snderr: SNDERR_NO_OBJECT, ..Default::default() },
        }
    }

    fn cmd_stop(&mut self, handle: SoundHandle, force_kill: bool) {
        match self.sounds.get_mut(&handle) {
            Some(SoundSlot::Wave(w)) => {
                if let Some(st) = &mut w.stream {
                    st.playing = false;
                    st.fade = None;
                }
                self.sw_voices.retain(|v| v.owner != handle);
            }
            Some(SoundSlot::Amb(a)) => {
                let ring_out = (a.flags_58 & 2) != 0 && !force_kill;
                if let Some(seq) = &mut a.sequencer {
                    seq.stop();
                }
                if !ring_out {
                    self.sw_voices.retain(|v| v.owner != handle);
                }
            }
            Some(SoundSlot::Playlist(p)) => {
                let cur = p.current;
                if let Some(h) = cur {
                    self.cmd_stop(h, true);
                }
            }
            _ => {}
        }
    }

    fn cmd_fade_out(&mut self, handle: SoundHandle) -> Response {
        match self.sounds.get(&handle) {
            Some(SoundSlot::Wave(w)) => {
                let ms = w.fade_out_ms.max(1);
                self.cmd_fade(handle, 0, -1, ms as i32)
            }
            Some(SoundSlot::Amb(_)) => {
                self.cmd_stop(handle, false);
                Response::default()
            }
            Some(SoundSlot::Playlist(_)) => {
                // Type8::vt[+0x28] fades out current track over 2000ms and clears it.
                self.cmd_playlist_fade(handle, 0, 2000)
            }
            _ => Response::default(),
        }
    }

    fn cmd_fade_in(&mut self, handle: SoundHandle) -> Response {
        if let Some(SoundSlot::Wave(w)) = self.sounds.get(&handle) {
            let ms = w.fade_in_ms.max(1);
            let vol = w.volume;
            return self.cmd_fade(handle, vol, 0, ms as i32);
        }
        Response::default()
    }

    /// `WaveSound::vt[+0x34](target_vol, start_vol, duration_ms)` where `-1`
    /// means "current volume" (`0x1003C2F0`).
    fn cmd_fade(&mut self, handle: SoundHandle, target_arg: i32, start_arg: i32, ms: i32) -> Response {
        let duration_ms = ms.max(1) as u32;
        if let Some(SoundSlot::Wave(w)) = self.sounds.get_mut(&handle) {
            let cur_norm = sw_volume_fixed(w.volume) as f64 / 65_536.0;
            let start = if start_arg < 0 {
                if let Some(s) = &w.stream {
                    s.fade.as_ref().map(|f| f.current).unwrap_or(w.volume as f64 / 127.0)
                } else {
                    cur_norm
                }
            } else if w.is_ds_stream() {
                start_arg.clamp(0, 127) as f64 / 127.0
            } else {
                sw_volume_fixed(start_arg) as f64 / 65_536.0
            };
            let target = if target_arg < 0 {
                if w.is_ds_stream() {
                    w.volume as f64 / 127.0
                } else {
                    cur_norm
                }
            } else if w.is_ds_stream() {
                target_arg.clamp(0, 127) as f64 / 127.0
            } else {
                sw_volume_fixed(target_arg) as f64 / 65_536.0
            };
            let stop_at_zero = target <= 0.0;
            let fade = FadeState::new(start, target, duration_ms, stop_at_zero);
            if let Some(st) = &mut w.stream {
                st.fade = Some(fade);
            } else {
                let mut applied = false;
                for v in &mut self.sw_voices {
                    if v.owner == handle {
                        v.fade = Some(fade.clone());
                        applied = true;
                    }
                }
                if !applied {
                    w.active_fade = Some(fade);
                }
            }
        }
        Response::default()
    }

    fn cmd_set_volume(&mut self, handle: SoundHandle, vol: i32) {
        let v = vol.clamp(0, 127);
        match self.sounds.get_mut(&handle) {
            Some(SoundSlot::Wave(w)) => {
                w.volume = v;
                if let Some(st) = &mut w.stream {
                    st.volume = v;
                    st.fade = None;
                }
                for voice in &mut self.sw_voices {
                    if voice.owner == handle {
                        voice.volume = v;
                    }
                }
            }
            Some(SoundSlot::Amb(a)) => {
                a.volume = v;
                if let Some(seq) = &mut a.sequencer {
                    seq.volume = v;
                }
            }
            Some(SoundSlot::Playlist(p)) => {
                p.volume = v;
                let cur = p.current;
                if let Some(h) = cur {
                    self.cmd_set_volume(h, v);
                }
            }
            _ => {}
        }
    }

    fn cmd_set_pan(&mut self, handle: SoundHandle, pan: i32) {
        let p = pan.clamp(-64, 63);
        match self.sounds.get_mut(&handle) {
            Some(SoundSlot::Wave(w)) => {
                w.pan = p;
                if let Some(st) = &mut w.stream {
                    st.pan = p;
                }
                for voice in &mut self.sw_voices {
                    if voice.owner == handle {
                        voice.pan = p;
                    }
                }
            }
            Some(SoundSlot::Amb(a)) => {
                a.pan = p;
                if let Some(seq) = &mut a.sequencer {
                    seq.pan = p;
                }
            }
            _ => {}
        }
    }

    fn cmd_set_loop(&mut self, handle: SoundHandle, looping: bool) {
        match self.sounds.get_mut(&handle) {
            Some(SoundSlot::Wave(w)) => {
                w.looping = looping;
                if let Some(st) = &mut w.stream {
                    st.looping = looping;
                }
                for voice in &mut self.sw_voices {
                    if voice.owner == handle {
                        voice.looping = looping;
                    }
                }
            }
            Some(SoundSlot::Amb(a)) => {
                a.looping = looping;
                if let Some(seq) = &mut a.sequencer {
                    seq.looping = looping;
                }
            }
            _ => {}
        }
    }

    fn cmd_set_pitch(&mut self, handle: SoundHandle, cents: i32) {
        if let Some(SoundSlot::Wave(w)) = self.sounds.get_mut(&handle) {
            w.pitch_cents = cents;
            let ratio = pitch_ratio_from_cents(cents);
            for voice in &mut self.sw_voices {
                if voice.owner == handle {
                    voice.step =
                        (f64::from(voice.wav.sample_rate) / f64::from(MIX_SAMPLE_RATE)) * ratio;
                }
            }
        }
    }

    fn cmd_query_state(&self, handle: SoundHandle) -> Response {
        let mut bits = 0u32;
        let mut dur = 0u32;
        let mut pos = 0u32;
        if self.is_playing(handle) {
            bits |= STATE_PLAYING;
        }
        match self.sounds.get(&handle) {
            Some(SoundSlot::Wave(w)) => {
                if w.is_loaded() {
                    bits |= STATE_LOADED;
                }
                if w.looping {
                    bits |= STATE_LOOPING;
                }
                if w.is_ds_stream() {
                    bits |= STATE_DS_BUFFER;
                }
                dur = w.duration_ms();
                pos = w.position_ms();
            }
            Some(SoundSlot::Amb(a)) => {
                if a.sequencer.is_some() {
                    bits |= STATE_LOADED;
                }
                if a.looping {
                    bits |= STATE_LOOPING;
                }
            }
            Some(SoundSlot::Playlist(p)) => {
                if p.current.is_some() || p.queued.is_some() {
                    bits |= STATE_LOADED;
                }
                if p.queued.is_some() {
                    bits |= STATE_PLAYLIST_HAS_QUEUED;
                }
            }
            _ => {}
        }
        Response { snderr: SNDERR_OK, value: bits, extra1: dur, extra2: pos }
    }

    fn cmd_amb_set_anim_frame(
        &mut self,
        handle: SoundHandle,
        frame: u32,
        flags_58: u32,
        ms_per_frame: u32,
    ) {
        self.amb_actions_scratch.clear();
        if let Some(SoundSlot::Amb(a)) = self.sounds.get_mut(&handle) {
            a.flags_58 = flags_58;
            a.ms_per_frame = ms_per_frame;
            if let Some(seq) = &mut a.sequencer {
                seq.flags_58 = flags_58;
                seq.ms_per_frame = ms_per_frame;
                seq.set_anim_frame(frame, &a.root_notes, &mut self.amb_actions_scratch);
            }
        }
        self.apply_amb_actions(handle);
    }

    fn apply_amb_actions(&mut self, handle: SoundHandle) {
        if self.amb_actions_scratch.is_empty() {
            return;
        }
        let actions = std::mem::take(&mut self.amb_actions_scratch);
        for action in &actions {
            match *action {
                AmbAction::NoteOn {
                    wav_index,
                    channel,
                    note,
                    gated,
                    volume,
                    pan,
                    pitch_cents,
                } => {
                    let wav = match self.sounds.get(&handle) {
                        Some(SoundSlot::Amb(a)) => a.child_wavs.get(wav_index).cloned(),
                        _ => None,
                    };
                    if let Some(wav) = wav {
                        if self.has_recent_voice_for_wav(&wav) {
                            continue;
                        }
                        let ratio = pitch_ratio_from_cents(pitch_cents);
                        let step =
                            (f64::from(wav.sample_rate) / f64::from(MIX_SAMPLE_RATE)) * ratio;
                        let has_smpl_loop = wav.loop_range.is_some();
                        self.sw_voices.push(SwVoice {
                            wav,
                            pos: 0.0,
                            step,
                            volume,
                            pan,
                            looping: has_smpl_loop,
                            fade: None,
                            amb_gate: gated.then_some((handle, channel, note)),
                            owner: handle,
                            started_at_frame: self.total_frames_rendered,
                        });
                    }
                }
                AmbAction::NoteOff { channel, note, release_ms } => {
                    for v in &mut self.sw_voices {
                        if v.amb_gate == Some((handle, channel, note)) {
                            v.amb_gate = None;
                            if release_ms == 0 {
                                v.fade = Some(FadeState::new(0.0, 0.0, 1, true));
                            } else {
                                let cur = sw_volume_fixed(v.volume) as f64 / 65_536.0;
                                v.fade = Some(FadeState::new(cur, 0.0, release_ms, true));
                            }
                        }
                    }
                }
            }
        }
        self.amb_actions_scratch = actions;
        self.amb_actions_scratch.clear();
    }

    fn cmd_playlist_queue_track(&mut self, pl_handle: SoundHandle, raw_path: &str) -> Response {
        let Some(SoundSlot::Playlist(p)) = self.sounds.get(&pl_handle) else {
            return Response { snderr: SNDERR_NO_OBJECT, ..Default::default() };
        };
        if p.queued.is_some() {
            return Response { snderr: SNDERR_ALREADY_LOADED, ..Default::default() };
        }
        let cur_handle = p.current;
        let scaled_vol = p.scaled_volume;

        // Create an internal streaming WaveSound for this track.
        let track_h = self.alloc_handle();
        let mut w = WaveObject::new();
        w.flags = 0x102;
        w.looping = true;
        w.volume = scaled_vol;
        self.sounds.insert(track_h, SoundSlot::Wave(w));

        let load_res = self.cmd_load(track_h, raw_path, 0x102);
        if load_res.snderr != SNDERR_OK {
            self.sounds.remove(&track_h);
            return load_res;
        }
        if let Some(SoundSlot::Wave(w)) = self.sounds.get_mut(&track_h) {
            if let Some(st) = &mut w.stream {
                st.playlist_owner = Some(pl_handle);
            }
        }

        // Store into queued slot and chain seamlessly if a current track exists.
        if let Some(SoundSlot::Playlist(p)) = self.sounds.get_mut(&pl_handle) {
            p.queued = Some(track_h);
        }
        if let Some(cur) = cur_handle {
            if let Some(SoundSlot::Wave(cw)) = self.sounds.get_mut(&cur) {
                if let Some(cst) = &mut cw.stream {
                    cst.chained_next = Some(track_h);
                }
            }
        }
        Response { snderr: SNDERR_OK, value: track_h, ..Default::default() }
    }

    fn cmd_playlist_play(&mut self, pl_handle: SoundHandle) -> Response {
        let (cur, queued, scaled_vol) = match self.sounds.get(&pl_handle) {
            Some(SoundSlot::Playlist(p)) => (p.current, p.queued, p.scaled_volume),
            _ => return Response { snderr: SNDERR_NO_OBJECT, ..Default::default() },
        };
        if cur.is_none() && queued.is_none() {
            return Response { snderr: SNDERR_UNLOADED, ..Default::default() };
        }
        if self.is_playing(pl_handle) {
            return Response::default();
        }
        let target = if let Some(c) = cur {
            c
        } else {
            let q = queued.unwrap();
            if let Some(SoundSlot::Playlist(p)) = self.sounds.get_mut(&pl_handle) {
                p.current = Some(q);
                p.queued = None;
            }
            q
        };
        self.cmd_set_loop(target, true);
        self.cmd_set_volume(target, scaled_vol);
        self.cmd_play(target)
    }

    fn cmd_playlist_set_scaled_volume(&mut self, pl_handle: SoundHandle, vol: i32) {
        let v = vol.clamp(0, 127);
        let cur = if let Some(SoundSlot::Playlist(p)) = self.sounds.get_mut(&pl_handle) {
            p.scaled_volume = v;
            p.current
        } else {
            None
        };
        if let Some(h) = cur {
            self.cmd_set_volume(h, v);
        }
    }

    fn cmd_playlist_fade(&mut self, pl_handle: SoundHandle, dir: i32, ms: i32) -> Response {
        let (cur, scaled_vol) = match self.sounds.get(&pl_handle) {
            Some(SoundSlot::Playlist(p)) => (p.current, p.scaled_volume),
            _ => return Response::default(),
        };
        let Some(h) = cur else { return Response::default() };
        if dir == 0 {
            self.cmd_fade(h, 0, -1, ms)
        } else {
            self.cmd_fade(h, scaled_vol, 0, ms)
        }
    }

    /// Renders `out.len() / 2` stereo frames at 44100 Hz into interleaved `i16`.
    pub fn render_frames(&mut self, out: &mut [i16]) {
        let frames = out.len() / 2;
        if frames == 0 {
            return;
        }
        if self.wave_paused {
            out.fill(0);
            return;
        }

        // Advance free-running `.amb` sequencers in small sub-blocks (1 ms = 44 frames)
        // for crisp sample-accurate MIDI timing without per-sample HashMap scans.
        const SUB_BLOCK: usize = 44;
        let mut done_frames = 0usize;
        let sw_bus_gain = ds_volume_gain(127, self.master_volume);

        // Collect handles of active streaming WaveSounds once per render call.
        let mut stream_handles: Vec<SoundHandle> = Vec::new();
        let mut amb_handles: Vec<SoundHandle> = Vec::new();
        for (&h, slot) in &self.sounds {
            match slot {
                SoundSlot::Wave(w) if w.stream.as_ref().is_some_and(|s| s.playing && !s.paused) => {
                    stream_handles.push(h);
                }
                SoundSlot::Amb(a) if a.sequencer.as_ref().is_some_and(|s| s.playing) => {
                    amb_handles.push(h);
                }
                _ => {}
            }
        }
        // Sort handles for strict determinism across HashMap iteration order.
        stream_handles.sort_unstable();
        amb_handles.sort_unstable();

        while done_frames < frames {
            let chunk = (frames - done_frames).min(SUB_BLOCK);

            // Step `.amb` sequencers (unless MIDI device paused).
            if !self.midi_paused && !amb_handles.is_empty() {
                let dt_ms = (chunk as f64 * 1000.0) / f64::from(MIX_SAMPLE_RATE);
                for &ah in &amb_handles {
                    self.amb_actions_scratch.clear();
                    if let Some(SoundSlot::Amb(a)) = self.sounds.get_mut(&ah) {
                        if let Some(seq) = &mut a.sequencer {
                            seq.advance(dt_ms, &a.root_notes, &mut self.amb_actions_scratch);
                        }
                    }
                    self.apply_amb_actions(ah);
                }
            }

            let mut promoted: Vec<(SoundHandle, SoundHandle, Option<SoundHandle>)> = Vec::new();

            for i in 0..chunk {
                let mut sw_l = 0.0_f64;
                let mut sw_r = 0.0_f64;

                // 1. Software-mixed WAV / `.amb` voices.
                let mut vi = 0;
                while vi < self.sw_voices.len() {
                    if self.sw_voices[vi].mix_frame(&mut sw_l, &mut sw_r) {
                        vi += 1;
                    } else {
                        self.sw_voices.swap_remove(vi);
                    }
                }

                let mut sum_l = sw_l * sw_bus_gain;
                let mut sum_r = sw_r * sw_bus_gain;

                // 2. DirectSound streaming tracks (MP3 music / playlists).
                for &sh in &stream_handles {
                    let Some(SoundSlot::Wave(w)) = self.sounds.get_mut(&sh) else { continue };
                    let Some(st) = &mut w.stream else { continue };
                    if !st.playing || st.paused {
                        continue;
                    }
                    let eff_vol = if let Some(fade) = &mut st.fade {
                        let v = (fade.current * 127.0).trunc() as i32;
                        let finished = fade.step();
                        if finished {
                            let stop_now = fade.stop_at_zero && fade.target <= 0.0;
                            st.fade = None;
                            if stop_now {
                                st.playing = false;
                                continue;
                            }
                        }
                        v
                    } else {
                        st.volume
                    };

                    if let Some((sl, sr)) = st.pull_44100_frame() {
                        let gain = ds_volume_gain(eff_vol, self.master_volume);
                        let (pl, pr) = ds_pan_gains(st.pan);
                        sum_l += sl * gain * pl;
                        sum_r += sr * gain * pr;
                    } else {
                        // Stream reached EOF! Check for seamless chained next track.
                        st.playing = false;
                        if let Some(next_h) = st.chained_next.take() {
                            promoted.push((sh, next_h, st.playlist_owner));
                        }
                    }
                }

                // Handle any seamless playlist handoff immediately so the next
                // frame pulls from the new track with zero gap!
                if !promoted.is_empty() {
                    for (old_h, next_h, pl_owner) in promoted.drain(..) {
                        let (vol, pan, fade) = match self.sounds.get_mut(&old_h) {
                            Some(SoundSlot::Wave(w)) => {
                                let f = w.stream.as_mut().and_then(|s| s.fade.take());
                                (w.volume, w.pan, f)
                            }
                            _ => (127, 0, None),
                        };
                        if let Some(SoundSlot::Wave(nw)) = self.sounds.get_mut(&next_h) {
                            nw.volume = vol;
                            nw.pan = pan;
                            if let Some(nst) = &mut nw.stream {
                                nst.volume = vol;
                                nst.pan = pan;
                                nst.fade = fade;
                                nst.looping = true;
                                nst.playing = true;
                                nst.paused = false;
                            }
                        }
                        if let Some(pl) = pl_owner {
                            if let Some(SoundSlot::Playlist(p)) = self.sounds.get_mut(&pl) {
                                p.current = Some(next_h);
                                p.queued = None;
                            }
                        }
                        self.sounds.remove(&old_h);
                        stream_handles.push(next_h);
                    }
                }

                let idx = (done_frames + i) * 2;
                out[idx] = clamp_i16(sum_l);
                out[idx + 1] = clamp_i16(sum_r);
            }

            done_frames += chunk;
            self.total_frames_rendered = self.total_frames_rendered.saturating_add(chunk as u64);
        }

        if let Some(rec) = &mut self.recorder {
            rec.write_samples(&out[..frames * 2]);
        }
    }

    /// Renders stereo `f32` samples in `[-1.0, 1.0]` at 44100 Hz.
    pub fn render_f32(&mut self, out: &mut [f32], scratch_i16: &mut Vec<i16>) {
        scratch_i16.resize(out.len(), 0);
        self.render_frames(scratch_i16);
        const SCALE: f32 = 1.0 / 32_768.0;
        for (dst, &src) in out.iter_mut().zip(scratch_i16.iter()) {
            *dst = f32::from(src) * SCALE;
        }
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_wav(sample_rate: u32, samples: Vec<i16>) -> Arc<DecodedWav> {
        Arc::new(DecodedWav {
            sample_rate,
            channels: 1,
            samples,
            loop_range: None,
            root_note: 60,
        })
    }

    #[test]
    fn sw_mixer_renders_exact_unity_pan_and_volume() {
        let mut engine = AudioEngine::new();
        let h = engine.alloc_handle();
        let mut w = WaveObject::new();
        w.wav = Some(synthetic_wav(44_100, vec![10_000, -10_000, 20_000, -20_000]));
        w.volume = 64; // V = 33280 -> gain = 33280/65536 = 0.5078125
        w.pan = -32; // V_L = 49920, V_R = 16640
        engine.sounds.insert(h, SoundSlot::Wave(w));
        assert_eq!(engine.cmd_play(h).snderr, SNDERR_OK);
        assert!(engine.is_playing(h));

        let mut out = [0i16; 12]; // 6 frames
        engine.render_frames(&mut out);
        assert_eq!(out[0], clamp_i16(10_000.0 * (49_920.0 / 65_536.0)));
        assert_eq!(out[1], clamp_i16(10_000.0 * (16_640.0 / 65_536.0)));
        assert_eq!(out[2], clamp_i16(-10_000.0 * (49_920.0 / 65_536.0)));
        assert_eq!(out[3], clamp_i16(-10_000.0 * (16_640.0 / 65_536.0)));
        // Past end of 4-frame WAV -> silent and no longer playing.
        assert_eq!((out[8], out[9]), (0, 0));
        assert!(!engine.is_playing(h));
    }

    #[test]
    fn sw_mixer_22050_linear_upsamples_matching_dsound() {
        let mut engine = AudioEngine::new();
        let h = engine.alloc_handle();
        let mut w = WaveObject::new();
        w.wav = Some(synthetic_wav(22_050, vec![0, 1000, 2000, 3000]));
        engine.sounds.insert(h, SoundSlot::Wave(w));
        engine.cmd_play(h);

        let mut out = [0i16; 16]; // 8 frames at 44100 Hz
        engine.render_frames(&mut out);
        // Even frames = exact samples; odd frames = linear midpoints.
        assert_eq!(out[0], 0);
        assert_eq!(out[2], 500);
        assert_eq!(out[4], 1000);
        assert_eq!(out[6], 1500);
        assert_eq!(out[8], 2000);
        assert_eq!(out[10], 2500);
        assert_eq!(out[12], 3000);
    }

    #[test]
    fn fade_out_reaches_zero_and_stops_voice() {
        let mut engine = AudioEngine::new();
        let h = engine.alloc_handle();
        let mut w = WaveObject::new();
        w.wav = Some(synthetic_wav(44_100, vec![10_000; 44_100]));
        engine.sounds.insert(h, SoundSlot::Wave(w));
        engine.cmd_play(h);
        engine.cmd_fade(h, 0, -1, 10); // 10ms = 441 frames

        let mut out = vec![0i16; 1000 * 2];
        engine.render_frames(&mut out);
        assert!(out[0] > 9000);
        assert!(out[220 * 2] > 4000 && out[220 * 2] < 6000);
        assert_eq!(out[600 * 2], 0);
        assert!(!engine.is_playing(h));
    }

    #[test]
    fn renders_real_civ3_wav_mp3_amb_and_playlist_if_present() {
        let conquests = Path::new("/mnt/datasets/winvm/civ3-src/Civilization III Complete/Conquests");
        if !conquests.is_dir() {
            return;
        }
        let mut engine = AudioEngine::new();
        engine.set_base_dir(conquests);
        engine.set_rng_seed(0xC1F3);
        engine.handle_request(RequestHeader::new(Op::WaveOpen, 0), "");
        engine.handle_request(RequestHeader::new(Op::MidiOpen, 0), "");

        // 1. Short WAV SFX
        let r_wav = engine.handle_request(
            RequestHeader::new(Op::CreateSound, 0).with_a(SoundKind::Wave as i32),
            "",
        );
        assert_eq!(r_wav.snderr, SNDERR_OK);
        let h_wav = r_wav.value;
        let load_wav = engine.handle_request(
            RequestHeader::new(Op::Load, h_wav),
            r"..\Sounds\Button1.wav",
        );
        assert_eq!(load_wav.snderr, SNDERR_OK);
        assert_eq!(engine.handle_request(RequestHeader::new(Op::Play, h_wav), "").snderr, SNDERR_OK);
        assert!(engine.is_playing(h_wav));
        let mut buf = vec![0i16; 4096 * 2];
        engine.render_frames(&mut buf);
        assert!(buf.iter().any(|&s| s != 0), "WAV SFX produced silence");

        // 2. Real Civ3 .amb unit animation sound (sync mode with frame advance)
        let r_amb = engine.handle_request(
            RequestHeader::new(Op::CreateSound, 0).with_a(SoundKind::Amb as i32),
            "",
        );
        assert_eq!(r_amb.snderr, SNDERR_OK);
        let h_amb = r_amb.value;
        let load_amb = engine.handle_request(
            RequestHeader::new(Op::Load, h_amb).with_a(1), // relative WAV resolution
            r"..\Art\Units\Cavalry\CavalryAttack.amb",
        );
        if load_amb.snderr == SNDERR_OK {
            engine.handle_request(
                RequestHeader::new(Op::AmbSyncMeta, h_amb).with_a(3).with_b(66),
                "",
            );
            assert_eq!(engine.handle_request(RequestHeader::new(Op::Play, h_amb), "").snderr, SNDERR_OK);
            let mut amb_energy = 0i64;
            for frame in 0..16 {
                engine.handle_request(
                    RequestHeader::new(Op::AmbSetAnimFrame, h_amb)
                        .with_a(frame)
                        .with_b(3)
                        .with_c(66),
                    "",
                );
                let mut fbuf = vec![0i16; 2910 * 2]; // ~66ms
                engine.render_frames(&mut fbuf);
                amb_energy += fbuf.iter().map(|&s| i64::from(s).abs()).sum::<i64>();
            }
            assert!(amb_energy > 0, ".amb sequencer produced zero audio energy");
        }

        // 3. Real Civ3 Type 8 playlist (MP3 music stream)
        let r_pl = engine.handle_request(
            RequestHeader::new(Op::CreateSound, 0).with_a(SoundKind::Playlist as i32),
            "",
        );
        assert_eq!(r_pl.snderr, SNDERR_OK);
        let h_pl = r_pl.value;
        let q1 = engine.handle_request(
            RequestHeader::new(Op::PlaylistQueueTrack, h_pl),
            r"..\Sounds\build\Ancient\AncECfull.mp3",
        );
        assert_eq!(q1.snderr, SNDERR_OK);
        assert_eq!(engine.handle_request(RequestHeader::new(Op::Play, h_pl), "").snderr, SNDERR_OK);
        let mut mp3_buf = vec![0i16; 22_050 * 2];
        engine.render_frames(&mut mp3_buf);
        assert!(mp3_buf.iter().any(|&s| s != 0), "MP3 playlist produced silence");
    }

    #[test]
    fn dedup_coalesces_repeated_starts_within_50ms_and_allows_after_window() {
        assert_eq!(SAME_SOUND_DEDUP_MS, 50);
        assert_eq!(SAME_SOUND_DEDUP_FRAMES, 2_205);

        let mut engine = AudioEngine::new();
        let wav_a = synthetic_wav(44_100, vec![10_000; 22_050]);
        let wav_b = synthetic_wav(44_100, vec![5_000; 22_050]);

        let h1 = engine.alloc_handle();
        let mut w1 = WaveObject::new();
        w1.wav = Some(wav_a.clone());
        engine.sounds.insert(h1, SoundSlot::Wave(w1));

        let h2 = engine.alloc_handle();
        let mut w2 = WaveObject::new();
        w2.wav = Some(wav_a.clone());
        engine.sounds.insert(h2, SoundSlot::Wave(w2));

        let h_other = engine.alloc_handle();
        let mut wo = WaveObject::new();
        wo.wav = Some(wav_b);
        engine.sounds.insert(h_other, SoundSlot::Wave(wo));

        // 1. Blast 10 starts of the same sample within < 50 ms (same handle & second handle).
        assert_eq!(engine.cmd_play(h1).snderr, SNDERR_OK);
        for _ in 0..5 {
            assert_eq!(engine.cmd_play(h1).snderr, SNDERR_OK);
        }
        let mut tmp = vec![0i16; 1_000 * 2]; // ~22.7 ms (< 50 ms)
        engine.render_frames(&mut tmp);
        assert_eq!(tmp[0], 10_000, "single voice amplitude expected, no stacking");
        for _ in 0..4 {
            assert_eq!(engine.cmd_play(h2).snderr, SNDERR_OK);
        }
        assert_eq!(engine.sw_voices.len(), 1, "all starts within 50 ms must share one voice");

        // 2. Starting a different sample within 50 ms is unaffected.
        assert_eq!(engine.cmd_play(h_other).snderr, SNDERR_OK);
        assert_eq!(engine.sw_voices.len(), 2, "different sample must start immediately");
        let mut mix = vec![0i16; 10 * 2];
        engine.render_frames(&mut mix);
        assert_eq!(mix[0], 15_000, "both distinct samples should mix cleanly");

        // 3. Advance past the 50 ms window (total >= 2205 frames since first start).
        let mut advance = vec![0i16; 1_300 * 2]; // total 2310 frames > 2205
        engine.render_frames(&mut advance);
        assert_eq!(engine.cmd_play(h1).snderr, SNDERR_OK);
        assert_eq!(
            engine.sw_voices.len(),
            3,
            "start after 50 ms window must launch a new voice"
        );

        // 4. Explicit stop followed by immediate replay within 50 ms starts a fresh voice.
        engine.cmd_stop(h1, true);
        assert_eq!(engine.sw_voices.len(), 1); // only h_other remains
        assert_eq!(engine.cmd_play(h1).snderr, SNDERR_OK);
        assert_eq!(engine.sw_voices.len(), 2, "replay after explicit stop must start");
    }
}
