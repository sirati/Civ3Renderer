//! Shared binary wire protocol between the 32-bit `sound.dll` shim and the
//! 64-bit native audio engine in `civ3r-native`.
//!
//! All integers are little-endian. Strings are UTF-8 (converted from
//! Windows-1252 on the 32-bit side when needed).

/// Opaque handle identifying a sound object (`WaveSound`, `AmbSound`,
/// `MusicPlaylist`, etc.) on the native engine side. `0` means invalid/none.
pub type SoundHandle = u32;

/// Sound types matching `sound.dll`'s `create_sound(..., type)` parameter.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundKind {
    Wave = 1,
    Midi = 2,
    Type3 = 3,
    Type4 = 4,
    Amb = 5,
    VoiceRx = 6,
    VoiceTx = 7,
    Playlist = 8,
}

impl SoundKind {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            1 => Some(Self::Wave),
            2 => Some(Self::Midi),
            3 => Some(Self::Type3),
            4 => Some(Self::Type4),
            5 => Some(Self::Amb),
            6 => Some(Self::VoiceRx),
            7 => Some(Self::VoiceTx),
            8 => Some(Self::Playlist),
            _ => None,
        }
    }
}

/// Command opcodes carried inside `civ3r_native::CALL_AUDIO`.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    // Global / device lifecycle
    InitTimer = 1,
    ReleaseAll = 2,
    WaveOpen = 3,
    WaveClose = 4,
    WavePause = 5,
    WaveResume = 6,
    WaveSetMasterVolume = 7,
    WaveStartRecord = 8,
    WaveStopRecord = 9,
    MidiOpen = 10,
    MidiClose = 11,
    MidiPause = 12,
    MidiResume = 13,

    // Sound object lifecycle & controls
    CreateSound = 20,
    DeleteSound = 21,
    Load = 22,
    Unload = 23,
    Play = 24,
    Stop = 25,
    FadeOut = 26,
    FadeIn = 27,
    Fade = 28,
    Pause = 29,
    SetVolume = 30,
    SetPan = 31,
    SetLoop = 32,
    SetPitch = 33,
    SetFlags = 34,
    SetFadeMs = 35,
    SetFadeInMs = 36,
    SetPositionMs = 37,
    QueryState = 38,
    PlayInstance = 39,
    StartVoice = 40,

    // AmbSound (Type 5) animation sync
    AmbSyncMeta = 50,
    AmbSetAnimFrame = 51,

    // MusicPlaylist (Type 8)
    PlaylistQueueTrack = 60,
    PlaylistSetScaledVolume = 61,
    PlaylistFade = 62,
}

impl Op {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            1 => Some(Self::InitTimer),
            2 => Some(Self::ReleaseAll),
            3 => Some(Self::WaveOpen),
            4 => Some(Self::WaveClose),
            5 => Some(Self::WavePause),
            6 => Some(Self::WaveResume),
            7 => Some(Self::WaveSetMasterVolume),
            8 => Some(Self::WaveStartRecord),
            9 => Some(Self::WaveStopRecord),
            10 => Some(Self::MidiOpen),
            11 => Some(Self::MidiClose),
            12 => Some(Self::MidiPause),
            13 => Some(Self::MidiResume),
            20 => Some(Self::CreateSound),
            21 => Some(Self::DeleteSound),
            22 => Some(Self::Load),
            23 => Some(Self::Unload),
            24 => Some(Self::Play),
            25 => Some(Self::Stop),
            26 => Some(Self::FadeOut),
            27 => Some(Self::FadeIn),
            28 => Some(Self::Fade),
            29 => Some(Self::Pause),
            30 => Some(Self::SetVolume),
            31 => Some(Self::SetPan),
            32 => Some(Self::SetLoop),
            33 => Some(Self::SetPitch),
            34 => Some(Self::SetFlags),
            35 => Some(Self::SetFadeMs),
            36 => Some(Self::SetFadeInMs),
            37 => Some(Self::SetPositionMs),
            38 => Some(Self::QueryState),
            39 => Some(Self::PlayInstance),
            40 => Some(Self::StartVoice),
            50 => Some(Self::AmbSyncMeta),
            51 => Some(Self::AmbSetAnimFrame),
            60 => Some(Self::PlaylistQueueTrack),
            61 => Some(Self::PlaylistSetScaledVolume),
            62 => Some(Self::PlaylistFade),
            _ => None,
        }
    }
}

/// Fixed-size header for every audio request sent over `CALL_AUDIO`, optionally
/// followed by a UTF-8 path string of `path_len` bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RequestHeader {
    pub op: u32,
    pub handle: SoundHandle,
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub path_len: u32,
}

impl RequestHeader {
    pub const fn new(op: Op, handle: SoundHandle) -> Self {
        Self { op: op as u32, handle, a: 0, b: 0, c: 0, path_len: 0 }
    }

    pub const fn with_a(mut self, a: i32) -> Self {
        self.a = a;
        self
    }

    pub const fn with_b(mut self, b: i32) -> Self {
        self.b = b;
        self
    }

    pub const fn with_c(mut self, c: i32) -> Self {
        self.c = c;
        self
    }
}

/// Fixed-size response returned from `CALL_AUDIO`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Response {
    /// `SNDERR` code (`0` = success).
    pub snderr: u32,
    /// Primary return value (created handle, `is_playing`, voice id, etc.).
    pub value: u32,
    /// Secondary value (e.g. duration_ms on `Load`/`QueryState`).
    pub extra1: u32,
    /// Tertiary value (e.g. position_ms or flags bitfield on `QueryState`).
    pub extra2: u32,
}

/// Bitflags returned in `Response::value` for `Op::QueryState`.
pub const STATE_LOADED: u32 = 1 << 0;
pub const STATE_PLAYING: u32 = 1 << 1;
pub const STATE_LOOPING: u32 = 1 << 2;
pub const STATE_DS_BUFFER: u32 = 1 << 3;
pub const STATE_PLAYLIST_HAS_QUEUED: u32 = 1 << 4;

/// Encodes a request header and optional UTF-8 path into `out`.
pub fn encode_request(hdr: RequestHeader, path: Option<&str>, out: &mut Vec<u8>) {
    out.clear();
    let bytes = path.map(str::as_bytes).unwrap_or(&[]);
    let mut hdr = hdr;
    hdr.path_len = bytes.len() as u32;
    out.extend_from_slice(bytemuck::bytes_of(&hdr));
    out.extend_from_slice(bytes);
}

/// Decodes a request buffer into `(RequestHeader, &str)`.
pub fn decode_request(buf: &[u8]) -> Option<(RequestHeader, &str)> {
    let hdr_size = core::mem::size_of::<RequestHeader>();
    if buf.len() < hdr_size {
        return None;
    }
    let hdr: RequestHeader = bytemuck::pod_read_unaligned(&buf[..hdr_size]);
    let path_len = hdr.path_len as usize;
    if buf.len() < hdr_size + path_len {
        return None;
    }
    let path = core::str::from_utf8(&buf[hdr_size..hdr_size + path_len]).ok()?;
    Some((hdr, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip_with_path() {
        let mut buf = Vec::new();
        let hdr = RequestHeader { op: Op::Load as u32, handle: 42, a: 1, b: -5, c: 2000, path_len: 0 };
        encode_request(hdr, Some("Sounds\\Menu\\Menu1.mp3"), &mut buf);
        let (dec, path) = decode_request(&buf).unwrap();
        assert_eq!(dec.op, Op::Load as u32);
        assert_eq!(dec.handle, 42);
        assert_eq!((dec.a, dec.b, dec.c), (1, -5, 2000));
        assert_eq!(path, "Sounds\\Menu\\Menu1.mp3");
    }

    #[test]
    fn request_rejects_truncated_buffer() {
        assert!(decode_request(&[0u8; 4]).is_none());
        let mut buf = Vec::new();
        encode_request(RequestHeader { op: Op::Play as u32, ..Default::default() }, Some("abc"), &mut buf);
        buf.pop();
        assert!(decode_request(&buf).is_none());
    }
}
