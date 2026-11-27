use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use std::{fs, io, process};

use heka_app::ui::model::{AppEntry, Backend, Command, ControllerEntry, Notice, Screen};
use heka_app::{log, text};
use heka_core::app_settings::{AppSettings, CloseAction};
use heka_core::autostart::Autostart;
use heka_core::setup::Driver;
use heka_core::window_placement::WindowPlacement;
use heka_windows::autostart::RunKey;

use crate::bridging::{Bridging, Status};
use crate::settings_file;

pub struct WindowBackend {
    bridging: Arc<Bridging>,
    autostart: RefCell<Autostart<RunKey>>,
    app: RefCell<AppSettings>,
}

impl WindowBackend {
    pub fn new(bridging: Arc<Bridging>, autostart: Autostart<RunKey>, app: AppSettings) -> Self {
        Self {
            bridging,
            autostart: RefCell::new(autostart),
            app: RefCell::new(app),
        }
    }

    pub fn starts_with_windows(&self) -> bool {
        self.autostart.borrow().is_enabled()
    }

    pub fn on_close(&self) -> CloseAction {
        self.app.borrow().on_close
    }

    fn set_start_with_windows(&self, enabled: bool) {
        if let Err(error) = self.autostart.borrow_mut().set_enabled(enabled) {
            log::line(&text::log::cannot_change_autostart(&error.to_string()));
        }
    }

    fn change_app(&self, change: impl FnOnce(&mut AppSettings)) {
        let mut app = self.app.borrow_mut();
        let mut changed = app.clone();
        change(&mut changed);
        if changed != *app {
            app.clone_from(&changed);
            self.bridging.set_app_settings(changed);
        }
    }

    fn app_entry(&self) -> AppEntry {
        let app = self.app.borrow();
        AppEntry {
            start_with_windows: self.starts_with_windows(),
            start_in_tray: app.start_in_tray,
            on_close: app.on_close,
        }
    }
}

impl Backend for WindowBackend {
    fn screen(&self, now: Instant) -> Screen {
        let snapshot = self.bridging.snapshot();
        let status = match &snapshot.status {
            Status::HidError(error) => Some(Notice::error(text::hid_error(error))),
            Status::Starting | Status::Running => None,
        };
        let unopened = snapshot
            .unopened
            .iter()
            .map(|unopened| Notice::warning(text::cannot_open(unopened)));
        let hiding = snapshot
            .hiding
            .iter()
            .map(|warning| Notice::warning(text::hiding_warning(warning)));
        let not_unplugged = snapshot
            .not_unplugged
            .iter()
            .map(|error| Notice::warning(text::not_unplugged(error)));
        let not_turned_off = snapshot
            .controllers
            .iter()
            .filter_map(Notice::cannot_turn_off);
        let missing = snapshot
            .setup
            .map(|setup| setup.missing())
            .unwrap_or_default();
        let bus_missing = missing
            .iter()
            .any(|missing| missing.driver == Driver::ViGEmBus);
        Screen {
            starting: snapshot.status == Status::Starting,
            missing,
            notices: status
                .into_iter()
                .chain(unopened)
                .chain(hiding)
                .chain(not_unplugged)
                .chain(not_turned_off)
                .collect(),
            controllers: snapshot
                .controllers
                .iter()
                .map(|view| ControllerEntry::from_view(view, now, bus_missing))
                .collect(),
            log: log::recent(),
            app: self.app_entry(),
        }
    }

    fn command(&self, command: Command) {
        let bridging = &self.bridging;
        match command {
            Command::Stop(mac) => bridging.stop_controller(mac),
            Command::Start(mac) => bridging.start_controller(mac),
            Command::SetOutputType(mac, output_type) => bridging.set_output_type(mac, output_type),
            Command::SetNickname(mac, name) => bridging.set_nickname(mac, name),
            Command::ChangePreference(mac, change) => bridging.change_preference(mac, change),
            Command::TestRumble(mac, motor) => bridging.test_rumble(mac, motor),
            Command::EndRumbleTest(mac) => bridging.end_rumble_test(mac),
            Command::TurnOff(mac) => bridging.turn_off(mac),
            Command::SetIdleTime(mac, idle_time) => bridging.set_idle_time(mac, idle_time),
            Command::CheckDrivers => bridging.check_drivers(),
            Command::OpenLogFolder => open_log_folder(),
            Command::SetStartWithWindows(enabled) => self.set_start_with_windows(enabled),
            Command::ChangeAppSetting(change) => self.change_app(|app| app.change(change)),
            Command::RememberWindow(placement) => {
                self.change_app(|app| app.window = Some(placement));
            }
            Command::OpenSettingsFolder => open_settings_folder(),
        }
    }

    fn window_placement(&self) -> Option<WindowPlacement> {
        self.app.borrow().window.clone()
    }
}

fn open_log_folder() {
    let Some(folder) = log::folder() else {
        return;
    };
    if let Err(error) = explore(&folder) {
        log::line(&text::log::cannot_open_log_folder(&error.to_string()));
    }
}

fn open_settings_folder() {
    let Some(folder) = settings_file::folder() else {
        return;
    };
    if let Err(error) = fs::create_dir_all(&folder).and_then(|()| explore(&folder)) {
        log::line(&text::log::cannot_open_settings_folder(&error.to_string()));
    }
}

fn explore(folder: &Path) -> io::Result<()> {
    let windows = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let explorer = PathBuf::from(windows).join("explorer.exe");
    process::Command::new(explorer).arg(folder).spawn()?;
    Ok(())
}
