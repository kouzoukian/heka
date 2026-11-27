use std::io;

use heka_core::desktop::{DesktopError, DesktopEvent, DesktopOutput, Key, MouseButton, WheelStep};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN,
    MOUSEEVENTF_XUP, MOUSEINPUT, SendInput, VK_PAUSE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{WHEEL_DELTA, XBUTTON1, XBUTTON2};

#[derive(Debug, Default, Clone, Copy)]
pub struct SendInputDesktop;

impl DesktopOutput for SendInputDesktop {
    fn send(&mut self, events: &[DesktopEvent]) -> Result<(), DesktopError> {
        let inputs: Vec<INPUT> = events.iter().flat_map(|&event| inputs_of(event)).collect();
        let count = u32::try_from(inputs.len()).expect("a handful of events fit in 32 bits");
        let size = size_of::<INPUT>() as i32;
        let sent = unsafe { SendInput(count, inputs.as_ptr(), size) };
        if sent == count {
            Ok(())
        } else {
            Err(DesktopError::new(io::Error::last_os_error().to_string()))
        }
    }
}

fn inputs_of(event: DesktopEvent) -> impl Iterator<Item = INPUT> {
    let (first, second) = match event {
        DesktopEvent::Key { key, down } => (Some(keyboard(key, down)), None),
        DesktopEvent::Button { button, down } => {
            let (pressed, released, data) = match button {
                MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0),
                MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
                MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
                MouseButton::Back => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, u32::from(XBUTTON1)),
                MouseButton::Forward => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, u32::from(XBUTTON2)),
            };
            (
                Some(mouse(if down { pressed } else { released }, data)),
                None,
            )
        }
        DesktopEvent::Wheel(step) => {
            let turned = match step {
                WheelStep::Up => 1,
                WheelStep::Down => -1,
            };
            (Some(wheel(MOUSEEVENTF_WHEEL, turned)), None)
        }
        DesktopEvent::Move { x, y } => (Some(movement(x, y)), None),
        DesktopEvent::Scroll { x, y } => (
            (x != 0).then(|| wheel(MOUSEEVENTF_HWHEEL, x)),
            (y != 0).then(|| wheel(MOUSEEVENTF_WHEEL, y)),
        ),
    };
    first.into_iter().chain(second)
}

fn wheel(flags: u32, notches: i32) -> INPUT {
    let turned = notches.saturating_mul(WHEEL_DELTA as i32);
    mouse(flags, turned as u32)
}

fn movement(x: i32, y: i32) -> INPUT {
    let mut input = mouse(MOUSEEVENTF_MOVE, 0);
    input.Anonymous.mi.dx = x;
    input.Anonymous.mi.dy = y;
    input
}

fn keyboard(key: Key, down: bool) -> INPUT {
    let (virtual_key, mut flags) = if key == Key::PAUSE {
        (VK_PAUSE, 0)
    } else {
        (0, KEYEVENTF_SCANCODE)
    };
    if key.is_extended() {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: virtual_key,
                wScan: u16::from(key.code()),
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn mouse(flags: u32, data: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
