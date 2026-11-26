//! The Link seam: where raw input reports enter heka.

use std::fmt;

use crate::dualsense;
use crate::state::ControllerState;

/// Which report layout a Link delivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReportFormat {
    /// DualSense or DualSense Edge over USB, report `0x01`.
    DualSenseUsb,
}

impl ReportFormat {
    /// Reads one input report of this format, report ID byte included.
    pub fn parse(self, report: &[u8]) -> Result<ControllerState, ReportError> {
        match self {
            Self::DualSenseUsb => dualsense::parse_usb_input_report(report),
        }
    }
}

/// Why a report could not be read as an input report of the expected format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportError {
    /// The report is shorter than the format's input report.
    TooShort { len: usize },
    /// The report has a different report ID, for example a feature or other input report.
    UnexpectedId { id: u8 },
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { len } => write!(f, "input report too short ({len} bytes)"),
            Self::UnexpectedId { id } => write!(f, "unexpected report ID {id:#04x}"),
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
}
