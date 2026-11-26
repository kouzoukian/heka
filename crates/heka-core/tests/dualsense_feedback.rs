//! Feedback in, DualSense USB output report bytes out.
//!
//! Expected bytes are written out by hand from `struct dualsense_output_report_usb`
//! and the `DS_OUTPUT_VALID_FLAG*` bits in Linux `hid-playstation.c`: byte 0 is the
//! report ID 0x02, byte 1 valid_flag0, byte 2 valid_flag1, byte 3 motor_right,
//! byte 4 motor_left, byte 39 valid_flag2, 63 bytes in all.

mod common;

use heka_core::dualsense::{
    PRODUCT_ID_DUALSENSE, PRODUCT_ID_DUALSENSE_EDGE, RumbleMode, bluetooth_output_report,
    rumble_mode, usb_output_report,
};
use heka_core::feedback::{Feedback, Rumble};
use heka_core::link::FeedbackFormat;

fn rumble(large: u8, small: u8) -> Feedback {
    Feedback {
        rumble: Rumble { large, small },
    }
}

/// A 63-byte report of zeros with `(offset, value)` bytes set.
fn expected(bytes: &[(usize, u8)]) -> Vec<u8> {
    let mut report = vec![0u8; 63];
    for &(offset, value) in bytes {
        report[offset] = value;
    }
    report
}

#[test]
fn compatible_rumble_drives_the_left_motor_from_the_large_motor() {
    let report = usb_output_report(&rumble(0xC8, 0x40), RumbleMode::Compatible);
    // valid_flag0 = COMPATIBLE_VIBRATION | HAPTICS_SELECT.
    assert_eq!(
        report.to_vec(),
        expected(&[(0, 0x02), (1, 0x03), (3, 0x40), (4, 0xC8)])
    );
}

#[test]
fn v2_rumble_selects_haptics_and_flags_compatible_vibration2() {
    let report = usb_output_report(&rumble(0xC8, 0x40), RumbleMode::CompatibleV2);
    // valid_flag0 = HAPTICS_SELECT, valid_flag2 = COMPATIBLE_VIBRATION2.
    assert_eq!(
        report.to_vec(),
        expected(&[(0, 0x02), (1, 0x02), (3, 0x40), (4, 0xC8), (39, 0x04)])
    );
}

#[test]
fn feedback_off_still_sets_the_rumble_flags_with_both_motors_at_zero() {
    let report = usb_output_report(&Feedback::OFF, RumbleMode::Compatible);
    assert_eq!(report.to_vec(), expected(&[(0, 0x02), (1, 0x03)]));
}

// Bluetooth. Expected bytes are written out by hand from `struct
// dualsense_output_report_bt` and `dualsense_init_output_report` in Linux
// `hid-playstation.c`: byte 0 is the report ID 0x31, byte 1 the sequence number in
// its high nibble, byte 2 the tag 0x10 (`DS_OUTPUT_TAG`), the common block from
// byte 3 (so valid_flag0 at 3, motor_right at 5, motor_left at 6, valid_flag2 at
// 41), 78 bytes in all, ending in a little-endian CRC32 seeded with 0xA2. Expected
// CRCs were computed with Python's `zlib.crc32(bytes([0xA2]) + report[:74])`.

/// A 78-byte report of zeros with `(offset, value)` bytes set and this CRC.
fn expected_bluetooth(bytes: &[(usize, u8)], crc: u32) -> Vec<u8> {
    let mut report = vec![0u8; 78];
    for &(offset, value) in bytes {
        report[offset] = value;
    }
    report[74..].copy_from_slice(&crc.to_le_bytes());
    report
}

#[test]
fn a_bluetooth_report_carries_the_usb_common_block_after_its_header_with_a_crc() {
    let report = bluetooth_output_report(&rumble(0xC8, 0x40), RumbleMode::CompatibleV2, 0);
    assert_eq!(
        report.to_vec(),
        expected_bluetooth(
            &[(0, 0x31), (2, 0x10), (3, 0x02), (5, 0x40), (6, 0xC8), (41, 0x04)],
            0x99D4_5A68
        )
    );
}

#[test]
fn a_bluetooth_report_puts_its_sequence_number_in_the_high_nibble() {
    let report = bluetooth_output_report(&Feedback::OFF, RumbleMode::Compatible, 5);
    assert_eq!(
        report.to_vec(),
        expected_bluetooth(&[(0, 0x31), (1, 0x50), (2, 0x10), (3, 0x03)], 0xC190_4480)
    );
}

#[test]
fn each_bluetooth_report_takes_the_next_sequence_number_wrapping_after_15() {
    // `dualsense_init_output_report`: "needs to be increased every report", 0..=15.
    let mut format = FeedbackFormat::dualsense_bluetooth(RumbleMode::Compatible);
    let sequence: Vec<u8> = (0..18)
        .map(|_| format.report(&Feedback::OFF)[1] >> 4)
        .collect();
    assert_eq!(
        sequence,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 1]
    );
    let mut usb = FeedbackFormat::DualSenseUsb(RumbleMode::Compatible);
    assert_eq!(
        usb.report(&rumble(1, 2)),
        usb_output_report(&rumble(1, 2), RumbleMode::Compatible).to_vec()
    );
}

// Rumble mode. The firmware info feature report is 0x20, 64 bytes, with the
// little-endian update version at bytes 44..46 (`dualsense_get_firmware_info`);
// version 2.21 (0x0215) and later use the v2 mode (`dualsense_create`).

/// DERIVED: a firmware info report of zeros with the update version set.
fn firmware_info(update_version: u16) -> Vec<u8> {
    let mut report = vec![0u8; 64];
    report[0] = 0x20;
    report[44..46].copy_from_slice(&update_version.to_le_bytes());
    report
}

#[test]
fn a_dualsense_on_update_version_2_21_or_later_uses_v2_rumble() {
    for version in [0x0215, 0x0216, 0x0300] {
        let info = firmware_info(version);
        assert_eq!(
            rumble_mode(PRODUCT_ID_DUALSENSE, Some(&info)),
            RumbleMode::CompatibleV2,
            "update version {version:#06x}"
        );
    }
}

#[test]
fn the_captured_dualsense_firmware_info_selects_v2_rumble() {
    let info = common::reports(include_str!("fixtures/dualsense_usb_firmware_info.hex")).remove(0);
    assert_eq!(info.len(), 64);
    assert_eq!(
        rumble_mode(PRODUCT_ID_DUALSENSE, Some(&info)),
        RumbleMode::CompatibleV2
    );
}

#[test]
fn an_older_dualsense_uses_compatible_rumble() {
    let info = firmware_info(0x0214);
    assert_eq!(
        rumble_mode(PRODUCT_ID_DUALSENSE, Some(&info)),
        RumbleMode::Compatible
    );
}

#[test]
fn a_dualsense_edge_always_uses_v2_rumble() {
    assert_eq!(
        rumble_mode(PRODUCT_ID_DUALSENSE_EDGE, None),
        RumbleMode::CompatibleV2
    );
}

#[test]
fn a_dualsense_without_readable_firmware_info_falls_back_to_compatible_rumble() {
    let wrong_id = {
        let mut info = firmware_info(0x0300);
        info[0] = 0x05;
        info
    };
    for info in [
        None,
        Some(&firmware_info(0x0300)[..40]),
        Some(&wrong_id[..]),
    ] {
        assert_eq!(
            rumble_mode(PRODUCT_ID_DUALSENSE, info),
            RumbleMode::Compatible
        );
    }
}
