use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::hiding::{
    DeviceInstancePath, Hiding, HidingChange, HidingError, HidingUnavailable, IdentifyError,
};
use crate::lifecycle::{Change, Removal};
use crate::mac::MacAddress;

pub const INVERSE_CHECK_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HideError {
    NoLink,
    Unidentified(IdentifyError),
    Failed(HidingError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HidingWarning {
    Unavailable(HidingUnavailable),
    EarlierRunNotRestored(HidingError),
    NotRestored(HidingError),
    NotAllowed(HidingError),
    NotHidden(HideError),
    Inverted,
}

type Devices<K> = BTreeMap<K, Result<DeviceInstancePath, IdentifyError>>;

pub struct Hider<K> {
    hiding: Option<Hiding>,
    devices: BTreeMap<MacAddress, Devices<K>>,
    wanted: BTreeSet<MacAddress>,
    left_visible: BTreeSet<MacAddress>,
    pending: BTreeSet<MacAddress>,
    allow_pending: bool,
    unavailable: Option<HidingUnavailable>,
    earlier_run_not_restored: Option<HidingError>,
    not_restored: Option<HidingError>,
    not_allowed: Option<HidingError>,
    not_hidden: Option<HideError>,
    inverted: bool,
    look_for_inverse_at: Option<Instant>,
}

impl<K: Ord> Hider<K> {
    pub fn new(
        hiding: Result<Hiding, HidingUnavailable>,
        left_visible: impl IntoIterator<Item = MacAddress>,
    ) -> Self {
        let (hiding, unavailable) = match hiding {
            Ok(hiding) => (Some(hiding), None),
            Err(error) => (None, Some(error)),
        };
        let mut hider = Self {
            allow_pending: hiding.is_some(),
            hiding,
            devices: BTreeMap::new(),
            wanted: BTreeSet::new(),
            left_visible: left_visible.into_iter().collect(),
            pending: BTreeSet::new(),
            unavailable,
            earlier_run_not_restored: None,
            not_restored: None,
            not_allowed: None,
            not_hidden: None,
            inverted: false,
            look_for_inverse_at: None,
        };
        if let Some(Err(error)) = hider.hiding.as_ref().map(Hiding::recover) {
            hider.earlier_run_not_restored = Some(error);
        }
        hider.allow();
        hider
    }

    pub fn warnings(&self) -> Vec<HidingWarning> {
        use HidingWarning as Warning;
        [
            self.unavailable.clone().map(Warning::Unavailable),
            self.earlier_run_not_restored
                .clone()
                .map(Warning::EarlierRunNotRestored),
            self.not_restored.clone().map(Warning::NotRestored),
            self.not_allowed.clone().map(Warning::NotAllowed),
            self.not_hidden.clone().map(Warning::NotHidden),
            self.inverted.then_some(Warning::Inverted),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    pub fn link_seen(
        &mut self,
        mac: MacAddress,
        link: K,
        device: Result<DeviceInstancePath, IdentifyError>,
    ) {
        self.devices.entry(mac).or_default().insert(link, device);
        if self.wanted.contains(&mac) {
            self.pending.insert(mac);
            self.hide(mac);
        }
    }

    pub fn link_gone(&mut self, mac: MacAddress, link: &K) {
        if let Some(devices) = self.devices.get_mut(&mac) {
            devices.remove(link);
        }
    }

    pub fn follow(&mut self, changes: &[Change]) {
        for (index, change) in changes.iter().enumerate() {
            let replaced = |mac| {
                changes[index + 1..]
                    .iter()
                    .any(|later| matches!(later, Change::Bridged { mac: m, .. } if *m == mac))
            };
            match *change {
                Change::Bridged { mac, .. } | Change::Returned { mac } => {
                    self.wanted.insert(mac);
                    self.pending.insert(mac);
                    self.hide(mac);
                }
                Change::Held { .. } => {}
                Change::Removed {
                    mac,
                    why: Removal::OutputTypeChanged,
                } if replaced(mac) => {}
                Change::Removed { mac, .. } => {
                    self.wanted.remove(&mac);
                    self.stop_trying_to_hide(mac);
                    self.release(mac);
                }
            }
        }
    }

    pub fn set_hide(&mut self, mac: MacAddress, hide: bool) {
        if hide {
            if self.left_visible.remove(&mac) && self.wanted.contains(&mac) {
                self.pending.insert(mac);
                self.hide(mac);
            }
        } else if self.left_visible.insert(mac) {
            self.stop_trying_to_hide(mac);
            self.release(mac);
        }
    }

    pub fn poll(&mut self, now: Instant) {
        self.retry_restore();
        if self.allow_pending {
            self.allow();
        }
        let pending: Vec<MacAddress> = self.pending.iter().copied().collect();
        for mac in pending {
            self.hide(mac);
        }
        self.look_for_inverse(now);
    }

    pub fn take_changes(&mut self) -> Vec<HidingChange> {
        self.hiding
            .as_ref()
            .map(Hiding::take_changes)
            .unwrap_or_default()
    }

    pub fn restore_all(&mut self) -> Result<(), HidingError> {
        self.hiding.as_ref().map_or(Ok(()), Hiding::restore_all)
    }

    fn allow(&mut self) {
        let Some(hiding) = &self.hiding else {
            return;
        };
        match hiding.allow_heka() {
            Ok(()) => {
                self.allow_pending = false;
                self.not_allowed = None;
            }
            Err(error) => self.not_allowed = Some(error),
        }
    }

    fn hide(&mut self, mac: MacAddress) {
        let Some(hiding) = &self.hiding else {
            self.pending.remove(&mac);
            return;
        };
        if self.left_visible.contains(&mac) {
            self.stop_trying_to_hide(mac);
            return;
        }
        let devices = self.devices.get(&mac).filter(|devices| !devices.is_empty());
        let result = match devices {
            Some(devices) => devices
                .values()
                .map(|device| match device {
                    Ok(device) => hiding
                        .hide(&mac.to_string(), device)
                        .map_err(HideError::Failed),
                    Err(error) => Err(HideError::Unidentified(*error)),
                })
                .fold(Ok(()), Result::and),
            None => Err(HideError::NoLink),
        };
        match result {
            Ok(()) => self.stop_trying_to_hide(mac),
            Err(error) => self.not_hidden = Some(error),
        }
    }

    fn stop_trying_to_hide(&mut self, mac: MacAddress) {
        self.pending.remove(&mac);
        if self.pending.is_empty() {
            self.not_hidden = None;
        }
    }

    fn release(&mut self, mac: MacAddress) {
        let released = self
            .hiding
            .as_ref()
            .map(|hiding| hiding.release(&mac.to_string()));
        match released {
            Some(Ok(())) => self.restored(),
            Some(Err(error)) => self.not_restored = Some(error),
            None => {}
        }
    }

    fn retry_restore(&mut self) {
        let Some(hiding) = &self.hiding else {
            return;
        };
        let result = if self.earlier_run_not_restored.is_some() {
            hiding.recover()
        } else if self.not_restored.is_some() {
            hiding.restore()
        } else {
            return;
        };
        match result {
            Ok(()) => self.restored(),
            Err(error) => match &mut self.earlier_run_not_restored {
                Some(earlier) => *earlier = error,
                None => self.not_restored = Some(error),
            },
        }
    }

    fn restored(&mut self) {
        self.earlier_run_not_restored = None;
        self.not_restored = None;
    }

    fn look_for_inverse(&mut self, now: Instant) {
        let Some(hiding) = &self.hiding else {
            return;
        };
        if self.look_for_inverse_at.is_some_and(|at| now < at) {
            return;
        }
        if let Ok(inverted) = hiding.inverted() {
            self.inverted = inverted;
        }
        self.look_for_inverse_at = Some(now + INVERSE_CHECK_INTERVAL);
    }
}
