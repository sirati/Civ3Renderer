//! Lock-free `civ3r_native.dll` client for dispatching `civ3_audio::wire` calls
//! from the 32-bit `sound.dll` shim to the 64-bit native audio thread.

/// Converts a Windows-1252 byte slice (Civ3 ANSI paths) to UTF-8.
pub fn cp1252_to_utf8(bytes: &[u8]) -> String {
    const HIGH_80_9F: [char; 32] = [
        '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}',
        '\u{017D}', '\u{008F}', '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
        '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
    ];
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if b < 0x80 {
            out.push(b as char);
        } else if b < 0xA0 {
            out.push(HIGH_80_9F[(b - 0x80) as usize]);
        } else {
            out.push(b as char);
        }
    }
    out
}

/// Reads a NUL-terminated ANSI C string safely (bounded at 4096 bytes).
#[cfg(all(windows, target_arch = "x86"))]
pub unsafe fn read_cstr(ptr: *const u8) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let mut len = 0usize;
    while len < 4096 {
        let b = unsafe { *ptr.add(len) };
        if b == 0 {
            break;
        }
        len += 1;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    Some(cp1252_to_utf8(slice))
}

#[cfg(all(windows, target_arch = "x86"))]
mod pe_client {
    use super::cp1252_to_utf8;
    use civ3_audio::wire::{encode_request, Op, RequestHeader, Response};
    use std::sync::OnceLock;
    use wine_unixlib::pe::UnixLib;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentDirectoryA(buf_len: u32, buf: *mut u8) -> u32;
    }

    /// Resolves relative Windows paths against the Win32 current directory
    /// (`GetCurrentDirectoryA`) before crossing the Wine unixlib boundary,
    /// because `SetCurrentDirectoryA` in Wine does not update the host Unix
    /// process's `chdir`.
    fn qualify_win32_path(path: &str) -> String {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        let bytes = trimmed.as_bytes();
        let is_abs = (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
            || bytes.starts_with(b"\\\\")
            || bytes.starts_with(b"/");
        if is_abs {
            return trimmed.to_owned();
        }
        let mut cwd_buf = [0u8; 520];
        let n = unsafe { GetCurrentDirectoryA((cwd_buf.len() - 1) as u32, cwd_buf.as_mut_ptr()) }
            as usize;
        if n == 0 || n >= cwd_buf.len() {
            return trimmed.to_owned();
        }
        let cwd = cp1252_to_utf8(&cwd_buf[..n]);
        if cwd.ends_with('\\') || cwd.ends_with('/') {
            format!("{cwd}{trimmed}")
        } else {
            format!("{cwd}\\{trimmed}")
        }
    }

    static NATIVE: OnceLock<Option<UnixLib>> = OnceLock::new();

    fn unixlib() -> Option<&'static UnixLib> {
        NATIVE
            .get_or_init(|| {
                let lib = UnixLib::load("civ3r_native.dll").ok()?;
                let mut limits = civ3r_native::Limits::default();
                if lib.call(civ3r_native::CALL_LIMITS, &mut limits) != 0 {
                    return None;
                }
                if (limits._pad & civ3r_native::LIMIT_AUDIO) == 0 {
                    return None;
                }
                Some(lib)
            })
            .as_ref()
    }

    pub fn is_available() -> bool {
        unixlib().is_some()
    }

    pub fn call(hdr: RequestHeader, path: &str) -> Result<Response, u32> {
        let Some(lib) = unixlib() else {
            return Err(3); // SNDERR_DEVICE_ERROR
        };
        let qualified;
        let send_path = if !path.is_empty()
            && matches!(
                Op::from_u32(hdr.op),
                Some(Op::Load | Op::PlaylistQueueTrack | Op::WaveStartRecord)
            ) {
            qualified = qualify_win32_path(path);
            qualified.as_str()
        } else {
            path
        };
        let mut req_bytes =
            Vec::with_capacity(std::mem::size_of::<RequestHeader>() + send_path.len());
        encode_request(hdr, (!send_path.is_empty()).then_some(send_path), &mut req_bytes);
        let mut resp_bytes = [0u8; std::mem::size_of::<Response>()];
        let mut args = civ3r_native::AudioCall {
            req: wine_unixlib::pe::buffer(&req_bytes),
            resp: wine_unixlib::pe::buffer_mut(&mut resp_bytes),
            resp_len: 0,
            _pad: 0,
        };
        let status = lib.call(civ3r_native::CALL_AUDIO, &mut args);
        if status != 0 || (args.resp_len as usize) < std::mem::size_of::<Response>() {
            return Err(3);
        }
        Ok(*bytemuck::from_bytes::<Response>(&resp_bytes))
    }
}

#[cfg(all(windows, target_arch = "x86"))]
pub use pe_client::{call, is_available};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cp1252_preserves_ascii_and_accents() {
        assert_eq!(cp1252_to_utf8(b"Sounds\\Button.wav"), "Sounds\\Button.wav");
        // Windows-1252: 0xE9 = 'é', 0xFC = 'ü', 0x80 = '€'
        assert_eq!(cp1252_to_utf8(&[0x63, 0x61, 0x66, 0xE9, 0x20, 0xFC, 0x80]), "café ü€");
    }
}
