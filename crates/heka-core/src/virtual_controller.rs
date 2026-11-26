use std::fmt;
use std::sync::{Mutex, PoisonError};

use crate::feedback::FeedbackSource;
use crate::state::ControllerState;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum OutputType {
    #[default]
    Xbox,
    Ps4,
}

pub trait VirtualBus: Send + Sync {
    fn plug(&self, output: OutputType) -> Result<Box<dyn VirtualController>, VirtualError>;
}

pub trait VirtualController: Send {
    fn output_type(&self) -> OutputType;

    fn update(&mut self, state: &ControllerState) -> Result<(), VirtualError>;

    fn unplug(self: Box<Self>) -> Result<(), VirtualError>;

    fn feedback(&mut self) -> Option<Box<dyn FeedbackSource>> {
        None
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtualFailure {
    NotReleased(String),
    NotUnplugged(String),
}

#[derive(Debug, Default)]
pub struct VirtualFailures(Mutex<Vec<VirtualFailure>>);

impl VirtualFailures {
    pub fn record(&self, failure: VirtualFailure) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(failure);
    }

    pub fn take(&self) -> Vec<VirtualFailure> {
        std::mem::take(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}
