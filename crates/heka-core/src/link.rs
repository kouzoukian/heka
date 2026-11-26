//! The Link seam: where raw input reports enter heka.

/// Which report layout a Link delivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReportFormat {
    /// DualSense or DualSense Edge over USB, report `0x01`.
    DualSenseUsb,
}

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

    /// Waits a short, bounded time for the next input report.
    fn read(&mut self, buf: &mut [u8]) -> LinkRead;
}
