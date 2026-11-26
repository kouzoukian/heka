//! Feedback in, DualShock 4 output report bytes out.
//!
//! Expected bytes are written out by hand from Linux `hid-playstation.c`
//! (`struct dualshock4_output_report_usb` and `_bt`, `struct
//! dualshock4_output_report_common`, the `DS4_OUTPUT_*` definitions).

mod common;

use heka_core::feedback::{Feedback, Lightbar, Rumble};
use heka_core::link::FeedbackFormat;

fn feedback(large: u8, small: u8, colour: Option<(u8, u8, u8)>) -> Feedback {
    Feedback {
        rumble: Rumble { large, small },
        lightbar: colour.map(|(red, green, blue)| Lightbar { red, green, blue }),
    }
}

/// A report of `len` zeros with `(offset, value)` bytes set.
fn zeros_but(len: usize, bytes: &[(usize, u8)]) -> Vec<u8> {
    let mut report = vec![0u8; len];
    for &(offset, value) in bytes {
        report[offset] = value;
    }
    report
}

/// USB: report ID 0x05, 32 bytes; valid_flag0 at 1, motor_right at 4, motor_left
/// at 5, then red, green and blue.
fn usb(bytes: &[(usize, u8)]) -> Vec<u8> {
    zeros_but(32, &[&[(0, 0x05)], bytes].concat())
}

/// Bluetooth: report ID 0x11, 78 bytes; hw_control 0xC0 (HID and CRC) with a 4 ms
/// report interval at 1, valid_flag0 at 3, motor_right at 6, motor_left at 7,
/// then red, green and blue, and a CRC32 seeded with 0xA2 in the last 4 bytes.
fn bluetooth(bytes: &[(usize, u8)]) -> Vec<u8> {
    let mut report = zeros_but(78, &[&[(0, 0x11), (1, 0xC4)], bytes].concat());
    common::seal(0xA2, &mut report);
    report
}

#[test]
fn usb_rumble_drives_the_left_motor_from_the_large_motor_and_leaves_the_lightbar() {
    let reports = FeedbackFormat::DualShock4Usb.reports(&feedback(0xC8, 0x40, None));
    // valid_flag0 = MOTOR.
    assert_eq!(reports, [usb(&[(1, 0x01), (4, 0x40), (5, 0xC8)])]);
}

#[test]
fn a_usb_lightbar_colour_is_sent_with_the_rumble() {
    let colour = Some((0x11, 0x22, 0x33));
    let reports = FeedbackFormat::DualShock4Usb.reports(&feedback(0xC8, 0x40, colour));
    // valid_flag0 = MOTOR | LED.
    let expected = usb(&[
        (1, 0x03),
        (4, 0x40),
        (5, 0xC8),
        (6, 0x11),
        (7, 0x22),
        (8, 0x33),
    ]);
    assert_eq!(reports, [expected]);
}

#[test]
fn bluetooth_rumble_carries_a_crc() {
    let reports = FeedbackFormat::DualShock4Bluetooth.reports(&feedback(0xC8, 0x40, None));
    assert_eq!(reports, [bluetooth(&[(3, 0x01), (6, 0x40), (7, 0xC8)])]);
}

#[test]
fn a_bluetooth_lightbar_colour_needs_no_setup_report() {
    let colour = Some((0x11, 0x22, 0x33));
    let reports = FeedbackFormat::DualShock4Bluetooth.reports(&feedback(0, 0, colour));
    let expected = bluetooth(&[(3, 0x03), (8, 0x11), (9, 0x22), (10, 0x33)]);
    assert_eq!(reports, [expected]);
}

#[test]
fn feedback_off_stops_both_motors_on_either_link() {
    assert_eq!(
        FeedbackFormat::DualShock4Usb.reports(&Feedback::OFF),
        [usb(&[(1, 0x01)])]
    );
    assert_eq!(
        FeedbackFormat::DualShock4Bluetooth.reports(&Feedback::OFF),
        [bluetooth(&[(3, 0x01)])]
    );
}
