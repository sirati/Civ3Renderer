#![forbid(unsafe_code)]

#[cfg(unix)]
fn main() {
    let result = civ3r_launcher::launch(std::env::args_os().skip(1));
    match result {
        Ok(status) => std::process::exit(status),
        Err(error) => {
            eprintln!("civ3-play-macos: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("civ3-play-macos: this Wine launcher requires a Unix host");
    std::process::exit(1);
}
