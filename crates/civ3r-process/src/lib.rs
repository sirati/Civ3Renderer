//! Find processes by an exact inherited environment capability and signal the
//! same process instance, even after it changes its Unix group or session.
//!
//! Acquiring identity precedes reading the marker. Signaling never falls back
//! to a bare PID, process name, command line or working directory.
#![cfg(unix)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::io;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod platform {
    use std::io;
    pub(super) struct Process;
    impl Process {
        pub fn signal(&self, _: super::Signal) -> io::Result<()> { Err(io::ErrorKind::Unsupported.into()) }
        pub fn exited(&self) -> io::Result<bool> { Err(io::ErrorKind::Unsupported.into()) }
    }
    pub(super) fn discover(_: &[u8], _: &[u8]) -> io::Result<Vec<(Process, Vec<Vec<u8>>)>> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Signal { Terminate, Kill }

/// A pinned process identity. Exiting/reusing its PID cannot redirect a signal.
pub struct Process {
    identity: platform::Process,
    arguments: Vec<Vec<u8>>,
}

impl Process {
    pub fn signal(&self, signal: Signal) -> io::Result<()> { self.identity.signal(signal) }
    pub fn exited(&self) -> io::Result<bool> { self.identity.exited() }
    /// Argument metadata is for conservative role filtering only. The exact
    /// environment capability, not these strings, established ownership.
    pub fn arguments(&self) -> &[Vec<u8>] { &self.arguments }
}

/// Only exact environment entries authorize ownership. Inaccessible or exited
/// processes are skipped. Unsupported identity-safe signaling fails closed.
pub fn carrying_environment(key: &[u8], value: &[u8]) -> io::Result<Vec<Process>> {
    if key.is_empty() || key.contains(&b'=') || key.contains(&0) || value.is_empty() || value.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid process ownership marker"));
    }
    platform::discover(key, value).map(|processes| processes.into_iter().map(|(identity, arguments)| Process { identity, arguments }).collect())
}

fn marked(environment: &[u8], key: &[u8], value: &[u8]) -> bool {
    environment.split(|byte| *byte == 0).any(|entry| {
        entry.strip_prefix(key).and_then(|suffix| suffix.strip_prefix(b"=")).is_some_and(|found| found == value)
    })
}

// KERN_PROCARGS2 includes argc, executable path and argv before the environment.
// An argv string that resembles a marker must never authorize ownership.
#[cfg(any(target_os = "macos", test))]
fn macos_arguments(arguments: &[u8]) -> Option<(Vec<Vec<u8>>, &[u8])> {
    let argc = i32::from_ne_bytes(arguments.get(..4)?.try_into().ok()?);
    if !(0..=65536).contains(&argc) { return None; }
    let mut rest = arguments.get(4..)?;
    rest = rest.get(rest.iter().position(|byte| *byte == 0)? + 1..)?;
    let first = rest.iter().position(|byte| *byte != 0).unwrap_or(rest.len());
    rest = &rest[first..];
    let mut argv = Vec::new();
    for _ in 0..argc {
        let end = rest.iter().position(|byte| *byte == 0)?;
        argv.push(rest[..end].to_vec());
        rest = rest.get(end + 1..)?;
    }
    Some((argv, rest))
}

#[cfg(test)]
fn macos_environment(arguments: &[u8]) -> Option<&[u8]> {
    macos_arguments(arguments).map(|(_, environment)| environment)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_is_an_exact_environment_entry() {
        let env = b"OTHER=token\0OWNED=token-suffix\0PREFIX_OWNED=token\0OWNED=token\0";
        assert!(marked(env, b"OWNED", b"token"));
        assert!(!marked(env, b"OWNED", b"toke"));
        assert!(!marked(b"PREFIX_OWNED=token\0", b"OWNED", b"token"));
    }

    #[test]
    fn darwin_empty_argv_entries_do_not_shift_the_environment_boundary() {
        let mut data = 3_i32.to_ne_bytes().to_vec();
        data.extend_from_slice(b"/bin/tool\0\0/bin/tool\0\0OWNED=token\0OTHER=value\0");
        let (argv, environment) = macos_arguments(&data).unwrap();
        assert_eq!(argv, [b"/bin/tool".to_vec(), vec![], b"OWNED=token".to_vec()]);
        assert!(!marked(environment, b"OWNED", b"token"));
        data.extend_from_slice(b"OWNED=token\0");
        assert!(marked(macos_environment(&data).unwrap(), b"OWNED", b"token"));
    }

    #[test]
    fn darwin_arguments_cannot_impersonate_an_environment_marker() {
        let mut data = 2_i32.to_ne_bytes().to_vec();
        data.extend_from_slice(b"/bin/tool\0\0/bin/tool\0OWNED=token\0OTHER=value\0");
        assert!(!marked(macos_environment(&data).unwrap(), b"OWNED", b"token"));
        data.extend_from_slice(b"OWNED=token\0");
        assert!(marked(macos_environment(&data).unwrap(), b"OWNED", b"token"));
        assert!(macos_environment(&[0, 0]).is_none());
    }
}

#[cfg(all(test, target_os = "linux"))]
mod escape_tests {
    use super::*;
    use std::{io::{BufRead, BufReader}, os::unix::process::CommandExt, process::{Child, Command, Stdio}, time::{Duration, SystemTime, UNIX_EPOCH}};

    const KEY: &str = "CIV3R_OWNERSHIP_TEST_TOKEN";
    struct ChildGuard(Child);
    impl Drop for ChildGuard { fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); } }
    struct MarkerGuard(String);
    impl Drop for MarkerGuard {
        fn drop(&mut self) {
            if let Ok(processes) = carrying_environment(KEY.as_bytes(), self.0.as_bytes()) {
                for process in processes { let _ = process.signal(Signal::Kill); }
            }
        }
    }

    #[test]
    #[ignore = "subprocess fixture, invoked by escaped_session_descendant_is_owned_without_touching_an_unrelated_process"]
    fn escape_fixture() {
        if std::env::var("CIV3R_TEST_ROLE").as_deref() == Ok("root") {
            Command::new(std::env::current_exe().unwrap())
                .args(["--ignored", "--exact", "escape_tests::escape_fixture", "--nocapture"])
                .env("CIV3R_TEST_ROLE", "leaf").spawn().unwrap();
        } else if std::env::var("CIV3R_TEST_ROLE").as_deref() == Ok("leaf") {
            rustix::process::setsid().unwrap();
            println!("OWNED_ESCAPE_READY={}", std::process::id());
            loop { std::thread::park(); }
        }
    }

    #[test]
    fn escaped_session_descendant_is_owned_without_touching_an_unrelated_process() {
        let token = format!("{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos());
        let _cleanup = MarkerGuard(token.clone());
        let mut unrelated = ChildGuard(Command::new("sleep").arg("30").env("WINEPREFIX", "same-prefix-fixture").env(KEY, "different-session").spawn().unwrap());
        let mut unmarked = ChildGuard(Command::new("sleep").arg("30").env("WINEPREFIX", "same-prefix-fixture").env_remove(KEY).spawn().unwrap());
        let mut root = ChildGuard(Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "escape_tests::escape_fixture", "--nocapture"])
            .env("CIV3R_TEST_ROLE", "root").env("WINEPREFIX", "same-prefix-fixture").env(KEY, &token).process_group(0).stdout(Stdio::piped()).spawn().unwrap());
        let stdout = root.0.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(pid) = line.split("OWNED_ESCAPE_READY=").nth(1).and_then(|pid| pid.trim().parse::<i32>().ok()) {
                    let _ = tx.send(pid); break;
                }
            }
        });
        let escaped = rx.recv_timeout(Duration::from_secs(5)).expect("escaped descendant did not announce readiness");
        root.0.wait().unwrap();
        assert_eq!(rustix::process::getsid(rustix::process::Pid::from_raw(escaped)).unwrap().as_raw_nonzero().get(), escaped);
        let owned = carrying_environment(KEY.as_bytes(), token.as_bytes()).unwrap();
        assert_eq!(owned.len(), 1, "only the detached grandchild remains owned after its parent exits");
        owned[0].signal(Signal::Terminate).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !owned[0].exited().unwrap() {
            assert!(std::time::Instant::now() < deadline, "owned detached process did not terminate");
            std::thread::yield_now();
        }
        assert!(unrelated.0.try_wait().unwrap().is_none(), "a different session's process must remain alive");
        assert!(unmarked.0.try_wait().unwrap().is_none(), "an unmarked client of the same prefix must remain alive");
    }
}
