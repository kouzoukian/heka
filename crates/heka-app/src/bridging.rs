//! Runs the Bridge lifecycle for every DualSense on USB.
//!
//! One supervisor thread owns the [`Supervisor`]: it connects to ViGEmBus, finds
//! Controllers as they come and go, carries out the player's Stop and Start, and
//! publishes a snapshot for the window. The window never waits on the thread.

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use heka_core::lifecycle::Change;
use heka_core::mac::MacAddress;
use heka_core::supervisor::{ControllerView, LinkId, Supervisor};
use heka_windows::hid::{DualSenseUsbLink, dualsense_usb_paths};
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
}

enum Command {
    Stop(MacAddress),
    Start(MacAddress),
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

    /// Removes every Virtual Controller and waits until they are gone.
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

/// Where per-Controller side effects of the lifecycle attach. Hiding (ticket #5)
/// hides on `Bridged` and `Returned` and restores on `Removed`.
fn after_changes(changes: &[Change]) {
    for change in changes {
        eprintln!("heka: {change:?}");
    }
}

fn supervise(inbox: &mpsc::Receiver<Command>, snapshot: &Mutex<Snapshot>) {
    let publish = |status: Status, controllers: Vec<ControllerView>| {
        *snapshot.lock().unwrap_or_else(PoisonError::into_inner) = Snapshot {
            status,
            controllers,
        };
    };

    let bus = loop {
        match VigemBus::connect() {
            Ok(bus) => break bus,
            Err(error) => publish(Status::NoBus(error.to_string()), Vec::new()),
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
            Err(RecvTimeoutError::Timeout) => Vec::new(),
        };
        if matches!(command, Ok(Command::Stop(_) | Command::Start(_))) {
            settings_file::save(supervisor.settings());
        }
        after_changes(&changes);

        let status = match discovery.scan(&mut supervisor, now) {
            Ok(()) => Status::Running,
            Err(error) => Status::HidError(error),
        };
        after_changes(&supervisor.poll(now));
        publish(status, supervisor.controllers());
    }
    after_changes(&supervisor.quit());
    publish(Status::Starting, Vec::new());
    // Dropping the Supervisor drops the bus last, which removes anything left.
}

/// Finds DualSense Links by HID path and reports arrivals and departures.
#[derive(Default)]
struct Discovery {
    api: Option<HidApi>,
    known: HashMap<CString, LinkId>,
}

impl Discovery {
    fn scan(&mut self, supervisor: &mut Supervisor, now: Instant) -> Result<(), String> {
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
                after_changes(&supervisor.link_gone(id, now));
            }
        }

        for path in present {
            if self.known.contains_key(&path) {
                continue;
            }
            // Retried next scan on failure, for example while another app holds it.
            match DualSenseUsbLink::open_path(api, &path) {
                Ok((link, mac)) => {
                    let (id, changes) = supervisor.link_arrived(mac, Box::new(link), now);
                    self.known.insert(path, id);
                    after_changes(&changes);
                }
                Err(error) => eprintln!("heka: cannot open a DualSense: {error}"),
            }
        }
        Ok(())
    }
}
