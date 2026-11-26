//! Controller state: what a Controller's inputs are at one moment, whatever its model or Link.

use std::fmt;
use std::ops::{BitOr, BitOrAssign};

/// A set of pressed buttons. The d-pad is four buttons so diagonals are two.
///
/// Names follow the DualSense. On a DualShock 4, Share is [`Buttons::CREATE`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Buttons(u32);

impl Buttons {
    pub const NONE: Self = Self(0);
    pub const CROSS: Self = Self(1 << 0);
    pub const CIRCLE: Self = Self(1 << 1);
    pub const SQUARE: Self = Self(1 << 2);
    pub const TRIANGLE: Self = Self(1 << 3);
    pub const L1: Self = Self(1 << 4);
    pub const R1: Self = Self(1 << 5);
    /// The L2 trigger's digital click. Its travel is [`ControllerState::left_trigger`].
    pub const L2: Self = Self(1 << 6);
    /// The R2 trigger's digital click. Its travel is [`ControllerState::right_trigger`].
    pub const R2: Self = Self(1 << 7);
    pub const L3: Self = Self(1 << 8);
    pub const R3: Self = Self(1 << 9);
    pub const CREATE: Self = Self(1 << 10);
    pub const OPTIONS: Self = Self(1 << 11);
    pub const PS: Self = Self(1 << 12);
    pub const TOUCHPAD: Self = Self(1 << 13);
    pub const MUTE: Self = Self(1 << 14);
    pub const DPAD_UP: Self = Self(1 << 15);
    pub const DPAD_DOWN: Self = Self(1 << 16);
    pub const DPAD_LEFT: Self = Self(1 << 17);
    pub const DPAD_RIGHT: Self = Self(1 << 18);

    /// Every button with a display name, in the order a live input view lists them.
    pub const NAMED: [(Self, &'static str); 19] = [
        (Self::CROSS, "Cross"),
        (Self::CIRCLE, "Circle"),
        (Self::SQUARE, "Square"),
        (Self::TRIANGLE, "Triangle"),
        (Self::DPAD_UP, "Up"),
        (Self::DPAD_DOWN, "Down"),
        (Self::DPAD_LEFT, "Left"),
        (Self::DPAD_RIGHT, "Right"),
        (Self::L1, "L1"),
        (Self::R1, "R1"),
        (Self::L2, "L2"),
        (Self::R2, "R2"),
        (Self::L3, "L3"),
        (Self::R3, "R3"),
        (Self::CREATE, "Create"),
        (Self::OPTIONS, "Options"),
        (Self::PS, "PS"),
        (Self::TOUCHPAD, "Touchpad"),
        (Self::MUTE, "Mute"),
    ];

    /// True if every button in `other` is pressed.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Presses `other` too when `pressed` is true.
    pub fn set(&mut self, other: Self, pressed: bool) {
        if pressed {
            self.0 |= other.0;
        } else {
            self.0 &= !other.0;
        }
    }
}

impl BitOr for Buttons {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Buttons {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl fmt::Debug for Buttons {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pressed = Self::NAMED
            .iter()
            .filter(|(button, _)| self.contains(*button))
            .map(|(_, name)| name);
        f.debug_set().entries(pressed).finish()
    }
}

/// One analog stick. Each axis runs `0..=255`: `0` is fully left or fully up,
/// `255` fully right or fully down, and [`Stick::CENTER`] is centred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stick {
    pub x: u8,
    pub y: u8,
}

impl Stick {
    pub const CENTER: Self = Self { x: 0x80, y: 0x80 };
}

/// Whether a Controller's battery is charging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Charge {
    Discharging,
    Charging,
    Full,
    /// Plugged in but not charging, for example because it is too hot or too cold.
    NotCharging,
    /// The Controller reports a charging error or a state heka does not know.
    Unknown,
}

/// A Controller's battery as it last reported it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Battery {
    /// `0..=100`. Controllers report it in steps of 10%.
    pub percent: u8,
    pub charge: Charge,
}

/// Everything a Controller reports that heka passes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControllerState {
    pub buttons: Buttons,
    pub left_stick: Stick,
    pub right_stick: Stick,
    /// L2 travel, `0` released to `255` fully pressed.
    pub left_trigger: u8,
    /// R2 travel, `0` released to `255` fully pressed.
    pub right_trigger: u8,
    /// `None` when the report carries no battery level.
    pub battery: Option<Battery>,
}

impl ControllerState {
    /// Everything released and both sticks centred.
    pub const NEUTRAL: Self = Self {
        buttons: Buttons::NONE,
        left_stick: Stick::CENTER,
        right_stick: Stick::CENTER,
        left_trigger: 0,
        right_trigger: 0,
        battery: None,
    };
}

impl Default for ControllerState {
    fn default() -> Self {
        Self::NEUTRAL
    }
}
