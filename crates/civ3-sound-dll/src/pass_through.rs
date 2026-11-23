//! Runtime mode selection and dynamic pass-through forwarding to `sound_orig.dll`.
//!
//! Mirrors `civ3-bridge/src/pass_through.rs`:
//! - `CIV3R_SOUND=original` (or `orig` / `0`): always forward to `sound_orig.dll`.
//! - `CIV3R_SOUND=native` (or `1`): always run the native Rust audio engine.
//! - Otherwise: native when `CIV3R_VIEWER` starts with `"native"` (the in-process
//!   unixlib mode where `civ3r_native.dll` is loaded), and pass-through otherwise
//!   (plain game launches, `--separate-viewer`, or non-native trace runs).

use std::ffi::OsStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Native,
    Original,
}

pub fn select_mode(sound_env: Option<&OsStr>, viewer_env: Option<&OsStr>) -> Mode {
    if let Some(v) = sound_env {
        if v.eq_ignore_ascii_case("original")
            || v.eq_ignore_ascii_case("orig")
            || v == OsStr::new("0")
        {
            return Mode::Original;
        }
        if v.eq_ignore_ascii_case("native") || v == OsStr::new("1") {
            return Mode::Native;
        }
    }
    if viewer_env
        .and_then(|v| v.to_str())
        .is_some_and(|s| s.starts_with("native"))
    {
        Mode::Native
    } else {
        Mode::Original
    }
}

pub fn mode_here() -> Mode {
    static CACHED: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        select_mode(
            std::env::var_os("CIV3R_SOUND").as_deref(),
            std::env::var_os("CIV3R_VIEWER").as_deref(),
        )
    })
}

#[cfg(all(windows, target_arch = "x86"))]
pub mod orig {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    const SNDERR_MISSING: u32 = 0x14;
    const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: u32 = 0x0000_0004;
    const GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT: u32 = 0x0000_0002;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryA(name: *const u8) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, proc_name: *const u8) -> *const c_void;
        fn GetModuleHandleExA(
            flags: u32,
            module_name: *const c_void,
            module_out: *mut *mut c_void,
        ) -> i32;
        fn GetModuleFileNameA(module: *mut c_void, buf: *mut u8, size: u32) -> u32;
    }

    type CreateSoundFn = unsafe extern "C" fn(*mut *mut c_void, *const u8, u32) -> u32;
    type DeleteSoundFn = unsafe extern "C" fn(*mut c_void) -> u32;
    type InitTimerFn = unsafe extern "C" fn(u32, u32) -> u32;
    type ReleaseSoundFn = unsafe extern "C" fn() -> u32;
    type CreateDevFn = unsafe extern "C" fn(*mut *mut c_void, u32) -> u32;
    type DeleteDevFn = unsafe extern "C" fn() -> u32;
    type GetVersionFn = unsafe extern "C" fn() -> u32;

    pub struct OrigExports {
        pub create_sound: CreateSoundFn,
        pub delete_sound: DeleteSoundFn,
        pub init_sound_timer: InitTimerFn,
        pub release_sound: ReleaseSoundFn,
        pub wave_create: CreateDevFn,
        pub wave_delete: DeleteDevFn,
        pub midi_create: CreateDevFn,
        pub midi_delete: DeleteDevFn,
        pub wave_in_create: CreateDevFn,
        pub wave_in_delete: DeleteDevFn,
        pub get_version: GetVersionFn,
    }

    // Function pointers loaded from a resident DLL are thread-safe to read.
    unsafe impl Send for OrigExports {}
    unsafe impl Sync for OrigExports {}

    static ORIG: OnceLock<Option<OrigExports>> = OnceLock::new();

    fn load_orig_module() -> *mut c_void {
        // Prefer `sound_orig.dll` in the same folder as this `sound.dll`.
        let mut this_mod: *mut c_void = std::ptr::null_mut();
        let ok = unsafe {
            GetModuleHandleExA(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                    | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                load_orig_module as *const c_void,
                &mut this_mod,
            )
        };
        if ok != 0 && !this_mod.is_null() {
            let mut path_buf = [0u8; 520];
            let len = unsafe {
                GetModuleFileNameA(this_mod, path_buf.as_mut_ptr(), (path_buf.len() - 1) as u32)
            } as usize;
            if len > 0 && len < path_buf.len() {
                if let Some(pos) = path_buf[..len].iter().rposition(|&b| b == b'\\' || b == b'/') {
                    let mut candidate = Vec::with_capacity(pos + 32);
                    candidate.extend_from_slice(&path_buf[..=pos]);
                    candidate.extend_from_slice(b"sound_orig.dll\0");
                    let h = unsafe { LoadLibraryA(candidate.as_ptr()) };
                    if !h.is_null() {
                        return h;
                    }
                }
            }
        }
        unsafe { LoadLibraryA(b"sound_orig.dll\0".as_ptr()) }
    }

    fn resolve_ordinal(h: *mut c_void, ord: usize) -> Option<*const c_void> {
        let p = unsafe { GetProcAddress(h, ord as *const u8) };
        if p.is_null() { None } else { Some(p) }
    }

    pub fn exports() -> Option<&'static OrigExports> {
        ORIG.get_or_init(|| {
            let h = load_orig_module();
            if h.is_null() {
                return None;
            }
            unsafe {
                Some(OrigExports {
                    create_sound: std::mem::transmute(resolve_ordinal(h, 1)?),
                    delete_sound: std::mem::transmute(resolve_ordinal(h, 2)?),
                    init_sound_timer: std::mem::transmute(resolve_ordinal(h, 3)?),
                    release_sound: std::mem::transmute(resolve_ordinal(h, 4)?),
                    wave_create: std::mem::transmute(resolve_ordinal(h, 5)?),
                    wave_delete: std::mem::transmute(resolve_ordinal(h, 6)?),
                    midi_create: std::mem::transmute(resolve_ordinal(h, 7)?),
                    midi_delete: std::mem::transmute(resolve_ordinal(h, 8)?),
                    wave_in_create: std::mem::transmute(resolve_ordinal(h, 9)?),
                    wave_in_delete: std::mem::transmute(resolve_ordinal(h, 10)?),
                    get_version: std::mem::transmute(resolve_ordinal(h, 11)?),
                })
            }
        })
        .as_ref()
    }

    pub unsafe fn create_sound(out: *mut *mut c_void, path: *const u8, ty: u32) -> u32 {
        exports().map_or(SNDERR_MISSING, |e| unsafe { (e.create_sound)(out, path, ty) })
    }
    pub unsafe fn delete_sound(obj: *mut c_void) -> u32 {
        exports().map_or(SNDERR_MISSING, |e| unsafe { (e.delete_sound)(obj) })
    }
    pub unsafe fn init_sound_timer(flags: u32, unused: u32) -> u32 {
        exports().map_or(SNDERR_MISSING, |e| unsafe { (e.init_sound_timer)(flags, unused) })
    }
    pub unsafe fn release_sound() -> u32 {
        exports().map_or(SNDERR_MISSING, |e| unsafe { (e.release_sound)() })
    }
    pub unsafe fn wave_create(out: *mut *mut c_void, flags: u32) -> u32 {
        exports().map_or(SNDERR_MISSING, |e| unsafe { (e.wave_create)(out, flags) })
    }
    pub unsafe fn wave_delete() -> u32 {
        exports().map_or(SNDERR_MISSING, |e| unsafe { (e.wave_delete)() })
    }
    pub unsafe fn midi_create(out: *mut *mut c_void, flags: u32) -> u32 {
        exports().map_or(SNDERR_MISSING, |e| unsafe { (e.midi_create)(out, flags) })
    }
    pub unsafe fn midi_delete() -> u32 {
        exports().map_or(SNDERR_MISSING, |e| unsafe { (e.midi_delete)() })
    }
    pub unsafe fn wave_in_create(out: *mut *mut c_void, flags: u32) -> u32 {
        exports().map_or(SNDERR_MISSING, |e| unsafe { (e.wave_in_create)(out, flags) })
    }
    pub unsafe fn wave_in_delete() -> u32 {
        exports().map_or(SNDERR_MISSING, |e| unsafe { (e.wave_in_delete)() })
    }
    pub unsafe fn get_version() -> u32 {
        // Returning 0 if sound_orig.dll is missing lets Civ3 cleanly disable audio.
        exports().map_or(0, |e| unsafe { (e.get_version)() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_selection_honors_explicit_override_and_native_viewer() {
        assert_eq!(select_mode(None, None), Mode::Original);
        assert_eq!(select_mode(None, Some(OsStr::new("external"))), Mode::Original);
        assert_eq!(select_mode(None, Some(OsStr::new("external-fullscreen"))), Mode::Original);
        assert_eq!(select_mode(None, Some(OsStr::new("native"))), Mode::Native);
        assert_eq!(select_mode(None, Some(OsStr::new("native-fullscreen"))), Mode::Native);
        assert_eq!(select_mode(Some(OsStr::new("original")), Some(OsStr::new("native"))), Mode::Original);
        assert_eq!(select_mode(Some(OsStr::new("native")), None), Mode::Native);
    }
}
