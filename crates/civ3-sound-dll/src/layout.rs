//! Exact 32-bit `#[repr(C)]` object layouts matching `sound.dll` and all direct
//! field accesses in `Civ3Conquests.exe`.
//!
//! Pointers are represented as `u32` so the exact byte offsets can be verified
//! by host unit tests (`x86_64`) as well as compiled directly on `i686`.

/// `Dll_Wave_Device` object (vtable has 43 slots, `0x00..0xA8`).
#[repr(C)]
pub struct WaveDeviceObj {
    pub vtable: u32,
    pub flags: u32,
    pub hwnd: u32,
    pub is_open: u32,
    pub suspended: u32,
    pub master_volume: i32,
}

/// `Dll_Midi_Device` object (vtable has 25 slots, `0x00..0x60`).
#[repr(C)]
pub struct MidiDeviceObj {
    pub vtable: u32,
    pub flags: u32,
    pub hwnd: u32,
    pub is_open: u32,
    pub suspended: u32,
}

/// `Dll_Wave_In_Device` object (vtable has 27 slots, `0x00..0x68`).
#[repr(C)]
pub struct WaveInDeviceObj {
    pub vtable: u32,
    pub flags: u32,
    pub is_open: u32,
}

/// Type 1 / Type 3 `WaveSound` object (vtable has 61 slots, `0x00..0xF0`).
#[repr(C)]
pub struct WaveSoundObj {
    pub vtable: u32,
    /// `+0x04`: volume in `0..=127` (init `127`).
    pub volume: i32,
    /// `+0x08`: pan in `-64..=63` (init `0`).
    pub pan: i32,
    pub pad_0c: [u32; 9],
    /// `+0x30`: loop flag (`0` or `1`).
    pub looping: u32,
    pub pad_34: u32,
    /// `+0x38`: fade-out duration in ms (init `1000`).
    pub fade_ms: u32,
    /// `+0x3C`: channel (`16` = unassigned).
    pub channel: u32,
    /// `+0x40`: child pointer — MUST be `0`.
    pub child_ptr: u32,
    /// `+0x44`: wave flags (`0x01`, `0x02`, `0x10`, `0x100`, ...).
    pub flags: u32,
    pub pad_48: [u32; 3],
    /// `+0x54`: sound type (`1`).
    pub sound_type: u32,
    /// `+0x58`: native `SoundHandle`.
    pub handle: u32,
    /// `+0x5C`: fade-in duration in ms (init `1000`).
    pub fade_in_ms: u32,
    /// `+0x60`: pitch shift in cents.
    pub pitch_cents: i32,
    /// `+0x64`: loaded flag (`0` or `0xFFFF_FFFF`).
    pub is_loaded: u32,
    /// `+0x68`: DirectSound streaming buffer flag (`0` or `1`).
    pub is_ds_buffer: i32,
    /// `+0x6C`: cached duration in ms.
    pub duration_ms: u32,
    /// `+0x70`: deferred path length (for `slot 34 configure` + `slot 33 load_deferred`).
    pub deferred_path_len: u32,
    /// `+0x74`: deferred path bytes (Windows-1252 / UTF-8).
    pub deferred_path: [u8; 260],
}

/// Type 5 `AmbSound` object (vtable has 78 slots, `0x00..0x134`).
#[repr(C)]
pub struct AmbSoundObj {
    pub vtable: u32,
    /// `+0x04`: volume in `0..=127` (init `127`).
    pub volume: i32,
    /// `+0x08`: pan in `-64..=63` (init `0`).
    pub pan: i32,
    pub pad_0c: [u32; 9],
    /// `+0x30`: loop flag.
    pub looping: u32,
    pub pad_34: u32,
    /// `+0x38`: fade duration in ms.
    pub fade_ms: u32,
    /// `+0x3C`: channel (`16`).
    pub channel: u32,
    /// `+0x40`: child pointer — MUST be `0` (`0x5FB570` recurses into `[obj+0x40]`).
    pub child_ptr: u32,
    /// `+0x44`: flags.
    pub flags: u32,
    pub pad_48: [u32; 3],
    /// `+0x54`: sound type (`5`).
    pub sound_type: u32,
    /// `+0x58`: written directly by `Civ3Conquests.exe`:
    /// bit 0 (`0x01`) = resolve keymap `.wav` relative to `.amb` dir;
    /// bit 1 (`0x02`) = FLC animation-sync mode & ring-out on stop.
    pub flags_58: u32,
    /// `+0x5C`: written directly by `Civ3Conquests.exe` (`0x5FB1C0`): ms per FLC frame.
    pub ms_per_frame: u32,
    /// `+0x60`: native `SoundHandle`.
    pub handle: u32,
    /// `+0x64`: current FLC frame (`slot 76` writes, `slot 77` reads).
    pub cur_frame: u32,
    /// `+0x68`: loaded flag (`0` or `0xFFFF_FFFF`).
    pub is_loaded: u32,
}

/// Stub object for Types 2, 4, 6, 7 (`VoiceSound` vtable has 38 slots, `0x00..0x94`).
#[repr(C)]
pub struct VoiceSoundObj {
    pub vtable: u32,
    pub volume: i32,
    pub pan: i32,
    pub pad_0c: [u32; 9],
    pub looping: u32,
    pub pad_34: u32,
    pub fade_ms: u32,
    pub channel: u32,
    pub child_ptr: u32,
    pub flags: u32,
    pub pad_48: [u32; 3],
    pub sound_type: u32,
    pub handle: u32,
}

/// Type 8 `MusicPlaylist` object (vtable has 36 slots, `0x00..0x8C`, exe expects >= `0x10C` bytes).
///
/// Owned directly by `Civ3Conquests.exe` at `[0xA52120]` with no wrapper.
/// The exe allocates and frees the C-string pointers in `tracks[0..40]` using
/// its own CRT heap — our destructor MUST NEVER free those pointers.
#[repr(C)]
pub struct PlaylistObj {
    pub vtable: u32,
    /// `+0x04`: volume (`0..=127`, init `127`) — read/written directly by exe.
    pub volume: i32,
    pub pan: i32,
    pub pad_0c: [u32; 9],
    pub looping: u32,
    pub pad_34: u32,
    pub fade_ms: u32,
    /// `+0x3C`: channel (`0..15`, init `16`) — read/written directly by exe.
    pub channel: u32,
    /// `+0x40`: child pointer — MUST be `0`.
    pub child_ptr: u32,
    /// `+0x44`: flags.
    pub flags: u32,
    pub pad_48: [u32; 3],
    /// `+0x54`: sound type (`8`).
    pub sound_type: u32,
    /// `+0x58..+0xF8`: `[u32; 40]` track `const char*` pointers owned by exe CRT.
    pub tracks: [u32; 40],
    /// `+0xF8`: current track marker (non-zero when playing/loaded).
    pub current_marker: u32,
    /// `+0xFC`: queued next track marker (non-zero when a next track is queued).
    pub queued_marker: u32,
    pub pad_100: u32,
    /// `+0x104`: flag returned by `vt[+0x88]` as `u8`.
    pub flag_104: u32,
    /// `+0x108`: scaled volume set by `vt[+0x8C]` (init `127`).
    pub scaled_volume: i32,
    /// `+0x10C`: native `SoundHandle` (past the original `0x10C` struct end).
    pub handle: u32,
}

/// Exact vtable slot counts verified against `sound.dll`.
pub const WAVE_DEVICE_SLOTS: usize = 43;
pub const MIDI_DEVICE_SLOTS: usize = 25;
pub const WAVE_IN_DEVICE_SLOTS: usize = 27;
pub const WAVE_SOUND_SLOTS: usize = 61;
pub const AMB_SOUND_SLOTS: usize = 78;
pub const VOICE_SOUND_SLOTS: usize = 38;
pub const PLAYLIST_SLOTS: usize = 36;

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn wave_sound_offsets_match_sound_dll() {
        assert_eq!(offset_of!(WaveSoundObj, vtable), 0x00);
        assert_eq!(offset_of!(WaveSoundObj, volume), 0x04);
        assert_eq!(offset_of!(WaveSoundObj, pan), 0x08);
        assert_eq!(offset_of!(WaveSoundObj, looping), 0x30);
        assert_eq!(offset_of!(WaveSoundObj, fade_ms), 0x38);
        assert_eq!(offset_of!(WaveSoundObj, channel), 0x3C);
        assert_eq!(offset_of!(WaveSoundObj, child_ptr), 0x40);
        assert_eq!(offset_of!(WaveSoundObj, flags), 0x44);
        assert_eq!(offset_of!(WaveSoundObj, sound_type), 0x54);
    }

    #[test]
    fn amb_sound_offsets_match_civ3_exe_direct_accesses() {
        assert_eq!(offset_of!(AmbSoundObj, vtable), 0x00);
        assert_eq!(offset_of!(AmbSoundObj, volume), 0x04);
        assert_eq!(offset_of!(AmbSoundObj, child_ptr), 0x40);
        assert_eq!(offset_of!(AmbSoundObj, flags), 0x44);
        assert_eq!(offset_of!(AmbSoundObj, sound_type), 0x54);
        assert_eq!(offset_of!(AmbSoundObj, flags_58), 0x58);
        assert_eq!(offset_of!(AmbSoundObj, ms_per_frame), 0x5C);
    }

    #[test]
    fn playlist_offsets_match_civ3_exe_direct_accesses() {
        assert_eq!(offset_of!(PlaylistObj, vtable), 0x00);
        assert_eq!(offset_of!(PlaylistObj, volume), 0x04);
        assert_eq!(offset_of!(PlaylistObj, channel), 0x3C);
        assert_eq!(offset_of!(PlaylistObj, child_ptr), 0x40);
        assert_eq!(offset_of!(PlaylistObj, flags), 0x44);
        assert_eq!(offset_of!(PlaylistObj, sound_type), 0x54);
        assert_eq!(offset_of!(PlaylistObj, tracks), 0x58);
        assert_eq!(offset_of!(PlaylistObj, current_marker), 0xF8);
        assert_eq!(offset_of!(PlaylistObj, queued_marker), 0xFC);
        assert_eq!(offset_of!(PlaylistObj, flag_104), 0x104);
        assert_eq!(offset_of!(PlaylistObj, scaled_volume), 0x108);
        assert!(size_of::<PlaylistObj>() >= 0x10C);
    }
}
