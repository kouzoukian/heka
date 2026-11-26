use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::app_settings::AppSettings;
use crate::mac::MacAddress;
use crate::preferences::Preferences;
use crate::virtual_controller::OutputType;

pub const MAX_NICKNAME_CHARS: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ControllerSettings {
    pub stopped: bool,
    pub output_type: OutputType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    #[serde(flatten)]
    pub preferences: Preferences,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Settings {
    app: AppSettings,
    controllers: BTreeMap<MacAddress, ControllerSettings>,
}

impl Settings {
    pub fn load(text: Option<&str>) -> Self {
        let Some(Value::Object(root)) = text.and_then(|text| serde_json::from_str(text).ok())
        else {
            return Self::default();
        };
        let controllers = root
            .get("controllers")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(mac, entry)| {
                let mac = mac.parse().ok()?;
                let mut entry = ControllerSettings::deserialize(entry).ok()?;
                entry.nickname = entry.nickname.as_deref().and_then(nickname);
                Some((mac, entry))
            })
            .collect();
        Self {
            app: AppSettings::load(root.get("app")),
            controllers,
        }
    }

    pub fn can_read(text: &str) -> bool {
        matches!(serde_json::from_str(text), Ok(Value::Object(_)))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("settings always serialize")
    }

    pub fn app(&self) -> &AppSettings {
        &self.app
    }

    pub fn set_app(&mut self, app: AppSettings) {
        self.app = app;
    }

    pub fn controller(&self, mac: MacAddress) -> ControllerSettings {
        self.controllers.get(&mac).cloned().unwrap_or_default()
    }

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

    pub fn set_nickname(&mut self, mac: MacAddress, name: &str) {
        self.controllers.entry(mac).or_default().nickname = nickname(name);
    }

    pub fn set_preferences(&mut self, mac: MacAddress, preferences: Preferences) {
        self.controllers.entry(mac).or_default().preferences = preferences;
    }
}

fn nickname(name: &str) -> Option<String> {
    let cut: String = name.trim().chars().take(MAX_NICKNAME_CHARS).collect();
    let cut = cut.trim_end();
    (!cut.is_empty()).then(|| cut.to_owned())
}
