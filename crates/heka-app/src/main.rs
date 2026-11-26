mod bridging;
mod settings_file;
mod setup_screen;
mod tray;

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, Context, IntoElement, ParentElement, QuitMode, Render, SharedString, Styled,
    TitlebarOptions, Window, WindowOptions, div, px,
};
use gpui_component::button::Button;
use gpui_component::{ActiveTheme, Root};
use heka_core::autostart::{self, Autostart};
use heka_core::lifecycle::Phase;
use heka_core::link::LinkKind;
use heka_core::setup::Driver;
use heka_core::state::{Buttons, ControllerState, Stick};
use heka_core::supervisor::ControllerView;
use heka_core::virtual_controller::OutputType;
use heka_windows::autostart::RunKey;
use heka_windows::single_instance::{self, Instance};

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

fn status_text(status: &Status, controllers: usize) -> SharedString {
    match status {
        Status::Starting => "Starting".into(),
        Status::HidError(error) => format!("Cannot look for Controllers: {error}").into(),
        Status::Running if controllers == 0 => "Connect a DualSense with a USB cable".into(),
        Status::Running => "".into(),
    }
}

fn phase_text(phase: Phase, output_type: OutputType) -> SharedString {
    match phase {
        Phase::Bridged => match output_type {
            OutputType::Xbox => "Bridged as Xbox".into(),
            OutputType::Ps4 => "Bridged as PS4".into(),
        },
        Phase::Held { until } => {
            let left = until.saturating_duration_since(Instant::now()).as_secs();
            format!("Held, reconnect within {left} s").into()
        }
        Phase::Stopped => "Stopped".into(),
        Phase::Waiting => "Not Bridged: 4 Controllers are already Bridged".into(),
    }
}

fn input_text(state: ControllerState) -> SharedString {
    let ControllerState {
        buttons,
        left_stick,
        right_stick,
        left_trigger,
        right_trigger,
        battery,
        ..
    } = state;
    let pressed: Vec<&str> = Buttons::NAMED
        .iter()
        .filter(|(button, _)| buttons.contains(*button))
        .map(|(_, name)| *name)
        .collect();
    let stick = |stick: Stick| format!("{:>3},{:>3}", stick.x, stick.y);
    let battery = battery.map_or_else(String::new, |b| format!("  Battery {}%", b.percent));
    format!(
        "L {}  R {}  L2 {left_trigger:>3}  R2 {right_trigger:>3}{battery}  {}",
        stick(left_stick),
        stick(right_stick),
        pressed.join(" "),
    )
    .into()
}

impl MainView {
    fn controller_row(
        &self,
        index: usize,
        view: &ControllerView,
        no_bus: bool,
        cx: &App,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let mac = view.mac;
        let bridging = Arc::clone(&self.bridging);
        let button = if view.phase == Phase::Stopped {
            Button::new(("start", index))
                .label("Start")
                .on_click(move |_, _, _| bridging.start_controller(mac))
        } else {
            Button::new(("stop", index))
                .label("Stop")
                .on_click(move |_, _, _| bridging.stop_controller(mac))
        };
        // A minimal Output Type switch; ticket #10 builds the real control.
        let (label, other) = match view.output_type {
            OutputType::Xbox => ("Output: Xbox", OutputType::Ps4),
            OutputType::Ps4 => ("Output: PS4", OutputType::Xbox),
        };
        let output_button = {
            let bridging = Arc::clone(&self.bridging);
            Button::new(("output-type", index))
                .label(label)
                .on_click(move |_, _, _| bridging.set_output_type(mac, other))
        };
        let mut phase = match view.phase {
            Phase::Waiting if no_bus => setup_screen::text::NOT_BRIDGED.into(),
            phase => phase_text(phase, view.output_type),
        };
        match view.active_link {
            Some(LinkKind::Usb) => phase = format!("{phase}, USB").into(),
            Some(LinkKind::Bluetooth) => phase = format!("{phase}, Bluetooth").into(),
            None => {}
        }
        if let Some(error) = &view.error {
            phase = format!("{phase} ({error})").into();
        }
        let input = view.live.latest().map(input_text).unwrap_or_default();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .rounded(px(6.))
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().child(SharedString::from(format!("DualSense {mac}"))))
                    .child(div().flex().gap_2().child(output_button).child(button)),
            )
            .child(div().text_color(theme.muted_foreground).child(phase))
            .child(div().font_family("monospace").child(input))
    }
}

impl Render for MainView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.bridging.snapshot();
        let no_bus = snapshot
            .setup
            .as_ref()
            .is_some_and(|setup| setup.is_missing(Driver::ViGEmBus));
        let rows: Vec<_> = snapshot
            .controllers
            .iter()
            .enumerate()
            .map(|(index, view)| self.controller_row(index, view, no_bus, cx))
            .collect();
        let setup_screen = snapshot.setup.as_ref().and_then(|setup| {
            let bridging = Arc::clone(&self.bridging);
            setup_screen::setup_screen(setup, move || bridging.check_drivers(), cx)
        });
        let theme = cx.theme();
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .children(setup_screen)
            .child(div().child(status_text(&snapshot.status, rows.len())))
            .children(
                snapshot
                    .hiding_note
                    .map(|note| div().text_color(theme.warning).child(note)),
            )
            .children(rows)
    }
}

/// Focuses the window, or opens it again after it was closed to the tray.
fn show_main_window(cx: &mut App, bridging: &Arc<Bridging>) {
    if let Some(window) = cx.windows().first() {
        let _ = window.update(cx, |_, window, _| window.activate_window());
        return;
    }
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("heka".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let bridging = Arc::clone(bridging);
    cx.open_window(options, |window, cx| {
        let view = cx.new(|cx| MainView::new(bridging, cx));
        // gpui-component expects a `Root` as the window's first view.
        cx.new(|cx| Root::new(view, window, cx))
    })
    .expect("failed to open the main window");
}

fn main() {
    // A second copy only asks the running one to show its window.
    let instance = match single_instance::claim() {
        Ok(Instance::Second) => return,
        Ok(Instance::First(instance)) => Some(instance),
        Err(error) => {
            eprintln!("heka: cannot check for a running copy: {error}");
            None
        }
    };
    let in_tray = autostart::opens_in_tray(std::env::args_os());

    // Closing the window leaves heka running in the tray; only Quit ends it.
    let application = gpui_platform::application().with_quit_mode(QuitMode::Explicit);
    application.run(move |cx: &mut App| {
        // Must run before any gpui-component element is created.
        gpui_component::init(cx);

        let bridging = Arc::new(Bridging::start());
        // Quitting removes every Virtual Controller and restores every Hidden
        // Controller before the process ends.
        cx.on_app_quit({
            let bridging = Arc::clone(&bridging);
            move |_| {
                bridging.quit();
                async {}
            }
        })
        .detach();

        let (show_requests_sender, show_requests) = mpsc::channel();
        if let Some(instance) = instance {
            instance.on_second_launch(move || {
                let _ = show_requests_sender.send(());
            });
        }
        let tray = tray::install(cx, autostart(), show_requests, {
            let bridging = Arc::clone(&bridging);
            move |cx| show_main_window(cx, &bridging)
        });
        if let Err(error) = &tray {
            // Without a tray icon, closing the window must quit as before.
            eprintln!("heka: cannot add the tray icon: {error}");
            cx.set_quit_mode(QuitMode::Default);
        }
        if !in_tray || tray.is_err() {
            show_main_window(cx, &bridging);
        }
    });
}

/// Start with Windows for this copy, pointed at wherever this copy now lives.
fn autostart() -> Autostart<RunKey> {
    let exe = std::env::current_exe().unwrap_or_else(|_| "heka.exe".into());
    let mut autostart = Autostart::new(RunKey, &exe);
    if let Err(error) = autostart.repair() {
        eprintln!("heka: cannot update Start with Windows: {error}");
    }
    autostart
}
