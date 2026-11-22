# Replacing the game's audio engine

Goal (user request, 2026-11): the game's audio reimplemented in Rust. The
current stack is slow and produces glitches on macOS. Output must equal the
old engine's whenever the old engine is not overwhelmed.

Status: interface decoding in progress. Nothing is implemented yet.

## The stack today

The game uses `sound.dll` (454,656 bytes; identical in the base game and
Conquests). That DLL imports `mss32.dll` (Miles Sound System), `DSOUND`,
`WINMM`, `ole32`, `USER32` and `KERNEL32`.

Assets (base game `Sounds/`): 98 `.wav` effects and 36 `.mp3` tracks
(diplomacy and menu music, decoded by Miles). Units' own sounds are `.wav`
files next to their animations under `Art/Units/`.

## How the game loads it [X PCGames]

`0x5F9C60`: `LoadLibraryA(".\\sound.dll")` (string at `0x72FE28`), module
handle in `[0xCAAD3C]`. Then `GetProcAddress(module, ordinal)` for ordinals
1 to 11, into the table at `0xCAACEC` (`[0xCAACEC + 4*(ordinal-1)]`). A
missing ordinal unloads it again (`0x5F9D00`) and the game runs silent.
`0x5F9D50` calls ordinal 11 and checks `(version & 0xFF00) == 0x0D00`;
otherwise it reports a version mismatch (strings `0x670C08`, `0x670B88`).

| Ordinal | Export | RVA | Game call site |
|---|---|---|---|
| 1 | `create_sound` | 0x137A | 0x5F9D7F, 0x5F9DB8 |
| 2 | `delete_sound` | 0x1AA0 | 0x5F9D9F |
| 3 | `init_sound_timer` | 0x1C8A | 0x5F9D3F |
| 4 | `release_sound` | 0x2699 | 0x5F9BC4 |
| 5 | `Dll_Wave_Device::create_device(Wave_Device**, unsigned long)` | 0x2009 | 0x5F8E4A |
| 6 | `Dll_Wave_Device::delete_device()` | 0x11B8 | 0x5F8E8A |
| 7 | `Dll_Midi_Device::create_device(Midi_Device**, unsigned long)` | 0x2207 | 0x5F966A |
| 8 | `Dll_Midi_Device::delete_device()` | 0x1A78 | 0x5F96AA |
| 9 | `Dll_Wave_In_Device::create_device(Wave_In_Device**, unsigned long)` | 0x2121 | 0x5F982A |
| 10 | `Dll_Wave_In_Device::delete_device()` | 0x2239 | 0x5F986A |
| 11 | `get_sound_version` | 0x27A7 | 0x5F9D5C |
| 12 | `WaveInDeviceMgr::call_back` (`__stdcall`, waveIn callback) | 0x17B2 | not taken by the game |

All exports are `jmp` thunks in `sound.dll`. The `create_device` exports
return a `SNDERR`: 0 on success, 0x0C when a device exists already, 0x14
when the export is missing (the game's own fallback values, `0x5F8E62`,
`0x5F8E6A`).

## The wave device [X sound.dll]

`Dll_Wave_Device::create_device` (`0x1000B200`) creates one shared device:
`operator new(0x41E0)`, constructor `0x1000B380` (through thunk
`0x10001FB9`), pointer kept in `[0x100B3A98]` and written to the out
parameter. Its vtable is at `0x1005A304`: 43 entries, `0x1005A304` to
`0x1005A3AC`, each a `jmp` thunk.

The game wraps it in its own object (functions around `0x5F8800` to
`0x5F9660`), the device pointer at wrapper `+0x14`. Virtual calls in that
code use these vtable offsets: `0x04, 0x08, 0x0C, 0x10, 0x14, 0x18, 0x1C,
0x20, 0x40, 0x64, 0x68, 0x70, 0x78, 0x80, 0x88, 0x8C, 0x9C, 0xA0`. Some of
them may belong to the per-sound objects the device hands out rather than
to the device itself; that is the next thing to separate.

## Plan

1. Decode every method the game calls on each device and on the objects
   they return: arguments, return values, and the semantics of volume, pan,
   loop, stop, streaming, and its timer (`init_sound_timer`). Do this from
   `sound.dll` and from the game's wrappers.
2. A 32-bit `sound.dll` replacement with the same ordinals and vtable
   layouts. It forwards to native code (the renderer's unixlib side, as the
   renderer does). It must not rely on Wine's DirectSound or WinMM.
   The devices are MSVC objects called through vtables with `thiscall`.
   `cxx` cannot define classes with virtual methods, so they are a
   `#[repr(C)]` struct whose first field points to a static table of
   `extern "thiscall"` functions, as in the jgl replacement.
3. Native audio from existing crates, not hand-written: decoding (WAV,
   MP3), resampling where Miles' own rule allows it, and output through the
   platform's audio API (CoreAudio, WASAPI, PipeWire/ALSA). Every crate and
   its dependency tree is vetted for malicious authors before it is added;
   the report goes in `docs/audio-crates-vetting.md`. Only what must match
   the old engine is written here: the device interface, and Miles' volume,
   pan and mixing rules where the output has to be equal.
4. Equality test: render the same call sequence through the old stack
   (capturing what it hands to DirectSound) and through ours, then compare
   the sample buffers within the old engine's own precision.
