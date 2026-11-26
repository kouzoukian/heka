//! Finding Controllers with `hidapi` and reading their input reports.
//!
//! Everything here only reads: input reports, and the pairing-info feature report
//! for the MAC address. No output report is ever sent and no feature report is set.

use std::ffi::{CStr, CString};
use std::fmt;

use hidapi::{BusType, DeviceInfo, HidApi, HidDevice, HidError};

use heka_core::dualsense;
use heka_core::link::{LinkRead, LinkReader, ReportError, ReportFormat};
use heka_core::mac::MacAddress;

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
}

impl DualSenseUsbLink {
    /// Opens the first DualSense found on USB, if any.
    pub fn open_first(api: &HidApi) -> Result<Option<Self>, HidError> {
        let Some(info) = dualsense_usb_devices(api).next() else {
            return Ok(None);
        };
        let device = info.open_device(api)?;
        Ok(Some(Self { device }))
    }

    /// Opens the DualSense at this HID path and reads its MAC address.
    pub fn open_path(api: &HidApi, path: &CStr) -> Result<(Self, MacAddress), OpenError> {
        let device = api.open_path(path).map_err(OpenError::Hid)?;
        let mut report = [0u8; dualsense::PAIRING_INFO_LEN];
        report[0] = dualsense::PAIRING_INFO_REPORT_ID;
        // Reads a feature report. Nothing is written to the Controller's state.
        let len = device
            .get_feature_report(&mut report)
            .map_err(OpenError::Hid)?;
        let mac = dualsense::parse_pairing_info(&report[..len.min(report.len())])
            .map_err(OpenError::Mac)?;
        Ok((Self { device }, mac))
    }
}

/// HID paths of every DualSense gamepad collection on USB.
pub fn dualsense_usb_paths(api: &HidApi) -> Vec<CString> {
    dualsense_usb_devices(api)
        .map(|info| info.path().to_owned())
        .collect()
}

/// Why a DualSense could not be opened.
#[derive(Debug)]
pub enum OpenError {
    Hid(HidError),
    /// The pairing-info report did not hold a MAC address.
    Mac(ReportError),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hid(error) => write!(f, "{error}"),
            Self::Mac(error) => write!(f, "cannot read the MAC address: {error}"),
        }
    }
}

impl std::error::Error for OpenError {}

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
