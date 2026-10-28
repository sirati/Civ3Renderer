/// The argument of every call. In: `frames` (wait until the window has
/// presented this many) and `timeout_ms`. Out: `frames` presented so far
/// and `waited_ms`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Frames {
    pub frames: u32,
    pub timeout_ms: u32,
    pub waited_ms: u32,
    pub _pad: u32,
}

/// The argument of [`CALL_INFO`]: the adapter, or why there is no window,
/// written into `out` (`len`: the size written).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Info {
    pub out: wine_unixlib::Buffer32,
    pub len: u32,
    pub _pad: u32,
}

/// Starts the window thread (once); returns at once.
pub const CALL_OPEN: u32 = 0;
/// Blocks the caller until `frames` have been presented, or `timeout_ms`.
/// `STATUS_UNSUCCESSFUL` on timeout or when the window failed.
pub const CALL_WAIT: u32 = 1;
/// [`Info`].
pub const CALL_INFO: u32 = 2;
/// Closes the window and waits (at most `timeout_ms`) for its thread to end.
pub const CALL_CLOSE: u32 = 3;
