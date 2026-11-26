use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const RUMBLE_TEST_TIME: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Motor {
    Large,
    Small,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rumble {
    pub large: u8,
    pub small: u8,
}

impl Rumble {
    pub const OFF: Self = Self { large: 0, small: 0 };

    pub fn only(motor: Motor) -> Self {
        match motor {
            Motor::Large => Self {
                large: u8::MAX,
                ..Self::OFF
            },
            Motor::Small => Self {
                small: u8::MAX,
                ..Self::OFF
            },
        }
    }

    pub fn swapped(self) -> Self {
        Self {
            large: self.small,
            small: self.large,
        }
    }
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

    pub fn rainbow(elapsed: Duration) -> Self {
        let into = elapsed.as_millis() / RAINBOW_STEP_MS * RAINBOW_STEP_MS;
        let period = RAINBOW_PERIOD.as_millis();
        let hue = (into % period * HUE_SECTORS * 255 / period) as u16;
        let rising = (hue % 255) as u8;
        let falling = 255 - rising;
        let (red, green, blue) = match hue / 255 {
            0 => (255, rising, 0),
            1 => (falling, 255, 0),
            2 => (0, 255, rising),
            3 => (0, falling, 255),
            4 => (rising, 0, 255),
            _ => (255, 0, falling),
        };
        Self { red, green, blue }
    }
}

pub const RAINBOW_PERIOD: Duration = Duration::from_secs(6);
const RAINBOW_STEP_MS: u128 = 50;
const HUE_SECTORS: u128 = 6;

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
