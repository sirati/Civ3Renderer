use super::*;
use std::process::{Child, Stdio};

/// A private home for one test, removed at the end.
struct Home(PathBuf);
impl Home {
    fn new(name: &str) -> Home {
        let path = std::env::temp_dir().join(format!("civ3r-launch-lock-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Home(path)
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A process that is no launcher, running for the test.
fn unrelated() -> Running {
    Running(Command::new("sleep").arg("30").stdin(Stdio::null()).spawn().unwrap())
}

/// A pid that ran and is gone (reaped).
fn dead_pid() -> u32 {
    let mut child = Command::new("true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

fn owned(claim: Claim) -> (LaunchLock, Option<Record>) {
    match claim {
        Claim::Owned { lock, stale } => (lock, stale),
        Claim::Busy(busy) => panic!("busy: {busy:?}"),
    }
}

#[test]
fn a_lock_left_by_a_dead_launcher_is_taken_over() {
    let home = Home::new("dead");
    let pid = dead_pid();
    fs::write(home.0.join(NAME), format!("pid={pid}\nstarted=1792830862\nsession=abc\n")).unwrap();
    let (lock, stale) = owned(claim(&home.0).unwrap());
    let stale = stale.expect("the dead launcher's record is reported");
    assert_eq!((stale.pid, stale.session.as_deref()), (pid, Some("abc")));
    // The file now names this launcher.
    let record = Record::parse(&fs::read_to_string(home.0.join(NAME)).unwrap()).unwrap();
    assert_eq!(record.pid, std::process::id());
    drop(lock);
    assert!(!home.0.join(NAME).exists(), "a clean end removes it");
    // A clean end leaves nothing to take over.
    let (_lock, stale) = owned(claim(&home.0).unwrap());
    assert_eq!(stale, None);
}

#[test]
fn an_earlier_launchers_directory_with_a_dead_pid_is_taken_over() {
    // The Mac's case: `launch.lock/pid` of a launcher AppKit ended with exit(0).
    let home = Home::new("legacy-dead");
    let pid = dead_pid();
    fs::create_dir(home.0.join(NAME)).unwrap();
    fs::write(home.0.join(NAME).join("pid"), format!("{pid}\n")).unwrap();
    let (_lock, stale) = owned(claim(&home.0).unwrap());
    assert_eq!(stale.map(|s| s.pid), Some(pid));
    assert!(fs::metadata(home.0.join(NAME)).unwrap().is_file());
}

#[test]
fn a_lock_held_by_a_running_launcher_is_busy_until_it_ends() {
    let home = Home::new("held");
    let (first, _) = owned(claim(&home.0).unwrap());
    // (flock is per open file: a second open in this process is kept out
    // as another process would be.)
    match claim(&home.0).unwrap() {
        Claim::Busy(Busy::Held(Some(record))) => assert_eq!(record.pid, std::process::id()),
        other => panic!("{other:?}"),
    }
    assert_eq!(holder(&home.0).unwrap().map(|r| r.pid), Some(std::process::id()));
    drop(first);
    assert_eq!(holder(&home.0).unwrap(), None);
    owned(claim(&home.0).unwrap());
}

#[test]
fn a_reused_pid_does_not_keep_a_lock() {
    // The record names a pid that runs again, as an unrelated process: with
    // nobody holding the flock the lock is free whatever runs there.
    let home = Home::new("reused");
    let other = unrelated();
    fs::write(home.0.join(NAME), format!("pid={}\nstarted=1\n", other.0.id())).unwrap();
    let (_lock, stale) = owned(claim(&home.0).unwrap());
    assert_eq!(stale.map(|s| s.pid), Some(other.0.id()));

    // An earlier launcher's directory naming a running process that is no
    // launcher.
    let home = Home::new("reused-legacy");
    fs::create_dir(home.0.join(NAME)).unwrap();
    fs::write(home.0.join(NAME).join("pid"), format!("{}\n", other.0.id())).unwrap();
    owned(claim(&home.0).unwrap());
}

#[test]
fn an_earlier_launchers_directory_is_kept_only_for_a_launcher_older_than_it() {
    let now = SystemTime::now();
    let written = now - Duration::from_secs(600);
    let launcher = |command: &str, elapsed: u64| ProcessInfo { command: command.into(), elapsed: Duration::from_secs(elapsed) };
    assert!(legacy_verdict(Some(&launcher("/r/runtime/bin/civ3-play-macos", 700)), written, now));
    assert!(legacy_verdict(Some(&launcher("civ3-play-macos", 600)), written, now));
    // Started after the lock was written: the pid was reused.
    assert!(!legacy_verdict(Some(&launcher("civ3-play-macos", 30)), written, now));
    // Not a launcher.
    assert!(!legacy_verdict(Some(&launcher("/bin/sleep", 7000)), written, now));
    // Gone.
    assert!(!legacy_verdict(None, written, now));

    // A live one stays busy.
    let home = Home::new("legacy-live");
    fs::create_dir(home.0.join(NAME)).unwrap();
    fs::write(home.0.join(NAME).join("pid"), "4242\n").unwrap();
    let probe = |pid| (pid == 4242).then(|| launcher("civ3-play-macos", 3600));
    match claim_with(&home.0, probe).unwrap() {
        Claim::Busy(Busy::Legacy(record)) => assert_eq!(record.pid, 4242),
        other => panic!("{other:?}"),
    }
    // Until the user removes it.
    remove(&home.0).unwrap();
    owned(claim_with(&home.0, probe).unwrap());
}

#[test]
fn removing_a_held_lock_lets_the_next_launcher_in_and_the_old_holder_cannot_undo_it() {
    let home = Home::new("remove");
    let (first, _) = owned(claim(&home.0).unwrap());
    assert!(matches!(claim(&home.0).unwrap(), Claim::Busy(_)));
    remove(&home.0).unwrap();
    let (mut second, stale) = owned(claim(&home.0).unwrap());
    assert_eq!(stale, None);
    second.note_session("feed").unwrap();
    drop(first);
    let record = Record::parse(&fs::read_to_string(home.0.join(NAME)).unwrap()).unwrap();
    assert_eq!(record.session.as_deref(), Some("feed"));
    assert!(matches!(claim(&home.0).unwrap(), Claim::Busy(Busy::Held(_))));
}

#[test]
fn a_symlinked_lock_is_refused() {
    let home = Home::new("symlink");
    std::os::unix::fs::symlink(home.0.join("elsewhere"), home.0.join(NAME)).unwrap();
    assert!(claim(&home.0).is_err());
    assert!(!home.0.join("elsewhere").exists());
}

#[test]
fn records_and_ps_times_parse() {
    assert_eq!(Record::parse("96399\n"), Some(Record { pid: 96399, started: 0, session: None }));
    assert_eq!(Record::parse("pid=7\nstarted=9\nsession=ab\n"), Some(Record { pid: 7, started: 9, session: Some("ab".into()) }));
    assert_eq!(Record::parse(""), None);
    assert_eq!(elapsed("05:07"), Some(Duration::from_secs(307)));
    assert_eq!(elapsed("1:05:07"), Some(Duration::from_secs(3907)));
    assert_eq!(elapsed("2-01:05:07"), Some(Duration::from_secs(2 * 86_400 + 3907)));
    assert_eq!(elapsed("x"), None);
    assert_eq!(utc(1_792_830_862), "2026-10-24 08:34:22");
    let record = Record { pid: 5, started: 1_792_830_862, session: None };
    assert_eq!(record.describe(UNIX_EPOCH + Duration::from_secs(1_792_831_162)), "launcher 5, started 2026-10-24 08:34:22 UTC (5 min ago)");
}

#[test]
fn a_running_process_is_seen_by_ps() {
    let other = unrelated();
    let info = process_info(other.0.id()).expect("sleep runs");
    assert!(info.command.ends_with("sleep"), "{info:?}");
    assert!(info.elapsed < Duration::from_secs(30));
    assert_eq!(process_info(dead_pid()), None);
}
