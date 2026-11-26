use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use heka_core::calibration::Calibration;
use heka_core::discovery::{
    Discovery, Finder, Found, FoundLink, LONGEST_RETRY_DELAY, OpenFailure, STEADY_LINK_TIME,
    Unopened,
};
use heka_core::lifecycle::{Change, Phase};
use heka_core::link::{ControllerModel, LinkRead, LinkReader, ReportFormat};
use heka_core::mac::MacAddress;
use heka_core::settings::Settings;
use heka_core::state::ControllerState;
use heka_core::supervisor::Supervisor;
use heka_core::virtual_controller::{OutputType, VirtualBus, VirtualController, VirtualError};

const USB: &str = "usb";
const SCAN: Duration = Duration::from_millis(250);
const MAC: MacAddress = MacAddress([0xA0, 0, 0, 0, 0, 1]);

struct FakeLink(Arc<AtomicBool>);

impl LinkReader for FakeLink {
    fn format(&self) -> ReportFormat {
        ReportFormat::DualSenseUsb(Calibration::UNCALIBRATED)
    }

    fn read(&mut self, _: &mut [u8]) -> LinkRead {
        if self.0.load(Ordering::Relaxed) {
            return LinkRead::Lost;
        }
        std::thread::sleep(Duration::from_millis(1));
        LinkRead::Timeout
    }
}

#[derive(Clone)]
enum Port {
    Opens,
    Refuses(&'static str),
    Virtual,
}

#[derive(Default)]
struct Ports {
    present: BTreeMap<&'static str, Port>,
    open: BTreeMap<&'static str, Arc<AtomicBool>>,
    attempts: usize,
}

impl Ports {
    fn with(path: &'static str, port: Port) -> Self {
        let mut ports = Self::default();
        ports.present.insert(path, port);
        ports
    }

    fn lose(&self, path: &str) -> bool {
        self.open
            .get(path)
            .is_some_and(|lost| !lost.swap(true, Ordering::Relaxed))
    }
}

impl Finder for Ports {
    type Path = &'static str;

    fn present(&mut self) -> Vec<&'static str> {
        self.present.keys().copied().collect()
    }

    fn open(&mut self, path: &&'static str) -> Result<FoundLink, OpenFailure> {
        self.attempts += 1;
        match &self.present[path] {
            Port::Opens => {
                let lost = Arc::new(AtomicBool::new(false));
                self.open.insert(path, Arc::clone(&lost));
                Ok(FoundLink {
                    mac: MAC,
                    reader: Box::new(FakeLink(lost)),
                })
            }
            Port::Refuses(error) => Err(OpenFailure::Refused(refusal(error))),
            Port::Virtual => Err(OpenFailure::VirtualController),
        }
    }
}

fn refusal(error: &str) -> Unopened {
    Unopened {
        model: ControllerModel::DualSense,
        error: error.into(),
    }
}

#[derive(Clone, Default)]
struct CountingBus(Arc<AtomicUsize>);

impl VirtualBus for CountingBus {
    fn plug(&self, _: OutputType) -> Result<Box<dyn VirtualController>, VirtualError> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(Box::new(Silent))
    }
}

struct Silent;

impl VirtualController for Silent {
    fn output_type(&self) -> OutputType {
        OutputType::Xbox
    }

    fn update(&mut self, _: &ControllerState) -> Result<(), VirtualError> {
        Ok(())
    }

    fn unplug(self: Box<Self>) -> Result<(), VirtualError> {
        Ok(())
    }
}

fn setup() -> (Supervisor, CountingBus, Discovery<&'static str>) {
    let bus = CountingBus::default();
    let supervisor = Supervisor::new(Box::new(bus.clone()), Settings::default());
    (supervisor, bus, Discovery::default())
}

fn poll_until_changed(supervisor: &mut Supervisor, now: Instant) -> Vec<Change> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let changes = supervisor.poll(now);
        if !changes.is_empty() {
            return changes;
        }
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn a_dropped_link_that_is_still_present_is_opened_again_and_resumes() {
    let t0 = Instant::now();
    let (mut supervisor, bus, mut discovery) = setup();
    let mut ports = Ports::with(USB, Port::Opens);
    discovery.scan(&mut ports, &mut supervisor, t0);
    ports.lose(USB);
    assert_eq!(
        poll_until_changed(&mut supervisor, t0),
        [Change::Held { mac: MAC }]
    );

    let found = discovery.scan(&mut ports, &mut supervisor, t0 + SCAN);

    assert_eq!(
        found,
        [
            Found::Gone {
                mac: MAC,
                path: USB
            },
            Found::Seen {
                mac: MAC,
                path: USB
            },
            Found::Changes(vec![Change::Returned { mac: MAC }]),
        ]
    );
    assert_eq!(supervisor.controllers()[0].phase, Phase::Bridged);
    assert_eq!(bus.0.load(Ordering::Relaxed), 1);
}

#[test]
fn a_link_that_stays_put_is_opened_once() {
    let t0 = Instant::now();
    let (mut supervisor, _, mut discovery) = setup();
    let mut ports = Ports::with(USB, Port::Opens);

    for scan in 0..100 {
        discovery.scan(&mut ports, &mut supervisor, t0 + SCAN * scan);
    }

    assert_eq!(ports.attempts, 1);
}

#[test]
fn a_link_that_keeps_failing_is_opened_at_a_bounded_pace() {
    let t0 = Instant::now();
    let (mut supervisor, _, mut discovery) = setup();
    let mut ports = Ports::with(USB, Port::Opens);

    for scan in 0..240 {
        let now = t0 + SCAN * scan;
        discovery.scan(&mut ports, &mut supervisor, now);
        if ports.lose(USB) {
            poll_until_changed(&mut supervisor, now);
        }
    }

    assert!((10..=20).contains(&ports.attempts), "{}", ports.attempts);
}

#[test]
fn a_link_that_held_for_a_while_is_opened_again_at_once() {
    let t0 = Instant::now();
    let (mut supervisor, _, mut discovery) = setup();
    let mut ports = Ports::with(USB, Port::Opens);
    let mut now = t0;
    let mut reopen = |ports: &mut Ports, supervisor: &mut Supervisor, now: &mut Instant| {
        for _ in 0..2 {
            *now += LONGEST_RETRY_DELAY;
            discovery.scan(ports, supervisor, *now);
        }
    };
    for _ in 0..6 {
        reopen(&mut ports, &mut supervisor, &mut now);
        assert!(ports.lose(USB));
        poll_until_changed(&mut supervisor, now);
    }
    reopen(&mut ports, &mut supervisor, &mut now);

    now += STEADY_LINK_TIME;
    assert!(ports.lose(USB));
    poll_until_changed(&mut supervisor, now);
    let before = ports.attempts;
    discovery.scan(&mut ports, &mut supervisor, now);

    assert_eq!(ports.attempts, before + 1);
}

#[test]
fn an_unplugged_link_is_held_and_its_return_resumes() {
    let t0 = Instant::now();
    let (mut supervisor, bus, mut discovery) = setup();
    let mut ports = Ports::with(USB, Port::Opens);
    discovery.scan(&mut ports, &mut supervisor, t0);

    ports.present.clear();
    let found = discovery.scan(&mut ports, &mut supervisor, t0 + SCAN);
    assert_eq!(
        found,
        [
            Found::Gone {
                mac: MAC,
                path: USB
            },
            Found::Changes(vec![Change::Held { mac: MAC }]),
        ]
    );

    ports.present.insert(USB, Port::Opens);
    discovery.scan(&mut ports, &mut supervisor, t0 + SCAN * 2);
    assert_eq!(supervisor.controllers()[0].phase, Phase::Bridged);
    assert_eq!(bus.0.load(Ordering::Relaxed), 1);
}

fn unopened(discovery: &Discovery<&'static str>) -> Vec<Unopened> {
    discovery.unopened().cloned().collect()
}

#[test]
fn a_controller_that_cannot_be_opened_is_listed_with_the_reason_and_reported_once() {
    let t0 = Instant::now();
    let (mut supervisor, _, mut discovery) = setup();
    let mut ports = Ports::with(USB, Port::Refuses("access is denied"));

    let reported: Vec<_> = (0..240)
        .flat_map(|scan| discovery.scan(&mut ports, &mut supervisor, t0 + SCAN * scan))
        .collect();

    assert_eq!(reported, [Found::CannotOpen(refusal("access is denied"))]);
    assert_eq!(unopened(&discovery), [refusal("access is denied")]);
    assert!((10..=20).contains(&ports.attempts), "{}", ports.attempts);
    assert!(supervisor.controllers().is_empty());
}

#[test]
fn a_new_reason_for_not_opening_is_reported_again() {
    let t0 = Instant::now();
    let (mut supervisor, _, mut discovery) = setup();
    let mut ports = Ports::with(USB, Port::Refuses("access is denied"));
    discovery.scan(&mut ports, &mut supervisor, t0);

    ports.present.insert(USB, Port::Refuses("it timed out"));
    let found = discovery.scan(&mut ports, &mut supervisor, t0 + LONGEST_RETRY_DELAY);

    assert_eq!(found, [Found::CannotOpen(refusal("it timed out"))]);
    assert_eq!(unopened(&discovery), [refusal("it timed out")]);
}

#[test]
fn the_notice_goes_once_the_controller_opens() {
    let t0 = Instant::now();
    let (mut supervisor, _, mut discovery) = setup();
    let mut ports = Ports::with(USB, Port::Refuses("access is denied"));
    discovery.scan(&mut ports, &mut supervisor, t0);

    ports.present.insert(USB, Port::Opens);
    discovery.scan(&mut ports, &mut supervisor, t0 + LONGEST_RETRY_DELAY);

    assert_eq!(unopened(&discovery), []);
    assert_eq!(supervisor.controllers().len(), 1);
}

#[test]
fn the_notice_goes_once_the_controller_is_unplugged_and_a_return_is_tried_at_once() {
    let t0 = Instant::now();
    let (mut supervisor, _, mut discovery) = setup();
    let mut ports = Ports::with(USB, Port::Refuses("access is denied"));
    for scan in 0..100 {
        discovery.scan(&mut ports, &mut supervisor, t0 + SCAN * scan);
    }

    ports.present.clear();
    discovery.scan(&mut ports, &mut supervisor, t0 + SCAN * 100);
    assert_eq!(unopened(&discovery), []);

    let before = ports.attempts;
    ports.present.insert(USB, Port::Opens);
    discovery.scan(&mut ports, &mut supervisor, t0 + SCAN * 101);
    assert_eq!(ports.attempts, before + 1);
}

#[test]
fn a_virtual_controller_is_looked_at_once_and_never_listed() {
    let t0 = Instant::now();
    let (mut supervisor, _, mut discovery) = setup();
    let mut ports = Ports::with(USB, Port::Virtual);

    for scan in 0..100 {
        assert_eq!(
            discovery.scan(&mut ports, &mut supervisor, t0 + SCAN * scan),
            []
        );
    }

    assert_eq!(ports.attempts, 1);
    assert_eq!(unopened(&discovery), []);
}
