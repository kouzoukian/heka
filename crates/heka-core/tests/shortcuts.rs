use std::time::{Duration, Instant};

use heka_core::app_settings::AppSettings;
use heka_core::profile::Input;
use heka_core::settings::Settings;
use heka_core::shortcuts::{
    ShortcutAction, ShortcutChange, ShortcutDetector, Shortcuts, TURN_OFF_HOLD, can_be_chorded,
    cycle,
};
use heka_core::state::Buttons;

fn on() -> Shortcuts {
    Shortcuts {
        enabled: true,
        ..Shortcuts::default()
    }
}

struct Pad {
    detector: ShortcutDetector,
    shortcuts: Shortcuts,
    start: Instant,
}

impl Pad {
    fn new(shortcuts: Shortcuts) -> Self {
        Self {
            detector: ShortcutDetector::default(),
            shortcuts,
            start: Instant::now(),
        }
    }

    fn at(&mut self, millis: u64, buttons: Buttons) -> (Buttons, Option<ShortcutAction>) {
        let now = self.start + Duration::from_millis(millis);
        let seen = self.detector.observe(&self.shortcuts, buttons, now);
        (seen.hide, seen.action)
    }
}

const PS: Buttons = Buttons::PS;
const R1: Buttons = Buttons::R1;
const L1: Buttons = Buttons::L1;
const OPTIONS: Buttons = Buttons::OPTIONS;

fn both(a: Buttons, b: Buttons) -> Buttons {
    a | b
}

#[test]
fn shortcuts_are_off_and_neutral_by_default() {
    let shortcuts = Shortcuts::default();

    assert!(!shortcuts.enabled);
    assert!(shortcuts.is_default());
    assert_eq!(shortcuts.next, Some(Input::R1));
    assert_eq!(shortcuts.previous, Some(Input::L1));
    assert_eq!(shortcuts.turn_off, Some(Input::Options));
}

#[test]
fn nothing_is_hidden_or_recognised_while_shortcuts_are_off() {
    let mut pad = Pad::new(Shortcuts::default());

    for millis in [0, 10, 20, 30] {
        assert_eq!(pad.at(millis, PS), (Buttons::NONE, None));
        assert_eq!(pad.at(millis + 1, both(PS, R1)), (Buttons::NONE, None));
    }
}

#[test]
fn ps_then_a_button_recognises_the_shortcut_once_and_hides_the_button() {
    let mut pad = Pad::new(on());

    assert_eq!(pad.at(0, PS), (Buttons::NONE, None));
    assert_eq!(
        pad.at(4, both(PS, R1)),
        (R1, Some(ShortcutAction::NextProfile))
    );
    assert_eq!(pad.at(8, both(PS, R1)), (R1, None));
    assert_eq!(pad.at(12, both(PS, R1)), (R1, None));
}

#[test]
fn the_button_is_shown_again_once_it_is_let_go_and_pressed_again_it_fires_again() {
    let mut pad = Pad::new(on());
    pad.at(0, PS);
    pad.at(4, both(PS, R1));

    assert_eq!(pad.at(8, PS), (Buttons::NONE, None));
    assert_eq!(
        pad.at(12, both(PS, R1)),
        (R1, Some(ShortcutAction::NextProfile))
    );
}

#[test]
fn the_other_button_first_and_ps_after_is_not_a_shortcut() {
    let mut pad = Pad::new(on());

    assert_eq!(pad.at(0, R1), (Buttons::NONE, None));
    assert_eq!(pad.at(4, both(PS, R1)), (Buttons::NONE, None));
    assert_eq!(pad.at(8, both(PS, R1)), (Buttons::NONE, None));
}

#[test]
fn ps_on_its_own_is_never_hidden_or_recognised() {
    let mut pad = Pad::new(on());

    for millis in [0, 4, 8, 12] {
        assert_eq!(pad.at(millis, PS), (Buttons::NONE, None));
    }
    assert_eq!(pad.at(16, Buttons::NONE), (Buttons::NONE, None));
}

#[test]
fn a_button_that_is_not_a_shortcut_passes_with_ps() {
    let mut pad = Pad::new(on());

    pad.at(0, PS);

    assert_eq!(pad.at(4, both(PS, Buttons::CROSS)), (Buttons::NONE, None));
}

#[test]
fn a_recognised_button_stays_hidden_until_let_go_even_after_ps_is() {
    let mut pad = Pad::new(on());
    pad.at(0, PS);
    pad.at(4, both(PS, R1));

    assert_eq!(pad.at(8, R1), (R1, None));
    assert_eq!(pad.at(12, Buttons::NONE), (Buttons::NONE, None));
}

#[test]
fn the_first_state_ever_seen_does_not_count_as_a_press() {
    let mut pad = Pad::new(on());

    assert_eq!(pad.at(0, both(PS, R1)), (Buttons::NONE, None));
    assert_eq!(pad.at(4, both(PS, R1)), (Buttons::NONE, None));
}

#[test]
fn the_previous_profile_has_its_own_button() {
    let mut pad = Pad::new(on());
    pad.at(0, PS);

    assert_eq!(
        pad.at(4, both(PS, L1)),
        (L1, Some(ShortcutAction::PreviousProfile))
    );
}

#[test]
fn two_shortcut_buttons_pressed_together_give_one_action_and_both_are_hidden() {
    let mut pad = Pad::new(on());
    pad.at(0, PS);

    let (hidden, action) = pad.at(4, both(both(PS, R1), L1));

    assert_eq!(action, Some(ShortcutAction::NextProfile));
    assert!(hidden.contains(R1) && hidden.contains(L1));
}

#[test]
fn turning_off_needs_about_two_seconds_of_holding() {
    let mut pad = Pad::new(on());
    pad.at(0, PS);

    assert_eq!(pad.at(4, both(PS, OPTIONS)), (OPTIONS, None));
    assert_eq!(pad.at(1000, both(PS, OPTIONS)), (OPTIONS, None));
    assert_eq!(pad.at(1999 + 4, both(PS, OPTIONS)), (OPTIONS, None));
    assert_eq!(
        pad.at(2000 + 4, both(PS, OPTIONS)),
        (OPTIONS, Some(ShortcutAction::TurnOff))
    );
    assert_eq!(pad.at(2500, both(PS, OPTIONS)), (OPTIONS, None));
    assert_eq!(pad.at(9000, both(PS, OPTIONS)), (OPTIONS, None));
    assert_eq!(TURN_OFF_HOLD, Duration::from_secs(2));
}

#[test]
fn letting_go_of_ps_before_the_time_is_up_cancels_turning_off() {
    let mut pad = Pad::new(on());
    pad.at(0, PS);
    pad.at(4, both(PS, OPTIONS));

    assert_eq!(pad.at(1500, OPTIONS), (OPTIONS, None));
    assert_eq!(pad.at(2500, OPTIONS), (OPTIONS, None));
    assert_eq!(pad.at(3500, both(PS, OPTIONS)), (OPTIONS, None));
}

#[test]
fn letting_go_of_the_button_before_the_time_is_up_cancels_turning_off() {
    let mut pad = Pad::new(on());
    pad.at(0, PS);
    pad.at(4, both(PS, OPTIONS));

    assert_eq!(pad.at(1500, PS), (Buttons::NONE, None));
    assert_eq!(pad.at(2500, PS), (Buttons::NONE, None));
}

#[test]
fn holding_again_after_turning_off_counts_a_new_two_seconds() {
    let mut pad = Pad::new(on());
    pad.at(0, PS);
    pad.at(4, both(PS, OPTIONS));
    assert_eq!(
        pad.at(2100, both(PS, OPTIONS)).1,
        Some(ShortcutAction::TurnOff)
    );
    pad.at(2200, PS);

    assert_eq!(pad.at(3000, both(PS, OPTIONS)).1, None);
    assert_eq!(pad.at(4900, both(PS, OPTIONS)).1, None);
    assert_eq!(
        pad.at(5100, both(PS, OPTIONS)).1,
        Some(ShortcutAction::TurnOff)
    );
}

#[test]
fn switching_shortcuts_off_forgets_a_hold_in_progress() {
    let mut pad = Pad::new(on());
    pad.at(0, PS);
    pad.at(4, both(PS, OPTIONS));

    pad.shortcuts = Shortcuts::default();
    assert_eq!(pad.at(100, both(PS, OPTIONS)), (Buttons::NONE, None));
    pad.shortcuts = on();

    assert_eq!(pad.at(3000, both(PS, OPTIONS)), (Buttons::NONE, None));
}

#[test]
fn the_buttons_can_be_chosen_but_not_twice_and_not_ps() {
    let mut shortcuts = on();

    assert!(shortcuts.change(ShortcutChange::Choose(
        ShortcutAction::NextProfile,
        Input::Cross
    )));
    assert_eq!(shortcuts.next, Some(Input::Cross));
    assert!(!shortcuts.change(ShortcutChange::Choose(
        ShortcutAction::PreviousProfile,
        Input::Cross
    )));
    assert_eq!(shortcuts.previous, Some(Input::L1));
    assert!(!shortcuts.change(ShortcutChange::Choose(ShortcutAction::TurnOff, Input::Ps)));
    assert!(shortcuts.change(ShortcutChange::Choose(
        ShortcutAction::NextProfile,
        Input::Cross
    )));
    assert_eq!(
        shortcuts.taken_by(Input::Cross),
        Some(ShortcutAction::NextProfile)
    );
    assert_eq!(shortcuts.taken_by(Input::Mute), None);
}

#[test]
fn some_buttons_cannot_be_part_of_a_shortcut() {
    for input in [
        Input::Ps,
        Input::TouchpadLeft,
        Input::TouchpadRight,
        Input::L2,
        Input::R2,
    ] {
        assert!(!can_be_chorded(input), "{input:?}");
    }
    for input in [
        Input::Cross,
        Input::L1,
        Input::Options,
        Input::Mute,
        Input::Touchpad,
        Input::L3,
    ] {
        assert!(can_be_chorded(input), "{input:?}");
    }
}

#[test]
fn switching_shortcuts_on_and_off_is_a_change_too() {
    let mut shortcuts = Shortcuts::default();

    assert!(shortcuts.change(ShortcutChange::Enabled(true)));
    assert!(shortcuts.enabled);
    assert!(shortcuts.change(ShortcutChange::Enabled(false)));
    assert!(shortcuts.is_default());
}

fn loaded(app: &str) -> Shortcuts {
    let text = format!(r#"{{"app": {{"shortcuts": {app}}}}}"#);
    Settings::load(Some(&text)).app().shortcuts
}

#[test]
fn shortcuts_survive_a_save_and_load() {
    let mut settings = Settings::default();
    settings.set_app(AppSettings {
        shortcuts: Shortcuts {
            enabled: true,
            next: Some(Input::Cross),
            previous: None,
            turn_off: Some(Input::Mute),
        },
        ..AppSettings::default()
    });

    let loaded = Settings::load(Some(&settings.to_json()));

    assert_eq!(
        loaded.app().shortcuts,
        Shortcuts {
            enabled: true,
            next: Some(Input::Cross),
            previous: None,
            turn_off: Some(Input::Mute),
        }
    );
}

#[test]
fn default_shortcuts_write_nothing_and_a_file_without_them_loads_the_defaults() {
    let json = Settings::default().to_json();

    assert!(!json.contains("shortcuts"));
    assert_eq!(
        Settings::load(Some(&json)).app().shortcuts,
        Shortcuts::default()
    );
}

#[test]
fn a_file_may_name_some_of_the_choices() {
    let shortcuts = loaded(r#"{"enabled": true, "next": "square"}"#);

    assert!(shortcuts.enabled);
    assert_eq!(shortcuts.next, Some(Input::Square));
    assert_eq!(shortcuts.previous, Some(Input::L1));
    assert_eq!(shortcuts.turn_off, Some(Input::Options));
}

#[test]
fn a_button_given_twice_stays_with_the_first_action() {
    let shortcuts = loaded(r#"{"next": "cross", "previous": "cross", "turn_off": "cross"}"#);

    assert_eq!(shortcuts.next, Some(Input::Cross));
    assert_eq!(shortcuts.previous, None);
    assert_eq!(shortcuts.turn_off, None);
}

#[test]
fn a_default_button_taken_by_an_earlier_action_leaves_the_later_one_unset() {
    let shortcuts = loaded(r#"{"next": "options"}"#);

    assert_eq!(shortcuts.next, Some(Input::Options));
    assert_eq!(shortcuts.turn_off, None);
}

#[test]
fn broken_choices_are_unset_and_never_fail_the_load() {
    let shortcuts =
        loaded(r#"{"enabled": "yes", "next": "ps", "previous": 5, "turn_off": "touchpad_left"}"#);

    assert!(!shortcuts.enabled);
    assert_eq!(shortcuts.next, None);
    assert_eq!(shortcuts.previous, None);
    assert_eq!(shortcuts.turn_off, None);
}

#[test]
fn an_explicit_null_leaves_an_action_without_a_button() {
    let shortcuts = loaded(r#"{"turn_off": null}"#);

    assert_eq!(shortcuts.turn_off, None);
    assert_eq!(shortcuts.next, Some(Input::R1));
}

#[test]
fn shortcuts_that_are_not_an_object_load_as_the_defaults() {
    for text in ["5", "null", "[]", "\"on\""] {
        assert_eq!(loaded(text), Shortcuts::default(), "{text}");
    }
}

#[test]
fn an_action_without_a_button_never_fires() {
    let mut pad = Pad::new(Shortcuts {
        enabled: true,
        next: None,
        ..Shortcuts::default()
    });
    pad.at(0, PS);

    assert_eq!(pad.at(4, both(PS, R1)), (Buttons::NONE, None));
}

#[test]
fn cycling_goes_through_the_default_profile_and_the_profiles_and_wraps() {
    let ids = ["racing", "shooter", "souls"];

    assert_eq!(cycle(None, &ids, true).as_deref(), Some("racing"));
    assert_eq!(
        cycle(Some("racing"), &ids, true).as_deref(),
        Some("shooter")
    );
    assert_eq!(cycle(Some("souls"), &ids, true), None);
    assert_eq!(cycle(None, &ids, false).as_deref(), Some("souls"));
    assert_eq!(cycle(Some("racing"), &ids, false), None);
    assert_eq!(
        cycle(Some("shooter"), &ids, false).as_deref(),
        Some("racing")
    );
}

#[test]
fn a_choice_that_is_gone_cycles_from_the_default_profile() {
    let ids = ["racing", "shooter"];

    assert_eq!(cycle(Some("gone"), &ids, true).as_deref(), Some("racing"));
    assert_eq!(cycle(Some("gone"), &ids, false).as_deref(), Some("shooter"));
}

#[test]
fn with_no_profiles_cycling_stays_on_the_default_one() {
    assert_eq!(cycle(None, &[], true), None);
    assert_eq!(cycle(None, &[], false), None);
    assert_eq!(cycle(Some("gone"), &[], true), None);
}
