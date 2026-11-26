use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::feedback::{Feedback, FeedbackEvent, FeedbackSource, Lightbar, PlayerLights};
use crate::link::{FeedbackFormat, LinkRead, LinkReader, LinkWriter, ReportFormat};
use crate::preferences::LivePreferences;
use crate::state::ControllerState;
use crate::virtual_controller::{VirtualController, VirtualError};

const READ_BUFFER_LEN: usize = 128;

#[derive(Debug, Default)]
pub struct LiveInput {
    latest: Mutex<Option<ControllerState>>,
}

impl LiveInput {
    pub fn latest(&self) -> Option<ControllerState> {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set(&self, state: Option<ControllerState>) {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = state;
    }
}

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

pub struct Plugged {
    controller: Box<dyn VirtualController>,
    feedback: Option<Box<dyn FeedbackSource>>,
    asked: Feedback,
    preferences: Arc<LivePreferences>,
}

impl Plugged {
    pub fn new(mut controller: Box<dyn VirtualController>) -> Self {
        let feedback = controller.feedback();
        Self {
            controller,
            feedback,
            asked: Feedback::OFF,
            preferences: Arc::default(),
        }
    }

    pub fn follow(&mut self, preferences: Arc<LivePreferences>) {
        self.preferences = preferences;
    }

    pub fn release(&mut self) -> Result<(), VirtualError> {
        self.controller.update(&ControllerState::NEUTRAL)
    }

    pub fn unplug(self) -> Result<(), VirtualError> {
        self.controller.unplug()
    }
}

pub enum BridgeEnd {
    Stopped { link: OpenLink, controller: Plugged },
    LinkLost { controller: Plugged },
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

enum InputEnd {
    Stopped(Box<dyn LinkReader>, Box<dyn VirtualController>),
    LinkLost(Box<dyn VirtualController>),
    Failed(Box<dyn LinkReader>, VirtualError),
}

type FeedbackParts = (Box<dyn FeedbackSource>, Box<dyn LinkWriter>, Feedback);

pub struct Bridge {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<InputEnd>>,
    feedback: Option<JoinHandle<FeedbackParts>>,
    spare_source: Option<Box<dyn FeedbackSource>>,
    spare_writer: Option<Box<dyn LinkWriter>>,
    asked: Feedback,
    preferences: Arc<LivePreferences>,
}

impl Bridge {
    pub fn start(
        link: Box<dyn LinkReader>,
        controller: Box<dyn VirtualController>,
        live: Arc<LiveInput>,
    ) -> std::io::Result<Self> {
        Self::resume(OpenLink::new(link), Plugged::new(controller), live)
    }

    pub fn resume(
        link: OpenLink,
        controller: Plugged,
        live: Arc<LiveInput>,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let (mut spare_source, mut spare_writer) = (None, None);
        let asked = controller.asked;
        let preferences = controller.preferences;
        let feedback = match (controller.feedback, link.writer) {
            (Some(source), Some(writer)) => {
                Some(thread::Builder::new().name("heka-feedback".into()).spawn({
                    let stop = Arc::clone(&stop);
                    let preferences = Arc::clone(&preferences);
                    move || run_feedback(source, writer, asked, &preferences, &stop)
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
            asked,
            preferences,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn stop(mut self) -> BridgeEnd {
        self.join()
            .expect("a Bridge has its threads until stopped or dropped")
    }

    fn join(&mut self) -> Option<BridgeEnd> {
        self.stop.store(true, Ordering::Relaxed);
        let (source, writer, asked) = match self.feedback.take().map(JoinHandle::join) {
            Some(Ok((source, writer, asked))) => (Some(source), Some(writer), asked),
            Some(Err(panic)) => std::panic::resume_unwind(panic),
            None => (
                self.spare_source.take(),
                self.spare_writer.take(),
                self.asked,
            ),
        };
        let end = match self.thread.take()?.join() {
            Ok(end) => end,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        let preferences = Arc::clone(&self.preferences);
        let plugged = |controller| Plugged {
            controller,
            feedback: source,
            asked,
            preferences,
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
                let Ok(state) = format.parse(&buf[..len.min(buf.len())]) else {
                    continue;
                };
                live.set(Some(state));
                if let Err(error) = controller.update(&state) {
                    let _ = controller.unplug();
                    break InputEnd::Failed(link, error);
                }
            }
            LinkRead::Timeout => {}
            LinkRead::Lost => {
                let _ = controller.update(&ControllerState::NEUTRAL);
                break InputEnd::LinkLost(controller);
            }
        }
    };
    live.set(None);
    end
}

const FEEDBACK_POLL: Duration = Duration::from_millis(50);

struct Output {
    writer: Box<dyn LinkWriter>,
    format: FeedbackFormat,
    shown: Feedback,
}

impl Output {
    fn show(&mut self, feedback: Feedback) {
        if feedback != self.shown
            && self
                .format
                .reports(&feedback)
                .iter()
                .all(|report| self.writer.write(report).is_ok())
        {
            self.shown = feedback;
        }
    }
}

fn run_feedback(
    mut source: Box<dyn FeedbackSource>,
    writer: Box<dyn LinkWriter>,
    mut asked: Feedback,
    preferences: &LivePreferences,
    stop: &AtomicBool,
) -> FeedbackParts {
    let mut output = Output {
        format: writer.format(),
        writer,
        shown: Feedback::OFF,
    };
    let mut applied = preferences.get();
    output.show(applied.apply(asked));
    while !stop.load(Ordering::Relaxed) {
        let event = source.next(FEEDBACK_POLL);
        let chosen = preferences.get();
        if chosen != applied {
            let mut next = chosen.apply(asked);
            if applied.lightbar.is_some() {
                next.lightbar = next.lightbar.or(Some(Lightbar::DEFAULT));
            }
            if applied.player_lights.is_some() {
                next.player_lights = next.player_lights.or(Some(PlayerLights::Off));
            }
            applied = chosen;
            output.show(next);
        }
        match event {
            FeedbackEvent::Feedback(feedback) => {
                asked = feedback;
                output.show(applied.apply(feedback));
            }
            FeedbackEvent::Timeout => {}
            FeedbackEvent::Closed => break,
        }
    }
    output.show(Feedback::OFF);
    (source, output.writer, asked)
}
