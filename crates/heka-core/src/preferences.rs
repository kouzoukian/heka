use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::feedback::{Feedback, Lightbar, PlayerLights, Rumble};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "u64")]
pub struct RumbleStrength(u8);

impl From<u64> for RumbleStrength {
    fn from(percent: u64) -> Self {
        Self::percent(u8::try_from(percent).unwrap_or(u8::MAX))
    }
}

impl RumbleStrength {
    pub const OFF: Self = Self(0);
    pub const FULL: Self = Self(100);

    pub fn percent(percent: u8) -> Self {
        Self(percent.min(Self::FULL.0))
    }

    pub fn scale(self, rumble: Rumble) -> Rumble {
        let percent = u16::from(self.0);
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
#[serde(untagged)]
pub enum LightbarChoice {
    Colour(Lightbar),
    Effect(LightbarEffect),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LightbarEffect {
    Rainbow,
}

impl LightbarChoice {
    pub const RAINBOW: Self = Self::Effect(LightbarEffect::Rainbow);

    pub fn colour(self, elapsed: Duration) -> Lightbar {
        match self {
            Self::Colour(colour) => colour,
            Self::Effect(LightbarEffect::Rainbow) => Lightbar::rainbow(elapsed),
        }
    }

    pub fn moves(self) -> bool {
        matches!(self, Self::Effect(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub rumble: RumbleStrength,
    pub lightbar: Option<LightbarChoice>,
    pub player_lights: Option<PlayerLights>,
    pub hide: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreferenceChange {
    Rumble(RumbleStrength),
    Lightbar(Option<LightbarChoice>),
    PlayerLights(Option<PlayerLights>),
    Hide(bool),
}

impl Preferences {
    pub fn change(&mut self, change: PreferenceChange) {
        match change {
            PreferenceChange::Rumble(rumble) => self.rumble = rumble,
            PreferenceChange::Lightbar(lightbar) => self.lightbar = lightbar,
            PreferenceChange::PlayerLights(lights) => self.player_lights = lights,
            PreferenceChange::Hide(hide) => self.hide = hide,
        }
    }

    pub fn apply(&self, asked: Feedback, elapsed: Duration) -> Feedback {
        let lightbar = match asked.lightbar {
            Some(colour) if colour != Lightbar::DARK => Some(colour),
            dark_or_none => self
                .lightbar
                .map(|choice| choice.colour(elapsed))
                .or(dark_or_none),
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
