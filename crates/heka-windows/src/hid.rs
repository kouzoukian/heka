//! Finding Controllers with `hidapi` and reading their input reports.
//!
//! Everything here only reads. No output or feature report is ever sent.

use std::ffi::{CStr, CString};

use hidapi::{BusType, DeviceInfo, HidApi, HidDevice, HidError};

use heka_core::dualsense;
use heka_core::link::{LinkRead, LinkReader, ReportFormat};

/// HID usage page and usage of a gamepad collection (HID Usage Tables 1.4, Generic Desktop).
const USAGE_PAGE_GENERIC_DESKTOP: u16 = 0x01;
const USAGE_GAMEPAD: u16 = 0x05;

/// How long one read waits before giving the loop a chance to check for a stop.
const READ_TIMEOUT_MS: i32 = 100;

/// Every DualSense HID collection reachable over USB, in enumeration order.
pub fn dualsense_usb_devices(api: &HidApi) -> impl Iterator<Item = &DeviceInfo> {
    api.device_list().filter(|info| {
        dualsense::is_dualsense(info.vendor_id(), info.product_id())
            && matches!(info.bus_type(), BusType::Usb)
            && info.usage_page() == USAGE_PAGE_GENERIC_DESKTOP
            && info.usage() == USAGE_GAMEPAD
    })
}

/// A USB Link to a DualSense, read on the Bridge's input thread.
pub struct DualSenseUsbLink {
    device: HidDevice,
    path: CString,
}

impl DualSenseUsbLink {
    /// Opens the first DualSense found on USB, if any.
    pub fn open_first(api: &HidApi) -> Result<Option<Self>, HidError> {
        let Some(info) = dualsense_usb_devices(api).next() else {
            return Ok(None);
        };
        let device = info.open_device(api)?;
        Ok(Some(Self {
            device,
            path: info.path().to_owned(),
        }))
    }

    /// The `hidapi` path of this Link, for [`crate::hidhide::device_instance_path`].
    pub fn hid_path(&self) -> &CStr {
        &self.path
    }
}

impl LinkReader for DualSenseUsbLink {
    fn format(&self) -> ReportFormat {
        ReportFormat::DualSenseUsb
    }

    fn read(&mut self, buf: &mut [u8]) -> LinkRead {
        match self.device.read_timeout(buf, READ_TIMEOUT_MS) {
            Ok(0) => LinkRead::Timeout,
            Ok(len) => LinkRead::Report(len),
            // hidapi reports an unplugged device as a read error.
            Err(_) => LinkRead::Lost,
        }
    }
}
