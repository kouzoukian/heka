mod common;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use heka_core::calibration::Calibration;
use heka_core::dualsense::{RumbleMode, usb_output_report};
use heka_core::dualshock4::{parse_bluetooth_serial, parse_pairing_info};
use heka_core::feedback::{Feedback, FeedbackEvent, FeedbackSource, Lightbar, Motor, Rumble};
use heka_core::idle::IdleTime;
use heka_core::lifecycle::{BRIDGE_RETRY_DELAY, Change, HOLD_TIME, Phase, Removal};
use heka_core::link::{
    ControllerModel, FeedbackFormat, LinkKind, LinkRead, LinkReader, LinkWriteError, LinkWriter,
    ReportFormat,
};
use heka_core::mac::MacAddress;
use heka_core::preferences::{LightbarChoice, PreferenceChange, Preferences, RumbleStrength};
use heka_core::radio::{Radio, TurnOffError};
use heka_core::settings::Settings;
use heka_core::state::{Buttons, ControllerState};
use heka_core::supervisor::{BridgeError, Supervisor};
use heka_core::threads::{SystemThreads, Task, Threads};
use heka_core::virtual_controller::{
    OutputType, VirtualBus, VirtualController, VirtualError, VirtualFailure,
};

#[derive(Default)]
struct Script {
    reports: VecDeque<Vec<u8>>,
    lost: bool,
    written: Vec<Vec<u8>>,
}

#[derive(Clone)]
struct LinkHandle(Arc<Mutex<Script>>, ReportFormat);

impl Default for LinkHandle {
    fn default() -> Self {
        Self::delivering(ReportFormat::DualSenseUsb(Calibration::UNCALIBRATED))
    }
}

impl LinkHandle {
    fn delivering(format: ReportFormat) -> Self {
        Self(Arc::default(), format)
    }

    fn bluetooth() -> Self {
        Self::delivering(ReportFormat::DualSenseBluetooth(Calibration::UNCALIBRATED))
    }

    fn send_input(&self, usb: Vec<u8>) {
        match self.1 {
            ReportFormat::DualSenseBluetooth(_) | ReportFormat::DualSenseEdgeBluetooth(_) => {
                self.send(common::bluetooth_from_usb(&usb))
            }
            ReportFormat::DualShock4Bluetooth(_) => self.send(common::ds4_bluetooth_from_usb(&usb)),
            ReportFormat::DualSenseUsb(_)
            | ReportFormat::DualSenseEdgeUsb(_)
            | ReportFormat::DualShock4Usb(_) => self.send(usb),
        }
    }

    fn reader(&self) -> Box<dyn LinkReader> {
        Box::new(FakeLink(self.clone()))
    }

    fn send(&self, report: Vec<u8>) {
        self.0.lock().unwrap().reports.push_back(report);
    }

    fn pull_out(&self) {
        self.0.lock().unwrap().lost = true;
    }
}

struct FakeLink(LinkHandle);

impl LinkReader for FakeLink {
    fn format(&self) -> ReportFormat {
        (self.0).1
    }

    fn read(&mut self, buf: &mut [u8]) -> LinkRead {
        let next = {
            let mut script = (self.0).0.lock().unwrap();
            if script.lost {
                return LinkRead::Lost;
            }
            script.reports.pop_front()
        };
        match next {
            Some(bytes) => {
                buf[..bytes.len()].copy_from_slice(&bytes);
                LinkRead::Report(bytes.len())
            }
            None => {
                std::thread::sleep(Duration::from_millis(1));
                LinkRead::Timeout
            }
        }
    }

    fn feedback_writer(&mut self) -> Option<Box<dyn LinkWriter>> {
        Some(Box::new(FakeWriter(self.0.clone())))
    }
}

struct FakeWriter(LinkHandle);

impl LinkWriter for FakeWriter {
    fn format(&self) -> FeedbackFormat {
        FeedbackFormat::DualSenseUsb(RumbleMode::CompatibleV2)
    }

    fn write(&mut self, report: &[u8]) -> Result<(), LinkWriteError> {
        (self.0).0.lock().unwrap().written.push(report.to_vec());
        Ok(())
    }
}

struct QuietGame;

impl FeedbackSource for QuietGame {
    fn next(&mut self, timeout: Duration) -> FeedbackEvent {
        std::thread::sleep(timeout);
        FeedbackEvent::Timeout
    }
}

#[derive(Default)]
struct Shown {
    output_type: OutputType,
    updates: Vec<ControllerState>,
    unplugged: bool,
    refuse_updates: bool,
    refuse_unplug: bool,
}

#[derive(Default)]
struct BusRecord {
    plugged: Vec<Arc<Mutex<Shown>>>,
    refuse_plugs: usize,
}

#[derive(Clone, Default)]
struct FakeBus(Arc<Mutex<BusRecord>>);

impl FakeBus {
    fn plugged(&self) -> usize {
        self.0.lock().unwrap().plugged.len()
    }

    fn shown(&self, index: usize) -> Arc<Mutex<Shown>> {
        Arc::clone(&self.0.lock().unwrap().plugged[index])
    }

    fn last_state(&self, index: usize) -> Option<ControllerState> {
        self.shown(index).lock().unwrap().updates.last().copied()
    }

    fn unplugged(&self, index: usize) -> bool {
        self.shown(index).lock().unwrap().unplugged
    }

    fn refuse_next_plugs(&self, count: usize) {
        self.0.lock().unwrap().refuse_plugs = count;
    }
}

impl VirtualBus for FakeBus {
    fn plug(&self, output_type: OutputType) -> Result<Box<dyn VirtualController>, VirtualError> {
        let mut record = self.0.lock().unwrap();
        if record.refuse_plugs > 0 {
            record.refuse_plugs -= 1;
            return Err(VirtualError::new("no free slot"));
        }
        let shown = Arc::new(Mutex::new(Shown {
            output_type,
            ..Shown::default()
        }));
        record.plugged.push(Arc::clone(&shown));
        Ok(Box::new(FakeController(shown)))
    }
}

struct FakeController(Arc<Mutex<Shown>>);

impl VirtualController for FakeController {
    fn output_type(&self) -> OutputType {
        self.0.lock().unwrap().output_type
    }

    fn update(&mut self, state: &ControllerState) -> Result<(), VirtualError> {
        let mut shown = self.0.lock().unwrap();
        if shown.refuse_updates {
            return Err(VirtualError::new("the bus went away"));
        }
        shown.updates.push(*state);
        Ok(())
    }

    fn unplug(self: Box<Self>) -> Result<(), VirtualError> {
        let mut shown = self.0.lock().unwrap();
        if shown.refuse_unplug {
            return Err(VirtualError::new("it will not leave"));
        }
        shown.unplugged = true;
        Ok(())
    }

    fn feedback(&mut self) -> Result<Option<Box<dyn FeedbackSource>>, VirtualError> {
        Ok(Some(Box::new(QuietGame)))
    }
}

fn no_free_slot() -> BridgeError {
    BridgeError::VirtualController("no free slot".into())
}

fn bus_gone() -> BridgeError {
    BridgeError::VirtualController("the bus went away".into())
}

fn mac(n: u8) -> MacAddress {
    MacAddress([0xA0, 0, 0, 0, 0, n])
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn poll_until_changed(supervisor: &mut Supervisor, now: Instant) -> Vec<Change> {
    let mut changes = Vec::new();
    wait_until(|| {
        changes = supervisor.poll(now);
        !changes.is_empty()
    });
    changes
}

fn cross() -> Vec<u8> {
    common::derived(&[(8, 0x28)])
}

fn setup() -> (Supervisor, FakeBus) {
    let bus = FakeBus::default();
    (
        Supervisor::new(Box::new(bus.clone()), Settings::default()),
        bus,
    )
}

#[test]
fn an_arriving_controller_is_bridged_to_a_new_virtual_controller() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let link = LinkHandle::default();

    let (_, changes) = supervisor.link_arrived(mac(1), link.reader(), t0);
    assert_eq!(
        changes,
        [Change::Bridged {
            mac: mac(1),
            output_type: OutputType::Xbox
        }]
    );
    assert_eq!(bus.plugged(), 1);

    link.send(cross());
    wait_until(|| bus.last_state(0).is_some());
    assert_eq!(bus.last_state(0).unwrap().buttons, Buttons::CROSS);

    let views = supervisor.controllers();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].mac, mac(1));
    assert_eq!(views[0].phase, Phase::Bridged);
    assert_eq!(
        views[0].live.latest().map(|s| s.buttons),
        Some(Buttons::CROSS)
    );
}

#[test]
fn four_controllers_bridge_independently_and_a_fifth_waits() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let links: Vec<LinkHandle> = (0..5).map(|_| LinkHandle::default()).collect();
    for (n, link) in (1..).zip(&links) {
        supervisor.link_arrived(mac(n), link.reader(), t0);
    }
    assert_eq!(bus.plugged(), 4);
    assert_eq!(supervisor.controllers()[4].phase, Phase::Waiting);

    links[1].send(cross());
    wait_until(|| bus.last_state(1).is_some());
    for other in [0, 2, 3] {
        assert_eq!(bus.last_state(other), None);
    }
}

#[test]
fn while_held_the_virtual_controller_shows_nothing_pressed_and_a_return_resumes_it() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);
    link.send(cross());
    wait_until(|| bus.last_state(0).is_some());

    link.pull_out();
    assert_eq!(
        poll_until_changed(&mut supervisor, t0),
        [Change::Held { mac: mac(1) }]
    );
    assert_eq!(bus.last_state(0), Some(ControllerState::NEUTRAL));
    assert!(!bus.unplugged(0));

    let back = LinkHandle::default();
    let (_, changes) = supervisor.link_arrived(mac(1), back.reader(), t0 + Duration::from_secs(10));
    assert_eq!(changes, [Change::Returned { mac: mac(1) }]);
    back.send(cross());
    wait_until(|| bus.last_state(0).map(|s| s.buttons) == Some(Buttons::CROSS));
    assert_eq!(bus.plugged(), 1, "the same Virtual Controller resumes");
}

#[test]
fn a_hold_that_expires_removes_the_virtual_controller() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);
    link.pull_out();
    poll_until_changed(&mut supervisor, t0);

    assert_eq!(
        supervisor.poll(t0 + HOLD_TIME - Duration::from_millis(1)),
        []
    );
    assert!(!bus.unplugged(0));
    assert_eq!(
        supervisor.poll(t0 + HOLD_TIME),
        [Change::Removed {
            mac: mac(1),
            why: Removal::Expired
        }]
    );
    assert!(bus.unplugged(0));
    assert!(supervisor.controllers().is_empty());
}

#[test]
fn a_link_reported_gone_before_its_bridge_notices_is_held_too() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let link = LinkHandle::default();
    let (id, _) = supervisor.link_arrived(mac(1), link.reader(), t0);
    link.send(cross());
    wait_until(|| bus.last_state(0).is_some());

    assert_eq!(supervisor.link_gone(id, t0), [Change::Held { mac: mac(1) }]);
    assert_eq!(bus.last_state(0), Some(ControllerState::NEUTRAL));
    assert!(!bus.unplugged(0));
    assert_eq!(supervisor.link_gone(id, t0), []);
}

#[test]
fn a_stale_link_leaving_does_not_disturb_the_controllers_new_link() {
    let t0 = Instant::now();
    let (mut supervisor, _bus) = setup();
    let first = LinkHandle::default();
    let (first_id, _) = supervisor.link_arrived(mac(1), first.reader(), t0);
    first.pull_out();
    poll_until_changed(&mut supervisor, t0);
    supervisor.link_arrived(mac(1), LinkHandle::default().reader(), t0);

    assert_eq!(supervisor.link_gone(first_id, t0), []);
    assert_eq!(supervisor.controllers()[0].phase, Phase::Bridged);
}

#[test]
fn stopping_removes_the_virtual_controller_and_starting_makes_a_new_one() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);

    supervisor.stop(mac(1), t0);
    assert!(bus.unplugged(0));
    assert_eq!(supervisor.controllers()[0].phase, Phase::Stopped);
    assert!(supervisor.settings().controller(mac(1)).stopped);

    supervisor.start(mac(1), t0);
    assert_eq!(bus.plugged(), 2);
    link.send(cross());
    wait_until(|| bus.last_state(1).is_some());
}

#[test]
fn a_virtual_controller_that_cannot_be_created_is_tried_again_later() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    bus.refuse_next_plugs(1);

    let (_, changes) = supervisor.link_arrived(mac(1), LinkHandle::default().reader(), t0);
    assert_eq!(
        changes,
        [
            Change::Bridged {
                mac: mac(1),
                output_type: OutputType::Xbox
            },
            Change::Removed {
                mac: mac(1),
                why: Removal::Failed
            },
        ]
    );
    let view = &supervisor.controllers()[0];
    assert_eq!(view.phase, Phase::Waiting);
    assert_eq!(view.error, Some(no_free_slot()));

    supervisor.poll(t0 + BRIDGE_RETRY_DELAY);
    assert_eq!(bus.plugged(), 1);
    assert_eq!(supervisor.controllers()[0].phase, Phase::Bridged);
    assert_eq!(supervisor.controllers()[0].error, None);
}

#[test]
fn quitting_removes_every_virtual_controller() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let held = LinkHandle::default();
    supervisor.link_arrived(mac(1), LinkHandle::default().reader(), t0);
    supervisor.link_arrived(mac(2), held.reader(), t0);
    held.pull_out();
    poll_until_changed(&mut supervisor, t0);

    let changes = supervisor.quit(t0);
    assert_eq!(changes.len(), 2);
    assert!(bus.unplugged(0) && bus.unplugged(1));
    assert!(supervisor.controllers().is_empty());
}

#[test]
fn changing_the_output_type_bridges_the_same_link_to_a_new_virtual_controller() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);

    let changes = supervisor.set_output_type(mac(1), OutputType::Ps4, t0);
    assert_eq!(
        changes,
        [
            Change::Removed {
                mac: mac(1),
                why: Removal::OutputTypeChanged
            },
            Change::Bridged {
                mac: mac(1),
                output_type: OutputType::Ps4
            },
        ]
    );
    assert!(bus.unplugged(0));
    assert_eq!(bus.plugged(), 2);
    assert_eq!(bus.shown(1).lock().unwrap().output_type, OutputType::Ps4);
    assert_eq!(supervisor.controllers()[0].output_type, OutputType::Ps4);
    assert_eq!(
        supervisor.settings().controller(mac(1)).output_type,
        OutputType::Ps4
    );

    link.send(cross());
    wait_until(|| bus.last_state(1).is_some());
    assert_eq!(bus.last_state(1).unwrap().buttons, Buttons::CROSS);
}

#[test]
fn heka_never_bridges_its_own_ps4_virtual_controller() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let ours = MacAddress([0xC0, 0x13, 0x37, 0x12, 0x34, 0x56]);

    let (_, changes) = supervisor.link_arrived(ours, LinkHandle::default().reader(), t0);
    assert_eq!(changes, []);
    assert_eq!(bus.plugged(), 0);
    assert!(supervisor.controllers().is_empty());
}

#[test]
fn a_dualshock_4_on_both_links_is_one_controller_that_stays_a_dualshock_4_while_held() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let by_bluetooth = parse_bluetooth_serial("a4ae11020304").unwrap();
    let by_usb = parse_pairing_info(&[0x12, 0x04, 0x03, 0x02, 0x11, 0xAE, 0xA4]).unwrap();
    let bluetooth =
        LinkHandle::delivering(ReportFormat::DualShock4Bluetooth(Calibration::UNCALIBRATED));
    let usb = LinkHandle::delivering(ReportFormat::DualShock4Usb(Calibration::UNCALIBRATED));

    supervisor.link_arrived(by_bluetooth, bluetooth.reader(), t0);
    let model = |supervisor: &Supervisor| supervisor.controllers()[0].model;
    assert_eq!(model(&supervisor), ControllerModel::DualShock4);
    bluetooth.send_input(common::ds4_usb(&[(5, 0x28)]));
    wait_until(|| bus.last_state(0).map(|s| s.buttons) == Some(Buttons::CROSS));

    let (usb_id, changes) = supervisor.link_arrived(by_usb, usb.reader(), t0);
    assert_eq!(changes, []);
    assert_eq!(bus.plugged(), 1);
    assert_eq!(supervisor.controllers().len(), 1);
    assert_eq!(supervisor.controllers()[0].active_link, Some(LinkKind::Usb));
    usb.send_input(common::ds4_usb(&[(5, 0x18)]));
    wait_until(|| bus.last_state(0).map(|s| s.buttons) == Some(Buttons::SQUARE));

    supervisor.link_gone(usb_id, t0);
    bluetooth.pull_out();
    poll_until_changed(&mut supervisor, t0);
    assert!(matches!(
        supervisor.controllers()[0].phase,
        Phase::Held { .. }
    ));
    assert_eq!(model(&supervisor), ControllerModel::DualShock4);
}

fn square() -> Vec<u8> {
    common::derived(&[(8, 0x18)])
}

fn shows_input_from(bus: &FakeBus, index: usize, from: &LinkHandle) {
    from.send_input(cross());
    wait_until(|| bus.last_state(index).map(|s| s.buttons) == Some(Buttons::CROSS));
    from.send_input(square());
    wait_until(|| bus.last_state(index).map(|s| s.buttons) == Some(Buttons::SQUARE));
}

#[test]
fn a_controller_on_usb_and_bluetooth_is_listed_once_with_one_virtual_controller() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let usb = LinkHandle::default();
    let bluetooth = LinkHandle::bluetooth();

    supervisor.link_arrived(mac(1), usb.reader(), t0);
    let (_, changes) = supervisor.link_arrived(mac(1), bluetooth.reader(), t0);
    assert_eq!(changes, []);
    assert_eq!(bus.plugged(), 1);
    let views = supervisor.controllers();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].active_link, Some(LinkKind::Usb));
    shows_input_from(&bus, 0, &usb);
}

#[test]
fn plugging_in_usb_makes_it_the_active_link_on_the_same_virtual_controller() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let bluetooth = LinkHandle::bluetooth();
    supervisor.link_arrived(mac(1), bluetooth.reader(), t0);
    shows_input_from(&bus, 0, &bluetooth);
    assert_eq!(
        supervisor.controllers()[0].active_link,
        Some(LinkKind::Bluetooth)
    );

    let usb = LinkHandle::default();
    let (_, changes) = supervisor.link_arrived(mac(1), usb.reader(), t0);
    assert_eq!(changes, []);
    assert_eq!(supervisor.controllers()[0].active_link, Some(LinkKind::Usb));
    shows_input_from(&bus, 0, &usb);
    bluetooth.send_input(cross());
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(bus.last_state(0).unwrap().buttons, Buttons::SQUARE);
    assert_eq!(bus.plugged(), 1);
    assert!(!bus.unplugged(0));
}

#[test]
fn unplugging_usb_falls_back_to_bluetooth_without_a_hold() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let bluetooth = LinkHandle::bluetooth();
    let usb = LinkHandle::default();
    supervisor.link_arrived(mac(1), bluetooth.reader(), t0);
    supervisor.link_arrived(mac(1), usb.reader(), t0);

    usb.pull_out();
    wait_until(|| {
        assert_eq!(supervisor.poll(t0), []);
        supervisor.controllers()[0].active_link == Some(LinkKind::Bluetooth)
    });
    assert_eq!(supervisor.controllers()[0].phase, Phase::Bridged);
    shows_input_from(&bus, 0, &bluetooth);
    assert_eq!(bus.plugged(), 1);
    assert!(!bus.unplugged(0));

    bluetooth.pull_out();
    assert_eq!(
        poll_until_changed(&mut supervisor, t0),
        [Change::Held { mac: mac(1) }]
    );
}

#[test]
fn a_usb_link_reported_gone_falls_back_to_bluetooth_too() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let bluetooth = LinkHandle::bluetooth();
    let usb = LinkHandle::default();
    let (bluetooth_id, _) = supervisor.link_arrived(mac(1), bluetooth.reader(), t0);
    let (usb_id, _) = supervisor.link_arrived(mac(1), usb.reader(), t0);

    assert_eq!(supervisor.link_gone(usb_id, t0), []);
    assert_eq!(
        supervisor.controllers()[0].active_link,
        Some(LinkKind::Bluetooth)
    );
    shows_input_from(&bus, 0, &bluetooth);
    assert_eq!(bus.plugged(), 1);

    assert_eq!(
        supervisor.link_gone(bluetooth_id, t0),
        [Change::Held { mac: mac(1) }]
    );
    assert_eq!(bus.last_state(0), Some(ControllerState::NEUTRAL));
    assert_eq!(supervisor.controllers()[0].active_link, None);
}

#[test]
fn an_idle_bluetooth_link_leaving_changes_nothing() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let usb = LinkHandle::default();
    supervisor.link_arrived(mac(1), usb.reader(), t0);
    let (bluetooth_id, _) = supervisor.link_arrived(mac(1), LinkHandle::bluetooth().reader(), t0);

    assert_eq!(supervisor.link_gone(bluetooth_id, t0), []);
    shows_input_from(&bus, 0, &usb);
    usb.pull_out();
    assert_eq!(
        poll_until_changed(&mut supervisor, t0),
        [Change::Held { mac: mac(1) }]
    );
}

#[test]
fn a_held_controller_returning_on_both_links_reads_usb() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let first = LinkHandle::default();
    supervisor.link_arrived(mac(1), first.reader(), t0);
    first.pull_out();
    poll_until_changed(&mut supervisor, t0);

    let bluetooth = LinkHandle::bluetooth();
    let (_, changes) = supervisor.link_arrived(mac(1), bluetooth.reader(), t0);
    assert_eq!(changes, [Change::Returned { mac: mac(1) }]);
    let usb = LinkHandle::default();
    supervisor.link_arrived(mac(1), usb.reader(), t0);
    shows_input_from(&bus, 0, &usb);
    assert_eq!(bus.plugged(), 1);
}

#[test]
fn without_a_bus_the_active_link_is_read_and_follows_usb() {
    let t0 = Instant::now();
    let mut supervisor = Supervisor::without_bus(Settings::default());
    let bluetooth = LinkHandle::bluetooth();
    let (_, changes) = supervisor.link_arrived(mac(1), bluetooth.reader(), t0);
    assert_eq!(changes, []);
    let live = Arc::clone(&supervisor.controllers()[0].live);
    bluetooth.send_input(cross());
    wait_until(|| live.latest().map(|s| s.buttons) == Some(Buttons::CROSS));

    let usb = LinkHandle::default();
    let (usb_id, _) = supervisor.link_arrived(mac(1), usb.reader(), t0);
    assert_eq!(supervisor.controllers()[0].active_link, Some(LinkKind::Usb));
    usb.send_input(square());
    wait_until(|| live.latest().map(|s| s.buttons) == Some(Buttons::SQUARE));

    assert_eq!(supervisor.link_gone(usb_id, t0), []);
    bluetooth.send_input(cross());
    wait_until(|| live.latest().map(|s| s.buttons) == Some(Buttons::CROSS));

    let bus = FakeBus::default();
    let changes = supervisor.connect_bus(Box::new(bus.clone()), t0);
    assert_eq!(
        changes,
        [Change::Bridged {
            mac: mac(1),
            output_type: OutputType::Xbox
        }]
    );
    shows_input_from(&bus, 0, &bluetooth);
}

const PINK: Lightbar = Lightbar {
    red: 255,
    green: 0,
    blue: 128,
};

fn pink() -> Preferences {
    Preferences {
        lightbar: Some(LightbarChoice::Colour(PINK)),
        ..Preferences::default()
    }
}

fn turned_pink(link: &LinkHandle) -> bool {
    let report = usb_output_report(
        &Feedback {
            lightbar: Some(PINK),
            ..Feedback::OFF
        },
        RumbleMode::CompatibleV2,
    );
    link.0.lock().unwrap().written.contains(&report.to_vec())
}

#[test]
fn new_preferences_reach_a_bridged_controller_at_once() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);

    supervisor.change_preference(
        mac(1),
        PreferenceChange::Lightbar(Some(LightbarChoice::Colour(PINK))),
    );

    wait_until(|| turned_pink(&link));
    assert_eq!(supervisor.controllers()[0].preferences, pink());
    assert_eq!(supervisor.settings().controller(mac(1)).preferences, pink());
    assert_eq!(bus.plugged(), 1, "the Virtual Controller is not replaced");
}

#[test]
fn remembered_preferences_apply_to_an_arriving_controller() {
    let mut settings = Settings::default();
    settings.set_preferences(mac(1), pink());
    let mut supervisor = Supervisor::new(Box::new(FakeBus::default()), settings);
    let link = LinkHandle::default();

    supervisor.link_arrived(mac(1), link.reader(), Instant::now());

    wait_until(|| turned_pink(&link));
}

#[test]
fn a_nickname_is_listed_and_remembered_without_touching_the_bridge() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    supervisor.link_arrived(mac(1), LinkHandle::default().reader(), t0);

    supervisor.set_nickname(mac(1), "Sofa");

    assert_eq!(
        supervisor.controllers()[0].nickname.as_deref(),
        Some("Sofa")
    );
    assert_eq!(supervisor.controllers()[0].phase, Phase::Bridged);
    assert_eq!(bus.plugged(), 1);
}

#[test]
fn a_virtual_controller_failing_as_its_link_goes_is_bridged_again_from_the_other_link() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let bluetooth = LinkHandle::bluetooth();
    let usb = LinkHandle::default();
    supervisor.link_arrived(mac(1), bluetooth.reader(), t0);
    let (usb_id, _) = supervisor.link_arrived(mac(1), usb.reader(), t0);
    let first = bus.shown(0);
    first.lock().unwrap().refuse_updates = true;
    usb.send_input(cross());
    wait_until(|| first.lock().unwrap().unplugged);

    assert_eq!(
        supervisor.link_gone(usb_id, t0),
        [Change::Removed {
            mac: mac(1),
            why: Removal::Failed
        }]
    );
    let view = &supervisor.controllers()[0];
    assert_eq!(view.phase, Phase::Waiting);
    assert_eq!(view.error, Some(bus_gone()));

    supervisor.poll(t0 + BRIDGE_RETRY_DELAY);
    assert_eq!(supervisor.controllers()[0].phase, Phase::Bridged);
    assert_eq!(supervisor.controllers()[0].error, None);
    shows_input_from(&bus, 1, &bluetooth);
}

#[test]
fn a_virtual_controller_failing_as_the_only_link_goes_leaves_nothing_behind() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let link = LinkHandle::default();
    let (id, _) = supervisor.link_arrived(mac(1), link.reader(), t0);
    let first = bus.shown(0);
    first.lock().unwrap().refuse_updates = true;
    link.send(cross());
    wait_until(|| first.lock().unwrap().unplugged);

    supervisor.link_gone(id, t0);

    assert!(supervisor.controllers().is_empty());
}

#[test]
fn a_link_the_supervisor_lets_go_is_told_to_whoever_found_it() {
    let t0 = Instant::now();
    let (mut supervisor, _bus) = setup();
    let link = LinkHandle::default();
    let (id, _) = supervisor.link_arrived(mac(1), link.reader(), t0);
    assert_eq!(supervisor.take_dropped_links(), []);

    link.pull_out();
    poll_until_changed(&mut supervisor, t0);

    assert_eq!(supervisor.take_dropped_links(), [id]);
    assert_eq!(supervisor.take_dropped_links(), []);
}

fn shows_live_input(supervisor: &Supervisor, place: usize, from: &LinkHandle) {
    let live = Arc::clone(&supervisor.controllers()[place].live);
    from.send_input(cross());
    wait_until(|| live.latest().map(|s| s.buttons) == Some(Buttons::CROSS));
    from.send_input(square());
    wait_until(|| live.latest().map(|s| s.buttons) == Some(Buttons::SQUARE));
}

fn sent_nothing(link: &LinkHandle) -> bool {
    link.0.lock().unwrap().written.is_empty()
}

#[test]
fn a_stopped_controller_shows_live_input_and_is_sent_nothing() {
    let t0 = Instant::now();
    let mut settings = Settings::default();
    settings.set_stopped(mac(1), true);
    settings.set_preferences(mac(1), pink());
    let bus = FakeBus::default();
    let mut supervisor = Supervisor::new(Box::new(bus.clone()), settings);
    let link = LinkHandle::default();

    supervisor.link_arrived(mac(1), link.reader(), t0);

    shows_live_input(&supervisor, 0, &link);
    assert!(sent_nothing(&link));
    assert_eq!(bus.plugged(), 0);
}

#[test]
fn stopping_watches_the_same_link_and_starting_bridges_it_again() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);

    supervisor.stop(mac(1), t0);
    shows_live_input(&supervisor, 0, &link);
    assert_eq!(bus.last_state(0), None);

    supervisor.start(mac(1), t0);
    shows_input_from(&bus, 1, &link);
    assert_eq!(supervisor.take_dropped_links(), []);
}

#[test]
fn a_fifth_controller_shows_live_input_until_a_bridge_is_free_for_it() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let links: Vec<LinkHandle> = (0..5).map(|_| LinkHandle::default()).collect();
    for (n, link) in (1..).zip(&links) {
        supervisor.link_arrived(mac(n), link.reader(), t0);
    }

    shows_live_input(&supervisor, 4, &links[4]);
    assert!(sent_nothing(&links[4]));

    supervisor.stop(mac(1), t0);
    assert_eq!(supervisor.controllers()[4].phase, Phase::Bridged);
    shows_input_from(&bus, 4, &links[4]);
    shows_live_input(&supervisor, 0, &links[0]);
}

#[test]
fn a_controller_whose_bridge_failed_shows_live_input_until_it_is_bridged() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    bus.refuse_next_plugs(1);
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);

    shows_live_input(&supervisor, 0, &link);
    assert!(sent_nothing(&link));
    assert_eq!(supervisor.controllers()[0].error, Some(no_free_slot()));

    supervisor.poll(t0 + BRIDGE_RETRY_DELAY);
    shows_input_from(&bus, 0, &link);
}

#[test]
fn a_watched_controller_that_loses_its_link_leaves_the_list() {
    let t0 = Instant::now();
    let (mut supervisor, _bus) = setup();
    let link = LinkHandle::default();
    let (id, _) = supervisor.link_arrived(mac(1), link.reader(), t0);
    supervisor.stop(mac(1), t0);

    link.pull_out();
    wait_until(|| {
        supervisor.poll(t0);
        supervisor.controllers().is_empty()
    });

    assert_eq!(supervisor.take_dropped_links(), [id]);
}

#[test]
fn starting_a_watched_controller_whose_link_has_just_gone_removes_it() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);
    supervisor.stop(mac(1), t0);
    shows_live_input(&supervisor, 0, &link);
    let live = Arc::clone(&supervisor.controllers()[0].live);
    link.pull_out();
    wait_until(|| live.latest().is_none());

    supervisor.start(mac(1), t0);

    assert!(supervisor.controllers().is_empty());
    assert_eq!(bus.plugged(), 1);
}

#[test]
fn a_held_virtual_controller_that_cannot_be_released_is_removed() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);
    bus.shown(0).lock().unwrap().refuse_updates = true;
    link.pull_out();

    assert_eq!(
        poll_until_changed(&mut supervisor, t0),
        [
            Change::Held { mac: mac(1) },
            Change::Removed {
                mac: mac(1),
                why: Removal::Failed
            },
        ]
    );
    assert!(bus.unplugged(0));
    assert!(supervisor.controllers().is_empty());
    assert!(
        supervisor
            .take_virtual_failures()
            .contains(&VirtualFailure::NotReleased("the bus went away".into()))
    );
}

#[test]
fn a_virtual_controller_that_cannot_be_removed_is_reported() {
    let t0 = Instant::now();
    let stop: fn(&mut Supervisor, Instant) = |supervisor, now| {
        supervisor.stop(mac(1), now);
    };
    let quit: fn(&mut Supervisor, Instant) = |supervisor, now| {
        supervisor.quit(now);
    };
    let change: fn(&mut Supervisor, Instant) = |supervisor, now| {
        supervisor.set_output_type(mac(1), OutputType::Ps4, now);
    };
    for remove in [stop, quit, change] {
        let (mut supervisor, bus) = setup();
        supervisor.link_arrived(mac(1), LinkHandle::default().reader(), t0);
        bus.shown(0).lock().unwrap().refuse_unplug = true;

        remove(&mut supervisor, t0);

        assert_eq!(
            supervisor.take_virtual_failures(),
            [VirtualFailure::NotUnplugged("it will not leave".into())]
        );
    }
}

#[test]
fn a_held_virtual_controller_that_cannot_be_removed_when_its_hold_ends_is_reported() {
    let t0 = Instant::now();
    let (mut supervisor, bus) = setup();
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);
    link.pull_out();
    poll_until_changed(&mut supervisor, t0);
    bus.shown(0).lock().unwrap().refuse_unplug = true;

    supervisor.poll(t0 + HOLD_TIME);

    assert_eq!(
        supervisor.take_virtual_failures(),
        [VirtualFailure::NotUnplugged("it will not leave".into())]
    );
}

#[derive(Default)]
struct FlakyThreads(Mutex<usize>);

impl FlakyThreads {
    fn refuse_next(&self, count: usize) {
        *self.0.lock().unwrap() = count;
    }
}

impl Threads for FlakyThreads {
    fn spawn(&self, name: &str, task: Task) -> std::io::Result<std::thread::JoinHandle<()>> {
        let mut refusals = self.0.lock().unwrap();
        if name == "heka-bridge" && *refusals > 0 {
            *refusals -= 1;
            return Err(std::io::Error::other("no more threads"));
        }
        SystemThreads.spawn(name, task)
    }
}

fn flaky() -> (Supervisor, FakeBus, Arc<FlakyThreads>) {
    let (supervisor, bus) = setup();
    let threads = Arc::new(FlakyThreads::default());
    let supervisor = supervisor.spawning_with(Arc::clone(&threads) as Arc<dyn Threads>);
    (supervisor, bus, threads)
}

fn no_thread() -> BridgeError {
    BridgeError::NoThread("no more threads".into())
}

#[test]
fn a_bridge_that_cannot_start_a_thread_is_tried_again_on_the_same_link() {
    let t0 = Instant::now();
    let (mut supervisor, bus, threads) = flaky();
    threads.refuse_next(1);
    let link = LinkHandle::default();

    supervisor.link_arrived(mac(1), link.reader(), t0);

    assert!(bus.unplugged(0));
    assert_eq!(supervisor.controllers()[0].phase, Phase::Waiting);
    assert_eq!(supervisor.controllers()[0].error, Some(no_thread()));
    supervisor.poll(t0 + BRIDGE_RETRY_DELAY);
    assert_eq!(supervisor.controllers()[0].error, None);
    shows_input_from(&bus, 1, &link);
}

#[test]
fn a_held_controller_that_cannot_start_a_thread_on_return_is_tried_again() {
    let t0 = Instant::now();
    let (mut supervisor, bus, threads) = flaky();
    let first = LinkHandle::default();
    supervisor.link_arrived(mac(1), first.reader(), t0);
    first.pull_out();
    poll_until_changed(&mut supervisor, t0);

    threads.refuse_next(1);
    let back = LinkHandle::default();
    supervisor.link_arrived(mac(1), back.reader(), t0);
    assert!(bus.unplugged(0));

    supervisor.poll(t0 + BRIDGE_RETRY_DELAY);
    shows_input_from(&bus, 1, &back);
}

#[test]
fn a_move_to_usb_that_cannot_start_a_thread_is_tried_again() {
    let t0 = Instant::now();
    let (mut supervisor, bus, threads) = flaky();
    supervisor.link_arrived(mac(1), LinkHandle::bluetooth().reader(), t0);

    threads.refuse_next(1);
    let usb = LinkHandle::default();
    supervisor.link_arrived(mac(1), usb.reader(), t0);
    assert!(bus.unplugged(0));

    supervisor.poll(t0 + BRIDGE_RETRY_DELAY);
    shows_input_from(&bus, 1, &usb);
}

#[test]
fn a_watch_that_cannot_start_a_thread_is_tried_again_at_the_next_poll() {
    let t0 = Instant::now();
    let threads = Arc::new(FlakyThreads::default());
    threads.refuse_next(1);
    let mut supervisor = Supervisor::without_bus(Settings::default())
        .spawning_with(Arc::clone(&threads) as Arc<dyn Threads>);
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);
    assert_eq!(supervisor.controllers()[0].error, Some(no_thread()));

    supervisor.poll(t0);

    assert_eq!(supervisor.controllers()[0].error, None);
    shows_live_input(&supervisor, 0, &link);
}

#[test]
fn two_preference_changes_are_both_kept() {
    let t0 = Instant::now();
    let (mut supervisor, _bus) = setup();
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);

    supervisor.change_preference(
        mac(1),
        PreferenceChange::Rumble(RumbleStrength::percent(25)),
    );
    let preferences = supervisor.change_preference(
        mac(1),
        PreferenceChange::Lightbar(Some(LightbarChoice::Colour(PINK))),
    );

    let expected = Preferences {
        rumble: RumbleStrength::percent(25),
        ..pink()
    };
    assert_eq!(preferences, expected);
    assert_eq!(supervisor.controllers()[0].preferences, expected);
    wait_until(|| turned_pink(&link));
}

fn rumble_report(large: u8, small: u8) -> Vec<u8> {
    let feedback = Feedback {
        rumble: Rumble { large, small },
        ..Feedback::OFF
    };
    usb_output_report(&feedback, RumbleMode::CompatibleV2).to_vec()
}

fn written(link: &LinkHandle) -> Vec<Vec<u8>> {
    link.0.lock().unwrap().written.clone()
}

#[test]
fn a_rumble_test_reaches_a_bridged_controller_until_it_is_ended() {
    let t0 = Instant::now();
    let (mut supervisor, _bus) = setup();
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);

    supervisor.test_rumble(mac(1), Motor::Small);
    wait_until(|| written(&link) == [rumble_report(0, 255)]);

    supervisor.end_rumble_test(mac(1));
    wait_until(|| written(&link) == [rumble_report(0, 255), rumble_report(0, 0)]);
}

#[test]
fn stopping_a_controller_ends_its_rumble_test() {
    let t0 = Instant::now();
    let (mut supervisor, _bus) = setup();
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);
    supervisor.test_rumble(mac(1), Motor::Large);
    wait_until(|| written(&link) == [rumble_report(255, 0)]);

    supervisor.stop(mac(1), t0);

    assert_eq!(written(&link), [rumble_report(255, 0), rumble_report(0, 0)]);
}

#[test]
fn a_controller_without_a_bridge_is_not_rumble_tested() {
    let t0 = Instant::now();
    let mut settings = Settings::default();
    settings.set_stopped(mac(1), true);
    let mut supervisor = Supervisor::new(Box::new(FakeBus::default()), settings);
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);

    supervisor.test_rumble(mac(1), Motor::Large);
    supervisor.start(mac(1), t0);
    shows_live_input(&supervisor, 0, &link);
    std::thread::sleep(Duration::from_millis(150));

    assert!(sent_nothing(&link));
}

#[derive(Default)]
struct Air {
    turned_off: Vec<MacAddress>,
    refuse: bool,
}

#[derive(Clone, Default)]
struct FakeRadio(Arc<Mutex<Air>>);

impl FakeRadio {
    fn turned_off(&self) -> Vec<MacAddress> {
        self.0.lock().unwrap().turned_off.clone()
    }

    fn refuse(&self, refuse: bool) {
        self.0.lock().unwrap().refuse = refuse;
    }
}

impl Radio for FakeRadio {
    fn end_link(&self, mac: MacAddress) -> Result<(), TurnOffError> {
        let mut air = self.0.lock().unwrap();
        if air.refuse {
            return Err(not_connected());
        }
        air.turned_off.push(mac);
        Ok(())
    }
}

fn not_connected() -> TurnOffError {
    TurnOffError::Failed("the device is not connected".into())
}

fn with_radio() -> (Supervisor, FakeBus, FakeRadio) {
    let (supervisor, bus) = setup();
    let radio = FakeRadio::default();
    let supervisor = supervisor.turning_off_with(Box::new(radio.clone()));
    (supervisor, bus, radio)
}

fn turned_off(n: u8) -> Change {
    Change::Removed {
        mac: mac(n),
        why: Removal::TurnedOff,
    }
}

#[test]
fn turning_off_a_bluetooth_controller_removes_its_virtual_controller_at_once() {
    let t0 = Instant::now();
    let (mut supervisor, bus, radio) = with_radio();
    let link = LinkHandle::bluetooth();
    let (id, _) = supervisor.link_arrived(mac(1), link.reader(), t0);
    assert!(supervisor.controllers()[0].has_bluetooth);

    assert_eq!(supervisor.turn_off(mac(1), t0), [turned_off(1)]);

    assert_eq!(radio.turned_off(), [mac(1)]);
    assert!(bus.unplugged(0));
    assert!(supervisor.controllers().is_empty());
    assert_eq!(supervisor.take_turned_off_links(), [id]);
    assert_eq!(supervisor.take_turned_off_links(), []);
    assert_eq!(supervisor.link_gone(id, t0), []);
    assert_eq!(supervisor.poll(t0), []);
}

#[test]
fn a_link_that_died_before_heka_let_it_go_is_not_held() {
    let t0 = Instant::now();
    let (mut supervisor, bus, _radio) = with_radio();
    let link = LinkHandle::bluetooth();
    supervisor.link_arrived(mac(1), link.reader(), t0);
    link.pull_out();
    wait_until(|| bus.last_state(0) == Some(ControllerState::NEUTRAL));

    assert_eq!(supervisor.turn_off(mac(1), t0), [turned_off(1)]);
    assert!(bus.unplugged(0));
}

#[test]
fn a_controller_also_on_usb_carries_on_over_usb_when_turned_off() {
    let t0 = Instant::now();
    let (mut supervisor, bus, radio) = with_radio();
    let usb = LinkHandle::default();
    supervisor.link_arrived(mac(1), LinkHandle::bluetooth().reader(), t0);
    supervisor.link_arrived(mac(1), usb.reader(), t0);

    assert_eq!(supervisor.turn_off(mac(1), t0), []);

    assert_eq!(radio.turned_off(), [mac(1)]);
    let view = &supervisor.controllers()[0];
    assert_eq!(view.phase, Phase::Bridged);
    assert!(!view.has_bluetooth);
    shows_input_from(&bus, 0, &usb);
    assert!(!bus.unplugged(0));
    assert_eq!(bus.plugged(), 1);
}

#[test]
fn turning_off_a_stopped_controller_takes_it_off_the_list() {
    let t0 = Instant::now();
    let (mut supervisor, _bus, radio) = with_radio();
    supervisor.link_arrived(mac(1), LinkHandle::bluetooth().reader(), t0);
    supervisor.stop(mac(1), t0);

    assert_eq!(supervisor.turn_off(mac(1), t0), []);

    assert_eq!(radio.turned_off(), [mac(1)]);
    assert!(supervisor.controllers().is_empty());
}

#[test]
fn a_controller_without_a_bluetooth_link_is_not_turned_off() {
    let t0 = Instant::now();
    let (mut supervisor, _bus, radio) = with_radio();
    supervisor.link_arrived(mac(1), LinkHandle::default().reader(), t0);
    assert!(!supervisor.controllers()[0].has_bluetooth);

    assert_eq!(supervisor.turn_off(mac(1), t0), []);
    assert_eq!(supervisor.turn_off(mac(2), t0), []);

    assert_eq!(radio.turned_off(), []);
    assert_eq!(supervisor.controllers()[0].phase, Phase::Bridged);
}

#[test]
fn a_failure_to_turn_off_leaves_the_bridge_and_is_listed_until_it_succeeds() {
    let t0 = Instant::now();
    let (mut supervisor, bus, radio) = with_radio();
    let link = LinkHandle::bluetooth();
    supervisor.link_arrived(mac(1), link.reader(), t0);
    radio.refuse(true);

    assert_eq!(supervisor.turn_off(mac(1), t0), []);

    let view = &supervisor.controllers()[0];
    assert_eq!(view.phase, Phase::Bridged);
    assert_eq!(view.turn_off_error, Some(not_connected()));
    assert_eq!(supervisor.take_turned_off_links(), []);
    shows_input_from(&bus, 0, &link);

    radio.refuse(false);
    assert_eq!(supervisor.turn_off(mac(1), t0), [turned_off(1)]);
}

#[test]
fn a_failure_to_turn_off_is_forgotten_with_the_bluetooth_link() {
    let t0 = Instant::now();
    let (mut supervisor, _bus, radio) = with_radio();
    supervisor.link_arrived(mac(1), LinkHandle::default().reader(), t0);
    let (bluetooth, _) = supervisor.link_arrived(mac(1), LinkHandle::bluetooth().reader(), t0);
    radio.refuse(true);
    supervisor.turn_off(mac(1), t0);

    supervisor.link_gone(bluetooth, t0);

    assert_eq!(supervisor.controllers()[0].turn_off_error, None);
}

#[test]
fn without_a_radio_turning_off_fails() {
    let t0 = Instant::now();
    let (mut supervisor, _bus) = setup();
    supervisor.link_arrived(mac(1), LinkHandle::bluetooth().reader(), t0);

    supervisor.turn_off(mac(1), t0);

    assert_eq!(
        supervisor.controllers()[0].turn_off_error,
        Some(TurnOffError::NoRadio)
    );
}

const FIVE_MINUTES: Duration = Duration::from_secs(300);
const ALMOST: Duration = Duration::from_millis(299_999);

fn idling(link: &LinkHandle, t0: Instant) -> (Supervisor, FakeBus, FakeRadio) {
    let (mut supervisor, bus, radio) = with_radio();
    supervisor.link_arrived(mac(1), link.reader(), t0);
    supervisor.set_idle_time(mac(1), Some(IdleTime::Five));
    link.send_input(common::derived(&[]));
    wait_until(|| bus.last_state(0).is_some());
    assert_eq!(supervisor.poll(t0), []);
    (supervisor, bus, radio)
}

#[test]
fn an_idle_bluetooth_controller_is_turned_off_after_its_idle_time() {
    let t0 = Instant::now();
    let (mut supervisor, bus, radio) = idling(&LinkHandle::bluetooth(), t0);
    assert_eq!(supervisor.controllers()[0].idle_time, Some(IdleTime::Five));

    assert_eq!(supervisor.poll(t0 + ALMOST), []);
    assert_eq!(radio.turned_off(), []);
    assert_eq!(supervisor.poll(t0 + FIVE_MINUTES), [turned_off(1)]);

    assert_eq!(radio.turned_off(), [mac(1)]);
    assert!(bus.unplugged(0));
}

#[test]
fn a_controller_has_no_idle_time_until_it_is_given_one() {
    let t0 = Instant::now();
    let (mut supervisor, _bus, radio) = with_radio();
    supervisor.link_arrived(mac(1), LinkHandle::bluetooth().reader(), t0);

    supervisor.poll(t0);
    supervisor.poll(t0 + Duration::from_secs(86_400));

    assert_eq!(radio.turned_off(), []);
}

#[test]
fn using_a_controller_starts_its_idle_time_over() {
    let t0 = Instant::now();
    let link = LinkHandle::bluetooth();
    let (mut supervisor, bus, radio) = idling(&link, t0);

    link.send_input(cross());
    wait_until(|| bus.last_state(0).map(|s| s.buttons) == Some(Buttons::CROSS));
    let used = t0 + Duration::from_secs(240);
    assert_eq!(supervisor.poll(used), []);

    assert_eq!(supervisor.poll(used + ALMOST), []);
    assert_eq!(radio.turned_off(), []);
    assert_eq!(supervisor.poll(used + FIVE_MINUTES), [turned_off(1)]);
}

#[test]
fn moving_a_controller_without_touching_it_leaves_it_idle() {
    let t0 = Instant::now();
    let link = LinkHandle::bluetooth();
    let (mut supervisor, bus, _radio) = idling(&link, t0);

    let reports = bus.shown(0).lock().unwrap().updates.len();
    link.send_input(common::derived(&[(16, 0x40), (17, 0x20)]));
    wait_until(|| bus.shown(0).lock().unwrap().updates.len() > reports);
    assert_ne!(
        bus.last_state(0).unwrap().motion,
        ControllerState::NEUTRAL.motion
    );

    assert_eq!(supervisor.poll(t0 + FIVE_MINUTES), [turned_off(1)]);
}

#[test]
fn the_idle_time_does_nothing_over_usb_and_starts_when_usb_leaves() {
    let t0 = Instant::now();
    let (mut supervisor, _bus, radio) = idling(&LinkHandle::bluetooth(), t0);
    let (usb, _) = supervisor.link_arrived(mac(1), LinkHandle::default().reader(), t0);

    let later = t0 + Duration::from_secs(3_600);
    supervisor.poll(later);
    supervisor.poll(later + FIVE_MINUTES);
    assert_eq!(radio.turned_off(), []);

    let unplugged = later + FIVE_MINUTES * 2;
    supervisor.link_gone(usb, unplugged);
    assert_eq!(supervisor.poll(unplugged), []);
    assert_eq!(supervisor.poll(unplugged + ALMOST), []);
    assert_eq!(supervisor.poll(unplugged + FIVE_MINUTES), [turned_off(1)]);
}

#[test]
fn choosing_an_idle_time_starts_it_from_then() {
    let t0 = Instant::now();
    let (mut supervisor, _bus, radio) = idling(&LinkHandle::bluetooth(), t0);

    let chosen = t0 + ALMOST;
    supervisor.set_idle_time(mac(1), Some(IdleTime::Ten));
    supervisor.poll(chosen);
    supervisor.set_idle_time(mac(1), Some(IdleTime::Five));
    assert_eq!(supervisor.poll(chosen + FIVE_MINUTES), []);
    assert_eq!(supervisor.poll(chosen + FIVE_MINUTES * 2), [turned_off(1)]);
    assert_eq!(radio.turned_off(), [mac(1)]);
}

#[test]
fn a_stopped_controller_is_turned_off_when_idle_too() {
    let t0 = Instant::now();
    let (mut supervisor, _bus, radio) = idling(&LinkHandle::bluetooth(), t0);
    supervisor.stop(mac(1), t0);

    supervisor.poll(t0 + FIVE_MINUTES);

    assert_eq!(radio.turned_off(), [mac(1)]);
    assert!(supervisor.controllers().is_empty());
}

#[test]
fn an_idle_controller_that_cannot_be_turned_off_is_tried_after_another_idle_time() {
    let t0 = Instant::now();
    let (mut supervisor, _bus, radio) = idling(&LinkHandle::bluetooth(), t0);
    radio.refuse(true);

    supervisor.poll(t0 + FIVE_MINUTES);
    assert_eq!(
        supervisor.controllers()[0].turn_off_error,
        Some(not_connected())
    );
    radio.refuse(false);

    assert_eq!(
        supervisor.poll(t0 + FIVE_MINUTES + Duration::from_millis(250)),
        []
    );
    assert_eq!(supervisor.poll(t0 + FIVE_MINUTES * 2), [turned_off(1)]);
}

#[test]
fn a_failure_to_turn_off_is_forgotten_when_the_bluetooth_link_stops_reading() {
    let t0 = Instant::now();
    let (mut supervisor, _bus, radio) = with_radio();
    let link = LinkHandle::bluetooth();
    supervisor.link_arrived(mac(1), link.reader(), t0);
    radio.refuse(true);
    supervisor.turn_off(mac(1), t0);

    link.pull_out();
    assert_eq!(
        poll_until_changed(&mut supervisor, t0),
        [Change::Held { mac: mac(1) }]
    );

    assert_eq!(supervisor.controllers()[0].turn_off_error, None);
}
