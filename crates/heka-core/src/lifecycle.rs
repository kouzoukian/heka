//! The Bridge lifecycle: which Controllers are Bridged, Held, Stopped or waiting.
//!
//! This is a pure state machine. Link and timer events go in, each carrying the
//! time it happens at, and [`Change`]s come out. It owns no threads, Links or
//! Virtual Controllers: [`crate::supervisor::Supervisor`] carries the changes out.
//!
//! The changes are also where per-Controller side effects attach. Hiding, for
//! example, hides a Controller on [`Change::Bridged`] and [`Change::Returned`]
//! (a returning Controller is a new device to the system), keeps it hidden while
//! [`Phase::Held`], and restores it on [`Change::Removed`], whatever the reason.

use std::time::{Duration, Instant};

use crate::mac::MacAddress;
use crate::settings::Settings;
use crate::virtual_controller::OutputType;

/// How many Controllers can be Bridged or Held at once.
pub const MAX_BRIDGES: usize = 4;

/// How long a Bridge whose Controller lost every Link keeps its Virtual Controller.
pub const HOLD_TIME: Duration = Duration::from_secs(30);

/// How long after a failed Bridge heka tries again.
pub const BRIDGE_RETRY_DELAY: Duration = Duration::from_secs(2);

/// Where one listed Controller stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Connected and driving its Virtual Controller.
    Bridged,
    /// Every Link lost. The Virtual Controller stays, released, until `until`.
    Held { until: Instant },
    /// Connected, and the player told heka not to Bridge it.
    Stopped,
    /// Connected and wanting a Bridge, but all [`MAX_BRIDGES`] are taken or the
    /// last attempt failed.
    Waiting,
}

/// Why a Virtual Controller is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    /// The player Stopped the Controller.
    Stopped,
    /// The Controller was Held for [`HOLD_TIME`] and did not return.
    Expired,
    /// The Virtual Controller could not be created or refused an update.
    Failed,
    /// heka is quitting.
    Quit,
}

/// A step in one Controller's lifecycle, for the caller to carry out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Create a Virtual Controller of this Output Type and Bridge the Controller's Link to it.
    Bridged {
        mac: MacAddress,
        output_type: OutputType,
    },
    /// The Controller lost every Link: release everything on its Virtual Controller and keep it.
    Held { mac: MacAddress },
    /// A Held Controller has a Link again: Bridge it to the same Virtual Controller.
    Returned { mac: MacAddress },
    /// Remove the Controller's Virtual Controller.
    Removed { mac: MacAddress, why: Removal },
}

#[derive(Debug)]
struct Entry {
    mac: MacAddress,
    phase: Phase,
    /// While [`Phase::Waiting`] after a failure: no new Bridge before this.
    retry_at: Option<Instant>,
}

/// Every listed Controller, in arrival order, with the settings that shape them.
#[derive(Debug)]
pub struct Lifecycle {
    settings: Settings,
    entries: Vec<Entry>,
}

impl Lifecycle {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            entries: Vec::new(),
        }
    }

    /// The settings as they stand, to be saved after [`Self::stop`] or [`Self::start`].
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Every listed Controller in arrival order: connected ones, and Held ones.
    pub fn controllers(&self) -> impl Iterator<Item = (MacAddress, Phase)> + '_ {
        self.entries.iter().map(|entry| (entry.mac, entry.phase))
    }

    pub fn phase(&self, mac: MacAddress) -> Option<Phase> {
        self.entry(mac).map(|entry| entry.phase)
    }

    /// The next time [`Self::tick`] has something to do.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.entries
            .iter()
            .filter_map(|entry| match entry.phase {
                Phase::Held { until } => Some(until),
                Phase::Waiting => entry.retry_at,
                _ => None,
            })
            .min()
    }

    /// A Link to this Controller appeared.
    pub fn link_arrived(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        if let Some(entry) = self.entry_mut(mac) {
            if let Phase::Held { .. } = entry.phase {
                entry.phase = Phase::Bridged;
                return vec![Change::Returned { mac }];
            }
            // Already connected. Several Links per Controller are ticket #8.
            return Vec::new();
        }
        let phase = if self.settings.controller(mac).stopped {
            Phase::Stopped
        } else {
            Phase::Waiting
        };
        self.entries.push(Entry {
            mac,
            phase,
            retry_at: None,
        });
        let mut changes = Vec::new();
        self.fill_slots(now, &mut changes);
        changes
    }

    /// The Controller's Link went away.
    pub fn link_lost(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let Some(index) = self.index(mac) else {
            return Vec::new();
        };
        match self.entries[index].phase {
            Phase::Bridged => {
                self.entries[index].phase = Phase::Held {
                    until: now + HOLD_TIME,
                };
                vec![Change::Held { mac }]
            }
            Phase::Held { .. } => Vec::new(),
            Phase::Stopped | Phase::Waiting => {
                self.entries.remove(index);
                Vec::new()
            }
        }
    }

    /// The player Stops this Controller. Remembered in the settings.
    pub fn stop(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let Some(index) = self.index(mac) else {
            return Vec::new();
        };
        self.settings.set_stopped(mac, true);
        let mut changes = Vec::new();
        match self.entries[index].phase {
            Phase::Stopped => return changes,
            Phase::Waiting => {}
            Phase::Bridged => changes.push(Change::Removed {
                mac,
                why: Removal::Stopped,
            }),
            Phase::Held { .. } => {
                self.entries.remove(index);
                changes.push(Change::Removed {
                    mac,
                    why: Removal::Stopped,
                });
                self.fill_slots(now, &mut changes);
                return changes;
            }
        }
        self.entries[index].phase = Phase::Stopped;
        self.entries[index].retry_at = None;
        self.fill_slots(now, &mut changes);
        changes
    }

    /// The player starts a Stopped Controller again. Remembered in the settings.
    pub fn start(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let mut changes = Vec::new();
        let Some(entry) = self.entry_mut(mac) else {
            return changes;
        };
        if entry.phase != Phase::Stopped {
            return changes;
        }
        entry.phase = Phase::Waiting;
        self.settings.set_stopped(mac, false);
        self.fill_slots(now, &mut changes);
        changes
    }

    /// The Bridge could not be made or broke down. Its Virtual Controller is gone;
    /// another is tried after [`BRIDGE_RETRY_DELAY`].
    pub fn bridge_failed(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let mut changes = Vec::new();
        let Some(entry) = self.entry_mut(mac) else {
            return changes;
        };
        if entry.phase != Phase::Bridged {
            return changes;
        }
        entry.phase = Phase::Waiting;
        entry.retry_at = Some(now + BRIDGE_RETRY_DELAY);
        changes.push(Change::Removed {
            mac,
            why: Removal::Failed,
        });
        self.fill_slots(now, &mut changes);
        changes
    }

    /// Time has passed: Holds expire and waiting Controllers get free slots.
    pub fn tick(&mut self, now: Instant) -> Vec<Change> {
        let mut changes = Vec::new();
        self.entries.retain(|entry| match entry.phase {
            Phase::Held { until } if until <= now => {
                changes.push(Change::Removed {
                    mac: entry.mac,
                    why: Removal::Expired,
                });
                false
            }
            _ => true,
        });
        self.fill_slots(now, &mut changes);
        changes
    }

    /// heka is quitting: every Virtual Controller goes, and the list is emptied.
    pub fn quit(&mut self) -> Vec<Change> {
        self.entries
            .drain(..)
            .filter(|entry| matches!(entry.phase, Phase::Bridged | Phase::Held { .. }))
            .map(|entry| Change::Removed {
                mac: entry.mac,
                why: Removal::Quit,
            })
            .collect()
    }

    /// Bridges waiting Controllers, oldest first, while slots are free.
    fn fill_slots(&mut self, now: Instant, changes: &mut Vec<Change>) {
        let mut used = self
            .entries
            .iter()
            .filter(|entry| matches!(entry.phase, Phase::Bridged | Phase::Held { .. }))
            .count();
        for entry in &mut self.entries {
            if used >= MAX_BRIDGES {
                break;
            }
            let ready = entry.retry_at.is_none_or(|at| at <= now);
            if entry.phase == Phase::Waiting && ready {
                entry.phase = Phase::Bridged;
                entry.retry_at = None;
                used += 1;
                changes.push(Change::Bridged {
                    mac: entry.mac,
                    output_type: self.settings.controller(entry.mac).output_type,
                });
            }
        }
    }

    fn index(&self, mac: MacAddress) -> Option<usize> {
        self.entries.iter().position(|entry| entry.mac == mac)
    }

    fn entry(&self, mac: MacAddress) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.mac == mac)
    }

    fn entry_mut(&mut self, mac: MacAddress) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|entry| entry.mac == mac)
    }
}
