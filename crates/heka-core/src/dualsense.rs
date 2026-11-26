//! DualSense and DualSense Edge input and output reports.
//!
//! Layout source: Linux `drivers/hid/hid-playstation.c` (`struct dualsense_input_report`,
//! the `DS_BUTTONS*` bit definitions and `ps_gamepad_hat_mapping`). Offsets below are
//! into the USB report as read from HID, so they are one more than the struct offsets
//! because byte 0 is the report ID (the kernel reads the struct from `&data[1]`).

use crate::calibration::{self, CALIBRATION_BLOCK_LEN, Calibration};
use crate::feedback::Feedback;
use crate::link::ReportError;
use crate::mac::MacAddress;
use crate::state::{Buttons, ControllerState, Finger, Motion, Stick};

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
    vendor_id == VENDOR_ID && matches!(product_id, PRODUCT_ID_DUALSENSE | PRODUCT_ID_DUALSENSE_EDGE)
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

/// Feature report holding the Controller's MAC address (`DS_FEATURE_REPORT_PAIRING_INFO`).
/// Reading it sends nothing to the Controller but the request.
pub const PAIRING_INFO_REPORT_ID: u8 = 0x09;
/// Length of the pairing-info feature report over USB, report ID included
/// (`DS_FEATURE_REPORT_PAIRING_INFO_SIZE`). Over Bluetooth it also carries a CRC.
pub const PAIRING_INFO_LEN: usize = 20;
/// Offset of the 6 MAC bytes: `dualsense_get_mac_address` copies them from `&buf[1]`,
/// least significant byte first (`mac_address` is "stored in little endian order").
const PAIRING_INFO_MAC: usize = 1;

/// Reads the MAC address from a pairing-info feature report, report ID byte included.
pub fn parse_pairing_info(report: &[u8]) -> Result<MacAddress, ReportError> {
    if report.len() < PAIRING_INFO_MAC + 6 {
        return Err(ReportError::TooShort { len: report.len() });
    }
    if report[0] != PAIRING_INFO_REPORT_ID {
        return Err(ReportError::UnexpectedId { id: report[0] });
    }
    let mut mac = [0u8; 6];
    mac.copy_from_slice(&report[PAIRING_INFO_MAC..PAIRING_INFO_MAC + 6]);
    // The label order is the reverse of the little-endian bytes (the kernel prints it with `%pMR`).
    mac.reverse();
    Ok(MacAddress(mac))
}

/// Feature report holding the motion sensors' factory calibration
/// (`DS_FEATURE_REPORT_CALIBRATION`). Reading it sends nothing to the Controller
/// but the request.
pub const CALIBRATION_REPORT_ID: u8 = 0x05;
/// Its length including the report ID (`DS_FEATURE_REPORT_CALIBRATION_SIZE`).
pub const CALIBRATION_REPORT_LEN: usize = 41;

/// Reads the calibration feature report, report ID byte included. Axes whose
/// data is unusable get the kernel's full-scale fallback.
pub fn parse_calibration(report: &[u8]) -> Result<Calibration, ReportError> {
    calibration::check_report(report, CALIBRATION_REPORT_ID, CALIBRATION_BLOCK_LEN)?;
    Ok(Calibration::from_paired_block(report))
}

const GYRO: usize = 16; // gyro[3], le16 each
const ACCEL: usize = 22; // accel[3], le16 each
const SENSOR_TIMESTAMP: usize = 28; // sensor_timestamp, le32
const TOUCH_POINTS: usize = 33; // points[2], 4 bytes each

/// `DS_TOUCH_POINT_INACTIVE`: set in a touch point's contact byte when no finger is down.
const TOUCH_INACTIVE: u8 = 1 << 7;

fn le16(report: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([report[offset], report[offset + 1]])
}

/// One touch point: contact, x_lo, x_hi (low nibble) | y_lo (high nibble), y_hi
/// (`struct dualsense_touch_point`, `DS_TOUCH_POINT_X`, `DS_TOUCH_POINT_Y`).
fn touch_point(point: &[u8]) -> Option<Finger> {
    if point[0] & TOUCH_INACTIVE != 0 {
        return None;
    }
    Some(Finger {
        id: point[0],
        x: u16::from(point[1]) | (u16::from(point[2] & 0x0F) << 8),
        y: u16::from(point[2] >> 4) | (u16::from(point[3]) << 4),
    })
}

/// Reads a DualSense USB input report, report ID byte included, applying the
/// Controller's motion calibration.
pub fn parse_usb_input_report(
    report: &[u8],
    calibration: &Calibration,
) -> Result<ControllerState, ReportError> {
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
    let gyro = std::array::from_fn(|i| le16(report, GYRO + 2 * i));
    let accel = std::array::from_fn(|i| le16(report, ACCEL + 2 * i));
    let timestamp = u32::from_le_bytes(
        report[SENSOR_TIMESTAMP..SENSOR_TIMESTAMP + 4]
            .try_into()
            .expect("four bytes"),
    );
    Ok(ControllerState {
        touch: std::array::from_fn(|i| {
            touch_point(&report[TOUCH_POINTS + 4 * i..TOUCH_POINTS + 4 * i + 4])
        }),
        motion: Motion {
            gyro: calibration.gyro(gyro),
            accel: calibration.accel(accel),
            timestamp,
        },
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

// Output report. Layout source: Linux `hid-playstation.c` (`struct
// dualsense_output_report_usb`, `struct dualsense_output_report_common`, the
// `DS_OUTPUT_VALID_FLAG*` bits and `dualsense_output_worker`). Offsets are into the
// USB report: the common block starts at byte 1, after the report ID.

/// Report ID of the USB output report (`DS_OUTPUT_REPORT_USB`).
pub const USB_OUTPUT_REPORT_ID: u8 = 0x02;
/// Length of the USB output report including its report ID (`DS_OUTPUT_REPORT_USB_SIZE`).
pub const USB_OUTPUT_REPORT_LEN: usize = 63;

const OUT_VALID_FLAG0: usize = 1; // common.valid_flag0
const OUT_MOTOR_RIGHT: usize = 3; // common.motor_right
const OUT_MOTOR_LEFT: usize = 4; // common.motor_left
const OUT_VALID_FLAG1: usize = 2; // common.valid_flag1
const OUT_VALID_FLAG2: usize = 39; // common.valid_flag2
const OUT_LIGHTBAR_RED: usize = 45; // common.lightbar_red
const OUT_LIGHTBAR_GREEN: usize = 46; // common.lightbar_green
const OUT_LIGHTBAR_BLUE: usize = 47; // common.lightbar_blue

const FLAG0_COMPATIBLE_VIBRATION: u8 = 1 << 0; // DS_OUTPUT_VALID_FLAG0_COMPATIBLE_VIBRATION
const FLAG0_HAPTICS_SELECT: u8 = 1 << 1; // DS_OUTPUT_VALID_FLAG0_HAPTICS_SELECT
const FLAG1_LIGHTBAR_CONTROL_ENABLE: u8 = 1 << 2; // DS_OUTPUT_VALID_FLAG1_LIGHTBAR_CONTROL_ENABLE
const FLAG2_COMPATIBLE_VIBRATION2: u8 = 1 << 2; // DS_OUTPUT_VALID_FLAG2_COMPATIBLE_VIBRATION2

/// How a DualSense imitates classic two-motor rumble with its haptic actuators.
///
/// The kernel notes that newer firmware made `CompatibleV2` the default on
/// PlayStation and other platforms because it feels like classic rumble, and that
/// both devices still accept `Compatible`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RumbleMode {
    /// The original rumble emulation, accepted by every firmware.
    Compatible,
    /// The revised emulation: DualSense update version 2.21 and later, every DualSense Edge.
    CompatibleV2,
}

/// Report ID of the firmware info feature report (`DS_FEATURE_REPORT_FIRMWARE_INFO`).
pub const FIRMWARE_INFO_REPORT_ID: u8 = 0x20;
/// Its length including the report ID (`DS_FEATURE_REPORT_FIRMWARE_INFO_SIZE`).
pub const FIRMWARE_INFO_REPORT_LEN: usize = 64;
/// Offset of the little-endian update version (`dualsense_get_firmware_info`).
const UPDATE_VERSION: usize = 44;
/// Update version 2.21, the first with v2 rumble (`DS_FEATURE_VERSION(2, 21)`).
const UPDATE_VERSION_V2_RUMBLE: u16 = 0x0215;

/// The rumble mode for a DualSense, from its product ID and its firmware info
/// feature report (report ID byte included), as `dualsense_create` decides it.
///
/// A DualSense whose firmware info cannot be read gets `Compatible`, which every
/// firmware accepts.
pub fn rumble_mode(product_id: u16, firmware_info: Option<&[u8]>) -> RumbleMode {
    if product_id == PRODUCT_ID_DUALSENSE_EDGE {
        return RumbleMode::CompatibleV2;
    }
    let update_version = firmware_info
        .filter(|info| info.len() >= FIRMWARE_INFO_REPORT_LEN && info[0] == FIRMWARE_INFO_REPORT_ID)
        .map(|info| u16::from_le_bytes([info[UPDATE_VERSION], info[UPDATE_VERSION + 1]]));
    match update_version {
        Some(version) if version >= UPDATE_VERSION_V2_RUMBLE => RumbleMode::CompatibleV2,
        _ => RumbleMode::Compatible,
    }
}

/// Builds the USB output report that sets the rumble motors and, when the
/// Feedback has one, the lightbar colour. Nothing else on the Controller changes.
///
/// The large (Xbox left) motor drives the DualSense left motor, as the kernel maps
/// a force-feedback effect's strong magnitude to `motor_left`. Over USB the
/// lightbar takes a colour without the setup step Bluetooth needs (the kernel's
/// `dualsense_reset_leds` comment).
pub fn usb_output_report(feedback: &Feedback, mode: RumbleMode) -> [u8; USB_OUTPUT_REPORT_LEN] {
    let mut report = [0u8; USB_OUTPUT_REPORT_LEN];
    report[0] = USB_OUTPUT_REPORT_ID;
    report[OUT_VALID_FLAG0] = FLAG0_HAPTICS_SELECT;
    match mode {
        RumbleMode::Compatible => report[OUT_VALID_FLAG0] |= FLAG0_COMPATIBLE_VIBRATION,
        RumbleMode::CompatibleV2 => report[OUT_VALID_FLAG2] |= FLAG2_COMPATIBLE_VIBRATION2,
    }
    report[OUT_MOTOR_LEFT] = feedback.rumble.large;
    report[OUT_MOTOR_RIGHT] = feedback.rumble.small;
    if let Some(lightbar) = feedback.lightbar {
        report[OUT_VALID_FLAG1] |= FLAG1_LIGHTBAR_CONTROL_ENABLE;
        report[OUT_LIGHTBAR_RED] = lightbar.red;
        report[OUT_LIGHTBAR_GREEN] = lightbar.green;
        report[OUT_LIGHTBAR_BLUE] = lightbar.blue;
    }
    report
}
