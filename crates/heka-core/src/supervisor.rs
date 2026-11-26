//! Carries out the Bridge lifecycle: owns every Link, Bridge and Virtual Controller.
//!
//! The caller finds Links and reports them, passes on the player's choices,
//! and calls [`Supervisor::poll`] regularly with the current time. Every call
//! returns the [`Change`]s it carried out, in order, so that per-Controller side
//! effects such as hiding can follow them (see [`crate::lifecycle`]).
//!
//! A Controller may have several Links, for example USB and Bluetooth at once.
//! It is listed once, by MAC address, and only its Active Link is read (see
//! [`active_link`]). When the Active Link changes, the Bridge moves to the new
//! one and keeps its Virtual Controller.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use crate::bridge::{Bridge, BridgeEnd, LiveInput, OpenLink, Plugged};
use crate::lifecycle::{Change, Lifecycle, Phase, Removal};
use crate::link::{ControllerModel, LinkKind, LinkReader, active_link};
use crate::mac::MacAddress;
use crate::preferences::{LivePreferences, Preferences};
use crate::settings::Settings;
use crate::virtual_controller::{OutputType, VirtualBus};
use crate::watch::Watch;

/// Names one Link given to [`Supervisor::link_arrived`], to report it gone later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkId(u64);

/// One listed Controller, for the UI.
#[derive(Debug, Clone)]
pub struct ControllerView {
    pub mac: MacAddress,
    /// Which kind of Controller it is.
    pub model: ControllerModel,
    pub phase: Phase,
    pub output_type: OutputType,
    /// What the player calls this Controller.
    pub nickname: Option<String>,
    pub preferences: Preferences,
    /// The Link heka reads, `None` while Held.
    pub active_link: Option<LinkKind>,
    /// The latest input while Bridged, or while connected and there is no bus.
    pub live: Arc<LiveInput>,
    /// Why the last Bridge attempt failed, until one succeeds.
    pub error: Option<String>,
}

/// One of a Controller's Links.
struct LinkSlot {
    id: LinkId,
    kind: LinkKind,
    /// The Link while nothing reads it. `None` while a Bridge or Watch reads it,
    /// or if an input thread could not be started and took it along.
    open: Option<OpenLink>,
}

/// What heka holds for one listed Controller.
#[derive(Default)]
struct Slot {
    /// As its Links say, remembered while it has none.
    model: ControllerModel,
    live: Arc<LiveInput>,
    /// Shared with the Bridge, so that it follows the player's changes.
    preferences: Arc<LivePreferences>,
    /// Every Link, in arrival order.
    links: Vec<LinkSlot>,
    /// The Link the Bridge or Watch reads.
    reading: Option<LinkId>,
    bridge: Option<Bridge>,
    /// Reads the Active Link while there is no bus.
    watch: Option<Watch>,
    /// The Virtual Controller while Held.
    held: Option<Plugged>,
    error: Option<String>,
}

impl Slot {
    /// The Link to read: the first USB Link, otherwise the first Link.
    fn active(&self) -> Option<&LinkSlot> {
        let kinds: Vec<LinkKind> = self.links.iter().map(|link| link.kind).collect();
        active_link(&kinds).map(|index| &self.links[index])
    }

    fn active_id(&self) -> Option<LinkId> {
        self.active().map(|link| link.id)
    }

    /// True when something reads a Link that is no longer the Active Link.
    fn reads_inactive_link(&self) -> bool {
        (self.bridge.is_some() || self.watch.is_some()) && self.reading != self.active_id()
    }

    /// Takes the Active Link out to be read, and remembers that it is.
    fn take_active(&mut self) -> Option<OpenLink> {
        let id = self.active_id()?;
        let link = self
            .links
            .iter_mut()
            .find(|link| link.id == id)?
            .open
            .take()?;
        self.reading = Some(id);
        Some(link)
    }

    /// The Link that was being read is idle again.
    fn put_back(&mut self, link: OpenLink) {
        let id = self.reading.take();
        if let Some(slot) = self.links.iter_mut().find(|slot| Some(slot.id) == id) {
            slot.open = Some(link);
        }
    }

    /// The Link that was being read is gone.
    fn forget_read(&mut self) {
        if let Some(id) = self.reading.take() {
            self.links.retain(|link| link.id != id);
        }
    }
}

pub struct Supervisor {
    /// `None` until Virtual Controllers can be made.
    bus: Option<Box<dyn VirtualBus>>,
    lifecycle: Lifecycle,
    slots: HashMap<MacAddress, Slot>,
    next_link_id: u64,
}

impl Supervisor {
    pub fn new(bus: Box<dyn VirtualBus>, settings: Settings) -> Self {
        Self {
            bus: Some(bus),
            lifecycle: Lifecycle::new(settings),
            slots: HashMap::new(),
            next_link_id: 0,
        }
    }

    /// A Supervisor with no way to make Virtual Controllers yet: it lists
    /// Controllers and reads their input, and Bridges none until
    /// [`Self::connect_bus`].
    pub fn without_bus(settings: Settings) -> Self {
        Self {
            bus: None,
            lifecycle: Lifecycle::paused(settings),
            slots: HashMap::new(),
            next_link_id: 0,
        }
    }

    pub fn has_bus(&self) -> bool {
        self.bus.is_some()
    }

    /// Virtual Controllers can be made now: Bridges the listed Controllers.
    /// Ignored when there already is a bus.
    pub fn connect_bus(&mut self, bus: Box<dyn VirtualBus>, now: Instant) -> Vec<Change> {
        if self.has_bus() {
            return Vec::new();
        }
        self.bus = Some(bus);
        let mut lost = Vec::new();
        for (&mac, slot) in &mut self.slots {
            if let Some(watch) = slot.watch.take() {
                match watch.stop() {
                    Some(link) => slot.put_back(link),
                    None => {
                        slot.forget_read();
                        lost.push(mac);
                    }
                }
            }
        }
        let mut changes = Vec::new();
        for mac in lost {
            changes.extend(self.lifecycle.link_lost(mac, now));
        }
        changes.extend(self.lifecycle.resume_bridging(now));
        self.apply(changes, now)
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
                let settings = self.settings().controller(mac);
                ControllerView {
                    mac,
                    model: slot.map(|slot| slot.model).unwrap_or_default(),
                    phase,
                    output_type: settings.output_type,
                    nickname: settings.nickname,
                    preferences: settings.preferences,
                    active_link: slot.and_then(Slot::active).map(|link| link.kind),
                    live: slot.map(|slot| Arc::clone(&slot.live)).unwrap_or_default(),
                    error: slot.and_then(|slot| slot.error.clone()),
                }
            })
            .collect()
    }

    /// A Link to the Controller with this MAC address appeared. A Controller
    /// already listed keeps its place and its Virtual Controller; if the new
    /// Link becomes its Active Link, the Controller is read from it from now on.
    ///
    /// A Link to a Virtual Controller, heka's own or another ViGEmBus client's,
    /// is closed and never listed.
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
        let link = OpenLink::new(link);
        let format = link.format();
        let slot = self.slot(mac);
        slot.model = format.model();
        slot.links.push(LinkSlot {
            id,
            kind: format.kind(),
            open: Some(link),
        });
        let changes = self.lifecycle.link_arrived(mac, now);
        let mut done = self.apply(changes, now);
        done.extend(self.follow_active_link(mac, now));
        (id, done)
    }

    /// The Link went away, as seen by whoever found it. A Link heka has already
    /// seen go is ignored.
    pub fn link_gone(&mut self, id: LinkId, now: Instant) -> Vec<Change> {
        let Some((&mac, _)) = self
            .slots
            .iter()
            .find(|(_, slot)| slot.links.iter().any(|link| link.id == id))
        else {
            return Vec::new();
        };
        let slot = self.slots.get_mut(&mac).expect("found above");
        let mut moving = None;
        if slot.reading == Some(id) {
            slot.watch = None;
            if let Some(bridge) = slot.bridge.take() {
                match bridge.stop() {
                    BridgeEnd::Stopped { controller, .. } | BridgeEnd::LinkLost { controller } => {
                        moving = Some(controller)
                    }
                    BridgeEnd::VirtualControllerFailed { .. } => {}
                }
            }
            slot.reading = None;
        }
        slot.links.retain(|link| link.id != id);
        let changes = match moving {
            Some(controller) => self.lost_read_link(mac, controller, now),
            None => self.lifecycle.link_lost(mac, now),
        };
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
                    slot.forget_read();
                    self.lost_read_link(mac, controller, now)
                }
                BridgeEnd::VirtualControllerFailed { link, error } => {
                    slot.put_back(link);
                    slot.error = Some(error.to_string());
                    self.lifecycle.bridge_failed(mac, now)
                }
                // Not asked to stop; keep the parts and wait for the lifecycle.
                BridgeEnd::Stopped { link, controller } => {
                    slot.put_back(link);
                    let _ = controller.unplug();
                    self.lifecycle.bridge_failed(mac, now)
                }
            };
            done.extend(self.apply(changes, now));
        }
        let unread: Vec<MacAddress> = self
            .slots
            .iter()
            .filter(|(_, slot)| slot.watch.as_ref().is_some_and(Watch::is_finished))
            .map(|(&mac, _)| mac)
            .collect();
        for mac in unread {
            let slot = self.slots.get_mut(&mac).expect("listed above");
            slot.watch = None;
            slot.forget_read();
            let changes = self.lifecycle.link_lost(mac, now);
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

    /// The player names this Controller; a blank name takes the name away.
    pub fn set_nickname(&mut self, mac: MacAddress, name: &str) {
        self.lifecycle.settings_mut().set_nickname(mac, name);
    }

    /// The player chooses this Controller's preferences. A running Bridge
    /// follows them at once. Hiding is the caller's to carry out.
    pub fn set_preferences(&mut self, mac: MacAddress, preferences: Preferences) {
        self.lifecycle
            .settings_mut()
            .set_preferences(mac, preferences);
        if let Some(slot) = self.slots.get(&mac) {
            slot.preferences.set(preferences);
        }
    }

    /// Removes every Virtual Controller and closes every Link.
    pub fn quit(&mut self) -> Vec<Change> {
        let changes = self.lifecycle.quit();
        // `now` only matters to follow-up Bridges, and quitting has none.
        let done = self.apply(changes, Instant::now());
        self.slots.clear();
        done
    }

    /// The Link a Bridge read is gone and the Bridge handed back its Virtual
    /// Controller: the Bridge moves to the Controller's next Link, or the
    /// Controller is Held if it has none.
    fn lost_read_link(
        &mut self,
        mac: MacAddress,
        controller: Plugged,
        now: Instant,
    ) -> Vec<Change> {
        let mut changes = self.lifecycle.link_lost(mac, now);
        if self.slot(mac).links.is_empty() {
            self.slot(mac).held = Some(release(controller));
        } else if let Err(error) = self.bridge_to(mac, controller) {
            self.slot(mac).error = Some(error);
            changes.extend(self.lifecycle.bridge_failed(mac, now));
        }
        changes
    }

    /// Moves whatever reads the Controller to its Active Link, if it reads another.
    fn follow_active_link(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let Some(slot) = self.slots.get_mut(&mac) else {
            return Vec::new();
        };
        if !slot.reads_inactive_link() {
            return Vec::new();
        }
        let mut changes = Vec::new();
        if let Some(watch) = slot.watch.take() {
            match watch.stop() {
                Some(link) => slot.put_back(link),
                None => {
                    slot.forget_read();
                    changes.extend(self.lifecycle.link_lost(mac, now));
                }
            }
        }
        let slot = self.slot(mac);
        if let Some(bridge) = slot.bridge.take() {
            match bridge.stop() {
                BridgeEnd::Stopped { link, controller } => {
                    slot.put_back(link);
                    if let Err(error) = self.bridge_to(mac, controller) {
                        self.slot(mac).error = Some(error);
                        changes.extend(self.lifecycle.bridge_failed(mac, now));
                    }
                }
                BridgeEnd::LinkLost { controller } => {
                    slot.forget_read();
                    changes.extend(self.lost_read_link(mac, controller, now));
                }
                BridgeEnd::VirtualControllerFailed { link, error } => {
                    slot.put_back(link);
                    slot.error = Some(error.to_string());
                    changes.extend(self.lifecycle.bridge_failed(mac, now));
                }
            }
        }
        self.apply(changes, now)
    }

    /// Carries out changes, and any the lifecycle answers with, in order.
    fn apply(&mut self, changes: Vec<Change>, now: Instant) -> Vec<Change> {
        let mut queue = VecDeque::from(changes);
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
                Change::Removed { mac, why } => queue.extend(self.remove(mac, why, now)),
            }
        }
        let listed: Vec<MacAddress> = self.lifecycle.controllers().map(|(mac, _)| mac).collect();
        self.slots.retain(|mac, _| listed.contains(mac));
        if !self.has_bus() {
            self.watch_idle_links();
        }
        done
    }

    /// Reads the Active Link of every Controller that nothing reads, so its
    /// input still shows without a bus.
    fn watch_idle_links(&mut self) {
        for slot in self.slots.values_mut() {
            if slot.watch.is_some() || slot.bridge.is_some() {
                continue;
            }
            let Some(link) = slot.take_active() else {
                continue;
            };
            match Watch::start(link, Arc::clone(&slot.live)) {
                Ok(watch) => slot.watch = Some(watch),
                Err(error) => slot.error = Some(format!("cannot start the input thread: {error}")),
            }
        }
    }

    /// Starts a Bridge from the Controller's Active Link, to its Held Virtual
    /// Controller when `resume` and there is one, otherwise to a new one.
    fn bridge(
        &mut self,
        mac: MacAddress,
        output_type: OutputType,
        resume: bool,
    ) -> Result<(), String> {
        let ready = self
            .slot(mac)
            .active()
            .is_some_and(|link| link.open.is_some());
        if !ready {
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
            None => {
                let bus = self.bus.as_ref().ok_or("there is no bus")?;
                Plugged::new(bus.plug(output_type).map_err(|e| e.to_string())?)
            }
        };
        self.bridge_to(mac, controller)
    }

    /// Starts a Bridge from the Controller's Active Link to this Virtual Controller.
    fn bridge_to(&mut self, mac: MacAddress, mut controller: Plugged) -> Result<(), String> {
        let preferences = self.settings().controller(mac).preferences;
        let slot = self.slot(mac);
        let link = slot
            .take_active()
            .ok_or("the Link was closed; reconnect the Controller")?;
        slot.preferences.set(preferences);
        controller.follow(Arc::clone(&slot.preferences));
        let bridge = Bridge::resume(link, controller, Arc::clone(&slot.live))
            .map_err(|e| format!("cannot start the input thread: {e}"))?;
        slot.bridge = Some(bridge);
        slot.error = None;
        Ok(())
    }

    /// Removes the Controller's Virtual Controller, and answers with what the
    /// lifecycle says if its Link turns out to be gone.
    fn remove(&mut self, mac: MacAddress, why: Removal, now: Instant) -> Vec<Change> {
        let slot = self.slot(mac);
        let mut link_lost = false;
        // Best effort throughout: ViGEmBus also removes Virtual Controllers when
        // heka's bus handle closes.
        if let Some(bridge) = slot.bridge.take() {
            match bridge.stop() {
                BridgeEnd::Stopped { link, controller } => {
                    slot.put_back(link);
                    let _ = controller.unplug();
                }
                BridgeEnd::LinkLost { controller } => {
                    slot.forget_read();
                    link_lost = true;
                    let _ = controller.unplug();
                }
                BridgeEnd::VirtualControllerFailed { link, .. } => slot.put_back(link),
            }
        }
        if let Some(controller) = slot.held.take() {
            let _ = controller.unplug();
        }
        if why == Removal::Quit {
            slot.links.clear();
            slot.reading = None;
        }
        if link_lost {
            self.lifecycle.link_lost(mac, now)
        } else {
            Vec::new()
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
