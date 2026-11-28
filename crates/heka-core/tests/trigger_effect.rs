use heka_core::dualsense::trigger_effect;
use heka_core::profile::{Mapper, Profile};
use heka_core::trigger_effect::{
    EffectChange, EffectKind, EffectSettings, TriggerEffect, TriggerEffects,
};
use heka_core::tuning::TriggerSide;

fn with_effect(side: TriggerSide, changes: &[EffectChange]) -> Profile {
    let mut profile = Profile::named("Racing");
    for &change in changes {
        profile.change_trigger_effect(side, change);
    }
    profile
}

fn chosen(changes: &[EffectChange]) -> EffectSettings {
    *with_effect(TriggerSide::Left, changes).trigger_effect(TriggerSide::Left)
}

#[test]
fn the_default_profile_asks_nothing_of_the_triggers() {
    assert_eq!(
        Mapper::new(&Profile::default()).trigger_effects(),
        TriggerEffects::default()
    );
}

#[test]
fn each_trigger_has_its_own_effect() {
    let mut profile = with_effect(
        TriggerSide::Left,
        &[EffectChange::Kind(EffectKind::Resistance)],
    );
    profile.change_trigger_effect(
        TriggerSide::Right,
        EffectChange::Kind(EffectKind::Vibration),
    );

    assert_eq!(
        Mapper::new(&profile).trigger_effects(),
        TriggerEffects {
            left: Some(TriggerEffect::Resistance {
                start: 30,
                strength: 60
            }),
            right: Some(TriggerEffect::Vibration {
                start: 30,
                strength: 60
            }),
        }
    );
}

#[test]
fn a_break_keeps_its_positions_where_the_controller_can_put_them() {
    let broken = |start, end| {
        let settings = chosen(&[
            EffectChange::Kind(EffectKind::Break),
            EffectChange::Start(start),
            EffectChange::End(end),
        ]);
        (settings.start, settings.end)
    };

    assert_eq!(broken(40, 60), (40, 60));
    assert_eq!(broken(0, 100), (20, 80));
    assert_eq!(broken(90, 10), (70, 80));
    assert_eq!(broken(50, 20), (50, 60));
}

#[test]
fn positions_go_in_tenths_and_strength_is_never_nothing() {
    let settings = chosen(&[
        EffectChange::Kind(EffectKind::Resistance),
        EffectChange::Start(100),
        EffectChange::Strength(0),
    ]);
    assert_eq!((settings.start, settings.strength), (90, 1));

    let settings = chosen(&[
        EffectChange::Kind(EffectKind::Vibration),
        EffectChange::Start(47),
        EffectChange::Strength(200),
    ]);
    assert_eq!((settings.start, settings.strength), (40, 100));
}

#[test]
fn a_trigger_effect_survives_a_save_and_load() {
    let profile = with_effect(
        TriggerSide::Right,
        &[
            EffectChange::Kind(EffectKind::Break),
            EffectChange::Start(30),
            EffectChange::End(50),
            EffectChange::Strength(80),
        ],
    );
    let reloaded = Profile::load(&profile.to_json(), "racing").unwrap();

    assert_eq!(reloaded, profile);
    assert!(!reloaded.is_plain());
}

#[test]
fn a_profile_file_names_the_effect_of_each_trigger() {
    let text = r#"{"left_trigger_effect": {"effect": "vibration", "start": 60, "strength": 40}}"#;
    let profile = Profile::parse(text, "buzz").unwrap();

    assert_eq!(
        Mapper::new(&profile).trigger_effects(),
        TriggerEffects {
            left: Some(TriggerEffect::Vibration {
                start: 60,
                strength: 40
            }),
            right: None,
        }
    );
}

#[test]
fn an_effect_that_cannot_be_read_is_off() {
    let text = r#"{"name": "Odd", "right_trigger_effect": {"effect": "laser", "start": "high"}}"#;
    let profile = Profile::load(text, "odd").unwrap();

    assert_eq!(
        *profile.trigger_effect(TriggerSide::Right),
        EffectSettings::default()
    );
}

#[test]
fn a_profile_without_effects_writes_nothing_for_them() {
    assert!(!Profile::named("Plain").to_json().contains("trigger_effect"));
}

fn bytes(start: &[u8]) -> [u8; 11] {
    let mut expected = [0u8; 11];
    expected[..start.len()].copy_from_slice(start);
    expected
}

#[test]
fn off_is_the_reset_effect() {
    assert_eq!(trigger_effect(TriggerEffect::Off), bytes(&[0x05]));
}

#[test]
fn resistance_covers_every_zone_from_its_start_with_one_force() {
    let effect = TriggerEffect::Resistance {
        start: 30,
        strength: 60,
    };
    assert_eq!(
        trigger_effect(effect),
        bytes(&[0x21, 0xF8, 0x03, 0x00, 0x48, 0x92, 0x24])
    );
}

#[test]
fn full_resistance_from_the_top_fills_all_ten_zones() {
    let effect = TriggerEffect::Resistance {
        start: 0,
        strength: 100,
    };
    assert_eq!(
        trigger_effect(effect),
        bytes(&[0x21, 0xFF, 0x03, 0xFF, 0xFF, 0xFF, 0x3F])
    );
}

#[test]
fn a_break_marks_where_it_starts_and_where_it_gives_way() {
    let effect = |start, end, strength| {
        trigger_effect(TriggerEffect::Break {
            start,
            end,
            strength,
        })
    };

    assert_eq!(effect(20, 70, 100), bytes(&[0x25, 0x84, 0x00, 0x07]));
    assert_eq!(effect(70, 80, 1), bytes(&[0x25, 0x80, 0x01, 0x00]));
}

#[test]
fn vibration_carries_its_zones_strength_and_pitch() {
    let effect = TriggerEffect::Vibration {
        start: 90,
        strength: 20,
    };
    assert_eq!(
        trigger_effect(effect),
        bytes(&[0x26, 0x00, 0x02, 0x00, 0x00, 0x00, 0x08, 0, 0, 30])
    );
}
