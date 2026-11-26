//! heka's window over made-up data, to look at the interface without Controllers.
//!
//! It touches no HID device, ViGEmBus or HidHide.
//!
//! `cargo run -p heka-app --example ui_preview [-- --empty] [--seconds N]`
//!
//! `--empty` shows the window with no Controller, and `--seconds N` closes it by
//! itself. It prints how often the window samples its backend.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use heka_app::text;
use heka_app::ui::assets::Assets;
use heka_app::ui::model::{
    ActiveLink, Backend, Battery, CardModel, CardState, Command, Notice, Screen,
};
use heka_app::ui::pad::{PadModel, TouchDot};
use heka_app::ui::view;
use heka_core::mac::MacAddress;
use heka_core::state::{Buttons, Stick};
use heka_core::virtual_controller::OutputType;

/// How often the sampling rate is printed.
const REPORT_EVERY: Duration = Duration::from_secs(2);

struct Fake {
    started: Instant,
    cards: RefCell<Vec<CardModel>>,
    /// Samples since the last report, and when that was.
    samples: RefCell<(u32, Instant)>,
}

impl Fake {
    fn new(empty: bool) -> Self {
        let now = Instant::now();
        let card = |last: u8, state, active_link, battery, output_type| CardModel {
            mac: MacAddress([0x02, 0, 0, 0, 0, last]),
            name: text::DUALSENSE,
            active_link,
            battery,
            output_type,
            state,
            error: None,
            pad: None,
        };
        let level = |percent, charging| Some(Battery { percent, charging });
        let mut cards = vec![
            card(
                1,
                CardState::Bridged,
                Some(ActiveLink::Usb),
                level(80, true),
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
                Some(ActiveLink::Bluetooth),
                level(25, false),
                OutputType::Ps4,
            ),
            card(
                4,
                CardState::NotBridged,
                Some(ActiveLink::Usb),
                None,
                OutputType::Xbox,
            ),
        ];
        cards[3].error = Some("made-up error for the preview".into());
        if empty {
            cards.clear();
        }
        Self {
            started: now,
            cards: RefCell::new(cards),
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
        let notices = if cards.is_empty() {
            Vec::new()
        } else {
            vec![Notice::warning(text::not_hidden("made-up warning"))]
        };
        Screen {
            starting: false,
            notices,
            cards,
        }
    }

    fn command(&self, command: Command) {
        eprintln!("ui_preview: {command:?}");
        let mut cards = self.cards.borrow_mut();
        let (Command::Stop(mac) | Command::Start(mac) | Command::SetOutputType(mac, _)) = command;
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
    let mut seconds = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--empty" => empty = true,
            "--seconds" => seconds = args.next().and_then(|value| value.parse::<u64>().ok()),
            other => panic!("unknown argument {other}"),
        }
    }
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            view::init(cx);
            view::open_window(cx, Rc::new(Fake::new(empty)))
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
