use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rumble {
    pub large: u8,
    pub small: u8,
}

impl Rumble {
    pub const OFF: Self = Self { large: 0, small: 0 };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Lightbar {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Lightbar {
    pub const DARK: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
    };
    pub const DEFAULT: Self = Self {
        red: 0,
        green: 0,
        blue: 128,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayerLights {
    Off,
    One,
    Two,
    Three,
    Four,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Feedback {
    pub rumble: Rumble,
    pub lightbar: Option<Lightbar>,
    pub player_lights: Option<PlayerLights>,
}

impl Feedback {
    pub const OFF: Self = Self {
        rumble: Rumble::OFF,
        lightbar: None,
        player_lights: None,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackEvent {
    Feedback(Feedback),
    Timeout,
    Closed,
}

pub trait FeedbackSource: Send {
    fn next(&mut self, timeout: Duration) -> FeedbackEvent;
}
