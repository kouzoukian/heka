use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use heka_app::log::{Entry, Recent};
use heka_app::text;
use heka_app::ui::assets::Assets;
use heka_app::ui::model::{
    AppEntry, Backend, Command, ControllerEntry, EntryState, Notice, Screen,
};
use heka_app::ui::pad::{PadModel, TouchDot};
use heka_app::ui::view::{self, Page};
use heka_core::app_settings::AppSettings;
use heka_core::clock::Timestamp;
use heka_core::hiding::{DeviceInstancePath, HidingChange};
use heka_core::lifecycle::Change;
use heka_core::link::{ControllerModel, LinkKind};
use heka_core::mac::MacAddress;
use heka_core::preferences::Preferences;
use heka_core::setup::{Driver, Missing};
use heka_core::state::{Battery, Buttons, Charge, Stick};
use heka_core::supervisor::BridgeError;
use heka_core::virtual_controller::OutputType;
use heka_core::window_placement::{Rect, WindowPlacement};

const REPORT_EVERY: Duration = Duration::from_secs(2);

struct Fake {
    started: Instant,
    controllers: RefCell<Vec<ControllerEntry>>,
    missing: RefCell<Vec<Missing>>,
    samples: RefCell<(u32, Instant)>,
    log: Recent,
    app: RefCell<AppSettings>,
    start_with_windows: Cell<bool>,
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
            has_bluetooth: active_link == Some(LinkKind::Bluetooth),
            idle_time: None,
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
            log: made_up_log(),
            app: RefCell::default(),
            start_with_windows: Cell::default(),
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
            log: Recent::clone(&self.log),
            app: AppEntry {
                start_with_windows: self.start_with_windows.get(),
                start_in_tray: self.app.borrow().start_in_tray,
                on_close: self.app.borrow().on_close,
            },
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
            | Command::ChangePreference(mac, _)
            | Command::TurnOff(mac)
            | Command::SetIdleTime(mac, _) => *mac,
            Command::SetStartWithWindows(on) => {
                self.start_with_windows.set(*on);
                return;
            }
            Command::ChangeAppSetting(change) => {
                self.app.borrow_mut().change(*change);
                return;
            }
            Command::RememberWindow(placement) => {
                self.app.borrow_mut().window = Some(placement.clone());
                return;
            }
            Command::TestRumble(..)
            | Command::EndRumbleTest(_)
            | Command::OpenLogFolder
            | Command::OpenSettingsFolder => return,
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
            Command::TurnOff(mac) => controllers.retain(|entry| entry.mac != mac),
            Command::SetIdleTime(_, idle_time) => entry.idle_time = idle_time,
            _ => {}
        }
    }

    fn window_placement(&self) -> Option<WindowPlacement> {
        self.app.borrow().window.clone()
    }
}

fn placement(words: &str) -> Option<WindowPlacement> {
    let mut numbers = words.split(',').map(str::parse::<i32>);
    let mut next = || numbers.next()?.ok();
    Some(WindowPlacement {
        bounds: Rect {
            x: next()?,
            y: next()?,
            width: next()?.try_into().ok()?,
            height: next()?.try_into().ok()?,
        },
        maximized: false,
        monitor: None,
    })
}

fn made_up_log() -> Recent {
    let mac = MacAddress([0x02, 0, 0, 0, 0, 1]);
    let messages = [
        text::log::started("0.1.1", false),
        "ViGEmBus found".into(),
        "HidHide found".into(),
        text::log::link_found(mac, ControllerModel::DualSense, LinkKind::Usb),
        text::log::change(&Change::Bridged {
            mac,
            output_type: OutputType::Xbox,
        }),
        text::log::hiding_change(&HidingChange::Hidden(DeviceInstancePath::new(
            r"HID\VID_054C&PID_0CE6&MI_03\8&15C30336&0&0000",
        ))),
        text::log::change(&Change::Held { mac }),
    ];
    Recent::new(
        messages
            .into_iter()
            .zip(0..)
            .map(|(message, second)| {
                let at = Timestamp {
                    year: 2026,
                    month: 1,
                    day: 1,
                    hour: 21,
                    minute: 3,
                    second,
                    millisecond: 0,
                };
                Entry { at, message }.into()
            })
            .collect(),
    )
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
    let mut window = None;
    let mut page = Page::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--empty" => empty = true,
            "--missing" => missing = true,
            "--first" => {
                first = args.next().and_then(|n| n.parse().ok()).expect("--first N");
            }
            "--settings" => page = Page::ControllerSettings,
            "--app-settings" => page = Page::AppSettings,
            "--window" => {
                let words = args.next();
                window = words.as_deref().and_then(placement);
                assert!(window.is_some(), "--window X,Y,WIDTH,HEIGHT");
            }
            "--log" => page = Page::Log,
            "--about" => page = Page::About,
            "--seconds" => seconds = args.next().and_then(|value| value.parse::<u64>().ok()),
            other => panic!("unknown argument {other}"),
        }
    }
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            view::init(cx);
            let fake = Fake::new(empty, missing, first);
            fake.app.borrow_mut().window = window;
            view::open_window(cx, Rc::new(fake), page).expect("failed to open the preview window");
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
