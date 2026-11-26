//! What the window shows, as plain data, and what the player can ask for.
//!
//! The window draws a [`Screen`] and sends [`Command`]s to a [`Backend`]. It
//! redraws only when two samples of the screen differ.

use std::time::{Duration, Instant};

use heka_core::lifecycle::Phase;
use heka_core::mac::MacAddress;
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
    /// Only sent for a choice in the card's [`CardModel::outputs`].
    SetOutput(MacAddress, OutputChoice),
}

/// Everything the window shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    /// heka has not looked for Controllers yet.
    pub starting: bool,
    /// Problems that concern heka as a whole.
    pub notices: Vec<Notice>,
    /// One per listed Controller, in arrival order.
    pub cards: Vec<CardModel>,
}

impl Screen {
    /// Before the first sample.
    pub const STARTING: Self = Self {
        starting: true,
        notices: Vec::new(),
        cards: Vec::new(),
    };

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

/// A position of the Output Type switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputChoice {
    Xbox,
    Ps4,
}

impl OutputChoice {
    /// In the order the switch shows them.
    pub const ALL: [Self; 2] = [Self::Xbox, Self::Ps4];

    /// The Output Type behind this choice, `None` while heka cannot create it.
    pub fn output_type(self) -> Option<OutputType> {
        match self {
            Self::Xbox => Some(OutputType::Xbox),
            Self::Ps4 => None,
        }
    }

    pub fn from_output_type(output_type: OutputType) -> Self {
        match output_type {
            OutputType::Xbox => Self::Xbox,
        }
    }
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
    pub output: OutputChoice,
    /// The choices the player can switch between.
    pub outputs: Vec<OutputChoice>,
    pub state: CardState,
    /// Why the last Bridge attempt failed.
    pub error: Option<String>,
    /// The live input, `None` while heka reads none.
    pub pad: Option<PadModel>,
}

impl CardModel {
    /// The card for one of the Supervisor's Controllers at this moment.
    pub fn from_view(view: &ControllerView, now: Instant) -> Self {
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
            output: OutputChoice::from_output_type(*output_type),
            outputs: OutputChoice::ALL
                .into_iter()
                .filter(|choice| choice.output_type().is_some())
                .collect(),
            state,
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

    /// The line under the drawing that explains the state, if it needs one.
    pub fn detail(&self) -> Option<String> {
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
