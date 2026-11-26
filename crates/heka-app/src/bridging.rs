//! Keeps one DualSense on USB Bridged to an Xbox Virtual Controller.
//!
//! A small supervisor thread connects to ViGEmBus, waits for a DualSense, starts a
//! Bridge, and starts a new one after the Controller is plugged back in. Ticket #6
//! replaces this with the multi-Controller lifecycle.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use heka_core::bridge::{Bridge, LiveInput};
use heka_core::virtual_controller::{OutputType, VirtualBus};
use heka_windows::hid::DualSenseUsbLink;
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
}

impl Bridging {
    pub fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let live = Arc::new(LiveInput::default());
        let status = Arc::new(Mutex::new(Status::Starting));
        let thread = thread::Builder::new()
            .name("heka-supervisor".into())
            .spawn({
                let (stop, live, status) = (stop.clone(), live.clone(), status.clone());
                move || supervise(&stop, &live, &status)
            })
            .expect("failed to start the supervisor thread");
        Self {
            stop,
            thread: Mutex::new(Some(thread)),
            live,
            status,
        }
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

fn supervise(stop: &AtomicBool, live: &Arc<LiveInput>, status: &Mutex<Status>) {
    let set_status = |new: Status| *status.lock().unwrap_or_else(PoisonError::into_inner) = new;

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
    while !stop.load(Ordering::Relaxed) {
        if bridge.as_ref().is_none_or(Bridge::is_finished) {
            if let Some(ended) = bridge.take() {
                eprintln!("heka: Bridge ended: {:?}", ended.stop());
            }
            match find_and_bridge(&mut api, &bus, live) {
                Ok(Some(started)) => {
                    bridge = Some(started);
                    set_status(Status::Bridged);
                }
                Ok(None) => set_status(Status::NoController),
                Err(error) => set_status(Status::HidError(error)),
            }
        }
        if !sleep_unless_stopped(stop) {
            break;
        }
    }
    if let Some(bridge) = bridge {
        bridge.stop();
    }
    // Dropping the bus last removes anything still plugged in.
    drop(bus);
}

fn find_and_bridge(
    api: &mut Option<HidApi>,
    bus: &VigemBus,
    live: &Arc<LiveInput>,
) -> Result<Option<Bridge>, String> {
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
    let controller = bus.plug(OutputType::Xbox).map_err(|e| e.to_string())?;
    let bridge =
        Bridge::start(Box::new(link), controller, Arc::clone(live)).map_err(|e| e.to_string())?;
    Ok(Some(bridge))
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
