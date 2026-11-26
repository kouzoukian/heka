//! What the live drawing shows, from a Controller state.

use heka_app::ui::pad::{PadModel, TouchDot, stick_offset, trigger_fill};
use heka_core::state::{Buttons, ControllerState, Stick};

#[test]
fn the_drawing_takes_buttons_sticks_and_triggers_from_the_controller_state() {
    let state = ControllerState {
        buttons: Buttons::CROSS | Buttons::TOUCHPAD,
        left_stick: Stick { x: 0, y: 255 },
        right_stick: Stick::CENTER,
        left_trigger: 12,
        right_trigger: 255,
        ..ControllerState::NEUTRAL
    };

    let pad = PadModel::from_state(&state);

    assert!(pad.pressed(Buttons::CROSS));
    assert!(pad.pressed(Buttons::TOUCHPAD));
    assert!(!pad.pressed(Buttons::CIRCLE));
    assert_eq!(pad.left_stick, Stick { x: 0, y: 255 });
    assert_eq!(pad.right_stick, Stick::CENTER);
    assert_eq!((pad.left_trigger, pad.right_trigger), (12, 255));
}

#[test]
fn a_stick_moves_from_the_centre_to_the_full_edge_on_each_axis() {
    assert_eq!(stick_offset(Stick::CENTER), (0.0, 0.0));
    assert_eq!(stick_offset(Stick { x: 255, y: 0 }), (1.0, -1.0));
    assert_eq!(stick_offset(Stick { x: 0, y: 255 }), (-1.0, 1.0));
}

#[test]
fn a_trigger_fills_from_empty_to_full() {
    assert_eq!(trigger_fill(0), 0.0);
    assert_eq!(trigger_fill(51), 0.2);
    assert_eq!(trigger_fill(255), 1.0);
}

#[test]
fn a_finger_is_placed_as_a_share_of_the_touchpad_and_kept_on_it() {
    // A touchpad whose last units are 1919 by 1079.
    let at = |x, y| TouchDot::from_units(x, y, 1919, 1079).share();

    assert_eq!(at(0, 0), (0.0, 0.0));
    assert_eq!(at(1919, 1079), (1.0, 1.0));
    assert_eq!(at(4000, 4000), (1.0, 1.0));
    let (x, y) = at(960, 270);
    assert!((x - 0.5).abs() < 0.002, "{x}");
    assert!((y - 0.25).abs() < 0.002, "{y}");
}
