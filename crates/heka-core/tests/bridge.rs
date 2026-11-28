mod common;

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use heka_core::bridge::{Bridge, BridgeEnd, LiveInput, OpenLink, Plugged};
use heka_core::calibration::Calibration;
use heka_core::desktop::{Chord, Desk, DesktopError, DesktopEvent, DesktopOutput, Key, WheelStep};
use heka_core::dualsense::{RumbleMode, is_bluetooth_lightbar_setup_report, usb_output_report};
use heka_core::feedback::{
    Feedback, FeedbackEvent, FeedbackSource, Lightbar, Motor, PlayerLights, RUMBLE_TEST_TIME,
    Rumble,
};
use heka_core::link::{
    FeedbackFormat, LinkRead, LinkReader, LinkWriteError, LinkWriter, ReportFormat,
};
use heka_core::live_mapper::LiveMapper;
use heka_core::macros::Mode;
use heka_core::preferences::{
    LightbarChoice, LivePreferences, MuteLight, Preferences, RumbleStrength,
};
use heka_core::profile::{Binding, Input, Mapper, Output, Profile};
use heka_core::state::{Buttons, ControllerState};
use heka_core::threads::{SystemThreads, Task, Threads};
use heka_core::trigger_effect::{EffectChange, EffectKind, TriggerEffect, TriggerEffects};
use heka_core::tuning::TriggerSide;
use heka_core::virtual_controller::{
    OutputType, VirtualController, VirtualError, VirtualFailure, VirtualFailures,
};

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
    failing: usize,
}

struct FakeWriter {
    written: Arc<Mutex<Written>>,
    gate: Option<Receiver<()>>,
    format: FeedbackFormat,
}

impl FakeWriter {
    fn new() -> (Box<Self>, Arc<Mutex<Written>>) {
        let written = Arc::new(Mutex::new(Written::default()));
        let writer = Box::new(Self {
            written: Arc::clone(&written),
            gate: None,
            format: FeedbackFormat::DualSenseUsb(RumbleMode::CompatibleV2),
        });
        (writer, written)
    }
}

impl LinkWriter for FakeWriter {
    fn format(&self) -> FeedbackFormat {
        self.format.clone()
    }

    fn write(&mut self, report: &[u8]) -> Result<(), LinkWriteError> {
        self.written.lock().unwrap().started += 1;
        if let Some(gate) = &self.gate {
            let _ = gate.recv();
        }
        let mut written = self.written.lock().unwrap();
        if written.failing > 0 {
            written.failing -= 1;
            return Err(LinkWriteError::new("the Controller is busy"));
        }
        written.reports.push(report.to_vec());
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
    fail_unplug: bool,
    feedback: Option<Receiver<Feedback>>,
    refuse_feedback: bool,
}

impl FakeController {
    fn new() -> (Box<Self>, Arc<Mutex<Record>>) {
        let record = Arc::new(Mutex::new(Record::default()));
        let controller = Box::new(Self {
            record: Arc::clone(&record),
            fail_updates: false,
            fail_unplug: false,
            refuse_feedback: false,
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
        if self.fail_unplug {
            return Err(VirtualError::new("it will not leave"));
        }
        self.record.lock().unwrap().unplugged = true;
        Ok(())
    }

    fn feedback(&mut self) -> Result<Option<Box<dyn FeedbackSource>>, VirtualError> {
        if self.refuse_feedback {
            return Err(VirtualError::new("no notifications"));
        }
        Ok(self
            .feedback
            .take()
            .map(|feedback| Box::new(FakeFeedback(feedback)) as Box<dyn FeedbackSource>))
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
        lightbar: Some(LightbarChoice::Colour(PINK)),
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
fn a_rainbow_keeps_changing_the_lightbar_with_nothing_from_the_game() {
    let Following {
        bridge,
        written,
        game: _game,
        _steps,
        ..
    } = following(Preferences {
        lightbar: Some(LightbarChoice::RAINBOW),
        ..Preferences::default()
    });
    wait_until(|| written.lock().unwrap().reports.len() >= 3);
    bridge.stop();

    let written = written.lock().unwrap();
    assert_ne!(written.reports[0], written.reports[1]);
    assert_ne!(written.reports[1], written.reports[2]);
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
        lightbar: Some(LightbarChoice::Colour(PINK)),
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
        lightbar: Some(LightbarChoice::Colour(PINK)),
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
        lightbar: Some(LightbarChoice::Colour(PINK)),
        ..Preferences::default()
    });
    wait_until(|| written.lock().unwrap().reports.len() == 1);
    game.send(rumble(200, 100)).unwrap();
    wait_until(|| written.lock().unwrap().reports.len() == 2);

    assert_eq!(
        written.lock().unwrap().reports[1],
        output_report(Feedback {
            lightbar: Some(PINK),
            ..rumble(50, 25)
        })
    );
    bridge.stop();
}

#[test]
fn a_rumble_test_runs_one_motor_at_the_chosen_strength_and_stops_by_itself() {
    let Following {
        bridge,
        written,
        game: _game,
        _steps,
        ..
    } = following(Preferences {
        rumble: RumbleStrength::percent(50),
        ..Preferences::default()
    });

    let started = Instant::now();
    bridge.test_rumble(Motor::Large);
    wait_until(|| written.lock().unwrap().reports.len() == 2);

    assert!(started.elapsed() >= RUMBLE_TEST_TIME);
    assert_eq!(
        reports(&written),
        [output_report(rumble(128, 0)), output_report(Feedback::OFF)]
    );
    bridge.stop();
}

#[test]
fn a_rumble_test_runs_the_named_motor_also_with_the_motors_swapped() {
    let Following {
        bridge,
        written,
        game: _game,
        _steps,
        ..
    } = following(Preferences {
        swap_motors: true,
        ..Preferences::default()
    });

    bridge.test_rumble(Motor::Small);
    wait_until(|| !reports(&written).is_empty());

    assert_eq!(reports(&written)[0], output_report(rumble(0, 255)));
    bridge.stop();
}

#[test]
fn a_rumble_test_stands_in_for_the_games_rumble_until_it_is_over() {
    let Following {
        bridge,
        written,
        game,
        _steps,
        ..
    } = following(Preferences::default());
    game.send(rumble(40, 40)).unwrap();
    wait_until(|| reports(&written).len() == 1);

    bridge.test_rumble(Motor::Large);
    wait_until(|| reports(&written).len() == 2);
    let green = Some(Lightbar {
        red: 0,
        green: 255,
        blue: 0,
    });
    game.send(Feedback {
        lightbar: green,
        ..rumble(60, 60)
    })
    .unwrap();
    wait_until(|| reports(&written).len() == 4);

    assert_eq!(
        reports(&written)[1..],
        [
            output_report(rumble(255, 0)),
            output_report(Feedback {
                lightbar: green,
                ..rumble(255, 0)
            }),
            output_report(Feedback {
                lightbar: green,
                ..rumble(60, 60)
            }),
        ]
    );
    bridge.stop();
}

#[test]
fn an_ended_rumble_test_stops_the_motors_at_once() {
    let Following {
        bridge,
        written,
        game: _game,
        _steps,
        ..
    } = following(Preferences::default());
    bridge.test_rumble(Motor::Large);
    wait_until(|| reports(&written).len() == 1);

    let ended = Instant::now();
    bridge.end_rumble_test();
    wait_until(|| reports(&written).len() == 2);

    assert!(ended.elapsed() < RUMBLE_TEST_TIME / 2);
    assert_eq!(reports(&written)[1], output_report(Feedback::OFF));
    bridge.stop();
}

#[test]
fn a_rumble_test_ends_with_its_bridge() {
    let Following {
        bridge,
        written,
        game: _game,
        _steps,
        ..
    } = following(Preferences::default());
    bridge.test_rumble(Motor::Large);
    wait_until(|| reports(&written).len() == 1);
    let BridgeEnd::Stopped { controller, .. } = bridge.stop() else {
        panic!("expected Stopped");
    };
    assert_eq!(reports(&written)[1], output_report(Feedback::OFF));

    let (other, _steps, on_other) = FakeLink::live();
    let bridge = Bridge::resume(OpenLink::new(other), controller, Arc::default()).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    bridge.stop();

    assert!(reports(&on_other).is_empty());
}

fn fail_writes(written: &Arc<Mutex<Written>>, count: usize) {
    written.lock().unwrap().failing = count;
}

fn reports(written: &Arc<Mutex<Written>>) -> Vec<Vec<u8>> {
    written.lock().unwrap().reports.clone()
}

fn started(written: &Arc<Mutex<Written>>) -> usize {
    written.lock().unwrap().started
}

struct Rumbling {
    bridge: Bridge,
    written: Arc<Mutex<Written>>,
    game: Sender<Feedback>,
    steps: Sender<Step>,
}

fn rumbling() -> Rumbling {
    let (link, steps, written) = FakeLink::live();
    let (controller, _, game) = FakeController::with_feedback();
    let bridge = Bridge::start(link, controller, Arc::default()).unwrap();
    game.send(rumble(200, 64)).unwrap();
    wait_until(|| reports(&written).len() == 1);
    Rumbling {
        bridge,
        written,
        game,
        steps,
    }
}

#[test]
fn a_failed_write_is_retried_without_new_feedback() {
    let Rumbling {
        bridge,
        written,
        game,
        steps: _steps,
    } = rumbling();

    fail_writes(&written, 2);
    game.send(Feedback::OFF).unwrap();
    wait_until(|| reports(&written).len() == 2);

    assert_eq!(reports(&written)[1], output_report(Feedback::OFF));
    assert_eq!(started(&written), 4);
    bridge.stop();
}

#[test]
fn a_write_that_keeps_failing_is_retried_at_a_bounded_pace() {
    let Rumbling {
        bridge,
        written,
        game,
        steps: _steps,
    } = rumbling();

    fail_writes(&written, usize::MAX);
    let failing_since = Instant::now();
    game.send(Feedback::OFF).unwrap();
    wait_until(|| started(&written) >= 5);

    assert!(failing_since.elapsed() >= Duration::from_millis(250));
    bridge.stop();
}

#[test]
fn the_game_asking_for_the_same_again_does_not_hurry_a_retry() {
    let Rumbling {
        bridge,
        written,
        game,
        steps: _steps,
    } = rumbling();

    fail_writes(&written, usize::MAX);
    let failing_since = Instant::now();
    for _ in 0..200 {
        game.send(Feedback::OFF).unwrap();
    }
    wait_until(|| started(&written) >= 4);

    assert!(failing_since.elapsed() >= Duration::from_millis(150));
    bridge.stop();
}

#[test]
fn stopping_the_bridge_retries_the_report_that_stops_the_motors() {
    let Rumbling {
        bridge,
        written,
        game: _game,
        steps: _steps,
    } = rumbling();

    fail_writes(&written, 2);
    bridge.stop();

    assert_eq!(
        reports(&written),
        [output_report(rumble(200, 64)), output_report(Feedback::OFF)]
    );
}

#[test]
fn stopping_the_bridge_gives_up_on_a_controller_that_never_takes_the_report() {
    let Rumbling {
        bridge,
        written,
        game: _game,
        steps: _steps,
    } = rumbling();

    fail_writes(&written, usize::MAX);
    assert!(matches!(bridge.stop(), BridgeEnd::Stopped { .. }));

    let attempts = started(&written);
    assert!((3..=6).contains(&attempts), "{attempts}");
}

#[test]
fn a_lost_link_ends_the_bridge_without_insisting_on_the_stop_report() {
    let Rumbling {
        bridge,
        written,
        game: _game,
        steps,
    } = rumbling();

    fail_writes(&written, usize::MAX);
    steps.send(Step::Lost).unwrap();
    wait_until(|| bridge.is_finished());
    assert!(matches!(bridge.stop(), BridgeEnd::LinkLost { .. }));

    assert_eq!(started(&written), 2);
}

#[test]
fn the_motors_are_stopped_when_a_silent_virtual_controller_meets_failing_writes() {
    let Rumbling {
        bridge,
        written,
        game,
        steps: _steps,
    } = rumbling();

    fail_writes(&written, 3);
    drop(game);
    wait_until(|| reports(&written).len() == 2);

    assert_eq!(reports(&written)[1], output_report(Feedback::OFF));
    bridge.stop();
}

#[test]
fn a_light_whose_write_failed_is_not_forgotten_when_the_game_rumbles() {
    let Following {
        bridge,
        written,
        preferences,
        game,
        _steps,
    } = following(Preferences {
        lightbar: Some(LightbarChoice::Colour(PINK)),
        ..Preferences::default()
    });
    wait_until(|| reports(&written).len() == 1);

    fail_writes(&written, 1);
    preferences.set(Preferences::default());
    wait_until(|| started(&written) >= 2);
    game.send(rumble(10, 20)).unwrap();
    let both = output_report(Feedback {
        lightbar: Some(Lightbar::DEFAULT),
        ..rumble(10, 20)
    });
    wait_until(|| reports(&written).last() == Some(&both));

    bridge.stop();
}

#[test]
fn a_bluetooth_lightbar_setup_that_failed_is_sent_again() {
    let (mut link, _steps, written) = FakeLink::live();
    link.writer.as_mut().unwrap().format =
        FeedbackFormat::dualsense_bluetooth(RumbleMode::Compatible);
    fail_writes(&written, 1);
    let (controller, _, _game) = FakeController::with_feedback();
    let preferences = Arc::new(LivePreferences::default());
    preferences.set(Preferences {
        lightbar: Some(LightbarChoice::Colour(PINK)),
        ..Preferences::default()
    });
    let mut controller = Plugged::new(controller);
    controller.follow(preferences);

    let bridge = Bridge::resume(OpenLink::new(link), controller, Arc::default()).unwrap();
    wait_until(|| reports(&written).len() == 2);

    let sent = reports(&written);
    assert!(is_bluetooth_lightbar_setup_report(&sent[0]));
    assert!(!is_bluetooth_lightbar_setup_report(&sent[1]));
    bridge.stop();
}

fn reporting(controller: Box<FakeController>) -> (Plugged, Arc<VirtualFailures>) {
    let failures = Arc::new(VirtualFailures::default());
    let mut plugged = Plugged::new(controller);
    plugged.report_to(Arc::clone(&failures));
    (plugged, failures)
}

fn not_unplugged() -> VirtualFailure {
    VirtualFailure::NotUnplugged("it will not leave".into())
}

#[test]
fn a_failing_virtual_controller_that_cannot_be_removed_is_reported() {
    let (mut controller, _) = FakeController::new();
    controller.fail_updates = true;
    controller.fail_unplug = true;
    let (controller, failures) = reporting(controller);
    let link = OpenLink::new(FakeLink::new([Step::Report(cross())]));

    let bridge = Bridge::resume(link, controller, Arc::default()).unwrap();
    wait_until(|| bridge.is_finished());
    bridge.stop();

    assert_eq!(failures.take(), [not_unplugged()]);
}

#[test]
fn a_dropped_bridge_reports_a_virtual_controller_that_cannot_be_removed() {
    let (mut controller, _) = FakeController::new();
    controller.fail_unplug = true;
    let (controller, failures) = reporting(controller);

    let link = OpenLink::new(FakeLink::new([]));
    drop(Bridge::resume(link, controller, Arc::default()).unwrap());

    assert_eq!(failures.take(), [not_unplugged()]);
}

#[test]
fn a_virtual_controller_that_cannot_be_released_on_a_lost_link_is_reported() {
    let (mut controller, _) = FakeController::new();
    controller.fail_updates = true;
    let (controller, failures) = reporting(controller);
    let link = OpenLink::new(FakeLink::new([Step::Lost]));

    let bridge = Bridge::resume(link, controller, Arc::default()).unwrap();
    wait_until(|| bridge.is_finished());
    let BridgeEnd::LinkLost { mut controller } = bridge.stop() else {
        panic!("expected LinkLost");
    };

    assert!(controller.release().is_err());
    let not_released = VirtualFailure::NotReleased("the bus went away".into());
    assert_eq!(failures.take(), [not_released.clone(), not_released]);
}

struct Refusing(&'static str);

impl Threads for Refusing {
    fn spawn(&self, name: &str, task: Task) -> std::io::Result<std::thread::JoinHandle<()>> {
        if name == self.0 {
            return Err(std::io::Error::other("no more threads"));
        }
        SystemThreads.spawn(name, task)
    }
}

#[test]
fn a_thread_that_cannot_start_hands_back_the_link_and_the_virtual_controller() {
    for refused in ["heka-feedback", "heka-bridge"] {
        let (link, steps, written) = FakeLink::live();
        let (controller, record, game) = FakeController::with_feedback();

        let failed = Bridge::resume_on(
            &Refusing(refused),
            OpenLink::new(link),
            Plugged::new(controller),
            Arc::default(),
        )
        .err()
        .expect(refused);

        assert_eq!(failed.error.to_string(), "no more threads");
        assert!(!record.lock().unwrap().unplugged);
        let bridge = Bridge::resume(failed.link, failed.controller, Arc::default()).unwrap();
        steps.send(Step::Report(cross())).unwrap();
        wait_until(|| record.lock().unwrap().updates.len() == 1);
        game.send(rumble(10, 20)).unwrap();
        wait_until(|| reports(&written).contains(&output_report(rumble(10, 20))));
        bridge.stop();
    }
}

#[test]
fn a_rainbow_does_not_hurry_the_retry_of_a_failing_write() {
    let Following {
        bridge,
        written,
        game: _game,
        _steps,
        ..
    } = following(Preferences {
        lightbar: Some(LightbarChoice::RAINBOW),
        ..Preferences::default()
    });
    wait_until(|| !reports(&written).is_empty());

    fail_writes(&written, usize::MAX);
    let before = started(&written);
    let failing_since = Instant::now();
    wait_until(|| started(&written) >= before + 5);

    assert!(failing_since.elapsed() >= Duration::from_millis(300));
    bridge.stop();
}

#[test]
fn a_virtual_controller_that_gives_no_feedback_is_reported_once() {
    let (mut controller, _) = FakeController::new();
    controller.refuse_feedback = true;
    let (controller, failures) = reporting(controller);

    drop(controller);

    let no_feedback = VirtualFailure::NoFeedback("no notifications".into());
    assert_eq!(failures.take(), [no_feedback]);
}

const M: Key = Key::new(0x32);
const Q: Key = Key::new(0x10);

fn down(key: Key) -> DesktopEvent {
    DesktopEvent::Key { key, down: true }
}

fn up(key: Key) -> DesktopEvent {
    DesktopEvent::Key { key, down: false }
}

#[derive(Clone, Default)]
struct FakeDesktop(Arc<Mutex<Vec<DesktopEvent>>>);

impl FakeDesktop {
    fn sent(&self) -> Vec<DesktopEvent> {
        self.0.lock().unwrap().clone()
    }
}

impl DesktopOutput for FakeDesktop {
    fn send(&mut self, events: &[DesktopEvent]) -> Result<(), DesktopError> {
        self.0.lock().unwrap().extend_from_slice(events);
        Ok(())
    }
}

fn cross_types(key: Key) -> Mapper {
    let mut profile = Profile::named("Desk");
    profile.bind(Input::Cross, Binding::Keys(Chord::of(key)));
    Mapper::new(&profile)
}

struct Typing {
    bridge: Bridge,
    steps: Sender<Step>,
    desktop: FakeDesktop,
    mapper: Arc<LiveMapper>,
    record: Arc<Mutex<Record>>,
}

fn typing() -> Typing {
    let (link, steps, _) = FakeLink::live();
    let (controller, record) = FakeController::new();
    let desktop = FakeDesktop::default();
    let mapper = Arc::new(LiveMapper::default());
    mapper.set(cross_types(M));
    let mut plugged = Plugged::new(controller);
    plugged.use_desk(Arc::new(Desk::new(Box::new(desktop.clone()))));
    plugged.map_with(Arc::clone(&mapper));
    let bridge = Bridge::resume(OpenLink::new(link), plugged, Arc::default()).unwrap();
    Typing {
        bridge,
        steps,
        desktop,
        mapper,
        record,
    }
}

#[test]
fn a_report_presses_and_releases_the_key_a_button_is_bound_to() {
    let typing = typing();

    typing.steps.send(Step::Report(cross())).unwrap();
    wait_until(|| typing.desktop.sent() == [down(M)]);
    typing
        .steps
        .send(Step::Report(common::captured_baseline()))
        .unwrap();
    wait_until(|| typing.desktop.sent() == [down(M), up(M)]);

    typing.bridge.stop();
    let record = typing.record.lock().unwrap();
    assert!(record.updates.iter().all(|s| s.buttons == Buttons::NONE));
}

#[test]
fn stopping_a_bridge_releases_its_keys_before_it_returns() {
    let typing = typing();
    typing.steps.send(Step::Report(cross())).unwrap();
    wait_until(|| typing.desktop.sent() == [down(M)]);

    let ended = typing.bridge.stop();

    assert_eq!(typing.desktop.sent(), [down(M), up(M)]);
    drop(ended);
}

#[test]
fn losing_the_link_releases_the_keys() {
    let typing = typing();
    typing.steps.send(Step::Report(cross())).unwrap();
    typing.steps.send(Step::Lost).unwrap();

    wait_until(|| typing.bridge.is_finished());

    assert_eq!(typing.desktop.sent(), [down(M), up(M)]);
}

#[test]
fn dropping_a_bridge_releases_its_keys() {
    let typing = typing();
    typing.steps.send(Step::Report(cross())).unwrap();
    wait_until(|| typing.desktop.sent() == [down(M)]);

    drop(typing.bridge);

    assert_eq!(typing.desktop.sent(), [down(M), up(M)]);
}

#[test]
fn a_failing_virtual_controller_leaves_no_key_held() {
    let (link, steps, _) = FakeLink::live();
    let (mut controller, _) = FakeController::new();
    controller.fail_updates = true;
    let desktop = FakeDesktop::default();
    let mapper = Arc::new(LiveMapper::default());
    mapper.set(cross_types(M));
    let mut plugged = Plugged::new(controller);
    plugged.use_desk(Arc::new(Desk::new(Box::new(desktop.clone()))));
    plugged.map_with(mapper);
    let bridge = Bridge::resume(OpenLink::new(link), plugged, Arc::default()).unwrap();

    steps.send(Step::Report(cross())).unwrap();
    wait_until(|| bridge.is_finished());

    assert_eq!(desktop.sent(), []);
}

#[test]
fn a_changed_binding_releases_the_old_key_even_before_another_report() {
    let typing = typing();
    typing.steps.send(Step::Report(cross())).unwrap();
    wait_until(|| typing.desktop.sent() == [down(M)]);

    typing.mapper.set(Mapper::default());

    wait_until(|| typing.desktop.sent() == [down(M), up(M)]);
    typing.bridge.stop();
}

#[test]
fn a_changed_profile_presses_what_a_held_button_is_bound_to_now() {
    let typing = typing();
    typing.steps.send(Step::Report(cross())).unwrap();
    wait_until(|| typing.desktop.sent() == [down(M)]);

    typing.mapper.set(cross_types(Q));
    typing.steps.send(Step::Report(cross())).unwrap();

    wait_until(|| typing.desktop.sent() == [down(M), up(M), down(Q)]);
    typing.bridge.stop();
}

#[test]
fn setting_the_same_mapping_again_keeps_the_keys_held() {
    let typing = typing();
    typing.steps.send(Step::Report(cross())).unwrap();
    wait_until(|| typing.desktop.sent() == [down(M)]);

    typing.mapper.set(cross_types(M));
    typing.steps.send(Step::Report(cross())).unwrap();
    wait_until(|| typing.record.lock().unwrap().updates.len() == 2);

    assert_eq!(typing.desktop.sent(), [down(M)]);
    typing.bridge.stop();
}

#[test]
fn a_binding_changed_to_the_wheel_turns_it_only_for_a_new_press() {
    let typing = typing();
    typing.steps.send(Step::Report(cross())).unwrap();
    wait_until(|| typing.desktop.sent() == [down(M)]);
    let mut profile = Profile::named("Desk");
    profile.bind(Input::Cross, Binding::Wheel(WheelStep::Up));

    typing.mapper.set(Mapper::new(&profile));
    typing.steps.send(Step::Report(cross())).unwrap();
    wait_until(|| typing.record.lock().unwrap().updates.len() == 2);
    typing
        .steps
        .send(Step::Report(common::captured_baseline()))
        .unwrap();
    typing.steps.send(Step::Report(cross())).unwrap();
    wait_until(|| typing.record.lock().unwrap().updates.len() == 4);
    typing.bridge.stop();

    assert_eq!(
        typing.desktop.sent(),
        [down(M), up(M), DesktopEvent::Wheel(WheelStep::Up)]
    );
}

#[test]
fn a_bridge_dropped_by_a_panicking_thread_still_releases_its_keys() {
    let typing = typing();
    typing.steps.send(Step::Report(cross())).unwrap();
    wait_until(|| typing.desktop.sent() == [down(M)]);
    let Typing {
        bridge,
        desktop,
        steps: _steps,
        ..
    } = typing;

    let panicked = std::thread::spawn(move || {
        let _bridge = bridge;
        panic!("the supervisor failed");
    })
    .join();

    assert!(panicked.is_err());
    wait_until(|| desktop.sent() == [down(M), up(M)]);
}

#[test]
fn a_turbo_button_goes_on_repeating_while_no_report_arrives() {
    let (link, steps, _) = FakeLink::live();
    let (controller, record) = FakeController::new();
    let mut profile = Profile::named("Turbo");
    profile.bind(Input::Cross, Binding::To(Output::Circle));
    profile.set_mode(Input::Cross, Mode::Turbo(30));
    let mapper = Arc::new(LiveMapper::default());
    mapper.set(Mapper::new(&profile));
    let mut plugged = Plugged::new(controller);
    plugged.map_with(mapper);
    let bridge = Bridge::resume(OpenLink::new(link), plugged, Arc::default()).unwrap();

    steps.send(Step::Report(cross())).unwrap();

    wait_until(|| record.lock().unwrap().updates.len() >= 5);
    bridge.stop();
    let record = record.lock().unwrap();
    let shown: Vec<Buttons> = record.updates[..4].iter().map(|s| s.buttons).collect();
    assert_eq!(
        shown,
        [
            Buttons::CIRCLE,
            Buttons::NONE,
            Buttons::CIRCLE,
            Buttons::NONE
        ]
    );
}

const MUTE_BUTTON: (usize, u8) = (10, 0x04);

fn charge(status: u8) -> Step {
    Step::Report(common::derived(&[(53, status)]))
}

fn lightbar(lightbar: Lightbar) -> Feedback {
    Feedback {
        lightbar: Some(lightbar),
        ..Feedback::OFF
    }
}

fn mute_light(lit: bool) -> Feedback {
    Feedback {
        mute_light: Some(lit),
        ..Feedback::OFF
    }
}

fn last_report(written: &Arc<Mutex<Written>>) -> Option<Vec<u8>> {
    written.lock().unwrap().reports.last().cloned()
}

fn wait_for(written: &Arc<Mutex<Written>>, feedback: Feedback) {
    let expected = output_report(feedback);
    wait_until(|| last_report(written).as_ref() == Some(&expected));
}

#[test]
fn the_battery_colour_follows_the_reports_and_gives_way_to_the_game() {
    let Following {
        bridge,
        written,
        game,
        _steps: steps,
        ..
    } = following(Preferences {
        battery_colour: true,
        ..Preferences::default()
    });

    steps.send(charge(0x01)).unwrap();
    wait_for(&written, lightbar(Lightbar::of_battery(15)));
    steps.send(charge(0x07)).unwrap();
    wait_for(&written, lightbar(Lightbar::of_battery(75)));

    game.send(lightbar(PINK)).unwrap();
    wait_for(&written, lightbar(PINK));
    steps.send(charge(0x02)).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(last_report(&written), Some(output_report(lightbar(PINK))));
    bridge.stop();
}

#[test]
fn a_low_battery_flashes_over_the_games_colour_until_it_charges() {
    let Following {
        bridge,
        written,
        game,
        _steps: steps,
        ..
    } = following(Preferences {
        low_battery_flash: true,
        ..Preferences::default()
    });
    game.send(lightbar(PINK)).unwrap();
    wait_for(&written, lightbar(PINK));

    steps.send(charge(0x00)).unwrap();
    wait_for(&written, lightbar(Lightbar::DARK));
    wait_for(&written, lightbar(PINK));
    wait_for(&written, lightbar(Lightbar::DARK));

    steps.send(charge(0x10)).unwrap();
    wait_for(&written, lightbar(PINK));
    std::thread::sleep(Duration::from_millis(1100));
    assert_eq!(last_report(&written), Some(output_report(lightbar(PINK))));
    bridge.stop();
}

#[test]
fn a_flash_that_ends_with_no_colour_to_show_goes_back_to_the_default() {
    let Following {
        bridge,
        written,
        game: _game,
        _steps: steps,
        ..
    } = following(Preferences {
        low_battery_flash: true,
        ..Preferences::default()
    });
    steps.send(charge(0x01)).unwrap();
    wait_for(&written, lightbar(Lightbar::DARK));

    steps.send(charge(0x11)).unwrap();
    wait_for(&written, lightbar(Lightbar::DEFAULT));
    bridge.stop();
}

#[test]
fn the_mute_button_toggles_the_mute_light() {
    let Following {
        bridge,
        written,
        game: _game,
        _steps: steps,
        ..
    } = following(Preferences {
        mute_light: Some(MuteLight::Button),
        ..Preferences::default()
    });
    wait_for(&written, mute_light(false));
    let press = || Step::Report(common::derived(&[MUTE_BUTTON]));
    let release = || Step::Report(common::derived(&[]));

    steps.send(release()).unwrap();
    steps.send(press()).unwrap();
    wait_for(&written, mute_light(true));
    steps.send(press()).unwrap();
    steps.send(release()).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(last_report(&written), Some(output_report(mute_light(true))));

    steps.send(press()).unwrap();
    wait_for(&written, mute_light(false));
    bridge.stop();
}

#[test]
fn a_mute_button_already_held_when_the_bridge_starts_toggles_nothing() {
    let Following {
        bridge,
        written,
        game: _game,
        _steps: steps,
        ..
    } = following(Preferences {
        mute_light: Some(MuteLight::Button),
        ..Preferences::default()
    });
    wait_for(&written, mute_light(false));

    steps
        .send(Step::Report(common::derived(&[MUTE_BUTTON])))
        .unwrap();
    std::thread::sleep(Duration::from_millis(150));

    assert_eq!(reports(&written).len(), 1);
    bridge.stop();
}

#[test]
fn a_toggled_mute_light_stays_lit_on_another_link() {
    let Following {
        bridge,
        written,
        game: _game,
        _steps: steps,
        ..
    } = following(Preferences {
        mute_light: Some(MuteLight::Button),
        ..Preferences::default()
    });
    steps.send(Step::Report(common::derived(&[]))).unwrap();
    steps
        .send(Step::Report(common::derived(&[MUTE_BUTTON])))
        .unwrap();
    wait_for(&written, mute_light(true));
    let BridgeEnd::Stopped { controller, .. } = bridge.stop() else {
        panic!("expected Stopped");
    };

    let (other, _steps, on_other) = FakeLink::live();
    let bridge = Bridge::resume(OpenLink::new(other), controller, Arc::default()).unwrap();
    wait_for(&on_other, mute_light(true));
    bridge.stop();
}

#[test]
fn choosing_the_mute_button_starts_with_the_light_off() {
    let Following {
        bridge,
        preferences,
        written,
        game: _game,
        _steps: steps,
    } = following(Preferences::default());
    steps.send(Step::Report(common::derived(&[]))).unwrap();
    steps
        .send(Step::Report(common::derived(&[MUTE_BUTTON])))
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));

    preferences.set(Preferences {
        mute_light: Some(MuteLight::Button),
        ..Preferences::default()
    });
    wait_for(&written, mute_light(false));
    std::thread::sleep(Duration::from_millis(150));

    assert_eq!(reports(&written), [output_report(mute_light(false))]);
    bridge.stop();
}

#[test]
fn a_mute_light_no_longer_chosen_is_put_out() {
    let Following {
        bridge,
        preferences,
        written,
        game: _game,
        _steps,
    } = following(Preferences {
        mute_light: Some(MuteLight::On),
        ..Preferences::default()
    });
    wait_for(&written, mute_light(true));

    preferences.set(Preferences::default());
    wait_for(&written, mute_light(false));
    bridge.stop();
}

#[test]
fn stopping_the_bridge_puts_the_lights_back_as_they_were() {
    let Following {
        bridge,
        written,
        game: _game,
        _steps,
        ..
    } = following(Preferences {
        lightbar: Some(LightbarChoice::Colour(PINK)),
        player_lights: Some(PlayerLights::One),
        mute_light: Some(MuteLight::On),
        ..Preferences::default()
    });
    wait_until(|| reports(&written).len() == 1);

    bridge.stop();

    assert_eq!(
        last_report(&written),
        Some(output_report(Feedback {
            lightbar: Some(Lightbar::DEFAULT),
            player_lights: Some(PlayerLights::Off),
            mute_light: Some(false),
            ..Feedback::OFF
        }))
    );
}

fn resisting() -> Mapper {
    let mut profile = Profile::named("Heavy");
    profile.change_trigger_effect(
        TriggerSide::Left,
        EffectChange::Kind(EffectKind::Resistance),
    );
    Mapper::new(&profile)
}

fn effects(left: Option<TriggerEffect>) -> Feedback {
    Feedback {
        trigger_effects: TriggerEffects { left, right: None },
        ..Feedback::OFF
    }
}

const RESISTANCE: TriggerEffect = TriggerEffect::Resistance {
    start: 30,
    strength: 60,
};

struct Resisting {
    bridge: Bridge,
    mapper: Arc<LiveMapper>,
    written: Arc<Mutex<Written>>,
    game: Sender<Feedback>,
    _steps: Sender<Step>,
}

fn resisting_bridge() -> Resisting {
    let (link, steps, written) = FakeLink::live();
    let (controller, _, game) = FakeController::with_feedback();
    let mapper = Arc::new(LiveMapper::default());
    mapper.set(resisting());
    let mut controller = Plugged::new(controller);
    controller.map_with(Arc::clone(&mapper));
    Resisting {
        bridge: Bridge::resume(OpenLink::new(link), controller, Arc::default()).unwrap(),
        mapper,
        written,
        game,
        _steps: steps,
    }
}

#[test]
fn a_profiles_trigger_effect_is_sent_as_the_bridge_starts_and_stays_with_rumble() {
    let Resisting {
        bridge,
        mapper: _mapper,
        written,
        game,
        _steps,
    } = resisting_bridge();
    wait_for(&written, effects(Some(RESISTANCE)));

    game.send(rumble(200, 64)).unwrap();
    wait_for(
        &written,
        Feedback {
            rumble: rumble(200, 64).rumble,
            ..effects(Some(RESISTANCE))
        },
    );
    bridge.stop();
}

#[test]
fn changing_to_a_profile_without_the_effect_turns_it_off() {
    let Resisting {
        bridge,
        mapper,
        written,
        game: _game,
        _steps,
    } = resisting_bridge();
    wait_for(&written, effects(Some(RESISTANCE)));

    mapper.set(Mapper::default());
    wait_for(&written, effects(Some(TriggerEffect::Off)));
    bridge.stop();
}

#[test]
fn a_trigger_effect_is_turned_off_with_its_bridge_and_sent_again_on_another_link() {
    let Resisting {
        bridge,
        mapper: _mapper,
        written,
        game: _game,
        _steps,
    } = resisting_bridge();
    wait_for(&written, effects(Some(RESISTANCE)));

    let BridgeEnd::Stopped { controller, .. } = bridge.stop() else {
        panic!("expected Stopped");
    };
    assert_eq!(
        last_report(&written),
        Some(output_report(effects(Some(TriggerEffect::Off))))
    );

    let (other, _steps, on_other) = FakeLink::live();
    let bridge = Bridge::resume(OpenLink::new(other), controller, Arc::default()).unwrap();
    wait_for(&on_other, effects(Some(RESISTANCE)));
    bridge.stop();
}

#[test]
fn chosen_lights_are_followed_after_the_virtual_controller_stops_sending_feedback() {
    let Following {
        bridge,
        preferences,
        written,
        game,
        _steps,
    } = following(Preferences::default());
    drop(game);

    preferences.set(Preferences {
        mute_light: Some(MuteLight::On),
        ..Preferences::default()
    });
    wait_for(&written, mute_light(true));
    bridge.stop();
}
