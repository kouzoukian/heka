use std::time::{Duration, Instant};

use crate::mac::MacAddress;
use crate::settings::Settings;
use crate::virtual_controller::OutputType;

pub const MAX_BRIDGES: usize = 4;

pub const HOLD_TIME: Duration = Duration::from_secs(30);

pub const BRIDGE_RETRY_DELAY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Bridged,
    Held { until: Instant },
    Stopped,
    Waiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    Stopped,
    Expired,
    Failed,
    OutputTypeChanged,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Bridged {
        mac: MacAddress,
        output_type: OutputType,
    },
    Held {
        mac: MacAddress,
    },
    Returned {
        mac: MacAddress,
    },
    Removed {
        mac: MacAddress,
        why: Removal,
    },
}

#[derive(Debug)]
struct Entry {
    mac: MacAddress,
    phase: Phase,
    links: usize,
    retry_at: Option<Instant>,
}

#[derive(Debug)]
pub struct Lifecycle {
    settings: Settings,
    entries: Vec<Entry>,
    bridging_paused: bool,
}

impl Lifecycle {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            entries: Vec::new(),
            bridging_paused: false,
        }
    }

    pub fn paused(settings: Settings) -> Self {
        Self {
            bridging_paused: true,
            ..Self::new(settings)
        }
    }

    pub fn resume_bridging(&mut self, now: Instant) -> Vec<Change> {
        self.bridging_paused = false;
        let mut changes = Vec::new();
        self.fill_slots(now, &mut changes);
        changes
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub(crate) fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    pub fn controllers(&self) -> impl Iterator<Item = (MacAddress, Phase)> + '_ {
        self.entries.iter().map(|entry| (entry.mac, entry.phase))
    }

    pub fn phase(&self, mac: MacAddress) -> Option<Phase> {
        self.entry(mac).map(|entry| entry.phase)
    }

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

    pub fn link_arrived(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        if let Some(entry) = self.entry_mut(mac) {
            entry.links += 1;
            if let Phase::Held { .. } = entry.phase {
                entry.phase = Phase::Bridged;
                return vec![Change::Returned { mac }];
            }
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
            links: 1,
            retry_at: None,
        });
        let mut changes = Vec::new();
        self.fill_slots(now, &mut changes);
        changes
    }

    pub fn link_lost(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let Some(index) = self.index(mac) else {
            return Vec::new();
        };
        let entry = &mut self.entries[index];
        if entry.links > 1 {
            entry.links -= 1;
            return Vec::new();
        }
        entry.links = 0;
        match entry.phase {
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

    pub fn set_output_type(
        &mut self,
        mac: MacAddress,
        output_type: OutputType,
        now: Instant,
    ) -> Vec<Change> {
        let mut changes = Vec::new();
        let Some(index) = self.index(mac) else {
            return changes;
        };
        if self.settings.controller(mac).output_type == output_type {
            return changes;
        }
        self.settings.set_output_type(mac, output_type);
        let removed = Change::Removed {
            mac,
            why: Removal::OutputTypeChanged,
        };
        match self.entries[index].phase {
            Phase::Bridged => changes.extend([removed, Change::Bridged { mac, output_type }]),
            Phase::Held { .. } => {
                self.entries.remove(index);
                changes.push(removed);
                self.fill_slots(now, &mut changes);
            }
            Phase::Stopped | Phase::Waiting => {}
        }
        changes
    }

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

    pub fn release_failed(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let mut changes = Vec::new();
        let Some(index) = self.index(mac) else {
            return changes;
        };
        if !matches!(self.entries[index].phase, Phase::Held { .. }) {
            return changes;
        }
        self.entries.remove(index);
        changes.push(Change::Removed {
            mac,
            why: Removal::Failed,
        });
        self.fill_slots(now, &mut changes);
        changes
    }

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

    fn fill_slots(&mut self, now: Instant, changes: &mut Vec<Change>) {
        if self.bridging_paused {
            return;
        }
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
