mod common;

use heka_core::dualsense::{
    PRODUCT_ID_DUALSENSE, PRODUCT_ID_DUALSENSE_EDGE, RumbleMode, bluetooth_output_report,
    rumble_mode, usb_output_report,
};
use heka_core::feedback::{Feedback, Lightbar, PlayerLights, Rumble};
use heka_core::link::FeedbackFormat;
use heka_core::trigger_effect::{TriggerEffect, TriggerEffects};

fn rumble(large: u8, small: u8) -> Feedback {
    Feedback {
        rumble: Rumble { large, small },
        ..Feedback::OFF
    }
}

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
    assert_eq!(
        report.to_vec(),
        expected(&[(0, 0x02), (1, 0x03), (3, 0x40), (4, 0xC8)])
    );
}

#[test]
fn v2_rumble_selects_haptics_and_flags_compatible_vibration2() {
    let report = usb_output_report(&rumble(0xC8, 0x40), RumbleMode::CompatibleV2);
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

#[test]
fn a_lightbar_colour_is_sent_with_the_rumble() {
    let feedback = Feedback {
        lightbar: Some(Lightbar {
            red: 0xFF,
            green: 0x10,
            blue: 0x80,
        }),
        ..rumble(0xC8, 0x40)
    };
    let report = usb_output_report(&feedback, RumbleMode::Compatible);
    assert_eq!(
        report.to_vec(),
        expected(&[
            (0, 0x02),
            (1, 0x03),
            (2, 0x04),
            (3, 0x40),
            (4, 0xC8),
            (45, 0xFF),
            (46, 0x10),
            (47, 0x80),
        ])
    );
}

#[test]
fn no_lightbar_leaves_the_controllers_lightbar_alone() {
    let report = usb_output_report(&rumble(0, 0), RumbleMode::Compatible);
    assert_eq!(report[2] & 0x04, 0);
}

#[test]
fn each_players_lights_are_sent_as_the_kernel_patterns_them() {
    for (lights, pattern) in [
        (PlayerLights::Off, 0x00),
        (PlayerLights::One, 0x04),
        (PlayerLights::Two, 0x0A),
        (PlayerLights::Three, 0x15),
        (PlayerLights::Four, 0x1B),
    ] {
        let feedback = Feedback {
            player_lights: Some(lights),
            ..Feedback::OFF
        };
        let report = usb_output_report(&feedback, RumbleMode::Compatible);
        assert_eq!(
            report.to_vec(),
            expected(&[(0, 0x02), (1, 0x03), (2, 0x10), (44, pattern)]),
            "{lights:?}"
        );
    }
}

#[test]
fn no_player_lights_leaves_the_controllers_own_alone() {
    let report = usb_output_report(&rumble(0, 0), RumbleMode::Compatible);
    assert_eq!((report[2] & 0x10, report[44]), (0, 0));
}

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
            &[
                (0, 0x31),
                (2, 0x10),
                (3, 0x02),
                (5, 0x40),
                (6, 0xC8),
                (41, 0x04)
            ],
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
    let mut format = FeedbackFormat::dualsense_bluetooth(RumbleMode::Compatible);
    let sequence: Vec<u8> = (0..18)
        .flat_map(|_| format.reports(&Feedback::OFF))
        .map(|report| report[1] >> 4)
        .collect();
    assert_eq!(
        sequence,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 1]
    );
}

#[test]
fn usb_feedback_is_one_usb_report() {
    let mut usb = FeedbackFormat::DualSenseUsb(RumbleMode::Compatible);
    assert_eq!(
        usb.reports(&rumble(1, 2)),
        [usb_output_report(&rumble(1, 2), RumbleMode::Compatible).to_vec()]
    );
}

fn coloured(red: u8, green: u8, blue: u8) -> Feedback {
    Feedback {
        lightbar: Some(Lightbar { red, green, blue }),
        ..Feedback::OFF
    }
}

#[test]
fn the_lightbar_setup_report_is_sent_until_it_has_been_written() {
    let mut format = FeedbackFormat::dualsense_bluetooth(RumbleMode::Compatible);
    let colour = coloured(0xFF, 0x10, 0x80);
    assert_eq!(format.reports(&colour).len(), 2);

    let again = format.reports(&colour);
    assert_eq!(again.len(), 2);

    format.written(&again[1]);
    let still = format.reports(&colour);
    assert_eq!(still.len(), 2, "only the setup report counts");

    format.written(&still[0]);
    assert_eq!(format.reports(&colour).len(), 1);
}

#[test]
fn the_first_bluetooth_colour_follows_a_lightbar_setup_report() {
    let mut format = FeedbackFormat::dualsense_bluetooth(RumbleMode::Compatible);
    let first = format.reports(&coloured(0xFF, 0x10, 0x80));
    for report in &first {
        format.written(report);
    }
    assert_eq!(
        first,
        [
            expected_bluetooth(&[(0, 0x31), (2, 0x10), (41, 0x02), (44, 0x02)], 0x3631_E643),
            expected_bluetooth(
                &[
                    (0, 0x31),
                    (1, 0x10),
                    (2, 0x10),
                    (3, 0x03),
                    (4, 0x04),
                    (47, 0xFF),
                    (48, 0x10),
                    (49, 0x80)
                ],
                0x9DDC_8BCE
            ),
        ]
    );

    let next = Feedback {
        rumble: Rumble { large: 0, small: 9 },
        ..coloured(0xFF, 0x10, 0x80)
    };
    assert_eq!(
        format.reports(&next),
        [expected_bluetooth(
            &[
                (0, 0x31),
                (1, 0x20),
                (2, 0x10),
                (3, 0x03),
                (4, 0x04),
                (5, 0x09),
                (47, 0xFF),
                (48, 0x10),
                (49, 0x80)
            ],
            0xD202_16EC
        )]
    );
}

#[test]
fn bluetooth_feedback_without_a_colour_needs_no_lightbar_setup() {
    let mut format = FeedbackFormat::dualsense_bluetooth(RumbleMode::Compatible);
    assert_eq!(format.reports(&rumble(1, 2)).len(), 1);
}

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

#[test]
fn the_mute_light_is_set_without_touching_the_microphone() {
    for (lit, value) in [(false, 0x00), (true, 0x01)] {
        let feedback = Feedback {
            mute_light: Some(lit),
            ..Feedback::OFF
        };
        let report = usb_output_report(&feedback, RumbleMode::Compatible);
        assert_eq!(
            report.to_vec(),
            expected(&[(0, 0x02), (1, 0x03), (2, 0x01), (9, value)])
        );
    }
}

#[test]
fn no_mute_light_leaves_the_controllers_own_alone() {
    let report = usb_output_report(&rumble(1, 2), RumbleMode::Compatible);
    assert_eq!((report[2] & 0x01, report[9]), (0, 0));
}

#[test]
fn a_right_trigger_effect_is_flagged_and_sent_beside_the_rumble() {
    let feedback = Feedback {
        trigger_effects: TriggerEffects {
            left: None,
            right: Some(TriggerEffect::Break {
                start: 20,
                end: 70,
                strength: 100,
            }),
        },
        ..rumble(0xC8, 0x40)
    };
    let report = usb_output_report(&feedback, RumbleMode::Compatible);
    assert_eq!(
        report.to_vec(),
        expected(&[
            (0, 0x02),
            (1, 0x07),
            (3, 0x40),
            (4, 0xC8),
            (11, 0x25),
            (12, 0x84),
            (14, 0x07),
        ])
    );
}

#[test]
fn both_triggers_and_the_lights_fit_in_one_report() {
    let feedback = Feedback {
        trigger_effects: TriggerEffects {
            left: Some(TriggerEffect::Off),
            right: Some(TriggerEffect::Off),
        },
        player_lights: Some(PlayerLights::One),
        mute_light: Some(true),
        ..coloured(1, 2, 3)
    };
    let report = usb_output_report(&feedback, RumbleMode::CompatibleV2);
    assert_eq!(
        report.to_vec(),
        expected(&[
            (0, 0x02),
            (1, 0x0E),
            (2, 0x15),
            (9, 0x01),
            (11, 0x05),
            (22, 0x05),
            (39, 0x04),
            (44, 0x04),
            (45, 1),
            (46, 2),
            (47, 3),
        ])
    );
}

#[test]
fn a_bluetooth_report_carries_the_mute_light_and_trigger_effects() {
    let feedback = Feedback {
        mute_light: Some(true),
        trigger_effects: TriggerEffects {
            left: Some(TriggerEffect::Resistance {
                start: 30,
                strength: 60,
            }),
            right: None,
        },
        ..Feedback::OFF
    };
    let report = bluetooth_output_report(&feedback, RumbleMode::Compatible, 0);
    assert_eq!(
        report.to_vec(),
        expected_bluetooth(
            &[
                (0, 0x31),
                (2, 0x10),
                (3, 0x0B),
                (4, 0x01),
                (11, 0x01),
                (24, 0x21),
                (25, 0xF8),
                (26, 0x03),
                (28, 0x48),
                (29, 0x92),
                (30, 0x24),
            ],
            0x99E6_5970
        )
    );
}
