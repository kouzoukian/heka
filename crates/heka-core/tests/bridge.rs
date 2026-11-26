mod common;

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use heka_core::bridge::{Bridge, BridgeEnd, LiveInput, OpenLink, Plugged};
use heka_core::calibration::Calibration;
use heka_core::dualsense::{RumbleMode, usb_output_report};
use heka_core::feedback::{
    Feedback, FeedbackEvent, FeedbackSource, Lightbar, PlayerLights, Rumble,
};
use heka_core::link::{
    FeedbackFormat, LinkRead, LinkReader, LinkWriteError, LinkWriter, ReportFormat,
};
use heka_core::preferences::{LivePreferences, Preferences, RumbleStrength};
use heka_core::state::{Buttons, ControllerState};
use heka_core::virtual_controller::{OutputType, VirtualController, VirtualError};

enum Step {
    Report(Vec<u8>),
    Lost,
}

struct FakeLink {
    script: VecDeque<Step>,
    later: Option<Receiver<Step>>,
    writer: Option<Box<FakeWriter>>,
}

impl FakeLink {
    fn new(script: impl IntoIterator<Item = Step>) -> Box<Self> {
        Box::new(Self {
            script: script.into_iter().collect(),
            later: None,
            writer: None,
        })
    }

    fn live() -> (Box<Self>, Sender<Step>, Arc<Mutex<Written>>) {
        let (steps, later) = mpsc::channel();
        let (writer, written) = FakeWriter::new();
        let link = Box::new(Self {
            script: VecDeque::new(),
            later: Some(later),
            writer: Some(writer),
        });
        (link, steps, written)
    }
}

impl LinkReader for FakeLink {
    fn format(&self) -> ReportFormat {
        ReportFormat::DualSenseUsb(Calibration::UNCALIBRATED)
    }

    fn read(&mut self, buf: &mut [u8]) -> LinkRead {
        let step = self.script.pop_front().or_else(|| {
            self.later
                .as_ref()
                .and_then(|later| later.recv_timeout(Duration::from_millis(1)).ok())
        });
        match step {
            Some(Step::Report(bytes)) => {
                buf[..bytes.len()].copy_from_slice(&bytes);
                LinkRead::Report(bytes.len())
            }
            Some(Step::Lost) => LinkRead::Lost,
            None => {
                std::thread::sleep(Duration::from_millis(1));
                LinkRead::Timeout
            }
        }
    }

    fn feedback_writer(&mut self) -> Option<Box<dyn LinkWriter>> {
        self.writer
            .take()
            .map(|writer| writer as Box<dyn LinkWriter>)
    }
}

#[derive(Default)]
struct Written {
    reports: Vec<Vec<u8>>,
    started: usize,
}

struct FakeWriter {
    written: Arc<Mutex<Written>>,
    gate: Option<Receiver<()>>,
}

impl FakeWriter {
    fn new() -> (Box<Self>, Arc<Mutex<Written>>) {
        let written = Arc::new(Mutex::new(Written::default()));
        let writer = Box::new(Self {
            written: Arc::clone(&written),
            gate: None,
        });
        (writer, written)
    }
}

impl LinkWriter for FakeWriter {
    fn format(&self) -> FeedbackFormat {
        FeedbackFormat::DualSenseUsb(RumbleMode::CompatibleV2)
    }

    fn write(&mut self, report: &[u8]) -> Result<(), LinkWriteError> {
        self.written.lock().unwrap().started += 1;
        if let Some(gate) = &self.gate {
            let _ = gate.recv();
        }
        self.written.lock().unwrap().reports.push(report.to_vec());
        Ok(())
    }
}

#[derive(Default)]
struct Record {
    updates: Vec<ControllerState>,
    unplugged: bool,
}

struct FakeController {
    record: Arc<Mutex<Record>>,
    fail_updates: bool,
    feedback: Option<Receiver<Feedback>>,
}

impl FakeController {
    fn new() -> (Box<Self>, Arc<Mutex<Record>>) {
        let record = Arc::new(Mutex::new(Record::default()));
        let controller = Box::new(Self {
            record: Arc::clone(&record),
            fail_updates: false,
            feedback: None,
        });
        (controller, record)
    }

    fn with_feedback() -> (Box<Self>, Arc<Mutex<Record>>, Sender<Feedback>) {
        let (mut controller, record) = Self::new();
        let (game, feedback) = mpsc::channel();
        controller.feedback = Some(feedback);
        (controller, record, game)
    }
}

impl VirtualController for FakeController {
    fn output_type(&self) -> OutputType {
        OutputType::Xbox
    }

    fn update(&mut self, state: &ControllerState) -> Result<(), VirtualError> {
        if self.fail_updates {
            return Err(VirtualError::new("the bus went away"));
        }
        self.record.lock().unwrap().updates.push(*state);
        Ok(())
    }

    fn unplug(self: Box<Self>) -> Result<(), VirtualError> {
        self.record.lock().unwrap().unplugged = true;
        Ok(())
    }

    fn feedback(&mut self) -> Option<Box<dyn FeedbackSource>> {
        self.feedback
            .take()
            .map(|feedback| Box::new(FakeFeedback(feedback)) as Box<dyn FeedbackSource>)
    }
}

struct FakeFeedback(Receiver<Feedback>);

impl FeedbackSource for FakeFeedback {
    fn next(&mut self, timeout: Duration) -> FeedbackEvent {
        match self.0.recv_timeout(timeout) {
            Ok(feedback) => FeedbackEvent::Feedback(feedback),
            Err(RecvTimeoutError::Timeout) => FeedbackEvent::Timeout,
            Err(RecvTimeoutError::Disconnected) => FeedbackEvent::Closed,
        }
    }
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn cross() -> Vec<u8> {
    common::derived(&[(8, 0x28)])
}

#[test]
fn captured_reports_reach_the_virtual_controller_in_order() {
    let idle = common::captured_idle();
    let (controller, record) = FakeController::new();
    let live = Arc::new(LiveInput::default());
    let link = FakeLink::new([
        Step::Report(idle[0].clone()),
        Step::Report(cross()),
        Step::Report(idle[1].clone()),
    ]);

    let bridge = Bridge::start(link, controller, Arc::clone(&live)).unwrap();
    wait_until(|| record.lock().unwrap().updates.len() == 3);
    bridge.stop();

    let record = record.lock().unwrap();
    let buttons: Vec<Buttons> = record.updates.iter().map(|s| s.buttons).collect();
    assert_eq!(buttons, [Buttons::NONE, Buttons::CROSS, Buttons::NONE]);
}

#[test]
fn the_live_input_shows_the_latest_state_while_bridged() {
    let (controller, record) = FakeController::new();
    let live = Arc::new(LiveInput::default());
    assert_eq!(live.latest(), None);

    let bridge = Bridge::start(
        FakeLink::new([Step::Report(cross())]),
        controller,
        Arc::clone(&live),
    )
    .unwrap();
    wait_until(|| record.lock().unwrap().updates.len() == 1);

    assert_eq!(live.latest().map(|s| s.buttons), Some(Buttons::CROSS));
    bridge.stop();
    assert_eq!(live.latest(), None);
}

#[test]
fn stopping_hands_back_the_link_and_the_virtual_controller() {
    let (controller, record) = FakeController::new();
    let bridge = Bridge::start(FakeLink::new([]), controller, Arc::default()).unwrap();

    let BridgeEnd::Stopped { link, controller } = bridge.stop() else {
        panic!("expected Stopped");
    };
    assert_eq!(
        link.format(),
        ReportFormat::DualSenseUsb(Calibration::UNCALIBRATED)
    );
    assert!(!record.lock().unwrap().unplugged);
    controller.unplug().unwrap();
    assert!(record.lock().unwrap().unplugged);
}

#[test]
fn dropping_a_bridge_removes_the_virtual_controller() {
    let (controller, record) = FakeController::new();
    let bridge = Bridge::start(FakeLink::new([]), controller, Arc::default()).unwrap();
    drop(bridge);
    assert!(record.lock().unwrap().unplugged);
}

#[test]
fn losing_the_link_releases_everything_and_hands_back_the_virtual_controller() {
    let (controller, record) = FakeController::new();
    let live = Arc::new(LiveInput::default());
    let bridge = Bridge::start(
        FakeLink::new([Step::Report(cross()), Step::Lost]),
        controller,
        Arc::clone(&live),
    )
    .unwrap();

    wait_until(|| bridge.is_finished());
    let BridgeEnd::LinkLost { controller } = bridge.stop() else {
        panic!("expected LinkLost");
    };
    {
        let record = record.lock().unwrap();
        assert_eq!(record.updates.last(), Some(&ControllerState::NEUTRAL));
        assert!(!record.unplugged, "a Held Virtual Controller stays");
    }
    assert_eq!(live.latest(), None);

    let bridge = Bridge::resume(
        OpenLink::new(FakeLink::new([Step::Report(cross())])),
        controller,
        Arc::clone(&live),
    )
    .unwrap();
    wait_until(|| live.latest().is_some());
    drop(bridge);
    let record = record.lock().unwrap();
    assert_eq!(
        record.updates.last().map(|s| s.buttons),
        Some(Buttons::CROSS)
    );
    assert!(record.unplugged);
}

#[test]
fn reports_of_another_kind_are_skipped() {
    let (controller, record) = FakeController::new();
    let bridge = Bridge::start(
        FakeLink::new([
            Step::Report(common::derived(&[(0, 0x31)])),
            Step::Report(cross()[..10].to_vec()),
            Step::Report(cross()),
        ]),
        controller,
        Arc::default(),
    )
    .unwrap();

    wait_until(|| !record.lock().unwrap().updates.is_empty());
    bridge.stop();
    let record = record.lock().unwrap();
    assert_eq!(record.updates.len(), 1);
    assert_eq!(record.updates[0].buttons, Buttons::CROSS);
}

#[test]
fn a_failing_virtual_controller_is_removed_and_the_link_handed_back() {
    let (mut controller, record) = FakeController::new();
    controller.fail_updates = true;
    let bridge = Bridge::start(
        FakeLink::new([Step::Report(cross())]),
        controller,
        Arc::default(),
    )
    .unwrap();

    wait_until(|| bridge.is_finished());
    assert!(matches!(
        bridge.stop(),
        BridgeEnd::VirtualControllerFailed { .. }
    ));
    assert!(record.lock().unwrap().unplugged);
}

fn rumble(large: u8, small: u8) -> Feedback {
    Feedback {
        rumble: Rumble { large, small },
        ..Feedback::OFF
    }
}

fn output_report(feedback: Feedback) -> Vec<u8> {
    usb_output_report(&feedback, RumbleMode::CompatibleV2).to_vec()
}

#[test]
fn rumble_from_the_game_reaches_the_controller() {
    let (link, _steps, written) = FakeLink::live();
    let (controller, _, game) = FakeController::with_feedback();
    let bridge = Bridge::start(link, controller, Arc::default()).unwrap();

    game.send(rumble(200, 64)).unwrap();
    wait_until(|| written.lock().unwrap().reports.len() == 1);

    assert_eq!(
        written.lock().unwrap().reports[0],
        output_report(rumble(200, 64))
    );
    bridge.stop();
}

#[test]
fn the_rumble_stops_when_the_game_stops_it() {
    let (link, _steps, written) = FakeLink::live();
    let (controller, _, game) = FakeController::with_feedback();
    let bridge = Bridge::start(link, controller, Arc::default()).unwrap();

    game.send(rumble(200, 64)).unwrap();
    game.send(Feedback::OFF).unwrap();
    wait_until(|| written.lock().unwrap().reports.len() == 2);

    assert_eq!(
        written.lock().unwrap().reports[1],
        output_report(Feedback::OFF)
    );
    bridge.stop();
}

#[test]
fn stopping_the_bridge_stops_the_rumble() {
    let (link, _steps, written) = FakeLink::live();
    let (controller, _, game) = FakeController::with_feedback();
    let bridge = Bridge::start(link, controller, Arc::default()).unwrap();

    game.send(rumble(200, 64)).unwrap();
    wait_until(|| written.lock().unwrap().reports.len() == 1);
    bridge.stop();

    assert_eq!(
        written.lock().unwrap().reports,
        [output_report(rumble(200, 64)), output_report(Feedback::OFF)]
    );
}

#[test]
fn losing_the_link_stops_the_rumble() {
    let (link, steps, written) = FakeLink::live();
    let (controller, _, game) = FakeController::with_feedback();
    let bridge = Bridge::start(link, controller, Arc::default()).unwrap();

    game.send(rumble(200, 64)).unwrap();
    wait_until(|| written.lock().unwrap().reports.len() == 1);
    steps.send(Step::Lost).unwrap();

    wait_until(|| written.lock().unwrap().reports.len() == 2);
    assert_eq!(
        written.lock().unwrap().reports[1],
        output_report(Feedback::OFF)
    );
    assert!(matches!(bridge.stop(), BridgeEnd::LinkLost { .. }));
}

#[test]
fn rumble_still_reaches_the_controller_after_a_hold() {
    let (link, steps, _) = FakeLink::live();
    let (controller, _, game) = FakeController::with_feedback();
    let bridge = Bridge::start(link, controller, Arc::default()).unwrap();
    steps.send(Step::Lost).unwrap();
    wait_until(|| bridge.is_finished());
    let BridgeEnd::LinkLost { controller } = bridge.stop() else {
        panic!("expected LinkLost");
    };

    let (back, _steps, written) = FakeLink::live();
    let bridge = Bridge::resume(OpenLink::new(back), controller, Arc::default()).unwrap();
    game.send(rumble(200, 64)).unwrap();
    wait_until(|| written.lock().unwrap().reports.len() == 1);
    assert_eq!(
        written.lock().unwrap().reports[0],
        output_report(rumble(200, 64))
    );
    bridge.stop();
}

#[test]
fn rumble_still_reaches_the_controller_after_a_stop_and_start() {
    let (link, _steps, written) = FakeLink::live();
    let (controller, _, game) = FakeController::with_feedback();
    let bridge = Bridge::start(link, controller, Arc::default()).unwrap();
    let BridgeEnd::Stopped { link, controller } = bridge.stop() else {
        panic!("expected Stopped");
    };

    let bridge = Bridge::resume(link, controller, Arc::default()).unwrap();
    game.send(rumble(10, 20)).unwrap();
    wait_until(|| written.lock().unwrap().reports.len() == 1);
    assert_eq!(
        written.lock().unwrap().reports[0],
        output_report(rumble(10, 20))
    );
    bridge.stop();
}

#[test]
fn a_bridge_that_never_rumbled_sends_the_controller_nothing() {
    let (link, steps, written) = FakeLink::live();
    let (controller, record, game) = FakeController::with_feedback();
    let bridge = Bridge::start(link, controller, Arc::default()).unwrap();

    game.send(Feedback::OFF).unwrap();
    steps.send(Step::Report(cross())).unwrap();
    wait_until(|| record.lock().unwrap().updates.len() == 1);
    bridge.stop();

    assert_eq!(written.lock().unwrap().started, 0);
}

#[test]
fn feedback_the_controller_already_shows_is_not_sent_again() {
    let (link, _steps, written) = FakeLink::live();
    let (controller, _, game) = FakeController::with_feedback();
    let bridge = Bridge::start(link, controller, Arc::default()).unwrap();

    game.send(rumble(200, 64)).unwrap();
    game.send(rumble(200, 64)).unwrap();
    game.send(rumble(10, 0)).unwrap();
    wait_until(|| written.lock().unwrap().reports.len() == 2);

    assert_eq!(
        written.lock().unwrap().reports,
        [output_report(rumble(200, 64)), output_report(rumble(10, 0))]
    );
    bridge.stop();
}

#[test]
fn the_rumble_stops_when_the_virtual_controller_stops_sending_feedback() {
    let (link, _steps, written) = FakeLink::live();
    let (controller, _, game) = FakeController::with_feedback();
    let bridge = Bridge::start(link, controller, Arc::default()).unwrap();

    game.send(rumble(200, 64)).unwrap();
    wait_until(|| written.lock().unwrap().reports.len() == 1);
    drop(game);

    wait_until(|| written.lock().unwrap().reports.len() == 2);
    assert_eq!(
        written.lock().unwrap().reports[1],
        output_report(Feedback::OFF)
    );
    assert!(!bridge.is_finished());
    bridge.stop();
}

#[test]
fn a_feedback_write_that_hangs_does_not_delay_input() {
    let (mut link, steps, written) = FakeLink::live();
    let (release, gate) = mpsc::channel();
    link.writer.as_mut().unwrap().gate = Some(gate);
    let (controller, record, game) = FakeController::with_feedback();
    let bridge = Bridge::start(link, controller, Arc::default()).unwrap();

    game.send(rumble(200, 64)).unwrap();
    wait_until(|| written.lock().unwrap().started == 1);
    steps.send(Step::Report(cross())).unwrap();
    wait_until(|| record.lock().unwrap().updates.len() == 1);
    assert!(written.lock().unwrap().reports.is_empty());

    drop(release);
    bridge.stop();
}

#[test]
fn a_bridge_moved_to_another_link_shows_the_games_last_feedback_there() {
    let (usb, _usb_steps, on_usb) = FakeLink::live();
    let (controller, _, game) = FakeController::with_feedback();
    let bridge = Bridge::start(usb, controller, Arc::default()).unwrap();
    let asked = Feedback {
        lightbar: Some(Lightbar {
            red: 1,
            green: 2,
            blue: 3,
        }),
        ..rumble(200, 64)
    };
    game.send(asked).unwrap();
    wait_until(|| on_usb.lock().unwrap().reports.len() == 1);
    let BridgeEnd::Stopped { controller, .. } = bridge.stop() else {
        panic!("expected Stopped");
    };

    let (other, _steps, on_other) = FakeLink::live();
    let bridge = Bridge::resume(OpenLink::new(other), controller, Arc::default()).unwrap();
    wait_until(|| !on_other.lock().unwrap().reports.is_empty());
    assert_eq!(on_other.lock().unwrap().reports[0], output_report(asked));
    bridge.stop();
}

const PINK: Lightbar = Lightbar {
    red: 255,
    green: 0,
    blue: 128,
};

struct Following {
    bridge: Bridge,
    preferences: Arc<LivePreferences>,
    written: Arc<Mutex<Written>>,
    game: Sender<Feedback>,
    _steps: Sender<Step>,
}

fn following(chosen: Preferences) -> Following {
    let (link, steps, written) = FakeLink::live();
    let (controller, _, game) = FakeController::with_feedback();
    let preferences = Arc::new(LivePreferences::default());
    preferences.set(chosen);
    let mut controller = Plugged::new(controller);
    controller.follow(Arc::clone(&preferences));
    Following {
        bridge: Bridge::resume(OpenLink::new(link), controller, Arc::default()).unwrap(),
        preferences,
        written,
        game,
        _steps: steps,
    }
}

#[test]
fn chosen_lights_show_as_soon_as_the_bridge_starts() {
    let Following {
        bridge,
        written,
        game: _game,
        _steps,
        ..
    } = following(Preferences {
        lightbar: Some(PINK),
        player_lights: Some(PlayerLights::Two),
        ..Preferences::default()
    });
    wait_until(|| written.lock().unwrap().reports.len() == 1);

    assert_eq!(
        written.lock().unwrap().reports[0],
        output_report(Feedback {
            lightbar: Some(PINK),
            player_lights: Some(PlayerLights::Two),
            ..Feedback::OFF
        })
    );
    bridge.stop();
}

#[test]
fn the_rumble_reaches_the_controller_at_the_chosen_strength() {
    let Following {
        bridge,
        written,
        game,
        _steps,
        ..
    } = following(Preferences {
        rumble: RumbleStrength::percent(50),
        ..Preferences::default()
    });

    game.send(rumble(200, 65)).unwrap();
    wait_until(|| written.lock().unwrap().reports.len() == 1);

    assert_eq!(
        written.lock().unwrap().reports[0],
        output_report(rumble(100, 33))
    );
    bridge.stop();
}

#[test]
fn a_running_bridge_follows_changed_preferences() {
    let Following {
        bridge,
        preferences,
        written,
        game,
        _steps,
    } = following(Preferences::default());
    game.send(rumble(200, 64)).unwrap();
    wait_until(|| written.lock().unwrap().reports.len() == 1);

    preferences.set(Preferences {
        rumble: RumbleStrength::OFF,
        lightbar: Some(PINK),
        ..Preferences::default()
    });
    wait_until(|| written.lock().unwrap().reports.len() == 2);

    assert_eq!(
        written.lock().unwrap().reports[1],
        output_report(Feedback {
            lightbar: Some(PINK),
            ..Feedback::OFF
        })
    );
    bridge.stop();
}

#[test]
fn lights_no_longer_chosen_go_back_to_how_they_started() {
    let Following {
        bridge,
        preferences,
        written,
        game: _game,
        _steps,
    } = following(Preferences {
        lightbar: Some(PINK),
        player_lights: Some(PlayerLights::One),
        ..Preferences::default()
    });
    wait_until(|| written.lock().unwrap().reports.len() == 1);

    preferences.set(Preferences::default());
    wait_until(|| written.lock().unwrap().reports.len() == 2);

    assert_eq!(
        written.lock().unwrap().reports[1],
        output_report(Feedback {
            lightbar: Some(Lightbar::DEFAULT),
            player_lights: Some(PlayerLights::Off),
            ..Feedback::OFF
        })
    );
    bridge.stop();
}

#[test]
fn a_bridge_keeps_following_its_preferences_when_it_moves_to_another_link() {
    let Following {
        bridge,
        preferences,
        game,
        _steps,
        ..
    } = following(Preferences::default());
    let BridgeEnd::Stopped { controller, .. } = bridge.stop() else {
        panic!("expected Stopped");
    };
    let (other, _steps, written) = FakeLink::live();
    let bridge = Bridge::resume(OpenLink::new(other), controller, Arc::default()).unwrap();

    preferences.set(Preferences {
        rumble: RumbleStrength::percent(25),
        ..Preferences::default()
    });
    std::thread::sleep(Duration::from_millis(200));
    game.send(rumble(200, 100)).unwrap();
    wait_until(|| !written.lock().unwrap().reports.is_empty());

    assert_eq!(
        written.lock().unwrap().reports[0],
        output_report(rumble(50, 25))
    );
    bridge.stop();
}
