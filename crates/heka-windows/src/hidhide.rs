//! HidHide's driver interface, for [`heka_core::hiding`].
//!
//! Source for every name and code here: nefarius/HidHide tag `v1.2.128.0` (MIT), the
//! version installed on the development PC.
//! - Control device `\\.\HidHide`: `HidHideCLI/HidHideCLI.rc` (`IDS_CONTROL_DEVICE_NAME`).
//! - IOCTL codes and buffer shapes: `HidHideCLI/src/FilterDriverProxy.cpp`, checked
//!   against the driver's handlers in `HidHide/src/Logic.c`.
//! - The control device takes one client at a time (`WdfDeviceInitSetExclusive` in
//!   `HidHide/src/ControlDevice.c`); HidHide's own client reports `ERROR_ACCESS_DENIED`
//!   on open as "in use" (`HidHideClient/src/HidHideClient.cpp`). So heka opens it
//!   for one operation at a time and closes it straight away.
//!
//! See `docs/hidhide.md` for what is verified and what is not.

use std::ffi::CStr;
use std::io;
use std::path::PathBuf;
use std::ptr;

use heka_core::hiding::{
    DeviceInstancePath, FileJournal, HidHide, HidHideConfig, HidHideError, Hiding,
};
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_Device_Interface_PropertyW, CR_SUCCESS,
};
use windows_sys::Win32::Devices::Properties::{DEVPKEY_Device_InstanceId, DEVPROP_TYPE_STRING};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, GENERIC_READ, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, PROCESS_NAME_NATIVE, QueryFullProcessImageNameW,
};

const CONTROL_DEVICE: &str = r"\\.\HidHide";

/// `CTL_CODE` from the Windows DDK (`devioctl.h`).
const fn ctl_code(device_type: u32, function: u32, method: u32, access: u32) -> u32 {
    (device_type << 16) | (access << 14) | (function << 2) | method
}

/// HidHide's custom device type (`IoControlDeviceType` in `FilterDriverProxy.cpp`).
const DEVICE_TYPE: u32 = 32769;
const METHOD_BUFFERED: u32 = 0;
const FILE_READ_DATA: u32 = 1;

const IOCTL_GET_WHITELIST: u32 = ctl_code(DEVICE_TYPE, 2048, METHOD_BUFFERED, FILE_READ_DATA);
const IOCTL_SET_WHITELIST: u32 = ctl_code(DEVICE_TYPE, 2049, METHOD_BUFFERED, FILE_READ_DATA);
const IOCTL_GET_BLACKLIST: u32 = ctl_code(DEVICE_TYPE, 2050, METHOD_BUFFERED, FILE_READ_DATA);
const IOCTL_SET_BLACKLIST: u32 = ctl_code(DEVICE_TYPE, 2051, METHOD_BUFFERED, FILE_READ_DATA);
const IOCTL_GET_ACTIVE: u32 = ctl_code(DEVICE_TYPE, 2052, METHOD_BUFFERED, FILE_READ_DATA);
const IOCTL_SET_ACTIVE: u32 = ctl_code(DEVICE_TYPE, 2053, METHOD_BUFFERED, FILE_READ_DATA);
const IOCTL_GET_WLINVERSE: u32 = ctl_code(DEVICE_TYPE, 2054, METHOD_BUFFERED, FILE_READ_DATA);

/// HidHide's driver, opened for each call.
#[derive(Debug, Default, Clone, Copy)]
pub struct HidHideDriver;

impl HidHide for HidHideDriver {
    fn read(&self) -> Result<HidHideConfig, HidHideError> {
        let device = ControlDevice::open()?;
        Ok(HidHideConfig {
            active: device.get_bool(IOCTL_GET_ACTIVE)?,
            inverse: device.get_bool(IOCTL_GET_WLINVERSE)?,
            hidden_devices: device.get_list(IOCTL_GET_BLACKLIST)?,
            allowed_apps: device.get_list(IOCTL_GET_WHITELIST)?,
        })
    }

    fn set_hidden_devices(&self, devices: &[String]) -> Result<(), HidHideError> {
        ControlDevice::open()?.set_list(IOCTL_SET_BLACKLIST, devices)
    }

    fn set_allowed_apps(&self, apps: &[String]) -> Result<(), HidHideError> {
        ControlDevice::open()?.set_list(IOCTL_SET_WHITELIST, apps)
    }

    fn set_active(&self, active: bool) -> Result<(), HidHideError> {
        let value = [u8::from(active)];
        ControlDevice::open()?.ioctl(IOCTL_SET_ACTIVE, &value, &mut [])?;
        Ok(())
    }
}

struct ControlDevice(HANDLE);

impl ControlDevice {
    /// Opens the control device the way `HidHideCLI` does: read access, fully shared.
    fn open() -> Result<Self, HidHideError> {
        let name = wide(CONTROL_DEVICE);
        // SAFETY: `name` is NUL-terminated and outlives the call.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            let error = io::Error::last_os_error();
            return Err(match error.raw_os_error().map(|code| code as u32) {
                Some(ERROR_FILE_NOT_FOUND) => HidHideError::NotInstalled,
                Some(ERROR_ACCESS_DENIED) => HidHideError::InUse,
                _ => HidHideError::Failed(error.to_string()),
            });
        }
        Ok(Self(handle))
    }

    /// One buffered IOCTL. Returns the number of output bytes.
    fn ioctl(&self, code: u32, input: &[u8], output: &mut [u8]) -> Result<usize, HidHideError> {
        let mut returned = 0u32;
        // SAFETY: both buffers are valid for the lengths passed; the call is synchronous.
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                code,
                if input.is_empty() {
                    ptr::null()
                } else {
                    input.as_ptr().cast()
                },
                len_u32(input.len())?,
                if output.is_empty() {
                    ptr::null_mut()
                } else {
                    output.as_mut_ptr().cast()
                },
                len_u32(output.len())?,
                &mut returned,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(HidHideError::Failed(io::Error::last_os_error().to_string()));
        }
        Ok(returned as usize)
    }

    fn get_bool(&self, code: u32) -> Result<bool, HidHideError> {
        let mut value = [0u8];
        match self.ioctl(code, &[], &mut value)? {
            1 => Ok(value[0] != 0),
            n => Err(HidHideError::Failed(format!(
                "HidHide returned {n} bytes for a BOOLEAN"
            ))),
        }
    }

    /// A REG_MULTI_SZ list: asked once for its size in bytes, then read.
    fn get_list(&self, code: u32) -> Result<Vec<String>, HidHideError> {
        let needed = self.ioctl(code, &[], &mut [])?;
        let mut bytes = vec![0u8; needed];
        let got = self.ioctl(code, &[], &mut bytes)?;
        bytes.truncate(got);
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect();
        Ok(from_multi_string(&units))
    }

    /// Replaces a list. An empty input buffer clears it (`Logic.c`, "No input then clear the list").
    fn set_list(&self, code: u32, items: &[String]) -> Result<(), HidHideError> {
        let bytes: Vec<u8> = to_multi_string(items)
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        self.ioctl(code, &bytes, &mut [])?;
        Ok(())
    }
}

impl Drop for ControlDevice {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateFileW and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

fn len_u32(len: usize) -> Result<u32, HidHideError> {
    u32::try_from(len).map_err(|_| HidHideError::Failed("buffer too large".into()))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Strings separated by NUL, ended by an empty string. Empty input means an empty list.
fn to_multi_string(items: &[String]) -> Vec<u16> {
    if items.is_empty() {
        return Vec::new();
    }
    let mut units: Vec<u16> = items
        .iter()
        .flat_map(|item| item.encode_utf16().chain(Some(0)))
        .collect();
    units.push(0);
    units
}

fn from_multi_string(units: &[u16]) -> Vec<String> {
    units
        .split(|&unit| unit == 0)
        .take_while(|item| !item.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// heka's own full image name as HidHide lists allowed applications, for example
/// `\Device\HarddiskVolume3\Tools\heka\heka.exe`. The driver matches allowed
/// applications against the image name the kernel reports when a process loads
/// (`OnSystemLoadImage` in `Logic.c`), which is this native form.
pub fn own_image_name() -> io::Result<String> {
    let mut buffer = vec![0u16; 1024];
    let mut len = buffer.len() as u32;
    // SAFETY: `len` holds the buffer's length in characters, as the API requires.
    let ok = unsafe {
        QueryFullProcessImageNameW(
            GetCurrentProcess(),
            PROCESS_NAME_NATIVE,
            buffer.as_mut_ptr(),
            &mut len,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(String::from_utf16_lossy(&buffer[..len as usize]))
}

/// The device instance path of the HID collection behind a `hidapi` path, which is
/// what HidHide hides (HidHide's own client lists each HID collection by it).
pub fn device_instance_path(hid_path: &CStr) -> io::Result<DeviceInstancePath> {
    let interface = wide(&hid_path.to_string_lossy());
    // Device instance IDs are at most 200 characters (MAX_DEVICE_ID_LEN).
    let mut buffer = [0u16; 256];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    let mut kind = 0u32;
    // SAFETY: `interface` is NUL-terminated; `size` is the buffer's size in bytes.
    let result = unsafe {
        CM_Get_Device_Interface_PropertyW(
            interface.as_ptr(),
            &DEVPKEY_Device_InstanceId,
            &mut kind,
            buffer.as_mut_ptr().cast(),
            &mut size,
            0,
        )
    };
    if result != CR_SUCCESS || kind != DEVPROP_TYPE_STRING {
        return Err(io::Error::other(format!(
            "cannot read the device instance ID (CONFIGRET {result})"
        )));
    }
    let units = &buffer[..(size as usize / 2).min(buffer.len())];
    let text = from_multi_string(units)
        .into_iter()
        .next()
        .unwrap_or_default();
    Ok(DeviceInstancePath::new(text))
}

/// heka's [`Hiding`] over the real driver, journalled at [`journal_path`]. Call
/// [`Hiding::recover`] on it once before the first Bridge.
pub fn hiding() -> io::Result<Hiding> {
    let journal = journal_path().ok_or_else(|| io::Error::other("LOCALAPPDATA is not set"))?;
    Ok(Hiding::new(
        Box::new(HidHideDriver),
        Box::new(FileJournal::new(journal)),
        own_image_name()?,
    ))
}

/// Where heka keeps its HidHide journal: `%LOCALAPPDATA%\heka\hiding.journal`.
pub fn journal_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|dir| PathBuf::from(dir).join("heka").join("hiding.journal"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_codes_match_ctl_code_by_hand() {
        // (32769 << 16) | (FILE_READ_DATA << 14) | (2048 << 2) | METHOD_BUFFERED
        assert_eq!(IOCTL_GET_WHITELIST, 0x8001_6000);
        assert_eq!(IOCTL_SET_ACTIVE, 0x8001_6014);
    }

    #[test]
    fn lists_survive_the_multi_string_encoding() {
        let items = vec![
            r"HID\VID_054C&PID_0CE6&MI_03\8&15C30336&0&0000".to_owned(),
            r"\Device\HarddiskVolume3\Tools\heka\heka.exe".to_owned(),
        ];
        assert_eq!(from_multi_string(&to_multi_string(&items)), items);
        assert!(to_multi_string(&[]).is_empty());
        assert!(from_multi_string(&[0, 0]).is_empty());
        assert!(from_multi_string(&[]).is_empty());
    }
}
