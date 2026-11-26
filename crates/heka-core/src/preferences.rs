//! How the player wants one Controller to behave: how strongly it rumbles, what
//! its lights show while it is Bridged, and whether it is Hidden.
//!
//! The rumble and the lights shape the Feedback on its way to the Controller
//! (see [`Preferences::apply`]). A running Bridge follows a change at once,
//! through the [`LivePreferences`] it shares with whoever changes them.

use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::feedback::{Feedback, Lightbar, PlayerLights, Rumble};

/// How strongly a Controller rumbles, as a percentage of what the game asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RumbleStrength(u8);

impl RumbleStrength {
    pub const OFF: Self = Self(0);
    pub const FULL: Self = Self(100);

    /// More than 100% counts as 100%.
    pub fn percent(percent: u8) -> Self {
        Self(percent.min(Self::FULL.0))
    }

    /// Both motors at this strength, rounded to the nearest step.
    pub fn scale(self, rumble: Rumble) -> Rumble {
        // Clamped here too: a settings file may hold any number.
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

/// What the player chose for one Controller. The defaults change nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub rumble: RumbleStrength,
    /// The lightbar colour while no game has set one, or `None` to leave the
    /// lightbar to the Controller.
    pub lightbar: Option<Lightbar>,
    /// What a DualSense's player lights show, or `None` to leave them to the
    /// Controller.
    pub player_lights: Option<PlayerLights>,
    /// Hide the Controller from games while it is Bridged or Held.
    pub hide: bool,
}

impl Preferences {
    /// What the Controller shows when a game asks for `asked`.
    ///
    /// A colour a game set wins over the player's. A dark lightbar counts as no
    /// colour: a PS4 Virtual Controller reports one with every rumble, also for
    /// games that never set a colour.
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

/// A Controller's preferences as they stand, for a running Bridge to follow.
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
