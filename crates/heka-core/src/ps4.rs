//! Translation from Controller state to a PS4 (DualShock 4) Virtual Controller report.
//!
//! The report is the DualShock 4 USB input report without its report ID: Linux
//! `hid-playstation.c` `struct dualshock4_input_report_usb` (common block, touch
//! report count, three touch reports, three reserved bytes), which is the same
//! 63 bytes as ViGEmBus's `DS4_REPORT_EX`. Button bits are the `DS_BUTTONS*`
//! definitions the kernel uses for both pads.

use crate::calibration::Calibration;
use crate::state::{
    Buttons, ControllerState, DUALSENSE_TOUCHPAD_HEIGHT, DUALSHOCK4_TOUCHPAD_HEIGHT, Finger,
    rescale_touch_y,
};

/// Length of a PS4 Virtual Controller report.
pub const REPORT_LEN: usize = 63;

// Offsets: `dualshock4_input_report_common` fields, then the USB touch block.
const LEFT_X: usize = 0;
const LEFT_Y: usize = 1;
const RIGHT_X: usize = 2;
const RIGHT_Y: usize = 3;
const BUTTONS_0: usize = 4;
const BUTTONS_1: usize = 5;
const BUTTONS_2: usize = 6;
const LEFT_TRIGGER: usize = 7;
const RIGHT_TRIGGER: usize = 8;
const SENSOR_TIMESTAMP: usize = 9; // le16
const GYRO: usize = 12; // gyro[3], le16 each
const ACCEL: usize = 18; // accel[3], le16 each
const NUM_TOUCH_REPORTS: usize = 32;
const TOUCH_REPORTS: usize = 33; // touch_reports[3], 9 bytes each
const TOUCH_REPORT_LEN: usize = 9;
const TOUCH_REPORT_COUNT: usize = 3;

/// The hat value for a released d-pad.
const HAT_RELEASED: u8 = 8;
/// Set in a touch point's contact byte when no finger is down (`DS_TOUCH_POINT_INACTIVE`).
const TOUCH_INACTIVE: u8 = 1 << 7;

/// Which DualShock 4 bit each Controller button drives. Mute has none.
const BUTTON_BITS: [(Buttons, usize, u8); 14] = [
    (Buttons::SQUARE, BUTTONS_0, 1 << 4),
    (Buttons::CROSS, BUTTONS_0, 1 << 5),
    (Buttons::CIRCLE, BUTTONS_0, 1 << 6),
    (Buttons::TRIANGLE, BUTTONS_0, 1 << 7),
    (Buttons::L1, BUTTONS_1, 1 << 0),
    (Buttons::R1, BUTTONS_1, 1 << 1),
    (Buttons::L2, BUTTONS_1, 1 << 2),
    (Buttons::R2, BUTTONS_1, 1 << 3),
    (Buttons::CREATE, BUTTONS_1, 1 << 4), // Share
    (Buttons::OPTIONS, BUTTONS_1, 1 << 5),
    (Buttons::L3, BUTTONS_1, 1 << 6),
    (Buttons::R3, BUTTONS_1, 1 << 7),
    (Buttons::PS, BUTTONS_2, 1 << 0),
    (Buttons::TOUCHPAD, BUTTONS_2, 1 << 1),
];

/// The DualShock 4 calibration ViGEmBus hands games for its virtual pad (feature
/// report `0x02`), from `sys/Ds4Pdo.cpp`. Motion is written as the raw readings
/// that this calibration turns back into the Controller's calibrated motion.
const VIRTUAL_CALIBRATION_REPORT: [u8; 37] = [
    0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x87, 0x22, 0x7B, 0xDD, 0xB2, 0x22, 0x47, 0xDD, 0xBD,
    0x22, 0x43, 0xDD, 0x1C, 0x02, 0x1C, 0x02, 0x7F, 0x1E, 0x2E, 0xDF, 0x60, 0x1F, 0x4C, 0xE0, 0x3A,
    0x1D, 0xC6, 0xDE, 0x08, 0x00,
];

/// Builds PS4 Virtual Controller reports for one Bridge. It remembers the last
/// touch so that the touch packet counter moves only when the touch changes.
#[derive(Debug, Clone)]
pub struct Ps4Translator {
    calibration: Calibration,
    last_touch: [Option<Finger>; 2],
    touch_counter: u8,
}

impl Default for Ps4Translator {
    fn default() -> Self {
        Self {
            // The USB DualShock 4 layout: pitch, yaw, roll plus/minus pairs
            // (`dualshock4_get_calibration_data`, `BUS_USB` branch).
            calibration: Calibration::from_paired_block(&VIRTUAL_CALIBRATION_REPORT),
            last_touch: [None, None],
            touch_counter: 0,
        }
    }
}

impl Ps4Translator {
    /// The report a PS4 Virtual Controller sends for this Controller state.
    pub fn report(&mut self, state: &ControllerState) -> [u8; REPORT_LEN] {
        let mut report = [0u8; REPORT_LEN];
        report[LEFT_X] = state.left_stick.x;
        report[LEFT_Y] = state.left_stick.y;
        report[RIGHT_X] = state.right_stick.x;
        report[RIGHT_Y] = state.right_stick.y;
        report[BUTTONS_0] = hat(state.buttons);
        for (button, byte, bit) in BUTTON_BITS {
            if state.buttons.contains(button) {
                report[byte] |= bit;
            }
        }
        report[LEFT_TRIGGER] = state.left_trigger;
        report[RIGHT_TRIGGER] = state.right_trigger;

        // 16 DualSense ticks (1/3 µs) make one DualShock 4 tick (16/3 µs).
        let timestamp = (state.motion.timestamp >> 4) as u16;
        report[SENSOR_TIMESTAMP..SENSOR_TIMESTAMP + 2].copy_from_slice(&timestamp.to_le_bytes());
        let gyro = self.calibration.raw_gyro(state.motion.gyro);
        let accel = self.calibration.raw_accel(state.motion.accel);
        for i in 0..3 {
            report[GYRO + 2 * i..GYRO + 2 * i + 2].copy_from_slice(&gyro[i].to_le_bytes());
            report[ACCEL + 2 * i..ACCEL + 2 * i + 2].copy_from_slice(&accel[i].to_le_bytes());
        }

        if state.touch != self.last_touch {
            self.last_touch = state.touch;
            self.touch_counter = self.touch_counter.wrapping_add(1);
        }
        report[NUM_TOUCH_REPORTS] = 1;
        for n in 0..TOUCH_REPORT_COUNT {
            let start = TOUCH_REPORTS + n * TOUCH_REPORT_LEN;
            let fingers = if n == 0 { state.touch } else { [None, None] };
            report[start] = if n == 0 { self.touch_counter } else { 0 };
            for (i, finger) in fingers.iter().enumerate() {
                let point = start + 1 + 4 * i;
                report[point..point + 4].copy_from_slice(&touch_point(*finger));
            }
        }
        report
    }
}

/// The hat value, clockwise from 0 = up, 8 = released (`ps_gamepad_hat_mapping`).
/// Opposite directions pressed together cancel out.
fn hat(buttons: Buttons) -> u8 {
    let up = buttons.contains(Buttons::DPAD_UP);
    let down = buttons.contains(Buttons::DPAD_DOWN);
    let left = buttons.contains(Buttons::DPAD_LEFT);
    let right = buttons.contains(Buttons::DPAD_RIGHT);
    let vertical = if up == down {
        0
    } else if up {
        1
    } else {
        -1
    };
    let horizontal = if left == right {
        0
    } else if right {
        1
    } else {
        -1
    };
    match (vertical, horizontal) {
        (1, 0) => 0,
        (1, 1) => 1,
        (0, 1) => 2,
        (-1, 1) => 3,
        (-1, 0) => 4,
        (-1, -1) => 5,
        (0, -1) => 6,
        (1, -1) => 7,
        _ => HAT_RELEASED,
    }
}

/// A touch point as the DualShock 4 packs it: contact, x_lo, x_hi:4|y_lo:4, y_hi.
fn touch_point(finger: Option<Finger>) -> [u8; 4] {
    let Some(finger) = finger else {
        return [TOUCH_INACTIVE, 0, 0, 0];
    };
    let y = rescale_touch_y(
        finger.y,
        DUALSENSE_TOUCHPAD_HEIGHT,
        DUALSHOCK4_TOUCHPAD_HEIGHT,
    );
    [
        finger.id & !TOUCH_INACTIVE,
        (finger.x & 0xFF) as u8,
        ((finger.x >> 8) & 0x0F) as u8 | ((y & 0x0F) << 4) as u8,
        (y >> 4) as u8,
    ]
}
