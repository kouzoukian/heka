//! One copy of heka per Windows session.
//!
//! The first copy creates a named event and waits on it. A later copy finds the
//! event already there, signals it so the first copy shows its window, and ends.

use std::io;
use std::thread;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows_sys::Win32::System::Threading::{
    CreateEventW, INFINITE, SetEvent, WaitForSingleObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};

/// `Local\` keeps it to the signed-in user's session.
const EVENT_NAME: &str = r"Local\heka.show-window";

pub enum Instance {
    /// No other copy is running.
    First(FirstInstance),
    /// Another copy is running and has been asked to show its window.
    Second,
}

/// Held by the running copy for as long as it runs.
pub struct FirstInstance {
    event: Handle,
}

struct Handle(HANDLE);

// The handle is only waited on and closed, both of which any thread may do.
unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// Claims this session for this copy, or hands over to the copy already running.
pub fn claim() -> io::Result<Instance> {
    let name: Vec<u16> = EVENT_NAME.encode_utf16().chain([0]).collect();
    // Auto-reset, so each later launch wakes the waiting thread once.
    let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) };
    if event.is_null() {
        return Err(io::Error::last_os_error());
    }
    let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let event = Handle(event);
    if !already_running {
        return Ok(Instance::First(FirstInstance { event }));
    }
    unsafe {
        // This copy was just started by the player, so it may bring a window to
        // the front. Pass that on to the running copy.
        AllowSetForegroundWindow(ASFW_ANY);
        if SetEvent(event.0) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(Instance::Second)
}

impl FirstInstance {
    /// Calls `show` on a thread of its own each time another copy is launched.
    pub fn on_second_launch(self, show: impl Fn() + Send + 'static) {
        thread::Builder::new()
            .name("heka-single-instance".into())
            .spawn(move || {
                let event = self.event;
                while unsafe { WaitForSingleObject(event.0, INFINITE) } == 0 {
                    show();
                }
            })
            .expect("failed to start the single-instance thread");
    }
}
