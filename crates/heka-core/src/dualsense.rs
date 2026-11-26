//! DualSense and DualSense Edge input and output reports.
//!
//! Layout source: Linux `drivers/hid/hid-playstation.c` (`struct dualsense_input_report`,
//! the `DS_BUTTONS*` bit definitions and `ps_gamepad_hat_mapping`). Offsets below are
//! into the USB report as read from HID, so they are one more than the struct offsets
//! because byte 0 is the report ID (the kernel reads the struct from `&data[1]`).

use crate::crc;
use crate::feedback::Feedback;
use crate::link::ReportError;
use crate::mac::MacAddress;
use crate::state::{Battery, Buttons, Charge, ControllerState, Stick};

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
const STATUS_0: usize = 53; // status[0]

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
        battery: Some(battery(report[STATUS_0])),
    })
}

/// Report ID of the full Bluetooth input report (`DS_INPUT_REPORT_BT`).
pub const BT_INPUT_REPORT_ID: u8 = 0x31;
/// Its length including the report ID and CRC (`DS_INPUT_REPORT_BT_SIZE`).
pub const BT_INPUT_REPORT_LEN: usize = 78;
/// Where `struct dualsense_input_report` starts: `dualsense_parse_report` reads it
/// from `&data[2]` over Bluetooth and from `&data[1]` over USB.
const BT_INPUT_COMMON: usize = 2;

/// Reads a DualSense full Bluetooth input report, report ID byte and CRC included.
///
/// The input itself is laid out as in the USB report, so it is read by
/// [`parse_usb_input_report`] from a USB-shaped copy: whatever that reads, a
/// Bluetooth Link reads too.
pub fn parse_bluetooth_input_report(report: &[u8]) -> Result<ControllerState, ReportError> {
    if report.len() < BT_INPUT_REPORT_LEN {
        return Err(ReportError::TooShort { len: report.len() });
    }
    if report[0] != BT_INPUT_REPORT_ID {
        return Err(ReportError::UnexpectedId { id: report[0] });
    }
    let report = &report[..BT_INPUT_REPORT_LEN];
    if !crc::has_valid_crc(crc::BT_INPUT_SEED, report) {
        return Err(ReportError::BadCrc);
    }
    let mut usb = [0u8; USB_INPUT_REPORT_LEN];
    usb[0] = USB_INPUT_REPORT_ID;
    usb[1..].copy_from_slice(&report[BT_INPUT_COMMON..BT_INPUT_COMMON + USB_INPUT_REPORT_LEN - 1]);
    parse_usb_input_report(&usb)
}

/// Reads `status[0]` as `dualsense_parse_report` does.
fn battery(status: u8) -> Battery {
    let capacity = status & 0x0F; // DS_STATUS0_BATTERY_CAPACITY
    let level = (capacity * 10 + 5).min(100);
    match status >> 4 {
        // DS_STATUS0_CHARGING
        0x0 => Battery {
            percent: level,
            charge: Charge::Discharging,
        },
        0x1 => Battery {
            percent: level,
            charge: Charge::Charging,
        },
        0x2 => Battery {
            percent: 100,
            charge: Charge::Full,
        },
        // Voltage or temperature out of range, or a temperature error.
        0xA | 0xB => Battery {
            percent: 0,
            charge: Charge::NotCharging,
        },
        // 0xF is a charging error.
        _ => Battery {
            percent: 0,
            charge: Charge::Unknown,
        },
    }
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
const OUT_VALID_FLAG2: usize = 39; // common.valid_flag2

const FLAG0_COMPATIBLE_VIBRATION: u8 = 1 << 0; // DS_OUTPUT_VALID_FLAG0_COMPATIBLE_VIBRATION
const FLAG0_HAPTICS_SELECT: u8 = 1 << 1; // DS_OUTPUT_VALID_FLAG0_HAPTICS_SELECT
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

/// Builds the USB output report that sets the rumble motors and nothing else.
///
/// The large (Xbox left) motor drives the DualSense left motor, as the kernel maps
/// a force-feedback effect's strong magnitude to `motor_left`.
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
    report
}

/// Report ID of the Bluetooth output report (`DS_OUTPUT_REPORT_BT`).
pub const BT_OUTPUT_REPORT_ID: u8 = 0x31;
/// Its length including the report ID and CRC (`DS_OUTPUT_REPORT_BT_SIZE`).
pub const BT_OUTPUT_REPORT_LEN: usize = 78;
/// The tag byte `dualsense_init_output_report` always sets (`DS_OUTPUT_TAG`).
const BT_OUTPUT_TAG: u8 = 0x10;
const OUT_BT_SEQ_TAG: usize = 1; // seq_tag: sequence number in bits 4..8 (`DS_OUTPUT_SEQ_NO`)
const OUT_BT_TAG: usize = 2; // tag
/// Where `struct dualsense_output_report_common` starts: after the report ID and two header bytes.
const OUT_BT_COMMON: usize = 3;
/// `sizeof(struct dualsense_output_report_common)`.
const OUT_COMMON_LEN: usize = 47;
/// How many sequence numbers a Bluetooth output report cycles through.
pub const BT_OUTPUT_SEQUENCE_LEN: u8 = 16;

/// Builds the Bluetooth output report that shows this Feedback, with sequence
/// number `seq` (`0..16`) and its CRC.
///
/// The common block is the one [`usb_output_report`] builds, so whatever that
/// sets on the Controller a Bluetooth Link sets too.
pub fn bluetooth_output_report(
    feedback: &Feedback,
    mode: RumbleMode,
    seq: u8,
) -> [u8; BT_OUTPUT_REPORT_LEN] {
    let usb = usb_output_report(feedback, mode);
    let mut report = [0u8; BT_OUTPUT_REPORT_LEN];
    report[0] = BT_OUTPUT_REPORT_ID;
    report[OUT_BT_SEQ_TAG] = (seq % BT_OUTPUT_SEQUENCE_LEN) << 4;
    report[OUT_BT_TAG] = BT_OUTPUT_TAG;
    report[OUT_BT_COMMON..OUT_BT_COMMON + OUT_COMMON_LEN]
        .copy_from_slice(&usb[1..=OUT_COMMON_LEN]);
    crc::seal(crc::BT_OUTPUT_SEED, &mut report);
    report
}
