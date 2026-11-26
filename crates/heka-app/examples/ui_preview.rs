use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use heka_app::text;
use heka_app::ui::assets::Assets;
use heka_app::ui::model::{Backend, Command, ControllerEntry, EntryState, Notice, Screen};
use heka_app::ui::pad::{PadModel, TouchDot};
use heka_app::ui::view;
use heka_core::link::LinkKind;
use heka_core::mac::MacAddress;
use heka_core::preferences::Preferences;
use heka_core::setup::{Driver, Missing};
use heka_core::state::{Battery, Buttons, Charge, Stick};
use heka_core::supervisor::BridgeError;
use heka_core::virtual_controller::OutputType;

const REPORT_EVERY: Duration = Duration::from_secs(2);

struct Fake {
    started: Instant,
    controllers: RefCell<Vec<ControllerEntry>>,
    missing: RefCell<Vec<Missing>>,
    samples: RefCell<(u32, Instant)>,
}

impl Fake {
    fn new(empty: bool, missing: bool, first: usize) -> Self {
        let now = Instant::now();
        let entry = |last: u8, state, active_link, battery, output_type| ControllerEntry {
            mac: MacAddress([0x02, 0, 0, 0, 0, last]),
            name: text::DUALSENSE,
            nickname: None,
            has_player_lights: true,
            preferences: Preferences::default(),
            experimental: false,
            active_link,
            battery,
            output_type,
            state,
            bus_missing: missing,
            error: None,
            pad: None,
        };
        let level = |percent, charge| Some(Battery { percent, charge });
        let mut controllers = vec![
            entry(
                1,
                EntryState::Bridged,
                Some(LinkKind::Usb),
                level(85, Charge::Charging),
                OutputType::Xbox,
            ),
            entry(
                2,
                EntryState::Held { seconds_left: 30 },
                None,
                None,
                OutputType::Ps4,
            ),
            entry(
                3,
                EntryState::Stopped,
                Some(LinkKind::Bluetooth),
                level(25, Charge::Discharging),
                OutputType::Ps4,
            ),
            entry(
                4,
                EntryState::NotBridged,
                Some(LinkKind::Usb),
                None,
                OutputType::Xbox,
            ),
        ];
        controllers[2].name = text::DUALSHOCK4;
        controllers[2].experimental = true;
        controllers[2].has_player_lights = false;
        controllers[1].nickname = Some("Sofa".into());
        let error = BridgeError::VirtualController("made-up error for the preview".into());
        controllers[3].error = Some(text::bridge_error(&error));
        if missing {
            for entry in &mut controllers {
                entry.state = EntryState::NotBridged;
                entry.active_link = entry.active_link.or(Some(LinkKind::Usb));
                entry.error = None;
            }
        }
        let before_first = first.saturating_sub(1) % controllers.len();
        controllers.rotate_left(before_first);
        if empty {
            controllers.clear();
        }
        let missing = [Driver::ViGEmBus, Driver::HidHide]
            .into_iter()
            .filter(|_| missing)
            .map(|driver| Missing {
                driver,
                error: None,
            })
            .collect();
        Self {
            started: now,
            controllers: RefCell::new(controllers),
            missing: RefCell::new(missing),
            samples: RefCell::new((0, now)),
        }
    }

    fn report_rate(&self, now: Instant) {
        let (count, since) = &mut *self.samples.borrow_mut();
        *count += 1;
        let elapsed = now.duration_since(*since);
        if elapsed >= REPORT_EVERY {
            eprintln!(
                "ui_preview: sampled {:.1} times per second",
                f64::from(*count) / elapsed.as_secs_f64()
            );
            (*count, *since) = (0, now);
        }
    }
}

impl Backend for Fake {
    fn screen(&self, now: Instant) -> Screen {
        self.report_rate(now);
        let t = now.duration_since(self.started).as_secs_f32();
        let mut controllers = self.controllers.borrow().clone();
        for entry in &mut controllers {
            if entry.active_link.is_some() {
                entry.pad = Some(moving_pad(t));
            }
            if let EntryState::Held { .. } = entry.state {
                entry.state = EntryState::Held {
                    seconds_left: 30 - (t as u64 % 30),
                };
            }
        }
        let missing = self.missing.borrow().clone();
        let notices = if controllers.is_empty() || !missing.is_empty() {
            Vec::new()
        } else {
            vec![Notice::warning(text::not_hidden("made-up warning"))]
        };
        Screen {
            starting: false,
            missing,
            notices,
            controllers,
        }
    }

    fn command(&self, command: Command) {
        eprintln!("ui_preview: {command:?}");
        let mut controllers = self.controllers.borrow_mut();
        let mac = match &command {
            Command::Stop(mac)
            | Command::Start(mac)
            | Command::SetOutputType(mac, _)
            | Command::SetNickname(mac, _)
            | Command::ChangePreference(mac, _) => *mac,
            Command::CheckDrivers => {
                self.missing.borrow_mut().clear();
                for entry in controllers.iter_mut() {
                    entry.bus_missing = false;
                    entry.state = EntryState::Bridged;
                }
                return;
            }
        };
        let Some(entry) = controllers.iter_mut().find(|entry| entry.mac == mac) else {
            return;
        };
        match command {
            Command::Stop(_) => {
                entry.state = EntryState::Stopped;
                entry.error = None;
            }
            Command::Start(_) => entry.state = EntryState::Bridged,
            Command::SetOutputType(_, output_type) => entry.output_type = output_type,
            Command::SetNickname(_, name) => {
                let name = name.trim();
                entry.nickname = (!name.is_empty()).then(|| name.to_owned());
            }
            Command::ChangePreference(_, change) => entry.preferences.change(change),
            Command::CheckDrivers => {}
        }
    }
}

fn moving_pad(t: f32) -> PadModel {
    let axis = |value: f32| (128. + 127. * value).clamp(0., 255.) as u8;
    let travel = |value: f32| (127.5 + 127.5 * value.sin()) as u8;
    let step = (t * 2.) as usize % Buttons::NAMED.len();
    let mut buttons = Buttons::NAMED[step].0;
    buttons |= Buttons::CROSS | Buttons::DPAD_LEFT;
    let finger = |x: f32, y: f32| Some(TouchDot::from_units(x as u16, y as u16, 1919, 1079));
    PadModel {
        buttons,
        left_stick: Stick {
            x: axis(t.cos()),
            y: axis(t.sin()),
        },
        right_stick: Stick::CENTER,
        left_trigger: travel(t),
        right_trigger: travel(t + 2.),
        touch: [
            finger(500. + 400. * t.sin(), 400.),
            finger(1500., 600. + 400. * t.cos()),
        ],
    }
}

fn main() {
    let mut empty = false;
    let mut missing = false;
    let mut first = 1;
    let mut seconds = None;
    let mut settings = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--empty" => empty = true,
            "--missing" => missing = true,
            "--first" => {
                first = args.next().and_then(|n| n.parse().ok()).expect("--first N");
            }
            "--settings" => settings = true,
            "--seconds" => seconds = args.next().and_then(|value| value.parse::<u64>().ok()),
            other => panic!("unknown argument {other}"),
        }
    }
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            view::init(cx);
            let open = if settings {
                view::open_window_in_settings
            } else {
                view::open_window
            };
            open(cx, Rc::new(Fake::new(empty, missing, first)))
                .expect("failed to open the preview window");
            if let Some(seconds) = seconds {
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(Duration::from_secs(seconds))
                        .await;
                    cx.update(|cx| cx.quit());
                })
                .detach();
            }
        });
}
