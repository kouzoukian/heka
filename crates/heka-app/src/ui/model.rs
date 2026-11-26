//! What the window shows, as plain data, and what the player can ask for.
//!
//! The window draws a [`Screen`] and sends [`Command`]s to a [`Backend`]. It
//! redraws only when two samples of the screen differ.

use std::rc::Rc;
use std::time::{Duration, Instant};

use heka_core::lifecycle::Phase;
use heka_core::link::{ControllerModel, LinkKind};
use heka_core::mac::MacAddress;
use heka_core::preferences::Preferences;
use heka_core::setup::Missing;
use heka_core::state::Battery;
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

/// Where the window's buttons send what the player asks for.
pub type SendCommand = Rc<dyn Fn(Command)>;

/// Something the player asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Stop(MacAddress),
    Start(MacAddress),
    /// Replaces the Controller's Virtual Controller with one of this Output Type.
    SetOutputType(MacAddress, OutputType),
    /// Names the Controller; a blank name takes the name away.
    SetNickname(MacAddress, String),
    SetPreferences(MacAddress, Preferences),
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
    pub controllers: Vec<ControllerEntry>,
}

impl Screen {
    /// The status bar's count of errors that concern heka as a whole.
    pub fn errors(&self) -> usize {
        self.notices_at(Level::Error)
    }

    /// The status bar's count of warnings, one for each missing driver among them.
    pub fn warnings(&self) -> usize {
        self.notices_at(Level::Warning) + self.missing.len()
    }

    fn notices_at(&self, level: Level) -> usize {
        self.notices
            .iter()
            .filter(|notice| notice.level == level)
            .count()
    }

    /// The status bar's count of Controllers and Bridges.
    pub fn summary(&self) -> String {
        let bridged = self
            .controllers
            .iter()
            .filter(|entry| entry.state == EntryState::Bridged)
            .count();
        text::summary(self.controllers.len(), bridged)
    }
}

/// Which Controller's pane shows: the first at the start, then the same
/// Controller wherever it moves. When it leaves, the one that takes its place is
/// shown.
#[derive(Debug, Clone, Copy, Default)]
pub struct Selection {
    mac: Option<MacAddress>,
    /// The selected entry's place when last seen.
    place: usize,
}

impl Selection {
    /// Selects the Controller the player picked among `controllers`.
    pub fn choose(&mut self, mac: MacAddress, controllers: &[ControllerEntry]) {
        self.mac = Some(mac);
        self.follow(controllers);
    }

    /// Keeps the selection on one of `controllers`. Call with every new screen.
    pub fn follow(&mut self, controllers: &[ControllerEntry]) {
        self.place = controllers
            .iter()
            .position(|entry| Some(entry.mac) == self.mac)
            .unwrap_or(self.place)
            .min(controllers.len().saturating_sub(1));
        self.mac = controllers.get(self.place).map(|entry| entry.mac);
    }

    /// The selected entry, none when there is no entry.
    pub fn shown<'a>(&self, controllers: &'a [ControllerEntry]) -> Option<&'a ControllerEntry> {
        controllers.iter().find(|entry| Some(entry.mac) == self.mac)
    }
}

/// How serious a [`Notice`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Warning,
    Error,
}

/// A message shown above the selected Controller's pane.
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

/// Where a Controller stands, as the window words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryState {
    Bridged,
    Held {
        seconds_left: u64,
    },
    Stopped,
    /// Connected and wanting a Bridge it does not have.
    NotBridged,
}

/// One Controller as the window lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerEntry {
    pub mac: MacAddress,
    pub name: &'static str,
    /// What the player calls this Controller.
    pub nickname: Option<String>,
    /// This kind of Controller has player lights.
    pub has_player_lights: bool,
    pub preferences: Preferences,
    /// heka's support for this kind of Controller is experimental.
    pub experimental: bool,
    /// `None` when the Controller has no Link.
    pub active_link: Option<LinkKind>,
    /// As the Controller last reported it, `None` while heka reads no input.
    pub battery: Option<Battery>,
    pub output_type: OutputType,
    pub state: EntryState,
    /// ViGEmBus is missing, so no Controller can be Bridged.
    pub bus_missing: bool,
    /// Why the last Bridge attempt failed.
    pub error: Option<String>,
    /// The live input, `None` while heka reads none.
    pub pad: Option<PadModel>,
}

impl ControllerEntry {
    /// The entry for one of the Supervisor's Controllers at this moment.
    pub fn from_view(view: &ControllerView, now: Instant, bus_missing: bool) -> Self {
        // Taken apart so that a new field cannot go unnoticed here.
        let ControllerView {
            mac,
            model,
            phase,
            output_type,
            nickname,
            preferences,
            active_link,
            live,
            error,
        } = view;
        let state = match *phase {
            Phase::Bridged => EntryState::Bridged,
            Phase::Held { until } => EntryState::Held {
                seconds_left: whole_seconds(until.saturating_duration_since(now)),
            },
            Phase::Stopped => EntryState::Stopped,
            Phase::Waiting => EntryState::NotBridged,
        };
        let input = live.latest();
        Self {
            mac: *mac,
            name: text::controller(*model),
            nickname: nickname.clone(),
            has_player_lights: *model == ControllerModel::DualSense,
            preferences: *preferences,
            experimental: *model == ControllerModel::DualShock4,
            active_link: *active_link,
            battery: input.and_then(|input| input.battery),
            output_type: *output_type,
            state,
            bus_missing,
            error: error.clone(),
            pad: input.map(|input| PadModel::from_state(&input)),
        }
    }

    /// What the window calls the Controller: the player's name for it, or its
    /// model's.
    pub fn title(&self) -> &str {
        self.nickname.as_deref().unwrap_or(self.name)
    }

    /// What changing a preference asks for, if it changes anything.
    pub fn prefer(&self, change: impl FnOnce(&mut Preferences)) -> Option<Command> {
        let mut preferences = self.preferences;
        change(&mut preferences);
        (preferences != self.preferences).then_some(Command::SetPreferences(self.mac, preferences))
    }

    /// The last two bytes of the MAC address, to tell apart two Controllers of
    /// one kind.
    pub fn short_mac(&self) -> String {
        let [.., fifth, sixth] = self.mac.0;
        format!("{fifth:02X}:{sixth:02X}")
    }

    /// What the entry's Stop or Start button asks for.
    pub fn toggle(&self) -> Command {
        match self.state {
            EntryState::Stopped => Command::Start(self.mac),
            _ => Command::Stop(self.mac),
        }
    }

    /// What choosing `output_type` on the switch asks for, if anything.
    pub fn choose(&self, output_type: OutputType) -> Option<Command> {
        (output_type != self.output_type).then_some(Command::SetOutputType(self.mac, output_type))
    }

    /// The line that explains the state, if it needs one.
    pub fn detail(&self) -> Option<String> {
        if self.bus_missing && self.state == EntryState::NotBridged {
            return Some(match &self.error {
                None => text::setup::NOT_BRIDGED.into(),
                Some(error) => text::with_error(text::setup::NOT_BRIDGED, error),
            });
        }
        if let Some(error) = &self.error {
            return Some(text::bridge_failed(error));
        }
        match self.state {
            EntryState::NotBridged => Some(text::all_bridges_taken()),
            EntryState::Held { .. } => Some(text::HELD_HINT.into()),
            EntryState::Bridged | EntryState::Stopped => None,
        }
    }
}

/// Rounded up, so a countdown shows 0 only once the time is over.
fn whole_seconds(duration: Duration) -> u64 {
    duration.as_millis().div_ceil(1000) as u64
}
