use std::ops::Add;

use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use crate::desktop::{Chord, DesktopState, Key};
use crate::state::Stick;
use crate::tuning::{StickTuning, unit};

pub const MIN_SPEED: u16 = 10;
pub const MAX_SPEED: u16 = 300;

pub const POINTER_PIXELS_A_SECOND: f32 = 1000.0;
pub const SCROLL_NOTCHES_A_SECOND: f32 = 20.0;

const POINTING_FLOOR: f32 = 0.06;
const KEY_FLOOR: f32 = 0.25;
const SIN_OF_HALF_A_SECTOR: f32 = 0.382_683;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StickUse {
    #[default]
    Stick,
    Mouse,
    Scroll,
    Wasd,
    Arrows,
    Keys,
}

impl StickUse {
    pub const ALL: [Self; 6] = [
        Self::Stick,
        Self::Mouse,
        Self::Scroll,
        Self::Wasd,
        Self::Arrows,
        Self::Keys,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    Left,
    Down,
    Right,
}

impl Direction {
    pub const ALL: [Self; 4] = [Self::Up, Self::Left, Self::Down, Self::Right];

    fn name(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Left => "left",
            Self::Down => "down",
            Self::Right => "right",
        }
    }
}

const WASD: [Key; 4] = [
    Key::new(0x11),
    Key::new(0x1E),
    Key::new(0x1F),
    Key::new(0x20),
];

const ARROWS: [Key; 4] = [
    Key::extended(0x48),
    Key::extended(0x4B),
    Key::extended(0x50),
    Key::extended(0x4D),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickUseChange {
    Use(StickUse),
    Speed(u16),
    Key(Direction, Chord),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StickRole {
    pub used_as: StickUse,
    pub speed: u16,
    keys: [Option<Chord>; 4],
}

impl Default for StickRole {
    fn default() -> Self {
        Self {
            used_as: StickUse::Stick,
            speed: 100,
            keys: [None; 4],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Velocity {
    pub pointer: (f32, f32),
    pub scroll: (f32, f32),
}

impl Add for Velocity {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        let sum = |a: (f32, f32), b: (f32, f32)| (a.0 + b.0, a.1 + b.1);
        Self {
            pointer: sum(self.pointer, other.pointer),
            scroll: sum(self.scroll, other.scroll),
        }
    }
}

impl StickRole {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn is_stick(&self) -> bool {
        self.used_as == StickUse::Stick
    }

    pub fn key(&self, direction: Direction) -> Option<Chord> {
        self.keys[direction as usize]
    }

    pub fn change(&mut self, change: StickUseChange) {
        match change {
            StickUseChange::Use(used_as) => self.used_as = used_as,
            StickUseChange::Speed(percent) => {
                self.speed = percent.clamp(MIN_SPEED, MAX_SPEED);
            }
            StickUseChange::Key(direction, chord) => {
                self.keys[direction as usize] = Some(chord);
            }
        }
    }

    pub(crate) fn load(value: &Value) -> Self {
        let mut role = Self::default();
        let field = |name: &str| value.get(name);
        if let Some(used_as) = field("use").and_then(|used| StickUse::deserialize(used).ok()) {
            role.used_as = used_as;
        }
        if let Some(speed) = field("speed").and_then(Value::as_u64) {
            role.change(StickUseChange::Speed(
                u16::try_from(speed).unwrap_or(MAX_SPEED),
            ));
        }
        for direction in Direction::ALL {
            let chord = field("keys")
                .and_then(|keys| keys.get(direction.name()))
                .and_then(Chord::load);
            role.keys[direction as usize] = chord;
        }
        role
    }

    pub fn press(&self, tuning: &StickTuning, stick: Stick, desktop: &mut DesktopState) {
        let keys = match self.used_as {
            StickUse::Wasd => WASD.map(|key| Some(Chord::of(key))),
            StickUse::Arrows => ARROWS.map(|key| Some(Chord::of(key))),
            StickUse::Keys => self.keys,
            StickUse::Stick | StickUse::Mouse | StickUse::Scroll => return,
        };
        let Some((x, y, _)) = pushed(tuning, stick, KEY_FLOOR) else {
            return;
        };
        let towards = [
            y < -SIN_OF_HALF_A_SECTOR,
            x < -SIN_OF_HALF_A_SECTOR,
            y > SIN_OF_HALF_A_SECTOR,
            x > SIN_OF_HALF_A_SECTOR,
        ];
        for (chord, pushed) in keys.into_iter().zip(towards) {
            if let Some(chord) = chord.filter(|_| pushed) {
                desktop.add_chord(chord);
            }
        }
    }

    pub fn velocity(&self, tuning: &StickTuning, stick: Stick) -> Velocity {
        let full = match self.used_as {
            StickUse::Mouse => POINTER_PIXELS_A_SECOND,
            StickUse::Scroll => SCROLL_NOTCHES_A_SECOND,
            _ => return Velocity::default(),
        };
        let Some((x, y, travel)) = pushed(tuning, stick, POINTING_FLOOR) else {
            return Velocity::default();
        };
        let speed = tuning.curve.apply(travel) * full * f32::from(self.speed) / 100.0;
        if self.used_as == StickUse::Mouse {
            Velocity {
                pointer: (x * speed, y * speed),
                ..Velocity::default()
            }
        } else {
            Velocity {
                scroll: (x * speed, -y * speed),
                ..Velocity::default()
            }
        }
    }
}

fn pushed(tuning: &StickTuning, stick: Stick, floor: f32) -> Option<(f32, f32, f32)> {
    let (x, y) = (unit(stick.x), unit(stick.y));
    let length = x.hypot(y);
    let dead = (f32::from(tuning.dead_zone) / 100.0).max(floor);
    (length > dead).then(|| {
        let travel = ((length - dead) / (1.0 - dead)).min(1.0);
        (x / length, y / length, travel)
    })
}

impl Serialize for StickRole {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Keys<'a>(&'a StickRole);
        impl Serialize for Keys<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(None)?;
                for direction in Direction::ALL {
                    if let Some(chord) = self.0.key(direction) {
                        map.serialize_entry(direction.name(), &chord)?;
                    }
                }
                map.end()
            }
        }
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("use", &self.used_as)?;
        map.serialize_entry("speed", &self.speed)?;
        if self.keys.iter().any(Option::is_some) {
            map.serialize_entry("keys", &Keys(self))?;
        }
        map.end()
    }
}
