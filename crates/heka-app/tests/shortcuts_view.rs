use heka_app::text;
use heka_app::ui::shortcuts::{action_name, available, choosable};
use heka_core::profile::Input;
use heka_core::shortcuts::{ShortcutAction, Shortcuts};

#[test]
fn only_buttons_that_can_be_part_of_a_shortcut_are_offered() {
    let offered = choosable();

    for input in [
        Input::Ps,
        Input::L2,
        Input::R2,
        Input::TouchpadLeft,
        Input::TouchpadRight,
    ] {
        assert!(!offered.contains(&input), "{input:?}");
    }
    for input in [
        Input::Cross,
        Input::R1,
        Input::Options,
        Input::Mute,
        Input::Touchpad,
    ] {
        assert!(offered.contains(&input), "{input:?}");
    }
    assert_eq!(offered.len(), Input::ALL.len() - 5);
}

#[test]
fn a_button_taken_by_another_action_is_not_available_to_this_one() {
    let shortcuts = Shortcuts::default();

    assert!(available(
        &shortcuts,
        ShortcutAction::NextProfile,
        Input::R1
    ));
    assert!(!available(
        &shortcuts,
        ShortcutAction::PreviousProfile,
        Input::R1
    ));
    assert!(available(
        &shortcuts,
        ShortcutAction::PreviousProfile,
        Input::Cross
    ));
    assert!(!available(
        &shortcuts,
        ShortcutAction::NextProfile,
        Input::Options
    ));
}

#[test]
fn a_choice_is_written_as_ps_plus_the_button() {
    assert_eq!(text::shortcut_choice(Some(Input::R1)), "PS + R1");
    assert_eq!(text::shortcut_choice(Some(Input::Create)), "PS + Create");
    assert_eq!(text::shortcut_choice(None), "No button");
}

#[test]
fn every_action_has_its_own_name() {
    let names: Vec<&str> = ShortcutAction::ALL.into_iter().map(action_name).collect();

    assert_eq!(names, ["Next Profile", "Previous Profile", "Turn off"]);
}

#[test]
fn the_notice_names_the_controller_and_the_profile() {
    assert_eq!(
        text::shortcut_profile("DualSense 55:66", Some("Racing"), None),
        "Profile for DualSense 55:66: Racing"
    );
    assert_eq!(
        text::shortcut_profile("Sofa", None, None),
        "Profile for Sofa: Default"
    );
}

#[test]
fn under_a_rule_the_notice_says_the_choice_returns_afterwards() {
    assert_eq!(
        text::shortcut_profile("Sofa", Some("Racing"), Some("cs2.exe")),
        "Profile for Sofa: Racing (a rule applies: cs2.exe; it returns afterwards)"
    );
}
