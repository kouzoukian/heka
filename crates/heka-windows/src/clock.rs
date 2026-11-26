use heka_core::clock::Timestamp;
use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

pub fn local_time() -> Timestamp {
    let mut now = SYSTEMTIME::default();
    unsafe { GetLocalTime(&mut now) };
    Timestamp {
        year: now.wYear,
        month: now.wMonth,
        day: now.wDay,
        hour: now.wHour,
        minute: now.wMinute,
        second: now.wSecond,
        millisecond: now.wMilliseconds,
    }
}
