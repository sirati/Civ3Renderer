//! The 32-bit caller of `wine_unixlib_echo.dll`'s unixlib: loads it the
//! way every builtin does (`wine_unixlib::pe::UnixLib`), makes each call,
//! then one with a bad address, which must fail without a crash. Prints
//! `ECHO_OK calls=20` and exits 0 when all hold; otherwise one
//! `ECHO_FAILED ...` line and a nonzero exit.

fn main() {
    std::process::exit(run());
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn run() -> i32 {
    println!("ECHO_FAILED this caller is a 32-bit Windows program");
    1
}

#[cfg(all(windows, target_arch = "x86"))]
fn run() -> i32 {
    use wine_unixlib_echo::args::{CALLS, Echo};
    use wine_unixlib::pe::{UnixLib, buffer, buffer_mut};

    let lib = match UnixLib::load("wine_unixlib_echo.dll") {
        Ok(lib) => lib,
        Err(status) => {
            println!("ECHO_FAILED load status={status:#x}");
            return 2;
        }
    };
    let input = b"wine unixlib echo";
    for code in 0..CALLS {
        let mut output = [0u8; 17];
        let mut args = Echo { input: buffer(input), output: buffer_mut(&mut output), value: 1000 + code, _pad: 0 };
        let status = lib.call(code, &mut args);
        let mut expected = *input;
        expected.reverse();
        if status != 0 || output != expected || args.value != (1000 + code) * 3 + code {
            println!("ECHO_FAILED call={code} status={status:#x} value={} output={:?}", args.value, String::from_utf8_lossy(&output));
            return 3;
        }
    }
    let mut output = [0u8; 4];
    let mut bad = Echo { input: wine_unixlib::Buffer32 { addr: 0x10, len: 16 }, output: buffer_mut(&mut output), value: 0, _pad: 0 };
    let status = lib.call(0, &mut bad);
    if status != wine_unixlib::STATUS_INVALID_PARAMETER {
        println!("ECHO_FAILED bad-address status={status:#x}");
        return 4;
    }
    println!("ECHO_OK calls={CALLS}");
    0
}
