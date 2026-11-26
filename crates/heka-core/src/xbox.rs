//! Translation from Controller state to an Xbox 360 (XUSB) report.
//!
//! Button bits are the XInput `XINPUT_GAMEPAD_*` values (Microsoft, `XINPUT_GAMEPAD`
//! structure), plus [`buttons::GUIDE`] from ViGEmBus's `XUSB_GAMEPAD_GUIDE`.

use crate::state::{Buttons, ControllerState};

/// XUSB button bits.
pub mod buttons {
    pub const DPAD_UP: u16 = 0x0001;
    pub const DPAD_DOWN: u16 = 0x0002;
    pub const DPAD_LEFT: u16 = 0x0004;
    pub const DPAD_RIGHT: u16 = 0x0008;
    /// Start, labelled Menu on newer pads.
    pub const START: u16 = 0x0010;
    /// Back, labelled View on newer pads.
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

/// One Xbox 360 input report, laid out like `XUSB_REPORT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct XboxReport {
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    /// `-32768..=32767`, right positive.
    pub thumb_lx: i16,
    /// `-32768..=32767`, up positive.
    pub thumb_ly: i16,
    pub thumb_rx: i16,
    pub thumb_ry: i16,
}

/// Which Xbox button each Controller button drives. Touchpad click, mute and the
/// L2/R2 clicks drive nothing: the triggers' travel already carries L2 and R2.
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
    /// The Xbox report a Virtual Controller sends for this Controller state.
    pub fn from_state(state: &ControllerState) -> Self {
        let buttons = BUTTON_MAP
            .iter()
            .filter(|(from, _)| state.buttons.contains(*from))
            .fold(0, |bits, (_, to)| bits | to);
        Self {
            buttons,
            left_trigger: state.left_trigger,
            right_trigger: state.right_trigger,
            thumb_lx: axis(state.left_stick.x),
            thumb_ly: inverted_axis(state.left_stick.y),
            thumb_rx: axis(state.right_stick.x),
            thumb_ry: inverted_axis(state.right_stick.y),
        }
    }
}

/// Scales `0..=255` (centre 128) to `-32768..=32767` so that 128 lands exactly on 0
/// and both ends reach the full range.
fn axis(value: u8) -> i16 {
    let offset = i32::from(value) - 128;
    let scaled = if offset < 0 {
        offset * 256
    } else {
        offset * 32767 / 127
    };
    scaled as i16
}

/// [`axis`] flipped, for a Y axis that grows downward on the Controller and upward on Xbox.
fn inverted_axis(value: u8) -> i16 {
    (-i32::from(axis(value))).clamp(i16::MIN.into(), i16::MAX.into()) as i16
}
