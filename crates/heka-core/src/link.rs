//! The Link seam: where raw input reports enter heka and Feedback leaves it.

use std::fmt;

use crate::dualsense::{self, RumbleMode};
use crate::feedback::Feedback;
use crate::state::ControllerState;

/// Which report layout a Link delivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReportFormat {
    /// DualSense or DualSense Edge over USB, report `0x01`.
    DualSenseUsb,
    /// DualSense or DualSense Edge over Bluetooth, full report `0x31`.
    DualSenseBluetooth,
}

impl ReportFormat {
    /// Reads one input report of this format, report ID byte included.
    pub fn parse(self, report: &[u8]) -> Result<ControllerState, ReportError> {
        match self {
            Self::DualSenseUsb => dualsense::parse_usb_input_report(report),
            Self::DualSenseBluetooth => dualsense::parse_bluetooth_input_report(report),
        }
    }

    /// Whether the Link delivering this format is USB or Bluetooth.
    pub fn kind(self) -> LinkKind {
        match self {
            Self::DualSenseUsb => LinkKind::Usb,
            Self::DualSenseBluetooth => LinkKind::Bluetooth,
        }
    }
}

/// How a Link reaches the PC. When a Controller has both, USB is the Active Link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LinkKind {
    Usb,
    Bluetooth,
}

/// Which of a Controller's Links heka reads from, given each Link's kind in
/// arrival order: the first USB Link, otherwise the first Link.
pub fn active_link(links: &[LinkKind]) -> Option<usize> {
    links
        .iter()
        .position(|&kind| kind == LinkKind::Usb)
        .or(if links.is_empty() { None } else { Some(0) })
}

/// Why a report could not be read as an input report of the expected format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportError {
    /// The report is shorter than the format's input report.
    TooShort { len: usize },
    /// The report has a different report ID, for example a feature or other input report.
    UnexpectedId { id: u8 },
    /// A Bluetooth report whose CRC does not match its contents.
    BadCrc,
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { len } => write!(f, "input report too short ({len} bytes)"),
            Self::UnexpectedId { id } => write!(f, "unexpected report ID {id:#04x}"),
            Self::BadCrc => f.write_str("report CRC does not match"),
        }
    }
}

impl std::error::Error for ReportError {}

/// The outcome of one read from a Link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkRead {
    /// A report of this many bytes was written to the start of the buffer.
    Report(usize),
    /// Nothing arrived in time. The Link is still up.
    Timeout,
    /// The Link is gone, for example because the cable was pulled.
    Lost,
}

/// Reads input reports from one Link. Implemented over HID in `heka-windows`.
pub trait LinkReader: Send {
    /// The layout of the reports this Link delivers.
    fn format(&self) -> ReportFormat;

    /// Waits a short, bounded time for the next input report, so that the caller
    /// can notice a stop request between reads.
    fn read(&mut self, buf: &mut [u8]) -> LinkRead;

    /// Hands over the way to send Feedback to the Controller over this Link, for
    /// another thread to own. `None` when the Link carries no Feedback, or once
    /// it has been taken.
    fn feedback_writer(&mut self) -> Option<Box<dyn LinkWriter>> {
        None
    }
}

/// Which output report layout a Link takes Feedback in, with whatever a Link must
/// remember between reports.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FeedbackFormat {
    /// DualSense or DualSense Edge over USB, report `0x02`, rumbling in this mode.
    DualSenseUsb(RumbleMode),
    /// DualSense or DualSense Edge over Bluetooth, report `0x31`, rumbling in this
    /// mode. Each report carries the next sequence number, `seq`, and a CRC32.
    DualSenseBluetooth { mode: RumbleMode, seq: u8 },
}

impl FeedbackFormat {
    /// A DualSense over Bluetooth, starting from sequence number 0.
    pub fn dualsense_bluetooth(mode: RumbleMode) -> Self {
        Self::DualSenseBluetooth { mode, seq: 0 }
    }

    /// The next output report that shows this Feedback on the Controller, report ID included.
    pub fn report(&mut self, feedback: &Feedback) -> Vec<u8> {
        match self {
            Self::DualSenseUsb(mode) => dualsense::usb_output_report(feedback, *mode).to_vec(),
            Self::DualSenseBluetooth { mode, seq } => {
                let report = dualsense::bluetooth_output_report(feedback, *mode, *seq);
                *seq = (*seq + 1) % dualsense::BT_OUTPUT_SEQUENCE_LEN;
                report.to_vec()
            }
        }
    }
}

/// Sends output reports to the Controller over one Link. Implemented over HID in
/// `heka-windows`. A write may block, so it never runs on the input thread.
pub trait LinkWriter: Send {
    /// The layout of the output reports this Link takes.
    fn format(&self) -> FeedbackFormat;

    /// Sends one output report, report ID included.
    fn write(&mut self, report: &[u8]) -> Result<(), LinkWriteError>;
}

/// An output report did not reach the Controller, carrying the backend's own error.
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
