//! heka's value under the current user's Run key, which Windows starts at sign-in.
//!
//! Never called by tests: the logic around it is tested in `heka-core` with a fake.

use std::io;

use heka_core::autostart::RunEntry;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "heka";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn check(status: u32) -> io::Result<()> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}

/// `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`, value `heka`.
#[derive(Debug, Default)]
pub struct RunKey;

impl RunEntry for RunKey {
    fn get(&self) -> io::Result<Option<String>> {
        let (key, value) = (wide(RUN_KEY), wide(VALUE_NAME));
        let mut buffer: Vec<u16> = vec![0; 512];
        loop {
            let mut bytes = (buffer.len() * 2) as u32;
            let status = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    buffer.as_mut_ptr().cast(),
                    &mut bytes,
                )
            };
            match status {
                ERROR_FILE_NOT_FOUND => return Ok(None),
                ERROR_MORE_DATA => buffer.resize(bytes as usize / 2 + 1, 0),
                _ => {
                    check(status)?;
                    // `bytes` counts the terminating null.
                    let len = (bytes as usize / 2).saturating_sub(1);
                    return Ok(Some(String::from_utf16_lossy(&buffer[..len])));
                }
            }
        }
    }

    fn set(&mut self, command: &str) -> io::Result<()> {
        let (key, value, data) = (wide(RUN_KEY), wide(VALUE_NAME), wide(command));
        check(unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        })
    }

    fn remove(&mut self) -> io::Result<()> {
        let (key, value) = (wide(RUN_KEY), wide(VALUE_NAME));
        match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) } {
            ERROR_FILE_NOT_FOUND => Ok(()),
            status => check(status),
        }
    }
}
