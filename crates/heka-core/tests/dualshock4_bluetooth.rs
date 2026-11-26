//! DualShock 4 Bluetooth input and calibration reports in, Controller state out.
//!
//! No DualShock 4 is available, so every report is DERIVED from the layout in
//! Linux `hid-playstation.c` (see `common::ds4_bluetooth_from_usb`). Offsets are
//! into the report with its report ID byte.

mod common;

use heka_core::calibration::Calibration;
use heka_core::dualshock4::{
    parse_bluetooth_calibration, parse_bluetooth_input_report, parse_usb_input_report,
};
use heka_core::link::ReportError;
use heka_core::state::{Buttons, ControllerState, Finger};

fn parse(report: &[u8]) -> Result<ControllerState, ReportError> {
    parse_bluetooth_input_report(report, &Calibration::UNCALIBRATED)
}

#[test]
fn a_bluetooth_report_carries_the_same_input_as_the_usb_report() {
    // Sticks, Cross, L1, PS, triggers, timestamp, motion, battery and one finger.
    let usb = common::ds4_usb(&[
        (1, 1),
        (2, 2),
        (3, 3),
        (4, 4),
        (5, 0x28),
        (6, 0x01),
        (7, 0x01),
        (8, 5),
        (9, 6),
        (10, 0x34),
        (11, 0x12),
        (13, 100),
        (15, 101),
        (17, 102),
        (19, 103),
        (21, 104),
        (23, 105),
        (30, 0x17),
        (35, 7),
        (36, 200),
        (38, 20),
    ]);
    let state = parse(&common::ds4_bluetooth_from_usb(&usb)).unwrap();
    assert_eq!(
        state,
        parse_usb_input_report(&usb, &Calibration::UNCALIBRATED).unwrap()
    );
    assert_eq!(state.buttons, Buttons::CROSS | Buttons::L1 | Buttons::PS);
}

#[test]
fn a_fourth_touch_report_can_be_the_newest() {
    // Touch reports start at byte 36, 9 bytes each: the fourth's first finger is at 64.
    let mut report = common::ds4_bluetooth_from_usb(&common::ds4_usb(&[]));
    report[35] = 4;
    report[64..68].copy_from_slice(&[9, 100, 0, 0]);
    common::seal(0xA1, &mut report);
    let finger = Finger {
        id: 9,
        x: 100,
        y: 0,
    };
    assert_eq!(parse(&report).unwrap().touch, [Some(finger), None]);
}

#[test]
fn a_report_padded_to_the_longest_report_the_link_takes_is_read() {
    // Windows hands every input report over at the length of the longest one.
    let mut report = common::ds4_bluetooth_from_usb(&common::ds4_usb(&[(5, 0x28)]));
    report.resize(128, 0);
    assert_eq!(parse(&report).unwrap().buttons, Buttons::CROSS);
}

#[test]
fn a_bluetooth_report_with_a_wrong_crc_is_refused() {
    let mut report = common::ds4_bluetooth_from_usb(&common::ds4_usb(&[]));
    report[10] ^= 0x01;
    assert_eq!(parse(&report), Err(ReportError::BadCrc));
}

#[test]
fn the_simple_bluetooth_report_and_short_reads_are_refused() {
    // Until a calibration feature report is read, the DualShock 4 sends the short
    // report 0x01 over Bluetooth (`DS4_INPUT_REPORT_BT_MINIMAL`).
    let full = common::ds4_bluetooth_from_usb(&common::ds4_usb(&[]));
    let mut simple = full.clone();
    simple[0] = 0x01;
    assert_eq!(parse(&simple), Err(ReportError::UnexpectedId { id: 0x01 }));
    assert_eq!(parse(&full[..10]), Err(ReportError::TooShort { len: 10 }));
}

/// DERIVED: a Bluetooth calibration feature report (0x05, 41 bytes). Unlike over
/// USB, the three gyro plus values come before the three minus values
/// (`dualshock4_get_calibration_data`), and a CRC32 seeded with 0xA3 ends it.
fn calibration_report() -> Vec<u8> {
    let fields: [i16; 17] = [
        0, 0, 0, // gyro biases: pitch, yaw, roll
        1000, 2000, 4000, // gyro plus: pitch, yaw, roll
        -1000, -2000, -4000, // gyro minus: pitch, yaw, roll
        500, 500, // gyro speed plus and minus
        8100, -8100, 8292, -8092, 4096, -4096, // accelerometer x, y, z plus and minus
    ];
    let mut report = vec![0x05];
    report.extend(fields.iter().flat_map(|field| field.to_le_bytes()));
    report.resize(41, 0);
    common::seal(0xA3, &mut report);
    report
}

#[test]
fn the_bluetooth_calibration_pairs_each_gyro_axis_with_its_own_limits() {
    let calibration = parse_bluetooth_calibration(&calibration_report()).unwrap();
    assert_eq!(
        calibration.gyro([100, 100, -100]),
        [51_200, 25_600, -12_800]
    );
    assert_eq!(calibration.accel([8100, 100, -2048]), [8192, 0, -4096]);
}

#[test]
fn a_bluetooth_calibration_report_that_cannot_be_trusted_is_refused() {
    let report = calibration_report();
    assert_eq!(
        parse_bluetooth_calibration(&report[..40]),
        Err(ReportError::TooShort { len: 40 })
    );
    let mut corrupt = report.clone();
    corrupt[7] ^= 0x01;
    assert_eq!(
        parse_bluetooth_calibration(&corrupt),
        Err(ReportError::BadCrc)
    );
    let mut other = report;
    other[0] = 0x02;
    assert_eq!(
        parse_bluetooth_calibration(&other),
        Err(ReportError::UnexpectedId { id: 0x02 })
    );
}
