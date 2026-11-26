use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::feedback::{Feedback, Lightbar, PlayerLights, Rumble};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RumbleStrength(u8);

impl RumbleStrength {
    pub const OFF: Self = Self(0);
    pub const FULL: Self = Self(100);

    pub fn percent(percent: u8) -> Self {
        Self(percent.min(Self::FULL.0))
    }

    pub fn scale(self, rumble: Rumble) -> Rumble {
        let percent = u16::from(self.0.min(Self::FULL.0));
        let scaled = |motor: u8| ((u16::from(motor) * percent + 50) / 100) as u8;
        Rumble {
            large: scaled(rumble.large),
            small: scaled(rumble.small),
        }
    }
}

impl Default for RumbleStrength {
    fn default() -> Self {
        Self::FULL
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub rumble: RumbleStrength,
    pub lightbar: Option<Lightbar>,
    pub player_lights: Option<PlayerLights>,
    pub hide: bool,
}

impl Preferences {
    pub fn apply(&self, asked: Feedback) -> Feedback {
        let lightbar = match asked.lightbar {
            Some(colour) if colour != Lightbar::DARK => Some(colour),
            dark_or_none => self.lightbar.or(dark_or_none),
        };
        Feedback {
            rumble: self.rumble.scale(asked.rumble),
            lightbar,
            player_lights: self.player_lights,
        }
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            rumble: RumbleStrength::FULL,
            lightbar: None,
            player_lights: None,
            hide: true,
        }
    }
}

#[derive(Debug, Default)]
pub struct LivePreferences {
    current: Mutex<Preferences>,
}

impl LivePreferences {
    pub fn get(&self) -> Preferences {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set(&self, preferences: Preferences) {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = preferences;
    }
}
