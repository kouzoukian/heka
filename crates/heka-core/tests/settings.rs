//! Stopped and Output Type are remembered per Controller across restarts.

use heka_core::mac::MacAddress;
use heka_core::settings::{ControllerSettings, Settings};
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
