use heka_core::state::{Buttons, ControllerState, Stick};

const LAST_TOUCH_X: u16 = 1919;
const LAST_TOUCH_Y: u16 = 1079;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchDot {
    x: u16,
    y: u16,
}

impl TouchDot {
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

    pub fn share(self) -> (f32, f32) {
        (f32::from(self.x) / 1000., f32::from(self.y) / 1000.)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadModel {
    pub buttons: Buttons,
    pub left_stick: Stick,
    pub right_stick: Stick,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub touch: [Option<TouchDot>; 2],
}

impl PadModel {
    pub const NEUTRAL: Self = Self {
        buttons: Buttons::NONE,
        left_stick: Stick::CENTER,
        right_stick: Stick::CENTER,
        left_trigger: 0,
        right_trigger: 0,
        touch: [None, None],
    };

    pub fn from_state(state: &ControllerState) -> Self {
        let ControllerState {
            buttons,
            left_stick,
            right_stick,
            left_trigger,
            right_trigger,
            touch,
            battery: _,
            motion: _,
        } = *state;
        Self {
            buttons,
            left_stick,
            right_stick,
            left_trigger,
            right_trigger,
            touch: touch.map(|finger| {
                finger.map(|finger| {
                    TouchDot::from_units(finger.x, finger.y, LAST_TOUCH_X, LAST_TOUCH_Y)
                })
            }),
        }
    }

    pub fn pressed(&self, button: Buttons) -> bool {
        self.buttons.contains(button)
    }
}

pub fn stick_offset(stick: Stick) -> (f32, f32) {
    let axis = |value: u8| {
        let centre = f32::from(Stick::CENTER.x);
        ((f32::from(value) - centre) / (255. - centre)).max(-1.)
    };
    (axis(stick.x), axis(stick.y))
}

pub fn trigger_fill(travel: u8) -> f32 {
    f32::from(travel) / 255.
}
