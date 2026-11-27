use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use crate::desktop::{Chord, DesktopState, Key, MouseButton};

pub const MAX_STEPS: usize = 32;
pub const MIN_WAIT: u16 = 1;
pub const MAX_WAIT: u16 = 10_000;
pub const NEW_STEP: Step = Step::Wait(50);

pub const MIN_RATE: u8 = 2;
pub const MAX_RATE: u8 = 30;
pub const USUAL_RATE: u8 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Hold,
    Toggle,
    Turbo(u8),
}

impl Mode {
    pub fn clamped(self) -> Self {
        match self {
            Self::Turbo(rate) => Self::Turbo(rate.clamp(MIN_RATE, MAX_RATE)),
            mode => mode,
        }
    }

    pub(crate) fn load(binding: &Value) -> Self {
        match binding.get("mode").and_then(Value::as_str) {
            Some("toggle") => Self::Toggle,
            Some("turbo") => {
                let rate = binding
                    .get("rate")
                    .and_then(Value::as_u64)
                    .map_or(USUAL_RATE, |rate| u8::try_from(rate).unwrap_or(MAX_RATE));
                Self::Turbo(rate).clamped()
            }
            _ => Self::Hold,
        }
    }

    pub(crate) fn serialize_into<M: SerializeMap>(self, map: &mut M) -> Result<(), M::Error> {
        match self {
            Self::Hold => Ok(()),
            Self::Toggle => map.serialize_entry("mode", "toggle"),
            Self::Turbo(rate) => {
                map.serialize_entry("mode", "turbo")?;
                map.serialize_entry("rate", &rate)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Keys(Chord),
    Mouse(MouseButton),
}

impl Target {
    pub(crate) fn add_to(self, desktop: &mut DesktopState) {
        match self {
            Self::Keys(chord) => desktop.add_chord(chord),
            Self::Mouse(button) => desktop.add_button(button),
        }
    }

    fn load(value: &Value) -> Option<Self> {
        match value {
            Value::String(_) => MouseButton::deserialize(value).ok().map(Self::Mouse),
            Value::Number(scan) => {
                let scan = u16::try_from(scan.as_u64()?).ok()?;
                Key::from_scan(scan).map(|key| Self::Keys(Chord::of(key)))
            }
            other => Chord::load(other).map(Self::Keys),
        }
    }
}

impl Serialize for Target {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Keys(chord) => chord.serialize(serializer),
            Self::Mouse(button) => button.serialize(serializer),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Press(Target),
    Release(Target),
    Wait(u16),
}

impl Step {
    fn clamped(self) -> Self {
        match self {
            Self::Wait(millis) => Self::Wait(millis.clamp(MIN_WAIT, MAX_WAIT)),
            step => step,
        }
    }

    fn load(value: &Value) -> Option<Self> {
        let (kind, value) = value.as_object()?.iter().next()?;
        match kind.as_str() {
            "press" => Target::load(value).map(Self::Press),
            "release" => Target::load(value).map(Self::Release),
            "wait" => {
                let millis = u16::try_from(value.as_u64()?).unwrap_or(MAX_WAIT);
                Some(Self::Wait(millis).clamped())
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroChange {
    Repeat(bool),
    Add,
    Set(usize, Step),
    Remove(usize),
    MoveUp(usize),
    MoveDown(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Macro {
    steps: Vec<Step>,
    pub repeat: bool,
}

impl Macro {
    pub fn repeating() -> Self {
        Self {
            steps: Vec::new(),
            repeat: true,
        }
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    pub fn change(&mut self, change: MacroChange) {
        let count = self.steps.len();
        match change {
            MacroChange::Repeat(repeat) => self.repeat = repeat,
            MacroChange::Add if count < MAX_STEPS => self.steps.push(NEW_STEP),
            MacroChange::Set(index, step) if index < count => self.steps[index] = step.clamped(),
            MacroChange::Remove(index) if index < count => {
                self.steps.remove(index);
            }
            MacroChange::MoveUp(index) if index > 0 && index < count => {
                self.steps.swap(index - 1, index);
            }
            MacroChange::MoveDown(index) if index + 1 < count => {
                self.steps.swap(index, index + 1);
            }
            _ => {}
        }
    }

    pub(crate) fn load(steps: &Value, binding: &Value) -> Option<Self> {
        let steps = steps
            .as_array()?
            .iter()
            .filter_map(Step::load)
            .take(MAX_STEPS)
            .collect();
        Some(Self {
            steps,
            repeat: binding
                .get("repeat")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    pub(crate) fn serialize_into<M: SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
        map.serialize_entry("macro", &self.steps)?;
        map.serialize_entry("repeat", &self.repeat)
    }
}
