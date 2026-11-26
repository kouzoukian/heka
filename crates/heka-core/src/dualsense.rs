//! DualSense and DualSense Edge input reports.
//!
//! Layout source: Linux `drivers/hid/hid-playstation.c` (`struct dualsense_input_report`,
//! the `DS_BUTTONS*` bit definitions and `ps_gamepad_hat_mapping`). Offsets below are
//! into the USB report as read from HID, so they are one more than the struct offsets
//! because byte 0 is the report ID (the kernel reads the struct from `&data[1]`).

use crate::state::{Buttons, ControllerState, Stick};

/// Sony's USB vendor ID.
pub const VENDOR_ID: u16 = 0x054C;
/// DualSense product ID.
pub const PRODUCT_ID_DUALSENSE: u16 = 0x0CE6;
/// DualSense Edge product ID (`hid-playstation.c`: "DualSense Edge (product 0x0df2)").
pub const PRODUCT_ID_DUALSENSE_EDGE: u16 = 0x0DF2;

/// Report ID of the USB input report (`DS_INPUT_REPORT_USB`).
pub const USB_INPUT_REPORT_ID: u8 = 0x01;
/// Length of the USB input report including its report ID (`DS_INPUT_REPORT_USB_SIZE`).
pub const USB_INPUT_REPORT_LEN: usize = 64;

/// True for a product ID that heka reads as a DualSense.
pub fn is_dualsense(vendor_id: u16, product_id: u16) -> bool {
    vendor_id == VENDOR_ID
        && matches!(product_id, PRODUCT_ID_DUALSENSE | PRODUCT_ID_DUALSENSE_EDGE)
}

// Byte offsets in the USB report (struct field offset + 1 for the report ID).
const LEFT_X: usize = 1; // x
const LEFT_Y: usize = 2; // y
const RIGHT_X: usize = 3; // rx
const RIGHT_Y: usize = 4; // ry
const LEFT_TRIGGER: usize = 5; // z
const RIGHT_TRIGGER: usize = 6; // rz
// Byte 7 is seq_number.
const BUTTONS_0: usize = 8; // buttons[0]
const BUTTONS_1: usize = 9; // buttons[1]
const BUTTONS_2: usize = 10; // buttons[2]

/// `DS_BUTTONS0_HAT_SWITCH`: the low nibble of buttons[0].
const HAT_MASK: u8 = 0x0F;

/// Bit masks from the `DS_BUTTONS0_*`, `DS_BUTTONS1_*` and `DS_BUTTONS2_*` definitions.
const BUTTON_BITS: [(usize, u8, Buttons); 15] = [
    (BUTTONS_0, 1 << 4, Buttons::SQUARE),
    (BUTTONS_0, 1 << 5, Buttons::CROSS),
    (BUTTONS_0, 1 << 6, Buttons::CIRCLE),
    (BUTTONS_0, 1 << 7, Buttons::TRIANGLE),
    (BUTTONS_1, 1 << 0, Buttons::L1),
    (BUTTONS_1, 1 << 1, Buttons::R1),
    (BUTTONS_1, 1 << 2, Buttons::L2),
    (BUTTONS_1, 1 << 3, Buttons::R2),
    (BUTTONS_1, 1 << 4, Buttons::CREATE),
    (BUTTONS_1, 1 << 5, Buttons::OPTIONS),
    (BUTTONS_1, 1 << 6, Buttons::L3),
    (BUTTONS_1, 1 << 7, Buttons::R3),
    (BUTTONS_2, 1 << 0, Buttons::PS),
    (BUTTONS_2, 1 << 1, Buttons::TOUCHPAD),
    (BUTTONS_2, 1 << 2, Buttons::MUTE),
];

/// The d-pad for a hat value, clockwise from 0 = up (`ps_gamepad_hat_mapping`).
/// Any value past 7 means released.
fn hat_to_dpad(hat: u8) -> Buttons {
    match hat {
        0 => Buttons::DPAD_UP,
        1 => Buttons::DPAD_UP | Buttons::DPAD_RIGHT,
        2 => Buttons::DPAD_RIGHT,
        3 => Buttons::DPAD_DOWN | Buttons::DPAD_RIGHT,
        4 => Buttons::DPAD_DOWN,
        5 => Buttons::DPAD_DOWN | Buttons::DPAD_LEFT,
        6 => Buttons::DPAD_LEFT,
        7 => Buttons::DPAD_UP | Buttons::DPAD_LEFT,
        _ => Buttons::NONE,
    }
}

/// Why a report could not be read as a DualSense USB input report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportError {
    /// The report is shorter than [`USB_INPUT_REPORT_LEN`].
    TooShort { len: usize },
    /// The report is not report [`USB_INPUT_REPORT_ID`].
    UnexpectedId { id: u8 },
}

/// Reads a DualSense USB input report, report ID byte included.
pub fn parse_usb_input_report(report: &[u8]) -> Result<ControllerState, ReportError> {
    if report.len() < USB_INPUT_REPORT_LEN {
        return Err(ReportError::TooShort { len: report.len() });
    }
    if report[0] != USB_INPUT_REPORT_ID {
        return Err(ReportError::UnexpectedId { id: report[0] });
    }
    let mut buttons = hat_to_dpad(report[BUTTONS_0] & HAT_MASK);
    for (byte, mask, button) in BUTTON_BITS {
        buttons.set(button, report[byte] & mask != 0);
    }
    Ok(ControllerState {
        buttons,
        left_stick: Stick {
            x: report[LEFT_X],
            y: report[LEFT_Y],
        },
        right_stick: Stick {
            x: report[RIGHT_X],
            y: report[RIGHT_Y],
        },
        left_trigger: report[LEFT_TRIGGER],
        right_trigger: report[RIGHT_TRIGGER],
    })
}
