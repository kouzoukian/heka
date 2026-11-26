//! DualSense Bluetooth input report in, Controller state out.
//!
//! No Bluetooth capture exists yet, so every report here is DERIVED from the
//! captured USB reports (see `common::bluetooth_from_usb` for the layout source).

mod common;

use heka_core::dualsense::{parse_bluetooth_input_report, parse_usb_input_report};
use heka_core::link::{ReportError, ReportFormat};
use heka_core::state::Buttons;

#[test]
fn a_bluetooth_report_carries_the_same_input_as_the_usb_report() {
    let pressed_low_battery = common::derived(&[(8, 0x28), (9, 0x01), (5, 0x40), (53, 0x02)]);
    for usb in common::captured_idle()
        .into_iter()
        .chain([pressed_low_battery])
    {
        let bluetooth = common::bluetooth_from_usb(&usb);
        assert_eq!(
            parse_bluetooth_input_report(&bluetooth),
            parse_usb_input_report(&usb)
        );
    }
    let state = ReportFormat::DualSenseBluetooth
        .parse(&common::bluetooth_from_usb(&common::derived(&[(8, 0x28)])))
        .unwrap();
    assert_eq!(state.buttons, Buttons::CROSS);
}

#[test]
fn a_bluetooth_report_with_a_wrong_crc_is_refused() {
    let mut report = common::bluetooth_from_usb(&common::derived(&[(8, 0x28)]));
    report[10] ^= 0x01;
    assert_eq!(
        parse_bluetooth_input_report(&report),
        Err(ReportError::BadCrc)
    );
}

#[test]
fn the_simple_bluetooth_report_and_short_reads_are_refused() {
    // Before full reports are enabled the DualSense sends report 0x01 over
    // Bluetooth (`dualsense_parse_report`: "Bluetooth uses a minimal HID report for
    // reportID 1").
    let full = common::bluetooth_from_usb(&common::captured_baseline());
    let mut simple = full.clone();
    simple[0] = 0x01;
    assert_eq!(
        parse_bluetooth_input_report(&simple),
        Err(ReportError::UnexpectedId { id: 0x01 })
    );
    assert_eq!(
        parse_bluetooth_input_report(&full[..10]),
        Err(ReportError::TooShort { len: 10 })
    );
}
