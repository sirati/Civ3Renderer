//! The bridge's tokio runtime: one current-thread scheduler driven by one
//! thread of ours (`civ3r-rt`), for the actors that mostly wait (the trace
//! request server, startup pollers). CPU-bound actors (publisher, art,
//! fonts, preview, replay) keep threads of their own and talk to everyone
//! through channels; on a shared scheduler their seconds-long work would
//! starve the waiting actors, and a multi-thread runtime would add worker
//! threads to the game's 32-bit process for nothing. The game's thread
//! never enters the runtime: it only sends (see `docs/concurrency.md`).

use std::future::Future;
use std::sync::OnceLock;

static HANDLE: OnceLock<Option<tokio::runtime::Handle>> = OnceLock::new();

fn start() -> Option<tokio::runtime::Handle> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .inspect_err(|e| log::error!("runtime: {e}"))
        .ok()?;
    let handle = runtime.handle().clone();
    std::thread::Builder::new()
        .name("civ3r-rt".into())
        .spawn(move || runtime.block_on(std::future::pending::<()>()))
        .inspect_err(|e| log::error!("runtime thread: {e}"))
        .ok()?;
    Some(handle)
}

/// The runtime, started on first use (never by the game's thread).
pub fn handle() -> Option<&'static tokio::runtime::Handle> {
    HANDLE.get_or_init(start).as_ref()
}

/// Runs `task` on the runtime; false if it could not start.
pub fn spawn(task: impl Future<Output = ()> + Send + 'static) -> bool {
    handle().map(|h| drop(h.spawn(task))).is_some()
}

#[cfg(test)]
mod tests {
    #[test]
    fn tasks_run_on_the_runtime_thread_and_answer_through_channels() {
        let (answer, answered) = tokio::sync::oneshot::channel();
        assert!(super::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            let _ = answer.send(std::thread::current().name().map(str::to_owned));
        }));
        assert_eq!(answered.blocking_recv().unwrap().as_deref(), Some("civ3r-rt"));
    }
}
