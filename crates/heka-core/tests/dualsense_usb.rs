//! DualSense USB input report in, Controller state out.

mod common;

use heka_core::calibration::Calibration;
use heka_core::dualsense::parse_usb_input_report;
use heka_core::link::ReportError;
use heka_core::state::{Buttons, ControllerState, Stick};

fn parse(report: &[u8]) -> Result<ControllerState, ReportError> {
    parse_usb_input_report(report, &Calibration::UNCALIBRATED)
}

#[test]
fn captured_idle_reports_show_nothing_pressed() {
    for report in common::captured_idle() {
        let state = parse(&report).unwrap();
        assert_eq!(state.buttons, Buttons::NONE);
        assert_eq!(state.left_trigger, 0);
        assert_eq!(state.right_trigger, 0);
    }
}

// DERIVED reports below. Offsets are from `struct dualsense_input_report` in Linux
// `hid-playstation.c`, plus one for the report ID: byte 8 is buttons[0] (hat in the
// low nibble, 8 = released; Square, Cross, Circle, Triangle in bits 4..7), byte 9 is
// buttons[1] (L1, R1, L2, R2, Create, Options, L3, R3 in bits 0..7), byte 10 is
// buttons[2] (PS, touchpad, mic mute in bits 0..2).

#[test]
fn derived_reports_show_each_button() {
    let cases = [
        ((8, 0x18), Buttons::SQUARE),
        ((8, 0x28), Buttons::CROSS),
        ((8, 0x48), Buttons::CIRCLE),
        ((8, 0x88), Buttons::TRIANGLE),
        ((9, 0x01), Buttons::L1),
        ((9, 0x02), Buttons::R1),
        ((9, 0x04), Buttons::L2),
        ((9, 0x08), Buttons::R2),
        ((9, 0x10), Buttons::CREATE),
        ((9, 0x20), Buttons::OPTIONS),
        ((9, 0x40), Buttons::L3),
        ((9, 0x80), Buttons::R3),
        ((10, 0x01), Buttons::PS),
        ((10, 0x02), Buttons::TOUCHPAD),
        ((10, 0x04), Buttons::MUTE),
    ];
    for (change, expected) in cases {
        let state = parse(&common::derived(&[change])).unwrap();
        assert_eq!(
            state.buttons, expected,
            "byte {} = {:#04x}",
            change.0, change.1
        );
    }
}

#[test]
fn derived_reports_show_every_dpad_direction() {
    let up = Buttons::DPAD_UP;
    let down = Buttons::DPAD_DOWN;
    let left = Buttons::DPAD_LEFT;
    let right = Buttons::DPAD_RIGHT;
    let cases = [
        (0, up),
        (1, up | right),
        (2, right),
        (3, down | right),
        (4, down),
        (5, down | left),
        (6, left),
        (7, up | left),
        (8, Buttons::NONE),
        (15, Buttons::NONE),
    ];
    for (hat, expected) in cases {
        let state = parse(&common::derived(&[(8, hat)])).unwrap();
        assert_eq!(state.buttons, expected, "hat {hat}");
    }
}

#[test]
fn derived_report_with_everything_pressed_keeps_buttons_and_dpad_apart() {
    let state = parse(&common::derived(&[(8, 0xF2), (9, 0xFF), (10, 0x07)])).unwrap();
    for (button, name) in Buttons::NAMED {
        let expected = !matches!(name, "Up" | "Down" | "Left");
        assert_eq!(state.buttons.contains(button), expected, "{name}");
    }
}

#[test]
fn derived_reports_show_stick_extremes_and_trigger_travel() {
    let state = parse(&common::derived(&[
        (1, 0x00),
        (2, 0xFF),
        (3, 0xFF),
        (4, 0x00),
        (5, 0x40),
        (6, 0xFF),
    ]))
    .unwrap();
    assert_eq!(state.left_stick, Stick { x: 0x00, y: 0xFF });
    assert_eq!(state.right_stick, Stick { x: 0xFF, y: 0x00 });
    assert_eq!(state.left_trigger, 0x40);
    assert_eq!(state.right_trigger, 0xFF);
}

#[test]
fn reports_that_are_not_dualsense_usb_input_are_refused() {
    let baseline = common::captured_baseline();
    assert_eq!(
        parse(&baseline[..10]),
        Err(ReportError::TooShort { len: 10 })
    );
    assert_eq!(
        parse(&common::derived(&[(0, 0x31)])),
        Err(ReportError::UnexpectedId { id: 0x31 })
    );
}

#[test]
fn captured_baseline_shows_the_resting_sticks() {
    let state = parse(&common::captured_baseline()).unwrap();
    // The first captured line starts `01 7e 7d 7f 7c`.
    assert_eq!(state.left_stick, Stick { x: 0x7e, y: 0x7d });
    assert_eq!(state.right_stick, Stick { x: 0x7f, y: 0x7c });
}
