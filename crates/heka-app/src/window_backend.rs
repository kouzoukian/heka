use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::time::Instant;

use heka_app::ui::model::{Backend, Command, ControllerEntry, Notice, Screen};
use heka_app::{log, text};
use heka_core::setup::Driver;

use crate::bridging::{Bridging, Status};

pub struct WindowBackend(pub Arc<Bridging>);

impl Backend for WindowBackend {
    fn screen(&self, now: Instant) -> Screen {
        let snapshot = self.0.snapshot();
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
                .collect(),
            controllers: snapshot
                .controllers
                .iter()
                .map(|view| ControllerEntry::from_view(view, now, bus_missing))
                .collect(),
            log: log::recent(),
        }
    }

    fn command(&self, command: Command) {
        let bridging = &self.0;
        match command {
            Command::Stop(mac) => bridging.stop_controller(mac),
            Command::Start(mac) => bridging.start_controller(mac),
            Command::SetOutputType(mac, output_type) => bridging.set_output_type(mac, output_type),
            Command::SetNickname(mac, name) => bridging.set_nickname(mac, name),
            Command::ChangePreference(mac, change) => bridging.change_preference(mac, change),
            Command::CheckDrivers => bridging.check_drivers(),
            Command::OpenLogFolder => open_log_folder(),
        }
    }
}

fn open_log_folder() {
    let Some(folder) = log::folder() else {
        return;
    };
    let windows = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let explorer = PathBuf::from(windows).join("explorer.exe");
    if let Err(error) = process::Command::new(explorer).arg(folder).spawn() {
        log::line(&text::log::cannot_open_log_folder(&error.to_string()));
    }
}
