//! Static MSVC 32-bit `extern "thiscall"` vtables matching `sound.dll`'s exact
//! slot counts, stack-pop byte counts, and direct exe-visible field semantics.

#![cfg(all(windows, target_arch = "x86"))]

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};

use civ3_audio::wire::{
    Op, RequestHeader, STATE_DS_BUFFER, STATE_LOADED, STATE_LOOPING, STATE_PLAYING,
    STATE_PLAYLIST_HAS_QUEUED,
};

use crate::client::{self, cp1252_to_utf8, read_cstr};
use crate::layout::*;

const SNDERR_OK: u32 = 0;
const SNDERR_DEVICE_ERROR: u32 = 3;
const SNDERR_FILE_OPEN: u32 = 8;
const SNDERR_INVALID_PARAM: u32 = 0x0A;
const SNDERR_UNSUPPORTED: u32 = 0x0B;
const SNDERR_UNLOADED: u32 = 0x14;
const SNDERR_EMPTY_SLOT: u32 = 0x24;
const SNDERR_MIDI_NONE: u32 = 0x2F;

#[inline]
fn safe<F: FnOnce() -> u32>(fallback: u32, f: F) -> u32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

#[inline]
fn fire(hdr: RequestHeader, path: &str) -> u32 {
    match client::call(hdr, path) {
        Ok(r) => r.snderr,
        Err(e) => e,
    }
}

// ============================================================================
// Generic exact-stack-pop no-op stubs (MSVC thiscall callee cleanup).
// ============================================================================

unsafe extern "thiscall" fn stub0(_this: *mut c_void) -> u32 {
    0
}
unsafe extern "thiscall" fn stub1(_this: *mut c_void, _a: u32) -> u32 {
    0
}
unsafe extern "thiscall" fn stub2(_this: *mut c_void, _a: u32, _b: u32) -> u32 {
    0
}
unsafe extern "thiscall" fn stub3(_this: *mut c_void, _a: u32, _b: u32, _c: u32) -> u32 {
    0
}
unsafe extern "thiscall" fn unsup1(_this: *mut c_void, _a: u32) -> u32 {
    SNDERR_UNSUPPORTED
}
unsafe extern "thiscall" fn unsup3(_this: *mut c_void, _a: u32, _b: u32, _c: u32) -> u32 {
    SNDERR_UNSUPPORTED
}
unsafe extern "thiscall" fn midi_2f_0(_this: *mut c_void) -> u32 {
    SNDERR_MIDI_NONE
}
unsafe extern "thiscall" fn midi_2f_1(_this: *mut c_void, _a: u32) -> u32 {
    SNDERR_MIDI_NONE
}
unsafe extern "thiscall" fn midi_2f_3(_this: *mut c_void, _a: u32, _b: u32, _c: u32) -> u32 {
    SNDERR_MIDI_NONE
}

// ============================================================================
// Dll_Wave_Device (43 slots, 0x00..0xA8)
// ============================================================================

unsafe extern "thiscall" fn wave_dev_dtor(this: *mut c_void, flags: u32) -> *mut c_void {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !this.is_null() {
            let dev = unsafe { &mut *(this as *mut WaveDeviceObj) };
            if dev.is_open != 0 {
                dev.is_open = 0;
                let _ = client::call(RequestHeader::new(Op::WaveClose, 0), "");
            }
            if (flags & 1) != 0 {
                unsafe { drop(Box::from_raw(this as *mut WaveDeviceObj)) };
            }
        }
    }));
    this
}

/// Slot 4 (+0x10): `open(flags, hwnd)` — stack order verified at 0x10011DE0.
unsafe extern "thiscall" fn wave_dev_open(this: *mut c_void, flags: u32, hwnd: u32) -> u32 {
    safe(SNDERR_DEVICE_ERROR, || {
        if this.is_null() {
            return SNDERR_INVALID_PARAM;
        }
        if !client::is_available() {
            return SNDERR_DEVICE_ERROR;
        }
        let dev = unsafe { &mut *(this as *mut WaveDeviceObj) };
        dev.flags = flags;
        dev.hwnd = hwnd;
        let err = fire(
            RequestHeader::new(Op::WaveOpen, 0).with_a(flags as i32).with_b(hwnd as i32),
            "",
        );
        if err == SNDERR_OK {
            dev.is_open = 1;
            dev.suspended = 0;
        }
        err
    })
}

unsafe extern "thiscall" fn wave_dev_close(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        if !this.is_null() {
            let dev = unsafe { &mut *(this as *mut WaveDeviceObj) };
            dev.is_open = 0;
        }
        fire(RequestHeader::new(Op::WaveClose, 0), "")
    })
}

unsafe extern "thiscall" fn wave_dev_count(this: *mut c_void) -> u32 {
    if this.is_null() {
        return 0;
    }
    let dev = unsafe { &*(this as *const WaveDeviceObj) };
    if dev.is_open != 0 { 1 } else { 0 }
}

unsafe extern "thiscall" fn wave_dev_select(_this: *mut c_void, idx: u32) -> u32 {
    if idx == 0 { SNDERR_OK } else { SNDERR_INVALID_PARAM }
}

unsafe extern "thiscall" fn wave_dev_name(
    _this: *mut c_void,
    idx: u32,
    buf: *mut u8,
    len: u32,
) -> u32 {
    safe(SNDERR_INVALID_PARAM, || {
        if idx != 0 || buf.is_null() || len == 0 {
            return SNDERR_INVALID_PARAM;
        }
        let name = b"Civ3 Native Audio";
        let max_copy = (len as usize).saturating_sub(1).min(name.len());
        unsafe {
            std::ptr::copy_nonoverlapping(name.as_ptr(), buf, max_copy);
            *buf.add(max_copy) = 0;
        }
        SNDERR_OK
    })
}

unsafe extern "thiscall" fn wave_dev_pause(_this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || fire(RequestHeader::new(Op::WavePause, 0), ""))
}

unsafe extern "thiscall" fn wave_dev_resume(_this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || fire(RequestHeader::new(Op::WaveResume, 0), ""))
}

unsafe extern "thiscall" fn wave_dev_start_rec(_this: *mut c_void, path_ptr: *const u8) -> u32 {
    safe(SNDERR_INVALID_PARAM, || {
        let Some(path) = (unsafe { read_cstr(path_ptr) }) else {
            return SNDERR_INVALID_PARAM;
        };
        fire(RequestHeader::new(Op::WaveStartRecord, 0), &path)
    })
}

unsafe extern "thiscall" fn wave_dev_stop_rec(_this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || fire(RequestHeader::new(Op::WaveStopRecord, 0), ""))
}

unsafe extern "thiscall" fn wave_dev_suspend(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        if !this.is_null() {
            unsafe { (*(this as *mut WaveDeviceObj)).suspended = 1 };
        }
        fire(RequestHeader::new(Op::WavePause, 0), "")
    })
}

unsafe extern "thiscall" fn wave_dev_restore(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        if !this.is_null() {
            unsafe { (*(this as *mut WaveDeviceObj)).suspended = 0 };
        }
        fire(RequestHeader::new(Op::WaveResume, 0), "")
    })
}

unsafe extern "thiscall" fn wave_dev_is_suspended(this: *mut c_void) -> u32 {
    if this.is_null() {
        return 0;
    }
    unsafe { (*(this as *const WaveDeviceObj)).suspended }
}

unsafe extern "thiscall" fn wave_dev_set_master_vol(this: *mut c_void, vol: i32) -> u32 {
    safe(SNDERR_OK, || {
        let v = vol.clamp(0, 127);
        if !this.is_null() {
            unsafe { (*(this as *mut WaveDeviceObj)).master_volume = v };
        }
        fire(RequestHeader::new(Op::WaveSetMasterVolume, 0).with_a(v), "")
    })
}

#[repr(C)]
pub struct VTable<const N: usize>(pub [*const c_void; N]);
unsafe impl<const N: usize> Sync for VTable<N> {}

pub static WAVE_DEVICE_VT: VTable<WAVE_DEVICE_SLOTS> = VTable([
    wave_dev_dtor as *const c_void,         // 0  (+0x00) ret 4
    stub1 as *const c_void,                 // 1  (+0x04) ret 4
    stub0 as *const c_void,                 // 2  (+0x08) ret 0
    stub0 as *const c_void,                 // 3  (+0x0C) ret 0
    wave_dev_open as *const c_void,         // 4  (+0x10) ret 8
    wave_dev_close as *const c_void,        // 5  (+0x14) ret 0
    wave_dev_count as *const c_void,        // 6  (+0x18) ret 0
    wave_dev_select as *const c_void,       // 7  (+0x1C) ret 4
    wave_dev_name as *const c_void,         // 8  (+0x20) ret 12
    stub1 as *const c_void,                 // 9  (+0x24) ret 4
    stub0 as *const c_void,                 // 10 (+0x28) ret 0
    stub1 as *const c_void,                 // 11 (+0x2C) ret 4
    stub1 as *const c_void,                 // 12 (+0x30) ret 4
    stub0 as *const c_void,                 // 13 (+0x34) ret 0
    stub0 as *const c_void,                 // 14 (+0x38) ret 0 (tick)
    stub1 as *const c_void,                 // 15 (+0x3C) ret 4
    stub0 as *const c_void,                 // 16 (+0x40) ret 0
    stub1 as *const c_void,                 // 17 (+0x44) ret 4
    stub0 as *const c_void,                 // 18 (+0x48) ret 0
    wave_dev_pause as *const c_void,        // 19 (+0x4C) ret 0
    wave_dev_resume as *const c_void,       // 20 (+0x50) ret 0
    wave_dev_start_rec as *const c_void,    // 21 (+0x54) ret 4
    wave_dev_stop_rec as *const c_void,     // 22 (+0x58) ret 0
    stub0 as *const c_void,                 // 23 (+0x5C) ret 0 (get_current_device -> 0)
    stub0 as *const c_void,                 // 24 (+0x60) ret 0
    wave_dev_suspend as *const c_void,      // 25 (+0x64) ret 0
    wave_dev_restore as *const c_void,      // 26 (+0x68) ret 0
    wave_dev_is_suspended as *const c_void, // 27 (+0x6C) ret 0
    wave_dev_set_master_vol as *const c_void, // 28 (+0x70) ret 4
    stub0 as *const c_void,                 // 29 (+0x74) ret 0 (get_ds_ptr -> 0)
    stub0 as *const c_void,                 // 30 (+0x78) ret 0 (3D support -> 0)
    unsup1 as *const c_void,                // 31 (+0x7C) ret 4
    unsup1 as *const c_void,                // 32 (+0x80) ret 4
    unsup1 as *const c_void,                // 33 (+0x84) ret 4
    stub0 as *const c_void,                 // 34 (+0x88) ret 0 (EAX -> 0)
    unsup3 as *const c_void,                // 35 (+0x8C) ret 12
    unsup3 as *const c_void,                // 36 (+0x90) ret 12
    unsup1 as *const c_void,                // 37 (+0x94) ret 4
    unsup1 as *const c_void,                // 38 (+0x98) ret 4
    unsup1 as *const c_void,                // 39 (+0x9C) ret 4
    unsup1 as *const c_void,                // 40 (+0xA0) ret 4
    unsup1 as *const c_void,                // 41 (+0xA4) ret 4
    unsup1 as *const c_void,                // 42 (+0xA8) ret 4
]);

// ============================================================================
// Dll_Midi_Device (25 slots, 0x00..0x60)
// ============================================================================

unsafe extern "thiscall" fn midi_dev_dtor(this: *mut c_void, flags: u32) -> *mut c_void {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !this.is_null() {
            let dev = unsafe { &mut *(this as *mut MidiDeviceObj) };
            if dev.is_open != 0 {
                dev.is_open = 0;
                let _ = client::call(RequestHeader::new(Op::MidiClose, 0), "");
            }
            if (flags & 1) != 0 {
                unsafe { drop(Box::from_raw(this as *mut MidiDeviceObj)) };
            }
        }
    }));
    this
}

unsafe extern "thiscall" fn midi_dev_open(this: *mut c_void, hwnd: u32, flags: u32) -> u32 {
    safe(SNDERR_OK, || {
        if !this.is_null() {
            let dev = unsafe { &mut *(this as *mut MidiDeviceObj) };
            dev.hwnd = hwnd;
            dev.flags = flags;
            dev.is_open = 1;
            dev.suspended = 0;
        }
        // MidiDevice::open always succeeds (game passes flags=0 for .amb sequencer).
        let _ = client::call(
            RequestHeader::new(Op::MidiOpen, 0).with_a(flags as i32).with_b(hwnd as i32),
            "",
        );
        SNDERR_OK
    })
}

unsafe extern "thiscall" fn midi_dev_close(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        if !this.is_null() {
            let dev = unsafe { &mut *(this as *mut MidiDeviceObj) };
            dev.is_open = 0;
        }
        let _ = client::call(RequestHeader::new(Op::MidiClose, 0), "");
        SNDERR_OK
    })
}

unsafe extern "thiscall" fn midi_dev_pause(_this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        let _ = client::call(RequestHeader::new(Op::MidiPause, 0), "");
        SNDERR_OK
    })
}

unsafe extern "thiscall" fn midi_dev_resume(_this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        let _ = client::call(RequestHeader::new(Op::MidiResume, 0), "");
        SNDERR_OK
    })
}

unsafe extern "thiscall" fn midi_dev_restore(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        if !this.is_null() {
            unsafe { (*(this as *mut MidiDeviceObj)).suspended = 0 };
        }
        let _ = client::call(RequestHeader::new(Op::MidiResume, 0), "");
        SNDERR_OK
    })
}

unsafe extern "thiscall" fn midi_dev_suspend(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        if !this.is_null() {
            unsafe { (*(this as *mut MidiDeviceObj)).suspended = 1 };
        }
        let _ = client::call(RequestHeader::new(Op::MidiPause, 0), "");
        SNDERR_OK
    })
}

unsafe extern "thiscall" fn midi_dev_is_suspended(this: *mut c_void) -> u32 {
    if this.is_null() {
        return 0;
    }
    unsafe { (*(this as *const MidiDeviceObj)).suspended }
}

pub static MIDI_DEVICE_VT: VTable<MIDI_DEVICE_SLOTS> = VTable([
    midi_dev_dtor as *const c_void,         // 0  (+0x00) ret 4
    stub1 as *const c_void,                 // 1  (+0x04) ret 4
    stub0 as *const c_void,                 // 2  (+0x08) ret 0
    stub0 as *const c_void,                 // 3  (+0x0C) ret 0
    midi_dev_open as *const c_void,         // 4  (+0x10) ret 8
    midi_dev_close as *const c_void,        // 5  (+0x14) ret 0
    midi_2f_0 as *const c_void,             // 6  (+0x18) ret 0
    midi_2f_1 as *const c_void,             // 7  (+0x1C) ret 4
    midi_2f_3 as *const c_void,             // 8  (+0x20) ret 12
    stub1 as *const c_void,                 // 9  (+0x24) ret 4
    stub0 as *const c_void,                 // 10 (+0x28) ret 0
    stub1 as *const c_void,                 // 11 (+0x2C) ret 4
    stub1 as *const c_void,                 // 12 (+0x30) ret 4
    stub0 as *const c_void,                 // 13 (+0x34) ret 0
    stub0 as *const c_void,                 // 14 (+0x38) ret 0 (tick)
    stub1 as *const c_void,                 // 15 (+0x3C) ret 4
    stub0 as *const c_void,                 // 16 (+0x40) ret 0
    stub1 as *const c_void,                 // 17 (+0x44) ret 4
    stub0 as *const c_void,                 // 18 (+0x48) ret 0
    midi_dev_pause as *const c_void,        // 19 (+0x4C) ret 0
    midi_dev_resume as *const c_void,       // 20 (+0x50) ret 0
    stub0 as *const c_void,                 // 21 (+0x54) ret 0
    midi_dev_restore as *const c_void,      // 22 (+0x58) ret 0
    midi_dev_suspend as *const c_void,      // 23 (+0x5C) ret 0
    midi_dev_is_suspended as *const c_void, // 24 (+0x60) ret 0
]);

// ============================================================================
// Dll_Wave_In_Device (27 slots, 0x00..0x68)
// ============================================================================

unsafe extern "thiscall" fn wave_in_dtor(this: *mut c_void, flags: u32) -> *mut c_void {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !this.is_null() && (flags & 1) != 0 {
            unsafe { drop(Box::from_raw(this as *mut WaveInDeviceObj)) };
        }
    }));
    this
}

unsafe extern "thiscall" fn wave_in_open(this: *mut c_void, flags: u32, _hwnd: u32) -> u32 {
    if !this.is_null() {
        let dev = unsafe { &mut *(this as *mut WaveInDeviceObj) };
        dev.flags = flags;
        dev.is_open = 1;
    }
    SNDERR_OK
}

unsafe extern "thiscall" fn wave_in_close(this: *mut c_void) -> u32 {
    if !this.is_null() {
        unsafe { (*(this as *mut WaveInDeviceObj)).is_open = 0 };
    }
    SNDERR_OK
}

pub static WAVE_IN_DEVICE_VT: VTable<WAVE_IN_DEVICE_SLOTS> = VTable([
    wave_in_dtor as *const c_void,  // 0  ret 4
    stub1 as *const c_void,         // 1  ret 4
    stub0 as *const c_void,         // 2  ret 0
    stub0 as *const c_void,         // 3  ret 0
    wave_in_open as *const c_void,  // 4  ret 8
    wave_in_close as *const c_void, // 5  ret 0
    stub0 as *const c_void,         // 6  ret 0
    stub1 as *const c_void,         // 7  ret 4
    stub3 as *const c_void,         // 8  ret 12
    stub1 as *const c_void,         // 9  ret 4
    stub0 as *const c_void,         // 10 ret 0
    stub1 as *const c_void,         // 11 ret 4
    stub1 as *const c_void,         // 12 ret 4
    stub0 as *const c_void,         // 13 ret 0
    stub0 as *const c_void,         // 14 ret 0
    stub1 as *const c_void,         // 15 ret 4
    stub0 as *const c_void,         // 16 ret 0
    stub1 as *const c_void,         // 17 ret 4
    stub0 as *const c_void,         // 18 ret 0
    stub0 as *const c_void,         // 19 ret 0
    stub0 as *const c_void,         // 20 ret 0
    stub1 as *const c_void,         // 21 ret 4
    stub0 as *const c_void,         // 22 ret 0
    stub0 as *const c_void,         // 23 ret 0
    stub0 as *const c_void,         // 24 ret 0
    stub1 as *const c_void,         // 25 ret 4
    stub1 as *const c_void,         // 26 ret 4
]);

// ============================================================================
// WaveSound — Type 1 / Type 3 (61 slots, 0x00..0xF0)
// ============================================================================

fn wave_set_deferred_path(obj: &mut WaveSoundObj, path: &str) {
    let bytes = path.as_bytes();
    let n = bytes.len().min(obj.deferred_path.len());
    obj.deferred_path[..n].copy_from_slice(&bytes[..n]);
    obj.deferred_path_len = n as u32;
}

fn wave_deferred_path(obj: &WaveSoundObj) -> String {
    let n = (obj.deferred_path_len as usize).min(obj.deferred_path.len());
    cp1252_to_utf8(&obj.deferred_path[..n])
}

fn wave_load_path(obj: &mut WaveSoundObj, path: &str) -> u32 {
    if path.is_empty() {
        return SNDERR_INVALID_PARAM;
    }
    wave_set_deferred_path(obj, path);
    match client::call(RequestHeader::new(Op::Load, obj.handle).with_a(obj.flags as i32), path) {
        Ok(r) => {
            if r.snderr == SNDERR_OK {
                obj.is_loaded = 0xFFFF_FFFF;
                obj.is_ds_buffer = r.value as i32;
                obj.duration_ms = r.extra1;
            }
            r.snderr
        }
        Err(e) => e,
    }
}

unsafe extern "thiscall" fn ws_dtor(this: *mut c_void, flags: u32) -> *mut c_void {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !this.is_null() {
            let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
            if obj.handle != 0 {
                let _ = client::call(RequestHeader::new(Op::DeleteSound, obj.handle), "");
                obj.handle = 0;
            }
            if (flags & 1) != 0 {
                unsafe { drop(Box::from_raw(this as *mut WaveSoundObj)) };
            }
        }
    }));
    this
}

unsafe extern "thiscall" fn ws_set_fade_ms(this: *mut c_void, ms: u32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        obj.fade_ms = ms;
        fire(RequestHeader::new(Op::SetFadeMs, obj.handle).with_a(ms as i32), "")
    })
}

unsafe extern "thiscall" fn ws_load(this: *mut c_void, path_ptr: *const u8) -> u32 {
    safe(SNDERR_INVALID_PARAM, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        let path = match unsafe { read_cstr(path_ptr) } {
            Some(p) if !p.is_empty() => p,
            _ => wave_deferred_path(obj),
        };
        wave_load_path(obj, &path)
    })
}

unsafe extern "thiscall" fn ws_unload(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        obj.is_loaded = 0;
        obj.is_ds_buffer = 0;
        obj.duration_ms = 0;
        fire(RequestHeader::new(Op::Unload, obj.handle), "")
    })
}

/// Matches `sound.dll` at `0x1003BF23..0x1003BF55` (`WaveSound::play`),
/// `0x1003C036..0x1003C066` (`start_voice`), and `0x1003C102..0x1003C134`
/// (`play_instance`): when a `WaveSound` was configured with flag `0x04`
/// (lazy load-on-play, used by Civ3's title screen music `0xa51fa0` and
/// credits music `0xa51f30` with `flags = 0x506`), the game skips calling
/// `WaveSound::load` up front and expects the first `play()` to load the
/// path saved during `create_sound`.
fn ensure_lazy_loaded(obj: &mut WaveSoundObj) -> u32 {
    if obj.is_loaded != 0 {
        return SNDERR_OK;
    }
    if (obj.flags & 0x04) == 0 {
        return SNDERR_UNLOADED;
    }
    let path = wave_deferred_path(obj);
    if path.is_empty() {
        return SNDERR_FILE_OPEN;
    }
    let err = wave_load_path(obj, &path);
    if err == SNDERR_OK { SNDERR_OK } else { SNDERR_FILE_OPEN }
}

unsafe extern "thiscall" fn ws_play(this: *mut c_void) -> u32 {
    safe(SNDERR_UNLOADED, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        let err = ensure_lazy_loaded(obj);
        if err != SNDERR_OK {
            return err;
        }
        fire(RequestHeader::new(Op::Play, obj.handle), "")
    })
}

unsafe extern "thiscall" fn ws_stop(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        fire(RequestHeader::new(Op::Stop, obj.handle), "")
    })
}

unsafe extern "thiscall" fn ws_fade_out_ms(this: *mut c_void, ms: u32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        obj.fade_ms = ms;
        let _ = fire(RequestHeader::new(Op::SetFadeMs, obj.handle).with_a(ms as i32), "");
        fire(RequestHeader::new(Op::FadeOut, obj.handle), "")
    })
}

unsafe extern "thiscall" fn ws_fade_out(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        fire(RequestHeader::new(Op::FadeOut, obj.handle), "")
    })
}

unsafe extern "thiscall" fn ws_fade_in(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        fire(RequestHeader::new(Op::FadeIn, obj.handle), "")
    })
}

unsafe extern "thiscall" fn ws_fade(
    this: *mut c_void,
    target_vol: i32,
    start_vol: i32,
    duration_ms: i32,
) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        fire(
            RequestHeader::new(Op::Fade, obj.handle)
                .with_a(target_vol)
                .with_b(start_vol)
                .with_c(duration_ms),
            "",
        )
    })
}

unsafe extern "thiscall" fn ws_pause(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        fire(RequestHeader::new(Op::Pause, obj.handle), "")
    })
}

unsafe extern "thiscall" fn ws_set_volume(this: *mut c_void, vol: i32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        obj.volume = vol.clamp(0, 127);
        fire(RequestHeader::new(Op::SetVolume, obj.handle).with_a(obj.volume), "")
    })
}

unsafe extern "thiscall" fn ws_set_pan(this: *mut c_void, pan: i32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        obj.pan = pan.clamp(-64, 63);
        fire(RequestHeader::new(Op::SetPan, obj.handle).with_a(obj.pan), "")
    })
}

unsafe extern "thiscall" fn ws_set_loop(this: *mut c_void, enabled: u32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        obj.looping = u32::from(enabled != 0);
        if obj.looping != 0 {
            obj.flags |= 2;
        } else {
            obj.flags &= !2;
        }
        fire(RequestHeader::new(Op::SetLoop, obj.handle).with_a(obj.looping as i32), "")
    })
}

unsafe extern "thiscall" fn ws_is_looping(this: *mut c_void) -> u32 {
    let obj = unsafe { &*(this as *const WaveSoundObj) };
    obj.looping
}

unsafe extern "thiscall" fn ws_is_playing(this: *mut c_void) -> u32 {
    safe(0, || {
        let obj = unsafe { &*(this as *const WaveSoundObj) };
        match client::call(RequestHeader::new(Op::QueryState, obj.handle), "") {
            Ok(r) => u32::from((r.value & STATE_PLAYING) != 0),
            Err(_) => 0,
        }
    })
}

unsafe extern "thiscall" fn ws_is_loaded(this: *mut c_void) -> u32 {
    let obj = unsafe { &*(this as *const WaveSoundObj) };
    obj.is_loaded
}

unsafe extern "thiscall" fn ws_set_flags(this: *mut c_void, flags: u32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        obj.flags |= flags;
        if (flags & 2) != 0 {
            obj.looping = 1;
        }
        fire(RequestHeader::new(Op::SetFlags, obj.handle).with_a(flags as i32), "")
    })
}

unsafe extern "thiscall" fn ws_get_flags(this: *mut c_void) -> u32 {
    let obj = unsafe { &*(this as *const WaveSoundObj) };
    obj.flags
}

unsafe extern "thiscall" fn ws_get_type(this: *mut c_void) -> u32 {
    let obj = unsafe { &*(this as *const WaveSoundObj) };
    obj.sound_type
}

unsafe extern "thiscall" fn ws_load_deferred(this: *mut c_void) -> u32 {
    safe(SNDERR_UNLOADED, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        let path = wave_deferred_path(obj);
        wave_load_path(obj, &path)
    })
}

unsafe extern "thiscall" fn ws_configure(this: *mut c_void, path_ptr: *const u8, flags: u32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        if let Some(path) = unsafe { read_cstr(path_ptr) } {
            wave_set_deferred_path(obj, &path);
        }
        obj.flags |= flags;
        if (flags & 2) != 0 {
            obj.looping = 1;
        }
        fire(RequestHeader::new(Op::SetFlags, obj.handle).with_a(flags as i32), "")
    })
}

unsafe extern "thiscall" fn ws_configure_and_load(
    this: *mut c_void,
    path_ptr: *const u8,
    flags: u32,
) -> u32 {
    safe(SNDERR_INVALID_PARAM, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        obj.flags |= flags;
        if (flags & 2) != 0 {
            obj.looping = 1;
        }
        let _ = fire(RequestHeader::new(Op::SetFlags, obj.handle).with_a(flags as i32), "");
        let path = match unsafe { read_cstr(path_ptr) } {
            Some(p) if !p.is_empty() => p,
            _ => wave_deferred_path(obj),
        };
        wave_load_path(obj, &path)
    })
}

unsafe extern "thiscall" fn ws_play_instance(this: *mut c_void, out_voice: *mut u32) -> u32 {
    safe(SNDERR_UNLOADED, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        let err = ensure_lazy_loaded(obj);
        if err != SNDERR_OK {
            return err;
        }
        match client::call(RequestHeader::new(Op::PlayInstance, obj.handle), "") {
            Ok(r) => {
                if !out_voice.is_null() {
                    unsafe { *out_voice = r.value };
                }
                r.snderr
            }
            Err(e) => e,
        }
    })
}

unsafe extern "thiscall" fn ws_start_voice(this: *mut c_void, voice: u32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        let err = ensure_lazy_loaded(obj);
        if err != SNDERR_OK {
            return err;
        }
        fire(RequestHeader::new(Op::StartVoice, obj.handle).with_a(voice as i32), "")
    })
}

unsafe extern "thiscall" fn ws_set_pitch(this: *mut c_void, cents: i32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        obj.pitch_cents = cents;
        fire(RequestHeader::new(Op::SetPitch, obj.handle).with_a(cents), "")
    })
}

unsafe extern "thiscall" fn ws_set_pos_ms(this: *mut c_void, ms: u32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut WaveSoundObj) };
        fire(RequestHeader::new(Op::SetPositionMs, obj.handle).with_a(ms as i32), "")
    })
}

unsafe extern "thiscall" fn ws_get_pos_ms(this: *mut c_void) -> u32 {
    safe(0, || {
        let obj = unsafe { &*(this as *const WaveSoundObj) };
        client::call(RequestHeader::new(Op::QueryState, obj.handle), "")
            .map_or(0, |r| r.extra2)
    })
}

unsafe extern "thiscall" fn ws_get_dur_ms(this: *mut c_void) -> u32 {
    let obj = unsafe { &*(this as *const WaveSoundObj) };
    obj.duration_ms
}

unsafe extern "thiscall" fn ws_is_ds_buf(this: *mut c_void) -> u32 {
    let obj = unsafe { &*(this as *const WaveSoundObj) };
    obj.is_ds_buffer as u32
}

pub static WAVE_SOUND_VT: VTable<WAVE_SOUND_SLOTS> = VTable([
    ws_dtor as *const c_void,               // 0  (+0x00) ret 4
    ws_set_fade_ms as *const c_void,        // 1  (+0x04) ret 4
    stub1 as *const c_void,                 // 2  (+0x08) ret 4
    stub1 as *const c_void,                 // 3  (+0x0C) ret 4
    ws_load as *const c_void,               // 4  (+0x10) ret 4
    ws_unload as *const c_void,             // 5  (+0x14) ret 0
    stub1 as *const c_void,                 // 6  (+0x18) ret 4
    ws_play as *const c_void,               // 7  (+0x1C) ret 0
    ws_stop as *const c_void,               // 8  (+0x20) ret 0
    ws_fade_out_ms as *const c_void,        // 9  (+0x24) ret 4
    ws_fade_out as *const c_void,           // 10 (+0x28) ret 0
    ws_fade_out_ms as *const c_void,        // 11 (+0x2C) ret 4 (calls vt[+0x54](ms) then vt[+0x28] fade_out)
    ws_fade_in as *const c_void,            // 12 (+0x30) ret 0
    ws_fade as *const c_void,               // 13 (+0x34) ret 12
    ws_pause as *const c_void,              // 14 (+0x38) ret 0
    stub0 as *const c_void,                 // 15 (+0x3C) ret 0
    ws_set_volume as *const c_void,         // 16 (+0x40) ret 4
    ws_set_pan as *const c_void,            // 17 (+0x44) ret 4
    ws_set_loop as *const c_void,           // 18 (+0x48) ret 4
    stub1 as *const c_void,                 // 19 (+0x4C) ret 4
    stub1 as *const c_void,                 // 20 (+0x50) ret 4
    ws_set_fade_ms as *const c_void,        // 21 (+0x54) ret 4 (sets fade-out ms used by vt[+0x28])
    ws_is_looping as *const c_void,         // 22 (+0x58) ret 0
    ws_is_playing as *const c_void,         // 23 (+0x5C) ret 0
    ws_is_loaded as *const c_void,          // 24 (+0x60) ret 0
    stub0 as *const c_void,                 // 25 (+0x64) ret 0
    stub0 as *const c_void,                 // 26 (+0x68) ret 0
    ws_set_flags as *const c_void,          // 27 (+0x6C) ret 4
    ws_get_flags as *const c_void,          // 28 (+0x70) ret 0
    stub0 as *const c_void,                 // 29 (+0x74) ret 0
    ws_get_type as *const c_void,           // 30 (+0x78) ret 0
    stub0 as *const c_void,                 // 31 (+0x7C) ret 0
    stub0 as *const c_void,                 // 32 (+0x80) ret 0
    ws_load_deferred as *const c_void,      // 33 (+0x84) ret 0
    ws_configure as *const c_void,          // 34 (+0x88) ret 8
    ws_load_deferred as *const c_void,      // 35 (+0x8C) ret 0 (commit_load)
    ws_configure_and_load as *const c_void, // 36 (+0x90) ret 8
    ws_play_instance as *const c_void,      // 37 (+0x94) ret 4
    ws_start_voice as *const c_void,        // 38 (+0x98) ret 4
    ws_set_pitch as *const c_void,          // 39 (+0x9C) ret 4
    stub1 as *const c_void,                 // 40 (+0xA0) ret 4
    stub0 as *const c_void,                 // 41 (+0xA4) ret 0
    stub3 as *const c_void,                 // 42 (+0xA8) ret 12
    stub3 as *const c_void,                 // 43 (+0xAC) ret 12
    stub3 as *const c_void,                 // 44 (+0xB0) ret 12
    stub3 as *const c_void,                 // 45 (+0xB4) ret 12
    ws_set_pos_ms as *const c_void,         // 46 (+0xB8) ret 4
    ws_get_pos_ms as *const c_void,         // 47 (+0xBC) ret 0
    stub0 as *const c_void,                 // 48 (+0xC0) ret 0
    stub3 as *const c_void,                 // 49 (+0xC4) ret 12
    ws_get_dur_ms as *const c_void,         // 50 (+0xC8) ret 0
    ws_is_ds_buf as *const c_void,          // 51 (+0xCC) ret 0
    stub1 as *const c_void,                 // 52 (+0xD0) ret 4
    stub1 as *const c_void,                 // 53 (+0xD4) ret 4
    stub3 as *const c_void,                 // 54 (+0xD8) ret 12
    stub1 as *const c_void,                 // 55 (+0xDC) ret 4
    stub1 as *const c_void,                 // 56 (+0xE0) ret 4
    stub1 as *const c_void,                 // 57 (+0xE4) ret 4
    stub0 as *const c_void,                 // 58 (+0xE8) ret 0
    stub1 as *const c_void,                 // 59 (+0xEC) ret 4
    stub2 as *const c_void,                 // 60 (+0xF0) ret 8
]);

// ============================================================================
// AmbSound — Type 5 (78 slots, 0x00..0x134)
// ============================================================================

fn sync_amb_meta(obj: &AmbSoundObj) {
    let _ = client::call(
        RequestHeader::new(Op::AmbSyncMeta, obj.handle)
            .with_a(obj.flags_58 as i32)
            .with_b(obj.ms_per_frame as i32),
        "",
    );
}

unsafe extern "thiscall" fn amb_dtor(this: *mut c_void, flags: u32) -> *mut c_void {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !this.is_null() {
            let obj = unsafe { &mut *(this as *mut AmbSoundObj) };
            if obj.handle != 0 {
                let _ = client::call(RequestHeader::new(Op::DeleteSound, obj.handle), "");
                obj.handle = 0;
            }
            if (flags & 1) != 0 {
                unsafe { drop(Box::from_raw(this as *mut AmbSoundObj)) };
            }
        }
    }));
    this
}

unsafe extern "thiscall" fn amb_load(this: *mut c_void, path_ptr: *const u8) -> u32 {
    safe(SNDERR_INVALID_PARAM, || {
        let obj = unsafe { &mut *(this as *mut AmbSoundObj) };
        let Some(path) = (unsafe { read_cstr(path_ptr) }) else {
            return SNDERR_INVALID_PARAM;
        };
        let err = fire(
            RequestHeader::new(Op::Load, obj.handle).with_a(obj.flags_58 as i32),
            &path,
        );
        if err == SNDERR_OK {
            obj.is_loaded = 0xFFFF_FFFF;
        }
        err
    })
}

unsafe extern "thiscall" fn amb_unload(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut AmbSoundObj) };
        obj.is_loaded = 0;
        fire(RequestHeader::new(Op::Unload, obj.handle), "")
    })
}

unsafe extern "thiscall" fn amb_play(this: *mut c_void) -> u32 {
    safe(SNDERR_UNLOADED, || {
        let obj = unsafe { &mut *(this as *mut AmbSoundObj) };
        if obj.is_loaded == 0 {
            return SNDERR_UNLOADED;
        }
        obj.cur_frame = 0;
        sync_amb_meta(obj);
        fire(RequestHeader::new(Op::Play, obj.handle), "")
    })
}

unsafe extern "thiscall" fn amb_stop(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &*(this as *const AmbSoundObj) };
        sync_amb_meta(obj);
        fire(RequestHeader::new(Op::Stop, obj.handle).with_a(0), "")
    })
}

unsafe extern "thiscall" fn amb_set_volume(this: *mut c_void, vol: i32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut AmbSoundObj) };
        obj.volume = vol.clamp(0, 127);
        fire(RequestHeader::new(Op::SetVolume, obj.handle).with_a(obj.volume), "")
    })
}

unsafe extern "thiscall" fn amb_set_pan(this: *mut c_void, pan: i32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut AmbSoundObj) };
        obj.pan = pan.clamp(-64, 63);
        fire(RequestHeader::new(Op::SetPan, obj.handle).with_a(obj.pan), "")
    })
}

unsafe extern "thiscall" fn amb_set_loop(this: *mut c_void, enabled: u32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut AmbSoundObj) };
        obj.looping = u32::from(enabled != 0);
        fire(RequestHeader::new(Op::SetLoop, obj.handle).with_a(obj.looping as i32), "")
    })
}

unsafe extern "thiscall" fn amb_is_looping(this: *mut c_void) -> u32 {
    unsafe { (*(this as *const AmbSoundObj)).looping }
}

unsafe extern "thiscall" fn amb_is_playing(this: *mut c_void) -> u32 {
    safe(0, || {
        let obj = unsafe { &*(this as *const AmbSoundObj) };
        match client::call(RequestHeader::new(Op::QueryState, obj.handle), "") {
            Ok(r) => u32::from((r.value & STATE_PLAYING) != 0),
            Err(_) => 0,
        }
    })
}

unsafe extern "thiscall" fn amb_is_loaded(this: *mut c_void) -> u32 {
    unsafe { (*(this as *const AmbSoundObj)).is_loaded }
}

unsafe extern "thiscall" fn amb_get_type(_this: *mut c_void) -> u32 {
    5
}

/// Slot 76 (+0x130): `set_anim_frame(cur_frame: u32)` — called by Civ3 FLC animation tick.
unsafe extern "thiscall" fn amb_set_anim_frame(this: *mut c_void, frame: u32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut AmbSoundObj) };
        obj.cur_frame = frame;
        fire(
            RequestHeader::new(Op::AmbSetAnimFrame, obj.handle)
                .with_a(frame as i32)
                .with_b(obj.flags_58 as i32)
                .with_c(obj.ms_per_frame as i32),
            "",
        )
    })
}

/// Slot 77 (+0x134): `get_anim_frame() -> u32`.
unsafe extern "thiscall" fn amb_get_anim_frame(this: *mut c_void) -> u32 {
    unsafe { (*(this as *const AmbSoundObj)).cur_frame }
}

pub static AMB_SOUND_VT: VTable<AMB_SOUND_SLOTS> = VTable([
    amb_dtor as *const c_void,           // 0  (+0x00) ret 4
    stub1 as *const c_void,              // 1  (+0x04) ret 4
    stub1 as *const c_void,              // 2  (+0x08) ret 4
    stub1 as *const c_void,              // 3  (+0x0C) ret 4
    amb_load as *const c_void,           // 4  (+0x10) ret 4
    amb_unload as *const c_void,         // 5  (+0x14) ret 0
    stub1 as *const c_void,              // 6  (+0x18) ret 4
    amb_play as *const c_void,           // 7  (+0x1C) ret 0
    amb_stop as *const c_void,           // 8  (+0x20) ret 0
    stub1 as *const c_void,              // 9  (+0x24) ret 4
    amb_stop as *const c_void,           // 10 (+0x28) ret 0 (fade_out -> stop w/ ring-out)
    stub1 as *const c_void,              // 11 (+0x2C) ret 4
    amb_play as *const c_void,           // 12 (+0x30) ret 0
    stub3 as *const c_void,              // 13 (+0x34) ret 12
    stub0 as *const c_void,              // 14 (+0x38) ret 0
    stub0 as *const c_void,              // 15 (+0x3C) ret 0
    amb_set_volume as *const c_void,     // 16 (+0x40) ret 4
    amb_set_pan as *const c_void,        // 17 (+0x44) ret 4
    amb_set_loop as *const c_void,       // 18 (+0x48) ret 4
    stub1 as *const c_void,              // 19 (+0x4C) ret 4
    stub1 as *const c_void,              // 20 (+0x50) ret 4
    stub1 as *const c_void,              // 21 (+0x54) ret 4
    amb_is_looping as *const c_void,     // 22 (+0x58) ret 0
    amb_is_playing as *const c_void,     // 23 (+0x5C) ret 0
    amb_is_loaded as *const c_void,      // 24 (+0x60) ret 0
    stub0 as *const c_void,              // 25 (+0x64) ret 0
    stub0 as *const c_void,              // 26 (+0x68) ret 0
    stub1 as *const c_void,              // 27 (+0x6C) ret 4
    stub0 as *const c_void,              // 28 (+0x70) ret 0
    stub0 as *const c_void,              // 29 (+0x74) ret 0
    amb_get_type as *const c_void,       // 30 (+0x78) ret 0
    stub0 as *const c_void,              // 31 (+0x7C) ret 0
    stub0 as *const c_void,              // 32 (+0x80) ret 0
    stub1 as *const c_void,              // 33 (+0x84) ret 4
    stub1 as *const c_void,              // 34 (+0x88) ret 4
    stub1 as *const c_void,              // 35 (+0x8C) ret 4
    stub1 as *const c_void,              // 36 (+0x90) ret 4
    stub2 as *const c_void,              // 37 (+0x94) ret 8
    stub1 as *const c_void,              // 38 (+0x98) ret 4
    stub1 as *const c_void,              // 39 (+0x9C) ret 4
    stub2 as *const c_void,              // 40 (+0xA0) ret 8
    stub2 as *const c_void,              // 41 (+0xA4) ret 8
    stub1 as *const c_void,              // 42 (+0xA8) ret 4
    stub2 as *const c_void,              // 43 (+0xAC) ret 8
    stub1 as *const c_void,              // 44 (+0xB0) ret 4
    stub0 as *const c_void,              // 45 (+0xB4) ret 0
    stub0 as *const c_void,              // 46 (+0xB8) ret 0
    stub2 as *const c_void,              // 47 (+0xBC) ret 8
    stub0 as *const c_void,              // 48 (+0xC0) ret 0
    stub2 as *const c_void,              // 49 (+0xC4) ret 8
    stub1 as *const c_void,              // 50 (+0xC8) ret 4
    stub2 as *const c_void,              // 51 (+0xCC) ret 8
    stub3 as *const c_void,              // 52 (+0xD0) ret 12
    stub1 as *const c_void,              // 53 (+0xD4) ret 4
    stub1 as *const c_void,              // 54 (+0xD8) ret 4
    stub1 as *const c_void,              // 55 (+0xDC) ret 4
    stub1 as *const c_void,              // 56 (+0xE0) ret 4
    stub1 as *const c_void,              // 57 (+0xE4) ret 4
    stub2 as *const c_void,              // 58 (+0xE8) ret 8
    stub1 as *const c_void,              // 59 (+0xEC) ret 4
    stub1 as *const c_void,              // 60 (+0xF0) ret 4
    stub2 as *const c_void,              // 61 (+0xF4) ret 8
    stub1 as *const c_void,              // 62 (+0xF8) ret 4
    stub1 as *const c_void,              // 63 (+0xFC) ret 4
    stub1 as *const c_void,              // 64 (+0x100) ret 4
    stub1 as *const c_void,              // 65 (+0x104) ret 4
    stub2 as *const c_void,              // 66 (+0x108) ret 8
    stub1 as *const c_void,              // 67 (+0x10C) ret 4
    stub2 as *const c_void,              // 68 (+0x110) ret 8
    stub3 as *const c_void,              // 69 (+0x114) ret 12
    stub2 as *const c_void,              // 70 (+0x118) ret 8
    stub0 as *const c_void,              // 71 (+0x11C) ret 0
    stub1 as *const c_void,              // 72 (+0x120) ret 4
    stub2 as *const c_void,              // 73 (+0x124) ret 8
    stub2 as *const c_void,              // 74 (+0x128) ret 8
    stub0 as *const c_void,              // 75 (+0x12C) ret 0
    amb_set_anim_frame as *const c_void, // 76 (+0x130) ret 4
    amb_get_anim_frame as *const c_void, // 77 (+0x134) ret 0
]);

// ============================================================================
// VoiceSound / Stub Types 2, 4, 6, 7 (38 slots, 0x00..0x94)
// ============================================================================

unsafe extern "thiscall" fn voice_dtor(this: *mut c_void, flags: u32) -> *mut c_void {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !this.is_null() {
            let obj = unsafe { &mut *(this as *mut VoiceSoundObj) };
            if obj.handle != 0 {
                let _ = client::call(RequestHeader::new(Op::DeleteSound, obj.handle), "");
                obj.handle = 0;
            }
            if (flags & 1) != 0 {
                unsafe { drop(Box::from_raw(this as *mut VoiceSoundObj)) };
            }
        }
    }));
    this
}

unsafe extern "thiscall" fn voice_get_type(this: *mut c_void) -> u32 {
    unsafe { (*(this as *const VoiceSoundObj)).sound_type }
}

pub static VOICE_SOUND_VT: VTable<VOICE_SOUND_SLOTS> = VTable([
    voice_dtor as *const c_void,     // 0  ret 4
    stub1 as *const c_void,          // 1  ret 4
    stub1 as *const c_void,          // 2  ret 4
    stub1 as *const c_void,          // 3  ret 4
    stub1 as *const c_void,          // 4  ret 4
    stub0 as *const c_void,          // 5  ret 0
    stub1 as *const c_void,          // 6  ret 4
    stub0 as *const c_void,          // 7  ret 0
    stub0 as *const c_void,          // 8  ret 0
    stub1 as *const c_void,          // 9  ret 4
    stub0 as *const c_void,          // 10 ret 0
    stub1 as *const c_void,          // 11 ret 4
    stub0 as *const c_void,          // 12 ret 0
    stub3 as *const c_void,          // 13 ret 12
    stub0 as *const c_void,          // 14 ret 0
    stub0 as *const c_void,          // 15 ret 0
    stub1 as *const c_void,          // 16 ret 4
    stub1 as *const c_void,          // 17 ret 4
    stub1 as *const c_void,          // 18 ret 4
    stub1 as *const c_void,          // 19 ret 4
    stub1 as *const c_void,          // 20 ret 4
    stub1 as *const c_void,          // 21 ret 4
    stub0 as *const c_void,          // 22 ret 0
    stub0 as *const c_void,          // 23 ret 0
    stub0 as *const c_void,          // 24 ret 0
    stub0 as *const c_void,          // 25 ret 0
    stub0 as *const c_void,          // 26 ret 0
    stub1 as *const c_void,          // 27 ret 4
    stub0 as *const c_void,          // 28 ret 0
    stub0 as *const c_void,          // 29 ret 0
    voice_get_type as *const c_void, // 30 ret 0
    stub0 as *const c_void,          // 31 ret 0
    stub0 as *const c_void,          // 32 ret 0
    stub3 as *const c_void,          // 33 ret 12
    stub0 as *const c_void,          // 34 ret 0
    stub1 as *const c_void,          // 35 ret 4
    stub0 as *const c_void,          // 36 ret 0
    stub0 as *const c_void,          // 37 ret 0
]);

// ============================================================================
// MusicPlaylist — Type 8 (36 slots, 0x00..0x8C)
// ============================================================================

/// Deleting dtor: MUST NEVER free pointers in `tracks[0..40]` (owned by exe CRT).
unsafe extern "thiscall" fn pl_dtor(this: *mut c_void, flags: u32) -> *mut c_void {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !this.is_null() {
            let obj = unsafe { &mut *(this as *mut PlaylistObj) };
            if obj.handle != 0 {
                let _ = client::call(RequestHeader::new(Op::DeleteSound, obj.handle), "");
                obj.handle = 0;
            }
            if (flags & 1) != 0 {
                unsafe { drop(Box::from_raw(this as *mut PlaylistObj)) };
            }
        }
    }));
    this
}

unsafe extern "thiscall" fn pl_play(this: *mut c_void) -> u32 {
    safe(SNDERR_UNLOADED, || {
        let obj = unsafe { &mut *(this as *mut PlaylistObj) };
        let err = fire(RequestHeader::new(Op::Play, obj.handle), "");
        if err == SNDERR_OK {
            obj.current_marker = 1;
            obj.queued_marker = 0;
        }
        err
    })
}

unsafe extern "thiscall" fn pl_stop(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut PlaylistObj) };
        obj.current_marker = 0;
        fire(RequestHeader::new(Op::Stop, obj.handle), "")
    })
}

unsafe extern "thiscall" fn pl_fade_out(this: *mut c_void) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut PlaylistObj) };
        obj.current_marker = 0;
        fire(RequestHeader::new(Op::FadeOut, obj.handle), "")
    })
}

unsafe extern "thiscall" fn pl_fade(
    this: *mut c_void,
    dir: i32,
    unused: i32,
    ms: i32,
) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut PlaylistObj) };
        fire(
            RequestHeader::new(Op::PlaylistFade, obj.handle)
                .with_a(dir)
                .with_b(unused)
                .with_c(ms),
            "",
        )
    })
}

unsafe extern "thiscall" fn pl_set_volume(this: *mut c_void, vol: i32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut PlaylistObj) };
        let v = vol & 0x7F;
        obj.volume = v;
        fire(RequestHeader::new(Op::SetVolume, obj.handle).with_a(v), "")
    })
}

unsafe extern "thiscall" fn pl_is_playing(this: *mut c_void) -> u32 {
    safe(0, || {
        let obj = unsafe { &mut *(this as *mut PlaylistObj) };
        match client::call(RequestHeader::new(Op::QueryState, obj.handle), "") {
            Ok(r) => {
                let playing = (r.value & STATE_PLAYING) != 0;
                obj.current_marker = u32::from(playing || (r.value & STATE_LOADED) != 0);
                obj.queued_marker = u32::from((r.value & STATE_PLAYLIST_HAS_QUEUED) != 0);
                u32::from(playing)
            }
            Err(_) => 0,
        }
    })
}

unsafe extern "thiscall" fn pl_is_loaded(this: *mut c_void) -> u32 {
    safe(0, || {
        let obj = unsafe { &*(this as *const PlaylistObj) };
        match client::call(RequestHeader::new(Op::QueryState, obj.handle), "") {
            Ok(r) => {
                if (r.value & (STATE_LOADED | STATE_DS_BUFFER | STATE_LOOPING)) != 0 {
                    0xFFFF_FFFF
                } else {
                    0
                }
            }
            Err(_) => 0,
        }
    })
}

unsafe extern "thiscall" fn pl_get_type(_this: *mut c_void) -> u32 {
    8
}

/// Slot 33 (+0x84): `queue_track(idx: u32) -> SNDERR`.
unsafe extern "thiscall" fn pl_queue_track(this: *mut c_void, idx: u32) -> u32 {
    safe(SNDERR_INVALID_PARAM, || {
        let obj = unsafe { &mut *(this as *mut PlaylistObj) };
        if idx >= 40 {
            return SNDERR_INVALID_PARAM;
        }
        let ptr = obj.tracks[idx as usize] as *const u8;
        if ptr.is_null() {
            return SNDERR_EMPTY_SLOT;
        }
        let Some(path) = (unsafe { read_cstr(ptr) }) else {
            return SNDERR_EMPTY_SLOT;
        };
        if path.is_empty() {
            return SNDERR_EMPTY_SLOT;
        }
        match client::call(RequestHeader::new(Op::PlaylistQueueTrack, obj.handle), &path) {
            Ok(r) => {
                if r.snderr == SNDERR_OK {
                    obj.queued_marker = 1;
                }
                r.snderr
            }
            Err(e) => e,
        }
    })
}

/// Slot 34 (+0x88): returns byte at `+0x104`.
unsafe extern "thiscall" fn pl_has_flag_104(this: *mut c_void) -> u32 {
    unsafe { (*(this as *const PlaylistObj)).flag_104 & 0xFF }
}

/// Slot 35 (+0x8C): sets scaled volume at `+0x108` and updates active track.
unsafe extern "thiscall" fn pl_set_scaled_volume(this: *mut c_void, vol: i32) -> u32 {
    safe(SNDERR_OK, || {
        let obj = unsafe { &mut *(this as *mut PlaylistObj) };
        let v = vol.clamp(0, 127);
        obj.scaled_volume = v;
        fire(RequestHeader::new(Op::PlaylistSetScaledVolume, obj.handle).with_a(v), "")
    })
}

pub static PLAYLIST_VT: VTable<PLAYLIST_SLOTS> = VTable([
    pl_dtor as *const c_void,              // 0  (+0x00) ret 4
    stub1 as *const c_void,                // 1  (+0x04) ret 4
    stub1 as *const c_void,                // 2  (+0x08) ret 4
    stub1 as *const c_void,                // 3  (+0x0C) ret 4
    stub1 as *const c_void,                // 4  (+0x10) ret 4
    pl_stop as *const c_void,              // 5  (+0x14) ret 0
    stub1 as *const c_void,                // 6  (+0x18) ret 4
    pl_play as *const c_void,              // 7  (+0x1C) ret 0
    pl_stop as *const c_void,              // 8  (+0x20) ret 0
    stub1 as *const c_void,                // 9  (+0x24) ret 4
    pl_fade_out as *const c_void,          // 10 (+0x28) ret 0
    stub1 as *const c_void,                // 11 (+0x2C) ret 4
    pl_play as *const c_void,              // 12 (+0x30) ret 0
    pl_fade as *const c_void,              // 13 (+0x34) ret 12
    stub0 as *const c_void,                // 14 (+0x38) ret 0
    stub0 as *const c_void,                // 15 (+0x3C) ret 0
    pl_set_volume as *const c_void,        // 16 (+0x40) ret 4
    stub1 as *const c_void,                // 17 (+0x44) ret 4
    stub1 as *const c_void,                // 18 (+0x48) ret 4
    stub1 as *const c_void,                // 19 (+0x4C) ret 4
    stub1 as *const c_void,                // 20 (+0x50) ret 4
    stub1 as *const c_void,                // 21 (+0x54) ret 4
    stub0 as *const c_void,                // 22 (+0x58) ret 0
    pl_is_playing as *const c_void,        // 23 (+0x5C) ret 0
    pl_is_loaded as *const c_void,         // 24 (+0x60) ret 0
    stub0 as *const c_void,                // 25 (+0x64) ret 0
    stub0 as *const c_void,                // 26 (+0x68) ret 0
    stub1 as *const c_void,                // 27 (+0x6C) ret 4
    stub0 as *const c_void,                // 28 (+0x70) ret 0
    stub0 as *const c_void,                // 29 (+0x74) ret 0
    pl_get_type as *const c_void,          // 30 (+0x78) ret 0
    stub0 as *const c_void,                // 31 (+0x7C) ret 0
    stub0 as *const c_void,                // 32 (+0x80) ret 0
    pl_queue_track as *const c_void,       // 33 (+0x84) ret 4
    pl_has_flag_104 as *const c_void,      // 34 (+0x88) ret 0
    pl_set_scaled_volume as *const c_void, // 35 (+0x8C) ret 4
]);
