//! A Bridge: one Controller's Link driving one Virtual Controller on its own input thread.
//!
//! The input thread reads the Link, parses each report, updates the Virtual Controller
//! and publishes the state to [`LiveInput`] for the UI to sample. The UI is never in the
//! input path.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::feedback::{Feedback, FeedbackEvent, FeedbackSource};
use crate::link::{LinkRead, LinkReader, LinkWriter};
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

/// Why a Bridge's input thread ended.
#[derive(Debug)]
pub enum BridgeEnd {
    /// [`Bridge::stop`] was called, or the Bridge was dropped.
    Stopped,
    /// The Controller lost its Link.
    LinkLost,
    /// The Virtual Controller refused an update.
    VirtualControllerFailed(VirtualError),
}

/// A running Bridge. Stopping or dropping it removes its Virtual Controller.
pub struct Bridge {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<BridgeEnd>>,
    feedback: Option<JoinHandle<()>>,
}

impl Bridge {
    /// Starts the input thread. Until it ends, it owns the Link and the Virtual Controller.
    ///
    /// When both carry Feedback, a Feedback thread passes it from the Virtual
    /// Controller to the Link, so a slow write never holds up input. It stops the
    /// rumble when the Bridge ends.
    ///
    /// When the Link is lost the Virtual Controller is released, then removed.
    pub fn start(
        mut link: Box<dyn LinkReader>,
        mut controller: Box<dyn VirtualController>,
        live: Arc<LiveInput>,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let feedback = match (controller.feedback(), link.feedback_writer()) {
            (Some(source), Some(writer)) => {
                Some(thread::Builder::new().name("heka-feedback".into()).spawn({
                    let stop = Arc::clone(&stop);
                    move || run_feedback(source, writer, &stop)
                })?)
            }
            _ => None,
        };
        let thread = thread::Builder::new().name("heka-bridge".into()).spawn({
            let stop = Arc::clone(&stop);
            move || {
                let end = run(link, controller, &live, &stop);
                // Ends the Feedback thread too when the input thread ends on its own.
                stop.store(true, Ordering::Relaxed);
                end
            }
        });
        let thread = match thread {
            Ok(thread) => thread,
            Err(error) => {
                stop.store(true, Ordering::Relaxed);
                if let Some(feedback) = feedback {
                    let _ = feedback.join();
                }
                return Err(error);
            }
        };
        Ok(Self {
            stop,
            thread: Some(thread),
            feedback,
        })
    }

    /// True once the input thread has ended on its own, for example on a lost Link.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Ends the input thread, waits for the Virtual Controller to be removed,
    /// and says why the Bridge ended.
    pub fn stop(mut self) -> BridgeEnd {
        self.join()
    }

    fn join(&mut self) -> BridgeEnd {
        self.stop.store(true, Ordering::Relaxed);
        let feedback = self.feedback.take().map(JoinHandle::join);
        let end = match self.thread.take().map(JoinHandle::join) {
            Some(Ok(end)) => end,
            Some(Err(panic)) => std::panic::resume_unwind(panic),
            None => BridgeEnd::Stopped,
        };
        if let Some(Err(panic)) = feedback {
            std::panic::resume_unwind(panic);
        }
        end
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        if self.thread.is_some() && !thread::panicking() {
            self.join();
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
            break BridgeEnd::Stopped;
        }
        match link.read(&mut buf) {
            LinkRead::Report(len) => {
                // Other reports on the same Link (or a short read) carry no input.
                let Ok(state) = format.parse(&buf[..len.min(buf.len())]) else {
                    continue;
                };
                live.set(Some(state));
                if let Err(error) = controller.update(&state) {
                    break BridgeEnd::VirtualControllerFailed(error);
                }
            }
            LinkRead::Timeout => {}
            LinkRead::Lost => {
                // Best effort: the Virtual Controller is removed next anyway.
                let _ = controller.update(&ControllerState::NEUTRAL);
                break BridgeEnd::LinkLost;
            }
        }
    };
    live.set(None);
    // Nothing more can be done if removal fails; ViGEmBus also removes the
    // Virtual Controller when heka's bus handle closes.
    let _ = controller.unplug();
    end
}

/// How long the Feedback thread waits for Feedback before checking for a stop.
const FEEDBACK_POLL: Duration = Duration::from_millis(50);

fn run_feedback(
    mut source: Box<dyn FeedbackSource>,
    mut writer: Box<dyn LinkWriter>,
    stop: &AtomicBool,
) {
    let format = writer.format();
    // What the Controller shows. It is at rest until heka first sends Feedback,
    // so a Bridge no game rumbles never sends an output report.
    let mut shown = Feedback::OFF;
    let mut show = |feedback: Feedback| {
        // Kept unchanged on a failed write, so the next Feedback is sent again.
        if feedback != shown && writer.write(&format.report(&feedback)).is_ok() {
            shown = feedback;
        }
    };
    while !stop.load(Ordering::Relaxed) {
        match source.next(FEEDBACK_POLL) {
            FeedbackEvent::Feedback(feedback) => show(feedback),
            FeedbackEvent::Timeout => {}
            FeedbackEvent::Closed => break,
        }
    }
    // The Bridge is ending, or the Virtual Controller is gone: nothing will stop
    // the motors after this. On a lost Link the write fails, which is harmless.
    show(Feedback::OFF);
}
