//! Finding Controllers with `hidapi`, reading their input reports and sending Feedback.
//!
//! Reading a Link never sends anything: output reports are only sent through a
//! [`ControllerWriter`], which the Bridge's Feedback thread owns and uses only
//! when a game sends Feedback. Opening asks for feature reports and nothing else:
//! the one holding the MAC address first (a DualShock 4 on Bluetooth gives its
//! address as its serial number instead), then the motion calibration, and for a
//! DualSense the firmware info too, for the rumble mode.
//!
//! A Virtual Controller made by ViGEmBus, heka's own included, carries a DualShock
//! 4's IDs, so it is opened like one. Opening stops at its MAC address: nothing
//! more is read from it and it is never opened for Feedback.
//!
//! Over Bluetooth, reading these reports is also what switches a Controller from
//! its short input report to the full one (SDL `SDL_hidapi_ps5.c`: reading the
//! serial number or firmware version "will also enable enhanced reports over
//! Bluetooth"; `SDL_hidapi_ps4.c`: a calibration report switches a DualShock 4
//! "into advanced report mode").

use std::ffi::{CStr, CString};
use std::fmt;

use hidapi::{BusType, DeviceInfo, HidApi, HidDevice, HidError};

use heka_core::calibration::Calibration;
use heka_core::link::{
    ControllerModel, FeedbackFormat, LinkKind, LinkRead, LinkReader, LinkWriteError, LinkWriter,
    ReportError, ReportFormat, controller_model,
};
use heka_core::mac::MacAddress;
use heka_core::{dualsense, dualshock4};

/// HID usage page and usage of a gamepad collection (HID Usage Tables 1.4, Generic Desktop).
const USAGE_PAGE_GENERIC_DESKTOP: u16 = 0x01;
const USAGE_GAMEPAD: u16 = 0x05;

/// How long one read waits before giving the loop a chance to check for a stop.
const READ_TIMEOUT_MS: i32 = 100;

/// The buffer a DualShock 4's feature reports are read into, as long as the one
/// SDL passes (`USB_PACKET_LENGTH`) and longer than any of them.
const DUALSHOCK4_FEATURE_BUFFER_LEN: usize = 64;

/// Every DualSense and DualShock 4 gamepad collection on USB or Bluetooth, in
/// enumeration order. ViGEmBus's virtual DualShock 4s are among them.
pub fn controller_devices(api: &HidApi) -> impl Iterator<Item = &DeviceInfo> {
    api.device_list().filter(|info| {
        controller_model(info.vendor_id(), info.product_id()).is_some()
            && link_kind(info.bus_type()).is_some()
            && info.usage_page() == USAGE_PAGE_GENERIC_DESKTOP
            && info.usage() == USAGE_GAMEPAD
    })
}

/// Every DualSense HID collection reachable over USB, in enumeration order.
pub fn dualsense_usb_devices(api: &HidApi) -> impl Iterator<Item = &DeviceInfo> {
    controller_devices(api).filter(|info| {
        dualsense::is_dualsense(info.vendor_id(), info.product_id())
            && matches!(info.bus_type(), BusType::Usb)
    })
}

fn link_kind(bus: BusType) -> Option<LinkKind> {
    match bus {
        BusType::Usb => Some(LinkKind::Usb),
        BusType::Bluetooth => Some(LinkKind::Bluetooth),
        _ => None,
    }
}

/// A USB or Bluetooth Link to a Controller, read on the Bridge's input thread.
pub struct ControllerLink {
    device: HidDevice,
    path: CString,
    format: ReportFormat,
    writer: Option<ControllerWriter>,
}

impl ControllerLink {
    /// Opens the Controller at this HID path, on USB or Bluetooth, and reads its
    /// MAC address and what is needed to read it and send it Feedback.
    ///
    /// The device is opened twice, so that Feedback can be written from another
    /// thread than the one reading input (`HidDevice` is not `Sync`). If the
    /// second open fails, the Link still reads input but carries no Feedback.
    pub fn open_path(api: &HidApi, path: &CStr) -> Result<(Self, MacAddress), OpenError> {
        let info = api
            .device_list()
            .find(|info| info.path() == path)
            .ok_or(OpenError::NotAController)?;
        let model = controller_model(info.vendor_id(), info.product_id())
            .ok_or(OpenError::NotAController)?;
        let kind = link_kind(info.bus_type()).ok_or(OpenError::NotAController)?;
        let device = api.open_path(path).map_err(OpenError::Hid)?;
        let mac = match (model, kind) {
            (ControllerModel::DualSense, _) => read_dualsense_mac(&device)?,
            (ControllerModel::DualShock4, LinkKind::Usb) => read_dualshock4_mac(&device)?,
            (ControllerModel::DualShock4, LinkKind::Bluetooth) => info
                .serial_number()
                .and_then(|serial| dualshock4::parse_bluetooth_serial(serial).ok())
                .ok_or(OpenError::SerialNumber)?,
        };
        // Before anything else is asked of it.
        if mac.is_virtual_controller() {
            return Err(OpenError::VirtualController);
        }
        let (format, feedback) = match model {
            ControllerModel::DualSense => dualsense_formats(&device, info.product_id(), kind),
            ControllerModel::DualShock4 => dualshock4_formats(&device, kind)?,
        };
        let writer = match api.open_path(path) {
            Ok(device) => Some(ControllerWriter {
                device,
                format: feedback,
            }),
            Err(error) => {
                eprintln!("heka: no Feedback for this Controller: {error}");
                None
            }
        };
        let link = Self {
            device,
            path: path.to_owned(),
            format,
            writer,
        };
        Ok((link, mac))
    }

    /// The `hidapi` path of this Link, for [`crate::hidhide::device_instance_path`].
    pub fn hid_path(&self) -> &CStr {
        &self.path
    }
}

/// How a DualSense's input is read and its Feedback written over this kind of Link.
fn dualsense_formats(
    device: &HidDevice,
    product_id: u16,
    kind: LinkKind,
) -> (ReportFormat, FeedbackFormat) {
    let firmware_info = read_firmware_info(device);
    let calibration = read_dualsense_calibration(device);
    let mode = dualsense::rumble_mode(product_id, firmware_info.as_deref());
    match kind {
        LinkKind::Usb => (
            ReportFormat::DualSenseUsb(calibration),
            FeedbackFormat::DualSenseUsb(mode),
        ),
        LinkKind::Bluetooth => (
            ReportFormat::DualSenseBluetooth(calibration),
            FeedbackFormat::dualsense_bluetooth(mode),
        ),
    }
}

/// As [`dualsense_formats`], for a DualShock 4.
///
/// Over Bluetooth a calibration report that cannot be asked for fails the open, to
/// be tried again: without it the DualShock 4 stays on its short input report,
/// which heka does not read. Calibration that cannot be used reads at full scale.
fn dualshock4_formats(
    device: &HidDevice,
    kind: LinkKind,
) -> Result<(ReportFormat, FeedbackFormat), OpenError> {
    let read = |id| feature_report(device, id, DUALSHOCK4_FEATURE_BUFFER_LEN);
    Ok(match kind {
        LinkKind::Usb => {
            let calibration = match read(dualshock4::USB_CALIBRATION_REPORT_ID) {
                Ok(report) => usable_calibration(dualshock4::parse_usb_calibration(&report)),
                Err(error) => {
                    eprintln!("heka: no motion calibration for this Controller: {error}");
                    Calibration::UNCALIBRATED
                }
            };
            (
                ReportFormat::DualShock4Usb(calibration),
                FeedbackFormat::DualShock4Usb,
            )
        }
        LinkKind::Bluetooth => {
            let report = read(dualshock4::BT_CALIBRATION_REPORT_ID).map_err(OpenError::Hid)?;
            let calibration = usable_calibration(dualshock4::parse_bluetooth_calibration(&report));
            (
                ReportFormat::DualShock4Bluetooth(calibration),
                FeedbackFormat::DualShock4Bluetooth,
            )
        }
    })
}

/// Reads one feature report, report ID included, into a buffer of `len` bytes.
/// Reading a feature report changes nothing the player can see on the Controller.
fn feature_report(device: &HidDevice, id: u8, len: usize) -> Result<Vec<u8>, HidError> {
    let mut report = vec![0u8; len];
    report[0] = id;
    let read = device.get_feature_report(&mut report)?;
    report.truncate(read);
    Ok(report)
}

/// Reads a DualSense's firmware info feature report.
pub fn read_firmware_info(device: &HidDevice) -> Option<Vec<u8>> {
    feature_report(
        device,
        dualsense::FIRMWARE_INFO_REPORT_ID,
        dualsense::FIRMWARE_INFO_REPORT_LEN,
    )
    .ok()
}

/// Reads a DualSense's motion calibration feature report, report ID included.
pub fn read_calibration_report(device: &HidDevice) -> Option<Vec<u8>> {
    feature_report(
        device,
        dualsense::CALIBRATION_REPORT_ID,
        dualsense::CALIBRATION_REPORT_LEN,
    )
    .ok()
}

/// A DualSense's motion calibration, or full scale if it cannot be read.
fn read_dualsense_calibration(device: &HidDevice) -> Calibration {
    match read_calibration_report(device) {
        Some(report) => usable_calibration(dualsense::parse_calibration(&report)),
        None => {
            eprintln!("heka: no motion calibration for this Controller");
            Calibration::UNCALIBRATED
        }
    }
}

/// The calibration that was parsed, or full scale if its report could not be.
fn usable_calibration(parsed: Result<Calibration, ReportError>) -> Calibration {
    parsed.unwrap_or_else(|error| {
        eprintln!("heka: no motion calibration for this Controller: {error}");
        Calibration::UNCALIBRATED
    })
}

/// Reads a DualSense's pairing-info feature report.
fn read_dualsense_mac(device: &HidDevice) -> Result<MacAddress, OpenError> {
    let report = feature_report(
        device,
        dualsense::PAIRING_INFO_REPORT_ID,
        dualsense::PAIRING_INFO_LEN,
    )
    .map_err(OpenError::Hid)?;
    dualsense::parse_pairing_info(&report).map_err(OpenError::Mac)
}

/// Reads the pairing-info feature report of a DualShock 4 on USB.
fn read_dualshock4_mac(device: &HidDevice) -> Result<MacAddress, OpenError> {
    let report = feature_report(
        device,
        dualshock4::PAIRING_INFO_REPORT_ID,
        DUALSHOCK4_FEATURE_BUFFER_LEN,
    )
    .map_err(OpenError::Hid)?;
    dualshock4::parse_pairing_info(&report).map_err(OpenError::Mac)
}

/// HID paths of every DualSense gamepad collection on USB.
pub fn dualsense_usb_paths(api: &HidApi) -> Vec<CString> {
    dualsense_usb_devices(api)
        .map(|info| info.path().to_owned())
        .collect()
}

/// HID paths of every [`controller_devices`] collection.
pub fn controller_paths(api: &HidApi) -> Vec<CString> {
    controller_devices(api)
        .map(|info| info.path().to_owned())
        .collect()
}

/// Why a Controller could not be opened.
#[derive(Debug)]
pub enum OpenError {
    Hid(HidError),
    /// The pairing-info report did not hold a MAC address.
    Mac(ReportError),
    /// A DualShock 4 on Bluetooth whose serial number is not a MAC address.
    SerialNumber,
    /// The device is a Virtual Controller, not a Controller.
    VirtualController,
    /// No listed Controller is at this path.
    NotAController,
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hid(error) => write!(f, "{error}"),
            Self::Mac(error) => write!(f, "cannot read the MAC address: {error}"),
            Self::SerialNumber => f.write_str("the serial number is not a MAC address"),
            Self::VirtualController => f.write_str("it is a Virtual Controller"),
            Self::NotAController => f.write_str("no Controller is there"),
        }
    }
}

impl std::error::Error for OpenError {}

impl LinkReader for ControllerLink {
    fn format(&self) -> ReportFormat {
        self.format
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

/// Sends output reports to a Controller over USB or Bluetooth, on the Bridge's
/// Feedback thread.
pub struct ControllerWriter {
    device: HidDevice,
    format: FeedbackFormat,
}

impl LinkWriter for ControllerWriter {
    fn format(&self) -> FeedbackFormat {
        self.format.clone()
    }

    fn write(&mut self, report: &[u8]) -> Result<(), LinkWriteError> {
        // An output report through WriteFile, which hidapi waits up to 1 s for.
        self.device
            .write(report)
            .map(|_| ())
            .map_err(LinkWriteError::new)
    }
}
