use std::ffi::{CStr, CString};

use hidapi::{BusType, DeviceInfo, HidApi, HidDevice, HidError};

use heka_core::calibration::Calibration;
use heka_core::discovery;
use heka_core::link::{
    ControllerModel, FeedbackFormat, LinkKind, LinkProblem, LinkRead, LinkReader, LinkWriteError,
    LinkWriter, ReportError, ReportFormat, controller_model,
};
use heka_core::mac::MacAddress;
use heka_core::{dualsense, dualshock4};

const USAGE_PAGE_GENERIC_DESKTOP: u16 = 0x01;
const USAGE_GAMEPAD: u16 = 0x05;

const READ_TIMEOUT_MS: i32 = 100;

const DUALSHOCK4_FEATURE_BUFFER_LEN: usize = 64;

pub fn controller_devices(api: &HidApi) -> impl Iterator<Item = &DeviceInfo> {
    api.device_list().filter(|info| {
        controller_model(info.vendor_id(), info.product_id()).is_some()
            && link_kind(info.bus_type()).is_some()
            && info.usage_page() == USAGE_PAGE_GENERIC_DESKTOP
            && info.usage() == USAGE_GAMEPAD
    })
}

pub fn dualsense_devices(api: &HidApi, kind: LinkKind) -> impl Iterator<Item = &DeviceInfo> {
    controller_devices(api).filter(move |info| {
        dualsense::is_dualsense(info.vendor_id(), info.product_id())
            && link_kind(info.bus_type()) == Some(kind)
    })
}

fn link_kind(bus: BusType) -> Option<LinkKind> {
    match bus {
        BusType::Usb => Some(LinkKind::Usb),
        BusType::Bluetooth => Some(LinkKind::Bluetooth),
        _ => None,
    }
}

pub struct ControllerLink {
    device: HidDevice,
    path: CString,
    format: ReportFormat,
    writer: Option<ControllerWriter>,
}

impl ControllerLink {
    pub fn open_path(
        api: &HidApi,
        path: &CStr,
    ) -> Result<(Self, MacAddress, Vec<LinkProblem>), OpenError> {
        let not_a_controller = || OpenError::Refused(discovery::OpenError::NotAController);
        let info = api
            .device_list()
            .find(|info| info.path() == path)
            .ok_or_else(not_a_controller)?;
        let model =
            controller_model(info.vendor_id(), info.product_id()).ok_or_else(not_a_controller)?;
        let kind = link_kind(info.bus_type()).ok_or_else(not_a_controller)?;
        let device = api.open_path(path).map_err(hid_error)?;
        let mac = match (model, kind) {
            (ControllerModel::DualSense, _) => read_dualsense_mac(&device)?,
            (ControllerModel::DualShock4, LinkKind::Usb) => read_dualshock4_mac(&device)?,
            (ControllerModel::DualShock4, LinkKind::Bluetooth) => info
                .serial_number()
                .and_then(|serial| dualshock4::parse_bluetooth_serial(serial).ok())
                .ok_or(OpenError::Refused(discovery::OpenError::SerialNumber))?,
        };
        if mac.is_virtual_controller() {
            return Err(OpenError::VirtualController);
        }
        let mut problems = Vec::new();
        let (format, feedback) = match model {
            ControllerModel::DualSense => {
                dualsense_formats(&device, info.product_id(), kind, &mut problems)
            }
            ControllerModel::DualShock4 => dualshock4_formats(&device, kind, &mut problems)?,
        };
        let writer = match api.open_path(path) {
            Ok(device) => Some(ControllerWriter {
                device,
                format: feedback,
            }),
            Err(error) => {
                problems.push(LinkProblem::NoFeedback(error.to_string()));
                None
            }
        };
        let link = Self {
            device,
            path: path.to_owned(),
            format,
            writer,
        };
        Ok((link, mac, problems))
    }

    pub fn hid_path(&self) -> &CStr {
        &self.path
    }
}

fn dualsense_formats(
    device: &HidDevice,
    product_id: u16,
    kind: LinkKind,
    problems: &mut Vec<LinkProblem>,
) -> (ReportFormat, FeedbackFormat) {
    let firmware_info = read_firmware_info(device);
    let calibration = read_dualsense_calibration(device, problems);
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

fn dualshock4_formats(
    device: &HidDevice,
    kind: LinkKind,
    problems: &mut Vec<LinkProblem>,
) -> Result<(ReportFormat, FeedbackFormat), OpenError> {
    let read = |id| feature_report(device, id, DUALSHOCK4_FEATURE_BUFFER_LEN);
    Ok(match kind {
        LinkKind::Usb => {
            let calibration = match read(dualshock4::USB_CALIBRATION_REPORT_ID) {
                Ok(report) => {
                    usable_calibration(dualshock4::parse_usb_calibration(&report), problems)
                }
                Err(error) => {
                    problems.push(LinkProblem::CalibrationUnread(Some(error.to_string())));
                    Calibration::UNCALIBRATED
                }
            };
            (
                ReportFormat::DualShock4Usb(calibration),
                FeedbackFormat::DualShock4Usb,
            )
        }
        LinkKind::Bluetooth => {
            let report = read(dualshock4::BT_CALIBRATION_REPORT_ID).map_err(hid_error)?;
            let calibration =
                usable_calibration(dualshock4::parse_bluetooth_calibration(&report), problems);
            (
                ReportFormat::DualShock4Bluetooth(calibration),
                FeedbackFormat::DualShock4Bluetooth,
            )
        }
    })
}

fn feature_report(device: &HidDevice, id: u8, len: usize) -> Result<Vec<u8>, HidError> {
    let mut report = vec![0u8; len];
    report[0] = id;
    let read = device.get_feature_report(&mut report)?;
    report.truncate(read);
    Ok(report)
}

pub fn read_firmware_info(device: &HidDevice) -> Option<Vec<u8>> {
    feature_report(
        device,
        dualsense::FIRMWARE_INFO_REPORT_ID,
        dualsense::FIRMWARE_INFO_REPORT_LEN,
    )
    .ok()
}

pub fn read_calibration_report(device: &HidDevice) -> Option<Vec<u8>> {
    feature_report(
        device,
        dualsense::CALIBRATION_REPORT_ID,
        dualsense::CALIBRATION_REPORT_LEN,
    )
    .ok()
}

fn read_dualsense_calibration(device: &HidDevice, problems: &mut Vec<LinkProblem>) -> Calibration {
    match read_calibration_report(device) {
        Some(report) => usable_calibration(dualsense::parse_calibration(&report), problems),
        None => {
            problems.push(LinkProblem::CalibrationUnread(None));
            Calibration::UNCALIBRATED
        }
    }
}

fn usable_calibration(
    parsed: Result<Calibration, ReportError>,
    problems: &mut Vec<LinkProblem>,
) -> Calibration {
    parsed.unwrap_or_else(|error| {
        problems.push(LinkProblem::CalibrationInvalid(error));
        Calibration::UNCALIBRATED
    })
}

fn read_dualsense_mac(device: &HidDevice) -> Result<MacAddress, OpenError> {
    let report = feature_report(
        device,
        dualsense::PAIRING_INFO_REPORT_ID,
        dualsense::PAIRING_INFO_LEN,
    )
    .map_err(hid_error)?;
    dualsense::parse_pairing_info(&report).map_err(address_error)
}

fn read_dualshock4_mac(device: &HidDevice) -> Result<MacAddress, OpenError> {
    let report = feature_report(
        device,
        dualshock4::PAIRING_INFO_REPORT_ID,
        DUALSHOCK4_FEATURE_BUFFER_LEN,
    )
    .map_err(hid_error)?;
    dualshock4::parse_pairing_info(&report).map_err(address_error)
}

pub fn controller_paths(api: &HidApi) -> Vec<CString> {
    controller_devices(api)
        .map(|info| info.path().to_owned())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    VirtualController,
    Refused(discovery::OpenError),
}

fn hid_error(error: HidError) -> OpenError {
    OpenError::Refused(discovery::OpenError::Hid(error.to_string()))
}

fn address_error(error: ReportError) -> OpenError {
    OpenError::Refused(discovery::OpenError::Address(error))
}

impl LinkReader for ControllerLink {
    fn format(&self) -> ReportFormat {
        self.format
    }

    fn read(&mut self, buf: &mut [u8]) -> LinkRead {
        match self.device.read_timeout(buf, READ_TIMEOUT_MS) {
            Ok(0) => LinkRead::Timeout,
            Ok(len) => LinkRead::Report(len),
            Err(_) => LinkRead::Lost,
        }
    }

    fn feedback_writer(&mut self) -> Option<Box<dyn LinkWriter>> {
        self.writer
            .take()
            .map(|writer| Box::new(writer) as Box<dyn LinkWriter>)
    }
}

pub struct ControllerWriter {
    device: HidDevice,
    format: FeedbackFormat,
}

impl LinkWriter for ControllerWriter {
    fn format(&self) -> FeedbackFormat {
        self.format.clone()
    }

    fn write(&mut self, report: &[u8]) -> Result<(), LinkWriteError> {
        self.device
            .write(report)
            .map(|_| ())
            .map_err(LinkWriteError::new)
    }
}
