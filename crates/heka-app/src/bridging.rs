//! Runs the Bridge lifecycle for every DualSense on USB.
//!
//! One supervisor thread owns the [`Supervisor`]: it connects to ViGEmBus, finds
//! Controllers as they come and go, carries out the player's Stop and Start, and
//! publishes a snapshot for the window. The window never waits on the thread.
//!
//! Hiding follows the lifecycle's changes: a Controller is Hidden with HidHide when
//! it is Bridged and again when it returns from a Hold, stays Hidden while Held, and
//! is restored whenever its Virtual Controller is removed. A start after a crash
//! first restores what the crashed run Hid.

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use heka_core::hiding::{DeviceInstancePath, Hiding};
use heka_core::lifecycle::{Change, Removal};
use heka_core::mac::MacAddress;
use heka_core::supervisor::{ControllerView, LinkId, Supervisor};
use heka_core::virtual_controller::OutputType;
use heka_windows::hid::{DualSenseUsbLink, dualsense_usb_paths};
use heka_windows::hidhide;
use heka_windows::vigem::VigemBus;
use hidapi::HidApi;

use crate::settings_file;

/// How often the supervisor looks for Controllers, ended Bridges and timers.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// What the supervisor is doing overall, for the window to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Starting,
    /// ViGEmBus could not be opened. Retried every poll.
    NoBus(String),
    /// HID enumeration failed. Retried every poll.
    HidError(String),
    Running,
}

/// What the window shows.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub status: Status,
    pub controllers: Vec<ControllerView>,
    /// Why a Controller is not Hidden when it should be.
    pub hiding_note: Option<String>,
}

enum Command {
    Stop(MacAddress),
    Start(MacAddress),
    SetOutputType(MacAddress, OutputType),
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

    /// Chooses the Controller's Output Type, replacing its Virtual Controller.
    pub fn set_output_type(&self, mac: MacAddress, output_type: OutputType) {
        let _ = self.commands.send(Command::SetOutputType(mac, output_type));
    }

    /// Removes every Virtual Controller, restores every Hidden Controller, and
    /// waits until both are done.
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
    let mut hider = Hider::open();
    let publish = |status: Status, controllers: Vec<ControllerView>, hider: &Hider| {
        *snapshot.lock().unwrap_or_else(PoisonError::into_inner) = Snapshot {
            status,
            controllers,
            hiding_note: hider.note.clone(),
        };
    };

    let bus = loop {
        match VigemBus::connect() {
            Ok(bus) => break bus,
            Err(error) => publish(Status::NoBus(error.to_string()), Vec::new(), &hider),
        }
        match inbox.recv_timeout(POLL_INTERVAL) {
            Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => return,
            // Nothing is listed yet, so there is nothing to stop or start.
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
        }
    };

    let mut supervisor = Supervisor::new(Box::new(bus), settings_file::load());
    let mut discovery = Discovery::default();
    loop {
        let command = inbox.recv_timeout(POLL_INTERVAL);
        let now = Instant::now();
        let changes = match command {
            Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Command::Stop(mac)) => supervisor.stop(mac, now),
            Ok(Command::Start(mac)) => supervisor.start(mac, now),
            Ok(Command::SetOutputType(mac, output_type)) => {
                supervisor.set_output_type(mac, output_type, now)
            }
            Err(RecvTimeoutError::Timeout) => Vec::new(),
        };
        if matches!(
            command,
            Ok(Command::Stop(_) | Command::Start(_) | Command::SetOutputType(..))
        ) {
            settings_file::save(supervisor.settings());
        }
        hider.follow(&changes);

        let status = match discovery.scan(&mut supervisor, &mut hider, now) {
            Ok(()) => Status::Running,
            Err(error) => Status::HidError(error),
        };
        hider.follow(&supervisor.poll(now));
        hider.retry();
        publish(status, supervisor.controllers(), &hider);
    }
    hider.follow(&supervisor.quit());
    hider.restore_all();
    publish(Status::Starting, Vec::new(), &hider);
    // Dropping the Supervisor drops the bus last, which removes anything left.
}

/// Keeps Controllers Hidden as the lifecycle changes. Each Controller is a hiding
/// owner keyed by its MAC address.
struct Hider {
    hiding: Option<Hiding>,
    /// The HidHide device of each Controller's latest Link.
    devices: HashMap<MacAddress, Result<DeviceInstancePath, String>>,
    /// Controllers to Hide that are not Hidden yet, retried every poll, for
    /// example while HidHide's own client holds the driver.
    pending: HashSet<MacAddress>,
    note: Option<String>,
}

impl Hider {
    /// Opens HidHide and undoes whatever a killed run left Hidden.
    fn open() -> Self {
        let mut hider = Self {
            hiding: None,
            devices: HashMap::new(),
            pending: HashSet::new(),
            note: None,
        };
        match hidhide::hiding() {
            Ok(hiding) => {
                if let Err(error) = hiding.recover() {
                    hider.note = Some(format!(
                        "Could not restore Controllers Hidden by an earlier run: {error}"
                    ));
                }
                hider.hiding = Some(hiding);
            }
            Err(error) => hider.note = Some(format!("Controllers cannot be Hidden: {error}")),
        }
        hider
    }

    /// Records the device behind a Link before the Supervisor hears of it.
    fn link_seen(&mut self, mac: MacAddress, link: &DualSenseUsbLink) {
        let device = hidhide::device_instance_path(link.hid_path())
            .map_err(|error| format!("cannot identify the Controller for HidHide: {error}"));
        self.devices.insert(mac, device);
    }

    fn follow(&mut self, changes: &[Change]) {
        for (index, change) in changes.iter().enumerate() {
            eprintln!("heka: {change:?}");
            // A Controller whose Virtual Controller is replaced by one of another
            // Output Type stays Hidden in between.
            let replaced = |mac| {
                changes[index + 1..]
                    .iter()
                    .any(|later| matches!(later, Change::Bridged { mac: m, .. } if *m == mac))
            };
            match *change {
                // A returning Controller is a new device to the system: Hide it again.
                Change::Bridged { mac, .. } | Change::Returned { mac } => {
                    self.pending.insert(mac);
                    self.hide(mac);
                }
                // Stays Hidden while Held.
                Change::Held { .. } => {}
                Change::Removed {
                    mac,
                    why: Removal::OutputTypeChanged,
                } if replaced(mac) => {}
                Change::Removed { mac, .. } => {
                    self.pending.remove(&mac);
                    self.release(mac);
                }
            }
        }
    }

    fn retry(&mut self) {
        let pending: Vec<MacAddress> = self.pending.iter().copied().collect();
        for mac in pending {
            self.hide(mac);
        }
    }

    fn hide(&mut self, mac: MacAddress) {
        let Some(hiding) = &self.hiding else {
            self.pending.remove(&mac);
            return;
        };
        let result = match self.devices.get(&mac) {
            Some(Ok(device)) => hiding
                .hide(&mac.to_string(), device)
                .map_err(|e| e.to_string()),
            Some(Err(error)) => Err(error.clone()),
            None => Err("no Link to Hide".into()),
        };
        match result {
            Ok(()) => {
                self.pending.remove(&mac);
                if self.pending.is_empty() {
                    self.note = None;
                }
            }
            Err(error) => {
                self.note = Some(format!("{error}. Games may see the Controller twice."));
            }
        }
    }

    fn release(&mut self, mac: MacAddress) {
        if let Some(Err(error)) = self.hiding.as_ref().map(|h| h.release(&mac.to_string())) {
            self.note = Some(format!("Could not restore the Controller: {error}"));
        }
    }

    fn restore_all(&mut self) {
        if let Some(Err(error)) = self.hiding.as_ref().map(Hiding::restore_all) {
            eprintln!("heka: could not restore HidHide: {error}");
        }
    }
}

/// Finds DualSense Links by HID path and reports arrivals and departures.
#[derive(Default)]
struct Discovery {
    api: Option<HidApi>,
    known: HashMap<CString, LinkId>,
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
        let present = dualsense_usb_paths(api);

        let gone: Vec<CString> = self
            .known
            .keys()
            .filter(|path| !present.contains(path))
            .cloned()
            .collect();
        for path in gone {
            if let Some(id) = self.known.remove(&path) {
                hider.follow(&supervisor.link_gone(id, now));
            }
        }

        for path in present {
            if self.known.contains_key(&path) {
                continue;
            }
            // Retried next scan on failure, for example while another app holds it.
            match DualSenseUsbLink::open_path(api, &path) {
                Ok((link, mac)) => {
                    hider.link_seen(mac, &link);
                    let (id, changes) = supervisor.link_arrived(mac, Box::new(link), now);
                    self.known.insert(path, id);
                    hider.follow(&changes);
                }
                Err(error) => eprintln!("heka: cannot open a DualSense: {error}"),
            }
        }
        Ok(())
    }
}
