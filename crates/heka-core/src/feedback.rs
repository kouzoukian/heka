//! Feedback: what a game sends to a Virtual Controller, on its way to the Controller.

use std::time::Duration;

use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Lightbar {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Lightbar {
    /// No light.
    pub const DARK: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
    };
    /// The blue a DualSense is given when nothing chooses a colour
    /// (`dualsense_create` in Linux `hid-playstation.c`).
    pub const DEFAULT: Self = Self {
        red: 0,
        green: 0,
        blue: 128,
    };
}

/// What the five white player lights under a DualSense's touchpad show: nothing,
/// or the pattern of one player. A DualShock 4 has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayerLights {
    Off,
    One,
    Two,
    Three,
    Four,
}

/// Everything the Controller is asked to show: what a game currently asks for,
/// shaped by the player's [`crate::preferences::Preferences`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Feedback {
    pub rumble: Rumble,
    /// The colour a game set, or `None` to leave the Controller's lightbar as it
    /// is: an Xbox Virtual Controller has no lightbar.
    pub lightbar: Option<Lightbar>,
    /// What the player lights show, or `None` to leave them as they are. Only
    /// the player chooses them: games never do.
    pub player_lights: Option<PlayerLights>,
}

impl Feedback {
    /// Both motors stopped, the lights left as they are.
    pub const OFF: Self = Self {
        rumble: Rumble::OFF,
        lightbar: None,
        player_lights: None,
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
