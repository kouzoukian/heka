use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::desktop::MouseButton;
use crate::state::Finger;

pub const MIN_SENSITIVITY: u16 = 25;
pub const MAX_SENSITIVITY: u16 = 300;

const TAP_TIME: Duration = Duration::from_millis(250);
const TAP_SLOP: u16 = 40;
const UNITS_A_NOTCH: f32 = 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchpadUse {
    #[default]
    Controller,
    Mouse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchpadChange {
    Use(TouchpadUse),
    Sensitivity(u16),
    InvertY(bool),
    TapClick(bool),
    TwoFingerClick(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TouchpadSettings {
    #[serde(rename = "use")]
    pub used_as: TouchpadUse,
    pub sensitivity: u16,
    pub invert_y: bool,
    pub tap_click: bool,
    pub two_finger_click: bool,
}

impl Default for TouchpadSettings {
    fn default() -> Self {
        Self {
            used_as: TouchpadUse::Controller,
            sensitivity: 100,
            invert_y: false,
            tap_click: true,
            two_finger_click: false,
        }
    }
}

impl TouchpadSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn moves_the_mouse(&self) -> bool {
        self.used_as == TouchpadUse::Mouse
    }

    pub fn change(&mut self, change: TouchpadChange) {
        match change {
            TouchpadChange::Use(used_as) => self.used_as = used_as,
            TouchpadChange::Sensitivity(percent) => {
                self.sensitivity = percent.clamp(MIN_SENSITIVITY, MAX_SENSITIVITY);
            }
            TouchpadChange::InvertY(invert) => self.invert_y = invert,
            TouchpadChange::TapClick(click) => self.tap_click = click,
            TouchpadChange::TwoFingerClick(click) => self.two_finger_click = click,
        }
    }

    pub(crate) fn load(value: &Value) -> Self {
        let defaults = Self::default();
        let mut settings = Self {
            used_as: given(value, "use").unwrap_or(defaults.used_as),
            sensitivity: defaults.sensitivity,
            invert_y: given(value, "invert_y").unwrap_or(defaults.invert_y),
            tap_click: given(value, "tap_click").unwrap_or(defaults.tap_click),
            two_finger_click: given(value, "two_finger_click").unwrap_or(defaults.two_finger_click),
        };
        if let Some(percent) = given(value, "sensitivity") {
            settings.change(TouchpadChange::Sensitivity(percent));
        }
        settings
    }
}

fn given<T: DeserializeOwned>(value: &Value, field: &str) -> Option<T> {
    T::deserialize(value.get(field)?).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Gesture {
    pub pointer: (f32, f32),
    pub scroll: (f32, f32),
    pub tap: Option<MouseButton>,
}

#[derive(Debug)]
struct Touch {
    since: Instant,
    most_fingers: usize,
    landed: [Option<Finger>; 2],
    last: [Option<Finger>; 2],
    can_tap: bool,
}

#[derive(Debug, Default)]
pub struct Gestures {
    touch: Option<Touch>,
    ignoring: bool,
}

impl Gestures {
    pub fn ignore_until_lifted(&mut self) {
        self.touch = None;
        self.ignoring = true;
    }

    pub fn follow(
        &mut self,
        settings: &TouchpadSettings,
        fingers: [Option<Finger>; 2],
        clicked: bool,
        now: Instant,
    ) -> Gesture {
        let down = fingers.iter().flatten().count();
        if down == 0 {
            self.ignoring = false;
            let tap = self.touch.take().and_then(|touch| touch.tap(settings, now));
            return Gesture {
                tap,
                ..Gesture::default()
            };
        }
        if self.ignoring {
            return Gesture::default();
        }
        let touch = self.touch.get_or_insert(Touch {
            since: now,
            most_fingers: 0,
            landed: [None; 2],
            last: [None; 2],
            can_tap: true,
        });
        touch.most_fingers = touch.most_fingers.max(down);
        touch.can_tap &= !clicked;
        let mut travelled = (0.0, 0.0);
        let mut counted = 0.0;
        for finger in fingers.into_iter().flatten() {
            let same = |other: &&Finger| other.id == finger.id;
            match touch.landed.iter().flatten().find(same) {
                Some(landed) => {
                    touch.can_tap &= landed.x.abs_diff(finger.x) <= TAP_SLOP
                        && landed.y.abs_diff(finger.y) <= TAP_SLOP;
                }
                None => {
                    if let Some(free) = touch.landed.iter_mut().find(|slot| slot.is_none()) {
                        *free = Some(finger);
                    } else {
                        touch.can_tap = false;
                    }
                }
            }
            if let Some(last) = touch.last.iter().flatten().find(same) {
                travelled.0 += f32::from(finger.x) - f32::from(last.x);
                travelled.1 += f32::from(finger.y) - f32::from(last.y);
                counted += 1.0;
            }
        }
        touch.last = fingers;
        if counted == 0.0 {
            return Gesture::default();
        }
        let (x, y) = (travelled.0 / counted, travelled.1 / counted);
        match touch.most_fingers {
            1 => {
                let sensitivity = f32::from(settings.sensitivity) / 100.0;
                let down = if settings.invert_y { -y } else { y };
                Gesture {
                    pointer: (x * sensitivity, down * sensitivity),
                    ..Gesture::default()
                }
            }
            _ if down == 2 => Gesture {
                scroll: (-x / UNITS_A_NOTCH, y / UNITS_A_NOTCH),
                ..Gesture::default()
            },
            _ => Gesture::default(),
        }
    }
}

impl Touch {
    fn tap(&self, settings: &TouchpadSettings, now: Instant) -> Option<MouseButton> {
        if !self.can_tap || now.saturating_duration_since(self.since) > TAP_TIME {
            return None;
        }
        match self.most_fingers {
            1 if settings.tap_click => Some(MouseButton::Left),
            2 if settings.two_finger_click => Some(MouseButton::Right),
            _ => None,
        }
    }
}
