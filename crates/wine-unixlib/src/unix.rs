//! The Unix side: the unixlib `.so` of a builtin DLL. On a Windows host
//! (Wine's WoW64 ported to x64 Windows) the same unixlib is a native x64
//! Windows DLL, `x86_64-native/NAME.dll`; Wine loads and calls it the same
//! way.

use crate::{Buffer32, STATUS_INVALID_PARAMETER, Status};

/// Exports a unixlib's call tables. Each handler is a safe
/// `fn(&mut Args) -> Status` whose `Args` is a `bytemuck::Pod` struct
/// without pointers (buffers as [`Buffer32`]), so 32- and 64-bit callers
/// share one layout and one table serves both. Panics are caught and
/// reported as `STATUS_UNSUCCESSFUL`; the end of a thread (by Wine or a
/// cancellation) waits for the handler to return, see [`dispatch`].
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
                // "C-unwind": a thread ended at the end of a call (see
                // `dispatch`) unwinds through here into Wine's frames.
                pub unsafe extern "C-unwind" fn $handler(args: *mut ::core::ffi::c_void) -> $crate::Status {
                    $crate::unix::dispatch(args, super::$handler)
                }
            )*
        }
        type __WineUnixEntry = unsafe extern "C-unwind" fn(*mut ::core::ffi::c_void) -> $crate::Status;
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
///
/// A panic of `handler` is caught here. The end of the thread is not: Wine
/// ends a thread (another thread's `ExitProcess`, `TerminateThread`) with
/// `SIGQUIT`, whose handler calls `pthread_exit`, and that, like
/// `pthread_cancel`, is a forced unwind. Caught by `catch_unwind` it aborts
/// the process (glibc: "FATAL: exception not rethrown"); it has to reach
/// the thread's start. So while `handler` runs, `SIGQUIT` is blocked and
/// cancellation disabled; both take effect once it has returned and the
/// arguments are written back, from this frame, outside the catch, and
/// the unwind passes through the entry point (`extern "C-unwind"`) into
/// Wine's frames. A thread that never returns from `handler` ends with
/// the process.
///
/// On Windows a thread ends without unwinding (`TerminateThread`,
/// `ExitProcess`): nothing to hold back.
#[doc(hidden)]
pub fn dispatch<T: bytemuck::Pod>(
    args: *mut core::ffi::c_void,
    handler: fn(&mut T) -> Status,
) -> Status {
    #[cfg(unix)]
    let held = ThreadEnd::hold();
    let status = dispatch_caught(args, handler);
    #[cfg(unix)]
    held.release();
    status
}

/// The ways a thread is ended from outside, held back for the length of a
/// call: Wine's `SIGQUIT` (blocked) and, on Linux, `pthread_cancel`
/// (disabled). [`ThreadEnd::release`] puts both back as they were; one
/// pending acts then.
#[cfg(unix)]
struct ThreadEnd {
    mask: libc::sigset_t,
    #[cfg(target_os = "linux")]
    cancel: libc::c_int,
}

/// The calls where the thread may end, declared as unwinding: a forced
/// unwind starts in them (in the `SIGQUIT` handler run on return from
/// `pthread_sigmask`, or at the cancellation point).
#[cfg(unix)]
mod thread_end {
    unsafe extern "C-unwind" {
        pub fn pthread_sigmask(
            how: libc::c_int,
            set: *const libc::sigset_t,
            old: *mut libc::sigset_t,
        ) -> libc::c_int;
    }
    /// glibc's cancellation state (not in the `libc` crate for Linux).
    #[cfg(target_os = "linux")]
    pub const CANCEL_DISABLE: libc::c_int = 1;
    #[cfg(target_os = "linux")]
    unsafe extern "C-unwind" {
        pub fn pthread_setcancelstate(state: libc::c_int, old: *mut libc::c_int) -> libc::c_int;
        /// A cancellation point: unwinds when one is pending.
        pub fn pthread_testcancel();
    }
}

#[cfg(unix)]
impl ThreadEnd {
    fn hold() -> ThreadEnd {
        // SAFETY: calls on this thread's own signal mask and cancellation
        // state, with pointers to locals.
        unsafe {
            let mut quit: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut quit);
            libc::sigaddset(&mut quit, libc::SIGQUIT);
            let mut mask: libc::sigset_t = std::mem::zeroed();
            thread_end::pthread_sigmask(libc::SIG_BLOCK, &quit, &mut mask);
            #[cfg(target_os = "linux")]
            let cancel = {
                let mut old = 0;
                thread_end::pthread_setcancelstate(thread_end::CANCEL_DISABLE, &mut old);
                old
            };
            ThreadEnd {
                mask,
                #[cfg(target_os = "linux")]
                cancel,
            }
        }
    }

    /// Puts back what [`ThreadEnd::hold`] found. Not a `Drop`: it is the
    /// point where the thread may end, which must not happen while it
    /// unwinds.
    fn release(self) {
        // SAFETY: as in `hold`. A pending `SIGQUIT` or cancellation ends
        // the thread in these calls, unwinding from here: no catch on the
        // way out.
        unsafe {
            #[cfg(target_os = "linux")]
            thread_end::pthread_setcancelstate(self.cancel, std::ptr::null_mut());
            thread_end::pthread_sigmask(libc::SIG_SETMASK, &self.mask, std::ptr::null_mut());
            #[cfg(target_os = "linux")]
            thread_end::pthread_testcancel();
        }
    }
}

fn dispatch_caught<T: bytemuck::Pod>(
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

/// The current process's memory through `ReadProcessMemory` and
/// `WriteProcessMemory`, which fail on a bad address rather than fault.
#[cfg(windows)]
mod process_memory {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn ReadProcessMemory(process: isize, base: *const c_void, buffer: *mut c_void, size: usize, read: *mut usize) -> i32;
        pub fn WriteProcessMemory(process: isize, base: *mut c_void, buffer: *const c_void, size: usize, written: *mut usize) -> i32;
    }
    /// `GetCurrentProcess()`.
    pub const CURRENT: isize = -1;
}

/// Copies a buffer out of the (32-bit) caller's memory. A bad address is an
/// error, not a crash.
#[cfg(windows)]
pub fn read(b: Buffer32) -> Result<Vec<u8>, AccessError> {
    let mut out = vec![0u8; b.len as usize];
    if out.is_empty() {
        return Ok(out);
    }
    let mut n = 0;
    // SAFETY: the destination covers `out`; the source is checked by the
    // system.
    let ok = unsafe {
        process_memory::ReadProcessMemory(process_memory::CURRENT, b.addr as usize as *const _, out.as_mut_ptr().cast(), out.len(), &mut n)
    };
    if ok == 0 || n != out.len() {
        return Err(AccessError { addr: b.addr, len: b.len, source: std::io::Error::last_os_error() });
    }
    Ok(out)
}

/// Copies `bytes` into the caller's memory at `b` (at most `b.len` bytes;
/// fails if `bytes` is longer).
#[cfg(windows)]
pub fn write(b: Buffer32, bytes: &[u8]) -> Result<(), AccessError> {
    if bytes.len() > b.len as usize {
        return Err(AccessError { addr: b.addr, len: b.len, source: std::io::Error::from(std::io::ErrorKind::InvalidInput) });
    }
    if bytes.is_empty() {
        return Ok(());
    }
    let mut n = 0;
    // SAFETY: the source covers `bytes` (only read); the destination is
    // checked by the system.
    let ok = unsafe {
        process_memory::WriteProcessMemory(process_memory::CURRENT, b.addr as usize as *mut _, bytes.as_ptr().cast(), bytes.len(), &mut n)
    };
    if ok == 0 || n != bytes.len() {
        return Err(AccessError { addr: b.addr, len: b.len, source: std::io::Error::last_os_error() });
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
    // (Windows has no signals to keep away from Wine.)
    #[cfg(unix)]
    // SAFETY: changes only the calling thread's signal mask using a valid set.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigfillset(&mut set);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
}

#[cfg(all(test, unix))]
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

/// A thread ended while inside a call (by Wine's `SIGQUIT`, or cancelled)
/// ends once the call has returned, without passing the call's
/// `catch_unwind`; a handler's panic is still caught. Each ending runs in
/// a child process of this test binary: done wrong, it aborts the process.
#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod thread_end_tests {
    use super::*;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    const CHILD: &str = "WINE_UNIXLIB_THREAD_END_CHILD";

    #[repr(C)]
    #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
    struct Args {
        value: u32,
    }

    crate::unix_call_table![slow, panics];

    static INSIDE: AtomicBool = AtomicBool::new(false);
    static RETURNED: AtomicBool = AtomicBool::new(false);

    /// Sits in cancellation points (`nanosleep`) for 300 ms, then answers.
    fn slow(args: &mut Args) -> Status {
        INSIDE.store(true, Ordering::SeqCst);
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(10));
        }
        args.value = 7;
        RETURNED.store(true, Ordering::SeqCst);
        crate::STATUS_SUCCESS
    }

    fn panics(_: &mut Args) -> Status {
        panic!("a handler's panic (expected by the test)");
    }

    unsafe extern "C-unwind" {
        /// Declared unwinding: it unwinds (libc's declaration says it does not).
        fn pthread_exit(value: *mut c_void) -> !;
    }

    /// An argument struct where a 32-bit caller's would be (below 4 GiB).
    fn low_args() -> *mut Args {
        // SAFETY: a fresh anonymous mapping, owned by the test (never unmapped).
        let p = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_32BIT,
                -1,
                0,
            )
        };
        assert_ne!(p, libc::MAP_FAILED);
        assert!((p as usize) < u32::MAX as usize);
        p.cast()
    }

    /// A thread as Wine has them: no Rust thread runtime around the call.
    extern "C-unwind" fn call_slow(args: *mut c_void) -> *mut c_void {
        // SAFETY: the entry point of the table above, on a low mapping.
        unsafe { __wine_unixlib_entries::slow(args) };
        std::ptr::null_mut()
    }

    fn spawn(args: *mut Args) -> libc::pthread_t {
        type Start = extern "C" fn(*mut c_void) -> *mut c_void;
        // SAFETY: the same ABI; "C-unwind" only lets the thread's end
        // unwind through it into glibc's thread start, which expects that.
        let start: Start = unsafe {
            std::mem::transmute::<extern "C-unwind" fn(*mut c_void) -> *mut c_void, Start>(call_slow)
        };
        let mut t: libc::pthread_t = 0;
        // SAFETY: valid out pointer, default attributes.
        let created = unsafe { libc::pthread_create(&mut t, std::ptr::null(), start, args.cast()) };
        assert_eq!(created, 0);
        while !INSIDE.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }
        t
    }

    fn join(t: libc::pthread_t) -> *mut c_void {
        let mut ret = std::ptr::null_mut();
        // SAFETY: a joinable thread of `spawn`, joined once.
        assert_eq!(unsafe { libc::pthread_join(t, &mut ret) }, 0);
        ret
    }

    fn in_child(test: &str) {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([test, "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{test}: {:?}\n{stdout}\n{stderr}", out.status);
        assert!(stdout.contains("1 passed"), "{test} did not run:\n{stdout}");
        assert!(!stderr.contains("FATAL"), "{test}:\n{stderr}");
    }

    fn child() -> bool {
        std::env::var_os(CHILD).is_some()
    }

    #[test]
    fn a_thread_cancelled_in_a_call_ends_when_it_returns() {
        in_child("unix::thread_end_tests::cancelled_child");
    }

    #[test]
    fn a_thread_wine_ends_in_a_call_ends_when_it_returns() {
        in_child("unix::thread_end_tests::sigquit_child");
    }

    #[test]
    fn cancelled_child() {
        if !child() {
            return;
        }
        let args = low_args();
        let t = spawn(args);
        // SAFETY: a live thread of `spawn`.
        assert_eq!(unsafe { libc::pthread_cancel(t) }, 0);
        // glibc's PTHREAD_CANCELED.
        assert_eq!(join(t) as isize, -1);
        assert!(RETURNED.load(Ordering::SeqCst));
        // SAFETY: the mapping of `low_args`, the thread is gone.
        assert_eq!(unsafe { (*args).value }, 7);
    }

    const QUIT: usize = 0x5157;

    /// What Wine's `SIGQUIT` handler does in the end (`abort_thread`).
    extern "C-unwind" fn quit(_: libc::c_int) {
        // SAFETY: ends this thread, unwinding its frames.
        unsafe { pthread_exit(QUIT as *mut c_void) }
    }

    #[test]
    fn sigquit_child() {
        if !child() {
            return;
        }
        // SAFETY: a handler for this child process only.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = quit as extern "C-unwind" fn(libc::c_int) as usize;
            libc::sigemptyset(&mut action.sa_mask);
            assert_eq!(libc::sigaction(libc::SIGQUIT, &action, std::ptr::null_mut()), 0);
        }
        let args = low_args();
        let t = spawn(args);
        // SAFETY: a live thread of `spawn`.
        assert_eq!(unsafe { libc::pthread_kill(t, libc::SIGQUIT) }, 0);
        assert_eq!(join(t) as usize, QUIT);
        assert!(RETURNED.load(Ordering::SeqCst));
        // SAFETY: as in `cancelled_child`.
        assert_eq!(unsafe { (*args).value }, 7);
    }

    #[test]
    fn a_panic_is_caught_and_the_thread_left_as_it_was() {
        fn state() -> (libc::sigset_t, libc::c_int) {
            // SAFETY: queries only (a null set; the state set and put back).
            unsafe {
                let mut mask: libc::sigset_t = std::mem::zeroed();
                libc::pthread_sigmask(libc::SIG_BLOCK, std::ptr::null(), &mut mask);
                let mut cancel = 0;
                thread_end::pthread_setcancelstate(0, &mut cancel);
                thread_end::pthread_setcancelstate(cancel, std::ptr::null_mut());
                (mask, cancel)
            }
        }
        let before = state();
        let args = low_args();
        // SAFETY: the table's entry point on a low mapping.
        let status = unsafe { __wine_unixlib_entries::panics(args.cast()) };
        assert_eq!(status, crate::STATUS_UNSUCCESSFUL);
        let after = state();
        // SAFETY: plain reads of two initialized signal sets.
        let quit = |m: &libc::sigset_t| unsafe { libc::sigismember(m, libc::SIGQUIT) };
        assert_eq!(quit(&after.0), quit(&before.0));
        assert_eq!(after.1, before.1);
    }
}
