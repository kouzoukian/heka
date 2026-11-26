use std::collections::BTreeMap;

use heka_core::hider::HidingWarning;
use heka_core::mac::MacAddress;
use heka_core::supervisor::{BridgeError, ControllerView};

use crate::text;

#[derive(Debug, Default)]
pub struct Reporter {
    bridge_errors: BTreeMap<MacAddress, BridgeError>,
    hiding: Vec<HidingWarning>,
    hid_error: Option<String>,
}

impl Reporter {
    pub fn controllers(&mut self, controllers: &[ControllerView]) -> Vec<String> {
        let errors: BTreeMap<MacAddress, BridgeError> = controllers
            .iter()
            .filter_map(|view| Some((view.mac, view.error.clone()?)))
            .collect();
        let news = errors
            .iter()
            .filter(|&(mac, error)| self.bridge_errors.get(mac) != Some(error))
            .map(|(&mac, error)| text::log::bridge_error(mac, error))
            .collect();
        self.bridge_errors = errors;
        news
    }

    pub fn hiding(&mut self, warnings: &[HidingWarning]) -> Vec<String> {
        let gone = self
            .hiding
            .iter()
            .filter(|warning| !warnings.contains(warning))
            .map(|warning| text::log::no_longer(&text::hiding_warning(warning)));
        let new = warnings
            .iter()
            .filter(|warning| !self.hiding.contains(warning))
            .map(text::hiding_warning);
        let news = gone.chain(new).collect();
        self.hiding = warnings.to_vec();
        news
    }

    pub fn hid_error(&mut self, error: Option<&str>) -> Option<String> {
        if self.hid_error.as_deref() == error {
            return None;
        }
        self.hid_error = error.map(str::to_owned);
        Some(match error {
            Some(error) => text::hid_error(error),
            None => text::log::LOOKING_AGAIN.into(),
        })
    }
}
