//! The Sonoma libproc audit-token API checks pidversion under a kernel proc
//! reference before signaling. If that API is absent, fail closed.
use std::{ffi::c_void, io};
use crate::{Signal, macos_arguments, marked};

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Identity {
    uuid: [u8; 16],
    unique: u64,
    parent_unique: u64,
    version: i32,
    reserved2: u32,
    reserved3: u64,
    reserved4: u64,
}

type SignalByAudit = unsafe extern "C" fn(*mut [u32; 8], i32) -> i32;

fn signal_api() -> io::Result<SignalByAudit> {
    // SAFETY: RTLD_DEFAULT is a valid lookup scope and this is a static,
    // NUL-terminated symbol name. libproc is loaded by the linked APIs below.
    let address = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"proc_signal_with_audittoken".as_ptr()) };
    if address.is_null() {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "identity-checked process signaling is unavailable"));
    }
    // SAFETY: Apple's libproc exports exactly this C signature. The symbol
    // remains loaded for the process lifetime; we never dlclose its library.
    Ok(unsafe { std::mem::transmute::<*mut c_void, SignalByAudit>(address) })
}

fn identity(pid: i32) -> io::Result<Identity> {
    let mut identity = Identity::default();
    // PROC_PIDUNIQIDENTIFIERINFO, XNU proc_info_private.h, stable on Sonoma.
    // SAFETY: the initialized repr(C) output matches proc_uniqidentifierinfo,
    // with the exact writable buffer length and valid process flavor.
    let size = unsafe { libc::proc_pidinfo(pid, 17, 0, (&mut identity as *mut Identity).cast(), size_of::<Identity>() as i32) };
    if size != size_of::<Identity>() as i32 { return Err(io::Error::last_os_error()); }
    Ok(identity)
}

pub(super) struct Process { pid: i32, identity: Identity, signal_api: SignalByAudit }

impl Process {
    pub fn signal(&self, signal: Signal) -> io::Result<()> {
        let mut token = [0_u32; 8];
        token[5] = self.pid as u32;
        token[7] = self.identity.version as u32;
        let signal = match signal { Signal::Terminate => libc::SIGTERM, Signal::Kill => libc::SIGKILL };
        // SAFETY: the pointer names a writable audit_token_t-sized array.
        // The kernel validates PID and pidversion atomically, then checks our
        // own signal privileges. Other token fields are unused for this API.
        let error = unsafe { (self.signal_api)(&mut token, signal) };
        if error == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(error)) }
    }

    pub fn exited(&self) -> io::Result<bool> {
        match identity(self.pid) {
            Ok(current) => Ok(current.unique != self.identity.unique || current.version != self.identity.version),
            Err(error) if matches!(error.raw_os_error(), Some(libc::ESRCH | libc::ENOENT)) => Ok(true),
            Err(error) => Err(error),
        }
    }
}

fn argument_capacity() -> io::Result<usize> {
    let mut maximum = 0_i32;
    let mut length = size_of::<i32>();
    let mut name = [libc::CTL_KERN, libc::KERN_ARGMAX];
    // SAFETY: this read-only sysctl has a two-component name and an exactly
    // sized, initialized i32 output buffer. It has no input payload.
    if unsafe { libc::sysctl(name.as_mut_ptr(), name.len() as u32, (&mut maximum as *mut i32).cast(), &mut length, std::ptr::null_mut(), 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if length != size_of::<i32>() || !(1..=2 * 1024 * 1024).contains(&maximum) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid kern.argmax capacity"));
    }
    Ok(maximum as usize)
}

fn arguments(pid: i32, bytes: &mut [u8]) -> io::Result<&[u8]> {
    // XNU rejects any non-null output capacity above ARG_MAX, even if the
    // actual environment is tiny. The caller uses exactly kern.argmax.
    let mut length = bytes.len();
    let mut name = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    // SAFETY: name and output buffers are valid for their exact lengths,
    // oldlenp names the output capacity, and there is no input payload.
    if unsafe { libc::sysctl(name.as_mut_ptr(), name.len() as u32, bytes.as_mut_ptr().cast(), &mut length, std::ptr::null_mut(), 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if length > bytes.len() { return Err(io::Error::new(io::ErrorKind::InvalidData, "process arguments exceed capacity")); }
    Ok(&bytes[..length])
}

pub(super) fn discover(key: &[u8], value: &[u8]) -> io::Result<Vec<(Process, Vec<Vec<u8>>)>> {
    let signal_api = signal_api()?;
    // SAFETY: the null buffer requests only the current process count.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count < 0 { return Err(io::Error::last_os_error()); }
    let mut pids = vec![0_i32; count as usize + 128];
    // SAFETY: pids is initialized writable i32 storage of the stated capacity.
    let count = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), (pids.len() * size_of::<i32>()) as i32) };
    if count < 0 { return Err(io::Error::last_os_error()); }
    pids.truncate((count as usize).min(pids.len()));
    let mut out = Vec::new();
    let mut argument_buffer = vec![0_u8; argument_capacity()?];
    for pid in pids.into_iter().filter(|pid| *pid > 0) {
        // Identity first. If this PID is reused before or after the later
        // marker read, the identity-checked signal still cannot hit its reuse.
        let Ok(identity) = identity(pid) else { continue };
        let Ok(arguments) = arguments(pid, &mut argument_buffer) else { continue };
        if let Some((argv, environment)) = macos_arguments(arguments) {
            if marked(environment, key, value) {
                out.push((Process { pid, identity, signal_api }, argv));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command};

    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
    }

    #[test]
    fn atomic_audit_token_rejects_stale_identity_and_preserves_its_errno_contract() {
        let mut child = ChildGuard(Command::new("/bin/sleep").arg("30").spawn().unwrap());
        let pid = child.0.id() as i32;
        let identity = identity(pid).unwrap();
        let api = signal_api().expect("this native probe requires identity-checked libproc signaling");
        let mut stale = [0_u32; 8];
        stale[5] = pid as u32;
        stale[7] = identity.version.wrapping_add(1) as u32;
        // SAFETY: this audit token deliberately mismatches only the version
        // of our own live fixture. The kernel must reject it without signaling.
        let raw_error = unsafe { api(&mut stale, libc::SIGKILL) };
        assert_eq!(raw_error, libc::ESRCH, "libproc returns positive errno, not -1 with errno");
        assert!(child.0.try_wait().unwrap().is_none(), "stale identity must not signal even our own live child");
        let process = Process { pid, identity, signal_api: api };
        process.signal(Signal::Terminate).unwrap();
        child.0.wait().unwrap();
        assert!(process.exited().unwrap());
        assert_eq!(process.signal(Signal::Kill).unwrap_err().raw_os_error(), Some(libc::ESRCH));
        eprintln!("audit-token native probe: stale identity ESRCH, live child preserved, valid TERM succeeded, exited identity ESRCH");
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    use std::{process::{Child, Command}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

    struct ChildGuard(Child);
    impl Drop for ChildGuard { fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); } }

    #[test]
    #[ignore = "unsigned native subprocess fixture for environment discovery"]
    fn discovery_fixture() {
        if std::env::var("CIV3R_PROCESS_DISCOVERY_FIXTURE").as_deref() == Ok("1") {
            loop { std::thread::park(); }
        }
    }

    #[test]
    fn marked_discovery_preserves_an_unmarked_same_prefix_client() {
        const KEY: &str = "CIV3R_PROCESS_NATIVE_TEST_MARKER";
        let token = format!("{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos());
        // SIP can omit the environment of Apple's restricted /bin/sleep.
        // Use our own unsigned native executable, like the Wine processes
        // this capability mechanism is intended to inspect.
        let executable = std::env::current_exe().unwrap();
        let args = ["--ignored", "--exact", "macos::discovery_tests::discovery_fixture", "--nocapture"];
        let mut owned = ChildGuard(Command::new(&executable).args(args).env("CIV3R_PROCESS_DISCOVERY_FIXTURE", "1").env(KEY, &token).env("WINEPREFIX", "same-native-probe-prefix").spawn().unwrap());
        let mut unrelated = ChildGuard(Command::new(&executable).args(args).env("CIV3R_PROCESS_DISCOVERY_FIXTURE", "1").env_remove(KEY).env("WINEPREFIX", "same-native-probe-prefix").spawn().unwrap());
        let found = discover(KEY.as_bytes(), token.as_bytes()).unwrap();
        assert_eq!(found.len(), 1, "only the exact capability authorizes discovery");
        assert!(found[0].1.iter().any(|arg| arg == b"macos::discovery_tests::discovery_fixture"));
        found[0].0.signal(Signal::Terminate).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !found[0].0.exited().unwrap() {
            assert!(Instant::now() < deadline, "terminated identity must be observed before its parent reaps it");
            std::thread::yield_now();
        }
        owned.0.wait().unwrap();
        assert!(unrelated.0.try_wait().unwrap().is_none(), "same-prefix unmarked client must remain alive");
        eprintln!("native discovery probe: exact marker and argv matched, TERM observed before reap, unmarked same-prefix child preserved");
    }
}
