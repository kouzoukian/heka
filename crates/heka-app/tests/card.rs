//! What a Controller's card says, from the Supervisor's view of the Controller.

use std::sync::Arc;
use std::time::{Duration, Instant};

use heka_app::ui::model::{ActiveLink, CardModel, CardState, Command, Notice, Screen};
use heka_core::bridge::LiveInput;
use heka_core::calibration::Calibration;
use heka_core::lifecycle::Phase;
use heka_core::link::{LinkRead, LinkReader, ReportFormat};
use heka_core::mac::MacAddress;
use heka_core::settings::Settings;
use heka_core::state::Buttons;
use heka_core::supervisor::{ControllerView, Supervisor};
use heka_core::virtual_controller::OutputType;

const MAC: MacAddress = MacAddress([0x02, 0, 0, 0, 0, 1]);

fn view(phase: Phase) -> ControllerView {
    ControllerView {
        mac: MAC,
        phase,
        output_type: OutputType::Xbox,
        live: Arc::new(LiveInput::default()),
        error: None,
    }
}

/// The card while ViGEmBus is there.
fn card_of(view: &ControllerView, now: Instant) -> CardModel {
    CardModel::from_view(view, now, false)
}

/// A DualSense at rest but for Cross held down.
struct CrossPressed;

impl LinkReader for CrossPressed {
    fn format(&self) -> ReportFormat {
        ReportFormat::DualSenseUsb(Calibration::UNCALIBRATED)
    }

    fn read(&mut self, buf: &mut [u8]) -> LinkRead {
        std::thread::sleep(Duration::from_millis(1));
        // Report ID 1, sticks centred, d-pad released (8) and Cross (bit 5), at
        // the offsets heka-core takes from `hid-playstation.c`.
        let mut report = [0u8; 64];
        report[0] = 0x01;
        report[1..5].fill(0x80);
        report[8] = 0x28;
        buf[..report.len()].copy_from_slice(&report);
        LinkRead::Report(report.len())
    }
}

#[test]
fn each_phase_has_its_card_state() {
    let now = Instant::now();
    let state = |phase| card_of(&view(phase), now).state;

    assert_eq!(state(Phase::Bridged), CardState::Bridged);
    assert_eq!(state(Phase::Stopped), CardState::Stopped);
    assert_eq!(state(Phase::Waiting), CardState::NotBridged);
}

#[test]
fn a_held_card_counts_down_in_whole_seconds_rounded_up() {
    let now = Instant::now();
    let held = view(Phase::Held {
        until: now + Duration::from_secs(30),
    });
    let state = |after_ms| card_of(&held, now + Duration::from_millis(after_ms)).state;

    assert_eq!(state(0), CardState::Held { seconds_left: 30 });
    assert_eq!(state(1), CardState::Held { seconds_left: 30 });
    assert_eq!(state(1_000), CardState::Held { seconds_left: 29 });
    assert_eq!(state(29_999), CardState::Held { seconds_left: 1 });
    // Past its time but not yet removed.
    assert_eq!(state(31_000), CardState::Held { seconds_left: 0 });
}

#[test]
fn a_card_names_its_controller_and_says_why_a_bridge_failed() {
    let mut failed = view(Phase::Waiting);
    failed.error = Some("no free slot".into());

    let card = card_of(&failed, Instant::now());

    assert_eq!(card.mac, MAC);
    assert_eq!(card.name, "DualSense");
    assert_eq!(
        card.detail().as_deref(),
        Some("Could not Bridge: no free slot")
    );
}

#[test]
fn a_card_explains_why_a_controller_is_not_bridged_and_what_a_hold_waits_for() {
    let now = Instant::now();
    let detail = |phase| card_of(&view(phase), now).detail();

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
fn without_vigem_bus_a_card_says_so_and_still_shows_live_input() {
    let mut supervisor = Supervisor::without_bus(Settings::default());
    supervisor.link_arrived(MAC, Box::new(CrossPressed), Instant::now());
    let deadline = Instant::now() + Duration::from_secs(5);
    while supervisor.controllers()[0].live.latest().is_none() {
        assert!(Instant::now() < deadline, "no input");
        std::thread::sleep(Duration::from_millis(1));
    }

    let card = CardModel::from_view(&supervisor.controllers()[0], Instant::now(), true);

    assert_eq!(card.state, CardState::NotBridged);
    assert_eq!(
        card.detail().as_deref(),
        Some("Not Bridged: ViGEmBus is missing")
    );
    assert!(card.pad.is_some_and(|pad| pad.pressed(Buttons::CROSS)));
}

#[test]
fn without_vigem_bus_a_card_also_says_why_input_cannot_be_read() {
    let mut waiting = view(Phase::Waiting);
    waiting.error = Some("no thread".into());

    let card = CardModel::from_view(&waiting, Instant::now(), true);

    assert_eq!(
        card.detail().as_deref(),
        Some("Not Bridged: ViGEmBus is missing (no thread)")
    );
}

#[test]
fn a_held_controller_has_no_active_link() {
    let now = Instant::now();
    let link = |phase| card_of(&view(phase), now).active_link;

    assert_eq!(link(Phase::Bridged), Some(ActiveLink::Usb));
    assert_eq!(link(Phase::Stopped), Some(ActiveLink::Usb));
    assert_eq!(link(Phase::Held { until: now }), None);
}

#[test]
fn the_output_switch_shows_the_controllers_output_type_and_asks_for_the_chosen_one() {
    let mut ps4 = view(Phase::Bridged);
    ps4.output_type = OutputType::Ps4;

    let card = card_of(&ps4, Instant::now());

    assert_eq!(card.output_type, OutputType::Ps4);
    assert_eq!(
        card.choose(OutputType::Xbox),
        Some(Command::SetOutputType(MAC, OutputType::Xbox))
    );
    // Choosing what is already chosen asks for nothing.
    assert_eq!(card.choose(OutputType::Ps4), None);
}

#[test]
fn the_card_button_starts_a_stopped_controller_and_stops_any_other() {
    let now = Instant::now();
    let command = |phase| card_of(&view(phase), now).toggle();

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
        cards: vec![card_of(&held, now + Duration::from_millis(after_ms))],
    };

    // The window redraws only when two samples differ.
    assert_eq!(screen(1), screen(900));
    assert_ne!(screen(900), screen(1_100));
}

#[test]
fn the_status_bar_counts_controllers_and_bridges() {
    let now = Instant::now();
    let summary = |phases: &[Phase]| {
        Screen {
            starting: false,
            missing: Vec::new(),
            notices: Vec::new(),
            cards: phases
                .iter()
                .map(|&phase| card_of(&view(phase), now))
                .collect(),
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
