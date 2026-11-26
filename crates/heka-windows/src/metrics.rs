use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::WindowsAndMessaging::SM_CXSMICON;

pub fn small_icon_size() -> u32 {
    let side = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) };
    u32::try_from(side)
        .ok()
        .filter(|&side| side > 0)
        .unwrap_or(16)
}
