mod common;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use heka_core::calibration::Calibration;
use heka_core::dualsense::{RumbleMode, usb_output_report};
use heka_core::dualshock4::{parse_bluetooth_serial, parse_pairing_info};
use heka_core::feedback::{Feedback, FeedbackEvent, FeedbackSource, Lightbar};
use heka_core::lifecycle::{BRIDGE_RETRY_DELAY, Change, HOLD_TIME, Phase, Removal};
use heka_core::link::{
    ControllerModel, FeedbackFormat, LinkKind, LinkRead, LinkReader, LinkWriteError, LinkWriter,
    ReportFormat,
};
use heka_core::mac::MacAddress;
use heka_core::preferences::{LightbarChoice, Preferences};
use heka_core::settings::Settings;
use heka_core::state::{Buttons, ControllerState};
use heka_core::supervisor::Supervisor;
use heka_core::virtual_controller::{OutputType, VirtualBus, VirtualController, VirtualError};

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
            ReportFormat::DualSenseBluetooth(_) => self.send(common::bluetooth_from_usb(&usb)),
            ReportFormat::DualShock4Bluetooth(_) => self.send(common::ds4_bluetooth_from_usb(&usb)),
            ReportFormat::DualSenseUsb(_) | ReportFormat::DualShock4Usb(_) => self.send(usb),
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
        self.0.lock().unwrap().updates.push(*state);
        Ok(())
    }

    fn unplug(self: Box<Self>) -> Result<(), VirtualError> {
        self.0.lock().unwrap().unplugged = true;
        Ok(())
    }

    fn feedback(&mut self) -> Option<Box<dyn FeedbackSource>> {
        Some(Box::new(QuietGame))
    }
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
    assert_eq!(view.error.as_deref(), Some("no free slot"));

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

    let changes = supervisor.quit();
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

    supervisor.set_preferences(mac(1), pink());

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
