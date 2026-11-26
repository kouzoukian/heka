//! Link reads in, Virtual Controller updates and Bridge endings out.

mod common;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use heka_core::bridge::{Bridge, BridgeEnd, LiveInput};
use heka_core::link::{LinkRead, LinkReader, ReportFormat};
use heka_core::state::{Buttons, ControllerState};
use heka_core::virtual_controller::{OutputType, VirtualController, VirtualError};

/// One scripted read. After the script runs out the Link stays up and silent.
enum Step {
    Report(Vec<u8>),
    Lost,
}

struct FakeLink {
    script: VecDeque<Step>,
}

impl FakeLink {
    fn new(script: impl IntoIterator<Item = Step>) -> Box<Self> {
        Box::new(Self {
            script: script.into_iter().collect(),
        })
    }
}

impl LinkReader for FakeLink {
    fn format(&self) -> ReportFormat {
        ReportFormat::DualSenseUsb
    }

    fn read(&mut self, buf: &mut [u8]) -> LinkRead {
        match self.script.pop_front() {
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
}

#[derive(Default)]
struct Record {
    updates: Vec<ControllerState>,
    unplugged: bool,
}

struct FakeController {
    record: Arc<Mutex<Record>>,
    fail_updates: bool,
}

impl FakeController {
    fn new() -> (Box<Self>, Arc<Mutex<Record>>) {
        let record = Arc::new(Mutex::new(Record::default()));
        let controller = Box::new(Self {
            record: Arc::clone(&record),
            fail_updates: false,
        });
        (controller, record)
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
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn cross() -> Vec<u8> {
    // DERIVED: captured baseline with Cross pressed (buttons[0] bit 5).
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
fn stopping_removes_the_virtual_controller() {
    let (controller, record) = FakeController::new();
    let bridge = Bridge::start(FakeLink::new([]), controller, Arc::default()).unwrap();

    assert!(matches!(bridge.stop(), BridgeEnd::Stopped));
    assert!(record.lock().unwrap().unplugged);
}

#[test]
fn losing_the_link_releases_everything_then_removes_the_virtual_controller() {
    let (controller, record) = FakeController::new();
    let live = Arc::new(LiveInput::default());
    let bridge = Bridge::start(
        FakeLink::new([Step::Report(cross()), Step::Lost]),
        controller,
        Arc::clone(&live),
    )
    .unwrap();

    wait_until(|| bridge.is_finished());
    assert!(matches!(bridge.stop(), BridgeEnd::LinkLost));
    let record = record.lock().unwrap();
    assert_eq!(record.updates.last(), Some(&ControllerState::NEUTRAL));
    assert!(record.unplugged);
    assert_eq!(live.latest(), None);
}

#[test]
fn reports_of_another_kind_are_skipped() {
    let (controller, record) = FakeController::new();
    let bridge = Bridge::start(
        FakeLink::new([
            // DERIVED: a report with another ID, then a truncated report.
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
fn a_failing_virtual_controller_ends_the_bridge_without_panicking() {
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
        BridgeEnd::VirtualControllerFailed(_)
    ));
    assert!(record.lock().unwrap().unplugged);
}
