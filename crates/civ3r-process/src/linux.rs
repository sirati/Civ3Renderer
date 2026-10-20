use std::{fs, io};
use rustix::{fd::OwnedFd, process::{Pid, PidfdFlags}};
use crate::{Signal, marked};

pub(super) struct Process { fd: OwnedFd }

impl Process {
    fn open(pid: i32) -> io::Result<Self> {
        let pid = Pid::from_raw(pid).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid pid"))?;
        Ok(Self { fd: rustix::process::pidfd_open(pid, PidfdFlags::empty())? })
    }

    pub fn signal(&self, signal: Signal) -> io::Result<()> {
        let signal = match signal { Signal::Terminate => rustix::process::Signal::TERM, Signal::Kill => rustix::process::Signal::KILL };
        rustix::process::pidfd_send_signal(&self.fd, signal).map_err(Into::into)
    }

    pub fn exited(&self) -> io::Result<bool> {
        let mut fds = [rustix::event::PollFd::new(&self.fd, rustix::event::PollFlags::IN)];
        Ok(rustix::event::poll(&mut fds, Some(&rustix::event::Timespec::default()))? != 0)
    }
}

pub(super) fn discover(key: &[u8], value: &[u8]) -> io::Result<Vec<(Process, Vec<Vec<u8>>)>> {
    let mut out = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let Ok(entry) = entry else { continue };
        let Some(pid) = entry.file_name().to_str().and_then(|pid| pid.parse().ok()) else { continue };
        // Pin identity first. Any later /proc PID reuse cannot redirect a signal.
        let process = match Process::open(pid) {
            Ok(process) => process,
            Err(error) if error.raw_os_error() == Some(rustix::io::Errno::NOSYS.raw_os_error()) => return Err(error),
            Err(_) => continue,
        };
        if fs::read(entry.path().join("environ")).is_ok_and(|env| marked(&env, key, value)) {
            if let Ok(arguments) = fs::read(entry.path().join("cmdline")) {
                out.push((process, arguments.strip_suffix(&[0]).unwrap_or(&arguments).split(|byte| *byte == 0).map(<[u8]>::to_vec).collect()));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn exited_identity_cannot_signal_a_later_process() {
        let mut first = Command::new("sleep").arg("30").spawn().unwrap();
        let pinned = Process::open(first.id() as i32).unwrap();
        first.kill().unwrap(); first.wait().unwrap();
        let mut unrelated = Command::new("sleep").arg("30").spawn().unwrap();
        assert!(pinned.exited().unwrap());
        assert_eq!(pinned.signal(Signal::Kill).unwrap_err().raw_os_error(), Some(rustix::io::Errno::SRCH.raw_os_error()));
        assert!(unrelated.try_wait().unwrap().is_none());
        unrelated.kill().unwrap(); unrelated.wait().unwrap();
    }
}
