//! A Bridge: one Controller's Link driving one Virtual Controller on its own input thread.
//!
//! The input thread reads the Link, parses each report, updates the Virtual Controller
//! and publishes the state to [`LiveInput`] for the UI to sample. The UI is never in the
//! input path.
//!
//! When the Bridge ends it hands back what is still usable, so that the caller
//! decides what happens next: a Held Virtual Controller is kept for the
//! Controller's return, a Stopped Controller's Link is kept open. Feedback travels
//! with them: [`OpenLink`] keeps the Link's Feedback writer and [`Plugged`] keeps
//! the Virtual Controller's Feedback source, so a later Bridge carries rumble too.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::feedback::{Feedback, FeedbackEvent, FeedbackSource};
use crate::link::{LinkRead, LinkReader, LinkWriter, ReportFormat};
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
    /// The state the Bridge (or [`crate::watch::Watch`]) last read, or `None` when
    /// none is running.
    pub fn latest(&self) -> Option<ControllerState> {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set(&self, state: Option<ControllerState>) {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = state;
    }
}

/// An open Link, with the way to send it Feedback if it has one.
pub struct OpenLink {
    reader: Box<dyn LinkReader>,
    writer: Option<Box<dyn LinkWriter>>,
}

impl OpenLink {
    pub fn new(mut reader: Box<dyn LinkReader>) -> Self {
        let writer = reader.feedback_writer();
        Self { reader, writer }
    }

    pub fn format(&self) -> ReportFormat {
        self.reader.format()
    }
}

/// A plugged-in Virtual Controller, with the Feedback games send it if it carries any.
pub struct Plugged {
    controller: Box<dyn VirtualController>,
    feedback: Option<Box<dyn FeedbackSource>>,
}

impl Plugged {
    pub fn new(mut controller: Box<dyn VirtualController>) -> Self {
        let feedback = controller.feedback();
        Self {
            controller,
            feedback,
        }
    }

    /// Shows everything released and sticks centred.
    pub fn release(&mut self) -> Result<(), VirtualError> {
        self.controller.update(&ControllerState::NEUTRAL)
    }

    /// Removes the Virtual Controller from the system.
    pub fn unplug(self) -> Result<(), VirtualError> {
        self.controller.unplug()
    }
}

/// Why a Bridge ended, with what it hands back.
pub enum BridgeEnd {
    /// [`Bridge::stop`] was called. Both ends are still live; the Virtual
    /// Controller shows the last state sent.
    Stopped { link: OpenLink, controller: Plugged },
    /// The Controller lost its Link. The Virtual Controller has been sent
    /// everything released and sticks centred, and stays plugged in.
    LinkLost { controller: Plugged },
    /// The Virtual Controller refused an update and has been removed.
    VirtualControllerFailed { link: OpenLink, error: VirtualError },
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

/// How the input thread ended, with the parts it owned.
enum InputEnd {
    Stopped(Box<dyn LinkReader>, Box<dyn VirtualController>),
    LinkLost(Box<dyn VirtualController>),
    Failed(Box<dyn LinkReader>, VirtualError),
}

type FeedbackParts = (Box<dyn FeedbackSource>, Box<dyn LinkWriter>);

/// A running Bridge. Dropping it, rather than calling [`Bridge::stop`], also
/// removes its Virtual Controller.
pub struct Bridge {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<InputEnd>>,
    feedback: Option<JoinHandle<FeedbackParts>>,
    /// Feedback halves with no counterpart, kept for whatever follows this Bridge.
    spare_source: Option<Box<dyn FeedbackSource>>,
    spare_writer: Option<Box<dyn LinkWriter>>,
}

impl Bridge {
    /// Starts a Bridge from a newly opened Link and a newly plugged Virtual Controller.
    pub fn start(
        link: Box<dyn LinkReader>,
        controller: Box<dyn VirtualController>,
        live: Arc<LiveInput>,
    ) -> std::io::Result<Self> {
        Self::resume(OpenLink::new(link), Plugged::new(controller), live)
    }

    /// Starts the input thread. Until it ends, it owns the Link and the Virtual Controller.
    ///
    /// When both carry Feedback, a Feedback thread passes it from the Virtual
    /// Controller to the Link, so a slow write never holds up input. It stops the
    /// rumble when the Bridge ends.
    pub fn resume(
        link: OpenLink,
        controller: Plugged,
        live: Arc<LiveInput>,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let (mut spare_source, mut spare_writer) = (None, None);
        let feedback = match (controller.feedback, link.writer) {
            (Some(source), Some(writer)) => {
                Some(thread::Builder::new().name("heka-feedback".into()).spawn({
                    let stop = Arc::clone(&stop);
                    move || run_feedback(source, writer, &stop)
                })?)
            }
            (source, writer) => {
                spare_source = source;
                spare_writer = writer;
                None
            }
        };
        let (reader, controller) = (link.reader, controller.controller);
        let thread = thread::Builder::new().name("heka-bridge".into()).spawn({
            let stop = Arc::clone(&stop);
            move || {
                let end = run(reader, controller, &live, &stop);
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
            spare_source,
            spare_writer,
        })
    }

    /// True once the input thread has ended on its own, for example on a lost Link.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Ends the Bridge if it is still running and says why it ended.
    pub fn stop(mut self) -> BridgeEnd {
        self.join()
            .expect("a Bridge has its threads until stopped or dropped")
    }

    fn join(&mut self) -> Option<BridgeEnd> {
        self.stop.store(true, Ordering::Relaxed);
        let (source, writer) = match self.feedback.take().map(JoinHandle::join) {
            Some(Ok((source, writer))) => (Some(source), Some(writer)),
            Some(Err(panic)) => std::panic::resume_unwind(panic),
            None => (self.spare_source.take(), self.spare_writer.take()),
        };
        let end = match self.thread.take()?.join() {
            Ok(end) => end,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        let plugged = |controller| Plugged {
            controller,
            feedback: source,
        };
        Some(match end {
            InputEnd::Stopped(reader, controller) => BridgeEnd::Stopped {
                link: OpenLink { reader, writer },
                controller: plugged(controller),
            },
            InputEnd::LinkLost(controller) => BridgeEnd::LinkLost {
                controller: plugged(controller),
            },
            InputEnd::Failed(reader, error) => BridgeEnd::VirtualControllerFailed {
                link: OpenLink { reader, writer },
                error,
            },
        })
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
) -> InputEnd {
    let format = link.format();
    let mut buf = [0u8; READ_BUFFER_LEN];
    let end = loop {
        if stop.load(Ordering::Relaxed) {
            break InputEnd::Stopped(link, controller);
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
                    break InputEnd::Failed(link, error);
                }
            }
            LinkRead::Timeout => {}
            LinkRead::Lost => {
                // Best effort: if the release fails, the next Bridge or removal still follows.
                let _ = controller.update(&ControllerState::NEUTRAL);
                break InputEnd::LinkLost(controller);
            }
        }
    };
    live.set(None);
    end
}

/// How long the Feedback thread waits for Feedback before checking for a stop.
const FEEDBACK_POLL: Duration = Duration::from_millis(50);

/// Passes Feedback on until the Bridge ends, then hands both ends back.
fn run_feedback(
    mut source: Box<dyn FeedbackSource>,
    mut writer: Box<dyn LinkWriter>,
    stop: &AtomicBool,
) -> FeedbackParts {
    let format = writer.format();
    // What the Controller shows. It is at rest until heka first sends Feedback,
    // so a Bridge no game rumbles never sends an output report.
    let mut shown = Feedback::OFF;
    let mut show = |writer: &mut Box<dyn LinkWriter>, feedback: Feedback| {
        // Kept unchanged on a failed write, so the next Feedback is sent again.
        if feedback != shown && writer.write(&format.report(&feedback)).is_ok() {
            shown = feedback;
        }
    };
    while !stop.load(Ordering::Relaxed) {
        match source.next(FEEDBACK_POLL) {
            FeedbackEvent::Feedback(feedback) => show(&mut writer, feedback),
            FeedbackEvent::Timeout => {}
            FeedbackEvent::Closed => break,
        }
    }
    // The Bridge is ending, or the Virtual Controller is gone: nothing will stop
    // the motors after this. On a lost Link the write fails, which is harmless.
    show(&mut writer, Feedback::OFF);
    (source, writer)
}
