//! What heka remembers between runs: per Controller, Stopped and Output Type.
//!
//! The text is JSON. Reading never fails: a missing or corrupt file gives the
//! defaults, and a broken entry gives that Controller the defaults. Where the
//! file lives is up to the caller.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::mac::MacAddress;
use crate::virtual_controller::OutputType;

/// What heka remembers about one Controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ControllerSettings {
    pub stopped: bool,
    pub output_type: OutputType,
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
                let entry = ControllerSettings::deserialize(entry).ok()?;
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
        self.controllers.get(&mac).copied().unwrap_or_default()
    }

    pub fn set_stopped(&mut self, mac: MacAddress, stopped: bool) {
        self.controllers.entry(mac).or_default().stopped = stopped;
    }

    pub fn set_output_type(&mut self, mac: MacAddress, output_type: OutputType) {
        self.controllers.entry(mac).or_default().output_type = output_type;
    }
}
