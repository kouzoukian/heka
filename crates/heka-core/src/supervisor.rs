//! Carries out the Bridge lifecycle: owns every Link, Bridge and Virtual Controller.
//!
//! The caller finds Links and reports them, passes on the player's Stop and Start,
//! and calls [`Supervisor::poll`] regularly with the current time. Every call
//! returns the [`Change`]s it carried out, in order, so that per-Controller side
//! effects such as hiding can follow them (see [`crate::lifecycle`]).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::bridge::{Bridge, BridgeEnd, LiveInput, OpenLink, Plugged};
use crate::lifecycle::{Change, Lifecycle, Phase, Removal};
use crate::link::LinkReader;
use crate::mac::MacAddress;
use crate::settings::Settings;
use crate::virtual_controller::{OutputType, VirtualBus};

/// Names one Link given to [`Supervisor::link_arrived`], to report it gone later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkId(u64);

/// One listed Controller, for the UI.
#[derive(Debug, Clone)]
pub struct ControllerView {
    pub mac: MacAddress,
    pub phase: Phase,
    pub output_type: OutputType,
    /// The latest input while Bridged. Kept across a Hold.
    pub live: Arc<LiveInput>,
    /// Why the last Bridge attempt failed, until one succeeds.
    pub error: Option<String>,
}

/// What heka holds for one listed Controller.
#[derive(Default)]
struct Slot {
    live: Arc<LiveInput>,
    /// The Controller's current Link, if it has one.
    link_id: Option<LinkId>,
    /// The Link while no Bridge reads it (Stopped, waiting, or between steps).
    link: Option<OpenLink>,
    bridge: Option<Bridge>,
    /// The Virtual Controller while Held.
    held: Option<Plugged>,
    error: Option<String>,
}

pub struct Supervisor {
    bus: Box<dyn VirtualBus>,
    lifecycle: Lifecycle,
    slots: HashMap<MacAddress, Slot>,
    next_link_id: u64,
}

impl Supervisor {
    pub fn new(bus: Box<dyn VirtualBus>, settings: Settings) -> Self {
        Self {
            bus,
            lifecycle: Lifecycle::new(settings),
            slots: HashMap::new(),
            next_link_id: 0,
        }
    }

    /// The settings as they stand, to be saved after [`Self::stop`] or [`Self::start`].
    pub fn settings(&self) -> &Settings {
        self.lifecycle.settings()
    }

    /// The next time [`Self::poll`] has a timer to act on.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.lifecycle.next_deadline()
    }

    /// Every listed Controller in arrival order.
    pub fn controllers(&self) -> Vec<ControllerView> {
        self.lifecycle
            .controllers()
            .map(|(mac, phase)| {
                let slot = self.slots.get(&mac);
                ControllerView {
                    mac,
                    phase,
                    output_type: self.settings().controller(mac).output_type,
                    live: slot.map(|slot| Arc::clone(&slot.live)).unwrap_or_default(),
                    error: slot.and_then(|slot| slot.error.clone()),
                }
            })
            .collect()
    }

    /// A Link to the Controller with this MAC address appeared.
    ///
    /// A second Link to a Controller that already has one is closed (ticket #8
    /// adds several Links per Controller). A Link to a Virtual Controller, heka's
    /// own or another ViGEmBus client's, is closed and never listed.
    pub fn link_arrived(
        &mut self,
        mac: MacAddress,
        link: Box<dyn LinkReader>,
        now: Instant,
    ) -> (LinkId, Vec<Change>) {
        let id = LinkId(self.next_link_id);
        self.next_link_id += 1;
        if mac.is_virtual_controller() {
            return (id, Vec::new());
        }
        let slot = self.slots.entry(mac).or_default();
        if slot.link_id.is_some() {
            return (id, Vec::new());
        }
        slot.link_id = Some(id);
        slot.link = Some(OpenLink::new(link));
        let changes = self.lifecycle.link_arrived(mac, now);
        (id, self.apply(changes, now))
    }

    /// The Link went away, as seen by whoever found it. A Link that is no
    /// longer its Controller's current one is ignored.
    pub fn link_gone(&mut self, id: LinkId, now: Instant) -> Vec<Change> {
        let Some((&mac, _)) = self.slots.iter().find(|(_, slot)| slot.link_id == Some(id)) else {
            return Vec::new();
        };
        let slot = self.slots.get_mut(&mac).expect("found above");
        slot.link_id = None;
        slot.link = None;
        if let Some(bridge) = slot.bridge.take() {
            match bridge.stop() {
                BridgeEnd::Stopped { controller, .. } => slot.held = Some(release(controller)),
                BridgeEnd::LinkLost { controller } => slot.held = Some(controller),
                BridgeEnd::VirtualControllerFailed { .. } => {}
            }
        }
        let changes = self.lifecycle.link_lost(mac, now);
        self.apply(changes, now)
    }

    /// Picks up Bridges that ended on their own and acts on timers.
    pub fn poll(&mut self, now: Instant) -> Vec<Change> {
        let ended: Vec<MacAddress> = self
            .slots
            .iter()
            .filter(|(_, slot)| slot.bridge.as_ref().is_some_and(Bridge::is_finished))
            .map(|(&mac, _)| mac)
            .collect();
        let mut done = Vec::new();
        for mac in ended {
            let slot = self.slots.get_mut(&mac).expect("listed above");
            let bridge = slot.bridge.take().expect("finished above");
            let changes = match bridge.stop() {
                BridgeEnd::LinkLost { controller } => {
                    slot.held = Some(controller);
                    slot.link_id = None;
                    self.lifecycle.link_lost(mac, now)
                }
                BridgeEnd::VirtualControllerFailed { link, error } => {
                    slot.link = Some(link);
                    slot.error = Some(error.to_string());
                    self.lifecycle.bridge_failed(mac, now)
                }
                // Not asked to stop; keep the parts and wait for the lifecycle.
                BridgeEnd::Stopped { link, controller } => {
                    slot.link = Some(link);
                    let _ = controller.unplug();
                    self.lifecycle.bridge_failed(mac, now)
                }
            };
            done.extend(self.apply(changes, now));
        }
        let changes = self.lifecycle.tick(now);
        done.extend(self.apply(changes, now));
        done
    }

    /// The player Stops this Controller.
    pub fn stop(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let changes = self.lifecycle.stop(mac, now);
        self.apply(changes, now)
    }

    /// The player starts this Stopped Controller again.
    pub fn start(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let changes = self.lifecycle.start(mac, now);
        self.apply(changes, now)
    }

    /// The player chooses this Controller's Output Type. A Bridged Controller's
    /// Link is Bridged again to a new Virtual Controller of that type.
    pub fn set_output_type(
        &mut self,
        mac: MacAddress,
        output_type: OutputType,
        now: Instant,
    ) -> Vec<Change> {
        let changes = self.lifecycle.set_output_type(mac, output_type, now);
        self.apply(changes, now)
    }

    /// Removes every Virtual Controller and closes every Link.
    pub fn quit(&mut self) -> Vec<Change> {
        let changes = self.lifecycle.quit();
        // `now` only matters to follow-up Bridges, and quitting has none.
        let done = self.apply(changes, Instant::now());
        self.slots.clear();
        done
    }

    /// Carries out changes, and any the lifecycle answers with, in order.
    fn apply(&mut self, changes: Vec<Change>, now: Instant) -> Vec<Change> {
        let mut queue = std::collections::VecDeque::from(changes);
        let mut done = Vec::new();
        while let Some(change) = queue.pop_front() {
            done.push(change);
            match change {
                Change::Bridged { mac, output_type } => {
                    if let Err(error) = self.bridge(mac, output_type, false) {
                        self.slot(mac).error = Some(error);
                        queue.extend(self.lifecycle.bridge_failed(mac, now));
                    }
                }
                Change::Held { .. } => {
                    // The Virtual Controller was released and kept when the Link went.
                }
                Change::Returned { mac } => {
                    let output_type = self.settings().controller(mac).output_type;
                    if let Err(error) = self.bridge(mac, output_type, true) {
                        self.slot(mac).error = Some(error);
                        queue.extend(self.lifecycle.bridge_failed(mac, now));
                    }
                }
                Change::Removed { mac, why } => self.remove(mac, why),
            }
        }
        let listed: Vec<MacAddress> = self.lifecycle.controllers().map(|(mac, _)| mac).collect();
        self.slots.retain(|mac, _| listed.contains(mac));
        done
    }

    /// Starts a Bridge from the Controller's idle Link, to its Held Virtual
    /// Controller when `resume` and there is one, otherwise to a new one.
    fn bridge(
        &mut self,
        mac: MacAddress,
        output_type: OutputType,
        resume: bool,
    ) -> Result<(), String> {
        if self.slot(mac).link.is_none() {
            // Only if the input thread could not be started last time, taking the Link with it.
            return Err("the Link was closed; reconnect the Controller".into());
        }
        let held = if resume {
            self.slot(mac).held.take()
        } else {
            None
        };
        let controller = match held {
            Some(controller) => controller,
            None => Plugged::new(self.bus.plug(output_type).map_err(|e| e.to_string())?),
        };
        let slot = self.slot(mac);
        let link = slot.link.take().expect("checked above");
        let bridge = Bridge::resume(link, controller, Arc::clone(&slot.live))
            .map_err(|e| format!("cannot start the input thread: {e}"))?;
        slot.bridge = Some(bridge);
        slot.error = None;
        Ok(())
    }

    fn remove(&mut self, mac: MacAddress, why: Removal) {
        let slot = self.slot(mac);
        // Best effort throughout: ViGEmBus also removes Virtual Controllers when
        // heka's bus handle closes.
        if let Some(bridge) = slot.bridge.take() {
            match bridge.stop() {
                BridgeEnd::Stopped { link, controller } => {
                    slot.link = Some(link);
                    let _ = controller.unplug();
                }
                BridgeEnd::LinkLost { controller } => {
                    slot.link_id = None;
                    let _ = controller.unplug();
                }
                BridgeEnd::VirtualControllerFailed { link, .. } => slot.link = Some(link),
            }
        }
        if let Some(controller) = slot.held.take() {
            let _ = controller.unplug();
        }
        if why == Removal::Quit {
            slot.link = None;
            slot.link_id = None;
        }
    }

    fn slot(&mut self, mac: MacAddress) -> &mut Slot {
        self.slots.entry(mac).or_default()
    }
}

/// Shows everything released and sticks centred, and keeps the Virtual Controller.
fn release(mut controller: Plugged) -> Plugged {
    // Best effort: a failure shows on the next update or removal.
    let _ = controller.release();
    controller
}
