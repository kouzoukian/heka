use std::sync::Arc;

use crate::bridge::{Bridge, BridgeEnd, LiveInput, OpenLink, Plugged};
use crate::live_gyro::LiveGyro;
use crate::state::ControllerState;
use crate::threads::Threads;
use crate::virtual_controller::{OutputType, VirtualController, VirtualError};

pub struct Watch(Bridge);

impl Watch {
    pub fn start_on(
        threads: &dyn Threads,
        link: OpenLink,
        live: Arc<LiveInput>,
        gyro: Arc<LiveGyro>,
    ) -> Result<Self, (std::io::Error, OpenLink)> {
        let mut nothing = Plugged::new(Box::new(Nothing));
        nothing.measure_with(gyro);
        Bridge::resume_on(threads, link, nothing, live)
            .map(Self)
            .map_err(|failed| (failed.error, failed.link))
    }

    pub fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    pub fn stop(self) -> Option<OpenLink> {
        match self.0.stop() {
            BridgeEnd::Stopped { link, .. } | BridgeEnd::VirtualControllerFailed { link, .. } => {
                Some(link)
            }
            BridgeEnd::LinkLost { .. } => None,
        }
    }
}

struct Nothing;

impl VirtualController for Nothing {
    fn output_type(&self) -> OutputType {
        OutputType::default()
    }

    fn update(&mut self, _: &ControllerState) -> Result<(), VirtualError> {
        Ok(())
    }

    fn unplug(self: Box<Self>) -> Result<(), VirtualError> {
        Ok(())
    }
}
