//! The signal handler wakes a pipe. A normal thread updates cancellation and
//! wakes the UI, including while it retains a completed failure diagnostic.
use signal_hook::{
    consts::{SIGHUP, SIGINT, SIGTERM},
    iterator::{Handle, Signals},
};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
};

pub(super) struct Listener {
    handle: Handle,
    reader: Option<JoinHandle<()>>,
}
impl Listener {
    pub(super) fn new(
        cancel: Arc<AtomicUsize>,
        wake: impl Fn(i32) -> bool + Send + 'static,
    ) -> io::Result<Self> {
        let mut signals = Signals::new([SIGINT, SIGTERM, SIGHUP])?;
        let handle = signals.handle();
        let reader = thread::Builder::new()
            .name("launcher-signals".into())
            .spawn(move || {
                for signal in signals.forever() {
                    let status = match signal {
                        SIGINT => 130,
                        SIGTERM => 143,
                        SIGHUP => 129,
                        _ => continue,
                    };
                    // Publish before enqueueing so Finished racing with this wake
                    // also observes interruption and cannot retain a failure UI.
                    cancel.store(status, Ordering::Relaxed);
                    if !wake(status as i32) {
                        break;
                    }
                }
            })?;
        Ok(Self {
            handle,
            reader: Some(reader),
        })
    }

    /// Called outside run_app. close wakes forever(), even with no signals.
    /// The reader only stores an atomic and sends an unbounded wake event.
    pub(super) fn finish(mut self) {
        self.handle.close();
        if let Some(reader) = self.reader.take() {
            if reader.join().is_err() {
                eprintln!("civ3-play-macos: signal delivery thread failed");
            }
        }
    }
}
impl Drop for Listener {
    fn drop(&mut self) {
        // Error/unwind paths must also release the pipe reader. Never join from
        // a destructor or a native UI callback. Its owned data outlives us.
        self.handle.close();
    }
}

#[cfg(test)]
mod tests;
