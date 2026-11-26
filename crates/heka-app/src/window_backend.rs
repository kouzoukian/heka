//! Connects the window to the supervisor thread.

use std::sync::Arc;
use std::time::Instant;

use heka_app::text;
use heka_app::ui::model::{Backend, CardModel, Command, Notice, Screen};
use heka_core::setup::Driver;

use crate::bridging::{Bridging, Status};

pub struct WindowBackend(pub Arc<Bridging>);

impl Backend for WindowBackend {
    /// A copy of what the supervisor thread last published and of each Bridge's
    /// latest input. It never waits for either thread's work.
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
            cards: snapshot
                .controllers
                .iter()
                .map(|view| CardModel::from_view(view, now, bus_missing))
                .collect(),
        }
    }

    fn command(&self, command: Command) {
        let bridging = &self.0;
        match command {
            Command::Stop(mac) => bridging.stop_controller(mac),
            Command::Start(mac) => bridging.start_controller(mac),
            Command::SetOutputType(mac, output_type) => bridging.set_output_type(mac, output_type),
            Command::CheckDrivers => bridging.check_drivers(),
        }
    }
}
