//! Finding Controllers with `hidapi`, reading their input reports and sending Feedback.
//!
//! Reading a Link never sends anything: output reports are only sent through a
//! [`DualSenseUsbWriter`], which the Bridge's Feedback thread owns and uses only
//! when a game sends Feedback. Opening reads two feature reports: pairing info
//! for the MAC address and firmware info for the rumble mode.

use std::ffi::{CStr, CString};
use std::fmt;

use hidapi::{BusType, DeviceInfo, HidApi, HidDevice, HidError};

use heka_core::dualsense::{self, RumbleMode};
use heka_core::link::{
    FeedbackFormat, LinkRead, LinkReader, LinkWriteError, LinkWriter, ReportError, ReportFormat,
};
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
    firmware_info: Option<Vec<u8>>,
    rumble_mode: RumbleMode,
    writer: Option<DualSenseUsbWriter>,
}

impl DualSenseUsbLink {
    /// Opens the first DualSense found on USB, if any, and reads its firmware info.
    ///
    /// The device is opened twice, so that Feedback can be written from another
    /// thread than the one reading input (`HidDevice` is not `Sync`). If the
    /// second open fails, the Link still reads input but carries no Feedback.
    pub fn open_first(api: &HidApi) -> Result<Option<Self>, HidError> {
        let Some(info) = dualsense_usb_devices(api).next() else {
            return Ok(None);
        };
        let device = info.open_device(api)?;
        let firmware_info = read_firmware_info(&device);
        let rumble_mode = dualsense::rumble_mode(info.product_id(), firmware_info.as_deref());
        let writer = match info.open_device(api) {
            Ok(device) => Some(DualSenseUsbWriter {
                device,
                rumble_mode,
            }),
            Err(error) => {
                eprintln!("heka: no Feedback for this DualSense: {error}");
                None
            }
        };
        Ok(Some(Self {
            device,
            firmware_info,
            rumble_mode,
            writer,
        }))
    }

    /// Opens the DualSense at this HID path, reads its MAC address and firmware info.
    ///
    /// Like [`Self::open_first`], the device is opened a second time for Feedback.
    pub fn open_path(api: &HidApi, path: &CStr) -> Result<(Self, MacAddress), OpenError> {
        let product_id = api
            .device_list()
            .find(|info| info.path() == path)
            .map_or(dualsense::PRODUCT_ID_DUALSENSE, DeviceInfo::product_id);
        let device = api.open_path(path).map_err(OpenError::Hid)?;
        let mac = read_mac(&device)?;
        let firmware_info = read_firmware_info(&device);
        let rumble_mode = dualsense::rumble_mode(product_id, firmware_info.as_deref());
        let writer = match api.open_path(path) {
            Ok(device) => Some(DualSenseUsbWriter {
                device,
                rumble_mode,
            }),
            Err(error) => {
                eprintln!("heka: no Feedback for this DualSense: {error}");
                None
            }
        };
        let link = Self {
            device,
            firmware_info,
            rumble_mode,
            writer,
        };
        Ok((link, mac))
    }

    /// The firmware info feature report as read on open, report ID included.
    pub fn firmware_info(&self) -> Option<&[u8]> {
        self.firmware_info.as_deref()
    }

    /// How this DualSense rumbles, decided from its product ID and firmware info.
    pub fn rumble_mode(&self) -> RumbleMode {
        self.rumble_mode
    }
}

/// Reads the firmware info feature report. Reading a feature report changes nothing
/// on the Controller.
fn read_firmware_info(device: &HidDevice) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; dualsense::FIRMWARE_INFO_REPORT_LEN];
    buf[0] = dualsense::FIRMWARE_INFO_REPORT_ID;
    let len = device.get_feature_report(&mut buf).ok()?;
    buf.truncate(len);
    Some(buf)
}

/// Reads the pairing-info feature report. Reading a feature report changes nothing
/// on the Controller.
fn read_mac(device: &HidDevice) -> Result<MacAddress, OpenError> {
    let mut report = [0u8; dualsense::PAIRING_INFO_LEN];
    report[0] = dualsense::PAIRING_INFO_REPORT_ID;
    let len = device
        .get_feature_report(&mut report)
        .map_err(OpenError::Hid)?;
    dualsense::parse_pairing_info(&report[..len.min(report.len())]).map_err(OpenError::Mac)
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

    fn feedback_writer(&mut self) -> Option<Box<dyn LinkWriter>> {
        self.writer
            .take()
            .map(|writer| Box::new(writer) as Box<dyn LinkWriter>)
    }
}

/// Sends output reports to a DualSense over USB, on the Bridge's Feedback thread.
pub struct DualSenseUsbWriter {
    device: HidDevice,
    rumble_mode: RumbleMode,
}

impl LinkWriter for DualSenseUsbWriter {
    fn format(&self) -> FeedbackFormat {
        FeedbackFormat::DualSenseUsb(self.rumble_mode)
    }

    fn write(&mut self, report: &[u8]) -> Result<(), LinkWriteError> {
        // An interrupt OUT transfer (WriteFile), which hidapi waits up to 1 s for.
        self.device
            .write(report)
            .map(|_| ())
            .map_err(LinkWriteError::new)
    }
}
