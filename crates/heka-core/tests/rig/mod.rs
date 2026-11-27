#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use heka_core::desktop::{Desk, DesktopError, DesktopEvent, DesktopOutput};
use heka_core::profile::{Mapper, Profile};
use heka_core::running::RunningMapper;
use heka_core::state::ControllerState;

#[derive(Clone, Default)]
struct FakeDesktop(Arc<Mutex<Vec<DesktopEvent>>>);

impl DesktopOutput for FakeDesktop {
    fn send(&mut self, events: &[DesktopEvent]) -> Result<(), DesktopError> {
        self.0.lock().unwrap().extend_from_slice(events);
        Ok(())
    }
}

pub struct Rig {
    running: RunningMapper,
    mapper: Mapper,
    desktop: FakeDesktop,
    latest: Option<ControllerState>,
    now: Instant,
}

impl Rig {
    pub fn new(profile: &Profile) -> Self {
        let desktop = FakeDesktop::default();
        let desk = Arc::new(Desk::new(Box::new(desktop.clone())));
        Self {
            running: RunningMapper::new(desk),
            mapper: Mapper::new(profile),
            desktop,
            latest: None,
            now: Instant::now(),
        }
    }

    pub fn report_after(&mut self, millis: u64, state: ControllerState) -> ControllerState {
        self.now += Duration::from_millis(millis);
        self.latest = Some(state);
        let shown = self.running.report(&self.mapper, &state, self.now);
        self.running.act();
        shown
    }

    pub fn tick_after(&mut self, millis: u64) -> Option<ControllerState> {
        self.now += Duration::from_millis(millis);
        let shown = self.running.tick(&self.mapper, self.now);
        self.running.act();
        shown
    }

    pub fn change(&mut self, mapper: Mapper) {
        self.mapper = mapper;
        self.running.start_over(&self.mapper, self.latest.as_ref());
    }

    pub fn let_go(&mut self) {
        self.running.let_go();
    }

    pub fn take(&mut self) -> Vec<DesktopEvent> {
        std::mem::take(&mut self.desktop.0.lock().unwrap())
    }
}

pub fn moved(events: &[DesktopEvent]) -> (i32, i32) {
    events.iter().fold((0, 0), |(x, y), event| match event {
        DesktopEvent::Move { x: dx, y: dy } => (x + dx, y + dy),
        _ => (x, y),
    })
}

pub fn scrolled(events: &[DesktopEvent]) -> (i32, i32) {
    events.iter().fold((0, 0), |(x, y), event| match event {
        DesktopEvent::Scroll { x: dx, y: dy } => (x + dx, y + dy),
        _ => (x, y),
    })
}

pub fn clicks(events: &[DesktopEvent]) -> Vec<DesktopEvent> {
    events
        .iter()
        .copied()
        .filter(|event| matches!(event, DesktopEvent::Button { .. }))
        .collect()
}
