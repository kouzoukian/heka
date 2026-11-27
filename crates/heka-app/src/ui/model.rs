use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use heka_core::app_settings::{AppChange, CloseAction};
use heka_core::feedback::Motor;
use heka_core::idle::IdleTime;
use heka_core::lifecycle::Phase;
use heka_core::link::{ControllerModel, LinkKind};
use heka_core::mac::MacAddress;
use heka_core::preferences::{PreferenceChange, Preferences};
use heka_core::profile::{Binding, Input, Profile};
use heka_core::release::Version;
use heka_core::setup::Missing;
use heka_core::state::Battery;
use heka_core::supervisor::ControllerView;
use heka_core::virtual_controller::OutputType;
use heka_core::window_placement::WindowPlacement;

use crate::log::Recent;
use crate::text;
use crate::ui::pad::PadModel;

pub trait Backend {
    fn screen(&self, now: Instant) -> Screen;

    fn command(&self, command: Command);

    fn window_placement(&self) -> Option<WindowPlacement>;
}

pub type SendCommand = Rc<dyn Fn(Command)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Stop(MacAddress),
    Start(MacAddress),
    SetOutputType(MacAddress, OutputType),
    SetNickname(MacAddress, String),
    ChangePreference(MacAddress, PreferenceChange),
    TestRumble(MacAddress, Motor),
    EndRumbleTest(MacAddress),
    TurnOff(MacAddress),
    SetIdleTime(MacAddress, Option<IdleTime>),
    CheckDrivers,
    OpenLogFolder,
    SetStartWithWindows(bool),
    ChangeAppSetting(AppChange),
    RememberWindow(WindowPlacement),
    OpenSettingsFolder,
    ChooseProfile(MacAddress, Option<String>),
    CreateProfile(String),
    BindInput(String, Input, Binding),
    SetProfileOutputType(String, Option<OutputType>),
    DuplicateProfile(Option<String>),
    RenameProfile(String, String),
    DeleteProfile(String),
    ImportProfile(PathBuf),
    ExportProfile(String, PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AppEntry {
    pub start_with_windows: bool,
    pub start_in_tray: bool,
    pub on_close: CloseAction,
    pub check_for_releases: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    pub starting: bool,
    pub missing: Vec<Missing>,
    pub notices: Vec<Notice>,
    pub controllers: Vec<ControllerEntry>,
    pub profiles: Vec<ProfileEntry>,
    pub log: Recent,
    pub app: AppEntry,
    pub newer_release: Option<Version>,
}

impl Screen {
    pub fn errors(&self) -> usize {
        self.notices_at(Level::Error)
    }

    pub fn warnings(&self) -> usize {
        self.notices_at(Level::Warning) + self.missing.len()
    }

    fn notices_at(&self, level: Level) -> usize {
        self.notices
            .iter()
            .filter(|notice| notice.level == level)
            .count()
    }

    pub fn summary(&self) -> String {
        let bridged = self
            .controllers
            .iter()
            .filter(|entry| entry.state == EntryState::Bridged)
            .count();
        text::summary(self.controllers.len(), bridged)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileEntry {
    pub id: String,
    pub profile: Profile,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Selection {
    mac: Option<MacAddress>,
    place: usize,
}

impl Selection {
    pub fn choose(&mut self, mac: MacAddress, controllers: &[ControllerEntry]) {
        self.mac = Some(mac);
        self.follow(controllers);
    }

    pub fn follow(&mut self, controllers: &[ControllerEntry]) {
        self.place = controllers
            .iter()
            .position(|entry| Some(entry.mac) == self.mac)
            .unwrap_or(self.place)
            .min(controllers.len().saturating_sub(1));
        self.mac = controllers.get(self.place).map(|entry| entry.mac);
    }

    pub fn shown<'a>(&self, controllers: &'a [ControllerEntry]) -> Option<&'a ControllerEntry> {
        controllers.iter().find(|entry| Some(entry.mac) == self.mac)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Warning,
    Error,
}

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

    pub fn cannot_turn_off(view: &ControllerView) -> Option<Self> {
        let error = view.turn_off_error.as_ref()?;
        let controller = view
            .nickname
            .clone()
            .unwrap_or_else(|| text::named(view.model, &short_mac(view.mac)));
        Some(Self::warning(text::cannot_turn_off(&controller, error)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryState {
    Bridged,
    Held { seconds_left: u64 },
    Stopped,
    NotBridged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerEntry {
    pub mac: MacAddress,
    pub name: &'static str,
    pub nickname: Option<String>,
    pub has_player_lights: bool,
    pub preferences: Preferences,
    pub experimental: bool,
    pub active_link: Option<LinkKind>,
    pub has_bluetooth: bool,
    pub idle_time: Option<IdleTime>,
    pub battery: Option<Battery>,
    pub output_type: OutputType,
    pub profile: Option<String>,
    pub output_type_from_profile: bool,
    pub state: EntryState,
    pub bus_missing: bool,
    pub error: Option<String>,
    pub pad: Option<PadModel>,
}

impl ControllerEntry {
    pub fn from_view(view: &ControllerView, now: Instant, bus_missing: bool) -> Self {
        let ControllerView {
            mac,
            model,
            phase,
            output_type,
            nickname,
            preferences,
            active_link,
            has_bluetooth,
            idle_time,
            profile,
            output_type_from_profile,
            live,
            error,
            turn_off_error: _,
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
            has_bluetooth: *has_bluetooth,
            idle_time: *idle_time,
            battery: input.and_then(|input| input.battery),
            output_type: *output_type,
            profile: profile.clone(),
            output_type_from_profile: *output_type_from_profile,
            state,
            bus_missing,
            error: error.as_ref().map(text::bridge_error),
            pad: input.map(|input| PadModel::from_state(&input)),
        }
    }

    pub fn title(&self) -> &str {
        self.nickname.as_deref().unwrap_or(self.name)
    }

    pub fn prefer(&self, change: PreferenceChange) -> Command {
        Command::ChangePreference(self.mac, change)
    }

    pub fn test_rumble(&self, motor: Motor) -> Option<Command> {
        (self.state == EntryState::Bridged).then_some(Command::TestRumble(self.mac, motor))
    }

    pub fn short_mac(&self) -> String {
        short_mac(self.mac)
    }

    pub fn turn_off(&self) -> Option<Command> {
        self.has_bluetooth.then_some(Command::TurnOff(self.mac))
    }

    pub fn choose_idle_time(&self, idle_time: Option<IdleTime>) -> Command {
        Command::SetIdleTime(self.mac, idle_time)
    }

    pub fn toggle(&self) -> Command {
        match self.state {
            EntryState::Stopped => Command::Start(self.mac),
            _ => Command::Stop(self.mac),
        }
    }

    pub fn choose(&self, output_type: OutputType) -> Option<Command> {
        (!self.output_type_from_profile && output_type != self.output_type)
            .then_some(Command::SetOutputType(self.mac, output_type))
    }

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

fn short_mac(mac: MacAddress) -> String {
    let [.., fifth, sixth] = mac.0;
    format!("{fifth:02X}:{sixth:02X}")
}

fn whole_seconds(duration: Duration) -> u64 {
    duration.as_millis().div_ceil(1000) as u64
}
