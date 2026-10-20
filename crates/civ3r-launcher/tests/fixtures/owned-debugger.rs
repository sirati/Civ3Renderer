// Host-only lifecycle fixture. Built as `winedbg` by the launcher unit test.
// No Wine, GUI, platform FFI, or guest compilation is involved.
fn main() {
    if std::env::var("CIV3R_CLEANUP_FIXTURE_ROLE").as_deref() == Ok("parent") {
        std::process::Command::new("setsid")
            .arg(std::env::current_exe().unwrap())
            .args(["--auto", "272", "528"])
            .env("CIV3R_CLEANUP_FIXTURE_ROLE", "leaf")
            .spawn().unwrap();
    } else {
        println!("DETACHED_DEBUGGER_READY");
        loop { std::thread::park(); }
    }
}
