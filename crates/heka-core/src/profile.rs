use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use crate::desktop::{Chord, DesktopState, MouseButton, WheelStep};
use crate::state::{Buttons, ControllerState, FineSticks, Stick, TOUCHPAD_WIDTH};
use crate::stick_use::{StickRole, StickUseChange, Velocity};
use crate::tuning::{
    StickChange, StickSide, StickTuning, TriggerChange, TriggerSide, TriggerTuning, fine,
    tuned_stick,
};
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
    Keys(Chord),
    Mouse(MouseButton),
    Wheel(WheelStep),
}

impl Binding {
    fn load(value: &Value) -> Option<Self> {
        match value {
            Value::Null => Some(Self::Nothing),
            Value::Object(fields) => {
                match (fields.get("keys"), fields.get("mouse"), fields.get("wheel")) {
                    (Some(keys), None, None) => Chord::load(keys).map(Self::Keys),
                    (None, Some(button), None) => given(button).map(Self::Mouse),
                    (None, None, Some(step)) => given(step).map(Self::Wheel),
                    _ => None,
                }
            }
            other => given(other).map(Self::To),
        }
    }
}

impl Serialize for Binding {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        fn tagged<S: Serializer>(
            serializer: S,
            tag: &str,
            value: impl Serialize,
        ) -> Result<S::Ok, S::Error> {
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry(tag, &value)?;
            map.end()
        }
        match self {
            Self::Default | Self::Nothing => serializer.serialize_none(),
            Self::To(output) => output.serialize(serializer),
            Self::Keys(chord) => tagged(serializer, "keys", chord),
            Self::Mouse(button) => tagged(serializer, "mouse", button),
            Self::Wheel(step) => tagged(serializer, "wheel", step),
        }
    }
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
    bindings: BTreeMap<Input, Binding>,
    #[serde(skip_serializing_if = "StickTuning::is_neutral")]
    pub left_stick: StickTuning,
    #[serde(skip_serializing_if = "StickTuning::is_neutral")]
    pub right_stick: StickTuning,
    #[serde(skip_serializing_if = "is_false")]
    pub swap_sticks: bool,
    #[serde(skip_serializing_if = "StickRole::is_default")]
    left_stick_use: StickRole,
    #[serde(skip_serializing_if = "StickRole::is_default")]
    right_stick_use: StickRole,
    #[serde(skip_serializing_if = "TriggerTuning::is_neutral")]
    pub left_trigger: TriggerTuning,
    #[serde(skip_serializing_if = "TriggerTuning::is_neutral")]
    pub right_trigger: TriggerTuning,
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
            left_stick_use: StickRole::default(),
            right_stick_use: StickRole::default(),
            left_trigger: TriggerTuning::default(),
            right_trigger: TriggerTuning::default(),
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
            "left_stick_use",
            "right_stick_use",
            "left_trigger",
            "right_trigger",
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
            .filter_map(|(input, binding)| {
                let input = given::<Input>(&Value::String(input.clone()))?;
                Some((input, Binding::load(binding)?))
            })
            .collect();
        let stick = |key: &str| root.get(key).map(StickTuning::load).unwrap_or_default();
        let trigger = |key: &str| root.get(key).map(TriggerTuning::load).unwrap_or_default();
        let stick_use = |key: &str| root.get(key).map(StickRole::load).unwrap_or_default();
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
            left_stick_use: stick_use("left_stick_use"),
            right_stick_use: stick_use("right_stick_use"),
            left_trigger: trigger("left_trigger"),
            right_trigger: trigger("right_trigger"),
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
        self.bindings
            .get(&input)
            .copied()
            .unwrap_or(Binding::Default)
    }

    pub fn bind(&mut self, input: Input, binding: Binding) {
        match binding {
            Binding::Default => self.bindings.remove(&input),
            binding => self.bindings.insert(input, binding),
        };
    }

    pub fn stick(&self, side: StickSide) -> &StickTuning {
        match side {
            StickSide::Left => &self.left_stick,
            StickSide::Right => &self.right_stick,
        }
    }

    pub fn change_stick(&mut self, side: StickSide, change: StickChange) {
        match side {
            StickSide::Left => self.left_stick.change(change),
            StickSide::Right => self.right_stick.change(change),
        }
    }

    pub fn stick_use(&self, side: StickSide) -> &StickRole {
        match side {
            StickSide::Left => &self.left_stick_use,
            StickSide::Right => &self.right_stick_use,
        }
    }

    pub fn change_stick_use(&mut self, side: StickSide, change: StickUseChange) {
        match side {
            StickSide::Left => self.left_stick_use.change(change),
            StickSide::Right => self.right_stick_use.change(change),
        }
    }

    pub fn trigger(&self, side: TriggerSide) -> &TriggerTuning {
        match side {
            TriggerSide::Left => &self.left_trigger,
            TriggerSide::Right => &self.right_trigger,
        }
    }

    pub fn change_trigger(&mut self, side: TriggerSide, change: TriggerChange) {
        match side {
            TriggerSide::Left => self.left_trigger.change(change),
            TriggerSide::Right => self.right_trigger.change(change),
        }
    }

    pub fn is_plain(&self) -> bool {
        self.output_type.is_none()
            && self.bindings.is_empty()
            && self.left_stick.is_neutral()
            && self.right_stick.is_neutral()
            && !self.swap_sticks
            && self.left_stick_use.is_default()
            && self.right_stick_use.is_default()
            && self.left_trigger.is_neutral()
            && self.right_trigger.is_neutral()
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mapped {
    pub shown: ControllerState,
    pub desktop: DesktopState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapper {
    slots: [Binding; Input::ALL.len()],
    left_stick: StickTuning,
    right_stick: StickTuning,
    swap_sticks: bool,
    left_stick_use: StickRole,
    right_stick_use: StickRole,
    left_trigger: TriggerTuning,
    right_trigger: TriggerTuning,
}

impl Default for Mapper {
    fn default() -> Self {
        Self {
            slots: [Binding::Default; Input::ALL.len()],
            left_stick: StickTuning::default(),
            right_stick: StickTuning::default(),
            swap_sticks: false,
            left_stick_use: StickRole::default(),
            right_stick_use: StickRole::default(),
            left_trigger: TriggerTuning::default(),
            right_trigger: TriggerTuning::default(),
        }
    }
}

impl Mapper {
    pub fn new(profile: &Profile) -> Self {
        let mut mapper = Self {
            left_stick: profile.left_stick,
            right_stick: profile.right_stick,
            swap_sticks: profile.swap_sticks,
            left_stick_use: profile.left_stick_use,
            right_stick_use: profile.right_stick_use,
            left_trigger: profile.left_trigger,
            right_trigger: profile.right_trigger,
            ..Self::default()
        };
        for input in Input::ALL {
            mapper.slots[input.index()] = profile.binding(input);
        }
        mapper
    }

    pub fn map(&self, state: &ControllerState) -> ControllerState {
        self.mapped(state).shown
    }

    pub fn mapped(&self, state: &ControllerState) -> Mapped {
        let mut buttons = Buttons::NONE;
        let mut left_full = false;
        let mut right_full = false;
        let mut desktop = DesktopState::default();
        let mut drive = |source: Input, own: Buttons| match self.slot(source) {
            Binding::Default => buttons |= own,
            Binding::Nothing => {}
            Binding::To(Output::LeftTrigger) => left_full = true,
            Binding::To(Output::RightTrigger) => right_full = true,
            Binding::To(output) => buttons |= output.buttons(),
            Binding::Keys(chord) => desktop.add_chord(chord),
            Binding::Mouse(button) => desktop.add_button(button),
            Binding::Wheel(step) => desktop.add_wheel(source.index(), step),
        };
        for input in Input::ALL {
            let own = input.buttons();
            if !self.pressed(input, state) {
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
                    let source = half
                        .filter(|&half| self.is_bound(half))
                        .unwrap_or(Input::Touchpad);
                    drive(source, own);
                }
                Input::TouchpadLeft | Input::TouchpadRight => {}
                _ => drive(input, own),
            }
        }
        if left_full || self.hair_trigger_fires(Input::L2, state.left_trigger) {
            buttons |= Buttons::L2;
        }
        if right_full || self.hair_trigger_fires(Input::R2, state.right_trigger) {
            buttons |= Buttons::R2;
        }
        let (left_stick, right_stick, fine_sticks) = self.sticks(state);
        self.left_stick_use
            .press(&self.left_stick, state.left_stick, &mut desktop);
        self.right_stick_use
            .press(&self.right_stick, state.right_stick, &mut desktop);
        let shown = ControllerState {
            buttons,
            left_trigger: self.trigger(Input::L2, left_full, state.left_trigger),
            right_trigger: self.trigger(Input::R2, right_full, state.right_trigger),
            left_stick,
            right_stick,
            fine_sticks,
            ..*state
        };
        Mapped { shown, desktop }
    }

    pub fn velocity(&self, state: &ControllerState) -> Velocity {
        let left = self
            .left_stick_use
            .velocity(&self.left_stick, state.left_stick);
        let right = self
            .right_stick_use
            .velocity(&self.right_stick, state.right_stick);
        left + right
    }

    fn sticks(&self, state: &ControllerState) -> (Stick, Stick, Option<FineSticks>) {
        let both_sticks = self.left_stick_use.is_stick() && self.right_stick_use.is_stick();
        if both_sticks && self.left_stick.is_neutral() && self.right_stick.is_neutral() {
            return if self.swap_sticks {
                (state.right_stick, state.left_stick, state.fine_sticks)
            } else {
                (state.left_stick, state.right_stick, state.fine_sticks)
            };
        }
        let own = |role: &StickRole, tuning: &StickTuning, stick: Stick| {
            if role.is_stick() {
                tuned_stick(tuning, stick)
            } else {
                (Stick::CENTER, (0, 0))
            }
        };
        let (left, left_wide) = own(&self.left_stick_use, &self.left_stick, state.left_stick);
        let (right, right_wide) = own(&self.right_stick_use, &self.right_stick, state.right_stick);
        if self.swap_sticks {
            (right, left, Some(fine(right_wide, left_wide)))
        } else {
            (left, right, Some(fine(left_wide, right_wide)))
        }
    }

    fn tuning_of(&self, input: Input) -> &TriggerTuning {
        match input {
            Input::L2 => &self.left_trigger,
            _ => &self.right_trigger,
        }
    }

    fn pressed(&self, input: Input, state: &ControllerState) -> bool {
        let travel = match input {
            Input::L2 => state.left_trigger,
            Input::R2 => state.right_trigger,
            _ => return state.buttons.contains(input.buttons()),
        };
        match self.tuning_of(input).presses(travel) {
            Some(pressed) if self.is_bound(input) => pressed,
            _ => state.buttons.contains(input.buttons()),
        }
    }

    fn hair_trigger_fires(&self, input: Input, travel: u8) -> bool {
        !self.is_bound(input) && self.tuning_of(input).fires_hair_trigger(travel)
    }

    fn slot(&self, input: Input) -> Binding {
        self.slots[input.index()]
    }

    fn is_bound(&self, input: Input) -> bool {
        self.slot(input) != Binding::Default
    }

    fn trigger(&self, input: Input, full: bool, travel: u8) -> u8 {
        if full {
            u8::MAX
        } else if !self.is_bound(input) {
            self.tuning_of(input).tune(travel)
        } else {
            0
        }
    }
}
