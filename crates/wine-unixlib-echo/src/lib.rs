//! `wine_unixlib_echo.dll` (i686 PE, marked builtin) and its unixlib
//! (the same source built natively: `x86_64-unix/wine_unixlib_echo.so`,
//! on Windows `x86_64-native/wine_unixlib_echo.dll`): 20 calls, each
//! reversing a buffer of the caller's into another and mixing its call
//! number into a value. The caller is `wine-unixlib-echo-app`.

pub mod args;

#[cfg(not(target_arch = "x86"))]
mod calls {
    use crate::args::Echo;
    use wine_unixlib::{STATUS_INVALID_PARAMETER, STATUS_SUCCESS, Status};

    fn echo(args: &mut Echo, code: u32) -> Status {
        let Ok(mut bytes) = wine_unixlib::unix::read(args.input) else { return STATUS_INVALID_PARAMETER };
        bytes.reverse();
        if wine_unixlib::unix::write(args.output, &bytes).is_err() {
            return STATUS_INVALID_PARAMETER;
        }
        args.value = args.value.wrapping_mul(3).wrapping_add(code);
        STATUS_SUCCESS
    }

    macro_rules! calls {
        ($($name:ident = $code:literal),* $(,)?) => {
            $(fn $name(args: &mut Echo) -> Status { echo(args, $code) })*
            wine_unixlib::unix_call_table![$($name),*];
            const _: () = assert!([$($code),*].len() == crate::args::CALLS as usize);
        };
    }
    calls![
        c0 = 0, c1 = 1, c2 = 2, c3 = 3, c4 = 4, c5 = 5, c6 = 6, c7 = 7, c8 = 8, c9 = 9,
        c10 = 10, c11 = 11, c12 = 12, c13 = 13, c14 = 14, c15 = 15, c16 = 16, c17 = 17, c18 = 18, c19 = 19,
    ];
}
