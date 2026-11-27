use crate::calibration::{self, Calibration};
use crate::crc;
use crate::dualsense;
use crate::feedback::Feedback;
use crate::link::ReportError;
use crate::mac::{InvalidMacAddress, MacAddress};
use crate::state::{
    Battery, Buttons, Charge, ControllerState, DUALSHOCK4_TOUCHPAD_HEIGHT, Finger, Motion, Stick,
    TOUCHPAD_HEIGHT, rescale_touch_y,
};

pub const PRODUCT_ID_DUALSHOCK4: u16 = 0x05C4;
pub const PRODUCT_ID_DUALSHOCK4_V2: u16 = 0x09CC;

pub fn is_dualshock4(vendor_id: u16, product_id: u16) -> bool {
    vendor_id == dualsense::VENDOR_ID
        && matches!(product_id, PRODUCT_ID_DUALSHOCK4 | PRODUCT_ID_DUALSHOCK4_V2)
}

pub const PAIRING_INFO_REPORT_ID: u8 = 0x12;
pub const PAIRING_INFO_LEN: usize = 16;

pub fn parse_pairing_info(report: &[u8]) -> Result<MacAddress, ReportError> {
    dualsense::mac_after_report_id(report, PAIRING_INFO_REPORT_ID)
}

pub fn parse_bluetooth_serial(serial: &str) -> Result<MacAddress, InvalidMacAddress> {
    if serial.len() != 12 || !serial.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(InvalidMacAddress);
    }
    match u64::from_str_radix(serial, 16) {
        Ok(0) | Err(_) => Err(InvalidMacAddress),
        Ok(address) => {
            let [_, _, bytes @ ..] = address.to_be_bytes();
            Ok(MacAddress(bytes))
        }
    }
}

pub const USB_OUTPUT_REPORT_ID: u8 = 0x05;
pub const USB_OUTPUT_REPORT_LEN: usize = 32;
const USB_OUTPUT_COMMON: usize = 1;

pub const BT_OUTPUT_REPORT_ID: u8 = 0x11;
pub const BT_OUTPUT_REPORT_LEN: usize = 78;
const BT_OUTPUT_COMMON: usize = 3;
const OUT_BT_HW_CONTROL: usize = 1;
const HW_CONTROL_HID_AND_CRC: u8 = 0x80 | 0x40;
const BT_POLL_INTERVAL_MS: u8 = 4;

const OUT_VALID_FLAG0: usize = 0;
const OUT_MOTOR_RIGHT: usize = 3;
const OUT_MOTOR_LEFT: usize = 4;
const OUT_LIGHTBAR: usize = 5;

const FLAG0_MOTOR: u8 = 0x01;
const FLAG0_LED: u8 = 0x02;

pub fn usb_output_report(feedback: &Feedback) -> [u8; USB_OUTPUT_REPORT_LEN] {
    let mut report = [0u8; USB_OUTPUT_REPORT_LEN];
    report[0] = USB_OUTPUT_REPORT_ID;
    write_feedback(&mut report[USB_OUTPUT_COMMON..], feedback);
    report
}

pub fn bluetooth_output_report(feedback: &Feedback) -> [u8; BT_OUTPUT_REPORT_LEN] {
    let mut report = [0u8; BT_OUTPUT_REPORT_LEN];
    report[0] = BT_OUTPUT_REPORT_ID;
    report[OUT_BT_HW_CONTROL] = HW_CONTROL_HID_AND_CRC | BT_POLL_INTERVAL_MS;
    write_feedback(&mut report[BT_OUTPUT_COMMON..], feedback);
    crc::seal(crc::BT_OUTPUT_SEED, &mut report);
    report
}

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

pub const USB_INPUT_REPORT_ID: u8 = 0x01;
pub const USB_INPUT_REPORT_LEN: usize = 64;

const USB_INPUT_COMMON: usize = 1;
const USB_TOUCH_COUNT: usize = 33;
const USB_TOUCH_REPORTS: usize = 3;

const LEFT_X: usize = 0;
const LEFT_Y: usize = 1;
const RIGHT_X: usize = 2;
const RIGHT_Y: usize = 3;
const BUTTONS: usize = 4;
const LEFT_TRIGGER: usize = 7;
const RIGHT_TRIGGER: usize = 8;
const SENSOR_TIMESTAMP: usize = 9;
const GYRO: usize = 12;
const ACCEL: usize = 18;
const STATUS_0: usize = 29;

const TOUCH_REPORT_LEN: usize = 9;

pub const USB_CALIBRATION_REPORT_ID: u8 = 0x02;
pub const USB_CALIBRATION_REPORT_LEN: usize = 37;

pub fn parse_usb_calibration(report: &[u8]) -> Result<Calibration, ReportError> {
    calibration::check_report(
        report,
        USB_CALIBRATION_REPORT_ID,
        USB_CALIBRATION_REPORT_LEN,
    )?;
    Ok(Calibration::from_paired_block(report))
}

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

pub const BT_INPUT_REPORT_ID: u8 = 0x11;
pub const BT_INPUT_REPORT_LEN: usize = 78;
const BT_INPUT_COMMON: usize = 3;
const BT_TOUCH_COUNT: usize = 35;
const BT_TOUCH_REPORTS: usize = 4;

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

pub const BT_CALIBRATION_REPORT_ID: u8 = 0x05;
pub const BT_CALIBRATION_REPORT_LEN: usize = 41;

pub fn parse_bluetooth_calibration(report: &[u8]) -> Result<Calibration, ReportError> {
    calibration::check_report(report, BT_CALIBRATION_REPORT_ID, BT_CALIBRATION_REPORT_LEN)?;
    let report = &report[..BT_CALIBRATION_REPORT_LEN];
    if !crc::has_valid_crc(crc::BT_FEATURE_SEED, report) {
        return Err(ReportError::BadCrc);
    }
    Ok(Calibration::from_grouped_block(report))
}

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
            timestamp: u32::from(timestamp) << 4,
        },
    }
}

fn newest_touch(touch: &[u8], capacity: usize) -> [Option<Finger>; 2] {
    let newest = usize::from(touch[0]).clamp(1, capacity) - 1;
    let points = 1 + newest * TOUCH_REPORT_LEN + 1;
    std::array::from_fn(|i| {
        let point = &touch[points + 4 * i..points + 4 * i + 4];
        dualsense::touch_point(point).map(|finger| Finger {
            y: rescale_touch_y(finger.y, DUALSHOCK4_TOUCHPAD_HEIGHT, TOUCHPAD_HEIGHT),
            ..finger
        })
    })
}

fn battery(status: u8) -> Battery {
    let capacity = status & 0x0F;
    let cable = status & (1 << 4) != 0;
    let level = (capacity * 10 + 5).min(100);
    let (percent, charge) = match (cable, capacity) {
        (false, _) => (level, Charge::Discharging),
        (true, 0..=10) => (level, Charge::Charging),
        (true, 11) => (100, Charge::Full),
        (true, 14) => (0, Charge::NotCharging),
        (true, _) => (0, Charge::Unknown),
    };
    Battery { percent, charge }
}
