use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

const EXTENDED_PREFIX: u16 = 0xE000;
const KEY_SLOTS: usize = 512;
const KEY_WORDS: usize = KEY_SLOTS / 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    code: u8,
    extended: bool,
}

impl Key {
    pub const ESCAPE: Self = Self::new(0x01);
    pub const LEFT_CTRL: Self = Self::new(0x1D);
    pub const RIGHT_CTRL: Self = Self::extended(0x1D);
    pub const LEFT_ALT: Self = Self::new(0x38);
    pub const RIGHT_ALT: Self = Self::extended(0x38);
    pub const LEFT_SHIFT: Self = Self::new(0x2A);
    pub const RIGHT_SHIFT: Self = Self::new(0x36);
    pub const LEFT_WINDOWS: Self = Self::extended(0x5B);
    pub const RIGHT_WINDOWS: Self = Self::extended(0x5C);
    pub const PRINT_SCREEN: Self = Self::extended(0x37);
    pub const PAUSE: Self = Self::new(0x45);

    pub const MODIFIERS: [Self; 8] = [
        Self::LEFT_CTRL,
        Self::RIGHT_CTRL,
        Self::LEFT_ALT,
        Self::RIGHT_ALT,
        Self::LEFT_SHIFT,
        Self::RIGHT_SHIFT,
        Self::LEFT_WINDOWS,
        Self::RIGHT_WINDOWS,
    ];

    pub const fn new(code: u8) -> Self {
        Self {
            code,
            extended: false,
        }
    }

    pub const fn extended(code: u8) -> Self {
        Self {
            code,
            extended: true,
        }
    }

    pub fn from_scan(scan: u16) -> Option<Self> {
        let [prefix, code] = scan.to_be_bytes();
        match prefix {
            _ if code == 0 => None,
            0 => Some(Self::new(code)),
            0xE0 => Some(Self::extended(code)),
            _ => None,
        }
    }

    pub fn scan(self) -> u16 {
        let prefix = if self.extended { EXTENDED_PREFIX } else { 0 };
        prefix | u16::from(self.code)
    }

    pub fn code(self) -> u8 {
        self.code
    }

    pub fn is_extended(self) -> bool {
        self.extended
    }

    pub fn is_modifier(self) -> bool {
        self.modifier_bit().is_some()
    }

    fn modifier_bit(self) -> Option<u8> {
        Self::MODIFIERS
            .iter()
            .position(|&modifier| modifier == self)
            .map(|index| 1 << index)
    }

    fn index(self) -> usize {
        usize::from(self.code) + if self.extended { KEY_SLOTS / 2 } else { 0 }
    }

    fn at(index: usize) -> Self {
        Self {
            code: (index % (KEY_SLOTS / 2)) as u8,
            extended: index >= KEY_SLOTS / 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Chord {
    modifiers: u8,
    key: Key,
}

impl Serialize for Chord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.keys().map(Key::scan))
    }
}

impl Chord {
    pub(crate) fn load(value: &Value) -> Option<Self> {
        let keys = Vec::<u16>::deserialize(value)
            .ok()?
            .into_iter()
            .map(Key::from_scan)
            .collect::<Option<Vec<_>>>()?;
        Self::new(&keys)
    }

    pub fn of(key: Key) -> Self {
        Self { modifiers: 0, key }
    }

    pub fn new(keys: &[Key]) -> Option<Self> {
        let (&key, before) = keys.split_last()?;
        let mut modifiers = 0;
        for &modifier in before {
            modifiers |= modifier.modifier_bit()?;
        }
        modifiers &= !key.modifier_bit().unwrap_or(0);
        Some(Self { modifiers, key })
    }

    pub fn key(self) -> Key {
        self.key
    }

    pub fn modifiers(self) -> impl Iterator<Item = Key> {
        Key::MODIFIERS
            .into_iter()
            .enumerate()
            .filter(move |(index, _)| self.modifiers & (1 << index) != 0)
            .map(|(_, modifier)| modifier)
    }

    pub fn keys(self) -> impl Iterator<Item = Key> {
        self.modifiers().chain([self.key])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl MouseButton {
    pub const ALL: [Self; 5] = [
        Self::Left,
        Self::Right,
        Self::Middle,
        Self::Back,
        Self::Forward,
    ];

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WheelStep {
    Up,
    Down,
}

impl WheelStep {
    pub const ALL: [Self; 2] = [Self::Up, Self::Down];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopEvent {
    Key { key: Key, down: bool },
    Button { button: MouseButton, down: bool },
    Wheel(WheelStep),
    Move { x: i32, y: i32 },
    Scroll { x: i32, y: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Nudge {
    pub pointer: (i32, i32),
    pub scroll: (i32, i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopError(String);

impl DesktopError {
    pub fn new(reason: impl Into<String>) -> Self {
        Self(reason.into())
    }
}

impl fmt::Display for DesktopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DesktopError {}

pub trait DesktopOutput: Send {
    fn send(&mut self, events: &[DesktopEvent]) -> Result<(), DesktopError>;
}

struct Nowhere;

impl DesktopOutput for Nowhere {
    fn send(&mut self, _: &[DesktopEvent]) -> Result<(), DesktopError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct KeySet([u64; KEY_WORDS]);

impl KeySet {
    fn set(&mut self, key: Key, on: bool) {
        let index = key.index();
        if on {
            self.0[index / 64] |= 1 << (index % 64);
        } else {
            self.0[index / 64] &= !(1 << (index % 64));
        }
    }

    fn contains(&self, key: Key) -> bool {
        let index = key.index();
        self.0[index / 64] & (1 << (index % 64)) != 0
    }

    fn without(&self, other: &Self) -> Self {
        Self(std::array::from_fn(|word| self.0[word] & !other.0[word]))
    }

    fn iter(self) -> impl Iterator<Item = Key> {
        self.0.into_iter().enumerate().flat_map(|(word, mut bits)| {
            std::iter::from_fn(move || {
                (bits != 0).then(|| {
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    Key::at(word * 64 + bit)
                })
            })
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DesktopState {
    keys: KeySet,
    buttons: u8,
    wheel_up: u32,
    wheel_down: u32,
}

impl DesktopState {
    pub fn add_chord(&mut self, chord: Chord) {
        for key in chord.keys() {
            self.keys.set(key, true);
        }
    }

    pub fn add_button(&mut self, button: MouseButton) {
        self.buttons |= button.bit();
    }

    pub fn add_wheel(&mut self, source: usize, step: WheelStep) {
        let sources = match step {
            WheelStep::Up => &mut self.wheel_up,
            WheelStep::Down => &mut self.wheel_down,
        };
        *sources |= 1 << source;
    }
}

struct Desktop {
    output: Box<dyn DesktopOutput>,
    key_holders: [u16; KEY_SLOTS],
    button_holders: [u16; MouseButton::ALL.len()],
    keys_down: KeySet,
    buttons_down: u8,
    failing: bool,
    failure: Option<DesktopError>,
}

impl Desktop {
    fn change(&mut self, old: &DesktopState, new: &DesktopState) {
        for key in old.keys.without(&new.keys).iter() {
            self.key_holders[key.index()] -= 1;
        }
        let taken = new.keys.without(&old.keys);
        for key in taken.iter() {
            self.key_holders[key.index()] += 1;
        }
        for button in MouseButton::ALL {
            let (was, is) = (old.buttons & button.bit(), new.buttons & button.bit());
            let holders = &mut self.button_holders[button as usize];
            if was != 0 && is == 0 {
                *holders -= 1;
            } else if was == 0 && is != 0 {
                *holders += 1;
            }
        }

        let mut events = self.releases();
        let pressed = |modifiers: bool| {
            taken
                .iter()
                .filter(move |key| key.is_modifier() == modifiers)
                .filter(|key| !self.keys_down.contains(*key))
                .map(|key| DesktopEvent::Key { key, down: true })
        };
        events.extend(pressed(true).chain(pressed(false)));
        events.extend(
            MouseButton::ALL
                .into_iter()
                .filter(|button| new.buttons & !old.buttons & button.bit() != 0)
                .filter(|button| self.buttons_down & button.bit() == 0)
                .map(|button| DesktopEvent::Button { button, down: true }),
        );
        for (step, turned) in [
            (WheelStep::Up, new.wheel_up & !old.wheel_up),
            (WheelStep::Down, new.wheel_down & !old.wheel_down),
        ] {
            let steps = turned.count_ones() as usize;
            events.extend(std::iter::repeat_n(DesktopEvent::Wheel(step), steps));
        }
        self.send(&events);
    }

    fn releases(&self) -> Vec<DesktopEvent> {
        let buttons = MouseButton::ALL
            .into_iter()
            .filter(|button| self.buttons_down & button.bit() != 0)
            .filter(|&button| self.button_holders[button as usize] == 0)
            .map(|button| DesktopEvent::Button {
                button,
                down: false,
            });
        let keys = |modifiers: bool| {
            self.keys_down
                .iter()
                .filter(move |key| key.is_modifier() == modifiers)
                .filter(|key| self.key_holders[key.index()] == 0)
                .map(|key| DesktopEvent::Key { key, down: false })
        };
        buttons.chain(keys(false)).chain(keys(true)).collect()
    }

    fn send(&mut self, events: &[DesktopEvent]) {
        if events.is_empty() {
            return;
        }
        let sent = self.output.send(events);
        for &event in events {
            match event {
                DesktopEvent::Key { key, down } if down || sent.is_ok() => {
                    self.keys_down.set(key, down);
                }
                DesktopEvent::Button { button, down: true } => self.buttons_down |= button.bit(),
                DesktopEvent::Button {
                    button,
                    down: false,
                } if sent.is_ok() => self.buttons_down &= !button.bit(),
                _ => {}
            }
        }
        match sent {
            Ok(()) => self.failing = false,
            Err(error) => {
                if !self.failing {
                    self.failure = Some(error);
                }
                self.failing = true;
            }
        }
    }
}

pub struct Desk(Mutex<Desktop>);

impl Desk {
    pub fn new(output: Box<dyn DesktopOutput>) -> Self {
        Self(Mutex::new(Desktop {
            output,
            key_holders: [0; KEY_SLOTS],
            button_holders: [0; MouseButton::ALL.len()],
            keys_down: KeySet::default(),
            buttons_down: 0,
            failing: false,
            failure: None,
        }))
    }

    pub fn settle(&self) {
        let mut desktop = self.desktop();
        let owed = desktop.releases();
        desktop.send(&owed);
    }

    pub fn take_failure(&self) -> Option<String> {
        self.desktop().failure.take().map(|error| error.to_string())
    }

    fn desktop(&self) -> MutexGuard<'_, Desktop> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for Desk {
    fn default() -> Self {
        Self::new(Box::new(Nowhere))
    }
}

pub struct Hands {
    desk: Arc<Desk>,
    held: DesktopState,
}

impl Hands {
    pub fn new(desk: Arc<Desk>) -> Self {
        Self {
            desk,
            held: DesktopState::default(),
        }
    }

    pub fn hold(&mut self, held: DesktopState) {
        if held != self.held {
            self.desk.desktop().change(&self.held, &held);
            self.held = held;
        }
    }

    pub fn nudge(&mut self, nudge: Nudge) {
        let Nudge {
            pointer: (x, y),
            scroll: (across, along),
        } = nudge;
        let moved = (x != 0 || y != 0).then_some(DesktopEvent::Move { x, y });
        let scrolled = (across != 0 || along != 0).then_some(DesktopEvent::Scroll {
            x: across,
            y: along,
        });
        if moved.is_some() || scrolled.is_some() {
            let events: Vec<DesktopEvent> = moved.into_iter().chain(scrolled).collect();
            self.desk.desktop().send(&events);
        }
    }

    pub fn start_over(&mut self, upcoming: DesktopState) {
        self.let_go();
        self.held.wheel_up = upcoming.wheel_up;
        self.held.wheel_down = upcoming.wheel_down;
    }

    pub fn let_go(&mut self) {
        self.hold(DesktopState {
            keys: KeySet::default(),
            buttons: 0,
            ..self.held
        });
    }
}

impl Drop for Hands {
    fn drop(&mut self) {
        self.let_go();
    }
}
