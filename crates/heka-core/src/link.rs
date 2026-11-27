use std::fmt;

use crate::calibration::Calibration;
use crate::dualsense::{self, RumbleMode};
use crate::dualshock4;
use crate::feedback::Feedback;
use crate::state::ControllerState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ControllerModel {
    #[default]
    DualSense,
    DualShock4,
}

pub fn controller_model(vendor_id: u16, product_id: u16) -> Option<ControllerModel> {
    if dualsense::is_dualsense(vendor_id, product_id) {
        Some(ControllerModel::DualSense)
    } else if dualshock4::is_dualshock4(vendor_id, product_id) {
        Some(ControllerModel::DualShock4)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReportFormat {
    DualSenseUsb(Calibration),
    DualSenseBluetooth(Calibration),
    DualSenseEdgeUsb(Calibration),
    DualSenseEdgeBluetooth(Calibration),
    DualShock4Usb(Calibration),
    DualShock4Bluetooth(Calibration),
}

impl ReportFormat {
    pub fn dualsense(product_id: u16, kind: LinkKind, calibration: Calibration) -> Self {
        let edge = product_id == dualsense::PRODUCT_ID_DUALSENSE_EDGE;
        match (edge, kind) {
            (false, LinkKind::Usb) => Self::DualSenseUsb(calibration),
            (false, LinkKind::Bluetooth) => Self::DualSenseBluetooth(calibration),
            (true, LinkKind::Usb) => Self::DualSenseEdgeUsb(calibration),
            (true, LinkKind::Bluetooth) => Self::DualSenseEdgeBluetooth(calibration),
        }
    }

    pub fn parse(self, report: &[u8]) -> Result<ControllerState, ReportError> {
        match self {
            Self::DualSenseUsb(calibration) => {
                dualsense::parse_usb_input_report(report, &calibration)
            }
            Self::DualSenseBluetooth(calibration) => {
                dualsense::parse_bluetooth_input_report(report, &calibration)
            }
            Self::DualSenseEdgeUsb(calibration) => {
                dualsense::parse_edge_usb_input_report(report, &calibration)
            }
            Self::DualSenseEdgeBluetooth(calibration) => {
                dualsense::parse_edge_bluetooth_input_report(report, &calibration)
            }
            Self::DualShock4Usb(calibration) => {
                dualshock4::parse_usb_input_report(report, &calibration)
            }
            Self::DualShock4Bluetooth(calibration) => {
                dualshock4::parse_bluetooth_input_report(report, &calibration)
            }
        }
    }

    pub fn kind(self) -> LinkKind {
        match self {
            Self::DualSenseUsb(_) | Self::DualSenseEdgeUsb(_) | Self::DualShock4Usb(_) => {
                LinkKind::Usb
            }
            Self::DualSenseBluetooth(_)
            | Self::DualSenseEdgeBluetooth(_)
            | Self::DualShock4Bluetooth(_) => LinkKind::Bluetooth,
        }
    }

    pub fn model(self) -> ControllerModel {
        match self {
            Self::DualSenseUsb(_)
            | Self::DualSenseBluetooth(_)
            | Self::DualSenseEdgeUsb(_)
            | Self::DualSenseEdgeBluetooth(_) => ControllerModel::DualSense,
            Self::DualShock4Usb(_) | Self::DualShock4Bluetooth(_) => ControllerModel::DualShock4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LinkKind {
    Usb,
    Bluetooth,
}

pub fn active_link(links: &[LinkKind]) -> Option<usize> {
    links
        .iter()
        .position(|&kind| kind == LinkKind::Usb)
        .or(if links.is_empty() { None } else { Some(0) })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportError {
    TooShort { len: usize },
    UnexpectedId { id: u8 },
    BadCrc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkProblem {
    NoFeedback(String),
    CalibrationUnread(Option<String>),
    CalibrationInvalid(ReportError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkRead {
    Report(usize),
    Timeout,
    Lost,
}

pub trait LinkReader: Send {
    fn format(&self) -> ReportFormat;

    fn read(&mut self, buf: &mut [u8]) -> LinkRead;

    fn feedback_writer(&mut self) -> Option<Box<dyn LinkWriter>> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FeedbackFormat {
    DualSenseUsb(RumbleMode),
    DualSenseBluetooth {
        mode: RumbleMode,
        seq: u8,
        lightbar_ready: bool,
    },
    DualShock4Usb,
    DualShock4Bluetooth,
}

impl FeedbackFormat {
    pub fn dualsense_bluetooth(mode: RumbleMode) -> Self {
        Self::DualSenseBluetooth {
            mode,
            seq: 0,
            lightbar_ready: false,
        }
    }

    pub fn reports(&mut self, feedback: &Feedback) -> Vec<Vec<u8>> {
        match self {
            Self::DualSenseUsb(mode) => {
                vec![dualsense::usb_output_report(feedback, *mode).to_vec()]
            }
            Self::DualSenseBluetooth {
                mode,
                seq,
                lightbar_ready,
            } => {
                let mut next_seq = || {
                    let current = *seq;
                    *seq = (*seq + 1) % dualsense::BT_OUTPUT_SEQUENCE_LEN;
                    current
                };
                let mut reports = Vec::with_capacity(2);
                if feedback.lightbar.is_some() && !*lightbar_ready {
                    reports.push(dualsense::bluetooth_lightbar_setup_report(next_seq()).to_vec());
                }
                reports
                    .push(dualsense::bluetooth_output_report(feedback, *mode, next_seq()).to_vec());
                reports
            }
            Self::DualShock4Usb => vec![dualshock4::usb_output_report(feedback).to_vec()],
            Self::DualShock4Bluetooth => {
                vec![dualshock4::bluetooth_output_report(feedback).to_vec()]
            }
        }
    }

    pub fn written(&mut self, report: &[u8]) {
        if let Self::DualSenseBluetooth { lightbar_ready, .. } = self
            && dualsense::is_bluetooth_lightbar_setup_report(report)
        {
            *lightbar_ready = true;
        }
    }
}

pub trait LinkWriter: Send {
    fn format(&self) -> FeedbackFormat;

    fn write(&mut self, report: &[u8]) -> Result<(), LinkWriteError>;
}

#[derive(Debug)]
pub struct LinkWriteError(Box<dyn std::error::Error + Send + Sync>);

impl LinkWriteError {
    pub fn new(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self(error.into())
    }
}

impl fmt::Display for LinkWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for LinkWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}
