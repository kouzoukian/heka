use std::sync::Arc;
use std::time::Instant;

use heka_app::text;
use heka_app::ui::model::{Backend, Command, ControllerEntry, Notice, Screen};
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
        let hiding = snapshot.hiding_note.map(Notice::warning);
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
            notices: status.into_iter().chain(hiding).collect(),
            controllers: snapshot
                .controllers
                .iter()
                .map(|view| ControllerEntry::from_view(view, now, bus_missing))
                .collect(),
        }
    }

    fn command(&self, command: Command) {
        let bridging = &self.0;
        match command {
            Command::Stop(mac) => bridging.stop_controller(mac),
            Command::Start(mac) => bridging.start_controller(mac),
            Command::SetOutputType(mac, output_type) => bridging.set_output_type(mac, output_type),
            Command::SetNickname(mac, name) => bridging.set_nickname(mac, name),
            Command::SetPreferences(mac, preferences) => bridging.set_preferences(mac, preferences),
            Command::CheckDrivers => bridging.check_drivers(),
        }
    }
}
