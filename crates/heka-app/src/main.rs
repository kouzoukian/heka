#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridging;
mod settings_file;
mod tray;
mod window_backend;

use std::rc::Rc;
use std::sync::{Arc, mpsc};

use gpui::{App, QuitMode};
use heka_app::log::{self, Log, LogFile};
use heka_app::text;
use heka_app::ui::assets::Assets;
use heka_app::ui::view;
use heka_core::autostart::{self, Autostart};
use heka_windows::autostart::RunKey;
use heka_windows::clock;
use heka_windows::single_instance::{self, Instance};

use bridging::Bridging;
use window_backend::WindowBackend;

fn show_main_window(cx: &mut App, bridging: &Arc<Bridging>) {
    if let Some(window) = cx.windows().first() {
        let _ = window.update(cx, |_, window, _| window.activate_window());
        return;
    }
    let backend = Rc::new(WindowBackend(Arc::clone(bridging)));
    view::open_window(cx, backend).expect("failed to open the main window");
}

fn main() {
    let (instance, unchecked) = match single_instance::claim() {
        Ok(Instance::Second) => return,
        Ok(Instance::First(instance)) => (Some(instance), None),
        Err(error) => (None, Some(error)),
    };
    let in_tray = autostart::opens_in_tray(std::env::args_os());
    start_log(in_tray);
    if let Some(error) = unchecked {
        log::line(&text::log::cannot_check_for_running_copy(
            &error.to_string(),
        ));
    }

    let application = gpui_platform::application()
        .with_assets(Assets)
        .with_quit_mode(QuitMode::Explicit);
    application.run(move |cx: &mut App| {
        view::init(cx);

        let bridging = Arc::new(Bridging::start());
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
            log::line(&text::log::cannot_add_tray_icon(error));
            cx.set_quit_mode(QuitMode::Default);
        }
        if !in_tray || tray.is_err() {
            show_main_window(cx, &bridging);
        }
    });
}

fn start_log(in_tray: bool) {
    let file = log::folder().map(|folder| LogFile::start(&folder, log::FILE_LIMIT));
    let (sink, error) = match file {
        Some(Ok(file)) => (Some(Box::new(file) as Box<dyn log::Sink>), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    };
    log::install(Log::start(sink, clock::local_time));
    log::catch_panics();
    log::line(&text::log::started(env!("CARGO_PKG_VERSION"), in_tray));
    if let Some(error) = error {
        log::line(&text::log::cannot_write(&error.to_string()));
    }
}

fn autostart() -> Autostart<RunKey> {
    let exe = std::env::current_exe().unwrap_or_else(|_| "heka.exe".into());
    let mut autostart = Autostart::new(RunKey, &exe);
    if let Err(error) = autostart.repair() {
        log::line(&text::log::cannot_update_autostart(&error.to_string()));
    }
    autostart
}
