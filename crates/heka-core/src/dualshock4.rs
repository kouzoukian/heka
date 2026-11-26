//! DualShock 4 (both hardware revisions) input, feature and output reports.
//! Experimental: written from public sources and never tried on a DualShock 4.
//!
//! Layout source: Linux `drivers/hid/hid-playstation.c` at commit `4dd10fbbeb9c`
//! (the `struct dualshock4_*` report layouts, the `DS4_*` definitions,
//! `dualshock4_parse_report`, `dualshock4_get_calibration_data` and
//! `dualshock4_get_mac_address`), and SDL `src/joystick/hidapi/SDL_hidapi_ps4.c`
//! at commit `3ebae2a704f9` where named. Offsets are into the report as read from
//! HID, report ID byte included.

use crate::calibration::{self, Calibration};
use crate::crc;
use crate::dualsense;
use crate::feedback::Feedback;
use crate::link::ReportError;
use crate::mac::{InvalidMacAddress, MacAddress};
use crate::state::{
    Battery, Buttons, Charge, ControllerState, DUALSENSE_TOUCHPAD_HEIGHT,
    DUALSHOCK4_TOUCHPAD_HEIGHT, Finger, Motion, Stick, rescale_touch_y,
};

/// Product ID of the first DualShock 4 (`USB_DEVICE_ID_SONY_PS4_CONTROLLER` in
/// Linux `drivers/hid/hid-ids.h`). ViGEmBus's virtual DualShock 4 uses it too.
pub const PRODUCT_ID_DUALSHOCK4: u16 = 0x05C4;
/// Product ID of the second hardware revision (`USB_DEVICE_ID_SONY_PS4_CONTROLLER_2`).
pub const PRODUCT_ID_DUALSHOCK4_V2: u16 = 0x09CC;

/// True for a product ID that heka reads as a DualShock 4.
pub fn is_dualshock4(vendor_id: u16, product_id: u16) -> bool {
    vendor_id == dualsense::VENDOR_ID
        && matches!(product_id, PRODUCT_ID_DUALSHOCK4 | PRODUCT_ID_DUALSHOCK4_V2)
}

/// Feature report holding the Controller's MAC address over USB
/// (`DS4_FEATURE_REPORT_PAIRING_INFO`). Reading it sends nothing to the Controller
/// but the request. Over Bluetooth use [`parse_bluetooth_serial`].
pub const PAIRING_INFO_REPORT_ID: u8 = 0x12;
/// Its length including the report ID (`DS4_FEATURE_REPORT_PAIRING_INFO_SIZE`).
pub const PAIRING_INFO_LEN: usize = 16;

/// Reads the MAC address from a pairing-info feature report, report ID byte included.
pub fn parse_pairing_info(report: &[u8]) -> Result<MacAddress, ReportError> {
    dualsense::mac_after_report_id(report, PAIRING_INFO_REPORT_ID)
}

/// The MAC address in the HID serial number of a DualShock 4 on Bluetooth: 12 hex
/// digits in the order printed on the label. The kernel too takes the address from
/// the Bluetooth stack there, not from a feature report
/// (`dualshock4_get_mac_address`; SDL `HIDAPI_DriverPS4_InitDevice` reads the
/// serial number so).
pub fn parse_bluetooth_serial(serial: &str) -> Result<MacAddress, InvalidMacAddress> {
    if serial.len() != 12 || !serial.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(InvalidMacAddress);
    }
    match u64::from_str_radix(serial, 16) {
        // All zeros is no address.
        Ok(0) | Err(_) => Err(InvalidMacAddress),
        Ok(address) => {
            let [_, _, bytes @ ..] = address.to_be_bytes();
            Ok(MacAddress(bytes))
        }
    }
}

// Output reports. Layout: `struct dualshock4_output_report_common`, which both
// Links carry, and the `DS4_OUTPUT_*` definitions.

/// Report ID of the USB output report (`DS4_OUTPUT_REPORT_USB`).
pub const USB_OUTPUT_REPORT_ID: u8 = 0x05;
/// Its length including the report ID (`DS4_OUTPUT_REPORT_USB_SIZE`).
pub const USB_OUTPUT_REPORT_LEN: usize = 32;
/// Where the common block starts over USB.
const USB_OUTPUT_COMMON: usize = 1;

/// Report ID of the Bluetooth output report (`DS4_OUTPUT_REPORT_BT`).
pub const BT_OUTPUT_REPORT_ID: u8 = 0x11;
/// Its length including the report ID and CRC (`DS4_OUTPUT_REPORT_BT_SIZE`).
pub const BT_OUTPUT_REPORT_LEN: usize = 78;
/// Where the common block starts over Bluetooth: after the report ID,
/// `hw_control` and `audio_control`.
const BT_OUTPUT_COMMON: usize = 3;
const OUT_BT_HW_CONTROL: usize = 1; // hw_control
/// The report carries HID data and ends in a CRC (`DS4_OUTPUT_HWCTL_HID`,
/// `DS4_OUTPUT_HWCTL_CRC32`).
const HW_CONTROL_HID_AND_CRC: u8 = 0x80 | 0x40;
/// The milliseconds between the DualShock 4's Bluetooth input reports, in the low
/// bits of `hw_control` (`DS4_BT_DEFAULT_POLL_INTERVAL_MS`). Every report states
/// it, as SDL's do, because 0 there means 1 ms.
const BT_POLL_INTERVAL_MS: u8 = 4;

// Offsets into `struct dualshock4_output_report_common`.
const OUT_VALID_FLAG0: usize = 0; // valid_flag0
const OUT_MOTOR_RIGHT: usize = 3; // motor_right
const OUT_MOTOR_LEFT: usize = 4; // motor_left
const OUT_LIGHTBAR: usize = 5; // lightbar_red, lightbar_green, lightbar_blue

const FLAG0_MOTOR: u8 = 0x01; // DS4_OUTPUT_VALID_FLAG0_MOTOR
const FLAG0_LED: u8 = 0x02; // DS4_OUTPUT_VALID_FLAG0_LED

/// Builds the USB output report that sets the rumble motors and, when the
/// Feedback has one, the lightbar colour. Nothing else on the Controller changes.
pub fn usb_output_report(feedback: &Feedback) -> [u8; USB_OUTPUT_REPORT_LEN] {
    let mut report = [0u8; USB_OUTPUT_REPORT_LEN];
    report[0] = USB_OUTPUT_REPORT_ID;
    write_feedback(&mut report[USB_OUTPUT_COMMON..], feedback);
    report
}

/// Builds the Bluetooth output report that shows this Feedback, as
/// [`usb_output_report`] does, with its CRC.
pub fn bluetooth_output_report(feedback: &Feedback) -> [u8; BT_OUTPUT_REPORT_LEN] {
    let mut report = [0u8; BT_OUTPUT_REPORT_LEN];
    report[0] = BT_OUTPUT_REPORT_ID;
    report[OUT_BT_HW_CONTROL] = HW_CONTROL_HID_AND_CRC | BT_POLL_INTERVAL_MS;
    write_feedback(&mut report[BT_OUTPUT_COMMON..], feedback);
    crc::seal(crc::BT_OUTPUT_SEED, &mut report);
    report
}

/// Fills in the common block. The large (Xbox left) motor drives the DualShock 4's
/// left motor, as `dualshock4_play_effect` maps the strong magnitude.
fn write_feedback(common: &mut [u8], feedback: &Feedback) {
    common[OUT_VALID_FLAG0] = FLAG0_MOTOR;
    common[OUT_MOTOR_LEFT] = feedback.rumble.large;
    common[OUT_MOTOR_RIGHT] = feedback.rumble.small;
    if let Some(lightbar) = feedback.lightbar {
        common[OUT_VALID_FLAG0] |= FLAG0_LED;
        common[OUT_LIGHTBAR..OUT_LIGHTBAR + 3].copy_from_slice(&[
            lightbar.red,
            lightbar.green,
            lightbar.blue,
        ]);
    }
}

/// Report ID of the USB input report (`DS4_INPUT_REPORT_USB`).
pub const USB_INPUT_REPORT_ID: u8 = 0x01;
/// Its length including the report ID (`DS4_INPUT_REPORT_USB_SIZE`).
pub const USB_INPUT_REPORT_LEN: usize = 64;

/// Where `struct dualshock4_input_report_common` starts over USB.
const USB_INPUT_COMMON: usize = 1;
/// `num_touch_reports`, which follows the common block; the touch reports follow it.
const USB_TOUCH_COUNT: usize = 33;
/// How many touch reports a USB input report holds.
const USB_TOUCH_REPORTS: usize = 3;

// Offsets into `struct dualshock4_input_report_common`.
const LEFT_X: usize = 0; // x
const LEFT_Y: usize = 1; // y
const RIGHT_X: usize = 2; // rx
const RIGHT_Y: usize = 3; // ry
const BUTTONS: usize = 4; // buttons[3]
const LEFT_TRIGGER: usize = 7; // z
const RIGHT_TRIGGER: usize = 8; // rz
const SENSOR_TIMESTAMP: usize = 9; // sensor_timestamp, le16
const GYRO: usize = 12; // gyro[3], le16 each
const ACCEL: usize = 18; // accel[3], le16 each
const STATUS_0: usize = 29; // status[0]

/// `struct dualshock4_touch_report`: a timestamp byte, then two 4-byte points.
const TOUCH_REPORT_LEN: usize = 9;

/// Feature report holding the motion sensors' factory calibration over USB
/// (`DS4_FEATURE_REPORT_CALIBRATION`). Reading it sends nothing to the Controller
/// but the request.
pub const USB_CALIBRATION_REPORT_ID: u8 = 0x02;
/// Its length including the report ID (`DS4_FEATURE_REPORT_CALIBRATION_SIZE`).
pub const USB_CALIBRATION_REPORT_LEN: usize = 37;

/// Reads the USB calibration feature report, report ID byte included. Axes whose
/// data is unusable get the kernel's full-scale fallback.
pub fn parse_usb_calibration(report: &[u8]) -> Result<Calibration, ReportError> {
    calibration::check_report(
        report,
        USB_CALIBRATION_REPORT_ID,
        USB_CALIBRATION_REPORT_LEN,
    )?;
    Ok(Calibration::from_paired_block(report))
}

/// Reads a DualShock 4 USB input report, report ID byte included, applying the
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
    Ok(state(
        &report[USB_INPUT_COMMON..],
        &report[USB_TOUCH_COUNT..],
        USB_TOUCH_REPORTS,
        calibration,
    ))
}

/// Report ID of the full Bluetooth input report (`DS4_INPUT_REPORT_BT`).
pub const BT_INPUT_REPORT_ID: u8 = 0x11;
/// Its length including the report ID and CRC (`DS4_INPUT_REPORT_BT_SIZE`).
pub const BT_INPUT_REPORT_LEN: usize = 78;
/// Where the common block starts: after the report ID and two header bytes
/// (`struct dualshock4_input_report_bt`).
const BT_INPUT_COMMON: usize = 3;
const BT_TOUCH_COUNT: usize = 35;
/// How many touch reports a Bluetooth input report holds.
const BT_TOUCH_REPORTS: usize = 4;

/// Reads a DualShock 4 full Bluetooth input report, report ID byte and CRC
/// included. Anything after the report, such as the padding Windows adds, is ignored.
pub fn parse_bluetooth_input_report(
    report: &[u8],
    calibration: &Calibration,
) -> Result<ControllerState, ReportError> {
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
    Ok(state(
        &report[BT_INPUT_COMMON..],
        &report[BT_TOUCH_COUNT..],
        BT_TOUCH_REPORTS,
        calibration,
    ))
}

/// Feature report holding the calibration over Bluetooth
/// (`DS4_FEATURE_REPORT_CALIBRATION_BT`). Reading it sends nothing to the
/// Controller but the request, and switches the DualShock 4 from its short input
/// report to the full one (SDL `SDL_hidapi_ps4.c`: reading a calibration report
/// switches Bluetooth controllers "into advanced report mode").
pub const BT_CALIBRATION_REPORT_ID: u8 = 0x05;
/// Its length including the report ID and CRC (`DS4_FEATURE_REPORT_CALIBRATION_BT_SIZE`).
pub const BT_CALIBRATION_REPORT_LEN: usize = 41;

/// Reads the Bluetooth calibration feature report, report ID byte and CRC included.
pub fn parse_bluetooth_calibration(report: &[u8]) -> Result<Calibration, ReportError> {
    calibration::check_report(report, BT_CALIBRATION_REPORT_ID, BT_CALIBRATION_REPORT_LEN)?;
    let report = &report[..BT_CALIBRATION_REPORT_LEN];
    // `ps_get_report` checks the CRC of this report over Bluetooth.
    if !crc::has_valid_crc(crc::BT_FEATURE_SEED, report) {
        return Err(ReportError::BadCrc);
    }
    Ok(Calibration::from_grouped_block(report))
}

/// The state in the parts both Links share: the common block, and the touch report
/// count followed by room for `capacity` touch reports.
fn state(
    common: &[u8],
    touch: &[u8],
    capacity: usize,
    calibration: &Calibration,
) -> ControllerState {
    let le16 = |at: usize| i16::from_le_bytes([common[at], common[at + 1]]);
    let gyro = std::array::from_fn(|i| le16(GYRO + 2 * i));
    let accel = std::array::from_fn(|i| le16(ACCEL + 2 * i));
    let timestamp = u16::from_le_bytes([common[SENSOR_TIMESTAMP], common[SENSOR_TIMESTAMP + 1]]);
    let mut buttons = dualsense::buttons(&common[BUTTONS..BUTTONS + 3]);
    // The bit a DualSense uses for mute belongs to a DualShock 4's report counter.
    buttons.set(Buttons::MUTE, false);
    ControllerState {
        buttons,
        left_stick: Stick {
            x: common[LEFT_X],
            y: common[LEFT_Y],
        },
        right_stick: Stick {
            x: common[RIGHT_X],
            y: common[RIGHT_Y],
        },
        left_trigger: common[LEFT_TRIGGER],
        right_trigger: common[RIGHT_TRIGGER],
        battery: Some(battery(common[STATUS_0])),
        touch: newest_touch(touch, capacity),
        motion: Motion {
            gyro: calibration.gyro(gyro),
            accel: calibration.accel(accel),
            // One DualShock 4 tick is 16/3 µs, one state tick 1/3 µs.
            timestamp: u32::from(timestamp) << 4,
        },
    }
}

/// The fingers of the newest touch report, which is the last one counted (the
/// kernel applies them in order). With none counted the first is read, as SDL does
/// whatever the count (`SDL_hidapi_ps4.c`).
fn newest_touch(touch: &[u8], capacity: usize) -> [Option<Finger>; 2] {
    let newest = usize::from(touch[0]).clamp(1, capacity) - 1;
    // Past the count byte and that touch report's timestamp.
    let points = 1 + newest * TOUCH_REPORT_LEN + 1;
    std::array::from_fn(|i| {
        let point = &touch[points + 4 * i..points + 4 * i + 4];
        dualsense::touch_point(point).map(|finger| Finger {
            y: rescale_touch_y(
                finger.y,
                DUALSHOCK4_TOUCHPAD_HEIGHT,
                DUALSENSE_TOUCHPAD_HEIGHT,
            ),
            ..finger
        })
    })
}

/// Reads `status[0]` as `dualshock4_parse_report` does: bit 4 says a cable is
/// plugged in, which changes what the capacity in bits 0..=3 means.
fn battery(status: u8) -> Battery {
    let capacity = status & 0x0F; // DS4_STATUS0_BATTERY_CAPACITY
    let cable = status & (1 << 4) != 0; // DS4_STATUS0_CABLE_STATE
    // Capacity 0 means 0..=9%, so the middle of the step is shown.
    let level = (capacity * 10 + 5).min(100);
    let (percent, charge) = match (cable, capacity) {
        (false, _) => (level, Charge::Discharging),
        (true, 0..=10) => (level, Charge::Charging),
        (true, 11) => (100, Charge::Full), // DS4_BATTERY_STATUS_FULL
        // 14 is a voltage or temperature error.
        (true, 14) => (0, Charge::NotCharging),
        // 15 is a charge error; the rest are undefined.
        (true, _) => (0, Charge::Unknown),
    };
    Battery { percent, charge }
}
