use std::ffi::OsStr;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::autostart;
use crate::game_rules::GameRules;
use crate::window_placement::WindowPlacement;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseAction {
    #[default]
    KeepRunning,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppSettings {
    pub start_in_tray: bool,
    pub on_close: CloseAction,
    pub check_for_releases: bool,
    #[serde(skip_serializing_if = "GameRules::is_empty")]
    pub game_rules: GameRules,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowPlacement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppChange {
    StartInTray(bool),
    OnClose(CloseAction),
    CheckForReleases(bool),
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            start_in_tray: false,
            on_close: CloseAction::default(),
            check_for_releases: true,
            game_rules: GameRules::default(),
            window: None,
        }
    }
}

impl AppSettings {
    pub(crate) fn load(entry: Option<&Value>) -> Self {
        let Some(entry) = entry else {
            return Self::default();
        };
        Self {
            start_in_tray: field(entry, "start_in_tray"),
            on_close: field(entry, "on_close"),
            check_for_releases: given(entry, "check_for_releases")
                .unwrap_or(Self::default().check_for_releases),
            game_rules: GameRules::load(entry.get("game_rules")),
            window: given::<WindowPlacement>(entry, "window")
                .filter(|window| window.bounds.width > 0 && window.bounds.height > 0),
        }
    }

    pub fn change(&mut self, change: AppChange) {
        match change {
            AppChange::StartInTray(start_in_tray) => self.start_in_tray = start_in_tray,
            AppChange::OnClose(on_close) => self.on_close = on_close,
            AppChange::CheckForReleases(check) => self.check_for_releases = check,
        }
    }

    pub fn opens_in_tray<I>(&self, args: I) -> bool
    where
        I: IntoIterator,
        I::Item: AsRef<OsStr>,
    {
        self.start_in_tray || autostart::opens_in_tray(args)
    }
}

fn field<T: DeserializeOwned + Default>(entry: &Value, name: &str) -> T {
    given(entry, name).unwrap_or_default()
}

fn given<T: DeserializeOwned>(entry: &Value, name: &str) -> Option<T> {
    entry.get(name).and_then(|value| T::deserialize(value).ok())
}
