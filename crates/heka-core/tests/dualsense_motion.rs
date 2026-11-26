mod common;

use heka_core::calibration::Calibration;
use heka_core::dualsense::{parse_calibration, parse_usb_input_report};
use heka_core::link::ReportError;
use heka_core::state::Finger;

fn calibration_report() -> Vec<u8> {
    let fields: [i16; 17] = [
        0, 0, 0, 8000, -8000, 4000, -4000, 8000, -8000, 500, 500, 8192, -8192, 8292, -8092, 4096,
        -4096,
    ];
    let mut report = vec![0u8; 41];
    report[0] = 0x05;
    for (i, field) in fields.iter().enumerate() {
        report[1 + 2 * i..3 + 2 * i].copy_from_slice(&field.to_le_bytes());
    }
    report
}

fn le16(offset: usize, value: i16) -> [(usize, u8); 2] {
    let [lo, hi] = value.to_le_bytes();
    [(offset, lo), (offset + 1, hi)]
}

#[test]
fn gyro_and_accel_apply_the_factory_calibration() {
    let calibration = parse_calibration(&calibration_report()).unwrap();
    let mut changes = Vec::new();
    changes.extend(le16(16, 1600));
    changes.extend(le16(18, -800));
    changes.extend(le16(20, 160));
    changes.extend(le16(22, 4096));
    changes.extend(le16(24, 100));
    changes.extend(le16(26, -4096));
    let state = parse_usb_input_report(&common::derived(&changes), &calibration).unwrap();
    assert_eq!(state.motion.gyro, [102_400, -102_400, 10_240]);
    assert_eq!(state.motion.accel, [4096, 0, -8192]);
}

#[test]
fn a_resting_controller_reads_one_g_and_almost_no_rotation() {
    let report = common::reports(include_str!("fixtures/dualsense_usb_calibration.hex")).remove(0);
    let calibration = parse_calibration(&report).unwrap();
    for report in common::captured_idle() {
        let motion = parse_usb_input_report(&report, &calibration)
            .unwrap()
            .motion;
        let g = motion
            .accel
            .iter()
            .map(|a| f64::from(*a).powi(2))
            .sum::<f64>()
            .sqrt();
        assert!((g / 8192.0 - 1.0).abs() < 0.05, "{:?} is {g}", motion.accel);
        for rate in motion.gyro {
            assert!(rate.abs() < 2 * 1024, "{:?}", motion.gyro);
        }
    }
}

#[test]
fn sensor_timestamp_is_read_as_is() {
    let changes = [(28, 0x78), (29, 0x56), (30, 0x34), (31, 0x12)];
    let state =
        parse_usb_input_report(&common::derived(&changes), &Calibration::UNCALIBRATED).unwrap();
    assert_eq!(state.motion.timestamp, 0x1234_5678);
}

#[test]
fn a_calibration_report_with_zero_ranges_falls_back_to_full_scale() {
    let mut report = vec![0u8; 41];
    report[0] = 0x05;
    let calibration = parse_calibration(&report).unwrap();
    assert_eq!(calibration, Calibration::UNCALIBRATED);
    let mut changes = Vec::new();
    changes.extend(le16(16, 32767));
    changes.extend(le16(22, -32767));
    let state = parse_usb_input_report(&common::derived(&changes), &calibration).unwrap();
    assert_eq!(state.motion.gyro[0], 2048 * 1024);
    assert_eq!(state.motion.accel[0], -4 * 8192);
}

#[test]
fn calibration_reports_of_the_wrong_kind_are_refused() {
    let report = calibration_report();
    assert_eq!(
        parse_calibration(&report[..20]),
        Err(ReportError::TooShort { len: 20 })
    );
    let mut other = report.clone();
    other[0] = 0x09;
    assert_eq!(
        parse_calibration(&other),
        Err(ReportError::UnexpectedId { id: 0x09 })
    );
}

#[test]
fn captured_idle_reports_show_no_finger_down() {
    for report in common::captured_idle() {
        let state = parse_usb_input_report(&report, &Calibration::UNCALIBRATED).unwrap();
        assert_eq!(state.touch, [None, None]);
    }
}

#[test]
fn two_fingers_are_read_with_their_contact_numbers_and_positions() {
    let changes = [
        (33, 0x05),
        (34, 0x7F),
        (35, 0x77),
        (36, 0x43),
        (37, 0x7F),
        (38, 0x23),
        (39, 0x51),
        (40, 0x04),
    ];
    let state =
        parse_usb_input_report(&common::derived(&changes), &Calibration::UNCALIBRATED).unwrap();
    assert_eq!(
        state.touch,
        [
            Some(Finger {
                id: 5,
                x: 1919,
                y: 1079
            }),
            Some(Finger {
                id: 0x7F,
                x: 0x123,
                y: 0x045
            }),
        ]
    );
}

#[test]
fn a_contact_with_bit_7_set_is_no_finger() {
    let changes = [
        (33, 0x85),
        (34, 0x10),
        (37, 0x02),
        (38, 0x20),
        (39, 0x00),
        (40, 0x00),
    ];
    let state =
        parse_usb_input_report(&common::derived(&changes), &Calibration::UNCALIBRATED).unwrap();
    assert_eq!(state.touch[0], None);
    assert_eq!(
        state.touch[1],
        Some(Finger {
            id: 2,
            x: 0x20,
            y: 0
        })
    );
}
