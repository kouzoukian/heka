use std::sync::Arc;

use crate::bridge::{Bridge, BridgeEnd, LiveInput, OpenLink, Plugged};
use crate::state::ControllerState;
use crate::virtual_controller::{OutputType, VirtualController, VirtualError};

pub struct Watch(Bridge);

impl Watch {
    pub fn start(link: OpenLink, live: Arc<LiveInput>) -> std::io::Result<Self> {
        Bridge::resume(link, Plugged::new(Box::new(Nothing)), live).map(Self)
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
