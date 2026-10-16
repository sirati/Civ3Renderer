//! The PE side: calling a builtin DLL's unixlib from 32-bit Windows code.

use std::ffi::c_void;

use crate::{Buffer32, STATUS_DLL_NOT_FOUND, Status};

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryVirtualMemory(process: isize, addr: *const c_void, class: u32, buf: *mut c_void, len: usize, ret: *mut usize) -> Status;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryA(name: *const u8) -> *mut c_void;
    fn GetModuleHandleA(name: *const u8) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
}

/// `MemoryWineUnixFuncs`: the query that yields a module's unixlib handle
/// (what winecrt0's `__wine_init_unix_call` does).
const MEMORY_WINE_UNIX_FUNCS: u32 = 1000;

type Dispatcher = unsafe extern "system" fn(u64, u32, *mut c_void) -> Status;

/// The unixlib of a loaded builtin DLL.
#[derive(Copy, Clone, Debug)]
pub struct UnixLib {
    handle: u64,
    dispatcher: usize,
}

impl UnixLib {
    /// Loads the builtin DLL `name` and connects to its unixlib.
    pub fn load(name: &str) -> Result<UnixLib, Status> {
        let c = std::ffi::CString::new(name).map_err(|_| STATUS_DLL_NOT_FOUND)?;
        // SAFETY: NUL-terminated name; NULL on failure.
        let module = unsafe { LoadLibraryA(c.as_ptr() as *const u8) };
        if module.is_null() {
            return Err(STATUS_DLL_NOT_FOUND);
        }
        let mut handle = 0u64;
        // SAFETY: writes 8 bytes into `handle`; Wine fails cleanly for
        // modules without a unixlib.
        let st = unsafe {
            NtQueryVirtualMemory(-1, module, MEMORY_WINE_UNIX_FUNCS, &mut handle as *mut u64 as *mut c_void, 8, std::ptr::null_mut())
        };
        if st != 0 {
            return Err(st);
        }
        // ntdll exports `__wine_unix_call_dispatcher` as a variable holding
        // the dispatcher's address.
        // SAFETY: plain lookups; the variable is read only if it exists.
        let dispatcher = unsafe {
            let ntdll = GetModuleHandleA(c"ntdll.dll".as_ptr() as *const u8);
            let var = GetProcAddress(ntdll, c"__wine_unix_call_dispatcher".as_ptr() as *const u8) as *const usize;
            if var.is_null() {
                return Err(STATUS_DLL_NOT_FOUND);
            }
            *var
        };
        Ok(UnixLib { handle, dispatcher })
    }

    /// Calls function `code` of the unixlib with `args`.
    ///
    /// `T` must be the plain-data argument struct function `code` expects
    /// (fields of fixed size, `u64` first, buffers as [`Buffer32`]): a
    /// mismatch is a protocol bug the Unix side has to tolerate, not memory
    /// unsafety here, since the Unix side only reads `size_of::<T>()`
    /// bytes it was promised.
    pub fn call<T: bytemuck::Pod>(&self, code: u32, args: &mut T) -> Status {
        // SAFETY: `dispatcher` is Wine's unix call dispatcher, whose ABI is
        // (UINT64 handle, UINT code, void *args).
        let f: Dispatcher = unsafe { std::mem::transmute::<usize, Dispatcher>(self.dispatcher) };
        unsafe { f(self.handle, code, args as *mut T as *mut c_void) }
    }
}

/// A [`Buffer32`] the Unix side reads (valid while `bytes` is borrowed).
pub fn buffer(bytes: &[u8]) -> Buffer32 {
    Buffer32 { addr: bytes.as_ptr() as usize as u32, len: bytes.len() as u32 }
}

/// A [`Buffer32`] the Unix side may write (valid while `bytes` is
/// borrowed mutably).
pub fn buffer_mut(bytes: &mut [u8]) -> Buffer32 {
    Buffer32 { addr: bytes.as_mut_ptr() as usize as u32, len: bytes.len() as u32 }
}
