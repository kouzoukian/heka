mod common;

use heka_core::calibration::Calibration;
use heka_core::dualsense::{parse_bluetooth_input_report, parse_usb_input_report};
use heka_core::state::{Battery, Buttons, Charge, ControllerState, Finger, Stick};

fn captured(name: &str) -> Vec<ControllerState> {
    common::captured_input(name)
        .iter()
        .map(|report| parse_usb_input_report(report, &Calibration::UNCALIBRATED).unwrap())
        .collect()
}

fn rests(stick: Stick) -> bool {
    let near = |value: u8| (0x70..=0x90).contains(&value);
    near(stick.x) && near(stick.y)
}

#[test]
fn captured_resting_report_shows_nothing_in_use() {
    for state in captured("rest") {
        assert_eq!(state.buttons, Buttons::NONE);
        assert!(rests(state.left_stick) && rests(state.right_stick));
        assert_eq!((state.left_trigger, state.right_trigger), (0, 0));
        assert_eq!(state.touch, [None, None]);
    }
}

#[test]
fn captured_reports_show_each_button_alone() {
    let cases = [
        ("cross", Buttons::CROSS),
        ("circle", Buttons::CIRCLE),
        ("square", Buttons::SQUARE),
        ("triangle", Buttons::TRIANGLE),
        ("dpad_up", Buttons::DPAD_UP),
        ("dpad_right", Buttons::DPAD_RIGHT),
        ("dpad_down", Buttons::DPAD_DOWN),
        ("dpad_left", Buttons::DPAD_LEFT),
        ("l1", Buttons::L1),
        ("r1", Buttons::R1),
        ("l2", Buttons::L2),
        ("r2", Buttons::R2),
        ("l3", Buttons::L3),
        ("r3", Buttons::R3),
        ("create", Buttons::CREATE),
        ("options", Buttons::OPTIONS),
        ("ps", Buttons::PS),
        ("touchpad", Buttons::TOUCHPAD),
        ("mute", Buttons::MUTE),
    ];
    for (name, button) in cases {
        for state in captured(name) {
            assert_eq!(state.buttons, button, "{name}");
        }
    }
}

#[test]
fn captured_reports_show_each_trigger_pulled_all_the_way() {
    for state in captured("l2") {
        assert_eq!((state.left_trigger, state.right_trigger), (255, 0));
    }
    for state in captured("r2") {
        assert_eq!((state.left_trigger, state.right_trigger), (0, 255));
    }
}

#[test]
fn captured_reports_show_each_stick_pushed_to_each_edge() {
    type Reached = fn(Stick) -> bool;
    let across = |value: u8| (0x40..=0xC0).contains(&value);
    let cases: [(&str, Reached); 4] = [
        ("up", |stick| stick.y < 0x10),
        ("right", |stick| stick.x > 0xEF),
        ("down", |stick| stick.y > 0xEF),
        ("left", |stick| stick.x < 0x10),
    ];
    for (direction, reached) in cases {
        let along_x = matches!(direction, "left" | "right");
        for side in ["left", "right"] {
            let name = format!("{side}_stick_{direction}");
            for state in captured(&name) {
                let (pushed, other) = match side {
                    "left" => (state.left_stick, state.right_stick),
                    _ => (state.right_stick, state.left_stick),
                };
                let aside = if along_x { pushed.y } else { pushed.x };
                assert!(reached(pushed) && across(aside), "{name}: {pushed:?}");
                assert!(rests(other), "{name}: {other:?}");
                assert_eq!(state.buttons, Buttons::NONE, "{name}");
            }
        }
    }
}

#[test]
fn captured_touchpad_click_shows_one_finger_on_the_left_half() {
    for state in captured("touchpad") {
        let [Some(finger), None] = state.touch else {
            panic!("one finger: {:?}", state.touch);
        };
        assert!(finger.x < 960, "{finger:?}");
    }
}

#[test]
fn captured_reports_show_two_fingers_in_opposite_corners() {
    for state in captured("two_fingers") {
        let [Some(first), Some(second)] = state.touch else {
            panic!("two fingers: {:?}", state.touch);
        };
        assert_ne!(first.id, second.id);
        let mut fingers: [Finger; 2] = [first, second];
        fingers.sort_by_key(|finger| finger.x);
        let [left, right] = fingers;
        assert!(left.x < 960 && left.y < 540, "{left:?}");
        assert!(right.x >= 960 && right.y >= 540, "{right:?}");
    }
}

#[test]
fn captured_bluetooth_reports_pass_their_crc_and_show_cross_held() {
    for report in common::captured_bluetooth_cross() {
        let state = parse_bluetooth_input_report(&report, &Calibration::UNCALIBRATED).unwrap();
        assert_eq!(state.buttons, Buttons::CROSS);
        assert!(rests(state.left_stick) && rests(state.right_stick));
        assert_eq!((state.left_trigger, state.right_trigger), (0, 0));
        assert_eq!(
            state.battery,
            Some(Battery {
                percent: 85,
                charge: Charge::Discharging
            })
        );
    }
}
