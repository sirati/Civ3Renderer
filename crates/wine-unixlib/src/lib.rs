//! Safe building blocks for Wine "dual stack" programs: Windows (PE) code
//! calling native Linux code in the same process, through Wine's unixlib
//! mechanism (`__wine_unix_call`).
//!
//! * The PE side ([`pe`], 32-bit Windows targets) asks Wine for the unixlib
//!   of a builtin DLL and calls its functions with a plain-data argument
//!   struct.
//! * The Unix side ([`unix`], Linux and macOS) exports the call tables Wine looks up
//!   in the `.so` ([`unix_call_table!`]), reads memory of 32-bit callers
//!   (under Wine's new WoW64 mode the 32-bit address space is the low
//!   4 GiB of the same 64-bit process), and starts native threads that are
//!   safe next to Wine.
//!
//! How Wine pairs the two (Wine 11, new WoW64): the PE file carries the
//! marker "Wine builtin DLL" at offset 0x40 (see `tools/mark-wine-builtin`);
//! it is found as `$WINEDLLPATH/i386-windows/NAME.dll` (a copy must also be
//! on the Windows search path) and its unixlib as
//! `$WINEDLLPATH/x86_64-unix/NAME.so`. On Apple silicon (ARM64 Wine, the
//! 32-bit PE translated by FEX through new WoW64) the unixlib is an ARM64
//! Mach-O dylib named `$WINEDLLPATH/aarch64-unix/NAME.so`; the calls and
//! the shared low 4 GiB are the same.
//!
//! All `unsafe` needed for this lives here.

/// An NTSTATUS. 0 is success.
pub type Status = i32;
pub const STATUS_SUCCESS: Status = 0;
pub const STATUS_INVALID_PARAMETER: Status = 0xC000_000Du32 as i32;
pub const STATUS_DLL_NOT_FOUND: Status = 0xC000_0135u32 as i32;
pub const STATUS_BUFFER_TOO_SMALL: Status = 0xC000_0023u32 as i32;
pub const STATUS_UNSUCCESSFUL: Status = 0xC000_0001u32 as i32;
/// The other side is busy; try again later (`STATUS_DEVICE_BUSY`).
pub const STATUS_DEVICE_BUSY: Status = 0x8000_0011u32 as i32;

/// A buffer in the 32-bit caller's memory: address and length. Argument
/// structs use it in place of pointers (the same layout on both sides).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Buffer32 {
    pub addr: u32,
    pub len: u32,
}

#[cfg(all(windows, target_arch = "x86"))]
pub mod pe;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod unix;
