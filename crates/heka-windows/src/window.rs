use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use windows_sys::Win32::UI::WindowsAndMessaging::IsIconic;

pub fn monitor_unless_minimized(window: isize) -> Option<u64> {
    let window = window as HWND;
    if unsafe { IsIconic(window) } != 0 {
        return None;
    }
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    (!monitor.is_null()).then_some(monitor as usize as u64)
}
