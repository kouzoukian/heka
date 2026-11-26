//! Reading a Controller's input while it cannot be Bridged, so that it can still
//! be shown live. Nothing is presented to games and nothing is sent to the Controller.

use std::sync::Arc;

use crate::bridge::{Bridge, BridgeEnd, LiveInput, OpenLink, Plugged};
use crate::state::ControllerState;
use crate::virtual_controller::{OutputType, VirtualController, VirtualError};

/// A Link being read into [`LiveInput`] on its own input thread.
///
/// It runs a Bridge's input thread with nothing in place of the Virtual Controller.
pub struct Watch(Bridge);

impl Watch {
    pub fn start(link: OpenLink, live: Arc<LiveInput>) -> std::io::Result<Self> {
        Bridge::resume(link, Plugged::new(Box::new(Nothing)), live).map(Self)
    }

    /// True once the Link has been lost.
    pub fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    /// Stops reading and hands the Link back, unless it has been lost.
    pub fn stop(self) -> Option<OpenLink> {
        match self.0.stop() {
            BridgeEnd::Stopped { link, .. } | BridgeEnd::VirtualControllerFailed { link, .. } => {
                Some(link)
            }
            BridgeEnd::LinkLost { .. } => None,
        }
    }
}

/// Stands in for a Virtual Controller and shows its state to nobody.
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
