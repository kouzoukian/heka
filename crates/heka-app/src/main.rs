#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridging;
mod profiles_file;
mod releases;
mod settings_file;
mod tray;
mod window_backend;

use std::rc::Rc;
use std::sync::{Arc, mpsc};

use gpui::{App, QuitMode};
use heka_app::log::{self, Log, LogFile};
use heka_app::text;
use heka_app::ui::assets::Assets;
use heka_app::ui::view::{self, Page};
use heka_core::app_settings::CloseAction;
use heka_core::autostart::Autostart;
use heka_windows::autostart::RunKey;
use heka_windows::clock;
use heka_windows::single_instance::{self, Instance};

use bridging::Bridging;
use releases::Releases;
use tray::Request;
use window_backend::WindowBackend;

fn show_main_window(cx: &mut App, backend: &Rc<WindowBackend>, page: Option<Page>) {
    if let Some(&window) = cx.windows().first() {
        let _ = window.update(cx, |_, window, _| window.activate_window());
        if let Some(page) = page {
            view::show_page(window, page, cx);
        }
        return;
    }
    let backend = Rc::clone(backend);
    view::open_window(cx, backend, page.unwrap_or_default())
        .expect("failed to open the main window");
}

fn main() {
    let (instance, unchecked) = match single_instance::claim() {
        Ok(Instance::Second) => return,
        Ok(Instance::First(instance)) => (Some(instance), None),
        Err(error) => (None, Some(error)),
    };
    let log_error = start_log();
    let (settings, unreadable) = settings_file::load();
    let (profiles, profile_problems) = profiles_file::load();
    let app = settings.app().clone();
    let in_tray = app.opens_in_tray(std::env::args_os());
    log::line(&text::log::started(env!("CARGO_PKG_VERSION"), in_tray));
    for message in log_error
        .into_iter()
        .chain(unreadable)
        .chain(profile_problems)
    {
        log::line(&message);
    }
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

        let bridging = Arc::new(Bridging::start(settings, profiles));
        cx.on_app_quit({
            let bridging = Arc::clone(&bridging);
            move |_| {
                bridging.quit();
                async {}
            }
        })
        .detach();

        let releases = Releases::start(app.check_for_releases);
        let backend = Rc::new(WindowBackend::new(bridging, releases, autostart(), app));
        let (requests_sender, requests) = mpsc::channel();
        if let Some(instance) = instance {
            let requests = requests_sender.clone();
            instance.on_second_launch(move || {
                let _ = requests.send(Request::Show);
            });
        }
        cx.on_window_closed({
            let backend = Rc::clone(&backend);
            move |_, _| {
                if backend.on_close() == CloseAction::Quit {
                    let _ = requests_sender.send(Request::Quit);
                }
            }
        })
        .detach();
        let tray = tray::install(cx, Rc::clone(&backend), requests, {
            let backend = Rc::clone(&backend);
            move |cx, page| show_main_window(cx, &backend, page)
        });
        if let Err(error) = &tray {
            log::line(&text::log::cannot_add_tray_icon(error));
            cx.set_quit_mode(QuitMode::Default);
        }
        if !in_tray || tray.is_err() {
            show_main_window(cx, &backend, None);
        }
    });
}

fn start_log() -> Option<String> {
    let file = log::folder().map(|folder| LogFile::start(&folder, log::FILE_LIMIT));
    let (sink, error) = match file {
        Some(Ok(file)) => (Some(Box::new(file) as Box<dyn log::Sink>), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    };
    log::install(Log::start(sink, clock::local_time));
    log::catch_panics();
    error.map(|error| text::log::cannot_write(&error.to_string()))
}

fn autostart() -> Autostart<RunKey> {
    let exe = std::env::current_exe().unwrap_or_else(|_| "heka.exe".into());
    let mut autostart = Autostart::new(RunKey, &exe);
    if let Err(error) = autostart.repair() {
        log::line(&text::log::cannot_update_autostart(&error.to_string()));
    }
    autostart
}
