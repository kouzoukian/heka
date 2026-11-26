//! heka's window over made-up data, to look at the interface without Controllers.
//!
//! It touches no HID device, ViGEmBus or HidHide.
//!
//! `cargo run -p heka-app --example ui_preview [-- --empty] [--missing] [--seconds N]`
//!
//! `--empty` shows the window with no Controller, `--missing` adds the setup
//! screen for both drivers, and `--seconds N` closes the window by itself. It
//! prints how often the window samples its backend.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use heka_app::text;
use heka_app::ui::assets::Assets;
use heka_app::ui::model::{Backend, CardModel, CardState, Command, Notice, Screen};
use heka_app::ui::pad::{PadModel, TouchDot};
use heka_app::ui::view;
use heka_core::link::LinkKind;
use heka_core::mac::MacAddress;
use heka_core::setup::{Driver, Missing};
use heka_core::state::{Battery, Buttons, Charge, Stick};
use heka_core::virtual_controller::OutputType;

/// How often the sampling rate is printed.
const REPORT_EVERY: Duration = Duration::from_secs(2);

struct Fake {
    started: Instant,
    cards: RefCell<Vec<CardModel>>,
    missing: RefCell<Vec<Missing>>,
    /// Samples since the last report, and when that was.
    samples: RefCell<(u32, Instant)>,
}

impl Fake {
    fn new(empty: bool, missing: bool) -> Self {
        let now = Instant::now();
        let card = |last: u8, state, active_link, battery, output_type| CardModel {
            mac: MacAddress([0x02, 0, 0, 0, 0, last]),
            name: text::DUALSENSE,
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
        let mut cards = vec![
            card(
                1,
                CardState::Bridged,
                Some(LinkKind::Usb),
                level(85, Charge::Charging),
                OutputType::Xbox,
            ),
            card(
                2,
                CardState::Held { seconds_left: 30 },
                None,
                None,
                OutputType::Ps4,
            ),
            card(
                3,
                CardState::Stopped,
                Some(LinkKind::Bluetooth),
                level(25, Charge::Discharging),
                OutputType::Ps4,
            ),
            card(
                4,
                CardState::NotBridged,
                Some(LinkKind::Usb),
                None,
                OutputType::Xbox,
            ),
        ];
        cards[2].name = text::DUALSHOCK4;
        cards[2].experimental = true;
        cards[3].error = Some("made-up error for the preview".into());
        if missing {
            // Without ViGEmBus nothing is Bridged, but input still shows.
            for card in &mut cards {
                card.state = CardState::NotBridged;
                card.active_link = card.active_link.or(Some(LinkKind::Usb));
                card.error = None;
            }
        }
        if empty {
            cards.clear();
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
            cards: RefCell::new(cards),
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
        let mut cards = self.cards.borrow().clone();
        for card in &mut cards {
            if card.bus_missing && card.state != CardState::Stopped {
                card.pad = Some(moving_pad(t));
            }
            match card.state {
                CardState::Bridged => card.pad = Some(moving_pad(t)),
                CardState::Held { .. } => {
                    card.state = CardState::Held {
                        seconds_left: 30 - (t as u64 % 30),
                    };
                }
                CardState::Stopped | CardState::NotBridged => {}
            }
        }
        let missing = self.missing.borrow().clone();
        let notices = if cards.is_empty() || !missing.is_empty() {
            Vec::new()
        } else {
            vec![Notice::warning(text::not_hidden("made-up warning"))]
        };
        Screen {
            starting: false,
            missing,
            notices,
            cards,
        }
    }

    fn command(&self, command: Command) {
        eprintln!("ui_preview: {command:?}");
        let mut cards = self.cards.borrow_mut();
        let mac = match command {
            Command::Stop(mac) | Command::Start(mac) | Command::SetOutputType(mac, _) => mac,
            Command::CheckDrivers => {
                // As if the player had installed both.
                self.missing.borrow_mut().clear();
                for card in cards.iter_mut() {
                    card.bus_missing = false;
                    card.state = CardState::Bridged;
                }
                return;
            }
        };
        let Some(card) = cards.iter_mut().find(|card| card.mac == mac) else {
            return;
        };
        match command {
            Command::Stop(_) => {
                card.state = CardState::Stopped;
                card.pad = None;
                card.error = None;
            }
            Command::Start(_) => card.state = CardState::Bridged,
            Command::SetOutputType(_, output_type) => card.output_type = output_type,
            Command::CheckDrivers => {}
        }
    }
}

/// Sticks circling, triggers pulling, one button after another and two fingers.
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
    let mut seconds = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--empty" => empty = true,
            "--missing" => missing = true,
            "--seconds" => seconds = args.next().and_then(|value| value.parse::<u64>().ok()),
            other => panic!("unknown argument {other}"),
        }
    }
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            view::init(cx);
            view::open_window(cx, Rc::new(Fake::new(empty, missing)))
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
