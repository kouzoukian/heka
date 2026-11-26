#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridging;
mod settings_file;
mod tray;
mod window_backend;

use std::rc::Rc;
use std::sync::{Arc, mpsc};

use gpui::{App, QuitMode};
use heka_app::ui::assets::Assets;
use heka_app::ui::view;
use heka_core::autostart::{self, Autostart};
use heka_windows::autostart::RunKey;
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
    let instance = match single_instance::claim() {
        Ok(Instance::Second) => return,
        Ok(Instance::First(instance)) => Some(instance),
        Err(error) => {
            eprintln!("heka: cannot check for a running copy: {error}");
            None
        }
    };
    let in_tray = autostart::opens_in_tray(std::env::args_os());

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
            eprintln!("heka: cannot add the tray icon: {error}");
            cx.set_quit_mode(QuitMode::Default);
        }
        if !in_tray || tray.is_err() {
            show_main_window(cx, &bridging);
        }
    });
}

fn autostart() -> Autostart<RunKey> {
    let exe = std::env::current_exe().unwrap_or_else(|_| "heka.exe".into());
    let mut autostart = Autostart::new(RunKey, &exe);
    if let Err(error) = autostart.repair() {
        eprintln!("heka: cannot update Start with Windows: {error}");
    }
    autostart
}
