mod bridging;

use std::sync::Arc;
use std::time::Duration;

use gpui::{
    App, AppContext, Context, IntoElement, ParentElement, Render, SharedString, Styled,
    TitlebarOptions, Window, WindowOptions, div, px,
};
use gpui_component::{ActiveTheme, Root};
use heka_core::state::{Buttons, ControllerState, Stick};

use bridging::{Bridging, Status};

/// The UI samples the latest input at about 60 Hz. It is never in the input path.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(16);

struct MainView {
    bridging: Arc<Bridging>,
}

impl MainView {
    fn new(bridging: Arc<Bridging>, cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SAMPLE_INTERVAL).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        Self { bridging }
    }
}

fn status_text(status: &Status) -> SharedString {
    match status {
        Status::Starting => "Starting".into(),
        Status::NoBus(error) => format!("ViGEmBus is not available: {error}").into(),
        Status::HidError(error) => format!("Cannot read the Controller: {error}").into(),
        Status::NoController => "Connect a DualSense with a USB cable".into(),
        Status::Bridged => "DualSense on USB, Bridged as Xbox".into(),
    }
}

fn stick_text(name: &str, stick: Stick) -> SharedString {
    format!("{name}  x {:>3}  y {:>3}", stick.x, stick.y).into()
}

impl Render for MainView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let status = self.bridging.status();
        let state = self.bridging.live().latest();

        let mut view = div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .child(div().child(status_text(&status)));

        let Some(state) = state else {
            return view.child(
                div()
                    .text_color(theme.muted_foreground)
                    .child("No live input"),
            );
        };
        let ControllerState {
            buttons,
            left_stick,
            right_stick,
            left_trigger,
            right_trigger,
        } = state;

        let chips = Buttons::NAMED.iter().map(|&(button, name)| {
            let pressed = buttons.contains(button);
            div()
                .px_2()
                .py_1()
                .rounded(px(4.))
                .border_1()
                .border_color(theme.border)
                .when_pressed(pressed, theme.primary, theme.primary_foreground)
                .child(name)
        });
        view = view
            .child(div().flex().flex_wrap().gap_1().children(chips))
            .child(div().child(stick_text("Left stick ", left_stick)))
            .child(div().child(stick_text("Right stick", right_stick)))
            .child(div().child(SharedString::from(format!(
                "L2 {left_trigger:>3}   R2 {right_trigger:>3}"
            ))));
        view
    }
}

/// Fills a button chip when pressed.
trait PressedStyle: Styled + Sized {
    fn when_pressed(self, pressed: bool, fill: gpui::Hsla, text: gpui::Hsla) -> Self {
        if pressed {
            self.bg(fill).text_color(text)
        } else {
            self
        }
    }
}

impl<T: Styled> PressedStyle for T {}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        // Must run before any gpui-component element is created.
        gpui_component::init(cx);

        let bridging = Arc::new(Bridging::start());
        // Quitting removes the Virtual Controller before the process ends.
        cx.on_app_quit({
            let bridging = Arc::clone(&bridging);
            move |_| {
                bridging.stop();
                async {}
            }
        })
        .detach();

        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("heka".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            let view = cx.new(|cx| MainView::new(bridging, cx));
            // gpui-component expects a `Root` as the window's first view.
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("failed to open the main window");
    });
}
