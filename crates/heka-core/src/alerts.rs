use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::lifecycle::Phase;
use crate::mac::MacAddress;
use crate::state::{Battery, Charge, LOW_BATTERY_PERCENT};
use crate::supervisor::{BridgeError, ControllerView};

pub const SETTLED_BRIDGE: Duration = Duration::from_secs(10);

const RECHARGED_ABOVE: u8 = LOW_BATTERY_PERCENT + 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Notifications {
    pub low_battery: bool,
    pub bridge_failed: bool,
    pub connections: bool,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            low_battery: true,
            bridge_failed: true,
            connections: false,
        }
    }
}

impl Notifications {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub(crate) fn load(value: Option<&Value>) -> Self {
        let defaults = Self::default();
        let Some(value) = value else {
            return defaults;
        };
        Self {
            low_battery: given(value, "low_battery").unwrap_or(defaults.low_battery),
            bridge_failed: given(value, "bridge_failed").unwrap_or(defaults.bridge_failed),
            connections: given(value, "connections").unwrap_or(defaults.connections),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watched {
    pub mac: MacAddress,
    pub name: String,
    pub phase: Phase,
    pub battery: Option<Battery>,
    pub failure: Option<BridgeError>,
}

impl Watched {
    pub fn of(view: &ControllerView, name: String) -> Self {
        Self {
            mac: view.mac,
            name,
            phase: view.phase,
            battery: view.live.battery(),
            failure: view.error.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alert {
    LowBattery { percent: u8 },
    BridgeFailed(BridgeError),
    Connected,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub mac: MacAddress,
    pub name: String,
    pub alert: Alert,
}

#[derive(Debug, Default)]
struct Known {
    name: String,
    listed: bool,
    warned_low: bool,
    warned_failed: bool,
    bridged_since: Option<Instant>,
}

impl Known {
    fn runs_low(&mut self, battery: Option<Battery>) -> Option<u8> {
        let battery = battery?;
        if battery.is_low() {
            let first = !self.warned_low;
            self.warned_low = true;
            return first.then_some(battery.percent);
        }
        let recharged = match battery.charge {
            Charge::Charging | Charge::Full => true,
            Charge::Discharging => battery.percent > RECHARGED_ABOVE,
            Charge::NotCharging | Charge::Unknown => false,
        };
        self.warned_low &= !recharged;
        None
    }

    fn fails(&mut self, controller: &Watched, now: Instant) -> Option<BridgeError> {
        let bridged = controller.phase == Phase::Bridged && controller.failure.is_none();
        if bridged {
            let since = *self.bridged_since.get_or_insert(now);
            if now.saturating_duration_since(since) >= SETTLED_BRIDGE {
                self.warned_failed = false;
            }
            return None;
        }
        self.bridged_since = None;
        match controller.phase {
            Phase::Stopped => {
                self.warned_failed = false;
                None
            }
            Phase::Waiting if !self.warned_failed => {
                let error = controller.failure.clone()?;
                self.warned_failed = true;
                Some(error)
            }
            Phase::Waiting | Phase::Bridged | Phase::Held { .. } => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct Alerts {
    looked: bool,
    known: BTreeMap<MacAddress, Known>,
}

impl Alerts {
    pub fn observe(
        &mut self,
        controllers: &[Watched],
        wanted: Notifications,
        now: Instant,
    ) -> Vec<Notification> {
        let mut found = Vec::new();
        let mut tell = |wanted: bool, mac: MacAddress, name: &str, alert: Alert| {
            if wanted {
                found.push(Notification {
                    mac,
                    name: name.to_owned(),
                    alert,
                });
            }
        };
        for controller in controllers {
            let mac = controller.mac;
            let known = self.known.entry(mac).or_default();
            known.name.clone_from(&controller.name);
            if !known.listed && self.looked {
                tell(wanted.connections, mac, &known.name, Alert::Connected);
            }
            known.listed = true;
            if let Some(percent) = known.runs_low(controller.battery) {
                let alert = Alert::LowBattery { percent };
                tell(wanted.low_battery, mac, &known.name, alert);
            }
            if let Some(error) = known.fails(controller, now) {
                let alert = Alert::BridgeFailed(error);
                tell(wanted.bridge_failed, mac, &known.name, alert);
            }
        }
        for (&mac, known) in &mut self.known {
            if known.listed && !controllers.iter().any(|controller| controller.mac == mac) {
                known.listed = false;
                known.warned_failed = false;
                known.bridged_since = None;
                tell(wanted.connections, mac, &known.name, Alert::Removed);
            }
        }
        self.looked = true;
        found
    }
}

fn given<T: DeserializeOwned>(value: &Value, name: &str) -> Option<T> {
    value
        .get(name)
        .and_then(|value| <T as Deserialize>::deserialize(value).ok())
}
