use std::sync::Arc;
use std::time::{Duration, Instant};

use heka_app::tray_model::{TOOLTIP_LIMIT, controllers, tooltip};
use heka_app::ui::model::{Command, ControllerEntry};
use heka_core::bridge::LiveInput;
use heka_core::lifecycle::Phase;
use heka_core::link::{ControllerModel, LinkKind};
use heka_core::mac::MacAddress;
use heka_core::preferences::Preferences;
use heka_core::state::{Battery, Charge};
use heka_core::supervisor::{ControllerView, DriftStatus};
use heka_core::virtual_controller::OutputType;

fn mac(last: u8) -> MacAddress {
    MacAddress([0x02, 0, 0, 0, 0, last])
}

fn entry(last: u8, phase: Phase, battery: Option<(u8, Charge)>) -> ControllerEntry {
    let view = ControllerView {
        mac: mac(last),
        model: ControllerModel::DualSense,
        phase,
        output_type: OutputType::Xbox,
        nickname: None,
        preferences: Preferences::default(),
        active_link: Some(LinkKind::Usb),
        has_bluetooth: false,
        idle_time: None,
        profile: None,
        applied_profile: None,
        rule_program: None,
        output_type_from_profile: false,
        live: Arc::new(LiveInput::default()),
        drift: DriftStatus::Unmeasured,
        error: None,
        turn_off_error: None,
    };
    let mut entry = ControllerEntry::from_view(&view, Instant::now(), false);
    entry.battery = battery.map(|(percent, charge)| Battery { percent, charge });
    entry
}

fn named(last: u8, name: &str) -> ControllerEntry {
    ControllerEntry {
        nickname: Some(name.into()),
        ..entry(last, Phase::Bridged, Some((85, Charge::Discharging)))
    }
}

#[test]
fn the_menu_lists_each_controller_with_its_state_and_battery() {
    let held = Phase::Held {
        until: Instant::now() + Duration::from_secs(30),
    };
    let entries = [
        entry(1, Phase::Bridged, Some((85, Charge::Charging))),
        entry(2, held, None),
        entry(3, Phase::Stopped, Some((25, Charge::Discharging))),
        entry(4, Phase::Waiting, Some((0, Charge::Unknown))),
    ];

    let labels: Vec<String> = controllers(&entries)
        .into_iter()
        .map(|controller| controller.label)
        .collect();

    assert_eq!(
        labels,
        [
            "DualSense 00:01: Bridged, 85%, charging",
            "DualSense 00:02: Held",
            "DualSense 00:03: Stopped, 25%",
            "DualSense 00:04: Not Bridged",
        ]
    );
}

#[test]
fn the_menu_starts_a_stopped_controller_and_stops_any_other() {
    let entries = [
        entry(1, Phase::Bridged, None),
        entry(2, Phase::Stopped, None),
        entry(3, Phase::Waiting, None),
    ];

    let offered: Vec<(&str, Command)> = controllers(&entries)
        .into_iter()
        .map(|controller| (controller.action, controller.command))
        .collect();

    assert_eq!(
        offered,
        [
            ("Stop", Command::Stop(mac(1))),
            ("Start", Command::Start(mac(2))),
            ("Stop", Command::Stop(mac(3))),
        ]
    );
}

#[test]
fn an_ampersand_in_a_name_is_not_taken_as_a_menu_shortcut() {
    let listed = controllers(&[named(1, "Ann & Bo")]);

    assert_eq!(listed[0].label, "Ann && Bo: Bridged, 85%");
    assert!(tooltip(&[named(1, "Ann & Bo")]).contains("Ann & Bo:"));
}

#[test]
fn the_tooltip_is_the_name_alone_with_no_controller() {
    assert_eq!(tooltip(&[]), "heka");
}

#[test]
fn the_tooltip_lists_each_controller_with_its_state_and_level() {
    let entries = [
        named(1, "Sofa"),
        entry(2, Phase::Stopped, Some((25, Charge::Charging))),
        entry(3, Phase::Waiting, None),
    ];

    assert_eq!(
        tooltip(&entries),
        "heka\nSofa: Bridged, 85%\nDualSense 00:02: Stopped, 25%\nDualSense 00:03: Not Bridged"
    );
}

#[test]
fn long_names_are_cut_so_that_every_controller_fits_the_tooltip() {
    let entries: Vec<ControllerEntry> = (1..=4)
        .map(|last| ControllerEntry {
            nickname: Some(format!("{last} the longest name there is")),
            ..entry(last, Phase::Waiting, Some((100, Charge::Full)))
        })
        .collect();

    let words = tooltip(&entries);

    assert!(words.encode_utf16().count() <= TOOLTIP_LIMIT, "{words}");
    assert_eq!(words.lines().count(), 5);
    for (line, last) in words.lines().skip(1).zip(1..) {
        assert!(line.starts_with(&format!("{last} the")), "{line}");
        assert!(line.ends_with("…: Not Bridged, 100%"), "{line}");
    }
}

#[test]
fn more_controllers_than_fit_still_make_a_tooltip_within_the_limit() {
    let entries: Vec<ControllerEntry> = (1..=12)
        .map(|last| entry(last, Phase::Waiting, None))
        .collect();

    assert!(tooltip(&entries).encode_utf16().count() <= TOOLTIP_LIMIT);
}
