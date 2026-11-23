//! 32-bit Windows `sound.dll` replacement for Civilization III: Conquests.
//!
//! Exports the exact 12 ordinals/names expected by `Civ3Conquests.exe`.
//! - In pass-through mode (`pass_through::Mode::Original`), dynamically loads
//!   `sound_orig.dll` and forwards ordinals 1..11 untouched.
//! - In native mode (`pass_through::Mode::Native`), dispatches all audio
//!   operations through `civ3r_native.dll` (`CALL_AUDIO`) to the 64-bit native
//!   Rust `civ3-audio` engine. No 32-bit x86 audio decoding, mixing, or
//!   multimedia timer runs under Wine.

pub mod client;
pub mod layout;
pub mod pass_through;
#[cfg(all(windows, target_arch = "x86"))]
pub mod vtables;

/// Sound DLL version checked by `Civ3Conquests.exe` at `0x5F9D5C`:
/// `(version & 0xFF00) == 0x0D00`.
pub const SOUND_VERSION: u32 = 0x0500_0D01;

/// Allocate from our own private heap on Windows so `sound.dll` never contends
/// with the game's process heap.
#[cfg(all(windows, target_arch = "x86"))]
#[global_allocator]
static ALLOCATOR: inproc::alloc::PrivateHeap = inproc::alloc::PrivateHeap::new();

/// Infers `SoundKind` when `create_sound` receives `sound_type == 0`, matching
/// `sound.dll` at `0x1000949E..0x1000950D`.
pub fn infer_sound_kind(path: Option<&str>, requested: u32) -> Option<civ3_audio::wire::SoundKind> {
    use civ3_audio::wire::SoundKind;
    let raw = if requested != 0 {
        requested
    } else if let Some(p) = path {
        let lower = p.to_ascii_lowercase();
        if lower.ends_with(".wav") {
            1
        } else if lower.ends_with(".mid") || lower.ends_with(".rmi") {
            2
        } else if lower.ends_with(".mp3") {
            3
        } else if lower.ends_with(".amb") {
            5
        } else {
            1
        }
    } else {
        1
    };
    SoundKind::from_u32(raw)
}

#[cfg(all(windows, target_arch = "x86"))]
mod exports {
    use std::ffi::c_void;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use civ3_audio::wire::{Op, RequestHeader, SoundKind};

    use crate::client::{self, read_cstr};
    use crate::infer_sound_kind;
    use crate::layout::*;
    use crate::pass_through::{self, Mode};
    use crate::vtables::*;
    use crate::SOUND_VERSION;

    const SNDERR_OK: u32 = 0;
    const SNDERR_INVALID_PARAM: u32 = 0x0A;
    const SNDERR_ALREADY_EXISTS: u32 = 0x0C;
    const SNDERR_UNLOADED: u32 = 0x14;

    static WAVE_DEVICE: AtomicUsize = AtomicUsize::new(0);
    static MIDI_DEVICE: AtomicUsize = AtomicUsize::new(0);
    static WAVE_IN_DEVICE: AtomicUsize = AtomicUsize::new(0);

    #[inline]
    fn guard<F: FnOnce() -> u32>(fallback: u32, f: F) -> u32 {
        catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn civ3_create_sound(
        out: *mut *mut c_void,
        path_ptr: *const u8,
        sound_type: u32,
    ) -> u32 {
        guard(SNDERR_INVALID_PARAM, || {
            if pass_through::mode_here() == Mode::Original {
                return unsafe { pass_through::orig::create_sound(out, path_ptr, sound_type) };
            }
            if out.is_null() {
                return SNDERR_INVALID_PARAM;
            }
            unsafe { *out = std::ptr::null_mut() };
            let path = unsafe { read_cstr(path_ptr) };
            let Some(kind) = infer_sound_kind(path.as_deref(), sound_type) else {
                return SNDERR_INVALID_PARAM;
            };
            let handle = match client::call(
                RequestHeader::new(Op::CreateSound, 0).with_a(kind as i32),
                "",
            ) {
                Ok(r) if r.snderr == SNDERR_OK => r.value,
                Ok(r) => return r.snderr,
                Err(e) => return e,
            };

            let ptr: *mut c_void = match kind {
                SoundKind::Wave | SoundKind::Type3 => {
                    let mut obj = Box::new(WaveSoundObj {
                        vtable: (&WAVE_SOUND_VT as *const _ as usize) as u32,
                        volume: 127,
                        pan: 0,
                        pad_0c: [0; 9],
                        looping: 0,
                        pad_34: 0,
                        fade_ms: 1000,
                        channel: 16,
                        child_ptr: 0,
                        flags: 0,
                        pad_48: [0; 3],
                        sound_type: kind as u32,
                        handle,
                        fade_in_ms: 1000,
                        pitch_cents: 0,
                        is_loaded: 0,
                        is_ds_buffer: 0,
                        duration_ms: 0,
                        deferred_path_len: 0,
                        deferred_path: [0; 512],
                    });
                    if let Some(p) = &path {
                        let b = p.as_bytes();
                        let n = b.len().min(obj.deferred_path.len());
                        obj.deferred_path[..n].copy_from_slice(&b[..n]);
                        obj.deferred_path_len = n as u32;
                    }
                    Box::into_raw(obj) as *mut c_void
                }
                SoundKind::Amb => {
                    let obj = Box::new(AmbSoundObj {
                        vtable: (&AMB_SOUND_VT as *const _ as usize) as u32,
                        volume: 127,
                        pan: 0,
                        pad_0c: [0; 9],
                        looping: 0,
                        pad_34: 0,
                        fade_ms: 1000,
                        channel: 16,
                        child_ptr: 0,
                        flags: 0,
                        pad_48: [0; 3],
                        sound_type: 5,
                        flags_58: 0,
                        ms_per_frame: 0,
                        handle,
                        cur_frame: 0,
                        is_loaded: 0,
                    });
                    Box::into_raw(obj) as *mut c_void
                }
                SoundKind::Playlist => {
                    let obj = Box::new(PlaylistObj {
                        vtable: (&PLAYLIST_VT as *const _ as usize) as u32,
                        volume: 127,
                        pan: 0,
                        pad_0c: [0; 9],
                        looping: 0,
                        pad_34: 0,
                        fade_ms: 2000,
                        channel: 16,
                        child_ptr: 0,
                        flags: 0,
                        pad_48: [0; 3],
                        sound_type: 8,
                        tracks: [0; 40],
                        current_marker: 0,
                        queued_marker: 0,
                        pad_100: 0,
                        flag_104: 0,
                        scaled_volume: 127,
                        handle,
                    });
                    Box::into_raw(obj) as *mut c_void
                }
                other => {
                    let obj = Box::new(VoiceSoundObj {
                        vtable: (&VOICE_SOUND_VT as *const _ as usize) as u32,
                        volume: 127,
                        pan: 0,
                        pad_0c: [0; 9],
                        looping: 0,
                        pad_34: 0,
                        fade_ms: 1000,
                        channel: 16,
                        child_ptr: 0,
                        flags: 0,
                        pad_48: [0; 3],
                        sound_type: other as u32,
                        handle,
                    });
                    Box::into_raw(obj) as *mut c_void
                }
            };
            unsafe { *out = ptr };
            SNDERR_OK
        })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn civ3_delete_sound(obj: *mut c_void) -> u32 {
        guard(SNDERR_OK, || {
            if pass_through::mode_here() == Mode::Original {
                return unsafe { pass_through::orig::delete_sound(obj) };
            }
            if obj.is_null() {
                return SNDERR_OK;
            }
            unsafe {
                let vtable = *(obj as *const *const usize);
                if !vtable.is_null() {
                    let dtor: unsafe extern "thiscall" fn(*mut c_void, u32) -> *mut c_void =
                        std::mem::transmute(*vtable);
                    dtor(obj, 1);
                }
            }
            SNDERR_OK
        })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn civ3_init_sound_timer(flags: u32, unused: u32) -> u32 {
        guard(SNDERR_OK, || {
            if pass_through::mode_here() == Mode::Original {
                return unsafe { pass_through::orig::init_sound_timer(flags, unused) };
            }
            let _ = client::call(RequestHeader::new(Op::InitTimer, 0).with_a(flags as i32), "");
            SNDERR_OK
        })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn civ3_release_sound() -> u32 {
        guard(SNDERR_OK, || {
            if pass_through::mode_here() == Mode::Original {
                return unsafe { pass_through::orig::release_sound() };
            }
            let _ = client::call(RequestHeader::new(Op::ReleaseAll, 0), "");
            SNDERR_OK
        })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn civ3_wave_create_device(
        out: *mut *mut c_void,
        flags: u32,
    ) -> u32 {
        guard(SNDERR_INVALID_PARAM, || {
            if pass_through::mode_here() == Mode::Original {
                return unsafe { pass_through::orig::wave_create(out, flags) };
            }
            if out.is_null() {
                return SNDERR_INVALID_PARAM;
            }
            if WAVE_DEVICE.load(Ordering::Acquire) != 0 {
                return SNDERR_ALREADY_EXISTS;
            }
            let obj = Box::into_raw(Box::new(WaveDeviceObj {
                vtable: (&WAVE_DEVICE_VT as *const _ as usize) as u32,
                flags,
                hwnd: 0,
                is_open: 0,
                suspended: 0,
                master_volume: 127,
            }));
            if WAVE_DEVICE
                .compare_exchange(0, obj as usize, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                unsafe { drop(Box::from_raw(obj)) };
                return SNDERR_ALREADY_EXISTS;
            }
            unsafe { *out = obj as *mut c_void };
            SNDERR_OK
        })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn civ3_wave_delete_device() -> u32 {
        guard(SNDERR_OK, || {
            if pass_through::mode_here() == Mode::Original {
                return unsafe { pass_through::orig::wave_delete() };
            }
            let ptr = WAVE_DEVICE.swap(0, Ordering::AcqRel) as *mut WaveDeviceObj;
            if ptr.is_null() {
                return SNDERR_UNLOADED;
            }
            unsafe {
                let vtable = *(ptr as *const *const usize);
                if !vtable.is_null() {
                    let dtor: unsafe extern "thiscall" fn(*mut c_void, u32) -> *mut c_void =
                        std::mem::transmute(*vtable);
                    dtor(ptr as *mut c_void, 1);
                }
            }
            SNDERR_OK
        })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn civ3_midi_create_device(
        out: *mut *mut c_void,
        flags: u32,
    ) -> u32 {
        guard(SNDERR_INVALID_PARAM, || {
            if pass_through::mode_here() == Mode::Original {
                return unsafe { pass_through::orig::midi_create(out, flags) };
            }
            if out.is_null() {
                return SNDERR_INVALID_PARAM;
            }
            if MIDI_DEVICE.load(Ordering::Acquire) != 0 {
                return SNDERR_ALREADY_EXISTS;
            }
            let obj = Box::into_raw(Box::new(MidiDeviceObj {
                vtable: (&MIDI_DEVICE_VT as *const _ as usize) as u32,
                flags,
                hwnd: 0,
                is_open: 0,
                suspended: 0,
            }));
            if MIDI_DEVICE
                .compare_exchange(0, obj as usize, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                unsafe { drop(Box::from_raw(obj)) };
                return SNDERR_ALREADY_EXISTS;
            }
            unsafe { *out = obj as *mut c_void };
            SNDERR_OK
        })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn civ3_midi_delete_device() -> u32 {
        guard(SNDERR_OK, || {
            if pass_through::mode_here() == Mode::Original {
                return unsafe { pass_through::orig::midi_delete() };
            }
            let ptr = MIDI_DEVICE.swap(0, Ordering::AcqRel) as *mut MidiDeviceObj;
            if ptr.is_null() {
                return SNDERR_UNLOADED;
            }
            unsafe {
                let vtable = *(ptr as *const *const usize);
                if !vtable.is_null() {
                    let dtor: unsafe extern "thiscall" fn(*mut c_void, u32) -> *mut c_void =
                        std::mem::transmute(*vtable);
                    dtor(ptr as *mut c_void, 1);
                }
            }
            SNDERR_OK
        })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn civ3_wave_in_create_device(
        out: *mut *mut c_void,
        flags: u32,
    ) -> u32 {
        guard(SNDERR_INVALID_PARAM, || {
            if pass_through::mode_here() == Mode::Original {
                return unsafe { pass_through::orig::wave_in_create(out, flags) };
            }
            if out.is_null() {
                return SNDERR_INVALID_PARAM;
            }
            if WAVE_IN_DEVICE.load(Ordering::Acquire) != 0 {
                return SNDERR_ALREADY_EXISTS;
            }
            let obj = Box::into_raw(Box::new(WaveInDeviceObj {
                vtable: (&WAVE_IN_DEVICE_VT as *const _ as usize) as u32,
                flags,
                is_open: 0,
            }));
            if WAVE_IN_DEVICE
                .compare_exchange(0, obj as usize, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                unsafe { drop(Box::from_raw(obj)) };
                return SNDERR_ALREADY_EXISTS;
            }
            unsafe { *out = obj as *mut c_void };
            SNDERR_OK
        })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn civ3_wave_in_delete_device() -> u32 {
        guard(SNDERR_OK, || {
            if pass_through::mode_here() == Mode::Original {
                return unsafe { pass_through::orig::wave_in_delete() };
            }
            let ptr = WAVE_IN_DEVICE.swap(0, Ordering::AcqRel) as *mut WaveInDeviceObj;
            if ptr.is_null() {
                return SNDERR_UNLOADED;
            }
            unsafe {
                let vtable = *(ptr as *const *const usize);
                if !vtable.is_null() {
                    let dtor: unsafe extern "thiscall" fn(*mut c_void, u32) -> *mut c_void =
                        std::mem::transmute(*vtable);
                    dtor(ptr as *mut c_void, 1);
                }
            }
            SNDERR_OK
        })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn civ3_get_sound_version() -> u32 {
        guard(0, || {
            if pass_through::mode_here() == Mode::Original {
                return unsafe { pass_through::orig::get_version() };
            }
            SOUND_VERSION
        })
    }

    /// Ordinal 12 (`?call_back@WaveInDeviceMgr@@SGXPAUHWAVEIN__@@GKKK@Z`, `__stdcall`, `ret 0x14`).
    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn civ3_wave_in_callback(
        _hwi: *mut c_void,
        _msg: u32,
        _instance: u32,
        _param1: u32,
        _param2: u32,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use civ3_audio::wire::SoundKind;

    #[test]
    fn sound_version_matches_civ3_exe_mask() {
        assert_eq!(SOUND_VERSION & 0xFF00, 0x0D00);
        assert_eq!(SOUND_VERSION, 0x0500_0D01);
    }

    #[test]
    fn sound_kind_inference_matches_sound_dll() {
        assert_eq!(infer_sound_kind(Some("foo.WAV"), 0), Some(SoundKind::Wave));
        assert_eq!(infer_sound_kind(Some("bar.mid"), 0), Some(SoundKind::Midi));
        assert_eq!(infer_sound_kind(Some("track.MP3"), 0), Some(SoundKind::Type3));
        assert_eq!(infer_sound_kind(Some("unit.amb"), 0), Some(SoundKind::Amb));
        assert_eq!(infer_sound_kind(None, 0), Some(SoundKind::Wave));
        assert_eq!(infer_sound_kind(None, 8), Some(SoundKind::Playlist));
        assert_eq!(infer_sound_kind(None, 99), None);
    }

    #[cfg(not(target_arch = "x86"))]
    #[test]
    fn failed_wave_unwind_sequence_closes_midi_cleanly() {
        // Mirrors civ3-sys::audio_init's unwind sequence when Wave_Device::open fails:
        // Midi_Device is created and opened, Wave_Device open fails, then Midi_Device::close
        // and Midi_Device::delete_device run before sound.dll is unloaded.
        let mut engine = civ3_audio::mixer::AudioEngine::new();
        assert_eq!(
            engine.handle_request(civ3_audio::wire::RequestHeader::new(civ3_audio::wire::Op::MidiOpen, 0), "").snderr,
            0
        );
        assert_eq!(
            engine.handle_request(civ3_audio::wire::RequestHeader::new(civ3_audio::wire::Op::MidiClose, 0), "").snderr,
            0
        );
        // Repeated close or ReleaseAll after failed Wave init must also stay clean.
        assert_eq!(
            engine.handle_request(civ3_audio::wire::RequestHeader::new(civ3_audio::wire::Op::MidiClose, 0), "").snderr,
            0
        );
        assert_eq!(
            engine.handle_request(civ3_audio::wire::RequestHeader::new(civ3_audio::wire::Op::ReleaseAll, 0), "").snderr,
            0
        );
    }
}
