use crate::calibration::{self, CALIBRATION_BLOCK_LEN, Calibration};
use crate::crc;
use crate::feedback::{Feedback, PlayerLights};
use crate::link::ReportError;
use crate::mac::MacAddress;
use crate::state::{Battery, Buttons, Charge, ControllerState, Finger, Motion, Stick};

pub const VENDOR_ID: u16 = 0x054C;
pub const PRODUCT_ID_DUALSENSE: u16 = 0x0CE6;
pub const PRODUCT_ID_DUALSENSE_EDGE: u16 = 0x0DF2;

pub const USB_INPUT_REPORT_ID: u8 = 0x01;
pub const USB_INPUT_REPORT_LEN: usize = 64;

pub fn is_dualsense(vendor_id: u16, product_id: u16) -> bool {
    vendor_id == VENDOR_ID && matches!(product_id, PRODUCT_ID_DUALSENSE | PRODUCT_ID_DUALSENSE_EDGE)
}

const LEFT_X: usize = 1;
const LEFT_Y: usize = 2;
const RIGHT_X: usize = 3;
const RIGHT_Y: usize = 4;
const LEFT_TRIGGER: usize = 5;
const RIGHT_TRIGGER: usize = 6;
const BUTTONS_0: usize = 8;
const BUTTONS_1: usize = 9;
const BUTTONS_2: usize = 10;
const STATUS_0: usize = 53;

const HAT_MASK: u8 = 0x0F;

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

const EDGE_BUTTON_BITS: [(u8, Buttons); 4] = [
    (1 << 4, Buttons::LEFT_FN),
    (1 << 5, Buttons::RIGHT_FN),
    (1 << 6, Buttons::LEFT_BACK),
    (1 << 7, Buttons::RIGHT_BACK),
];

fn with_edge_buttons(mut state: ControllerState, buttons_2: u8) -> ControllerState {
    for (mask, button) in EDGE_BUTTON_BITS {
        state.buttons.set(button, buttons_2 & mask != 0);
    }
    state
}

pub(crate) fn buttons(bytes: &[u8]) -> Buttons {
    let mut buttons = hat_to_dpad(bytes[0] & HAT_MASK);
    for (byte, mask, button) in BUTTON_BITS {
        buttons.set(button, bytes[byte - BUTTONS_0] & mask != 0);
    }
    buttons
}

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

pub const PAIRING_INFO_REPORT_ID: u8 = 0x09;
pub const PAIRING_INFO_LEN: usize = 20;
const PAIRING_INFO_MAC: usize = 1;

pub fn parse_pairing_info(report: &[u8]) -> Result<MacAddress, ReportError> {
    mac_after_report_id(report, PAIRING_INFO_REPORT_ID)
}

pub(crate) fn mac_after_report_id(report: &[u8], id: u8) -> Result<MacAddress, ReportError> {
    if report.len() < PAIRING_INFO_MAC + 6 {
        return Err(ReportError::TooShort { len: report.len() });
    }
    if report[0] != id {
        return Err(ReportError::UnexpectedId { id: report[0] });
    }
    let mut mac = [0u8; 6];
    mac.copy_from_slice(&report[PAIRING_INFO_MAC..PAIRING_INFO_MAC + 6]);
    mac.reverse();
    Ok(MacAddress(mac))
}

pub const CALIBRATION_REPORT_ID: u8 = 0x05;
pub const CALIBRATION_REPORT_LEN: usize = 41;

pub fn parse_calibration(report: &[u8]) -> Result<Calibration, ReportError> {
    calibration::check_report(report, CALIBRATION_REPORT_ID, CALIBRATION_BLOCK_LEN)?;
    Ok(Calibration::from_paired_block(report))
}

const GYRO: usize = 16;
const ACCEL: usize = 22;
const SENSOR_TIMESTAMP: usize = 28;
const TOUCH_POINTS: usize = 33;

const TOUCH_INACTIVE: u8 = 1 << 7;

fn le16(report: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([report[offset], report[offset + 1]])
}

pub(crate) fn touch_point(point: &[u8]) -> Option<Finger> {
    if point[0] & TOUCH_INACTIVE != 0 {
        return None;
    }
    Some(Finger {
        id: point[0],
        x: u16::from(point[1]) | (u16::from(point[2] & 0x0F) << 8),
        y: u16::from(point[2] >> 4) | (u16::from(point[3]) << 4),
    })
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
        buttons: buttons(&report[BUTTONS_0..=BUTTONS_2]),
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
        fine_sticks: None,
        battery: Some(battery(report[STATUS_0])),
    })
}

pub fn parse_edge_usb_input_report(
    report: &[u8],
    calibration: &Calibration,
) -> Result<ControllerState, ReportError> {
    let state = parse_usb_input_report(report, calibration)?;
    Ok(with_edge_buttons(state, report[BUTTONS_2]))
}

pub const BT_INPUT_REPORT_ID: u8 = 0x31;
pub const BT_INPUT_REPORT_LEN: usize = 78;
const BT_INPUT_COMMON: usize = 2;
const BT_BUTTONS_2: usize = BT_INPUT_COMMON + BUTTONS_2 - 1;

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
    let mut usb = [0u8; USB_INPUT_REPORT_LEN];
    usb[0] = USB_INPUT_REPORT_ID;
    usb[1..].copy_from_slice(&report[BT_INPUT_COMMON..BT_INPUT_COMMON + USB_INPUT_REPORT_LEN - 1]);
    parse_usb_input_report(&usb, calibration)
}

pub fn parse_edge_bluetooth_input_report(
    report: &[u8],
    calibration: &Calibration,
) -> Result<ControllerState, ReportError> {
    let state = parse_bluetooth_input_report(report, calibration)?;
    Ok(with_edge_buttons(state, report[BT_BUTTONS_2]))
}

fn battery(status: u8) -> Battery {
    let capacity = status & 0x0F;
    let level = (capacity * 10 + 5).min(100);
    match status >> 4 {
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
        0xA | 0xB => Battery {
            percent: 0,
            charge: Charge::NotCharging,
        },
        _ => Battery {
            percent: 0,
            charge: Charge::Unknown,
        },
    }
}

pub const USB_OUTPUT_REPORT_ID: u8 = 0x02;
pub const USB_OUTPUT_REPORT_LEN: usize = 63;

const OUT_VALID_FLAG0: usize = 1;
const OUT_MOTOR_RIGHT: usize = 3;
const OUT_MOTOR_LEFT: usize = 4;
const OUT_VALID_FLAG1: usize = 2;
const OUT_VALID_FLAG2: usize = 39;
const OUT_PLAYER_LEDS: usize = 44;
const OUT_LIGHTBAR_RED: usize = 45;
const OUT_LIGHTBAR_GREEN: usize = 46;
const OUT_LIGHTBAR_BLUE: usize = 47;

const FLAG0_COMPATIBLE_VIBRATION: u8 = 1 << 0;
const FLAG0_HAPTICS_SELECT: u8 = 1 << 1;
const FLAG1_LIGHTBAR_CONTROL_ENABLE: u8 = 1 << 2;
const FLAG2_COMPATIBLE_VIBRATION2: u8 = 1 << 2;
const FLAG1_PLAYER_INDICATOR_CONTROL_ENABLE: u8 = 1 << 4;

fn player_leds(lights: PlayerLights) -> u8 {
    match lights {
        PlayerLights::Off => 0,
        PlayerLights::One => 0b00100,
        PlayerLights::Two => 0b01010,
        PlayerLights::Three => 0b10101,
        PlayerLights::Four => 0b11011,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RumbleMode {
    Compatible,
    CompatibleV2,
}

pub const FIRMWARE_INFO_REPORT_ID: u8 = 0x20;
pub const FIRMWARE_INFO_REPORT_LEN: usize = 64;
const UPDATE_VERSION: usize = 44;
const UPDATE_VERSION_V2_RUMBLE: u16 = 0x0215;

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
    if let Some(lights) = feedback.player_lights {
        report[OUT_VALID_FLAG1] |= FLAG1_PLAYER_INDICATOR_CONTROL_ENABLE;
        report[OUT_PLAYER_LEDS] = player_leds(lights);
    }
    report
}

pub const BT_OUTPUT_REPORT_ID: u8 = 0x31;
pub const BT_OUTPUT_REPORT_LEN: usize = 78;
const BT_OUTPUT_TAG: u8 = 0x10;
const OUT_BT_SEQ_TAG: usize = 1;
const OUT_BT_TAG: usize = 2;
const OUT_BT_COMMON: usize = 3;
const OUT_COMMON_LEN: usize = 47;
pub const BT_OUTPUT_SEQUENCE_LEN: u8 = 16;

pub fn bluetooth_output_report(
    feedback: &Feedback,
    mode: RumbleMode,
    seq: u8,
) -> [u8; BT_OUTPUT_REPORT_LEN] {
    bluetooth_from_usb(&usb_output_report(feedback, mode), seq)
}

const OUT_LIGHTBAR_SETUP: usize = 42;
const FLAG2_LIGHTBAR_SETUP_CONTROL_ENABLE: u8 = 1 << 1;
const LIGHTBAR_SETUP_LIGHT_OUT: u8 = 1 << 1;

pub fn bluetooth_lightbar_setup_report(seq: u8) -> [u8; BT_OUTPUT_REPORT_LEN] {
    let mut usb = [0u8; USB_OUTPUT_REPORT_LEN];
    usb[0] = USB_OUTPUT_REPORT_ID;
    usb[OUT_VALID_FLAG2] = FLAG2_LIGHTBAR_SETUP_CONTROL_ENABLE;
    usb[OUT_LIGHTBAR_SETUP] = LIGHTBAR_SETUP_LIGHT_OUT;
    bluetooth_from_usb(&usb, seq)
}

pub fn is_bluetooth_lightbar_setup_report(report: &[u8]) -> bool {
    const FLAG2: usize = OUT_BT_COMMON + OUT_VALID_FLAG2 - 1;
    report.len() == BT_OUTPUT_REPORT_LEN
        && report[0] == BT_OUTPUT_REPORT_ID
        && report[FLAG2] & FLAG2_LIGHTBAR_SETUP_CONTROL_ENABLE != 0
}

fn bluetooth_from_usb(usb: &[u8; USB_OUTPUT_REPORT_LEN], seq: u8) -> [u8; BT_OUTPUT_REPORT_LEN] {
    let mut report = [0u8; BT_OUTPUT_REPORT_LEN];
    report[0] = BT_OUTPUT_REPORT_ID;
    report[OUT_BT_SEQ_TAG] = (seq % BT_OUTPUT_SEQUENCE_LEN) << 4;
    report[OUT_BT_TAG] = BT_OUTPUT_TAG;
    report[OUT_BT_COMMON..OUT_BT_COMMON + OUT_COMMON_LEN].copy_from_slice(&usb[1..=OUT_COMMON_LEN]);
    crc::seal(crc::BT_OUTPUT_SEED, &mut report);
    report
}
