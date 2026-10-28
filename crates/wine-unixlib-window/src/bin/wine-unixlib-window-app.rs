//! The 32-bit caller of `wine_unixlib_window.dll`'s unixlib. Modes (the
//! first argument):
//!
//! * `wait`: opens the window, waits for 60 frames, then four guest threads
//!   block in calls at once, each until 120 more frames are presented, while
//!   the main thread goes on; then closes the window. Prints
//!   `WINDOW_OK mode=wait frames=N adapter=...` and exits 0.
//! * `exit-live`: opens the window, waits for 30 frames, prints
//!   `WINDOW_EXIT_LIVE frames=N adapter=...` and calls `ExitProcess(37)`
//!   with the window thread still drawing. The process must end with 37.
//! * `worker-exit-live`: as `exit-live`, but the main thread is blocked in a
//!   call (waiting a minute for frames that never come) when a guest worker
//!   prints `WINDOW_WORKER_EXIT frames=N` and calls `ExitProcess(38)`. The
//!   process must end with 38 well before the minute.
//!
//! Failures print one `WINDOW_FAILED ...` line: exit 2 (load or usage),
//! 3 (open or wait), 4 (close), 5 (a blocked call returned in
//! `worker-exit-live`).

fn main() {
    std::process::exit(run(std::env::args().nth(1).unwrap_or_default().as_str()));
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn run(_: &str) -> i32 {
    println!("WINDOW_FAILED this caller is a 32-bit Windows program");
    2
}

#[cfg(all(windows, target_arch = "x86"))]
fn run(mode: &str) -> i32 {
    use std::io::Write;
    use wine_unixlib::pe::UnixLib;
    use wine_unixlib_window::args::*;

    if !matches!(mode, "wait" | "exit-live" | "worker-exit-live") {
        println!("WINDOW_FAILED usage: wait | exit-live | worker-exit-live");
        return 2;
    }
    let lib = match UnixLib::load("wine_unixlib_window.dll") {
        Ok(lib) => lib,
        Err(status) => {
            println!("WINDOW_FAILED load status={status:#x}");
            return 2;
        }
    };
    let adapter = |lib: &UnixLib| {
        let mut text = vec![0u8; 512];
        let mut args = Info { out: wine_unixlib::pe::buffer_mut(&mut text), len: 0, _pad: 0 };
        let status = lib.call(CALL_INFO, &mut args);
        text.truncate(args.len.min(512) as usize);
        if status == 0 { String::from_utf8_lossy(&text).into_owned() } else { format!("status={status:#x}") }
    };
    let wait = |lib: &UnixLib, frames: u32, timeout_ms: u32| {
        let mut args = Frames { frames, timeout_ms, ..Default::default() };
        (lib.call(CALL_WAIT, &mut args), args)
    };
    let mut open = Frames::default();
    let status = lib.call(CALL_OPEN, &mut open);
    if status != 0 {
        println!("WINDOW_FAILED open status={status:#x}");
        return 3;
    }
    let first = if mode == "wait" { 60 } else { 30 };
    let (status, reached) = wait(&lib, first, 30_000);
    if status != 0 {
        println!("WINDOW_FAILED first-frames status={status:#x} frames={} waited_ms={} adapter={}", reached.frames, reached.waited_ms, adapter(&lib));
        return 3;
    }
    match mode {
        "exit-live" => {
            println!("WINDOW_EXIT_LIVE frames={} adapter={}", reached.frames, adapter(&lib));
            let _ = std::io::stdout().flush();
            std::process::exit(37);
        }
        "worker-exit-live" => {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let (_, now) = wait(&lib, 0, 0);
                println!("WINDOW_WORKER_EXIT frames={}", now.frames);
                let _ = std::io::stdout().flush();
                std::process::exit(38);
            });
            let (status, args) = wait(&lib, u32::MAX, 60_000);
            println!("WINDOW_FAILED blocked call returned status={status:#x} waited_ms={}", args.waited_ms);
            5
        }
        _ => {
            let target = reached.frames + 120;
            let waiters: Vec<_> = (0..4).map(|_| std::thread::spawn(move || wait(&lib, target, 30_000))).collect();
            let mut spins = 0u64;
            while !waiters.iter().all(|w| w.is_finished()) {
                spins += 1;
                std::thread::yield_now();
            }
            for (i, waiter) in waiters.into_iter().enumerate() {
                let (status, args) = waiter.join().unwrap_or((-1, Frames::default()));
                if status != 0 || args.frames < target {
                    println!("WINDOW_FAILED waiter={i} status={status:#x} frames={} target={target}", args.frames);
                    return 3;
                }
            }
            if spins == 0 {
                println!("WINDOW_FAILED main thread did not run while the waiters were blocked");
                return 3;
            }
            let adapter = adapter(&lib);
            let mut close = Frames { timeout_ms: 10_000, ..Default::default() };
            let status = lib.call(CALL_CLOSE, &mut close);
            if status != 0 {
                println!("WINDOW_FAILED close status={status:#x} waited_ms={}", close.waited_ms);
                return 4;
            }
            println!("WINDOW_OK mode=wait frames={} adapter={adapter}", close.frames);
            0
        }
    }
}
