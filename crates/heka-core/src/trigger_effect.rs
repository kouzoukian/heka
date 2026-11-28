use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ZONE: u8 = 10;
pub const LAST_START: u8 = 90;
pub const BREAK_FIRST_START: u8 = 20;
pub const BREAK_LAST_START: u8 = 70;
pub const BREAK_LAST_END: u8 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    #[default]
    Off,
    Resistance,
    Break,
    Vibration,
}

impl EffectKind {
    pub const ALL: [Self; 4] = [Self::Off, Self::Resistance, Self::Break, Self::Vibration];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectChange {
    Kind(EffectKind),
    Start(u8),
    End(u8),
    Strength(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerEffect {
    Off,
    Resistance { start: u8, strength: u8 },
    Break { start: u8, end: u8, strength: u8 },
    Vibration { start: u8, strength: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TriggerEffects {
    pub left: Option<TriggerEffect>,
    pub right: Option<TriggerEffect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct EffectSettings {
    pub effect: EffectKind,
    pub start: u8,
    pub end: u8,
    pub strength: u8,
}

impl Default for EffectSettings {
    fn default() -> Self {
        Self {
            effect: EffectKind::Off,
            start: 30,
            end: 70,
            strength: 60,
        }
    }
}

impl EffectSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn clamped(self) -> Self {
        let zoned = |percent: u8| percent / ZONE * ZONE;
        let (first, last) = match self.effect {
            EffectKind::Break => (BREAK_FIRST_START, BREAK_LAST_START),
            _ => (0, LAST_START),
        };
        let start = zoned(self.start).clamp(first, last);
        let end = match self.effect {
            EffectKind::Break => zoned(self.end).clamp(start + ZONE, BREAK_LAST_END),
            _ => zoned(self.end.min(100)),
        };
        Self {
            start,
            end,
            strength: self.strength.clamp(1, 100),
            ..self
        }
    }

    pub fn change(&mut self, change: EffectChange) {
        match change {
            EffectChange::Kind(effect) => self.effect = effect,
            EffectChange::Start(percent) => self.start = percent,
            EffectChange::End(percent) => self.end = percent,
            EffectChange::Strength(percent) => self.strength = percent,
        }
        *self = self.clamped();
    }

    pub fn effect(&self) -> Option<TriggerEffect> {
        let Self {
            effect,
            start,
            end,
            strength,
        } = self.clamped();
        match effect {
            EffectKind::Off => None,
            EffectKind::Resistance => Some(TriggerEffect::Resistance { start, strength }),
            EffectKind::Break => Some(TriggerEffect::Break {
                start,
                end,
                strength,
            }),
            EffectKind::Vibration => Some(TriggerEffect::Vibration { start, strength }),
        }
    }

    pub(crate) fn load(value: &Value) -> Self {
        let defaults = Self::default();
        Self {
            effect: given(value, "effect").unwrap_or(defaults.effect),
            start: given(value, "start").unwrap_or(defaults.start),
            end: given(value, "end").unwrap_or(defaults.end),
            strength: given(value, "strength").unwrap_or(defaults.strength),
        }
        .clamped()
    }
}

fn given<T: DeserializeOwned>(value: &Value, name: &str) -> Option<T> {
    value.get(name).and_then(|value| T::deserialize(value).ok())
}
