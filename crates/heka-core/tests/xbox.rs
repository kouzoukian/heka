//! DualSense USB input report in, Xbox (XUSB) report out.
//!
//! Expected button bits are the XInput `XINPUT_GAMEPAD_*` values from Microsoft's
//! `XINPUT_GAMEPAD` documentation, plus Guide = 0x0400 (ViGEmBus `XUSB_GAMEPAD_GUIDE`).

mod common;

use heka_core::dualsense::parse_usb_input_report;
use heka_core::xbox::XboxReport;

fn xbox(report: &[u8]) -> XboxReport {
    XboxReport::from_state(&parse_usb_input_report(report).unwrap())
}

#[test]
fn captured_idle_reports_press_nothing_and_rest_near_centre() {
    for report in common::captured_idle() {
        let xbox = xbox(&report);
        assert_eq!(xbox.buttons, 0);
        assert_eq!((xbox.left_trigger, xbox.right_trigger), (0, 0));
        for axis in [xbox.thumb_lx, xbox.thumb_ly, xbox.thumb_rx, xbox.thumb_ry] {
            assert!(axis.unsigned_abs() < 2048, "resting axis {axis}");
        }
    }
}

#[test]
fn captured_baseline_keeps_the_resting_stick_direction() {
    // Captured bytes: left x 0x7e (a little left), left y 0x7d (a little up),
    // right x 0x7f (a little left), right y 0x7c (a little up). Xbox Y is up-positive.
    let xbox = xbox(&common::captured_baseline());
    assert!(xbox.thumb_lx < 0);
    assert!(xbox.thumb_ly > 0);
    assert!(xbox.thumb_rx < 0);
    assert!(xbox.thumb_ry > 0);
}

// DERIVED reports: see tests/dualsense_usb.rs for the byte layout and its source.

#[test]
fn derived_reports_map_each_button_to_its_xbox_button() {
    let cases = [
        ((8, 0x28), 0x1000), // Cross -> A
        ((8, 0x48), 0x2000), // Circle -> B
        ((8, 0x18), 0x4000), // Square -> X
        ((8, 0x88), 0x8000), // Triangle -> Y
        ((9, 0x01), 0x0100), // L1 -> left shoulder
        ((9, 0x02), 0x0200), // R1 -> right shoulder
        ((9, 0x40), 0x0040), // L3 -> left thumb
        ((9, 0x80), 0x0080), // R3 -> right thumb
        ((9, 0x10), 0x0020), // Create -> Back (View)
        ((9, 0x20), 0x0010), // Options -> Start (Menu)
        ((10, 0x01), 0x0400), // PS -> Guide
        ((8, 0x00), 0x0001), // hat up -> d-pad up
        ((8, 0x04), 0x0002), // hat down -> d-pad down
        ((8, 0x06), 0x0004), // hat left -> d-pad left
        ((8, 0x02), 0x0008), // hat right -> d-pad right
        ((8, 0x01), 0x0009), // hat up-right -> d-pad up + right
    ];
    for (change, expected) in cases {
        assert_eq!(
            xbox(&common::derived(&[change])).buttons,
            expected,
            "byte {} = {:#04x}",
            change.0,
            change.1
        );
    }
}

#[test]
fn derived_touchpad_click_mute_and_trigger_clicks_press_nothing() {
    for change in [(10, 0x02), (10, 0x04), (9, 0x04), (9, 0x08)] {
        assert_eq!(xbox(&common::derived(&[change])).buttons, 0, "{change:?}");
    }
}

#[test]
fn derived_triggers_pass_their_travel_through() {
    let xbox = xbox(&common::derived(&[(5, 0x40), (6, 0xFF)]));
    assert_eq!((xbox.left_trigger, xbox.right_trigger), (0x40, 0xFF));
}

#[test]
fn derived_stick_extremes_reach_the_xbox_extremes_with_up_positive() {
    // Left stick fully left and up, right stick fully right and down.
    let xbox = xbox(&common::derived(&[(1, 0x00), (2, 0x00), (3, 0xFF), (4, 0xFF)]));
    assert_eq!(xbox.thumb_lx, -32768);
    assert_eq!(xbox.thumb_ly, 32767);
    assert_eq!(xbox.thumb_rx, 32767);
    assert!(xbox.thumb_ry <= -32767);
}

#[test]
fn derived_centred_sticks_are_exactly_centred() {
    let xbox = xbox(&common::derived(&[(1, 0x80), (2, 0x80), (3, 0x80), (4, 0x80)]));
    assert_eq!(
        [xbox.thumb_lx, xbox.thumb_ly, xbox.thumb_rx, xbox.thumb_ry],
        [0; 4]
    );
}

#[test]
fn the_neutral_state_is_the_all_zero_report() {
    let neutral = XboxReport::from_state(&heka_core::state::ControllerState::NEUTRAL);
    assert_eq!(neutral, XboxReport::default());
}
