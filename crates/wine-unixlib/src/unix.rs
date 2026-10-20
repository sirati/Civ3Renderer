//! The Unix side: the unixlib `.so` of a builtin DLL.

use crate::{Buffer32, STATUS_INVALID_PARAMETER, Status};

/// Exports a unixlib's call tables. Each handler is a safe
/// `fn(&mut Args) -> Status` whose `Args` is a `bytemuck::Pod` struct
/// without pointers (buffers as [`Buffer32`]), so 32- and 64-bit callers
/// share one layout and one table serves both. Panics are caught and
/// reported as `STATUS_UNSUCCESSFUL`.
///
/// ```ignore
/// wine_unixlib::unix_call_table![start, publish];
/// fn start(args: &mut StartArgs) -> wine_unixlib::Status { ... }
/// ```
#[macro_export]
macro_rules! unix_call_table {
    ($($handler:ident),* $(,)?) => {
        #[doc(hidden)]
        mod __wine_unixlib_entries {
            $(
                pub unsafe extern "C" fn $handler(args: *mut ::core::ffi::c_void) -> $crate::Status {
                    $crate::unix::dispatch(args, super::$handler)
                }
            )*
        }
        type __WineUnixEntry = unsafe extern "C" fn(*mut ::core::ffi::c_void) -> $crate::Status;
        #[unsafe(no_mangle)]
        #[used]
        pub static __wine_unix_call_funcs: [__WineUnixEntry; [$(stringify!($handler)),*].len()] =
            [$(__wine_unixlib_entries::$handler),*];
        #[unsafe(no_mangle)]
        #[used]
        pub static __wine_unix_call_wow64_funcs: [__WineUnixEntry; [$(stringify!($handler)),*].len()] =
            [$(__wine_unixlib_entries::$handler),*];
    };
}

/// Runs `handler` on the argument struct at `args` (used by
/// [`unix_call_table!`]).
#[doc(hidden)]
pub fn dispatch<T: bytemuck::Pod>(
    args: *mut core::ffi::c_void,
    handler: fn(&mut T) -> Status,
) -> Status {
    if args.is_null() || !(args as usize).is_multiple_of(align_of::<T>()) {
        return STATUS_INVALID_PARAMETER;
    }
    // Copy in and out through the guarded accessors: the struct lives in
    // the caller's memory, which we do not otherwise trust.
    let at = Buffer32 {
        addr: args as usize as u32,
        len: size_of::<T>() as u32,
    };
    if args as usize > u32::MAX as usize {
        return STATUS_INVALID_PARAMETER;
    }
    let Ok(bytes) = read(at) else {
        return STATUS_INVALID_PARAMETER;
    };
    let mut value: T = bytemuck::pod_read_unaligned(&bytes);
    let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(&mut value)))
        .unwrap_or(crate::STATUS_UNSUCCESSFUL);
    if write(at, bytemuck::bytes_of(&value)).is_err() {
        return STATUS_INVALID_PARAMETER;
    }
    status
}

/// Memory of the caller that could not be accessed.
#[derive(Debug, thiserror::Error)]
#[error("cannot access {len} bytes at {addr:#x}: {source}")]
pub struct AccessError {
    pub addr: u32,
    pub len: u32,
    pub source: std::io::Error,
}

#[cfg(target_os = "linux")]
fn iovec(addr: usize, len: usize) -> libc::iovec {
    libc::iovec {
        iov_base: addr as *mut libc::c_void,
        iov_len: len,
    }
}

/// Copies a buffer out of the (32-bit) caller's memory. Goes through the
/// kernel (`process_vm_readv` on ourselves), so a bad address is an error,
/// not a crash.
#[cfg(target_os = "linux")]
pub fn read(b: Buffer32) -> Result<Vec<u8>, AccessError> {
    let mut out = vec![0u8; b.len as usize];
    let local = iovec(out.as_mut_ptr() as usize, out.len());
    let remote = iovec(b.addr as usize, b.len as usize);
    // SAFETY: the local iovec covers `out`; the remote one is checked by
    // the kernel.
    let n = unsafe { libc::process_vm_readv(libc::getpid(), &local, 1, &remote, 1, 0) };
    if n != b.len as isize {
        return Err(AccessError {
            addr: b.addr,
            len: b.len,
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(out)
}

/// Copies `bytes` into the caller's memory at `b` (at most `b.len` bytes;
/// fails if `bytes` is longer).
#[cfg(target_os = "linux")]
pub fn write(b: Buffer32, bytes: &[u8]) -> Result<(), AccessError> {
    if bytes.len() > b.len as usize {
        return Err(AccessError {
            addr: b.addr,
            len: b.len,
            source: std::io::Error::from(std::io::ErrorKind::InvalidInput),
        });
    }
    let local = iovec(bytes.as_ptr() as usize, bytes.len());
    let remote = iovec(b.addr as usize, bytes.len());
    // SAFETY: the local iovec covers `bytes` (only read); the remote one is
    // checked by the kernel.
    let n = unsafe { libc::process_vm_writev(libc::getpid(), &local, 1, &remote, 1, 0) };
    if n != bytes.len() as isize {
        return Err(AccessError {
            addr: b.addr,
            len: b.len,
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod mach {
    pub type Task = libc::mach_port_t;
    pub const KERN_SUCCESS: i32 = 0;
    unsafe extern "C" {
        /// The process's own task port (what `mach_task_self()` returns).
        static mach_task_self_: Task;
        pub fn mach_vm_read_overwrite(
            task: Task,
            address: u64,
            size: u64,
            data: u64,
            out_size: *mut u64,
        ) -> i32;
        pub fn mach_vm_write(task: Task, address: u64, data: usize, count: u32) -> i32;
    }
    pub fn task() -> Task {
        // SAFETY: reads the process's own task port, set up by libSystem
        // before any of our code runs and never changed.
        unsafe { mach_task_self_ }
    }
}

#[cfg(target_os = "macos")]
fn mach_error(b: Buffer32, kern: i32) -> AccessError {
    AccessError {
        addr: b.addr,
        len: b.len,
        source: std::io::Error::other(format!("Mach VM error {kern}")),
    }
}

/// Copies a buffer out of the (32-bit) caller's memory. Goes through the
/// kernel (`mach_vm_read_overwrite` on our own task), so a bad address is an
/// error, not a crash.
#[cfg(target_os = "macos")]
pub fn read(b: Buffer32) -> Result<Vec<u8>, AccessError> {
    let mut out = vec![0u8; b.len as usize];
    if out.is_empty() {
        return Ok(out);
    }
    let mut copied = 0u64;
    // SAFETY: the destination covers `out`; the source range is checked by
    // the kernel.
    let kern = unsafe {
        mach::mach_vm_read_overwrite(
            mach::task(),
            b.addr.into(),
            b.len.into(),
            out.as_mut_ptr() as u64,
            &mut copied,
        )
    };
    if kern != mach::KERN_SUCCESS || copied != u64::from(b.len) {
        return Err(mach_error(b, kern));
    }
    Ok(out)
}

/// Copies `bytes` into the caller's memory at `b` (at most `b.len` bytes;
/// fails if `bytes` is longer). The kernel checks the destination
/// (`mach_vm_write` on our own task).
#[cfg(target_os = "macos")]
pub fn write(b: Buffer32, bytes: &[u8]) -> Result<(), AccessError> {
    if bytes.len() > b.len as usize {
        return Err(AccessError {
            addr: b.addr,
            len: b.len,
            source: std::io::Error::from(std::io::ErrorKind::InvalidInput),
        });
    }
    if bytes.is_empty() {
        return Ok(());
    }
    // SAFETY: the source covers `bytes` (only read); the destination range
    // is checked by the kernel.
    let kern = unsafe {
        mach::mach_vm_write(
            mach::task(),
            b.addr.into(),
            bytes.as_ptr() as usize,
            bytes.len() as u32,
        )
    };
    if kern != mach::KERN_SUCCESS {
        return Err(mach_error(b, kern));
    }
    Ok(())
}

/// Starts a native thread for long-running work. Wine does not know such
/// threads (no TEB), so they block every signal: Wine's handlers would
/// otherwise run on them with a stale TEB. They must not call back into
/// Windows code.
pub fn spawn_native<F: FnOnce() + Send + 'static>(
    name: &str,
    f: F,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            block_native_signals();
            f()
        })
}

/// Establishes the signal policy for a native thread Wine does not know.
/// Call before native work or drivers create children, so those children
/// inherit the mask. Never call on Wine's Windows or AppKit event-loop thread;
/// this thread must not call back into Windows code afterward.
pub fn block_native_signals() {
    // SAFETY: changes only the calling thread's signal mask using a valid set.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigfillset(&mut set);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
}

#[cfg(test)]
mod signal_policy_tests {
    use super::*;

    fn mask() -> libc::sigset_t {
        // SAFETY: output points to a valid signal set; null input only queries.
        unsafe {
            let mut mask = std::mem::zeroed();
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut mask),
                0
            );
            mask
        }
    }

    struct Restore(libc::sigset_t);
    impl Drop for Restore {
        fn drop(&mut self) {
            // SAFETY: restores the calling thread's previously queried mask.
            unsafe {
                assert_eq!(
                    libc::pthread_sigmask(libc::SIG_SETMASK, &self.0, std::ptr::null_mut()),
                    0
                );
            }
        }
    }

    fn blocked(mask: &libc::sigset_t, signal: libc::c_int) -> libc::c_int {
        // SAFETY: mask is initialized; sigismember reports invalid numbers.
        unsafe { libc::sigismember(mask, signal) }
    }

    #[test]
    fn native_policy_blocks_signals_is_inherited_and_can_be_restored() {
        std::thread::spawn(|| {
            let original = mask();
            let restore = Restore(original);
            block_native_signals();
            let assert_policy = || {
                let current = mask();
                for signal in [
                    libc::SIGINT,
                    libc::SIGUSR1,
                    libc::SIGUSR2,
                    libc::SIGCHLD,
                    libc::SIGALRM,
                    libc::SIGPIPE,
                    libc::SIGTERM,
                    libc::SIGSEGV,
                ] {
                    assert_eq!(blocked(&current, signal), 1, "signal {signal}");
                }
                for signal in [libc::SIGKILL, libc::SIGSTOP] {
                    assert_eq!(blocked(&current, signal), 0);
                }
            };
            assert_policy();
            std::thread::spawn(assert_policy).join().unwrap();
            drop(restore);
            let restored = mask();
            for signal in 1..128 {
                let expected = blocked(&original, signal);
                if expected >= 0 {
                    assert_eq!(blocked(&restored, signal), expected, "signal {signal}");
                }
            }
        })
        .join()
        .unwrap();
    }
}
