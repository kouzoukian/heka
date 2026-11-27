use heka_core::profile::{Binding, Input, Mapper, Output, Profile};
use heka_core::state::{Buttons, ControllerState};
use heka_core::tuning::{Curve, TriggerChange, TriggerSide, TriggerTuning};
use heka_core::xbox::XboxReport;

fn pulled(left: u8, right: u8) -> ControllerState {
    ControllerState {
        left_trigger: left,
        right_trigger: right,
        ..ControllerState::NEUTRAL
    }
}

fn left(travel: u8) -> ControllerState {
    pulled(travel, 0)
}

fn mapper(left: TriggerTuning, right: TriggerTuning, bindings: &[(Input, Binding)]) -> Mapper {
    let mut profile = Profile::named("Test");
    profile.left_trigger = left;
    profile.right_trigger = right;
    for (input, binding) in bindings {
        profile.bind(*input, *binding);
    }
    Mapper::new(&profile)
}

fn tuned(tuning: TriggerTuning) -> Mapper {
    mapper(tuning, TriggerTuning::default(), &[])
}

fn within(value: u8, low: u8, high: u8) {
    assert!(
        (low..=high).contains(&value),
        "{value} is not within {low}..={high}"
    );
}

#[test]
fn neutral_settings_leave_every_travel_and_report_identical() {
    let plain = Mapper::new(&Profile::default());
    let only_a_press_point = tuned(TriggerTuning {
        press_point: Some(30),
        ..TriggerTuning::default()
    });

    for travel in 0..=255u8 {
        let input = pulled(travel, 255 - travel);
        for mapper in [&plain, &only_a_press_point] {
            let mapped = mapper.map(&input);
            assert_eq!(mapped.left_trigger, travel);
            assert_eq!(mapped.right_trigger, 255 - travel);
            assert_eq!(mapped.buttons, input.buttons);
            let report = XboxReport::from_state(&mapped);
            assert_eq!(report, XboxReport::from_state(&input));
        }
    }
}

#[test]
fn the_default_tuning_is_neutral_and_any_change_is_not() {
    assert!(TriggerTuning::default().is_neutral());
    for changed in [
        TriggerTuning {
            dead_zone: 1,
            ..TriggerTuning::default()
        },
        TriggerTuning {
            max_zone: 99,
            ..TriggerTuning::default()
        },
        TriggerTuning {
            curve: Curve::Soft,
            ..TriggerTuning::default()
        },
        TriggerTuning {
            hair_trigger: Some(50),
            ..TriggerTuning::default()
        },
        TriggerTuning {
            press_point: Some(50),
            ..TriggerTuning::default()
        },
    ] {
        assert!(!changed.is_neutral(), "{changed:?}");
    }
}

#[test]
fn a_press_point_alone_does_not_shape_the_travel() {
    let tuning = TriggerTuning {
        press_point: Some(40),
        ..TriggerTuning::default()
    };

    assert!(!tuning.shapes_travel());
    assert!(
        TriggerTuning {
            hair_trigger: Some(40),
            ..TriggerTuning::default()
        }
        .shapes_travel()
    );
}

#[test]
fn nothing_and_everything_stay_nothing_and_everything() {
    let tuning = TriggerTuning {
        dead_zone: 10,
        max_zone: 90,
        curve: Curve::Soft,
        ..TriggerTuning::default()
    };

    assert_eq!(tuning.tune(0), 0);
    assert_eq!(tuning.tune(255), 255);
}

#[test]
fn a_dead_zone_swallows_the_first_of_the_travel_and_rescales_the_rest() {
    let tuning = TriggerTuning {
        dead_zone: 20,
        ..TriggerTuning::default()
    };

    assert_eq!(tuning.tune(50), 0);
    within(tuning.tune(60), 10, 12);
    within(tuning.tune(153), 127, 129);
}

#[test]
fn a_max_zone_reaches_full_travel_early() {
    let tuning = TriggerTuning {
        max_zone: 60,
        ..TriggerTuning::default()
    };

    assert_eq!(tuning.tune(153), 255);
    assert_eq!(tuning.tune(200), 255);
    within(tuning.tune(128), 212, 214);
}

#[test]
fn each_curve_bends_the_travel_in_its_own_way() {
    let at = |curve| {
        TriggerTuning {
            curve,
            ..TriggerTuning::default()
        }
        .tune(128)
    };

    within(at(Curve::Linear), 127, 129);
    within(at(Curve::Soft), 63, 65);
    within(at(Curve::Softer), 31, 33);
    within(at(Curve::Sharp), 191, 193);
    within(at(Curve::Smooth), 127, 129);
}

#[test]
fn the_tuned_travel_never_falls_as_the_pull_deepens() {
    for curve in Curve::ALL {
        let tuning = TriggerTuning {
            dead_zone: 8,
            max_zone: 92,
            curve,
            ..TriggerTuning::default()
        };
        let mut last = 0;
        for travel in 0..=255u8 {
            let value = tuning.tune(travel);
            assert!(value >= last, "{curve:?} at {travel}");
            last = value;
        }
    }
}

#[test]
fn a_hair_trigger_reads_as_fully_pressed_from_its_point() {
    let tuning = TriggerTuning {
        hair_trigger: Some(30),
        ..TriggerTuning::default()
    };

    within(tuning.tune(76), 75, 77);
    assert_eq!(tuning.tune(77), 255);
    assert_eq!(tuning.tune(120), 255);
    assert_eq!(tuning.tune(0), 0);
    assert!(!tuning.fires_hair_trigger(76));
    assert!(tuning.fires_hair_trigger(77));
}

#[test]
fn a_hair_trigger_cannot_fire_inside_the_dead_zone() {
    let tuning = TriggerTuning {
        dead_zone: 40,
        hair_trigger: Some(20),
        ..TriggerTuning::default()
    };

    assert_eq!(tuning.tune(100), 0);
    assert!(!tuning.fires_hair_trigger(100));
    assert_eq!(tuning.tune(110), 255);
}

#[test]
fn a_hair_trigger_at_the_very_start_fires_with_any_pull() {
    let tuning = TriggerTuning {
        hair_trigger: Some(1),
        ..TriggerTuning::default()
    };

    assert_eq!(tuning.tune(0), 0);
    assert_eq!(tuning.tune(3), 255);
}

#[test]
fn an_unbound_trigger_is_tuned_and_the_other_one_is_left_alone() {
    let mapper = tuned(TriggerTuning {
        curve: Curve::Soft,
        ..TriggerTuning::default()
    });

    let mapped = mapper.map(&pulled(128, 128));

    within(mapped.left_trigger, 63, 65);
    assert_eq!(mapped.right_trigger, 128);
}

#[test]
fn each_trigger_has_its_own_tuning() {
    let mapper = mapper(
        TriggerTuning {
            curve: Curve::Soft,
            ..TriggerTuning::default()
        },
        TriggerTuning {
            curve: Curve::Sharp,
            ..TriggerTuning::default()
        },
        &[],
    );

    let mapped = mapper.map(&pulled(128, 128));

    within(mapped.left_trigger, 63, 65);
    within(mapped.right_trigger, 191, 193);
}

#[test]
fn a_hair_trigger_also_presses_the_button_of_an_unbound_trigger() {
    let mapper = tuned(TriggerTuning {
        hair_trigger: Some(30),
        ..TriggerTuning::default()
    });

    assert_eq!(mapper.map(&left(100)).buttons, Buttons::L2);
    assert_eq!(mapper.map(&left(50)).buttons, Buttons::NONE);
    assert_eq!(mapper.map(&pulled(0, 100)).buttons, Buttons::NONE);
}

#[test]
fn the_controllers_own_click_is_kept_beside_a_hair_trigger() {
    let mapper = tuned(TriggerTuning {
        hair_trigger: Some(90),
        ..TriggerTuning::default()
    });
    let clicked = ControllerState {
        buttons: Buttons::L2,
        left_trigger: 60,
        ..ControllerState::NEUTRAL
    };

    assert_eq!(mapper.map(&clicked).buttons, Buttons::L2);
}

#[test]
fn a_rebound_trigger_still_gives_up_its_travel_whatever_its_tuning() {
    let mapper = mapper(
        TriggerTuning {
            hair_trigger: Some(10),
            ..TriggerTuning::default()
        },
        TriggerTuning::default(),
        &[(Input::L2, Binding::To(Output::Cross))],
    );
    let state = ControllerState {
        buttons: Buttons::L2,
        left_trigger: 200,
        ..ControllerState::NEUTRAL
    };

    let mapped = mapper.map(&state);

    assert_eq!(mapped.left_trigger, 0);
    assert_eq!(mapped.buttons, Buttons::CROSS);
}

#[test]
fn a_button_that_presses_a_trigger_fully_is_not_tuned() {
    let mapper = mapper(
        TriggerTuning {
            dead_zone: 30,
            curve: Curve::Softer,
            ..TriggerTuning::default()
        },
        TriggerTuning::default(),
        &[(Input::Cross, Binding::To(Output::LeftTrigger))],
    );
    let state = ControllerState {
        buttons: Buttons::CROSS,
        ..ControllerState::NEUTRAL
    };

    assert_eq!(mapper.map(&state).left_trigger, 255);
}

#[test]
fn a_bound_trigger_counts_as_pressed_from_its_press_point() {
    let mapper = mapper(
        TriggerTuning {
            press_point: Some(50),
            ..TriggerTuning::default()
        },
        TriggerTuning::default(),
        &[(Input::L2, Binding::To(Output::Cross))],
    );

    assert_eq!(mapper.map(&left(127)).buttons, Buttons::NONE);
    assert_eq!(mapper.map(&left(128)).buttons, Buttons::CROSS);
    assert_eq!(mapper.map(&left(255)).buttons, Buttons::CROSS);
}

#[test]
fn a_bound_trigger_ignores_the_controllers_own_click_when_it_has_a_press_point() {
    let mapper = mapper(
        TriggerTuning {
            press_point: Some(80),
            ..TriggerTuning::default()
        },
        TriggerTuning::default(),
        &[(Input::L2, Binding::To(Output::Cross))],
    );
    let clicked = ControllerState {
        buttons: Buttons::L2,
        left_trigger: 100,
        ..ControllerState::NEUTRAL
    };

    assert_eq!(mapper.map(&clicked).buttons, Buttons::NONE);
}

#[test]
fn a_bound_trigger_without_a_press_point_follows_the_controllers_click() {
    let mapper = mapper(
        TriggerTuning::default(),
        TriggerTuning::default(),
        &[(Input::L2, Binding::To(Output::Cross))],
    );
    let clicked = ControllerState {
        buttons: Buttons::L2,
        left_trigger: 100,
        ..ControllerState::NEUTRAL
    };

    assert_eq!(mapper.map(&clicked).buttons, Buttons::CROSS);
    assert_eq!(mapper.map(&left(255)).buttons, Buttons::NONE);
}

#[test]
fn a_press_point_does_not_touch_an_unbound_trigger() {
    let mapper = tuned(TriggerTuning {
        press_point: Some(90),
        ..TriggerTuning::default()
    });
    let clicked = ControllerState {
        buttons: Buttons::L2,
        left_trigger: 30,
        ..ControllerState::NEUTRAL
    };

    assert_eq!(mapper.map(&clicked).buttons, Buttons::L2);
    assert_eq!(mapper.map(&left(250)).buttons, Buttons::NONE);
}

#[test]
fn the_right_trigger_has_its_own_press_point() {
    let mapper = mapper(
        TriggerTuning::default(),
        TriggerTuning {
            press_point: Some(20),
            ..TriggerTuning::default()
        },
        &[(Input::R2, Binding::To(Output::Circle))],
    );

    assert_eq!(mapper.map(&pulled(255, 50)).buttons, Buttons::NONE);
    assert_eq!(mapper.map(&pulled(0, 52)).buttons, Buttons::CIRCLE);
}

#[test]
fn the_reports_carry_the_tuned_travel_as_bytes() {
    let mapper = tuned(TriggerTuning {
        curve: Curve::Soft,
        ..TriggerTuning::default()
    });
    let mapped = mapper.map(&pulled(128, 0));
    let mut translator = heka_core::ps4::Ps4Translator::default();

    let xbox = XboxReport::from_state(&mapped);
    let ps4 = translator.report(&mapped);

    assert_eq!(xbox.left_trigger, mapped.left_trigger);
    assert_eq!(ps4[7], mapped.left_trigger);
    within(xbox.left_trigger, 63, 65);
}

#[test]
fn out_of_range_settings_are_brought_into_range() {
    let wild = TriggerTuning {
        dead_zone: 90,
        max_zone: 10,
        hair_trigger: Some(0),
        press_point: Some(250),
        ..TriggerTuning::default()
    }
    .clamped();

    assert_eq!(wild.dead_zone, 50);
    assert_eq!(wild.max_zone, 51);
    assert_eq!(wild.hair_trigger, Some(1));
    assert_eq!(wild.press_point, Some(100));
}

#[test]
fn a_change_sets_one_setting_and_keeps_the_rest() {
    let mut tuning = TriggerTuning::default();

    tuning.change(TriggerChange::DeadZone(10));
    tuning.change(TriggerChange::MaxZone(90));
    tuning.change(TriggerChange::Curve(Curve::Sharp));
    tuning.change(TriggerChange::HairTrigger(Some(40)));
    tuning.change(TriggerChange::PressPoint(Some(60)));

    assert_eq!(
        tuning,
        TriggerTuning {
            dead_zone: 10,
            max_zone: 90,
            curve: Curve::Sharp,
            hair_trigger: Some(40),
            press_point: Some(60),
        }
    );
    tuning.change(TriggerChange::HairTrigger(None));
    assert_eq!(tuning.hair_trigger, None);
}

#[test]
fn changing_back_to_the_defaults_is_neutral_again() {
    let mut tuning = TriggerTuning::default();
    tuning.change(TriggerChange::HairTrigger(Some(40)));
    tuning.change(TriggerChange::PressPoint(Some(60)));

    tuning.change(TriggerChange::HairTrigger(None));
    tuning.change(TriggerChange::PressPoint(None));

    assert!(tuning.is_neutral());
}

#[test]
fn a_profile_changes_the_trigger_it_is_told_to() {
    let mut profile = Profile::named("Test");

    profile.change_trigger(TriggerSide::Right, TriggerChange::HairTrigger(Some(25)));
    profile.change_trigger(TriggerSide::Left, TriggerChange::Curve(Curve::Soft));

    assert_eq!(profile.trigger(TriggerSide::Right).hair_trigger, Some(25));
    assert_eq!(profile.trigger(TriggerSide::Left).hair_trigger, None);
    assert_eq!(profile.trigger(TriggerSide::Left).curve, Curve::Soft);
    assert!(!profile.is_plain());
}

#[test]
fn tuning_survives_a_save_and_load() {
    let mut profile = Profile::named("Shooter");
    profile.left_trigger = TriggerTuning {
        dead_zone: 6,
        max_zone: 92,
        curve: Curve::Smooth,
        hair_trigger: Some(35),
        press_point: Some(55),
    };
    profile.right_trigger.hair_trigger = Some(20);

    let loaded = Profile::load(&profile.to_json(), "ignored");

    assert_eq!(loaded, Some(profile));
}

#[test]
fn a_profile_without_trigger_tuning_writes_none_and_an_old_file_loads_neutral() {
    let json = Profile::named("Plain").to_json();

    assert!(!json.contains("trigger"));
    let old = Profile::load(r#"{"name": "Old", "swap_sticks": true}"#, "Old").unwrap();
    assert!(old.left_trigger.is_neutral());
    assert!(old.right_trigger.is_neutral());
}

#[test]
fn off_options_are_not_written() {
    let mut profile = Profile::named("A");
    profile.left_trigger.curve = Curve::Soft;

    let json = profile.to_json();

    assert!(json.contains("left_trigger"));
    assert!(!json.contains("hair_trigger"));
    assert!(!json.contains("press_point"));
}

#[test]
fn broken_tuning_loads_as_what_can_be_read() {
    let text = r#"{
        "name": "Mixed",
        "left_trigger": {
            "dead_zone": "big",
            "curve": "weird",
            "hair_trigger": 250,
            "press_point": null,
            "max_zone": 70
        },
        "right_trigger": 5
    }"#;

    let profile = Profile::load(text, "Mixed").unwrap();

    assert_eq!(
        profile.left_trigger,
        TriggerTuning {
            max_zone: 70,
            hair_trigger: Some(100),
            ..TriggerTuning::default()
        }
    );
    assert!(profile.right_trigger.is_neutral());
}

#[test]
fn a_file_with_only_trigger_settings_is_a_profile() {
    assert!(Profile::parse(r#"{"left_trigger": {}}"#, "A").is_ok());
    assert!(Profile::parse(r#"{"right_trigger": {"curve": "soft"}}"#, "A").is_ok());
}
