use crate::state::{Buttons, ControllerState};

pub mod buttons {
    pub const DPAD_UP: u16 = 0x0001;
    pub const DPAD_DOWN: u16 = 0x0002;
    pub const DPAD_LEFT: u16 = 0x0004;
    pub const DPAD_RIGHT: u16 = 0x0008;
    pub const START: u16 = 0x0010;
    pub const BACK: u16 = 0x0020;
    pub const LEFT_THUMB: u16 = 0x0040;
    pub const RIGHT_THUMB: u16 = 0x0080;
    pub const LEFT_SHOULDER: u16 = 0x0100;
    pub const RIGHT_SHOULDER: u16 = 0x0200;
    pub const GUIDE: u16 = 0x0400;
    pub const A: u16 = 0x1000;
    pub const B: u16 = 0x2000;
    pub const X: u16 = 0x4000;
    pub const Y: u16 = 0x8000;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct XboxReport {
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub thumb_lx: i16,
    pub thumb_ly: i16,
    pub thumb_rx: i16,
    pub thumb_ry: i16,
}

const BUTTON_MAP: [(Buttons, u16); 15] = [
    (Buttons::CROSS, buttons::A),
    (Buttons::CIRCLE, buttons::B),
    (Buttons::SQUARE, buttons::X),
    (Buttons::TRIANGLE, buttons::Y),
    (Buttons::L1, buttons::LEFT_SHOULDER),
    (Buttons::R1, buttons::RIGHT_SHOULDER),
    (Buttons::L3, buttons::LEFT_THUMB),
    (Buttons::R3, buttons::RIGHT_THUMB),
    (Buttons::CREATE, buttons::BACK),
    (Buttons::OPTIONS, buttons::START),
    (Buttons::PS, buttons::GUIDE),
    (Buttons::DPAD_UP, buttons::DPAD_UP),
    (Buttons::DPAD_DOWN, buttons::DPAD_DOWN),
    (Buttons::DPAD_LEFT, buttons::DPAD_LEFT),
    (Buttons::DPAD_RIGHT, buttons::DPAD_RIGHT),
];

impl XboxReport {
    pub fn from_state(state: &ControllerState) -> Self {
        let buttons = BUTTON_MAP
            .iter()
            .filter(|(from, _)| state.buttons.contains(*from))
            .fold(0, |bits, (_, to)| bits | to);
        let (thumb_lx, thumb_ly, thumb_rx, thumb_ry) = match state.fine_sticks {
            Some(fine) => (
                fine.left_x,
                flipped(fine.left_y),
                fine.right_x,
                flipped(fine.right_y),
            ),
            None => (
                axis(state.left_stick.x),
                inverted_axis(state.left_stick.y),
                axis(state.right_stick.x),
                inverted_axis(state.right_stick.y),
            ),
        };
        Self {
            buttons,
            left_trigger: state.left_trigger,
            right_trigger: state.right_trigger,
            thumb_lx,
            thumb_ly,
            thumb_rx,
            thumb_ry,
        }
    }
}

fn flipped(value: i16) -> i16 {
    (-i32::from(value)).clamp(i16::MIN.into(), i16::MAX.into()) as i16
}

fn axis(value: u8) -> i16 {
    let offset = i32::from(value) - 128;
    let scaled = if offset < 0 {
        offset * 256
    } else {
        offset * 32767 / 127
    };
    scaled as i16
}

fn inverted_axis(value: u8) -> i16 {
    (-i32::from(axis(value))).clamp(i16::MIN.into(), i16::MAX.into()) as i16
}
