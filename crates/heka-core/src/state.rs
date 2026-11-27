use std::fmt;
use std::ops::{BitOr, BitOrAssign};

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
    pub const L2: Self = Self(1 << 6);
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
    pub const LEFT_FN: Self = Self(1 << 19);
    pub const RIGHT_FN: Self = Self(1 << 20);
    pub const LEFT_BACK: Self = Self(1 << 21);
    pub const RIGHT_BACK: Self = Self(1 << 22);

    pub const NAMED: [(Self, &'static str); 23] = [
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
        (Self::LEFT_FN, "Left Fn"),
        (Self::RIGHT_FN, "Right Fn"),
        (Self::LEFT_BACK, "Left back"),
        (Self::RIGHT_BACK, "Right back"),
    ];

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stick {
    pub x: u8,
    pub y: u8,
}

impl Stick {
    pub const CENTER: Self = Self { x: 0x80, y: 0x80 };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FineSticks {
    pub left_x: i16,
    pub left_y: i16,
    pub right_x: i16,
    pub right_y: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Charge {
    Discharging,
    Charging,
    Full,
    NotCharging,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Battery {
    pub percent: u8,
    pub charge: Charge,
}

pub const TOUCHPAD_WIDTH: u16 = 1920;
pub const TOUCHPAD_HEIGHT: u16 = 1080;
pub(crate) const DUALSHOCK4_TOUCHPAD_HEIGHT: u16 = 942;

pub(crate) fn rescale_touch_y(y: u16, from: u16, to: u16) -> u16 {
    let (from, to) = (u32::from(from) - 1, u32::from(to) - 1);
    let y = u32::from(y).min(from);
    ((y * to + from / 2) / from) as u16
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Finger {
    pub id: u8,
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Motion {
    pub gyro: [i32; 3],
    pub accel: [i32; 3],
    pub timestamp: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControllerState {
    pub buttons: Buttons,
    pub left_stick: Stick,
    pub right_stick: Stick,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub fine_sticks: Option<FineSticks>,
    pub battery: Option<Battery>,
    pub touch: [Option<Finger>; 2],
    pub motion: Motion,
}

impl ControllerState {
    pub const NEUTRAL: Self = Self {
        buttons: Buttons::NONE,
        left_stick: Stick::CENTER,
        right_stick: Stick::CENTER,
        left_trigger: 0,
        right_trigger: 0,
        fine_sticks: None,
        battery: None,
        touch: [None, None],
        motion: Motion {
            gyro: [0; 3],
            accel: [0; 3],
            timestamp: 0,
        },
    };
}

impl Default for ControllerState {
    fn default() -> Self {
        Self::NEUTRAL
    }
}
