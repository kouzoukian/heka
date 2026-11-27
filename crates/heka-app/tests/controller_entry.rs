use std::sync::Arc;
use std::time::{Duration, Instant};

use heka_app::text;
use heka_app::ui::model::{Command, ControllerEntry, EntryState, Notice, Screen};
use heka_core::bridge::LiveInput;
use heka_core::calibration::Calibration;
use heka_core::discovery::{OpenError, Unopened};
use heka_core::feedback::{Motor, PlayerLights};
use heka_core::hider::{HideError, HidingWarning};
use heka_core::hiding::{HidHideError, HidingError};
use heka_core::idle::IdleTime;
use heka_core::lifecycle::Phase;
use heka_core::link::{ControllerModel, LinkKind, LinkRead, LinkReader, ReportError, ReportFormat};
use heka_core::mac::MacAddress;
use heka_core::preferences::{PreferenceChange, Preferences, RumbleStrength};
use heka_core::radio::TurnOffError;
use heka_core::settings::Settings;
use heka_core::setup::{Driver, DriverError, Missing};
use heka_core::state::{Battery, Buttons, Charge};
use heka_core::supervisor::{BridgeError, ControllerView, Supervisor};
use heka_core::virtual_controller::OutputType;

const MAC: MacAddress = MacAddress([0x02, 0, 0, 0, 0, 1]);

fn view(phase: Phase) -> ControllerView {
    ControllerView {
        mac: MAC,
        model: ControllerModel::DualSense,
        phase,
        output_type: OutputType::Xbox,
        nickname: None,
        preferences: Preferences::default(),
        active_link: Some(LinkKind::Usb),
        has_bluetooth: false,
        idle_time: None,
        profile: None,
        output_type_from_profile: false,
        live: Arc::new(LiveInput::default()),
        error: None,
        turn_off_error: None,
    }
}

fn entry_of(view: &ControllerView, now: Instant) -> ControllerEntry {
    ControllerEntry::from_view(view, now, false)
}

struct CrossPressed;

impl LinkReader for CrossPressed {
    fn format(&self) -> ReportFormat {
        ReportFormat::DualSenseUsb(Calibration::UNCALIBRATED)
    }

    fn read(&mut self, buf: &mut [u8]) -> LinkRead {
        std::thread::sleep(Duration::from_millis(1));
        let mut report = [0u8; 64];
        report[0] = 0x01;
        report[1..5].fill(0x80);
        report[8] = 0x28;
        report[53] = 0x18;
        buf[..report.len()].copy_from_slice(&report);
        LinkRead::Report(report.len())
    }
}

#[test]
fn each_phase_has_its_entry_state() {
    let now = Instant::now();
    let state = |phase| entry_of(&view(phase), now).state;

    assert_eq!(state(Phase::Bridged), EntryState::Bridged);
    assert_eq!(state(Phase::Stopped), EntryState::Stopped);
    assert_eq!(state(Phase::Waiting), EntryState::NotBridged);
}

#[test]
fn a_held_entry_counts_down_in_whole_seconds_rounded_up() {
    let now = Instant::now();
    let held = view(Phase::Held {
        until: now + Duration::from_secs(30),
    });
    let state = |after_ms| entry_of(&held, now + Duration::from_millis(after_ms)).state;

    assert_eq!(state(0), EntryState::Held { seconds_left: 30 });
    assert_eq!(state(1), EntryState::Held { seconds_left: 30 });
    assert_eq!(state(1_000), EntryState::Held { seconds_left: 29 });
    assert_eq!(state(29_999), EntryState::Held { seconds_left: 1 });
    assert_eq!(state(31_000), EntryState::Held { seconds_left: 0 });
}

#[test]
fn an_entry_names_its_controller_and_says_why_a_bridge_failed() {
    let mut failed = view(Phase::Waiting);
    failed.error = Some(BridgeError::VirtualController("no free slot".into()));

    let entry = entry_of(&failed, Instant::now());

    assert_eq!(entry.mac, MAC);
    assert_eq!(entry.short_mac(), "00:01");
    assert_eq!(entry.name, "DualSense");
    assert_eq!(
        entry.detail().as_deref(),
        Some("Could not Bridge: no free slot")
    );
}

#[test]
fn only_a_dualshock_4_entry_says_that_its_support_is_experimental() {
    let mut dualshock4 = view(Phase::Bridged);
    dualshock4.model = ControllerModel::DualShock4;

    let entry = entry_of(&dualshock4, Instant::now());

    assert_eq!(entry.name, "DualShock 4");
    assert!(entry.experimental);
    assert!(!entry_of(&view(Phase::Bridged), Instant::now()).experimental);
}

#[test]
fn an_entry_explains_why_a_controller_is_not_bridged_and_what_a_hold_waits_for() {
    let now = Instant::now();
    let detail = |phase| entry_of(&view(phase), now).detail();

    assert_eq!(
        detail(Phase::Waiting).as_deref(),
        Some("4 Controllers are already Bridged.")
    );
    assert_eq!(
        detail(Phase::Held { until: now }).as_deref(),
        Some("Reconnect the Controller to keep its Virtual Controller.")
    );
    assert_eq!(detail(Phase::Bridged), None);
    assert_eq!(detail(Phase::Stopped), None);
}

#[test]
fn without_vigem_bus_an_entry_says_so_and_still_shows_live_input() {
    let mut supervisor = Supervisor::without_bus(Settings::default());
    supervisor.link_arrived(MAC, Box::new(CrossPressed), Instant::now());
    let deadline = Instant::now() + Duration::from_secs(5);
    while supervisor.controllers()[0].live.latest().is_none() {
        assert!(Instant::now() < deadline, "no input");
        std::thread::sleep(Duration::from_millis(1));
    }

    let entry = ControllerEntry::from_view(&supervisor.controllers()[0], Instant::now(), true);

    assert_eq!(entry.state, EntryState::NotBridged);
    assert_eq!(
        entry.detail().as_deref(),
        Some("Not Bridged: ViGEmBus is missing")
    );
    assert!(entry.pad.is_some_and(|pad| pad.pressed(Buttons::CROSS)));
}

#[test]
fn without_vigem_bus_an_entry_also_says_why_input_cannot_be_read() {
    let mut waiting = view(Phase::Waiting);
    waiting.error = Some(BridgeError::NoThread("no thread".into()));

    let entry = ControllerEntry::from_view(&waiting, Instant::now(), true);

    assert_eq!(
        entry.detail().as_deref(),
        Some("Not Bridged: ViGEmBus is missing (cannot start a thread: no thread)")
    );
}

#[test]
fn an_entry_shows_the_active_link_the_supervisor_reads() {
    let now = Instant::now();
    let link = |active_link| {
        let mut shown = view(Phase::Bridged);
        shown.active_link = active_link;
        entry_of(&shown, now).active_link
    };

    assert_eq!(link(Some(LinkKind::Bluetooth)), Some(LinkKind::Bluetooth));
    assert_eq!(link(Some(LinkKind::Usb)), Some(LinkKind::Usb));
    assert_eq!(link(None), None);
}

#[test]
fn an_entry_shows_the_battery_the_controller_reports_and_none_without_input() {
    let mut supervisor = Supervisor::without_bus(Settings::default());
    supervisor.link_arrived(MAC, Box::new(CrossPressed), Instant::now());
    let deadline = Instant::now() + Duration::from_secs(5);
    while supervisor.controllers()[0].live.latest().is_none() {
        assert!(Instant::now() < deadline, "no input");
        std::thread::sleep(Duration::from_millis(1));
    }

    let entry = entry_of(&supervisor.controllers()[0], Instant::now());

    assert_eq!(
        entry.battery,
        Some(Battery {
            percent: 85,
            charge: Charge::Charging
        })
    );
    assert_eq!(
        entry_of(&view(Phase::Stopped), Instant::now()).battery,
        None
    );
}

#[test]
fn a_battery_is_worded_without_a_level_the_controller_does_not_give() {
    let words = |percent, charge| text::battery(Some(Battery { percent, charge }));

    assert_eq!(words(75, Charge::Discharging), "75%");
    assert_eq!(words(85, Charge::Charging), "85%, charging");
    assert_eq!(words(100, Charge::Full), "100%, full");
    assert_eq!(words(0, Charge::NotCharging), "Battery not charging");
    assert_eq!(words(0, Charge::Unknown), "Battery unknown");
    assert_eq!(text::battery(None), "Battery unknown");
    let level = Some(Battery {
        percent: 75,
        charge: Charge::Discharging,
    });
    assert_eq!(text::battery_in_full(level), "Battery 75%");
    assert_eq!(text::battery_in_full(None), "Battery unknown");
}

#[test]
fn the_output_switch_shows_the_controllers_output_type_and_asks_for_the_chosen_one() {
    let mut ps4 = view(Phase::Bridged);
    ps4.output_type = OutputType::Ps4;

    let entry = entry_of(&ps4, Instant::now());

    assert_eq!(entry.output_type, OutputType::Ps4);
    assert_eq!(
        entry.choose(OutputType::Xbox),
        Some(Command::SetOutputType(MAC, OutputType::Xbox))
    );
    assert_eq!(entry.choose(OutputType::Ps4), None);
}

#[test]
fn the_entry_button_starts_a_stopped_controller_and_stops_any_other() {
    let now = Instant::now();
    let command = |phase| entry_of(&view(phase), now).toggle();

    assert_eq!(command(Phase::Stopped), Command::Start(MAC));
    assert_eq!(command(Phase::Bridged), Command::Stop(MAC));
    assert_eq!(command(Phase::Waiting), Command::Stop(MAC));
    assert_eq!(command(Phase::Held { until: now }), Command::Stop(MAC));
}

#[test]
fn a_screen_changes_only_when_something_shown_changes() {
    let now = Instant::now();
    let held = view(Phase::Held {
        until: now + Duration::from_secs(30),
    });
    let screen = |after_ms| Screen {
        starting: false,
        missing: Vec::new(),
        notices: vec![Notice::warning("warning")],
        controllers: vec![entry_of(&held, now + Duration::from_millis(after_ms))],
        profiles: Vec::new(),
        log: Default::default(),
        app: Default::default(),
        newer_release: None,
    };

    assert_eq!(screen(1), screen(900));
    assert_ne!(screen(900), screen(1_100));
}

#[test]
fn the_status_bar_counts_errors_and_warnings_missing_drivers_among_them() {
    let screen = Screen {
        starting: false,
        missing: vec![Missing {
            driver: Driver::HidHide,
            error: None,
        }],
        notices: vec![
            Notice::warning("one"),
            Notice::error("two"),
            Notice::warning("three"),
        ],
        controllers: Vec::new(),
        profiles: Vec::new(),
        log: Default::default(),
        app: Default::default(),
        newer_release: None,
    };

    assert_eq!(screen.errors(), 1);
    assert_eq!(screen.warnings(), 3);
}

#[test]
fn the_status_bar_counts_controllers_and_bridges() {
    let now = Instant::now();
    let summary = |phases: &[Phase]| {
        Screen {
            starting: false,
            missing: Vec::new(),
            notices: Vec::new(),
            controllers: phases
                .iter()
                .map(|&phase| entry_of(&view(phase), now))
                .collect(),
            profiles: Vec::new(),
            log: Default::default(),
            app: Default::default(),
            newer_release: None,
        }
        .summary()
    };

    assert_eq!(summary(&[]), "No Controllers");
    assert_eq!(summary(&[Phase::Bridged]), "1 Controller, 1 Bridged");
    assert_eq!(
        summary(&[Phase::Bridged, Phase::Stopped]),
        "2 Controllers, 1 Bridged"
    );
}

#[test]
fn an_entry_is_called_by_its_nickname_when_it_has_one() {
    let now = Instant::now();
    let mut named = view(Phase::Bridged);
    assert_eq!(entry_of(&named, now).title(), text::DUALSENSE);

    named.nickname = Some("Sofa".into());
    assert_eq!(entry_of(&named, now).title(), "Sofa");
}

#[test]
fn only_a_dualsense_has_player_lights() {
    let now = Instant::now();
    let mut other = view(Phase::Bridged);
    assert!(entry_of(&other, now).has_player_lights);

    other.model = ControllerModel::DualShock4;
    assert!(!entry_of(&other, now).has_player_lights);
}

#[test]
fn only_a_bridged_controller_can_have_its_rumble_tested() {
    let now = Instant::now();
    let test = |phase| entry_of(&view(phase), now).test_rumble(Motor::Small);

    assert_eq!(
        test(Phase::Bridged),
        Some(Command::TestRumble(MAC, Motor::Small))
    );
    assert_eq!(test(Phase::Stopped), None);
    assert_eq!(test(Phase::Waiting), None);
    assert_eq!(test(Phase::Held { until: now }), None);
}

#[test]
fn changes_made_from_one_sample_are_all_kept() {
    let entry = entry_of(&view(Phase::Bridged), Instant::now());
    let mut supervisor = Supervisor::without_bus(Settings::default());

    for change in [
        PreferenceChange::Rumble(RumbleStrength::OFF),
        PreferenceChange::Hide(false),
        PreferenceChange::PlayerLights(Some(PlayerLights::Two)),
        PreferenceChange::Hide(true),
    ] {
        let Command::ChangePreference(mac, change) = entry.prefer(change) else {
            panic!("expected a preference change");
        };
        supervisor.change_preference(mac, change);
    }

    assert_eq!(
        supervisor.settings().controller(MAC).preferences,
        Preferences {
            rumble: RumbleStrength::OFF,
            player_lights: Some(PlayerLights::Two),
            ..Preferences::default()
        }
    );
}

#[test]
fn hiding_warnings_say_what_failed_and_what_to_change() {
    let busy = "HidHide is busy: close the HidHide Configuration Client or HidHideCLI";
    let error = || HidingError::HidHide(HidHideError::InUse);
    let words = |warning| text::hiding_warning(&warning);

    assert!(!busy.to_lowercase().contains("hide the controller"));
    assert_eq!(
        words(HidingWarning::NotRestored(error())),
        format!("Could not restore a Controller, trying again: {busy}.")
    );
    assert!(words(HidingWarning::EarlierRunNotRestored(error())).starts_with("Could not restore"));
    assert_eq!(
        words(HidingWarning::NotHidden(HideError::Failed(error()))),
        format!("Could not Hide a Controller, so games may see it twice: {busy}.")
    );
    let inverted = words(HidingWarning::Inverted);
    assert!(inverted.contains("twice"));
    assert!(inverted.contains("Turn off the inverse application cloak"));
}

#[test]
fn a_controller_that_cannot_be_opened_is_named_with_the_reason() {
    let unopened = Unopened {
        model: ControllerModel::DualShock4,
        error: OpenError::Hid("access is denied".into()),
    };

    assert_eq!(
        text::cannot_open(&unopened),
        "A DualShock 4 was found but cannot be opened: access is denied"
    );
}

#[test]
fn the_errors_heka_core_returns_are_worded_for_the_player() {
    assert_eq!(
        text::open_error(&OpenError::Address(ReportError::UnexpectedId { id: 5 })),
        "cannot read the MAC address: unexpected report ID 0x05"
    );
    let heading = |driver, error| {
        text::setup::heading(&Missing {
            driver,
            error: Some(error),
        })
    };
    assert_eq!(
        heading(Driver::ViGEmBus, DriverError::Bus("access denied".into())),
        "ViGEmBus cannot be used: access denied"
    );
    assert_eq!(
        heading(
            Driver::HidHide,
            DriverError::HidHide(HidHideError::Failed("bad reply".into()))
        ),
        "HidHide cannot be used: bad reply"
    );
    assert_eq!(
        heading(
            Driver::HidHide,
            DriverError::HidHide(HidHideError::UnexpectedSize(0))
        ),
        "HidHide cannot be used: HidHide failed: HidHide returned 0 bytes for a BOOLEAN"
    );
}

#[test]
fn only_an_entry_with_a_bluetooth_link_can_be_turned_off() {
    let now = Instant::now();
    assert_eq!(entry_of(&view(Phase::Bridged), now).turn_off(), None);

    let mut on_both = view(Phase::Stopped);
    on_both.has_bluetooth = true;

    assert_eq!(
        entry_of(&on_both, now).turn_off(),
        Some(Command::TurnOff(MAC))
    );
}

#[test]
fn an_entry_carries_its_idle_time_and_asks_for_another() {
    let mut chosen = view(Phase::Bridged);
    chosen.idle_time = Some(IdleTime::Ten);

    let entry = entry_of(&chosen, Instant::now());

    assert_eq!(entry.idle_time, Some(IdleTime::Ten));
    assert_eq!(
        entry.choose_idle_time(None),
        Command::SetIdleTime(MAC, None)
    );
}

#[test]
fn a_failure_to_turn_off_is_a_warning_naming_the_controller() {
    let mut failed = view(Phase::Bridged);
    assert_eq!(Notice::cannot_turn_off(&failed), None);
    failed.turn_off_error = Some(TurnOffError::Failed("access is denied".into()));

    assert_eq!(
        Notice::cannot_turn_off(&failed),
        Some(Notice::warning(
            "Could not turn off DualSense 00:01: access is denied"
        ))
    );

    failed.nickname = Some("Sofa".into());
    failed.turn_off_error = Some(TurnOffError::NoRadio);
    assert_eq!(
        Notice::cannot_turn_off(&failed),
        Some(Notice::warning(
            "Could not turn off Sofa: no Bluetooth radio was found"
        ))
    );
}
