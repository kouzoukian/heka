//! A Bridge: one Controller's Link driving one Virtual Controller on its own input thread.
//!
//! The input thread reads the Link, parses each report, updates the Virtual Controller
//! and publishes the state to [`LiveInput`] for the UI to sample. The UI is never in the
//! input path.
//!
//! When the input thread ends it hands back what is still usable, so that the
//! caller decides what happens next: a Held Virtual Controller is kept for the
//! Controller's return, a Stopped Controller's Link is kept open.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use crate::link::{LinkRead, LinkReader};
use crate::state::ControllerState;
use crate::virtual_controller::{VirtualController, VirtualError};

/// Larger than any input report heka reads (DualSense over Bluetooth is 78 bytes).
const READ_BUFFER_LEN: usize = 128;

/// The latest Controller state of a running Bridge, for the UI to sample.
#[derive(Debug, Default)]
pub struct LiveInput {
    latest: Mutex<Option<ControllerState>>,
}

impl LiveInput {
    /// The state the Bridge last sent, or `None` when no Bridge is running.
    pub fn latest(&self) -> Option<ControllerState> {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set(&self, state: Option<ControllerState>) {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = state;
    }
}

/// Why a Bridge's input thread ended, with what it hands back.
pub enum BridgeEnd {
    /// [`Bridge::stop`] was called. Both ends are still live; the Virtual
    /// Controller shows the last state sent.
    Stopped {
        link: Box<dyn LinkReader>,
        controller: Box<dyn VirtualController>,
    },
    /// The Controller lost its Link. The Virtual Controller has been sent
    /// everything released and sticks centred, and stays plugged in.
    LinkLost {
        controller: Box<dyn VirtualController>,
    },
    /// The Virtual Controller refused an update and has been removed.
    VirtualControllerFailed {
        link: Box<dyn LinkReader>,
        error: VirtualError,
    },
}

impl fmt::Debug for BridgeEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped { .. } => f.write_str("Stopped"),
            Self::LinkLost { .. } => f.write_str("LinkLost"),
            Self::VirtualControllerFailed { error, .. } => {
                write!(f, "VirtualControllerFailed({error})")
            }
        }
    }
}

/// A running Bridge. Dropping it, rather than calling [`Bridge::stop`], also
/// removes its Virtual Controller.
pub struct Bridge {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<BridgeEnd>>,
}

impl Bridge {
    /// Starts the input thread. Until it ends, it owns the Link and the Virtual Controller.
    pub fn start(
        link: Box<dyn LinkReader>,
        controller: Box<dyn VirtualController>,
        live: Arc<LiveInput>,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread = thread::Builder::new().name("heka-bridge".into()).spawn({
            let stop = Arc::clone(&stop);
            move || run(link, controller, &live, &stop)
        })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }

    /// True once the input thread has ended on its own, for example on a lost Link.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Ends the input thread if it is still running and says why it ended.
    pub fn stop(mut self) -> BridgeEnd {
        self.join()
            .expect("a Bridge has its thread until stopped or dropped")
    }

    fn join(&mut self) -> Option<BridgeEnd> {
        self.stop.store(true, Ordering::Relaxed);
        match self.thread.take().map(JoinHandle::join) {
            Some(Ok(end)) => Some(end),
            Some(Err(panic)) => std::panic::resume_unwind(panic),
            None => None,
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        if self.thread.is_none() || thread::panicking() {
            return;
        }
        // Nothing more can be done if removal fails; ViGEmBus also removes the
        // Virtual Controller when heka's bus handle closes.
        match self.join() {
            Some(BridgeEnd::Stopped { controller, .. } | BridgeEnd::LinkLost { controller }) => {
                let _ = controller.unplug();
            }
            Some(BridgeEnd::VirtualControllerFailed { .. }) | None => {}
        }
    }
}

fn run(
    mut link: Box<dyn LinkReader>,
    mut controller: Box<dyn VirtualController>,
    live: &LiveInput,
    stop: &AtomicBool,
) -> BridgeEnd {
    let format = link.format();
    let mut buf = [0u8; READ_BUFFER_LEN];
    let end = loop {
        if stop.load(Ordering::Relaxed) {
            break BridgeEnd::Stopped { link, controller };
        }
        match link.read(&mut buf) {
            LinkRead::Report(len) => {
                // Other reports on the same Link (or a short read) carry no input.
                let Ok(state) = format.parse(&buf[..len.min(buf.len())]) else {
                    continue;
                };
                live.set(Some(state));
                if let Err(error) = controller.update(&state) {
                    // Best effort: it already failed once.
                    let _ = controller.unplug();
                    break BridgeEnd::VirtualControllerFailed { link, error };
                }
            }
            LinkRead::Timeout => {}
            LinkRead::Lost => {
                // Best effort: if the release fails, the next Bridge or removal still follows.
                let _ = controller.update(&ControllerState::NEUTRAL);
                break BridgeEnd::LinkLost { controller };
            }
        }
    };
    live.set(None);
    end
}
