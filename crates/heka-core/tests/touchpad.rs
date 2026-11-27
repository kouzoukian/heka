mod rig;

use heka_core::desktop::{DesktopEvent, MouseButton};
use heka_core::profile::{Binding, Input, Mapper, Profile};
use heka_core::state::{Buttons, ControllerState, Finger};
use heka_core::touchpad::{TouchpadChange, TouchpadSettings, TouchpadUse};
use rig::{Rig, clicks, moved, scrolled};

fn mouse(changes: &[TouchpadChange]) -> Profile {
    let mut profile = Profile::named("Desk");
    profile.change_touchpad(TouchpadChange::Use(TouchpadUse::Mouse));
    for &change in changes {
        profile.change_touchpad(change);
    }
    profile
}

fn finger(id: u8, x: u16, y: u16) -> Option<Finger> {
    Some(Finger { id, x, y })
}

fn touching(first: Option<Finger>, second: Option<Finger>) -> ControllerState {
    ControllerState {
        touch: [first, second],
        ..ControllerState::NEUTRAL
    }
}

fn one(x: u16, y: u16) -> ControllerState {
    touching(finger(1, x, y), None)
}

fn two(y: u16) -> ControllerState {
    touching(finger(1, 600, y), finger(2, 900, y))
}

fn lifted() -> ControllerState {
    ControllerState::NEUTRAL
}

const LEFT_CLICK: [DesktopEvent; 2] = [
    DesktopEvent::Button {
        button: MouseButton::Left,
        down: true,
    },
    DesktopEvent::Button {
        button: MouseButton::Left,
        down: false,
    },
];

const RIGHT_CLICK: [DesktopEvent; 2] = [
    DesktopEvent::Button {
        button: MouseButton::Right,
        down: true,
    },
    DesktopEvent::Button {
        button: MouseButton::Right,
        down: false,
    },
];

fn tap(rig: &mut Rig, touch: ControllerState, held_for: u64) {
    rig.report_after(4, touch);
    rig.report_after(held_for, touch);
    rig.report_after(4, lifted());
    rig.report_after(40, lifted());
    rig.report_after(4, lifted());
}

#[test]
fn the_touchpad_goes_to_the_virtual_controller_unless_it_moves_the_mouse() {
    let mut rig = Rig::new(&Profile::default());

    let shown = rig.report_after(0, one(100, 100));
    rig.report_after(4, one(300, 100));

    assert_eq!(shown.touch, one(100, 100).touch);
    assert_eq!(rig.take(), []);
}

#[test]
fn a_touchpad_that_moves_the_mouse_is_not_shown_to_the_game() {
    let mut rig = Rig::new(&mouse(&[]));

    let shown = rig.report_after(0, two(100));

    assert_eq!(shown.touch, [None, None]);
}

#[test]
fn one_finger_moves_the_pointer_as_far_as_it_travels() {
    let mut rig = Rig::new(&mouse(&[]));

    rig.report_after(0, one(100, 500));
    rig.report_after(4, one(160, 480));
    rig.report_after(4, one(200, 450));

    assert_eq!(moved(&rig.take()), (100, -50));
}

#[test]
fn sensitivity_and_vertical_inversion_shape_the_movement() {
    let mut rig = Rig::new(&mouse(&[
        TouchpadChange::Sensitivity(50),
        TouchpadChange::InvertY(true),
    ]));

    rig.report_after(0, one(100, 500));
    for step in 1..=100 {
        rig.report_after(4, one(100 + step, 500 + step));
    }

    assert_eq!(moved(&rig.take()), (50, -50));
}

#[test]
fn a_finger_that_lands_elsewhere_does_not_make_the_pointer_jump() {
    let mut rig = Rig::new(&mouse(&[]));

    rig.report_after(0, one(100, 100));
    rig.report_after(400, lifted());
    rig.report_after(4, touching(finger(2, 1500, 900), None));
    rig.report_after(4, touching(finger(3, 200, 200), None));

    assert_eq!(moved(&rig.take()), (0, 0));
}

#[test]
fn the_second_finger_slot_alone_moves_the_pointer_too() {
    let mut rig = Rig::new(&mouse(&[]));

    rig.report_after(0, touching(None, finger(2, 100, 100)));
    rig.report_after(4, touching(None, finger(2, 150, 100)));

    assert_eq!(moved(&rig.take()), (50, 0));
}

#[test]
fn a_short_still_touch_clicks_the_left_button() {
    let mut rig = Rig::new(&mouse(&[]));

    tap(&mut rig, one(500, 500), 100);

    assert_eq!(rig.take(), LEFT_CLICK);
}

#[test]
fn the_click_of_a_tap_is_held_briefly_even_without_further_reports() {
    let mut rig = Rig::new(&mouse(&[]));
    rig.report_after(0, one(500, 500));
    rig.report_after(50, lifted());
    assert_eq!(rig.take(), LEFT_CLICK[..1]);

    rig.tick_after(10);
    assert_eq!(rig.take(), []);

    rig.tick_after(30);
    assert_eq!(rig.take(), LEFT_CLICK[1..]);
}

#[test]
fn a_long_touch_or_one_that_moves_is_not_a_tap() {
    let mut long = Rig::new(&mouse(&[]));
    tap(&mut long, one(500, 500), 400);
    assert_eq!(clicks(&long.take()), []);

    let mut dragged = Rig::new(&mouse(&[]));
    dragged.report_after(0, one(500, 500));
    dragged.report_after(20, one(700, 500));
    dragged.report_after(20, one(500, 500));
    dragged.report_after(4, lifted());
    assert_eq!(clicks(&dragged.take()), []);
}

#[test]
fn a_tap_can_be_turned_off() {
    let mut rig = Rig::new(&mouse(&[TouchpadChange::TapClick(false)]));

    tap(&mut rig, one(500, 500), 100);

    assert_eq!(rig.take(), []);
}

#[test]
fn a_two_finger_tap_clicks_the_right_button_once_chosen() {
    let mut off = Rig::new(&mouse(&[]));
    tap(&mut off, two(500), 100);
    assert_eq!(off.take(), []);

    let mut on = Rig::new(&mouse(&[TouchpadChange::TwoFingerClick(true)]));
    on.report_after(0, one(600, 500));
    on.report_after(10, two(500));
    on.report_after(60, one(600, 500));
    on.report_after(10, lifted());
    on.report_after(40, lifted());
    assert_eq!(on.take(), RIGHT_CLICK);
}

#[test]
fn pressing_the_touchpad_down_is_not_also_a_tap() {
    let mut rig = Rig::new(&mouse(&[]));
    let pressed = ControllerState {
        buttons: Buttons::TOUCHPAD,
        ..one(500, 500)
    };

    rig.report_after(0, one(500, 500));
    rig.report_after(20, pressed);
    rig.report_after(20, one(500, 500));
    rig.report_after(20, lifted());

    assert_eq!(rig.take(), []);
}

#[test]
fn two_fingers_dragged_scroll_and_leave_the_pointer() {
    let mut rig = Rig::new(&mouse(&[]));

    rig.report_after(0, two(100));
    for step in 1..=60 {
        rig.report_after(4, two(100 + step * 10));
    }
    let events = rig.take();

    assert_eq!(moved(&events), (0, 0));
    assert_eq!(scrolled(&events), (0, 10));
}

#[test]
fn two_fingers_dragged_sideways_scroll_sideways() {
    let mut rig = Rig::new(&mouse(&[]));
    let at = |x| touching(finger(1, x, 500), finger(2, x + 300, 500));

    rig.report_after(0, at(800));
    rig.report_after(4, at(680));

    assert_eq!(scrolled(&rig.take()), (2, 0));
}

#[test]
fn the_finger_left_after_scrolling_does_not_move_the_pointer() {
    let mut rig = Rig::new(&mouse(&[]));

    rig.report_after(0, two(100));
    rig.report_after(4, one(600, 100));
    rig.report_after(4, one(900, 400));

    assert_eq!(moved(&rig.take()), (0, 0));
}

#[test]
fn a_click_binding_keeps_working_and_its_drag_is_released_with_everything_else() {
    let mut profile = mouse(&[]);
    profile.bind(Input::Touchpad, Binding::Mouse(MouseButton::Left));
    let mut rig = Rig::new(&profile);
    let dragging = |x| ControllerState {
        buttons: Buttons::TOUCHPAD,
        ..one(x, 500)
    };

    rig.report_after(0, dragging(100));
    rig.report_after(4, dragging(150));
    let events = rig.take();
    assert_eq!(clicks(&events), LEFT_CLICK[..1]);
    assert_eq!(moved(&events), (50, 0));

    rig.let_go();
    assert_eq!(rig.take(), LEFT_CLICK[1..]);
}

#[test]
fn a_changed_mapper_drops_a_tap_in_progress() {
    let mut rig = Rig::new(&mouse(&[]));
    rig.report_after(0, one(500, 500));

    rig.change(Mapper::new(&mouse(&[TouchpadChange::Sensitivity(200)])));
    rig.report_after(20, one(500, 500));
    rig.report_after(20, lifted());

    assert_eq!(rig.take(), []);
}

#[test]
fn an_unbound_touchpad_click_still_reaches_the_game() {
    let mut rig = Rig::new(&mouse(&[]));
    let pressed = ControllerState {
        buttons: Buttons::TOUCHPAD,
        ..ControllerState::NEUTRAL
    };

    assert_eq!(rig.report_after(0, pressed).buttons, Buttons::TOUCHPAD);
}

#[test]
fn touchpad_settings_survive_a_save_and_load_and_stay_out_of_a_plain_file() {
    let profile = mouse(&[
        TouchpadChange::Sensitivity(150),
        TouchpadChange::TapClick(false),
        TouchpadChange::TwoFingerClick(true),
        TouchpadChange::InvertY(true),
    ]);

    assert_eq!(Profile::load(&profile.to_json(), "x"), Some(profile));
    let plain: serde_json::Value =
        serde_json::from_str(&Profile::named("Plain").to_json()).unwrap();
    assert!(plain.get("touchpad").is_none());
}

#[test]
fn broken_touchpad_settings_fall_back_field_by_field() {
    let text = r#"{"touchpad": {"use": "mouse", "sensitivity": 5, "tap_click": "yes"}}"#;

    let profile = Profile::load(text, "Desk").unwrap();

    assert_eq!(
        *profile.touchpad(),
        TouchpadSettings {
            used_as: TouchpadUse::Mouse,
            sensitivity: 25,
            ..TouchpadSettings::default()
        }
    );
}
