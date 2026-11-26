//! What the live drawing of a Controller shows, as plain data.

use heka_core::state::{Buttons, ControllerState, Stick};

/// A finger on the touchpad, as a share of the pad in thousandths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchDot {
    x: u16,
    y: u16,
}

impl TouchDot {
    /// A finger at `(x, y)` on a touchpad running `0..=last_x` and `0..=last_y`.
    pub fn from_units(x: u16, y: u16, last_x: u16, last_y: u16) -> Self {
        let share = |value: u16, last: u16| {
            let last = u32::from(last.max(1));
            (u32::from(value).min(last) * 1000 / last) as u16
        };
        Self {
            x: share(x, last_x),
            y: share(y, last_y),
        }
    }

    /// `(0, 0)` is the pad's top left corner and `(1, 1)` its bottom right.
    pub fn share(self) -> (f32, f32) {
        (f32::from(self.x) / 1000., f32::from(self.y) / 1000.)
    }
}

/// The parts of a Controller state that the drawing shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadModel {
    pub buttons: Buttons,
    pub left_stick: Stick,
    pub right_stick: Stick,
    pub left_trigger: u8,
    pub right_trigger: u8,
    /// Up to two fingers on the touchpad.
    pub touch: [Option<TouchDot>; 2],
}

impl PadModel {
    /// Everything released, both sticks centred and no finger down.
    pub const NEUTRAL: Self = Self {
        buttons: Buttons::NONE,
        left_stick: Stick::CENTER,
        right_stick: Stick::CENTER,
        left_trigger: 0,
        right_trigger: 0,
        touch: [None, None],
    };

    pub fn from_state(state: &ControllerState) -> Self {
        // Taken apart so that a new input cannot go unnoticed here.
        let ControllerState {
            buttons,
            left_stick,
            right_stick,
            left_trigger,
            right_trigger,
        } = *state;
        Self {
            buttons,
            left_stick,
            right_stick,
            left_trigger,
            right_trigger,
            // heka does not read finger positions yet (ticket #7 does).
            touch: [None, None],
        }
    }

    pub fn pressed(&self, button: Buttons) -> bool {
        self.buttons.contains(button)
    }
}

/// How far a stick is from its centre on each axis, `-1.0..=1.0`, positive to
/// the right and downwards.
pub fn stick_offset(stick: Stick) -> (f32, f32) {
    let axis = |value: u8| {
        let centre = f32::from(Stick::CENTER.x);
        ((f32::from(value) - centre) / (255. - centre)).max(-1.)
    };
    (axis(stick.x), axis(stick.y))
}

/// How far a trigger is pulled, `0.0..=1.0`.
pub fn trigger_fill(travel: u8) -> f32 {
    f32::from(travel) / 255.
}
