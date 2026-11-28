mod common;

use heka_core::feedback::{Feedback, Lightbar, Rumble};
use heka_core::link::FeedbackFormat;
use heka_core::trigger_effect::{TriggerEffect, TriggerEffects};

fn feedback(large: u8, small: u8, colour: Option<(u8, u8, u8)>) -> Feedback {
    Feedback {
        rumble: Rumble { large, small },
        lightbar: colour.map(|(red, green, blue)| Lightbar { red, green, blue }),
        ..Feedback::OFF
    }
}

fn zeros_but(len: usize, bytes: &[(usize, u8)]) -> Vec<u8> {
    let mut report = vec![0u8; len];
    for &(offset, value) in bytes {
        report[offset] = value;
    }
    report
}

fn usb(bytes: &[(usize, u8)]) -> Vec<u8> {
    zeros_but(32, &[&[(0, 0x05)], bytes].concat())
}

fn bluetooth(bytes: &[(usize, u8)]) -> Vec<u8> {
    let mut report = zeros_but(78, &[&[(0, 0x11), (1, 0xC4)], bytes].concat());
    common::seal(0xA2, &mut report);
    report
}

#[test]
fn usb_rumble_drives_the_left_motor_from_the_large_motor_and_leaves_the_lightbar() {
    let reports = FeedbackFormat::DualShock4Usb.reports(&feedback(0xC8, 0x40, None));
    assert_eq!(reports, [usb(&[(1, 0x01), (4, 0x40), (5, 0xC8)])]);
}

#[test]
fn a_usb_lightbar_colour_is_sent_with_the_rumble() {
    let colour = Some((0x11, 0x22, 0x33));
    let reports = FeedbackFormat::DualShock4Usb.reports(&feedback(0xC8, 0x40, colour));
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

#[test]
fn a_dualshock4_ignores_the_mute_light_and_trigger_effects() {
    let dualsense_only = Feedback {
        mute_light: Some(true),
        trigger_effects: TriggerEffects {
            left: Some(TriggerEffect::Resistance {
                start: 30,
                strength: 60,
            }),
            right: Some(TriggerEffect::Off),
        },
        ..feedback(0xC8, 0x40, Some((1, 2, 3)))
    };
    let plain = feedback(0xC8, 0x40, Some((1, 2, 3)));

    for mut format in [
        FeedbackFormat::DualShock4Usb,
        FeedbackFormat::DualShock4Bluetooth,
    ] {
        assert_eq!(format.reports(&dualsense_only), format.reports(&plain));
    }
}
