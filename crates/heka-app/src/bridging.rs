use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use heka_app::text;
use heka_core::hiding::{DeviceInstancePath, Hiding};
use heka_core::lifecycle::{Change, Removal};
use heka_core::mac::MacAddress;
use heka_core::preferences::Preferences;
use heka_core::setup::{self, Driver, Setup};
use heka_core::supervisor::{ControllerView, LinkId, Supervisor};
use heka_core::virtual_controller::OutputType;
use heka_windows::drivers::InstalledDrivers;
use heka_windows::hid::{ControllerLink, OpenError, controller_paths};
use heka_windows::hidhide;
use hidapi::HidApi;

use crate::settings_file;

const POLL_INTERVAL: Duration = Duration::from_millis(250);

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
    pub hiding_note: Option<String>,
}

enum Command {
    Stop(MacAddress),
    Start(MacAddress),
    SetOutputType(MacAddress, OutputType),
    SetNickname(MacAddress, String),
    SetPreferences(MacAddress, Preferences),
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
            hiding_note: None,
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

    pub fn set_preferences(&self, mac: MacAddress, preferences: Preferences) {
        let _ = self
            .commands
            .send(Command::SetPreferences(mac, preferences));
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
    }
}

impl Drop for Bridging {
    fn drop(&mut self) {
        self.quit();
    }
}

fn supervise(inbox: &mpsc::Receiver<Command>, snapshot: &Mutex<Snapshot>) {
    let settings = settings_file::load();
    let mut hider = Hider::open(settings.never_hidden().collect());
    let publish = |status: Status, setup: &Setup, controllers, hider: &Hider| {
        let hiding_note = if setup.is_missing(Driver::HidHide) {
            None
        } else {
            hider.note.clone()
        };
        *snapshot.lock().unwrap_or_else(PoisonError::into_inner) = Snapshot {
            status,
            setup: Some(setup.clone()),
            controllers,
            hiding_note,
        };
    };

    let mut supervisor = Supervisor::without_bus(settings);
    let (mut detected, changes) = setup::check(&InstalledDrivers, &mut supervisor, Instant::now());
    hider.follow(&changes);
    let mut discovery = Discovery::default();
    let mut status = Status::Starting;
    let mut looked: Option<Instant> = None;
    let mut unsaved = false;
    loop {
        let command = inbox.recv_timeout(POLL_INTERVAL);
        let now = Instant::now();
        let burst = matches!(
            command,
            Ok(Command::SetNickname(..) | Command::SetPreferences(..))
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
            Ok(Command::SetPreferences(mac, preferences)) => {
                unsaved = true;
                supervisor.set_preferences(mac, preferences);
                hider.set_hide(mac, preferences.hide);
                Vec::new()
            }
            Ok(Command::CheckDrivers) => {
                let (found, changes) = setup::check(&InstalledDrivers, &mut supervisor, now);
                detected = found;
                changes
            }
            Err(RecvTimeoutError::Timeout) => Vec::new(),
        };
        hider.follow(&changes);

        let due = looked.is_none_or(|at| now.duration_since(at) >= POLL_INTERVAL);
        if !burst || due {
            if unsaved {
                settings_file::save(supervisor.settings());
                unsaved = false;
            }
            status = match discovery.scan(&mut supervisor, &mut hider, now) {
                Ok(()) => Status::Running,
                Err(error) => Status::HidError(error),
            };
            hider.follow(&supervisor.poll(now));
            hider.retry();
            looked = Some(now);
        }
        publish(status.clone(), &detected, supervisor.controllers(), &hider);
    }
    if unsaved {
        settings_file::save(supervisor.settings());
    }
    hider.follow(&supervisor.quit());
    hider.restore_all();
    publish(Status::Starting, &detected, Vec::new(), &hider);
}

struct Hider {
    hiding: Option<Hiding>,
    devices: HashMap<MacAddress, HashMap<CString, Result<DeviceInstancePath, String>>>,
    wanted: HashSet<MacAddress>,
    seen: HashSet<MacAddress>,
    pending: HashSet<MacAddress>,
    allow_pending: bool,
    note: Option<String>,
}

impl Hider {
    fn open(seen: HashSet<MacAddress>) -> Self {
        let mut hider = Self {
            hiding: None,
            devices: HashMap::new(),
            wanted: HashSet::new(),
            seen,
            pending: HashSet::new(),
            allow_pending: false,
            note: None,
        };
        match hidhide::hiding() {
            Ok(hiding) => {
                if let Err(error) = hiding.recover() {
                    hider.note = Some(text::cannot_restore_earlier_run(&error.to_string()));
                }
                hider.hiding = Some(hiding);
                hider.allow_pending = true;
                hider.allow();
            }
            Err(error) => hider.note = Some(text::cannot_hide_at_all(&error.to_string())),
        }
        hider
    }

    fn allow(&mut self) {
        let Some(hiding) = &self.hiding else {
            self.allow_pending = false;
            return;
        };
        match hiding.allow_heka() {
            Ok(()) => {
                self.allow_pending = false;
                if self.pending.is_empty() {
                    self.note = None;
                }
            }
            Err(error) => self.note = Some(text::not_allowed(&error.to_string())),
        }
    }

    fn link_seen(&mut self, mac: MacAddress, link: &ControllerLink) {
        let device = hidhide::device_instance_path(link.hid_path())
            .map_err(|error| text::cannot_identify_for_hiding(&error.to_string()));
        self.devices
            .entry(mac)
            .or_default()
            .insert(link.hid_path().to_owned(), device);
        if self.wanted.contains(&mac) {
            self.pending.insert(mac);
            self.hide(mac);
        }
    }

    fn link_gone(&mut self, mac: MacAddress, path: &CString) {
        if let Some(devices) = self.devices.get_mut(&mac) {
            devices.remove(path);
        }
    }

    fn follow(&mut self, changes: &[Change]) {
        for (index, change) in changes.iter().enumerate() {
            eprintln!("heka: {change:?}");
            let replaced = |mac| {
                changes[index + 1..]
                    .iter()
                    .any(|later| matches!(later, Change::Bridged { mac: m, .. } if *m == mac))
            };
            match *change {
                Change::Bridged { mac, .. } | Change::Returned { mac } => {
                    self.wanted.insert(mac);
                    self.pending.insert(mac);
                    self.hide(mac);
                }
                Change::Held { .. } => {}
                Change::Removed {
                    mac,
                    why: Removal::OutputTypeChanged,
                } if replaced(mac) => {}
                Change::Removed { mac, .. } => {
                    self.wanted.remove(&mac);
                    self.pending.remove(&mac);
                    self.release(mac);
                }
            }
        }
    }

    fn retry(&mut self) {
        if self.allow_pending {
            self.allow();
        }
        let pending: Vec<MacAddress> = self.pending.iter().copied().collect();
        for mac in pending {
            self.hide(mac);
        }
    }

    fn set_hide(&mut self, mac: MacAddress, hide: bool) {
        if hide {
            if self.seen.remove(&mac) && self.wanted.contains(&mac) {
                self.pending.insert(mac);
                self.hide(mac);
            }
        } else if self.seen.insert(mac) {
            if self.pending.remove(&mac) && self.pending.is_empty() && !self.allow_pending {
                self.note = None;
            }
            self.release(mac);
        }
    }

    fn hide(&mut self, mac: MacAddress) {
        let Some(hiding) = &self.hiding else {
            self.pending.remove(&mac);
            return;
        };
        if self.seen.contains(&mac) {
            self.pending.remove(&mac);
            return;
        }
        let devices = self.devices.get(&mac).filter(|devices| !devices.is_empty());
        let result = match devices {
            Some(devices) => devices
                .values()
                .map(|device| match device {
                    Ok(device) => hiding
                        .hide(&mac.to_string(), device)
                        .map_err(|e| e.to_string()),
                    Err(error) => Err(error.clone()),
                })
                .fold(Ok(()), Result::and),
            None => Err(text::NO_LINK_TO_HIDE.into()),
        };
        match result {
            Ok(()) => {
                self.pending.remove(&mac);
                if self.pending.is_empty() && !self.allow_pending {
                    self.note = None;
                }
            }
            Err(error) => {
                self.note = Some(text::not_hidden(&error));
            }
        }
    }

    fn release(&mut self, mac: MacAddress) {
        if let Some(Err(error)) = self.hiding.as_ref().map(|h| h.release(&mac.to_string())) {
            self.note = Some(text::cannot_restore(&error.to_string()));
        }
    }

    fn restore_all(&mut self) {
        if let Some(Err(error)) = self.hiding.as_ref().map(Hiding::restore_all) {
            eprintln!("heka: could not restore HidHide: {error}");
        }
    }
}

#[derive(Default)]
struct Discovery {
    api: Option<HidApi>,
    known: HashMap<CString, (LinkId, MacAddress)>,
    virtual_controllers: HashSet<CString>,
}

impl Discovery {
    fn scan(
        &mut self,
        supervisor: &mut Supervisor,
        hider: &mut Hider,
        now: Instant,
    ) -> Result<(), String> {
        let api = match &mut self.api {
            Some(api) => {
                api.refresh_devices().map_err(|e| e.to_string())?;
                api
            }
            None => self.api.insert(HidApi::new().map_err(|e| e.to_string())?),
        };
        let present = controller_paths(api);
        self.virtual_controllers
            .retain(|path| present.contains(path));

        let gone: Vec<CString> = self
            .known
            .keys()
            .filter(|path| !present.contains(path))
            .cloned()
            .collect();
        for path in gone {
            if let Some((id, mac)) = self.known.remove(&path) {
                hider.link_gone(mac, &path);
                hider.follow(&supervisor.link_gone(id, now));
            }
        }

        for path in present {
            if self.known.contains_key(&path) || self.virtual_controllers.contains(&path) {
                continue;
            }
            match ControllerLink::open_path(api, &path) {
                Ok((link, mac)) => {
                    hider.link_seen(mac, &link);
                    let (id, changes) = supervisor.link_arrived(mac, Box::new(link), now);
                    self.known.insert(path, (id, mac));
                    hider.follow(&changes);
                }
                Err(OpenError::VirtualController) => {
                    self.virtual_controllers.insert(path);
                }
                Err(error) => eprintln!("heka: cannot open a Controller: {error}"),
            }
        }
        Ok(())
    }
}
