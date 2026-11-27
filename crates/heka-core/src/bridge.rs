use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::desktop::Desk;
use crate::feedback::{
    Feedback, FeedbackEvent, FeedbackSource, Lightbar, Motor, PlayerLights, RUMBLE_TEST_TIME,
    Rumble,
};
use crate::idle::Activity;
use crate::link::{FeedbackFormat, LinkRead, LinkReader, LinkWriter, ReportFormat};
use crate::live_gyro::{FollowedGyro, LiveGyro};
use crate::live_mapper::LiveMapper;
use crate::live_shortcuts::LiveShortcuts;
use crate::preferences::{LightbarChoice, LivePreferences, Preferences};
use crate::rate::RateMeter;
use crate::running::RunningMapper;
use crate::shortcuts::ShortcutDetector;
use crate::state::ControllerState;
use crate::threads::{SystemThreads, Threads, Worker};
use crate::virtual_controller::{VirtualController, VirtualError, VirtualFailure, VirtualFailures};

const READ_BUFFER_LEN: usize = 128;

#[derive(Debug, Default)]
pub struct LiveInput {
    seen: Mutex<Seen>,
}

#[derive(Debug, Default)]
struct Seen {
    latest: Option<ControllerState>,
    raw: Option<ControllerState>,
    activity: Activity,
    times_used: u64,
    meter: RateMeter,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveSample {
    pub raw: Option<ControllerState>,
    pub shown: Option<ControllerState>,
    pub report_rate: Option<f32>,
}

impl LiveInput {
    pub fn latest(&self) -> Option<ControllerState> {
        self.seen().latest
    }

    pub fn latest_raw(&self) -> Option<ControllerState> {
        self.seen().raw
    }

    pub fn report_rate(&self) -> Option<f32> {
        self.seen().meter.per_second()
    }

    pub fn sample(&self) -> LiveSample {
        let seen = self.seen();
        LiveSample {
            raw: seen.raw,
            shown: seen.latest,
            report_rate: seen.meter.per_second(),
        }
    }

    pub fn times_used(&self) -> u64 {
        self.seen().times_used
    }

    fn show(&self, shown: ControllerState) {
        self.seen().latest = Some(shown);
    }

    fn set(&self, physical: Option<&ControllerState>, shown: Option<ControllerState>) {
        let mut seen = self.seen();
        match physical {
            Some(state) => {
                if seen.activity.observe(state) {
                    seen.times_used += 1;
                }
                seen.meter.tick(Instant::now());
            }
            None => seen.meter.reset(),
        }
        seen.raw = physical.copied();
        seen.latest = shown;
    }

    fn seen(&self) -> MutexGuard<'_, Seen> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
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
    no_feedback: Option<String>,
    asked: Feedback,
    preferences: Arc<LivePreferences>,
    mapper: Arc<LiveMapper>,
    shortcuts: Arc<LiveShortcuts>,
    desk: Arc<Desk>,
    gyro: Arc<LiveGyro>,
    failures: Arc<VirtualFailures>,
}

impl Plugged {
    pub fn new(mut controller: Box<dyn VirtualController>) -> Self {
        let (feedback, no_feedback) = match controller.feedback() {
            Ok(feedback) => (feedback, None),
            Err(error) => (None, Some(error.to_string())),
        };
        Self {
            controller,
            feedback,
            no_feedback,
            asked: Feedback::OFF,
            preferences: Arc::default(),
            mapper: Arc::default(),
            shortcuts: Arc::default(),
            desk: Arc::default(),
            gyro: Arc::default(),
            failures: Arc::default(),
        }
    }

    pub fn shortcuts_with(&mut self, shortcuts: Arc<LiveShortcuts>) {
        self.shortcuts = shortcuts;
    }

    pub fn follow(&mut self, preferences: Arc<LivePreferences>) {
        self.preferences = preferences;
    }

    pub fn map_with(&mut self, mapper: Arc<LiveMapper>) {
        self.mapper = mapper;
    }

    pub fn measure_with(&mut self, gyro: Arc<LiveGyro>) {
        self.gyro = gyro;
    }

    pub fn use_desk(&mut self, desk: Arc<Desk>) {
        self.desk = desk;
    }

    pub fn report_to(&mut self, failures: Arc<VirtualFailures>) {
        if let Some(error) = self.no_feedback.take() {
            failures.record(VirtualFailure::NoFeedback(error));
        }
        self.failures = failures;
    }

    pub fn release(&mut self) -> Result<(), VirtualError> {
        release(self.controller.as_mut(), &self.failures)
    }

    pub fn unplug(self) -> Result<(), VirtualError> {
        unplug(self.controller, &self.failures)
    }
}

fn release(
    controller: &mut dyn VirtualController,
    failures: &VirtualFailures,
) -> Result<(), VirtualError> {
    controller
        .update(&ControllerState::NEUTRAL)
        .inspect_err(|error| failures.record(VirtualFailure::NotReleased(error.to_string())))
}

fn unplug(
    controller: Box<dyn VirtualController>,
    failures: &VirtualFailures,
) -> Result<(), VirtualError> {
    controller
        .unplug()
        .inspect_err(|error| failures.record(VirtualFailure::NotUnplugged(error.to_string())))
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

pub struct StartError {
    pub error: std::io::Error,
    pub link: OpenLink,
    pub controller: Plugged,
}

impl fmt::Debug for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StartError({})", self.error)
    }
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}

impl std::error::Error for StartError {}

enum InputEnd {
    Stopped(Box<dyn LinkReader>, Box<dyn VirtualController>),
    LinkLost(Box<dyn VirtualController>),
    Failed(Box<dyn LinkReader>, VirtualError),
}

type InputParts = (Box<dyn LinkReader>, Box<dyn VirtualController>);

type FeedbackParts = (Box<dyn FeedbackSource>, Box<dyn LinkWriter>, Feedback);

type FeedbackEnd = (
    Option<Box<dyn FeedbackSource>>,
    Option<Box<dyn LinkWriter>>,
    Feedback,
);

#[derive(Clone, Copy)]
struct RumbleTest {
    motor: Motor,
    until: Instant,
}

#[derive(Default)]
struct Flags {
    stop: AtomicBool,
    link_lost: AtomicBool,
    rumble_test: Mutex<Option<RumbleTest>>,
}

impl Flags {
    fn tested_motor(&self) -> Option<Motor> {
        self.rumble_test()
            .filter(|test| Instant::now() < test.until)
            .map(|test| test.motor)
    }

    fn set_rumble_test(&self, test: Option<RumbleTest>) {
        *self.rumble_test() = test;
    }

    fn rumble_test(&self) -> MutexGuard<'_, Option<RumbleTest>> {
        self.rumble_test
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct Bridge {
    flags: Arc<Flags>,
    input: Option<Worker<InputEnd>>,
    feedback: Option<Worker<FeedbackParts>>,
    spare_source: Option<Box<dyn FeedbackSource>>,
    spare_writer: Option<Box<dyn LinkWriter>>,
    asked: Feedback,
    preferences: Arc<LivePreferences>,
    mapper: Arc<LiveMapper>,
    shortcuts: Arc<LiveShortcuts>,
    desk: Arc<Desk>,
    gyro: Arc<LiveGyro>,
    failures: Arc<VirtualFailures>,
}

impl Bridge {
    pub fn start(
        link: Box<dyn LinkReader>,
        controller: Box<dyn VirtualController>,
        live: Arc<LiveInput>,
    ) -> Result<Self, Box<StartError>> {
        Self::resume(OpenLink::new(link), Plugged::new(controller), live)
    }

    pub fn resume(
        link: OpenLink,
        controller: Plugged,
        live: Arc<LiveInput>,
    ) -> Result<Self, Box<StartError>> {
        Self::resume_on(&SystemThreads, link, controller, live)
    }

    pub fn resume_on(
        threads: &dyn Threads,
        link: OpenLink,
        controller: Plugged,
        live: Arc<LiveInput>,
    ) -> Result<Self, Box<StartError>> {
        let OpenLink { reader, writer } = link;
        let Plugged {
            controller,
            feedback: source,
            no_feedback: _,
            asked,
            preferences,
            mapper,
            shortcuts,
            desk,
            gyro,
            failures,
        } = controller;
        let flags = Arc::new(Flags::default());
        let mut bridge = Self {
            flags: Arc::clone(&flags),
            input: None,
            feedback: None,
            spare_source: None,
            spare_writer: None,
            asked,
            preferences: Arc::clone(&preferences),
            mapper: Arc::clone(&mapper),
            shortcuts: Arc::clone(&shortcuts),
            desk: Arc::clone(&desk),
            gyro: Arc::clone(&gyro),
            failures: Arc::clone(&failures),
        };
        match (source, writer) {
            (Some(source), Some(writer)) => {
                let started = Worker::spawn(threads, "heka-feedback", (source, writer), {
                    let flags = Arc::clone(&flags);
                    move |(source, writer)| {
                        run_feedback(source, writer, asked, &preferences, &flags)
                    }
                });
                match started {
                    Ok(worker) => bridge.feedback = Some(worker),
                    Err((error, (source, writer))) => {
                        bridge.spare_source = Some(source);
                        bridge.spare_writer = Some(writer);
                        return Err(bridge.hand_back(error, (reader, controller)));
                    }
                }
            }
            (source, writer) => {
                bridge.spare_source = source;
                bridge.spare_writer = writer;
            }
        }
        let shared = Shared {
            live,
            failures,
            mapper,
            shortcuts,
            desk,
            gyro,
        };
        let started = Worker::spawn(
            threads,
            "heka-bridge",
            (reader, controller),
            move |(reader, controller): InputParts| {
                let end = run(reader, controller, &flags.stop, shared);
                if matches!(end, InputEnd::LinkLost(_)) {
                    flags.link_lost.store(true, Ordering::Release);
                }
                flags.stop.store(true, Ordering::Release);
                end
            },
        );
        match started {
            Ok(worker) => {
                bridge.input = Some(worker);
                Ok(bridge)
            }
            Err((error, parts)) => Err(bridge.hand_back(error, parts)),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.input.as_ref().is_none_or(Worker::is_finished)
    }

    pub fn test_rumble(&self, motor: Motor) {
        let until = Instant::now() + RUMBLE_TEST_TIME;
        self.flags
            .set_rumble_test(Some(RumbleTest { motor, until }));
    }

    pub fn end_rumble_test(&self) {
        self.flags.set_rumble_test(None);
    }

    pub fn stop(mut self) -> BridgeEnd {
        self.join()
            .expect("a Bridge has its threads until stopped or dropped")
    }

    fn hand_back(
        mut self,
        error: std::io::Error,
        (reader, controller): InputParts,
    ) -> Box<StartError> {
        let (source, writer, asked) = self.end_feedback();
        Box::new(StartError {
            error,
            link: OpenLink { reader, writer },
            controller: self.plugged(controller, source, asked),
        })
    }

    fn end_feedback(&mut self) -> FeedbackEnd {
        self.flags.stop.store(true, Ordering::Release);
        match self.feedback.take().map(Worker::join) {
            Some((source, writer, asked)) => (Some(source), Some(writer), asked),
            None => (
                self.spare_source.take(),
                self.spare_writer.take(),
                self.asked,
            ),
        }
    }

    fn plugged(
        &self,
        controller: Box<dyn VirtualController>,
        feedback: Option<Box<dyn FeedbackSource>>,
        asked: Feedback,
    ) -> Plugged {
        Plugged {
            controller,
            feedback,
            no_feedback: None,
            asked,
            preferences: Arc::clone(&self.preferences),
            mapper: Arc::clone(&self.mapper),
            shortcuts: Arc::clone(&self.shortcuts),
            desk: Arc::clone(&self.desk),
            gyro: Arc::clone(&self.gyro),
            failures: Arc::clone(&self.failures),
        }
    }

    fn join(&mut self) -> Option<BridgeEnd> {
        let (source, writer, asked) = self.end_feedback();
        Some(match self.input.take()?.join() {
            InputEnd::Stopped(reader, controller) => BridgeEnd::Stopped {
                link: OpenLink { reader, writer },
                controller: self.plugged(controller, source, asked),
            },
            InputEnd::LinkLost(controller) => BridgeEnd::LinkLost {
                controller: self.plugged(controller, source, asked),
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
        if thread::panicking() {
            self.flags.stop.store(true, Ordering::Release);
            return;
        }
        if self.input.is_none() {
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

struct Shared {
    live: Arc<LiveInput>,
    failures: Arc<VirtualFailures>,
    mapper: Arc<LiveMapper>,
    shortcuts: Arc<LiveShortcuts>,
    desk: Arc<Desk>,
    gyro: Arc<LiveGyro>,
}

fn run(
    mut link: Box<dyn LinkReader>,
    mut controller: Box<dyn VirtualController>,
    stop: &AtomicBool,
    shared: Shared,
) -> InputEnd {
    let Shared {
        live,
        failures,
        mapper,
        shortcuts,
        desk,
        gyro,
    } = shared;
    let mut running = RunningMapper::new(desk);
    let mut gyro = FollowedGyro::new(gyro);
    running.set_drift(gyro.drift());
    let format = link.format();
    let mut mapper = mapper.follow();
    let mut configured = shortcuts.follow();
    let mut detector = ShortcutDetector::default();
    let mut buf = [0u8; READ_BUFFER_LEN];
    let mut latest = None;
    let end = loop {
        if stop.load(Ordering::Acquire) {
            break InputEnd::Stopped(link, controller);
        }
        let read = link.read(&mut buf);
        if mapper.refresh() {
            running.start_over(mapper.current(), latest.as_ref());
        }
        if let Some(drift) = gyro.changed_drift() {
            running.set_drift(drift);
        }
        match read {
            LinkRead::Report(len) => {
                let Ok(state) = format.parse(&buf[..len.min(buf.len())]) else {
                    continue;
                };
                let now = Instant::now();
                gyro.read(Some(&state.motion), now);
                let observed = detector.observe(configured.get(), state.buttons, now);
                if let Some(action) = observed.action {
                    shortcuts.push(action);
                }
                let mut passed_on = state;
                passed_on.buttons.set(observed.hide, false);
                latest = Some(passed_on);
                let shown = running.report(mapper.current(), &passed_on, now);
                live.set(Some(&state), Some(shown));
                if let Err(error) = controller.update(&shown) {
                    let _ = unplug(controller, &failures);
                    break InputEnd::Failed(link, error);
                }
                running.act();
            }
            LinkRead::Timeout => {
                let now = Instant::now();
                gyro.read(None, now);
                if let Some(shown) = running.tick(mapper.current(), now) {
                    live.show(shown);
                    if let Err(error) = controller.update(&shown) {
                        let _ = unplug(controller, &failures);
                        break InputEnd::Failed(link, error);
                    }
                }
                running.act();
            }
            LinkRead::Lost => {
                let _ = release(controller.as_mut(), &failures);
                break InputEnd::LinkLost(controller);
            }
        }
    };
    running.let_go();
    live.set(None, None);
    end
}

const FEEDBACK_POLL: Duration = Duration::from_millis(50);

const WRITE_RETRY_DELAY: Duration = Duration::from_millis(100);

const STOP_ATTEMPTS: u32 = 5;

const STOP_RETRY_DELAY: Duration = Duration::from_millis(20);

const STOP_TIME_LIMIT: Duration = Duration::from_millis(500);

struct Output {
    writer: Box<dyn LinkWriter>,
    format: FeedbackFormat,
    wanted: Feedback,
    shown: Option<Feedback>,
    retry_at: Instant,
}

impl Output {
    fn new(writer: Box<dyn LinkWriter>) -> Self {
        Self {
            format: writer.format(),
            writer,
            wanted: Feedback::OFF,
            shown: Some(Feedback::OFF),
            retry_at: Instant::now(),
        }
    }

    fn show(&mut self, feedback: Feedback) {
        let feedback = self.with_unwritten_lights(feedback);
        let waiting = self.shown.is_none() && feedback == self.wanted;
        self.wanted = feedback;
        if !waiting && !self.is_up_to_date() {
            self.write();
        }
    }

    fn retry(&mut self) {
        if self.shown.is_none() && Instant::now() >= self.retry_at {
            self.write();
        }
    }

    fn stop_motors(&mut self, link_lost: &AtomicBool) {
        self.wanted = self.with_unwritten_lights(Feedback::OFF);
        let started = Instant::now();
        for attempt in 1..=STOP_ATTEMPTS {
            if self.is_up_to_date() {
                return;
            }
            self.write();
            let give_up = attempt == STOP_ATTEMPTS
                || link_lost.load(Ordering::Acquire)
                || started.elapsed() >= STOP_TIME_LIMIT;
            if give_up || self.is_up_to_date() {
                return;
            }
            thread::sleep(STOP_RETRY_DELAY);
        }
    }

    fn is_up_to_date(&self) -> bool {
        self.shown == Some(self.wanted)
    }

    fn with_unwritten_lights(&self, mut feedback: Feedback) -> Feedback {
        if self.shown.is_none() {
            feedback.lightbar = feedback.lightbar.or(self.wanted.lightbar);
            feedback.player_lights = feedback.player_lights.or(self.wanted.player_lights);
        }
        feedback
    }

    fn write(&mut self) {
        let reports = self.format.reports(&self.wanted);
        let written = reports.iter().all(|report| {
            let written = self.writer.write(report).is_ok();
            if written {
                self.format.written(report);
            }
            written
        });
        self.shown = written.then_some(self.wanted);
        self.retry_at = Instant::now() + WRITE_RETRY_DELAY;
    }
}

fn shaped(
    preferences: Preferences,
    testing: Option<Motor>,
    asked: Feedback,
    elapsed: Duration,
) -> Feedback {
    let mut feedback = preferences.apply(asked, elapsed);
    if let Some(motor) = testing {
        feedback.rumble = preferences.rumble.scale(Rumble::only(motor));
    }
    feedback
}

fn run_feedback(
    mut source: Box<dyn FeedbackSource>,
    writer: Box<dyn LinkWriter>,
    mut asked: Feedback,
    preferences: &LivePreferences,
    flags: &Flags,
) -> FeedbackParts {
    let stopping = || flags.stop.load(Ordering::Acquire);
    let mut output = Output::new(writer);
    let started = Instant::now();
    let mut applied = preferences.get();
    let mut testing = None;
    output.show(shaped(applied, testing, asked, started.elapsed()));
    while !stopping() {
        let event = source.next(FEEDBACK_POLL);
        let motor = flags.tested_motor();
        let chosen = preferences.get();
        if chosen != applied || motor != testing {
            let mut next = shaped(chosen, motor, asked, started.elapsed());
            if applied.lightbar.is_some() {
                next.lightbar = next.lightbar.or(Some(Lightbar::DEFAULT));
            }
            if applied.player_lights.is_some() {
                next.player_lights = next.player_lights.or(Some(PlayerLights::Off));
            }
            applied = chosen;
            testing = motor;
            output.show(next);
        }
        match event {
            FeedbackEvent::Feedback(feedback) => {
                asked = feedback;
                output.show(shaped(applied, testing, asked, started.elapsed()));
            }
            FeedbackEvent::Timeout
                if applied.lightbar.is_some_and(LightbarChoice::moves)
                    && output.shown.is_some() =>
            {
                output.show(shaped(applied, testing, asked, started.elapsed()));
            }
            FeedbackEvent::Timeout => {}
            FeedbackEvent::Closed => {
                output.show(Feedback::OFF);
                while !output.is_up_to_date() && !stopping() {
                    thread::sleep(FEEDBACK_POLL);
                    output.retry();
                }
                break;
            }
        }
        output.retry();
    }
    output.stop_motors(&flags.link_lost);
    (source, output.writer, asked)
}
