//! Audio file decoding via `symphonia` (WAV PCM/8-bit/16-bit/ADPCM and MP3).
//!
//! Short WAV sound effects are decoded into memory ([`DecodedWav`]) alongside
//! RIFF `smpl` loop/root-note metadata. MP3 music and large streaming WAVs are
//! decoded incrementally packet-by-packet via [`StreamDecoder`] so large files
//! never block startup or consume megabytes of uncompressed PCM.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Cursor;
use std::path::Path;

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Time;

#[derive(Debug, Clone)]
pub struct DecodeError(pub String);

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DecodeError {}

/// Preloaded decoded WAV sound effect in signed 16-bit PCM.
#[derive(Clone, Debug)]
pub struct DecodedWav {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved `i16` samples (`frames * channels`).
    pub samples: Vec<i16>,
    /// Optional RIFF `smpl` loop range `(start_frame, end_frame_exclusive)`.
    pub loop_range: Option<(usize, usize)>,
    /// Unity MIDI root note from RIFF `smpl` chunk (default 60 = middle C,
    /// matching `sound.dll` `0x10033D59`).
    pub root_note: u8,
}

impl DecodedWav {
    pub fn frame_count(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / usize::from(self.channels)
        }
    }

    pub fn duration_ms(&self) -> u32 {
        if self.sample_rate == 0 {
            return 0;
        }
        ((self.frame_count() as u64 * 1000) / u64::from(self.sample_rate)) as u32
    }
}

/// Extracted RIFF container metadata (`smpl` chunk and raw `data` slice for
/// format `0x55` MP3-in-WAV fallback).
struct RiffMeta {
    loop_range: Option<(usize, usize)>,
    root_note: u8,
    format_tag: u16,
    data_range: Option<(usize, usize)>,
}

fn inspect_riff_metadata(bytes: &[u8]) -> RiffMeta {
    let mut meta = RiffMeta {
        loop_range: None,
        root_note: 60,
        format_tag: 0,
        data_range: None,
    };
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return meta;
    }
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let tag = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let payload_start = pos + 8;
        let payload_end = payload_start.saturating_add(size).min(bytes.len());
        let payload = &bytes[payload_start..payload_end];

        if tag == b"fmt " && payload.len() >= 2 {
            meta.format_tag = u16::from_le_bytes([payload[0], payload[1]]);
        } else if tag == b"data" {
            meta.data_range = Some((payload_start, payload_end));
        } else if tag == b"smpl" && payload.len() >= 36 {
            // RIFF smpl chunk:
            // +12: dwMIDIUnityNote, +28: cSampleLoops, +36: loop structs (24 bytes each:
            // +8: dwStart, +12: dwEnd inclusive). Matches sound.dll 0x10033CDB..0x10033D46.
            let unity = u32::from_le_bytes([
                payload[12],
                payload[13],
                payload[14],
                payload[15],
            ]);
            if unity <= 127 {
                meta.root_note = unity as u8;
            }
            let num_loops = u32::from_le_bytes([
                payload[28],
                payload[29],
                payload[30],
                payload[31],
            ]);
            if num_loops > 0 && payload.len() >= 36 + 16 {
                let start = u32::from_le_bytes([
                    payload[36 + 8],
                    payload[36 + 9],
                    payload[36 + 10],
                    payload[36 + 11],
                ]) as usize;
                let end = u32::from_le_bytes([
                    payload[36 + 12],
                    payload[36 + 13],
                    payload[36 + 14],
                    payload[36 + 15],
                ]) as usize;
                if end >= start {
                    meta.loop_range = Some((start, end + 1));
                }
            }
        }

        let advance = 8usize.saturating_add(size).saturating_add(size & 1);
        if advance == 0 || pos.saturating_add(advance) <= pos {
            break;
        }
        pos = pos.saturating_add(advance);
    }
    meta
}

fn open_symphonia(
    source: Box<dyn MediaSource>,
    ext_hint: Option<&str>,
) -> Result<(Box<dyn FormatReader>, Box<dyn AudioDecoder>, u32, u32, u16, Option<u64>), DecodeError> {
    let mss = MediaSourceStream::new(source, MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(ext) = ext_hint {
        hint.with_extension(ext);
    }
    let format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| DecodeError(format!("probe failed: {e}")))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| DecodeError("no audio track found".into()))?;
    let track_id = track.id;
    let n_frames = track.num_frames;
    let audio_params = match track.codec_params.as_ref() {
        Some(CodecParameters::Audio(p)) => p.clone(),
        _ => return Err(DecodeError("track has no audio codec parameters".into())),
    };
    let sample_rate = audio_params.sample_rate.unwrap_or(22_050);
    let channels = audio_params
        .channels
        .as_ref()
        .map(|c| c.count() as u16)
        .unwrap_or(1)
        .max(1);
    let decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&audio_params, &AudioDecoderOptions::default())
        .map_err(|e| DecodeError(format!("codec init failed: {e}")))?;
    Ok((format, decoder, track_id, sample_rate, channels, n_frames))
}

/// Decodes a whole WAV (or MP3-in-WAV) file from disk into a [`DecodedWav`].
pub fn decode_wav_file(path: &Path) -> Result<DecodedWav, DecodeError> {
    let bytes = std::fs::read(path)
        .map_err(|e| DecodeError(format!("reading {}: {e}", path.display())))?;
    decode_wav_bytes(bytes, path.extension().and_then(|e| e.to_str()))
}

/// Decodes a whole WAV (or MP3-in-WAV / MP3) buffer in memory into [`DecodedWav`].
pub fn decode_wav_bytes(bytes: Vec<u8>, ext_hint: Option<&str>) -> Result<DecodedWav, DecodeError> {
    let meta = inspect_riff_metadata(&bytes);

    let opened = open_symphonia(Box::new(Cursor::new(bytes.clone())), ext_hint).or_else(|first_err| {
        // Fallback for format 0x55 (MP3 wrapped in RIFF WAVE, e.g. CCM2 Elvis):
        // hand the raw `data` chunk payload to Symphonia's MP3 reader+decoder.
        if meta.format_tag == 0x55 {
            if let Some((start, end)) = meta.data_range {
                let mp3_bytes = bytes[start..end].to_vec();
                return open_symphonia(Box::new(Cursor::new(mp3_bytes)), Some("mp3"));
            }
        }
        Err(first_err)
    });

    let (mut format, mut decoder, track_id, mut sample_rate, mut channels, _n_frames) = opened?;
    let mut samples: Vec<i16> = Vec::new();
    let mut tmp: Vec<i16> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(_) => break,
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = decoded.spec();
                sample_rate = spec.rate();
                channels = (spec.channels().count() as u16).max(1);
                tmp.clear();
                decoded.copy_to_vec_interleaved(&mut tmp);
                samples.extend_from_slice(&tmp);
            }
            Err(SymError::DecodeError(_)) => continue,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => break,
        }
    }

    let total_frames = if channels == 0 { 0 } else { samples.len() / usize::from(channels) };
    let loop_range = meta.loop_range.and_then(|(s, e)| {
        let e = e.min(total_frames);
        (s < e).then_some((s, e))
    });

    Ok(DecodedWav {
        sample_rate,
        channels,
        samples,
        loop_range,
        root_note: meta.root_note,
    })
}

/// Incremental packet-by-packet decoder for streaming MP3 music and large WAVs.
///
/// Produces interleaved stereo `(i16, i16)` frames on demand without decoding
/// the entire file upfront.
pub struct StreamDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    sample_rate: u32,
    channels: u16,
    total_frames: Option<u64>,
    frames_delivered: u64,
    queue: VecDeque<(i16, i16)>,
    scratch: Vec<i16>,
    eof: bool,
}

impl StreamDecoder {
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        let file = File::open(path)
            .map_err(|e| DecodeError(format!("opening {}: {e}", path.display())))?;
        let ext = path.extension().and_then(|e| e.to_str());
        let (format, decoder, track_id, sample_rate, channels, total_frames) =
            open_symphonia(Box::new(file), ext)?;
        Ok(Self {
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
            total_frames,
            frames_delivered: 0,
            queue: VecDeque::with_capacity(4096),
            scratch: Vec::with_capacity(4096),
            eof: false,
        })
    }

    pub fn from_bytes(bytes: Vec<u8>, ext_hint: Option<&str>) -> Result<Self, DecodeError> {
        let (format, decoder, track_id, sample_rate, channels, total_frames) =
            open_symphonia(Box::new(Cursor::new(bytes)), ext_hint)?;
        Ok(Self {
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
            total_frames,
            frames_delivered: 0,
            queue: VecDeque::with_capacity(4096),
            scratch: Vec::with_capacity(4096),
            eof: false,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn is_eof(&self) -> bool {
        self.eof && self.queue.is_empty()
    }

    pub fn duration_ms(&self) -> u32 {
        match self.total_frames {
            Some(frames) if self.sample_rate > 0 => {
                ((frames * 1000) / u64::from(self.sample_rate)) as u32
            }
            _ => 0,
        }
    }

    pub fn position_ms(&self) -> u32 {
        if self.sample_rate == 0 {
            return 0;
        }
        ((self.frames_delivered * 1000) / u64::from(self.sample_rate)) as u32
    }

    pub fn seek_ms(&mut self, ms: u32) {
        let secs = i64::from(ms / 1000);
        let nanos = (ms % 1000) * 1_000_000;
        let Some(time) = Time::try_new(secs, nanos) else {
            return;
        };
        let to = SeekTo::Time { time, track_id: Some(self.track_id) };
        if self.format.seek(SeekMode::Accurate, to).is_ok() {
            self.decoder.reset();
            self.queue.clear();
            self.eof = false;
            self.frames_delivered = (u64::from(ms) * u64::from(self.sample_rate)) / 1000;
        }
    }

    pub fn rewind(&mut self) {
        self.seek_ms(0);
    }

    /// Returns the next decoded stereo frame `(left, right)`, or `None` at EOF.
    pub fn next_frame(&mut self) -> Option<(i16, i16)> {
        while self.queue.is_empty() && !self.eof {
            self.decode_next_packet();
        }
        let frame = self.queue.pop_front();
        if frame.is_some() {
            self.frames_delivered += 1;
        }
        frame
    }

    fn decode_next_packet(&mut self) {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => {
                    self.eof = true;
                    return;
                }
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    self.eof = true;
                    return;
                }
                Err(SymError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(_) => {
                    self.eof = true;
                    return;
                }
            };
            if packet.track_id != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = decoded.spec();
                    self.sample_rate = spec.rate();
                    let ch = spec.channels().count().max(1);
                    self.channels = ch as u16;
                    self.scratch.clear();
                    decoded.copy_to_vec_interleaved(&mut self.scratch);
                    if ch == 1 {
                        for &s in &self.scratch {
                            self.queue.push_back((s, s));
                        }
                    } else {
                        for frame in self.scratch.chunks_exact(ch) {
                            self.queue.push_back((frame[0], frame[1]));
                        }
                    }
                    if !self.queue.is_empty() {
                        return;
                    }
                }
                Err(SymError::DecodeError(_)) => continue,
                Err(_) => {
                    self.eof = true;
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_real_civ3_wav_adpcm_mp3_in_wav_and_mp3_if_present() {
        let root = Path::new("/mnt/datasets/winvm/civ3-src/Civilization III Complete");
        if !root.is_dir() {
            return;
        }

        // 1. Standard 22050 Hz mono 16-bit WAV
        let wav = root.join("Sounds/Button.wav");
        if wav.is_file() {
            let d = decode_wav_file(&wav).expect("decode Button.wav");
            assert_eq!(d.sample_rate, 22_050);
            assert_eq!(d.channels, 1);
            assert!(d.frame_count() > 100);
        }

        // 2. Real Civ3 MP3 music via StreamDecoder
        let mp3 = root.join("Sounds/Build/Ancient/AncEC.mp3");
        if mp3.is_file() {
            let mut st = StreamDecoder::open(&mp3).expect("open AncEC.mp3");
            assert_eq!(st.sample_rate(), 44_100);
            let mut non_zero = 0usize;
            for _ in 0..44_100 {
                if let Some((l, r)) = st.next_frame() {
                    if l != 0 || r != 0 {
                        non_zero += 1;
                    }
                }
            }
            assert!(non_zero > 1000, "MP3 should produce non-zero PCM");
        }

        // 3. CCM2 format-0x55 MP3-in-WAV (if present)
        let elvis = root.join("Conquests/Scenarios/CCM2/Art/Units/Elvis/(Blue Suede Shoes).wav");
        if elvis.is_file() {
            let d = decode_wav_file(&elvis).expect("decode Elvis MP3-in-WAV");
            assert!(d.frame_count() > 1000);
        }
    }
}
