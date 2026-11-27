mod rig;

use heka_core::desktop::{Chord, DesktopEvent, Key, MouseButton, WheelStep};
use heka_core::macros::{Macro, MacroChange, Mode, Step, Target};
use heka_core::profile::{Binding, Input, Mapper, Output, Profile};
use heka_core::state::{Buttons, ControllerState};
use rig::Rig;

const Q: Key = Key::new(0x10);
const E: Key = Key::new(0x12);

fn down(key: Key) -> DesktopEvent {
    DesktopEvent::Key { key, down: true }
}

fn up(key: Key) -> DesktopEvent {
    DesktopEvent::Key { key, down: false }
}

fn press(key: Key) -> Step {
    Step::Press(Target::Keys(Chord::of(key)))
}

fn release(key: Key) -> Step {
    Step::Release(Target::Keys(Chord::of(key)))
}

fn bound(binding: Binding, mode: Mode) -> Profile {
    let mut profile = Profile::named("Macro");
    profile.bind(Input::Cross, binding);
    profile.set_mode(Input::Cross, mode);
    profile
}

fn with_macro(steps: &[Step], repeat: bool) -> Profile {
    let mut profile = Profile::named("Macro");
    for (index, &step) in steps.iter().enumerate() {
        profile.change_macro(Input::Cross, MacroChange::Add);
        profile.change_macro(Input::Cross, MacroChange::Set(index, step));
    }
    profile.change_macro(Input::Cross, MacroChange::Repeat(repeat));
    profile
}

fn cross() -> ControllerState {
    ControllerState {
        buttons: Buttons::CROSS,
        ..ControllerState::NEUTRAL
    }
}

fn rest() -> ControllerState {
    ControllerState::NEUTRAL
}

fn run_for(rig: &mut Rig, millis: u64, step: u64, state: ControllerState) -> Vec<DesktopEvent> {
    for _ in 0..millis / step {
        rig.report_after(step, state);
    }
    rig.take()
}

fn count(events: &[DesktopEvent], wanted: DesktopEvent) -> usize {
    events.iter().filter(|&&event| event == wanted).count()
}

#[test]
fn a_toggle_holds_after_one_press_and_lets_go_after_the_next() {
    let mut rig = Rig::new(&bound(Binding::Keys(Chord::of(Q)), Mode::Toggle));

    rig.report_after(0, cross());
    rig.report_after(4, rest());
    rig.report_after(4, rest());
    assert_eq!(rig.take(), [down(Q)]);

    rig.report_after(4, cross());
    assert_eq!(rig.take(), [up(Q)]);
    rig.report_after(4, cross());
    rig.report_after(4, rest());
    assert_eq!(rig.take(), []);
}

#[test]
fn a_toggle_keeps_a_virtual_button_pressed() {
    let mut rig = Rig::new(&bound(Binding::To(Output::Circle), Mode::Toggle));

    rig.report_after(0, cross());
    assert_eq!(rig.report_after(4, rest()).buttons, Buttons::CIRCLE);
    rig.report_after(4, cross());
    assert_eq!(rig.report_after(4, rest()).buttons, Buttons::NONE);
}

#[test]
fn turbo_repeats_at_the_chosen_rate_while_held() {
    for step in [1, 4, 8, 10] {
        let mut rig = Rig::new(&bound(Binding::Keys(Chord::of(Q)), Mode::Turbo(5)));
        rig.report_after(0, cross());

        let events = run_for(&mut rig, 960, step, cross());

        assert_eq!(count(&events, down(Q)), 5, "{step}");
        assert_eq!(count(&events, up(Q)), 5, "{step}");
    }
}

#[test]
fn turbo_starts_pressed_and_ends_with_the_button() {
    let mut rig = Rig::new(&bound(Binding::Mouse(MouseButton::Left), Mode::Turbo(10)));
    let click = |down| DesktopEvent::Button {
        button: MouseButton::Left,
        down,
    };

    rig.report_after(0, cross());
    assert_eq!(rig.take(), [click(true)]);
    rig.report_after(49, cross());
    assert_eq!(rig.take(), []);
    rig.report_after(1, cross());
    assert_eq!(rig.take(), [click(false)]);
    rig.report_after(50, cross());
    assert_eq!(rig.take(), [click(true)]);
    rig.report_after(4, rest());
    assert_eq!(rig.take(), [click(false)]);
    rig.report_after(500, rest());
    assert_eq!(rig.take(), []);
}

#[test]
fn turbo_repeats_a_virtual_button_even_between_reports() {
    let mut rig = Rig::new(&bound(Binding::To(Output::Circle), Mode::Turbo(10)));
    assert_eq!(rig.report_after(0, cross()).buttons, Buttons::CIRCLE);

    assert_eq!(rig.tick_after(10), None);
    assert_eq!(
        rig.tick_after(45).map(|shown| shown.buttons),
        Some(Buttons::NONE)
    );
    assert_eq!(
        rig.tick_after(50).map(|shown| shown.buttons),
        Some(Buttons::CIRCLE)
    );
}

#[test]
fn turbo_turns_the_wheel_once_each_time() {
    let mut rig = Rig::new(&bound(Binding::Wheel(WheelStep::Down), Mode::Turbo(10)));
    rig.report_after(0, cross());

    let events = run_for(&mut rig, 280, 4, cross());

    assert_eq!(count(&events, DesktopEvent::Wheel(WheelStep::Down)), 3);
}

#[test]
fn a_macro_runs_its_steps_in_time_once_per_press() {
    let steps = [press(Q), Step::Wait(50), release(Q), press(E), release(E)];
    let mut rig = Rig::new(&with_macro(&steps, false));

    rig.report_after(0, cross());
    assert_eq!(rig.take(), [down(Q)]);
    rig.report_after(49, cross());
    assert_eq!(rig.take(), []);
    rig.report_after(1, cross());
    assert_eq!(rig.take(), [up(Q), down(E), up(E)]);
    assert_eq!(run_for(&mut rig, 400, 4, cross()), []);

    rig.report_after(4, rest());
    rig.report_after(4, cross());
    assert_eq!(rig.take(), [down(Q)]);
}

#[test]
fn a_macro_drives_nothing_on_the_virtual_controller() {
    let mut rig = Rig::new(&with_macro(&[press(Q)], false));

    assert_eq!(rig.report_after(0, cross()).buttons, Buttons::NONE);
}

#[test]
fn a_macro_runs_on_time_however_often_reports_arrive() {
    let steps = [press(Q), Step::Wait(25), release(Q), Step::Wait(25)];
    for step in [1, 4, 7, 11] {
        let mut rig = Rig::new(&with_macro(&steps, true));
        rig.report_after(0, cross());

        let events = run_for(&mut rig, 990, step, cross());

        assert_eq!(count(&events, down(Q)), 20, "{step}");
    }
}

#[test]
fn a_macro_goes_on_between_reports() {
    let mut rig = Rig::new(&with_macro(&[press(Q), Step::Wait(20), release(Q)], false));
    rig.report_after(0, cross());
    rig.take();

    rig.tick_after(20);

    assert_eq!(rig.take(), [up(Q)]);
}

#[test]
fn a_repeating_macro_without_waits_does_not_run_away() {
    let mut rig = Rig::new(&with_macro(&[press(Q), release(Q)], true));
    rig.report_after(0, cross());

    let events = run_for(&mut rig, 100, 1, cross());

    assert_eq!(count(&events, down(Q)), 11);
}

#[test]
fn a_macro_catching_up_after_a_stall_does_not_burst() {
    let mut rig = Rig::new(&with_macro(
        &[press(Q), Step::Wait(10), release(Q), Step::Wait(10)],
        true,
    ));
    rig.report_after(0, cross());
    rig.take();

    rig.report_after(5000, cross());

    assert!(rig.take().len() <= 2);
}

#[test]
fn letting_go_cancels_a_macro_and_releases_what_it_held() {
    let steps = [press(Q), Step::Wait(500), press(E)];
    let mut rig = Rig::new(&with_macro(&steps, false));
    rig.report_after(0, cross());

    rig.report_after(100, rest());
    assert_eq!(rig.take(), [down(Q), up(Q)]);
    assert_eq!(run_for(&mut rig, 800, 4, rest()), []);
}

#[test]
fn a_key_a_macro_leaves_pressed_is_held_until_the_button_is_let_go() {
    let mut rig = Rig::new(&with_macro(&[press(Q)], false));

    rig.report_after(0, cross());
    assert_eq!(run_for(&mut rig, 200, 4, cross()), [down(Q)]);
    rig.report_after(4, rest());

    assert_eq!(rig.take(), [up(Q)]);
}

#[test]
fn a_changed_mapper_cancels_macros_and_toggles_until_the_next_press() {
    let mut profile = with_macro(&[press(Q), Step::Wait(500), release(Q)], true);
    profile.bind(Input::Circle, Binding::Keys(Chord::of(E)));
    profile.set_mode(Input::Circle, Mode::Toggle);
    let mut rig = Rig::new(&profile);
    let both = ControllerState {
        buttons: Buttons::CROSS | Buttons::CIRCLE,
        ..ControllerState::NEUTRAL
    };
    rig.report_after(0, both);
    rig.take();

    rig.change(Mapper::new(&profile));
    let mut released = rig.take();
    released.sort_by_key(|event| format!("{event:?}"));
    assert_eq!(released.len(), 2);
    assert_eq!(run_for(&mut rig, 100, 4, both), []);

    rig.report_after(4, rest());
    rig.report_after(4, cross());
    assert_eq!(rig.take(), [down(Q)]);
}

#[test]
fn ending_releases_what_macros_and_toggles_hold() {
    let mut profile = with_macro(&[press(Q)], false);
    profile.bind(Input::Circle, Binding::Keys(Chord::of(E)));
    profile.set_mode(Input::Circle, Mode::Toggle);
    let mut rig = Rig::new(&profile);
    rig.report_after(
        0,
        ControllerState {
            buttons: Buttons::CROSS | Buttons::CIRCLE,
            ..ControllerState::NEUTRAL
        },
    );
    rig.take();

    rig.let_go();

    let released = rig.take();
    assert!(
        released.contains(&up(Q)) && released.contains(&up(E)),
        "{released:?}"
    );
    assert_eq!(rig.tick_after(10), None);
    assert_eq!(rig.take(), []);
}

#[test]
fn a_macro_shares_a_key_with_a_binding() {
    let mut profile = with_macro(&[press(Q), Step::Wait(20), release(Q)], false);
    profile.bind(Input::Circle, Binding::Keys(Chord::of(Q)));
    let mut rig = Rig::new(&profile);
    let circle = Buttons::CIRCLE;
    let holding = |buttons| ControllerState {
        buttons,
        ..ControllerState::NEUTRAL
    };

    rig.report_after(0, holding(circle));
    rig.report_after(4, holding(circle | Buttons::CROSS));
    rig.report_after(40, holding(circle | Buttons::CROSS));
    assert_eq!(rig.take(), [down(Q)]);
}

#[test]
fn a_mode_applies_only_to_a_button_key_or_mouse_binding() {
    let mut profile = Profile::named("Modes");
    profile.set_mode(Input::Cross, Mode::Toggle);
    assert!(profile.is_plain());

    profile.bind(Input::Cross, Binding::To(Output::Circle));
    profile.set_mode(Input::Cross, Mode::Turbo(100));
    assert_eq!(profile.mode(Input::Cross), Mode::Turbo(30));
    profile.bind(Input::Cross, Binding::Keys(Chord::of(Q)));
    assert_eq!(profile.mode(Input::Cross), Mode::Turbo(30));
    profile.bind(Input::Cross, Binding::Nothing);
    profile.set_mode(Input::Cross, Mode::Toggle);
    assert_eq!(profile.mode(Input::Cross), Mode::Hold);
}

#[test]
fn macro_steps_are_edited_by_hand() {
    let mut profile = with_macro(&[press(Q), Step::Wait(50), release(Q)], false);
    let steps = |profile: &Profile| profile.macro_of(Input::Cross).unwrap().steps().to_vec();

    profile.change_macro(Input::Cross, MacroChange::MoveUp(1));
    assert_eq!(steps(&profile), [Step::Wait(50), press(Q), release(Q)]);
    profile.change_macro(Input::Cross, MacroChange::MoveDown(1));
    assert_eq!(steps(&profile), [Step::Wait(50), release(Q), press(Q)]);
    profile.change_macro(Input::Cross, MacroChange::Remove(0));
    assert_eq!(steps(&profile), [release(Q), press(Q)]);
    profile.change_macro(Input::Cross, MacroChange::Set(0, Step::Wait(60_000)));
    assert_eq!(steps(&profile)[0], Step::Wait(10_000));
    for change in [
        MacroChange::MoveUp(0),
        MacroChange::MoveDown(1),
        MacroChange::Remove(9),
        MacroChange::Set(9, press(E)),
    ] {
        profile.change_macro(Input::Cross, change);
    }
    assert_eq!(steps(&profile).len(), 2);
    for _ in 0..100 {
        profile.change_macro(Input::Cross, MacroChange::Add);
    }
    assert_eq!(steps(&profile).len(), 32);
    assert_eq!(profile.binding(Input::Cross), Binding::Macro);
    profile.bind(Input::Cross, Binding::Default);
    assert_eq!(profile.macro_of(Input::Cross), None);
    assert!(profile.is_plain());
}

#[test]
fn modes_and_macros_survive_a_save_and_load() {
    let mut profile = with_macro(
        &[
            press(Q),
            Step::Wait(50),
            Step::Press(Target::Mouse(MouseButton::Right)),
            release(Q),
        ],
        true,
    );
    profile.bind(Input::Circle, Binding::To(Output::Cross));
    profile.set_mode(Input::Circle, Mode::Toggle);
    profile.bind(Input::Square, Binding::Mouse(MouseButton::Left));
    profile.set_mode(Input::Square, Mode::Turbo(15));

    let loaded = Profile::load(&profile.to_json(), "ignored");

    assert_eq!(loaded, Some(profile));
}

#[test]
fn the_file_writes_modes_and_macros_plainly() {
    let mut profile = with_macro(
        &[
            press(Q),
            Step::Wait(50),
            Step::Release(Target::Mouse(MouseButton::Left)),
        ],
        false,
    );
    profile.bind(Input::Circle, Binding::To(Output::Cross));
    profile.set_mode(Input::Circle, Mode::Turbo(15));

    let json: serde_json::Value = serde_json::from_str(&profile.to_json()).unwrap();

    assert_eq!(
        json["bindings"]["cross"],
        serde_json::json!({
            "macro": [{"press": [16]}, {"wait": 50}, {"release": "left"}],
            "repeat": false
        })
    );
    assert_eq!(
        json["bindings"]["circle"],
        serde_json::json!({"button": "cross", "mode": "turbo", "rate": 15})
    );
}

#[test]
fn broken_macro_steps_and_modes_are_dropped() {
    let text = r#"{"bindings": {
        "cross": {"macro": [{"press": 16}, {"wait": 0}, {"jump": 1}, {"press": "thumb"},
                            {"release": [16]}, 7], "repeat": "yes"},
        "circle": {"button": "cross", "mode": "sticky"},
        "square": {"keys": [16], "mode": "turbo", "rate": 500},
        "triangle": {"macro": 4}
    }}"#;

    let profile = Profile::load(text, "Macro").unwrap();

    let steps = profile.macro_of(Input::Cross).unwrap();
    assert_eq!(steps.steps(), [press(Q), Step::Wait(1), release(Q)]);
    assert!(!steps.repeat);
    assert_eq!(profile.binding(Input::Circle), Binding::To(Output::Cross));
    assert_eq!(profile.mode(Input::Circle), Mode::Hold);
    assert_eq!(profile.mode(Input::Square), Mode::Turbo(30));
    assert_eq!(profile.binding(Input::Triangle), Binding::Default);
}

#[test]
fn an_empty_macro_does_nothing() {
    let mut profile = Profile::named("Macro");
    profile.change_macro(Input::Cross, MacroChange::Repeat(true));
    let mut rig = Rig::new(&profile);

    rig.report_after(0, cross());
    assert_eq!(run_for(&mut rig, 100, 4, cross()), []);
    assert_eq!(profile.macro_of(Input::Cross), Some(&Macro::repeating()));
}
