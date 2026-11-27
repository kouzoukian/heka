use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::state::{FineSticks, Stick};

pub const MAX_DEAD_ZONE: u8 = 50;
pub const MAX_ANTI_DEAD_ZONE: u8 = 50;
pub const MIN_MAX_ZONE: u8 = 50;
pub const MIN_SENSITIVITY: u16 = 50;
pub const MAX_SENSITIVITY: u16 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Curve {
    #[default]
    Linear,
    Soft,
    Softer,
    Sharp,
    Smooth,
}

impl Curve {
    pub const ALL: [Self; 5] = [
        Self::Linear,
        Self::Soft,
        Self::Softer,
        Self::Sharp,
        Self::Smooth,
    ];

    pub fn apply(self, travel: f32) -> f32 {
        match self {
            Self::Linear => travel,
            Self::Soft => travel * travel,
            Self::Softer => travel * travel * travel,
            Self::Sharp => 1.0 - (1.0 - travel) * (1.0 - travel),
            Self::Smooth => travel * travel * (3.0 - 2.0 * travel),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct StickTuning {
    pub dead_zone: u8,
    pub anti_dead_zone: u8,
    pub max_zone: u8,
    pub sensitivity: u16,
    pub curve: Curve,
    pub invert_x: bool,
    pub invert_y: bool,
}

impl Default for StickTuning {
    fn default() -> Self {
        Self {
            dead_zone: 0,
            anti_dead_zone: 0,
            max_zone: 100,
            sensitivity: 100,
            curve: Curve::Linear,
            invert_x: false,
            invert_y: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StickSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StickChange {
    DeadZone(u8),
    AntiDeadZone(u8),
    MaxZone(u8),
    Sensitivity(u16),
    Curve(Curve),
    InvertX(bool),
    InvertY(bool),
}

impl StickTuning {
    pub fn change(&mut self, change: StickChange) {
        match change {
            StickChange::DeadZone(percent) => self.dead_zone = percent,
            StickChange::AntiDeadZone(percent) => self.anti_dead_zone = percent,
            StickChange::MaxZone(percent) => self.max_zone = percent,
            StickChange::Sensitivity(percent) => self.sensitivity = percent,
            StickChange::Curve(curve) => self.curve = curve,
            StickChange::InvertX(invert) => self.invert_x = invert,
            StickChange::InvertY(invert) => self.invert_y = invert,
        }
        *self = self.clamped();
    }

    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    pub fn clamped(self) -> Self {
        let dead_zone = self.dead_zone.min(MAX_DEAD_ZONE);
        Self {
            dead_zone,
            anti_dead_zone: self.anti_dead_zone.min(MAX_ANTI_DEAD_ZONE),
            max_zone: self.max_zone.clamp(MIN_MAX_ZONE, 100).max(dead_zone + 1),
            sensitivity: self.sensitivity.clamp(MIN_SENSITIVITY, MAX_SENSITIVITY),
            ..self
        }
    }

    pub(crate) fn load(value: &Value) -> Self {
        let defaults = Self::default();
        let field = |name: &str| value.get(name);
        Self {
            dead_zone: given(field("dead_zone")).unwrap_or(defaults.dead_zone),
            anti_dead_zone: given(field("anti_dead_zone")).unwrap_or(defaults.anti_dead_zone),
            max_zone: given(field("max_zone")).unwrap_or(defaults.max_zone),
            sensitivity: given(field("sensitivity")).unwrap_or(defaults.sensitivity),
            curve: given(field("curve")).unwrap_or(defaults.curve),
            invert_x: given(field("invert_x")).unwrap_or(defaults.invert_x),
            invert_y: given(field("invert_y")).unwrap_or(defaults.invert_y),
        }
        .clamped()
    }

    pub fn tune(&self, x: f32, y: f32) -> (f32, f32) {
        let travel = x.hypot(y);
        let dead = f32::from(self.dead_zone) / 100.0;
        let anti = f32::from(self.anti_dead_zone) / 100.0;
        let max = f32::from(self.max_zone) / 100.0;
        let sensitivity = f32::from(self.sensitivity) / 100.0;
        let (mut x, mut y) = if travel <= dead || travel <= f32::EPSILON {
            (0.0, 0.0)
        } else {
            let scaled = ((travel - dead) / (max - dead)).clamp(0.0, 1.0);
            let lifted = anti + scaled * (1.0 - anti);
            let sensed = (lifted * sensitivity).clamp(0.0, 1.0);
            let shaped = self.curve.apply(sensed).clamp(0.0, 1.0);
            (x / travel * shaped, y / travel * shaped)
        };
        if self.invert_x {
            x = -x;
        }
        if self.invert_y {
            y = -y;
        }
        (x, y)
    }
}

fn given<T: DeserializeOwned>(value: Option<&Value>) -> Option<T> {
    T::deserialize(value?).ok()
}

pub fn unit(value: u8) -> f32 {
    let offset = i32::from(value) - 128;
    if offset < 0 {
        offset as f32 / 128.0
    } else {
        offset as f32 / 127.0
    }
}

pub fn byte(unit: f32) -> u8 {
    let offset = if unit < 0.0 {
        (unit * 128.0).round()
    } else {
        (unit * 127.0).round()
    };
    (128.0 + offset).clamp(0.0, 255.0) as u8
}

pub fn wide(unit: f32) -> i16 {
    let scaled = if unit < 0.0 {
        (unit * 32768.0).round()
    } else {
        (unit * 32767.0).round()
    };
    scaled.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

pub fn wide_byte(value: u8) -> i16 {
    let offset = i32::from(value) - 128;
    if offset < 0 {
        (offset * 256) as i16
    } else {
        (offset * 32767 / 127) as i16
    }
}

pub fn tuned_stick(tuning: &StickTuning, stick: Stick) -> (Stick, (i16, i16)) {
    if tuning.is_neutral() {
        return (stick, (wide_byte(stick.x), wide_byte(stick.y)));
    }
    let (x, y) = tuning.tune(unit(stick.x), unit(stick.y));
    (
        Stick {
            x: byte(x),
            y: byte(y),
        },
        (wide(x), wide(y)),
    )
}

pub fn fine(left: (i16, i16), right: (i16, i16)) -> FineSticks {
    FineSticks {
        left_x: left.0,
        left_y: left.1,
        right_x: right.0,
        right_y: right.1,
    }
}

pub const MIN_POINT: u8 = 1;
pub const MAX_POINT: u8 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerChange {
    DeadZone(u8),
    MaxZone(u8),
    Curve(Curve),
    HairTrigger(Option<u8>),
    PressPoint(Option<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct TriggerTuning {
    pub dead_zone: u8,
    pub max_zone: u8,
    pub curve: Curve,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hair_trigger: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub press_point: Option<u8>,
}

impl Default for TriggerTuning {
    fn default() -> Self {
        Self {
            dead_zone: 0,
            max_zone: 100,
            curve: Curve::Linear,
            hair_trigger: None,
            press_point: None,
        }
    }
}

impl TriggerTuning {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    pub fn shapes_travel(&self) -> bool {
        Self {
            press_point: None,
            ..*self
        } != Self::default()
    }

    pub fn clamped(self) -> Self {
        let dead_zone = self.dead_zone.min(MAX_DEAD_ZONE);
        let point = |point: u8| point.clamp(MIN_POINT, MAX_POINT);
        Self {
            dead_zone,
            max_zone: self.max_zone.clamp(MIN_MAX_ZONE, 100).max(dead_zone + 1),
            hair_trigger: self.hair_trigger.map(point),
            press_point: self.press_point.map(point),
            ..self
        }
    }

    pub fn change(&mut self, change: TriggerChange) {
        match change {
            TriggerChange::DeadZone(percent) => self.dead_zone = percent,
            TriggerChange::MaxZone(percent) => self.max_zone = percent,
            TriggerChange::Curve(curve) => self.curve = curve,
            TriggerChange::HairTrigger(point) => self.hair_trigger = point,
            TriggerChange::PressPoint(point) => self.press_point = point,
        }
        *self = self.clamped();
    }

    pub(crate) fn load(value: &Value) -> Self {
        let defaults = Self::default();
        let field = |name: &str| value.get(name);
        Self {
            dead_zone: given(field("dead_zone")).unwrap_or(defaults.dead_zone),
            max_zone: given(field("max_zone")).unwrap_or(defaults.max_zone),
            curve: given(field("curve")).unwrap_or(defaults.curve),
            hair_trigger: given(field("hair_trigger")).unwrap_or(defaults.hair_trigger),
            press_point: given(field("press_point")).unwrap_or(defaults.press_point),
        }
        .clamped()
    }

    pub fn fires_hair_trigger(&self, travel: u8) -> bool {
        let Some(point) = self.hair_trigger else {
            return false;
        };
        let share = f32::from(travel) / 255.0;
        share > f32::from(self.dead_zone) / 100.0 && share >= f32::from(point) / 100.0
    }

    pub fn tune(&self, travel: u8) -> u8 {
        if !self.shapes_travel() {
            return travel;
        }
        if self.fires_hair_trigger(travel) {
            return u8::MAX;
        }
        let share = f32::from(travel) / 255.0;
        let dead = f32::from(self.dead_zone) / 100.0;
        let max = f32::from(self.max_zone) / 100.0;
        if share <= dead {
            return 0;
        }
        let scaled = ((share - dead) / (max - dead)).clamp(0.0, 1.0);
        (self.curve.apply(scaled).clamp(0.0, 1.0) * 255.0).round() as u8
    }

    pub fn presses(&self, travel: u8) -> Option<bool> {
        let point = self.press_point?;
        Some(u32::from(travel) >= (u32::from(point) * 255 + 50) / 100)
    }
}
