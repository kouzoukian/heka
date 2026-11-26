use std::io;
use std::thread;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows_sys::Win32::System::Threading::{
    CreateEventW, INFINITE, SetEvent, WaitForSingleObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};

const EVENT_NAME: &str = r"Local\heka.show-window";

pub enum Instance {
    First(FirstInstance),
    Second,
}

pub struct FirstInstance {
    event: Handle,
}

struct Handle(HANDLE);

unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub fn claim() -> io::Result<Instance> {
    let name: Vec<u16> = EVENT_NAME.encode_utf16().chain([0]).collect();
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
        AllowSetForegroundWindow(ASFW_ANY);
        if SetEvent(event.0) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(Instance::Second)
}

impl FirstInstance {
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
