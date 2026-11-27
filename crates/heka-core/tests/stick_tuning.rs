use heka_core::profile::{Mapper, Profile};
use heka_core::state::{Buttons, ControllerState, Stick};
use heka_core::tuning::{Curve, StickChange, StickSide, StickTuning};
use heka_core::xbox::XboxReport;

fn state(left: (u8, u8), right: (u8, u8)) -> ControllerState {
    ControllerState {
        left_stick: Stick {
            x: left.0,
            y: left.1,
        },
        right_stick: Stick {
            x: right.0,
            y: right.1,
        },
        ..ControllerState::NEUTRAL
    }
}

fn left(x: u8, y: u8) -> ControllerState {
    state((x, y), (128, 128))
}

fn mapper(left: StickTuning, right: StickTuning, swap: bool) -> Mapper {
    let mut profile = Profile::named("Test");
    profile.left_stick = left;
    profile.right_stick = right;
    profile.swap_sticks = swap;
    Mapper::new(&profile)
}

fn tuned(tuning: StickTuning) -> Mapper {
    mapper(tuning, StickTuning::default(), false)
}

fn within(value: u8, low: u8, high: u8) {
    assert!(
        (low..=high).contains(&value),
        "{value} is not within {low}..={high}"
    );
}

#[test]
fn neutral_settings_leave_every_state_and_report_identical() {
    let mapper = Mapper::new(&Profile::default());

    for x in 0..=255u8 {
        for y in 0..=255u8 {
            let input = state((x, y), (y, x));
            let mapped = mapper.map(&input);
            assert_eq!(mapped, input);
            assert_eq!(
                XboxReport::from_state(&mapped),
                XboxReport::from_state(&input)
            );
        }
    }
}

#[test]
fn the_default_tuning_is_neutral_and_any_change_is_not() {
    assert!(StickTuning::default().is_neutral());
    for changed in [
        StickTuning {
            dead_zone: 1,
            ..StickTuning::default()
        },
        StickTuning {
            anti_dead_zone: 1,
            ..StickTuning::default()
        },
        StickTuning {
            max_zone: 99,
            ..StickTuning::default()
        },
        StickTuning {
            sensitivity: 101,
            ..StickTuning::default()
        },
        StickTuning {
            curve: Curve::Soft,
            ..StickTuning::default()
        },
        StickTuning {
            invert_x: true,
            ..StickTuning::default()
        },
        StickTuning {
            invert_y: true,
            ..StickTuning::default()
        },
    ] {
        assert!(!changed.is_neutral(), "{changed:?}");
    }
}

#[test]
fn the_centre_stays_the_centre_however_the_stick_is_tuned() {
    let heavy = StickTuning {
        dead_zone: 20,
        anti_dead_zone: 10,
        max_zone: 80,
        sensitivity: 150,
        curve: Curve::Soft,
        ..StickTuning::default()
    };

    let mapped = tuned(heavy).map(&left(128, 128));

    assert_eq!(mapped.left_stick, Stick { x: 128, y: 128 });
    let fine = mapped.fine_sticks.unwrap();
    assert_eq!((fine.left_x, fine.left_y), (0, 0));
}

#[test]
fn full_travel_stays_full_travel() {
    let tuning = StickTuning {
        dead_zone: 10,
        max_zone: 90,
        ..StickTuning::default()
    };
    let mapper = tuned(tuning);

    let right = mapper.map(&left(255, 128));
    let far_left = mapper.map(&left(0, 128));

    assert_eq!(right.left_stick, Stick { x: 255, y: 128 });
    assert_eq!(right.fine_sticks.unwrap().left_x, 32767);
    assert_eq!(far_left.left_stick, Stick { x: 0, y: 128 });
    assert_eq!(far_left.fine_sticks.unwrap().left_x, -32768);
}

#[test]
fn a_dead_zone_swallows_small_movements_and_rescales_the_rest() {
    let mapper = tuned(StickTuning {
        dead_zone: 10,
        ..StickTuning::default()
    });

    assert_eq!(mapper.map(&left(140, 128)).left_stick.x, 128);
    assert_eq!(mapper.map(&left(128, 116)).left_stick.y, 128);
    within(mapper.map(&left(192, 128)).left_stick.x, 184, 186);
}

#[test]
fn an_anti_dead_zone_lifts_the_smallest_movement_to_its_edge() {
    let mapper = tuned(StickTuning {
        anti_dead_zone: 20,
        ..StickTuning::default()
    });

    let mapped = mapper.map(&left(129, 128));

    assert_eq!(mapped.left_stick.x, 154);
    assert_eq!(
        mapper.map(&left(128, 128)).left_stick,
        Stick { x: 128, y: 128 }
    );
}

#[test]
fn a_max_zone_reaches_full_travel_early() {
    let mapper = tuned(StickTuning {
        max_zone: 50,
        ..StickTuning::default()
    });

    assert_eq!(mapper.map(&left(192, 128)).left_stick.x, 255);
    assert_eq!(mapper.map(&left(160, 128)).left_stick.x, 192);
}

#[test]
fn sensitivity_scales_the_travel() {
    let double = tuned(StickTuning {
        sensitivity: 200,
        ..StickTuning::default()
    });
    let half = tuned(StickTuning {
        sensitivity: 50,
        ..StickTuning::default()
    });

    within(double.map(&left(160, 128)).left_stick.x, 191, 193);
    within(half.map(&left(255, 128)).left_stick.x, 191, 193);
    assert_eq!(double.map(&left(255, 128)).left_stick.x, 255);
}

#[test]
fn each_curve_bends_the_travel_in_its_own_way() {
    let at = |curve| {
        tuned(StickTuning {
            curve,
            ..StickTuning::default()
        })
        .map(&left(192, 128))
        .left_stick
        .x
    };

    within(at(Curve::Linear), 191, 193);
    within(at(Curve::Soft), 159, 161);
    within(at(Curve::Softer), 143, 145);
    within(at(Curve::Sharp), 223, 225);
    within(at(Curve::Smooth), 191, 193);
}

#[test]
fn every_curve_starts_at_nothing_ends_at_everything_and_never_falls() {
    for curve in Curve::ALL {
        assert_eq!(curve.apply(0.0), 0.0, "{curve:?}");
        assert!((curve.apply(1.0) - 1.0).abs() < 1e-6, "{curve:?}");
        let mut last = 0.0;
        for step in 0..=100 {
            let value = curve.apply(step as f32 / 100.0);
            assert!(value >= last - 1e-6, "{curve:?} at {step}");
            last = value;
        }
    }
}

#[test]
fn a_full_diagonal_is_held_to_the_circle() {
    let mapper = tuned(StickTuning {
        dead_zone: 10,
        ..StickTuning::default()
    });

    let mapped = mapper.map(&left(255, 255));

    assert_eq!(mapped.left_stick.x, mapped.left_stick.y);
    assert_eq!(mapped.left_stick.x, 218);
    let fine = mapped.fine_sticks.unwrap();
    assert_eq!(fine.left_x, fine.left_y);
    assert!((23169..=23171).contains(&fine.left_x));
}

#[test]
fn a_diagonal_keeps_its_direction_when_it_is_rescaled() {
    let mapper = tuned(StickTuning {
        dead_zone: 10,
        ..StickTuning::default()
    });

    let half = mapper.map(&left(192, 192)).left_stick;
    let steep = mapper.map(&left(255, 192)).left_stick;

    assert_eq!(half.x, half.y);
    within(half.x, 188, 190);
    within(steep.x, 240, 242);
    within(steep.y, 184, 186);
    assert!(steep.x > steep.y);
}

#[test]
fn the_opposite_diagonal_is_the_mirror_of_the_first() {
    let mapper = tuned(StickTuning {
        dead_zone: 10,
        ..StickTuning::default()
    });

    let corner = mapper.map(&left(0, 0)).left_stick;

    assert_eq!(corner.x, corner.y);
    within(corner.x, 36, 38);
}

#[test]
fn inverting_flips_the_axis_and_leaves_the_other_alone() {
    let x = tuned(StickTuning {
        invert_x: true,
        ..StickTuning::default()
    });
    let y = tuned(StickTuning {
        invert_y: true,
        ..StickTuning::default()
    });

    let inverted_x = x.map(&left(200, 128)).left_stick;
    let inverted_y = y.map(&left(128, 60)).left_stick;

    within(inverted_x.x, 55, 56);
    assert_eq!(inverted_x.y, 128);
    assert_eq!(inverted_y.x, 128);
    within(inverted_y.y, 195, 196);
    assert_eq!(x.map(&left(128, 128)).left_stick, Stick { x: 128, y: 128 });
}

#[test]
fn swapping_exchanges_the_sticks_and_nothing_else() {
    let mapper = mapper(StickTuning::default(), StickTuning::default(), true);
    let input = ControllerState {
        buttons: Buttons::CROSS | Buttons::L3,
        left_trigger: 40,
        ..state((10, 20), (200, 50))
    };

    let mapped = mapper.map(&input);

    assert_eq!(mapped.left_stick, Stick { x: 200, y: 50 });
    assert_eq!(mapped.right_stick, Stick { x: 10, y: 20 });
    assert_eq!(mapped.fine_sticks, None);
    assert_eq!(mapped.buttons, input.buttons);
    assert_eq!(mapped.left_trigger, 40);
}

#[test]
fn a_stick_keeps_its_own_tuning_when_the_sticks_are_swapped() {
    let invert_x = StickTuning {
        invert_x: true,
        ..StickTuning::default()
    };
    let mapper = mapper(invert_x, StickTuning::default(), true);

    let mapped = mapper.map(&state((255, 128), (128, 0)));

    assert_eq!(mapped.right_stick, Stick { x: 0, y: 128 });
    assert_eq!(mapped.left_stick, Stick { x: 128, y: 0 });
    let fine = mapped.fine_sticks.unwrap();
    assert_eq!((fine.right_x, fine.right_y), (-32768, 0));
    assert_eq!((fine.left_x, fine.left_y), (0, -32768));
}

#[test]
fn tuning_one_stick_leaves_the_other_exactly_as_it_was() {
    let mapper = tuned(StickTuning {
        curve: Curve::Soft,
        dead_zone: 15,
        ..StickTuning::default()
    });

    for value in 0..=255u8 {
        let input = state((value, 255 - value), (value, 255 - value));
        let mapped = mapper.map(&input);
        let before = XboxReport::from_state(&input);
        let after = XboxReport::from_state(&mapped);
        assert_eq!(mapped.right_stick, input.right_stick);
        assert_eq!(after.thumb_rx, before.thumb_rx, "{value}");
        assert_eq!(after.thumb_ry, before.thumb_ry, "{value}");
    }
}

#[test]
fn the_xbox_report_carries_more_precision_than_a_byte_can() {
    let mapper = tuned(StickTuning {
        curve: Curve::Softer,
        ..StickTuning::default()
    });
    let at = |offset: u8| mapper.map(&left(128 + offset, 128));

    assert_eq!(at(10).left_stick.x, 128);
    assert_eq!(at(0).fine_sticks.unwrap().left_x, 0);
    assert!(at(5).fine_sticks.unwrap().left_x > 0);
    assert!(at(10).fine_sticks.unwrap().left_x > at(5).fine_sticks.unwrap().left_x);
    assert_eq!(
        XboxReport::from_state(&at(10)).thumb_lx,
        at(10).fine_sticks.unwrap().left_x
    );

    let bytes: std::collections::BTreeSet<u8> =
        (0..=127u8).map(|offset| at(offset).left_stick.x).collect();
    let wide: std::collections::BTreeSet<i16> = (0..=127u8)
        .map(|offset| at(offset).fine_sticks.unwrap().left_x)
        .collect();
    assert!(wide.len() > bytes.len());
    assert!(wide.len() >= 120);
    assert!(bytes.len() < 100);
}

#[test]
fn the_xbox_report_flips_the_tuned_y_like_the_untuned_one() {
    let mapper = tuned(StickTuning {
        dead_zone: 5,
        ..StickTuning::default()
    });

    let up = XboxReport::from_state(&mapper.map(&left(128, 0)));
    let down = XboxReport::from_state(&mapper.map(&left(128, 255)));

    assert_eq!(up.thumb_ly, 32767);
    assert_eq!(down.thumb_ly, -32767);
    assert_eq!(up.thumb_lx, 0);
}

#[test]
fn a_ps4_report_still_gets_bytes() {
    let mapped = tuned(StickTuning {
        dead_zone: 10,
        ..StickTuning::default()
    })
    .map(&left(255, 128));
    let mut translator = heka_core::ps4::Ps4Translator::default();

    let report = translator.report(&mapped);

    assert_eq!(report[0], 255);
    assert_eq!(report[1], 128);
}

#[test]
fn out_of_range_settings_are_brought_into_range() {
    let wild = StickTuning {
        dead_zone: 90,
        anti_dead_zone: 90,
        max_zone: 10,
        sensitivity: 1000,
        ..StickTuning::default()
    }
    .clamped();

    assert_eq!(wild.dead_zone, 50);
    assert_eq!(wild.anti_dead_zone, 50);
    assert_eq!(wild.max_zone, 51);
    assert_eq!(wild.sensitivity, 200);
    let low = StickTuning {
        sensitivity: 1,
        ..StickTuning::default()
    }
    .clamped();
    assert_eq!(low.sensitivity, 50);
}

#[test]
fn a_max_zone_never_sits_at_or_below_the_dead_zone() {
    let tuning = StickTuning {
        dead_zone: 40,
        max_zone: 50,
        ..StickTuning::default()
    }
    .clamped();

    assert!(tuning.max_zone > tuning.dead_zone);
    let mapped = tuned(tuning).map(&left(255, 128));
    assert_eq!(mapped.left_stick.x, 255);
}

#[test]
fn tuning_survives_a_save_and_load() {
    let mut profile = Profile::named("Racing");
    profile.left_stick = StickTuning {
        dead_zone: 12,
        anti_dead_zone: 5,
        max_zone: 90,
        sensitivity: 120,
        curve: Curve::Smooth,
        invert_x: true,
        invert_y: false,
    };
    profile.right_stick.invert_y = true;
    profile.swap_sticks = true;

    let loaded = Profile::load(&profile.to_json(), "ignored");

    assert_eq!(loaded, Some(profile));
}

#[test]
fn a_profile_without_tuning_writes_none_and_a_v2_file_without_it_loads_neutral() {
    let plain = Profile::named("Plain");

    let json = plain.to_json();

    assert!(!json.contains("stick"));
    let loaded = Profile::load(r#"{"name": "Old", "bindings": {"cross": "circle"}}"#, "Old");
    let loaded = loaded.unwrap();
    assert!(loaded.left_stick.is_neutral());
    assert!(loaded.right_stick.is_neutral());
    assert!(!loaded.swap_sticks);
}

#[test]
fn broken_tuning_loads_as_what_can_be_read() {
    let text = r#"{
        "name": "Mixed",
        "left_stick": {
            "dead_zone": "big",
            "curve": "weird",
            "sensitivity": 500,
            "invert_x": true,
            "anti_dead_zone": -4
        },
        "right_stick": 5,
        "swap_sticks": "yes"
    }"#;

    let profile = Profile::load(text, "Mixed").unwrap();

    assert_eq!(
        profile.left_stick,
        StickTuning {
            sensitivity: 200,
            invert_x: true,
            ..StickTuning::default()
        }
    );
    assert!(profile.right_stick.is_neutral());
    assert!(!profile.swap_sticks);
}

#[test]
fn a_file_with_only_stick_settings_is_a_profile() {
    assert!(Profile::parse(r#"{"swap_sticks": true}"#, "A").is_ok());
    assert!(Profile::parse(r#"{"left_stick": {}}"#, "A").is_ok());
}

#[test]
fn tuning_makes_a_profile_not_plain() {
    let mut profile = Profile::named("A");
    assert!(profile.is_plain());

    profile.swap_sticks = true;
    assert!(!profile.is_plain());

    profile.swap_sticks = false;
    profile.right_stick.curve = Curve::Soft;
    assert!(!profile.is_plain());
}

#[test]
fn a_change_sets_one_setting_and_keeps_the_rest() {
    let mut tuning = StickTuning::default();

    tuning.change(StickChange::DeadZone(12));
    tuning.change(StickChange::AntiDeadZone(5));
    tuning.change(StickChange::MaxZone(90));
    tuning.change(StickChange::Sensitivity(130));
    tuning.change(StickChange::Curve(Curve::Sharp));
    tuning.change(StickChange::InvertX(true));
    tuning.change(StickChange::InvertY(true));

    assert_eq!(
        tuning,
        StickTuning {
            dead_zone: 12,
            anti_dead_zone: 5,
            max_zone: 90,
            sensitivity: 130,
            curve: Curve::Sharp,
            invert_x: true,
            invert_y: true,
        }
    );
}

#[test]
fn a_change_out_of_range_is_brought_into_range() {
    let mut tuning = StickTuning::default();

    tuning.change(StickChange::Sensitivity(900));
    tuning.change(StickChange::DeadZone(80));

    assert_eq!(tuning.sensitivity, 200);
    assert_eq!(tuning.dead_zone, 50);
    assert_eq!(tuning.max_zone, 100);
}

#[test]
fn a_dead_zone_as_wide_as_the_max_zone_lifts_the_max_zone() {
    let mut tuning = StickTuning::default();
    tuning.change(StickChange::MaxZone(50));

    tuning.change(StickChange::DeadZone(30));
    assert_eq!(tuning.max_zone, 50);
    tuning.change(StickChange::DeadZone(50));

    assert_eq!(tuning.max_zone, 51);
}

#[test]
fn changing_back_to_the_defaults_is_neutral_again() {
    let mut tuning = StickTuning::default();
    tuning.change(StickChange::InvertX(true));
    tuning.change(StickChange::Sensitivity(150));

    tuning.change(StickChange::InvertX(false));
    tuning.change(StickChange::Sensitivity(100));

    assert!(tuning.is_neutral());
}

#[test]
fn a_profile_changes_the_stick_it_is_told_to() {
    let mut profile = Profile::named("Test");

    profile.change_stick(StickSide::Right, StickChange::InvertY(true));
    profile.change_stick(StickSide::Left, StickChange::Curve(Curve::Soft));

    assert!(profile.stick(StickSide::Right).invert_y);
    assert!(!profile.stick(StickSide::Left).invert_y);
    assert_eq!(profile.stick(StickSide::Left).curve, Curve::Soft);
    assert_eq!(profile.stick(StickSide::Right).curve, Curve::Linear);
    assert!(!profile.is_plain());
}
