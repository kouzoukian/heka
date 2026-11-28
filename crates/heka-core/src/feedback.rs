use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::trigger_effect::{TriggerEffect, TriggerEffects};

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

    pub fn of_battery(percent: u8) -> Self {
        let share = u16::from(percent.min(100));
        let rising = |share: u16| (share * 255 / 50) as u8;
        let (red, green) = if share <= 50 {
            (u8::MAX, rising(share))
        } else {
            (rising(100 - share), u8::MAX)
        };
        Self {
            red,
            green,
            blue: 0,
        }
    }

    pub fn flashing(self, elapsed: Duration) -> Self {
        let period = FLASH_PERIOD.as_millis();
        if elapsed.as_millis() % period < period / 2 {
            self
        } else {
            Self::DARK
        }
    }

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
pub const FLASH_PERIOD: Duration = Duration::from_secs(1);
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
    pub mute_light: Option<bool>,
    pub trigger_effects: TriggerEffects,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Claimed {
    pub lightbar: bool,
    pub player_lights: bool,
    pub mute_light: bool,
    pub left_effect: bool,
    pub right_effect: bool,
}

impl Claimed {
    pub fn and(self, other: Self) -> Self {
        Self {
            lightbar: self.lightbar || other.lightbar,
            player_lights: self.player_lights || other.player_lights,
            mute_light: self.mute_light || other.mute_light,
            left_effect: self.left_effect || other.left_effect,
            right_effect: self.right_effect || other.right_effect,
        }
    }
}

impl Feedback {
    pub const OFF: Self = Self {
        rumble: Rumble::OFF,
        lightbar: None,
        player_lights: None,
        mute_light: None,
        trigger_effects: TriggerEffects {
            left: None,
            right: None,
        },
    };

    pub fn claims(&self) -> Claimed {
        Claimed {
            lightbar: self.lightbar.is_some(),
            player_lights: self.player_lights.is_some(),
            mute_light: self.mute_light.is_some(),
            left_effect: self.trigger_effects.left.is_some(),
            right_effect: self.trigger_effects.right.is_some(),
        }
    }

    pub fn or(self, earlier: Self) -> Self {
        Self {
            rumble: self.rumble,
            lightbar: self.lightbar.or(earlier.lightbar),
            player_lights: self.player_lights.or(earlier.player_lights),
            mute_light: self.mute_light.or(earlier.mute_light),
            trigger_effects: TriggerEffects {
                left: self.trigger_effects.left.or(earlier.trigger_effects.left),
                right: self.trigger_effects.right.or(earlier.trigger_effects.right),
            },
        }
    }

    pub fn or_at_rest(self, claimed: Claimed) -> Self {
        self.or(Self {
            rumble: Rumble::OFF,
            lightbar: claimed.lightbar.then_some(Lightbar::DEFAULT),
            player_lights: claimed.player_lights.then_some(PlayerLights::Off),
            mute_light: claimed.mute_light.then_some(false),
            trigger_effects: TriggerEffects {
                left: claimed.left_effect.then_some(TriggerEffect::Off),
                right: claimed.right_effect.then_some(TriggerEffect::Off),
            },
        })
    }
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
