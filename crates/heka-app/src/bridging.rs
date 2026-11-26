use std::ffi::CString;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use heka_app::report::Reporter;
use heka_app::{log, text};
use heka_core::discovery::{Discovery, Finder, Found, FoundLink, OpenFailure, Unopened};
use heka_core::hider::{Hider, HidingWarning};
use heka_core::lifecycle::Change;
use heka_core::link::controller_model;
use heka_core::mac::MacAddress;
use heka_core::preferences::PreferenceChange;
use heka_core::setup::{self, Driver, Setup};
use heka_core::supervisor::{ControllerView, Supervisor};
use heka_core::virtual_controller::{OutputType, VirtualFailure};
use heka_windows::drivers::InstalledDrivers;
use heka_windows::hid::{ControllerLink, OpenError, controller_paths};
use heka_windows::hidhide;
use hidapi::HidApi;

use crate::settings_file;

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const LOG_FLUSH: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Starting,
    HidError(String),
    Running,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub status: Status,
    pub setup: Option<Setup>,
    pub controllers: Vec<ControllerView>,
    pub hiding: Vec<HidingWarning>,
    pub unopened: Vec<Unopened>,
    pub not_unplugged: Vec<String>,
}

enum Command {
    Stop(MacAddress),
    Start(MacAddress),
    SetOutputType(MacAddress, OutputType),
    SetNickname(MacAddress, String),
    ChangePreference(MacAddress, PreferenceChange),
    CheckDrivers,
    Quit,
}

pub struct Bridging {
    commands: Sender<Command>,
    thread: Mutex<Option<JoinHandle<()>>>,
    snapshot: Arc<Mutex<Snapshot>>,
}

impl Bridging {
    pub fn start() -> Self {
        let (commands, inbox) = mpsc::channel();
        let snapshot = Arc::new(Mutex::new(Snapshot {
            status: Status::Starting,
            setup: None,
            controllers: Vec::new(),
            hiding: Vec::new(),
            unopened: Vec::new(),
            not_unplugged: Vec::new(),
        }));
        let thread = thread::Builder::new()
            .name("heka-supervisor".into())
            .spawn({
                let snapshot = Arc::clone(&snapshot);
                move || supervise(&inbox, &snapshot)
            })
            .expect("failed to start the supervisor thread");
        Self {
            commands,
            thread: Mutex::new(Some(thread)),
            snapshot,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn stop_controller(&self, mac: MacAddress) {
        let _ = self.commands.send(Command::Stop(mac));
    }

    pub fn start_controller(&self, mac: MacAddress) {
        let _ = self.commands.send(Command::Start(mac));
    }

    pub fn set_output_type(&self, mac: MacAddress, output_type: OutputType) {
        let _ = self.commands.send(Command::SetOutputType(mac, output_type));
    }

    pub fn set_nickname(&self, mac: MacAddress, name: String) {
        let _ = self.commands.send(Command::SetNickname(mac, name));
    }

    pub fn change_preference(&self, mac: MacAddress, change: PreferenceChange) {
        let _ = self.commands.send(Command::ChangePreference(mac, change));
    }

    pub fn check_drivers(&self) {
        let _ = self.commands.send(Command::CheckDrivers);
    }

    pub fn quit(&self) {
        let _ = self.commands.send(Command::Quit);
        let thread = self
            .thread
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(thread) = thread {
            let _ = thread.join();
        }
        log::flush(LOG_FLUSH);
    }
}

impl Drop for Bridging {
    fn drop(&mut self) {
        self.quit();
    }
}

fn supervise(inbox: &mpsc::Receiver<Command>, snapshot: &Mutex<Snapshot>) {
    let settings = settings_file::load();
    let mut hider = Hider::new(hidhide::hiding(), settings.never_hidden());
    let mut supervisor = Supervisor::without_bus(settings);
    let (mut detected, changes) = setup::check(&InstalledDrivers, &mut supervisor, Instant::now());
    log_all(text::log::drivers(&detected));
    follow(&mut hider, &changes);
    let mut reporter = Reporter::default();
    let mut discovery = Discovery::default();
    let mut api = None;
    let mut status = Status::Starting;
    let mut not_unplugged = Vec::new();
    let mut looked: Option<Instant> = None;
    let mut unsaved = false;
    loop {
        let command = inbox.recv_timeout(POLL_INTERVAL);
        let now = Instant::now();
        let burst = matches!(
            command,
            Ok(Command::SetNickname(..) | Command::ChangePreference(..))
        );
        let changes = match command {
            Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Command::Stop(mac)) => {
                unsaved = true;
                supervisor.stop(mac, now)
            }
            Ok(Command::Start(mac)) => {
                unsaved = true;
                supervisor.start(mac, now)
            }
            Ok(Command::SetOutputType(mac, output_type)) => {
                unsaved = true;
                supervisor.set_output_type(mac, output_type, now)
            }
            Ok(Command::SetNickname(mac, name)) => {
                unsaved = true;
                supervisor.set_nickname(mac, &name);
                Vec::new()
            }
            Ok(Command::ChangePreference(mac, change)) => {
                unsaved = true;
                let preferences = supervisor.change_preference(mac, change);
                hider.set_hide(mac, preferences.hide);
                Vec::new()
            }
            Ok(Command::CheckDrivers) => {
                let (found, changes) = setup::check(&InstalledDrivers, &mut supervisor, now);
                detected = found;
                log_all(text::log::drivers(&detected));
                changes
            }
            Err(RecvTimeoutError::Timeout) => Vec::new(),
        };
        follow(&mut hider, &changes);

        let due = looked.is_none_or(|at| now.duration_since(at) >= POLL_INTERVAL);
        if !burst || due {
            if unsaved {
                settings_file::save(supervisor.settings());
                unsaved = false;
            }
            status = match refreshed(&mut api) {
                Ok(api) => {
                    let found = discovery.scan(&mut HidFinder(api), &mut supervisor, now);
                    act_on(found, &mut hider);
                    Status::Running
                }
                Err(error) => Status::HidError(error),
            };
            follow(&mut hider, &supervisor.poll(now));
            hider.poll(now);
            looked = Some(now);
        }
        report_failures(&mut supervisor, &mut not_unplugged);
        log_hiding_changes(&mut hider);
        let hiding = if detected.is_missing(Driver::HidHide) {
            Vec::new()
        } else {
            hider.warnings()
        };
        let controllers = supervisor.controllers();
        log_all(reporter.controllers(&controllers));
        log_all(reporter.hiding(&hiding));
        match &status {
            Status::HidError(error) => log_all(reporter.hid_error(Some(error))),
            Status::Running => log_all(reporter.hid_error(None)),
            Status::Starting => {}
        }
        publish(
            snapshot,
            Snapshot {
                status: status.clone(),
                setup: Some(detected.clone()),
                controllers,
                hiding,
                unopened: discovery.unopened().cloned().collect(),
                not_unplugged: not_unplugged.clone(),
            },
        );
    }
    log::line(text::log::QUITTING);
    if unsaved {
        settings_file::save(supervisor.settings());
    }
    follow(&mut hider, &supervisor.quit());
    report_failures(&mut supervisor, &mut not_unplugged);
    if let Err(error) = hider.restore_all() {
        log::line(&text::log::cannot_restore_on_quit(&error));
    }
    log_hiding_changes(&mut hider);
}

fn publish(snapshot: &Mutex<Snapshot>, latest: Snapshot) {
    *snapshot.lock().unwrap_or_else(PoisonError::into_inner) = latest;
}

fn log_all(messages: impl IntoIterator<Item = String>) {
    for message in messages {
        log::line(&message);
    }
}

fn log_hiding_changes(hider: &mut Hider<CString>) {
    log_all(hider.take_changes().iter().map(text::log::hiding_change));
}

fn follow(hider: &mut Hider<CString>, changes: &[Change]) {
    log_all(changes.iter().map(text::log::change));
    hider.follow(changes);
}

fn act_on(found: Vec<Found<CString>>, hider: &mut Hider<CString>) {
    for found in found {
        match found {
            Found::Seen {
                mac,
                path,
                model,
                link,
                problems,
            } => {
                log::line(&text::log::link_found(mac, model, link));
                log_all(
                    problems
                        .iter()
                        .map(|problem| text::log::link_problem(mac, problem)),
                );
                let device = hidhide::device_instance_path(&path);
                hider.link_seen(mac, path, device);
            }
            Found::Gone { mac, path } => {
                log::line(&text::log::link_gone(mac));
                hider.link_gone(mac, &path);
            }
            Found::Changes(changes) => follow(hider, &changes),
            Found::CannotOpen(unopened) => {
                log::line(&text::cannot_open(&unopened));
            }
        }
    }
}

fn report_failures(supervisor: &mut Supervisor, not_unplugged: &mut Vec<String>) {
    for failure in supervisor.take_virtual_failures() {
        match failure {
            VirtualFailure::NoFeedback(error) => {
                log::line(&text::log::no_virtual_feedback(&error));
            }
            VirtualFailure::NotReleased(error) => log::line(&text::not_released(&error)),
            VirtualFailure::NotUnplugged(error) => {
                log::line(&text::not_unplugged(&error));
                if !not_unplugged.contains(&error) {
                    not_unplugged.push(error);
                }
            }
        }
    }
}

fn refreshed(api: &mut Option<HidApi>) -> Result<&HidApi, String> {
    let api = match api {
        Some(api) => {
            api.refresh_devices().map_err(|e| e.to_string())?;
            api
        }
        None => api.insert(HidApi::new().map_err(|e| e.to_string())?),
    };
    Ok(api)
}

struct HidFinder<'a>(&'a HidApi);

impl Finder for HidFinder<'_> {
    type Path = CString;

    fn present(&mut self) -> Vec<CString> {
        controller_paths(self.0)
    }

    fn open(&mut self, path: &CString) -> Result<FoundLink, OpenFailure> {
        match ControllerLink::open_path(self.0, path) {
            Ok((link, mac, problems)) => Ok(FoundLink {
                mac,
                reader: Box::new(link),
                problems,
            }),
            Err(OpenError::VirtualController) => Err(OpenFailure::VirtualController),
            Err(OpenError::Refused(error)) => {
                let model = self
                    .0
                    .device_list()
                    .find(|info| info.path() == path.as_c_str())
                    .and_then(|info| controller_model(info.vendor_id(), info.product_id()))
                    .unwrap_or_default();
                Err(OpenFailure::Refused(Unopened { model, error }))
            }
        }
    }
}
