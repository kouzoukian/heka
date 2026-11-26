//! What heka remembers between runs: per Controller, Stopped, Output Type, the
//! player's name for it and its [`Preferences`].
//!
//! The text is JSON. Reading never fails: a missing or corrupt file gives the
//! defaults, and a broken entry gives that Controller the defaults. Where the
//! file lives is up to the caller.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::mac::MacAddress;
use crate::preferences::Preferences;
use crate::virtual_controller::OutputType;

/// The longest name a Controller can be given, in characters.
pub const MAX_NICKNAME_CHARS: usize = 24;

/// What heka remembers about one Controller.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ControllerSettings {
    pub stopped: bool,
    pub output_type: OutputType,
    /// What the player calls this Controller, `None` to name it by its model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    /// Written beside the other fields in the file.
    #[serde(flatten)]
    pub preferences: Preferences,
}

/// Everything heka remembers.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Settings {
    controllers: BTreeMap<MacAddress, ControllerSettings>,
}

impl Settings {
    /// Reads settings saved by [`Settings::to_json`]. `None` means there is no file.
    pub fn load(text: Option<&str>) -> Self {
        let Some(Value::Object(root)) = text.and_then(|text| serde_json::from_str(text).ok())
        else {
            return Self::default();
        };
        let Some(Value::Object(entries)) = root.get("controllers") else {
            return Self::default();
        };
        let controllers = entries
            .iter()
            .filter_map(|(mac, entry)| {
                let mac = mac.parse().ok()?;
                let mut entry = ControllerSettings::deserialize(entry).ok()?;
                entry.nickname = entry.nickname.as_deref().and_then(nickname);
                Some((mac, entry))
            })
            .collect();
        Self { controllers }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("settings always serialize")
    }

    /// This Controller's settings, the defaults if it has none.
    pub fn controller(&self, mac: MacAddress) -> ControllerSettings {
        self.controllers.get(&mac).cloned().unwrap_or_default()
    }

    /// The Controllers the player lets games see.
    pub fn never_hidden(&self) -> impl Iterator<Item = MacAddress> + '_ {
        self.controllers
            .iter()
            .filter(|(_, settings)| !settings.preferences.hide)
            .map(|(&mac, _)| mac)
    }

    pub fn set_stopped(&mut self, mac: MacAddress, stopped: bool) {
        self.controllers.entry(mac).or_default().stopped = stopped;
    }

    pub fn set_output_type(&mut self, mac: MacAddress, output_type: OutputType) {
        self.controllers.entry(mac).or_default().output_type = output_type;
    }

    /// Names this Controller. A blank name takes the name away.
    pub fn set_nickname(&mut self, mac: MacAddress, name: &str) {
        self.controllers.entry(mac).or_default().nickname = nickname(name);
    }

    pub fn set_preferences(&mut self, mac: MacAddress, preferences: Preferences) {
        self.controllers.entry(mac).or_default().preferences = preferences;
    }
}

/// A name as it is remembered: without the space around it, at most
/// [`MAX_NICKNAME_CHARS`] long, `None` if nothing is left.
fn nickname(name: &str) -> Option<String> {
    let cut: String = name.trim().chars().take(MAX_NICKNAME_CHARS).collect();
    let cut = cut.trim_end();
    (!cut.is_empty()).then(|| cut.to_owned())
}
