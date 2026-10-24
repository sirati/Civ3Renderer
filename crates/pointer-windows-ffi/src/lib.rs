//! The Windows calls under `pointer-windows`: [`clip_cursor`] clips the
//! system pointer to a rectangle and [`clip_rect`] tells the clip in
//! effect; [`cursor_base_size`] and [`cursor_metric_size`] tell the user's
//! pointer size. Empty off Windows.

#![cfg(windows)]

/// The system pointer's clip in screen coordinates, `[left, top, right,
/// bottom]` (the whole desktop while there is none), as the calling
/// thread's DPI awareness sees it; `None` if it cannot be read.
pub fn clip_rect() -> Option<[i32; 4]> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClipCursor;
    let mut r = RECT::default();
    // SAFETY: GetClipCursor writes one valid RECT.
    (unsafe { GetClipCursor(&mut r) } != 0).then_some([r.left, r.top, r.right, r.bottom])
}

/// Clips the system pointer to `screen` (`[left, top, right, bottom]`),
/// or releases the clip with `None`. Whether the system took it. The
/// system cuts the rectangle to the desktop ([`clip_rect`] tells).
pub fn clip_cursor(screen: Option<[i32; 4]>) -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::ClipCursor;
    let rect = screen.map(|s| RECT { left: s[0], top: s[1], right: s[2], bottom: s[3] });
    // SAFETY: ClipCursor reads a valid RECT for the duration of the call;
    // null releases the clip.
    unsafe { ClipCursor(rect.as_ref().map_or(std::ptr::null(), |r| r as *const RECT)) != 0 }
}

/// The user's pointer size at 96 DPI, in pixels: the accessibility
/// setting `HKCU\Control Panel\Cursors\CursorBaseSize` (a DWORD; Windows
/// writes it once the size is changed, 32 being the default). `None` when
/// it is absent or not a positive DWORD.
pub fn cursor_base_size() -> Option<u32> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    let key: Vec<u16> = "Control Panel\\Cursors\0".encode_utf16().collect();
    let value: Vec<u16> = "CursorBaseSize\0".encode_utf16().collect();
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: both names are NUL-terminated UTF-16 that outlive the call;
    // RRF_RT_REG_DWORD restricts the value to 4 bytes, the size of `data`
    // as told in `size`; a null type pointer is permitted.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    (status == 0 && data > 0).then_some(data)
}

/// The system's cursor width metric (`SM_CXCURSOR`) at 96 DPI, in pixels.
pub fn cursor_metric_size() -> Option<u32> {
    use windows_sys::Win32::UI::HiDpi::GetSystemMetricsForDpi;
    use windows_sys::Win32::UI::WindowsAndMessaging::SM_CXCURSOR;
    // SAFETY: a value query without pointers; 0 means failure.
    let size = unsafe { GetSystemMetricsForDpi(SM_CXCURSOR, 96) };
    u32::try_from(size).ok().filter(|&s| s > 0)
}
