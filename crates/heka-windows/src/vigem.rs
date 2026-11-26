//! The ViGEmBus backend for Virtual Controllers (ADR 0001), through `vigem-rust`.
//!
//! This only moves reports: what they contain is decided in `heka-core`.

use heka_core::state::ControllerState;
use heka_core::virtual_controller::{OutputType, VirtualBus, VirtualController, VirtualError};
use heka_core::xbox::XboxReport;
use vigem_rust::{Client, Ready, TargetHandle, X360Button, X360Report, Xbox360};

/// A connection to ViGEmBus. Every Virtual Controller it plugs is removed when
/// the Virtual Controller is unplugged, or at the latest when this is dropped.
pub struct VigemBus {
    client: Client,
}

impl VigemBus {
    /// Opens ViGEmBus. Fails when the driver is not installed or not running.
    pub fn connect() -> Result<Self, VirtualError> {
        let client = Client::connect().map_err(VirtualError::new)?;
        Ok(Self { client })
    }
}

impl VirtualBus for VigemBus {
    fn plug(&self, output: OutputType) -> Result<Box<dyn VirtualController>, VirtualError> {
        match output {
            OutputType::Xbox => {
                let target = self
                    .client
                    .new_x360_target()
                    .plug()
                    .and_then(|pending| pending.wait_for_ready())
                    .map_err(VirtualError::new)?;
                Ok(Box::new(VigemXbox { target }))
            }
        }
    }
}

struct VigemXbox {
    target: TargetHandle<Xbox360, Ready>,
}

impl VirtualController for VigemXbox {
    fn output_type(&self) -> OutputType {
        OutputType::Xbox
    }

    fn update(&mut self, state: &ControllerState) -> Result<(), VirtualError> {
        self.target
            .update(&x360_report(&XboxReport::from_state(state)))
            .map_err(VirtualError::new)
    }

    fn unplug(self: Box<Self>) -> Result<(), VirtualError> {
        self.target.unplug().map_err(VirtualError::new)
    }
}

/// heka-core's Xbox report uses the same XUSB bit values as `X360Button`.
fn x360_report(report: &XboxReport) -> X360Report {
    X360Report {
        buttons: X360Button::from_bits_retain(report.buttons),
        left_trigger: report.left_trigger,
        right_trigger: report.right_trigger,
        thumb_lx: report.thumb_lx,
        thumb_ly: report.thumb_ly,
        thumb_rx: report.thumb_rx,
        thumb_ry: report.thumb_ry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use heka_core::xbox::buttons;

    #[test]
    fn xbox_button_bits_match_vigem_rust() {
        let pairs = [
            (buttons::DPAD_UP, X360Button::DPAD_UP),
            (buttons::DPAD_DOWN, X360Button::DPAD_DOWN),
            (buttons::DPAD_LEFT, X360Button::DPAD_LEFT),
            (buttons::DPAD_RIGHT, X360Button::DPAD_RIGHT),
            (buttons::START, X360Button::START),
            (buttons::BACK, X360Button::BACK),
            (buttons::LEFT_THUMB, X360Button::LEFT_THUMB),
            (buttons::RIGHT_THUMB, X360Button::RIGHT_THUMB),
            (buttons::LEFT_SHOULDER, X360Button::LEFT_SHOULDER),
            (buttons::RIGHT_SHOULDER, X360Button::RIGHT_SHOULDER),
            (buttons::GUIDE, X360Button::GUIDE),
            (buttons::A, X360Button::A),
            (buttons::B, X360Button::B),
            (buttons::X, X360Button::X),
            (buttons::Y, X360Button::Y),
        ];
        for (ours, theirs) in pairs {
            assert_eq!(ours, theirs.bits());
        }
    }
}
