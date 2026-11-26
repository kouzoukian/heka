//! Feedback: what a game sends to a Virtual Controller, on its way to the Controller.

use std::time::Duration;

/// Rumble motor speeds, 0 = stopped, 255 = full, in Xbox terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rumble {
    /// The large, low-frequency motor (the Xbox left motor).
    pub large: u8,
    /// The small, high-frequency motor (the Xbox right motor).
    pub small: u8,
}

impl Rumble {
    pub const OFF: Self = Self { large: 0, small: 0 };
}

/// A lightbar colour, each channel 0 = off to 255 = full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Lightbar {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

/// Everything a game currently asks the Controller to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Feedback {
    pub rumble: Rumble,
    /// The colour a game set, or `None` to leave the Controller's lightbar as it
    /// is: an Xbox Virtual Controller has no lightbar.
    pub lightbar: Option<Lightbar>,
}

impl Feedback {
    /// Both motors stopped, the lightbar left as it is.
    pub const OFF: Self = Self {
        rumble: Rumble::OFF,
        lightbar: None,
    };
}

/// The outcome of waiting for Feedback from a Virtual Controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackEvent {
    /// A game sent this Feedback.
    Feedback(Feedback),
    /// Nothing arrived in time.
    Timeout,
    /// No more Feedback will come, for example because the Virtual Controller is gone.
    Closed,
}

/// Feedback games send to one Virtual Controller. Implemented over ViGEmBus in
/// `heka-windows`.
pub trait FeedbackSource: Send {
    /// Waits at most `timeout` for the next Feedback.
    fn next(&mut self, timeout: Duration) -> FeedbackEvent;
}
