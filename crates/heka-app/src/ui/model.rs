//! What the window shows, as plain data, and what the player can ask for.
//!
//! The window draws a [`Screen`] and sends [`Command`]s to a [`Backend`]. It
//! redraws only when two samples of the screen differ.

use std::time::{Duration, Instant};

use heka_core::lifecycle::Phase;
use heka_core::mac::MacAddress;
use heka_core::setup::Missing;
use heka_core::supervisor::ControllerView;
use heka_core::virtual_controller::OutputType;

use crate::text;
use crate::ui::pad::PadModel;

/// Where the window's contents come from, and where the player's choices go.
///
/// Both calls come from the window's thread, [`Backend::screen`] about 60 times a
/// second, so neither may wait on a Bridge or a driver.
pub trait Backend {
    /// What to show now.
    fn screen(&self, now: Instant) -> Screen;

    fn command(&self, command: Command);
}

/// Something the player asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Stop(MacAddress),
    Start(MacAddress),
    /// Replaces the Controller's Virtual Controller with one of this Output Type.
    SetOutputType(MacAddress, OutputType),
    /// Look for the drivers again.
    CheckDrivers,
}

/// Everything the window shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    /// heka has not looked for Controllers yet.
    pub starting: bool,
    /// The drivers the setup screen shows, none when heka has them all.
    pub missing: Vec<Missing>,
    /// Problems that concern heka as a whole.
    pub notices: Vec<Notice>,
    /// One per listed Controller, in arrival order.
    pub cards: Vec<CardModel>,
}

impl Screen {
    /// The status bar's count of Controllers and Bridges.
    pub fn summary(&self) -> String {
        let bridged = self
            .cards
            .iter()
            .filter(|card| card.state == CardState::Bridged)
            .count();
        text::summary(self.cards.len(), bridged)
    }
}

/// How serious a [`Notice`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Warning,
    Error,
}

/// A message shown above the cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub level: Level,
    pub text: String,
}

impl Notice {
    pub fn warning(text: impl Into<String>) -> Self {
        Self {
            level: Level::Warning,
            text: text.into(),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            level: Level::Error,
            text: text.into(),
        }
    }
}

/// Where a Controller stands, as its card words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardState {
    Bridged,
    Held {
        seconds_left: u64,
    },
    Stopped,
    /// Connected and wanting a Bridge it does not have.
    NotBridged,
}

/// The Link heka reads a Controller from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveLink {
    Usb,
    Bluetooth,
}

/// A Controller's battery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    /// `0..=100`.
    pub percent: u8,
    pub charging: bool,
}

/// One Controller's card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardModel {
    pub mac: MacAddress,
    pub name: &'static str,
    /// `None` when the Controller has no Link.
    pub active_link: Option<ActiveLink>,
    /// `None` while unknown.
    pub battery: Option<Battery>,
    pub output_type: OutputType,
    pub state: CardState,
    /// ViGEmBus is missing, so no Controller can be Bridged.
    pub bus_missing: bool,
    /// Why the last Bridge attempt failed.
    pub error: Option<String>,
    /// The live input, `None` while heka reads none.
    pub pad: Option<PadModel>,
}

impl CardModel {
    /// The card for one of the Supervisor's Controllers at this moment.
    pub fn from_view(view: &ControllerView, now: Instant, bus_missing: bool) -> Self {
        // Taken apart so that a new field cannot go unnoticed here.
        let ControllerView {
            mac,
            phase,
            output_type,
            live,
            error,
        } = view;
        let state = match *phase {
            Phase::Bridged => CardState::Bridged,
            Phase::Held { until } => CardState::Held {
                seconds_left: whole_seconds(until.saturating_duration_since(now)),
            },
            Phase::Stopped => CardState::Stopped,
            Phase::Waiting => CardState::NotBridged,
        };
        // heka reads only DualSense Controllers over USB so far (ticket #8 adds
        // Bluetooth and the battery level).
        let active_link = match state {
            CardState::Held { .. } => None,
            _ => Some(ActiveLink::Usb),
        };
        Self {
            mac: *mac,
            name: text::DUALSENSE,
            active_link,
            battery: None,
            output_type: *output_type,
            state,
            bus_missing,
            error: error.clone(),
            pad: live.latest().map(|state| PadModel::from_state(&state)),
        }
    }

    /// What the card's Stop or Start button asks for.
    pub fn toggle(&self) -> Command {
        match self.state {
            CardState::Stopped => Command::Start(self.mac),
            _ => Command::Stop(self.mac),
        }
    }

    /// What choosing `output_type` on the card's switch asks for, if anything.
    pub fn choose(&self, output_type: OutputType) -> Option<Command> {
        (output_type != self.output_type).then_some(Command::SetOutputType(self.mac, output_type))
    }

    /// The line under the drawing that explains the state, if it needs one.
    pub fn detail(&self) -> Option<String> {
        if self.bus_missing && self.state == CardState::NotBridged {
            return Some(match &self.error {
                None => text::setup::NOT_BRIDGED.into(),
                Some(error) => text::with_error(text::setup::NOT_BRIDGED, error),
            });
        }
        if let Some(error) = &self.error {
            return Some(text::bridge_failed(error));
        }
        match self.state {
            CardState::NotBridged => Some(text::all_bridges_taken()),
            CardState::Held { .. } => Some(text::HELD_HINT.into()),
            CardState::Bridged | CardState::Stopped => None,
        }
    }
}

/// Rounded up, so a countdown shows 0 only once the time is over.
fn whole_seconds(duration: Duration) -> u64 {
    duration.as_millis().div_ceil(1000) as u64
}
