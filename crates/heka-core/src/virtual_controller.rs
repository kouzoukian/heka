//! The Virtual Controller seam (ADR 0001).
//!
//! heka-core decides what a Virtual Controller shows; a backend in `heka-windows`
//! (ViGEmBus today) makes it exist. Another driver can replace ViGEmBus by
//! implementing these two traits.

use std::fmt;

use crate::state::ControllerState;

/// The kind of Virtual Controller a Controller drives.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum OutputType {
    /// An Xbox 360 controller.
    #[default]
    Xbox,
}

/// Creates Virtual Controllers.
pub trait VirtualBus: Send + Sync {
    /// Plugs in a new Virtual Controller of this Output Type, with nothing pressed.
    fn plug(&self, output: OutputType) -> Result<Box<dyn VirtualController>, VirtualError>;
}

/// One plugged-in Virtual Controller, owned by its Bridge's input thread.
pub trait VirtualController: Send {
    fn output_type(&self) -> OutputType;

    /// Shows this Controller state to games, translated for the Output Type.
    fn update(&mut self, state: &ControllerState) -> Result<(), VirtualError>;

    /// Removes the Virtual Controller from the system.
    fn unplug(self: Box<Self>) -> Result<(), VirtualError>;
}

/// A backend failure, carrying the backend's own error.
#[derive(Debug)]
pub struct VirtualError(Box<dyn std::error::Error + Send + Sync>);

impl VirtualError {
    pub fn new(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self(error.into())
    }
}

impl fmt::Display for VirtualError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for VirtualError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}
