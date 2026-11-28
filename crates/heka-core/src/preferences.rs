use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::feedback::{Claimed, Feedback, Lightbar, PlayerLights, Rumble};
use crate::state::Battery;

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
#[serde(rename_all = "lowercase")]
pub enum MuteLight {
    Off,
    On,
    Button,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Conditions {
    pub elapsed: Duration,
    pub battery: Option<Battery>,
    pub mute_toggled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub rumble: RumbleStrength,
    pub swap_motors: bool,
    pub lightbar: Option<LightbarChoice>,
    #[serde(skip_serializing_if = "is_false")]
    pub battery_colour: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub low_battery_flash: bool,
    pub player_lights: Option<PlayerLights>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mute_light: Option<MuteLight>,
    pub hide: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreferenceChange {
    Rumble(RumbleStrength),
    SwapMotors(bool),
    Lightbar(Option<LightbarChoice>),
    BatteryColour(bool),
    LowBatteryFlash(bool),
    PlayerLights(Option<PlayerLights>),
    MuteLight(Option<MuteLight>),
    Hide(bool),
}

impl Preferences {
    pub fn change(&mut self, change: PreferenceChange) {
        match change {
            PreferenceChange::Rumble(rumble) => self.rumble = rumble,
            PreferenceChange::SwapMotors(swap) => self.swap_motors = swap,
            PreferenceChange::Lightbar(lightbar) => self.lightbar = lightbar,
            PreferenceChange::BatteryColour(shown) => self.battery_colour = shown,
            PreferenceChange::LowBatteryFlash(flash) => self.low_battery_flash = flash,
            PreferenceChange::PlayerLights(lights) => self.player_lights = lights,
            PreferenceChange::MuteLight(light) => self.mute_light = light,
            PreferenceChange::Hide(hide) => self.hide = hide,
        }
    }

    pub fn apply(&self, asked: Feedback, conditions: Conditions) -> Feedback {
        let lightbar = match asked.lightbar {
            Some(colour) if colour != Lightbar::DARK => Some(colour),
            dark_or_none => self.own_colour(conditions).or(dark_or_none),
        };
        let lightbar = if self.flashes(conditions.battery) {
            let lit = lightbar
                .filter(|&colour| colour != Lightbar::DARK)
                .unwrap_or(Lightbar::DEFAULT);
            Some(lit.flashing(conditions.elapsed))
        } else {
            lightbar
        };
        let rumble = if self.swap_motors {
            asked.rumble.swapped()
        } else {
            asked.rumble
        };
        Feedback {
            rumble: self.rumble.scale(rumble),
            lightbar,
            player_lights: self.player_lights,
            mute_light: self.mute_light.map(|light| match light {
                MuteLight::Off => false,
                MuteLight::On => true,
                MuteLight::Button => conditions.mute_toggled,
            }),
            trigger_effects: asked.trigger_effects,
        }
    }

    pub fn claims(&self, battery: Option<Battery>) -> Claimed {
        Claimed {
            lightbar: self.lightbar.is_some()
                || self.battery_level(battery).is_some()
                || self.flashes(battery),
            player_lights: self.player_lights.is_some(),
            mute_light: self.mute_light.is_some(),
            ..Claimed::default()
        }
    }

    pub fn moves(&self, battery: Option<Battery>) -> bool {
        let rainbow = self.lightbar.is_some_and(LightbarChoice::moves)
            && self.battery_level(battery).is_none();
        rainbow || self.flashes(battery)
    }

    fn own_colour(&self, conditions: Conditions) -> Option<Lightbar> {
        match self.battery_level(conditions.battery) {
            Some(level) => Some(Lightbar::of_battery(level)),
            None => self
                .lightbar
                .map(|choice| choice.colour(conditions.elapsed)),
        }
    }

    fn battery_level(&self, battery: Option<Battery>) -> Option<u8> {
        battery
            .filter(|_| self.battery_colour)
            .and_then(Battery::level)
    }

    fn flashes(&self, battery: Option<Battery>) -> bool {
        self.low_battery_flash && battery.is_some_and(Battery::is_low)
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            rumble: RumbleStrength::FULL,
            swap_motors: false,
            lightbar: None,
            battery_colour: false,
            low_battery_flash: false,
            player_lights: None,
            mute_light: None,
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
