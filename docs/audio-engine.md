# Replacing the game's audio engine

Goal (user request, 2026-11): replace Civilization III: Conquests' entire 32-bit
audio stack (`sound.dll`, Stage B) with native Rust inside the 64-bit Wine
unixlib (`civ3r-native`) plus a thin 32-bit `sound.dll` shim, so no 32-bit x86
audio decoding, mixing, or multimedia timer code runs under Wine WoW64 on Linux,
macOS, or the Windows WoW64 port. Output must equal the original engine's
whenever the original is not overwhelmed: the same sounds started and stopped at
the same moments, matching `sound.dll` and DirectSound's exact volume, pan,
pitch, fade, looping, mixing, playlist, and `.amb` animation-sequencer
semantics, with MP3 decoded to matching PCM within decoder rounding.

Status: reverse engineering of `sound.dll` and `Civ3Conquests-Unmodded.exe`
complete; implementation in progress.

## 1. The stack today [X sound.dll, PCGames]

The game loads `.\sound.dll` (454,656 bytes; identical in base Civ3 and
Conquests). That DLL imports `mss32.dll` (Miles Sound System), `DSOUND`,
`WINMM`, `ole32`, `USER32`, and `KERNEL32`.

Miles does very little: `sound.dll` imports only 9 functions from `mss32.dll`
(`AIL_startup`, `AIL_shutdown`, `AIL_set_redist_directory`, `AIL_file_size`,
`AIL_file_read`, `AIL_mem_alloc_lock`, `AIL_mem_free_lock`,
`RIB_find_file_provider`, `RIB_request_interface`), solely to locate and drive
Miles' ASI MP3 decoder provider for streaming MP3 music. Everything else lives
inside `sound.dll`:

- **Two mixing paths** both feeding a DirectSound primary buffer configured to
  **44100 Hz, 16-bit signed stereo** (`0x1001204d`..`0x10012074`):
  1. **Software mixer (`[0x100B3A6C]`)** — mixes normal WAV SFX and `.amb`
     unit-animation WAVs in 64-bit IEEE `f64` into a single `0x10000`-byte
     22050 Hz 16-bit stereo DirectSound streaming buffer (`[0x100B3A70]`), with
     up to 16 simultaneous polyphonic voices per `WaveSound`, exact linear
     interpolation for pitch shifting, and RIFF `smpl` loop points.
  2. **DirectSound streaming buffer path** (`inner+0x58 & 0x10000000`, selected
     for MP3 files and large streaming WAVs with flag `0x100`) — decodes MP3 via
     Miles ASI into 44100 Hz 16-bit signed stereo PCM in `0x10000`-byte chunks
     into a `0x40000`-byte circular DirectSound secondary buffer (`0x100336F0`),
     with DirectSound hardware/software attenuation and seamless gapless
     playlist chaining (`0x10039840`).
- **Multimedia timer (`init_sound_timer`, `timeSetEvent` / `timeBeginPeriod`)**
  — drives software-mixer refills, MP3 streaming refills, fades, and `.amb`
  sequencer ticks.
- **`.amb` unit animation sequencer (Sound Type 5)** — Civ3's custom multi-WAV
  MIDI sequencer used for all 19 unit FLC animation slots (`0x4024F0`). It is
  **not** General MIDI output: `Civ3Conquests.exe` opens `Midi_Device` with
  `flags = 0` (never calling `midiOutOpen`), using `Midi_Device` solely to tick
  `.amb` sequences and trigger `WaveSound` WAV voices.
- **`Wave_In_Device`** — `waveIn*` recording for multiplayer voice chat. Only
  opened if `sound_init` receives `flags & 8`; both call sites in
  `Civ3Conquests.exe` (`0x54FDC9`, `0x56C412`) pass `flags = 2`.

### Asset corpus (Civilization III Complete + expansions + scenarios)

- **22,995 `.wav` files**: 22,753 PCM mono 22050 Hz 16-bit; 55 PCM mono 22050
  Hz 8-bit; 46 PCM stereo 22050 Hz 16-bit; 44 PCM mono 44100 Hz 16-bit; 43 PCM
  stereo 44100 Hz 16-bit; 12 MS-ADPCM stereo 22050 Hz 4-bit (CCM2); 10 PCM mono
  48000 Hz 16-bit; 17 at 11025 Hz; 5 at 8000/16000/20050/24000 Hz; 1 MP3-in-WAV
  (format `0x55`, CCM2 Elvis).
- **36+ `.mp3` music tracks** under `Sounds/` (diplomacy, menu, era playlists).
- **2,646 `.amb` unit-animation sound scripts** (608 in base/PTW/Conquests,
  2,038 in scenarios).

## 2. Exports and loading protocol [X PCGames, sound.dll]

`0x5F9C60` calls `LoadLibraryA(".\sound.dll")`, stores the handle at
`[0xCAAD3C]`, and resolves ordinals **1 through 11** via `GetProcAddress` into
the table `[0xCAACEC + 4*(ordinal - 1)]`. Any missing ordinal unloads
`sound.dll` (`0x5F9D00`) and the game runs silent. `0x5F9D50` calls ordinal 11
(`get_sound_version`) and verifies `(version & 0xFF00) == 0x0D00` (original
returns `0x05000D01`).

| Ord | Calling conv | Exact PE export name | RVA | Game call site | Semantics |
|---:|---|---|---|---|---|
| 1 | `__cdecl` | `create_sound` | 0x137A | `0x5F9D7F`, `0x5F9DB8` | `(Sound** out, const char* path, u32 type) -> SNDERR`; `type == 0` infers from extension (`.wav`->1, `.mid`->2, `.mp3`->3/1, default 1); types 1..8 supported |
| 2 | `__cdecl` | `delete_sound` | 0x1AA0 | `0x5F9D9F` | `(Sound* obj) -> SNDERR`; calls deleting dtor `vt[0](1)` |
| 3 | `__cdecl` | `init_sound_timer` | 0x1C8A | `0x5F9D3F` | `(u32 flags, u32 unused) -> SNDERR`; initializes global timer/audio runtime |
| 4 | `__cdecl` | `release_sound` | 0x2699 | `0x5F9BC4` | `() -> SNDERR`; shuts down timer and releases global sound state |
| 5 | `__cdecl` | `?create_device@Dll_Wave_Device@@KA?AW4SNDERR@@PAPAVWave_Device@@K@Z` | 0x2009 | `0x5F8E4A` | `(Wave_Device** out, u32 flags) -> SNDERR`; singleton (`0x0C` if already exists) |
| 6 | `__cdecl` | `?delete_device@Dll_Wave_Device@@KA?AW4SNDERR@@XZ` | 0x11B8 | `0x5F8E8A` | `() -> SNDERR`; destroys singleton (`0x14` if none) |
| 7 | `__cdecl` | `?create_device@Dll_Midi_Device@@KA?AW4SNDERR@@PAPAVMidi_Device@@K@Z` | 0x2207 | `0x5F966A` | `(Midi_Device** out, u32 flags) -> SNDERR`; singleton (`0x0C` if exists) |
| 8 | `__cdecl` | `?delete_device@Dll_Midi_Device@@KA?AW4SNDERR@@XZ` | 0x1A78 | `0x5F96AA` | `() -> SNDERR`; destroys singleton |
| 9 | `__cdecl` | `?create_device@Dll_Wave_In_Device@@KA?AW4SNDERR@@PAPAVWave_In_Device@@K@Z` | 0x2121 | `0x5F982A` | `(Wave_In_Device** out, u32 flags) -> SNDERR`; singleton |
| 10 | `__cdecl` | `?delete_device@Dll_Wave_In_Device@@KA?AW4SNDERR@@XZ` | 0x2239 | `0x5F986A` | `() -> SNDERR`; destroys singleton |
| 11 | `__cdecl` | `get_sound_version` | 0x27A7 | `0x5F9D5C` | `() -> u32`; returns `0x05000D01` |
| 12 | `__stdcall` | `?call_back@WaveInDeviceMgr@@SGXPAUHWAVEIN__@@GKKK@Z` | 0x17B2 | unused by exe | `ret 0x14`; no-op stub in replacement |

Common `SNDERR` codes observed in `sound.dll` and `Civ3Conquests.exe`:
`0` = OK, `1` = not initialized / device missing, `3` = device open/hardware
error, `6` = busy/generic error, `8` = file open error, `0x0A` = invalid
parameter, `0x0B` = unsupported, `0x0C` = already exists, `0x0E` = already
playing, `0x0F` = already loaded/queued, `0x12` = invalid format, `0x13` = no
device/object, `0x14` = missing function / unloaded, `0x24` = empty slot,
`0x28` = busy, `0x29` = end of track.

## 3. Object vtables and direct exe field accesses [X sound.dll, PCGames]

All virtual methods use MSVC `extern "thiscall"` (`this` in `ECX`, callee pops
stack arguments).

### 3.1 Devices

- **`Dll_Wave_Device` (vtable `0x1005A304`, 43 slots, `0x00..0xA8`)**:
  - `slot 0 (+0x00, ret 4)`: deleting dtor `(flags)`.
  - `slot 4 (+0x10, ret 8)`: `open(flags, hwnd)` — note stack order: `[esp+4]=flags`, `[esp+8]=hwnd`! Opens audio output, initializes software mixer & decoder runtime.
  - `slot 5 (+0x14, ret 0)`: `close()` — stops active streams, closes device.
  - `slot 6 (+0x18, ret 0)`: `get_device_count() -> u32` (returns `1` when open, `0` when closed).
  - `slot 7 (+0x1C, ret 4)`: `select_device(idx) -> SNDERR` (`0` for `idx == 0`, `0x0A` out of range).
  - `slot 8 (+0x20, ret 12)`: `get_device_name(idx, buf, len) -> SNDERR`.
  - `slot 14 (+0x38, ret 0)`: `tick()` — timer tick (in replacement, audio thread ticks itself; slot is a harmless no-op).
  - `slot 19 (+0x4C, ret 0)`: `pause()` — pauses audio output.
  - `slot 20 (+0x50, ret 0)`: `resume()` — resumes audio output.
  - `slot 21 (+0x54, ret 4)` / `slot 22 (+0x58, ret 0)`: start/stop WAV mix recording (`mmioWrite`).
  - `slot 23 (+0x5C, ret 0)`: `get_current_device() -> u32` (`0`).
  - `slot 25 (+0x64, ret 0)` / `slot 26 (+0x68, ret 0)` / `slot 27 (+0x6C, ret 0)`: suspend / restore / is_suspended.
  - `slot 28 (+0x70, ret 4)`: **`set_master_volume(vol: i32) -> SNDERR`** (clamped `0..=127`; controls master gain across SFX and DS music!).
  - `slot 29 (+0x74, ret 0)`: `get_ds_ptr() -> u32` (returns `0` in native mode).
  - `slot 30 (+0x78, ret 0)` / `slot 34 (+0x88, ret 0)`: 3D/EAX capability queries (`0`).
  - Remaining slots (`1..3`, `9..13`, `15..18`, `24`, `31..33`, `35..42`): exact stack pop counts preserved (`ret 0/4/12`), returning `0` or `0x0B`.
- **`Dll_Midi_Device` (vtable `0x1005A28C`, 25 slots, `0x00..0x60`)**:
  - `slot 0 (+0x00, ret 4)`: deleting dtor.
  - `slot 4 (+0x10, ret 8)`: `open(hwnd, flags)` — game passes `flags = 0` (no hardware MIDI synth; only activates `.amb` sequencer clock). Always succeeds (`0`).
  - `slot 5 (+0x14, ret 0)`: `close()` — stops `.amb` sequences cleanly; safe under `audio_init.rs` failed-Wave unwind.
  - `slot 6..8`: return `0x2F` (matching original).
  - `slot 14 (+0x38, ret 0)`: `tick()` (handled natively by audio engine clock).
  - `slot 19/20 (+0x4C/+0x50)`: pause/resume.
  - `slot 22/23/24 (+0x58/+0x5C/+0x60)`: resume/suspend/is_suspended.
- **`Dll_Wave_In_Device` (vtable `0x1005A3D4`, 27 slots, `0x00..0x68`)**:
  - Cleanly answers all 27 slots with exact stack cleanups; `open`/`close` return `0` (silent capture stub, no WinMM `waveIn*` dependency).

### 3.2 Sound objects and exe-visible fields

The game's wrappers (`0x5F9E00..0x5FBC00`) and music manager (`0x535D00..0x536100`)
read and write specific fields directly on the objects returned by
`create_sound`. Our 32-bit `#[repr(C)]` structs preserve every offset exactly:

1. **`WaveSound` — Type 1 (vtable `0x1005B800`, 61 slots, `0x00..0xF0`)**:
   - Exe-visible fields: `+0x04` volume (`0..=127`, init `127`), `+0x08` pan (`-64..=63`, init `0`), `+0x30` loop, `+0x38` fade_ms (`1000`), `+0x3C` channel (`16`), `+0x40` child ptr (`0`), `+0x44` flags, `+0x54` type (`1`).
   - Key slots:
     - `0 (+0x00, ret 4)`: deleting dtor.
     - `1 (+0x04, ret 4)`: `set_fade_ms(ms)`.
     - `4 (+0x10, ret 4)`: `load(const char* path) -> SNDERR`.
     - `5 (+0x14, ret 0)`: `unload() -> SNDERR`.
     - `7 (+0x1C, ret 0)`: `play() -> SNDERR`.
     - `8 (+0x20, ret 0)`: `stop() -> SNDERR`.
     - `9 (+0x24, ret 4)`: `fade_out_ms(ms)` -> `set_fade_ms(ms); fade_out()`.
     - `10 (+0x28, ret 0)`: `fade_out() -> SNDERR`.
     - `12 (+0x30, ret 0)`: `fade_in() -> SNDERR`.
     - `13 (+0x34, ret 12)`: `fade(target_vol, start_vol, duration_ms)` — `target`/`start` in `0..=127` or `-1` meaning "current volume" (`0x1003C2F0`).
     - `14 (+0x38, ret 0)`: `pause()`.
     - `16 (+0x40, ret 4)`: `set_volume(vol)`.
     - `17 (+0x44, ret 4)`: `set_pan(pan)`.
     - `18 (+0x48, ret 4)`: `set_loop(enabled)`.
     - `21 (+0x54, ret 4)`: `set_fade_in_ms(ms)`.
     - `22 (+0x58, ret 0)`: `is_looping()`.
     - `23 (+0x5C, ret 0)`: `is_playing() -> u32` (`1` or `0`).
     - `24 (+0x60, ret 0)`: `is_loaded() -> u32` (`-1` or `0` in original; non-zero truthy).
     - `27 (+0x6C, ret 4)`: `set_flags(flags)` (`0x01` preload RAM, `0x02` loop, `0x10` DS static, `0x20` DS 3D, `0x40` DS secondary, `0x100` streaming DS buffer, `0x200` queued chain).
     - `28 (+0x70, ret 0)`: `get_flags() -> u32`.
     - `30 (+0x78, ret 0)`: `get_type() -> 1`.
     - `33 (+0x84, ret 0)`: `load_deferred() -> SNDERR` (loads path stored via slot 34).
     - `34 (+0x88, ret 8)`: `configure(path, flags)`.
     - `35 (+0x8C, ret 0)`: `commit_load()`.
     - `36 (+0x90, ret 8)`: `configure_and_load(path, flags)`.
     - `37 (+0x94, ret 4)`: `play_instance(u32* out_voice)`.
     - `38 (+0x98, ret 4)`: `start_voice(u32 voice)`.
     - `39 (+0x9C, ret 4)`: `set_pitch(cents: i32)`.
     - `46 (+0xB8, ret 4)`: `set_position_ms(ms)`.
     - `47 (+0xBC, ret 0)`: `get_position_ms() -> u32`.
     - `50 (+0xC8, ret 0)`: `get_duration_ms() -> u32`.
     - `51 (+0xCC, ret 0)`: `is_ds_buffer() -> i32`.

2. **`AmbSound` — Type 5 (vtable `0x1005A000`, 78 slots, `0x00..0x134`)**:
   - **Critical exe-visible fields**:
     - `+0x40`: **MUST be `0` (`NULL`)**, because `0x5FB570` recurses into `[obj + 0x40]`.
     - `+0x54`: type (`5`, returned by `vt[+0x78]`).
     - `+0x58`: `u32` flags written directly by the exe:
       - bit 0 (`0x01`): resolve keymap `.wav` files relative to the `.amb` file's directory (`0x5FB570`).
       - bit 1 (`0x02`): **FLC animation-sync mode & ring-out on stop** (`0x5FBA1F`).
     - `+0x5C`: `u32 ms_per_frame` written directly by `0x5FB1C0` (computed at `0x402587` as `flc->anim_time_ms / flc->num_frames`).
   - Key slots:
     - `0 (+0x00, ret 4)`: deleting dtor.
     - `4 (+0x10, ret 4)`: `load(const char* path) -> SNDERR` (reads `self.flags_58` bit 0 for relative `.wav` resolution).
     - `5 (+0x14, ret 0)`: `unload()`.
     - `7 (+0x1C, ret 0)`: `play()` (syncs `self.flags_58` and `self.ms_per_frame` to native).
     - `8 (+0x20, ret 0)` / `10 (+0x28, ret 0)`: `stop()` / `fade_out()`. When `flags_58 & 2 != 0`, stops MIDI scheduling while letting already-triggered WAV voices finish ringing out (`0x100211CC`).
     - `16 (+0x40, ret 4)`: `set_volume(vol)`.
     - `17 (+0x44, ret 4)`: `set_pan(pan)`.
     - `18 (+0x48, ret 4)`: `set_loop(enabled)`.
     - `23 (+0x5C, ret 0)`: `is_playing()`.
     - `24 (+0x60, ret 0)`: `is_loaded()`.
     - `30 (+0x78, ret 0)`: `get_type() -> 5`.
     - `76 (+0x130, ret 4)`: `set_anim_frame(frame: u32)` — called by `Civ3Conquests.exe` at `0x402B88`, `0x402D13`, `0x4033B9`. When `flags_58 & 2 != 0`, sets `target_ms = frame * ms_per_frame`; if `target_ms < elapsed_ms` and looping is enabled, restarts MIDI tracks from tick 0 (`0x1001FCC0`).
     - `77 (+0x134, ret 0)`: `get_anim_frame() -> u32`.

3. **`MusicPlaylist` — Type 8 (vtable `0x1005B3A4`, 36 slots, `0x00..0x8C`, size `0x10C`)**:
   - Owned directly by `Civ3Conquests.exe` at `[0xA52120]` with **no wrapper**.
   - **Critical exe-visible fields**:
     - `+0x04`: `u32 volume` (`0..=127`, init `127`) — read/written directly by exe.
     - `+0x3C`: `u32 channel` (`0..15`, init `16`) — read/written directly by exe.
     - `+0x40`: `u32` (`0`).
     - `+0x44`: `u32 flags`.
     - `+0x54`: `u32` type (`8`).
     - `+0x58..+0xF8`: **`[u32; 40]` track `char*` pointers** — `0x5FAFF0` allocates these with the exe's CRT heap and writes them here; `0x5FB050` frees them with the exe's `operator delete` (`0x6499C4`) and zeroes them BEFORE calling `delete_sound`. **Our Type 8 destructor must never free pointers in `+0x58..+0xF8`.**
     - `+0xF8`: current track marker; `+0xFC`: queued next track marker; `+0x104`: flag returned by `vt[+0x88]`; `+0x108`: scaled volume set by `vt[+0x8C]`.
   - Key slots:
     - `0 (+0x00, ret 4)`: deleting dtor.
     - `7 (+0x1C, ret 0)`: `play()` — starts current track (or promotes queued track if current is empty), looping, at `scaled_volume`.
     - `8 (+0x20, ret 0)`: `stop()`.
     - `10 (+0x28, ret 0)`: `fade_out()` — fades current track to 0 over **2000 ms** (`0x7D0`) and stops/clears it.
     - `13 (+0x34, ret 12)`: `fade(dir, unused, ms)` — if `dir == 0`, fades current -> 0 over `ms`; else fades 0 -> current over `ms`.
     - `16 (+0x40, ret 4)`: `set_volume(vol)` — masks `vol & 0x7F`, stores at `+0x04`, updates active track.
     - `23 (+0x5C, ret 0)`: `is_playing()`.
     - `30 (+0x78, ret 0)`: `get_type() -> 8`.
     - `33 (+0x84, ret 4)`: `queue_track(idx: u32) -> SNDERR` — reads C-string at `tracks[idx]` (`idx < 40`, else `0x0A`; NULL -> `0x24`; already queued -> `0x0F`). Loads as streaming MP3/WAV; if a current track is already playing, chains it so the queued track starts gaplessly at current EOF and promotes to current.
     - `34 (+0x88, ret 0)`: `has_flag_104() -> u8`.
     - `35 (+0x8C, ret 4)`: `set_scaled_volume(vol)` — stores at `+0x108` and applies to current track.

4. **Types 2 (`MidiSound`), 4, 6 & 7 (`VoiceSound`, vtable `0x1005B450`, 38 slots)**:
   - Stub objects with exact vtable lengths, stack cleanups, and `+0x54` type tags so any defensive call returns cleanly without crashing.

## 4. Exact DSP, mixing, and `.amb` rules [X sound.dll]

### 4.1 Software mixer path (`WaveSound` SFX & `.amb` WAVs)

- **Accumulator**: 64-bit IEEE `f64` stereo buffer (`0x10068A08`), rounded to `i32`
  (`fistp`, round-to-nearest-even) and clamped to `[-32768, 32767]`
  (`0x10007D19..0x10007D4B`).
- **8-bit unsigned PCM**: converted via `(b as i32 << 8) - 32768` (`i16`).
- **Software volume (`0x1003C280` -> `0x10036910`)**:
  - Clamp `v` to `0..=127`.
  - `V = if v == 0 { 0 } else { (v + 1) << 9 }` (`0..=65536`; `v = 127 -> 65536`).
  - Linear gain `g = V as f64 / 65536.0` (scaled by master volume `M / 65536.0`
    from `WaveDevice::set_master_volume`!).
- **Software pan (`0x10036A90`)**:
  - Clamp `pan` to `-64..=63`.
  - If `pan == 0`: `V_L = V`, `V_R = V`.
  - Else: `div = if pan < 0 { 64 } else { 63 }`, `delta = (V * pan) / div`
    (truncating integer division), `V_R = (V + delta).min(65536)`,
    `V_L = (V - delta).min(65536)`; `gain_L = V_L / 65536.0`,
    `gain_R = V_R / 65536.0`.
- **Pitch shifting (`0x1003C3C0`)**:
  - Input `cents` wrapped via `if cents < -1200 || cents > 1200 { cents %= 1200 }`.
  - `semi = cents / 100` (truncated toward 0), `rem = (cents % 100).abs()`,
    `idx = (semi + 12) as usize` (`0..=24`), neighbor `next = if cents < 0 { idx - 1 } else { idx + 1 }`.
  - Linearly interpolates `sound.dll`'s 25-entry `f64` semitone table at
    `0x1005B6E8` (`0.5` to `2.0`):
    `ratio = table[idx] + (table[next] - table[idx]) * (rem as f64) * 0.01`.
- **Resampling & linear interpolation (`0x10019B10`, `0x10019EC0`, `0x1001A260`)**:
  - Fractional phase accumulator `pos += (src_rate / 44100.0) * pitch_ratio`
    with linear interpolation `s = s0 + frac * (s1 - s0)`.
  - For 22050 Hz WAVs at unity pitch (`step = 0.5`), this matches `sound.dll`'s
    22050 Hz software mixer linearly upsampled 2x by DirectSound into the
    44100 Hz primary buffer identically, while preserving full 44.1 kHz
    bandwidth for 44100 Hz WAVs and accurate playback speed for all other WAV
    sample rates in the corpus (8000..48000 Hz).
- **WAV `smpl` loop points (`0x10033CDB..0x10033D46`)**:
  - If a WAV contains a RIFF `smpl` chunk with `cSampleLoops > 0`, loop start
    and end (inclusive `+ 1`) are taken from loop entry 0; otherwise the full
    sample range loops when looping is enabled.
- **Polyphony**: up to 16 simultaneous voices per `WaveSound` (`0x10032C44`).

### 4.2 DirectSound streaming buffer path (MP3 music & playlists)

- **Streaming format**: 44100 Hz, 16-bit signed stereo (`0x1005B5E8`).
- **DirectSound attenuation curve (`DSBuf_set_vol`, `0x1000F430`)**:
  - Clamp `vol` and `master_vol` to `0..=127`.
  - `combined = ((master as f64 / 127.0) * (vol as f64 / 127.0) * 127.0).trunc() as i32`.
  - If `combined <= 0`: `mB = -10000` (gain `0.0`).
  - Else: **`mB = (1000.0 * (combined as f64 / 127.0).ln()).trunc() as i32`**
    (natural logarithm `fldln2; fyl2x`!).
  - DirectSound amplitude gain: `10f64.powf(mB as f64 / 2000.0)`
    (approximately `(combined / 127.0)^1.1513`, with `-48.44 dB` at `combined = 1`).
- **Fades (`0x100363D0`, `0x100376E0`)**:
  - Linear ramp in `[0.0, 1.0]` over `duration_ms`; for streaming buffers,
    each tick maps the ramped level `f` via `DSBuf_set_vol((f * 127.0).trunc())`.
  - When a fade-out reaches target `0`, playback stops automatically.
- **Seamless playlist chaining (`0x10039840`, `0x10037C58`)**:
  - When a queued track is chained onto a playing track, reaching EOF on the
    current track immediately begins rendering from the queued track on the very
    next sample frame with zero gap and transfers volume/fade state cleanly.

### 4.3 `.amb` container and MIDI sequencer (`0x10004200`, `0x1001FCC0`, `0x100219C0`)

- **Container**: sequence of Little-Endian FourCC chunks (`tag: [u8; 4]`,
  `size: u32`, payload) immediately followed by a Standard MIDI File
  (`MThd` + `MTrk` chunks):
  - `prgm`: 28-byte header (`id`, `flags`, `pitch_max`, `pitch_min`,
    `vol_max`, `vol_min`, ...) + NUL-terminated program name + optional
    NUL-terminated keymap name.
    - `flags & 0x01`: random pitch in `[pitch_min, pitch_max]` cents.
    - `flags & 0x02`: random volume delta `(amb_vol * rand[vol_min, vol_max]) / 127`.
    - `flags & 0x08`: gated (Note-Off stops/fades the voice over `kmap.release_ms`;
      when clear, one-shot WAV plays to completion).
    - `flags & 0x10`: fixed pitch (when clear, transposes by
      `(midi_note - wav.root_note) * 100` cents).
  - `kmap`: `u32 flags`, `u32 release_ms` (default 250 ms), name, and note-range
    entries `(min_note, max_note, polyphony, wav_filename)`. `flags & 1` selects
    a random entry per Note-On; `flags & 8` ignores MIDI velocity.
  - `glbl`, `chnl`, `trak`, `ctrl`: parsed and stored (`glbl` global params;
    `chnl`/`trak`/`ctrl` have 0 occurrences across all 2,646 Civ3/CCM2 `.amb`
    files).
  - **CCM2 padding quirk**: 18 scenario `.amb` files (e.g. `GalleyAttack.amb`)
    have a 1-byte `\x00` padding byte before a chunk tag (`kmap` size off by 1);
    the parser skips stray `\x00` bytes between chunks cleanly.
- **MIDI dispatch (`0x10020C70`, `0x100219C0`, `0x10021E90`)**:
  - `0x9n` (`vel > 0`): Note-On; computes
    `v0 = ((vel + track_vol_off) * ch_vol) / 127`,
    `v1 = (v0 * amb_vol) / 127` (+ optional random vol delta), pan `ch_pan - 63`,
    pitch shift, and triggers the mapped `WaveSound` voice (enabling looping if
    the WAV has a `smpl` loop).
  - `0x8n` or `0x9n` (`vel == 0`): Note-Off (only stops/fades voices whose
    program has `flags & 0x08`).
  - `0xCn`: Program Change (`channel.program = prog`).
  - `0xBn`: Control Change (CC7 volume, CC10 pan, CC0/32 bank).
  - `0xFF 0x51`: Set Tempo (`us_per_quarter`; `ms_per_tick = us_per_quarter / (1000.0 * ppq)`).
  - `0xFF 0x2F`: End of Track.
- **Animation sync (`0x1001FCC0`)**:
  - When `amb+0x58 & 2 != 0`: sequence time tracks
    `target_ms = cur_frame * ms_per_frame` (driven by `vt[+0x130](cur_frame)`).
    When `target_ms < elapsed_ms` and looping is set, all tracks reset to tick 0.
  - When `amb+0x58 & 2 == 0`: sequence time advances with the 44100 Hz sample
    clock (zero WinMM timer jitter).

## 5. Implementation architecture

### 5.1 Crate layout

- **`crates/civ3-audio`** (`#![forbid(unsafe_code)]`, 100% lock-free):
  - Shared binary wire protocol (`wire`) compiled on all targets (32-bit x86 and
    64-bit native).
  - On non-x86 native targets:
    - Vetted crates (`docs/audio-crates-vetting.md`):
      - `symphonia = "=0.6.1"` (`default-features = false`, features `["wav", "pcm", "mp3", "adpcm"]`)
      - `cpal = "=0.18.2"` (`default-features = false` — ALSA on Linux, CoreAudio on macOS, WASAPI on Windows)
      - `rubato = "=5.1.0"` (`default-features = false`) — used strictly when the host `cpal` device's sample rate differs from 44100 Hz (bypassed at 44100 Hz and in offline rendering).
      - `smallvec` pinned to `1.15.2` in `Cargo.lock`.
    - `dsp`: exact `sound.dll` volume, pan, DS dB curve, pitch table, rounding.
    - `decoder`: `symphonia`-based preloaded WAV decoder (with RIFF `smpl` loop
      extraction and format `0x55` MP3-in-WAV fallback into `symphonia`'s MP3
      decoder) and incremental streaming MP3/WAV decoder.
    - `amb`: `.amb` container + SMF parser and sample-accurate sequencer.
    - `mixer`: deterministic 44100 Hz stereo `f64` `AudioEngine` with
      `render_frames` used identically by live playback and offline tests.
    - `output`: lock-free native audio thread (`handoff::Mailbox` command queue,
      wait-free atomic `is_playing` state table, `#![forbid(unsafe_code)]`
      `AtomicU32` SPSC ring buffer feeding the real-time `cpal` callback,
      optional `rubato` host-rate conversion, graceful headless/silent fallback
      if no host audio device is present, and optional `CIV3R_AUDIO_DUMP` WAV
      capture).
- **`crates/civ3r-native`**:
  - Adds `CALL_AUDIO = 9` and `LIMIT_AUDIO = 16` (`AudioCall` struct) to
    `unix_call_table!`.
  - Spawns the native audio thread via `wine_unixlib::unix::spawn_native`
    (blocking Wine signals) and resolves Windows paths to host paths
    (case-insensitive on Unix, direct on Windows).
- **`crates/civ3-sound-dll`** (`i686-pc-windows-gnu` `cdylib` -> `sound.dll` +
  `sound.def` exporting ordinals 1..12):
  - **Pass-through mode**: mirrors `civ3-bridge/src/pass_through.rs`. When
    `CIV3R_SOUND=original`, or when neither `CIV3R_VIEWER` is set nor
    `CIV3R_TRACE=on` (and `CIV3R_SOUND != native`), dynamically loads
    `sound_orig.dll` and forwards ordinals 1..11 transparently.
  - **Native mode**: loads `civ3r_native.dll` via `wine_unixlib::pe::UnixLib`
    and dispatches through `CALL_AUDIO`. Implements all device and sound
    vtables/structs with exact layouts and `catch_unwind` crash protection; any
    failure returns a valid `SNDERR` so the game runs silent and never crashes.

### 5.2 Relationship to `crates/civ3-sys/src/audio_init.rs`

`audio_init.rs` repairs a bug in `Civ3Conquests.exe` at `0x5F9B7D`: when
`Wave_Device::open` fails, the game unloads `sound.dll` without first closing
and deleting `Midi_Device`, leaving a stale vtable pointer in `0xCAAAD8+0x14`.
We **deliberately retain `audio_init.rs`**:
- In pass-through mode (`sound_orig.dll`), the original DirectSound open can
  still fail and trigger `0x5F9B7D`.
- In native mode, if `civ3r_native.dll` is unavailable and `Wave_Device::open`
  returns an error, `audio_init.rs` cleanly unwinds our `Midi_Device` before
  `FreeLibrary(sound.dll)`.
Our `Midi_Device::close` (`vt[+0x14]`) and `delete_device` (ordinal 8) are
explicitly tested against that exact unwind sequence.

### 5.3 Installation and recovery

- `tools/install-sound` (and `tools/install-native`) backs up the original
  `sound.dll` to `sound_orig.dll` before installing our `sound.dll` (never
  overwriting an existing `sound_orig.dll`), with `--uninstall` to restore the
  original byte-for-byte.
- `crates/civ3r-launcher` stages `sound.dll` / `sound_orig.dll` alongside
  `IFC23.dll` / `ifc23_orig.dll` in private game installs when present.

## 6. Equality and live verification plan

1. **Deterministic numerical equality harness**: a 32-bit Windows test driver
   runs identical `sound.dll` call sequences against:
   - original `sound.dll` + `Mss32.dll` (capturing DirectSound buffers via an
     in-process COM `IDirectSound` / `IDirectSoundBuffer` capture harness under
     Wine so both the 22050 Hz SW mixer buffer and 44100 Hz MP3 streaming
     buffers + `SetVolume`/`SetPan`/`SetFrequency` calls are captured
     deterministically without audio hardware), and
   - our replacement `civ3-audio` / `sound.dll` + `civ3r-native` engine.
   Test cases: mono 16-bit & 8-bit WAV SFX (unity, volume 64, pan -32/+32,
   pitch ±500 cents, looping, multi-voice mix), stereo WAV SFX, real Civ3 MP3
   music (unity, volume 64, 2000 ms fade-out, playlist transition), and real
   Civ3 `.amb` unit-animation sequences. Measured SNR / RMS / max-abs error will
   be recorded here.
2. **Live headless Wayland Civ3 verification**: run `tools/civ3-native` +
   `tools/civ3-session` (with full sha256 backup/restore of touched game files),
   verifying clean startup, menu/diplomacy music, SFX, unit animations,
   Preferences volume sliders, and clean shutdown.
