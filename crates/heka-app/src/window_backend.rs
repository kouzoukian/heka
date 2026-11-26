//! Connects the window to the supervisor thread.

use std::time::Instant;

use heka_app::text;
use heka_app::ui::model::{Backend, CardModel, Command, Notice, Screen};

use crate::bridging::{Bridging, Status};

impl Backend for Bridging {
    /// A copy of what the supervisor thread last published and of each Bridge's
    /// latest input. It never waits for either thread's work.
    fn screen(&self, now: Instant) -> Screen {
        let snapshot = self.snapshot();
        let status = match &snapshot.status {
            Status::NoBus(error) => Some(Notice::error(text::no_bus(error))),
            Status::HidError(error) => Some(Notice::error(text::hid_error(error))),
            Status::Starting | Status::Running => None,
        };
        let hiding = snapshot.hiding_note.map(Notice::warning);
        Screen {
            starting: snapshot.status == Status::Starting,
            notices: status.into_iter().chain(hiding).collect(),
            cards: snapshot
                .controllers
                .iter()
                .map(|view| CardModel::from_view(view, now))
                .collect(),
        }
    }

    fn command(&self, command: Command) {
        match command {
            Command::Stop(mac) => self.stop_controller(mac),
            Command::Start(mac) => self.start_controller(mac),
            Command::SetOutputType(mac, output_type) => self.set_output_type(mac, output_type),
        }
    }
}
