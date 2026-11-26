//! What a Controller's card says, from the Supervisor's view of the Controller.

use std::sync::Arc;
use std::time::{Duration, Instant};

use heka_app::ui::model::{ActiveLink, CardModel, CardState, Command, Notice, Screen};
use heka_core::bridge::LiveInput;
use heka_core::lifecycle::Phase;
use heka_core::mac::MacAddress;
use heka_core::supervisor::ControllerView;
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

#[test]
fn each_phase_has_its_card_state() {
    let now = Instant::now();
    let state = |phase| CardModel::from_view(&view(phase), now).state;

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
    let state =
        |after_ms| CardModel::from_view(&held, now + Duration::from_millis(after_ms)).state;

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

    let card = CardModel::from_view(&failed, Instant::now());

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
    let detail = |phase| CardModel::from_view(&view(phase), now).detail();

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
fn a_held_controller_has_no_active_link() {
    let now = Instant::now();
    let link = |phase| CardModel::from_view(&view(phase), now).active_link;

    assert_eq!(link(Phase::Bridged), Some(ActiveLink::Usb));
    assert_eq!(link(Phase::Stopped), Some(ActiveLink::Usb));
    assert_eq!(link(Phase::Held { until: now }), None);
}

#[test]
fn the_output_switch_shows_the_controllers_output_type_and_asks_for_the_chosen_one() {
    let mut ps4 = view(Phase::Bridged);
    ps4.output_type = OutputType::Ps4;

    let card = CardModel::from_view(&ps4, Instant::now());

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
    let command = |phase| CardModel::from_view(&view(phase), now).toggle();

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
        notices: vec![Notice::warning("warning")],
        cards: vec![CardModel::from_view(
            &held,
            now + Duration::from_millis(after_ms),
        )],
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
            notices: Vec::new(),
            cards: phases
                .iter()
                .map(|&phase| CardModel::from_view(&view(phase), now))
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
