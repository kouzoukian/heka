//! Finding Controllers with `hidapi`, reading their input reports and sending Feedback.
//!
//! Reading a Link never sends anything: output reports are only sent through a
//! [`DualSenseUsbWriter`], which the Bridge's Feedback thread owns and uses only
//! when a game sends Feedback.

use std::ffi::{CStr, CString};

use hidapi::{BusType, DeviceInfo, HidApi, HidDevice, HidError};

use heka_core::dualsense::{self, RumbleMode};
use heka_core::link::{
    FeedbackFormat, LinkRead, LinkReader, LinkWriteError, LinkWriter, ReportFormat,
};

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
            path: info.path().to_owned(),
            firmware_info,
            rumble_mode,
            writer,
        }))
    }

    /// The `hidapi` path of this Link, for [`crate::hidhide::device_instance_path`].
    pub fn hid_path(&self) -> &CStr {
        &self.path
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
