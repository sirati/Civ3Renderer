//! One launcher per private home: the launch lock.
//!
//! `launch.lock` (in the home) is a file the launcher holds an exclusive
//! `flock` on for as long as it runs. The system lets the lock go when the
//! process ends, however it ends (a crash, a kill, AppKit's `exit` after a
//! Quit Apple event), so no lock outlives its launcher. What the file holds
//! ([`Record`]: the launcher's pid, when it started, and its game's session
//! marker once the game runs) is for diagnostics and for finding a game the
//! launcher left behind; the lock itself is the `flock`.
//!
//! Earlier launchers made `launch.lock` a directory with a `pid` file and
//! removed it on a clean exit only. Such a directory is taken over when its
//! pid is gone or is not a launcher that started before the lock was written
//! (the pid was reused); see [`legacy_verdict`].
//!
//! A launcher that ends cleanly removes the file (so it exists only while a
//! launcher runs, or after one that crashed); so does [`remove`], the
//! user's override. Another launcher may have the file open then and lock
//! the removed file, so [`claim`] checks that the file it locked is still
//! the one at the path, and opens it again if not.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The lock's name in the private home.
pub const NAME: &str = "launch.lock";

/// What a launcher writes into its lock.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Record {
    pub pid: u32,
    /// Seconds since the Unix epoch when the lock was taken.
    pub started: u64,
    /// The game's session marker (`CIV3R_SESSION_TOKEN`), once it runs.
    pub session: Option<String>,
}

impl Record {
    fn parse(text: &str) -> Option<Record> {
        let mut record = Record::default();
        for line in text.lines() {
            let line = line.trim();
            match line.split_once('=') {
                Some(("pid", value)) => record.pid = value.parse().ok()?,
                Some(("started", value)) => record.started = value.parse().ok()?,
                Some(("session", value)) if !value.is_empty() => record.session = Some(value.into()),
                Some(_) => (),
                // The legacy `pid` file: the number alone.
                None if record.pid == 0 => record.pid = line.parse().ok()?,
                None => (),
            }
        }
        (record.pid != 0).then_some(record)
    }

    fn text(&self) -> String {
        let mut text = format!("pid={}\nstarted={}\n", self.pid, self.started);
        if let Some(session) = &self.session {
            text.push_str(&format!("session={session}\n"));
        }
        text
    }

    /// "launcher 123, started 2026-10-24 08:34:22 UTC (5 min ago)".
    pub fn describe(&self, now: SystemTime) -> String {
        if self.pid == 0 {
            return "an earlier launcher".into();
        }
        if self.started == 0 {
            return format!("launcher {}", self.pid);
        }
        let ago = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()).saturating_sub(self.started);
        format!("launcher {}, started {} UTC ({} ago)", self.pid, utc(self.started), human(ago))
    }
}

fn utc(seconds: u64) -> String {
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let (days, rest) = ((seconds / 86_400) as i64, seconds % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}", rest / 3600, rest / 60 % 60, rest % 60)
}

fn human(seconds: u64) -> String {
    match seconds {
        0..120 => format!("{seconds} s"),
        120..7200 => format!("{} min", seconds / 60),
        7200..172_800 => format!("{} h", seconds / 3600),
        _ => format!("{} days", seconds / 86_400),
    }
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The held lock. Dropping it (or the process ending) lets it go.
#[derive(Debug)]
pub struct LaunchLock {
    file: File,
    path: PathBuf,
    record: Record,
}

impl LaunchLock {
    /// Notes the game's session marker, so that a later launcher can find a
    /// game this one left running.
    pub fn note_session(&mut self, marker: &str) -> io::Result<()> {
        self.record.session = Some(marker.into());
        self.write()
    }

    fn write(&mut self) -> io::Result<()> {
        self.file.set_len(0)?;
        self.file.rewind()?;
        self.file.write_all(self.record.text().as_bytes())?;
        self.file.sync_data()
    }
}

impl Drop for LaunchLock {
    fn drop(&mut self) {
        // Removed while still locked, and only if it is still ours (not
        // removed by the user and claimed by another launcher since).
        let _ = self.file.set_len(0);
        if let (Ok(ours), Ok(there)) = (self.file.metadata(), fs::symlink_metadata(&self.path)) {
            if ours.dev() == there.dev() && ours.ino() == there.ino() {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

/// Why the lock is not free.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Busy {
    /// A running process holds the `flock`: by construction, a launcher.
    Held(Option<Record>),
    /// An earlier launcher's directory lock whose pid is a launcher still
    /// running.
    Legacy(Record),
}

impl Busy {
    pub fn record(&self) -> Option<&Record> {
        match self {
            Busy::Held(record) => record.as_ref(),
            Busy::Legacy(record) => Some(record),
        }
    }
}

/// What [`claim`] found.
#[derive(Debug)]
pub enum Claim {
    /// The lock is ours. `stale` is the record of an earlier launcher that
    /// ended without letting its lock go cleanly (it is gone: taken over).
    Owned { lock: LaunchLock, stale: Option<Record> },
    Busy(Busy),
}

/// How a process looks to [`legacy_verdict`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessInfo {
    /// Its command (`ps -o comm=`): a path or a name.
    pub command: String,
    /// How long it has run.
    pub elapsed: Duration,
}

/// Whether an earlier launcher's directory lock is still its launcher's:
/// the pid is running, is one of our launchers, and started before the
/// lock was written (else the pid was reused by a later process).
pub fn legacy_verdict(process: Option<&ProcessInfo>, written: SystemTime, now: SystemTime) -> bool {
    let Some(process) = process else { return false };
    let name = process.command.rsplit('/').next().unwrap_or_default();
    let ours = name.starts_with("civ3-play") || name.starts_with("civ3-app");
    // The process started at `now - elapsed`; `ps` rounds to seconds.
    let started = now.checked_sub(process.elapsed);
    let before = started.is_none_or(|started| started <= written + Duration::from_secs(2));
    ours && before
}

/// `pid` as `ps` sees it, if it runs.
pub fn process_info(pid: u32) -> Option<ProcessInfo> {
    let alive = rustix::process::Pid::from_raw(pid as i32).is_some_and(|p| match rustix::process::test_kill_process(p) {
        Ok(()) => true,
        Err(e) => e == rustix::io::Errno::PERM,
    });
    if !alive {
        return None;
    }
    let output = Command::new("ps").args(["-o", "etime=", "-o", "comm=", "-p", &pid.to_string()]).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().find(|l| !l.trim().is_empty())?.trim();
    let (etime, command) = line.split_once(char::is_whitespace)?;
    Some(ProcessInfo { command: command.trim().into(), elapsed: elapsed(etime)? })
}

/// `ps`'s `etime`: `[[dd-]hh:]mm:ss`.
fn elapsed(etime: &str) -> Option<Duration> {
    let (days, clock) = match etime.split_once('-') {
        Some((d, rest)) => (d.parse::<u64>().ok()?, rest),
        None => (0, etime),
    };
    let parts: Vec<u64> = clock.split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let seconds = match parts[..] {
        [m, s] => m * 60 + s,
        [h, m, s] => h * 3600 + m * 60 + s,
        _ => return None,
    };
    Some(Duration::from_secs(days * 86_400 + seconds))
}

/// Takes the lock in `home` if no running launcher holds it. A lock left by
/// a launcher that is gone is taken over (and reported in `stale`).
pub fn claim(home: &Path) -> io::Result<Claim> {
    claim_with(home, process_info)
}

fn claim_with(home: &Path, probe: impl Fn(u32) -> Option<ProcessInfo>) -> io::Result<Claim> {
    let path = home.join(NAME);
    let mut stale = None;
    if fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir()) {
        let record = fs::read_to_string(path.join("pid")).ok().and_then(|t| Record::parse(&t));
        let written = fs::metadata(path.join("pid")).or_else(|_| fs::metadata(&path)).and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
        if let Some(record) = &record {
            if legacy_verdict(probe(record.pid).as_ref(), written, SystemTime::now()) {
                return Ok(Claim::Busy(Busy::Legacy(record.clone())));
            }
        }
        remove_legacy(&path)?;
        stale = Some(record.unwrap_or_default());
    }
    for _ in 0..8 {
        let mut file = OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).custom_flags(no_follow()).open(&path)?;
        let mut text = String::new();
        let _ = file.read_to_string(&mut text);
        let previous = Record::parse(&text);
        match file.try_lock() {
            Ok(()) => (),
            Err(fs::TryLockError::WouldBlock) => return Ok(Claim::Busy(Busy::Held(previous))),
            Err(fs::TryLockError::Error(e)) => return Err(e),
        }
        // Locked: is it still the file at the path (not one [`remove`]d)?
        let (held, now) = (file.metadata()?, fs::symlink_metadata(&path));
        if !now.is_ok_and(|now| now.dev() == held.dev() && now.ino() == held.ino()) {
            continue;
        }
        // A record left in the file: its launcher ended without clearing it.
        let stale = previous.filter(|r| r.pid != std::process::id()).or(stale);
        let mut lock = LaunchLock { file, path: path.clone(), record: Record { pid: std::process::id(), started: now_unix(), session: None } };
        lock.write()?;
        return Ok(Claim::Owned { lock, stale });
    }
    Err(io::Error::other(format!("{} keeps being replaced", path.display())))
}

fn no_follow() -> i32 {
    rustix::fs::OFlags::NOFOLLOW.bits() as i32
}

fn remove_legacy(path: &Path) -> io::Result<()> {
    match fs::remove_file(path.join("pid")) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => (),
    }
    fs::remove_dir(path)
}

/// The user's override: removes the lock in `home` whoever holds it, so that
/// the next [`claim`] takes a new one. A launcher that still runs keeps its
/// lock on the removed file and no longer keeps others out.
pub fn remove(home: &Path) -> io::Result<()> {
    let path = home.join(NAME);
    match fs::symlink_metadata(&path) {
        Ok(m) if m.is_dir() => remove_legacy(&path),
        Ok(_) => fs::remove_file(&path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Who holds the lock in `home` now: `None` if nobody (for `stop`).
pub fn holder(home: &Path) -> io::Result<Option<Record>> {
    let path = home.join(NAME);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
        Ok(m) if m.is_dir() => {
            let record = fs::read_to_string(path.join("pid")).ok().and_then(|t| Record::parse(&t));
            let written = fs::metadata(&path).and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
            Ok(record.filter(|r| legacy_verdict(process_info(r.pid).as_ref(), written, SystemTime::now())))
        }
        Ok(_) => {
            let mut file = File::open(&path)?;
            let mut text = String::new();
            let _ = file.read_to_string(&mut text);
            match file.try_lock_shared() {
                Ok(()) => Ok(None),
                Err(fs::TryLockError::WouldBlock) => Ok(Some(Record::parse(&text).unwrap_or_default())),
                Err(fs::TryLockError::Error(e)) => Err(e),
            }
        }
    }
}

/// The lock's path in `home`.
pub fn path(home: &Path) -> PathBuf {
    home.join(NAME)
}

#[cfg(test)]
mod tests;
