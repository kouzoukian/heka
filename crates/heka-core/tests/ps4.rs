//! Controller state in, PS4 Virtual Controller report out.
//!
//! The report is the DualShock 4 USB input report without its report ID
//! (`struct dualshock4_input_report_usb` in Linux `hid-playstation.c`), which is
//! also ViGEmBus's 63-byte `DS4_REPORT_EX`: sticks at 0..4, buttons[3] at 4..7,
//! triggers at 7 and 8, sensor timestamp (le16) at 9, gyro[3] at 12, accel[3] at 18,
//! touch report count at 32, first touch report at 33 (packet counter, then two
//! points of contact, x_lo, x_hi:4|y_lo:4, y_hi).

use heka_core::calibration::Calibration;
use heka_core::ps4::Ps4Translator;
use heka_core::state::{Buttons, ControllerState, Finger, Motion, Stick};

fn report(state: &ControllerState) -> [u8; 63] {
    Ps4Translator::default().report(state)
}

fn pressed(buttons: Buttons) -> ControllerState {
    ControllerState {
        buttons,
        ..ControllerState::NEUTRAL
    }
}

#[test]
fn neutral_state_centres_sticks_releases_the_dpad_and_lifts_every_finger() {
    let bytes = report(&ControllerState::NEUTRAL);
    assert_eq!(&bytes[0..9], &[0x80, 0x80, 0x80, 0x80, 0x08, 0, 0, 0, 0]);
    assert_eq!(bytes[32], 1, "one touch report");
    assert_eq!(bytes[34] & 0x80, 0x80, "finger 1 up");
    assert_eq!(bytes[38] & 0x80, 0x80, "finger 2 up");
}

#[test]
fn every_button_lands_on_its_dualshock_4_bit() {
    // `DS_BUTTONS0_*`, `DS_BUTTONS1_*` and `DS_BUTTONS2_*`, shared by both pads in
    // `hid-playstation.c`. Create is the DualShock 4's Share.
    let cases = [
        (Buttons::SQUARE, 4, 0x18),
        (Buttons::CROSS, 4, 0x28),
        (Buttons::CIRCLE, 4, 0x48),
        (Buttons::TRIANGLE, 4, 0x88),
        (Buttons::L1, 5, 0x01),
        (Buttons::R1, 5, 0x02),
        (Buttons::L2, 5, 0x04),
        (Buttons::R2, 5, 0x08),
        (Buttons::CREATE, 5, 0x10),
        (Buttons::OPTIONS, 5, 0x20),
        (Buttons::L3, 5, 0x40),
        (Buttons::R3, 5, 0x80),
        (Buttons::PS, 6, 0x01),
        (Buttons::TOUCHPAD, 6, 0x02),
    ];
    for (button, byte, value) in cases {
        let bytes = report(&pressed(button));
        assert_eq!(bytes[byte], value, "{button:?}");
        for other in [4, 5, 6] {
            if other != byte {
                let idle = if other == 4 { 0x08 } else { 0 };
                assert_eq!(bytes[other], idle, "{button:?} leaks into byte {other}");
            }
        }
    }
}

#[test]
fn mute_has_no_dualshock_4_button() {
    let bytes = report(&pressed(Buttons::MUTE));
    assert_eq!(&bytes[4..7], &[0x08, 0, 0]);
}

#[test]
fn the_dpad_becomes_a_hat_clockwise_from_up() {
    let up = Buttons::DPAD_UP;
    let down = Buttons::DPAD_DOWN;
    let left = Buttons::DPAD_LEFT;
    let right = Buttons::DPAD_RIGHT;
    let cases = [
        (up, 0),
        (up | right, 1),
        (right, 2),
        (down | right, 3),
        (down, 4),
        (down | left, 5),
        (left, 6),
        (up | left, 7),
        (up | down, 8),
        (left | right, 8),
    ];
    for (dpad, hat) in cases {
        let bytes = report(&pressed(dpad | Buttons::CROSS));
        assert_eq!(bytes[4], 0x20 | hat, "{dpad:?}");
    }
}

#[test]
fn sticks_and_triggers_pass_through() {
    let state = ControllerState {
        left_stick: Stick { x: 0, y: 255 },
        right_stick: Stick { x: 255, y: 1 },
        left_trigger: 0x40,
        right_trigger: 0xFF,
        ..ControllerState::NEUTRAL
    };
    let bytes = report(&state);
    assert_eq!(&bytes[0..4], &[0, 255, 255, 1]);
    assert_eq!((bytes[7], bytes[8]), (0x40, 0xFF));
}

#[test]
fn two_fingers_keep_their_contact_numbers_with_y_scaled_to_the_dualshock_4_pad() {
    // DualSense touchpad 1920 x 1080, DualShock 4 1920 x 942 (`DS_TOUCHPAD_*` and
    // `DS4_TOUCHPAD_*`): x is unchanged, y 0..=1079 maps onto 0..=941.
    let state = ControllerState {
        touch: [
            Some(Finger {
                id: 5,
                x: 1919,
                y: 1079,
            }),
            Some(Finger {
                id: 0x7F,
                x: 960,
                y: 540,
            }),
        ],
        ..ControllerState::NEUTRAL
    };
    let bytes = report(&state);
    // Finger 1: contact 5, x 1919 = 0x77F, y 941 = 0x3AD.
    assert_eq!(&bytes[34..38], &[0x05, 0x7F, 0xD7, 0x3A]);
    // Finger 2: contact 0x7F, x 960 = 0x3C0, y 540 * 941 / 1079 = 470.9, so 471 = 0x1D7.
    assert_eq!(&bytes[38..42], &[0x7F, 0xC0, 0x73, 0x1D]);
}

#[test]
fn the_touch_packet_counter_moves_when_the_touch_changes() {
    let mut translator = Ps4Translator::default();
    let touching = |x| ControllerState {
        touch: [Some(Finger { id: 1, x, y: 0 }), None],
        ..ControllerState::NEUTRAL
    };
    let first = translator.report(&touching(10))[33];
    let same = translator.report(&touching(10))[33];
    let moved = translator.report(&touching(11))[33];
    let lifted = translator.report(&ControllerState::NEUTRAL)[33];
    assert_eq!(same, first);
    assert_eq!(moved, first.wrapping_add(1));
    assert_eq!(lifted, first.wrapping_add(2));
}

/// ViGEmBus's answer to a DualShock 4 calibration request (feature report `0x02`),
/// copied from `sys/Ds4Pdo.cpp` (`HID_REPORT_ID_1` response). Games calibrate the
/// PS4 Virtual Controller's motion with it.
const VIGEM_CALIBRATION: [u8; 37] = [
    0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x87, 0x22, 0x7B, 0xDD, 0xB2, 0x22, 0x47, 0xDD,
    0xBD, 0x22, 0x43, 0xDD, 0x1C, 0x02, 0x1C, 0x02, 0x7F, 0x1E, 0x2E, 0xDF, 0x60, 0x1F, 0x4C,
    0xE0, 0x3A, 0x1D, 0xC6, 0xDE, 0x08, 0x00,
];

fn le16(bytes: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

#[test]
fn motion_reads_back_through_the_virtual_controllers_calibration() {
    let game = Calibration::from_paired_block(&VIGEM_CALIBRATION);
    let cases = [
        Motion {
            gyro: [102_400, -51_200, 0],
            accel: [0, 8192, -8192],
            timestamp: 0,
        },
        Motion {
            gyro: [-1_000_000, 1_000_000, 1024],
            accel: [4096, -4096, 16_384],
            timestamp: 0,
        },
    ];
    for motion in cases {
        let bytes = report(&ControllerState {
            motion,
            ..ControllerState::NEUTRAL
        });
        let gyro = game.gyro(std::array::from_fn(|i| le16(&bytes, 12 + 2 * i)));
        let accel = game.accel(std::array::from_fn(|i| le16(&bytes, 18 + 2 * i)));
        for i in 0..3 {
            // One raw step of the virtual pad is about 63 gyro units and 1 accel
            // unit: rounding to it, then the game truncating, costs under 1.5 steps.
            assert!((gyro[i] - motion.gyro[i]).abs() <= 94, "gyro {i}: {gyro:?}");
            assert!((accel[i] - motion.accel[i]).abs() <= 2, "accel {i}: {accel:?}");
        }
    }
}

#[test]
fn one_g_on_x_is_a_worked_raw_value() {
    // Accel x: plus 0x1E7F = 7807, minus 0xDF2E = -8402, so range 16209 and bias
    // 7807 - 8104 = -297. One g (8192) is 8192 * 16209 / 16384 - 297 = 7807.5, so 7808.
    let bytes = report(&ControllerState {
        motion: Motion {
            accel: [8192, 0, 0],
            ..Motion::default()
        },
        ..ControllerState::NEUTRAL
    });
    assert_eq!(le16(&bytes, 18), 7808);
}

#[test]
fn the_sensor_timestamp_becomes_dualshock_4_ticks() {
    // DualSense ticks are 1/3 µs, DualShock 4 ticks 16/3 µs (`hid-playstation.c`
    // timestamp conversions), so one DualShock 4 tick is 16 DualSense ticks.
    let at = |timestamp| {
        let bytes = report(&ControllerState {
            motion: Motion {
                timestamp,
                ..Motion::default()
            },
            ..ControllerState::NEUTRAL
        });
        u16::from_le_bytes([bytes[9], bytes[10]])
    };
    assert_eq!(at(0x0001_2340), 0x1234);
    assert_eq!(at(0xFFFF_FFFF), 0xFFFF);
    assert_eq!(at(0x0010_0000), 0x0000);
}
