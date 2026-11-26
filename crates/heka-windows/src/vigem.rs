use std::time::Duration;

use heka_core::feedback::{Feedback, FeedbackEvent, FeedbackSource, Lightbar, Rumble};
use heka_core::ps4::Ps4Translator;
use heka_core::state::ControllerState;
use heka_core::virtual_controller::{OutputType, VirtualBus, VirtualController, VirtualError};
use heka_core::xbox::XboxReport;
use vigem_rust::{
    Client, Ds4Notification, Ds4ReportEx, DualShock4, NotificationReceiver, Ready,
    RecvTimeoutError, TargetHandle, X360Button, X360Notification, X360Report, Xbox360,
};

pub struct VigemBus {
    client: Client,
}

impl VigemBus {
    pub fn connect() -> Result<Self, VirtualError> {
        let client = Client::connect().map_err(VirtualError::new)?;
        Ok(Self { client })
    }
}

impl VirtualBus for VigemBus {
    fn plug(&self, output: OutputType) -> Result<Box<dyn VirtualController>, VirtualError> {
        match output {
            OutputType::Xbox => {
                let target = self
                    .client
                    .new_x360_target()
                    .plug()
                    .and_then(|pending| pending.wait_for_ready())
                    .map_err(VirtualError::new)?;
                Ok(Box::new(VigemXbox { target }))
            }
            OutputType::Ps4 => {
                let target = self
                    .client
                    .new_ds4_target()
                    .plug()
                    .and_then(|pending| pending.wait_for_ready())
                    .map_err(VirtualError::new)?;
                Ok(Box::new(VigemPs4 {
                    target,
                    translator: Ps4Translator::default(),
                }))
            }
        }
    }
}

struct VigemPs4 {
    target: TargetHandle<DualShock4, Ready>,
    translator: Ps4Translator,
}

impl VirtualController for VigemPs4 {
    fn output_type(&self) -> OutputType {
        OutputType::Ps4
    }

    fn update(&mut self, state: &ControllerState) -> Result<(), VirtualError> {
        let report = Ds4ReportEx::from_bytes(self.translator.report(state));
        self.target.update_ex(&report).map_err(VirtualError::new)
    }

    fn unplug(self: Box<Self>) -> Result<(), VirtualError> {
        self.target.unplug().map_err(VirtualError::new)
    }

    fn feedback(&mut self) -> Result<Option<Box<dyn FeedbackSource>>, VirtualError> {
        let receiver = self
            .target
            .register_notification()
            .map_err(VirtualError::new)?;
        Ok(Some(Box::new(VigemPs4Feedback(receiver))))
    }
}

struct VigemPs4Feedback(NotificationReceiver<Ds4Notification>);

impl FeedbackSource for VigemPs4Feedback {
    fn next(&mut self, timeout: Duration) -> FeedbackEvent {
        match self.0.recv_timeout(timeout) {
            Ok(notification) => FeedbackEvent::Feedback(Feedback {
                rumble: Rumble {
                    large: notification.large_motor,
                    small: notification.small_motor,
                },
                lightbar: Some(Lightbar {
                    red: notification.lightbar.red,
                    green: notification.lightbar.green,
                    blue: notification.lightbar.blue,
                }),
                player_lights: None,
            }),
            Err(RecvTimeoutError::Timeout) => FeedbackEvent::Timeout,
            Err(_) => FeedbackEvent::Closed,
        }
    }
}

struct VigemXbox {
    target: TargetHandle<Xbox360, Ready>,
}

impl VirtualController for VigemXbox {
    fn output_type(&self) -> OutputType {
        OutputType::Xbox
    }

    fn update(&mut self, state: &ControllerState) -> Result<(), VirtualError> {
        self.target
            .update(&x360_report(&XboxReport::from_state(state)))
            .map_err(VirtualError::new)
    }

    fn unplug(self: Box<Self>) -> Result<(), VirtualError> {
        self.target.unplug().map_err(VirtualError::new)
    }

    fn feedback(&mut self) -> Result<Option<Box<dyn FeedbackSource>>, VirtualError> {
        let receiver = self
            .target
            .register_notification()
            .map_err(VirtualError::new)?;
        Ok(Some(Box::new(VigemXboxFeedback(receiver))))
    }
}

struct VigemXboxFeedback(NotificationReceiver<X360Notification>);

impl FeedbackSource for VigemXboxFeedback {
    fn next(&mut self, timeout: Duration) -> FeedbackEvent {
        match self.0.recv_timeout(timeout) {
            Ok(notification) => FeedbackEvent::Feedback(Feedback {
                rumble: Rumble {
                    large: notification.large_motor,
                    small: notification.small_motor,
                },
                lightbar: None,
                player_lights: None,
            }),
            Err(RecvTimeoutError::Timeout) => FeedbackEvent::Timeout,
            Err(_) => FeedbackEvent::Closed,
        }
    }
}

fn x360_report(report: &XboxReport) -> X360Report {
    X360Report {
        buttons: X360Button::from_bits_retain(report.buttons),
        left_trigger: report.left_trigger,
        right_trigger: report.right_trigger,
        thumb_lx: report.thumb_lx,
        thumb_ly: report.thumb_ly,
        thumb_rx: report.thumb_rx,
        thumb_ry: report.thumb_ry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use heka_core::xbox::buttons;

    #[test]
    fn xbox_button_bits_match_vigem_rust() {
        let pairs = [
            (buttons::DPAD_UP, X360Button::DPAD_UP),
            (buttons::DPAD_DOWN, X360Button::DPAD_DOWN),
            (buttons::DPAD_LEFT, X360Button::DPAD_LEFT),
            (buttons::DPAD_RIGHT, X360Button::DPAD_RIGHT),
            (buttons::START, X360Button::START),
            (buttons::BACK, X360Button::BACK),
            (buttons::LEFT_THUMB, X360Button::LEFT_THUMB),
            (buttons::RIGHT_THUMB, X360Button::RIGHT_THUMB),
            (buttons::LEFT_SHOULDER, X360Button::LEFT_SHOULDER),
            (buttons::RIGHT_SHOULDER, X360Button::RIGHT_SHOULDER),
            (buttons::GUIDE, X360Button::GUIDE),
            (buttons::A, X360Button::A),
            (buttons::B, X360Button::B),
            (buttons::X, X360Button::X),
            (buttons::Y, X360Button::Y),
        ];
        for (ours, theirs) in pairs {
            assert_eq!(ours, theirs.bits());
        }
    }

    #[test]
    fn ps4_report_bytes_land_on_vigem_rusts_ds4_fields() {
        use heka_core::state::{Buttons, Finger, Motion, Stick};
        use vigem_rust::{Ds4ButtonFlags, Ds4Dpad, Ds4SpecialButton, TouchState};

        let state = ControllerState {
            buttons: Buttons::CROSS
                | Buttons::CREATE
                | Buttons::R3
                | Buttons::PS
                | Buttons::TOUCHPAD
                | Buttons::DPAD_DOWN
                | Buttons::DPAD_LEFT,
            left_stick: Stick { x: 1, y: 2 },
            right_stick: Stick { x: 3, y: 4 },
            left_trigger: 5,
            right_trigger: 6,
            battery: None,
            touch: [
                Some(Finger {
                    id: 9,
                    x: 100,
                    y: 0,
                }),
                None,
            ],
            motion: Motion {
                gyro: [102_400, 0, -102_400],
                accel: [8192, 0, -8192],
                timestamp: 0x0001_2340,
            },
        };
        let bytes = Ps4Translator::default().report(&state);
        let report = Ds4ReportEx::from_bytes(bytes);
        let le16 = |at: usize| i16::from_le_bytes([bytes[at], bytes[at + 1]]);
        assert_eq!(
            [{ report.gyro_x }, { report.gyro_y }, { report.gyro_z }],
            [le16(12), 0, le16(16)]
        );
        assert!({ report.gyro_x } > 0 && { report.gyro_z } < 0);
        assert_eq!(
            [{ report.accel_x }, { report.accel_z }],
            [le16(18), le16(22)]
        );
        assert_eq!(
            (
                report.thumb_lx,
                report.thumb_ly,
                report.thumb_rx,
                report.thumb_ry
            ),
            (1, 2, 3, 4)
        );
        let buttons = report.buttons;
        assert_eq!(buttons.dpad(), Ds4Dpad::SouthWest);
        assert_eq!(
            buttons.flags(),
            Ds4ButtonFlags::CROSS | Ds4ButtonFlags::SHARE | Ds4ButtonFlags::THUMB_RIGHT
        );
        assert_eq!(
            report.special,
            Ds4SpecialButton::PS | Ds4SpecialButton::TOUCHPAD
        );
        assert_eq!((report.trigger_left, report.trigger_right), (5, 6));
        assert_eq!({ report.timestamp }, 0x1234);
        assert_eq!(report.touch_packets_n, 1);
        let touch = report.current_touch;
        assert!(matches!(
            touch.finger_1(),
            TouchState::Down { tracking_num: 9, at } if (at.x(), at.y()) == (100, 0)
        ));
        assert!(matches!(touch.finger_2(), TouchState::Up));
    }
}
