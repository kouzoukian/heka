//! Stopped and Output Type are remembered per Controller across restarts.

use heka_core::feedback::{Lightbar, PlayerLights};
use heka_core::mac::MacAddress;
use heka_core::preferences::{Preferences, RumbleStrength};
use heka_core::settings::{ControllerSettings, MAX_NICKNAME_CHARS, Settings};
use heka_core::virtual_controller::OutputType;

const MAC: MacAddress = MacAddress([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);

#[test]
fn a_missing_settings_file_gives_defaults() {
    let settings = Settings::load(None);
    assert_eq!(settings, Settings::default());
    assert_eq!(
        settings.controller(MAC),
        ControllerSettings {
            stopped: false,
            output_type: OutputType::Xbox,
            nickname: None,
            preferences: Preferences {
                rumble: RumbleStrength::FULL,
                lightbar: None,
                player_lights: None,
                hide: true,
            },
        }
    );
}

#[test]
fn a_corrupt_settings_file_gives_defaults() {
    for text in ["", "{", "not json", "[1, 2]", r#"{"controllers": 3}"#] {
        assert_eq!(Settings::load(Some(text)), Settings::default(), "{text}");
    }
}

#[test]
fn stopped_survives_a_save_and_load() {
    let mut settings = Settings::default();
    settings.set_stopped(MAC, true);

    let reloaded = Settings::load(Some(&settings.to_json()));
    assert!(reloaded.controller(MAC).stopped);
    assert_eq!(reloaded, settings);
}

#[test]
fn the_file_is_keyed_by_mac_address() {
    let text =
        r#"{"controllers": {"11:22:33:44:55:66": {"stopped": true, "output_type": "xbox"}}}"#;
    let settings = Settings::load(Some(text));
    assert!(settings.controller(MAC).stopped);
}

#[test]
fn a_ps4_output_type_is_remembered_across_a_save() {
    let text = r#"{"controllers": {"11:22:33:44:55:66": {"output_type": "ps4"}}}"#;
    let settings = Settings::load(Some(text));
    assert_eq!(settings.controller(MAC).output_type, OutputType::Ps4);
    let reloaded = Settings::load(Some(&settings.to_json()));
    assert_eq!(reloaded.controller(MAC).output_type, OutputType::Ps4);
}

#[test]
fn unknown_fields_and_bad_entries_do_not_lose_the_good_ones() {
    // A later version may add fields; a hand edit may break one entry.
    let text = r#"{
        "future": 1,
        "controllers": {
            "11:22:33:44:55:66": {"stopped": true, "new_field": "x"},
            "not a mac": {"stopped": true},
            "AA:BB:CC:DD:EE:FF": {"output_type": "gamecube"}
        }
    }"#;
    let settings = Settings::load(Some(text));
    assert!(settings.controller(MAC).stopped);
    let other: MacAddress = "AA:BB:CC:DD:EE:FF".parse().unwrap();
    assert_eq!(settings.controller(other), ControllerSettings::default());
}

#[test]
fn a_nickname_and_preferences_survive_a_save_and_load() {
    let mut settings = Settings::default();
    settings.set_nickname(MAC, "Sofa");
    settings.set_preferences(
        MAC,
        Preferences {
            rumble: RumbleStrength::percent(25),
            lightbar: Some(Lightbar {
                red: 1,
                green: 2,
                blue: 3,
            }),
            player_lights: Some(PlayerLights::Four),
            hide: false,
        },
    );

    let reloaded = Settings::load(Some(&settings.to_json()));
    assert_eq!(reloaded, settings);
    assert_eq!(reloaded.controller(MAC).nickname.as_deref(), Some("Sofa"));
}

#[test]
fn preferences_are_written_beside_the_other_fields() {
    let text = r#"{"controllers": {"11:22:33:44:55:66": {
        "stopped": true, "nickname": "Sofa", "rumble": 50,
        "lightbar": {"red": 9, "green": 8, "blue": 7},
        "player_lights": "two", "hide": false
    }}}"#;
    let controller = Settings::load(Some(text)).controller(MAC);

    assert!(controller.stopped);
    assert_eq!(controller.nickname.as_deref(), Some("Sofa"));
    assert_eq!(controller.preferences.rumble, RumbleStrength::percent(50));
    assert_eq!(
        controller.preferences.player_lights,
        Some(PlayerLights::Two)
    );
    assert!(!controller.preferences.hide);
}

#[test]
fn a_nickname_is_trimmed_cut_to_length_and_dropped_when_blank() {
    let mut settings = Settings::default();
    let mut named = |name: &str| {
        settings.set_nickname(MAC, name);
        settings.controller(MAC).nickname
    };

    assert_eq!(named("  Sofa ").as_deref(), Some("Sofa"));
    assert_eq!(named("   "), None);
    let long = "é".repeat(MAX_NICKNAME_CHARS + 5);
    assert_eq!(named(&long), Some("é".repeat(MAX_NICKNAME_CHARS)));
    // A hand edit is read the same way.
    let text = r#"{"controllers": {"11:22:33:44:55:66": {"nickname": "  "}}}"#;
    assert_eq!(Settings::load(Some(text)).controller(MAC).nickname, None);
}

#[test]
fn only_controllers_set_to_be_seen_are_never_hidden() {
    let other = MacAddress([1; 6]);
    let mut settings = Settings::default();
    settings.set_stopped(other, true);
    settings.set_preferences(
        MAC,
        Preferences {
            hide: false,
            ..Preferences::default()
        },
    );

    assert_eq!(settings.never_hidden().collect::<Vec<_>>(), [MAC]);
}
