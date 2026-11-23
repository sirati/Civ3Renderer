//! Deterministic 32-bit Windows audio capture driver for comparing the real
//! original `sound.dll` + `Mss32.dll` against our native `civ3-audio` engine.
//!
//! Run under Wine (`tools/wine-run`):
//!   equality_capture.exe <orig|native> <conquests_dir> <out_dir>
//!
//! In `orig` mode, patches `sound.dll`'s IAT for `ole32!CoCreateInstance`,
//! `DSOUND!@2` (`DirectSoundEnumerateA`), `WINMM!timeGetTime`,
//! `WINMM!timeSetEvent`, and `WINMM!timeKillEvent` so `sound.dll` renders into
//! deterministic in-memory `IDirectSoundBuffer` rings driven by our virtual
//! clock — zero audio hardware, zero OS timer jitter.

#[cfg(not(all(windows, target_arch = "x86")))]
fn main() {
    eprintln!("equality_capture must be built for i686-pc-windows-gnu");
}

#[cfg(all(windows, target_arch = "x86"))]
fn main() {
    win32::run();
}

#[cfg(all(windows, target_arch = "x86"))]
mod win32 {
    use std::cell::RefCell;
    use std::ffi::{c_void, CString};
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryA(name: *const u8) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, proc_name: *const u8) -> *const c_void;
        fn SetCurrentDirectoryA(path: *const u8) -> i32;
        fn VirtualProtect(addr: *mut c_void, size: usize, new_prot: u32, old_prot: *mut u32) -> i32;
    }

    const PAGE_READWRITE: u32 = 0x04;
    const OUT_RATE: u32 = 44_100;

    // =========================================================================
    // Deterministic virtual clock + COM DirectSound capture state (single-thread)
    // =========================================================================

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct WaveFormatEx {
        format_tag: u16,
        channels: u16,
        samples_per_sec: u32,
        avg_bytes_per_sec: u32,
        block_align: u16,
        bits_per_sample: u16,
        cb_size: u16,
    }

    #[repr(C)]
    struct DsBufferDesc {
        dw_size: u32,
        dw_flags: u32,
        dw_buffer_bytes: u32,
        dw_reserved: u32,
        lpwfx_format: *const WaveFormatEx,
    }

    #[repr(C)]
    struct DsCaps {
        dw_size: u32,
        dw_flags: u32,
        dw_min_secondary_sample_rate: u32,
        dw_max_secondary_sample_rate: u32,
        dw_primary_buffers: u32,
        dw_max_hw_mixing_all_buffers: u32,
        dw_max_hw_mixing_static_buffers: u32,
        dw_max_hw_mixing_streaming_buffers: u32,
        dw_free_hw_mixing_all_buffers: u32,
        dw_free_hw_mixing_static_buffers: u32,
        dw_free_hw_mixing_streaming_buffers: u32,
        dw_max_hw_3d_all_buffers: u32,
        dw_max_hw_3d_static_buffers: u32,
        dw_max_hw_3d_streaming_buffers: u32,
        dw_free_hw_3d_all_buffers: u32,
        dw_free_hw_3d_static_buffers: u32,
        dw_free_hw_3d_streaming_buffers: u32,
        dw_total_hw_mem_bytes: u32,
        dw_free_hw_mem_bytes: u32,
        dw_max_contiguous_free_hw_mem_bytes: u32,
        dw_unlock_transfer_rate_hw_buffers: u32,
        dw_play_cpu_overhead_sw_buffers: u32,
        dw_reserved1: u32,
        dw_reserved2: u32,
    }

    #[repr(C)]
    struct DsBCaps {
        dw_size: u32,
        dw_flags: u32,
        dw_buffer_bytes: u32,
        dw_unlock_transfer_rate: u32,
        dw_play_cpu_overhead: u32,
    }

    struct CaptureBuffer {
        is_primary: bool,
        wfx: WaveFormatEx,
        data: Vec<u8>,
        play_cursor: usize,
        playing: bool,
        looping: bool,
        volume_mb: i32,
        pan_mb: i32,
        freq: u32,
        last_set_volume_mb: Option<i32>,
    }

    #[repr(C)]
    struct ComVTable<const N: usize>([*const c_void; N]);
    unsafe impl<const N: usize> Sync for ComVTable<N> {}

    #[repr(C)]
    struct ComDsBuffer {
        vtable: *const ComVTable<21>,
        ref_count: u32,
        buf_id: usize,
    }

    #[repr(C)]
    struct ComDirectSound {
        vtable: *const ComVTable<11>,
        ref_count: u32,
    }

    struct SimState {
        time_ms: u32,
        buffers: Vec<Option<CaptureBuffer>>,
        timer_cb: Option<unsafe extern "system" fn(u32, u32, usize, usize, usize)>,
        timer_user: usize,
        last_volume_mb: Option<i32>,
        lock_count: u32,
        play_count: u32,
    }

    thread_local! {
        static SIM: RefCell<SimState> = RefCell::new(SimState {
            time_ms: 1000,
            buffers: Vec::new(),
            timer_cb: None,
            timer_user: 0,
            last_volume_mb: None,
            lock_count: 0,
            play_count: 0,
        });
    }

    // ---- Fake IDirectSoundBuffer (21 COM __stdcall methods) ----

    unsafe extern "system" fn dsb_qi(_this: *mut ComDsBuffer, _iid: *const c_void, out: *mut *mut c_void) -> i32 {
        if !out.is_null() {
            unsafe { *out = std::ptr::null_mut() };
        }
        -2147467262 // E_NOINTERFACE
    }
    unsafe extern "system" fn dsb_addref(this: *mut ComDsBuffer) -> u32 {
        unsafe {
            (*this).ref_count += 1;
            (*this).ref_count
        }
    }
    unsafe extern "system" fn dsb_release(this: *mut ComDsBuffer) -> u32 {
        unsafe {
            if (*this).ref_count > 1 {
                (*this).ref_count -= 1;
                return (*this).ref_count;
            }
            let id = (*this).buf_id;
            SIM.with(|s| {
                let mut st = s.borrow_mut();
                if let Some(slot) = st.buffers.get_mut(id) {
                    *slot = None;
                }
            });
            drop(Box::from_raw(this));
            0
        }
    }
    unsafe extern "system" fn dsb_get_caps(this: *mut ComDsBuffer, caps: *mut DsBCaps) -> i32 {
        if caps.is_null() {
            return -2147024809;
        }
        let id = unsafe { (*this).buf_id };
        SIM.with(|s| {
            let st = s.borrow();
            if let Some(Some(b)) = st.buffers.get(id) {
                unsafe {
                    (*caps).dw_flags = if b.is_primary { 1 } else { 0x0000_00E0 };
                    (*caps).dw_buffer_bytes = b.data.len() as u32;
                    (*caps).dw_unlock_transfer_rate = 0;
                    (*caps).dw_play_cpu_overhead = 0;
                }
            }
        });
        0
    }
    unsafe extern "system" fn dsb_get_current_pos(
        this: *mut ComDsBuffer,
        out_play: *mut u32,
        out_write: *mut u32,
    ) -> i32 {
        let id = unsafe { (*this).buf_id };
        SIM.with(|s| {
            let st = s.borrow();
            if let Some(Some(b)) = st.buffers.get(id) {
                let len = b.data.len().max(1);
                let play = b.play_cursor % len;
                let lead = ((b.wfx.avg_bytes_per_sec as usize) / 100).min(len / 8) & !3;
                let write = (play + lead) % len;
                unsafe {
                    if !out_play.is_null() {
                        *out_play = play as u32;
                    }
                    if !out_write.is_null() {
                        *out_write = write as u32;
                    }
                }
            }
        });
        0
    }
    unsafe extern "system" fn dsb_get_format(
        this: *mut ComDsBuffer,
        wfx: *mut WaveFormatEx,
        size: u32,
        written: *mut u32,
    ) -> i32 {
        let id = unsafe { (*this).buf_id };
        let need = std::mem::size_of::<WaveFormatEx>() as u32;
        if !written.is_null() {
            unsafe { *written = need };
        }
        if !wfx.is_null() && size >= need {
            SIM.with(|s| {
                let st = s.borrow();
                if let Some(Some(b)) = st.buffers.get(id) {
                    unsafe { *wfx = b.wfx };
                }
            });
        }
        0
    }
    unsafe extern "system" fn dsb_get_volume(this: *mut ComDsBuffer, out: *mut i32) -> i32 {
        let id = unsafe { (*this).buf_id };
        if !out.is_null() {
            SIM.with(|s| {
                if let Some(Some(b)) = s.borrow().buffers.get(id) {
                    unsafe { *out = b.volume_mb };
                }
            });
        }
        0
    }
    unsafe extern "system" fn dsb_get_pan(this: *mut ComDsBuffer, out: *mut i32) -> i32 {
        let id = unsafe { (*this).buf_id };
        if !out.is_null() {
            SIM.with(|s| {
                if let Some(Some(b)) = s.borrow().buffers.get(id) {
                    unsafe { *out = b.pan_mb };
                }
            });
        }
        0
    }
    unsafe extern "system" fn dsb_get_freq(this: *mut ComDsBuffer, out: *mut u32) -> i32 {
        let id = unsafe { (*this).buf_id };
        if !out.is_null() {
            SIM.with(|s| {
                if let Some(Some(b)) = s.borrow().buffers.get(id) {
                    unsafe { *out = b.freq };
                }
            });
        }
        0
    }
    unsafe extern "system" fn dsb_get_status(this: *mut ComDsBuffer, out: *mut u32) -> i32 {
        let id = unsafe { (*this).buf_id };
        if !out.is_null() {
            SIM.with(|s| {
                if let Some(Some(b)) = s.borrow().buffers.get(id) {
                    let mut st = 0u32;
                    if b.playing {
                        st |= 1;
                        if b.looping {
                            st |= 4;
                        }
                    }
                    unsafe { *out = st };
                }
            });
        }
        0
    }
    unsafe extern "system" fn dsb_init(_this: *mut ComDsBuffer, _ds: *mut c_void, _desc: *const c_void) -> i32 {
        0
    }
    unsafe extern "system" fn dsb_lock(
        this: *mut ComDsBuffer,
        offset: u32,
        bytes: u32,
        p1: *mut *mut c_void,
        s1: *mut u32,
        p2: *mut *mut c_void,
        s2: *mut u32,
        flags: u32,
    ) -> i32 {
        let id = unsafe { (*this).buf_id };
        SIM.with(|s| {
            let mut st = s.borrow_mut();
            st.lock_count += 1;
            if let Some(Some(b)) = st.buffers.get_mut(id) {
                let len = b.data.len();
                if len == 0 {
                    return;
                }
                let off = if (flags & 1) != 0 {
                    // DSBLOCK_FROMWRITECURSOR
                    b.play_cursor % len
                } else {
                    (offset as usize) % len
                };
                let req = if (flags & 2) != 0 {
                    // DSBLOCK_ENTIREBUFFER
                    len
                } else {
                    (bytes as usize).min(len)
                };
                let first = req.min(len - off);
                let second = req - first;
                let base = b.data.as_mut_ptr();
                unsafe {
                    if !p1.is_null() {
                        *p1 = base.add(off) as *mut c_void;
                    }
                    if !s1.is_null() {
                        *s1 = first as u32;
                    }
                    if !p2.is_null() {
                        *p2 = if second > 0 { base as *mut c_void } else { std::ptr::null_mut() };
                    }
                    if !s2.is_null() {
                        *s2 = second as u32;
                    }
                }
            }
        });
        0
    }
    unsafe extern "system" fn dsb_play(
        this: *mut ComDsBuffer,
        _res: u32,
        _prio: u32,
        flags: u32,
    ) -> i32 {
        let id = unsafe { (*this).buf_id };
        SIM.with(|s| {
            let mut st = s.borrow_mut();
            st.play_count += 1;
            if let Some(Some(b)) = st.buffers.get_mut(id) {
                b.playing = true;
                b.looping = (flags & 1) != 0;
            }
        });
        0
    }
    unsafe extern "system" fn dsb_set_current_pos(this: *mut ComDsBuffer, pos: u32) -> i32 {
        let id = unsafe { (*this).buf_id };
        SIM.with(|s| {
            if let Some(Some(b)) = s.borrow_mut().buffers.get_mut(id) {
                let len = b.data.len().max(1);
                b.play_cursor = (pos as usize) % len;
            }
        });
        0
    }
    unsafe extern "system" fn dsb_set_format(this: *mut ComDsBuffer, wfx: *const WaveFormatEx) -> i32 {
        if wfx.is_null() {
            return -2147024809;
        }
        let id = unsafe { (*this).buf_id };
        let f = unsafe { *wfx };
        SIM.with(|s| {
            if let Some(Some(b)) = s.borrow_mut().buffers.get_mut(id) {
                b.wfx = f;
                b.freq = f.samples_per_sec;
            }
        });
        0
    }
    unsafe extern "system" fn dsb_set_volume(this: *mut ComDsBuffer, vol: i32) -> i32 {
        let id = unsafe { (*this).buf_id };
        SIM.with(|s| {
            let mut st = s.borrow_mut();
            let mut record = false;
            if let Some(Some(b)) = st.buffers.get_mut(id) {
                b.volume_mb = vol.clamp(-10_000, 0);
                b.last_set_volume_mb = Some(b.volume_mb);
                if !b.is_primary {
                    record = true;
                }
            }
            if record {
                st.last_volume_mb = Some(vol.clamp(-10_000, 0));
            }
        });
        0
    }
    unsafe extern "system" fn dsb_set_pan(this: *mut ComDsBuffer, pan: i32) -> i32 {
        let id = unsafe { (*this).buf_id };
        SIM.with(|s| {
            if let Some(Some(b)) = s.borrow_mut().buffers.get_mut(id) {
                b.pan_mb = pan.clamp(-10_000, 10_000);
            }
        });
        0
    }
    unsafe extern "system" fn dsb_set_freq(this: *mut ComDsBuffer, freq: u32) -> i32 {
        let id = unsafe { (*this).buf_id };
        SIM.with(|s| {
            if let Some(Some(b)) = s.borrow_mut().buffers.get_mut(id) {
                b.freq = if freq == 0 { b.wfx.samples_per_sec } else { freq };
            }
        });
        0
    }
    unsafe extern "system" fn dsb_stop(this: *mut ComDsBuffer) -> i32 {
        let id = unsafe { (*this).buf_id };
        SIM.with(|s| {
            if let Some(Some(b)) = s.borrow_mut().buffers.get_mut(id) {
                b.playing = false;
            }
        });
        0
    }
    unsafe extern "system" fn dsb_unlock(
        _this: *mut ComDsBuffer,
        _p1: *mut c_void,
        _s1: u32,
        _p2: *mut c_void,
        _s2: u32,
    ) -> i32 {
        0
    }
    unsafe extern "system" fn dsb_restore(_this: *mut ComDsBuffer) -> i32 {
        0
    }

    static DSB_VT: ComVTable<21> = ComVTable([
        dsb_qi as *const c_void,
        dsb_addref as *const c_void,
        dsb_release as *const c_void,
        dsb_get_caps as *const c_void,
        dsb_get_current_pos as *const c_void,
        dsb_get_format as *const c_void,
        dsb_get_volume as *const c_void,
        dsb_get_pan as *const c_void,
        dsb_get_freq as *const c_void,
        dsb_get_status as *const c_void,
        dsb_init as *const c_void,
        dsb_lock as *const c_void,
        dsb_play as *const c_void,
        dsb_set_current_pos as *const c_void,
        dsb_set_format as *const c_void,
        dsb_set_volume as *const c_void,
        dsb_set_pan as *const c_void,
        dsb_set_freq as *const c_void,
        dsb_stop as *const c_void,
        dsb_unlock as *const c_void,
        dsb_restore as *const c_void,
    ]);

    // ---- Fake IDirectSound (11 COM __stdcall methods) ----

    unsafe extern "system" fn ds_qi(_this: *mut ComDirectSound, _iid: *const c_void, out: *mut *mut c_void) -> i32 {
        if !out.is_null() {
            unsafe { *out = std::ptr::null_mut() };
        }
        -2147467262
    }
    unsafe extern "system" fn ds_addref(this: *mut ComDirectSound) -> u32 {
        unsafe {
            (*this).ref_count += 1;
            (*this).ref_count
        }
    }
    unsafe extern "system" fn ds_release(this: *mut ComDirectSound) -> u32 {
        unsafe {
            if (*this).ref_count > 1 {
                (*this).ref_count -= 1;
                return (*this).ref_count;
            }
            drop(Box::from_raw(this));
            0
        }
    }
    unsafe extern "system" fn ds_create_buffer(
        _this: *mut ComDirectSound,
        desc: *const DsBufferDesc,
        out_buf: *mut *mut ComDsBuffer,
        _unk: *mut c_void,
    ) -> i32 {
        if desc.is_null() || out_buf.is_null() {
            return -2147024809;
        }
        let d = unsafe { &*desc };
        let is_primary = (d.dw_flags & 1) != 0;
        let wfx = if !d.lpwfx_format.is_null() {
            unsafe { *d.lpwfx_format }
        } else {
            WaveFormatEx {
                format_tag: 1,
                channels: 2,
                samples_per_sec: OUT_RATE,
                avg_bytes_per_sec: OUT_RATE * 4,
                block_align: 4,
                bits_per_sample: 16,
                cb_size: 0,
            }
        };
        let bytes = if is_primary { 4096 } else { d.dw_buffer_bytes as usize };
        let buf_id = SIM.with(|s| {
            let mut st = s.borrow_mut();
            let id = st.buffers.len();
            st.buffers.push(Some(CaptureBuffer {
                is_primary,
                wfx,
                data: vec![0u8; bytes],
                play_cursor: 0,
                playing: false,
                looping: false,
                volume_mb: 0,
                pan_mb: 0,
                freq: wfx.samples_per_sec,
                last_set_volume_mb: None,
            }));
            id
        });
        let obj = Box::into_raw(Box::new(ComDsBuffer {
            vtable: &DSB_VT,
            ref_count: 1,
            buf_id,
        }));
        unsafe { *out_buf = obj };
        0
    }
    unsafe extern "system" fn ds_get_caps(_this: *mut ComDirectSound, caps: *mut DsCaps) -> i32 {
        if !caps.is_null() {
            unsafe {
                // DSCAPS_PRIMARY16BIT | DSCAPS_PRIMARYSTEREO | DSCAPS_SECONDARY16BIT | DSCAPS_SECONDARYSTEREO
                // Deliberately omit DSCAPS_EMULDRIVER (0x20) so sound.dll uses GetCurrentPosition
                // (0x10010920) to write directly ahead of play_cursor on every tick.
                (*caps).dw_flags = 0x0000_0F0F;
                (*caps).dw_min_secondary_sample_rate = 100;
                (*caps).dw_max_secondary_sample_rate = 100_000;
                (*caps).dw_primary_buffers = 1;
            }
        }
        0
    }
    unsafe extern "system" fn ds_dup_buffer(
        _this: *mut ComDirectSound,
        _src: *mut c_void,
        _dst: *mut *mut c_void,
    ) -> i32 {
        -2147467263 // E_NOTIMPL
    }
    unsafe extern "system" fn ds_set_coop(_this: *mut ComDirectSound, _hwnd: u32, _level: u32) -> i32 {
        0
    }
    unsafe extern "system" fn ds_compact(_this: *mut ComDirectSound) -> i32 {
        0
    }
    unsafe extern "system" fn ds_get_speaker(_this: *mut ComDirectSound, out: *mut u32) -> i32 {
        if !out.is_null() {
            unsafe { *out = 2 }; // DSSPEAKER_STEREO
        }
        0
    }
    unsafe extern "system" fn ds_set_speaker(_this: *mut ComDirectSound, _cfg: u32) -> i32 {
        0
    }
    unsafe extern "system" fn ds_initialize(_this: *mut ComDirectSound, _guid: *const c_void) -> i32 {
        0
    }

    static DS_VT: ComVTable<11> = ComVTable([
        ds_qi as *const c_void,
        ds_addref as *const c_void,
        ds_release as *const c_void,
        ds_create_buffer as *const c_void,
        ds_get_caps as *const c_void,
        ds_dup_buffer as *const c_void,
        ds_set_coop as *const c_void,
        ds_compact as *const c_void,
        ds_get_speaker as *const c_void,
        ds_set_speaker as *const c_void,
        ds_initialize as *const c_void,
    ]);

    // ---- IAT hook callbacks for sound.dll ----

    unsafe extern "system" fn fake_co_create_instance(
        _clsid: *const c_void,
        _outer: *mut c_void,
        _ctx: u32,
        _iid: *const c_void,
        out: *mut *mut c_void,
    ) -> i32 {
        let ds = Box::into_raw(Box::new(ComDirectSound { vtable: &DS_VT, ref_count: 1 }));
        unsafe { *out = ds as *mut c_void };
        0
    }

    unsafe extern "system" fn fake_ds_enumerate(
        cb: unsafe extern "system" fn(*const c_void, *const u8, *const u8, *mut c_void) -> i32,
        ctx: *mut c_void,
    ) -> i32 {
        unsafe {
            cb(std::ptr::null(), b"Mock DirectSound\0".as_ptr(), b"mock\0".as_ptr(), ctx);
        }
        0
    }

    unsafe extern "system" fn fake_time_get_time() -> u32 {
        SIM.with(|s| s.borrow().time_ms)
    }

    unsafe extern "system" fn fake_time_set_event(
        _delay: u32,
        _res: u32,
        cb: unsafe extern "system" fn(u32, u32, usize, usize, usize),
        user: usize,
        _event: u32,
    ) -> u32 {
        SIM.with(|s| {
            let mut st = s.borrow_mut();
            st.timer_cb = Some(cb);
            st.timer_user = user;
        });
        1
    }

    unsafe extern "system" fn fake_time_kill_event(_id: u32) -> u32 {
        SIM.with(|s| {
            s.borrow_mut().timer_cb = None;
        });
        0
    }

    /// Patches `sound.dll`'s IAT entries for deterministic capture.
    unsafe fn patch_sound_dll_iat(module: *mut c_void) {
        let base = module as usize;
        let e_lfanew = unsafe { *((base + 0x3C) as *const u32) } as usize;
        // PE32 optional header starts at e_lfanew + 0x18; Import Table RVA is at +0x68 (+0x80 from PE sig).
        let import_rva = unsafe { *((base + e_lfanew + 0x80) as *const u32) } as usize;
        if import_rva == 0 {
            return;
        }
        let mut desc = (base + import_rva) as *const u32;
        loop {
            let orig_first_thunk = unsafe { *desc.add(0) } as usize;
            let name_rva = unsafe { *desc.add(3) } as usize;
            let first_thunk = unsafe { *desc.add(4) } as usize;
            if name_rva == 0 {
                break;
            }
            let dll_name = read_ascii_cstr((base + name_rva) as *const u8).to_ascii_lowercase();
            let int_base = if orig_first_thunk != 0 { base + orig_first_thunk } else { base + first_thunk };
            let iat_base = base + first_thunk;
            let mut idx = 0usize;
            loop {
                let entry = unsafe { *((int_base + idx * 4) as *const u32) };
                if entry == 0 {
                    break;
                }
                let slot = (iat_base + idx * 4) as *mut usize;
                let replacement: Option<usize> = if (entry & 0x8000_0000) != 0 {
                    let ord = entry & 0xFFFF;
                    if dll_name == "dsound.dll" && ord == 2 {
                        Some(fake_ds_enumerate as *const () as usize)
                    } else {
                        None
                    }
                } else {
                    let fn_name = read_ascii_cstr((base + entry as usize + 2) as *const u8);
                    match (dll_name.as_str(), fn_name.as_str()) {
                        ("ole32.dll", "CoCreateInstance") => Some(fake_co_create_instance as *const () as usize),
                        ("winmm.dll", "timeGetTime") => Some(fake_time_get_time as *const () as usize),
                        ("winmm.dll", "timeSetEvent") => Some(fake_time_set_event as *const () as usize),
                        ("winmm.dll", "timeKillEvent") => Some(fake_time_kill_event as *const () as usize),
                        _ => None,
                    }
                };
                if let Some(new_fn) = replacement {
                    let mut old_prot = 0u32;
                    unsafe {
                        VirtualProtect(slot as *mut c_void, 4, PAGE_READWRITE, &mut old_prot);
                        *slot = new_fn;
                        VirtualProtect(slot as *mut c_void, 4, old_prot, &mut old_prot);
                    }
                }
                idx += 1;
            }
            desc = unsafe { desc.add(5) };
        }
    }

    fn read_ascii_cstr(ptr: *const u8) -> String {
        let mut s = String::new();
        let mut i = 0usize;
        while i < 256 {
            let b = unsafe { *ptr.add(i) };
            if b == 0 {
                break;
            }
            s.push(b as char);
            i += 1;
        }
        s
    }

    fn fire_timer() {
        let cb = SIM.with(|s| {
            let st = s.borrow();
            (st.timer_cb, st.timer_user)
        });
        if let (Some(f), user) = cb {
            unsafe { f(1, 0, user, 0, 0) };
        }
    }

    /// Fires `sound.dll`'s timer callback so it mixes/refills ahead of
    /// `play_cursor`, pulls `dt_ms` (20ms -> 882 frames @ 44100 Hz) from all
    /// playing secondary DirectSound buffers, applies DS volume/pan + linear
    /// upsampling into 44100 Hz stereo, and advances play cursors + virtual time.
    fn sim_step(out_pcm: &mut Vec<i16>, dt_ms: u32) {
        fire_timer();
        let out_frames = ((OUT_RATE as usize) * (dt_ms as usize)) / 1000;
        let mut acc_l = vec![0.0_f64; out_frames];
        let mut acc_r = vec![0.0_f64; out_frames];

        // 1. Read samples that sound.dll wrote into playing buffers, and advance cursors.
        SIM.with(|s| {
            let mut st = s.borrow_mut();
            for slot in st.buffers.iter_mut() {
                let Some(b) = slot.as_mut() else { continue };
                if b.is_primary || !b.playing || b.data.is_empty() {
                    continue;
                }
                let sr = b.freq.max(1);
                let ch = (b.wfx.channels as usize).max(1);
                let bps = (b.wfx.bits_per_sample / 8).max(1) as usize;
                let frame_bytes = ch * bps;
                let total_src_frames = b.data.len() / frame_bytes;
                if total_src_frames == 0 {
                    continue;
                }
                let vol_gain = if b.volume_mb <= -10_000 {
                    0.0
                } else {
                    10f64.powf(f64::from(b.volume_mb) / 2000.0)
                };
                let (pan_l, pan_r) = if b.pan_mb < 0 {
                    (1.0, 10f64.powf(f64::from(b.pan_mb) / 2000.0))
                } else if b.pan_mb > 0 {
                    (10f64.powf(f64::from(-b.pan_mb) / 2000.0), 1.0)
                } else {
                    (1.0, 1.0)
                };

                let step = f64::from(sr) / f64::from(OUT_RATE);
                let start_frame = b.play_cursor / frame_bytes;
                for i in 0..out_frames {
                    let fpos = start_frame as f64 + (i as f64) * step;
                    let i0 = (fpos as usize) % total_src_frames;
                    let i1 = (i0 + 1) % total_src_frames;
                    let frac = fpos - fpos.floor();
                    let read_lr = |fi: usize| -> (f64, f64) {
                        let o = fi * frame_bytes;
                        if bps == 2 {
                            let l = i16::from_le_bytes([b.data[o], b.data[o + 1]]) as f64;
                            let r = if ch >= 2 {
                                i16::from_le_bytes([b.data[o + 2], b.data[o + 3]]) as f64
                            } else {
                                l
                            };
                            (l, r)
                        } else {
                            let l = ((i32::from(b.data[o]) << 8) - 32_768) as f64;
                            let r = if ch >= 2 {
                                ((i32::from(b.data[o + 1]) << 8) - 32_768) as f64
                            } else {
                                l
                            };
                            (l, r)
                        }
                    };
                    let (l0, r0) = read_lr(i0);
                    let (l1, r1) = read_lr(i1);
                    acc_l[i] += (l0 + frac * (l1 - l0)) * vol_gain * pan_l;
                    acc_r[i] += (r0 + frac * (r1 - r0)) * vol_gain * pan_r;
                }

                let adv_frames = ((sr as usize) * (dt_ms as usize)) / 1000;
                let adv_bytes = adv_frames * frame_bytes;
                if !b.looping && b.play_cursor + adv_bytes >= b.data.len() {
                    b.play_cursor = 0;
                    b.playing = false;
                } else {
                    b.play_cursor = (b.play_cursor + adv_bytes) % b.data.len();
                }
            }
            st.time_ms = st.time_ms.wrapping_add(dt_ms);
        });

        for i in 0..out_frames {
            out_pcm.push(acc_l[i].round_ties_even().clamp(-32_768.0, 32_767.0) as i16);
            out_pcm.push(acc_r[i].round_ties_even().clamp(-32_768.0, 32_767.0) as i16);
        }
    }

    // ---- sound.dll C ABI helpers ----

    type CreateSoundFn = unsafe extern "C" fn(*mut *mut c_void, *const u8, u32) -> u32;
    type DeleteSoundFn = unsafe extern "C" fn(*mut c_void) -> u32;
    type InitTimerFn = unsafe extern "C" fn(u32, u32) -> u32;
    type ReleaseSoundFn = unsafe extern "C" fn() -> u32;
    type CreateDevFn = unsafe extern "C" fn(*mut *mut c_void, u32) -> u32;
    type DeleteDevFn = unsafe extern "C" fn() -> u32;
    type GetVerFn = unsafe extern "C" fn() -> u32;

    struct SoundApi {
        create_sound: CreateSoundFn,
        delete_sound: DeleteSoundFn,
        init_timer: InitTimerFn,
        release_sound: ReleaseSoundFn,
        wave_create: CreateDevFn,
        wave_delete: DeleteDevFn,
        midi_create: CreateDevFn,
        midi_delete: DeleteDevFn,
        get_version: GetVerFn,
    }

    unsafe fn vt_slot(obj: *mut c_void, slot: usize) -> usize {
        let vt = unsafe { *(obj as *const *const usize) };
        unsafe { *vt.add(slot) }
    }
    unsafe fn call0(obj: *mut c_void, slot: usize) -> u32 {
        let f: unsafe extern "thiscall" fn(*mut c_void) -> u32 = unsafe { std::mem::transmute(vt_slot(obj, slot)) };
        unsafe { f(obj) }
    }
    unsafe fn call1_u32(obj: *mut c_void, slot: usize, a: u32) -> u32 {
        let f: unsafe extern "thiscall" fn(*mut c_void, u32) -> u32 = unsafe { std::mem::transmute(vt_slot(obj, slot)) };
        unsafe { f(obj, a) }
    }
    unsafe fn call1_i32(obj: *mut c_void, slot: usize, a: i32) -> u32 {
        let f: unsafe extern "thiscall" fn(*mut c_void, i32) -> u32 = unsafe { std::mem::transmute(vt_slot(obj, slot)) };
        unsafe { f(obj, a) }
    }
    unsafe fn call1_ptr(obj: *mut c_void, slot: usize, p: *const u8) -> u32 {
        let f: unsafe extern "thiscall" fn(*mut c_void, *const u8) -> u32 = unsafe { std::mem::transmute(vt_slot(obj, slot)) };
        unsafe { f(obj, p) }
    }
    unsafe fn call2_u32(obj: *mut c_void, slot: usize, a: u32, b: u32) -> u32 {
        let f: unsafe extern "thiscall" fn(*mut c_void, u32, u32) -> u32 = unsafe { std::mem::transmute(vt_slot(obj, slot)) };
        unsafe { f(obj, a, b) }
    }
    unsafe fn call3_i32(obj: *mut c_void, slot: usize, a: i32, b: i32, c: i32) -> u32 {
        let f: unsafe extern "thiscall" fn(*mut c_void, i32, i32, i32) -> u32 = unsafe { std::mem::transmute(vt_slot(obj, slot)) };
        unsafe { f(obj, a, b, c) }
    }

    fn write_wav(path: &Path, samples: &[i16]) {
        let data_bytes = (samples.len() * 2) as u32;
        let mut h = [0u8; 44];
        h[0..4].copy_from_slice(b"RIFF");
        h[4..8].copy_from_slice(&data_bytes.saturating_add(36).to_le_bytes());
        h[8..12].copy_from_slice(b"WAVE");
        h[12..16].copy_from_slice(b"fmt ");
        h[16..20].copy_from_slice(&16u32.to_le_bytes());
        h[20..22].copy_from_slice(&1u16.to_le_bytes());
        h[22..24].copy_from_slice(&2u16.to_le_bytes());
        h[24..28].copy_from_slice(&OUT_RATE.to_le_bytes());
        h[28..32].copy_from_slice(&(OUT_RATE * 4).to_le_bytes());
        h[32..34].copy_from_slice(&4u16.to_le_bytes());
        h[34..36].copy_from_slice(&16u16.to_le_bytes());
        h[36..40].copy_from_slice(b"data");
        h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
        let mut f = File::create(path).expect("create output wav");
        f.write_all(&h).unwrap();
        f.write_all(bytemuck::cast_slice(samples)).unwrap();
    }

    fn write_synthetic_wav_8bit(path: &Path) {
        // 22050 Hz mono 8-bit unsigned sine wave (440 Hz, 132300 frames = 132300 bytes > 0x20000).
        // Must exceed sound.dll's 128 KiB preload threshold so original sound.dll routes through
        // its working Inner_sw_mix_stream 8-bit kernel (0x1001a260) instead of Firaxis's broken
        // <128 KiB Inner_sw_mix_voice dispatch bug (0x100393da).
        let sr = 22_050u32;
        let n = 132_300usize;
        let mut data = Vec::with_capacity(n);
        for i in 0..n {
            let t = (i as f64) / f64::from(sr);
            let s = (128.0 + 100.0 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()).round() as u8;
            data.push(s);
        }
        let mut f = File::create(path).unwrap();
        let data_bytes = data.len() as u32;
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(data_bytes + 36).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
        f.write_all(&1u16.to_le_bytes()).unwrap(); // mono
        f.write_all(&sr.to_le_bytes()).unwrap();
        f.write_all(&sr.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&8u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data_bytes.to_le_bytes()).unwrap();
        f.write_all(&data).unwrap();
    }

    fn write_synthetic_wav_stereo16(path: &Path) {
        // 22050 Hz stereo 16-bit: L=330Hz, R=660Hz, 35000 frames = 140000 bytes > 0x20000.
        // Exceeds 128 KiB so original sound.dll routes through its working Inner_sw_mix_stream
        // stereo kernel (0x1001aa60) instead of Firaxis's <128 KiB Inner_sw_mix_voice bug (0x100393b1).
        let sr = 22_050u32;
        let n = 35_000usize;
        let mut pcm = Vec::with_capacity(n * 2);
        for i in 0..n {
            let t = (i as f64) / f64::from(sr);
            pcm.push((16_000.0 * (2.0 * std::f64::consts::PI * 330.0 * t).sin()) as i16);
            pcm.push((12_000.0 * (2.0 * std::f64::consts::PI * 660.0 * t).sin()) as i16);
        }
        let mut f = File::create(path).unwrap();
        let data_bytes = (pcm.len() * 2) as u32;
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(data_bytes + 36).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&2u16.to_le_bytes()).unwrap();
        f.write_all(&sr.to_le_bytes()).unwrap();
        f.write_all(&(sr * 4).to_le_bytes()).unwrap();
        f.write_all(&4u16.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data_bytes.to_le_bytes()).unwrap();
        f.write_all(bytemuck::cast_slice(&pcm)).unwrap();
    }

    /// Copies the real Cavalry unit `.wav` files into `out_dir` and writes a
    /// copy of `CavalryAttack.amb` with its random pitch/volume/keymap flags
    /// cleared so the `.amb` MIDI sequencer and FLC animation sync can be
    /// verified sample-for-sample without cross-RNG divergence.
    fn prepare_deterministic_cavalry_amb(conquests_dir: &Path, out_dir: &Path, dest: &Path) {
        let Some(civ3_root) = conquests_dir.parent() else { return };
        let cav_dir = civ3_root.join("Art").join("Units").join("Cavalry");
        if let Ok(rd) = std::fs::read_dir(&cav_dir) {
            for ent in rd.flatten() {
                let p = ent.path();
                if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")) {
                    if let Some(name) = p.file_name() {
                        let _ = std::fs::copy(&p, out_dir.join(name));
                    }
                }
            }
        }
        let Ok(mut bytes) = std::fs::read(cav_dir.join("CavalryAttack.amb")) else {
            return;
        };
        let mut pos = 0usize;
        while pos + 8 <= bytes.len() {
            if &bytes[pos..pos + 4] == b"MThd" {
                break;
            }
            if bytes[pos] == 0 {
                pos += 1;
                continue;
            }
            let tag = [bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]];
            let sz = u32::from_le_bytes([
                bytes[pos + 4],
                bytes[pos + 5],
                bytes[pos + 6],
                bytes[pos + 7],
            ]) as usize;
            let pl = pos + 8;
            if &tag == b"prgm" && pl + 8 <= bytes.len() {
                let fl = u32::from_le_bytes([
                    bytes[pl + 4],
                    bytes[pl + 5],
                    bytes[pl + 6],
                    bytes[pl + 7],
                ]);
                let cleared = fl & !0x03; // clear PRGM_RAND_PITCH | PRGM_RAND_VOL
                bytes[pl + 4..pl + 8].copy_from_slice(&cleared.to_le_bytes());
            } else if &tag == b"kmap" && pl + 8 <= bytes.len() {
                let fl = u32::from_le_bytes([
                    bytes[pl + 4],
                    bytes[pl + 5],
                    bytes[pl + 6],
                    bytes[pl + 7],
                ]);
                let cleared = fl & !0x01; // clear KMAP_RAND_ENTRY
                bytes[pl + 4..pl + 8].copy_from_slice(&cleared.to_le_bytes());
            }
            pos = pl.saturating_add(sz);
        }
        let _ = std::fs::write(dest, &bytes);
    }

    pub fn run() {
        let args: Vec<String> = std::env::args().collect();
        if args.len() < 4 {
            eprintln!("usage: equality_capture <dll_path> <conquests_dir> <out_dir> [patch_iat:0|1]");
            std::process::exit(2);
        }
        let dll_path = &args[1];
        let conquests_dir = &args[2];
        let out_dir = Path::new(&args[3]);
        let patch_iat = args.get(4).map_or(true, |s| s != "0");

        std::fs::create_dir_all(out_dir).expect("create out_dir");
        let synth_8bit = out_dir.join("synth_8bit.wav");
        let synth_stereo = out_dir.join("synth_stereo16.wav");
        write_synthetic_wav_8bit(&synth_8bit);
        write_synthetic_wav_stereo16(&synth_stereo);

        let c_cwd = CString::new(conquests_dir.as_str()).unwrap();
        unsafe { SetCurrentDirectoryA(c_cwd.as_ptr() as *const u8) };

        let c_dll = CString::new(dll_path.as_str()).unwrap();
        let hmod = unsafe { LoadLibraryA(c_dll.as_ptr() as *const u8) };
        assert!(!hmod.is_null(), "LoadLibraryA({dll_path}) failed");
        if patch_iat {
            unsafe { patch_sound_dll_iat(hmod) };
        }

        let api = unsafe {
            let ord = |n: usize| -> *const c_void {
                let p = GetProcAddress(hmod, n as *const u8);
                assert!(!p.is_null(), "missing ordinal {n} in {dll_path}");
                p
            };
            SoundApi {
                create_sound: std::mem::transmute(ord(1)),
                delete_sound: std::mem::transmute(ord(2)),
                init_timer: std::mem::transmute(ord(3)),
                release_sound: std::mem::transmute(ord(4)),
                wave_create: std::mem::transmute(ord(5)),
                wave_delete: std::mem::transmute(ord(6)),
                midi_create: std::mem::transmute(ord(7)),
                midi_delete: std::mem::transmute(ord(8)),
                get_version: std::mem::transmute(ord(11)),
            }
        };

        let ver = unsafe { (api.get_version)() };
        println!("version={ver:#010x}");
        assert_eq!(ver & 0xFF00, 0x0D00);

        // Exact Civ3 initialization order: init_sound_timer FIRST, then midi_dev, then wave_dev!
        assert_eq!(unsafe { (api.init_timer)(0, 0) }, 0);
        let mut midi_dev: *mut c_void = std::ptr::null_mut();
        let mut wave_dev: *mut c_void = std::ptr::null_mut();
        assert_eq!(unsafe { (api.midi_create)(&mut midi_dev, 0) }, 0);
        assert_eq!(unsafe { call2_u32(midi_dev, 4, 1, 0) }, 0);
        assert_eq!(unsafe { (api.wave_create)(&mut wave_dev, 0) }, 0);
        assert_eq!(unsafe { call2_u32(wave_dev, 4, 2, 1) }, 0);
        SIM.with(|s| {
            let st = s.borrow();
            println!(
                "init_stats: timer_cb={} buffers={} locks={} plays={}",
                st.timer_cb.is_some(),
                st.buffers.len(),
                st.lock_count,
                st.play_count
            );
        });

        // Helper: drain the SW mixer ring buffer between scenarios so no tail bleeds across tests.
        let drain_ring = || {
            if patch_iat {
                let mut discard = Vec::new();
                for _ in 0..90 {
                    sim_step(&mut discard, 20);
                }
            }
        };

        // Helper: run a WAV/MP3 scenario and capture `steps * 20ms` of PCM.
        let run_steps = |name: &str, steps: usize, per_step: &mut dyn FnMut(usize)| {
            let mut pcm = Vec::with_capacity(steps * 882 * 2);
            if patch_iat {
                for step in 0..steps {
                    per_step(step);
                    sim_step(&mut pcm, 20);
                }
                write_wav(&out_dir.join(format!("{name}.wav")), &pcm);
            } else {
                // Native DLL mode: use WaveDevice's built-in WAV recorder (slot 21/22)!
                let rec_path = out_dir.join(format!("{name}.wav"));
                let c_rec = CString::new(rec_path.to_str().unwrap()).unwrap();
                unsafe { call1_ptr(wave_dev, 21, c_rec.as_ptr() as *const u8) };
                for step in 0..steps {
                    per_step(step);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                unsafe { call0(wave_dev, 22) };
            }
        };

        let c_button = CString::new(r"..\Sounds\Button1.wav").unwrap();
        let c_enter = CString::new(r"..\Sounds\EnterTurn.wav").unwrap();
        let c_barbarian = CString::new(r"..\Sounds\Barbarian Raid.wav").unwrap();
        let c_mp3 = CString::new(r"..\Sounds\build\ancient\AncECfull.mp3").unwrap();
        let c_8bit = CString::new(synth_8bit.to_str().unwrap()).unwrap();
        let c_stereo = CString::new(synth_stereo.to_str().unwrap()).unwrap();

        // Prepare a deterministic copy of real CavalryAttack.amb (same Firaxis
        // chunks, MIDI tracks, and Cavalry WAVs, with PRGM_RAND_PITCH/VOL and
        // KMAP_RAND_ENTRY cleared) so we verify exact sample-level .amb MIDI
        // sequencing and FLC animation-frame sync without cross-RNG drift.
        let det_amb_path = out_dir.join("CavalryAttack_det.amb");
        prepare_deterministic_cavalry_amb(Path::new(conquests_dir), out_dir, &det_amb_path);
        let c_amb_det = CString::new(det_amb_path.to_str().unwrap()).unwrap();

        // Scenario 1: Mono 16-bit WAV unity
        {
            drain_ring();
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 1) }, 0);
            assert_eq!(unsafe { call1_ptr(s, 4, c_button.as_ptr() as *const u8) }, 0);
            assert_eq!(unsafe { call0(s, 7) }, 0);
            run_steps("wav_mono16_unity", 70, &mut |_| {});
            unsafe { call0(s, 8); (api.delete_sound)(s) };
        }

        // Scenario 2: Mono 16-bit WAV vol=64, pan=-32
        {
            drain_ring();
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 1) }, 0);
            assert_eq!(unsafe { call1_ptr(s, 4, c_button.as_ptr() as *const u8) }, 0);
            unsafe {
                call1_i32(s, 16, 64);
                call1_i32(s, 17, -32);
                call0(s, 7);
            }
            run_steps("wav_mono16_vol64_pan_l32", 70, &mut |_| {});
            unsafe { call0(s, 8); (api.delete_sound)(s) };
        }

        // Scenario 3: Mono 16-bit WAV vol=64, pan=+32
        {
            drain_ring();
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 1) }, 0);
            assert_eq!(unsafe { call1_ptr(s, 4, c_button.as_ptr() as *const u8) }, 0);
            unsafe {
                call1_i32(s, 16, 64);
                call1_i32(s, 17, 32);
                call0(s, 7);
            }
            run_steps("wav_mono16_vol64_pan_r32", 70, &mut |_| {});
            unsafe { call0(s, 8); (api.delete_sound)(s) };
        }

        // Scenario 4: Pitch +500 cents
        {
            drain_ring();
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 1) }, 0);
            assert_eq!(unsafe { call1_ptr(s, 4, c_button.as_ptr() as *const u8) }, 0);
            unsafe {
                call1_i32(s, 39, 500);
                call0(s, 7);
            }
            run_steps("wav_mono16_pitch_p500", 70, &mut |_| {});
            unsafe { call0(s, 8); (api.delete_sound)(s) };
        }

        // Scenario 5: Pitch -500 cents
        {
            drain_ring();
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 1) }, 0);
            assert_eq!(unsafe { call1_ptr(s, 4, c_button.as_ptr() as *const u8) }, 0);
            unsafe {
                call1_i32(s, 39, -500);
                call0(s, 7);
            }
            run_steps("wav_mono16_pitch_m500", 80, &mut |_| {});
            unsafe { call0(s, 8); (api.delete_sound)(s) };
        }

        // Scenario 6: Looping mono 16-bit WAV
        {
            drain_ring();
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 1) }, 0);
            assert_eq!(unsafe { call1_ptr(s, 4, c_button.as_ptr() as *const u8) }, 0);
            unsafe {
                call1_u32(s, 18, 1);
                call0(s, 7);
            }
            run_steps("wav_mono16_loop", 90, &mut |_| {});
            unsafe { call0(s, 8); (api.delete_sound)(s) };
        }

        // Scenario 7: Two-voice simultaneous mix
        {
            drain_ring();
            let mut s1: *mut c_void = std::ptr::null_mut();
            let mut s2: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s1, std::ptr::null(), 1) }, 0);
            assert_eq!(unsafe { (api.create_sound)(&mut s2, std::ptr::null(), 1) }, 0);
            assert_eq!(unsafe { call1_ptr(s1, 4, c_button.as_ptr() as *const u8) }, 0);
            assert_eq!(unsafe { call1_ptr(s2, 4, c_enter.as_ptr() as *const u8) }, 0);
            unsafe {
                call1_i32(s1, 16, 90);
                call1_i32(s1, 17, -20);
                call1_i32(s2, 16, 80);
                call1_i32(s2, 17, 20);
                call0(s1, 7);
                call0(s2, 7);
            }
            run_steps("wav_two_voice_mix", 90, &mut |_| {});
            unsafe {
                call0(s1, 8);
                call0(s2, 8);
                (api.delete_sound)(s1);
                (api.delete_sound)(s2);
            }
        }

        // Scenario 8: 8-bit unsigned mono WAV
        {
            drain_ring();
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 1) }, 0);
            assert_eq!(unsafe { call1_ptr(s, 4, c_8bit.as_ptr() as *const u8) }, 0);
            unsafe { call0(s, 7) };
            run_steps("wav_8bit_mono", 50, &mut |_| {});
            unsafe { call0(s, 8); (api.delete_sound)(s) };
        }

        // Scenario 9a: 16-bit stereo WAV on SW mixer with vol=96, pan=-16
        {
            drain_ring();
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 1) }, 0);
            assert_eq!(unsafe { call1_ptr(s, 4, c_stereo.as_ptr() as *const u8) }, 0);
            unsafe {
                call1_i32(s, 16, 96);
                call1_i32(s, 17, -16);
                call0(s, 7);
            }
            run_steps("wav_stereo16_sw_vol96_pan_l16", 35, &mut |_| {});
            unsafe { call0(s, 8); (api.delete_sound)(s) };
        }

        // Scenario 9b: Real Civ3 stereo 16-bit WAV (Barbarian Raid.wav) via DirectSound stream (flag 0x100)
        {
            drain_ring();
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 1) }, 0);
            unsafe { call1_u32(s, 27, 0x100) };
            assert_eq!(unsafe { call1_ptr(s, 4, c_barbarian.as_ptr() as *const u8) }, 0);
            unsafe {
                call1_i32(s, 16, 96);
                call0(s, 7);
            }
            // 65 steps = 1.30s (57330 output frames < sound.dll's 128 KiB mmio pre-buffer of 65536 frames).
            run_steps("wav_stereo16_stream_vol96", 65, &mut |_| {});
            unsafe { call0(s, 8); (api.delete_sound)(s) };
        }

        // Scenario 10: Real Civ3 MP3 streaming at unity (vol=127)
        {
            drain_ring();
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 1) }, 0);
            unsafe { call1_u32(s, 27, 0x100) };
            assert_eq!(unsafe { call1_ptr(s, 4, c_mp3.as_ptr() as *const u8) }, 0);
            unsafe { call0(s, 7) };
            run_steps("mp3_unity", 120, &mut |_| {});
            unsafe { call0(s, 8); (api.delete_sound)(s) };
        }

        // Scenario 11: Real Civ3 MP3 streaming at vol=64 (checks exact DSBuf_set_vol(64,127) = -701 mB)
        {
            drain_ring();
            SIM.with(|s| s.borrow_mut().last_volume_mb = None);
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 1) }, 0);
            unsafe { call1_u32(s, 27, 0x100) };
            assert_eq!(unsafe { call1_ptr(s, 4, c_mp3.as_ptr() as *const u8) }, 0);
            unsafe {
                call1_i32(s, 16, 64);
                call0(s, 7);
            }
            let mb = SIM.with(|s| s.borrow().last_volume_mb);
            println!("mp3_vol64_mb={mb:?}");
            run_steps("mp3_vol64", 120, &mut |_| {});
            unsafe { call0(s, 8); (api.delete_sound)(s) };
        }

        // Scenario 12: Real Civ3 MP3 fade-out over 1000ms
        {
            drain_ring();
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 1) }, 0);
            unsafe { call1_u32(s, 27, 0x100) };
            assert_eq!(unsafe { call1_ptr(s, 4, c_mp3.as_ptr() as *const u8) }, 0);
            unsafe { call0(s, 7) };
            run_steps("mp3_fadeout", 100, &mut |step| {
                if step == 20 {
                    // t=400ms: fade to 0 over 1000ms
                    unsafe { call3_i32(s, 13, 0, -1, 1000) };
                }
            });
            unsafe { call0(s, 8); (api.delete_sound)(s) };
        }

        // Scenario 13: Real Civ3 Type 8 MusicPlaylist (queue_track + scaled_volume=80 + play)
        {
            drain_ring();
            let mut pl: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut pl, std::ptr::null(), 8) }, 0);
            unsafe {
                // Populate tracks[0] at +0x58 with a C-string pointer (caller-owned, never freed by sound.dll).
                *((pl as usize + 0x58) as *mut *const u8) = c_mp3.as_ptr() as *const u8;
                assert_eq!(call1_u32(pl, 33, 0), 0); // queue_track(0)
                call1_i32(pl, 35, 80); // set_scaled_volume(80)
                assert_eq!(call0(pl, 7), 0); // play()
            }
            run_steps("playlist_mp3_vol80", 120, &mut |_| {});
            unsafe {
                call0(pl, 8);
                *((pl as usize + 0x58) as *mut usize) = 0;
                (api.delete_sound)(pl);
            }
        }

        // Scenario 14: Deterministic CavalryAttack.amb (real MIDI/WAVs with RNG bits cleared)
        // Stepped in 60ms ticks (exact multiple of both MidiDevice 15ms and WaveDevice 30ms timers)
        // with ms_per_frame = 60 so sound.dll's 30ms timer has zero beat-aliasing against our clock.
        {
            drain_ring();
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 5) }, 0);
            unsafe {
                *((s as usize + 0x58) as *mut u32) = 3;
                *((s as usize + 0x5C) as *mut u32) = 60;
            }
            let load_err = unsafe { call1_ptr(s, 4, c_amb_det.as_ptr() as *const u8) };
            println!("amb_det_load_err={load_err}");
            if load_err == 0 {
                unsafe { call0(s, 7) };
                let mut pcm = Vec::with_capacity(30 * 2646 * 2);
                if patch_iat {
                    for step in 0..30usize {
                        unsafe { call1_u32(s, 76, step as u32) };
                        sim_step(&mut pcm, 60);
                    }
                    write_wav(&out_dir.join("amb_cavalry_det.wav"), &pcm);
                } else {
                    let rec_path = out_dir.join("amb_cavalry_det.wav");
                    let c_rec = CString::new(rec_path.to_str().unwrap()).unwrap();
                    unsafe { call1_ptr(wave_dev, 21, c_rec.as_ptr() as *const u8) };
                    for step in 0..30usize {
                        unsafe { call1_u32(s, 76, step as u32) };
                        std::thread::sleep(std::time::Duration::from_millis(60));
                    }
                    unsafe { call0(wave_dev, 22) };
                }
                unsafe { call0(s, 8) };
            }
            unsafe { (api.delete_sound)(s) };
        }

        // Scenario 15: Unmodified real Civ3 .amb unit animation (CavalryAttack.amb with random pitch)
        {
            drain_ring();
            drain_ring();
            let c_amb = CString::new(r"..\Art\Units\Cavalry\CavalryAttack.amb").unwrap();
            let mut s: *mut c_void = std::ptr::null_mut();
            assert_eq!(unsafe { (api.create_sound)(&mut s, std::ptr::null(), 5) }, 0);
            unsafe {
                // Set exe-visible +0x58 flags = 3 (relative WAV + FLC anim sync) and +0x5C ms_per_frame = 66
                *((s as usize + 0x58) as *mut u32) = 3;
                *((s as usize + 0x5C) as *mut u32) = 66;
            }
            let load_err = unsafe { call1_ptr(s, 4, c_amb.as_ptr() as *const u8) };
            println!("amb_load_err={load_err}");
            if load_err == 0 {
                unsafe { call0(s, 7) };
                run_steps("amb_cavalry_attack", 90, &mut |step| {
                    let frame = ((step * 20) / 66) as u32;
                    unsafe { call1_u32(s, 76, frame) };
                });
                unsafe { call0(s, 8) };
            }
            unsafe { (api.delete_sound)(s) };
        }

        // Exact Civ3 teardown order: close/delete MIDI BEFORE Wave BEFORE release_sound!
        unsafe {
            call0(midi_dev, 5);
            (api.midi_delete)();
            call0(wave_dev, 5);
            (api.wave_delete)();
            (api.release_sound)();
        }
        SIM.with(|s| {
            let st = s.borrow();
            println!(
                "end_stats: buffers={} locks={} plays={}",
                st.buffers.len(),
                st.lock_count,
                st.play_count
            );
        });
        println!("equality_capture_done=1");
    }
}
