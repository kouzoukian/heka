//! Keeps one DualSense on USB Bridged to an Xbox Virtual Controller.
//!
//! A small supervisor thread connects to ViGEmBus, waits for a DualSense, starts a
//! Bridge, and starts a new one after the Controller is plugged back in. Ticket #6
//! replaces this with the multi-Controller lifecycle.
//!
//! While Bridged the Controller is Hidden with HidHide; it is restored when the Bridge
//! ends and on stop, and a start after a crash restores what the crashed run Hid.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use heka_core::bridge::{Bridge, LiveInput};
use heka_core::hiding::{DeviceInstancePath, Hiding};
use heka_core::virtual_controller::{OutputType, VirtualBus};
use heka_windows::hid::DualSenseUsbLink;
use heka_windows::hidhide;
use heka_windows::vigem::VigemBus;
use hidapi::HidApi;

/// How often the supervisor looks for a Controller or a dead Bridge.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// What the supervisor is doing, for the window to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Starting,
    /// ViGEmBus could not be opened. Retried every poll.
    NoBus(String),
    /// HID enumeration or opening the Controller failed. Retried every poll.
    HidError(String),
    /// ViGEmBus is open, no DualSense is on USB.
    NoController,
    Bridged,
}

pub struct Bridging {
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
    live: Arc<LiveInput>,
    status: Arc<Mutex<Status>>,
    hiding_note: Arc<Mutex<Option<String>>>,
}

impl Bridging {
    pub fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let live = Arc::new(LiveInput::default());
        let status = Arc::new(Mutex::new(Status::Starting));
        let hiding_note = Arc::new(Mutex::new(None));
        let thread = thread::Builder::new()
            .name("heka-supervisor".into())
            .spawn({
                let (stop, live, status, note) = (
                    stop.clone(),
                    live.clone(),
                    status.clone(),
                    hiding_note.clone(),
                );
                move || supervise(&stop, &live, &status, &note)
            })
            .expect("failed to start the supervisor thread");
        Self {
            stop,
            thread: Mutex::new(Some(thread)),
            live,
            status,
            hiding_note,
        }
    }

    /// Why the Controller is not Hidden, when it should be.
    pub fn hiding_note(&self) -> Option<String> {
        self.hiding_note
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn live(&self) -> &LiveInput {
        &self.live
    }

    pub fn status(&self) -> Status {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Stops the Bridge and waits until its Virtual Controller is removed.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
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
        self.stop();
    }
}

fn supervise(
    stop: &AtomicBool,
    live: &Arc<LiveInput>,
    status: &Mutex<Status>,
    hiding_note: &Mutex<Option<String>>,
) {
    let set_status = |new: Status| *status.lock().unwrap_or_else(PoisonError::into_inner) = new;

    // Undo whatever a killed run left Hidden before anything else.
    let hiding = match hidhide::hiding() {
        Ok(hiding) => Some(hiding),
        Err(error) => {
            *hiding_note.lock().unwrap_or_else(PoisonError::into_inner) =
                Some(format!("Controllers cannot be Hidden: {error}"));
            None
        }
    };
    let hiding = HidingStep {
        hiding: hiding.as_ref(),
        note: hiding_note,
    };
    if let Some(Err(error)) = hiding.hiding.map(Hiding::recover) {
        hiding.set_note(Some(format!(
            "Could not restore Controllers Hidden by an earlier run: {error}"
        )));
    }

    let bus = loop {
        match VigemBus::connect() {
            Ok(bus) => break bus,
            Err(error) => set_status(Status::NoBus(error.to_string())),
        }
        if !sleep_unless_stopped(stop) {
            return;
        }
    };

    let mut api: Option<HidApi> = None;
    let mut bridge: Option<Bridge> = None;
    let mut device = None;
    let mut hidden = false;
    while !stop.load(Ordering::Relaxed) {
        if bridge.as_ref().is_none_or(Bridge::is_finished) {
            if let Some(ended) = bridge.take() {
                eprintln!("heka: Bridge ended: {:?}", ended.stop());
                hiding.release();
            }
            match find_and_bridge(&mut api, &bus, live) {
                Ok(Some((started, path))) => {
                    bridge = Some(started);
                    device = Some(path);
                    hidden = false;
                    set_status(Status::Bridged);
                }
                Ok(None) => set_status(Status::NoController),
                Err(error) => set_status(Status::HidError(error)),
            }
        }
        // Retried every poll, for example while HidHide's own client holds the driver.
        if let (Some(_), Some(path), false) = (&bridge, &device, hidden) {
            hidden = hiding.hide(path);
        }
        if !sleep_unless_stopped(stop) {
            break;
        }
    }
    if let Some(bridge) = bridge {
        bridge.stop();
    }
    hiding.restore_all();
    // Dropping the bus last removes anything still plugged in.
    drop(bus);
}

type Bridged = (Bridge, std::io::Result<DeviceInstancePath>);

fn find_and_bridge(
    api: &mut Option<HidApi>,
    bus: &VigemBus,
    live: &Arc<LiveInput>,
) -> Result<Option<Bridged>, String> {
    let api = match api {
        Some(api) => {
            api.refresh_devices().map_err(|e| e.to_string())?;
            api
        }
        None => api.insert(HidApi::new().map_err(|e| e.to_string())?),
    };
    let Some(link) = DualSenseUsbLink::open_first(api).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let device = hidhide::device_instance_path(link.hid_path());
    let controller = bus.plug(OutputType::Xbox).map_err(|e| e.to_string())?;
    let bridge =
        Bridge::start(Box::new(link), controller, Arc::clone(live)).map_err(|e| e.to_string())?;
    Ok(Some((bridge, device)))
}

/// The one Controller this supervisor Bridges. Ticket #6 keys Controllers properly.
const OWNER: &str = "dualsense-usb";

/// Hides the Bridged Controller and says why it could not, for the window.
struct HidingStep<'a> {
    hiding: Option<&'a Hiding>,
    note: &'a Mutex<Option<String>>,
}

impl HidingStep<'_> {
    fn set_note(&self, note: Option<String>) {
        *self.note.lock().unwrap_or_else(PoisonError::into_inner) = note;
    }

    /// True once the Controller is Hidden, so the caller can stop retrying.
    fn hide(&self, device: &std::io::Result<DeviceInstancePath>) -> bool {
        let Some(hiding) = self.hiding else {
            return true;
        };
        let result = match device {
            Ok(device) => hiding.hide(OWNER, device).map_err(|e| e.to_string()),
            Err(error) => Err(format!(
                "cannot identify the Controller for HidHide: {error}"
            )),
        };
        match result {
            Ok(()) => {
                self.set_note(None);
                true
            }
            Err(error) => {
                self.set_note(Some(format!(
                    "{error}. Games may see the Controller twice."
                )));
                false
            }
        }
    }

    fn release(&self) {
        if let Some(Err(error)) = self.hiding.map(|hiding| hiding.release(OWNER)) {
            self.set_note(Some(format!("Could not restore the Controller: {error}")));
        }
    }

    fn restore_all(&self) {
        if let Some(Err(error)) = self.hiding.map(Hiding::restore_all) {
            eprintln!("heka: could not restore HidHide: {error}");
        }
    }
}

/// Sleeps one poll interval in short steps. False if a stop was requested.
fn sleep_unless_stopped(stop: &AtomicBool) -> bool {
    let until = Instant::now() + POLL_INTERVAL;
    while Instant::now() < until {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        thread::sleep(Duration::from_millis(20));
    }
    !stop.load(Ordering::Relaxed)
}
