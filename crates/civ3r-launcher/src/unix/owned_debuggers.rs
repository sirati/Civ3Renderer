//! Additional cleanup for Wine's automatic debugger, which double-forks and
//! creates a fresh session outside the original ManagedChild process group.
//! Exact launch capability and atomic process identity are both mandatory.

use super::{Command, Result, random_hex, STOP_TIMEOUT, POLL_INTERVAL, Instant, thread};
use civ3r_process::Process;

const MARKER: &str = "CIV3R_SESSION_TOKEN";

pub(super) struct OwnedDebuggers(String);

impl OwnedDebuggers {
    pub fn new(command: &mut Command) -> Result<Self> {
        let marker = random_hex()?;
        command.env(MARKER, &marker);
        Ok(Self(marker))
    }

    pub fn processes(&self) -> std::io::Result<Vec<Process>> {
        Ok(civ3r_process::carrying_environment(MARKER.as_bytes(), self.0.as_bytes())?
            .into_iter().filter(|process| automatic_debugger(process.arguments())).collect())
    }

    pub fn stop(&self) {
        let processes = match self.processes() {
            Ok(processes) => processes,
            Err(error) => {
                eprintln!("civ3-play-macos: cannot inspect owned automatic debuggers: {error}; original process-group cleanup was preserved");
                return;
            }
        };
        signal(&processes, civ3r_process::Signal::Terminate);
        let deadline = Instant::now() + STOP_TIMEOUT;
        while processes.iter().any(|process| !process.exited().unwrap_or(false)) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() { break; }
            // Wait only while an owned debugger is still alive, bounded by
            // the same termination deadline as the original group guard.
            thread::sleep(POLL_INTERVAL.min(remaining));
        }
        // The original group is dead. Refresh once for a debugger that was
        // spawned while that group was stopping, then kill only pinned owned
        // identities. An unrelated reused PID cannot receive these signals.
        signal(&processes, civ3r_process::Signal::Kill);
        match self.processes() {
            Ok(latest) => signal(&latest, civ3r_process::Signal::Kill),
            Err(error) => eprintln!("civ3-play-macos: cannot refresh owned automatic debuggers: {error}"),
        }
    }
}

fn signal(processes: &[Process], signal: civ3r_process::Signal) {
    for process in processes {
        if let Err(error) = process.signal(signal) {
            if error.raw_os_error() != Some(rustix::io::Errno::SRCH.raw_os_error()) {
                eprintln!("civ3-play-macos: cannot signal an owned automatic debugger: {error}");
            }
        }
    }
}

/// This is only a conservative role exclusion AFTER capability ownership.
/// Shared wineserver/services are never selected, even if they inherited the
/// marker while the prefix booted. An unmarked debugger is never ours.
fn automatic_debugger(arguments: &[Vec<u8>]) -> bool {
    // Wine's native argument block includes a trailing empty argv entry.
    let end = arguments.iter().rposition(|argument| !argument.is_empty()).map_or(0, |last| last + 1);
    let mut arguments = &arguments[..end];
    // ntdll exec_wineloader reserves argv slots for the native preloader and
    // loader before the Windows command. Match this early loading phase too.
    for _ in 0..2 {
        if arguments.first().is_some_and(|program| wine_loader(program)) { arguments = &arguments[1..]; }
        else { break; }
    }
    let [program, auto, pid, event] = arguments else { return false };
    let name = program.rsplit(|byte| matches!(*byte, b'/' | b'\\')).next().unwrap_or_default();
    (name.eq_ignore_ascii_case(b"winedbg") || name.eq_ignore_ascii_case(b"winedbg.exe"))
        && auto == b"--auto"
        && decimal(pid).is_some_and(|pid| pid != 0)
        && decimal(event).is_some()
}

fn wine_loader(program: &[u8]) -> bool {
    let name = program.rsplit(|byte| *byte == b'/').next().unwrap_or_default();
    [b"wine".as_slice(), b"wine64", b"wine-preloader", b"wine64-preloader"].contains(&name)
}

fn decimal(value: &[u8]) -> Option<u32> {
    if value.is_empty() || !value.iter().all(u8::is_ascii_digit) { return None; }
    std::str::from_utf8(value).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn argv(arguments: &[&str]) -> Vec<Vec<u8>> { arguments.iter().map(|argument| argument.as_bytes().to_vec()).collect() }

    #[test]
    fn only_the_automatic_debugger_role_is_eligible_after_ownership() {
        assert!(automatic_debugger(&argv(&["winedbg", "--auto", "272", "528", ""])));
        assert!(automatic_debugger(&argv(&["/opt/wine/bin/wine64-preloader", "/opt/wine/bin/wine64", "winedbg", "--auto", "272", "528"])));
        assert!(automatic_debugger(&argv(&["/opt/wine/bin/wine", "winedbg", "--auto", "272", "528"])));
        assert!(automatic_debugger(&argv(&[r"C:\windows\system32\WINEDBG.EXE", "--auto", "272", "528"])));
        for arguments in [
            vec!["wineserver", "--auto", "272", "528"],
            vec!["services.exe", "--auto", "272", "528"],
            vec!["winedbg", "--gdb", "272", "528"],
            vec!["winedbg", "--auto", "not-a-pid", "528"],
            vec!["winedbg", "--auto", "272", "528", "unrecognized"],
            vec!["winedbg", "--auto", "0", "528"],
        ] { assert!(!automatic_debugger(&argv(&arguments)), "{arguments:?}"); }
    }
}
