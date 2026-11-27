use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::state::{Buttons, ControllerState, FineSticks, Stick, TOUCHPAD_WIDTH};
use crate::tuning::{StickTuning, fine, tuned_stick};
use crate::virtual_controller::OutputType;

pub const DEFAULT_NAME: &str = "Default";
pub const MAX_NAME_CHARS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    Cross,
    Circle,
    Square,
    Triangle,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    L1,
    R1,
    L2,
    R2,
    L3,
    R3,
    Create,
    Options,
    Ps,
    Touchpad,
    TouchpadLeft,
    TouchpadRight,
    Mute,
    LeftFn,
    RightFn,
    LeftBack,
    RightBack,
}

impl Input {
    pub const ALL: [Self; 25] = [
        Self::Cross,
        Self::Circle,
        Self::Square,
        Self::Triangle,
        Self::DpadUp,
        Self::DpadDown,
        Self::DpadLeft,
        Self::DpadRight,
        Self::L1,
        Self::R1,
        Self::L2,
        Self::R2,
        Self::L3,
        Self::R3,
        Self::Create,
        Self::Options,
        Self::Ps,
        Self::Touchpad,
        Self::TouchpadLeft,
        Self::TouchpadRight,
        Self::Mute,
        Self::LeftFn,
        Self::RightFn,
        Self::LeftBack,
        Self::RightBack,
    ];

    pub fn buttons(self) -> Buttons {
        match self {
            Self::Cross => Buttons::CROSS,
            Self::Circle => Buttons::CIRCLE,
            Self::Square => Buttons::SQUARE,
            Self::Triangle => Buttons::TRIANGLE,
            Self::DpadUp => Buttons::DPAD_UP,
            Self::DpadDown => Buttons::DPAD_DOWN,
            Self::DpadLeft => Buttons::DPAD_LEFT,
            Self::DpadRight => Buttons::DPAD_RIGHT,
            Self::L1 => Buttons::L1,
            Self::R1 => Buttons::R1,
            Self::L2 => Buttons::L2,
            Self::R2 => Buttons::R2,
            Self::L3 => Buttons::L3,
            Self::R3 => Buttons::R3,
            Self::Create => Buttons::CREATE,
            Self::Options => Buttons::OPTIONS,
            Self::Ps => Buttons::PS,
            Self::Touchpad | Self::TouchpadLeft | Self::TouchpadRight => Buttons::TOUCHPAD,
            Self::Mute => Buttons::MUTE,
            Self::LeftFn => Buttons::LEFT_FN,
            Self::RightFn => Buttons::RIGHT_FN,
            Self::LeftBack => Buttons::LEFT_BACK,
            Self::RightBack => Buttons::RIGHT_BACK,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Output {
    Cross,
    Circle,
    Square,
    Triangle,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    L1,
    R1,
    L3,
    R3,
    Create,
    Options,
    Ps,
    LeftTrigger,
    RightTrigger,
}

impl Output {
    pub const ALL: [Self; 17] = [
        Self::Cross,
        Self::Circle,
        Self::Square,
        Self::Triangle,
        Self::DpadUp,
        Self::DpadDown,
        Self::DpadLeft,
        Self::DpadRight,
        Self::L1,
        Self::R1,
        Self::L3,
        Self::R3,
        Self::Create,
        Self::Options,
        Self::Ps,
        Self::LeftTrigger,
        Self::RightTrigger,
    ];

    fn buttons(self) -> Buttons {
        match self {
            Self::Cross => Buttons::CROSS,
            Self::Circle => Buttons::CIRCLE,
            Self::Square => Buttons::SQUARE,
            Self::Triangle => Buttons::TRIANGLE,
            Self::DpadUp => Buttons::DPAD_UP,
            Self::DpadDown => Buttons::DPAD_DOWN,
            Self::DpadLeft => Buttons::DPAD_LEFT,
            Self::DpadRight => Buttons::DPAD_RIGHT,
            Self::L1 => Buttons::L1,
            Self::R1 => Buttons::R1,
            Self::L3 => Buttons::L3,
            Self::R3 => Buttons::R3,
            Self::Create => Buttons::CREATE,
            Self::Options => Buttons::OPTIONS,
            Self::Ps => Buttons::PS,
            Self::LeftTrigger => Buttons::L2,
            Self::RightTrigger => Buttons::R2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    Default,
    Nothing,
    To(Output),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    NotJson(String),
    NotAProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Profile {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_type: Option<OutputType>,
    bindings: BTreeMap<Input, Option<Output>>,
    #[serde(skip_serializing_if = "StickTuning::is_neutral")]
    pub left_stick: StickTuning,
    #[serde(skip_serializing_if = "StickTuning::is_neutral")]
    pub right_stick: StickTuning,
    #[serde(skip_serializing_if = "is_false")]
    pub swap_sticks: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Default for Profile {
    fn default() -> Self {
        Self::named(DEFAULT_NAME)
    }
}

impl Profile {
    pub fn named(name: &str) -> Self {
        Self {
            name: clean_name(name).unwrap_or_else(|| DEFAULT_NAME.to_owned()),
            output_type: None,
            bindings: BTreeMap::new(),
            left_stick: StickTuning::default(),
            right_stick: StickTuning::default(),
            swap_sticks: false,
        }
    }

    pub fn load(text: &str, fallback_name: &str) -> Option<Self> {
        let Value::Object(root) = serde_json::from_str(text).ok()? else {
            return None;
        };
        Self::from_object(&root, fallback_name)
    }

    pub fn parse(text: &str, fallback_name: &str) -> Result<Self, ProfileError> {
        let value: Value =
            serde_json::from_str(text).map_err(|error| ProfileError::NotJson(error.to_string()))?;
        let Value::Object(root) = value else {
            return Err(ProfileError::NotAProfile);
        };
        let looks_like_one = [
            "name",
            "output_type",
            "bindings",
            "left_stick",
            "right_stick",
            "swap_sticks",
        ]
        .iter()
        .any(|key| root.contains_key(*key));
        if !looks_like_one {
            return Err(ProfileError::NotAProfile);
        }
        Self::from_object(&root, fallback_name).ok_or(ProfileError::NotAProfile)
    }

    fn from_object(root: &serde_json::Map<String, Value>, fallback_name: &str) -> Option<Self> {
        let name = root
            .get("name")
            .and_then(Value::as_str)
            .and_then(clean_name)
            .or_else(|| clean_name(fallback_name))?;
        let output_type = root.get("output_type").and_then(given::<OutputType>);
        let bindings = root
            .get("bindings")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(input, output)| {
                let input = given::<Input>(&Value::String(input.clone()))?;
                let output = match output {
                    Value::Null => None,
                    other => Some(given::<Output>(other)?),
                };
                Some((input, output))
            })
            .collect();
        let stick = |key: &str| root.get(key).map(StickTuning::load).unwrap_or_default();
        Some(Self {
            name,
            output_type,
            bindings,
            left_stick: stick("left_stick"),
            right_stick: stick("right_stick"),
            swap_sticks: root
                .get("swap_sticks")
                .and_then(given::<bool>)
                .unwrap_or(false),
        })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a profile always serializes")
    }

    pub fn rename(&mut self, name: &str) {
        if let Some(name) = clean_name(name) {
            self.name = name;
        }
    }

    pub fn binding(&self, input: Input) -> Binding {
        match self.bindings.get(&input) {
            None => Binding::Default,
            Some(None) => Binding::Nothing,
            Some(Some(output)) => Binding::To(*output),
        }
    }

    pub fn bind(&mut self, input: Input, binding: Binding) {
        match binding {
            Binding::Default => self.bindings.remove(&input),
            Binding::Nothing => self.bindings.insert(input, None),
            Binding::To(output) => self.bindings.insert(input, Some(output)),
        };
    }

    pub fn is_plain(&self) -> bool {
        self.output_type.is_none()
            && self.bindings.is_empty()
            && self.left_stick.is_neutral()
            && self.right_stick.is_neutral()
            && !self.swap_sticks
    }
}

fn clean_name(name: &str) -> Option<String> {
    let cut: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
    let cut = cut.trim_end();
    (!cut.is_empty()).then(|| cut.to_owned())
}

fn given<T: DeserializeOwned>(value: &Value) -> Option<T> {
    T::deserialize(value).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Unbound,
    Nothing,
    To(Output),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapper {
    slots: [Slot; Input::ALL.len()],
    left_stick: StickTuning,
    right_stick: StickTuning,
    swap_sticks: bool,
}

impl Default for Mapper {
    fn default() -> Self {
        Self {
            slots: [Slot::Unbound; Input::ALL.len()],
            left_stick: StickTuning::default(),
            right_stick: StickTuning::default(),
            swap_sticks: false,
        }
    }
}

impl Mapper {
    pub fn new(profile: &Profile) -> Self {
        let mut mapper = Self {
            left_stick: profile.left_stick,
            right_stick: profile.right_stick,
            swap_sticks: profile.swap_sticks,
            ..Self::default()
        };
        for input in Input::ALL {
            mapper.slots[input.index()] = match profile.binding(input) {
                Binding::Default => Slot::Unbound,
                Binding::Nothing => Slot::Nothing,
                Binding::To(output) => Slot::To(output),
            };
        }
        mapper
    }

    pub fn map(&self, state: &ControllerState) -> ControllerState {
        let mut buttons = Buttons::NONE;
        let mut left_full = false;
        let mut right_full = false;
        let mut drive = |slot: Slot, own: Buttons| match slot {
            Slot::Unbound => buttons |= own,
            Slot::Nothing => {}
            Slot::To(Output::LeftTrigger) => left_full = true,
            Slot::To(Output::RightTrigger) => right_full = true,
            Slot::To(output) => buttons |= output.buttons(),
        };
        for input in Input::ALL {
            let own = input.buttons();
            if !state.buttons.contains(own) {
                continue;
            }
            match input {
                Input::Touchpad => {
                    let half = state.touch[0].map(|finger| {
                        if finger.x < TOUCHPAD_WIDTH / 2 {
                            Input::TouchpadLeft
                        } else {
                            Input::TouchpadRight
                        }
                    });
                    let slot = half
                        .map(|half| self.slot(half))
                        .filter(|slot| *slot != Slot::Unbound)
                        .unwrap_or_else(|| self.slot(Input::Touchpad));
                    drive(slot, own);
                }
                Input::TouchpadLeft | Input::TouchpadRight => {}
                _ => drive(self.slot(input), own),
            }
        }
        if left_full {
            buttons |= Buttons::L2;
        }
        if right_full {
            buttons |= Buttons::R2;
        }
        let (left_stick, right_stick, fine_sticks) = self.sticks(state);
        ControllerState {
            buttons,
            left_trigger: self.trigger(Input::L2, left_full, state.left_trigger),
            right_trigger: self.trigger(Input::R2, right_full, state.right_trigger),
            left_stick,
            right_stick,
            fine_sticks,
            ..*state
        }
    }

    fn sticks(&self, state: &ControllerState) -> (Stick, Stick, Option<FineSticks>) {
        if self.left_stick.is_neutral() && self.right_stick.is_neutral() {
            return if self.swap_sticks {
                (state.right_stick, state.left_stick, state.fine_sticks)
            } else {
                (state.left_stick, state.right_stick, state.fine_sticks)
            };
        }
        let (left, left_wide) = tuned_stick(&self.left_stick, state.left_stick);
        let (right, right_wide) = tuned_stick(&self.right_stick, state.right_stick);
        if self.swap_sticks {
            (right, left, Some(fine(right_wide, left_wide)))
        } else {
            (left, right, Some(fine(left_wide, right_wide)))
        }
    }

    fn slot(&self, input: Input) -> Slot {
        self.slots[input.index()]
    }

    fn trigger(&self, input: Input, full: bool, travel: u8) -> u8 {
        if full {
            u8::MAX
        } else if self.slot(input) == Slot::Unbound {
            travel
        } else {
            0
        }
    }
}
