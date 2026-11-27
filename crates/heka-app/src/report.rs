use std::collections::BTreeMap;

use heka_core::hider::HidingWarning;
use heka_core::mac::MacAddress;
use heka_core::radio::TurnOffError;
use heka_core::supervisor::{BridgeError, ControllerView};

use crate::text;

#[derive(Debug, Default)]
pub struct Reporter {
    bridge_errors: BTreeMap<MacAddress, BridgeError>,
    turn_off_errors: BTreeMap<MacAddress, TurnOffError>,
    hiding: Vec<HidingWarning>,
    hid_error: Option<String>,
}

impl Reporter {
    pub fn controllers(&mut self, controllers: &[ControllerView]) -> Vec<String> {
        let mut lines = news(
            &mut self.bridge_errors,
            controllers,
            |view| view.error.clone(),
            text::log::bridge_error,
        );
        lines.extend(news(
            &mut self.turn_off_errors,
            controllers,
            |view| view.turn_off_error.clone(),
            text::log::cannot_turn_off,
        ));
        lines
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

fn news<E: PartialEq>(
    reported: &mut BTreeMap<MacAddress, E>,
    controllers: &[ControllerView],
    error_of: impl Fn(&ControllerView) -> Option<E>,
    words: impl Fn(MacAddress, &E) -> String,
) -> Vec<String> {
    let errors: BTreeMap<MacAddress, E> = controllers
        .iter()
        .filter_map(|view| Some((view.mac, error_of(view)?)))
        .collect();
    let news = errors
        .iter()
        .filter(|&(mac, error)| reported.get(mac) != Some(error))
        .map(|(&mac, error)| words(mac, error))
        .collect();
    *reported = errors;
    news
}
