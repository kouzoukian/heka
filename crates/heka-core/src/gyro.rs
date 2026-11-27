use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::profile::Input;

pub const MIN_SENSITIVITY: u16 = 10;
pub const MAX_SENSITIVITY: u16 = 300;
pub const MAX_DEAD_ZONE: u8 = 20;

pub const POINTER_PIXELS_A_DEGREE: f32 = 20.0;
pub const FULL_STICK_DEGREES_A_SECOND: f32 = 180.0;

const UNITS_A_DEGREE: f32 = 1024.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GyroUse {
    #[default]
    Off,
    Mouse,
    RightStick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GyroActivation {
    Always,
    #[default]
    WhileHeld,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GyroAxis {
    #[default]
    Yaw,
    Roll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GyroChange {
    Use(GyroUse),
    Activation(GyroActivation),
    Button(Input),
    Axis(GyroAxis),
    SensitivityX(u16),
    SensitivityY(u16),
    DeadZone(u8),
    InvertX(bool),
    InvertY(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GyroSettings {
    #[serde(rename = "use")]
    pub used_as: GyroUse,
    pub activation: GyroActivation,
    pub button: Input,
    pub axis: GyroAxis,
    pub sensitivity_x: u16,
    pub sensitivity_y: u16,
    pub dead_zone: u8,
    pub invert_x: bool,
    pub invert_y: bool,
}

impl Default for GyroSettings {
    fn default() -> Self {
        Self {
            used_as: GyroUse::Off,
            activation: GyroActivation::WhileHeld,
            button: Input::L2,
            axis: GyroAxis::Yaw,
            sensitivity_x: 100,
            sensitivity_y: 100,
            dead_zone: 2,
            invert_x: false,
            invert_y: false,
        }
    }
}

impl GyroSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn change(&mut self, change: GyroChange) {
        let sensitivity = |percent: u16| percent.clamp(MIN_SENSITIVITY, MAX_SENSITIVITY);
        match change {
            GyroChange::Use(used_as) => self.used_as = used_as,
            GyroChange::Activation(activation) => self.activation = activation,
            GyroChange::Button(button) => self.button = button,
            GyroChange::Axis(axis) => self.axis = axis,
            GyroChange::SensitivityX(percent) => self.sensitivity_x = sensitivity(percent),
            GyroChange::SensitivityY(percent) => self.sensitivity_y = sensitivity(percent),
            GyroChange::DeadZone(degrees) => self.dead_zone = degrees.min(MAX_DEAD_ZONE),
            GyroChange::InvertX(invert) => self.invert_x = invert,
            GyroChange::InvertY(invert) => self.invert_y = invert,
        }
    }

    pub(crate) fn load(value: &Value) -> Self {
        let mut settings = Self::default();
        let changes = [
            given(value, "use").map(GyroChange::Use),
            given(value, "activation").map(GyroChange::Activation),
            given(value, "button").map(GyroChange::Button),
            given(value, "axis").map(GyroChange::Axis),
            given(value, "sensitivity_x").map(GyroChange::SensitivityX),
            given(value, "sensitivity_y").map(GyroChange::SensitivityY),
            given(value, "dead_zone").map(GyroChange::DeadZone),
            given(value, "invert_x").map(GyroChange::InvertX),
            given(value, "invert_y").map(GyroChange::InvertY),
        ];
        for change in changes.into_iter().flatten() {
            settings.change(change);
        }
        settings
    }

    pub fn rates(&self, gyro: [i32; 3], drift: [i32; 3]) -> (f32, f32) {
        let axis = |index: usize| (gyro[index] - drift[index]) as f32 / UNITS_A_DEGREE;
        let [pitch, yaw, roll] = [axis(0), axis(1), axis(2)];
        let across = match self.axis {
            GyroAxis::Yaw => -yaw,
            GyroAxis::Roll => -roll,
        };
        let down = -pitch;
        let speed = across.hypot(down);
        let dead = f32::from(self.dead_zone);
        if speed <= dead {
            return (0.0, 0.0);
        }
        let eased = (speed - dead) / speed;
        let shaped = |rate: f32, sensitivity: u16, invert: bool| {
            let rate = rate * eased * f32::from(sensitivity) / 100.0;
            if invert { -rate } else { rate }
        };
        (
            shaped(across, self.sensitivity_x, self.invert_x),
            shaped(down, self.sensitivity_y, self.invert_y),
        )
    }
}

fn given<T: DeserializeOwned>(value: &Value, field: &str) -> Option<T> {
    T::deserialize(value.get(field)?).ok()
}
