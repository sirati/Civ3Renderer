//! Parser and deterministic sequencer for Civilization III `.amb` unit
//! animation sound scripts (`SoundKind::Amb`, type 5).
//!
//! Format decoded from `sound.dll` (`0x10004200`, `0x1001FCC0`, `0x10020C70`,
//! `0x100219C0`, `0x10021E90`): sequential FourCC chunks (`prgm`, `kmap`,
//! `glbl`, optional `chnl`/`trak`/`ctrl`) followed immediately by a Standard
//! MIDI File (`MThd` + `MTrk` chunks).

use crate::dsp::amb_scale_127;

/// Program flags (`prgm` chunk `+0x04`).
pub const PRGM_RAND_PITCH: u32 = 1 << 0;
pub const PRGM_RAND_VOL: u32 = 1 << 1;
pub const PRGM_GATED: u32 = 1 << 3;
pub const PRGM_FIXED_PITCH: u32 = 1 << 4;

/// Keymap flags (`kmap` chunk `+0x04`).
pub const KMAP_RAND_ENTRY: u32 = 1 << 0;
pub const KMAP_IGNORE_VELOCITY: u32 = 1 << 3;

#[derive(Clone, Debug)]
pub struct AmbProgram {
    pub id: u32,
    pub flags: u32,
    pub pitch_max: i32,
    pub pitch_min: i32,
    pub vol_max: i32,
    pub vol_min: i32,
    pub name: String,
    pub keymap_name: String,
}

#[derive(Clone, Debug)]
pub struct AmbKeymapEntry {
    pub high_note: u8,
    pub low_note: u8,
    pub polyphony: u32,
    pub wav_name: String,
}

#[derive(Clone, Debug)]
pub struct AmbKeymap {
    pub flags: u32,
    pub name: String,
    pub entries: Vec<AmbKeymapEntry>,
    pub release_ms: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MidiEventKind {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8, velocity: u8 },
    ProgramChange { channel: u8, program: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    SetTempo { us_per_quarter: u32 },
    EndOfTrack,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiEvent {
    /// Absolute tick from start of track.
    pub tick: u32,
    pub kind: MidiEventKind,
}

#[derive(Clone, Debug)]
pub struct AmbScript {
    pub programs: Vec<AmbProgram>,
    pub keymaps: Vec<AmbKeymap>,
    pub ppq: u16,
    pub tracks: Vec<Vec<MidiEvent>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        if self.remaining() < n {
            return Err(ParseError(format!("unexpected EOF at {} (need {n})", self.pos)));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn read_u8(&mut self) -> Result<u8, ParseError> {
        Ok(self.read_bytes(1)?[0])
    }

    fn read_u16_be(&mut self) -> Result<u16, ParseError> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn read_u32_le(&mut self) -> Result<u32, ParseError> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_i32_le(&mut self) -> Result<i32, ParseError> {
        let b = self.read_bytes(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_u32_be(&mut self) -> Result<u32, ParseError> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_cstr(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        while self.pos < self.data.len() && self.data[self.pos] != 0 {
            self.pos += 1;
        }
        if self.pos >= self.data.len() {
            return Err(ParseError(format!("unterminated string at {start}")));
        }
        let bytes = &self.data[start..self.pos];
        self.pos += 1; // consume NUL
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    fn read_vlq(&mut self) -> Result<u32, ParseError> {
        let mut val = 0u32;
        for _ in 0..4 {
            let b = self.read_u8()?;
            val = (val << 7) | u32::from(b & 0x7F);
            if b & 0x80 == 0 {
                return Ok(val);
            }
        }
        Err(ParseError(format!("VLQ overflow at {}", self.pos)))
    }
}

/// Parses an `.amb` file from raw bytes.
pub fn parse_amb(bytes: &[u8]) -> Result<AmbScript, ParseError> {
    let mut cur = Cursor::new(bytes);
    let mut programs = Vec::new();
    let mut keymaps = Vec::new();

    loop {
        // Skip stray NUL padding bytes (seen in 18 CCM2 files where a hex-edited
        // keymap name exceeded the declared chunk size by 1 byte).
        while cur.remaining() > 0 && cur.data[cur.pos] == 0 {
            cur.pos += 1;
        }
        if cur.remaining() < 4 {
            return Err(ParseError("missing MThd in .amb".into()));
        }
        let tag = cur.read_bytes(4)?;
        match tag {
            b"MThd" => break,
            b"prgm" => {
                let sz = cur.read_u32_le()? as usize;
                let end = cur.pos + sz;
                let id = cur.read_u32_le()?;
                let flags = cur.read_u32_le()?;
                let pitch_max = cur.read_i32_le()?;
                let pitch_min = cur.read_i32_le()?;
                let vol_max = cur.read_i32_le()?;
                let vol_min = cur.read_i32_le()?;
                let _release_or_pad = cur.read_u32_le()?;
                let name = cur.read_cstr()?;
                let keymap_name = if cur.pos < end {
                    cur.read_cstr()?
                } else {
                    name.clone()
                };
                if cur.pos < end && end <= cur.data.len() {
                    cur.pos = end;
                }
                programs.push(AmbProgram {
                    id,
                    flags,
                    pitch_max,
                    pitch_min,
                    vol_max,
                    vol_min,
                    name,
                    keymap_name,
                });
            }
            b"kmap" => {
                // Parse sequentially by fields, matching sound.dll's mmioRead
                // sequence (`0x10004626`), so CCM2 files whose declared chunk
                // size is 1 byte off still parse cleanly without losing sync.
                let _declared_sz = cur.read_u32_le()?;
                let _ver = cur.read_u32_le()?;
                let flags = cur.read_u32_le()?;
                let _unk = cur.read_u32_le()?;
                let name = cur.read_cstr()?;
                let count = cur.read_u32_le()? as usize;
                let hdr_sz = cur.read_u32_le()? as usize;
                let mut entries = Vec::with_capacity(count.min(128));
                for _ in 0..count {
                    let hdr_start = cur.pos;
                    let high_note = if hdr_sz >= 4 { cur.read_u32_le()? as u8 } else { 127 };
                    let low_note = if hdr_sz >= 8 { cur.read_u32_le()? as u8 } else { 0 };
                    let polyphony = if hdr_sz >= 12 { cur.read_u32_le()? } else { 1 };
                    let hdr_end = hdr_start + hdr_sz;
                    if hdr_end <= cur.data.len() && cur.pos < hdr_end {
                        cur.pos = hdr_end;
                    }
                    let wav_name = cur.read_cstr()?;
                    entries.push(AmbKeymapEntry {
                        high_note: high_note.min(127),
                        low_note: low_note.min(127),
                        polyphony: polyphony.max(1),
                        wav_name,
                    });
                }
                let release_ms = cur.read_u32_le()?;
                keymaps.push(AmbKeymap { flags, name, entries, release_ms });
            }
            b"glbl" | b"chnl" | b"trak" | b"ctrl" => {
                let sz = cur.read_u32_le()? as usize;
                let _ = cur.read_bytes(sz)?;
            }
            other => {
                return Err(ParseError(format!(
                    "unexpected .amb chunk {:?} at offset {}",
                    other,
                    cur.pos - 4
                )));
            }
        }
    }

    // Parse SMF header (`MThd` tag already consumed).
    let mthd_len = cur.read_u32_be()? as usize;
    if mthd_len < 6 {
        return Err(ParseError("MThd too short".into()));
    }
    let mthd_end = cur.pos + mthd_len;
    let _format = cur.read_u16_be()?;
    let ntrks = cur.read_u16_be()? as usize;
    let division = cur.read_u16_be()?;
    let ppq = if division & 0x8000 == 0 { division.max(1) } else { 480 };
    if mthd_end <= cur.data.len() {
        cur.pos = mthd_end;
    }

    let mut tracks = Vec::with_capacity(ntrks);
    for _ in 0..ntrks {
        if cur.remaining() < 8 {
            break;
        }
        let tag = cur.read_bytes(4)?;
        if tag != b"MTrk" {
            return Err(ParseError(format!("expected MTrk, got {tag:?}")));
        }
        let len = cur.read_u32_be()? as usize;
        let trk_bytes = cur.read_bytes(len)?;
        tracks.push(parse_midi_track(trk_bytes)?);
    }

    Ok(AmbScript { programs, keymaps, ppq, tracks })
}

fn parse_midi_track(bytes: &[u8]) -> Result<Vec<MidiEvent>, ParseError> {
    let mut cur = Cursor::new(bytes);
    let mut events = Vec::new();
    let mut abs_tick = 0u32;
    let mut running_status = 0u8;

    while cur.remaining() > 0 {
        let delta = cur.read_vlq()?;
        abs_tick = abs_tick.saturating_add(delta);
        let peek = cur.data[cur.pos];
        let status = if peek & 0x80 != 0 {
            cur.pos += 1;
            if peek < 0xF0 {
                running_status = peek;
            }
            peek
        } else {
            if running_status == 0 {
                return Err(ParseError(format!("missing running status at {}", cur.pos)));
            }
            running_status
        };

        match status & 0xF0 {
            0x80 => {
                let note = cur.read_u8()?;
                let velocity = cur.read_u8()?;
                events.push(MidiEvent {
                    tick: abs_tick,
                    kind: MidiEventKind::NoteOff { channel: status & 0x0F, note, velocity },
                });
            }
            0x90 => {
                let note = cur.read_u8()?;
                let velocity = cur.read_u8()?;
                let kind = if velocity == 0 {
                    MidiEventKind::NoteOff { channel: status & 0x0F, note, velocity: 0 }
                } else {
                    MidiEventKind::NoteOn { channel: status & 0x0F, note, velocity }
                };
                events.push(MidiEvent { tick: abs_tick, kind });
            }
            0xA0 | 0xE0 => {
                let _ = cur.read_bytes(2)?;
            }
            0xB0 => {
                let controller = cur.read_u8()?;
                let value = cur.read_u8()?;
                events.push(MidiEvent {
                    tick: abs_tick,
                    kind: MidiEventKind::ControlChange {
                        channel: status & 0x0F,
                        controller,
                        value,
                    },
                });
            }
            0xC0 => {
                let program = cur.read_u8()?;
                events.push(MidiEvent {
                    tick: abs_tick,
                    kind: MidiEventKind::ProgramChange { channel: status & 0x0F, program },
                });
            }
            0xD0 => {
                let _ = cur.read_u8()?;
            }
            0xF0 => match status {
                0xFF => {
                    let meta_type = cur.read_u8()?;
                    let len = cur.read_vlq()? as usize;
                    let payload = cur.read_bytes(len)?;
                    if meta_type == 0x51 && payload.len() >= 3 {
                        let us = (u32::from(payload[0]) << 16)
                            | (u32::from(payload[1]) << 8)
                            | u32::from(payload[2]);
                        events.push(MidiEvent {
                            tick: abs_tick,
                            kind: MidiEventKind::SetTempo { us_per_quarter: us },
                        });
                    } else if meta_type == 0x2F {
                        events.push(MidiEvent { tick: abs_tick, kind: MidiEventKind::EndOfTrack });
                        break;
                    }
                }
                0xF0 | 0xF7 => {
                    let len = cur.read_vlq()? as usize;
                    let _ = cur.read_bytes(len)?;
                }
                _ => {}
            },
            _ => {}
        }
    }

    Ok(events)
}

/// Deterministic LCG used for `.amb` random pitch/volume/keymap variations
/// when enabled on a program/keymap.
#[derive(Clone, Debug)]
pub struct AmbRng {
    state: u32,
}

impl AmbRng {
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    pub fn next_u16(&mut self) -> u16 {
        self.state = self.state.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.state >> 16) & 0x7FFF) as u16
    }

    pub fn range_i32(&mut self, min: i32, max: i32) -> i32 {
        let span = max - min;
        if span <= 0 {
            return min;
        }
        let r = i32::from(self.next_u16()) % (span + 1);
        min + r
    }
}

/// Action emitted by the `.amb` sequencer for the mixer to execute on child WAVs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AmbAction {
    NoteOn {
        /// Index into the `.amb` sound's preloaded child WAV list.
        wav_index: usize,
        /// Program ID & MIDI note key for gated Note-Off matching.
        channel: u8,
        note: u8,
        gated: bool,
        volume: i32,
        pan: i32,
        pitch_cents: i32,
    },
    NoteOff {
        channel: u8,
        note: u8,
        release_ms: u32,
    },
}

#[derive(Clone, Copy, Debug)]
struct ChannelState {
    program: u8,
    volume: i32,
    pan: i32,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self { program: 0, volume: 127, pan: 63 }
    }
}

/// Linked program -> keymap -> child WAV index table for runtime sequencing.
#[derive(Clone, Debug)]
pub struct ResolvedProgram {
    pub id: u32,
    pub flags: u32,
    pub pitch_min: i32,
    pub pitch_max: i32,
    pub vol_min: i32,
    pub vol_max: i32,
    pub kmap_flags: u32,
    pub release_ms: u32,
    /// Maps MIDI note `0..128` -> `Option<usize>` child WAV index.
    pub note_map: [Option<usize>; 128],
    /// Unique child WAV indices referenced by this keymap (for `KMAP_RAND_ENTRY`).
    pub all_wavs: Vec<usize>,
}

/// Runtime `.amb` sequencer state, stepped directly inside the 44100 Hz mixer.
#[derive(Clone, Debug)]
pub struct AmbSequencer {
    pub ppq: u16,
    pub tracks: Vec<Vec<MidiEvent>>,
    pub programs: Vec<ResolvedProgram>,
    pub playing: bool,
    pub looping: bool,
    /// Exe-visible flags at `AmbSound+0x58`: bit 0 = relative path, bit 1 = anim sync & ring-out on stop.
    pub flags_58: u32,
    /// Animation `ms_per_frame` written by exe at `AmbSound+0x5C`.
    pub ms_per_frame: u32,
    /// Current animation frame updated by `AmbSound::vt[+0x130]`.
    pub cur_frame: u32,
    pub volume: i32,
    pub pan: i32,
    elapsed_ms: f64,
    ms_per_tick: f64,
    track_cursors: Vec<usize>,
    channels: [ChannelState; 16],
    /// Currently held gated `(channel, note) -> (wav_index, release_ms)` flags.
    held_notes: [[bool; 128]; 16],
    rng: AmbRng,
}

impl AmbSequencer {
    pub fn new(script: &AmbScript, programs: Vec<ResolvedProgram>, rng_seed: u32) -> Self {
        let track_count = script.tracks.len();
        Self {
            ppq: script.ppq.max(1),
            tracks: script.tracks.clone(),
            programs,
            playing: false,
            looping: false,
            flags_58: 0,
            ms_per_frame: 0,
            cur_frame: 0,
            volume: 127,
            pan: 0,
            elapsed_ms: 0.0,
            // Standard MIDI default tempo: 500,000 us / quarter (120 BPM).
            ms_per_tick: 500.0 / f64::from(script.ppq.max(1)),
            track_cursors: vec![0; track_count],
            channels: [ChannelState::default(); 16],
            held_notes: [[false; 128]; 16],
            rng: AmbRng::new(rng_seed),
        }
    }

    /// Resets playback to tick 0 and marks the sequence playing.
    pub fn start(&mut self) {
        self.reset_tracks();
        self.playing = true;
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.held_notes = [[false; 128]; 16];
    }

    fn reset_tracks(&mut self) {
        self.elapsed_ms = 0.0;
        self.ms_per_tick = 500.0 / f64::from(self.ppq.max(1));
        for c in &mut self.track_cursors {
            *c = 0;
        }
        self.channels = [ChannelState::default(); 16];
        self.held_notes = [[false; 128]; 16];
    }

    /// Updates animation frame from `AmbSound::vt[+0x130]` and processes due
    /// events immediately when in animation-sync mode (`flags_58 & 2 != 0`).
    pub fn set_anim_frame(&mut self, frame: u32, root_notes: &[u8], out: &mut Vec<AmbAction>) {
        self.cur_frame = frame;
        if !self.playing || (self.flags_58 & 2) == 0 {
            return;
        }
        let target_ms = f64::from(frame) * f64::from(self.ms_per_frame);
        if target_ms < self.elapsed_ms {
            if self.looping {
                self.reset_tracks();
            } else {
                self.playing = false;
                return;
            }
        }
        self.elapsed_ms = target_ms;
        self.dispatch_due_events(root_notes, out);
    }

    /// Advances the sequencer by `dt_ms` (in free-running mode) or checks
    /// animation-sync events.
    pub fn advance(&mut self, dt_ms: f64, root_notes: &[u8], out: &mut Vec<AmbAction>) {
        if !self.playing {
            return;
        }
        if (self.flags_58 & 2) != 0 {
            // Animation-sync mode: time tracks `cur_frame * ms_per_frame`
            // (already handled on `set_anim_frame`; still check initial tick 0
            // events when frame 0 starts).
            let target_ms = f64::from(self.cur_frame) * f64::from(self.ms_per_frame);
            if target_ms >= self.elapsed_ms {
                self.elapsed_ms = target_ms;
                self.dispatch_due_events(root_notes, out);
            }
            return;
        }

        self.elapsed_ms += dt_ms;
        self.dispatch_due_events(root_notes, out);
    }

    fn dispatch_due_events(&mut self, root_notes: &[u8], out: &mut Vec<AmbAction>) {
        let mut any_remaining = false;
        for ti in 0..self.tracks.len() {
            loop {
                let idx = self.track_cursors[ti];
                let Some(ev) = self.tracks[ti].get(idx).cloned() else {
                    break;
                };
                let ev_ms = f64::from(ev.tick) * self.ms_per_tick;
                if ev_ms > self.elapsed_ms + 1e-6 {
                    any_remaining = true;
                    break;
                }
                self.track_cursors[ti] += 1;
                match ev.kind {
                    MidiEventKind::SetTempo { us_per_quarter } => {
                        if us_per_quarter > 0 {
                            self.ms_per_tick =
                                (f64::from(us_per_quarter) / 1000.0) / f64::from(self.ppq.max(1));
                        }
                    }
                    MidiEventKind::ProgramChange { channel, program } => {
                        self.channels[usize::from(channel & 0x0F)].program = program;
                    }
                    MidiEventKind::ControlChange { channel, controller, value } => {
                        let ch = &mut self.channels[usize::from(channel & 0x0F)];
                        match controller {
                            7 => ch.volume = i32::from(value & 0x7F),
                            10 => ch.pan = i32::from(value & 0x7F),
                            _ => {}
                        }
                    }
                    MidiEventKind::NoteOn { channel, note, velocity } => {
                        self.handle_note_on(channel & 0x0F, note & 0x7F, velocity & 0x7F, root_notes, out);
                    }
                    MidiEventKind::NoteOff { channel, note, .. } => {
                        self.handle_note_off(channel & 0x0F, note & 0x7F, out);
                    }
                    MidiEventKind::EndOfTrack => {
                        self.track_cursors[ti] = self.tracks[ti].len();
                        break;
                    }
                }
            }
            if self.track_cursors[ti] < self.tracks[ti].len() {
                any_remaining = true;
            }
        }

        if !any_remaining {
            if self.looping {
                self.reset_tracks();
            } else {
                self.playing = false;
            }
        }
    }

    fn find_program(&self, prog_id: u8) -> Option<usize> {
        self.programs.iter().position(|p| p.id == u32::from(prog_id))
    }

    fn handle_note_on(
        &mut self,
        channel: u8,
        note: u8,
        velocity: u8,
        root_notes: &[u8],
        out: &mut Vec<AmbAction>,
    ) {
        let ch = self.channels[usize::from(channel)];
        let Some(pi) = self.find_program(ch.program) else {
            return;
        };
        if self.held_notes[usize::from(channel)][usize::from(note)] {
            return;
        }
        let p = &self.programs[pi];
        let wav_index = if (p.kmap_flags & KMAP_RAND_ENTRY) != 0 && !p.all_wavs.is_empty() {
            let r = self.rng.next_u16() as usize % p.all_wavs.len();
            p.all_wavs[r]
        } else {
            match p.note_map[usize::from(note)] {
                Some(w) => w,
                None => return,
            }
        };

        let p_flags = p.flags;
        let kmap_flags = p.kmap_flags;
        let pitch_min = p.pitch_min;
        let pitch_max = p.pitch_max;
        let vol_min = p.vol_min;
        let vol_max = p.vol_max;
        let gated = (p_flags & PRGM_GATED) != 0;
        if gated {
            self.held_notes[usize::from(channel)][usize::from(note)] = true;
        }

        // Pitch (`0x10021B7A..0x10021BD5`):
        let rand_pitch = if (p_flags & PRGM_RAND_PITCH) != 0 {
            self.rng.range_i32(pitch_min, pitch_max)
        } else {
            0
        };
        let pitch_cents = if (p_flags & PRGM_FIXED_PITCH) == 0 {
            let root = i32::from(*root_notes.get(wav_index).unwrap_or(&60));
            (i32::from(note) - root) * 100 + rand_pitch
        } else {
            rand_pitch
        };

        // Volume (`0x10021C56..0x10021CFC`):
        let mut vol = if (kmap_flags & KMAP_IGNORE_VELOCITY) == 0 {
            let v0 = amb_scale_127(i32::from(velocity), ch.volume);
            amb_scale_127(v0, self.volume)
        } else {
            amb_scale_127(ch.volume, self.volume)
        };
        if (p_flags & PRGM_RAND_VOL) != 0 {
            let rv = self.rng.range_i32(vol_min, vol_max);
            vol += amb_scale_127(self.volume, rv);
        }
        vol = vol.clamp(0, 127);

        // Pan (`0x10021D02`): MIDI pan 0..127 -> -63..+64, plus sound pan, clamped -64..=63.
        let pan = ((ch.pan - 63) + self.pan).clamp(-64, 63);

        out.push(AmbAction::NoteOn {
            wav_index,
            channel,
            note,
            gated,
            volume: vol,
            pan,
            pitch_cents,
        });
    }

    fn handle_note_off(&mut self, channel: u8, note: u8, out: &mut Vec<AmbAction>) {
        if !self.held_notes[usize::from(channel)][usize::from(note)] {
            return;
        }
        self.held_notes[usize::from(channel)][usize::from(note)] = false;
        let ch = self.channels[usize::from(channel)];
        let release_ms = self
            .find_program(ch.program)
            .map(|pi| self.programs[pi].release_ms)
            .unwrap_or(250);
        out.push(AmbAction::NoteOff { channel, note, release_ms });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parses_all_civ3_amb_corpus_files_if_present() {
        let root = Path::new("/mnt/datasets/winvm/civ3-src/Civilization III Complete");
        if !root.is_dir() {
            return;
        }
        let mut count = 0usize;
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for entry in rd.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("amb"))
                {
                    let data = std::fs::read(&p).unwrap();
                    let parsed = parse_amb(&data)
                        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", p.display()));
                    assert!(!parsed.tracks.is_empty(), "no MIDI tracks in {}", p.display());
                    count += 1;
                }
            }
        }
        assert!(count >= 600, "expected >= 600 .amb files, found {count}");
    }

    #[test]
    fn anim_sync_advances_on_frame_and_loops_on_wrap() {
        let script = AmbScript {
            programs: vec![],
            keymaps: vec![],
            ppq: 480,
            tracks: vec![vec![
                MidiEvent {
                    tick: 0,
                    kind: MidiEventKind::ProgramChange { channel: 0, program: 1 },
                },
                MidiEvent {
                    tick: 480, // 500 ms at default 120 BPM
                    kind: MidiEventKind::NoteOn { channel: 0, note: 60, velocity: 100 },
                },
                MidiEvent { tick: 960, kind: MidiEventKind::EndOfTrack },
            ]],
        };
        let mut note_map = [None; 128];
        note_map[60] = Some(0);
        let prog = ResolvedProgram {
            id: 1,
            flags: PRGM_FIXED_PITCH,
            pitch_min: 0,
            pitch_max: 0,
            vol_min: 0,
            vol_max: 0,
            kmap_flags: 0,
            release_ms: 250,
            note_map,
            all_wavs: vec![0],
        };
        let mut seq = AmbSequencer::new(&script, vec![prog], 1);
        seq.flags_58 = 2; // anim sync
        seq.ms_per_frame = 100;
        seq.looping = true;
        seq.start();

        let mut actions = Vec::new();
        seq.set_anim_frame(4, &[60], &mut actions); // 400ms < 500ms
        assert!(actions.is_empty());
        seq.set_anim_frame(5, &[60], &mut actions); // 500ms -> NoteOn!
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], AmbAction::NoteOn { wav_index: 0, volume: 100, .. }));

        // Wrap animation back to frame 0 -> resets tracks; frame 5 triggers again!
        actions.clear();
        seq.set_anim_frame(0, &[60], &mut actions);
        assert!(actions.is_empty());
        seq.set_anim_frame(5, &[60], &mut actions);
        assert_eq!(actions.len(), 1);
    }
}
