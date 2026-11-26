mod common;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use heka_core::calibration::Calibration;
use heka_core::hiding::{HidHideError, HidHideStatus};
use heka_core::lifecycle::{Change, Phase};
use heka_core::link::{LinkRead, LinkReader, ReportFormat};
use heka_core::mac::MacAddress;
use heka_core::settings::Settings;
use heka_core::setup::{self, BusUnavailable, Driver, DriverError, Drivers, Missing};
use heka_core::state::{Buttons, ControllerState};
use heka_core::supervisor::Supervisor;
use heka_core::virtual_controller::{OutputType, VirtualBus, VirtualController, VirtualError};

#[derive(Default)]
struct Script {
    reports: VecDeque<Vec<u8>>,
    lost: bool,
}

#[derive(Clone, Default)]
struct LinkHandle(Arc<Mutex<Script>>);

impl LinkHandle {
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
        ReportFormat::DualSenseUsb(Calibration::UNCALIBRATED)
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
}

#[derive(Clone, Default)]
struct FakeBus(Arc<Mutex<Vec<Vec<ControllerState>>>>);

impl FakeBus {
    fn plugged(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    fn last_state(&self, index: usize) -> Option<ControllerState> {
        self.0.lock().unwrap()[index].last().copied()
    }
}

impl VirtualBus for FakeBus {
    fn plug(&self, _: OutputType) -> Result<Box<dyn VirtualController>, VirtualError> {
        let mut plugged = self.0.lock().unwrap();
        plugged.push(Vec::new());
        Ok(Box::new(FakeController(self.clone(), plugged.len() - 1)))
    }
}

struct FakeController(FakeBus, usize);

impl VirtualController for FakeController {
    fn output_type(&self) -> OutputType {
        OutputType::Xbox
    }

    fn update(&mut self, state: &ControllerState) -> Result<(), VirtualError> {
        (self.0).0.lock().unwrap()[self.1].push(*state);
        Ok(())
    }

    fn unplug(self: Box<Self>) -> Result<(), VirtualError> {
        Ok(())
    }
}

struct FakeDrivers {
    vigem_bus: Mutex<Option<BusUnavailable>>,
    hidhide: Mutex<HidHideStatus>,
    bus: FakeBus,
}

impl FakeDrivers {
    fn new(vigem_bus: Option<BusUnavailable>, hidhide: HidHideStatus) -> Self {
        Self {
            vigem_bus: Mutex::new(vigem_bus),
            hidhide: Mutex::new(hidhide),
            bus: FakeBus::default(),
        }
    }

    fn without_vigem_bus() -> Self {
        Self::new(Some(BusUnavailable::NotInstalled), HidHideStatus::Ready)
    }

    fn install_vigem_bus(&self) {
        *self.vigem_bus.lock().unwrap() = None;
    }

    fn install_hidhide(&self) {
        *self.hidhide.lock().unwrap() = HidHideStatus::Ready;
    }
}

impl Drivers for FakeDrivers {
    fn open_vigem_bus(&self) -> Result<Box<dyn VirtualBus>, BusUnavailable> {
        match self.vigem_bus.lock().unwrap().clone() {
            Some(why) => Err(why),
            None => Ok(Box::new(self.bus.clone())),
        }
    }

    fn hidhide(&self) -> HidHideStatus {
        self.hidhide.lock().unwrap().clone()
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

fn cross() -> Vec<u8> {
    common::derived(&[(8, 0x28)])
}

fn live_buttons(supervisor: &Supervisor, index: usize) -> Option<Buttons> {
    supervisor.controllers()[index]
        .live
        .latest()
        .map(|state| state.buttons)
}

fn started(drivers: &FakeDrivers, now: Instant) -> Supervisor {
    let mut supervisor = Supervisor::without_bus(Settings::default());
    setup::check(drivers, &mut supervisor, now);
    supervisor
}

#[test]
fn without_vigem_bus_a_controller_is_listed_with_live_input_and_not_bridged() {
    let t0 = Instant::now();
    let drivers = FakeDrivers::without_vigem_bus();
    let mut supervisor = started(&drivers, t0);
    let link = LinkHandle::default();

    let (_, changes) = supervisor.link_arrived(mac(1), link.reader(), t0);
    assert_eq!(changes, []);
    assert_eq!(supervisor.poll(t0 + Duration::from_secs(60)), []);

    assert_eq!(supervisor.controllers().len(), 1);
    assert_eq!(supervisor.controllers()[0].phase, Phase::Waiting);
    link.send(cross());
    wait_until(|| live_buttons(&supervisor, 0) == Some(Buttons::CROSS));
    assert_eq!(drivers.bus.plugged(), 0);
}

#[test]
fn checking_again_after_installing_vigem_bus_bridges_the_listed_controllers() {
    let t0 = Instant::now();
    let drivers = FakeDrivers::without_vigem_bus();
    let mut supervisor = started(&drivers, t0);
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);

    assert_eq!(setup::check(&drivers, &mut supervisor, t0).1, []);

    drivers.install_vigem_bus();
    let (found, changes) = setup::check(&drivers, &mut supervisor, t0);
    assert_eq!(found.missing(), []);
    assert_eq!(
        changes,
        [Change::Bridged {
            mac: mac(1),
            output_type: OutputType::Xbox
        }]
    );

    assert_eq!(supervisor.controllers()[0].phase, Phase::Bridged);
    link.send(cross());
    wait_until(|| drivers.bus.plugged() == 1 && drivers.bus.last_state(0).is_some());
    assert_eq!(drivers.bus.last_state(0).unwrap().buttons, Buttons::CROSS);
    assert_eq!(live_buttons(&supervisor, 0), Some(Buttons::CROSS));
}

#[test]
fn without_vigem_bus_a_controller_that_loses_its_link_leaves_the_list() {
    let t0 = Instant::now();
    let drivers = FakeDrivers::without_vigem_bus();
    let mut supervisor = started(&drivers, t0);
    let pulled = LinkHandle::default();
    let reported = LinkHandle::default();
    supervisor.link_arrived(mac(1), pulled.reader(), t0);
    let (id, _) = supervisor.link_arrived(mac(2), reported.reader(), t0);

    pulled.pull_out();
    wait_until(|| {
        assert_eq!(supervisor.poll(t0), []);
        supervisor.controllers().len() == 1
    });
    assert_eq!(supervisor.link_gone(id, t0), []);
    assert!(supervisor.controllers().is_empty());

    let back = LinkHandle::default();
    supervisor.link_arrived(mac(1), back.reader(), t0);
    back.send(cross());
    wait_until(|| live_buttons(&supervisor, 0) == Some(Buttons::CROSS));
}

#[test]
fn a_stopped_controller_shows_input_without_vigem_bus_and_stays_stopped_when_it_arrives() {
    let t0 = Instant::now();
    let drivers = FakeDrivers::without_vigem_bus();
    let mut supervisor = started(&drivers, t0);
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);
    supervisor.stop(mac(1), t0);
    link.send(cross());
    wait_until(|| live_buttons(&supervisor, 0) == Some(Buttons::CROSS));

    drivers.install_vigem_bus();
    assert_eq!(setup::check(&drivers, &mut supervisor, t0).1, []);
    assert_eq!(supervisor.controllers()[0].phase, Phase::Stopped);

    supervisor.start(mac(1), t0);
    link.send(cross());
    wait_until(|| drivers.bus.plugged() == 1 && drivers.bus.last_state(0).is_some());
}

#[test]
fn checking_again_with_vigem_bus_present_leaves_bridges_alone() {
    let t0 = Instant::now();
    let drivers = FakeDrivers::new(None, HidHideStatus::NotInstalled);
    let mut supervisor = started(&drivers, t0);
    supervisor.link_arrived(mac(1), LinkHandle::default().reader(), t0);

    assert_eq!(setup::check(&drivers, &mut supervisor, t0).1, []);
    assert_eq!(supervisor.controllers()[0].phase, Phase::Bridged);
    assert_eq!(drivers.bus.plugged(), 1);
}

#[test]
fn every_combination_of_detection_results_lists_exactly_the_missing_drivers() {
    let bus_cases = [
        (None, None),
        (
            Some(BusUnavailable::NotInstalled),
            Some(Missing {
                driver: Driver::ViGEmBus,
                error: None,
            }),
        ),
        (
            Some(BusUnavailable::Failed("access denied".into())),
            Some(Missing {
                driver: Driver::ViGEmBus,
                error: Some(DriverError::Bus("access denied".into())),
            }),
        ),
    ];
    let hidhide_cases = [
        (HidHideStatus::Ready, None),
        (HidHideStatus::InUse, None),
        (
            HidHideStatus::NotInstalled,
            Some(Missing {
                driver: Driver::HidHide,
                error: None,
            }),
        ),
        (
            HidHideStatus::Failed(HidHideError::Failed("bad reply".into())),
            Some(Missing {
                driver: Driver::HidHide,
                error: Some(DriverError::HidHide(HidHideError::Failed(
                    "bad reply".into(),
                ))),
            }),
        ),
    ];
    for (bus, bus_missing) in &bus_cases {
        for (hidhide, hidhide_missing) in &hidhide_cases {
            let drivers = FakeDrivers::new(bus.clone(), hidhide.clone());
            let mut supervisor = Supervisor::without_bus(Settings::default());
            let (found, _) = setup::check(&drivers, &mut supervisor, Instant::now());

            let expected: Vec<Missing> = [bus_missing, hidhide_missing]
                .into_iter()
                .flatten()
                .cloned()
                .collect();
            assert_eq!(found.missing(), expected, "{bus:?} {hidhide:?}");

            supervisor.link_arrived(mac(1), LinkHandle::default().reader(), Instant::now());
            let bridged = supervisor.controllers()[0].phase == Phase::Bridged;
            assert_eq!(bridged, bus.is_none(), "{bus:?} {hidhide:?}");
        }
    }
}

#[test]
fn each_driver_links_to_its_official_releases() {
    assert_eq!(
        Driver::ViGEmBus.installer_url(),
        "https://github.com/nefarius/ViGEmBus/releases/latest"
    );
    assert_eq!(
        Driver::HidHide.installer_url(),
        "https://github.com/nefarius/HidHide/releases/latest"
    );
}

#[test]
fn checking_again_after_installing_hidhide_stops_listing_it() {
    let t0 = Instant::now();
    let drivers = FakeDrivers::new(None, HidHideStatus::NotInstalled);
    let mut supervisor = Supervisor::without_bus(Settings::default());
    assert_eq!(
        setup::check(&drivers, &mut supervisor, t0)
            .0
            .missing()
            .len(),
        1
    );

    drivers.install_hidhide();
    assert_eq!(setup::check(&drivers, &mut supervisor, t0).0.missing(), []);
}

#[test]
fn a_link_lost_just_before_vigem_bus_arrives_is_not_bridged() {
    let t0 = Instant::now();
    let drivers = FakeDrivers::without_vigem_bus();
    let mut supervisor = started(&drivers, t0);
    let link = LinkHandle::default();
    supervisor.link_arrived(mac(1), link.reader(), t0);
    link.send(cross());
    wait_until(|| live_buttons(&supervisor, 0).is_some());
    link.pull_out();
    wait_until(|| live_buttons(&supervisor, 0).is_none());

    drivers.install_vigem_bus();
    assert_eq!(setup::check(&drivers, &mut supervisor, t0).1, []);
    assert!(supervisor.controllers().is_empty());
    assert_eq!(drivers.bus.plugged(), 0);
}

#[test]
fn an_output_type_chosen_without_vigem_bus_is_used_once_it_arrives() {
    let t0 = Instant::now();
    let drivers = FakeDrivers::without_vigem_bus();
    let mut supervisor = started(&drivers, t0);
    supervisor.link_arrived(mac(1), LinkHandle::default().reader(), t0);
    assert_eq!(supervisor.set_output_type(mac(1), OutputType::Ps4, t0), []);

    drivers.install_vigem_bus();
    assert_eq!(
        setup::check(&drivers, &mut supervisor, t0).1,
        [Change::Bridged {
            mac: mac(1),
            output_type: OutputType::Ps4
        }]
    );
}
