//! Real signals run in isolated test subprocesses. The synthetic lifecycle
//! worker completes independently of the signal reader, just as production
//! keeps the main-thread diagnostic alive after its lifecycle worker exits.
use super::*;
use crate::startup::Startup;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MARKER: &str = "CIV3R_SIGNAL_TEST_OWNER";
const READY: &str = "CIV3R_SIGNAL_READY";
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn subprocess(mode: &str, terminate: bool) {
    let token = format!(
        "{}-{}-{mode}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut child = OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "startup::signals::tests::signal_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(MARKER, &token)
            .env("CIV3R_SIGNAL_TEST_MODE", mode)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let stdout = child.0.stdout.take().unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let output = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let line = line.unwrap();
            // The test harness may prefix this line with the test name.
            if line.ends_with(READY) {
                let _ = ready_tx.send(());
            }
        }
    });
    ready_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("signal fixture never became ready");
    if terminate {
        let owned =
            civ3r_process::carrying_environment(MARKER.as_bytes(), token.as_bytes()).unwrap();
        assert_eq!(
            owned.len(),
            1,
            "only the exact marked fixture may receive TERM"
        );
        owned[0].signal(civ3r_process::Signal::Terminate).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "signal fixture {mode} failed: {status}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "signal fixture {mode} did not exit without KILL"
        );
        thread::sleep(Duration::from_millis(10));
    }
    output.join().unwrap();
}

#[test]
fn term_after_lifecycle_worker_completion_wakes_retained_receiver() {
    subprocess("after-worker", true);
}
#[test]
fn term_before_worker_completion_cannot_be_replaced_by_failure() {
    subprocess("before-worker", true);
}
#[test]
fn closed_ui_receiver_releases_signal_reader() {
    subprocess("closed-ui", true);
}
#[test]
fn closing_without_a_signal_wakes_and_joins_reader() {
    subprocess("close-without-signal", false);
}

#[test]
#[ignore = "invoked only by the bounded owning subprocess tests"]
fn signal_child() {
    let mode = std::env::var("CIV3R_SIGNAL_TEST_MODE").expect("requires owning subprocess");
    let cancel = Arc::new(AtomicUsize::new(0));
    let mut ui = Startup::new(cancel.clone());
    let (tx, rx) = mpsc::channel();
    let listener = Listener::new(cancel.clone(), move |status| tx.send(status).is_ok()).unwrap();
    let (worker_tx, worker_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        release_rx.recv().unwrap();
        // An actual failing child is fully reaped before the completed-worker
        // arm reports readiness. It cannot be the recipient of the later TERM.
        let status = Command::new("/bin/sh")
            .args(["-c", "exit 1"])
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(1));
        worker_tx
            .send(Err::<i32, String>("synthetic game failed".into()))
            .unwrap();
    });
    if mode != "before-worker" {
        release_tx.send(()).unwrap();
    }
    let result = if mode == "before-worker" {
        None
    } else {
        let result = worker_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            !ui.complete(result.clone()),
            "failure must be retained until interrupted"
        );
        Some(result)
    };
    // For the completed arm, also ensure the thread has fully exited before
    // publishing READY. This is the exact lifetime missing in the old code.
    let worker = if result.is_some() {
        worker.join().unwrap();
        None
    } else {
        Some(worker)
    };
    let receiver = if mode == "closed-ui" {
        drop(rx);
        None
    } else {
        Some(rx)
    };
    println!("{READY}");
    std::io::stdout().flush().unwrap();
    if mode == "close-without-signal" {
        listener.finish();
        assert_eq!(cancel.load(Ordering::Relaxed), 0);
        return;
    }
    if let Some(receiver) = receiver {
        let status = receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("TERM did not wake the retained UI receiver");
        assert_eq!(status, 143);
        let exit_now = ui.interrupt(status);
        assert_eq!(exit_now, mode != "before-worker");
        if mode == "before-worker" {
            release_tx.send(()).unwrap();
        }
        let result =
            result.unwrap_or_else(|| worker_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        if let Some(worker) = worker {
            worker.join().unwrap();
        }
        assert!(
            ui.complete(result),
            "interruption must suppress retained failure"
        );
        assert_eq!(ui.result.unwrap().unwrap(), 143);
    } else {
        // Sending to a closed UI queue returns false and releases the reader.
        // Join is bounded externally even if that closure path regresses.
        let deadline = Instant::now() + Duration::from_secs(3);
        while !listener.reader.as_ref().unwrap().is_finished() {
            assert!(
                Instant::now() < deadline,
                "closed receiver kept signal reader alive"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(cancel.load(Ordering::Relaxed), 143);
    }
    listener.finish();
}
