use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::profile::Input;
use crate::state::Buttons;

pub const TURN_OFF_HOLD: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutAction {
    NextProfile,
    PreviousProfile,
    TurnOff,
}

impl ShortcutAction {
    pub const ALL: [Self; 3] = [Self::NextProfile, Self::PreviousProfile, Self::TurnOff];

    fn index(self) -> usize {
        self as usize
    }
}

pub fn can_be_chorded(input: Input) -> bool {
    !matches!(
        input,
        Input::Ps | Input::TouchpadLeft | Input::TouchpadRight | Input::L2 | Input::R2
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Shortcuts {
    pub enabled: bool,
    pub next: Option<Input>,
    pub previous: Option<Input>,
    pub turn_off: Option<Input>,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            enabled: false,
            next: Some(Input::R1),
            previous: Some(Input::L1),
            turn_off: Some(Input::Options),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutChange {
    Enabled(bool),
    Choose(ShortcutAction, Input),
}

impl Shortcuts {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn button(&self, action: ShortcutAction) -> Option<Input> {
        match action {
            ShortcutAction::NextProfile => self.next,
            ShortcutAction::PreviousProfile => self.previous,
            ShortcutAction::TurnOff => self.turn_off,
        }
    }

    pub fn taken_by(&self, input: Input) -> Option<ShortcutAction> {
        ShortcutAction::ALL
            .into_iter()
            .find(|&action| self.button(action) == Some(input))
    }

    pub fn change(&mut self, change: ShortcutChange) -> bool {
        match change {
            ShortcutChange::Enabled(enabled) => {
                self.enabled = enabled;
                true
            }
            ShortcutChange::Choose(action, input) => {
                let free = self.taken_by(input).is_none_or(|owner| owner == action);
                if !can_be_chorded(input) || !free {
                    return false;
                }
                match action {
                    ShortcutAction::NextProfile => self.next = Some(input),
                    ShortcutAction::PreviousProfile => self.previous = Some(input),
                    ShortcutAction::TurnOff => self.turn_off = Some(input),
                }
                true
            }
        }
    }

    pub(crate) fn load(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        let defaults = Self::default();
        let mut loaded = Self {
            enabled: given(value.get("enabled")).unwrap_or(defaults.enabled),
            next: chosen(value, "next", defaults.next),
            previous: chosen(value, "previous", defaults.previous),
            turn_off: chosen(value, "turn_off", defaults.turn_off),
        };
        let mut taken: Vec<Input> = Vec::new();
        for action in ShortcutAction::ALL {
            let button = loaded.button(action);
            let usable = button.filter(|button| !taken.contains(button));
            if let Some(button) = usable {
                taken.push(button);
            }
            match action {
                ShortcutAction::NextProfile => loaded.next = usable,
                ShortcutAction::PreviousProfile => loaded.previous = usable,
                ShortcutAction::TurnOff => loaded.turn_off = usable,
            }
        }
        loaded
    }

    pub fn chords(&self) -> [Option<(ShortcutAction, Buttons)>; 3] {
        ShortcutAction::ALL.map(|action| {
            self.button(action)
                .filter(|&input| can_be_chorded(input))
                .map(|input| (action, input.buttons()))
        })
    }
}

fn chosen(value: &Value, key: &str, default: Option<Input>) -> Option<Input> {
    match value.get(key) {
        None => default,
        Some(Value::Null) => None,
        Some(other) => given::<Input>(Some(other)).filter(|&input| can_be_chorded(input)),
    }
}

fn given<T: DeserializeOwned>(value: Option<&Value>) -> Option<T> {
    T::deserialize(value?).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observed {
    pub hide: Buttons,
    pub action: Option<ShortcutAction>,
}

#[derive(Debug, Default)]
pub struct ShortcutDetector {
    ps: bool,
    down: [bool; 3],
    hidden: [bool; 3],
    off_since: Option<Instant>,
    off_fired: bool,
}

impl ShortcutDetector {
    pub fn observe(&mut self, shortcuts: &Shortcuts, buttons: Buttons, now: Instant) -> Observed {
        if !shortcuts.enabled {
            *self = Self::default();
            return Observed {
                hide: Buttons::NONE,
                action: None,
            };
        }
        let ps = buttons.contains(Buttons::PS);
        let chords = shortcuts.chords();
        let mut hide = Buttons::NONE;
        let mut action = None;
        for (chord, slot) in chords.iter().zip(0..) {
            let Some((what, button)) = *chord else {
                self.hidden[slot] = false;
                self.down[slot] = false;
                continue;
            };
            let down = buttons.contains(button);
            if self.hidden[slot] {
                if down {
                    hide |= button;
                } else {
                    self.hidden[slot] = false;
                }
            } else if down && !self.down[slot] && ps && self.ps {
                self.hidden[slot] = true;
                hide |= button;
                if what == ShortcutAction::TurnOff {
                    self.off_since = Some(now);
                    self.off_fired = false;
                } else if action.is_none() {
                    action = Some(what);
                }
            }
            self.down[slot] = down;
        }
        self.time_turn_off(&chords, ps, now, &mut action);
        self.ps = ps;
        Observed { hide, action }
    }

    fn time_turn_off(
        &mut self,
        chords: &[Option<(ShortcutAction, Buttons)>; 3],
        ps: bool,
        now: Instant,
        action: &mut Option<ShortcutAction>,
    ) {
        let Some(since) = self.off_since else {
            return;
        };
        let held = self.hidden[ShortcutAction::TurnOff.index()]
            && chords[ShortcutAction::TurnOff.index()].is_some();
        if !(ps && held) {
            self.off_since = None;
            return;
        }
        if !self.off_fired && now.saturating_duration_since(since) >= TURN_OFF_HOLD {
            self.off_fired = true;
            if action.is_none() {
                *action = Some(ShortcutAction::TurnOff);
            }
        }
    }
}

pub fn cycle(current: Option<&str>, ids: &[&str], forward: bool) -> Option<String> {
    let places = ids.len() + 1;
    let here = current
        .and_then(|id| ids.iter().position(|candidate| *candidate == id))
        .map_or(0, |index| index + 1);
    let next = if forward {
        (here + 1) % places
    } else {
        (here + places - 1) % places
    };
    next.checked_sub(1).map(|index| ids[index].to_owned())
}
