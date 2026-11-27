use heka_core::profile::{Binding, DEFAULT_NAME, Input, Mapper, Output, Profile};
use heka_core::state::{Battery, Buttons, Charge, ControllerState, Finger, Stick};
use heka_core::virtual_controller::OutputType;

fn pressed(buttons: Buttons) -> ControllerState {
    ControllerState {
        buttons,
        ..ControllerState::NEUTRAL
    }
}

fn mapper(bindings: &[(Input, Binding)]) -> Mapper {
    let mut profile = Profile::named("Test");
    for (input, binding) in bindings {
        profile.bind(*input, *binding);
    }
    Mapper::new(&profile)
}

fn every_button() -> Buttons {
    Buttons::NAMED
        .iter()
        .fold(Buttons::NONE, |all, (button, _)| all | *button)
}

#[test]
fn the_default_profile_leaves_every_state_as_it_is() {
    let mapper = Mapper::new(&Profile::default());
    let busy = ControllerState {
        buttons: every_button(),
        left_stick: Stick { x: 3, y: 250 },
        right_stick: Stick { x: 200, y: 17 },
        left_trigger: 77,
        right_trigger: 130,
        battery: Some(Battery {
            percent: 60,
            charge: Charge::Discharging,
        }),
        touch: [
            Some(Finger {
                id: 4,
                x: 100,
                y: 900,
            }),
            None,
        ],
        ..ControllerState::NEUTRAL
    };

    for state in [ControllerState::NEUTRAL, busy] {
        assert_eq!(mapper.map(&state), state);
    }
    for (button, _) in Buttons::NAMED {
        assert_eq!(mapper.map(&pressed(button)), pressed(button));
    }
}

#[test]
fn the_default_profile_is_called_default_and_is_plain() {
    let profile = Profile::default();

    assert_eq!(profile.name, DEFAULT_NAME);
    assert!(profile.is_plain());
}

#[test]
fn a_button_can_drive_another_button() {
    let mapper = mapper(&[(Input::Cross, Binding::To(Output::Circle))]);

    assert_eq!(
        mapper.map(&pressed(Buttons::CROSS | Buttons::SQUARE)),
        pressed(Buttons::CIRCLE | Buttons::SQUARE)
    );
}

#[test]
fn buttons_can_swap_places() {
    let mapper = mapper(&[
        (Input::Cross, Binding::To(Output::Circle)),
        (Input::Circle, Binding::To(Output::Cross)),
    ]);

    assert_eq!(
        mapper.map(&pressed(Buttons::CROSS)),
        pressed(Buttons::CIRCLE)
    );
    assert_eq!(
        mapper.map(&pressed(Buttons::CIRCLE)),
        pressed(Buttons::CROSS)
    );
    assert_eq!(
        mapper.map(&pressed(Buttons::CROSS | Buttons::CIRCLE)),
        pressed(Buttons::CROSS | Buttons::CIRCLE)
    );
}

#[test]
fn a_button_can_drive_nothing() {
    let mapper = mapper(&[(Input::Triangle, Binding::Nothing)]);

    assert_eq!(
        mapper.map(&pressed(Buttons::TRIANGLE | Buttons::CROSS)),
        pressed(Buttons::CROSS)
    );
}

#[test]
fn two_buttons_can_drive_the_same_one() {
    let mapper = mapper(&[
        (Input::L1, Binding::To(Output::Cross)),
        (Input::R1, Binding::To(Output::Cross)),
    ]);

    assert_eq!(
        mapper.map(&pressed(Buttons::L1 | Buttons::R1)),
        pressed(Buttons::CROSS)
    );
    assert_eq!(mapper.map(&pressed(Buttons::R1)), pressed(Buttons::CROSS));
}

#[test]
fn a_d_pad_direction_can_drive_a_button_and_a_button_a_direction() {
    let mapper = mapper(&[
        (Input::DpadUp, Binding::To(Output::Triangle)),
        (Input::Square, Binding::To(Output::DpadLeft)),
    ]);

    assert_eq!(
        mapper.map(&pressed(Buttons::DPAD_UP | Buttons::SQUARE)),
        pressed(Buttons::TRIANGLE | Buttons::DPAD_LEFT)
    );
}

#[test]
fn the_extra_buttons_can_drive_virtual_buttons() {
    let mapper = mapper(&[
        (Input::Mute, Binding::To(Output::Ps)),
        (Input::LeftFn, Binding::To(Output::L3)),
        (Input::RightBack, Binding::To(Output::Options)),
    ]);

    assert_eq!(mapper.map(&pressed(Buttons::MUTE)), pressed(Buttons::PS));
    assert_eq!(mapper.map(&pressed(Buttons::LEFT_FN)), pressed(Buttons::L3));
    assert_eq!(
        mapper.map(&pressed(Buttons::RIGHT_BACK)),
        pressed(Buttons::OPTIONS)
    );
}

#[test]
fn a_button_can_press_a_trigger_all_the_way() {
    let mapper = mapper(&[(Input::Cross, Binding::To(Output::RightTrigger))]);

    let mapped = mapper.map(&pressed(Buttons::CROSS));

    assert_eq!(mapped.right_trigger, 255);
    assert_eq!(mapped.left_trigger, 0);
    assert_eq!(mapped.buttons, Buttons::R2);
}

#[test]
fn an_unbound_trigger_keeps_its_travel() {
    let mapper = mapper(&[(Input::Cross, Binding::To(Output::Circle))]);
    let state = ControllerState {
        left_trigger: 90,
        ..ControllerState::NEUTRAL
    };

    assert_eq!(mapper.map(&state).left_trigger, 90);
}

#[test]
fn a_rebound_trigger_gives_up_its_travel_and_its_click() {
    let mapper = mapper(&[(Input::L2, Binding::To(Output::Cross))]);
    let state = ControllerState {
        buttons: Buttons::L2,
        left_trigger: 255,
        right_trigger: 40,
        ..ControllerState::NEUTRAL
    };

    let mapped = mapper.map(&state);

    assert_eq!(mapped.buttons, Buttons::CROSS);
    assert_eq!(mapped.left_trigger, 0);
    assert_eq!(mapped.right_trigger, 40);
}

#[test]
fn a_silenced_trigger_sends_no_travel() {
    let mapper = mapper(&[(Input::R2, Binding::Nothing)]);
    let state = ControllerState {
        buttons: Buttons::R2,
        right_trigger: 200,
        ..ControllerState::NEUTRAL
    };

    let mapped = mapper.map(&state);

    assert_eq!(mapped.buttons, Buttons::NONE);
    assert_eq!(mapped.right_trigger, 0);
}

#[test]
fn a_trigger_can_drive_the_other_trigger() {
    let mapper = mapper(&[(Input::L2, Binding::To(Output::RightTrigger))]);
    let state = ControllerState {
        buttons: Buttons::L2,
        left_trigger: 255,
        ..ControllerState::NEUTRAL
    };

    let mapped = mapper.map(&state);

    assert_eq!(mapped.buttons, Buttons::R2);
    assert_eq!(mapped.left_trigger, 0);
    assert_eq!(mapped.right_trigger, 255);
}

fn click_at(x: u16) -> ControllerState {
    ControllerState {
        buttons: Buttons::TOUCHPAD,
        touch: [Some(Finger { id: 1, x, y: 500 }), None],
        ..ControllerState::NEUTRAL
    }
}

#[test]
fn the_whole_touchpad_click_can_be_rebound() {
    let mapper = mapper(&[(Input::Touchpad, Binding::To(Output::Options))]);

    assert_eq!(mapper.map(&click_at(100)).buttons, Buttons::OPTIONS);
    assert_eq!(mapper.map(&click_at(1800)).buttons, Buttons::OPTIONS);
}

#[test]
fn the_touchpad_halves_can_drive_different_buttons() {
    let mapper = mapper(&[
        (Input::TouchpadLeft, Binding::To(Output::L1)),
        (Input::TouchpadRight, Binding::To(Output::R1)),
    ]);

    assert_eq!(mapper.map(&click_at(100)).buttons, Buttons::L1);
    assert_eq!(mapper.map(&click_at(959)).buttons, Buttons::L1);
    assert_eq!(mapper.map(&click_at(960)).buttons, Buttons::R1);
    assert_eq!(mapper.map(&click_at(1800)).buttons, Buttons::R1);
}

#[test]
fn a_half_without_a_binding_follows_the_whole_touchpad() {
    let mapper = mapper(&[
        (Input::Touchpad, Binding::To(Output::Create)),
        (Input::TouchpadLeft, Binding::To(Output::L1)),
    ]);

    assert_eq!(mapper.map(&click_at(100)).buttons, Buttons::L1);
    assert_eq!(mapper.map(&click_at(1800)).buttons, Buttons::CREATE);
}

#[test]
fn a_click_without_a_finger_follows_the_whole_touchpad() {
    let mapper = mapper(&[(Input::TouchpadLeft, Binding::To(Output::L1))]);

    assert_eq!(
        mapper.map(&pressed(Buttons::TOUCHPAD)),
        pressed(Buttons::TOUCHPAD)
    );
}

#[test]
fn a_half_can_be_silenced_while_the_other_clicks_as_before() {
    let mapper = mapper(&[(Input::TouchpadLeft, Binding::Nothing)]);

    assert_eq!(mapper.map(&click_at(100)).buttons, Buttons::NONE);
    assert_eq!(mapper.map(&click_at(1800)).buttons, Buttons::TOUCHPAD);
}

#[test]
fn nothing_but_buttons_and_triggers_changes() {
    let mapper = mapper(&[(Input::Cross, Binding::To(Output::Circle))]);
    let state = ControllerState {
        buttons: Buttons::CROSS,
        left_stick: Stick { x: 1, y: 2 },
        right_stick: Stick { x: 3, y: 4 },
        battery: Some(Battery {
            percent: 40,
            charge: Charge::Charging,
        }),
        touch: [Some(Finger { id: 2, x: 5, y: 6 }), None],
        ..ControllerState::NEUTRAL
    };

    let mapped = mapper.map(&state);

    assert_eq!(
        mapped,
        ControllerState {
            buttons: Buttons::CIRCLE,
            ..state
        }
    );
}

#[test]
fn binding_back_to_default_forgets_the_binding() {
    let mut profile = Profile::named("Test");
    profile.bind(Input::Cross, Binding::To(Output::Circle));
    profile.bind(Input::Cross, Binding::Default);

    assert_eq!(profile.binding(Input::Cross), Binding::Default);
    assert!(profile.is_plain());
}

#[test]
fn a_profile_survives_a_save_and_load() {
    let mut profile = Profile::named("Racing");
    profile.output_type = Some(OutputType::Ps4);
    profile.bind(Input::Cross, Binding::To(Output::RightTrigger));
    profile.bind(Input::Mute, Binding::Nothing);
    profile.bind(Input::TouchpadLeft, Binding::To(Output::L1));

    let loaded = Profile::load(&profile.to_json(), "ignored");

    assert_eq!(loaded, Some(profile));
}

#[test]
fn the_file_names_the_bindings() {
    let mut profile = Profile::named("Racing");
    profile.bind(Input::Cross, Binding::To(Output::Circle));
    profile.bind(Input::Mute, Binding::Nothing);

    let json: serde_json::Value = serde_json::from_str(&profile.to_json()).unwrap();

    assert_eq!(json["name"], "Racing");
    assert_eq!(json["bindings"]["cross"], "circle");
    assert!(json["bindings"]["mute"].is_null());
    assert!(json["bindings"].get("mute").is_some());
    assert!(json.get("output_type").is_none());
}

#[test]
fn a_file_that_is_not_a_profile_loads_as_nothing() {
    for text in ["", "{", "[]", "3", "null", "\"profile\""] {
        assert_eq!(Profile::load(text, "Fallback"), None, "{text}");
    }
}

#[test]
fn a_broken_binding_is_dropped_and_the_rest_load() {
    let text = r#"{
        "name": "Mixed",
        "output_type": "ps4",
        "bindings": {
            "cross": "circle",
            "nonsense": "circle",
            "square": "nonsense",
            "triangle": 5,
            "mute": null,
            "l1": ["r1"]
        }
    }"#;

    let profile = Profile::load(text, "Fallback").unwrap();

    assert_eq!(profile.name, "Mixed");
    assert_eq!(profile.output_type, Some(OutputType::Ps4));
    assert_eq!(profile.binding(Input::Cross), Binding::To(Output::Circle));
    assert_eq!(profile.binding(Input::Mute), Binding::Nothing);
    assert_eq!(profile.binding(Input::Square), Binding::Default);
    assert_eq!(profile.binding(Input::Triangle), Binding::Default);
    assert_eq!(profile.binding(Input::L1), Binding::Default);
}

#[test]
fn a_missing_or_blank_name_falls_back_to_the_file_name() {
    for text in ["{}", r#"{"name": "   "}"#, r#"{"name": 4}"#] {
        let profile = Profile::load(text, "Racing").unwrap();
        assert_eq!(profile.name, "Racing", "{text}");
    }
    assert_eq!(Profile::load("{}", "  "), None);
}

#[test]
fn an_unknown_output_type_means_the_controllers_own() {
    let profile = Profile::load(r#"{"output_type": "switch"}"#, "Racing").unwrap();

    assert_eq!(profile.output_type, None);
}

#[test]
fn unknown_fields_are_ignored() {
    let profile = Profile::load(r#"{"name": "A", "from_the_future": [1, 2]}"#, "A").unwrap();

    assert!(profile.is_plain());
}

#[test]
fn a_name_is_trimmed_and_cut() {
    let long = "x".repeat(40);

    assert_eq!(Profile::named("  Racing  ").name, "Racing");
    assert_eq!(Profile::named(&long).name.chars().count(), 24);
    assert_eq!(Profile::named("   ").name, DEFAULT_NAME);

    let mut profile = Profile::named("Racing");
    profile.rename("   ");
    assert_eq!(profile.name, "Racing");
    profile.rename(" Shooter ");
    assert_eq!(profile.name, "Shooter");
}
